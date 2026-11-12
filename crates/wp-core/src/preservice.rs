//! [트랙 A] 예배 전 화면 자동 만들기 (Electron 판 preservice.js)
//! 지난주 말씀카드 + 이번 주 QT 첫날 본문 → 슬라이드쇼 순서 하나 (자동으로 넘기며 반복 · 카운트다운)
//!  - 말씀카드 그림: 설교쇼츠 편집기 '교인 자료' 의 카드 PNG 폴더에서 가장 최근 묶음 (이름_01.png, 이름_02.png …)
//!  - 말씀 글: 설교쇼츠 편집기 '홈페이지 묶음' (<폴더>/<날짜>/bundle.json 의 cardLine)
//!  - QT 첫날 본문: 장절, 또는 QT 자료(JSON days[0] · '1일째 · 제목 (장절)')

use crate::core::Core;
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// 이 PC 의 오늘 'YYYY-MM-DD'
pub fn local_day() -> String {
    let ms = crate::util::now_ms() + crate::localtime::local_offset_ms();
    let z = ms.div_euclid(86_400_000) + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}")
}

#[derive(Clone, Debug)]
pub struct CardFile {
    pub path: PathBuf,
    pub name: String,
}
#[derive(Clone, Debug)]
pub struct CardGroup {
    pub base: String,
    pub mtime: u128,
    pub files: Vec<CardFile>,
}

/// 숫자는 숫자 크기로 견주는 이름 정렬 (이름_2 < 이름_10)
fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    static D: Lazy<Regex> = Lazy::new(|| Regex::new(r"\d+|\D+").unwrap());
    let ta: Vec<&str> = D.find_iter(a).map(|m| m.as_str()).collect();
    let tb: Vec<&str> = D.find_iter(b).map(|m| m.as_str()).collect();
    for (x, y) in ta.iter().zip(tb.iter()) {
        let o = match (x.parse::<u64>(), y.parse::<u64>()) {
            (Ok(p), Ok(q)) => p.cmp(&q),
            _ => x.cmp(y),
        };
        if o != std::cmp::Ordering::Equal {
            return o;
        }
    }
    ta.len().cmp(&tb.len())
}

/// 폴더의 그림들을 '이름_01' 묶음으로 (최근 것 먼저, 최대 max)
pub fn card_groups(folder: &str, max: usize) -> Result<Vec<CardGroup>, String> {
    let dir = folder.trim();
    if dir.is_empty() {
        return Err("말씀카드 그림 폴더를 고르세요.".into());
    }
    let rd = std::fs::read_dir(dir).map_err(|_| format!("폴더를 열 수 없습니다: {dir}"))?;
    static TAIL: Lazy<Regex> = Lazy::new(|| Regex::new(r"[_\-\s]?\(?\d{1,3}\)?$").unwrap());
    let mut groups: Vec<CardGroup> = vec![];
    for e in rd.flatten() {
        let p = e.path();
        if !crate::library::is_image(&p) {
            continue;
        }
        let Ok(m) = e.metadata() else { continue };
        if !m.is_file() {
            continue;
        }
        let name = e.file_name().to_string_lossy().to_string();
        let mt = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis()).unwrap_or(0);
        let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let base = TAIL.replace(&stem, "").to_string();
        let base = if base.is_empty() { name.clone() } else { base };
        match groups.iter_mut().find(|g| g.base == base) {
            Some(g) => {
                g.files.push(CardFile { path: p, name });
                g.mtime = g.mtime.max(mt);
            }
            None => groups.push(CardGroup { base, mtime: mt, files: vec![CardFile { path: p, name }] }),
        }
    }
    groups.sort_by(|a, b| b.mtime.cmp(&a.mtime));
    groups.truncate(max);
    for g in groups.iter_mut() {
        g.files.sort_by(|a, b| natural_cmp(&a.name, &b.name));
    }
    Ok(groups)
}

/// 설교쇼츠 '홈페이지 묶음' 폴더에서 오늘(포함) 이전의 가장 최근 묶음 → { date, cardLine, service }
pub fn latest_bundle(root: &str, today: &str) -> Option<Value> {
    let dir = root.trim();
    if dir.is_empty() {
        return None;
    }
    static DATE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\d{4}-\d{2}-\d{2}$").unwrap());
    let mut dates: Vec<String> = std::fs::read_dir(dir).ok()?.flatten().map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| DATE.is_match(n) && n.as_str() <= today).collect();
    dates.sort();
    dates.reverse();
    for d in dates {
        let Ok(t) = std::fs::read_to_string(Path::new(dir).join(&d).join("bundle.json")) else { continue };
        let Ok(b) = serde_json::from_str::<Value>(t.trim_start_matches('\u{feff}')) else { continue };
        return Some(json!({ "date": d, "cardLine": b["cardLine"].as_str().unwrap_or("").trim(), "service": b["service"].as_str().unwrap_or("") }));
    }
    None
}

