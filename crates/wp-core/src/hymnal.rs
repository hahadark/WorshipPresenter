//! [트랙 A] 베들레헴 성경의 찬송가 (Electron 판 hymnal.js · hymnlyrics.js · bethlehem.js 찾기)
//! - 가사 DB(.hdb, SQLite hymnal(chapter, title, htext)) + 악보 그림 묶음(.cmp, zip: p{장}.png)
//! - 새찬송가 1~645장 · 교독문 701~836 (701 = 교독문 1) · 찬미가 1~899장
//! - 사용자가 가진 베들레헴 데이터를 읽기만 한다 (공개판에는 넣지 않음)

use crate::core::Core;
use once_cell::sync::Lazy;
use regex::Regex;
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde_json::{json, Value};
use std::io::Read;
use std::path::{Path, PathBuf};

pub struct BookDef {
    pub id: &'static str,
    pub hymns: i64,
    pub reading: Option<(i64, i64)>,
}

pub const BOOKS: [BookDef; 2] = [BookDef { id: "새찬송가", hymns: 645, reading: Some((701, 836)) }, BookDef { id: "찬미가", hymns: 899, reading: None }];

static TAG: Lazy<Regex> = Lazy::new(|| Regex::new(r"<[^>]+>").unwrap());
static WS: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s+").unwrap());
static BRBR: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)<br\s*/?>\s*<br\s*/?>").unwrap());
static BR: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)<br\s*/?>").unwrap());
static VNUM: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\d+\s*\.\s*").unwrap());

pub fn clean(s: &str) -> String {
    let s = TAG.replace_all(s, " ").replace("&nbsp;", " ").replace("&amp;", "&");
    WS.replace_all(&s, " ").trim().to_string()
}

/// "<b>1</b>. 가사 <br><br><small>[후렴]</small> 가사<br><br><b>2</b>. …" → (절들, 후렴)
pub fn parse_htext(html: &str) -> (Vec<String>, String) {
    let mut verses = vec![];
    let mut chorus = String::new();
    for block in BRBR.split(html) {
        if clean(block).is_empty() {
            continue;
        }
        if block.contains("[후렴]") {
            if chorus.is_empty() {
                chorus = clean(&block.replacen("[후렴]", "", 1));
            }
            continue;
        }
        verses.push(VNUM.replace(&clean(block), "").to_string());
    }
    (verses, chorus)
}

fn syl(s: &str) -> usize {
    s.chars().filter(|c| !c.is_whitespace()).count()
}

/// 글을 낱말 경계에서 parts 조각으로 고르게 (글자 수 기준)
pub fn split_even(text: &str, parts: usize) -> Vec<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if parts <= 1 || words.len() <= 1 {
        return vec![words.join(" ")];
    }
    let total = syl(text) as f64;
    let mut out: Vec<String> = vec![];
    let mut cur: Vec<&str> = vec![];
    let mut acc = 0usize;
    for i in 0..words.len() {
        let w = words[i];
        let left = words.len() - i;
        let need = parts as i64 - out.len() as i64 - 1;
        cur.push(w);
        acc += syl(w);
        let goal = total * (out.len() + 1) as f64 / parts as f64;
        let next = if i + 1 < words.len() { (acc + syl(words[i + 1])) as f64 } else { f64::INFINITY };
        if need > 0 && (left as i64 - 1) >= need && (acc as f64 - goal).abs() <= (next - goal).abs() {
            out.push(cur.join(" "));
            cur.clear();
        }
    }
    if !cur.is_empty() {
        out.push(cur.join(" "));
    }
    while out.len() < parts {
        out.push(String::new());
    }
    out
}

fn two_lines(s: &str) -> String {
    split_even(s, 2).into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join("\n")
}

