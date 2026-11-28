//! 로컬 웹 서버 (127.0.0.1 전용) — 출력 화면만 웹으로 남긴다
//!  - /outputs/main      예배당 화면 (출력 창 · OBS)
//!  - /outputs/overlay   투명 자막 (OBS 브라우저 소스)
//!  - /outputs/stage     무대 화면
//!  - /events            Server-Sent Events (접속 즉시 최신 상태, 바뀔 때마다)
//! 주소·데이터 모양은 Electron 판과 같아서 OBS 설정을 바꿀 필요가 없다.

use crate::core::{Ev, OutputClient, Shared};
#[cfg(target_arch = "wasm32")]
use crate::web::tokio_shim as tokio; // [웹판] fs · net 자리 메움
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
    if !safe_relative_path(rel) {
        return status(StatusCode::FORBIDDEN);
    }
    let (Ok(root), Ok(file)) = (tokio::fs::canonicalize(base).await, tokio::fs::canonicalize(base.join(rel)).await) else {
        return status(StatusCode::NOT_FOUND);
    };
    if !file.starts_with(&root) {
        return status(StatusCode::FORBIDDEN);
    }
    match tokio::fs::read(&file).await {
        Ok(buf) => { let buf = if rel.ends_with(".html") { crate::i18n::with_lang(buf) } else { buf }; ([(header::CONTENT_TYPE, mime(&file)), (header::CACHE_CONTROL, "no-cache")], buf).into_response() } // [트랙 D] 영어 화면
        Err(_) => (StatusCode::NOT_FOUND, "Not found").into_response(),
    }
}

/// 드라이브·UNC·상위 폴더·NTFS 대체 스트림은 정적 파일 주소로 받지 않는다.
fn safe_relative_path(rel: &str) -> bool {
    !rel.is_empty() && !rel.contains(':') && !rel.contains('\0')
        && !rel.starts_with('\\')
        && !rel.split(['/', '\\']).any(|s| s == "..")
        && !Path::new(rel).is_absolute()
        && Path::new(rel).components().all(|c| matches!(c, std::path::Component::Normal(_) | std::path::Component::CurDir))
}

fn inside_folder(file: &Path, folder: &Path) -> bool {
    std::fs::canonicalize(folder).ok().is_some_and(|root| file.starts_with(root))
}

/// 라이브러리 파일: 등록한 폴더 · 끌어다 놓은 파일만. 실제 경로와 폴더 경계를 비교한다.
fn allowed_lib_file(core: &crate::core::Core, p: &str) -> Option<PathBuf> {
    let candidate = PathBuf::from(p);
    if !candidate.is_absolute() || p.contains('\0') {
        return None;
    }
    let f = std::fs::canonicalize(&candidate).ok()?;
    if !f.is_file() || !(crate::library::is_image(&f) || crate::library::is_audio(&f) || crate::library::is_video(&f)) {
        return None;
    }
    let s = core.settings.get();
    for key in ["imageFolders", "audioFolders", "hymnFolders"] {
        for d in s[key].as_array().into_iter().flatten() {
            if let Some(folder) = d["path"].as_str().filter(|s| !s.is_empty()) {
                if inside_folder(&f, Path::new(folder)) { return Some(f); }
            }
        }
    }
    let dropped = s["droppedFiles"].as_array().into_iter().flatten().any(|d| {
        d.as_str().or_else(|| d["path"].as_str())
            .and_then(|p| std::fs::canonicalize(p).ok()).as_ref() == Some(&f)
    });
    if dropped || inside_folder(&f, &core.data_dir) { Some(f) } else { None }
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
            let end = match (a, b) { (Some(_), Some(b)) => b.min(size.saturating_sub(1)), _ => size.saturating_sub(1) }.min(start.saturating_add(8 * 1024 * 1024 - 1));
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
            // 대용량 영상을 통째로 메모리에 올리지 않고 64KB씩 보낸다.
            let chunks = futures_util::stream::try_unfold(fh, |mut file| async move {
                let mut buf = vec![0u8; 64 * 1024];
                let n = file.read(&mut buf).await?;
                if n == 0 { return Ok::<_, std::io::Error>(None); }
                buf.truncate(n);
                Ok(Some((buf, file)))
            });
            ([(header::CONTENT_TYPE, ty.to_string()), (header::CONTENT_LENGTH, size.to_string()), (header::ACCEPT_RANGES, "bytes".into()), (header::CACHE_CONTROL, "max-age=60".into())], Body::from_stream(chunks)).into_response()
        }
    }
}

