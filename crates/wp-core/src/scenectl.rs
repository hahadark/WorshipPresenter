//! 화면 구성 전환 · 자동화 (로드맵 4단계, docs/roadmap-sources.md '4단계 한 것')
//!
//! - **전환**: 구성이 바뀔 때 웹 출력이 쓰는 모양. `move`(옮겨 가기, 기본) = 같은 소스는 그 요소 그대로 새 자리로 옮겨 가고
//!   새로 생기거나 사라지는 칸은 서서히 · `fade`(서서히) = 바뀐 칸 모두 서서히 바꿈 · `cut`(바로).
//!   고르는 차례: 바뀌어 들어가는 구성의 `transition` → 출력의 `outputTransitions[출력]` → 설정 `sceneTransition` → move 0.4초.
//!   출력 데이터에 `sceneTransition: {type, ms}` 를 덧붙인다 (옛 output.js 는 모른 채 바로 바꿈 — 예전과 같음).
//! - **지금 한 번**: 외부 단추 · 리모컨이 고른 구성은 '다음 순서로 넘어갈 때까지'만 (저장 안 함). hold 면 출력 기본(`outputScenes`)을 바꿈.
//! - **송출할 때 자동으로 '구성 바꾸기'**: 순서 동작 `actions.scene = { <출력>: <구성 id | "full"> }` → 출력 기본을 바꾸는 남는 동작.
use crate::core::{Core, Ev};
use anyhow::{bail, Result};
use serde_json::{json, Value};

/// 전환 종류 (저장 값, 화면 이름)
pub const KINDS: [(&str, &str); 3] = [("move", "옮겨 가기"), ("fade", "서서히"), ("cut", "바로 (컷)")];
/// 기본 전환 시간 (ms)
pub const DEFAULT_MS: i64 = 400;
pub const MAX_MS: i64 = 3000;

/// 전환 값 하나 정리 → {type, ms}. 모르는 값이면 None (= 따라감)
pub fn normalize(v: &Value) -> Option<Value> {
    let ty = v["type"].as_str().filter(|t| KINDS.iter().any(|(k, _)| k == t))?;
    let ms = v["ms"].as_f64().map(|x| x.round() as i64).unwrap_or(DEFAULT_MS).clamp(0, MAX_MS);
    Some(json!({ "type": ty, "ms": if ty == "cut" { 0 } else { ms } }))
}

/// 설정 전체 기본 전환
pub fn global(settings: &Value) -> Value {
    normalize(&settings["sceneTransition"]).unwrap_or_else(|| json!({ "type": "move", "ms": DEFAULT_MS }))
}

/// 이 출력 · 이 구성으로 바뀔 때 쓸 전환
pub fn resolve(settings: &Value, output: &str, scene_id: &str) -> Value {
    if scene_id != crate::scenes::FULL {
        if let Some(t) = crate::scenes::find_scene(settings, scene_id).and_then(|s| normalize(&s["transition"])) {
            return t;
        }
    }
    normalize(&settings["outputTransitions"][output]).unwrap_or_else(|| global(settings))
}

/// 설정에 전환 저장. target: "" = 전체 기본, "output:<id>" = 출력, "scene:<id>" = 구성. value Null = 따라감(지움)
pub fn set(settings: &mut Value, target: &str, value: &Value) -> Result<()> {
    let n = if value.is_null() { None } else { Some(normalize(value).ok_or_else(|| anyhow::anyhow!("{}", crate::i18n::tr("전환 모양이 맞지 않습니다.")))?) };
    if target.is_empty() {
        match n {
            Some(n) => settings["sceneTransition"] = n,
            None => {
                settings.as_object_mut().map(|o| o.remove("sceneTransition"));
            }
        }
        return Ok(());
    }
    if let Some(o) = target.strip_prefix("output:") {
        if !crate::outputs::is_output(settings, o) {
            bail!("{}", crate::i18n::tr("모르는 출력입니다."));
        }
        if !settings["outputTransitions"].is_object() {
            settings["outputTransitions"] = json!({});
        }
        let m = settings["outputTransitions"].as_object_mut().unwrap();
        match n {
            Some(n) => {
                m.insert(o.into(), n);
            }
            None => {
                m.remove(o);
            }
        }
        if m.is_empty() {
            settings.as_object_mut().map(|x| x.remove("outputTransitions"));
        }
        return Ok(());
    }
    if let Some(id) = target.strip_prefix("scene:") {
        let list = settings["scenes"].as_array_mut().ok_or_else(|| anyhow::anyhow!("{}", crate::i18n::tr("화면 구성을 찾을 수 없습니다.")))?;
        let sc = list.iter_mut().find(|s| s["id"] == id).ok_or_else(|| anyhow::anyhow!("{}", crate::i18n::tr("화면 구성을 찾을 수 없습니다.")))?;
        match n {
            Some(n) => sc["transition"] = n,
            None => {
                sc.as_object_mut().map(|o| o.remove("transition"));
            }
        }
        return Ok(());
    }
    bail!("{}", crate::i18n::tr("모르는 전환 대상입니다."))
}

