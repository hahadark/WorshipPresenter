//! [웹판] AI 부르기: 설치판처럼 PC 가 교회 키로 직접 부르지 않고, 클라우드 `/api/v1/ai/chat` 이 교회 키를 붙여 대신 부른다 (docs/web/ai.md).
//! 요청 본문은 설치판 `aihelp::request_body` 그대로 + `feature`. 사람이 누른 단추 처리 안에서만 부른다(`X-WP-Action: user`) —
//! 웹에서 AI 를 부르는 길은 `hymns::run_bg` 로 시작한 일(단추를 눌러 시작)뿐이고 타이머 · 동기화에서는 부르지 않는다.
//!
//! **기다리는 동안 화면이 멈추지 않게 (4단계)**: 설치판의 AI 일(`cuecheck::ask_ai` …)은 '답이 올 때까지 기다리는' 함수다.
//! 웹에는 스레드가 없으므로 `run_replay` 가 그 일을 '받아 적기' 로 한 번 돌린다 — `chat` 은 기다리지 않고 요청만 적어 두고
//! 특별한 오류로 빠져나온다. 그 사이 요청을 비동기로 보내고(화면은 계속 돈다), 답이 오면 같은 일을 다시 돌린다 —
//! 이번에는 `chat` 이 받아 둔 답을 바로 돌려준다. 일은 장절 · 글을 만들어 묻고 답을 읽기만 해서 두 번 돌려도 같다.
use crate::web::net::post_json_sync;
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use wasm_bindgen::JsCast;

static CLOUD: AtomicBool = AtomicBool::new(false);

thread_local! {
    /// run_replay 안: chat 은 기다리지 않고 요청을 적어 둔다
    static CAPTURE: Cell<bool> = const { Cell::new(false) };
    static PENDING: RefCell<Option<(String, Value)>> = const { RefCell::new(None) };
    /// 요청 본문 → 받은 답 (한 일이 끝나면 비운다)
    static CACHE: RefCell<HashMap<String, Result<(String, Value), String>>> = RefCell::new(HashMap::new());
    /// 지금 기다리는 AI 요청 수 (화면 표시용)
    static WAITING: Cell<usize> = const { Cell::new(0) };
}

/// chat 이 '받아 적기' 중에 돌려주는 오류 (사람에게 보이지 않는다 — run_replay 가 가로챈다)
pub const PENDING_ERR: &str = "\u{1}wp-ai-pending";

/// 로그인한 웹 조작 화면(ai 권한)이면 켠다
pub fn enable(on: bool) {
    CLOUD.store(on, Ordering::Relaxed);
}
pub fn cloud() -> bool {
    CLOUD.load(Ordering::Relaxed)
}
/// 답을 기다리는 AI 요청 수
pub fn waiting() -> usize {
    WAITING.with(|w| w.get())
}

/// 어느 기능의 요청인지 (동의 열쇠). 설치판 코드를 고치지 않으려고 시스템 글로 알아본다
pub fn feature_of(system: &str) -> &'static str {
    if system == crate::cuecheck::ai_system() {
        "cuecheck"
    } else if system == crate::sermonslides::ai_system() {
        "sermon"
    } else if system == crate::lyricsclean::ai_system() {
        "lyrics"
    } else if system == crate::voicefollow::translate_system() {
        "translate"
    } else if system == crate::recap::summary_system() {
        "recap"
    } else if system == crate::saycmd::ai_system() {
        "say"
    } else {
        "bulletin"
    }
}

fn interpret(status: u16, v: &Value, feature: &str) -> Result<(String, Value), String> {
    let err = v["error"].as_str().unwrap_or("").to_string();
    match status {
        200..=299 => Ok((crate::aihelp::response_text(v)?, v["usage"].clone())),
        403 if v["needConsent"].as_bool().unwrap_or(false) => Err(format!(
            "교회 AI 의 '{}' 기능에 주인의 동의가 필요합니다. 주인 화면(/owner) → AI 에서 동의해 주세요.",
            crate::aihelp::feature_name(feature)
        )),
        409 if v["needKey"].as_bool().unwrap_or(false) => Err("교회 AI 키가 없습니다. 주인이 주인 화면(/owner) → AI 에서 키를 넣어 주세요.".into()),
        401 => Err("로그인이 끝났습니다. 새로 고침해 다시 로그인해 주세요.".into()),
        0 => Err("클라우드에 닿지 못했습니다. 인터넷 연결을 확인해 주세요.".into()),
        _ => Err(format!("AI 오류 ({status}): {err}")),
    }
}