/// 베들레헴 찬송가 악보 그림 // [트랙 A]
async fn hymnal_score(State(core): State<Shared>, axum::extract::Path((book, file)): axum::extract::Path<(String, String)>) -> Response {
    let n: Option<i64> = file.strip_suffix(".png").and_then(|x| x.parse().ok());
    let png = n.and_then(|n| crate::hymnal::score(&core.lock(), &book, n));
    match png {
        Some(b) => ([(header::CONTENT_TYPE, "image/png"), (header::CACHE_CONTROL, "max-age=86400")], b).into_response(),
        None => status(StatusCode::NOT_FOUND),
    }
}

/// 찬송가 캐시 그림 (SVG 면 안의 그림을 data: 로 넣어서) // [트랙 A]
async fn hymn_file(State(core): State<Shared>, axum::extract::Path((key, file)): axum::extract::Path<(String, String)>) -> Response {
    let Some(fp) = crate::hymns::cache_path(&core.lock(), &key, &file) else { return status(StatusCode::NOT_FOUND) };
    if let Some(svg) = crate::hymns::svg_to_send(&fp) {
        return ([(header::CONTENT_TYPE, "image/svg+xml"), (header::CACHE_CONTROL, "no-cache")], svg).into_response();
    }
    let dir = fp.parent().unwrap().to_path_buf();
    send_file(&dir, &file).await
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
    // 추가 출력(out-…, 로드맵 3단계)은 설정에 있을 때만. 모르는 이름은 예전처럼 예배당
    let client = q.get("client").map(|s| s.as_str()).filter(|c| ["main", "overlay", "stage"].contains(c) || crate::outputs::find_extra(core.lock().settings.get(), c).is_some()).unwrap_or("main").to_string();
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
        // [트랙 B] 그리기·강조: 접속하면 지금 것, 바뀌면 'draw' (무대 화면은 빼고)
        if client != "stage" { let d = core.lock().draw_json(); if d["strokes"].as_array().is_some_and(|a| !a.is_empty()) || !d["spot"].is_null() { yielder.send(Ok(Event::default().event("draw").data(d.to_string()))).await; } }
        loop {
            match rx.recv().await {
                Ok(Ev::Draw) if client != "stage" => { // [트랙 B]
                    let d = core.lock().draw_json().to_string();
                    if !yielder.send(Ok(Event::default().event("draw").data(d))).await { break; }
                }
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
    let sse = Sse::new(stream);
    #[cfg(not(target_arch = "wasm32"))]
    let sse = sse.keep_alive(KeepAlive::new().interval(std::time::Duration::from_secs(15))); // [웹판] keep_alive 는 소켓 서버에만
    sse
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

/// 조작 서버(127.0.0.1 · Host 검사를 지난 요청)라는 표시. 내부망 출력 서버는 같은 길(output_routes)을 쓰지만 이 표시가 없다.
#[derive(Clone, Copy)]
pub struct LocalControl;

async fn command(State(core): State<Shared>, headers: HeaderMap, local: Option<axum::Extension<LocalControl>>, Json(b): Json<Value>) -> Response {
    if headers.get("x-worshipcue").and_then(|v| v.to_str().ok()) != Some("1") {
        return status(StatusCode::FORBIDDEN);
    }
    // 출력 화면이 부르는 명령: 영상 메타데이터·끝남
    match b["cmd"].as_str() {
        Some("videoMeta") => { core.lock().video_meta(b["args"]["key"].as_str().unwrap_or(""), b["args"]["duration"].as_f64()); Json(json!({ "ok": true, "result": null })).into_response() }
        Some("videoEnded") => { core.lock().video_ended(b["args"]["key"].as_str().unwrap_or(""), b["args"]["duration"].as_f64()); Json(json!({ "ok": true, "result": null })).into_response() } // [트랙 B]
        // 카메라 도우미(/helper/cameras)가 보낸 장치 목록: 일회용 토큰·개수·길이를 검사한다 (cameras::parse_devices)
        Some("cameraDevices") => match core.lock().receive_camera_devices(&b["args"]) {
            Ok(n) => Json(json!({ "ok": true, "result": { "count": n } })).into_response(),
            Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "ok": false, "error": e }))).into_response(),
        },
        // 카메라 동시 열기 시험 페이지(/helper/camtest)의 알림: 조작 서버에서만, 일회용 토큰 검사 (camtest::report)
        Some("camTestReport") if local.is_some() => match crate::camtest::report(&b["args"]) {
            Ok(()) => { core.lock().emit(Ev::Ui); Json(json!({ "ok": true, "result": null })).into_response() }
            Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "ok": false, "error": e }))).into_response(),
        },
        // [점검 A] 외부 연동 명령 (Electron 판과 같은 이름·응답): 조작 서버에서만. 내부망 출력에서는 위의 영상 메타데이터·끝남 알림만 (Electron: req.lan → 403)
        Some(cmd) if local.is_some() => {
            let t0 = web_time::Instant::now();
            let mut r = crate::extcmd::respond(&mut core.lock(), cmd, &b["args"]);
            if let Some(o) = r.as_object_mut() { o.insert("ms".into(), json!((t0.elapsed().as_secs_f64() * 10_000.0).round() / 10.0)); }
            Json(r).into_response()
        }
        _ => (StatusCode::FORBIDDEN, Json(json!({ "ok": false }))).into_response(),
    }
}

