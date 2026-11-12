//! 내부망 출력 (Electron 판 server.js lanGate): 다른 PC(송출 PC 의 OBS)에서 출력 화면만 열기
//! 0.0.0.0:<포트>(기본 37550). 열쇠(key)가 주소(?key=) 나 쿠키(wclan) 로 맞아야 하고, 출력 화면에 필요한 주소만 열린다.

use crate::core::{Core, Ev, Shared};
use crate::trigger::safe_eq;
use axum::body::Body;
use axum::extract::State;
use axum::http::{header, HeaderValue, Method, Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde_json::{json, Value};

pub const DEFAULT_PORT: u16 = 37550;
const COOKIE: &str = "wclan";

struct Run {
    port: u16,
    stop: tokio::sync::oneshot::Sender<()>,
}
static RUN: Lazy<Mutex<Option<Run>>> = Lazy::new(|| Mutex::new(None));
static ERROR: Lazy<Mutex<String>> = Lazy::new(|| Mutex::new(String::new()));

fn pct_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Some(v) = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(if b[i] == b'+' { b' ' } else { b[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

fn pct_encode(s: &str) -> String {
    s.bytes().map(|c| if c.is_ascii_alphanumeric() || b"-_.~".contains(&c) { (c as char).to_string() } else { format!("%{c:02X}") }).collect()
}

/// 이 요청이 내부망에서 열려도 되는 길인가 (Electron lanGate 와 같은 규칙)
pub fn allowed(method: &Method, path: &str, query_client: Option<&str>) -> bool {
    if method == Method::GET {
        if path == "/events" {
            return matches!(query_client, Some("main" | "overlay" | "stage"));
        }
        let out_page = matches!(path, "/outputs/main" | "/outputs/overlay" | "/outputs/stage");
        let out_asset = path.strip_prefix("/outputs/").is_some_and(|f| !f.contains('/') && (f.ends_with(".js") || f.ends_with(".css")) && f.chars().all(|c| c.is_alphanumeric() || "._-".contains(c)));
        return out_page || out_asset || path.starts_with("/shared/") || path.starts_with("/media/") || path == "/lib/file" || path.starts_with("/hymn/") || path.starts_with("/hymnal/score/");
    }
    method == Method::POST && (path == "/api/ack" || path == "/api/command")
}

fn query_get(q: &str, k: &str) -> Option<String> {
    q.split('&').find_map(|kv| kv.split_once('=').filter(|(a, _)| *a == k).map(|(_, v)| pct_decode(v)))
}

async fn gate(State(core): State<Shared>, req: Request<Body>, next: Next) -> Response {
    let want = core.lock().settings.str(&["lanOutput", "key"]);
    let q = req.uri().query().unwrap_or("").to_string();
    let from_url = query_get(&q, "key").unwrap_or_default();
    let from_cookie = req
        .headers()
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|c| c.split(';').find_map(|p| p.trim().strip_prefix(&format!("{COOKIE}=")).map(pct_decode)))
        .unwrap_or_default();
    let ok = safe_eq(&from_url, &want) || safe_eq(&from_cookie, &want);
    let path = pct_decode(req.uri().path());
    if !ok || !allowed(req.method(), &path, query_get(&q, "client").as_deref()) {
        return (StatusCode::FORBIDDEN, [(header::CONTENT_TYPE, "text/plain; charset=utf-8")], "Worship Presenter: key required (설정 → 출력 → 내부망 출력의 주소를 그대로 쓰세요)").into_response();
    }
    // /api/command 는 출력 화면이 알리는 '영상 끝남' 만 (server.rs command 가 그 밖은 거절)
    let set_cookie = !from_url.is_empty();
    let mut res = next.run(req).await;
    if set_cookie {
        if let Ok(v) = HeaderValue::from_str(&format!("{COOKIE}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age=31536000", pct_encode(&want))) {
            res.headers_mut().insert(header::SET_COOKIE, v);
        }
    }
    res
}

pub fn router(core: Shared) -> axum::Router {
    crate::server::output_routes(core.clone()).layer(middleware::from_fn_with_state(core.clone(), gate)).with_state(core)
}

pub fn status(core: &Core) -> Value {
    let s = &core.settings.get()["lanOutput"];
    let run = RUN.lock();
    let port = run.as_ref().map(|r| r.port).unwrap_or(s["port"].as_u64().unwrap_or(DEFAULT_PORT as u64) as u16);
    let key = pct_encode(s["key"].as_str().unwrap_or(""));
    let addrs = crate::remote::lan_addresses();
    let mk = |p: &str| if run.is_some() { addrs.iter().map(|a| format!("http://{a}:{port}{p}?key={key}")).collect::<Vec<_>>() } else { vec![] };
    json!({ "enabled": s["enabled"].as_bool().unwrap_or(false), "running": run.is_some(), "port": port, "error": ERROR.lock().clone(),
        "main": mk("/outputs/main"), "overlay": mk("/outputs/overlay"), "stage": mk("/outputs/stage") })
}

/// 새 열쇠 (lanOutputInfo · newLanKey)
pub fn new_key(core: &mut Core) -> Value {
    let b: [u8; 12] = rand::random();
    let k = crate::dpapi::b64url(&b);
    core.settings.update(|d| d["lanOutput"]["key"] = json!(k));
    core.emit(Ev::Ui);
    status(core)
}

pub fn info(core: &mut Core) -> Value {
    if core.settings.str(&["lanOutput", "key"]).is_empty() {
        return new_key(core);
    }
    status(core)
}

pub async fn sync(core: Shared) {
    let (want, host) = {
        let c = core.lock();
        let s = &c.settings.get()["lanOutput"];
        let p = if s["enabled"].as_bool().unwrap_or(false) { s["port"].as_u64().filter(|p| *p > 0).unwrap_or(DEFAULT_PORT as u64) as u16 } else { 0 };
        (p, s["host"].as_str().unwrap_or("0.0.0.0").to_string())
    };
    let cur = RUN.lock().as_ref().map(|r| r.port);
    if cur.is_some() && (want == 0 || Some(want) != cur) {
        if let Some(r) = RUN.lock().take() {
            let _ = r.stop.send(());
        }
        core.lock().emit(Ev::Outputs);
    }
    if want != 0 && RUN.lock().is_none() {
        if core.lock().settings.str(&["lanOutput", "key"]).is_empty() {
            new_key(&mut core.lock());
        }
        match tokio::net::TcpListener::bind((host.as_str(), want)).await {
            Ok(l) => {
                let (tx, rx) = tokio::sync::oneshot::channel();
                *RUN.lock() = Some(Run { port: want, stop: tx });
                ERROR.lock().clear();
                let app = router(core.clone());
                tokio::spawn(async move {
                    let _ = axum::serve(l, app).with_graceful_shutdown(async { let _ = rx.await; }).await;
                });
                log::info!("내부망 출력 서버 시작: {want}");
            }
            Err(e) => *ERROR.lock() = if e.kind() == std::io::ErrorKind::AddrInUse { format!("포트 {want} 를 다른 프로그램이 쓰고 있습니다.") } else { e.to_string() },
        }
        core.lock().emit(Ev::Outputs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rules() {
        assert!(allowed(&Method::GET, "/outputs/main", None));
        assert!(allowed(&Method::GET, "/outputs/output.js", None));
        assert!(!allowed(&Method::GET, "/outputs/../x.js", None));
        assert!(allowed(&Method::GET, "/events", Some("overlay")));
        assert!(!allowed(&Method::GET, "/events", Some("control")));
        assert!(!allowed(&Method::GET, "/api/state", None));
        assert!(!allowed(&Method::GET, "/dock", None));
        assert!(!allowed(&Method::GET, "/api/trigger/next", None));
        assert!(allowed(&Method::POST, "/api/ack", None));
        assert!(!allowed(&Method::POST, "/r/do", None));
        assert_eq!(pct_decode("a%2Bb+c"), "a+b c");
    }
}
