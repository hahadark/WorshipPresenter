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


// ================ 클라우드 (2단계 길: /api/v1/documents · /api/v1/sync) + 6단계 오프라인 ================
// 로그인한 조작 화면은 큐시트 · 찬양을 교회 클라우드에서 받고, 고친 것을 '올릴 줄'(outbox.rs)에 먼저 쓴 뒤 `POST /api/v1/sync` 로 20개씩 올린다.
// 줄 · 받은 문서 · 받은 차례(head)는 IndexedDB(offline.rs → window.wpOffline.kv)에 둔다 → 새로 고쳐도 · 인터넷이 끊겨도 남는다.
// 인터넷이 끊기면 고친 것은 줄에 쌓이고, 다시 이어지면(3 → 6 → … 15초마다 · 브라우저 online 사건) 올린 뒤 받는다(`more` 끝까지).
// 충돌(판이 다름)은 덮어쓰지 않는다 — 설치판(cloudsync.rs)과 같은 규칙: 원래 자리 = 클라우드 판, 이 화면의 것 = 새 id 의 '○○ (충돌 사본 HH:MM)'.
// 20초마다(보이는 동안) 받기로 다른 곳(설치판 · 다른 웹)에서 고친 것을 들인다. 다음 · 이전 같은 순간 조작은 문서가 아니라 줄에 들어오지 않는다.
// 문서 모양(payload)은 설치판 JSON 그대로 — wp_core::web::seed::export_docs / import_docs. 설계: docs/web/offline.md

use super::offline::{self, Net};
use super::outbox::{Book, Conflict, Key, Mutation, Persist};
use serde_json::json;
use std::collections::{HashMap, HashSet};

#[derive(Default)]
struct Cloud {
    book: Book,
    head: i64,
    busy: bool,
    again: bool,
    ready: bool,
    loading: bool,
    warned: bool,
    /// 이번 끊김을 알렸나 (다시 이어지면 지움)
    offline_noted: bool,
    /// 알린 저장 멈춤 코드 (같은 것을 자꾸 알리지 않게)
    blocked_noted: String,
    retry_ms: i32,
    retry_armed: bool,
    /// 내가 올린 변경 (동기화 받기에서 다시 오면 건너뜀)
    mine: HashSet<String>,
    idb_warned: bool,
}

thread_local! {
    static CL: std::cell::RefCell<Cloud> = std::cell::RefCell::new(Cloud::default());
    static CPENDING: Cell<bool> = const { Cell::new(false) };
}

fn kinds() -> [&'static str; 2] {
    ["cue", "song"]
}

/// 이 화면(설치판 코어)이 담을 수 있는 문서인가 — 찬양 id 는 영문 · 숫자 4~40자 (songs.rs 규칙).
/// 규칙 밖 id 를 넣으면 코어가 새 id 로 저장해 열 때마다 '새 찬양' 이 생긴다 → 설치판 동기화처럼 건너뛴다 (올리지도 지우지도 않음)
fn storable(kind: &str, id: &str) -> bool {
    kind != "song" || ((4..=40).contains(&id.len()) && id.chars().all(|c| c.is_ascii_alphanumeric()))
}

thread_local! {
    /// 문서 본문 기억: (종류, id) → (바뀐 표시, 본문). 고칠 때마다 큐시트 · 찬양 전부를 다시 꺼내지 않게
    /// (찬양이 수백 곡이면 한 번에 수백 번 읽어 조작 화면이 굼떠졌다 — 2026-11-28 끝에서 끝 시험에서 드러남)
    static EXC: std::cell::RefCell<HashMap<Key, (String, Value)>> = std::cell::RefCell::new(HashMap::new());
}