/// QT 자료에서 첫날 본문 → { title, dayTitle, ref }
pub fn parse_qt(text: &str, looks_like_ref: &dyn Fn(&str) -> bool) -> Result<Value, String> {
    let t = text.trim_start_matches('\u{feff}').trim();
    if t.is_empty() {
        return Err("이번 주 QT 첫날 본문(장절)을 넣거나 QT 자료를 붙여 넣으세요.".into());
    }
    static BLK: Lazy<Regex> = Lazy::new(|| Regex::new(r"```(?:json|JSON)?\s*([\s\S]*?)```").unwrap());
    let mut cands: Vec<String> = BLK.captures_iter(t).map(|m| m[1].to_string()).collect();
    cands.push(t.to_string());
    for b in cands {
        if !b.contains(['{', '[']) {
            continue;
        }
        let (Some(a), Some(z)) = (b.find('{'), b.rfind('}')) else { continue };
        if z <= a {
            continue;
        }
        if let Ok(o) = serde_json::from_str::<Value>(&b[a..=z]) {
            let qt = if o["qt"].is_object() { &o["qt"] } else { &o };
            if let Some(d) = qt["days"].as_array().and_then(|ds| ds.iter().find(|x| x["passage"].as_str().is_some_and(|p| !p.is_empty()))) {
                return Ok(json!({ "title": qt["title"].as_str().unwrap_or("").trim(), "dayTitle": d["title"].as_str().unwrap_or("").trim(), "ref": d["passage"].as_str().unwrap_or("").trim() }));
            }
        }
    }
    let lines: Vec<&str> = t.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
    static DAY1: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)^(?:1\s*일\s*째|첫\s*(?:째\s*)?날|1\s*일|day\s*1)\s*[·:：.\-–]?\s*(.*?)\s*[(（]([^()（）]+)[)）]\s*$").unwrap());
    for l in &lines {
        if let Some(m) = DAY1.captures(l) {
            if looks_like_ref(&m[2]) {
                return Ok(json!({ "title": "", "dayTitle": m[1].trim(), "ref": m[2].trim() }));
            }
        }
    }
    static INNER: Lazy<Regex> = Lazy::new(|| Regex::new(r"[(（]([^()（）]+)[)）]").unwrap());
    static LEAD: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[^·:：]*[·:：]\s*").unwrap());
    for l in &lines {
        if let Some(m) = INNER.captures(l) {
            if looks_like_ref(&m[1]) {
                let rest = l.replacen(&m[0], "", 1);
                return Ok(json!({ "title": "", "dayTitle": LEAD.replace(&rest, "").trim(), "ref": m[1].trim() }));
            }
        }
        if looks_like_ref(l) {
            return Ok(json!({ "title": "", "dayTitle": "", "ref": l }));
        }
    }
    Err("QT 첫날 본문 장절을 찾지 못했습니다. \"마 6:9-13\" 처럼 장절을 넣어 주세요.".into())
}

