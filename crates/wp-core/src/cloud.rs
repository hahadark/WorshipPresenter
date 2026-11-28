//! 클라우드 연결 (웹판 1단계, docs/web/architecture.md): 지금 로컬 서버(127.0.0.1:37528)로 내보내는 출력 상태를
//! 클라우드(Cloudflare 의 교회 중계실)로도 보내고, 클라우드의 리모컨 · 웹 편집기 · 출력 화면이 청한 것을 받아 처리한다.
//! - 설정 `cloud { enabled(기본 꺼짐), url, keyEnc(설치판 연결 열쇠 '교회.열쇠', DPAPI · 키체인), remote(기본 켬), editor(기본 켬) }`
//! - 이 PC 에서 밖으로 나가는 WebSocket 하나만 (포트 열기 · 방화벽 없음). 따로 도는 스레드라 조작 화면을 막지 않는다.
//! - 꺼져 있으면 아무것도 하지 않는다 (스레드는 설정만 1초마다 본다). 끊기면 1 → 2 → … 30초 간격으로 다시 잇는다.
//! - 이으면 전체(모든 화면의 render · draw · remote)부터, 그 뒤로는 바뀐 것만. 송출 자료 모양은 로컬 SSE 와 같다.
//! - 받는 요청은 계약의 길만 (리모컨 /r/* · 편집기 /e/* · 미디어). 미디어는 이 PC 의 로컬 서버에서 읽어 그대로 보낸다
//!   → 폴더 경계 · Range · 찬송가 SVG 처리를 다시 짜지 않는다 (내부망 출력과 같은 범위).

use crate::core::{Core, Ev, Shared};
use crate::util::now_ms;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use web_time::{Duration, Instant};
use tungstenite::{Message, WebSocket};

/// 계약 버전 (cloud/src/protocol.mjs PROTOCOL 과 같게)
pub const PROTOCOL: u64 = 1;
/// 이진 조각 하나 (protocol.mjs LIMITS.chunk)
const CHUNK: usize = 256 * 1024;
/// 리모컨 · 편집기 요청 본문 한도 (설치판 서버와 같게)
const BODY_MAX: usize = 2_000_000;

// ---------- 화면에 보일 상태 ----------
#[derive(Clone, Debug, Default)]
pub struct Status {
    /// off · connecting · online · error
    pub state: String,
    pub error: String,
    pub since: i64,
    /// 클라우드로 보고 있는 화면 (화면 이름, 종류)
    pub viewers: Vec<(String, String)>,
    pub sent: u64,
    pub requests: u64,
}
static STATUS: Lazy<Mutex<Status>> = Lazy::new(|| Mutex::new(Status { state: "off".into(), ..Default::default() }));
static STARTED: AtomicBool = AtomicBool::new(false);

pub fn status() -> Status {
    STATUS.lock().clone()
}
fn set_state(state: &str, error: &str) {
    let mut s = STATUS.lock();
    if s.state != state {
        s.since = now_ms();
    }
    s.state = state.into();
    s.error = error.into();
    if state != "online" {
        s.viewers.clear();
    }
}

// ---------- 설정 ----------
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conf {
    pub enabled: bool,
    pub url: String,
    pub key: String,
    pub remote: bool,
    pub editor: bool,
}

pub fn conf(core: &Core) -> Conf {
    let c = &core.settings.get()["cloud"];
    // 화면에서 넣은 열쇠는 암호화(keyEnc). 평문 `key` 는 점검(격리한 자료 폴더)용으로만 읽는다 (ai.key 와 같은 규칙)
    let key = c["keyEnc"].as_str().filter(|x| !x.is_empty()).and_then(crate::dpapi::decrypt).or_else(|| c["key"].as_str().map(String::from)).unwrap_or_default().trim().to_string();
    Conf {
        enabled: c["enabled"].as_bool().unwrap_or(false),
        url: c["url"].as_str().unwrap_or("").trim().trim_end_matches('/').to_string(),
        key,
        remote: c["remote"].as_bool().unwrap_or(true),
        editor: c["editor"].as_bool().unwrap_or(true),
    }
}

/// 연결 열쇠 넣기 (None = 그대로, Some("") = 지우기). 열쇠는 화면 · 로그 · /api/state 에 내보내지 않는다
pub fn set_key(core: &mut Core, key: &str) {
    let k = key.trim();
    let enc = if k.is_empty() { String::new() } else { crate::dpapi::encrypt(k) };
    core.settings.update(|d| {
        if !d["cloud"].is_object() {
            d["cloud"] = json!({});
        }
        d["cloud"]["keyEnc"] = json!(enc);
        if let Some(o) = d["cloud"].as_object_mut() {
            o.remove("key");
        }
    });
}

pub fn has_key(core: &Core) -> bool {
    !conf(core).key.is_empty()
}

