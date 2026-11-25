//! 본문 정리와 검색용 정규화 (Electron 판 src/core/textnorm.js 와 같은 규칙)

use once_cell::sync::Lazy;
use regex::Regex;
use unicode_normalization::UnicodeNormalization;

static TAG: Lazy<Regex> = Lazy::new(|| Regex::new(r"</?[a-zA-Z][^<>]*>").unwrap());
static BR: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)<br\s*/?>").unwrap());
static WS: Lazy<Regex> = Lazy::new(|| Regex::new(r"[\r\n\t]+").unwrap());
static SP2: Lazy<Regex> = Lazy::new(|| Regex::new(r" {2,}").unwrap());

/// 표시용 본문 정리: HTML/XML 태그만 제거하고 맞춤법·구두점은 보존한다.
pub fn clean_display_text(s: &str) -> String {
    let s: String = s.nfc().collect();
    let s = BR.replace_all(&s, " ");
    let s = TAG.replace_all(&s, "");
    let s = s
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&amp;", "&");
    let s = WS.replace_all(&s, " ");
    SP2.replace_all(&s, " ").trim().to_string()
}

/// 검색용 정규화: 구두점 제거, 공백 정리, 소문자화
pub fn search_normalize(s: &str) -> String {
    let lower: String = s.nfc().collect::<String>().to_lowercase();
    let mut out = String::with_capacity(lower.len());
    let mut space = false;
    for ch in lower.chars() {
        if ch.is_alphanumeric() {
            if space && !out.is_empty() {
                out.push(' ');
            }
            space = false;
            out.push(ch);
        } else {
            space = true;
        }
    }
    out
}

/// 띄어쓰기 차이를 무시한 검색용 문자열
pub fn compact(s: &str) -> String {
    search_normalize(s).chars().filter(|c| !c.is_whitespace()).collect()
}

/// 한글 초성 19자 (호환용 자모, 음절 순서)
const CHOSEONG: [char; 19] = ['ㄱ', 'ㄲ', 'ㄴ', 'ㄷ', 'ㄸ', 'ㄹ', 'ㅁ', 'ㅂ', 'ㅃ', 'ㅅ', 'ㅆ', 'ㅇ', 'ㅈ', 'ㅉ', 'ㅊ', 'ㅋ', 'ㅌ', 'ㅍ', 'ㅎ'];

fn is_choseong(c: char) -> bool {
    CHOSEONG.contains(&c)
}

/// 음절의 초성 ('주' → 'ㅈ'). 한글 음절이 아니면 None
pub fn choseong_of(c: char) -> Option<char> {
    let n = c as u32;
    (0xAC00..=0xD7A3).contains(&n).then(|| CHOSEONG[((n - 0xAC00) / 588) as usize])
}

/// 검색어에 초성(ㄱㄴㄷ…)이 들어 있나 — 들어 있으면 초성 검색(제목만)으로 찾는다
pub fn has_choseong(q: &str) -> bool {
    q.nfc().any(is_choseong)
}

/// 제목 찾기용 비교 문자열: NFC(맥에서 만든 파일 이름의 풀어쓴 한글도 모아 쓴 글자로) · 띄어쓰기 빼기 · 소문자
pub fn squash(s: &str) -> String {
    s.nfc().filter(|c| !c.is_whitespace()).collect::<String>().to_lowercase()
}

/// 띄어쓰기·대소문자를 무시하고 hay 에 q 가 들어 있나. q 에 초성이 있으면 그 자리는 그 초성으로 시작하는 음절과 맞는다:
/// 'ㅈㅁㅂㄹ' · '주ㅁㅂㄹ' · '주만바라' 모두 '주만 바라 볼지라' 에 든다.
pub fn fuzzy_contains(hay: &str, q: &str) -> bool {
    let q: Vec<char> = squash(q).chars().collect();
    if q.is_empty() {
        return true;
    }
    let h: Vec<char> = squash(hay).chars().collect();
    if !q.iter().any(|c| is_choseong(*c)) {
        let (h, q): (String, String) = (h.into_iter().collect(), q.into_iter().collect());
        return h.contains(&q);
    }
    let eq = |hc: char, qc: char| hc == qc || is_choseong(qc) && choseong_of(hc) == Some(qc);
    h.len() >= q.len() && (0..=h.len() - q.len()).any(|i| q.iter().enumerate().all(|(k, qc)| eq(h[i + k], *qc)))
}

/// fuzzy_contains 와 같은 맞추기로 앞에서부터 맞는가 (검색 결과에서 앞자리로 올릴 때)
pub fn fuzzy_starts_with(hay: &str, q: &str) -> bool {
    let q: Vec<char> = squash(q).chars().collect();
    let h: Vec<char> = squash(hay).chars().collect();
    h.len() >= q.len() && q.iter().enumerate().all(|(k, qc)| h[k] == *qc || is_choseong(*qc) && choseong_of(h[k]) == Some(*qc))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn norm() {
        assert_eq!(search_normalize("하나님이, 세상을  사랑하사!"), "하나님이 세상을 사랑하사");
        assert_eq!(compact("하나님이 세상을"), "하나님이세상을");
        assert_eq!(clean_display_text("<b>태초에</b>&nbsp;하나님이"), "태초에 하나님이");
    }

    #[test]
    fn choseong_search() {
        let t = "주만 바라 볼지라";
        assert_eq!(choseong_of('주'), Some('ㅈ'));
        assert_eq!(choseong_of('꽃'), Some('ㄲ'));
        assert_eq!(choseong_of('A'), None);
        assert!(fuzzy_contains(t, "ㅈㅁㅂㄹ"));
        assert!(fuzzy_contains(t, "주ㅁㅂㄹ"));
        assert!(fuzzy_contains(t, "ㅂㄹ ㅂㅈ"), "중간부터 · 띄어쓰기 무시");
        assert!(fuzzy_contains(t, "주만바라볼지라"));
        assert!(fuzzy_contains(t, "주만 바라 볼 지라"));
        assert!(!fuzzy_contains(t, "ㄱㅁㅂ"));
        assert!(!fuzzy_contains(t, "주ㄱ"));
        assert!(fuzzy_contains("Way Maker", "waymaker"));
        assert!(fuzzy_contains("아무 제목", ""));
        // 맥에서 만든 파일 이름(풀어쓴 한글, NFD)
        let nfd: String = "주만 바라 볼지라".nfd().collect();
        assert!(fuzzy_contains(&nfd, "주만바라"));
        assert!(fuzzy_contains(&nfd, "ㅈㅁㅂㄹ"));
        assert!(fuzzy_starts_with(t, "ㅈㅁ"));
        assert!(!fuzzy_starts_with(t, "ㅁㅂ"));
        assert!(has_choseong("주ㅁ") && !has_choseong("주만"));
    }
}