/// 출력이 지운 뒤 전환 지정도 지움 (outputs::delete_extra 뒤에 부름)
pub fn forget_output(settings: &mut Value, id: &str) {
    if let Some(m) = settings["outputTransitions"].as_object_mut() {
        m.remove(id);
        if m.is_empty() {
            settings.as_object_mut().map(|x| x.remove("outputTransitions"));
        }
    }
}

/// 순서 동작 '구성 바꾸기' 정리: { <출력>: <구성 id | "full"> } — 출력 id · 구성 id 모양만 본다 (없는 것은 실행 때 건너뜀)
pub fn normalize_action(v: &Value) -> Option<Value> {
    let m = v.as_object()?;
    let ok = |s: &str| (1..=40).contains(&s.len()) && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    let out: serde_json::Map<String, Value> = m.iter().filter(|(k, v)| ok(k) && v.as_str().is_some_and(ok)).take(10).map(|(k, v)| (k.clone(), v.clone())).collect();
    (!out.is_empty()).then(|| Value::Object(out))
}

/// 요약 글 한 조각: "구성: 방송 (OBS) → 카메라 + 자막"
pub fn action_summary(settings: &Value, v: &Value) -> Vec<String> {
    let names: Vec<(String, String)> = crate::outputs::list(settings);
    v.as_object()
        .into_iter()
        .flatten()
        .map(|(o, id)| {
            let on = names.iter().find(|(k, _)| k == o).map(|(_, n)| crate::i18n::tr(n)).unwrap_or_else(|| o.clone());
            let sn = scene_name(settings, id.as_str().unwrap_or(""));
            crate::i18n::tr_args("구성: {1} → {2}", &[&on, &sn])
        })
        .collect()
}

/// 구성 이름 (full = 전체 화면 (기본), 없는 구성 = ?)
pub fn scene_name(settings: &Value, id: &str) -> String {
    if id == crate::scenes::FULL || id.is_empty() {
        return crate::i18n::tr("전체 화면 (기본)");
    }
    crate::scenes::find_scene(settings, id).and_then(|s| s["name"].as_str().map(String::from)).unwrap_or_else(|| "?".into())
}

/// 송출 중인 순서의 열쇠 ('지금 한 번' 이 언제까지인지): 큐시트 순서면 큐시트 id + 번호, 아니면 제목
pub fn live_key(c: &Core) -> String {
    match c.pres.live.as_ref() {
        None => String::new(),
        Some(l) => {
            let p = &l.deck.passage;
            let ci = p["cueIndex"].as_i64().unwrap_or(-1);
            if ci >= 0 {
                format!("cue:{}:{}", p["cueId"].as_str().unwrap_or(""), ci)
            } else {
                format!("t:{}|{}", p["title"].as_str().unwrap_or(""), p["query"].as_str().unwrap_or(""))
            }
        }
    }
}

/// 지금 한 번 상태 (저장하지 않음)
#[derive(Default, Clone, Debug)]
pub struct SceneOnce {
    /// 출력 id → 구성 id
    pub map: std::collections::HashMap<String, String>,
    /// 고를 때 송출 중이던 순서의 열쇠
    pub key: String,
}

impl Core {
    /// '지금 한 번' 으로 고른 구성 (송출 중인 순서가 바뀌었으면 없음)
    pub fn scene_once_for(&self, output: &str) -> Option<String> {
        let o = &self.b.scene_once;
        if o.map.is_empty() || o.key != live_key(self) {
            return None;
        }
        o.map.get(output).filter(|id| id.as_str() == crate::scenes::FULL || crate::scenes::find_scene(self.settings.get(), id).is_some()).cloned()
    }

