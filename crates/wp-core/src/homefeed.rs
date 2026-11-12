//! [트랙 A] 홈페이지 주보 연동 (Electron 판 homefeed.js)
//! 교회 홈페이지가 내는 주보 JSON (예: https://교회주소/bulletin/feed.json) 을 읽어
//!  - 예배 순서 → 큐시트 (찬송가·장절 확인은 주보 AI 와 같은 plan_to_saved_cue)
//!  - 교회 소식(광고) · 일정 · 기도 제목 → 공지 슬라이드 ('---' 로 장 나눔)
//! JSON: { church, updated, bulletins: [ { date, issue, url, sermon:{title,passage,preacher}, verse:{text,ref},
//!   services:[{ name, time, host, items:[{ part, content, by, stand }] }], news:[{title,body,link}], prayers:[{title,body}], events:[{when,title,note}] } ] }

use crate::core::Core;
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::{json, Value};

fn s(v: &Value, n: usize) -> String {
    let t = match v {
        Value::String(x) => x.clone(),
        Value::Null => String::new(),
        x => x.to_string(),
    };
    t.replace('\r', "").trim().chars().take(n).collect()
}
fn arr(v: &Value) -> Vec<Value> {
    v.as_array().cloned().unwrap_or_default()
}

/// 받은 JSON → 정리된 주보 목록 (최신순)
pub fn normalize_feed(o: &Value) -> Result<Value, String> {
    if !o.is_object() && !o.is_array() {
        return Err(crate::i18n::tr("주보 자료가 JSON 이 아닙니다.").into());
    }
    let list = if o.is_array() { arr(o) } else if o["bulletins"].is_array() { arr(&o["bulletins"]) } else if !o["date"].is_null() { vec![o.clone()] } else {
        return Err(crate::i18n::tr("주보 자료에서 'bulletins' 를 찾지 못했습니다. 홈페이지의 주보 JSON 주소(예: …/bulletin/feed.json)인지 확인하세요.").into());
    };
    static DATE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\d{4}-\d{2}-\d{2}").unwrap());
    let mut bulletins: Vec<Value> = list
        .iter()
        .map(|b| {
            let date = s(&b["date"], 40);
            json!({
                "date": if DATE.is_match(&date) { date.chars().take(10).collect::<String>() } else { String::new() },
                "issue": s(&b["issue"], 60), "url": s(&b["url"], 500),
                "sermon": { "title": s(&b["sermon"]["title"], 200), "passage": s(&b["sermon"]["passage"], 100), "preacher": s(&b["sermon"]["preacher"], 80) },
                "verse": if b["verse"]["text"].as_str().is_some_and(|x| !x.is_empty()) { json!({ "text": s(&b["verse"]["text"], 500), "ref": s(&b["verse"]["ref"], 80) }) } else { Value::Null },
                "services": arr(&b["services"]).iter().map(|v| json!({
                    "name": s(&v["name"], 80), "time": s(&v["time"], 40), "host": s(&v["host"], 120),
                    "items": arr(&v["items"]).iter().map(|x| json!({ "part": s(&x["part"], 80), "content": s(&x["content"], 300), "by": s(&x["by"], 80), "stand": x["stand"].as_bool().unwrap_or(false) })).filter(|x| x["part"] != "").collect::<Vec<_>>(),
                })).filter(|v| v["name"] != "" || v["items"].as_array().is_some_and(|a| !a.is_empty())).collect::<Vec<_>>(),
                "news": arr(&b["news"]).iter().map(|n| json!({ "title": s(&n["title"], 200), "body": s(&n["body"], 1500), "link": s(&n["link"], 500) })).filter(|n| n["title"] != "" || n["body"] != "").collect::<Vec<_>>(),
                "prayers": arr(&b["prayers"]).iter().map(|n| json!({ "title": s(&n["title"], 200), "body": s(&n["body"], 1500) })).filter(|n| n["title"] != "" || n["body"] != "").collect::<Vec<_>>(),
                "events": arr(&b["events"]).iter().map(|e| json!({ "when": s(&e["when"], 60), "title": s(&e["title"], 200), "note": s(&e["note"], 300) })).filter(|e| e["title"] != "").collect::<Vec<_>>(),
            })
        })
        .filter(|b| b["date"] != "" || b["services"].as_array().is_some_and(|a| !a.is_empty()) || b["news"].as_array().is_some_and(|a| !a.is_empty()))
        .collect();
    bulletins.sort_by(|a, b| b["date"].as_str().unwrap_or("").cmp(a["date"].as_str().unwrap_or("")));
    if bulletins.is_empty() {
        return Err(crate::i18n::tr("주보가 하나도 없습니다.").into());
    }
    Ok(json!({ "church": s(&o["church"], 120), "updated": s(&o["updated"], 40), "bulletins": bulletins }))
}