/// 다른 컴퓨터 이름으로 들어온 요청은 거절 (DNS rebinding 막기)
async fn host_check(State(core): State<Shared>, mut req: Request<Body>, next: Next) -> Response {
    let port = core.lock().port;
    let host = req.headers().get(header::HOST).and_then(|v| v.to_str().ok()).unwrap_or("").to_lowercase();
    let ok = [format!("127.0.0.1:{port}"), format!("localhost:{port}"), format!("[::1]:{port}")].contains(&host);
    if !ok {
        return status(StatusCode::MISDIRECTED_REQUEST);
    }
    req.extensions_mut().insert(LocalControl);
    let mut res = next.run(req).await;
    res.headers_mut().insert("X-Content-Type-Options", HeaderValue::from_static("nosniff"));
    res
}

/// 출력 화면 길 (조작 서버와 내부망 출력이 같이 씀) // [트랙 D] router 에서 나눔
pub fn output_routes(core: Shared) -> Router<Shared> {
    let web = core.lock().web_dir.clone();
    let (w1, w2, w3, w4) = (web.clone(), web.clone(), web.clone(), web.clone());
    let media = core.lock().media_dir.clone();
    Router::new()
        .route("/", get(|| async { Redirect::temporary("/outputs/main") }))
        .route("/outputs/main", get(move || { let w = w1.clone(); async move { send_file(&w, "output/index.html").await } }))
        .route("/outputs/overlay", get(move || { let w = w2.clone(); async move { send_file(&w, "output/index.html").await } }))
        .route("/outputs/stage", get(move || { let w = w3.clone(); async move { send_file(&w, "output/stage.html").await } }))
        // 추가 출력 /outputs/out-… (로드맵 3단계)은 예배당 화면과 같은 페이지 — output.js 가 주소 끝 이름으로 자기 출력을 안다
        .route("/outputs/{*f}", get({ let w = web.clone(); move |State(c): State<Shared>, axum::extract::Path(f): axum::extract::Path<String>| { let w = w.clone(); async move { if crate::outputs::find_extra(c.lock().settings.get(), &f).is_some() { send_file(&w, "output/index.html").await } else { send_file(&w.join("output"), &f).await } } } }))
        .route("/shared/{*f}", get(move |axum::extract::Path(f): axum::extract::Path<String>| { let w = w4.clone(); async move { send_file(&w.join("shared"), &f).await } }))
        .route("/media/{f}", get(move |axum::extract::Path(f): axum::extract::Path<String>| { let m = media.clone(); async move { send_file(&m, &f).await } }))
        .route("/lib/file", get(lib_file))
        .route("/hymn/{key}/{file}", get(hymn_file)) // [트랙 A]
        .route("/hymnal/score/{book}/{file}", get(hymnal_score)) // [트랙 A]
        .route("/events", get(events))
        // Electron getState 의 맨 위 키 + 출력 화면용 키 (extcmd::state_json). 내부망 출력에서는 lan.rs 가 이 길을 막는다
        .route("/api/state", get(|State(c): State<Shared>| async move { Json(crate::extcmd::state_json(&c.lock())) }))
        .route("/api/live", get(|State(c): State<Shared>| async move { Json(c.lock().pres.live_state()) }))
        .route("/api/ack", post(ack))
        .route("/api/command", post(command))
}

