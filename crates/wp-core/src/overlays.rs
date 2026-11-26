//! [트랙 B] 오버레이(로고·시계·카운트다운·배너·글자) · 회중 메시지 · 무대 화면 상태 · 매크로·예약
//! 설정(settings.json)의 overlays · messageTemplates · stagePresets · macros · schedule 를 그대로 쓴다.

use crate::util::{now_ms, short_id};
use anyhow::{bail, Result};
use serde_json::{json, Value};

pub const OVERLAY_TYPES: [&str; 6] = ["logo", "clock", "countdown", "banner", "text", "name"];
/// 이름 자막 모양: 막대(왼쪽 색 띠 + 반투명 바탕) · 상자(둥근 바탕) · 글자만(그림자)
pub const NAME_STYLES: [&str; 3] = ["bar", "box", "plain"];
/// 이름 자막은 하나만 두고 이름·직분을 바꿔 띄운다
pub const NAME_TAG_ID: &str = "nametag";
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
    let def_name = match ty { "logo" => "로고", "clock" => "시계", "countdown" => "카운트다운", "banner" => "배너", "name" => "이름 자막", _ => "글자" };
    let name = cut(&o["name"], 80).trim().to_string();
    let outputs: Vec<Value> = match o["outputs"].as_array() {
        Some(a) => a.iter().filter(|x| *x == "main" || *x == "overlay").cloned().collect(),
        None => vec![json!("main"), json!("overlay")],
    };
    let pos = o["position"].as_str().filter(|p| POSITIONS.contains(p)).unwrap_or(match ty { "banner" => "bottom-center", "name" => "bottom-left", _ => "top-right" });
    let color = cut(&o["color"], 40);
    let accent = cut(&o["accent"], 40);
    let mut v = json!({
        "id": id, "type": ty, "name": if name.is_empty() { def_name.to_string() } else { name },
        "on": o["on"].as_bool().unwrap_or(false), "outputs": outputs, "position": pos,
        "margin": clampf(&o["margin"], 40.0, 0.0, 400.0), "size": clampf(&o["size"], if ty == "logo" { 140.0 } else { 44.0 }, 10.0, 800.0),
        "color": if color.is_empty() { "#ffffff".to_string() } else { color },
        "bg": cut(&o["bg"], 60), "opacity": clampf(&o["opacity"], 1.0, 0.05, 1.0),
        "image": cut(&o["image"], 600), "text": cut(&o["text"], 1000), "target": cut(&o["target"], 40),
        "startedAt": o["startedAt"].as_i64().unwrap_or(0), "doneText": cut(&o["doneText"], 200),
        "format": if o["format"] == "12" { "12" } else { "24" }, "showSeconds": o["showSeconds"].as_bool().unwrap_or(true), "full": o["full"].as_bool().unwrap_or(false),
    });
    if ty == "name" {
        // 이름 자막: 이름(text) · 직분(sub) · 모양 · 강조색 · 저절로 내릴 시각(hideAt, 0 = 계속)
        v["size"] = json!(clampf(&o["size"], 52.0, 10.0, 800.0));
        v["sub"] = json!(cut(&o["sub"], 120));
        v["style"] = json!(o["style"].as_str().filter(|s| NAME_STYLES.contains(s)).unwrap_or("bar"));
        v["accent"] = json!(if accent.is_empty() { "#3d7bff".to_string() } else { accent });
        v["hideAt"] = json!(o["hideAt"].as_i64().unwrap_or(0).max(0));
        if o["bg"].is_null() {
            v["bg"] = json!("rgba(10,14,22,0.78)");
        }
        if o["outputs"].is_null() {
            v["outputs"] = json!(["overlay"]); // 이름 자막은 보통 방송(OBS)에만. 예배당 화면도 켤 수 있다
        }
    }
    Ok(v)
}

/// 이름 명단 [{name, role}] (최대 60명, 빈 이름은 버림)
pub fn people(settings: &Value) -> Vec<Value> {
    settings["people"].as_array().into_iter().flatten().filter(|p| p["name"].as_str().is_some_and(|n| !n.trim().is_empty())).cloned().collect()
}

pub fn save_people(settings: &mut Value, list: &[Value]) {
    let clean: Vec<Value> = list
        .iter()
        .map(|p| json!({ "name": cut(&p["name"], 60).trim(), "role": cut(&p["role"], 120).trim() }))
        .filter(|p| p["name"].as_str().is_some_and(|n| !n.is_empty()))
        .take(60)
        .collect();
    settings["people"] = json!(clean);
}

