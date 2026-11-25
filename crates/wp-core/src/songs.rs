//! [트랙 B] 찬양 라이브러리 (Electron 판 library.js Songs · songimport.js parseSongText)
//! 같은 bible.db 의 Songs 테이블을 그대로 쓴다.

use crate::util::{iso_now, short_id};
use anyhow::{bail, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

pub fn ensure_schema(db: &Connection) -> Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS Songs (id TEXT PRIMARY KEY, title TEXT NOT NULL, lyrics TEXT NOT NULL DEFAULT '', credit TEXT NOT NULL DEFAULT '', tags TEXT NOT NULL DEFAULT '', updated_at TEXT);")?;
    let cols: Vec<String> = {
        let mut st = db.prepare("PRAGMA table_info(Songs)")?;
        st.query_map([], |r| r.get::<_, String>(1))?.filter_map(|x| x.ok()).collect()
    };
    if !cols.iter().any(|c| c == "arrangement") {
        db.execute_batch("ALTER TABLE Songs ADD COLUMN arrangement TEXT NOT NULL DEFAULT ''")?;
    }
    if !cols.iter().any(|c| c == "arrangements") {
        db.execute_batch("ALTER TABLE Songs ADD COLUMN arrangements TEXT NOT NULL DEFAULT '[]'")?;
    }
    Ok(())
}

fn cut(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

fn valid_id(s: &str) -> bool {
    (4..=40).contains(&s.len()) && s.chars().all(|c| c.is_ascii_alphanumeric())
}

/// 저장한 부를 순서 정리: [{name, order}] 최대 12개, 이름이 겹치면 뒤의 것
pub fn norm_arr(a: &Value) -> Vec<Value> {
    let mut out: Vec<(String, String)> = vec![];
    for x in a.as_array().into_iter().flatten() {
        let name = cut(x["name"].as_str().unwrap_or(""), 30).trim().to_string();
        let order = cut(x["order"].as_str().unwrap_or(""), 300).trim().to_string();
        if name.is_empty() || order.is_empty() {
            continue;
        }
        out.retain(|(n, _)| n != &name);
        out.push((name, order));
    }
    let skip = out.len().saturating_sub(12);
    out.into_iter().skip(skip).map(|(n, o)| json!({ "name": n, "order": o })).collect()
}

/// 코드 [G] [Am7] 빼기
pub fn strip_chords(l: &str) -> String {
    let re = regex::Regex::new(r"\[[A-G](?:#|b|♯|♭)?(?:maj|min|m|M|sus|dim|aug|add|°|ø)?\d{0,2}(?:(?:sus|add|maj|b|#|\+|-)\d{1,2})*(?:/[A-G](?:#|b|♯|♭)?)?\]").unwrap();
    let s = re.replace_all(l, "");
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn squash(s: &str) -> String {
    crate::textnorm::squash(s)
}

/// 찬양 찾기: 띄어쓰기 무시 · 제목·가사·태그. 검색어에 초성(ㅈㅁㅂㄹ · 주ㅁㅂㄹ)이 있으면 제목만 초성으로.
/// 제목이 맞는 곡 → 그 가운데 제목이 검색어로 시작하는 곡이 앞
pub fn list_songs(db: &Connection, query: &str) -> Vec<Value> {
    let q = squash(query);
    let cho = crate::textnorm::has_choseong(query);
    let Ok(mut st) = db.prepare("SELECT id, title, lyrics, credit, tags, COALESCE(updated_at,'') FROM Songs ORDER BY title COLLATE NOCASE") else { return vec![] };
    let rows: Vec<(String, String, String, String, String, String)> = st
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))
        .map(|it| it.filter_map(|x| x.ok()).collect())
        .unwrap_or_default();
    let mut out: Vec<(bool, bool, Value)> = vec![];
    for (id, title, lyrics, credit, tags, upd) in rows {
        let hit = q.is_empty() || crate::textnorm::fuzzy_contains(&title, query);
        if !hit && (cho || !squash(&format!("{title}{lyrics}{tags}")).contains(&q)) {
            continue;
        }
        let first = lyrics.lines().map(strip_chords).find(|l| !l.is_empty() && !(l.starts_with('[') && l.ends_with(']'))).unwrap_or_default();
        let start = !q.is_empty() && crate::textnorm::fuzzy_starts_with(&title, query);
        out.push((hit, start, json!({ "id": id, "title": title, "first": first, "credit": credit, "tags": tags, "updatedAt": upd, "titleHit": hit })));
    }
    out.sort_by_key(|x| (!x.0, !x.1));
    out.into_iter().take(500).map(|x| x.2).collect()
}