/// 절들 → 슬라이드 글 (한 장에 3절 또는 160자 안쪽)
pub fn verse_slides(verses: &[Value], head: &str) -> Vec<String> {
    let mut out: Vec<Vec<String>> = vec![];
    let mut cur: Vec<String> = vec![];
    let mut len = 0;
    for v in verses {
        let t = format!("{} {}", v["verse"], v["text"].as_str().unwrap_or("").trim());
        let n = t.chars().count();
        if !cur.is_empty() && (cur.len() >= 3 || len + n > 160) {
            out.push(std::mem::take(&mut cur));
            len = 0;
        }
        cur.push(t);
        len += n;
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out.into_iter().map(|c| format!("**{head}**\n{}", c.join("\n"))).collect()
}

fn one(t: &str) -> String {
    static D: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^\s*-{3,}\s*$").unwrap());
    D.replace_all(&t.replace('\r', ""), "—").trim().to_string()
}

/// 슬라이드쇼 순서 만들기
/// a: { images:[url], cardLine, verse:{ text, ref }, qt:{ title, dayTitle, ref, passage:{ title, verses } }, countdown:'HH:MM', autoSec }
pub fn build_item(a: &Value) -> Result<Value, String> {
    let mut pages: Vec<String> = vec![];
    if let Some(c) = a["cardLine"].as_str().map(one).filter(|c| !c.is_empty()) {
        pages.push(format!("**지난주 말씀**\n{c}"));
    }
    if let Some(t) = a["verse"]["text"].as_str().filter(|t| !t.is_empty()) {
        let r = one(a["verse"]["ref"].as_str().unwrap_or(""));
        pages.push(format!("**{}**\n{}", if r.is_empty() { "이번 주 말씀".into() } else { r }, one(t)));
    }
    let qt = &a["qt"];
    if let Some(vs) = qt["passage"]["verses"].as_array().filter(|v| !v.is_empty()) {
        let r = one(qt["passage"]["title"].as_str().filter(|x| !x.is_empty()).or(qt["ref"].as_str()).unwrap_or(""));
        let dt = one(qt["dayTitle"].as_str().filter(|x| !x.is_empty()).or(qt["title"].as_str()).unwrap_or(""));
        pages.push(["**이번 주 QT · 첫째 날**".to_string(), dt, r.clone()].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join("\n"));
        pages.extend(verse_slides(vs, &r));
    }
    let images: Vec<String> = a["images"].as_array().into_iter().flatten().filter_map(|x| x.as_str()).filter(|x| !x.is_empty()).take(60).map(String::from).collect();
    if images.is_empty() && pages.is_empty() {
        return Err("넣을 것이 없습니다. 말씀카드 그림·말씀 글·QT 본문 중 하나 이상 고르세요.".into());
    }
    let sec = a["autoSec"].as_f64().filter(|x| *x > 0.0).unwrap_or(8.0).round().clamp(3.0, 60.0);
    static HM: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\d{1,2}:\d{2}$").unwrap());
    let cd = a["countdown"].as_str().filter(|x| HM.is_match(x)).unwrap_or("");
    Ok(json!({
        "type": "slideshow", "title": "예배 전 화면", "images": images, "body": pages.join("\n---\n"),
        "autoSec": sec, "autoLoop": true, "countdown": cd, "countdownLabel": "예배 시작까지",
        "note": format!("말씀카드 {}장 · 글 {}장", images.len(), pages.len()),
    }))
}

/// 예배 전 설정 (카드 폴더 · 묶음 폴더 · 넘김 초). 묶음 폴더가 비면 흔한 자리를 찾아본다
pub fn info(core: &Core) -> Value {
    let p = &core.settings.get()["preService"];
    let mut bundle = p["bundleDir"].as_str().unwrap_or("").to_string();
    if bundle.is_empty() {
        let docs = dirs::document_dir().map(|d| d.join("홈페이지보내기"));
        for g in [Some(PathBuf::from(r"F:\aside\홈페이지보내기")), docs].into_iter().flatten() {
            if g.is_dir() {
                bundle = g.to_string_lossy().to_string();
                break;
            }
        }
    }
    json!({ "cardFolder": p["cardFolder"].as_str().unwrap_or(""), "bundleDir": bundle, "autoSec": p["autoSec"].as_f64().unwrap_or(8.0), "feedUrl": core.settings.get()["home"]["feedUrl"].as_str().unwrap_or("") })
}

/// 말씀카드 폴더 → 묶음들 (그림은 끌어다 놓은 파일처럼 등록해 출력 화면에 보이게) · 폴더를 기억
pub fn cards(core: &mut Core, folder: &str) -> Result<Value, String> {
    let groups = card_groups(folder, 6)?;
    let paths: Vec<PathBuf> = groups.iter().flat_map(|g| g.files.iter().map(|f| f.path.clone())).take(80).collect();
    if !paths.is_empty() {
        core.settings.update(|d| {
            crate::library::add_dropped(d, &paths);
        });
    }
    let f = folder.trim().chars().take(500).collect::<String>();
    core.settings.update(|d| {
        if !d["preService"].is_object() {
            d["preService"] = json!({});
        }
        d["preService"]["cardFolder"] = json!(f);
    });
    Ok(json!({ "groups": groups.iter().map(|g| json!({ "base": g.base, "mtime": g.mtime as f64, "files": g.files.iter().map(|f| json!({ "name": f.name, "path": f.path.to_string_lossy(), "url": crate::library::file_url(&f.path) })).collect::<Vec<_>>() })).collect::<Vec<_>>() }))
}

/// 홈페이지 묶음 폴더 → 최근 카드 문장 · 폴더를 기억
pub fn bundle(core: &mut Core, dir: &str) -> Option<Value> {
    let d = dir.trim().to_string();
    if !d.is_empty() {
        let dd: String = d.chars().take(500).collect();
        core.settings.update(|s| {
            if !s["preService"].is_object() {
                s["preService"] = json!({});
            }
            s["preService"]["bundleDir"] = json!(dd);
        });
    }
    latest_bundle(&d, &local_day())
}

/// QT 자료·장절 → 첫날 본문 (성경 DB)
pub fn qt(core: &mut Core, text: &str) -> Result<Value, String> {
    let mut q = parse_qt(text, &|s| crate::parser::looks_like_reference(s))?;
    let v = core.version_id();
    let r = core.store.resolve_reference(&v, q["ref"].as_str().unwrap_or(""));
    let verses = r["passage"]["verses"].as_array().cloned().unwrap_or_default();
    if r["ok"] != json!(true) || verses.is_empty() {
        return Err(format!("'{}' 본문을 성경에서 찾지 못했습니다. 장절을 확인하세요.", q["ref"].as_str().unwrap_or("")));
    }
    q["passage"] = json!({ "title": r["passage"]["title"], "verses": verses.iter().take(30).map(|v| json!({ "verse": v["verse"], "text": v["text"] })).collect::<Vec<_>>() });
    Ok(q)
}

/// 넘김 초를 기억하고 순서 만들기
pub fn item(core: &mut Core, a: &Value) -> Result<Value, String> {
    if let Some(sec) = a["autoSec"].as_f64().filter(|x| *x > 0.0) {
        let s = sec.round().clamp(3.0, 60.0);
        core.settings.update(|d| {
            if !d["preService"].is_object() {
                d["preService"] = json!({});
            }
            d["preService"]["autoSec"] = json!(s);
        });
    }
    build_item(a)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn groups_bundle_qt_item() {
        let root = std::env::temp_dir().join(format!("wp-pre-{}", std::process::id()));
        let cards = root.join("cards");
        std::fs::create_dir_all(&cards).unwrap();
        for n in ["옛카드_01.png", "옛카드_02.png"] {
            std::fs::write(cards.join(n), b"x").unwrap();
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
        for n in ["말씀카드_10.png", "말씀카드_2.png", "말씀카드_01.png", "메모.txt"] {
            std::fs::write(cards.join(n), b"x").unwrap();
        }
        let g = card_groups(&cards.to_string_lossy(), 6).unwrap();
        assert_eq!(g.len(), 2);
        assert_eq!(g[0].base, "말씀카드");
        assert_eq!(g[0].files.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), vec!["말씀카드_01.png", "말씀카드_2.png", "말씀카드_10.png"]);
        // 홈페이지 묶음: 오늘 이후 날짜는 건너뜀
        let b = root.join("bundles");
        for (d, line) in [("2026-11-08", "지난 말씀"), ("2026-11-15", "다음 주")] {
            std::fs::create_dir_all(b.join(d)).unwrap();
            std::fs::write(b.join(d).join("bundle.json"), json!({ "cardLine": line }).to_string()).unwrap();
        }
        let lb = latest_bundle(&b.to_string_lossy(), "2026-11-12").unwrap();
        assert_eq!((lb["date"].as_str(), lb["cardLine"].as_str()), (Some("2026-11-08"), Some("지난 말씀")));
        // QT
        let is_ref = |s: &str| crate::parser::looks_like_reference(s);
        assert_eq!(parse_qt("1일째 · 기도의 시작 (마태복음 6:9-10)\n2일째 · 둘째 (마 6:11)", &is_ref).unwrap()["ref"], "마태복음 6:9-10");
        assert_eq!(parse_qt("{\"qt\":{\"title\":\"주기도\",\"days\":[{\"day\":1,\"title\":\"하나\",\"passage\":\"마 6:9-13\"}]}}", &is_ref).unwrap()["dayTitle"], "하나");
        assert_eq!(parse_qt("마 6:9-13", &is_ref).unwrap()["ref"], "마 6:9-13");
        assert!(parse_qt("아무 글", &is_ref).is_err());
        // 순서
        let vs: Vec<Value> = (9..=13).map(|n| json!({ "verse": n, "text": "말씀" })).collect();
        let it = build_item(&json!({ "images": ["/lib/file?p=a"], "cardLine": "지난 말씀", "qt": { "dayTitle": "하나", "ref": "마 6:9-13", "passage": { "title": "마태복음 6:9-13", "verses": vs } }, "countdown": "11:00", "autoSec": 1 })).unwrap();
        assert_eq!(it["type"], "slideshow");
        assert_eq!(it["autoSec"], 3.0);
        assert_eq!(it["countdown"], "11:00");
        assert_eq!(it["body"].as_str().unwrap().matches("\n---\n").count(), 3); // 카드 글 · QT 머리 · 절 3+2
        assert!(build_item(&json!({})).is_err());
        assert_eq!(local_day().len(), 10);
        let _ = std::fs::remove_dir_all(&root);
    }
}
