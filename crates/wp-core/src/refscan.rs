//! 글 속에서 성경 장절 찾기 — 설교 원고(요3:16 · 로마서 8장 28절)와 음성 인식 글(요한 복음 십오 장 오 절)
//!
//! 장절 풀이는 parser.rs 를 그대로 쓰고, 여기서는 '어디가 장절인가' 만 찾는다.
//! 음성 인식이 틀리게 적는 꼴을 견딘다:
//!  - 책 이름 사이 띄어쓰기: '요한 복음' · '로마 서' · '고린도 전서' · '요한 1서'
//!  - 한자어 숫자: '팔 장' · '십오 절' · '이십삼 편' · '백십구 편'
//!  - '편'(시편) · '에서 … 까지' · '부터' · '와' 로 이은 절
//!  - 비슷하게 들리는 이름: '로마스' → 로마서, '요한 개시록' → 요한계시록 같은 몇 가지 (SOUND_ALIKE)

use crate::books::{Book, BOOKS};
use crate::parser::{parse_reference, Parsed};
use once_cell::sync::Lazy;
use regex::Regex;

/// 찾은 장절 하나
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    /// 원래 글에서의 바이트 자리
    pub start: usize,
    pub end: usize,
    /// parser 가 읽는 꼴 ('요한복음 15:5')
    pub reference: String,
    pub book_id: i64,
    pub chapter: i64,
    pub verse: Option<i64>,
    /// 원래 글 그대로
    pub raw: String,
}

/// 음성 인식이 잘못 적는 책 이름 (붙여 쓴 꼴 → 바른 이름)
const SOUND_ALIKE: [(&str, &str); 14] = [
    ("로마스", "로마서"),
    ("요한개시록", "요한계시록"),
    ("개시록", "요한계시록"),
    ("계시룩", "요한계시록"),
    ("마태북음", "마태복음"),
    ("요한북음", "요한복음"),
    ("누가북음", "누가복음"),
    ("마가북음", "마가복음"),
    ("시편서", "시편"),
    ("잠원", "잠언"),
    ("이사아", "이사야"),
    ("에배소서", "에베소서"),
    ("빌립보서신", "빌립보서"),
    ("히부리서", "히브리서"),
];

/// 말로 하는 책 이름 (붙여 쓴 꼴): 정식 이름 · 별칭(두 글자 이상 한국어) · SOUND_ALIKE
static SPOKEN: Lazy<Vec<(String, &'static Book)>> = Lazy::new(|| {
    let mut v: Vec<(String, &'static Book)> = vec![];
    let hangul = |s: &str| s.chars().all(|c| ('가'..='힣').contains(&c) || c.is_ascii_digit());
    for b in BOOKS.iter() {
        let mut names = vec![b.name.clone()];
        names.extend(b.aliases.iter().filter(|a| a.chars().count() >= 2 && hangul(a)).cloned());
        if b.short.chars().count() >= 2 && hangul(&b.short) {
            names.push(b.short.clone());
        }
        for n in names {
            v.push((n, b));
        }
    }
    // '요한1서' 꼴 (숫자) · '계시록'
    for (k, a) in [(62, "요한1서"), (63, "요한2서"), (64, "요한3서")] {
        v.push((a.to_string(), BOOKS.iter().find(|b| b.id == k).unwrap()));
    }
    for (pre, id) in [("고린도1서", 46), ("고린도2서", 47), ("데살로니가1서", 52), ("데살로니가2서", 53), ("디모데1서", 54), ("디모데2서", 55), ("베드로1서", 60), ("베드로2서", 61), ("계시록", 66), ("사도행전", 44)] {
        v.push((pre.to_string(), BOOKS.iter().find(|b| b.id == id).unwrap()));
    }
    for (wrong, right) in SOUND_ALIKE {
        if let Some(b) = BOOKS.iter().find(|b| b.name == right) {
            v.push((wrong.to_string(), b));
        }
    }
    // 긴 이름부터 (요한일서 가 요한 보다 먼저)
    v.sort_by(|a, b| b.0.chars().count().cmp(&a.0.chars().count()));
    v.dedup_by(|a, b| a.0 == b.0);
    v
});

/// 이름을 '글자 사이 띄어쓰기 허용' 정규식 조각으로
fn spaced(name: &str) -> String {
    name.chars().map(|c| regex::escape(&c.to_string())).collect::<Vec<_>>().join(r"\s?")
}

const NUM: &str = r"(?:\d+|[일이삼사오육륙칠팔구십백]+)";