/// 찬송가 PPT 악보 슬라이드 n 장에 가사를 맞춘다 (1절 → 후렴 → 2절 … 여러 배치 중 글자 비율이 가장 맞는 것)
pub fn align_lyrics(verses: &[String], chorus: &str, n: usize) -> Vec<String> {
    if verses.is_empty() || n < 1 {
        return vec![];
    }
    let k = verses.len();
    let avg_v = verses.iter().map(|v| syl(v)).sum::<usize>() as f64 / k as f64;
    type F = fn(usize, usize) -> bool;
    let plans_def: [(&str, F); 4] = [("first-last", |i, k| i == 0 || i == k - 1), ("each", |_, _| true), ("last", |i, k| i == k - 1), ("none", |_, _| false)];
    let mut plans: Vec<(F, usize, usize, f64)> = vec![];
    for (name, f) in plans_def {
        if chorus.is_empty() && name != "none" {
            continue;
        }
        let m = (0..k).filter(|&i| f(i, k)).count();
        let cmax = if m > 0 { 4 } else { 0 };
        for c in (if m > 0 { 1 } else { 0 })..=cmax {
            let used = m * c;
            if used > n { continue; }
            let rem = n - used;
            if rem < k || rem % k != 0 { continue; }
            let s = rem / k;
            let per_v = avg_v / s as f64;
            let per_c = if c > 0 { syl(chorus) as f64 / c as f64 } else { per_v };
            let score = (per_v / per_c).ln().abs() + match name { "first-last" => 0.0, "each" => 0.05, _ => 0.1 };
            plans.push((f, s, c, score));
        }
    }
    let mut out = vec![];
    if !plans.is_empty() {
        plans.sort_by(|a, b| a.3.partial_cmp(&b.3).unwrap());
        let (f, s, c, _) = plans[0];
        for (i, v) in verses.iter().enumerate() {
            out.extend(split_even(v, s).iter().map(|x| two_lines(x)));
            if c > 0 && f(i, k) {
                out.extend(split_even(chorus, c).iter().map(|x| two_lines(x)));
            }
        }
    } else {
        let all = verses.iter().flat_map(|v| if chorus.is_empty() { vec![v.as_str()] } else { vec![v.as_str(), chorus] }).collect::<Vec<_>>().join(" ");
        out.extend(split_even(&all, n).iter().map(|x| two_lines(x)));
    }
    out.truncate(n);
    out
}

/// 베들레헴 설치 폴더 후보 (Electron candidates + findDirs)
pub fn candidates() -> Vec<PathBuf> {
    let mut out = vec![];
    let home = dirs::home_dir().unwrap_or_default();
    let mut docs = vec![home.join("Documents"), home.join("문서")];
    for base in [home.clone(), PathBuf::from("D:\\"), PathBuf::from("E:\\")] {
        if let Ok(rd) = std::fs::read_dir(&base) {
            for e in rd.flatten() {
                if e.file_name().to_string_lossy().to_lowercase().starts_with("onedrive") {
                    docs.push(e.path().join("문서"));
                    docs.push(e.path().join("Documents"));
                }
            }
        }
    }
    for d in &docs {
        out.push(d.join("베들레헴 성경").join("BethlehemWin"));
        out.push(d.join("BethlehemWin"));
    }
    for pf in [std::env::var("ProgramFiles(x86)").ok(), std::env::var("ProgramFiles").ok()].into_iter().flatten() {
        out.push(PathBuf::from(pf).join("BethlehemWin"));
    }
    for d in "CDEFGHIJ".chars() {
        out.push(PathBuf::from(format!("{d}:\\BethlehemWin")));
    }
    for sub in ["Desktop", "Downloads", "바탕 화면"] {
        out.push(home.join(sub).join("BethlehemWin"));
    }
    out
}

/// 찾는 폴더들 (앞이 우선): 데이터 폴더 hymnal → 설정 → Electron 판 데이터 → 설치 위치
pub fn dirs_for(core: &Core) -> Vec<PathBuf> {
    let mut v = vec![core.data_dir.join("hymnal"), core.data_dir.clone()];
    let s = core.settings.str(&["bethlehemDir"]);
    if !s.is_empty() {
        v.push(PathBuf::from(s));
    }
    if let Some(e) = crate::hymns::electron_data() {
        v.push(e.join("hymnal"));
    }
    v.extend(candidates());
    v
}

pub fn find(core: &Core, book: &str, ext: &str) -> Option<PathBuf> {
    dirs_for(core).into_iter().map(|d| d.join(format!("{book}{ext}"))).find(|f| f.is_file())
}

fn open(f: &Path) -> Option<Connection> {
    Connection::open_with_flags(f, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX).ok()
}

fn kind_of(book: &str, n: i64) -> &'static str {
    let Some(b) = BOOKS.iter().find(|x| x.id == book) else { return "" };
    if n >= 1 && n <= b.hymns {
        return "hymn";
    }
    if let Some((a, z)) = b.reading {
        if n >= a && n <= z {
            return "reading";
        }
    }
    ""
}

