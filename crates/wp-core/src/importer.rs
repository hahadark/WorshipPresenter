//! [트랙 C] 성경 번역본 가져오기 (Electron importer.js): JSON 4형식 · CSV/TSV · Zefania XML · 베들레헴 .bdb(SQLite)
//! 1) analyze_file(): 파일을 읽어 검증 보고서 (DB 그대로)  2) commit(): 오류가 없으면 한 트랜잭션으로 반영
//! 삭제 delete_version. 표 모양은 db.rs 와 같다 (BibleVerses + FTS).

use crate::books::{get_book, resolve_book, Book, BOOKS};
use crate::textnorm::{clean_display_text, compact, search_normalize};
use anyhow::Result;
use once_cell::sync::Lazy;
use regex::Regex;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;

#[derive(Clone, Debug)]
struct Raw {
    book: Value,
    index: Option<usize>,
    chapter: Value,
    verse: Value,
    text: String,
    line: Option<usize>,
    note_only: bool,
}

#[derive(Default, Clone, Debug)]
struct Parsed {
    meta: Value,
    verses: Vec<Raw>,
    headings: Vec<Raw>,
    notes: Vec<Raw>,
}

#[derive(Clone, Debug, Default)]
pub struct Row {
    pub book: i64,
    pub chapter: i64,
    pub verse: i64,
    pub text: String,
}

#[derive(Clone, Debug, Default)]
pub struct Data {
    pub verses: Vec<Row>,
    pub headings: Vec<Row>,
    pub notes: Vec<Row>,
}

#[derive(Clone, Debug)]
pub struct Analysis {
    /// { file, format, encoding, errors, warnings, notes, stats }
    pub report: Value,
    pub data: Option<Data>,
    pub meta: Value,
}

fn raw(book: Value, index: Option<usize>, ch: Value, v: Value, t: &Value) -> Raw {
    let text = match t { Value::String(s) => s.clone(), Value::Null => String::new(), o => o.as_str().map(String::from).unwrap_or_else(|| o.to_string()) };
    Raw { book, index, chapter: ch, verse: v, text, line: None, note_only: false }
}

// ---------- 인코딩 ----------
pub fn decode_buffer(buf: &[u8]) -> (String, &'static str, bool) {
    if buf.starts_with(&[0xef, 0xbb, 0xbf]) {
        return (String::from_utf8_lossy(&buf[3..]).into(), "UTF-8 (BOM)", false);
    }
    if buf.starts_with(&[0xff, 0xfe]) {
        return (encoding_rs::UTF_16LE.decode_without_bom_handling(&buf[2..]).0.into(), "UTF-16LE", false);
    }
    if buf.starts_with(&[0xfe, 0xff]) {
        return (encoding_rs::UTF_16BE.decode_without_bom_handling(&buf[2..]).0.into(), "UTF-16BE", false);
    }
    match std::str::from_utf8(buf) {
        Ok(s) => (s.to_string(), "UTF-8", false),
        Err(_) => (encoding_rs::EUC_KR.decode(buf).0.into(), "EUC-KR (CP949)", true),
    }
}

fn first<'a>(o: &'a Value, keys: &[&str]) -> &'a Value {
    for k in keys {
        if !o[*k].is_null() {
            return &o[*k];
        }
    }
    &Value::Null
}

