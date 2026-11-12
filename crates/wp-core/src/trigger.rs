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
        return Err(crate::i18n::tr("그 순서가 없습니다.").into());
    }
    let d = core.item_deck(index as usize)?;
    core.pres.preview = Some(d.clone());
    core.send_deck(d);
    Ok(json!({ "index": index }))
}

/// LIVE와 열린 큐가 다르면 이동을 거부한다. 앞으로 음악 재생, 뒤로 음악 건너뛰기.
pub fn advance_live_cue(core: &mut Core, dir: i64) -> Result<Value, String> {
    let p = core.pres.live.as_ref().map(|l| &l.deck.passage).ok_or("송출 중인 큐시트 순서가 없습니다.")?;
    let id = p["cueId"].as_str().filter(|id| !id.is_empty()).ok_or("송출 중인 큐시트 순서가 없습니다.")?;
    let ci = p["cueIndex"].as_i64().filter(|i| *i >= 0).ok_or("송출 중인 큐시트 순서가 없습니다.")?;
    let cue = core.cue.as_ref().filter(|c| c.id == id).ok_or("송출 중인 큐시트를 먼저 여세요.")?;
    let n = cue.items.len() as i64;
    let mut next = ci + dir;
    while next >= 0 && next < n {
        let audio = core.cue.as_ref().unwrap().items[next as usize]["type"] == "audio";
        if !audio {
            let mut deck = core.item_deck(next as usize)?;
            if dir < 0 { deck.index = deck.slides.len().saturating_sub(1); }
            core.pres.preview = Some(deck.clone());
            core.send_deck(deck);
            return Ok(json!({ "index": next }));
        }
        if dir > 0 {
            let deck = core.item_deck(next as usize)?;
            core.send_deck(deck);
        }
        next += dir;
    }
    Err("그 순서가 없습니다.".into())
}

/// 동작 실행. 아직 Rust 판에 없는 기능(타이머·무대 메시지·그리기·매크로·오버레이)은 알림
pub fn run(core: &mut Core, action: &str, q: &HashMap<String, String>) -> Result<Value, String> {
    match action {
        "next" => core.step(1),
        "prev" => core.step(-1),
        "send" => core.send_preview(),
        "hide" => core.toggle_clear(),
        "logo" => core.toggle_logo(),
        "bg" => core.toggle_bg(),
        "clear-all" => core.clear_all(),
        "cue-next" | "cue-prev" => {
            return advance_live_cue(core, if action == "cue-next" { 1 } else { -1 });
        }
        // [트랙 B] 오버레이 · 타이머 · 무대 메시지 · 매크로
        "overlays" => core.toggle_overlays_off(),
        "timer" => {
            let min = q.get("min").and_then(|x| x.parse::<f64>().ok()).filter(|x| *x > 0.0).unwrap_or(30.0);
            core.stage_timer("start", min).map_err(|e| e.to_string())?;
        }
        "timer-pause" => {
            let act = if core.b.stage.timer.running { "pause" } else { "resume" };
            core.stage_timer(act, 0.0).map_err(|e| e.to_string())?;
        }
        "timer-reset" => core.stage_timer("reset", 0.0).map_err(|e| e.to_string())?,
        "message" => core.stage_message(&q.get("text").cloned().unwrap_or_default()),
        "macro" => {
            let key = q.get("id").or(q.get("name")).cloned().unwrap_or_default();
            core.run_macro(&key).map_err(|e| e.to_string())?;
        }
        "draw-clear" => core.draw_clear(), // [트랙 B]
        _ => return Err(crate::i18n::tr_args("알 수 없는 동작: {1}", &[&action])),
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

#[cfg(test)]
mod tests_b {
    // [트랙 B] 외부 단추 → 타이머 · 무대 메시지 · 오버레이 · 매크로
    use super::*;
    fn core() -> Core {
        let dir = std::env::temp_dir().join(format!("wp-trig-{}", crate::util::now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("bible.db"), b"").unwrap(); // 빈 DB (기존 자료 복사 안 함)
        Core::open(dir.clone(), dir, None).unwrap()
    }
    #[test]
    fn timer_message_macro() {
        let mut c = core();
        let q = |p: &[(&str, &str)]| p.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect::<HashMap<_, _>>();
        run(&mut c, "timer", &q(&[("min", "5")])).unwrap();
        assert!(c.b.stage.timer.running);
        assert_eq!(c.b.stage.timer.duration, 300_000);
        run(&mut c, "timer-pause", &q(&[])).unwrap();
        assert!(!c.b.stage.timer.running);
        run(&mut c, "timer-pause", &q(&[])).unwrap();
        assert!(c.b.stage.timer.running);
        run(&mut c, "timer-reset", &q(&[])).unwrap();
        assert_eq!(c.b.stage.timer.duration, 0);
        run(&mut c, "message", &q(&[("text", "설교 5분 남음")])).unwrap();
        assert_eq!(c.b.stage.message, "설교 5분 남음");
        run(&mut c, "overlays", &q(&[])).unwrap();
        assert!(run(&mut c, "macro", &q(&[("name", "없는 매크로")])).is_err());
        c.save_macro(&serde_json::json!({ "name": "로고", "actions": { "logo": true } })).unwrap();
        run(&mut c, "macro", &q(&[("name", "로고")])).unwrap();
    }
}