pub fn chat(system: &str, user: &str, json_mode: bool, max_tokens: u32) -> Result<(String, Value), String> {
    let feature = feature_of(system);
    let mut body = crate::aihelp::request_body("", system, user, json_mode, max_tokens);
    body["feature"] = Value::from(feature);
    let key = body.to_string();
    if let Some(r) = CACHE.with(|c| c.borrow().get(&key).cloned()) {
        return r;
    }
    if CAPTURE.with(|c| c.get()) {
        PENDING.with(|p| *p.borrow_mut() = Some((key, body)));
        return Err(PENDING_ERR.into());
    }
    // run_replay 밖에서 부른 것 (지금 웹에는 없다): 예전처럼 기다려 받는다
    let (status, v) = post_json_sync("/api/v1/ai/chat", &body, &[("x-wp-action", "user")])?;
    interpret(status, &v, feature)
}

/// 비동기로 보낸다 (fetch)
async fn send(body: Value) -> Result<(String, Value), String> {
    let feature = body["feature"].as_str().unwrap_or("bulletin").to_string();
    let Some(w) = web_sys::window() else { return Err("창 없음".into()) };
    let init = web_sys::RequestInit::new();
    init.set_method("POST");
    init.set_credentials(web_sys::RequestCredentials::SameOrigin);
    let h = web_sys::Headers::new().map_err(|_| "요청을 만들지 못했습니다")?;
    let _ = h.set("content-type", "application/json");
    let _ = h.set("x-worshipcue", "1");
    let _ = h.set("x-wp-action", "user");
    init.set_headers(&h);
    init.set_body(&wasm_bindgen::JsValue::from_str(&body.to_string()));
    let res = match wasm_bindgen_futures::JsFuture::from(w.fetch_with_str_and_init("/api/v1/ai/chat", &init)).await {
        Ok(r) => r,
        Err(_) => return interpret(0, &Value::Null, &feature),
    };
    let res: web_sys::Response = res.dyn_into().map_err(|_| "응답 오류")?;
    let st = res.status();
    let text = match res.text() {
        Ok(t) => wasm_bindgen_futures::JsFuture::from(t).await.ok().and_then(|v| v.as_string()).unwrap_or_default(),
        Err(_) => String::new(),
    };
    interpret(st, &serde_json::from_str(&text).unwrap_or(Value::Null), &feature)
}

/// hymns::run_bg 의 웹 자리: 일을 '받아 적기' 로 돌리고, AI 요청이 있으면 비동기로 보내 답이 온 뒤 다시 돌린다
pub fn run_replay<T: 'static>(work: Box<dyn Fn() -> T>, done: Box<dyn FnOnce(T)>) {
    let work: Rc<dyn Fn() -> T> = Rc::from(work);
    crate::web::defer(30, move || attempt(work, done, 0));
}

fn attempt<T: 'static>(work: Rc<dyn Fn() -> T>, done: Box<dyn FnOnce(T)>, round: u32) {
    CAPTURE.with(|c| c.set(true));
    let r = work();
    CAPTURE.with(|c| c.set(false));
    match PENDING.with(|p| p.borrow_mut().take()) {
        Some((key, body)) if round < 8 => {
            drop(r);
            WAITING.with(|w| w.set(w.get() + 1));
            wasm_bindgen_futures::spawn_local(async move {
                let res = send(body).await;
                WAITING.with(|w| w.set(w.get().saturating_sub(1)));
                CACHE.with(|c| c.borrow_mut().insert(key, res));
                attempt(work, done, round + 1);
            });
        }
        _ => {
            CACHE.with(|c| c.borrow_mut().clear());
            done(r);
        }
    }
}
