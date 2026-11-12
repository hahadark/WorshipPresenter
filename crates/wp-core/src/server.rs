//! 로컬 웹 서버 (127.0.0.1 전용) — 출력 화면만 웹으로 남긴다
//!  - /outputs/main      예배당 화면 (출력 창 · OBS)
//!  - /outputs/overlay   투명 자막 (OBS 브라우저 소스)
//!  - /outputs/stage     무대 화면
//!  - /events            Server-Sent Events (접속 즉시 최신 상태, 바뀔 때마다)
//! 주소·데이터 모양은 Electron 판과 같아서 OBS 설정을 바꿀 필요가 없다.

use crate::core::{Ev, OutputClient, Shared};
use crate::util::now_ms;
use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, HeaderValue, Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::stream::Stream;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::convert::Infallible;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static SEQ: AtomicU64 = AtomicU64::new(0);

fn mime(p: &Path) -> &'static str {
    match p.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).as_deref() {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("bmp") => "image/bmp",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        Some("mp3") => "audio/mpeg",
        Some("wav") => "audio/wav",
        Some("m4a") => "audio/mp4",
        Some("ogg") | Some("oga") | Some("opus") => "audio/ogg",
        Some("flac") => "audio/flac",
        Some("mp4") | Some("m4v") => "video/mp4",
        Some("webm") => "video/webm",
        Some("mov") => "video/quicktime",
        _ => "application/octet-stream",
    }
}

fn status(code: StatusCode) -> Response {
    (code, ()).into_response()
}

/// base 안의 파일만 보낸다 (../ 로 빠져나가지 못하게)
async fn send_file(base: &Path, rel: &str) -> Response {
    let rel = rel.trim_start_matches('/');
    if rel.split(['/', '\\']).any(|s| s == "..") {
        return status(StatusCode::FORBIDDEN);
    }
    let file = base.join(rel);
    match tokio::fs::read(&file).await {
        Ok(buf) => ([(header::CONTENT_TYPE, mime(&file)), (header::CACHE_CONTROL, "no-cache")], buf).into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "Not found").into_response(),
    }
}

/// 라이브러리 파일: 등록한 폴더 · 끌어다 놓은 파일만 (Range 요청 지원 → 영상·소리 위치 이동)
fn allowed_lib_file(core: &crate::core::Core, p: &str) -> Option<PathBuf> {
    let f = PathBuf::from(p);
    if !f.is_absolute() || !f.is_file() {
        return None;
    }
    let s = core.settings.get();
    let norm = |x: &str| x.replace('/', "\\").to_lowercase();
    let fp = norm(p);
    for key in ["imageFolders", "audioFolders", "hymnFolders"] {
        for d in s[key].as_array().into_iter().flatten() {
            let dp = norm(d["path"].as_str().unwrap_or(""));
            if !dp.is_empty() && fp.starts_with(&(dp.trim_end_matches('\\').to_string() + "\\")) {
                return Some(f);
            }
        }
    }
    let dropped = s["droppedFiles"].as_array().into_iter().flatten().any(|d| d.as_str().map(norm).as_deref() == Some(fp.as_str()) || d["path"].as_str().map(norm).as_deref() == Some(fp.as_str()));
    let in_data = fp.starts_with(&norm(&core.data_dir.to_string_lossy()));
    if dropped || in_data { Some(f) } else { None }
}

