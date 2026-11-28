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
        // 종류는 live.kind 로도 알려 준다 (새 화면은 아이콘으로 그림). 그림 글자(이모지)는 쓰지 않는다
        "video" => crate::i18n::tr("영상").into(),
        k @ ("image" | "score") => sl["caption"].as_str().filter(|x| !x.is_empty()).map(String::from).unwrap_or_else(|| if k == "score" { crate::i18n::tr("악보").into() } else { crate::i18n::tr("그림").into() }),
        "camera" => sl["camera"]["label"].as_str().filter(|x| !x.is_empty()).map(String::from).unwrap_or_else(|| crate::i18n::tr("카메라").into()),
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
    // 덧붙인 필드(새 리모컨 화면용 — 예전 화면은 모르는 필드를 그냥 지나친다):
    //   live.kind · nextKind(슬라이드 종류 → 아이콘), live.cueId(송출 중인 것이 어느 큐시트의 순서인지 — 열린 큐시트와 다르면 줄 표시를 하지 않게),
    //   live.nextItem(마지막 장에서 '다음' 을 누르면 넘어갈 순서), cue.id, 순서의 note(진행 메모), app(언어 · 판)
    let kind = |x: &Value| x["kind"].as_str().unwrap_or("").to_string();
    json!({
        "live": {
            "has": live["hasLive"], "visible": live["visible"], "index": live["index"], "count": live["count"],
            "title": if title.is_null() { json!("") } else { title }, "text": slide_text(sl), "next": slide_text(&live["nextSlide"]),
            "image": image, "labels": labels, "cueIndex": p["cueIndex"].as_i64().unwrap_or(-1),
            "kind": kind(sl), "nextKind": kind(&live["nextSlide"]), "cueId": p["cueId"].as_str().unwrap_or(""), "nextItem": next_item_title(core, p),
        },
        "cue": core.cue.as_ref().map(|c| json!({ "id": c.id, "name": c.name, "items": c.items.iter().map(|it| json!({ "title": it["title"], "type": it["type"], "note": it["note"].as_str().unwrap_or("").chars().take(80).collect::<String>() })).collect::<Vec<_>>() })),
        "app": { "lang": if s["uiLang"] == "en" { "en" } else { "ko" }, "version": env!("CARGO_PKG_VERSION") },
        // Rust 판은 조작 화면이 같은 프로그램이라 늘 열려 있음
        "control": true,
        "stage": { "message": core.b.stage.message, "timer": core.b.stage.timer_json() }, // [트랙 B]
        // 이름 자막: 명단과 지금 떠 있는 사람
        "names": { "people": core.people(), "on": core.name_tag_on() },
        "obs": { "pip": s["output"]["obsPip"].as_bool().unwrap_or(false), "full": core.obs_full },
        // 화면 구성 단추 (로드맵 4단계): 사용자 구성이 있을 때만
        "scenes": scenes_state(core),
    })
}

/// 리모컨의 화면 구성 줄: 출력마다 지금 구성 · '지금 한 번' 인지, 고를 수 있는 구성
pub fn scenes_state(core: &Core) -> Value {
    let s = core.settings.get();
    let list = crate::scenes::user_scenes(s);
    if list.is_empty() {
        return Value::Null;
    }
    let outs: Vec<Value> = crate::outputs::list(s)
        .into_iter()
        .map(|(id, name)| {
            let (now, from) = core.scene_choice(&id, true);
            json!({ "id": id, "name": crate::i18n::tr(&name), "now": now, "once": from == crate::outputs::From::Once, "base": crate::scenes::output_scene_id(s, &id) })
        })
        .collect();
    json!({ "outputs": outs, "list": list.iter().map(|x| json!({ "id": x["id"], "name": x["name"] })).collect::<Vec<_>>() })
}