/// 지금 큐시트 · 찬양 문서들: (종류, id) → 본문. 열린 큐시트는 늘 새로, 나머지는 바뀐 표시(고친 때 · 순서 수 · 이름)가 같으면 기억한 것
fn exported(core: &Shared) -> HashMap<Key, Value> {
    let t0 = js_sys::Date::now();
    let c = core.lock();
    let open = c.cue.as_ref().map(|q| q.id.clone());
    let mut m = HashMap::new();
    let mut seen: Vec<Key> = vec![];
    EXC.with(|e| {
        let mut e = e.borrow_mut();
        for s in c.store.list_cues() {
            if open.as_deref() == Some(s.id.as_str()) {
                continue;
            }
            let k: Key = ("cue".into(), s.id.clone());
            let stamp = format!("{}|{}|{}|{}|{}|{}", s.updated_at, s.count, s.name, s.service_date, s.pinned, s.template);
            let v = match e.get(&k) {
                Some((st, v)) if *st == stamp => v.clone(),
                _ => match c.store.get_cue(&s.id) {
                    Some(q) => {
                        let v = wp_core::web::seed::cue_doc(&q);
                        e.insert(k.clone(), (stamp, v.clone()));
                        v
                    }
                    None => continue,
                },
            };
            seen.push(k.clone());
            m.insert(k, v);
        }
        if let Some(q) = &c.cue {
            m.insert(("cue".into(), q.id.clone()), wp_core::web::seed::cue_doc(q));
        }
        for s in wp_core::songs::list_songs(&c.store.db, "") {
            let Some(id) = s["id"].as_str() else { continue };
            let k: Key = ("song".into(), id.to_string());
            // 바뀐 표시: 목록 줄 전체 (고친 때 · 제목 · 첫 줄 · 저작 · 태그 · 조) — 같은 초 안에 두 번 고쳐도 대개 다르다
            let stamp = s.to_string();
            let v = match e.get(&k) {
                Some((st, v)) if *st == stamp => v.clone(),
                _ => match wp_core::songs::get_song(&c.store.db, id) {
                    Some(v) => {
                        e.insert(k.clone(), (stamp, v.clone()));
                        v
                    }
                    None => continue,
                },
            };
            seen.push(k.clone());
            m.insert(k, v);
        }
        e.retain(|k, _| seen.contains(k));
    });
    let ms = js_sys::Date::now() - t0;
    if ms > 100.0 {
        log::warn!("교회 문서 꺼내기가 {ms:.0}ms 걸림 (큐시트 · 찬양 {}개)", m.len());
    }
    m
}

fn mutation_id() -> String {
    format!("web-{:x}-{:x}", js_sys::Date::now() as u64, (js_sys::Math::random() * 1e12) as u64)
}

// ---------- IndexedDB 열쇠 (교회마다 따로) ----------
fn k_doc(k: &Key) -> String {
    format!("d|{}|{}|{}", super::cloud::church(), k.0, k.1)
}
fn k_mut(n: u64) -> String {
    format!("o|{}|{:016}", super::cloud::church(), n)
}
fn k_meta() -> String {
    format!("m|{}", super::cloud::church())
}

/// 저장할 것을 IndexedDB 에 한 묶음으로 (보내기 전에 끝난다)
async fn save(core: &Shared, ops: Vec<Persist>) {
    if ops.is_empty() {
        return;
    }
    let list: Vec<Value> = ops
        .into_iter()
        .map(|p| match p {
            Persist::PutMut(m) => json!(["put", k_mut(m.n), m.to_json().to_string()]),
            Persist::DelMut(n) => json!(["del", k_mut(n)]),
            Persist::PutDoc(k, rev, p) => json!(["put", k_doc(&k), json!({ "kind": k.0, "id": k.1, "revision": rev, "deleted": p.is_none(), "payload": p }).to_string()]),
            Persist::DelDoc(k) => json!(["del", k_doc(&k)]),
        })
        .collect();
    if let Err(e) = offline::kv_batch(&json!(list)).await {
        log::warn!("오프라인 보관함 쓰기 실패: {e}");
        if !CL.with(|c| std::mem::replace(&mut c.borrow_mut().idb_warned, true)) {
            core.lock().notify(format!("고친 것을 이 브라우저에 보관하지 못했습니다 ({e}). 인터넷이 끊긴 채 새로 고치면 사라질 수 있습니다."), "error");
        }
    }
}

async fn save_head(head: i64) {
    let _ = offline::kv_batch(&json!([["put", k_meta(), json!({ "head": head, "full": true, "at": js_sys::Date::now() }).to_string()]])).await;
}

struct Cache {
    docs: Vec<(Key, i64, Option<Value>)>,
    head: i64,
    queue: Vec<Mutation>,
}

