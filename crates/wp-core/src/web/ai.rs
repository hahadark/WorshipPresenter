//! [웹판] AI 부르기: 설치판처럼 PC 가 교회 키로 직접 부르지 않고, 클라우드 `/api/v1/ai/chat` 이 교회 키를 붙여 대신 부른다 (docs/web/ai.md).
//! 요청 본문은 설치판 `aihelp::request_body` 그대로 + `feature`. 사람이 누른 단추 처리 안에서만 부른다(`X-WP-Action: user`) —
//! 웹에서 AI 를 부르는 길은 `hymns::run_bg` 로 시작한 일(단추를 눌러 시작)뿐이고 타이머 · 동기화에서는 부르지 않는다.
use crate::web::net::post_json_sync;
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};

static CLOUD: AtomicBool = AtomicBool::new(false);

/// 로그인한 웹 조작 화면(ai 권한)이면 켠다
pub fn enable(on: bool) {
    CLOUD.store(on, Ordering::Relaxed);
}
pub fn cloud() -> bool {
    CLOUD.load(Ordering::Relaxed)
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

pub fn chat(system: &str, user: &str, json_mode: bool, max_tokens: u32) -> Result<(String, Value), String> {
    let feature = feature_of(system);
    let mut body = crate::aihelp::request_body("", system, user, json_mode, max_tokens);
    body["feature"] = Value::from(feature);
    let (status, v) = post_json_sync("/api/v1/ai/chat", &body, &[("x-wp-action", "user")])?;
    let err = v["error"].as_str().unwrap_or("").to_string();
    match status {
        200..=299 => Ok((crate::aihelp::response_text(&v)?, v["usage"].clone())),
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