async fn lib_file(State(core): State<Shared>, Query(q): Query<HashMap<String, String>>, headers: HeaderMap) -> Response {
    let Some(p) = q.get("p") else { return status(StatusCode::NOT_FOUND) };
    let Some(file) = allowed_lib_file(&core.lock(), p) else { return status(StatusCode::NOT_FOUND) };
    let Ok(meta) = tokio::fs::metadata(&file).await else { return status(StatusCode::NOT_FOUND) };
    let size = meta.len();
    let ty = mime(&file);
    let range = headers.get(header::RANGE).and_then(|v| v.to_str().ok()).and_then(|r| {
        let r = r.strip_prefix("bytes=")?;
        let (a, b) = r.split_once('-')?;
        Some((a.parse::<u64>().ok(), b.parse::<u64>().ok()))
    });
    use tokio::io::{AsyncReadExt, AsyncSeekExt};
    let Ok(mut fh) = tokio::fs::File::open(&file).await else { return status(StatusCode::NOT_FOUND) };
    match range {
        Some((a, b)) => {
            let start = match (a, b) { (Some(a), _) => a, (None, Some(n)) => size.saturating_sub(n), _ => 0 };
            // 한 번에 최대 8MB (브라우저는 이어서 다시 요청한다)
            let end = match (a, b) { (Some(_), Some(b)) => b.min(size.saturating_sub(1)), _ => size.saturating_sub(1) }.min(start + 8 * 1024 * 1024 - 1);
            if start > end || start >= size {
                return (StatusCode::RANGE_NOT_SATISFIABLE, [(header::CONTENT_RANGE, format!("bytes */{size}"))]).into_response();
            }
            let mut buf = vec![0u8; (end - start + 1) as usize];
            if fh.seek(std::io::SeekFrom::Start(start)).await.is_err() || fh.read_exact(&mut buf).await.is_err() {
                return status(StatusCode::INTERNAL_SERVER_ERROR);
            }
            (StatusCode::PARTIAL_CONTENT, [(header::CONTENT_TYPE, ty.to_string()), (header::CONTENT_RANGE, format!("bytes {start}-{end}/{size}")), (header::ACCEPT_RANGES, "bytes".into())], buf).into_response()
        }
        None => {
            let mut buf = Vec::with_capacity(size as usize);
            if fh.read_to_end(&mut buf).await.is_err() {
                return status(StatusCode::INTERNAL_SERVER_ERROR);
            }
            ([(header::CONTENT_TYPE, ty), (header::ACCEPT_RANGES, "bytes"), (header::CACHE_CONTROL, "max-age=60")], buf).into_response()
        }
    }
}

/// 접속이 끊기면 목록에서 지운다
struct ClientGuard {
    id: u64,
    core: Shared,
}
impl Drop for ClientGuard {
    fn drop(&mut self) {
        let mut c = self.core.lock();
        c.outputs.remove(&self.id);
        c.emit(Ev::Outputs);
    }
}

fn render_event(core: &crate::core::Core, client: &str) -> Event {
    let data = if client == "stage" { core.stage_payload() } else { core.output_payload(client) };
    Event::default().event("render").data(data.to_string())
}

