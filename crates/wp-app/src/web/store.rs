//! [웹판] 저장 층 — 계약 docs/web/architecture.md 5절의 `Transport` (문서 읽기 · 쓰기 · 구독 · 송출 · 명령) 가운데 '문서' 쪽.
//!
//! 지금 구현: `Local` — 큐시트 · 찬양 문서를 브라우저 localStorage(`wpdocs:v1`)에 둔다. 처음에는 견본(seed.json),
//! 고친 뒤에는 저장해 둔 문서가 견본보다 앞선다. 성경 본문은 문서가 아니다(견본 · 2단계 /api/v1/bible).
//! 다음: `Cloud` — 같은 문서 모양(payload = 설치판 JSON)을 `/api/v1/documents/{kind}/{id}` 에 If-Match(revision)로.
//!   409 면 자동으로 덮어쓰지 않고 사본을 만들어 사람이 고르게 한다 (계약 6절 · 웹 편집기 충돌 규칙).
//! 송출은 store 가 아니라 live.rs (BroadcastChannel · 뒤에 /bridge).
use eframe::wasm_bindgen::{self, prelude::*, JsCast as _};
use serde_json::Value;
use std::cell::Cell;
use wp_core::core::{Ev, Shared};

const KEY: &str = "wpdocs:v1";

/// 문서 저장소 한 겹. 2단계에서 Cloud 를 더해 갈아 끼운다
pub trait DocStore {
    fn load(&self) -> Option<Value>;
    fn save(&self, docs: &Value) -> Result<(), String>;
}

pub struct Local;

impl DocStore for Local {
    fn load(&self) -> Option<Value> {
        let s = web_sys::window()?.local_storage().ok()??.get_item(KEY).ok()??;
        serde_json::from_str(&s).ok()
    }
    fn save(&self, docs: &Value) -> Result<(), String> {
        let st = web_sys::window().and_then(|w| w.local_storage().ok().flatten()).ok_or("브라우저 저장소를 쓸 수 없습니다")?;
        st.set_item(KEY, &docs.to_string()).map_err(|_| "브라우저 저장 공간이 가득 찼거나 막혀 있습니다".to_string())
    }
}