/// 쓸 수 있는 찬송가책
pub fn books(core: &Core) -> Vec<Value> {
    BOOKS
        .iter()
        .filter_map(|b| {
            let count: i64 = find(core, b.id, ".hdb").and_then(|f| open(&f)).and_then(|db| db.query_row("SELECT COUNT(*) FROM hymnal WHERE chapter <= ?", [b.hymns], |r| r.get(0)).ok()).unwrap_or(0);
            let scores = find(core, b.id, ".cmp").is_some();
            (count > 0 || scores).then(|| json!({ "id": b.id, "name": b.id, "lyrics": count > 0, "count": count, "scores": scores, "reading": b.reading.is_some() && count > 0 }))
        })
        .collect()
}

/// 찾기: 번호(10 · 10장) · 교독문 번호(교독문 1) · 제목 · 가사
pub fn search(core: &Core, query: &str, limit: usize) -> Vec<Value> {
    let q = query.trim();
    if q.is_empty() {
        return vec![];
    }
    let m_read = Regex::new(r"^교독문\s*(\d{1,3})$").unwrap().captures(q).and_then(|m| m[1].parse::<i64>().ok());
    let m_num = Regex::new(r"^(\d{1,3})\s*장?$").unwrap().captures(q).and_then(|m| m[1].parse::<i64>().ok());
    let key: String = q.chars().filter(|c| !c.is_whitespace()).collect();
    let mut out = vec![];
    for b in BOOKS.iter() {
        let Some(db) = find(core, b.id, ".hdb").and_then(|f| open(&f)) else { continue };
        let rows: Vec<(i64, String)> = if let (Some(n), Some((a, _))) = (m_read, b.reading) {
            db.prepare("SELECT chapter, title FROM hymnal WHERE chapter = ?").and_then(|mut s| s.query_map([a - 1 + n], |r| Ok((r.get(0)?, r.get(1)?))).map(|it| it.flatten().collect())).unwrap_or_default()
        } else if let Some(n) = m_num {
            db.prepare("SELECT chapter, title FROM hymnal WHERE chapter = ?").and_then(|mut s| s.query_map([n], |r| Ok((r.get(0)?, r.get(1)?))).map(|it| it.flatten().collect())).unwrap_or_default()
        } else if key.chars().count() >= 2 {
            let like = format!("%{}%", q.replace(['%', '_'], ""));
            let likek = format!("%{}%", key.replace(['%', '_'], ""));
            db.prepare("SELECT chapter, title FROM hymnal WHERE (title LIKE ?1 OR replace(title, ' ', '') LIKE ?2 OR htext LIKE ?1) ORDER BY (replace(title, ' ', '') LIKE ?2) DESC, chapter LIMIT ?3")
                .and_then(|mut s| s.query_map(rusqlite::params![like, likek, limit as i64], |r| Ok((r.get(0)?, r.get(1)?))).map(|it| it.flatten().collect()))
                .unwrap_or_default()
        } else {
            vec![]
        };
        let has_cmp = find(core, b.id, ".cmp").is_some();
        for (ch, title) in rows {
            let kind = kind_of(b.id, ch);
            if kind.is_empty() {
                continue;
            }
            let label = if kind == "reading" { format!("교독문 {} {}", ch - b.reading.unwrap().0 + 1, title) } else { format!("{ch}장 {title}") };
            out.push(json!({ "book": b.id, "number": ch, "title": title, "kind": kind, "label": label, "scores": kind == "hymn" && has_cmp }));
        }
    }
    out.truncate(limit);
    out
}