// ---------- JSON ----------
fn parse_json(text: &str, report: &mut Value) -> Result<Parsed, String> {
    let data: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let mut out = Parsed::default();
    if data.is_object() && data["verses"].is_array() {
        out.meta = first(&data, &["version", "meta"]).clone();
        for r in data["verses"].as_array().unwrap() {
            out.verses.push(raw(first(r, &["book", "book_id", "b"]).clone(), None, first(r, &["chapter", "c"]).clone(), first(r, &["verse", "v"]).clone(), first(r, &["text", "t"])));
        }
        for h in data["headings"].as_array().into_iter().flatten() {
            let v = if h["verse"].is_null() { json!(1) } else { h["verse"].clone() };
            out.headings.push(raw(h["book"].clone(), None, h["chapter"].clone(), v, &h["text"]));
        }
        for n in data["notes"].as_array().into_iter().flatten() {
            out.notes.push(raw(n["book"].clone(), None, n["chapter"].clone(), n["verse"].clone(), &n["text"]));
        }
        report["format"] = json!("JSON (워십프리젠터)");
        return Ok(out);
    }
    if let Some(arr) = data.as_array().filter(|a| !a.is_empty()) {
        let f = &arr[0];
        if (!f["verse"].is_null() || !f["v"].is_null()) && !f["chapters"].is_array() {
            for r in arr {
                out.verses.push(raw(first(r, &["book", "book_id", "b"]).clone(), None, first(r, &["chapter", "c"]).clone(), first(r, &["verse", "v"]).clone(), first(r, &["text", "t"])));
            }
            report["format"] = json!("JSON (구절 배열)");
            return Ok(out);
        }
        if f["chapters"].is_array() {
            for (bi, b) in arr.iter().enumerate() {
                let bv = first(b, &["book", "name", "abbrev"]).clone();
                for (ci, chap) in b["chapters"].as_array().into_iter().flatten().enumerate() {
                    if let Some(vs) = chap.as_array() {
                        for (vi, t) in vs.iter().enumerate() {
                            out.verses.push(raw(bv.clone(), Some(bi), json!(ci + 1), json!(vi + 1), t));
                        }
                    } else if chap.is_object() {
                        if let Some(vs) = chap["verses"].as_array() {
                            let c = if chap["chapter"].is_null() { json!(ci + 1) } else { chap["chapter"].clone() };
                            for (vi, t) in vs.iter().enumerate() {
                                let v = if t["verse"].is_null() { json!(vi + 1) } else { t["verse"].clone() };
                                let tx = if t.is_string() { t.clone() } else { t["text"].clone() };
                                out.verses.push(raw(bv.clone(), Some(bi), c.clone(), v, &tx));
                            }
                        } else {
                            for (vk, t) in chap.as_object().unwrap() {
                                out.verses.push(raw(bv.clone(), Some(bi), json!(ci + 1), json!(vk.parse::<i64>().unwrap_or(-1)), t));
                            }
                        }
                    }
                }
            }
            report["format"] = json!("JSON (책별 장 배열)");
            report["notes"].as_array_mut().unwrap().push(json!("책 이름을 인식하지 못하면 배열 순서(66권 표준 순서)로 책을 지정합니다."));
            return Ok(out);
        }
    }
    if let Some(o) = data.as_object() {
        for (bk, chapters) in o {
            let Some(cs) = chapters.as_object() else { continue };
            for (ck, verses) in cs {
                let Some(vs) = verses.as_object() else { continue };
                for (vk, t) in vs {
                    out.verses.push(raw(json!(bk), None, json!(ck.parse::<i64>().unwrap_or(-1)), json!(vk.parse::<i64>().unwrap_or(-1)), t));
                }
            }
        }
        report["format"] = json!("JSON (책/장/절 객체)");
        return Ok(out);
    }
    Err(crate::i18n::tr("지원하지 않는 JSON 구조입니다.").into())
}

// ---------- CSV ----------
pub fn parse_csv_rows(text: &str) -> Vec<Vec<String>> {
    let mut rows = vec![];
    let (mut row, mut field, mut q) = (vec![], String::new(), false);
    let cs: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < cs.len() {
        let ch = cs[i];
        if q {
            if ch == '"' {
                if cs.get(i + 1) == Some(&'"') { field.push('"'); i += 1; } else { q = false; }
            } else { field.push(ch); }
        } else if ch == '"' { q = true; }
        else if ch == ',' || ch == '\t' { row.push(std::mem::take(&mut field)); }
        else if ch == '\n' { row.push(std::mem::take(&mut field)); rows.push(std::mem::take(&mut row)); }
        else if ch != '\r' { field.push(ch); }
        i += 1;
    }
    if !field.is_empty() || !row.is_empty() { row.push(field); rows.push(row); }
    rows.into_iter().filter(|r: &Vec<String>| r.iter().any(|c| !c.trim().is_empty())).collect()
}

