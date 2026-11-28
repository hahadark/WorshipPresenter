//! [웹판] 저장 층 — 계약 docs/web/architecture.md 5절의 `Transport` (문서 읽기 · 쓰기 · 구독 · 송출 · 명령) 가운데 '문서' 쪽.
//!
//! 지금 구현: `Local` — 큐시트 · 찬양 문서를 브라우저 localStorage(`wpdocs:v1`)에 둔다. 처음에는 견본(seed.json),
//! 고친 뒤에는 저장해 둔 문서가 견본보다 앞선다. 성경 본문은 문서가 아니다(견본 · 2단계 /api/v1/bible).
//! 다음: `Cloud` — 같은 문서 모양(payload = 설치판 JSON)을 `/api/v1/documents/{kind}/{id}` 에 If-Match(revision)로.
//!   409 면 자동으로 덮어쓰지 않고 사본을 만들어 사람이 고르게 한다 (계약 6절 · 웹 편집기 충돌 규칙).
//! 송출은 store 가 아니라 live.rs (BroadcastChannel · 뒤에 /bridge).
use eframe::wasm_bindgen::{self, prelude::*, JsCast as _};
use serde_json::Value;
use std::cell::Cell;
use wp_core::core::{Ev, Shared};

const KEY: &str = "wpdocs:v1";

/// 문서 저장소 한 겹. 2단계에서 Cloud 를 더해 갈아 끼운다
pub trait DocStore {
    fn load(&self) -> Option<Value>;
    fn save(&self, docs: &Value) -> Result<(), String>;
}

pub struct Local;

impl DocStore for Local {
    fn load(&self) -> Option<Value> {
        let s = web_sys::window()?.local_storage().ok()??.get_item(KEY).ok()??;
        serde_json::from_str(&s).ok()
    }
    fn save(&self, docs: &Value) -> Result<(), String> {
        let st = web_sys::window().and_then(|w| w.local_storage().ok().flatten()).ok_or("브라우저 저장소를 쓸 수 없습니다")?;
        st.set_item(KEY, &docs.to_string()).map_err(|_| "브라우저 저장 공간이 가득 찼거나 막혀 있습니다".to_string())
    }
}

thread_local! {
    static PENDING: Cell<bool> = const { Cell::new(false) };
    static LAST: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

/// 저장해 둔 문서가 있으면 DB 에 넣는다 (견본을 넣은 뒤에 불러 견본보다 앞서게)
pub fn restore(core: &mut wp_core::core::Core) {
    if let Some(d) = Local.load() {
        match wp_core::web::seed::import_docs(core, &d) {
            Ok((c, s)) => log::info!("저장해 둔 문서: 큐시트 {c} · 찬양 {s}"),
            Err(e) => log::warn!("저장해 둔 문서를 넣지 못함: {e:#}"),
        }
    }
}

fn flush(core: &Shared) {
    PENDING.with(|p| p.set(false));
    let docs = wp_core::web::seed::export_docs(&core.lock());
    let s = docs.to_string();
    if LAST.with(|l| *l.borrow() == s) {
        return;
    }
    match Local.save(&docs) {
        Ok(()) => LAST.with(|l| *l.borrow_mut() = s),
        Err(e) => core.lock().notify(format!("자료를 브라우저에 저장하지 못했습니다: {e}"), "error"),
    }
}

/// 바뀔 때마다(Ev::Ui) 1초 모아서 저장. 페이지를 떠날 때(pagehide)도 한 번
pub fn start(core: Shared) {
    LAST.with(|l| *l.borrow_mut() = wp_core::web::seed::export_docs(&core.lock()).to_string());
    let mut rx = core.lock().tx.subscribe();
    let c = core.clone();
    wasm_bindgen_futures::spawn_local(async move {
        loop {
            match rx.recv().await {
                Ok(Ev::Ui) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    if PENDING.with(|p| p.replace(true)) {
                        continue;
                    }
                    let c2 = c.clone();
                    let cb = Closure::once_into_js(move || flush(&c2));
                    if let Some(w) = web_sys::window() {
                        let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(cb.unchecked_ref(), 1000);
                    }
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
    });
    let c = core.clone();
    let hide = Closure::<dyn FnMut()>::new(move || flush(&c));
    if let Some(w) = web_sys::window() {
        let _ = w.add_event_listener_with_callback("pagehide", hide.as_ref().unchecked_ref());
    }
    hide.forget();
}