async fn load_cache() -> Option<Cache> {
    if !offline::available() {
        return None;
    }
    let meta: Value = serde_json::from_str(&offline::kv_get(&k_meta()).await?).ok()?;
    if !meta["full"].as_bool().unwrap_or(false) {
        return None;
    }
    let church = super::cloud::church();
    let docs = offline::kv_list(&format!("d|{church}|"))
        .await
        .into_iter()
        .filter_map(|(_, v)| {
            let v: Value = serde_json::from_str(&v).ok()?;
            let k = (v["kind"].as_str()?.to_string(), v["id"].as_str()?.to_string());
            let p = if v["deleted"].as_bool().unwrap_or(false) { None } else { Some(v["payload"].clone()).filter(|p| p.is_object()) };
            Some((k, v["revision"].as_i64().unwrap_or(0), p))
        })
        .collect();
    let queue = offline::kv_list(&format!("o|{church}|")).await.into_iter().filter_map(|(_, v)| Mutation::from_json(&serde_json::from_str(&v).ok()?)).collect();
    Some(Cache { docs, head: meta["head"].as_i64().unwrap_or(0), queue })
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

/// 이 화면 DB 에서 지운다 (다른 곳에서 지움 · 줄에 남은 지우기)
fn remove_local(core: &Shared, k: &Key) {
    let mut c = core.lock();
    if k.0 == "cue" {
        let _ = c.store.db.execute("UPDATE Cuesheets SET deleted_at = ?2 WHERE id = ?1", [k.1.as_str(), wp_core::util::iso_now().as_str()]);
        c.cues = c.store.list_cues();
    } else {
        let _ = wp_core::songs::delete_song(&c.store.db, &k.1);
    }
    c.emit(Ev::Ui);
}

fn open_first(core: &Shared) {
    let mut c = core.lock();
    let want = c.settings.get()["activeCueId"].as_str().unwrap_or("").to_string();
    let pick = if c.cues.iter().any(|x| x.id == want) { Some(want) } else { c.cues.first().map(|x| x.id.clone()) };
    if let Some(id) = pick {
        c.open_cue(&id);
    }
    c.emit(Ev::Ui);
}

async fn list_all(kind: &str) -> Result<Vec<Value>, (u16, Value)> {
    let (mut out, mut cursor) = (vec![], String::new());
    loop {
        let q = if cursor.is_empty() { String::new() } else { format!("&cursor={}", String::from(js_sys::encode_uri_component(&cursor))) };
        let (st, v) = super::cloud::api("GET", &format!("/api/v1/documents?kind={kind}&include=payload&deleted=1&limit=200{q}"), None, &[]).await;
        if st != 200 {
            return Err((st, v));
        }
        out.extend(v["documents"].as_array().cloned().unwrap_or_default());
        match v["next"].as_str() {
            Some(n) if !n.is_empty() => cursor = n.to_string(),
            _ => return Ok(out),
        }
    }
}

/// 답을 보고: 닿음 · 닿지 않음 · 저장소 멈춤 · 그 밖의 오류
enum Reach {
    Ok,
    Offline,
    Blocked(String, String),
    Fail(String),
}

fn reach(st: u16, v: &Value) -> Reach {
    let code = v["code"].as_str().unwrap_or("");
    if st == 0 || (code.is_empty() && matches!(st, 502..=504 | 520..=530)) {
        return Reach::Offline;
    }
    if (200..300).contains(&st) {
        return Reach::Ok;
    }
    if code.starts_with("drive_") || code.starts_with("tier_") {
        return Reach::Blocked(code.to_string(), save_error(st, v));
    }
    Reach::Fail(format!("{st}: {}", save_error(st, v)))
}

/// 저장 오류 글 (교회 등급 · 구글 드라이브 오류 코드는 docs/web/tiers.md 5절)
fn save_error(st: u16, v: &Value) -> String {
    let e = v["error"].as_str().unwrap_or("인터넷 연결을 확인해 주세요").to_string();
    match v["code"].as_str().unwrap_or("") {
        "drive_disconnected" => "교회 구글 드라이브 연결이 끊겼습니다. 주인이 주인 화면에서 다시 연결해 주세요.".into(),
        "drive_quota" => "교회 구글 드라이브 공간이 가득 찼습니다.".into(),
        "drive_rate" | "tier_limit" if st == 429 => "요청이 많아 잠시 쉬었다가 다시 저장합니다.".into(),
        "drive_busy" => "다른 곳에서 저장하는 중이라 잠시 뒤 다시 저장합니다.".into(),
        "tier_moving" => "교회 자료를 옮기는 중이라 지금은 읽기만 됩니다.".into(),
        "tier_blocked" => format!("이 교회 등급에서는 할 수 없는 일입니다: {e}"),
        _ => e,
    }
}

fn set_online() {
    if offline::net() != Net::Online {
        offline::check(); // 끊긴 사이 새 판이 나왔을 수 있다 (알리기만)
    }
    CL.with(|c| {
        let mut c = c.borrow_mut();
        c.retry_ms = 0;
        c.offline_noted = false;
        c.blocked_noted.clear();
    });
    offline::set_net(Net::Online);
}

fn go_offline(core: &Shared) {
    offline::set_net(Net::Offline);
    if !CL.with(|c| std::mem::replace(&mut c.borrow_mut().offline_noted, true)) {
        let mut g = core.lock();
        g.notify("인터넷 연결이 끊겼습니다. 고친 것은 이 브라우저에 모아 두었다가 다시 이어지면 올립니다. 같은 브라우저의 출력 창은 계속 받습니다. 휴대폰 카메라는 새로 연결할 수 없으니 설치판의 내부망 휴대폰 카메라를 쓰세요.", "info");
        g.emit(Ev::Ui);
    }
    schedule_retry(core, None);
}

fn go_blocked(core: &Shared, code: String, msg: String) {
    offline::set_net(Net::Blocked { code: code.clone(), msg: msg.clone() });
    if CL.with(|c| std::mem::replace(&mut c.borrow_mut().blocked_noted, code.clone())) != code {
        core.lock().notify(format!("교회 클라우드에 저장하지 못했습니다: {msg} (고친 것은 이 브라우저에 남아 있고, 나중에 다시 올립니다)"), "error");
    }
    schedule_retry(core, Some(if matches!(code.as_str(), "drive_rate" | "drive_busy" | "tier_limit") { 15_000 } else { 30_000 }));
}

/// 다시 해 보기 (끊김: 3 → 6 → 12 → 15초, 저장 멈춤: 정한 시간). 이미 걸려 있으면 그대로
fn schedule_retry(core: &Shared, fixed: Option<i32>) {
    let d = CL.with(|c| {
        let mut c = c.borrow_mut();
        if c.retry_armed {
            return None;
        }
        c.retry_armed = true;
        c.retry_ms = if c.retry_ms == 0 { 3000 } else { (c.retry_ms * 2).min(15_000) };
        Some(fixed.unwrap_or(c.retry_ms))
    });
    if let Some(d) = d {
        let c = core.clone();
        wp_core::web::defer(d + (js_sys::Math::random() * 400.0) as i32, move || {
            CL.with(|x| x.borrow_mut().retry_armed = false);
            wasm_bindgen_futures::spawn_local(sync_now(c));
        });
    }
}

/// 이으면 할 일: (처음이면 받기) → 올리기 → 받기
async fn sync_now(core: Shared) {
    if !CL.with(|c| c.borrow().ready) {
        initial_load(&core).await;
        if !CL.with(|c| c.borrow().ready) {
            return;
        }
    }
    push(core.clone()).await;
    if offline::net() != Net::Offline {
        pull(core).await;
    }
}

/// IndexedDB 에 남겨 둔 것으로 먼저 연다 (인터넷 없이도 마지막 자료 + 못 올린 고친 것)
fn open_from_cache(core: &Shared, cache: Cache) {
    let mut docs = json!({ "cue": [], "song": [] });
    for (k, _, p) in &cache.docs {
        if !storable(&k.0, &k.1) {
            continue;
        }
        if let (Some(p), Some(a)) = (p, docs[k.0.as_str()].as_array_mut()) {
            a.push(p.clone());
        }
    }
    {
        let mut c = core.lock();
        if let Err(e) = wp_core::web::seed::import_docs(&mut c, &docs) {
            c.notify(format!("보관해 둔 교회 자료를 넣지 못했습니다: {e:#}"), "error");
        }
    }
    let now = exported(core);
    CL.with(|cl| {
        let mut cl = cl.borrow_mut();
        for (k, rev, p) in &cache.docs {
            cl.book.rev.insert(k.clone(), *rev);
            if p.is_some() {
                if let Some(x) = now.get(k) {
                    cl.book.base.insert(k.clone(), x.to_string());
                }
            }
        }
        cl.book.set_queue(cache.queue.clone());
        cl.head = cache.head;
        cl.ready = true;
    });
    // 못 올린 고친 것을 위에 (문서마다 마지막 것)
    let mut last: HashMap<Key, Option<Value>> = HashMap::new();
    for m in &cache.queue {
        last.insert(m.key(), m.payload.clone());
    }
    for (k, p) in &last {
        match p {
            Some(p) => {
                let mut c = core.lock();
                let _ = wp_core::web::seed::import_docs(&mut c, &json!({ k.0.as_str(): [p] }));
            }
            None => remove_local(core, k),
        }
    }
    open_first(core);
    offline::set_pending(cache.queue.len());
    offline::set_loaded();
    log::info!("보관해 둔 교회 자료: 문서 {} · 올릴 것 {} · 차례 {}", cache.docs.len(), cache.queue.len(), cache.head);
}

/// 처음 받기 (이 브라우저에 보관한 것이 없을 때 · reset) — head 를 먼저 받아 목록 사이에 바뀐 것을 놓치지 않는다
async fn initial_load(core: &Shared) {
    if CL.with(|c| std::mem::replace(&mut c.borrow_mut().loading, true)) {
        return;
    }
    let r = initial_load_inner(core).await;
    CL.with(|c| c.borrow_mut().loading = false);
    if let Err((st, v)) = r {
        match reach(st, &v) {
            Reach::Offline => go_offline(core),
            Reach::Blocked(code, msg) => go_blocked(core, code, msg),
            _ => {
                core.lock().notify(format!("교회 자료를 받지 못했습니다: {}", save_error(st, &v)), "error");
                schedule_retry(core, Some(30_000));
            }
        }
    }
}

async fn initial_load_inner(core: &Shared) -> Result<(), (u16, Value)> {
    let (st, h) = super::cloud::api("GET", "/api/v1/sync?after=0&limit=1", None, &[]).await;
    if st != 200 {
        return Err((st, h));
    }
    let head = h["head"].as_i64().unwrap_or(0);
    let mut docs = json!({ "cue": [], "song": [] });
    let mut got: Vec<(Key, i64, Option<Value>)> = vec![];
    for k in kinds() {
        for d in list_all(k).await? {
            let key = (k.to_string(), d["id"].as_str().unwrap_or("").to_string());
            if key.1.is_empty() || !storable(k, &key.1) || CL.with(|c| c.borrow().book.pending(&key)) {
                continue;
            }
            let rev = d["revision"].as_i64().unwrap_or(0);
            if d["deleted"].as_bool().unwrap_or(false) || !d["payload"].is_object() {
                got.push((key, rev, None));
                continue;
            }
            docs[k].as_array_mut().unwrap().push(d["payload"].clone());
            got.push((key, rev, Some(d["payload"].clone())));
        }
    }
    {
        let mut c = core.lock();
        if let Err(e) = wp_core::web::seed::import_docs(&mut c, &docs) {
            c.notify(format!("교회 자료를 넣지 못했습니다: {e:#}"), "error");
        }
    }
    let now = exported(core);
    let mut ops = vec![];
    CL.with(|cl| {
        let mut cl = cl.borrow_mut();
        for (k, rev, p) in &got {
            cl.book.rev.insert(k.clone(), *rev);
            match p.as_ref().and_then(|_| now.get(k)) {
                Some(x) => {
                    cl.book.base.insert(k.clone(), x.to_string());
                }
                None => {
                    cl.book.base.remove(k);
                }
            }
            ops.push(Persist::PutDoc(k.clone(), *rev, p.clone()));
        }
        cl.head = head;
        cl.ready = true;
    });
    save(core, ops).await;
    save_head(head).await;
    open_first(core);
    set_online();
    offline::set_loaded();
    prefetch_images(core);
    log::info!("교회 자료: 문서 {} · 차례 {head}", got.len());
    Ok(())
}

/// 큐시트 · 찬양 안의 교회 그림 주소를 미리 받아 둔다 (끊긴 채 새로 고쳐도 보이게). 영상은 담지 않는다
fn prefetch_images(core: &Shared) {
    let s = wp_core::web::seed::export_docs(&core.lock()).to_string();
    let mut urls: Vec<String> = vec![];
    let mut rest = s.as_str();
    while let Some(i) = rest.find("/api/v1/assets/") {
        let tail = &rest[i..];
        let end = tail.find(|ch: char| ch == '"' || ch == '\\' || ch == ')' || ch.is_whitespace()).unwrap_or(tail.len());
        let u = &tail[..end];
        let low = u.to_ascii_lowercase();
        let image = [".png", ".jpg", ".jpeg", ".gif", ".webp", ".svg", ".bmp"].iter().any(|e| low.contains(&format!("e={e}")) || low.contains(&format!("e=%2e{}", &e[1..])));
        if image && !urls.iter().any(|x| x == u) {
            urls.push(u.to_string());
        }
        rest = &tail[end.max(1)..];
        if urls.len() >= 60 {
            break;
        }
    }
    if !urls.is_empty() {
        offline::prefetch(&urls);
    }
}

/// 이 탭이 주인이 된 뒤 (탭 잠금) — 미뤄 둔 올리기 · 받기
pub fn kick(core: Shared) {
    wasm_bindgen_futures::spawn_local(async move {
        record(&core).await;
        sync_now(core).await;
    });
}

/// 로그인한 조작 화면: (보관한 것으로 먼저 열기) → 받기 · 올리기 → 고칠 때마다 줄에 → 20초마다 받기
pub fn cloud_start(core: Shared) {
    wasm_bindgen_futures::spawn_local(async move {
        if let Some(cache) = load_cache().await {
            open_from_cache(&core, cache);
        }
        watch(core.clone());
        sync_now(core).await;
    });
}

fn watch(core: Shared) {
    // 고칠 때마다 (1초 모아서): 줄에 쓰고 → 이어져 있으면 올린다
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
                    wp_core::web::defer(1000, move || {
                        CPENDING.with(|p| p.set(false));
                        wasm_bindgen_futures::spawn_local(async move {
                            record(&c3).await;
                            if offline::net() == Net::Online {
                                push(c3).await;
                            }
                        });
                    });
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
    });
    // 떠나거나 숨길 때 바로 줄에 (1초를 기다리지 않게 — 새로 고쳐도 남도록)
    for ev in ["pagehide", "visibilitychange"] {
        let c = core.clone();
        let f = Closure::<dyn FnMut()>::new(move || {
            let c = c.clone();
            wasm_bindgen_futures::spawn_local(async move { record(&c).await });
        });
        if let Some(w) = web_sys::window() {
            let _ = w.add_event_listener_with_callback(ev, f.as_ref().unchecked_ref());
        }
        f.forget();
    }
    // 20초마다 받기 (숨긴 탭 · 끊긴 동안은 묻지 않는다 — 끊긴 동안은 다시 해 보기가 따로 돈다)
    let c4 = core.clone();
    let tick = Closure::<dyn FnMut()>::new(move || {
        let hidden = web_sys::window().and_then(|w| w.document()).is_some_and(|d| d.hidden());
        if !hidden && offline::net() == Net::Online {
            wasm_bindgen_futures::spawn_local(pull(c4.clone()));
        }
    });
    if let Some(w) = web_sys::window() {
        let _ = w.set_interval_with_callback_and_timeout_and_arguments_0(tick.as_ref().unchecked_ref(), 20_000);
    }
    tick.forget();
    // 브라우저가 '다시 이어짐' 을 알리면 바로
    let c5 = core.clone();
    offline::start(move || {
        if offline::net() != Net::Online {
            wasm_bindgen_futures::spawn_local(sync_now(c5.clone()));
        }
    });
}