/// 열쇠 모양 '교회.열쇠' (교회 ID 는 영문 소문자 · 숫자 · -)
pub fn key_ok(key: &str) -> bool {
    let Some((church, secret)) = key.split_once('.') else { return false };
    let c = church.as_bytes();
    (2..=40).contains(&c.len())
        && c.iter().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
        && c[0] != b'-'
        && c[c.len() - 1] != b'-'
        && secret.len() >= 20
        && secret.chars().all(|ch| ch.is_ascii_alphanumeric())
}

/// 클라우드 주소 → 설치판 연결 주소. https 만 (이 PC 안 · 시험용 http://127.0.0.1 · localhost 는 허용)
pub fn bridge_url(url: &str) -> Result<String, String> {
    let u = url::Url::parse(url.trim()).map_err(|_| crate::i18n::tr("클라우드 주소가 올바르지 않습니다 (예: https://….workers.dev)."))?;
    let host = u.host_str().unwrap_or("");
    let ws = match u.scheme() {
        "https" => "wss",
        "http" if host == "127.0.0.1" || host == "localhost" || host == "[::1]" => "ws",
        _ => return Err(crate::i18n::tr("클라우드 주소는 https:// 로 시작해야 합니다 (연결 열쇠가 그대로 보이지 않게).")),
    };
    let port = u.port().map(|p| format!(":{p}")).unwrap_or_default();
    Ok(format!("{ws}://{host}{port}/bridge"))
}

// ---------- 시작 ----------
/// 조작 서버를 띄운 뒤 한 번 (server::serve). 스레드 하나가 설정을 보고 켜고 끈다
pub fn init(core: Shared) {
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let _ = std::thread::Builder::new().name("wp-cloud".into()).spawn(move || run(core));
}

fn run(core: Shared) {
    let mut backoff = 1u64;
    let mut stuck: Option<Conf> = None; // 열쇠가 틀렸거나 지워진 설정 — 설정이 바뀔 때까지 다시 잇지 않는다
    loop {
        let cf = conf(&core.lock());
        if !cf.enabled || cf.url.is_empty() || cf.key.is_empty() {
            let why = if cf.enabled { crate::i18n::tr("클라우드 주소와 연결 열쇠를 넣어 주세요.") } else { String::new() };
            set_state("off", &why);
            stuck = None;
            std::thread::sleep(Duration::from_secs(1));
            continue;
        }
        if stuck.as_ref() == Some(&cf) {
            std::thread::sleep(Duration::from_secs(1));
            continue;
        }
        set_state("connecting", "");
        let t0 = Instant::now();
        let mut wait_s = backoff;
        match session(&core, &cf) {
            End::Settings => { backoff = 1; continue; }
            End::Fatal(e) => { log::warn!("클라우드 연결 멈춤: {e}"); set_state("error", &e); stuck = Some(cf); continue; }
            End::Retry(e) => { log::info!("클라우드 연결 끊김: {e}"); set_state("error", &e); }
            End::Replaced => { set_state("error", &crate::i18n::tr("다른 PC 가 같은 연결 열쇠로 클라우드에 붙어 이 연결이 끊겼습니다. 1분 뒤 다시 잇습니다.")); wait_s = 60; }
        }
        // 오래 붙어 있다 끊긴 것이면 처음부터 (1초)
        if t0.elapsed() > Duration::from_secs(60) {
            backoff = 1;
            wait_s = wait_s.min(if wait_s == 60 { 60 } else { 1 });
        }
        // 흔들림을 줘서 (교회 여러 PC · 클라우드가 다시 켜질 때 한꺼번에 붙지 않게)
        let wait = Duration::from_millis(wait_s * 1000 + (rand::random::<u64>() % 700));
        let until = Instant::now() + wait;
        while Instant::now() < until {
            std::thread::sleep(Duration::from_millis(200));
            if conf(&core.lock()) != cf {
                break;
            }
        }
        backoff = (backoff * 2).min(30);
    }
}

enum End {
    /// 설정이 바뀜 (끄기 · 주소 · 열쇠)
    Settings,
    /// 다시 이어 볼 만한 끊김
    Retry(String),
    /// 같은 열쇠로 다른 곳이 붙음 (4001)
    Replaced,
    /// 열쇠가 틀림 · 지워짐 · 계약이 다름 — 설정을 고칠 때까지 멈춤
    Fatal(String),
}

// ---------- 연결 하나 ----------
enum Stream {
    Plain(TcpStream),
    Tls(Box<rustls::StreamOwned<rustls::ClientConnection, TcpStream>>),
}
impl Stream {
    fn tcp(&self) -> &TcpStream {
        match self {
            Stream::Plain(s) => s,
            Stream::Tls(s) => &s.sock,
        }
    }
}
impl Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Stream::Plain(s) => s.read(buf),
            Stream::Tls(s) => s.read(buf),
        }
    }
}
impl Write for Stream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Stream::Plain(s) => s.write(buf),
            Stream::Tls(s) => s.write(buf),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Stream::Plain(s) => s.flush(),
            Stream::Tls(s) => s.flush(),
        }
    }
}

