//! [트랙 C] 사용 기록 (Electron usagelog.js): 송출할 때마다 한 줄, 같은 것을 5분 안에 다시 띄우면 적지 않는다.
//! 저작권 보고 CSV (엑셀용 BOM). 날짜 거르기는 UTC 기준 (TODO: 이 PC 시간대)

use crate::util::{iso_now, now_ms};
use rusqlite::{params_from_iter, Connection};
use serde::Serialize;
use serde_json::Value;

const DEDUPE_MS: i64 = 5 * 60 * 1000;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Row {
    pub id: i64,
    pub at: String,
    pub kind: String,
    pub title: String,
    pub credit: String,
    pub cue: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Summary {
    pub kind: String,
    pub title: String,
    pub credit: String,
    pub count: usize,
    pub first: String,
    pub last: String,
}

#[derive(Default, Clone)]
pub struct Filter {
    pub from: String,
    pub to: String,
    pub kinds: Vec<String>,
    pub q: String,
}

pub fn ensure(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS UsageLog (id INTEGER PRIMARY KEY AUTOINCREMENT, at TEXT NOT NULL, kind TEXT NOT NULL DEFAULT '', title TEXT NOT NULL DEFAULT '', credit TEXT NOT NULL DEFAULT '', cue TEXT NOT NULL DEFAULT '');
         CREATE INDEX IF NOT EXISTS UsageLog_at ON UsageLog (at);",
    )
}

/// 'YYYY-MM-DDTHH:MM:SS.mmmZ' → ms (UTC)
pub fn iso_ms(s: &str) -> Option<i64> {
    let n = |a: usize, l: usize| s.get(a..a + l)?.parse::<i64>().ok();
    let (y, mo, d, h, mi, se) = (n(0, 4)?, n(5, 2)?, n(8, 2)?, n(11, 2)?, n(14, 2)?, n(17, 2)?);
    let ms = n(20, 3).unwrap_or(0);
    let y2 = if mo <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Some(((days * 86400 + h * 3600 + mi * 60 + se) * 1000) + ms)
}

/// 송출한 본문 → 기록 (기록했으면 Some)
pub fn record(db: &Connection, passage: &Value, cue_name: &str) -> Option<Row> {
    let kind = passage["itemType"]
        .as_str()
        .filter(|x| !x.is_empty())
        .map(String::from)
        .unwrap_or_else(|| if passage["book"].is_object() { "bible".into() } else { "other".into() });
    let title = passage["itemTitle"].as_str().filter(|x| !x.trim().is_empty()).or(passage["title"].as_str()).unwrap_or("").trim().to_string();
    if title.is_empty() {
        return None;
    }
    let last: Option<String> = db.query_row("SELECT at FROM UsageLog WHERE kind = ? AND title = ? ORDER BY id DESC LIMIT 1", [&kind, &title], |r| r.get(0)).ok();
    if let Some(ms) = last.as_deref().and_then(iso_ms) {
        if now_ms() - ms < DEDUPE_MS {
            return None;
        }
    }
    let cut = |s: &str, n: usize| s.chars().take(n).collect::<String>();
    let row = Row { id: 0, at: iso_now(), kind, title: cut(&title, 300), credit: cut(passage["credit"].as_str().unwrap_or(""), 300), cue: cut(cue_name, 200) };
    db.execute("INSERT INTO UsageLog (at, kind, title, credit, cue) VALUES (?, ?, ?, ?, ?)", [&row.at, &row.kind, &row.title, &row.credit, &row.cue]).ok()?;
    Some(Row { id: db.last_insert_rowid(), ..row })
}

pub fn list(db: &Connection, f: &Filter) -> Vec<Row> {
    let mut wh: Vec<String> = vec![];
    let mut vals: Vec<String> = vec![];
    if !f.from.is_empty() {
        wh.push("at >= ?".into());
        vals.push(format!("{}T00:00:00.000Z", f.from));
    }
    if !f.to.is_empty() {
        wh.push("at <= ?".into());
        vals.push(format!("{}T23:59:59.999Z", f.to));
    }
    if !f.kinds.is_empty() {
        wh.push(format!("kind IN ({})", vec!["?"; f.kinds.len()].join(",")));
        vals.extend(f.kinds.iter().cloned());
    }
    if !f.q.trim().is_empty() {
        wh.push("title LIKE ?".into());
        vals.push(format!("%{}%", f.q.split_whitespace().collect::<Vec<_>>().join("%")));
    }
    let w = if wh.is_empty() { String::new() } else { format!("WHERE {}", wh.join(" AND ")) };
    let sql = format!("SELECT id, at, kind, title, credit, cue FROM UsageLog {w} ORDER BY at DESC LIMIT 5000");
    let Ok(mut st) = db.prepare(&sql) else { return vec![] };
    st.query_map(params_from_iter(vals.iter()), |r| Ok(Row { id: r.get(0)?, at: r.get(1)?, kind: r.get(2)?, title: r.get(3)?, credit: r.get(4)?, cue: r.get(5)? }))
        .map(|it| it.filter_map(|x| x.ok()).collect())
        .unwrap_or_default()
}