/// 이 화면에서 바뀐 문서를 줄에 쓴다 (보내기 전에 IndexedDB 에)
async fn record(core: &Shared) {
    if !CL.with(|c| c.borrow().ready) || !offline::is_primary() {
        return; // 보기 전용 탭(같은 브라우저의 다른 탭이 주인)은 올릴 줄을 건드리지 않는다
    }
    let now = exported(core);
    if !super::cloud::can("write") {
        // 보기만 권한: 화면에서는 고쳐지지만 클라우드에는 남지 않는다 — 한 번만 알림
        let changed = CL.with(|c| now.iter().any(|(k, v)| c.borrow().book.base.get(k) != Some(&v.to_string())));
        if changed && !CL.with(|c| std::mem::replace(&mut c.borrow_mut().warned, true)) {
            core.lock().notify("보기 권한이라 고친 것이 교회 클라우드에 저장되지 않습니다.", "error");
        }
        return;
    }
    let ops = CL.with(|c| c.borrow_mut().book.diff(&now, mutation_id, js_sys::Date::now()));
    save(core, ops).await;
    // 올리는 중(push)이면 그쪽이 끝에 센다 — 충돌을 푸는 사이 잠깐 0 으로 보이지 않게
    if !CL.with(|c| c.borrow().busy) {
        offline::set_pending(CL.with(|c| c.borrow().book.len()));
    }
}

