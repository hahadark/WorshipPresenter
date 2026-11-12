//! 휴대폰 리모컨 (Electron 판 remote.js): 같은 Wi-Fi 의 휴대폰·태블릿에서 다음·이전·숨김·순서 송출
//! - 켰을 때만 0.0.0.0:<포트>(기본 37540) 에 작은 서버. 모든 요청은 PIN 이 맞아야 하고 5번 틀리면 30초 막음
//! - OBS 도킹: 조작 서버(127.0.0.1)의 /dock 은 PIN 없이 (이 PC 안에서만 열림, POST 는 X-WorshipCue 머리글)

use crate::core::{Core, Ev, Shared};
use crate::util::now_ms;
use axum::body::Bytes;
use axum::extract::{ConnectInfo, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use regex::Regex;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;

pub const DEFAULT_PORT: u16 = 37540;

/// 리모컨 '끄기': 조작 화면이 이것을 보고 예배당 창도 닫는다
pub static OFF_REQUEST: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

struct Run {
    port: u16,
    stop: tokio::sync::oneshot::Sender<()>,
}

static RUN: Lazy<Mutex<Option<Run>>> = Lazy::new(|| Mutex::new(None));
static ERROR: Lazy<Mutex<String>> = Lazy::new(|| Mutex::new(String::new()));
static FAILS: Lazy<Mutex<HashMap<String, (u32, i64)>>> = Lazy::new(|| Mutex::new(HashMap::new()));

/// 이 PC 의 내부망 주소 (밖으로 패킷을 보내지 않고 길만 물어 봄)
pub fn lan_addresses() -> Vec<String> {
    let mut out = vec![];
    if let Ok(s) = std::net::UdpSocket::bind("0.0.0.0:0") {
        for probe in ["10.255.255.255:1", "192.168.255.255:1", "172.31.255.255:1"] {
            if s.connect(probe).is_ok() {
                if let Ok(a) = s.local_addr() {
                    let ip = a.ip().to_string();
                    if !ip.starts_with("169.254.") && !ip.starts_with("127.") && ip != "0.0.0.0" && !out.contains(&ip) {
                        out.push(ip);
                    }
                }
            }
        }
    }
    out
}

pub fn new_pin() -> String {
    (100000 + rand::random::<u32>() % 900000).to_string()
}

pub fn status(core: &Core) -> Value {
    let r = &core.settings.get()["remote"];
    let run = RUN.lock();
    let port = run.as_ref().map(|x| x.port).unwrap_or(r["port"].as_u64().unwrap_or(DEFAULT_PORT as u64) as u16);
    json!({
        "enabled": r["enabled"].as_bool().unwrap_or(false), "pin": r["pin"].as_str().unwrap_or(""), "port": port,
        "running": run.is_some(), "error": ERROR.lock().clone(),
        "urls": if run.is_some() { lan_addresses().iter().map(|a| format!("http://{a}:{port}/")).collect::<Vec<_>>() } else { vec![] },
    })
}

/// PIN 확인 (틀리면 5번마다 30초 막기)
pub fn check_pin(ip: &str, pin: &str, want: &str) -> Result<(), (StatusCode, &'static str)> {
    let mut f = FAILS.lock();
    let e = f.entry(ip.to_string()).or_insert((0, 0));
    if e.1 > now_ms() {
        return Err((StatusCode::TOO_MANY_REQUESTS, "잠시 후 다시 시도하세요 (PIN 을 여러 번 틀렸습니다)."));
    }
    if crate::trigger::safe_eq(pin, want) {
        f.remove(ip);
        return Ok(());
    }
    e.0 += 1;
    if e.0 % 5 == 0 {
        e.1 = now_ms() + 30_000;
    }
    Err((StatusCode::UNAUTHORIZED, "PIN 이 맞지 않습니다."))
}

static MARKS: Lazy<Regex> = Lazy::new(|| Regex::new(r"\*\*|\{색:#[0-9a-fA-F]{3,8}\}|\{/색\}|\{크기:\d{2,3}\}|\{/크기\}").unwrap());
static CHORDS: Lazy<Regex> = Lazy::new(|| Regex::new(r"\[([A-G](?:#|b|♯|♭)?(?:maj|min|m|M|sus|dim|aug|add|°|ø)?\d{0,2}(?:(?:sus|add|maj|b|#|\+|-)\d{1,2})*(?:/[A-G](?:#|b|♯|♭)?)?)\]").unwrap());
static SP2: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s{2,}").unwrap());

fn slide_text(sl: &Value) -> String {
    if sl.is_null() {
        return String::new();
    }
    match sl["kind"].as_str().unwrap_or("") {
        "title" => [sl["ref"].as_str().unwrap_or(""), sl["subtitle"].as_str().unwrap_or("")].iter().filter(|x| !x.is_empty()).cloned().collect::<Vec<_>>().join(" · "),
        "video" => "🎬 영상".into(),
        k @ ("image" | "score") => sl["caption"].as_str().filter(|x| !x.is_empty()).map(String::from).unwrap_or_else(|| if k == "score" { "🎼 악보".into() } else { "🖼 이미지".into() }),
        "blank" => "(빈 화면)".into(),
        _ => {
            let t = sl["parts"].as_array().into_iter().flatten().map(|p| p["text"].as_str().unwrap_or("")).collect::<Vec<_>>().join(" ");
            let t = MARKS.replace_all(&t, "");
            let t = CHORDS.replace_all(&t, "");
            SP2.replace_all(&t, " ").chars().take(200).collect()
        }
    }
}

/// 리모컨이 보는 상태 (stateFor)
pub fn state_for(core: &Core) -> Value {
    let live = core.pres.live_state();
    let p = &live["passage"];
    let title = if p["cueIndex"].as_i64().unwrap_or(-1) >= 0 && p["itemTitle"].as_str().is_some_and(|x| !x.is_empty()) { p["itemTitle"].clone() } else { p["title"].clone() };
    let sl = &live["slide"];
    let image = if (sl["kind"] == "image" || sl["kind"] == "score") && sl["image"].as_str().is_some_and(|x| !x.is_empty()) { sl["image"].clone() } else { json!("") };
    let labels: Vec<String> = live["slides"].as_array().into_iter().flatten().map(|x| x.as_str().unwrap_or("").strip_prefix('¶').unwrap_or("").to_string()).collect();
    let s = core.settings.get();
    json!({
        "live": {
            "has": live["hasLive"], "visible": live["visible"], "index": live["index"], "count": live["count"],
            "title": if title.is_null() { json!("") } else { title }, "text": slide_text(sl), "next": slide_text(&live["nextSlide"]),
            "image": image, "labels": labels, "cueIndex": p["cueIndex"].as_i64().unwrap_or(-1),
        },
        "cue": core.cue.as_ref().map(|c| json!({ "name": c.name, "items": c.items.iter().map(|it| json!({ "title": it["title"], "type": it["type"] })).collect::<Vec<_>>() })),
        // Rust 판은 조작 화면이 같은 프로그램이라 늘 열려 있음
        "control": true,
        "stage": { "message": "" },
        "obs": { "pip": s["output"]["obsPip"].as_bool().unwrap_or(false), "full": false },
    })
}

/// 리모컨에서 할 수 있는 일만
pub fn act(core: &mut Core, a: &Value) -> Result<Value, String> {
    match a["action"].as_str().unwrap_or("") {
        "next" => core.step(1),
        "prev" => core.step(-1),
        "toggle" => core.toggle_clear(),
        // 아예 끄기: 글 · 배경 · 로고를 지운다 (예배당 창은 조작 화면이 끔 — Ev::Outputs 로 알림)
        "off" => {
            core.clear_all();
            OFF_REQUEST.store(true, std::sync::atomic::Ordering::SeqCst); // 조작 화면이 보고 예배당 창을 닫는다
            core.emit(Ev::Ui);
        }
        "goto" => core.goto_live(a["index"].as_u64().unwrap_or(0) as usize),
        "cue" => return crate::trigger::send_cue_item(core, a["index"].as_i64().unwrap_or(0)),
        "obsPip" => {
            let on = !core.settings.get()["output"]["obsPip"].as_bool().unwrap_or(false);
            core.settings.update(|d| d["output"]["obsPip"] = json!(on));
            core.emit(Ev::Live);
        }
        "message" | "timer" | "obsFull" => return Err("이 조작은 아직 Rust 판에 없습니다 (준비 중).".into()),
        _ => return Err("알 수 없는 조작입니다.".into()),
    }
    Ok(json!({ "ok": true }))
}

fn js(code: StatusCode, v: Value) -> Response {
    (code, [(header::CACHE_CONTROL, "no-store")], Json(v)).into_response()
}

async fn file(web: &Path, rel: &str, ty: &'static str, lang_en: bool) -> Response {
    match tokio::fs::read(web.join(rel)).await {
        Ok(buf) => {
            let buf = if lang_en && rel.ends_with(".html") { with_lang(&buf, "/").into_bytes() } else { buf };
            ([(header::CONTENT_TYPE, ty), (header::CACHE_CONTROL, "no-cache")], buf).into_response()
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

/// 화면 언어가 영어면 <html data-lang="en"> 과 번역 스크립트 (server.js withLang)
pub fn with_lang(buf: &[u8], prefix: &str) -> String {
    String::from_utf8_lossy(buf)
        .replacen(r#"<html lang="ko">"#, r#"<html lang="en" data-lang="en">"#, 1)
        .replacen("<head>", &format!(r#"<head><script src="{prefix}i18n-en.js"></script><script src="{prefix}i18n.js"></script>"#), 1)
}

fn web_lang(core: &Shared) -> (std::path::PathBuf, bool) {
    let c = core.lock();
    (c.web_dir.clone(), c.settings.str(&["uiLang"]) == "en")
}

// ---------- 휴대폰 리모컨 서버 (0.0.0.0) ----------
async fn r_index(State(core): State<Shared>) -> Response {
    let (w, en) = web_lang(&core);
    file(&w, "remote/index.html", "text/html; charset=utf-8", en).await
}
async fn r_js(State(core): State<Shared>, uri: axum::http::Uri) -> Response {
    let (w, _) = web_lang(&core);
    let rel = match uri.path() {
        "/remote.js" => "remote/remote.js",
        "/groups.js" => "shared/groups.js",
        "/i18n.js" => "shared/i18n.js",
        _ => "shared/i18n-en.js",
    };
    file(&w, rel, "text/javascript; charset=utf-8", false).await
}
async fn r_state(State(core): State<Shared>, ConnectInfo(addr): ConnectInfo<SocketAddr>, Query(q): Query<HashMap<String, String>>) -> Response {
    let c = core.lock();
    let want = c.settings.str(&["remote", "pin"]);
    if let Err((code, e)) = check_pin(&addr.ip().to_string(), q.get("pin").map(|s| s.as_str()).unwrap_or(""), &want) {
        return js(code, json!({ "ok": false, "error": e }));
    }
    let mut v = state_for(&c);
    v["ok"] = json!(true);
    js(StatusCode::OK, v)
}
async fn r_do(State(core): State<Shared>, ConnectInfo(addr): ConnectInfo<SocketAddr>, body: Bytes) -> Response {
    if body.len() > 4000 {
        return js(StatusCode::BAD_REQUEST, json!({ "ok": false, "error": "잘못된 요청" }));
    }
    let Ok(a) = serde_json::from_slice::<Value>(&body) else { return js(StatusCode::BAD_REQUEST, json!({ "ok": false, "error": "잘못된 요청" })) };
    let mut c = core.lock();
    let want = c.settings.str(&["remote", "pin"]);
    if let Err((code, e)) = check_pin(&addr.ip().to_string(), a["pin"].as_str().unwrap_or(""), &want) {
        return js(code, json!({ "ok": false, "error": e }));
    }
    match act(&mut c, &a) {
        Ok(r) => js(StatusCode::OK, json!({ "ok": true, "result": r })),
        Err(e) => js(StatusCode::OK, json!({ "ok": false, "error": e })),
    }
}

pub fn remote_router(core: Shared) -> Router {
    Router::new()
        .route("/", get(r_index))
        .route("/index.html", get(r_index))
        .route("/remote.js", get(r_js))
        .route("/groups.js", get(r_js))
        .route("/i18n.js", get(r_js))
        .route("/i18n-en.js", get(r_js))
        .route("/r/state", get(r_state))
        .route("/r/do", post(r_do))
        .with_state(core)
}

// ---------- OBS 도킹 (조작 서버 127.0.0.1 에 붙음, PIN 없이) ----------
async fn dock_index(State(core): State<Shared>) -> Response {
    let (w, en) = web_lang(&core);
    file(&w, "remote/index.html", "text/html; charset=utf-8", en).await
}
async fn dock_state(State(core): State<Shared>) -> Response {
    let c = core.lock();
    let mut v = state_for(&c);
    v["ok"] = json!(true);
    v["dock"] = json!(true);
    js(StatusCode::OK, v)
}
async fn dock_do(State(core): State<Shared>, headers: HeaderMap, body: Bytes) -> Response {
    // 다른 웹사이트가 몰래 보내지 못하게 X-WorshipCue 머리글 (브라우저 사전 확인에서 막힌다)
    if headers.get("x-worshipcue").and_then(|v| v.to_str().ok()) != Some("1") {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Ok(a) = serde_json::from_slice::<Value>(&body) else { return js(StatusCode::BAD_REQUEST, json!({ "ok": false, "error": "잘못된 요청" })) };
    match act(&mut core.lock(), &a) {
        Ok(r) => js(StatusCode::OK, json!({ "ok": true, "result": r })),
        Err(e) => js(StatusCode::OK, json!({ "ok": false, "error": e })),
    }
}

pub fn dock_routes() -> Router<Shared> {
    Router::new()
        .route("/dock", get(dock_index))
        .route("/dock/", get(dock_index))
        .route("/remote.js", get(r_js))
        .route("/groups.js", get(r_js))
        .route("/i18n.js", get(r_js))
        .route("/i18n-en.js", get(r_js))
        .route("/r/state", get(dock_state))
        .route("/r/do", post(dock_do))
}

// ---------- 켜고 끄기 ----------
async fn start(core: Shared, port: u16, host: &str) {
    match tokio::net::TcpListener::bind((host, port)).await {
        Ok(l) => {
            let (tx, rx) = tokio::sync::oneshot::channel();
            *RUN.lock() = Some(Run { port, stop: tx });
            ERROR.lock().clear();
            let app = remote_router(core.clone()).into_make_service_with_connect_info::<SocketAddr>();
            tokio::spawn(async move {
                let _ = axum::serve(l, app).with_graceful_shutdown(async { let _ = rx.await; }).await;
            });
            log::info!("휴대폰 리모컨 서버 시작: {port}");
        }
        Err(e) => {
            *ERROR.lock() = if e.kind() == std::io::ErrorKind::AddrInUse { format!("포트 {port} 를 다른 프로그램이 쓰고 있습니다.") } else { e.to_string() };
        }
    }
    core.lock().emit(Ev::Ui);
}

pub fn stop() {
    if let Some(r) = RUN.lock().take() {
        let _ = r.stop.send(());
    }
}

/// 설정에 맞춰 켜고 끈다 (포트가 바뀌면 다시)
pub async fn sync(core: Shared) {
    let (enabled, port, host) = {
        let c = core.lock();
        let r = &c.settings.get()["remote"];
        (r["enabled"].as_bool().unwrap_or(false), r["port"].as_u64().filter(|p| *p > 0).unwrap_or(DEFAULT_PORT as u64) as u16, r["host"].as_str().unwrap_or("0.0.0.0").to_string())
    };
    let running = RUN.lock().as_ref().map(|r| r.port);
    if running.is_some() && (!enabled || running != Some(port)) {
        stop();
        core.lock().emit(Ev::Ui);
    }
    if enabled && RUN.lock().is_none() {
        start(core, port, &host).await;
    }
}

/// 조작 서버를 띄울 때 한 번: 설정이 바뀔 때마다 리모컨·내부망 서버를 맞춘다
pub fn init(core: Shared) {
    let mut rx = core.lock().tx.subscribe();
    tokio::spawn(async move {
        sync(core.clone()).await;
        crate::lan::sync(core.clone()).await;
        loop {
            match rx.recv().await {
                Ok(Ev::Ui) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    sync(core.clone()).await;
                    crate::lan::sync(core.clone()).await;
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pin_lockout() {
        assert!(check_pin("1.2.3.4", "123456", "123456").is_ok());
        for _ in 0..4 {
            assert_eq!(check_pin("9.9.9.9", "000000", "123456").unwrap_err().0, StatusCode::UNAUTHORIZED);
        }
        assert_eq!(check_pin("9.9.9.9", "000000", "123456").unwrap_err().0, StatusCode::UNAUTHORIZED);
        // 5번 틀린 뒤에는 맞아도 잠시 막힘
        assert_eq!(check_pin("9.9.9.9", "123456", "123456").unwrap_err().0, StatusCode::TOO_MANY_REQUESTS);
        assert!(check_pin("x", "", "").is_err());
        assert_eq!(new_pin().len(), 6);
    }
    #[test]
    fn text() {
        assert_eq!(slide_text(&json!({ "kind": "text", "parts": [{ "text": "**주** [C]님을  찬양" }] })), "주 님을 찬양");
    }
}
