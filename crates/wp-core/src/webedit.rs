//! 웹 편집기: 다른 컴퓨터(또는 이 PC)의 브라우저에서 큐시트를 고친다 (예배송출의 'PIN 으로 여는 웹 편집기' 를 참고해 새로 씀).
//! - 설정 `webEditor { enabled(기본 꺼짐), port 37545, pin }`. 연결 → 웹 편집기.
//! - PIN 을 정하지 않으면 127.0.0.1 에만 열고 PIN 을 묻지 않는다 (이 PC 의 브라우저에서만).
//!   PIN 을 정하면 0.0.0.0 에 열고 모든 요청에 PIN — 5번 틀리면 30초 막음(휴대폰 리모컨과 같은 규칙).
//! - 쓰기 요청은 X-WorshipCue 머리글이 있어야 한다 (다른 웹사이트가 몰래 보내지 못하게).
//! - 동시 편집: 불러올 때의 updatedAt 을 함께 보내고, 그 사이 조작 화면이나 다른 브라우저가 고쳤으면 저장하지 않고 '다시 불러오기' 를 부탁한다.
//! - 고칠 수 있는 것: 큐시트 이름·날짜·메모, 순서 차례·지우기·더하기(제목·찬양·성경·공지·빈 화면), 제목·부제·가사·장절·공지 글·진행 메모.
//!   그 밖의 칸(템플릿·꾸밈·PPT 등)은 그대로 둔다.

use crate::core::{Core, Ev, Shared};
use axum::body::Bytes;
use axum::extract::{ConnectInfo, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::SocketAddr;

pub const DEFAULT_PORT: u16 = 37545;
/// 웹 편집기에서 새로 넣을 수 있는 순서 종류
pub const NEW_TYPES: [&str; 5] = ["title", "song", "bible", "text", "blank"];
/// 웹 편집기가 고치는 칸 (나머지는 원래 값을 지킨다)
const EDIT_KEYS: [&str; 7] = ["title", "subtitle", "lyrics", "ref", "body", "note", "order"];

struct Run {
    port: u16,
    host: String,
    stop: tokio::sync::oneshot::Sender<()>,
}
static RUN: Lazy<Mutex<Option<Run>>> = Lazy::new(|| Mutex::new(None));
static ERROR: Lazy<Mutex<String>> = Lazy::new(|| Mutex::new(String::new()));

fn conf(core: &Core) -> (bool, u16, String) {
    let w = &core.settings.get()["webEditor"];
    (w["enabled"].as_bool().unwrap_or(false), w["port"].as_u64().filter(|p| *p > 0 && *p < 65536).unwrap_or(DEFAULT_PORT as u64) as u16, w["pin"].as_str().unwrap_or("").trim().to_string())
}

/// PIN 을 정했으면 내부망(0.0.0.0), 아니면 이 PC 안(127.0.0.1)
pub fn host_for(pin: &str) -> &'static str {
    if pin.is_empty() { "127.0.0.1" } else { "0.0.0.0" }
}

pub fn status(core: &Core) -> Value {
    let (enabled, port, pin) = conf(core);
    let run = RUN.lock();
    let port = run.as_ref().map(|r| r.port).unwrap_or(port);
    let lan = run.as_ref().is_some_and(|r| r.host == "0.0.0.0");
    let mut urls = vec![format!("http://127.0.0.1:{port}/")];
    if lan {
        urls.extend(crate::remote::lan_addresses().iter().map(|a| format!("http://{a}:{port}/")));
    }
    json!({ "enabled": enabled, "pin": pin, "port": port, "running": run.is_some(), "lan": lan, "error": ERROR.lock().clone(), "urls": if run.is_some() { urls } else { vec![] } })
}

/// 들어온 요청이 열려도 되는가. PIN 이 없으면 이 PC 에서 온 것만
pub fn allow(ip: &std::net::IpAddr, pin_given: &str, want: &str) -> Result<(), (StatusCode, &'static str)> {
    if want.is_empty() {
        return if ip.is_loopback() { Ok(()) } else { Err((StatusCode::FORBIDDEN, "PIN 을 정해야 다른 컴퓨터에서 열 수 있습니다.")) };
    }
    crate::remote::check_pin(&format!("edit:{ip}"), pin_given, want)
}

fn js(code: StatusCode, v: Value) -> Response {
    (code, [(header::CACHE_CONTROL, "no-store")], Json(v)).into_response()
}
fn fail(code: StatusCode, e: impl Into<String>) -> Response {
    js(code, json!({ "ok": false, "error": crate::i18n::tr(&e.into()) }))
}

/// 큐시트 목록 (휴지통 빼고)
pub fn list(core: &Core) -> Value {
    let active = core.cue.as_ref().map(|c| c.id.clone()).unwrap_or_default();
    json!({ "cues": core.store.list_cues(), "active": active })
}