/// 줄에 있는 것을 20개씩 올린다
async fn push(core: Shared) {
    if !CL.with(|c| c.borrow().ready) || !super::cloud::can("write") || !offline::is_primary() {
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
        let (batch, ops) = CL.with(|c| c.borrow_mut().book.take_batch(20));
        if batch.is_empty() {
            break;
        }
        save(&core, ops).await; // 보낸 횟수를 먼저 남긴다 (답을 잃어도 같은 mutationId 로)
        let body = json!({ "mutations": batch.iter().map(|m| m.wire()).collect::<Vec<_>>() });
        let (st, v) = super::cloud::api("POST", "/api/v1/sync", Some(&body), &[]).await;
        match reach(st, &v) {
            Reach::Ok => {
                set_online();
                let o = CL.with(|c| c.borrow_mut().book.apply_results(&batch, &v["results"]));
                CL.with(|c| c.borrow_mut().mine.extend(o.applied.iter().cloned()));
                save(&core, o.persist).await;
                let had_conflict = !o.conflicts.is_empty();
                for c in o.conflicts {
                    resolve(&core, c).await;
                }
                for (k, e) in o.invalid {
                    core.lock().notify(format!("교회 클라우드가 '{}' 를 받지 않았습니다: {e}", k.1), "error");
                }
                if had_conflict {
                    record(&core).await; // 사본 · 되살리기를 줄에
                }
                if v["results"].as_array().is_none_or(|a| a.is_empty()) {
                    break; // 답이 이상하면 그만 (다음에 같은 mutationId 로 다시)
                }
            }
            Reach::Offline => {
                go_offline(&core);
                break;
            }
            Reach::Blocked(code, msg) => {
                go_blocked(&core, code, msg);
                break;
            }
            Reach::Fail(e) => {
                if !CL.with(|c| std::mem::replace(&mut c.borrow_mut().warned, true)) {
                    core.lock().notify(format!("교회 클라우드에 저장하지 못했습니다 ({e}). 고친 것은 이 브라우저에 남아 있습니다."), "error");
                }
                schedule_retry(&core, Some(30_000));
                break;
            }
        }
    }
    let again = CL.with(|c| {
        let mut c = c.borrow_mut();
        c.busy = false;
        std::mem::replace(&mut c.again, false)
    });
    offline::set_pending(CL.with(|c| c.borrow().book.len()));
    if again && offline::net() == Net::Online {
        wasm_bindgen_futures::spawn_local(push(core));
    }
}

