//! 휴대폰을 카메라로 (쉬운방송의 생각을 참고해 새로 짬 — 코드는 가져오지 않음)
//!
//! - 휴대폰: 같은 Wi-Fi 에서 QR 로 `https://<이 PC>:37560/cam?k=<열쇠>` 를 연다. 휴대폰 브라우저는 https 에서만 카메라를 열어 주므로
//!   이 PC 가 스스로 만든 인증서로 https 를 연다(처음 한 번 '안전하지 않음' 안내가 뜬다). 앱 설치 없음.
//! - 출력 화면(예배당 창 · OBS 브라우저 소스): 카메라 장치 id 가 `phone:<휴대폰 id>` 이면 render.js 가 WebRTC 로 받는다.
//! - 이 서버는 연결 정보(SDP)만 이어 준다. 영상은 휴대폰 → 출력 화면으로 같은 내부망 안에서 바로 간다(인터넷을 거치지 않음).
//! - 휴대폰 크롬은 주소를 'xxxx.local' 로 가리기도 해서, 이어 줄 때 실제 IP 로 바꿔 적는다.
//! - 이어 주기는 짧은 요청 되풀이(폴링)로 한다: 휴대폰 0.7초마다, 출력 화면은 답을 기다릴 때만.

use crate::core::{Ev, Shared};
use axum::{
    body::Bytes,
    extract::{ConnectInfo, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use regex::Regex;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

pub const DEFAULT_PORT: u16 = 37560;
/// 이 시간 동안 소식이 없으면 연결이 끊긴 것으로 본다
const ALIVE_MS: i64 = 8_000;
/// 답을 받지 못한 요청은 이만큼 지나면 버린다
const ANSWER_TTL_MS: i64 = 30_000;

#[derive(Debug, Clone, Default)]
pub struct Phone {
    pub name: String,
    pub ip: Option<IpAddr>,
    pub last_seen: i64,
    /// 휴대폰에 보낼 것 (offer · bye)
    pub queue: Vec<Value>,
    /// 지금 이 휴대폰 영상을 받는 화면 수 (휴대폰이 알려 줌)
    pub viewers: usize,
    /// 휴대폰이 알려 준 화질 (예: "1920x1080")
    pub size: String,
}

#[derive(Default)]
struct Hub {
    phones: HashMap<String, Phone>,
    /// 출력 화면(viewer id) → (받은 시각, 휴대폰의 답 SDP)
    answers: HashMap<String, (i64, String)>,
}

static HUB: Lazy<Mutex<Hub>> = Lazy::new(Default::default);

fn now() -> i64 {
    crate::util::now_ms()
}

fn ok_id(s: &str) -> bool {
    (4..=64).contains(&s.len()) && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

// ---------------- 주소 고치기 ----------------

static MDNS: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\.local\b").unwrap());

/// SDP 의 'xxxx.local' 주소를 실제 IP 로 (크롬이 내부 주소를 가린 것)
pub fn fix_mdns(sdp: &str, ip: &str) -> String {
    MDNS.replace_all(sdp, ip).to_string()
}

/// 이 PC 의 내부망 주소 가운데 휴대폰과 같은 대역(앞 세 자리)인 것
pub fn pick_lan_ip(addrs: &[String], phone: Option<IpAddr>) -> Option<String> {
    let p = phone.map(|ip| ip.to_string()).unwrap_or_default();
    let head = |s: &str| s.rsplit_once('.').map(|x| x.0.to_string()).unwrap_or_default();
    addrs.iter().find(|a| !p.is_empty() && head(a) == head(&p)).or_else(|| addrs.first()).cloned()
}

// ---------------- 상태 ----------------

/// 지금 연결된 휴대폰들 → 카메라 목록에 넣을 장치 (deviceId "phone:<id>")
pub fn devices() -> Vec<crate::cameras::Device> {
    let t = now();
    let hub = HUB.lock();
    let mut v: Vec<crate::cameras::Device> = hub
        .phones
        .iter()
        .filter(|(_, p)| t - p.last_seen < ALIVE_MS)
        .map(|(id, p)| crate::cameras::Device { device_id: format!("phone:{id}"), label: format!("휴대폰 · {}", if p.name.is_empty() { "이름 없음" } else { &p.name }) })
        .collect();
    v.sort_by(|a, b| a.label.cmp(&b.label));
    v
}

pub fn phones_json() -> Vec<Value> {
    let t = now();
    let hub = HUB.lock();
    let mut v: Vec<Value> = hub
        .phones
        .iter()
        .filter(|(_, p)| t - p.last_seen < 60_000)
        .map(|(id, p)| json!({ "id": id, "deviceId": format!("phone:{id}"), "name": p.name, "online": t - p.last_seen < ALIVE_MS, "viewers": p.viewers, "size": p.size, "ip": p.ip.map(|x| x.to_string()) }))
        .collect();
    v.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    v
}

struct Run {
    port: u16,
    stop: tokio::sync::oneshot::Sender<()>,
}
static RUN: Lazy<Mutex<Option<Run>>> = Lazy::new(|| Mutex::new(None));
static ERROR: Lazy<Mutex<String>> = Lazy::new(|| Mutex::new(String::new()));

/// 설정 phoneCam { enabled, port, key }
pub fn status(core: &crate::core::Core) -> Value {
    let s = &core.settings.get()["phoneCam"];
    let run = RUN.lock();
    let port = run.as_ref().map(|x| x.port).unwrap_or(s["port"].as_u64().unwrap_or(DEFAULT_PORT as u64) as u16);
    let key = s["key"].as_str().unwrap_or("");
    json!({
        "enabled": s["enabled"].as_bool().unwrap_or(false), "port": port, "running": run.is_some(), "error": ERROR.lock().clone(),
        "urls": if run.is_some() { crate::remote::lan_addresses().iter().map(|a| format!("https://{a}:{port}/cam?k={key}")).collect::<Vec<_>>() } else { vec![] },
        "phones": phones_json(),
    })
}

pub fn new_key() -> String {
    (0..16).map(|_| format!("{:x}", rand::random::<u8>() % 16)).collect()
}

// ---------------- 휴대폰 쪽 (https) ----------------

fn js(code: StatusCode, v: Value) -> Response {
    (code, [(header::CACHE_CONTROL, "no-store")], Json(v)).into_response()
}

fn key_ok(core: &Shared, k: &str) -> bool {
    let want = core.lock().settings.get()["phoneCam"]["key"].as_str().unwrap_or("").to_string();
    !want.is_empty() && k == want
}

async fn cam_page(State(core): State<Shared>, Query(q): Query<HashMap<String, String>>) -> Response {
    if !key_ok(&core, q.get("k").map(String::as_str).unwrap_or("")) {
        return (StatusCode::FORBIDDEN, [(header::CONTENT_TYPE, "text/html; charset=utf-8")], "<meta name=viewport content='width=device-width'><p style='font:18px sans-serif;padding:20px'>주소의 열쇠가 맞지 않습니다. 워십프리젠터 화면의 QR 을 다시 찍어 주세요.</p>").into_response();
    }
    let web = core.lock().web_dir.clone();
    match tokio::fs::read(web.join("phone").join("cam.html")).await {
        Ok(b) => ([(header::CONTENT_TYPE, "text/html; charset=utf-8"), (header::CACHE_CONTROL, "no-store")], b).into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

/// TLS 리스너로 들어온 상대 주소 (axum 의 ConnectInfo 는 TcpListener 에만 있어서)
#[derive(Clone, Copy, Debug)]
pub struct PeerAddr(pub SocketAddr);

impl axum::extract::connect_info::Connected<axum::serve::IncomingStream<'_, TlsListener>> for PeerAddr {
    fn connect_info(stream: axum::serve::IncomingStream<'_, TlsListener>) -> Self {
        PeerAddr(*stream.remote_addr())
    }
}

/// 휴대폰이 0.7초마다: 살아 있음 알리기 + 보낼 것 받기. { k, id, name, viewers, size }
async fn cam_poll(State(core): State<Shared>, ConnectInfo(PeerAddr(addr)): ConnectInfo<PeerAddr>, body: Bytes) -> Response {
    let Ok(a) = serde_json::from_slice::<Value>(&body) else { return js(StatusCode::BAD_REQUEST, json!({ "ok": false })) };
    if !key_ok(&core, a["k"].as_str().unwrap_or("")) {
        return js(StatusCode::FORBIDDEN, json!({ "ok": false, "error": "key" }));
    }
    let id = a["id"].as_str().unwrap_or("").to_string();
    if !ok_id(&id) {
        return js(StatusCode::BAD_REQUEST, json!({ "ok": false, "error": "id" }));
    }
    let (msgs, fresh) = {
        let mut hub = HUB.lock();
        let fresh = hub.phones.get(&id).is_none_or(|p| now() - p.last_seen >= ALIVE_MS);
        let p = hub.phones.entry(id.clone()).or_default();
        p.name = a["name"].as_str().unwrap_or("").chars().filter(|c| !c.is_control()).take(40).collect();
        p.ip = Some(addr.ip());
        p.last_seen = now();
        p.viewers = a["viewers"].as_u64().unwrap_or(0) as usize;
        p.size = a["size"].as_str().unwrap_or("").chars().take(20).collect();
        (std::mem::take(&mut p.queue), fresh)
    };
    if fresh {
        core.lock().emit(Ev::Ui); // 카메라 목록에 새로 보이게
    }
    js(StatusCode::OK, json!({ "ok": true, "msgs": msgs }))
}

/// 휴대폰의 답 { k, id, viewer, sdp }
async fn cam_answer(State(core): State<Shared>, ConnectInfo(PeerAddr(addr)): ConnectInfo<PeerAddr>, body: Bytes) -> Response {
    let Ok(a) = serde_json::from_slice::<Value>(&body) else { return js(StatusCode::BAD_REQUEST, json!({ "ok": false })) };
    if !key_ok(&core, a["k"].as_str().unwrap_or("")) {
        return js(StatusCode::FORBIDDEN, json!({ "ok": false }));
    }
    let viewer = a["viewer"].as_str().unwrap_or("").to_string();
    let sdp = a["sdp"].as_str().unwrap_or("");
    if !ok_id(&viewer) || sdp.is_empty() || sdp.len() > 200_000 {
        return js(StatusCode::BAD_REQUEST, json!({ "ok": false }));
    }
    let sdp = fix_mdns(sdp, &addr.ip().to_string());
    let mut hub = HUB.lock();
    let t = now();
    hub.answers.retain(|_, (at, _)| t - *at < ANSWER_TTL_MS);
    hub.answers.insert(viewer, (t, sdp));
    js(StatusCode::OK, json!({ "ok": true }))
}

pub fn phone_router(core: Shared) -> Router {
    Router::new()
        .route("/", get(|| async { (StatusCode::OK, [(header::CONTENT_TYPE, "text/plain; charset=utf-8")], "WorshipPresenter phone camera") }))
        .route("/cam", get(cam_page))
        .route("/cam/poll", post(cam_poll))
        .route("/cam/answer", post(cam_answer))
        .with_state(core)
}

// ---------------- 출력 화면 쪽 (이 PC 의 조작 서버, 127.0.0.1) ----------------

/// 다른 웹사이트가 몰래 보내지 못하게 X-WorshipCue 머리글 (브라우저 사전 확인에서 막힌다)
fn same_app(h: &axum::http::HeaderMap) -> bool {
    h.get("x-worshipcue").and_then(|v| v.to_str().ok()) == Some("1")
}

async fn v_offer_h(headers: axum::http::HeaderMap, body: Bytes) -> Response {
    if !same_app(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    v_offer(body).await
}

async fn v_bye_h(headers: axum::http::HeaderMap, body: Bytes) -> Response {
    if !same_app(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    v_bye(body).await
}

/// 출력 화면이 휴대폰 영상을 청한다 { phone, viewer, sdp }
async fn v_offer(body: Bytes) -> Response {
    let Ok(a) = serde_json::from_slice::<Value>(&body) else { return js(StatusCode::BAD_REQUEST, json!({ "ok": false })) };
    let (phone, viewer, sdp) = (a["phone"].as_str().unwrap_or(""), a["viewer"].as_str().unwrap_or(""), a["sdp"].as_str().unwrap_or(""));
    if !ok_id(phone) || !ok_id(viewer) || sdp.is_empty() || sdp.len() > 200_000 {
        return js(StatusCode::BAD_REQUEST, json!({ "ok": false }));
    }
    let mut hub = HUB.lock();
    hub.answers.remove(viewer);
    let Some(p) = hub.phones.get_mut(phone).filter(|p| now() - p.last_seen < ALIVE_MS) else {
        return js(StatusCode::OK, json!({ "ok": false, "error": "offline" }));
    };
    // 이 PC 의 주소가 가려져 있으면 휴대폰과 같은 대역의 실제 주소로
    let sdp = match pick_lan_ip(&crate::remote::lan_addresses(), p.ip) {
        Some(ip) => fix_mdns(sdp, &ip),
        None => sdp.to_string(),
    };
    p.queue.retain(|m| m["viewer"] != viewer);
    p.queue.push(json!({ "t": "offer", "viewer": viewer, "sdp": sdp }));
    js(StatusCode::OK, json!({ "ok": true }))
}

async fn v_answer(Query(q): Query<HashMap<String, String>>) -> Response {
    let viewer = q.get("viewer").map(String::as_str).unwrap_or("");
    match HUB.lock().answers.remove(viewer) {
        Some((_, sdp)) => js(StatusCode::OK, json!({ "ok": true, "sdp": sdp })),
        None => js(StatusCode::OK, json!({ "ok": true, "wait": true })),
    }
}

async fn v_bye(body: Bytes) -> Response {
    let Ok(a) = serde_json::from_slice::<Value>(&body) else { return js(StatusCode::BAD_REQUEST, json!({ "ok": false })) };
    let (phone, viewer) = (a["phone"].as_str().unwrap_or(""), a["viewer"].as_str().unwrap_or(""));
    if let Some(p) = HUB.lock().phones.get_mut(phone) {
        p.queue.push(json!({ "t": "bye", "viewer": viewer }));
    }
    js(StatusCode::OK, json!({ "ok": true }))
}

pub fn viewer_routes() -> Router<Shared> {
    Router::new()
        .route("/phonecam/offer", post(v_offer_h))
        .route("/phonecam/answer", get(v_answer))
        .route("/phonecam/bye", post(v_bye_h))
        .route("/phonecam/list", get(|| async { Json(json!({ "phones": phones_json() })) }))
}

// ---------------- https (스스로 만든 인증서) ----------------

/// 인증서: 처음 한 번 만들어 자료 폴더에 둔다 (바뀌지 않아야 휴대폰이 한 번 넘어간 안내를 기억한다)
pub fn load_or_make_cert(dir: &Path) -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
    let (cf, kf) = (dir.join("phonecam-cert.der"), dir.join("phonecam-key.der"));
    if let (Ok(c), Ok(k)) = (std::fs::read(&cf), std::fs::read(&kf)) {
        if !c.is_empty() && !k.is_empty() {
            return Ok((c, k));
        }
    }
    let mut names: Vec<String> = vec!["localhost".into(), "worshippresenter.local".into()];
    names.extend(crate::remote::lan_addresses());
    let ck = rcgen::generate_simple_self_signed(names)?;
    let (c, k) = (ck.cert.der().to_vec(), ck.signing_key.serialize_der());
    std::fs::write(&cf, &c)?;
    std::fs::write(&kf, &k)?;
    Ok((c, k))
}

pub fn tls_config(cert: Vec<u8>, key: Vec<u8>) -> anyhow::Result<Arc<rustls::ServerConfig>> {
    use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let cfg = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?
        .with_no_client_auth()
        .with_single_cert(vec![CertificateDer::from(cert)], PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key)))?;
    Ok(Arc::new(cfg))
}

/// TLS 로 받는 axum 리스너 (악수는 5초 안에, 실패한 연결은 버리고 다음)
pub struct TlsListener {
    tcp: tokio::net::TcpListener,
    acc: tokio_rustls::TlsAcceptor,
}

impl axum::serve::Listener for TlsListener {
    type Io = tokio_rustls::server::TlsStream<tokio::net::TcpStream>;
    type Addr = SocketAddr;
    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            let (s, a) = match self.tcp.accept().await {
                Ok(x) => x,
                Err(_) => {
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    continue;
                }
            };
            if let Ok(Ok(t)) = tokio::time::timeout(Duration::from_secs(5), self.acc.accept(s)).await {
                return (t, a);
            }
        }
    }
    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        self.tcp.local_addr()
    }
}