thread_local! {
    static PENDING: Cell<bool> = const { Cell::new(false) };
    static LAST: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// 저장해 둔 문서가 있으면 DB 에 넣는다 (견본을 넣은 뒤에 불러 견본보다 앞서게)
pub fn restore(core: &mut wp_core::core::Core) {
    if let Some(d) = Local.load() {
        match wp_core::web::seed::import_docs(core, &d) {
            Ok((c, s)) => log::info!("저장해 둔 문서: 큐시트 {c} · 찬양 {s}"),
            Err(e) => log::warn!("저장해 둔 문서를 넣지 못함: {e:#}"),
        }
    }
}

fn flush(core: &Shared) {
    PENDING.with(|p| p.set(false));
    let docs = wp_core::web::seed::export_docs(&core.lock());
    let s = docs.to_string();
    if LAST.with(|l| *l.borrow() == s) {
        return;
    }
    match Local.save(&docs) {
        Ok(()) => LAST.with(|l| *l.borrow_mut() = s),
        Err(e) => core.lock().notify(format!("자료를 브라우저에 저장하지 못했습니다: {e}"), "error"),
    }
}

/// 바뀔 때마다(Ev::Ui) 1초 모아서 저장. 페이지를 떠날 때(pagehide)도 한 번
pub fn start(core: Shared) {
    LAST.with(|l| *l.borrow_mut() = wp_core::web::seed::export_docs(&core.lock()).to_string());
    let mut rx = core.lock().tx.subscribe();
    let c = core.clone();
    wasm_bindgen_futures::spawn_local(async move {
        loop {
            match rx.recv().await {
                Ok(Ev::Ui) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    if PENDING.with(|p| p.replace(true)) {
                        continue;
                    }
                    let c2 = c.clone();
                    let cb = Closure::once_into_js(move || flush(&c2));
                    if let Some(w) = web_sys::window() {
                        let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(cb.unchecked_ref(), 1000);
                    }
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
    });
    let c = core.clone();
    let hide = Closure::<dyn FnMut()>::new(move || flush(&c));
    if let Some(w) = web_sys::window() {
        let _ = w.add_event_listener_with_callback("pagehide", hide.as_ref().unchecked_ref());
    }
    hide.forget();
}

// ================ 클라우드 (2단계 길: /api/v1/documents · /api/v1/sync) ================
// 로그인한 조작 화면은 큐시트 · 찬양을 교회 클라우드에서 받고, 고친 것을 문서마다 If-Match(판)로 올린다.
// 충돌(409)이면 덮어쓰지 않는다: 최신 것을 불러오고, 이 화면에서 고친 것은 '(내 사본)' 새 문서로 남긴다 (사람이 고름).
// 20초마다(보이는 동안) 동기화 받기로 다른 곳(설치판 · 다른 웹)에서 고친 것을 들인다.
// 문서 모양(payload)은 설치판 JSON 그대로 — wp_core::web::seed::export_docs / import_docs.

use serde_json::json;
use std::collections::HashMap;

type Key = (String, String);

#[derive(Default)]
struct Cloud {
    rev: HashMap<Key, i64>,
    /// 마지막으로 클라우드와 맞춘 본문 (같으면 올리지 않는다)
    sent: HashMap<Key, String>,
    head: i64,
    busy: bool,
    again: bool,
    ready: bool,
    warned: bool,
    /// 내가 올린 변경 (동기화 받기에서 다시 오면 건너뜀)
    mine: std::collections::HashSet<String>,
}

thread_local! {
    static CL: std::cell::RefCell<Cloud> = std::cell::RefCell::new(Cloud::default());
    static CPENDING: Cell<bool> = const { Cell::new(false) };
}

fn kinds() -> [&'static str; 2] {
    ["cue", "song"]
}

/// export_docs 를 (종류, id) → 본문 으로
fn exported(core: &Shared) -> HashMap<Key, Value> {
    let d = wp_core::web::seed::export_docs(&core.lock());
    let mut m = HashMap::new();
    for k in kinds() {
        for x in d[k].as_array().into_iter().flatten() {
            if let Some(id) = x["id"].as_str() {
                m.insert((k.to_string(), id.to_string()), x.clone());
            }
        }
    }
    m
}

fn mutation_id() -> String {
    format!("web-{:x}-{:x}", js_sys::Date::now() as u64, (js_sys::Math::random() * 1e12) as u64)
}

fn doc_path(kind: &str, id: &str) -> String {
    format!("/api/v1/documents/{kind}/{}", String::from(js_sys::encode_uri_component(id)))
}

/// 하나만 DB 에 넣는다 (고친 곳이 다른 곳일 때 · 충돌 뒤 최신)
fn import_one(core: &Shared, kind: &str, payload: &Value) {
    let mut c = core.lock();
    let open = c.cue.as_ref().map(|q| q.id.clone());
    let _ = wp_core::web::seed::import_docs(&mut c, &json!({ kind: [payload] }));
    // 열려 있는 큐시트가 바뀌었으면 화면도 새것으로
    if kind == "cue" && open.as_deref() == payload["id"].as_str() {
        if let Some(id) = open {
            c.open_cue(&id);
        }
    }
    c.emit(Ev::Ui);
}

async fn list_all(kind: &str) -> Result<Vec<Value>, String> {
    let (mut out, mut cursor) = (vec![], String::new());
    loop {
        let q = if cursor.is_empty() { String::new() } else { format!("&cursor={}", String::from(js_sys::encode_uri_component(&cursor))) };
        let (st, v) = super::cloud::api("GET", &format!("/api/v1/documents?kind={kind}&include=payload&limit=200{q}"), None, &[]).await;
        if st != 200 {
            return Err(v["error"].as_str().unwrap_or("문서 목록을 받지 못했습니다").to_string());
        }
        out.extend(v["documents"].as_array().cloned().unwrap_or_default());
        match v["next"].as_str() {
            Some(n) if !n.is_empty() => cursor = n.to_string(),
            _ => return Ok(out),
        }
    }
}

/// 로그인한 조작 화면: 받기 → 고칠 때마다 올리기 → 20초마다 받기
pub fn cloud_start(core: Shared) {
    wasm_bindgen_futures::spawn_local(async move {
        let mut docs = json!({ "cue": [], "song": [] });
        let mut revs = vec![];
        for k in kinds() {
            match list_all(k).await {
                Ok(list) => {
                    for d in list {
                        if d["deleted"].as_bool().unwrap_or(false) || !d["payload"].is_object() {
                            continue;
                        }
                        revs.push(((k.to_string(), d["id"].as_str().unwrap_or("").to_string()), d["revision"].as_i64().unwrap_or(0)));
                        docs[k].as_array_mut().unwrap().push(d["payload"].clone());
                    }
                }
                Err(e) => {
                    core.lock().notify(format!("교회 자료를 받지 못했습니다: {e}"), "error");
                    return;
                }
            }
        }
        {
            let mut c = core.lock();
            let n = (docs["cue"].as_array().map_or(0, |a| a.len()), docs["song"].as_array().map_or(0, |a| a.len()));
            if let Err(e) = wp_core::web::seed::import_docs(&mut c, &docs) {
                c.notify(format!("교회 자료를 넣지 못했습니다: {e:#}"), "error");
            }
            // 처음 연 큐시트: 설정에 남은 것(이 브라우저) 또는 목록 첫째
            let want = c.settings.get()["activeCueId"].as_str().unwrap_or("").to_string();
            let pick = if c.cues.iter().any(|x| x.id == want) { Some(want) } else { c.cues.first().map(|x| x.id.clone()) };
            if let Some(id) = pick {
                c.open_cue(&id);
            }
            log::info!("교회 자료: 큐시트 {} · 찬양 {}", n.0, n.1);
            c.emit(Ev::Ui);
        }
        let (_, h) = super::cloud::api("GET", "/api/v1/sync?after=0&limit=1", None, &[]).await;
        let now = exported(&core);
        CL.with(|cl| {
            let mut cl = cl.borrow_mut();
            for (k, r) in revs {
                if let Some(p) = now.get(&k) {
                    cl.sent.insert(k.clone(), p.to_string());
                }
                cl.rev.insert(k, r);
            }
            cl.head = h["head"].as_i64().unwrap_or(0);
            cl.ready = true;
        });
        // 고칠 때마다 (1.5초 모아서)
        let mut rx = core.lock().tx.subscribe();
        let c2 = core.clone();
        wasm_bindgen_futures::spawn_local(async move {
            loop {
                match rx.recv().await {
                    Ok(Ev::Ui) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        if CPENDING.with(|p| p.replace(true)) {
                            continue;
                        }
                        let c3 = c2.clone();
                        wp_core::web::defer(1500, move || {
                            CPENDING.with(|p| p.set(false));
                            wasm_bindgen_futures::spawn_local(push(c3));
                        });
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
        });
        // 20초마다 받기 (숨긴 탭은 묻지 않는다 — 요청 수)
        let c4 = core.clone();
        let tick = Closure::<dyn FnMut()>::new(move || {
            let hidden = web_sys::window().and_then(|w| w.document()).is_some_and(|d| d.hidden());
            if !hidden {
                wasm_bindgen_futures::spawn_local(pull(c4.clone()));
            }
        });
        if let Some(w) = web_sys::window() {
            let _ = w.set_interval_with_callback_and_timeout_and_arguments_0(tick.as_ref().unchecked_ref(), 20_000);
        }
        tick.forget();
    });
}

/// 바뀐 문서를 올린다
async fn push(core: Shared) {
    if !CL.with(|c| c.borrow().ready) {
        return;
    }
    if !super::cloud::can("write") {
        // 보기만 권한: 화면에서는 고쳐지지만 클라우드에는 남지 않는다 — 한 번만 알림
        let now = exported(&core);
        let changed = CL.with(|c| now.iter().any(|(k, v)| c.borrow().sent.get(k) != Some(&v.to_string())));
        if changed && !CL.with(|c| std::mem::replace(&mut c.borrow_mut().warned, true)) {
            core.lock().notify("보기 권한이라 고친 것이 교회 클라우드에 저장되지 않습니다.", "error");
        }
        return;
    }
    let go = CL.with(|c| {
        let mut c = c.borrow_mut();
        if c.busy {
            c.again = true;
            return false;
        }
        c.busy = true;
        true
    });
    if !go {
        return;
    }
    loop {
        let now = exported(&core);
        let todo: Vec<(Key, Value)> = CL.with(|c| {
            let c = c.borrow();
            now.iter().filter(|(k, v)| c.sent.get(*k) != Some(&v.to_string())).map(|(k, v)| (k.clone(), v.clone())).collect()
        });
        let gone: Vec<Key> = CL.with(|c| c.borrow().sent.keys().filter(|k| !now.contains_key(*k)).cloned().collect());
        for (k, v) in todo {
            put(&core, &k, &v).await;
        }
        for k in gone {
            delete(&core, &k).await;
        }
        if !CL.with(|c| std::mem::replace(&mut c.borrow_mut().again, false)) {
            break;
        }
    }
    CL.with(|c| c.borrow_mut().busy = false);
}

async fn put(core: &Shared, k: &Key, payload: &Value) {
    let rev = CL.with(|c| c.borrow().rev.get(k).copied());
    let mid = mutation_id();
    let rv = rev.map(|r| r.to_string()).unwrap_or_default();
    let mut h: Vec<(&str, &str)> = vec![("x-mutation-id", &mid)];
    if rev.is_some() {
        h.push(("if-match", &rv));
    }
    let (st, v) = super::cloud::api("PUT", &doc_path(&k.0, &k.1), Some(payload), &h).await;
    match st {
        200 | 201 => CL.with(|c| {
            let mut c = c.borrow_mut();
            c.rev.insert(k.clone(), v["revision"].as_i64().unwrap_or(0));
            c.sent.insert(k.clone(), payload.to_string());
            c.mine.insert(mid.clone());
            c.warned = false;
        }),
        409 => conflict(core, k, payload, &v["current"]).await,
        _ => {
            let e = v["error"].as_str().unwrap_or("인터넷 연결을 확인해 주세요").to_string();
            // 다음 고침 때 다시 해 본다 (sent 를 바꾸지 않음). 같은 오류를 자꾸 띄우지 않게 한 번만
            if !CL.with(|c| std::mem::replace(&mut c.borrow_mut().warned, true)) {
                core.lock().notify(format!("교회 클라우드에 저장하지 못했습니다 ({st}): {e}"), "error");
            }
        }
    }
}

/// 다른 곳에서 먼저 고쳤다: 최신 것을 들이고, 이 화면의 것은 '(내 사본)' 새 문서로
async fn conflict(core: &Shared, k: &Key, mine: &Value, current: &Value) {
    let copy_id = format!("{}~{:x}", k.1, js_sys::Date::now() as u64);
    let mut copy = mine.clone();
    copy["id"] = json!(copy_id);
    let name_key = if k.0 == "cue" { "name" } else { "title" };
    let name = mine[name_key].as_str().unwrap_or("").to_string();
    copy[name_key] = json!(format!("{name} (내 사본)"));
    let (st, v) = super::cloud::api("PUT", &doc_path(&k.0, &copy_id), Some(&copy), &[("x-mutation-id", &mutation_id())]).await;
    if st == 201 || st == 200 {
        import_one(core, &k.0, &copy);
        let now = exported(core);
        CL.with(|c| {
            let mut c = c.borrow_mut();
            let ck = (k.0.clone(), copy_id.clone());
            c.rev.insert(ck.clone(), v["revision"].as_i64().unwrap_or(1));
            if let Some(p) = now.get(&ck) {
                c.sent.insert(ck, p.to_string());
            }
        });
    }
    if current["payload"].is_object() && !current["deleted"].as_bool().unwrap_or(false) {
        import_one(core, &k.0, &current["payload"]);
        let now = exported(core);
        CL.with(|c| {
            let mut c = c.borrow_mut();
            c.rev.insert(k.clone(), current["revision"].as_i64().unwrap_or(0));
            if let Some(p) = now.get(k) {
                c.sent.insert(k.clone(), p.to_string());
            }
        });
    } else {
        CL.with(|c| {
            let mut c = c.borrow_mut();
            c.rev.remove(k);
            c.sent.remove(k);
        });
    }
    core.lock().notify(format!("다른 곳에서 먼저 고친 '{name}' — 최신 것을 불러왔고, 이 화면에서 고친 것은 '(내 사본)' 으로 남겼습니다."), "error");
}

async fn delete(core: &Shared, k: &Key) {
    let Some(rev) = CL.with(|c| c.borrow().rev.get(k).copied()) else {
        CL.with(|c| c.borrow_mut().sent.remove(k));
        return;
    };
    let mid = mutation_id();
    let rs = rev.to_string();
    let (st, v) = super::cloud::api("DELETE", &doc_path(&k.0, &k.1), None, &[("if-match", &rs), ("x-mutation-id", &mid)]).await;
    if st == 200 || st == 404 {
        CL.with(|c| {
            let mut c = c.borrow_mut();
            c.rev.remove(k);
            c.sent.remove(k);
            c.mine.insert(mid);
        });
    } else if st == 409 && v["current"]["payload"].is_object() {
        // 지우려는 사이에 다른 곳에서 고침: 지우지 않고 최신 것으로 되살린다
        import_one(core, &k.0, &v["current"]["payload"]);
        let now = exported(core);
        CL.with(|c| {
            let mut c = c.borrow_mut();
            c.rev.insert(k.clone(), v["current"]["revision"].as_i64().unwrap_or(0));
            if let Some(p) = now.get(k) {
                c.sent.insert(k.clone(), p.to_string());
            }
        });
        core.lock().notify("다른 곳에서 고친 문서라 지우지 않고 최신 것으로 되살렸습니다.", "error");
    }
}

/// 다른 곳에서 고친 것을 들인다
async fn pull(core: Shared) {
    if !CL.with(|c| c.borrow().ready && !c.borrow().busy) {
        return;
    }
    loop {
        let after = CL.with(|c| c.borrow().head);
        let (st, v) = super::cloud::api("GET", &format!("/api/v1/sync?after={after}"), None, &[]).await;
        if st != 200 {
            return;
        }
        let now = exported(&core);
        for ch in v["changes"].as_array().into_iter().flatten() {
            if ch["superseded"].as_bool().unwrap_or(false) {
                continue;
            }
            let kind = ch["kind"].as_str().unwrap_or("");
            if !kinds().contains(&kind) {
                continue;
            }
            let k: Key = (kind.to_string(), ch["id"].as_str().unwrap_or("").to_string());
            if ch["mutationId"].as_str().is_some_and(|m| CL.with(|c| c.borrow().mine.contains(m))) {
                continue;
            }
            // 이 화면에서 고치다 만 것이 있으면 들이지 않는다 — 올릴 때 409 로 사본 규칙을 따른다
            if CL.with(|c| now.get(&k).is_some_and(|p| c.borrow().sent.get(&k) != Some(&p.to_string()))) {
                continue;
            }
            if ch["op"] == "delete" {
                {
                    let mut c = core.lock();
                    if kind == "cue" {
                        let _ = c.store.db.execute("UPDATE Cuesheets SET deleted_at = ?2 WHERE id = ?1", [k.1.as_str(), wp_core::util::iso_now().as_str()]);
                        c.cues = c.store.list_cues();
                    } else {
                        let _ = wp_core::songs::delete_song(&c.store.db, &k.1);
                    }
                    c.emit(Ev::Ui);
                }
                CL.with(|c| {
                    let mut c = c.borrow_mut();
                    c.rev.remove(&k);
                    c.sent.remove(&k);
                });
            } else if ch["payload"].is_object() {
                import_one(&core, kind, &ch["payload"]);
                let after_import = exported(&core);
                CL.with(|c| {
                    let mut c = c.borrow_mut();
                    c.rev.insert(k.clone(), ch["revision"].as_i64().unwrap_or(0));
                    if let Some(p) = after_import.get(&k) {
                        c.sent.insert(k.clone(), p.to_string());
                    }
                });
            }
        }
        CL.with(|c| c.borrow_mut().head = v["next"].as_i64().unwrap_or(after));
        if !v["more"].as_bool().unwrap_or(false) {
            return;
        }
    }
}