/// 말·원고 공통: 정식 이름 + 장 + (절)
static SPOKEN_RE: Lazy<Regex> = Lazy::new(|| {
    let names: Vec<String> = SPOKEN.iter().map(|(n, _)| spaced(n)).collect();
    Regex::new(&format!(
        r"(?P<book>{})\s*(?P<ch>{NUM})\s*(?:장|편|:)\s*(?:(?P<v1>{NUM})\s*절?)?(?:\s*(?:-|~|에서|부터|부터서)\s*(?P<v2>{NUM})\s*절?\s*(?:까지)?)?",
        names.join("|")
    ))
    .unwrap()
});

/// 원고에 흔한 줄임 꼴: '요3:16' · '롬 8:28' · '시 23:1-6' · '요일 4:8' (숫자:숫자 가 꼭 있어야 — '창' 같은 낱말과 헷갈리지 않게)
static SHORT_RE: Lazy<Regex> = Lazy::new(|| {
    let mut shorts: Vec<String> = vec![];
    for b in BOOKS.iter() {
        shorts.push(b.short.clone());
        shorts.extend(b.aliases.iter().filter(|a| a.chars().all(|c| ('가'..='힣').contains(&c))).cloned());
    }
    shorts.sort_by(|a, b| b.chars().count().cmp(&a.chars().count()));
    shorts.dedup();
    let alt: Vec<String> = shorts.iter().map(|s| regex::escape(s)).collect();
    Regex::new(&format!(r"(?:^|[^가-힣])(?P<book>{})\s?(?P<ch>\d+)\s?:\s?(?P<v1>\d+)(?:\s?[-~]\s?(?P<v2>\d+))?", alt.join("|"))).unwrap()
});

/// 한자어 숫자 → 수 ('십오' 15 · '이십삼' 23 · '백십구' 119). 숫자 글자면 그대로
pub fn kor_num(s: &str) -> Option<i64> {
    if let Ok(n) = s.parse::<i64>() {
        return Some(n);
    }
    let digit = |c: char| match c {
        '일' => Some(1),
        '이' => Some(2),
        '삼' => Some(3),
        '사' => Some(4),
        '오' => Some(5),
        '육' | '륙' => Some(6),
        '칠' => Some(7),
        '팔' => Some(8),
        '구' => Some(9),
        _ => None,
    };
    let (mut total, mut cur) = (0i64, 0i64);
    let mut any = false;
    for c in s.chars() {
        match c {
            '백' => {
                total += if cur == 0 { 1 } else { cur } * 100;
                cur = 0;
            }
            '십' => {
                total += if cur == 0 { 1 } else { cur } * 10;
                cur = 0;
            }
            _ => {
                let d = digit(c)?;
                if cur != 0 {
                    return None; // '이삼' 같은 것은 숫자가 아님
                }
                cur = d;
            }
        }
        any = true;
    }
    if !any {
        return None;
    }
    Some(total + cur).filter(|n| *n > 0)
}

fn book_of(name: &str) -> Option<&'static Book> {
    let k: String = name.chars().filter(|c| !c.is_whitespace()).collect();
    SPOKEN.iter().find(|(n, _)| *n == k).map(|(_, b)| *b).or_else(|| crate::books::resolve_book(&k).0)
}

fn build(book: &'static Book, ch: i64, v1: Option<i64>, v2: Option<i64>) -> Option<(String, Option<i64>)> {
    if ch < 1 || ch > book.chapters {
        return None;
    }
    let r = match (v1, v2) {
        (Some(a), Some(b)) if b > a => format!("{} {ch}:{a}-{b}", book.name),
        (Some(a), _) => format!("{} {ch}:{a}", book.name),
        (None, _) => format!("{} {ch}", book.name),
    };
    match parse_reference(&r) {
        Parsed::Ok { .. } => Some((r, v1)),
        _ => None,
    }
}