fn parse_csv(text: &str, report: &mut Value) -> Result<Parsed, String> {
    let rows = parse_csv_rows(text);
    if rows.is_empty() { return Err(crate::i18n::tr("CSV 파일이 비어 있습니다.").into()); }
    let head: Vec<String> = rows[0].iter().map(|h| h.trim().to_lowercase()).collect();
    let idx = |names: &[&str]| head.iter().position(|h| names.contains(&h.as_str()));
    let (mut ib, mut ic, mut iv, mut it) = (idx(&["book", "book_id", "b", "책", "권"]), idx(&["chapter", "c", "장"]), idx(&["verse", "v", "절"]), idx(&["text", "t", "본문", "내용"]));
    let mut start = 1;
    if ib.is_none() || ic.is_none() || iv.is_none() || it.is_none() {
        (ib, ic, iv, it, start) = (Some(0), Some(1), Some(2), Some(3), 0);
        report["notes"].as_array_mut().unwrap().push(json!("CSV 머리글이 없어 열 순서를 책,장,절,본문으로 간주했습니다."));
    }
    let (ib, ic, iv, it) = (ib.unwrap(), ic.unwrap(), iv.unwrap(), it.unwrap());
    let mut out = Parsed::default();
    for (r, row) in rows.iter().enumerate().skip(start) {
        let g = |i: usize| row.get(i).cloned().unwrap_or_default();
        let text = if row.len() > it { row[it..].join(",") } else { String::new() };
        out.verses.push(Raw { book: json!(g(ib)), index: None, chapter: json!(g(ic)), verse: json!(g(iv)), text, line: Some(r + 1), note_only: false });
    }
    report["format"] = json!("CSV");
    Ok(out)
}

// ---------- Zefania XML ----------
static HEX_ENT: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)&#x([0-9a-f]+);").unwrap());
static DEC_ENT: Lazy<Regex> = Lazy::new(|| Regex::new(r"&#(\d+);").unwrap());
fn decode_entities(s: &str) -> String {
    let s = HEX_ENT.replace_all(s, |c: &regex::Captures| u32::from_str_radix(&c[1], 16).ok().and_then(char::from_u32).map(String::from).unwrap_or_default());
    let s = DEC_ENT.replace_all(&s, |c: &regex::Captures| c[1].parse::<u32>().ok().and_then(char::from_u32).map(String::from).unwrap_or_default());
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}
static ATTR: Lazy<Regex> = Lazy::new(|| Regex::new(r#"([\w:-]+)\s*=\s*("([^"]*)"|'([^']*)')"#).unwrap());
fn attrs(tag: &str) -> BTreeMap<String, String> {
    ATTR.captures_iter(tag).map(|m| (m[1].to_lowercase(), decode_entities(m.get(3).or(m.get(4)).map(|x| x.as_str()).unwrap_or("")))).collect()
}
static NOTE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?is)<NOTE\b[^>]*>(.*?)</NOTE>").unwrap());
static NOTE_EMPTY: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)<NOTE\b[^>]*/>").unwrap());
static BR_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)<BR\b[^>]*/?>").unwrap());
static TAG_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"<[^>]+>").unwrap());
static INFO: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?is)<INFORMATION>(.*?)</INFORMATION>").unwrap());
static ROOT: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)<XMLBIBLE\b[^>]*>").unwrap());
static BOOK_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?is)<BIBLEBOOK\b([^>]*)>(.*?)</BIBLEBOOK>").unwrap());
static CHAP_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?is)<CHAPTER\b([^>]*)>(.*?)</CHAPTER>").unwrap());
static TOK_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?is)<CAPTION\b([^>]*)>(.*?)</CAPTION>|<VERS\b([^>]*)>(.*?)</VERS>|<VERS\b([^>]*)/>").unwrap());