/// 큐시트 하나 + 고칠 때 맞춰 볼 updatedAt
pub fn cue_of(core: &Core, id: &str) -> Option<Value> {
    let c = core.store.get_cue(id)?;
    let up = core.store.list_cues().into_iter().find(|s| s.id == id).map(|s| s.updated_at).unwrap_or_default();
    Some(json!({ "id": c.id, "name": c.name, "serviceDate": c.service_date, "startTime": c.start_time, "memo": c.memo, "items": c.items, "updatedAt": up }))
}

/// 저장: base(불러올 때의 updatedAt)가 지금과 다르면 Err("conflict"). 순서는 id 로 원래 것과 맞춰 고친 칸만 바꾼다.
pub fn save(core: &mut Core, v: &Value) -> Result<Value, String> {
    let id = v["id"].as_str().unwrap_or("");
    let Some(old) = core.store.get_cue(id) else { return Err("큐시트를 찾을 수 없습니다.".into()) };
    let now = core.store.list_cues().into_iter().find(|s| s.id == id).map(|s| s.updated_at).unwrap_or_default();
    if v["base"].as_str().unwrap_or("") != now {
        return Err("conflict".into());
    }
    let given = v["items"].as_array().ok_or("큐시트 형식이 올바르지 않습니다.")?;
    if given.len() > 500 {
        return Err("순서가 너무 많습니다.".into());
    }
    let by_id: HashMap<String, &Value> = old.items.iter().filter_map(|it| it["id"].as_str().map(|i| (i.to_string(), it))).collect();
    let mut items = vec![];
    for g in given {
        let gid = g["id"].as_str().unwrap_or("");
        let mut it = match by_id.get(gid) {
            Some(o) => (*o).clone(),
            None => {
                let ty = g["type"].as_str().unwrap_or("");
                if !NEW_TYPES.contains(&ty) {
                    return Err("넣을 수 없는 순서 종류입니다.".into());
                }
                let mut n = crate::core::new_item(ty, "");
                if ty == "blank" {
                    n["mode"] = json!("background");
                }
                // 찬양 라이브러리에서 더한 곡: 라이브러리와 잇는다 (가사를 라이브러리에서 고치면 따라감)
                if ty == "song" {
                    for k in ["songId", "credit"] {
                        if let Some(s) = g[k].as_str() {
                            n[k] = json!(s.chars().take(300).collect::<String>());
                        }
                    }
                }
                n
            }
        };
        for k in EDIT_KEYS {
            if let Some(s) = g[k].as_str() {
                // 원래 없던 칸은 그 종류에 맞는 것만 (예: 제목 순서에 가사 칸을 만들지 않음)
                if it.get(k).is_some() || k == "title" || k == "note" {
                    it[k] = json!(s.chars().take(if k == "lyrics" || k == "body" { 20000 } else { 300 }).collect::<String>());
                }
            }
        }
        items.push(it);
    }
    let mut cue = json!({ "id": id, "name": v["name"].as_str().unwrap_or(&old.name), "serviceDate": v["serviceDate"].as_str().unwrap_or(&old.service_date), "memo": v["memo"].as_str().unwrap_or(&old.memo), "items": items });
    if cue["serviceDate"].as_str().is_some_and(|d| !d.is_empty() && !(d.len() == 10 && d.as_bytes()[4] == b'-' && d.as_bytes()[7] == b'-')) {
        cue["serviceDate"] = json!(old.service_date);
    }
    crate::extcmd::save_cue_json(core, &cue)?;
    log::info!("웹 편집기: 큐시트 저장 {id} (순서 {}개)", items_len(&cue));
    cue_of(core, id).ok_or_else(|| "저장한 큐시트를 다시 읽지 못했습니다.".into())
}

fn items_len(c: &Value) -> usize {
    c["items"].as_array().map(|a| a.len()).unwrap_or(0)
}

/// 새 큐시트 (빈 것, 조작 화면의 열린 큐시트는 바꾸지 않는다)
pub fn create(core: &mut Core, name: &str) -> Result<Value, String> {
    let name = name.trim();
    let name = if name.is_empty() { crate::i18n::tr("새 큐시트") } else { name.to_string() };
    let r = crate::extcmd::save_cue_json(core, &json!({ "name": name, "items": [] }))?;
    cue_of(core, r["id"].as_str().unwrap_or("")).ok_or_else(|| "만든 큐시트를 읽지 못했습니다.".into())
}

