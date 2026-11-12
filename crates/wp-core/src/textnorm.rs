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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn norm() {
        assert_eq!(search_normalize("하나님이, 세상을  사랑하사!"), "하나님이 세상을 사랑하사");
        assert_eq!(compact("하나님이 세상을"), "하나님이세상을");
        assert_eq!(clean_display_text("<b>태초에</b>&nbsp;하나님이"), "태초에 하나님이");
    }
}
