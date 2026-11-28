//! [웹판] 브라우저(wasm32)에서만 들어가는 얇은 플랫폼 층. 설치판 빌드에는 들어가지 않는다.
//! 설계 · 판정: docs/web/egui-web.md
//!
//! - `tokio_shim`: 브라우저에 없는 tokio::fs · tokio::net 자리를 메운다 (부르면 '지원하지 않음' 오류).
//!   서버 모듈(server · remote · webedit · phonecam)의 처리기 코드를 고치지 않고 그대로 컴파일하려는 것.
//! - `axum_shim`: axum 의 `tokio` 기능에만 있는 ConnectInfo.
//! - `time`: std::time::Instant · SystemTime 대신 쓰는 것은 web-time 크레이트 (lib.rs 의 `time` 참고).
//! - 글꼴 자리(`register_font`): 브라우저는 C:\Windows\Fonts 를 못 읽는다. 시작할 때 받은 글꼴 바이트를 경로 이름으로 맡겨 두면
//!   `measure::font_file(경로)` 가 그것을 돌려준다 (조작 화면 Pretendard · 허락받은 PC 글꼴).
//! - 백그라운드 일(`defer`): 스레드가 없다 → 다음 차례(setTimeout)에 같은 스레드에서 돌린다. 지금 잡은 잠금이 풀린 뒤라 막히지 않는다.

pub mod ai;
pub mod axum_shim;
pub mod net;
pub mod seed;
pub mod tokio_shim;
pub mod vfs;

use std::collections::HashMap;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

static FONTS: once_cell::sync::Lazy<parking_lot::Mutex<HashMap<String, &'static [u8]>>> = once_cell::sync::Lazy::new(Default::default);

/// 글꼴 바이트를 경로 이름으로 맡겨 둔다 (프로그램이 끝날 때까지 쥔다 — 몇 개뿐)
pub fn register_font(path: &str, bytes: Vec<u8>) {
    FONTS.lock().insert(path.to_string(), Box::leak(bytes.into_boxed_slice()));
}

pub fn registered_font(path: &str) -> Option<&'static [u8]> {
    FONTS.lock().get(path).copied()
}

/// 백그라운드 일 자리: 지금 하던 일(프레임 · 잠금)이 끝난 뒤 같은 스레드에서 돌린다. ms 는 기다릴 시간
/// (화면이 '하는 중…' 을 한 번 그린 뒤 시작하게 하려면 30~60ms)
pub fn defer(ms: i32, f: impl FnOnce() + 'static) {
    let cb = Closure::once_into_js(f);
    let global = js_sys::global();
    if let Ok(st) = js_sys::Reflect::get(&global, &JsValue::from_str("setTimeout")) {
        if let Some(st) = st.dyn_ref::<js_sys::Function>() {
            let _ = st.call2(&JsValue::NULL, &cb, &JsValue::from_f64(ms as f64));
        }
    }
}