fn parse_zefania(text: &str, report: &mut Value) -> Result<Parsed, String> {
    let mut out = Parsed::default();
    if let Some(info) = INFO.captures(text) {
        let get = |t: &str| Regex::new(&format!(r"(?is)<{t}[^>]*>(.*?)</{t}>")).unwrap().captures(&info[1]).map(|m| decode_entities(&m[1]).trim().to_string()).unwrap_or_default();
        let src = { let s = get("source"); if s.is_empty() { get("publisher") } else { s } };
        out.meta = json!({ "name": get("title"), "language": get("language"), "copyright": get("rights"), "source": src, "identifier": get("identifier") });
    }
    let root = ROOT.find(text).ok_or("Zefania XML(<XMLBIBLE>) 형식이 아닙니다.")?;
    let ra = attrs(root.as_str());
    if out.meta["name"].as_str().unwrap_or("").is_empty() {
        if let Some(n) = ra.get("biblename") { out.meta["name"] = json!(n); }
    }
    for bm in BOOK_RE.captures_iter(text) {
        let ba = attrs(&bm[1]);
        let bv = json!(ba.get("bnumber").or(ba.get("bname")).or(ba.get("bsname")).cloned().unwrap_or_default());
        for cm in CHAP_RE.captures_iter(&bm[2]) {
            let ca = attrs(&cm[1]);
            let ch = json!(ca.get("cnumber").cloned().unwrap_or_default());
            let mut pending: Vec<String> = vec![];
            for t in TOK_RE.captures_iter(&cm[2]) {
                if let Some(cap) = t.get(2) {
                    pending.push(clean_display_text(&decode_entities(&TAG_RE.replace_all(cap.as_str(), ""))));
                } else {
                    let va = attrs(t.get(3).or(t.get(5)).map(|x| x.as_str()).unwrap_or(""));
                    let vn = json!(va.get("vnumber").cloned().unwrap_or_default());
                    let inner = t.get(4).map(|x| x.as_str()).unwrap_or("");
                    let mut notes = vec![];
                    let body = NOTE_RE.replace_all(inner, |c: &regex::Captures| { notes.push(clean_display_text(&decode_entities(&c[1]))); String::new() }).to_string();
                    let body = NOTE_EMPTY.replace_all(&body, "");
                    let body = BR_RE.replace_all(&body, " ");
                    let body = decode_entities(&TAG_RE.replace_all(&body, ""));
                    out.verses.push(Raw { book: bv.clone(), index: None, chapter: ch.clone(), verse: vn.clone(), text: body, line: None, note_only: false });
                    for c in pending.drain(..) { out.headings.push(raw(bv.clone(), None, ch.clone(), vn.clone(), &json!(c))); }
                    for n in notes { out.notes.push(raw(bv.clone(), None, ch.clone(), vn.clone(), &json!(n))); }
                }
            }
        }
    }
    report["format"] = json!("Zefania XML");
    Ok(out)
}

// ---------- 베들레헴 (.bdb, SQLite) ----------
static SLASH_NOTE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s/\s*\*").unwrap());
static SLASH_SPLIT: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s/\s*").unwrap());
static LEAD_HEAD: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*(\[(?:제\s*)?\d+\s*권\]|<([^<>a-zA-Z/][^<>]*)>)\s*").unwrap());
static MID_HEAD: Lazy<Regex> = Lazy::new(|| Regex::new(r"<([^<>a-zA-Z/][^<>]*)>\s*").unwrap());
static FN_MARK: Lazy<Regex> = Lazy::new(|| Regex::new(r"\[[a-z]{1,2}\]").unwrap());

/// 본문에서 단락 표제·각주를 떼어 낸다 → (본문, 표제들, 각주들)
pub fn split_bethlehem_text(raw: &str) -> (String, Vec<String>, Vec<String>) {
    let mut text = raw.replace("\r\n", " ").replace('\n', " ");
    let (mut heads, mut notes) = (vec![], vec![]);
    if let Some(m) = SLASH_NOTE.find(&text) {
        for n in SLASH_SPLIT.split(&text[m.start()..]) {
            let t = n.trim_start_matches('*').trim();
            if !t.is_empty() { notes.push(t.to_string()); }
        }
        text.truncate(m.start());
    }
    while let Some(c) = LEAD_HEAD.captures(&text) {
        let h = c.get(2).map(|x| x.as_str().to_string()).unwrap_or_else(|| { let s = &c[1]; s[1..s.len() - 1].to_string() });
        heads.push(h.trim().to_string());
        let n = c.get(0).unwrap().end();
        text = text[n..].to_string();
    }
    let text = MID_HEAD.replace_all(&text, |c: &regex::Captures| { heads.push(c[1].trim().to_string()); String::new() }).to_string();
    let text = FN_MARK.replace_all(&text, "").replace('*', "");
    (text, heads, notes)
}