/// 제목으로 한 곡 찾기 (띄어쓰기·대소문자 무시): 같은 제목 → 그 제목으로 시작 → 찾은 것이 하나뿐이면 그것
/// (주보 → 큐시트의 찬양 찾기와 같은 규칙)
pub fn find_by_title(db: &Connection, name: &str) -> Option<Value> {
    let q = squash(name);
    if q.is_empty() {
        return None;
    }
    let hits = list_songs(db, name);
    let t = |x: &Value| squash(x["title"].as_str().unwrap_or(""));
    let s = hits.iter().find(|x| t(x) == q).or_else(|| hits.iter().find(|x| t(x).starts_with(&q))).or(if hits.len() == 1 { hits.first() } else { None });
    s.and_then(|x| get_song(db, x["id"].as_str().unwrap_or("")))
}

pub fn get_song(db: &Connection, id: &str) -> Option<Value> {
    db.query_row("SELECT id, title, lyrics, credit, tags, arrangement, arrangements, COALESCE(updated_at,'') FROM Songs WHERE id = ?", [id], |r| {
        let arrs: String = r.get(6)?;
        Ok(json!({ "id": r.get::<_, String>(0)?, "title": r.get::<_, String>(1)?, "lyrics": r.get::<_, String>(2)?, "credit": r.get::<_, String>(3)?, "tags": r.get::<_, String>(4)?,
            "arrangement": r.get::<_, String>(5)?, "arrangements": norm_arr(&serde_json::from_str(&arrs).unwrap_or(json!([]))), "updatedAt": r.get::<_, String>(7)? }))
    })
    .optional()
    .ok()
    .flatten()
}

pub fn save_song(db: &Connection, s: &Value) -> Result<Value> {
    let title = cut(s["title"].as_str().unwrap_or(""), 200).trim().to_string();
    if title.is_empty() {
        bail!("찬양 제목을 입력하세요.");
    }
    let id = s["id"].as_str().filter(|x| valid_id(x)).map(String::from).unwrap_or_else(|| short_id("s"));
    // arrangements 를 안 보내면 저장해 둔 것을 그대로
    let keep = if s.get("arrangements").is_none() { get_song(db, &id).map(|x| x["arrangements"].clone()).unwrap_or(json!([])) } else { s["arrangements"].clone() };
    let g = |k: &str, n: usize| cut(s[k].as_str().unwrap_or(""), n);
    db.execute(
        "INSERT OR REPLACE INTO Songs (id, title, lyrics, credit, tags, arrangement, arrangements, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        params![id, title, g("lyrics", 50000), g("credit", 300), g("tags", 300), g("arrangement", 300), serde_json::to_string(&norm_arr(&keep))?, iso_now()],
    )?;
    Ok(get_song(db, &id).unwrap_or(Value::Null))
}

/// 부를 순서 하나 저장 (이름이 같으면 고침, order 가 비면 지움)
pub fn save_arrangement(db: &Connection, id: &str, name: &str, order: &str) -> Result<Value> {
    let Some(mut s) = get_song(db, id) else { bail!("찬양을 찾을 수 없습니다.") };
    let mut list: Vec<Value> = s["arrangements"].as_array().cloned().unwrap_or_default();
    list.retain(|x| x["name"] != name);
    if !order.trim().is_empty() {
        list.push(json!({ "name": name, "order": order }));
    }
    s["arrangements"] = json!(list);
    save_song(db, &s)
}