fn connect(cf: &Conf) -> Result<WebSocket<Stream>, End> {
    use tungstenite::client::IntoClientRequest;
    let url = bridge_url(&cf.url).map_err(End::Fatal)?;
    let u = url::Url::parse(&url).map_err(|e| End::Fatal(e.to_string()))?;
    let host = u.host_str().unwrap_or("").trim_start_matches('[').trim_end_matches(']').to_string();
    let port = u.port_or_known_default().unwrap_or(443);
    use std::net::ToSocketAddrs;
    let addr = (host.as_str(), port).to_socket_addrs().map_err(|e| End::Retry(crate::i18n::tr_args("클라우드 주소를 찾지 못했습니다 (인터넷 연결 확인): {1}", &[&e])))?.next().ok_or_else(|| End::Retry("주소 없음".into()))?;
    let tcp = TcpStream::connect_timeout(&addr, Duration::from_secs(10)).map_err(|e| End::Retry(crate::i18n::tr_args("클라우드에 연결하지 못했습니다: {1}", &[&e])))?;
    let _ = tcp.set_nodelay(true);
    let _ = tcp.set_read_timeout(Some(Duration::from_secs(15)));
    let _ = tcp.set_write_timeout(Some(Duration::from_secs(15)));
    let stream = if u.scheme() == "wss" {
        let mut roots = rustls::RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let cfg = rustls::ClientConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .map_err(|e| End::Fatal(e.to_string()))?
            .with_root_certificates(roots)
            .with_no_client_auth();
        let name = rustls::pki_types::ServerName::try_from(host.clone()).map_err(|e| End::Fatal(e.to_string()))?;
        let conn = rustls::ClientConnection::new(Arc::new(cfg), name).map_err(|e| End::Retry(e.to_string()))?;
        Stream::Tls(Box::new(rustls::StreamOwned::new(conn, tcp)))
    } else {
        Stream::Plain(tcp)
    };
    let mut req = url.as_str().into_client_request().map_err(|e| End::Fatal(e.to_string()))?;
    let auth = format!("Bearer {}", cf.key).parse().map_err(|_| End::Fatal(crate::i18n::tr("연결 열쇠에 쓸 수 없는 글자가 있습니다.")))?;
    req.headers_mut().insert("authorization", auth);
    req.headers_mut().insert("user-agent", format!("WorshipPresenter/{} (cloud bridge)", env!("CARGO_PKG_VERSION")).parse().unwrap());
    let cfgws = tungstenite::protocol::WebSocketConfig::default().max_message_size(Some(8 << 20)).max_frame_size(Some(8 << 20));
    match tungstenite::client::client_with_config(req, stream, Some(cfgws)) {
        Ok((ws, _)) => {
            let _ = ws.get_ref().tcp().set_read_timeout(Some(Duration::from_millis(40)));
            Ok(ws)
        }
        Err(tungstenite::HandshakeError::Failure(tungstenite::Error::Http(r))) => {
            let code = r.status().as_u16();
            let body = r.body().as_ref().map(|b| String::from_utf8_lossy(b).to_string()).unwrap_or_default();
            let msg = serde_json::from_str::<Value>(&body).ok().and_then(|v| v["error"].as_str().map(String::from)).unwrap_or_default();
            if code == 401 || code == 403 {
                Err(End::Fatal(crate::i18n::tr_args("연결 열쇠가 맞지 않습니다 ({1}). 클라우드 주인 화면에서 '설치판 연결' 열쇠를 새로 만들어 넣어 주세요.", &[&if msg.is_empty() { code.to_string() } else { msg }])))
            } else if code == 429 {
                Err(End::Retry(crate::i18n::tr("클라우드가 잠시 연결을 막았습니다 (여러 번 틀림). 잠시 뒤 다시 잇습니다.")))
            } else {
                Err(End::Retry(format!("HTTP {code} {msg}")))
            }
        }
        Err(e) => Err(End::Retry(crate::i18n::tr_args("클라우드와 연결하지 못했습니다: {1}", &[&e]))),
    }
}

/// 다른 스레드(미디어)가 연결로 보낼 것
enum Out {
    Text(String),
    Bin(Vec<u8>),
}

/// 연결 하나의 상태: 보낸 것 (같은 것을 다시 보내지 않게)
#[derive(Default)]
struct Sent {
    render: HashMap<String, String>,
    draw: String,
    remote: String,
}

