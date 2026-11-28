//! [웹판] 조작 화면 글꼴(Pretendard, OFL)을 실행 파일(wasm)에 넣지 않고 따로 받는다 — 첫 열기 크기를 줄이고 따로 캐시되게.
//! 시작 전에 보통 · 굵게 둘만 받고, 중간 · 조금 굵게 둘은 화면이 뜬 뒤 받아서 다시 끼운다 (그 전에는 보통 · 굵게로 대신).
//! 받은 바이트는 `wp_core::web::register_font` 에 경로 이름(/wp/fonts/…)으로 맡긴다 → theme.rs 가 같은 길로 꺼낸다.
use eframe::egui;
use eframe::wasm_bindgen::JsCast as _;

const DIR: &str = "/wp/fonts/";
pub const FIRST: [&str; 2] = ["Pretendard-Regular.ttf", "Pretendard-Bold.ttf"];
pub const LATER: [&str; 2] = ["Pretendard-Medium.ttf", "Pretendard-SemiBold.ttf"];

fn key(f: &str) -> String {
    format!("{DIR}{f}")
}

/// theme::font_definitions 가 부른다: 받아 둔 것만 넣고, 아직 없는 굵기는 가까운 것으로
pub fn add_pretendard(add: &mut impl FnMut(&str, &'static [u8])) {
    let get = |f: &str| wp_core::web::registered_font(&key(f));
    let Some(regular) = get("Pretendard-Regular.ttf") else { return };
    let bold = get("Pretendard-Bold.ttf").unwrap_or(regular);
    add("pretendard", regular);
    add("pretendard-medium", get("Pretendard-Medium.ttf").unwrap_or(regular));
    add("pretendard-semibold", get("Pretendard-SemiBold.ttf").unwrap_or(bold));
    add("pretendard-bold", bold);
}

/// 받기 시작 (브라우저가 바로 받기 시작한다 — 여러 개를 먼저 시작해 두고 차례로 기다리면 함께 받는다)
fn start(url: &str) -> Option<js_sys::Promise> {
    Some(web_sys::window()?.fetch_with_str(url))
}

async fn bytes_of(p: js_sys::Promise) -> Option<Vec<u8>> {
    let res: web_sys::Response = wasm_bindgen_futures::JsFuture::from(p).await.ok()?.dyn_into().ok()?;
    if !res.ok() {
        return None;
    }
    let buf = wasm_bindgen_futures::JsFuture::from(res.array_buffer().ok()?).await.ok()?;
    Some(js_sys::Uint8Array::new(&buf).to_vec())
}

/// 같은 곳의 파일 바이트
pub async fn fetch_bytes(url: &str) -> Option<Vec<u8>> {
    bytes_of(start(url)?).await
}

async fn load(base: &str, files: &[&str]) -> usize {
    let started: Vec<_> = files.iter().map(|f| (*f, start(&format!("{base}fonts/{f}")))).collect();
    let mut n = 0;
    for (f, p) in started {
        match match p { Some(p) => bytes_of(p).await, None => None } {
            Some(v) => {
                n += v.len();
                wp_core::web::register_font(&key(f), v);
            }
            None => log::warn!("글꼴을 받지 못함: {f}"),
        }
    }
    n
}

/// 시작 전: 보통 · 굵게. 받은 바이트 수
pub async fn load_first(base: &str) -> usize {
    load(base, &FIRST).await
}

/// 화면이 뜬 뒤: 나머지 굵기를 받아 글꼴을 다시 끼운다
pub fn load_later(base: String, ctx: egui::Context) {
    wasm_bindgen_futures::spawn_local(async move {
        if load(&base, &LATER).await > 0 {
            crate::theme::install_fonts(&ctx);
            ctx.request_repaint();
        }
    });
}
