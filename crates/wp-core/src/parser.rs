//! 장절 파서 (Electron 판 parser.js 와 같은 규칙)
//! "요3:16", "요한복음 3장 16절", "행5:27-32", "롬 8", "창1:1-2:3", "롬8:1,28", "요 3 16" 등

use crate::books::{resolve_book, Book};
use once_cell::sync::Lazy;
use regex::Regex;
use serde::Serialize;
use unicode_normalization::UnicodeNormalization;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Range {
    pub sc: i64,
    pub sv: Option<i64>,
    pub ec: i64,
    pub ev: Option<i64>,
}

#[derive(Debug, Clone)]
pub enum Parsed {
    Ok { book: &'static Book, ranges: Vec<Range> },
    Err {
        error: String,
        book: Option<&'static Book>,
        candidates: Vec<&'static Book>,
        incomplete: bool,
        ambiguous: bool,
        rest: String,
    },
}

fn err(msg: impl Into<String>) -> Parsed {
    Parsed::Err { error: msg.into(), book: None, candidates: vec![], incomplete: false, ambiguous: false, rest: String::new() }
}

pub fn normalize_input(s: &str) -> String {
    let s: String = s.nfc().collect();
    s.chars()
        .map(|c| match c {
            '\u{3000}' | '\u{a0}' => ' ',
            '~' | '～' | '〜' | '–' | '—' | '−' => '-',
            '：' => ':',
            c => c,
        })
        .collect::<String>()
        .trim()
        .to_string()
}

static JOHN_N: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(요한)\s*([123])\s*서").unwrap());
static SPLIT: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(\d?\s*[^\d\s:.,\-][^\d:,\-]*?)\s*(\d.*)?$").unwrap());
static DASH_SP: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s*-\s*").unwrap());
static SEPS: Lazy<Regex> = Lazy::new(|| Regex::new(r"[:장절.,，、;]").unwrap());
static NUMTOK: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\d+(-\d+)?$").unwrap());
static RANGE_TOK: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\d+-\d+$").unwrap());
static JANG_NUM: Lazy<Regex> = Lazy::new(|| Regex::new(r"장(\d)").unwrap());
static DOT: Lazy<Regex> = Lazy::new(|| Regex::new(r"(\d)\.(\d)").unwrap());
static COMMAS: Lazy<Regex> = Lazy::new(|| Regex::new(r"[,，、;]+").unwrap());
static SEG: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(\d+)(?::(\d+))?(?:-(\d+)(?::(\d+))?)?$").unwrap());

fn split_book_and_rest(s: &str) -> Option<(String, String)> {
    let s = JOHN_N
        .replace(s, |c: &regex::Captures| {
            let n = match &c[2] { "1" => "일", "2" => "이", _ => "삼" };
            format!("{}{}서", &c[1], n)
        })
        .to_string();
    let m = SPLIT.captures(&s)?;
    Some((m[1].trim().to_string(), m.get(2).map(|x| x.as_str().trim().to_string()).unwrap_or_default()))
}

fn spaces_to_colon(rest: &str) -> String {
    let r = DASH_SP.replace_all(rest.trim(), "-").to_string();
    if SEPS.is_match(&r) {
        return rest.to_string();
    }
    let t: Vec<&str> = r.split_whitespace().collect();
    if t.len() < 2 || !t.iter().all(|x| NUMTOK.is_match(x)) {
        return rest.to_string();
    }
    let has_dash = |x: &&str| x.contains('-');
    if t.len() == 2 {
        return format!("{}:{}", t[0], t[1]);
    }
    if t.len() == 3 && !t[0].contains('-') && RANGE_TOK.is_match(t[1]) && !t[2].contains('-') {
        let (v1, c2) = t[1].split_once('-').unwrap();
        return format!("{}:{}-{}:{}", t[0], v1, c2, t[2]);
    }
    if t.len() == 3 && !t.iter().any(has_dash) {
        return format!("{}:{}-{}", t[0], t[1], t[2]);
    }
    if t.len() == 4 && !t.iter().any(has_dash) {
        return format!("{}:{}-{}:{}", t[0], t[1], t[2], t[3]);
    }
    rest.to_string()
}

fn normalize_rest(rest: &str) -> String {
    let r: String = spaces_to_colon(rest).chars().filter(|c| !c.is_whitespace()).collect();
    let r = JANG_NUM.replace_all(&r, ":$1").replace('장', "").replace('절', "");
    let r = DOT.replace_all(&r, "$1:$2").to_string();
    let r = COMMAS.replace_all(&r, ",").to_string();
    r.trim_matches(',').to_string()
}

pub fn parse_reference(input: &str) -> Parsed {
    let s = normalize_input(input);
    if s.is_empty() {
        return err("검색어를 입력하세요.");
    }
    let Some((book_part, rest_raw)) = split_book_and_rest(&s) else {
        return err("성경 책 이름을 인식할 수 없습니다.");
    };
    if book_part.is_empty() {
        return err("성경 책 이름을 인식할 수 없습니다.");
    }
    let (book, cands) = resolve_book(&book_part);
    let Some(book) = book else {
        if cands.len() > 1 {
            return Parsed::Err {
                error: format!("'{}'에 해당하는 책이 여러 권입니다. 후보에서 선택하세요.", book_part),
                book: None,
                candidates: cands,
                incomplete: false,
                ambiguous: true,
                rest: rest_raw,
            };
        }
        return err(format!("'{}'은(는) 알 수 없는 성경 책 이름입니다.", book_part));
    };
    let incomplete = |msg: String| Parsed::Err { error: msg, book: Some(book), candidates: vec![], incomplete: true, ambiguous: false, rest: String::new() };
    let with_book = |msg: String| Parsed::Err { error: msg, book: Some(book), candidates: vec![], incomplete: false, ambiguous: false, rest: String::new() };
    if rest_raw.is_empty() {
        return incomplete(format!("{}: 장을 입력하세요. (예: {}1:1)", book.name, book.short));
    }
    let rest = normalize_rest(&rest_raw);
    if rest.is_empty() {
        return incomplete(format!("{}: 장을 입력하세요.", book.name));
    }
    let mut ranges = vec![];
    let mut ctx: Option<i64> = None;
    for seg in rest.split(',') {
        let Some(m) = SEG.captures(seg) else {
            return with_book(format!("'{}' 장절 형식을 인식할 수 없습니다. (예: 요3:16, 행5:27-32)", rest_raw));
        };
        let n = |i: usize| m.get(i).and_then(|x| x.as_str().parse::<i64>().ok());
        let a = n(1).unwrap_or(0);
        let (b, c, d) = (n(2), n(3), n(4));
        let r = match (b, d) {
            (None, None) => {
                if let Some(ch) = ctx {
                    Range { sc: ch, sv: Some(a), ec: ch, ev: Some(c.unwrap_or(a)) }
                } else if book.single_chapter {
                    ctx = Some(1);
                    Range { sc: 1, sv: Some(a), ec: 1, ev: Some(c.unwrap_or(a)) }
                } else {
                    Range { sc: a, sv: None, ec: c.unwrap_or(a), ev: None }
                }
            }
            (Some(b), None) => {
                ctx = Some(a);
                Range { sc: a, sv: Some(b), ec: a, ev: Some(c.unwrap_or(b)) }
            }
            (Some(b), Some(d)) => {
                let c = c.unwrap_or(a);
                ctx = Some(c);
                Range { sc: a, sv: Some(b), ec: c, ev: Some(d) }
            }
            (None, Some(d)) => {
                let c = c.unwrap_or(a);
                ctx = Some(c);
                Range { sc: a, sv: None, ec: c, ev: Some(d) }
            }
        };
        if r.sc < 1 || r.ec < 1 || r.sv.is_some_and(|v| v < 1) || r.ev.is_some_and(|v| v < 1) {
            return with_book("장과 절 번호는 1 이상이어야 합니다.".into());
        }
        if r.ec < r.sc || (r.ec == r.sc && matches!((r.sv, r.ev), (Some(s), Some(e)) if e < s)) {
            return with_book("구절 범위의 끝이 시작보다 앞에 있습니다.".into());
        }
        ranges.push(r);
    }
    Parsed::Ok { book, ranges }
}

/// 입력이 장절 검색처럼 보이는지 (자동 모드 판별용)
pub fn looks_like_reference(input: &str) -> bool {
    let s = normalize_input(input);
    if !s.chars().any(|c| c.is_ascii_digit()) {
        return false;
    }
    match parse_reference(&s) {
        Parsed::Ok { .. } => true,
        Parsed::Err { ambiguous, book, .. } => ambiguous || book.is_some(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ok(s: &str) -> (i64, Vec<Range>) {
        match parse_reference(s) {
            Parsed::Ok { book, ranges } => (book.id, ranges),
            Parsed::Err { error, .. } => panic!("{s}: {error}"),
        }
    }
    #[test]
    fn refs() {
        assert_eq!(ok("요3:16"), (43, vec![Range { sc: 3, sv: Some(16), ec: 3, ev: Some(16) }]));
        assert_eq!(ok("요한복음 3장 16절").1, vec![Range { sc: 3, sv: Some(16), ec: 3, ev: Some(16) }]);
        assert_eq!(ok("행5:27-32").1, vec![Range { sc: 5, sv: Some(27), ec: 5, ev: Some(32) }]);
        assert_eq!(ok("롬 8").1, vec![Range { sc: 8, sv: None, ec: 8, ev: None }]);
        assert_eq!(ok("창1:1-2:3").1, vec![Range { sc: 1, sv: Some(1), ec: 2, ev: Some(3) }]);
        assert_eq!(ok("롬8:1,28").1.len(), 2);
        assert_eq!(ok("요 3 16").1, vec![Range { sc: 3, sv: Some(16), ec: 3, ev: Some(16) }]);
        assert_eq!(ok("요한1서 1:9").0, 62);
        assert_eq!(ok("유 3").1, vec![Range { sc: 1, sv: Some(3), ec: 1, ev: Some(3) }]);
        assert_eq!(ok("John 3:16").0, 43);
        assert!(looks_like_reference("시 23"));
        assert!(!looks_like_reference("사랑"));
    }
}