/// 한 장 → 큐시트 순서로 쓸 모양 (찬송: 가사·부를 순서·악보 / 교독문: 공지 글)
pub fn get(core: &Core, book: &str, number: i64) -> Result<Value, String> {
    let db = find(core, book, ".hdb").and_then(|f| open(&f)).ok_or(crate::i18n::tr_args("{1} 가사 파일을 찾지 못했습니다. 베들레헴 폴더를 지정하거나 가져오기를 하세요.", &[&book]))?;
    let r: Option<(i64, String, String)> = db.query_row("SELECT chapter, title, htext FROM hymnal WHERE chapter = ? ORDER BY id LIMIT 1", [number], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).optional().map_err(|e| e.to_string())?;
    let (ch, title, htext) = r.ok_or(crate::i18n::tr_args("{1} {2}장이 없습니다.", &[&book, &number]))?;
    let kind = kind_of(book, ch);
    let b = BOOKS.iter().find(|x| x.id == book).unwrap();
    if kind == "reading" {
        let lines: Vec<(String, bool)> = BR.split(&htext).map(|l| (clean(l), l.to_lowercase().contains("<b>"))).filter(|l| !l.0.is_empty()).collect();
        let pages: Vec<String> = lines.chunks(2).map(|c| c.iter().map(|(t, p)| if *p { format!("**{t}**") } else { t.clone() }).collect::<Vec<_>>().join("\n")).collect();
        let n = ch - b.reading.unwrap().0 + 1;
        return Ok(json!({ "kind": kind, "book": book, "number": ch, "title": format!("교독문 {n} {title}"), "body": pages.join("\n---\n") }));
    }
    let (verses, chorus) = parse_htext(&htext);
    let slides = |text: &str| {
        let t = clean(text);
        let n = syl(&t);
        let mut lines = ((n as f64 / 14.0).round() as usize).max(2);
        if lines % 2 == 1 {
            lines += 1;
        }
        let ls = split_even(&t, lines);
        ls.chunks(2).map(|c| c.join("\n")).collect::<Vec<_>>().join("\n\n")
    };
    let mut parts = vec![];
    let mut order = vec![];
    for (k, v) in verses.iter().enumerate() {
        parts.push(format!("[{}절]\n{}", k + 1, slides(v)));
        order.push(format!("{}절", k + 1));
        if !chorus.is_empty() {
            order.push("후렴".into());
        }
    }
    if !chorus.is_empty() {
        parts.push(format!("[후렴]\n{}", slides(&chorus)));
    }
    let score = if find(core, book, ".cmp").is_some() { format!("/hymnal/score/{}/{}.png", pct(book), ch) } else { String::new() };
    Ok(json!({ "kind": "hymn", "book": book, "number": ch, "title": format!("{ch}장 {title}"), "lyrics": parts.join("\n\n"), "order": if chorus.is_empty() { String::new() } else { order.join(" ") }, "credit": format!("{book} {ch}장"), "score": score }))
}

fn pct(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() { (b as char).to_string() } else { format!("%{:02X}", b) }).collect()
}

/// 새찬송가 가사 (PPT 악보 슬라이드의 OBS 자막용) → (절들, 후렴)
pub fn lyrics(core: &Core, number: i64) -> Option<(Vec<String>, String)> {
    if number < 1 {
        return None;
    }
    let db = find(core, "새찬송가", ".hdb").and_then(|f| open(&f))?;
    let h: String = db.query_row("SELECT htext FROM hymnal WHERE chapter = ? ORDER BY id LIMIT 1", [number], |r| r.get(0)).ok()?;
    let (v, c) = parse_htext(&h);
    (!v.is_empty()).then_some((v, c))
}

/// 악보 그림 (png). .cmp 는 압축 안 한(또는 deflate) zip
pub fn score(core: &Core, book: &str, number: i64) -> Option<Vec<u8>> {
    let f = find(core, book, ".cmp")?;
    let mut z = zip::ZipArchive::new(std::fs::File::open(f).ok()?).ok()?;
    let want = format!("p{number}.png");
    let name = z.file_names().filter_map(|n| n.ok().map(|n| n.to_string())).find(|n| n.eq_ignore_ascii_case(&want))?;
    let mut e = z.by_name(&name).ok()?;
    let mut buf = vec![];
    e.read_to_end(&mut buf).ok()?;
    Some(buf)
}

/// 베들레헴 폴더 → 데이터 폴더 hymnal 로 복사
pub fn import_from(core: &Core, dir: &Path) -> Vec<Value> {
    let dest = core.data_dir.join("hymnal");
    let _ = std::fs::create_dir_all(&dest);
    let mut done = vec![];
    for b in BOOKS.iter() {
        let (mut l, mut s) = (false, false);
        for ext in [".hdb", ".cmp"] {
            let src = dir.join(format!("{}{}", b.id, ext));
            if !src.is_file() {
                continue;
            }
            let d = dest.join(format!("{}{}", b.id, ext));
            if src != d && std::fs::copy(&src, &d).is_ok() {
                if ext == ".hdb" { l = true } else { s = true }
            }
        }
        if l || s {
            done.push(json!({ "book": b.id, "lyrics": l, "scores": s }));
        }
    }
    done
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn htext() {
        let (v, c) = parse_htext("<b>1</b>. 만복의 근원 하나님 <br><br><small>[후렴]</small> 아멘 아멘 <br><br><b>2</b>. 둘째 절 가사");
        assert_eq!(v, vec!["만복의 근원 하나님".to_string(), "둘째 절 가사".to_string()]);
        assert_eq!(c, "아멘 아멘");
        assert_eq!(split_even("가 나 다 라", 2), vec!["가 나".to_string(), "다 라".to_string()]);
        let a = align_lyrics(&v, &c, 4);
        assert_eq!(a.len(), 4);
    }
}
