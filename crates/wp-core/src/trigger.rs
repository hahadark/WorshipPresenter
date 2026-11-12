//! 스트림덱 · Companion · 외부 단추: GET /api/trigger/<동작>?key=… (Electron 판 core.js trigger · TRIGGERS)
//! 열쇠(key)가 맞아야 동작한다 (다른 웹사이트가 함부로 못 누르게).

use crate::core::{Core, Shared};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{json, Value};
use std::collections::HashMap;

/// 외부 단추 동작 목록 (안내용, Electron 판과 같음)
pub const TRIGGERS: &[(&str, &str)] = &[
    ("next", "다음 슬라이드 (F6)"),
    ("prev", "이전 슬라이드 (F5)"),
    ("send", "미리보기 송출 (F9)"),
    ("hide", "글자 숨김·복원 (F10)"),
    ("cue-next", "다음 순서 송출"),
    ("cue-prev", "이전 순서 송출"),
    ("logo", "로고 화면 켜기·끄기"),
    ("bg", "배경 지우기·보이기"),
    ("overlays", "오버레이 끄기·다시 켜기"),
    ("clear-all", "모두 지우기"),
    ("timer", "설교 타이머 시작 (?min=30)"),
    ("timer-pause", "타이머 일시정지·계속"),
    ("timer-reset", "타이머 지우기"),
    ("message", "무대 메시지 (?text=…, 비우면 지움)"),
    ("draw-clear", "화면에 그린 것 지우기"),
    ("macro", "매크로 실행 (?name=매크로 이름)"),
];

pub fn actions() -> Value {
    json!(TRIGGERS.iter().map(|(id, name)| json!({ "id": id, "name": name })).collect::<Vec<_>>())
}

/// 새 열쇠 (9바이트 → base64url 12자)
pub fn new_key(core: &mut Core) -> Value {
    let b: [u8; 9] = rand::random();
    let key = crate::dpapi::b64url(&b);
    let k = key.clone();
    core.settings.update(|d| d["triggerKey"] = json!(k));
    json!({ "key": key, "actions": actions() })
}

pub fn info(core: &mut Core) -> Value {
    if core.settings.str(&["triggerKey"]).is_empty() {
        return new_key(core);
    }
    json!({ "key": core.settings.str(&["triggerKey"]), "actions": actions() })
}

/// 길이까지 같아야 하고, 내용 비교는 끝까지 (시간 차로 알아내지 못하게)
pub fn safe_eq(a: &str, b: &str) -> bool {
    if a.is_empty() || b.is_empty() || a.len() != b.len() {
        return false;
    }
    a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// 큐시트의 앞·뒤 순서를 바로 송출 (리모컨 'cue' 와 같이 씀)
pub fn send_cue_item(core: &mut Core, index: i64) -> Result<Value, String> {
    let n = core.cue.as_ref().map(|c| c.items.len() as i64).unwrap_or(0);
    if index < 0 || index >= n {
        return Err("그 순서가 없습니다.".into());
    }
    let d = core.item_deck(index as usize)?;
    core.pres.preview = Some(d.clone());
    core.send_deck(d);
    Ok(json!({ "index": index }))
}

/// 동작 실행. 아직 Rust 판에 없는 기능(타이머·무대 메시지·그리기·매크로·오버레이)은 알림
pub fn run(core: &mut Core, action: &str, q: &HashMap<String, String>) -> Result<Value, String> {
    let _ = q;
    match action {
        "next" => core.step(1),
        "prev" => core.step(-1),
        "send" => core.send_preview(),
        "hide" => core.toggle_clear(),
        "logo" => core.toggle_logo(),
        "bg" => core.toggle_bg(),
        "clear-all" => core.clear_all(),
        "cue-next" | "cue-prev" => {
            let ci = core.pres.live.as_ref().filter(|l| !l.deck.passage["cueId"].as_str().unwrap_or("").is_empty()).and_then(|l| l.deck.passage["cueIndex"].as_i64()).unwrap_or(-1);
            if ci < 0 {
                return Err("송출 중인 큐시트 순서가 없습니다.".into());
            }
            return send_cue_item(core, ci + if action == "cue-next" { 1 } else { -1 });
        }
        "overlays" | "timer" | "timer-pause" | "timer-reset" | "message" | "draw-clear" | "macro" => {
            return Err(format!("'{action}' 동작은 아직 Rust 판에 없습니다 (준비 중)."));
        }
        _ => return Err(format!("알 수 없는 동작: {action}")),
    }
    Ok(Value::Bool(true))
}

async fn handle(State(core): State<Shared>, Path(action): Path<String>, Query(q): Query<HashMap<String, String>>) -> Response {
    let mut c = core.lock();
    let want = c.settings.str(&["triggerKey"]);
    let got = q.get("key").cloned().unwrap_or_default();
    if !safe_eq(&got, &want) {
        return (StatusCode::FORBIDDEN, Json(json!({ "ok": false, "error": "열쇠(key)가 맞지 않습니다. 워십프리젠터 → 도구 → 스트림덱·외부 단추에서 주소를 다시 복사하세요." }))).into_response();
    }
    match run(&mut c, &action, &q) {
        Ok(_) => Json(json!({ "ok": true, "result": true })).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Json(json!({ "ok": false, "error": e }))).into_response(),
    }
}

/// 조작 서버(127.0.0.1)에 붙이는 길
pub fn routes() -> Router<Shared> {
    Router::new().route("/api/trigger/{action}", get(handle))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn eq() {
        assert!(safe_eq("abc", "abc"));
        assert!(!safe_eq("abc", "abd"));
        assert!(!safe_eq("", ""));
        assert_eq!(TRIGGERS.len(), 16);
    }
}