fn parse_bethlehem(file: &Path, report: &mut Value) -> Result<Parsed, String> {
    let db = Connection::open_with_flags(file, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|e| e.to_string())?;
    let has: Option<String> = db.query_row("SELECT name FROM sqlite_master WHERE type='table' AND name='Bible'", [], |r| r.get(0)).ok();
    if has.is_none() { return Err(crate::i18n::tr("베들레헴 성경 형식(Bible 테이블)이 아닙니다. 주석(.cdb)·사전 파일은 가져올 수 없습니다.").into()); }
    let stem = file.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let name = Regex::new(r"^\d{10,}-").unwrap().replace(&stem, "").to_string();
    let mut out = Parsed { meta: json!({ "name": name }), ..Default::default() };
    let mut st = db.prepare("SELECT book, chapter, verse, btext FROM Bible ORDER BY book, chapter, verse").map_err(|e| e.to_string())?;
    let rows = st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, r.get::<_, Option<String>>(3)?.unwrap_or_default()))).map_err(|e| e.to_string())?;
    let (mut nh, mut nn) = (0, 0);
    for row in rows.flatten() {
        let (b, c, v, t) = row;
        let (text, heads, notes) = split_bethlehem_text(&t);
        let note_only = text.trim().is_empty() && !notes.is_empty();
        out.verses.push(Raw { book: json!(b), index: None, chapter: json!(c), verse: json!(v), text, line: None, note_only });
        for h in heads { out.headings.push(raw(json!(b), None, json!(c), json!(v), &json!(h))); nh += 1; }
        for n in notes { out.notes.push(raw(json!(b), None, json!(c), json!(v), &json!(n))); nn += 1; }
    }
    report["format"] = json!("베들레헴 성경 (.bdb)");
    report["encoding"] = json!("UTF-8 (SQLite)");
    if nh > 0 { report["notes"].as_array_mut().unwrap().push(json!(format!("본문 안의 단락 표제 {nh}개를 본문과 분리했습니다. (템플릿에서 표시 여부 선택)"))); }
    if nn > 0 { report["notes"].as_array_mut().unwrap().push(json!(format!("본문 안의 각주 {nn}개를 분리하고 각주 표시(*, [a])를 지웠습니다."))); }
    Ok(out)
}

// ---------- 검증 ----------
const MAX_LIST: usize = 30;
fn add_issue(list: &mut Value, msg: String) {
    let a = list.as_array_mut().unwrap();
    if a.len() < MAX_LIST { a.push(json!(msg)); } else if a.len() == MAX_LIST { a.push(json!("… (이하 생략)")); }
}

fn book_from(v: &Value, idx: Option<usize>) -> Option<&'static Book> {
    let s = match v { Value::Null => String::new(), Value::String(s) => s.trim().to_string(), o => o.to_string() };
    if s.is_empty() { return idx.and_then(|i| get_book(i as i64 + 1)); }
    if s.chars().all(|c| c.is_ascii_digit()) { return get_book(s.parse().ok()?); }
    resolve_book(&s).0
}
fn int(v: &Value) -> Option<i64> {
    match v { Value::Number(n) => n.as_f64().filter(|f| f.fract() == 0.0).map(|f| f as i64), Value::String(s) => s.trim().parse().ok(), _ => None }
}

fn validate(p: &Parsed, report: &mut Value) -> Data {
    let mut verses: Vec<Row> = vec![];
    let mut keys = HashSet::new();
    let mut unknown = BTreeSet::new();
    for r in &p.verses {
        let where_ = match r.line { Some(l) => format!("{l}행"), None => format!("{} {}:{}", r.book.as_str().map(String::from).unwrap_or_else(|| r.book.to_string()), r.chapter, r.verse) };
        let Some(book) = book_from(&r.book, r.index) else { unknown.insert(r.book.as_str().map(String::from).unwrap_or_else(|| r.book.to_string())); continue };
        let Some(ch) = int(&r.chapter).filter(|c| (1..=200).contains(c)) else { add_issue(&mut report["errors"], format!("장 번호 오류: {where_}")); continue };
        let Some(v) = int(&r.verse).filter(|v| (0..=200).contains(v)) else { add_issue(&mut report["errors"], format!("절 번호 오류: {where_}")); continue };
        if v == 0 { add_issue(&mut report["warnings"], format!("0절(시편 표제 등)은 본문에서 제외했습니다: {} {}:0", book.name, ch)); continue; }
        let text = clean_display_text(&r.text);
        if text.is_empty() && r.note_only { add_issue(&mut report["warnings"], format!("원본에 본문이 없는 절(각주만 있음, 제외): {} {}:{}", book.name, ch, v)); continue; }
        if text.is_empty() { add_issue(&mut report["errors"], format!("본문 누락: {} {}:{}", book.name, ch, v)); continue; }
        if text.contains('\u{fffd}') { add_issue(&mut report["warnings"], format!("깨진 문자(인코딩 확인 필요): {} {}:{}", book.name, ch, v)); }
        if !keys.insert((book.id, ch, v)) { add_issue(&mut report["errors"], format!("중복 구절: {} {}:{}", book.name, ch, v)); continue; }
        verses.push(Row { book: book.id, chapter: ch, verse: v, text });
    }
    for b in unknown { add_issue(&mut report["errors"], format!("알 수 없는 책 이름/번호: '{b}'")); }
    let mut st: BTreeMap<i64, BTreeMap<i64, BTreeSet<i64>>> = BTreeMap::new();
    for v in &verses { st.entry(v.book).or_default().entry(v.chapter).or_default().insert(v.verse); }
    for b in BOOKS.iter() {
        let Some(m) = st.get(&b.id) else { add_issue(&mut report["warnings"], format!("책 없음: {}", b.name)); continue };
        // 파일에 터무니없이 큰 장·절 번호가 있어도 점검이 끝없이 돌지 않게 상한
        let max_ch = m.keys().max().copied().unwrap_or(0).min(b.chapters.max(1) + 50);
        if max_ch != b.chapters { add_issue(&mut report["warnings"], format!("{}: {}장 (표준 {}장)", b.name, max_ch, b.chapters)); }
        for c in 1..=max_ch {
            let Some(set) = m.get(&c) else { add_issue(&mut report["errors"], format!("장 누락: {} {}장", b.name, c)); continue };
            let max_v = set.iter().max().copied().unwrap_or(0).min(400);
            let missing: Vec<i64> = (1..=max_v).filter(|v| !set.contains(v)).collect();
            if !missing.is_empty() {
                let list = missing.iter().take(5).map(|x| x.to_string()).collect::<Vec<_>>().join(",");
                add_issue(&mut report["warnings"], format!("절 번호 누락: {} {}장 {}{}절 (원본 편집 체계일 수 있음)", b.name, c, list, if missing.len() > 5 { "…" } else { "" }));
            }
        }
    }
    let side = |list: &[Raw], def_verse: i64| -> Vec<Row> {
        list.iter().filter_map(|h| {
            let b = book_from(&h.book, None)?;
            if h.text.is_empty() { return None; }
            Some(Row { book: b.id, chapter: int(&h.chapter).unwrap_or(0), verse: int(&h.verse).filter(|x| *x != 0).unwrap_or(def_verse), text: clean_display_text(&h.text) })
        }).collect()
    };
    let headings = side(&p.headings, 1);
    let notes = side(&p.notes, 0);
    verses.sort_by_key(|v| (v.book, v.chapter, v.verse));
    report["stats"] = json!({ "verses": verses.len(), "books": st.len(), "headings": headings.len(), "notes": notes.len() });
    Data { verses, headings, notes }
}