/// 주소에서 받기 (실제 네트워크 — 다른 스레드에서)
pub fn fetch_feed(url: &str) -> Result<Value, String> {
    let u: String = url.trim().chars().take(500).collect();
    if !(u.to_lowercase().starts_with("http://") || u.to_lowercase().starts_with("https://")) {
        return Err(crate::i18n::tr("홈페이지 주보 주소를 넣으세요 (예: https://교회주소/bulletin/feed.json).").into());
    }
    // 한글 도메인(구세군대구사랑교회.kr)은 퓨니코드로
    let u = url::Url::parse(&u).map(|x| x.to_string()).map_err(|_| format!("주소가 올바르지 않습니다: {u}"))?;
    let resp = ureq::get(&u).header("Accept", "application/json").config().http_status_as_error(false).build().call().map_err(|e| format!("홈페이지에 연결하지 못했습니다: {e}"))?;
    let st = resp.status().as_u16();
    if !(200..300).contains(&st) {
        return Err(crate::i18n::tr_args("홈페이지에서 주보를 받지 못했습니다 ({1}).", &[&st]));
    }
    let text = resp.into_body().read_to_string().map_err(|e| format!("홈페이지에서 주보를 받지 못했습니다: {e}"))?;
    let o: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|_| "받은 내용이 JSON 이 아닙니다. 주보 JSON 주소(…/bulletin/feed.json)인지 확인하세요.".to_string())?;
    normalize_feed(&o)
}

/// 큐시트의 예배 날짜와 같은 주보 (없으면 가장 최근)
pub fn pick_bulletin<'a>(feed: &'a Value, date: &str) -> Option<&'a Value> {
    let list = feed["bulletins"].as_array()?;
    list.iter().find(|b| !date.is_empty() && b["date"] == date).or(list.first())
}

fn one(t: &str) -> String {
    static N: Lazy<Regex> = Lazy::new(|| Regex::new(r"\n+").unwrap());
    N.replace_all(t, " ").trim().to_string()
}

fn is_news_part(p: &str) -> bool {
    static R: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(교회\s*소식|광고|알림|공지)").unwrap());
    R.is_match(&p.split_whitespace().collect::<Vec<_>>().join(" "))
}

/// 광고 · 일정 · 기도 제목 → 공지 슬라이드 글. sel: { news: [번호…] | "all" | 없음, events: bool, prayers: bool }
pub fn news_body(b: &Value, sel: &Value) -> String {
    let news = arr(&b["news"]);
    let pick: Vec<usize> = match &sel["news"] {
        Value::Array(a) => a.iter().filter_map(|x| x.as_u64().map(|v| v as usize)).collect(),
        _ => (0..news.len()).collect(),
    };
    let mut pages: Vec<String> = vec![];
    for i in pick {
        let Some(n) = news.get(i) else { continue };
        let (t, body) = (n["title"].as_str().unwrap_or(""), n["body"].as_str().unwrap_or(""));
        pages.push([if t.is_empty() { String::new() } else { format!("**{}**", one(t)) }, body.to_string()].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join("\n"));
    }
    let events = arr(&b["events"]);
    if sel["events"].as_bool().unwrap_or(false) && !events.is_empty() {
        let mut lines = vec!["**이번 주 일정**".to_string()];
        for e in &events {
            let (w, t, n) = (e["when"].as_str().unwrap_or(""), e["title"].as_str().unwrap_or(""), e["note"].as_str().unwrap_or(""));
            lines.push(format!("{}{}{}", if w.is_empty() { String::new() } else { format!("{w}  ") }, one(t), if n.is_empty() { String::new() } else { format!(" — {}", one(n)) }));
        }
        pages.push(lines.join("\n"));
    }
    if sel["prayers"].as_bool().unwrap_or(false) {
        for p in arr(&b["prayers"]) {
            let t = p["title"].as_str().filter(|x| !x.is_empty()).unwrap_or("기도 제목");
            pages.push([format!("**{}**", one(t)), p["body"].as_str().unwrap_or("").to_string()].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join("\n"));
        }
    }
    static D: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^\s*-{3,}\s*$").unwrap());
    pages.iter().map(|p| D.replace_all(p, "—").to_string()).collect::<Vec<_>>().join("\n---\n")
}

/// '오전 11:00' · '오후 1:30' · '저녁 8:00' · '14:00' → 'HH:MM'
pub fn to_hhmm(t: &str) -> String {
    static R: Lazy<Regex> = Lazy::new(|| Regex::new(r"(오전|오후|저녁|밤|새벽)?\s*(\d{1,2})\s*[:시]\s*(\d{2})?").unwrap());
    let Some(m) = R.captures(t) else { return String::new() };
    let mut h: u32 = m[2].parse().unwrap_or(0);
    let min = m.get(3).map(|x| x.as_str()).unwrap_or("00");
    if m.get(1).is_some_and(|x| ["오후", "저녁", "밤"].contains(&x.as_str())) && h < 12 {
        h += 12;
    }
    if h > 23 {
        return String::new();
    }
    format!("{h:02}:{min}")
}