pub fn delete_song(db: &Connection, id: &str) -> Result<()> {
    db.execute("DELETE FROM Songs WHERE id = ?", [id])?;
    Ok(())
}

/// 가사 정리: 빈 줄이 없으면 lines 줄씩 슬라이드로 묶는다
pub fn arrange_lyrics(body: &str, lines: usize) -> String {
    let src = body.replace('\r', "");
    let src = src.trim();
    if src.lines().any(|l| l.trim().is_empty()) {
        return src.to_string();
    }
    let ls: Vec<&str> = src.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
    ls.chunks(lines.max(1)).map(|c| c.join("\n")).collect::<Vec<_>>().join("\n\n")
}

/// 붙여 넣은 가사 → 곡 목록. '# 제목' 줄로 여러 곡, 없으면 첫 줄(빈 줄이 뒤따르면)을 제목으로
pub fn parse_song_text(text: &str, fallback: &str, lines: usize) -> Vec<Value> {
    let src = text.replace('\r', "");
    let mut parts: Vec<(String, String)> = vec![];
    let head = regex::Regex::new(r"^#\s*\S").unwrap();
    if src.lines().any(|l| head.is_match(l)) {
        let mut cur: Option<(String, String)> = None;
        for l in src.lines() {
            if head.is_match(l) {
                if let Some(c) = cur.take() {
                    parts.push(c);
                }
                cur = Some((l.trim_start_matches('#').trim().to_string(), String::new()));
            } else if let Some(c) = &mut cur {
                c.1.push_str(l);
                c.1.push('\n');
            }
        }
        if let Some(c) = cur {
            parts.push(c);
        }
    } else {
        let mut ls: Vec<&str> = src.trim().lines().collect();
        let mut title = fallback.to_string();
        if title.is_empty() && ls.len() > 2 && ls[0].trim().chars().count() <= 40 && ls[1].trim().is_empty() {
            title = ls.remove(0).trim().to_string();
        }
        if title.is_empty() {
            title = cut(ls.iter().find(|l| !l.trim().is_empty()).map(|l| l.trim()).unwrap_or(""), 40);
        }
        parts.push((title, ls.join("\n")));
    }
    parts
        .into_iter()
        .map(|(t, b)| json!({ "title": cut(&t, 200), "lyrics": arrange_lyrics(&b, lines), "credit": "", "arrangement": "" }))
        .filter(|x| !x["title"].as_str().unwrap_or("").is_empty() && !x["lyrics"].as_str().unwrap_or("").trim().is_empty())
        .collect()
}