fn session(core: &Shared, cf: &Conf) -> End {
    let mut ws = match connect(cf) {
        Ok(w) => w,
        Err(e) => return e,
    };
    log::info!("클라우드에 연결함: {}", cf.url);
    let mut rx = core.lock().tx.subscribe();
    let (out_tx, out_rx) = mpsc::sync_channel::<Out>(16);
    let cancelled: Arc<Mutex<HashSet<u64>>> = Arc::new(Mutex::new(HashSet::new()));
    let mut sent = Sent::default();
    let lang = if core.lock().settings.str(&["uiLang"]) == "en" { "en" } else { "ko" };
    let outputs = clients(&core.lock());
    let hello = json!({ "t": "hello", "v": PROTOCOL, "app": format!("WorshipPresenter {}", env!("CARGO_PKG_VERSION")), "lang": lang, "outputs": outputs, "caps": { "remote": cf.remote, "editor": cf.editor, "media": true } });
    if let Err(e) = ws.send(Message::text(hello.to_string())) {
        return End::Retry(e.to_string());
    }
    let (mut live, mut draw, mut ui) = (true, true, true);
    let mut last_seen = Instant::now();
    let mut last_ping = Instant::now();
    let mut last_conf = Instant::now();
    let mut online_marked = false;
    loop {
        // 1) 받기 (40ms 기다림)
        match ws.read() {
            Ok(Message::Text(t)) => {
                last_seen = Instant::now();
                let t = t.as_str();
                if t == "pong" {
                    continue;
                }
                let Ok(m) = serde_json::from_str::<Value>(t) else { continue };
                match m["t"].as_str().unwrap_or("") {
                    "ready" => {
                        if m["v"].as_u64() != Some(PROTOCOL) {
                            return End::Fatal(crate::i18n::tr("클라우드와 설치판의 연결 방식(계약 버전)이 다릅니다. 설치판을 새 버전으로 올려 주세요."));
                        }
                        set_state("online", "");
                        online_marked = true;
                    }
                    "sync" => { sent = Sent::default(); live = true; draw = true; ui = true; }
                    "viewers" => {
                        STATUS.lock().viewers = m["list"].as_array().into_iter().flatten().map(|v| (v["client"].as_str().unwrap_or("").to_string(), v["kind"].as_str().unwrap_or("").to_string())).collect();
                    }
                    "cmd" => view_command(core, &m),
                    "req" => {
                        STATUS.lock().requests += 1;
                        if let Some(r) = handle_request(core, cf, &m, &out_tx, &cancelled) {
                            if ws.send(Message::text(r.to_string())).is_err() {
                                return End::Retry("보내기 실패".into());
                            }
                        }
                    }
                    "cancel" => { cancelled.lock().insert(m["id"].as_u64().unwrap_or(0)); }
                    _ => {}
                }
            }
            Ok(Message::Close(f)) => {
                let code = f.as_ref().map(|f| u16::from(f.code)).unwrap_or(1005);
                return match code {
                    4001 => End::Replaced,
                    4003 => End::Fatal(crate::i18n::tr("클라우드에서 이 연결 열쇠를 지웠습니다. 주인 화면에서 새로 만들어 넣어 주세요.")),
                    4008 => End::Fatal(crate::i18n::tr("클라우드와 설치판의 연결 방식(계약 버전)이 다릅니다. 설치판을 새 버전으로 올려 주세요.")),
                    _ => End::Retry(crate::i18n::tr_args("클라우드가 연결을 닫았습니다 ({1}).", &[&code])),
                };
            }
            Ok(_) => { last_seen = Instant::now(); }
            Err(tungstenite::Error::Io(e)) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
            Err(e) => return End::Retry(e.to_string()),
        }
        if !online_marked && last_seen.elapsed() > Duration::from_secs(15) {
            return End::Retry(crate::i18n::tr("클라우드가 답하지 않습니다."));
        }
        // 2) 코어에서 바뀐 것
        loop {
            match rx.try_recv() {
                Ok(Ev::Live) => live = true,
                Ok(Ev::Draw) => draw = true,
                Ok(Ev::Ui) => ui = true,
                Ok(Ev::Outputs) => {}
                Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_)) => { live = true; draw = true; ui = true; }
                Err(tokio::sync::broadcast::error::TryRecvError::Empty) => break,
                Err(tokio::sync::broadcast::error::TryRecvError::Closed) => return End::Retry("코어 닫힘".into()),
            }
        }
        // 3) 보내기 (바뀐 것만)
        if online_marked && (live || draw || ui) {
            let frames = collect(&core.lock(), &mut sent, live || ui, draw, ui);
            live = false; draw = false; ui = false;
            for f in frames {
                if let Err(e) = ws.send(Message::text(f)) {
                    return End::Retry(e.to_string());
                }
                STATUS.lock().sent += 1;
            }
        }
        // 4) 미디어 스레드가 보낸 조각
        for _ in 0..8 {
            match out_rx.try_recv() {
                Ok(Out::Text(t)) => { if ws.send(Message::text(t)).is_err() { return End::Retry("보내기 실패".into()); } }
                Ok(Out::Bin(b)) => { if ws.send(Message::binary(b)).is_err() { return End::Retry("보내기 실패".into()); } }
                Err(_) => break,
            }
        }
        // 5) 살아 있는지 (25초마다 글 'ping' → 중계실이 깨지 않고 'pong'). 70초 아무것도 없으면 다시 잇는다
        if last_ping.elapsed() > Duration::from_secs(25) {
            last_ping = Instant::now();
            if ws.send(Message::text("ping")).is_err() {
                return End::Retry("보내기 실패".into());
            }
        }
        if last_seen.elapsed() > Duration::from_secs(70) {
            return End::Retry(crate::i18n::tr("클라우드에서 오래 답이 없어 다시 잇습니다."));
        }
        // 6) 설정이 바뀌었으면 (1초마다 본다)
        if last_conf.elapsed() > Duration::from_secs(1) {
            last_conf = Instant::now();
            if conf(&core.lock()) != *cf {
                let _ = ws.close(None);
                let _ = ws.flush();
                return End::Settings;
            }
        }
    }
}