/// 주보 한 호의 예배 하나 → 큐시트 계획 (plan_to_saved_cue 가 찬송가 PPT · 장절을 확인)
pub fn service_to_plan(b: &Value, service: usize, looks_like_ref: &dyn Fn(&str) -> bool, news: &Value) -> Result<Value, String> {
    let services = arr(&b["services"]);
    let v = services.get(service).or(services.first()).ok_or("이 주보에는 예배 순서가 없습니다.")?;
    let body = news_body(b, news);
    let mut items: Vec<Value> = vec![];
    let mut placed = false;
    static CLEAN: Lazy<Regex> = Lazy::new(|| Regex::new(r"[/|·•→➔]").unwrap());
    static SONG: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(찬송|찬양|특송|헌금\s*송|봉헌\s*송)").unwrap());
    static QUOTES: Lazy<Regex> = Lazy::new(|| Regex::new(r#"[「」『』“”"'‘’]"#).unwrap());
    let nospace = |t: &str| t.chars().filter(|c| !c.is_whitespace()).collect::<String>();
    for x in arr(&v["items"]) {
        let part = x["part"].as_str().unwrap_or("").to_string();
        let content = x["content"].as_str().unwrap_or("").to_string();
        let by = x["by"].as_str().unwrap_or("").to_string();
        let note = if x["stand"].as_bool().unwrap_or(false) { "*일어서서" } else { "" };
        if is_news_part(&part) {
            items.push(json!({ "type": "text", "title": nospace(&part), "body": body, "note": note }));
            placed = true;
            continue;
        }
        if ["설교", "말씀"].contains(&nospace(&part).as_str()) && !content.is_empty() {
            let sub = [format!("“{content}”"), by.clone()].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · ");
            items.push(json!({ "type": "title", "title": part, "subtitle": sub, "note": note }));
            continue;
        }
        let clean = |t: &str| CLEAN.replace_all(t, " ").split_whitespace().collect::<Vec<_>>().join(" ");
        let line = [clean(&part), clean(&content)].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" ");
        let parsed = crate::bulletin::parse_order_text(&line, looks_like_ref);
        if let Some(p) = parsed["items"].as_array().and_then(|a| a.first()).filter(|p| ["hymn", "bible", "reading"].contains(&p["type"].as_str().unwrap_or(""))) {
            let mut p = p.clone();
            if p["type"] == "bible" {
                p["title"] = json!(nospace(&part));
            }
            let pn = p["note"].as_str().unwrap_or("").to_string();
            p["note"] = json!(if note.is_empty() { pn } else { note.to_string() });
            items.push(p);
            continue;
        }
        if SONG.is_match(&part) && !content.is_empty() && !content.chars().any(|c| c.is_ascii_digit()) {
            items.push(json!({ "type": "song", "title": part, "song": QUOTES.replace_all(&content, "").trim(), "note": note }));
            continue;
        }
        let sub = [content, by].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · ");
        items.push(json!({ "type": "title", "title": part, "subtitle": sub, "note": note }));
    }
    let date = b["date"].as_str().unwrap_or("");
    let name = format!("{}{}", if date.is_empty() { String::new() } else { format!("{date} ") }, v["name"].as_str().filter(|x| !x.is_empty()).unwrap_or("예배"));
    Ok(json!({ "name": name.trim(), "serviceDate": date, "startTime": to_hhmm(v["time"].as_str().unwrap_or("")), "items": items, "newsPlaced": placed }))
}

/// 받은 주보의 예배 하나 → 큐시트 저장
pub fn make_cue(core: &mut Core, bulletin: &Value, service: usize, news: &Value) -> Result<Value, String> {
    let b = normalize_feed(&json!({ "bulletins": [bulletin] }))?["bulletins"][0].clone();
    let plan = service_to_plan(&b, service, &|s| crate::parser::looks_like_reference(s), news)?;
    let mut r = core.plan_to_saved_cue(&plan)?;
    r["newsPlaced"] = plan["newsPlaced"].clone();
    r["model"] = json!("홈페이지 주보");
    Ok(r)
}

/// 광고·일정·기도 제목 → 공지 순서 (넣는 것은 조작 화면: 실행 취소가 되게)
pub fn news_item(bulletin: &Value, news: &Value) -> Result<Value, String> {
    let b = normalize_feed(&json!({ "bulletins": [bulletin] }))?["bulletins"][0].clone();
    let body = news_body(&b, news);
    if body.trim().is_empty() {
        return Err(crate::i18n::tr("넣을 광고가 없습니다. 하나 이상 고르세요.").into());
    }
    Ok(json!({ "type": "text", "title": "교회 소식", "body": body, "note": format!("홈페이지 주보 {}", b["date"].as_str().unwrap_or("")).trim() }))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> Value {
        json!({ "church": "○○교회", "bulletins": [
            { "date": "2026-11-08", "services": [{ "name": "주일예배", "time": "오전 11:00", "items": [{ "part": "찬송", "content": "1장" }] }] },
            { "date": "2026-11-15T00:00", "issue": "제12호", "sermon": { "title": "선한 목자" },
              "services": [{ "name": "주일 낮 예배", "time": "오후 1:30", "items": [
                { "part": "예배로의 부름", "content": "" },
                { "part": "찬송", "content": "새찬송가 8장", "stand": true },
                { "part": "성경 봉독", "content": "요한복음 10:11-15" },
                { "part": "설교", "content": "선한 목자", "by": "홍길동 사관" },
                { "part": "찬양", "content": "「여기에 모인 우리」" },
                { "part": "교회 소식", "content": "" } ] }],
              "news": [{ "title": "바자회", "body": "다음 주일 오후\n---\n친교실" }, { "title": "새가족", "body": "환영합니다" }],
              "events": [{ "when": "11/20 (목)", "title": "구역 모임", "note": "저녁 7시" }],
              "prayers": [{ "title": "", "body": "아픈 이들" }] } ] })
    }
    #[test]
    fn feed_plan_news() {
        let f = normalize_feed(&sample()).unwrap();
        assert_eq!(f["bulletins"][0]["date"], "2026-11-15");
        assert_eq!(pick_bulletin(&f, "2026-11-08").unwrap()["date"], "2026-11-08");
        assert_eq!(pick_bulletin(&f, "").unwrap()["date"], "2026-11-15");
        let b = &f["bulletins"][0];
        let body = news_body(b, &json!({ "news": [0], "events": true, "prayers": true }));
        assert_eq!(body, "**바자회**\n다음 주일 오후\n—\n친교실\n---\n**이번 주 일정**\n11/20 (목)  구역 모임 — 저녁 7시\n---\n**기도 제목**\n아픈 이들");
        let is_ref = |s: &str| crate::parser::looks_like_reference(s);
        let p = service_to_plan(b, 0, &is_ref, &json!({})).unwrap();
        assert_eq!(p["name"], "2026-11-15 주일 낮 예배");
        assert_eq!(p["startTime"], "13:30");
        let it = p["items"].as_array().unwrap();
        let types: Vec<&str> = it.iter().map(|x| x["type"].as_str().unwrap()).collect();
        assert_eq!(types, vec!["title", "hymn", "bible", "title", "song", "text"]);
        assert_eq!(it[1]["note"], "*일어서서");
        assert_eq!(it[2]["title"], "성경봉독");
        assert_eq!(it[3]["subtitle"], "“선한 목자” · 홍길동 사관");
        assert_eq!(it[4]["song"], "여기에 모인 우리");
        assert!(it[5]["body"].as_str().unwrap().contains("새가족"));
        assert_eq!(p["newsPlaced"], true);
        assert_eq!(to_hhmm("저녁 8시"), "20:00");
        assert_eq!(to_hhmm("14:00"), "14:00");
        assert!(normalize_feed(&json!({ "x": 1 })).is_err());
        assert!(news_item(b, &json!({ "news": [] })).is_err());
    }
}

#[cfg(test)]
mod real_tests {
    /// 실제 홈페이지에서 한 번: WP_FEED_URL=… cargo test -p wp-core real_feed -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_feed() {
        let Ok(url) = std::env::var("WP_FEED_URL") else { return };
        let f = super::fetch_feed(&url).unwrap();
        let b = &f["bulletins"][0];
        println!("교회 {} · 주보 {}개 · 최근 {} · 예배 {}개 · 소식 {}개", f["church"], f["bulletins"].as_array().unwrap().len(), b["date"], b["services"].as_array().unwrap().len(), b["news"].as_array().unwrap().len());
        let is_ref = |s: &str| crate::parser::looks_like_reference(s);
        if let Ok(p) = super::service_to_plan(b, 0, &is_ref, &serde_json::json!({})) {
            for it in p["items"].as_array().unwrap() {
                println!("  {} · {} {}", it["type"], it["title"], it["ref"].as_str().or(it["number"].as_i64().map(|_| "").filter(|_| false)).unwrap_or(""));
            }
            println!("이름 {} · 시작 {}", p["name"], p["startTime"]);
        }
    }
}