pub fn router(core: Shared) -> Router {
    output_routes(core.clone())
        .merge(crate::remote::dock_routes()) // [트랙 D] OBS 도킹
        .merge(crate::trigger::routes()) // [트랙 D] 스트림덱·외부 단추
        .merge(crate::camhelper::routes()) // 카메라 찾기·미리보기 도우미 (조작 PC 전용)
        .merge(crate::phonecam::viewer_routes()) // 휴대폰 카메라: 출력 화면이 영상을 청하는 길 (이 PC 전용)
        .layer(middleware::from_fn_with_state(core.clone(), host_check))
        .with_state(core)
}

/// 서버를 띄운다. 포트가 쓰이고 있으면 다음 포트로 (최대 10번)
#[cfg(not(target_arch = "wasm32"))]
pub async fn serve(core: Shared) -> anyhow::Result<u16> {
    let want = core.lock().port;
    let mut last = None;
    for p in want..want + 10 {
        match tokio::net::TcpListener::bind(("127.0.0.1", p)).await {
            Ok(l) => {
                core.lock().port = p;
                let app = router(core.clone());
                crate::remote::init(core.clone()); // [트랙 D] 리모컨·내부망 서버를 설정에 맞춰 켜고 끔
                crate::cloud::init(core.clone()); // 클라우드 연결 (웹판, 기본 꺼짐 — 켜야 밖으로 나감)
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


#[cfg(test)]
mod file_boundary_tests {
    use super::*;

    fn fixture() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("wp-file-boundary-{}", rand::random::<u64>()));
        std::fs::create_dir_all(dir.join("data")).unwrap();
        std::fs::create_dir_all(dir.join("data-old")).unwrap();
        std::fs::write(dir.join("data/allowed.txt"), b"allowed").unwrap();
        std::fs::write(dir.join("data/allowed.png"), b"allowed").unwrap();
        std::fs::write(dir.join("data/settings.json"), b"{}").unwrap();
        std::fs::write(dir.join("data-old/private.png"), b"private").unwrap();
        dir
    }

    #[test]
    fn static_paths_reject_drive_unc_parent_and_streams() {
        for value in ["../secret", "a/../../secret", "a\\..\\secret", "C:\\Windows\\win.ini", "C:win.ini", "\\\\server\\share\\secret", "file.txt:secret", "\\secret", "a\0b"] {
            assert!(!safe_relative_path(value), "거절되지 않은 경로: {value:?}");
        }
        assert!(safe_relative_path("assets/그림.png"));
        assert!(safe_relative_path("./output.js"));
    }

    #[tokio::test]
    async fn static_files_require_real_folder_boundary() {
        let dir = fixture();
        let root = dir.join("data");
        assert_eq!(send_file(&root, "allowed.txt").await.status(), StatusCode::OK);
        assert_eq!(send_file(&root, "../data-old/private.txt").await.status(), StatusCode::FORBIDDEN);
        assert_eq!(send_file(&root, "C:\\Windows\\win.ini").await.status(), StatusCode::FORBIDDEN);
        assert_eq!(send_file(&root, "missing.txt").await.status(), StatusCode::NOT_FOUND);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn media_stream_and_range_handle_large_start_safely() {
        let dir = fixture();
        let data = dir.join("data");
        let file = data.join("allowed.png");
        let bytes = vec![42u8; 160_000];
        std::fs::write(&file, &bytes).unwrap();
        drop(rusqlite::Connection::open(data.join("bible.db")).unwrap());
        let core = crate::core::Core::open(data.clone(), data.join("web"), None).unwrap();
        let shared = std::sync::Arc::new(crate::lockcheck::Mutex::new(core));
        let query = HashMap::from([("p".to_string(), file.to_string_lossy().into_owned())]);
        let full = lib_file(State(shared.clone()), Query(query.clone()), HeaderMap::new()).await;
        assert_eq!(full.status(), StatusCode::OK);
        assert_eq!(full.headers()[header::CONTENT_LENGTH], "160000");
        assert_eq!(axum::body::to_bytes(full.into_body(), 200_000).await.unwrap().as_ref(), bytes);
        let mut headers = HeaderMap::new();
        headers.insert(header::RANGE, HeaderValue::from_static("bytes=2-6"));
        let range = lib_file(State(shared.clone()), Query(query.clone()), headers).await;
        assert_eq!(range.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(range.headers()[header::CONTENT_RANGE], "bytes 2-6/160000");
        assert_eq!(axum::body::to_bytes(range.into_body(), 100).await.unwrap().as_ref(), &[42u8; 5]);
        let mut headers = HeaderMap::new();
        headers.insert(header::RANGE, HeaderValue::from_static("bytes=18446744073709551615-"));
        let invalid = lib_file(State(shared.clone()), Query(query), headers).await;
        assert_eq!(invalid.status(), StatusCode::RANGE_NOT_SATISFIABLE);
        drop(shared);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn library_paths_do_not_accept_a_sibling_with_same_prefix() {
        let dir = fixture();
        let data = dir.join("data");
        // 빈 DB를 먼저 만들어 사용자 자료를 복사하지 않는 독립 시험으로 고정한다.
        drop(rusqlite::Connection::open(data.join("bible.db")).unwrap());
        let mut core = crate::core::Core::open(data.clone(), data.join("web"), None).unwrap();
        assert!(allowed_lib_file(&core, &data.join("allowed.png").to_string_lossy()).is_some());
        assert!(allowed_lib_file(&core, &data.join("allowed.txt").to_string_lossy()).is_none());
        assert!(allowed_lib_file(&core, &data.join("bible.db").to_string_lossy()).is_none());
        assert!(allowed_lib_file(&core, &data.join("settings.json").to_string_lossy()).is_none());
        let private = dir.join("data-old/private.png");
        assert!(allowed_lib_file(&core, &private.to_string_lossy()).is_none());
        assert!(allowed_lib_file(&core, &data.join("../data-old/private.png").to_string_lossy()).is_none());
        core.settings.update(|s| s["droppedFiles"] = json!([private.to_string_lossy(), data.join("settings.json").to_string_lossy()]));
        assert!(allowed_lib_file(&core, &private.to_string_lossy()).is_some());
        assert!(allowed_lib_file(&core, &data.join("settings.json").to_string_lossy()).is_none());
        drop(core);
        std::fs::remove_dir_all(dir).unwrap();
    }
}


#[cfg(test)]
mod camera_command_tests {
    use super::*;

    /// 사용자 자료를 건드리지 않는 임시 코어 (빈 DB 를 먼저 만들어 자동 복사를 막는다)
    fn temp_core() -> (Shared, PathBuf) {
        let dir = std::env::temp_dir().join(format!("wp-cam-cmd-{}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).unwrap();
        drop(rusqlite::Connection::open(dir.join("bible.db")).unwrap());
        let core = crate::core::Core::open(dir.clone(), dir.join("web"), None).unwrap();
        (std::sync::Arc::new(crate::lockcheck::Mutex::new(core)), dir)
    }

    /// 실제 HTTP 로 가짜 도우미 POST 를 보내 계약(헤더·토큰·검증·저장)을 시험한다. 카메라는 켜지 않는다.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn camera_devices_contract_over_http() {
        let (core, dir) = temp_core();
        let l = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = l.local_addr().unwrap().port();
        core.lock().port = port;
        let app = router(core.clone());
        tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
        let base = format!("http://127.0.0.1:{port}");
        let post = move |body: Value, header_on: bool| {
            let url = format!("{base}/api/command");
            let agent: ureq::Agent = ureq::Agent::config_builder().http_status_as_error(false).build().into();
            let mut r = agent.post(&url);
            if header_on { r = r.header("x-worshipcue", "1"); }
            let mut res = r.send_json(body).unwrap();
            let code = res.status().as_u16();
            let text = res.body_mut().read_to_string().unwrap_or_default();
            (code, text)
        };
        let token = core.lock().start_camera_probe();
        let good = json!({ "cmd": "cameraDevices", "args": { "token": token, "devices": [{ "deviceId": "f00d", "label": "USB 캡처보드", "kind": "videoinput" }], "error": "" } });
        // 헤더 없으면 403
        let (c, _) = { let p = post.clone(); let g = good.clone(); tokio::task::spawn_blocking(move || p(g, false)).await.unwrap() };
        assert_eq!(c, 403);
        assert!(core.lock().b.cams.devices.is_empty());
        // 너무 많은 장치는 400, 목록 그대로
        let many: Vec<Value> = (0..40).map(|i| json!({ "deviceId": format!("d{i}") })).collect();
        let bad = json!({ "cmd": "cameraDevices", "args": { "token": token, "devices": many } });
        let (c, _) = { let p = post.clone(); tokio::task::spawn_blocking(move || p(bad, true)).await.unwrap() };
        assert_eq!(c, 400);
        assert!(core.lock().b.cams.devices.is_empty());
        // 올바른 결과는 저장
        let (c, t) = { let p = post.clone(); let g = good.clone(); tokio::task::spawn_blocking(move || p(g, true)).await.unwrap() };
        assert_eq!(c, 200, "{t}");
        assert_eq!(serde_json::from_str::<Value>(&t).unwrap()["result"]["count"], 1);
        assert_eq!(core.lock().camera_choices()[0].label, "USB 캡처보드");
        // 같은 토큰을 다시 쓰면 거절
        let (c, _) = { let p = post.clone(); let g = good.clone(); tokio::task::spawn_blocking(move || p(g, true)).await.unwrap() };
        assert_eq!(c, 400);
        // 도우미 페이지가 나온다
        let url = format!("http://127.0.0.1:{port}/helper/cameras?token=x");
        let page = tokio::task::spawn_blocking(move || ureq::get(&url).call().unwrap().body_mut().read_to_string().unwrap()).await.unwrap();
        assert!(page.contains("cameraDevices") && page.contains("x-worshipcue"));
        let url = format!("http://127.0.0.1:{port}/helper/camera?id=f00d");
        let page = tokio::task::spawn_blocking(move || ureq::get(&url).call().unwrap().body_mut().read_to_string().unwrap()).await.unwrap();
        assert!(page.contains("/shared/render.js") && page.contains("cameraStream"));
        drop(core);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn helper_routes_are_not_on_lan_output_routes() {
        // 내부망 출력 서버(lan.rs)는 output_routes 만 쓴다 → 도우미 길은 router() 에서만 붙는다
        let src = include_str!("server.rs");
        let i = src.find("pub fn output_routes").unwrap();
        let j = src.find("pub fn router").unwrap();
        assert!(!src[i..j].contains("camhelper"));
        assert!(src[j..].contains("camhelper::routes()"));
    }
}
#[cfg(test)]
mod extcmd_http_tests {
    use super::*;

    /// [점검 A] 외부 연동 명령은 조작 서버에서만: 머리글 없으면 403, 내부망 출력 서버에서는 '영상 끝남' 말고 403 (Electron 판과 같은 경계)
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn command_boundary_control_vs_lan() {
        let dir = std::env::temp_dir().join(format!("wp-extcmd-http-{}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).unwrap();
        drop(rusqlite::Connection::open(dir.join("bible.db")).unwrap());
        let core: Shared = std::sync::Arc::new(crate::lockcheck::Mutex::new(crate::core::Core::open(dir.clone(), dir.join("web"), None).unwrap()));
        core.lock().settings.update(|d| d["lanOutput"] = json!({ "enabled": false, "port": 0, "key": "lanKeyForTest01" }));
        let l = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = l.local_addr().unwrap().port();
        core.lock().port = port;
        let app = router(core.clone());
        tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
        let l2 = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let lan_port = l2.local_addr().unwrap().port();
        let lan = crate::lan::router(core.clone());
        tokio::spawn(async move { axum::serve(l2, lan).await.unwrap() });
        let post = |url: String, body: Value, header_on: bool| async move {
            tokio::task::spawn_blocking(move || {
                let agent: ureq::Agent = ureq::Agent::config_builder().http_status_as_error(false).build().into();
                let mut r = agent.post(&url);
                if header_on { r = r.header("x-worshipcue", "1"); }
                let mut res = r.send_json(body).unwrap();
                (res.status().as_u16(), serde_json::from_str::<Value>(&res.body_mut().read_to_string().unwrap_or_default()).unwrap_or(Value::Null))
            }).await.unwrap()
        };
        let ctl = format!("http://127.0.0.1:{port}/api/command");
        let lanu = format!("http://127.0.0.1:{lan_port}/api/command?key=lanKeyForTest01");
        // 조작 서버: 머리글 없으면 403
        assert_eq!(post(ctl.clone(), json!({ "cmd": "stageMessage", "args": { "text": "x" } }), false).await.0, 403);
        assert_eq!(core.lock().b.stage.message, "");
        // 조작 서버: 머리글이 있으면 Electron 과 같은 봉투
        let (code, r) = post(ctl.clone(), json!({ "cmd": "stageMessage", "args": { "text": "설교 5분 남음" } }), true).await;
        assert_eq!((code, r["ok"].clone(), r["result"]["message"].clone()), (200, json!(true), json!("설교 5분 남음")));
        assert!(r["liveVersion"].is_number() && r["ms"].is_number());
        let (code, r) = post(ctl.clone(), json!({ "cmd": "noSuchCommand" }), true).await;
        assert_eq!((code, r["ok"].clone()), (200, json!(false)));
        // 내부망 출력 서버: 열쇠가 맞아도 조작 명령은 403, 무대 메시지는 그대로
        let (code, _) = post(lanu.clone(), json!({ "cmd": "stageMessage", "args": { "text": "내부망에서" } }), true).await;
        assert_eq!(code, 403);
        assert_eq!(post(lanu.clone(), json!({ "cmd": "clearAll" }), true).await.0, 403);
        assert_eq!(post(lanu.clone(), json!({ "cmd": "saveCue", "args": { "cue": { "name": "x", "items": [] } } }), true).await.0, 403);
        assert_eq!(core.lock().b.stage.message, "설교 5분 남음");
        // 내부망 출력 서버: 출력 화면이 알리는 '영상 끝남' 은 받는다
        assert_eq!(post(lanu.clone(), json!({ "cmd": "videoMeta", "args": {} }), false).await.0, 403);
        assert_eq!(post(lanu.clone(), json!({ "cmd": "videoMeta", "args": {} }), true).await.0, 200);
        assert_eq!(post(lanu, json!({ "cmd": "videoEnded", "args": {} }), true).await.0, 200);
        // 상태: 조작 서버는 Electron 모양, 내부망에서는 막힘
        let get = |url: String| async move { tokio::task::spawn_blocking(move || { let agent: ureq::Agent = ureq::Agent::config_builder().http_status_as_error(false).build().into(); let mut res = agent.get(&url).call().unwrap(); (res.status().as_u16(), res.body_mut().read_to_string().unwrap_or_default()) }).await.unwrap() };
        let (code, text) = get(format!("http://127.0.0.1:{port}/api/state")).await;
        assert_eq!(code, 200);
        let s: Value = serde_json::from_str(&text).unwrap();
        assert!(s["live"].is_object() && s["stage"].is_object() && s["obsView"].is_object() && s.get("cameras").is_some());
        assert!(!text.contains("lanKeyForTest01"), "내부망 열쇠가 상태에 들어 있다");
        assert_eq!(get(format!("http://127.0.0.1:{lan_port}/api/state?key=lanKeyForTest01")).await.0, 403);
        drop(core);
        let _ = std::fs::remove_dir_all(dir);
    }
}