/// 클라우드로 보낼 화면 이름: main · overlay · 추가 출력 · stage
pub fn clients(core: &Core) -> Vec<String> {
    let mut v = crate::outputs::all_ids(core.settings.get());
    v.push("stage".into());
    v
}

/// 비교용 글: 매번 바뀌는 타이머 칸(now · remaining)을 끝 시각으로 바꾼다 (무대 화면 `timer` · 리모컨 `stage.timer`).
/// 화면 쪽은 받은 끝 시각으로 스스로 줄여 그리므로, 타이머가 도는 것만으로는 다시 보내지 않는다.
fn tick_key(v: &Value) -> String {
    fn mask(t: &mut Value) {
        if t.is_object() && t.get("now").is_some() {
            let end = if t["running"] == true { (t["now"].as_i64().unwrap_or(0) + t["remaining"].as_i64().unwrap_or(0)) / 1000 } else { t["remaining"].as_i64().unwrap_or(0) };
            *t = json!({ "running": t["running"], "duration": t["duration"], "end": end });
        }
    }
    let mut k = v.clone();
    mask(&mut k["timer"]);
    mask(&mut k["stage"]["timer"]);
    k.to_string()
}

/// 보낼 글 프레임들 (바뀐 것만)
fn collect(core: &Core, sent: &mut Sent, renders: bool, draw: bool, remote: bool) -> Vec<String> {
    let mut out = vec![];
    if renders {
        for c in clients(core) {
            let data = if c == "stage" { core.stage_payload() } else { core.output_payload(&c) };
            let k = tick_key(&data);
            if sent.render.get(&c) != Some(&k) {
                out.push(format!(r#"{{"t":"render","client":{},"data":{data}}}"#, json!(c)));
                sent.render.insert(c, k);
            }
        }
    }
    if draw {
        let s = core.draw_json().to_string();
        if sent.draw != s {
            out.push(format!(r#"{{"t":"draw","data":{s}}}"#));
            sent.draw = s;
        }
    }
    if remote {
        let v = crate::remote::state_for(core);
        let k = tick_key(&v);
        if sent.remote != k {
            out.push(format!(r#"{{"t":"remote","data":{v}}}"#));
            sent.remote = k;
        }
    }
    out
}

/// 출력 화면의 영상 알림 (설치판 /api/command 의 videoMeta · videoEnded 와 같게)
fn view_command(core: &Shared, m: &Value) {
    let key = m["args"]["key"].as_str().unwrap_or("");
    let d = m["args"]["duration"].as_f64();
    match m["cmd"].as_str() {
        Some("videoMeta") => core.lock().video_meta(key, d),
        Some("videoEnded") => core.lock().video_ended(key, d),
        _ => {}
    }
}

fn res(id: u64, status: u16, body: Value) -> Value {
    json!({ "t": "res", "id": id, "status": status, "headers": { "content-type": "application/json; charset=utf-8" }, "body": body })
}

/// 미디어 길 (클라우드 protocol.mjs mediaPath · 설치판 lan.rs allowed 와 같은 범위)
pub fn media_path(path: &str) -> bool {
    (path.starts_with("/media/") && path.len() > 7 && !path[7..].contains('/'))
        || path == "/lib/file"
        || path.starts_with("/hymn/")
        || path.starts_with("/hymnal/score/")
}

/// 요청 하나. JSON 답은 바로 돌려주고(Some), 미디어는 다른 스레드가 out 으로 보낸다(None)
fn handle_request(core: &Shared, cf: &Conf, m: &Value, out: &mpsc::SyncSender<Out>, cancelled: &Arc<Mutex<HashSet<u64>>>) -> Option<Value> {
    let id = m["id"].as_u64().unwrap_or(0);
    let method = m["method"].as_str().unwrap_or("GET");
    let path = m["path"].as_str().unwrap_or("");
    let q = |k: &str| m["query"][k].as_str().unwrap_or("").to_string();
    let body = &m["body"];
    if body.to_string().len() > BODY_MAX {
        return Some(res(id, 413, json!({ "ok": false, "error": "너무 큽니다." })));
    }
    let deny = |what: &str| Some(res(id, 403, json!({ "ok": false, "error": crate::i18n::tr_args("설치판의 클라우드 연결 설정에서 {1} 을(를) 꺼 두었습니다.", &[&crate::i18n::tr(what)]) })));
    match (method, path) {
        ("GET", "/r/state") => {
            if !cf.remote { return deny("리모컨"); }
            let mut v = crate::remote::state_for(&core.lock());
            v["ok"] = json!(true);
            Some(res(id, 200, v))
        }
        ("POST", "/r/do") => {
            if !cf.remote { return deny("리모컨"); }
            let r = crate::remote::act(&mut core.lock(), body);
            Some(res(id, 200, match r { Ok(r) => json!({ "ok": true, "result": r }), Err(e) => json!({ "ok": false, "error": e }) }))
        }
        ("GET", "/e/state" | "/e/cue" | "/e/songs") | ("POST", "/e/save" | "/e/new" | "/e/song") => {
            if !cf.editor { return deny("웹 편집기"); }
            let mut c = core.lock();
            let r: Result<Value, String> = match path {
                "/e/state" => Ok(crate::webedit::list(&c)),
                "/e/cue" => crate::webedit::cue_of(&c, &q("id")).map(|cue| json!({ "cue": cue })).ok_or_else(|| "큐시트를 찾을 수 없습니다.".to_string()),
                "/e/songs" => Ok(json!({ "songs": c.list_songs(&q("q")).into_iter().take(40).map(|s| json!({ "id": s["id"], "title": s["title"], "credit": s["credit"], "key": s["key"] })).collect::<Vec<_>>() })),
                "/e/save" => crate::webedit::save(&mut c, body).map(|cue| json!({ "cue": cue })),
                "/e/new" => crate::webedit::create(&mut c, body["name"].as_str().unwrap_or("")).map(|cue| json!({ "cue": cue })),
                _ => c.song_item(body["id"].as_str().unwrap_or("")).map(|it| json!({ "item": it })).ok_or_else(|| "곡을 찾을 수 없습니다.".to_string()),
            };
            if method == "POST" && r.is_ok() {
                log::info!("클라우드 웹 편집기: {path}");
            }
            Some(match r {
                Ok(mut v) => { v["ok"] = json!(true); res(id, 200, v) }
                Err(e) if e == "conflict" => res(id, 409, json!({ "ok": false, "conflict": true, "error": crate::i18n::tr("그 사이 다른 곳(조작 화면이나 다른 브라우저)에서 이 큐시트를 고쳤습니다. 다시 불러온 뒤 고쳐 주세요.") })),
                Err(e) if path == "/e/cue" => res(id, 404, json!({ "ok": false, "error": crate::i18n::tr(&e) })),
                Err(e) => res(id, 200, json!({ "ok": false, "error": crate::i18n::tr(&e) })),
            })
        }
        ("GET", p) if media_path(p) => {
            let port = core.lock().port;
            let mut url = format!("http://127.0.0.1:{port}{p}");
            if let Some(obj) = m["query"].as_object().filter(|o| !o.is_empty()) {
                let qs: Vec<String> = obj.iter().map(|(k, v)| format!("{}={}", enc(k), enc(v.as_str().unwrap_or("")))).collect();
                url.push('?');
                url.push_str(&qs.join("&"));
            }
            let range = m["headers"]["range"].as_str().map(String::from);
            let (out, cancelled) = (out.clone(), cancelled.clone());
            let _ = std::thread::Builder::new().name("wp-cloud-media".into()).spawn(move || media(id, &url, range, out, cancelled));
            None
        }
        _ => Some(res(id, 404, json!({ "ok": false, "error": "없는 주소입니다." }))),
    }
}

fn enc(s: &str) -> String {
    s.bytes().map(|c| if c.is_ascii_alphanumeric() || b"-_.~".contains(&c) { (c as char).to_string() } else { format!("%{c:02X}") }).collect()
}

/// 이진 조각 머리: 요청 번호(u32 BE) + 표시(1 끝 · 2 중단)
pub fn chunk(id: u64, flags: u8, bytes: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(5 + bytes.len());
    v.extend_from_slice(&(id as u32).to_be_bytes());
    v.push(flags);
    v.extend_from_slice(bytes);
    v
}

/// 미디어 하나를 로컬 서버에서 읽어 조각으로 (다른 스레드)
fn media(id: u64, url: &str, range: Option<String>, out: mpsc::SyncSender<Out>, cancelled: Arc<Mutex<HashSet<u64>>>) {
    let agent: ureq::Agent = ureq::Agent::config_builder().http_status_as_error(false).timeout_global(Some(Duration::from_secs(600))).build().into();
    let mut r = agent.get(url);
    if let Some(rg) = &range {
        r = r.header("range", rg);
    }
    let mut resp = match r.call() {
        Ok(x) => x,
        Err(e) => {
            let _ = out.send(Out::Text(res(id, 502, json!({ "ok": false, "error": e.to_string() })).to_string()));
            return;
        }
    };
    let status = resp.status().as_u16();
    let mut headers = serde_json::Map::new();
    for k in ["content-type", "content-length", "content-range", "accept-ranges"] {
        if let Some(v) = resp.headers().get(k).and_then(|v| v.to_str().ok()) {
            headers.insert(k.into(), json!(v));
        }
    }
    if out.send(Out::Text(json!({ "t": "res", "id": id, "status": status, "headers": headers, "stream": true }).to_string())).is_err() {
        return;
    }
    let mut reader = resp.body_mut().as_reader();
    let mut buf = vec![0u8; CHUNK];
    loop {
        if cancelled.lock().remove(&id) {
            return;
        }
        // 조각을 가득 채워 보낸다 (작은 조각이 많으면 메시지 수가 늘어난다)
        let mut n = 0;
        let mut eof = false;
        while n < CHUNK {
            match reader.read(&mut buf[n..]) {
                Ok(0) => { eof = true; break; }
                Ok(k) => n += k,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => { let _ = out.send(Out::Bin(chunk(id, 2, &[]))); return; }
            }
        }
        if out.send(Out::Bin(chunk(id, if eof { 1 } else { 0 }, &buf[..n]))).is_err() || eof {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_core() -> (Core, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("wp-cloud-{}-{}", std::process::id(), crate::util::short_id("t")));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("bible.db"), b"").unwrap();
        (Core::open(dir.clone(), dir.clone(), None).unwrap(), dir)
    }

    #[test]
    fn url_and_key_rules() {
        assert_eq!(bridge_url("https://wp.example.workers.dev/").unwrap(), "wss://wp.example.workers.dev/bridge");
        assert_eq!(bridge_url("http://127.0.0.1:8797").unwrap(), "ws://127.0.0.1:8797/bridge");
        assert!(bridge_url("http://example.com").is_err(), "인터넷 너머 http 는 열쇠가 그대로 보이므로 막는다");
        assert!(bridge_url("ftp://x").is_err() && bridge_url("").is_err());
        assert!(key_ok("sample.AbCdEfGhJkMnPqRsTuVwXyZ23"));
        for bad in ["sample", ".abcdefghijklmnopqrstu", "Sample.abcdefghijklmnopqrstu", "sample.short", "-a.abcdefghijklmnopqrstu", "sample.abcdefghij klmnopqrstu"] {
            assert!(!key_ok(bad), "{bad}");
        }
        assert!(media_path("/lib/file") && media_path("/media/a.png") && media_path("/hymn/k/1.svg") && media_path("/hymnal/score/b/1.png"));
        assert!(!media_path("/api/state") && !media_path("/media/a/b") && !media_path("/media/"));
        assert_eq!(chunk(0x01020304, 1, &[9]), vec![1, 2, 3, 4, 1, 9]);
    }

    #[test]
    fn off_by_default_and_key_is_not_in_settings_text() {
        let (mut c, dir) = temp_core();
        let cf = conf(&c);
        assert!(!cf.enabled && cf.url.is_empty() && cf.key.is_empty(), "기본 꺼짐");
        assert!(cf.remote && cf.editor);
        set_key(&mut c, "sample.AbCdEfGhJkMnPqRsTuVwXyZ23");
        assert!(has_key(&c));
        if cfg!(windows) {
            assert!(!c.settings.get().to_string().contains("AbCdEfGhJkMnPqRsTuVwXyZ23"), "열쇠 원문이 설정 파일에 없다");
        }
        assert!(!crate::extcmd::state_json(&c).to_string().contains("AbCdEfGh"), "/api/state 에도 없다");
        set_key(&mut c, "");
        assert!(!has_key(&c));
        drop(c);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn sends_everything_once_then_only_changes() {
        let (mut c, dir) = temp_core();
        let mut sent = Sent::default();
        let first = collect(&c, &mut sent, true, true, true);
        let kinds: Vec<String> = first.iter().map(|f| serde_json::from_str::<Value>(f).unwrap()["t"].as_str().unwrap().to_string()).collect();
        assert_eq!(kinds.iter().filter(|k| *k == "render").count(), 3, "main · overlay · stage");
        assert!(kinds.contains(&"draw".to_string()) && kinds.contains(&"remote".to_string()));
        let f0: Value = serde_json::from_str(&first[0]).unwrap();
        assert_eq!(f0["data"], c.output_payload("main"), "로컬 SSE 와 같은 출력 자료");
        assert!(collect(&c, &mut sent, true, true, true).is_empty(), "바뀐 것이 없으면 보내지 않는다");
        // 무대 메시지를 바꾸면 무대 화면 · 리모컨만 다시
        c.stage_message("설교 5분 남음");
        let again = collect(&c, &mut sent, true, true, true);
        assert!(!again.is_empty());
        assert!(again.iter().all(|f| !f.contains(r#""client":"main""#)), "예배당 화면은 그대로");
        drop(c);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn requests_follow_contract_and_settings() {
        let (mut c, dir) = temp_core();
        c.new_cue("주일").unwrap();
        let shared: Shared = Arc::new(crate::lockcheck::Mutex::new(c));
        let (tx, _rx) = mpsc::sync_channel(4);
        let cancel = Arc::new(Mutex::new(HashSet::new()));
        let on = Conf { enabled: true, url: "https://x".into(), key: "k".into(), remote: true, editor: true };
        let r = handle_request(&shared, &on, &json!({ "id": 7, "method": "GET", "path": "/r/state", "query": {} }), &tx, &cancel).unwrap();
        assert_eq!((r["t"].as_str(), r["id"].as_u64(), r["status"].as_u64(), r["body"]["ok"].as_bool()), (Some("res"), Some(7), Some(200), Some(true)));
        let r = handle_request(&shared, &on, &json!({ "id": 8, "method": "POST", "path": "/r/do", "body": { "action": "message", "text": "클라우드에서" } }), &tx, &cancel).unwrap();
        assert_eq!(r["body"]["ok"], true);
        assert_eq!(shared.lock().b.stage.message, "클라우드에서", "리모컨 조작이 설치판으로 돌아온다");
        let st = handle_request(&shared, &on, &json!({ "id": 9, "method": "GET", "path": "/e/state", "query": {} }), &tx, &cancel).unwrap();
        let id = st["body"]["active"].as_str().unwrap().to_string();
        let cue = handle_request(&shared, &on, &json!({ "id": 10, "method": "GET", "path": "/e/cue", "query": { "id": id } }), &tx, &cancel).unwrap();
        let base = cue["body"]["cue"]["updatedAt"].clone();
        std::thread::sleep(Duration::from_millis(5));
        let save = handle_request(&shared, &on, &json!({ "id": 11, "method": "POST", "path": "/e/save", "body": { "id": id, "base": base, "name": "주일 2부", "items": [ { "type": "title", "title": "환영" } ] } }), &tx, &cancel).unwrap();
        assert_eq!(save["status"], 200, "{save}");
        assert_eq!(shared.lock().cue.as_ref().unwrap().name, "주일 2부");
        std::thread::sleep(Duration::from_millis(5));
        let again = handle_request(&shared, &on, &json!({ "id": 12, "method": "POST", "path": "/e/save", "body": { "id": id, "base": base, "items": [] } }), &tx, &cancel).unwrap();
        assert_eq!((again["status"].as_u64(), again["body"]["conflict"].as_bool()), (Some(409), Some(true)), "충돌은 409 그대로");
        // 계약 밖의 길 · 설치판 전용 길은 받지 않는다
        for p in ["/api/state", "/api/live", "/dock", "/api/trigger/next", "/helper/cameras"] {
            assert_eq!(handle_request(&shared, &on, &json!({ "id": 1, "method": "GET", "path": p }), &tx, &cancel).unwrap()["status"], 404, "{p}");
        }
        assert_eq!(handle_request(&shared, &on, &json!({ "id": 1, "method": "POST", "path": "/api/command", "body": { "cmd": "clearAll" } }), &tx, &cancel).unwrap()["status"], 404);
        // 설정에서 끄면 403
        let off = Conf { remote: false, editor: false, ..on.clone() };
        assert_eq!(handle_request(&shared, &off, &json!({ "id": 2, "method": "POST", "path": "/r/do", "body": { "action": "next" } }), &tx, &cancel).unwrap()["status"], 403);
        assert_eq!(handle_request(&shared, &off, &json!({ "id": 3, "method": "GET", "path": "/e/state" }), &tx, &cancel).unwrap()["status"], 403);
        drop(shared);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn tick_key_ignores_ticking_timer() {
        let a = json!({ "stage": { "timer": { "running": true, "remaining": 5000, "now": 1000, "duration": 60000 } }, "live": { "index": 1 } });
        let b = json!({ "stage": { "timer": { "running": true, "remaining": 4000, "now": 2000, "duration": 60000 } }, "live": { "index": 1 } });
        assert_eq!(tick_key(&a), tick_key(&b));
        let c = json!({ "stage": { "timer": { "running": true, "remaining": 4000, "now": 2000, "duration": 60000 } }, "live": { "index": 2 } });
        assert_ne!(tick_key(&a), tick_key(&c));
        let s1 = json!({ "timer": { "running": true, "remaining": 5000, "now": 1000, "duration": 1 } });
        let s2 = json!({ "timer": { "running": true, "remaining": 3000, "now": 3000, "duration": 1 } });
        assert_eq!(tick_key(&s1), tick_key(&s2), "무대 화면 타이머도");
    }
}