/// 이름 자막 띄우기: 하나뿐인 이름 자막 오버레이의 이름·직분을 바꾸고 켠다 (없으면 만든다).
/// seconds > 0 이면 그만큼 뒤에 저절로 내린다 (flow::tick).
pub fn show_name(settings: &mut Value, name: &str, role: &str, seconds: f64, now: i64) -> Result<Value> {
    if name.trim().is_empty() {
        bail!("이름을 입력하세요.");
    }
    let cur = settings["overlays"].as_array().and_then(|l| l.iter().find(|x| x["id"] == NAME_TAG_ID)).cloned().unwrap_or_else(|| json!({ "id": NAME_TAG_ID, "type": "name" }));
    let mut o = cur;
    o["type"] = json!("name");
    o["text"] = json!(name.trim());
    o["sub"] = json!(role.trim());
    o["on"] = json!(true);
    o["hideAt"] = json!(if seconds >= 1.0 { now + (seconds * 1000.0) as i64 } else { 0 });
    save_overlay(settings, &o)
}

/// 이름 자막 내리기 (없으면 아무것도 안 함) → 내렸는가
pub fn hide_name(settings: &mut Value) -> bool {
    let Some(o) = settings["overlays"].as_array_mut().and_then(|l| l.iter_mut().find(|x| x["id"] == NAME_TAG_ID)) else { return false };
    let was = o["on"] == true;
    o["on"] = json!(false);
    o["hideAt"] = json!(0);
    was
}

