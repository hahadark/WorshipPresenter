//! [웹판] 브라우저(wasm32)에서만 들어가는 얇은 플랫폼 층. 설치판 빌드에는 들어가지 않는다.
//! 설계 · 판정: docs/web/egui-web.md
//!
//! - `tokio_shim`: 브라우저에 없는 tokio::fs · tokio::net 자리를 메운다 (부르면 '지원하지 않음' 오류).
//!   서버 모듈(server · remote · webedit · phonecam)의 처리기 코드를 고치지 않고 그대로 컴파일하려는 것.
//! - `axum_shim`: axum 의 `tokio` 기능에만 있는 ConnectInfo.
//! - `time`: std::time::Instant · SystemTime 대신 쓰는 것은 web-time 크레이트 (lib.rs 의 `time` 참고).

pub mod axum_shim;
pub mod seed;
pub mod tokio_shim;
pub mod vfs;