/// 송출 중인 순서 다음에 올 순서의 제목 (마지막 장에서 '다음' 을 누르면 넘어갈 곳).
/// 송출 중인 것이 열린 큐시트의 순서이고 '끝에서 다음 순서로'(cueAdvance)가 켜져 있을 때만 — 아니면 빈 글.
fn next_item_title(core: &Core, p: &Value) -> String {
    let (Some(c), Some(id), Some(ci)) = (core.cue.as_ref(), p["cueId"].as_str().filter(|x| !x.is_empty()), p["cueIndex"].as_i64().filter(|i| *i >= 0)) else { return String::new() };
    if c.id != id || core.settings.get()["cueAdvance"].as_bool() == Some(false) {
        return String::new();
    }
    // 음악 순서는 건너뛴다 (trigger::advance_live_cue 와 같은 규칙)
    c.items.iter().skip(ci as usize + 1).find(|it| it["type"] != "audio").and_then(|it| it["title"].as_str()).unwrap_or("").to_string()
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
        // [트랙 B] 무대 메시지 · 타이머 (리모컨 화면의 op: start·pause·resume·reset·add)
        "message" => core.stage_message(a["text"].as_str().unwrap_or("")),
        // 이름 자막: index 번째 사람 띄우기 · index < 0 이면 내리기
        "name" => match a["index"].as_i64() {
            Some(i) if i >= 0 => {
                core.show_person(i as usize).map_err(|e| e.to_string())?;
            }
            _ => core.hide_name_tag(),
        },
        "timer" if a["op"] == "until" => core.stage_timer_until(a["at"].as_str().unwrap_or("")).map_err(|e| e.to_string())?, // 몇 시까지
        "timer" => core.stage_timer(a["op"].as_str().unwrap_or(""), a["minutes"].as_f64().unwrap_or(0.0)).map_err(|e| e.to_string())?,
        // 화면 구성 (로드맵 4단계): 기본 지금 한 번, hold 면 출력 기본
        "scene" => {
            let out = a["output"].as_str().unwrap_or("overlay");
            let id = a["id"].as_str().unwrap_or("");
            if a["clear"] == true {
                core.clear_scene_once(out);
            } else {
                core.trigger_scene(out, id, a["hold"] == true).map_err(|e| e.to_string())?;
            }
        }
        "obsFull" => { let f = a["on"].as_bool().unwrap_or(!core.obs_full); crate::themes::set_obs_view(core, Some(f), None); } // [트랙 C]
        _ => return Err(crate::i18n::tr("알 수 없는 조작입니다.").into()),
    }
    Ok(json!({ "ok": true }))
}

fn js(code: StatusCode, v: Value) -> Response {
    (code, [(header::CACHE_CONTROL, "no-store")], Json(v)).into_response()
}

