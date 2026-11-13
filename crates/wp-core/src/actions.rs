//! 큐시트 순서의 '⚡ 송출할 때 자동으로' (item.actions · item.music) — Electron 판과 같은 저장 키.
//!
//! - 정규화: Electron `src/core/cues.js:67-90 normalizeItem` 과 같은 칸·범위
//!   `timer(분 1~240), stageMsg(200자), clearStage, logo:'on'|'off', bg:'hide'|'show', ovOn, ovOff(id|'*'),
//!    stopMusic, companion('페이지/줄/칸, …'), stagePin('live'|'order'|프리셋 id), macro(id), dev:[{device,action}],
//!    obsScene, obsStream:'start'|'stop', obsRecord:'start'|'stop'` + 순서의 `music`, `musicVolume(0~1, 기본 0.8)`
//! - 요약 글: Electron `cue.js actSummary` 와 같은 차례
//! - 매크로로 저장: Electron `macropanel.js saveFrom` 이 보내는 `{ name, actions, music, musicVolume }`
//!
//! 실행은 `trackd::run_item_actions` (여기서는 건드리지 않는다).
use crate::i18n::{tr, tr_args};
use serde_json::{json, Value};

fn cut(v: &Value, n: usize) -> String {
    v.as_str().map(|s| s.chars().take(n).collect()).unwrap_or_default()
}
fn id_ok(s: &str) -> bool {
    (3..=30).contains(&s.len()) && s.chars().all(|c| c.is_ascii_alphanumeric())
}

/// Electron `cues.js` 의 actions 정규화. 비어 있으면 `Value::Null` (Electron 은 키를 넣지 않는다).
pub fn normalize_actions(a: &Value) -> Value {
    let Some(a) = a.as_object() else { return Value::Null };
    let a = Value::Object(a.clone());
    let mut act = serde_json::Map::new();
    let tm = a["timer"].as_f64().or_else(|| a["timer"].as_str().and_then(|s| s.trim().parse().ok())).unwrap_or(0.0).round();
    if tm > 0.0 {
        act.insert("timer".into(), json!(tm.min(240.0) as i64));
    }
    if a["stageMsg"].as_str().is_some_and(|s| !s.is_empty()) {
        act.insert("stageMsg".into(), json!(cut(&a["stageMsg"], 200)));
    }
    if matches!(a["logo"].as_str(), Some("on") | Some("off")) {
        act.insert("logo".into(), a["logo"].clone());
    }
    if matches!(a["bg"].as_str(), Some("hide") | Some("show")) {
        act.insert("bg".into(), a["bg"].clone());
    }
    for k in ["ovOn", "ovOff"] {
        if a[k].as_str().is_some_and(|s| !s.is_empty()) {
            act.insert(k.into(), json!(cut(&a[k], 60)));
        }
    }
    if a["stopMusic"] == true {
        act.insert("stopMusic".into(), json!(true));
    }
    if a["clearStage"] == true {
        act.insert("clearStage".into(), json!(true));
    }
    if let Some(s) = a["companion"].as_str() {
        let loc: Vec<String> = crate::companion::parse_locations(s).iter().map(|l| format!("{}/{}/{}", l.page, l.row, l.col)).collect();
        if !loc.is_empty() {
            act.insert("companion".into(), json!(loc.join(", ")));
        }
    }
    if a["stagePin"].as_str().is_some_and(|s| !s.is_empty()) {
        act.insert("stagePin".into(), json!(cut(&a["stagePin"], 40)));
    }
    if a["macro"].as_str().is_some_and(id_ok) {
        act.insert("macro".into(), a["macro"].clone());
    }
    if let Some(l) = a["dev"].as_array() {
        let dev: Vec<Value> = l
            .iter()
            .filter(|x| x["device"].as_str().is_some_and(id_ok) && x["action"].as_str().is_some_and(|s| !s.trim().is_empty()))
            .take(8)
            .map(|x| json!({ "device": x["device"], "action": cut(&x["action"], 300).trim() }))
            .collect();
        if !dev.is_empty() {
            act.insert("dev".into(), json!(dev));
        }
    }
    if a["obsScene"].as_str().is_some_and(|s| !s.is_empty()) {
        act.insert("obsScene".into(), json!(cut(&a["obsScene"], 120)));
    }
    for k in ["obsStream", "obsRecord"] {
        if matches!(a[k].as_str(), Some("start") | Some("stop")) {
            act.insert(k.into(), a[k].clone());
        }
    }
    if act.is_empty() { Value::Null } else { Value::Object(act) }
}