static ID_OK: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)^[a-z0-9_-]{2,32}$").unwrap());

pub fn validate_meta(meta: &Value) -> Vec<String> {
    let s = |k: &str| meta[k].as_str().unwrap_or("").trim().to_string();
    let mut e = vec![];
    if !ID_OK.is_match(&s("id")) { e.push("번역본 ID는 영문·숫자·-·_ 2~32자로 입력하세요. (예: krv)".into()); }
    if s("name").is_empty() { e.push("번역본 이름을 입력하세요.".into()); }
    if s("source").is_empty() { e.push("데이터 출처를 입력하세요.".into()); }
    if s("license").is_empty() { e.push("사용 허가(라이선스) 정보를 입력하세요. 권리 확인이 안 된 번역본은 가져올 수 없습니다.".into()); }
    e
}

/// 파일을 읽어 분석 (DB 는 건드리지 않음)
pub fn analyze_file(file: &Path) -> Analysis {
    let mut report = json!({ "file": file.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(), "format": "", "encoding": "", "errors": [], "warnings": [], "notes": [], "stats": {} });
    let buf = match std::fs::read(file) { Ok(b) => b, Err(e) => { report["errors"] = json!([format!("파일을 읽을 수 없습니다: {e}")]); return Analysis { report, data: None, meta: json!({}) } } };
    let parsed = if buf.starts_with(b"SQLite format 3\0") {
        parse_bethlehem(file, &mut report)
    } else {
        let (text, enc, conv) = decode_buffer(&buf);
        report["encoding"] = json!(enc);
        if conv { report["notes"].as_array_mut().unwrap().push(json!("UTF-8이 아니어서 EUC-KR(CP949)로 변환했습니다. 본문이 올바르게 보이는지 확인하세요.")); }
        let ext = file.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let t = text.trim_start();
        if ext == "json" || t.starts_with('[') || t.starts_with('{') { parse_json(&text, &mut report) }
        else if ext == "xml" || t.starts_with("<?xml") || text.to_uppercase().contains("<XMLBIBLE") { parse_zefania(&text, &mut report) }
        else if ["csv", "tsv", "txt"].contains(&ext.as_str()) { parse_csv(&text, &mut report) }
        else { Err(crate::i18n::tr("지원하지 않는 파일 형식입니다. (JSON, CSV, Zefania XML, 베들레헴 .bdb)").into()) }
    };
    let parsed = match parsed { Ok(p) => p, Err(e) => { report["errors"].as_array_mut().unwrap().push(json!(format!("파일을 해석할 수 없습니다: {e}"))); return Analysis { report, data: None, meta: json!({}) } } };
    let data = validate(&parsed, &mut report);
    if data.verses.is_empty() { report["errors"].as_array_mut().unwrap().push(json!("가져올 수 있는 구절이 없습니다.")); }
    Analysis { report, data: Some(data), meta: if parsed.meta.is_object() { parsed.meta } else { json!({}) } }
}