fn hhmm() -> String {
    let d = js_sys::Date::new_0();
    format!("{:02}:{:02}", d.get_hours(), d.get_minutes())
}

/// 올리다 판이 달랐던 문서 — 설치판 cloudsync 와 같은 규칙 (docs/web/sync.md 4절)
async fn resolve(core: &Shared, c: Conflict) {
    let k = c.key.clone();
    let cur = &c.current;
    let cur_alive = cur["payload"].is_object() && !cur["deleted"].as_bool().unwrap_or(false);
    let rev = cur["revision"].as_i64();
    let name_key = if k.0 == "cue" { "name" } else { "title" };
    match (&c.mine, cur_alive) {
        (Some(mine), true) => {
            // 원래 자리 = 클라우드 판
            import_one(core, &k.0, &cur["payload"]);
            let cur_s = exported(core).get(&k).map(|v| v.to_string());
            CL.with(|cl| cl.borrow_mut().book.accept(&k, rev.unwrap_or(0), cur_s.clone()));
            save(core, vec![Persist::PutDoc(k.clone(), rev.unwrap_or(0), Some(cur["payload"].clone()))]).await;
            if cur_s.as_deref() == Some(mine.to_string().as_str()) {
                return; // 같은 내용 — 판만 맞춤
            }
            // 이 화면의 것 = 새 id 의 '○○ (충돌 사본 HH:MM)' (다음 줄 쓰기에서 새 문서로 올라감)
            let mut copy = mine.clone();
            let new_id = wp_core::util::short_id(if k.0 == "cue" { "c" } else { "s" });
            copy["id"] = json!(new_id);
            let name = mine[name_key].as_str().unwrap_or("").to_string();
            let suffix = if wp_core::i18n::is_en() { format!(" (conflict copy {})", hhmm()) } else { format!(" (충돌 사본 {})", hhmm()) };
            copy[name_key] = json!(format!("{name}{suffix}"));
            import_one(core, &k.0, &copy);
            core.lock().notify(format!("다른 곳에서 먼저 고친 '{name}' — 원래 자리에 최신 것을 불러왔고, 이 화면에서 고친 것은 '{name}{suffix}' 으로 남겼습니다. 둘 중 고르세요."), "error");
        }
        (Some(mine), false) => {
            // 다른 곳에서 지웠는데 이 화면에서 고침 → 지우지 않고 되살린다 (묘비 판으로 다시 올림). 클라우드에 아예 없으면 새로 만든다
            CL.with(|cl| {
                let mut cl = cl.borrow_mut();
                match rev {
                    Some(r) => cl.book.accept(&k, r, None),
                    None => cl.book.forget(&k),
                }
            });
            let name = mine[name_key].as_str().unwrap_or("").to_string();
            core.lock().notify(format!("다른 곳에서 지운 '{name}' 을 이 화면에서 고치고 있어 지우지 않고 되살립니다."), "info");
        }
        (None, true) => {
            // 이 화면에서 지웠는데 다른 곳에서 고침 → 클라우드 판으로 되살림
            import_one(core, &k.0, &cur["payload"]);
            let text = exported(core).get(&k).map(|v| v.to_string());
            CL.with(|cl| cl.borrow_mut().book.accept(&k, rev.unwrap_or(0), text));
            save(core, vec![Persist::PutDoc(k.clone(), rev.unwrap_or(0), Some(cur["payload"].clone()))]).await;
            core.lock().notify("다른 곳에서 고친 문서라 지우지 않고 최신 것으로 되살렸습니다.", "error");
        }
        (None, false) => {
            // 양쪽 다 지움
            CL.with(|cl| {
                let mut cl = cl.borrow_mut();
                match rev {
                    Some(r) => cl.book.accept(&k, r, None),
                    None => cl.book.forget(&k),
                }
            });
            save(core, vec![Persist::DelDoc(k.clone())]).await;
        }
    }
}

