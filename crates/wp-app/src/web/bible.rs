//! [웹판] 교회 클라우드의 성경 번역본을 메모리 SQLite 에 넣는다 — 그러면 설치판 코어의 장절 · 낱말 검색이 그대로 돈다.
//! 책마다 `/api/v1/control/bible/book` 으로 받고(번역본 하나 = 66번, 브라우저가 몇 개씩 함께 받음), 받은 것은 IndexedDB 에 둔다.
//! 번역본이 다시 들어오면(importedAt · 절 수가 바뀜) 그때만 다시 받는다 (D1 행 읽기 아끼기).
//! 화면은 먼저 뜨고, 성경은 뒤에서 받는다 (받는 동안 검색 결과가 비어 있음 → 알림).
use eframe::wasm_bindgen::{prelude::*, JsCast as _};
use serde_json::{json, Value};
use wp_core::core::{Ev, Shared};
use wp_core::importer::{Data, Row};

/// index.html 의 window.wpIdb.get/put (Promise)
async fn idb(op: &str, key: &str, val: Option<&str>) -> Option<String> {
    let w = web_sys::window()?;
    let o = js_sys::Reflect::get(&w, &JsValue::from_str("wpIdb")).ok()?;
    let f: js_sys::Function = js_sys::Reflect::get(&o, &JsValue::from_str(op)).ok()?.dyn_into().ok()?;
    let p = match val {
        Some(v) => f.call2(&o, &JsValue::from_str(key), &JsValue::from_str(v)).ok()?,
        None => f.call1(&o, &JsValue::from_str(key)).ok()?,
    };
    wasm_bindgen_futures::JsFuture::from(js_sys::Promise::from(p)).await.ok()?.as_string()
}

fn rows(v: &Value) -> Vec<Row> {
    v.as_array().into_iter().flatten().filter_map(|r| Some(Row { book: r[0].as_i64()?, chapter: r[1].as_i64()?, verse: r[2].as_i64()?, text: r[3].as_str()?.to_string() })).collect()
}

/// 66권을 받는다 → [[책, [[장,절,글]…], [[장,절,글]…]] …]
async fn fetch_books(version: &str) -> Result<Vec<Value>, String> {
    let w = web_sys::window().ok_or("창 없음")?;
    // 모두 먼저 시작해 두면 브라우저가 몇 개씩 함께 받는다
    let started: Vec<(i64, js_sys::Promise)> = (1..=66).map(|b| (b, w.fetch_with_str(&format!("/api/v1/control/bible/book?version={}&book={b}", js_sys::encode_uri_component(version))))).collect();
    let mut out = vec![];
    for (b, p) in started {
        let res: web_sys::Response = wasm_bindgen_futures::JsFuture::from(p).await.map_err(|_| "성경을 받지 못했습니다 (인터넷 연결)".to_string())?.dyn_into().map_err(|_| "응답 오류")?;
        if res.status() == 404 {
            continue;
        }
        let t = wasm_bindgen_futures::JsFuture::from(res.text().map_err(|_| "응답 오류")?).await.ok().and_then(|v| v.as_string()).unwrap_or_default();
        let j: Value = serde_json::from_str(&t).unwrap_or(Value::Null);
        if !res.ok() {
            return Err(j["error"].as_str().unwrap_or("성경을 받지 못했습니다").to_string());
        }
        let vs: Vec<Value> = j["verses"].as_array().into_iter().flatten().map(|r| json!([b, r[0], r[1], r[2]])).collect();
        if vs.is_empty() {
            continue;
        }
        let hs: Vec<Value> = j["headings"].as_array().into_iter().flatten().map(|r| json!([b, r[0], r[1], r[2]])).collect();
        out.push(json!([b, vs, hs]));
    }
    Ok(out)
}

pub fn start(core: Shared) {
    wasm_bindgen_futures::spawn_local(async move {
        if let Err(e) = load(&core).await {
            core.lock().notify(format!("성경을 받지 못했습니다: {e}"), "error");
        }
    });
}

async fn load(core: &Shared) -> Result<(), String> {
    let church = super::cloud::church();
    let (st, v) = super::cloud::api("GET", "/api/v1/bible/versions", None, &[]).await;
    // 6단계: 인터넷이 끊겼으면 이 브라우저에 보관한 번역본 목록 · 본문으로 (받은 적이 있을 때)
    let list_key = format!("bible:{church}:versions");
    let versions = if st == 200 {
        let list = v["versions"].as_array().cloned().unwrap_or_default();
        let _ = idb("put", &list_key, Some(&Value::Array(list.clone()).to_string())).await;
        list
    } else {
        match idb("get", &list_key, None).await.and_then(|s| serde_json::from_str::<Value>(&s).ok()).and_then(|v| v.as_array().cloned()) {
            Some(list) if st == 0 || st >= 500 => list,
            _ => return Err(v["error"].as_str().unwrap_or("번역본 목록").to_string()),
        }
    };
    let offline = st != 200;
    if versions.is_empty() {
        core.lock().notify("교회 클라우드에 성경 번역본이 없습니다. 주인이 번역본을 넣어야 성경 검색이 됩니다 (cloud/scripts/bible-import.mjs).", "info");
        return Ok(());
    }
    for ver in versions {
        let id = ver["id"].as_str().unwrap_or("").to_string();
        if id.is_empty() {
            continue;
        }
        let stamp = format!("{}:{}", ver["importedAt"], ver["verses"]);
        let key = format!("bible:{church}:{id}");
        let cached: Option<Value> = idb("get", &key, None).await.and_then(|s| serde_json::from_str(&s).ok());
        let books = match cached.filter(|c| c["stamp"].as_str() == Some(&stamp)) {
            Some(c) => c["books"].as_array().cloned().unwrap_or_default(),
            None if offline => continue, // 끊긴 동안 보관하지 않은 번역본은 건너뜀
            None => {
                core.lock().notify(format!("성경 '{}' 받는 중…", ver["name"].as_str().unwrap_or(&id)), "info");
                let b = fetch_books(&id).await?;
                let _ = idb("put", &key, Some(&json!({ "stamp": stamp, "books": b }).to_string())).await;
                b
            }
        };
        let (mut verses, mut headings) = (vec![], vec![]);
        for b in &books {
            verses.extend(rows(&b[1]));
            headings.extend(rows(&b[2]));
        }
        let note = ver["licenseNote"].as_str().unwrap_or("").to_string();
        let meta = json!({ "id": id, "name": ver["name"], "language": ver["language"], "copyright": note, "license": note, "source": "교회 클라우드" });
        let n = verses.len();
        let mut c = core.lock();
        wp_core::importer::replace_version(&c.store.db, &meta, &Data { verses, headings, notes: vec![] }).map_err(|e| e.to_string())?;
        let _ = c.store.reload_structure();
        if c.version_id().is_empty() || c.store.list_versions().iter().all(|x| x.id != c.version_id()) {
            c.set_version(&id);
        }
        log::info!("성경 {id}: {n}절");
        c.emit(Ev::Ui);
    }
    Ok(())
}