    /// 송출한 뒤 (send_deck): 다른 순서로 넘어갔으면 '지금 한 번' 을 지운다
    pub fn scene_once_after_send(&mut self) {
        if !self.b.scene_once.map.is_empty() && self.b.scene_once.key != live_key(self) {
            self.b.scene_once = SceneOnce::default();
        }
    }

    /// 외부 단추 · 리모컨: 출력의 구성 바꾸기. hold = 출력 기본을 바꿈(남음), 아니면 다음 순서까지.
    /// output "all" = 모든 출력. id 는 구성 id · "full" · 구성 이름(대소문자 무시)
    pub fn trigger_scene(&mut self, output: &str, id: &str, hold: bool) -> Result<String> {
        let s = self.settings.get();
        let id = id.trim();
        let sid = if id.is_empty() || id == crate::scenes::FULL {
            crate::scenes::FULL.to_string()
        } else if crate::scenes::find_scene(s, id).is_some() {
            id.to_string()
        } else {
            crate::scenes::user_scenes(s)
                .into_iter()
                .find(|x| x["name"].as_str().is_some_and(|n| n.trim().eq_ignore_ascii_case(id)))
                .and_then(|x| x["id"].as_str().map(String::from))
                .ok_or_else(|| anyhow::anyhow!("{}", crate::i18n::tr("화면 구성을 찾을 수 없습니다.")))?
        };
        let outs: Vec<String> = if output == "all" {
            crate::outputs::all_ids(s)
        } else if crate::outputs::is_output(s, output) {
            vec![output.to_string()]
        } else {
            bail!("{}", crate::i18n::tr("모르는 출력입니다."));
        };
        if hold {
            let mut err = Ok(());
            self.settings.update(|d| {
                for o in &outs {
                    if let Err(e) = crate::scenes::set_output_scene(d, o, &sid) {
                        err = Err(e);
                    }
                }
            });
            err?;
            // 남는 동작은 '지금 한 번' 보다 뒤에 고른 것 — 같은 출력의 한 번 지정은 지운다
            for o in &outs {
                self.b.scene_once.map.remove(o);
            }
        } else {
            let key = live_key(self);
            if self.b.scene_once.key != key {
                self.b.scene_once = SceneOnce { map: Default::default(), key };
            }
            for o in &outs {
                self.b.scene_once.map.insert(o.clone(), sid.clone());
            }
        }
        self.emit(Ev::Live);
        self.emit(Ev::Ui);
        Ok(sid)
    }

    /// '지금 한 번' 지정 풀기 (출력 하나 또는 "all")
    pub fn clear_scene_once(&mut self, output: &str) {
        if output == "all" {
            self.b.scene_once = SceneOnce::default();
        } else {
            self.b.scene_once.map.remove(output);
        }
        self.emit(Ev::Live);
        self.emit(Ev::Ui);
    }

    /// 순서 동작 '구성 바꾸기' 실행 (출력 기본을 바꿈). 없는 출력 · 구성은 건너뛰고 알림
    pub fn run_scene_action(&mut self, v: &Value) {
        let Some(m) = normalize_action(v) else { return };
        let mut bad = vec![];
        let mut changed = vec![];
        self.settings.update(|d| {
            for (o, id) in m.as_object().unwrap() {
                let id = id.as_str().unwrap_or("");
                match crate::scenes::set_output_scene(d, o, id) {
                    Ok(()) => changed.push(o.clone()),
                    Err(_) => bad.push(o.clone()),
                }
            }
        });
        for o in changed {
            self.b.scene_once.map.remove(&o);
        }
        if !bad.is_empty() {
            self.notify(crate::i18n::tr("송출할 때 자동으로: 찾을 수 없는 출력이나 화면 구성은 건너뛰었습니다."), "warn");
        }
    }

    /// 전환 설정 저장 (target: "" · "output:<id>" · "scene:<id>")
    pub fn set_scene_transition(&mut self, target: &str, value: &Value) -> Result<()> {
        let mut out = Ok(());
        self.settings.update(|d| out = set(d, target, value));
        self.emit(Ev::Live);
        self.emit(Ev::Ui);
        out
    }