/// 다른 곳에서 고친 것을 들인다 (`more` 끝까지). 이 화면 줄에 있는 문서는 들이지 않는다 — 올릴 때 충돌 규칙을 따른다
async fn pull(core: Shared) {
    if !CL.with(|c| c.borrow().ready && !c.borrow().busy) {
        return;
    }
    record(&core).await; // 고치다 만 것(1초 모으기 전)을 먼저 줄에
    loop {
        let after = CL.with(|c| c.borrow().head);
        let (st, v) = super::cloud::api("GET", &format!("/api/v1/sync?after={after}"), None, &[]).await;
        match reach(st, &v) {
            Reach::Ok => {}
            Reach::Offline => return go_offline(&core),
            Reach::Blocked(code, msg) => return go_blocked(&core, code, msg),
            Reach::Fail(_) => return,
        }
        if v["reset"].as_bool().unwrap_or(false) {
            // 변경 기록이 잘렸다 → 목록부터 다시 (줄에 있는 것은 그대로 두고 나중에 충돌 규칙으로)
            CL.with(|c| c.borrow_mut().ready = false);
            initial_load(&core).await;
            return;
        }
        let mut ops = vec![];
        let mut imported: Vec<(Key, i64)> = vec![];
        for ch in v["changes"].as_array().into_iter().flatten() {
            if ch["superseded"].as_bool().unwrap_or(false) {
                continue;
            }
            let kind = ch["kind"].as_str().unwrap_or("");
            if !kinds().contains(&kind) || !storable(kind, ch["id"].as_str().unwrap_or("")) {
                continue;
            }
            let k: Key = (kind.to_string(), ch["id"].as_str().unwrap_or("").to_string());
            if ch["mutationId"].as_str().is_some_and(|m| CL.with(|c| c.borrow().mine.contains(m))) {
                continue;
            }
            if CL.with(|c| c.borrow().book.pending(&k)) {
                continue;
            }
            let rev = ch["revision"].as_i64().unwrap_or(0);
            if ch["op"] == "delete" {
                remove_local(&core, &k);
                CL.with(|c| c.borrow_mut().book.accept(&k, rev, None));
                ops.push(Persist::PutDoc(k, rev, None));
            } else if ch["payload"].is_object() {
                import_one(&core, kind, &ch["payload"]);
                ops.push(Persist::PutDoc(k.clone(), rev, Some(ch["payload"].clone())));
                imported.push((k, rev));
            }
        }
        // 들인 문서들의 '맞춘 본문' 은 다 들인 뒤 한 번만 꺼낸다 (바뀐 것마다 전부 꺼내면 화면이 오래 멈췄다 — web-scenes 8d761e2)
        if !imported.is_empty() {
            let after = exported(&core);
            CL.with(|c| {
                let mut c = c.borrow_mut();
                for (k, rev) in &imported {
                    c.book.accept(k, *rev, after.get(k).map(|v| v.to_string()));
                }
            });
        }
        let head = v["next"].as_i64().unwrap_or(after);
        CL.with(|c| c.borrow_mut().head = head);
        save(&core, ops).await;
        save_head(head).await;
        if !v["more"].as_bool().unwrap_or(false) {
            break;
        }
    }
    // 받기가 되었다 = 닿는다 (저장소 멈춤이었으면 줄이 빌 때 풀린다 — 올리기가 정함)
    let queued = CL.with(|c| c.borrow().book.len());
    if offline::net() == Net::Offline || (queued == 0 && offline::net() != Net::Online) {
        set_online();
        if queued > 0 {
            wasm_bindgen_futures::spawn_local(push(core));
        }
    }
}

/// 점검용 (?debug)
pub fn debug() -> Value {
    CL.with(|c| {
        let c = c.borrow();
        json!({ "ready": c.ready, "head": c.head, "queue": c.book.queue.iter().map(|m| json!([m.kind, m.id, m.op, m.base_revision, m.tries])).collect::<Vec<_>>() })
    })
}