/// 순서 한 개의 music · musicVolume · actions 를 Electron 과 같은 모양으로 (audio 순서는 이 칸이 없다)
pub fn normalize_item_actions(it: &mut Value) {
    if !it.is_object() {
        return;
    }
    if it["type"] == "audio" {
        for k in ["music", "musicVolume", "actions"] {
            it.as_object_mut().unwrap().remove(k);
        }
        return;
    }
    it["music"] = json!(cut(&it["music"], 1500));
    let vol = it["musicVolume"].as_f64().unwrap_or(0.8);
    it["musicVolume"] = json!(if vol.is_finite() { vol.clamp(0.0, 1.0) } else { 0.0 });
    match normalize_actions(&it["actions"]) {
        Value::Null => {
            it.as_object_mut().unwrap().remove("actions");
        }
        a => it["actions"] = a,
    }
}

/// 동작 수 (단추의 "(n)" 표시) — Electron `Object.keys(a).length + (it.music ? 1 : 0)`
pub fn count(it: &Value) -> usize {
    it["actions"].as_object().map(|o| o.len()).unwrap_or(0) + usize::from(it["music"].as_str().is_some_and(|m| !m.is_empty()))
}

/// Electron `cue.js actSummary` 와 같은 요약 글 (' · ' 로 이음). 없으면 빈 글.
pub fn summary(settings: &Value, it: &Value) -> String {
    let a = &it["actions"];
    if !a.is_object() {
        return String::new();
    }
    let s = |k: &str| a[k].as_str().unwrap_or("");
    let name_of = |list: &str, id: &str| -> String { settings[list].as_array().into_iter().flatten().find(|o| o["id"] == id).and_then(|o| o["name"].as_str()).unwrap_or("?").to_string() };
    let ov_name = |id: &str| if id == "*" { tr("모두") } else { name_of("overlays", id) };
    let mut out: Vec<String> = vec![];
    if let Some(t) = a["timer"].as_f64().filter(|x| *x > 0.0) {
        out.push(tr_args("⏱ 타이머 {1}분", &[&(t as i64)]));
    }
    if !s("stageMsg").is_empty() {
        out.push(tr("💬 무대 메시지"));
    }
    if a["clearStage"] == true {
        out.push(tr("메시지 지움"));
    }
    if !s("logo").is_empty() {
        out.push(if s("logo") == "on" { tr("로고 켜기") } else { tr("로고 끄기") });
    }
    if !s("bg").is_empty() {
        out.push(if s("bg") == "hide" { tr("배경 지우기") } else { tr("배경 보이기") });
    }
    if !s("ovOn").is_empty() {
        out.push(tr_args("오버레이 켜기: {1}", &[&ov_name(s("ovOn"))]));
    }
    if !s("ovOff").is_empty() {
        out.push(tr_args("오버레이 끄기: {1}", &[&ov_name(s("ovOff"))]));
    }
    if a["stopMusic"] == true {
        out.push(tr("음악 멈춤"));
    }
    match s("stagePin") {
        "" => {}
        "live" => out.push(tr("무대 고정 풀기")),
        "order" => out.push(tr("무대: 순서표")),
        p => out.push(tr_args("무대 고정: {1}", &[&name_of("stagePresets", p)])),
    }
    if !s("macro").is_empty() {
        out.push(tr_args("매크로: {1}", &[&name_of("macros", s("macro"))]));
    }
    if !s("companion").is_empty() {
        out.push(format!("Companion {}", s("companion")));
    }
    for x in a["dev"].as_array().into_iter().flatten() {
        if let Some(d) = settings["devices"].as_array().into_iter().flatten().find(|d| d["id"] == x["device"]) {
            out.push(format!("{}: {}", d["name"].as_str().unwrap_or(""), x["action"].as_str().unwrap_or("")));
        }
    }
    if !s("obsScene").is_empty() {
        out.push(tr_args("OBS 장면: {1}", &[&s("obsScene")]));
    }
    if !s("obsStream").is_empty() {
        out.push(if s("obsStream") == "start" { tr("OBS 방송 시작") } else { tr("OBS 방송 끝") });
    }
    if !s("obsRecord").is_empty() {
        out.push(if s("obsRecord") == "start" { tr("OBS 녹화 시작") } else { tr("OBS 녹화 끝") });
    }
    out.join(" · ")
}