// ---------- 서버 ----------
async fn index(State(core): State<Shared>) -> Response {
    let w = core.lock().web_dir.clone();
    match tokio::fs::read(w.join("editor/index.html")).await {
        Ok(b) => ([(header::CONTENT_TYPE, "text/html; charset=utf-8"), (header::CACHE_CONTROL, "no-cache")], b).into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

fn pin_of(h: &HeaderMap, q: &HashMap<String, String>) -> String {
    h.get("x-pin").and_then(|v| v.to_str().ok()).map(String::from).or_else(|| q.get("pin").cloned()).unwrap_or_default()
}

async fn api_get(State(core): State<Shared>, ConnectInfo(addr): ConnectInfo<SocketAddr>, uri: axum::http::Uri, h: HeaderMap, Query(q): Query<HashMap<String, String>>) -> Response {
    let c = core.lock();
    let (_, _, want) = conf(&c);
    if let Err((code, e)) = allow(&addr.ip(), &pin_of(&h, &q), &want) {
        return fail(code, e);
    }
    let r = match uri.path() {
        "/e/state" => Some(list(&c)),
        "/e/cue" => cue_of(&c, q.get("id").map(|s| s.as_str()).unwrap_or("")).map(|cue| json!({ "cue": cue })),
        "/e/songs" => {
            let list: Vec<Value> = c.list_songs(q.get("q").map(|s| s.as_str()).unwrap_or("")).into_iter().take(40).map(|s| json!({ "id": s["id"], "title": s["title"], "credit": s["credit"], "key": s["key"] })).collect();
            Some(json!({ "songs": list }))
        }
        _ => None,
    };
    match r {
        Some(mut v) => {
            v["ok"] = json!(true);
            js(StatusCode::OK, v)
        }
        None => fail(StatusCode::NOT_FOUND, "큐시트를 찾을 수 없습니다."),
    }
}

async fn api_post(State(core): State<Shared>, ConnectInfo(addr): ConnectInfo<SocketAddr>, uri: axum::http::Uri, h: HeaderMap, body: Bytes) -> Response {
    if h.get("x-worshipcue").and_then(|v| v.to_str().ok()) != Some("1") {
        return StatusCode::FORBIDDEN.into_response();
    }
    if body.len() > 2_000_000 {
        return fail(StatusCode::PAYLOAD_TOO_LARGE, "너무 큽니다.");
    }
    let Ok(a) = serde_json::from_slice::<Value>(&body) else { return fail(StatusCode::BAD_REQUEST, "잘못된 요청") };
    let mut c = core.lock();
    let (_, _, want) = conf(&c);
    if let Err((code, e)) = allow(&addr.ip(), &pin_of(&h, &HashMap::new()), &want) {
        return fail(code, e);
    }
    let r = match uri.path() {
        "/e/save" => save(&mut c, &a).map(|cue| json!({ "cue": cue })),
        "/e/new" => create(&mut c, a["name"].as_str().unwrap_or("")).map(|cue| json!({ "cue": cue })),
        // 찬양 라이브러리의 곡 → 넣을 순서 (가사·부를 순서 포함)
        "/e/song" => c.song_item(a["id"].as_str().unwrap_or("")).map(|it| json!({ "item": it })).ok_or_else(|| "곡을 찾을 수 없습니다.".to_string()),
        _ => Err("알 수 없는 요청".into()),
    };
    match r {
        Ok(mut v) => {
            v["ok"] = json!(true);
            js(StatusCode::OK, v)
        }
        Err(e) if e == "conflict" => js(StatusCode::CONFLICT, json!({ "ok": false, "conflict": true, "error": crate::i18n::tr("그 사이 다른 곳(조작 화면이나 다른 브라우저)에서 이 큐시트를 고쳤습니다. 다시 불러온 뒤 고쳐 주세요.") })),
        Err(e) => fail(StatusCode::OK, e),
    }
}

pub fn router(core: Shared) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/index.html", get(index))
        .route("/e/state", get(api_get))
        .route("/e/cue", get(api_get))
        .route("/e/songs", get(api_get))
        .route("/e/save", post(api_post))
        .route("/e/new", post(api_post))
        .route("/e/song", post(api_post))
        .with_state(core)
}

async fn start(core: Shared, port: u16, host: &str) {
    match tokio::net::TcpListener::bind((host, port)).await {
        Ok(l) => {
            let (tx, rx) = tokio::sync::oneshot::channel();
            *RUN.lock() = Some(Run { port, host: host.to_string(), stop: tx });
            ERROR.lock().clear();
            let app = router(core.clone()).into_make_service_with_connect_info::<SocketAddr>();
            tokio::spawn(async move {
                let _ = axum::serve(l, app).with_graceful_shutdown(async { let _ = rx.await; }).await;
            });
            log::info!("웹 편집기 서버 시작: {host}:{port}");
        }
        Err(e) => {
            let msg = if e.kind() == std::io::ErrorKind::AddrInUse { crate::i18n::tr_args("포트 {1} 를 다른 프로그램이 쓰고 있습니다.", &[&port]) } else { e.to_string() };
            let mut cur = ERROR.lock();
            if *cur == msg {
                return; // 같은 오류를 되풀이해 알리지 않는다 (remote.rs 와 같은 까닭)
            }
            log::warn!("웹 편집기 서버를 켜지 못함: {msg}");
            *cur = msg;
        }
    }
    core.lock().emit(Ev::Ui);
}

pub fn stop() {
    if let Some(r) = RUN.lock().take() {
        let _ = r.stop.send(());
    }
}

/// 설정에 맞춰 켜고 끈다 (포트나 PIN 유무가 바뀌면 다시)
pub async fn sync(core: Shared) {
    let (enabled, port, pin) = conf(&core.lock());
    let host = host_for(&pin);
    let running = RUN.lock().as_ref().map(|r| (r.port, r.host.clone()));
    if let Some((p, h)) = running {
        if !enabled || p != port || h != host {
            stop();
            ERROR.lock().clear();
            core.lock().emit(Ev::Ui);
        }
    }
    if enabled && RUN.lock().is_none() {
        start(core, port, host).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn core() -> (Core, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("wp-webedit-{}-{}", std::process::id(), crate::util::short_id("t")));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("bible.db"), b"").unwrap();
        (Core::open(dir.clone(), dir.clone(), None).unwrap(), dir)
    }

    #[test]
    fn pin_rules() {
        let local: std::net::IpAddr = "127.0.0.1".parse().unwrap();
        let other: std::net::IpAddr = "192.168.0.7".parse().unwrap();
        assert!(allow(&local, "", "").is_ok(), "PIN 이 없으면 이 PC 에서는 묻지 않는다");
        assert_eq!(allow(&other, "", "").unwrap_err().0, StatusCode::FORBIDDEN, "PIN 이 없으면 다른 컴퓨터는 막는다");
        assert!(allow(&other, "4321", "4321").is_ok());
        assert_eq!(allow(&local, "1", "4321").unwrap_err().0, StatusCode::UNAUTHORIZED, "PIN 을 정하면 이 PC 도 묻는다");
        assert_eq!((host_for(""), host_for("1")), ("127.0.0.1", "0.0.0.0"));
    }

    #[test]
    fn save_edits_fields_keeps_others_and_detects_conflict() {
        let (mut c, dir) = core();
        c.new_cue("주일").unwrap();
        let mut t = crate::core::new_item("title", "설교");
        t["tplMain"] = json!("tpl-x");
        c.add_item(t, None);
        c.add_item(crate::core::new_item("song", "찬양"), None);
        let id = c.cue.as_ref().unwrap().id.clone();
        let got = cue_of(&c, &id).unwrap();
        let it = got["items"].as_array().unwrap();
        let (a, b) = (it[0]["id"].as_str().unwrap().to_string(), it[1]["id"].as_str().unwrap().to_string());
        std::thread::sleep(std::time::Duration::from_millis(5));
        // 차례 바꾸기 · 고치기 · 새 성경 순서 · 이름
        let v = json!({ "id": id, "base": got["updatedAt"], "name": "주일 2부", "items": [
            { "id": b, "lyrics": "새 가사", "title": "찬양" },
            { "id": a, "title": "말씀", "lyrics": "제목 순서에는 안 생김" },
            { "type": "bible", "title": "봉독", "ref": "요 3:16" } ] });
        let saved = save(&mut c, &v).unwrap();
        let items = saved["items"].as_array().unwrap();
        assert_eq!(saved["name"], "주일 2부");
        assert_eq!((items[0]["lyrics"].as_str(), items[1]["title"].as_str()), (Some("새 가사"), Some("말씀")));
        assert_eq!(items[1]["tplMain"], "tpl-x", "고치지 않는 칸은 지킨다");
        assert!(items[1].get("lyrics").is_none());
        assert_eq!((items[2]["type"].as_str(), items[2]["ref"].as_str()), (Some("bible"), Some("요 3:16")));
        assert!(!items[2]["id"].as_str().unwrap().is_empty());
        assert_eq!(c.cue.as_ref().unwrap().items.len(), 3, "조작 화면에 열린 큐시트도 새로 읽는다");
        // 예전 updatedAt 으로 저장하면 충돌
        std::thread::sleep(std::time::Duration::from_millis(5));
        assert_eq!(save(&mut c, &v).unwrap_err(), "conflict");
        // 넣을 수 없는 종류
        let v2 = json!({ "id": id, "base": saved["updatedAt"], "items": [ { "type": "hymn", "title": "x" } ] });
        assert!(save(&mut c, &v2).is_err());
        // 새 큐시트는 열린 큐시트를 바꾸지 않는다
        let n = create(&mut c, "청년부").unwrap();
        assert_eq!(n["name"], "청년부");
        assert_eq!(c.cue.as_ref().unwrap().id, id);
        drop(c);
        let _ = std::fs::remove_dir_all(dir);
    }
}