/// 글 속 장절 모두 (앞에서부터, 겹치지 않게)
pub fn find_refs(text: &str) -> Vec<Found> {
    let mut out: Vec<Found> = vec![];
    for m in SPOKEN_RE.captures_iter(text) {
        let all = m.get(0).unwrap();
        let Some(book) = book_of(&m["book"]) else { continue };
        // 두 글자 이하 이름('요일' · '시편')은 앞이 한글이면 낱말의 일부 ('수요일 3장')
        let short = m["book"].chars().filter(|c| !c.is_whitespace()).count() <= 2;
        if short && text[..all.start()].chars().next_back().is_some_and(|c| ('가'..='힣').contains(&c)) {
            continue;
        }
        let Some(ch) = kor_num(&m["ch"]) else { continue };
        let v1 = m.name("v1").and_then(|x| kor_num(x.as_str()));
        let v2 = m.name("v2").and_then(|x| kor_num(x.as_str()));
        // '장' 없이 ':' 만 쓴 것도 되지만, 숫자가 한글이면 '장·편' 이 있어야 (말 '로마서 이' 같은 것 막기)
        if let Some((r, v)) = build(book, ch, v1, v2) {
            out.push(Found { start: all.start(), end: all.end(), reference: r, book_id: book.id, chapter: ch, verse: v, raw: all.as_str().trim().to_string() });
        }
    }
    for m in SHORT_RE.captures_iter(text) {
        let b = m.name("book").unwrap();
        let end = m.get(0).unwrap().end();
        if out.iter().any(|f| b.start() < f.end && end > f.start) {
            continue;
        }
        let Some(book) = crate::books::resolve_book(b.as_str()).0 else { continue };
        let ch = m["ch"].parse().unwrap_or(0);
        let v1 = m["v1"].parse().ok();
        let v2 = m.name("v2").and_then(|x| x.as_str().parse().ok());
        if let Some((r, v)) = build(book, ch, v1, v2) {
            out.push(Found { start: b.start(), end, reference: r, book_id: book.id, chapter: ch, verse: v, raw: text[b.start()..end].trim().to_string() });
        }
    }
    out.sort_by_key(|f| f.start);
    out
}

/// 음성 인식이 '책 이름' 을 들었는지 (장 번호가 다음 조각으로 넘어간 경우에 쓴다: '요한복음' | '15장 5절')
pub fn ends_with_book(text: &str) -> Option<&'static Book> {
    let t: String = text.trim_end_matches(|c: char| c.is_whitespace() || ",.".contains(c)).chars().filter(|c| !c.is_whitespace()).collect();
    SPOKEN.iter().find(|(n, _)| n.chars().count() >= 2 && t.ends_with(n.as_str())).map(|(_, b)| *b)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn refs(t: &str) -> Vec<String> {
        find_refs(t).into_iter().map(|f| f.reference).collect()
    }
    #[test]
    fn numbers() {
        assert_eq!(kor_num("십오"), Some(15));
        assert_eq!(kor_num("이십삼"), Some(23));
        assert_eq!(kor_num("백십구"), Some(119));
        assert_eq!(kor_num("팔"), Some(8));
        assert_eq!(kor_num("십"), Some(10));
        assert_eq!(kor_num("12"), Some(12));
        assert_eq!(kor_num("이삼"), None);
        assert_eq!(kor_num("사랑"), None);
    }
    #[test]
    fn spoken_forms() {
        assert_eq!(refs("오늘 본문은 요한복음 15장 5절 말씀입니다"), vec!["요한복음 15:5"]);
        assert_eq!(refs("요한 복음 십오 장 오 절을 보십시오"), vec!["요한복음 15:5"]);
        assert_eq!(refs("로마서 팔 장 이십팔 절"), vec!["로마서 8:28"]);
        assert_eq!(refs("로마 서 8장 28절"), vec!["로마서 8:28"]);
        assert_eq!(refs("시편 이십삼 편 일 절에서 육 절까지"), vec!["시편 23:1-6"]);
        assert_eq!(refs("고린도 전서 13장 4절부터 7절까지"), vec!["고린도전서 13:4-7"]);
        assert_eq!(refs("요한 1서 4장 8절"), vec!["요한일서 4:8"]);
        assert_eq!(refs("요한일서 4장 8절과 로마스 5장 8절"), vec!["요한일서 4:8", "로마서 5:8"]);
        assert_eq!(refs("창세기 1장"), vec!["창세기 1"]);
        assert_eq!(refs("요한 개시록 21장 4절"), vec!["요한계시록 21:4"]);
    }
    #[test]
    fn written_short_forms() {
        assert_eq!(refs("(요3:16) 하나님이 세상을"), vec!["요한복음 3:16"]);
        assert_eq!(refs("롬 8:28, 시 23:1-6 참고"), vec!["로마서 8:28", "시편 23:1-6"]);
        assert_eq!(refs("빌4:13"), vec!["빌립보서 4:13"]);
    }
    #[test]
    fn no_false_hits() {
        assert!(refs("창문을 열고 3시 10분에 만납시다").is_empty());
        assert!(refs("요한이 3장을 썼다").is_empty() || refs("요한이 3장을 썼다") != vec!["요한복음 3"], "'요한' 만으로는 책이 아님");
        assert!(refs("로마서 200장").is_empty(), "없는 장");
        assert!(refs("사랑장 13장").is_empty());
        assert!(refs("수요일 3장 보세요").is_empty());
    }
    #[test]
    fn split_across_chunks() {
        assert_eq!(ends_with_book("오늘 함께 읽을 말씀은 요한 복음").map(|b| b.id), Some(43));
        assert!(ends_with_book("감사합니다").is_none());
    }
}