    /// 출력 데이터의 sceneTransition
    pub fn scene_transition_for(&self, output: &str, scene_id: &str) -> Value {
        resolve(self.settings.get(), output, scene_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transition_order_and_normalize() {
        let mut s = json!({});
        assert_eq!(resolve(&s, "main", "full"), json!({ "type": "move", "ms": 400 }));
        crate::scenes::save_scene(&mut s, &json!({ "id": "sc-a", "name": "가", "layers": [{ "source": "live" }] })).unwrap();
        set(&mut s, "", &json!({ "type": "fade", "ms": 700 })).unwrap();
        assert_eq!(resolve(&s, "overlay", "sc-a"), json!({ "type": "fade", "ms": 700 }));
        set(&mut s, "output:overlay", &json!({ "type": "cut", "ms": 900 })).unwrap();
        assert_eq!(resolve(&s, "overlay", "sc-a"), json!({ "type": "cut", "ms": 0 }));
        assert_eq!(resolve(&s, "main", "sc-a"), json!({ "type": "fade", "ms": 700 }));
        set(&mut s, "scene:sc-a", &json!({ "type": "move", "ms": 99999 })).unwrap();
        assert_eq!(resolve(&s, "overlay", "sc-a"), json!({ "type": "move", "ms": 3000 }));
        // 구성 저장(정리)을 다시 해도 transition 은 남는다
        let sc = crate::scenes::find_scene(&s, "sc-a").unwrap();
        crate::scenes::save_scene(&mut s, &sc).unwrap();
        assert_eq!(crate::scenes::find_scene(&s, "sc-a").unwrap()["transition"]["type"], "move");
        set(&mut s, "scene:sc-a", &Value::Null).unwrap();
        set(&mut s, "output:overlay", &Value::Null).unwrap();
        assert!(s.get("outputTransitions").is_none());
        assert!(set(&mut s, "", &json!({ "type": "zoom" })).is_err());
        assert!(set(&mut s, "output:nope", &json!({ "type": "cut" })).is_err());
    }

    fn core() -> Core {
        let dir = std::env::temp_dir().join(format!("wp-scenectl-{}", crate::util::short_id("t")));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("bible.db"), b"").unwrap(); // 실제 자료를 복사하지 않게 빈 DB
        Core::open(dir.clone(), dir, None).unwrap()
    }

    /// 로드맵 4단계 시험 기준 (코어): 지금 한 번은 다음 순서에서 되돌아오고, hold · 순서 동작은 남는다
    #[test]
    fn once_hold_and_item_action() {
        let mut c = core();
        c.save_scene(&json!({ "id": "sc-cam", "name": "카메라 전체", "layers": [{ "source": "camera:c1" }, { "source": "live", "tpl": "overlay" }] })).unwrap();
        c.save_scene(&json!({ "id": "sc-text", "name": "본문 전체", "layers": [{ "source": "background" }, { "source": "live" }] })).unwrap();
        c.new_cue("주일").unwrap();
        for t in ["하나", "둘", "셋", "넷"] {
            c.add_item(crate::core::new_item("title", t), None);
        }
        // 셋째 순서: 송출할 때 예배당 기본을 '본문 전체' 로 (남는 동작)
        let mut it = c.cue.as_ref().unwrap().items[2].clone();
        it["actions"] = json!({ "scene": { "main": "sc-text", "nope": "sc-text" } });
        crate::actions::normalize_item_actions(&mut it);
        assert_eq!(it["actions"]["scene"], json!({ "main": "sc-text", "nope": "sc-text" }));
        c.cue.as_mut().unwrap().items[2] = it;
        let sid = |c: &Core, o: &str| c.output_payload(o)["scene"]["id"].as_str().unwrap_or("").to_string();
        assert!(c.send_item(0));
        assert_eq!(c.output_payload("overlay")["sceneTransition"], json!({ "type": "move", "ms": 400 }));
        // 지금 한 번 (외부 단추)
        let q = |p: &[(&str, &str)]| p.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect::<std::collections::HashMap<_, _>>();
        crate::trigger::run(&mut c, "scene", &q(&[("output", "overlay"), ("id", "카메라 전체")])).unwrap();
        assert_eq!(sid(&c, "overlay"), "sc-cam");
        assert_eq!(c.scene_choice("overlay", true).1, crate::outputs::From::Once);
        assert_eq!(c.scene_choice("overlay", false).0, "full", "미리보기 묶음에는 한 번 지정이 안 붙음");
        assert!(c.send_item(0), "같은 순서를 다시 보내도 그대로");
        assert_eq!(sid(&c, "overlay"), "sc-cam");
        assert!(c.send_item(1));
        assert_eq!(sid(&c, "overlay"), "full", "다음 순서에서 되돌아옴");
        assert_eq!(crate::scenes::output_scene_id(c.settings.get(), "overlay"), "full", "저장 안 함");
        // hold = 출력 기본
        crate::trigger::run(&mut c, "scene", &q(&[("output", "overlay"), ("id", "sc-cam"), ("hold", "1")])).unwrap();
        assert!(c.send_item(2));
        assert_eq!(sid(&c, "overlay"), "sc-cam", "hold 는 남음");
        assert_eq!(sid(&c, "main"), "sc-text", "순서 동작 '구성 바꾸기' (모르는 출력은 건너뜀)");
        assert!(c.send_item(3));
        assert_eq!((sid(&c, "main"), sid(&c, "overlay")), ("sc-text".into(), "sc-cam".into()));
        // 모든 출력 한 번 · 풀기 · 없는 구성
        crate::trigger::run(&mut c, "scene", &q(&[("output", "all"), ("id", "full")])).unwrap();
        assert_eq!((sid(&c, "main"), sid(&c, "overlay")), ("full".into(), "full".into()));
        crate::trigger::run(&mut c, "scene", &q(&[("output", "all")])).unwrap();
        assert_eq!(sid(&c, "overlay"), "sc-cam");
        assert!(crate::trigger::run(&mut c, "scene", &q(&[("output", "overlay"), ("id", "없는 구성")])).is_err());
        assert!(crate::trigger::run(&mut c, "scene", &q(&[("output", "로비"), ("id", "full")])).is_err());
        // 전환: 구성 → 출력 → 전체
        c.set_scene_transition("output:overlay", &json!({ "type": "fade", "ms": 800 })).unwrap();
        assert_eq!(c.output_payload("overlay")["sceneTransition"], json!({ "type": "fade", "ms": 800 }));
        c.set_scene_transition("scene:sc-cam", &json!({ "type": "cut" })).unwrap();
        assert_eq!(c.output_payload("overlay")["sceneTransition"]["type"], "cut");
        // 요약 글
        assert!(crate::actions::summary(c.settings.get(), &c.cue.as_ref().unwrap().items[2]).contains("본문 전체"));
    }

    /// 실시간 자막(AI 5번)은 'caption' 소스: 띄운 동안만 기본 구성 · 맨 위 자동 칸에 생기고, 구성에 넣으면 그 자리
    #[test]
    fn caption_is_a_source() {
        let cap = json!({ "id": crate::voicefollow::CAPTION_ID, "type": "banner" });
        let ids = |sc: &Value| sc["layers"].as_array().unwrap().iter().map(|l| l["source"].as_str().unwrap().to_string()).collect::<Vec<_>>();
        let d = crate::sources::default_scene("overlay", &[cap.clone()], &[]);
        assert_eq!(ids(&d), ["background", "media", "slide-bg", "live", "caption", "message", "logo-screen", "draw", "pip"]);
        assert!(!ids(&crate::sources::default_scene("overlay", &[], &[])).contains(&"caption".to_string()), "자막이 없으면 칸도 없음 (2.0.8 과 같은 차례)");
        let sc = json!({ "id": "sc-a", "layers": [{ "id": "l1", "source": "camera:c1" }, { "id": "l2", "source": "caption", "rect": [0, 0, 1, 0.3] }, { "id": "l3", "source": "live" }] });
        assert!(crate::scenes::ok_source("caption"));
        let e = crate::scenes::effective_scene(&sc, "overlay", &[cap], &|_| None);
        assert_eq!(ids(&e)[..3], ["camera:c1", "caption", "live"]);
        assert_eq!(ids(&e).iter().filter(|x| *x == "caption").count(), 1);
    }

    #[test]
    fn action_normalize() {
        assert_eq!(normalize_action(&json!({ "overlay": "sc-a", "bad key!": "x", "main": 3 })), Some(json!({ "overlay": "sc-a" })));
        assert_eq!(normalize_action(&json!({})), None);
        assert_eq!(normalize_action(&json!("x")), None);
    }
}