/// 순서의 동작 묶음 → 매크로 (Electron `macropanel.js saveFrom`: `{ name, actions, music, musicVolume }`).
/// 동작이 하나도 없으면 Err (Electron: '먼저 ⚡ 송출할 때 자동으로 에서 동작을 정하세요.')
pub fn macro_from_item(name: &str, it: &Value) -> Result<Value, String> {
    let actions = normalize_actions(&it["actions"]);
    let music = it["music"].as_str().unwrap_or("");
    if actions.is_null() && music.is_empty() {
        return Err(tr("먼저 ⚡ 송출할 때 자동으로 에서 동작을 정하세요."));
    }
    let name = name.trim();
    if name.is_empty() {
        return Err(tr("매크로 이름을 쓰세요."));
    }
    Ok(json!({
        "name": name,
        "actions": if actions.is_null() { json!({}) } else { actions },
        "music": music,
        "musicVolume": it["musicVolume"].as_f64().unwrap_or(0.8).clamp(0.0, 1.0),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_matches_electron_keys_and_ranges() {
        let a = json!({
            "timer": "300", "stageMsg": "설교 시작", "clearStage": true, "logo": "on", "bg": "hide",
            "ovOn": "ov1", "ovOff": "*", "stopMusic": true, "companion": "1/0/2, 9/9, 2/1/0",
            "stagePin": "order", "macro": "mabc12", "dev": [{"device": "dx32a", "action": " ch1 on "}, {"device": "x", "action": "no"}],
            "obsScene": "설교", "obsStream": "start", "obsRecord": "bogus", "unknown": 1
        });
        let n = normalize_actions(&a);
        assert_eq!(n["timer"], 240);
        assert_eq!(n["stageMsg"], "설교 시작");
        assert_eq!(n["clearStage"], true);
        assert_eq!(n["logo"], "on");
        assert_eq!(n["bg"], "hide");
        assert_eq!(n["ovOn"], "ov1");
        assert_eq!(n["ovOff"], "*");
        assert_eq!(n["stopMusic"], true);
        assert_eq!(n["companion"], "1/0/2, 2/1/0");
        assert_eq!(n["stagePin"], "order");
        assert_eq!(n["macro"], "mabc12");
        assert_eq!(n["dev"], json!([{"device": "dx32a", "action": "ch1 on"}]));
        assert_eq!(n["obsScene"], "설교");
        assert_eq!(n["obsStream"], "start");
        assert!(n.get("obsRecord").is_none(), "start/stop 아니면 버림");
        assert!(n.get("unknown").is_none());
        // 빈 것·틀린 값만 있으면 Null (Electron 은 actions 키를 넣지 않음)
        assert!(normalize_actions(&json!({"timer": 0, "logo": "maybe", "companion": "x"})).is_null());
        assert!(normalize_actions(&json!("not an object")).is_null());
    }

    #[test]
    fn normalize_item_handles_music_and_audio() {
        let mut it = json!({"type": "title", "title": "광고", "music": "/lib/file?p=a.mp3", "musicVolume": 1.7, "actions": {"logo": "bad"}});
        normalize_item_actions(&mut it);
        assert_eq!(it["musicVolume"], 1.0);
        assert!(it.get("actions").is_none());
        assert_eq!(count(&it), 1);
        let mut au = json!({"type": "audio", "music": "x", "actions": {"timer": 5}});
        normalize_item_actions(&mut au);
        assert!(au.get("actions").is_none() && au.get("music").is_none());
    }

    #[test]
    fn summary_follows_electron_order_and_names() {
        let settings = json!({
            "overlays": [{"id": "ov1", "name": "시계"}],
            "stagePresets": [{"id": "sp1", "name": "설교 요약"}],
            "macros": [{"id": "m1abc", "name": "예배 시작"}],
            "devices": [{"id": "dx32a", "name": "본당 X32"}]
        });
        let it = json!({"actions": {"timer": 30, "logo": "off", "ovOn": "ov1", "ovOff": "*", "stagePin": "sp1", "macro": "m1abc", "companion": "1/0/2", "dev": [{"device": "dx32a", "action": "ch1 on"}], "obsScene": "설교", "obsRecord": "stop"}});
        assert_eq!(summary(&settings, &it), "⏱ 타이머 30분 · 로고 끄기 · 오버레이 켜기: 시계 · 오버레이 끄기: 모두 · 무대 고정: 설교 요약 · 매크로: 예배 시작 · Companion 1/0/2 · 본당 X32: ch1 on · OBS 장면: 설교 · OBS 녹화 끝");
        assert_eq!(summary(&settings, &json!({"title": "x"})), "");
    }

    #[test]
    fn macro_from_item_uses_electron_shape() {
        let it = json!({"title": "설교", "actions": {"timer": 40, "stagePin": "live"}, "music": "", "musicVolume": 0.5});
        let m = macro_from_item("설교 시작", &it).unwrap();
        assert_eq!(m, json!({"name": "설교 시작", "actions": {"timer": 40, "stagePin": "live"}, "music": "", "musicVolume": 0.5}));
        assert!(macro_from_item("x", &json!({"title": "빈 순서"})).is_err());
        assert!(macro_from_item("  ", &it).is_err());
    }
}
