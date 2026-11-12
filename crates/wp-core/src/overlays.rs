//! [트랙 B] 오버레이(로고·시계·카운트다운·배너·글자) · 회중 메시지 · 무대 화면 상태 · 매크로·예약
//! 설정(settings.json)의 overlays · messageTemplates · stagePresets · macros · schedule 를 그대로 쓴다.

use crate::util::{now_ms, short_id};
use anyhow::{bail, Result};
use serde_json::{json, Value};

pub const OVERLAY_TYPES: [&str; 5] = ["logo", "clock", "countdown", "banner", "text"];
pub const POSITIONS: [&str; 9] = ["top-left", "top-center", "top-right", "middle-left", "center", "middle-right", "bottom-left", "bottom-center", "bottom-right"];

fn cut(v: &Value, n: usize) -> String {
    v.as_str().unwrap_or("").chars().take(n).collect()
}
fn clampf(v: &Value, d: f64, a: f64, b: f64) -> f64 {
    v.as_f64().map(|x| x.clamp(a, b)).unwrap_or(d)
}
fn ok_id(v: &Value, min: usize) -> Option<String> {
    v.as_str().filter(|x| (min..=40).contains(&x.len()) && x.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')).map(String::from)
}

/// library.js normalizeOverlay 와 같은 정리
pub fn normalize_overlay(o: &Value) -> Result<Value> {
    let ty = o["type"].as_str().unwrap_or("");
    if !OVERLAY_TYPES.contains(&ty) {
        bail!("알 수 없는 오버레이 종류입니다.");
    }
    let id = ok_id(&o["id"], 4).unwrap_or_else(|| short_id("o"));
    let def_name = match ty { "logo" => "로고", "clock" => "시계", "countdown" => "카운트다운", "banner" => "배너", _ => "글자" };
    let name = cut(&o["name"], 80).trim().to_string();
    let outputs: Vec<Value> = match o["outputs"].as_array() {
        Some(a) => a.iter().filter(|x| *x == "main" || *x == "overlay").cloned().collect(),
        None => vec![json!("main"), json!("overlay")],
    };
    let pos = o["position"].as_str().filter(|p| POSITIONS.contains(p)).unwrap_or(if ty == "banner" { "bottom-center" } else { "top-right" });
    let color = cut(&o["color"], 40);
    Ok(json!({
        "id": id, "type": ty, "name": if name.is_empty() { def_name.to_string() } else { name },
        "on": o["on"].as_bool().unwrap_or(false), "outputs": outputs, "position": pos,
        "margin": clampf(&o["margin"], 40.0, 0.0, 400.0), "size": clampf(&o["size"], if ty == "logo" { 140.0 } else { 44.0 }, 10.0, 800.0),
        "color": if color.is_empty() { "#ffffff".to_string() } else { color },
        "bg": cut(&o["bg"], 60), "opacity": clampf(&o["opacity"], 1.0, 0.05, 1.0),
        "image": cut(&o["image"], 600), "text": cut(&o["text"], 1000), "target": cut(&o["target"], 40),
        "startedAt": o["startedAt"].as_i64().unwrap_or(0), "doneText": cut(&o["doneText"], 200),
        "format": if o["format"] == "12" { "12" } else { "24" }, "showSeconds": o["showSeconds"].as_bool().unwrap_or(true), "full": o["full"].as_bool().unwrap_or(false),
    }))
}

pub fn save_overlay(settings: &mut Value, o: &Value) -> Result<Value> {
    let n = normalize_overlay(o)?;
    if !settings["overlays"].is_array() {
        settings["overlays"] = json!([]);
    }
    let list = settings["overlays"].as_array_mut().unwrap();
    match list.iter().position(|x| x["id"] == n["id"]) {
        Some(i) => list[i] = n.clone(),
        None => list.push(n.clone()),
    }
    Ok(n)
}

pub fn delete_overlay(settings: &mut Value, id: &str) {
    if let Some(l) = settings["overlays"].as_array_mut() {
        l.retain(|x| x["id"] != id);
    }
}

/// 켜기·끄기 (on 이 None 이면 뒤집기). 카운트다운은 켤 때 시작 시각을 새로
pub fn toggle_overlay(settings: &mut Value, id: &str, on: Option<bool>) -> Result<Value> {
    let Some(o) = settings["overlays"].as_array_mut().and_then(|l| l.iter_mut().find(|x| x["id"] == id)) else { bail!("오버레이를 찾을 수 없습니다.") };
    let next = on.unwrap_or(!o["on"].as_bool().unwrap_or(false));
    o["on"] = json!(next);
    if next && o["type"] == "countdown" {
        o["startedAt"] = json!(now_ms());
    }
    Ok(o.clone())
}

/// 오버레이 모두 끄기 ↔ 다시 켜기 (끈 것을 기억)
pub fn toggle_all_off(settings: &mut Value, saved: &mut Vec<String>) {
    let on: Vec<String> = settings["overlays"].as_array().into_iter().flatten().filter(|o| o["on"] == true).filter_map(|o| o["id"].as_str().map(String::from)).collect();
    if !on.is_empty() {
        for id in &on {
            let _ = toggle_overlay(settings, id, Some(false));
        }
        *saved = on;
    } else {
        for id in saved.drain(..) {
            let _ = toggle_overlay(settings, &id, Some(true));
        }
    }
}

/// 회중 메시지 → 출력에 겹치는 배너 오버레이
#[derive(Clone, Debug)]
pub struct Message {
    pub text: String,
    pub outputs: Vec<String>,
    pub size: f64,
    pub until: i64,
    pub at: i64,
}

pub fn make_message(text: &str, outputs: &[String], seconds: i64, size: f64) -> Result<Message> {
    let text: String = text.trim().chars().take(300).collect();
    if text.is_empty() {
        bail!("보일 메시지를 쓰세요.");
    }
    let re = regex::Regex::new(r"\{[^}]+\}").unwrap();
    if re.is_match(&text) {
        bail!("채우지 않은 칸이 있습니다: {}", re.find_iter(&text).map(|m| m.as_str()).collect::<Vec<_>>().join(" "));
    }
    let outputs: Vec<String> = outputs.iter().filter(|x| *x == "main" || *x == "overlay").cloned().collect();
    if outputs.is_empty() {
        bail!("보일 화면(예배당 · OBS)을 고르세요.");
    }
    let sec = seconds.clamp(0, 3600);
    Ok(Message { text, outputs, size, until: if sec > 0 { now_ms() + sec * 1000 } else { 0 }, at: now_ms() })
}

pub fn message_live(m: &Option<Message>) -> bool {
    m.as_ref().is_some_and(|m| m.until == 0 || now_ms() <= m.until)
}

pub fn message_overlay(m: &Option<Message>, output: &str) -> Vec<Value> {
    let Some(mm) = m else { return vec![] };
    if !mm.outputs.iter().any(|x| x == output) || !message_live(m) {
        return vec![];
    }
    normalize_overlay(&json!({ "id": "wpmessage", "type": "banner", "on": true, "name": "회중 메시지", "text": mm.text, "outputs": mm.outputs, "position": "bottom-center", "full": true,
        "size": if mm.size > 0.0 { mm.size } else { 52.0 }, "color": "#ffffff", "bg": "rgba(12,14,20,0.88)", "margin": 0 }))
    .map(|x| vec![x])
    .unwrap_or_default()
}

/// 메시지 틀의 칸: {이름} 같은 것
pub fn template_fields(text: &str) -> Vec<String> {
    let mut v: Vec<String> = regex::Regex::new(r"\{([^}]+)\}").unwrap().captures_iter(text).map(|c| c[1].to_string()).collect();
    v.dedup();
    v
}
pub fn fill_template(text: &str, values: &[(String, String)]) -> String {
    let mut s = text.to_string();
    for (k, v) in values {
        if !v.trim().is_empty() {
            s = s.replace(&format!("{{{k}}}"), v.trim());
        }
    }
    s
}

pub fn save_message_template(settings: &mut Value, t: &Value) -> Result<String> {
    let name = cut(&t["name"], 40).trim().to_string();
    let text = cut(&t["text"], 300).trim().to_string();
    if name.is_empty() || text.is_empty() {
        bail!("틀 이름과 글을 쓰세요.");
    }
    let id = ok_id(&t["id"], 3).unwrap_or_else(|| short_id("mt"));
    if !settings["messageTemplates"].is_array() {
        settings["messageTemplates"] = json!([]);
    }
    let l = settings["messageTemplates"].as_array_mut().unwrap();
    l.retain(|x| x["id"] != id.as_str());
    l.push(json!({ "id": id, "name": name, "text": text, "seconds": t["seconds"].as_i64().unwrap_or(0).clamp(0, 3600) }));
    let skip = l.len().saturating_sub(40);
    l.drain(..skip);
    Ok(id)
}

// ---------------- 무대 화면 ----------------
#[derive(Clone, Debug, Default)]
pub struct Timer {
    pub running: bool,
    pub end_at: i64,
    pub remaining: i64,
    pub duration: i64,
}

#[derive(Clone, Debug, Default)]
pub struct Stage {
    pub timer: Timer,
    pub message: String,
    pub pin: Option<Value>,
}

impl Stage {
    /// start(분) · pause · resume · reset · add(분)
    pub fn timer_action(&mut self, act: &str, minutes: f64) -> Result<()> {
        let t = &mut self.timer;
        let now = now_ms();
        let cur = if t.running { t.end_at - now } else { t.remaining };
        let ms = (minutes * 60000.0).round() as i64;
        match act {
            "start" => {
                if ms <= 0 {
                    bail!("시간(분)을 입력하세요.");
                }
                *t = Timer { running: true, end_at: now + ms, remaining: ms, duration: ms };
            }
            "pause" => {
                t.running = false;
                t.remaining = cur;
            }
            "resume" => {
                t.running = true;
                t.end_at = now + cur;
            }
            "reset" => *t = Timer::default(),
            "add" => {
                if t.running { t.end_at += ms } else { t.remaining = cur + ms }
                t.duration += ms;
            }
            _ => bail!("알 수 없는 타이머 조작입니다."),
        }
        Ok(())
    }

    pub fn timer_json(&self) -> Value {
        let t = &self.timer;
        json!({ "running": t.running, "remaining": if t.running { t.end_at - now_ms() } else { t.remaining }, "duration": t.duration, "now": now_ms() })
    }

    /// 고정: {mode: order|text, text, name} · None(지금 화면으로)
    pub fn set_pin(&mut self, p: Option<&Value>) {
        self.pin = p.filter(|p| p["mode"] == "order" || p["mode"] == "text").map(|p| json!({ "mode": p["mode"], "text": cut(&p["text"], 2000), "name": cut(&p["name"], 60), "id": p["id"].as_str().unwrap_or("") }));
    }
}

pub fn save_stage_preset(settings: &mut Value, p: &Value) -> Result<String> {
    if p["mode"] != "order" && p["mode"] != "text" {
        bail!("고정할 것을 고르세요.");
    }
    let name = cut(&p["name"], 60).trim().to_string();
    if name.is_empty() {
        bail!("프리셋 이름을 입력하세요.");
    }
    let id = ok_id(&p["id"], 3).unwrap_or_else(|| short_id("sp"));
    if !settings["stagePresets"].is_array() {
        settings["stagePresets"] = json!([]);
    }
    let l = settings["stagePresets"].as_array_mut().unwrap();
    l.retain(|x| x["id"] != id.as_str());
    l.push(json!({ "id": id, "name": name, "mode": p["mode"], "text": cut(&p["text"], 2000) }));
    Ok(id)
}

// ---------------- 매크로 · 예약 ----------------
/// 매크로: { id, name, color, actions: { logo, clearAll, hideText, bgOff, overlaysOff, overlayOn:[id], overlayOff:[id], message, stageMessage, timer(분), cueItem } }
pub fn save_macro(settings: &mut Value, m: &Value) -> Result<String> {
    let name = cut(&m["name"], 40).trim().to_string();
    if name.is_empty() {
        bail!("매크로 이름을 쓰세요.");
    }
    let id = ok_id(&m["id"], 3).unwrap_or_else(|| short_id("m"));
    if !settings["macros"].is_array() {
        settings["macros"] = json!([]);
    }
    let mut v = m.clone();
    v["id"] = json!(id);
    v["name"] = json!(name);
    let l = settings["macros"].as_array_mut().unwrap();
    match l.iter().position(|x| x["id"] == id.as_str()) {
        Some(i) => l[i] = v,
        None => l.push(v),
    }
    Ok(id)
}

pub fn delete_macro(settings: &mut Value, id: &str) {
    if let Some(l) = settings["macros"].as_array_mut() {
        l.retain(|x| x["id"] != id);
    }
}

/// 예약 [{id, days:[0~6], time:'HH:MM', macroId, enabled}] 중 지금 맞는 것 → 매크로 id (같은 날 한 번)
pub fn due_schedules(schedule: &Value, weekday: u32, hhmm: &str, fired: &mut Vec<String>, day_key: &str) -> Vec<String> {
    let mut out = vec![];
    for s in schedule.as_array().into_iter().flatten() {
        if !s["enabled"].as_bool().unwrap_or(true) || s["time"] != hhmm {
            continue;
        }
        if !s["days"].as_array().is_some_and(|d| d.iter().any(|x| x.as_u64() == Some(weekday as u64))) {
            continue;
        }
        let key = format!("{}@{}", s["id"].as_str().unwrap_or(""), day_key);
        if fired.contains(&key) {
            continue;
        }
        fired.push(key);
        if let Some(m) = s["macroId"].as_str() {
            out.push(m.to_string());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overlays() {
        let mut s = json!({});
        let o = save_overlay(&mut s, &json!({ "type": "clock" })).unwrap();
        assert_eq!(o["name"], "시계");
        assert_eq!(o["position"], "top-right");
        let id = o["id"].as_str().unwrap().to_string();
        assert_eq!(toggle_overlay(&mut s, &id, None).unwrap()["on"], true);
        let mut saved = vec![];
        toggle_all_off(&mut s, &mut saved);
        assert_eq!(s["overlays"][0]["on"], false);
        toggle_all_off(&mut s, &mut saved);
        assert_eq!(s["overlays"][0]["on"], true);
        assert!(normalize_overlay(&json!({ "type": "x" })).is_err());
        delete_overlay(&mut s, &id);
        assert_eq!(s["overlays"].as_array().unwrap().len(), 0);
    }
    #[test]
    fn message() {
        assert!(make_message("{이름} 부모님", &["main".into()], 0, 0.0).is_err());
        assert_eq!(template_fields("{이름} 어린이 {이름}"), vec!["이름"]);
        let m = make_message(&fill_template("{이름} 부모님", &[("이름".into(), "홍길동".into())]), &["main".into()], 30, 0.0).unwrap();
        assert_eq!(m.text, "홍길동 부모님");
        assert_eq!(message_overlay(&Some(m.clone()), "main").len(), 1);
        assert_eq!(message_overlay(&Some(m), "overlay").len(), 0);
    }
    #[test]
    fn timer() {
        let mut st = Stage::default();
        st.timer_action("start", 30.0).unwrap();
        assert!(st.timer.running);
        st.timer_action("pause", 0.0).unwrap();
        assert!(!st.timer.running && st.timer.remaining > 29 * 60000);
        st.timer_action("add", 5.0).unwrap();
        assert_eq!(st.timer.duration, 35 * 60000);
        assert!(st.timer_action("start", 0.0).is_err());
        st.set_pin(Some(&json!({ "mode": "text", "text": "기도 제목" })));
        assert_eq!(st.pin.as_ref().unwrap()["text"], "기도 제목");
    }
    #[test]
    fn schedule() {
        let sc = json!([{ "id": "s1", "days": [0], "time": "10:55", "macroId": "m1", "enabled": true }]);
        let mut fired = vec![];
        assert_eq!(due_schedules(&sc, 0, "10:55", &mut fired, "d1"), vec!["m1"]);
        assert!(due_schedules(&sc, 0, "10:55", &mut fired, "d1").is_empty());
        assert!(due_schedules(&sc, 1, "10:55", &mut fired, "d2").is_empty());
    }
}