/// 여러 곡 한꺼번에 저장 (같은 제목이 있으면 건너뜀)
pub fn save_imported(db: &Connection, songs: &[Value]) -> Result<Value> {
    let have: Vec<String> = list_songs(db, "").iter().map(|s| squash(s["title"].as_str().unwrap_or(""))).collect();
    let (mut saved, mut skipped) = (0, 0);
    for s in songs {
        if have.contains(&squash(s["title"].as_str().unwrap_or(""))) {
            skipped += 1;
            continue;
        }
        save_song(db, s)?;
        saved += 1;
    }
    Ok(json!({ "saved": saved, "skipped": skipped }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn choseong_and_title_lookup() {
        let db = Connection::open_in_memory().unwrap();
        ensure_schema(&db).unwrap();
        for (t, l) in [("주만 바라 볼지라", "하나님의 사랑을"), ("은혜", "내가 누려왔던 모든 것들이"), ("주님 마음 내게 주소서", "주만 바라")] {
            save_song(&db, &json!({ "title": t, "lyrics": l })).unwrap();
        }
        let titles = |q: &str| list_songs(&db, q).iter().map(|x| x["title"].as_str().unwrap().to_string()).collect::<Vec<_>>();
        assert_eq!(titles("ㅈㅁㅂㄹ"), ["주만 바라 볼지라"], "초성은 제목만");
        assert_eq!(titles("주ㅁㅂㄹ"), ["주만 바라 볼지라"]);
        assert_eq!(titles("ㅇㅎ"), ["은혜"]);
        assert_eq!(titles("주만바라"), ["주만 바라 볼지라", "주님 마음 내게 주소서"], "글자 검색은 가사까지, 제목이 맞는 곡이 앞");
        assert_eq!(titles("ㅈ").first().map(String::as_str), Some("주님 마음 내게 주소서"), "제목순 · 둘 다 ㅈ 으로 시작");
        assert_eq!(find_by_title(&db, "주만바라볼지라").unwrap()["title"], "주만 바라 볼지라");
        assert_eq!(find_by_title(&db, "은 혜").unwrap()["title"], "은혜");
        assert!(find_by_title(&db, "없는 곡").is_none());
        assert!(find_by_title(&db, "").is_none());
    }    #[test]
    fn cue_save_keeps_default_order_and_named_arrangements() {
        let db = Connection::open_in_memory().unwrap();
        ensure_schema(&db).unwrap();
        let s = save_song(&db, &json!({"title":"순서 회귀", "lyrics":"[1절]\n가사", "arrangement":"1 C 2 C"})).unwrap();
        let id = s["id"].as_str().unwrap();
        save_arrangement(&db, id, "짧게", "1 C").unwrap();
        // 큐의 order는 라이브러리 계약 arrangement로 바꿔 저장한다.
        let saved = save_song(&db, &json!({"id":id,"title":"순서 회귀","lyrics":"가사","arrangement":"2 C 1 C"})).unwrap();
        assert_eq!(saved["arrangement"], "2 C 1 C");
        assert_eq!(saved["arrangements"][0]["order"], "1 C");
        save_arrangement(&db, id, "짧게", "C 1 C").unwrap();
        assert_eq!(get_song(&db,id).unwrap()["arrangements"][0]["order"], "C 1 C");
        save_arrangement(&db, id, "짧게", "").unwrap();
        let saved = get_song(&db,id).unwrap();
        assert_eq!(saved["arrangement"], "2 C 1 C");
        assert!(saved["arrangements"].as_array().unwrap().is_empty());
    }
    #[test]
    fn songs_crud() {
        let db = Connection::open_in_memory().unwrap();
        ensure_schema(&db).unwrap();
        let s = save_song(&db, &json!({ "title": "주 하나님 지으신", "lyrics": "[1절]\n[G]주 하나님 지으신\n모든 세계", "credit": "작사 미상" })).unwrap();
        let id = s["id"].as_str().unwrap().to_string();
        assert_eq!(list_songs(&db, "하나님").len(), 1);
        assert_eq!(list_songs(&db, "하나님")[0]["first"], "주 하나님 지으신");
        save_arrangement(&db, &id, "짧게", "1 C").unwrap();
        save_arrangement(&db, &id, "짧게", "1 C C").unwrap();
        assert_eq!(get_song(&db, &id).unwrap()["arrangements"].as_array().unwrap().len(), 1);
        save_song(&db, &json!({ "id": id, "title": "주 하나님 지으신", "lyrics": "x" })).unwrap();
        assert_eq!(get_song(&db, &id).unwrap()["arrangements"][0]["order"], "1 C C");
        assert_eq!(save_imported(&db, &[json!({ "title": "주 하나님  지으신", "lyrics": "x" }), json!({ "title": "새 곡", "lyrics": "y" })]).unwrap()["saved"], 1);
        delete_song(&db, &id).unwrap();
        assert!(get_song(&db, &id).is_none());
    }
    #[test]
    fn parse_text() {
        let v = parse_song_text("# 첫 곡\n가\n나\n다\n라\n# 둘째 곡\n마\n\n바", "", 2);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0]["lyrics"], "가\n나\n\n다\n라");
        let w = parse_song_text("나의 기쁨 나의 소망\n\n첫 줄\n둘째 줄", "", 2);
        assert_eq!(w[0]["title"], "나의 기쁨 나의 소망");
    }
}