async fn file(web: &Path, rel: &str, ty: &'static str, lang_en: bool) -> Response {
    match tokio::fs::read(web.join(rel)).await {
        Ok(buf) => {
            let buf = if rel.ends_with(".html") { with_app_lang(&buf, lang_en).into_bytes() } else { buf };
            ([(header::CONTENT_TYPE, ty), (header::CACHE_CONTROL, "no-cache")], buf).into_response()
        }
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

/// 웹 화면(휴대폰 리모컨 · 휴대폰 카메라 · 웹 편집기)에 앱의 화면 언어를 알려 준다: `<html lang="ko" data-app-lang="ko|en">`.
/// 화면마다 작은 사전을 따로 갖고 있어(shared/ui/wpweb.js 의 WPW.dict) 큰 영어 사전(i18n-en.js)을 휴대폰에 내려보내지 않는다.
pub fn with_app_lang(buf: &[u8], en: bool) -> String {
    String::from_utf8_lossy(buf).replacen(r#"<html lang="ko">"#, if en { r#"<html lang="ko" data-app-lang="en">"# } else { r#"<html lang="ko" data-app-lang="ko">"# }, 1)
}

/// 세 웹 화면이 함께 쓰는 모양 · 도구 · 글꼴 (`/wpui/<이름>`). 정해 둔 이름만 — 다른 파일은 열어 주지 않는다.
/// 인터넷 없이 내부망에서만 열리므로 글꼴(Pretendard)도 이 PC 가 준다. 글꼴은 주소에 ?v= 를 붙여 부르고 오래 기억하게 한다.
pub async fn ui_asset(web: &Path, name: &str) -> Response {
    let (rel, ty, cache) = match name {
        "wpweb.css" => ("shared/ui/wpweb.css", "text/css; charset=utf-8", "no-cache"),
        "wpweb.js" => ("shared/ui/wpweb.js", "text/javascript; charset=utf-8", "no-cache"),
        "pretendard.woff2" => ("shared/fonts/PretendardVariable.woff2", "font/woff2", "public, max-age=31536000, immutable"),
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    match tokio::fs::read(web.join(rel)).await {
        Ok(buf) => ([(header::CONTENT_TYPE, ty), (header::CACHE_CONTROL, cache)], buf).into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

/// `/wpui/<이름>` (리모컨 · 도킹 · 휴대폰 카메라 · 웹 편집기 서버가 함께 씀)
pub async fn ui_route(State(core): State<Shared>, axum::extract::Path(name): axum::extract::Path<String>) -> Response {
    let w = core.lock().web_dir.clone();
    ui_asset(&w, &name).await
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
        "/remote.css" => "remote/remote.css",
        "/groups.js" => "shared/groups.js",
        "/i18n.js" => "shared/i18n.js",
        _ => "shared/i18n-en.js",
    };
    file(&w, rel, if rel.ends_with(".css") { "text/css; charset=utf-8" } else { "text/javascript; charset=utf-8" }, false).await
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
        .route("/remote.css", get(r_js))
        .route("/wpui/{name}", get(ui_route))
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
        .route("/remote.css", get(r_js))
        .route("/wpui/{name}", get(ui_route))
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
            let msg = if e.kind() == std::io::ErrorKind::AddrInUse { format!("포트 {port} 를 다른 프로그램이 쓰고 있습니다.") } else { e.to_string() };
            // 같은 오류면 알리지 않는다: init 이 Ev::Ui 마다 sync → start 를 다시 부르므로,
            // 포트가 쓰이고 있을 때 여기서 매번 Ev::Ui 를 보내면 끝없이 돈다(초당 수천 번, 가만히 둘 때 CPU 60%)
            let mut cur = ERROR.lock();
            if *cur == msg {
                return;
            }
            log::warn!("휴대폰 리모컨 서버를 켜지 못함: {msg}");
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
        crate::phonecam::sync(core.clone()).await;
        crate::webedit::sync(core.clone()).await;
        loop {
            match rx.recv().await {
                Ok(Ev::Ui) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    sync(core.clone()).await;
                    crate::lan::sync(core.clone()).await;
                    crate::phonecam::sync(core.clone()).await;
                    crate::webedit::sync(core.clone()).await;
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

#[cfg(test)]
mod timer_until_tests {
    /// 리모컨 '그 시각까지': 이상한 시각·지난 시각은 오류 글, 타이머는 그대로
    #[test]
    fn remote_timer_until() {
        let dir = std::env::temp_dir().join(format!("wp-rmt-{}", crate::util::now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("bible.db"), b"").unwrap();
        let mut c = crate::core::Core::open(dir.clone(), dir.clone(), None).unwrap();
        let e = super::act(&mut c, &serde_json::json!({ "action": "timer", "op": "until", "at": "몰라" })).unwrap_err();
        assert!(e.contains("11:00"), "{e}");
        assert!(super::act(&mut c, &serde_json::json!({ "action": "timer", "op": "until", "at": "0:00" })).is_err());
        assert!(!c.b.stage.timer.running);
        drop(c);
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[cfg(test)]
mod web_ui_tests {
    use serde_json::json;
    /// 새 리모컨 화면이 쓰는 덧붙인 필드 (예전 화면은 지나친다) · 화면 언어 표시
    #[test]
    fn state_has_added_fields_and_app_lang() {
        let dir = std::env::temp_dir().join(format!("wp-rmt-ui-{}-{}", std::process::id(), crate::util::now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("bible.db"), b"").unwrap();
        let mut c = crate::core::Core::open(dir.clone(), dir.clone(), None).unwrap();
        c.new_cue("주일").unwrap();
        let mut a = crate::core::new_item("title", "예배로 부름");
        a["note"] = json!("인도자 신호 뒤");
        c.add_item(a, None);
        c.add_item(crate::core::new_item("audio", "반주"), None);
        c.add_item(crate::core::new_item("title", "설교"), None);
        crate::trigger::send_cue_item(&mut c, 0).unwrap();
        let s = super::state_for(&c);
        let cue_id = c.cue.as_ref().unwrap().id.clone();
        assert_eq!(s["live"]["kind"], "title");
        assert_eq!(s["live"]["cueId"], cue_id.as_str());
        assert_eq!(s["live"]["nextItem"], "설교", "마지막 장에서 '다음' 은 음악 순서를 건너뛴 다음 순서");
        assert_eq!(s["cue"]["id"], cue_id.as_str());
        assert_eq!(s["cue"]["items"][0]["note"], "인도자 신호 뒤");
        assert_eq!(s["app"]["lang"], "ko");
        // 예전 필드는 그대로
        for k in ["has", "visible", "index", "count", "title", "text", "next", "image", "labels", "cueIndex"] {
            assert!(s["live"].get(k).is_some(), "live.{k}");
        }
        drop(c);
        let _ = std::fs::remove_dir_all(dir);
        assert!(super::with_app_lang(b"<html lang=\"ko\"><head>", true).starts_with("<html lang=\"ko\" data-app-lang=\"en\">"));
        assert!(super::with_app_lang(b"<html lang=\"ko\"><head>", false).contains("data-app-lang=\"ko\""));
    }

    /// /wpui/ 는 정해 둔 세 파일만 (다른 경로를 열어 주지 않는다)
    #[test]
    fn ui_asset_only_known_names() {
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let web = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web");
        rt.block_on(async {
            assert_eq!(super::ui_asset(&web, "wpweb.css").await.status(), axum::http::StatusCode::OK);
            assert_eq!(super::ui_asset(&web, "wpweb.js").await.status(), axum::http::StatusCode::OK);
            assert_eq!(super::ui_asset(&web, "pretendard.woff2").await.status(), axum::http::StatusCode::OK);
            assert_eq!(super::ui_asset(&web, "../settings.json").await.status(), axum::http::StatusCode::NOT_FOUND);
            assert_eq!(super::ui_asset(&web, "remote.js").await.status(), axum::http::StatusCode::NOT_FOUND);
        });
    }
}