async fn events(State(core): State<Shared>, Query(q): Query<HashMap<String, String>>, headers: HeaderMap) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let client = q.get("client").map(|s| s.as_str()).filter(|c| ["main", "overlay", "stage"].contains(c)).unwrap_or("main").to_string();
    let ua = headers.get(header::USER_AGENT).and_then(|v| v.to_str().ok()).unwrap_or("");
    let kind = if q.contains_key("window") { "window" } else if ua.to_lowercase().contains("obs") { "obs" } else { "browser" };
    let id = SEQ.fetch_add(1, Ordering::Relaxed) + 1;
    let (first, mut rx) = {
        let mut c = core.lock();
        c.outputs.insert(id, OutputClient { client: client.clone(), kind: kind.into(), connected_at: now_ms(), ack: None });
        c.emit(Ev::Outputs);
        (render_event(&c, &client), c.tx.subscribe())
    };
    let guard = ClientGuard { id, core: core.clone() };
    let stream = async_stream(move |yielder| async move {
        let _guard = guard;
        yielder.send(Ok(Event::default().retry(std::time::Duration::from_millis(1000)).comment("hi"))).await;
        yielder.send(Ok(first)).await;
        yielder.send(Ok(Event::default().event("hello").data(json!({ "id": id }).to_string()))).await;
        loop {
            match rx.recv().await {
                Ok(Ev::Live) => {
                    let ev = render_event(&core.lock(), &client);
                    if !yielder.send(Ok(ev)).await {
                        break;
                    }
                }
                Ok(_) => {}
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    let ev = render_event(&core.lock(), &client);
                    if !yielder.send(Ok(ev)).await {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    });
    Sse::new(stream).keep_alive(KeepAlive::new().interval(std::time::Duration::from_secs(15)))
}

/// 아주 작은 async 스트림 도우미 (mpsc 로 이어 줌)
struct Yielder(tokio::sync::mpsc::Sender<Result<Event, Infallible>>);
impl Yielder {
    async fn send(&self, e: Result<Event, Infallible>) -> bool {
        self.0.send(e).await.is_ok()
    }
}
fn async_stream<F, Fut>(f: F) -> impl Stream<Item = Result<Event, Infallible>>
where
    F: FnOnce(Yielder) -> Fut,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let (tx, rx) = tokio::sync::mpsc::channel(16);
    tokio::spawn(f(Yielder(tx)));
    tokio_stream::wrappers::ReceiverStream::new(rx)
}

async fn ack(State(core): State<Shared>, Json(b): Json<Value>) -> Json<Value> {
    let mut c = core.lock();
    if let Some(id) = b["id"].as_u64() {
        if let Some(o) = c.outputs.get_mut(&id) {
            o.ack = Some(json!({ "version": b["version"], "at": now_ms(), "width": b["width"], "height": b["height"], "visible": b["visible"], "latency": b["latency"] }));
            c.emit(Ev::Outputs);
        }
    }
    Json(json!({ "ok": true }))
}

async fn command(State(_core): State<Shared>, headers: HeaderMap, Json(b): Json<Value>) -> Response {
    if headers.get("x-worshipcue").and_then(|v| v.to_str().ok()) != Some("1") {
        return status(StatusCode::FORBIDDEN);
    }
    // 출력 화면이 부르는 명령: 영상 끝남 (아직 영상 송출은 다음 단계)
    match b["cmd"].as_str() {
        Some("videoEnded") => Json(json!({ "ok": true, "result": null })).into_response(),
        _ => Json(json!({ "ok": false, "error": "조작 명령은 Rust 판 조작 화면에서만 합니다." })).into_response(),
    }
}

/// 다른 컴퓨터 이름으로 들어온 요청은 거절 (DNS rebinding 막기)
async fn host_check(State(core): State<Shared>, req: Request<Body>, next: Next) -> Response {
    let port = core.lock().port;
    let host = req.headers().get(header::HOST).and_then(|v| v.to_str().ok()).unwrap_or("").to_lowercase();
    let ok = [format!("127.0.0.1:{port}"), format!("localhost:{port}"), format!("[::1]:{port}")].contains(&host);
    if !ok {
        return status(StatusCode::MISDIRECTED_REQUEST);
    }
    let mut res = next.run(req).await;
    res.headers_mut().insert("X-Content-Type-Options", HeaderValue::from_static("nosniff"));
    res
}

pub fn router(core: Shared) -> Router {
    let web = core.lock().web_dir.clone();
    let (w1, w2, w3, w4) = (web.clone(), web.clone(), web.clone(), web.clone());
    let media = core.lock().media_dir.clone();
    Router::new()
        .route("/", get(|| async { Redirect::temporary("/outputs/main") }))
        .route("/outputs/main", get(move || { let w = w1.clone(); async move { send_file(&w, "output/index.html").await } }))
        .route("/outputs/overlay", get(move || { let w = w2.clone(); async move { send_file(&w, "output/index.html").await } }))
        .route("/outputs/stage", get(move || { let w = w3.clone(); async move { send_file(&w, "output/stage.html").await } }))
        .route("/outputs/{*f}", get({ let w = web.clone(); move |axum::extract::Path(f): axum::extract::Path<String>| { let w = w.clone(); async move { send_file(&w.join("output"), &f).await } } }))
        .route("/shared/{*f}", get(move |axum::extract::Path(f): axum::extract::Path<String>| { let w = w4.clone(); async move { send_file(&w.join("shared"), &f).await } }))
        .route("/media/{f}", get(move |axum::extract::Path(f): axum::extract::Path<String>| { let m = media.clone(); async move { send_file(&m, &f).await } }))
        .route("/lib/file", get(lib_file))
        .route("/events", get(events))
        .route("/api/state", get(|State(c): State<Shared>| async move { Json(c.lock().output_payload("main")) }))
        .route("/api/live", get(|State(c): State<Shared>| async move { Json(c.lock().pres.live_state()) }))
        .route("/api/ack", post(ack))
        .route("/api/command", post(command))
        .layer(middleware::from_fn_with_state(core.clone(), host_check))
        .with_state(core)
}

/// 서버를 띄운다. 포트가 쓰이고 있으면 다음 포트로 (최대 10번)
pub async fn serve(core: Shared) -> anyhow::Result<u16> {
    let want = core.lock().port;
    let mut last = None;
    for p in want..want + 10 {
        match tokio::net::TcpListener::bind(("127.0.0.1", p)).await {
            Ok(l) => {
                core.lock().port = p;
                let app = router(core.clone());
                tokio::spawn(async move {
                    if let Err(e) = axum::serve(l, app).await {
                        log::error!("서버 오류: {e}");
                    }
                });
                return Ok(p);
            }
            Err(e) => last = Some(e),
        }
    }
    Err(anyhow::anyhow!("포트를 열 수 없습니다: {:?}", last))
}