async fn start(core: Shared, port: u16) {
    let dir = core.lock().data_dir.clone();
    let cfg = load_or_make_cert(&dir).and_then(|(c, k)| tls_config(c, k));
    let res = match cfg {
        Ok(cfg) => tokio::net::TcpListener::bind(("0.0.0.0", port)).await.map(|l| (l, cfg)).map_err(|e| if e.kind() == std::io::ErrorKind::AddrInUse { format!("포트 {port} 를 다른 프로그램이 쓰고 있습니다.") } else { e.to_string() }),
        Err(e) => Err(format!("인증서를 만들지 못했습니다: {e}")),
    };
    match res {
        Ok((l, cfg)) => {
            let (tx, rx) = tokio::sync::oneshot::channel();
            *RUN.lock() = Some(Run { port, stop: tx });
            ERROR.lock().clear();
            let listener = TlsListener { tcp: l, acc: tokio_rustls::TlsAcceptor::from(cfg) };
            let app = phone_router(core.clone()).into_make_service_with_connect_info::<PeerAddr>();
            tokio::spawn(async move {
                let _ = axum::serve(listener, app).with_graceful_shutdown(async { let _ = rx.await; }).await;
            });
            log::info!("휴대폰 카메라 서버 시작: https {port}");
        }
        Err(msg) => {
            let mut cur = ERROR.lock();
            if *cur == msg {
                return; // 같은 오류를 Ev::Ui 마다 다시 알리면 끝없이 돈다 (remote.rs 와 같은 이유)
            }
            log::warn!("휴대폰 카메라 서버를 켜지 못함: {msg}");
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

/// 설정에 맞춰 켜고 끈다 (remote::init 이 Ev::Ui 마다 부른다)
pub async fn sync(core: Shared) {
    let (enabled, port) = {
        let c = core.lock();
        let s = &c.settings.get()["phoneCam"];
        (s["enabled"].as_bool().unwrap_or(false), s["port"].as_u64().filter(|p| *p > 0).unwrap_or(DEFAULT_PORT as u64) as u16)
    };
    let running = RUN.lock().as_ref().map(|r| r.port);
    if running.is_some() && (!enabled || running != Some(port)) {
        stop();
        core.lock().emit(Ev::Ui);
    }
    if enabled && RUN.lock().is_none() {
        start(core, port).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mdns_and_ip() {
        let sdp = "a=candidate:1 1 udp 2113937151 0a1b2c3d-1111-2222-3333-444455556666.local 54321 typ host generation 0\r\nc=IN IP4 0.0.0.0\r\n";
        let f = fix_mdns(sdp, "192.168.0.10");
        assert!(f.contains(" 192.168.0.10 54321 typ host"));
        assert!(!f.contains(".local"));
        assert_eq!(fix_mdns("a=candidate:1 1 udp 1 10.0.0.5 5000 typ host", "1.2.3.4"), "a=candidate:1 1 udp 1 10.0.0.5 5000 typ host", "실제 IP 는 그대로");
        let addrs = vec!["10.0.0.7".to_string(), "192.168.0.10".to_string()];
        assert_eq!(pick_lan_ip(&addrs, "192.168.0.55".parse().ok()).as_deref(), Some("192.168.0.10"));
        assert_eq!(pick_lan_ip(&addrs, "172.16.0.2".parse().ok()).as_deref(), Some("10.0.0.7"), "같은 대역이 없으면 첫 주소");
        assert_eq!(pick_lan_ip(&[], None), None);
        assert!(ok_id("ab12-xy") && !ok_id("a") && !ok_id("../x"));
    }

    #[test]
    fn cert_roundtrip() {
        let dir = std::env::temp_dir().join(format!("wp-phonecam-{}", crate::util::short_id("t")));
        std::fs::create_dir_all(&dir).unwrap();
        let (c, k) = load_or_make_cert(&dir).unwrap();
        assert!(tls_config(c.clone(), k).is_ok());
        let (c2, _) = load_or_make_cert(&dir).unwrap();
        assert_eq!(c, c2, "한 번 만든 인증서를 그대로 쓴다");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// 실제 https 서버를 띄워 열쇠 확인 · 휴대폰 알리기 · 이어 주기를 끝까지
    #[tokio::test(flavor = "multi_thread")]
    async fn https_relay() {
        let dir = std::env::temp_dir().join(format!("wp-phonecam-srv-{}", crate::util::short_id("t")));
        std::fs::create_dir_all(dir.join("web").join("phone")).unwrap();
        std::fs::write(dir.join("bible.db"), b"").unwrap();
        std::fs::write(dir.join("web").join("phone").join("cam.html"), "<html>cam</html>").unwrap();
        let mut core = crate::core::Core::open(dir.clone(), dir.clone(), None).unwrap();
        core.web_dir = dir.join("web");
        core.settings.update(|d| d["phoneCam"] = json!({ "enabled": true, "port": 0, "key": "k123456789abcdef" }));
        let shared: Shared = Arc::new(crate::lockcheck::Mutex::new(core));
        let (c, k) = load_or_make_cert(&dir).unwrap();
        let tcp = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = tcp.local_addr().unwrap().port();
        let listener = TlsListener { tcp, acc: tokio_rustls::TlsAcceptor::from(tls_config(c, k).unwrap()) };
        let app = phone_router(shared.clone()).into_make_service_with_connect_info::<PeerAddr>();
        tokio::spawn(async move { let _ = axum::serve(listener, app).await; });
        // 스스로 만든 인증서를 믿지 않는 클라이언트 대신, 인증서 검사를 끈 ureq 로 (시험에서만)
        let base = format!("https://127.0.0.1:{port}");
        let r = tokio::task::spawn_blocking(move || {
            let tls = ureq::tls::TlsConfig::builder().disable_verification(true).build();
            let agent: ureq::Agent = ureq::Agent::config_builder().tls_config(tls).http_status_as_error(false).build().into();
            let page_bad = agent.get(format!("{base}/cam?k=nope")).call().unwrap().status().as_u16();
            let page = agent.get(format!("{base}/cam?k=k123456789abcdef")).call().unwrap().into_body().read_to_string().unwrap();
            let poll = agent.post(format!("{base}/cam/poll")).send_json(json!({ "k": "k123456789abcdef", "id": "phone01", "name": "강단 갤럭시", "viewers": 0, "size": "1920x1080" })).unwrap().into_body().read_to_string().unwrap();
            (page_bad, page, poll, agent, base)
        })
        .await
        .unwrap();
        assert_eq!(r.0, 403);
        assert_eq!(r.1, "<html>cam</html>");
        assert!(r.2.contains("\"ok\":true"));
        assert!(devices().iter().any(|d| d.device_id == "phone:phone01" && d.label.contains("강단 갤럭시")));
        // 출력 화면 → offer → 휴대폰이 받음 → answer → 출력 화면이 받음
        let off = v_offer(Bytes::from(json!({ "phone": "phone01", "viewer": "view01", "sdp": "v=0 offer" }).to_string())).await;
        assert_eq!(off.status(), StatusCode::OK);
        let (agent, base) = (r.3, r.4);
        let got = tokio::task::spawn_blocking(move || {
            let poll = agent.post(format!("{base}/cam/poll")).send_json(json!({ "k": "k123456789abcdef", "id": "phone01", "name": "강단 갤럭시" })).unwrap().into_body().read_to_string().unwrap();
            let ans = agent.post(format!("{base}/cam/answer")).send_json(json!({ "k": "k123456789abcdef", "id": "phone01", "viewer": "view01", "sdp": "v=0 answer 0a1b2c3d-1111-2222-3333-444455556666.local" })).unwrap().status().as_u16();
            (poll, ans)
        })
        .await
        .unwrap();
        assert!(got.0.contains("\"t\":\"offer\"") && got.0.contains("view01"), "{}", got.0);
        assert_eq!(got.1, 200);
        let mut q = HashMap::new();
        q.insert("viewer".to_string(), "view01".to_string());
        let a = v_answer(Query(q.clone())).await;
        let body = axum::body::to_bytes(a.into_body(), 1 << 20).await.unwrap();
        let v: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["sdp"], "v=0 answer 127.0.0.1", "휴대폰의 가린 주소는 실제 IP 로");
        let again: Value = serde_json::from_slice(&axum::body::to_bytes(v_answer(Query(q)).await.into_body(), 1 << 20).await.unwrap()).unwrap();
        assert_eq!(again["wait"], true, "답은 한 번만");
        // 없는 휴대폰
        let off2 = v_offer(Bytes::from(json!({ "phone": "nophone", "viewer": "view02", "sdp": "x" }).to_string())).await;
        let v2: Value = serde_json::from_slice(&axum::body::to_bytes(off2.into_body(), 1 << 20).await.unwrap()).unwrap();
        assert_eq!(v2["error"], "offline");
        let _ = std::fs::remove_dir_all(dir);
    }
}