/// 번역본 전체를 한 트랜잭션으로 바꾼다 (실패하면 그대로)
pub fn replace_version(db: &Connection, meta: &Value, d: &Data) -> Result<()> {
    let id = meta["id"].as_str().unwrap_or("").trim().to_string();
    let tx = db.unchecked_transaction()?;
    tx.execute("INSERT INTO BibleVersesFts(BibleVersesFts, rowid, search_compact) SELECT 'delete', rowid, search_compact FROM BibleVerses WHERE version_id = ?", [&id])?;
    for t in ["BibleVerses", "BibleHeadings", "BibleNotes"] { tx.execute(&format!("DELETE FROM {t} WHERE version_id = ?"), [&id])?; }
    let s = |k: &str| meta[k].as_str().unwrap_or("").trim().to_string();
    let lang = if s("language").is_empty() { "ko".into() } else { s("language") };
    tx.execute("INSERT OR REPLACE INTO BibleVersions (id, name, language, copyright, license, source, imported_at, verse_count) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        params![id, s("name"), lang, s("copyright"), s("license"), s("source"), crate::util::iso_now(), d.verses.len() as i64])?;
    {
        let mut iv = tx.prepare("INSERT INTO BibleVerses (version_id, book_id, chapter, verse, text, search_text, search_compact) VALUES (?, ?, ?, ?, ?, ?, ?)")?;
        let mut ifs = tx.prepare("INSERT INTO BibleVersesFts (rowid, search_compact) VALUES (?, ?)")?;
        for v in &d.verses {
            let sc = compact(&v.text);
            iv.execute(params![id, v.book, v.chapter, v.verse, v.text, search_normalize(&v.text), sc])?;
            ifs.execute(params![tx.last_insert_rowid(), sc])?;
        }
        let mut ih = tx.prepare("INSERT INTO BibleHeadings (version_id, book_id, chapter, verse, text) VALUES (?, ?, ?, ?, ?)")?;
        for h in &d.headings { ih.execute(params![id, h.book, h.chapter, h.verse, h.text])?; }
        let mut inn = tx.prepare("INSERT INTO BibleNotes (version_id, book_id, chapter, verse, text) VALUES (?, ?, ?, ?, ?)")?;
        for n in &d.notes { inn.execute(params![id, n.book, n.chapter, n.verse, n.text])?; }
    }
    tx.commit()?;
    Ok(())
}

/// 분석 결과를 DB 에 반영 → 구절 수. 오류가 있거나 메타가 불완전하면 아무것도 바꾸지 않는다
pub fn commit(core: &mut crate::core::Core, a: &Analysis, meta: &Value) -> Result<usize, Vec<String>> {
    let me = validate_meta(meta);
    if !me.is_empty() { return Err(me); }
    let errs: Vec<String> = a.report["errors"].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(String::from)).collect();
    let Some(d) = a.data.as_ref().filter(|_| errs.is_empty()) else {
        let mut e = vec!["검증 오류가 있어 가져올 수 없습니다.".to_string()];
        e.extend(errs);
        return Err(e);
    };
    replace_version(&core.store.db, meta, d).map_err(|e| vec![e.to_string()])?;
    let _ = core.store.reload_structure();
    let id = meta["id"].as_str().unwrap_or("").trim().to_string();
    if core.version_id().is_empty() { core.set_version(&id); }
    core.emit(crate::core::Ev::Ui);
    Ok(d.verses.len())
}