/// 지금 떠 있는 이름 자막 {name, role, hideAt} (없으면 None)
pub fn name_on(settings: &Value) -> Option<Value> {
    settings["overlays"].as_array()?.iter().find(|x| x["id"] == NAME_TAG_ID && x["on"] == true).map(|o| json!({ "name": o["text"], "role": o["sub"], "hideAt": o["hideAt"] }))
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
/// 순서의 '송출할 때 자동으로'(actions) 정리 — Electron cues.js normalizeItem(71~89줄) 과 같은 칸·범위.
/// 칸: timer(분, 1~240) · stageMsg · clearStage · logo on|off · bg hide|show · ovOn · ovOff(id | '*') · stopMusic · companion
///     · stagePin(live | order | 프리셋 id) · macro(id) · dev[{device, action}] · obsScene · obsStream start|stop · obsRecord start|stop
pub fn normalize_actions(a: &Value) -> Value {
    let mut act = serde_json::Map::new();
    let Some(a) = a.as_object() else { return Value::Object(act) };
    let a = Value::Object(a.clone());
    let s = |k: &str, n: usize| cut(&a[k], n);
    let tm = a["timer"].as_f64().unwrap_or(0.0).round() as i64;
    if tm > 0 {
        act.insert("timer".into(), json!(tm.min(240)));
    }
    if !s("stageMsg", 200).is_empty() {
        act.insert("stageMsg".into(), json!(s("stageMsg", 200)));
    }
    if a["logo"] == "on" || a["logo"] == "off" {
        act.insert("logo".into(), a["logo"].clone());
    }
    if a["bg"] == "hide" || a["bg"] == "show" {
        act.insert("bg".into(), a["bg"].clone());
    }
    for k in ["ovOn", "ovOff"] {
        if !s(k, 60).is_empty() {
            act.insert(k.into(), json!(s(k, 60)));
        }
    }
    for k in ["stopMusic", "clearStage"] {
        if a[k].as_bool() == Some(true) || a[k].as_str().is_some_and(|x| !x.is_empty()) || a[k].as_i64().is_some_and(|x| x != 0) {
            act.insert(k.into(), json!(true));
        }
    }
    if !s("companion", 400).is_empty() {
        let loc: Vec<String> = crate::companion::parse_locations(&s("companion", 400)).iter().map(|l| format!("{}/{}/{}", l.page, l.row, l.col)).collect();
        if !loc.is_empty() {
            act.insert("companion".into(), json!(loc.join(", ")));
        }
    }
    if !s("stagePin", 40).is_empty() {
        act.insert("stagePin".into(), json!(s("stagePin", 40)));
    }
    if let Some(m) = ok_id(&a["macro"], 3).filter(|x| x.len() <= 30 && !x.contains('-')) {
        act.insert("macro".into(), json!(m));
    }
    let dev: Vec<Value> = a["dev"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|x| ok_id(&x["device"], 3).is_some_and(|d| d.len() <= 30 && !d.contains('-')) && !cut(&x["action"], 300).trim().is_empty())
        .take(8)
        .map(|x| json!({ "device": x["device"], "action": cut(&x["action"], 300).trim() }))
        .collect();
    if !dev.is_empty() {
        act.insert("dev".into(), json!(dev));
    }
    if !s("obsScene", 120).is_empty() {
        act.insert("obsScene".into(), json!(s("obsScene", 120)));
    }
    for k in ["obsStream", "obsRecord"] {
        if a[k] == "start" || a[k] == "stop" {
            act.insert(k.into(), a[k].clone());
        }
    }
    Value::Object(act)
}

/// 예전 Rust 판(2026-11-12) 매크로 칸 → Electron 칸. Electron 칸이 이미 있으면 그대로.
/// 옮길 수 있는 것: logo(bool)·bgOff(bool)·overlaysOff·overlayOn[0]·overlayOff[0]·stageMessage·timer·stagePin{mode:'order'}.
/// 옮길 수 없는 것(clearAll·hideText·message·cueItem·stagePin{mode:'text'}) 은 버린다 (Electron 매크로에 없는 동작).
pub fn legacy_actions(a: &Value) -> Value {
    let Some(o) = a.as_object() else { return a.clone() };
    let legacy = ["clearAll", "hideText", "bgOff", "overlaysOff", "overlayOn", "overlayOff", "message", "messageSeconds", "stageMessage", "cueItem"];
    let is_legacy = o.keys().any(|k| legacy.contains(&k.as_str())) || a["logo"].is_boolean() || a["stagePin"].is_object();
    if !is_legacy {
        return a.clone();
    }
    let mut n = serde_json::Map::new();
    if let Some(b) = a["logo"].as_bool() {
        n.insert("logo".into(), json!(if b { "on" } else { "off" }));
    } else if a["logo"].is_string() {
        n.insert("logo".into(), a["logo"].clone());
    }
    if let Some(b) = a["bgOff"].as_bool() {
        n.insert("bg".into(), json!(if b { "hide" } else { "show" }));
    } else if a["bg"].is_string() {
        n.insert("bg".into(), a["bg"].clone());
    }
    if a["overlaysOff"].as_bool() == Some(true) {
        n.insert("ovOff".into(), json!("*"));
    } else if let Some(id) = a["overlayOff"].as_array().and_then(|l| l.first()).and_then(|x| x.as_str()) {
        n.insert("ovOff".into(), json!(id));
    }
    if let Some(id) = a["overlayOn"].as_array().and_then(|l| l.first()).and_then(|x| x.as_str()) {
        n.insert("ovOn".into(), json!(id));
    }
    if let Some(t) = a["stageMessage"].as_str().filter(|x| !x.is_empty()) {
        n.insert("stageMsg".into(), json!(t));
    } else if a["stageMessage"].as_str() == Some("") {
        n.insert("clearStage".into(), json!(true));
    }
    if a["timer"].as_f64().is_some_and(|x| x > 0.0) {
        n.insert("timer".into(), a["timer"].clone());
    }
    if a["stagePin"]["mode"] == "order" {
        n.insert("stagePin".into(), json!("order"));
    }
    // Electron 칸이 섞여 있으면 그대로 넘긴다
    for (k, v) in o {
        if !legacy.contains(&k.as_str()) && k != "logo" && k != "stagePin" && !n.contains_key(k) {
            n.insert(k.clone(), v.clone());
        }
    }
    Value::Object(n)
}

/// 매크로 하나의 실행할 actions (예전 칸이면 변환) — 실행·요약이 모두 이것을 쓴다
pub fn macro_actions(m: &Value) -> Value {
    normalize_actions(&legacy_actions(&m["actions"]))
}

/// 매크로 목록 읽을 때: 예전 Rust 칸으로 저장된 것을 Electron 칸으로 고쳐 둔다. 고친 개수.
pub fn migrate_macros(settings: &mut Value) -> usize {
    let mut n = 0;
    if let Some(l) = settings["macros"].as_array_mut() {
        for m in l.iter_mut() {
            let conv = legacy_actions(&m["actions"]);
            if conv != m["actions"] {
                m["actions"] = normalize_actions(&conv);
                n += 1;
            }
        }
    }
    n
}

/// 매크로 저장 — Electron core.js saveMacro 와 같게: { id, name, color, actions(순서의 '송출할 때 자동으로' 칸), music, musicVolume, startup }
/// 같은 id 는 자리를 지키며 고치고, startup 은 하나만. 최대 40개.
pub fn save_macro(settings: &mut Value, m: &Value) -> Result<String> {
    let name = cut(&m["name"], 40).trim().to_string();
    if name.is_empty() {
        bail!("매크로 이름을 입력하세요.");
    }
    let actions = macro_actions(m);
    let music = cut(&m["music"], 1500);
    if actions.as_object().is_some_and(|o| o.is_empty()) && music.is_empty() {
        bail!("매크로에 넣을 동작이 없습니다. 순서의 ⚡ 송출할 때 자동으로 에서 동작을 정한 뒤 저장하세요.");
    }
    let id = ok_id(&m["id"], 3).filter(|x| x.len() <= 30 && !x.contains('-')).unwrap_or_else(|| short_id("mc"));
    let color = m["color"].as_str().filter(|c| c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|ch| ch.is_ascii_hexdigit())).unwrap_or("");
    let startup = m["startup"].as_bool() == Some(true);
    let mut one = json!({ "id": id, "name": name, "color": color, "actions": actions });
    if !music.is_empty() {
        one["music"] = json!(music);
        one["musicVolume"] = json!(clampf(&m["musicVolume"], 0.8, 0.0, 1.0));
    }
    if startup {
        one["startup"] = json!(true);
    }
    if !settings["macros"].is_array() {
        settings["macros"] = json!([]);
    }
    let l = settings["macros"].as_array_mut().unwrap();
    if startup {
        for x in l.iter_mut() {
            x["startup"] = json!(false);
        }
    }
    match l.iter().position(|x| x["id"] == id.as_str()) {
        Some(i) => l[i] = one,
        None => l.push(one),
    }
    l.truncate(40);
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
    fn name_tags() {
        let mut s = json!({});
        save_people(&mut s, &[json!({ "name": " 김택근 ", "role": "사관" }), json!({ "name": "", "role": "빈 이름" }), json!({ "name": "홍길동" })]);
        let p = people(&s);
        assert_eq!(p.len(), 2);
        assert_eq!(p[0]["name"], "김택근");
        assert_eq!(p[1]["role"], "");
        assert!(name_on(&s).is_none());
        let o = show_name(&mut s, "김택근", "사관 · 설교", 0.0, 1000).unwrap();
        assert_eq!((o["type"].as_str(), o["name"].as_str(), o["position"].as_str(), o["style"].as_str()), (Some("name"), Some("이름 자막"), Some("bottom-left"), Some("bar")));
        assert_eq!(o["outputs"], json!(["overlay"]), "기본은 방송(OBS)에만");
        assert_eq!(o["hideAt"], 0);
        // 다른 사람을 띄우면 같은 자막이 바뀜 (하나만) · 고쳤던 모양·출력은 그대로
        let mut o2 = o.clone();
        o2["style"] = json!("box");
        o2["outputs"] = json!(["main", "overlay"]);
        save_overlay(&mut s, &o2).unwrap();
        let o3 = show_name(&mut s, "홍길동", "기도", 8.0, 1000).unwrap();
        assert_eq!(s["overlays"].as_array().unwrap().len(), 1);
        assert_eq!((o3["text"].as_str(), o3["sub"].as_str(), o3["style"].as_str(), o3["hideAt"].as_i64()), (Some("홍길동"), Some("기도"), Some("box"), Some(9000)));
        assert_eq!(o3["outputs"], json!(["main", "overlay"]));
        assert_eq!(name_on(&s).unwrap()["name"], "홍길동");
        assert!(hide_name(&mut s));
        assert!(!hide_name(&mut s), "이미 내렸으면 바뀜 없음");
        assert!(name_on(&s).is_none());
        assert!(show_name(&mut s, " ", "", 0.0, 0).is_err());
        assert_eq!(normalize_overlay(&json!({ "type": "name", "style": "이상한것" })).unwrap()["style"], "bar");
    }
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