pub fn summary(db: &Connection, f: &Filter) -> Vec<Summary> {
    let mut out: Vec<Summary> = vec![];
    for r in list(db, f) {
        match out.iter_mut().find(|s| s.kind == r.kind && s.title == r.title) {
            Some(s) => {
                s.count += 1;
                if r.at < s.first {
                    s.first = r.at.clone();
                }
                if r.at > s.last {
                    s.last = r.at.clone();
                }
                if s.credit.is_empty() && !r.credit.is_empty() {
                    s.credit = r.credit.clone();
                }
            }
            None => out.push(Summary { kind: r.kind.clone(), title: r.title.clone(), credit: r.credit.clone(), count: 1, first: r.at.clone(), last: r.at.clone() }),
        }
    }
    out.sort_by(|a, b| b.count.cmp(&a.count).then(a.title.cmp(&b.title)));
    out
}

pub fn kind_name(k: &str) -> &str {
    match k {
        "song" => "찬양",
        "hymn" => "찬송가",
        "bible" => "성경",
        "text" => "공지",
        "title" => "제목",
        "image" => "이미지",
        "video" => "영상",
        "slideshow" => "슬라이드쇼",
        "camera" => "카메라",
        "blank" => "빈 화면",
        "other" => "기타",
        o => o,
    }
}

fn esc(s: &str) -> String {
    if s.contains([',', '"', '\n']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() }
}

/// CSV (BOM 포함). summary_mode=true 면 제목별 횟수
pub fn csv(db: &Connection, f: &Filter, summary_mode: bool) -> String {
    let mut lines: Vec<String> = vec![];
    if summary_mode {
        lines.push("종류,제목,저작권·출처,횟수,처음,마지막".into());
        for s in summary(db, f) {
            let row = [kind_name(&s.kind).to_string(), s.title.clone(), s.credit.clone(), s.count.to_string(), s.first.chars().take(10).collect(), s.last.chars().take(10).collect()];
            lines.push(row.iter().map(|x| esc(x)).collect::<Vec<_>>().join(","));
        }
    } else {
        lines.push("날짜,시각,종류,제목,저작권·출처,큐시트".into());
        for r in list(db, f) {
            let row = [r.at.get(..10).unwrap_or("").to_string(), r.at.get(11..16).unwrap_or("").to_string(), kind_name(&r.kind).to_string(), r.title.clone(), r.credit.clone(), r.cue.clone()];
            lines.push(row.iter().map(|x| esc(x)).collect::<Vec<_>>().join(","));
        }
    }
    format!("\u{feff}{}\r\n", lines.join("\r\n"))
}

/// before('YYYY-MM-DD') 이전만, 비우면 전부
pub fn clear(db: &Connection, before: &str) -> usize {
    if before.is_empty() {
        db.execute("DELETE FROM UsageLog", []).unwrap_or(0)
    } else {
        db.execute("DELETE FROM UsageLog WHERE at < ?", [format!("{before}T00:00:00.000Z")]).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn usage() {
        let db = Connection::open_in_memory().unwrap();
        ensure(&db).unwrap();
        let p = json!({ "itemType": "song", "itemTitle": "주 하나님 지으신, 모든 세계", "credit": "CCLI 1" });
        assert!(record(&db, &p, "주일 예배").is_some());
        assert!(record(&db, &p, "주일 예배").is_none()); // 5분 안 같은 곡
        record(&db, &json!({ "title": "요한복음 3:16", "book": { "id": 43 } }), "").unwrap();
        assert!(record(&db, &json!({ "title": "  " }), "").is_none());
        assert_eq!(list(&db, &Filter::default()).len(), 2);
        assert_eq!(list(&db, &Filter { kinds: vec!["bible".into()], ..Default::default() }).len(), 1);
        let c = csv(&db, &Filter::default(), true);
        assert!(c.starts_with('\u{feff}'));
        assert!(c.contains("\"주 하나님 지으신, 모든 세계\""));
        assert_eq!(clear(&db, ""), 2);
        assert_eq!(iso_ms("1970-01-01T00:00:01.000Z"), Some(1000));
        assert_eq!(iso_ms(&crate::util::iso_now()).map(|m| (now_ms() - m).abs() < 5000), Some(true));
    }
}