/// 번역본 삭제 (쓰던 것이면 첫 번역본으로)
pub fn delete_version(core: &mut crate::core::Core, id: &str) -> Result<()> {
    let db = &core.store.db;
    let tx = db.unchecked_transaction()?;
    tx.execute("INSERT INTO BibleVersesFts(BibleVersesFts, rowid, search_compact) SELECT 'delete', rowid, search_compact FROM BibleVerses WHERE version_id = ?", [id])?;
    for t in ["BibleVerses", "BibleHeadings", "BibleNotes"] { tx.execute(&format!("DELETE FROM {t} WHERE version_id = ?"), [id])?; }
    tx.execute("DELETE FROM BibleVersions WHERE id = ?", [id])?;
    tx.commit()?;
    core.store.reload_structure()?;
    if core.version_id() == id {
        let first = core.store.list_versions().first().map(|v| v.id.clone()).unwrap_or_default();
        core.set_version(&first);
    }
    core.emit(crate::core::Ev::Ui);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tmp(name: &str, body: &[u8]) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("wp-imp-{}-{name}", std::process::id()));
        std::fs::write(&p, body).unwrap();
        p
    }
    #[test]
    fn json_and_csv_and_xml() {
        let a = analyze_file(&tmp("a.json", r#"{"version":{"name":"시험"},"verses":[{"book":"창","chapter":1,"verse":1,"text":"태초에"},{"book":1,"chapter":1,"verse":1,"text":"중복"}]}"#.as_bytes()));
        assert_eq!(a.report["format"], json!("JSON (워십프리젠터)"));
        assert_eq!(a.report["stats"]["verses"], json!(1));
        assert!(a.report["errors"].as_array().unwrap().iter().any(|e| e.as_str().unwrap().starts_with("중복 구절")));
        let c = analyze_file(&tmp("b.csv", "책,장,절,본문\n요,3,16,\"하나님이, 세상을\"\n".as_bytes()));
        assert_eq!(c.data.unwrap().verses[0].text, "하나님이, 세상을");
        let x = analyze_file(&tmp("c.xml", r#"<?xml version="1.0"?><XMLBIBLE biblename="T"><BIBLEBOOK bnumber="43"><CHAPTER cnumber="3"><CAPTION>제목</CAPTION><VERS vnumber="16">하나님이<NOTE>각주</NOTE> 세상을</VERS></CHAPTER></BIBLEBOOK></XMLBIBLE>"#.as_bytes()));
        assert_eq!(x.meta["name"], json!("T"));
        let d = x.data.unwrap();
        assert_eq!(d.verses[0].text, "하나님이 세상을");
        assert_eq!(d.headings[0].text, "제목");
        assert_eq!(d.notes[0].text, "각주");
        let k = analyze_file(&tmp("d.txt", &encoding_rs::EUC_KR.encode("요,3,16,하나님이\n").0));
        assert_eq!(k.report["encoding"], json!("EUC-KR (CP949)"));
        assert_eq!(k.data.unwrap().verses[0].text, "하나님이");
    }
    #[test]
    fn bethlehem_split() {
        let (t, h, n) = split_bethlehem_text("<세계의 시작> 태초에 하나님이*[a] 천지를 / *각주 하나");
        assert_eq!(t.trim(), "태초에 하나님이 천지를");
        assert_eq!(h, vec!["세계의 시작"]);
        assert_eq!(n, vec!["각주 하나"]);
        assert!(!validate_meta(&json!({ "id": "x" })).is_empty());
    }
}

#[cfg(test)]
mod real {
    /// 실제 베들레헴 파일 (WP_BDB 경로가 있을 때만): cargo test -p wp-core real_bdb -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_bdb() {
        let Ok(p) = std::env::var("WP_BDB") else { return };
        let a = super::analyze_file(std::path::Path::new(&p));
        println!("{}", serde_json::to_string(&a.report).unwrap());
        assert!(a.data.unwrap().verses.len() > 30000);
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    #[test]
    fn replace_and_search() {
        let f = std::env::temp_dir().join(format!("wp-imp-db-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&f);
        let mut st = crate::db::BibleStore::open(&f).unwrap();
        let d = Data { verses: vec![Row { book: 43, chapter: 3, verse: 16, text: "하나님이 세상을 이처럼 사랑하사".into() }], ..Default::default() };
        replace_version(&st.db, &json!({ "id": "tst", "name": "시험", "source": "s", "license": "l" }), &d).unwrap();
        st.reload_structure().unwrap();
        assert_eq!(st.resolve_reference("tst", "요3:16")["ok"], json!(true));
        assert_eq!(st.search_text("tst", "사랑하사", 10)["results"].as_array().unwrap().len(), 1);
        replace_version(&st.db, &json!({ "id": "tst", "name": "시험2", "source": "s", "license": "l" }), &d).unwrap();
        assert_eq!(st.list_versions().len(), 1);
    }
}
