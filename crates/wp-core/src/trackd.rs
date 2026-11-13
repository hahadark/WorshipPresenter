//! [트랙 D] 코어 연결: 주보 → 큐시트 저장 · AI 설정(키는 DPAPI 로 암호화) · 링크/폴더 열기

use crate::bulletin::{self, Input, Resolvers};
use crate::core::{Core, Ev};
use crate::db::Cue;
use crate::util::short_id;
use serde_json::{json, Value};
use std::collections::HashMap;

impl Core {
    /// 저장된 AI 키 (keyEnc 를 풀고, 없으면 예전 평문 key)
    pub fn ai_key(&self) -> String {
        let a = &self.settings.get()["ai"];
        if let Some(enc) = a["keyEnc"].as_str().filter(|x| !x.is_empty()) {
            if let Some(k) = crate::dpapi::decrypt(enc) {
                return k;
            }
        }
        a["key"].as_str().unwrap_or("").to_string()
    }

    /// AI 설정 (키를 화면에 돌려주지 않는다)
    pub fn ai_settings(&self) -> Value {
        let a = &self.settings.get()["ai"];
        json!({ "model": a["model"].as_str().filter(|x| !x.is_empty()).unwrap_or(bulletin::DEFAULT_MODEL), "endpoint": a["endpoint"].as_str().unwrap_or(""), "hasKey": !self.ai_key().is_empty(),
            "models": bulletin::RECOMMENDED.iter().map(|(id, n)| json!({ "id": id, "name": n })).collect::<Vec<_>>() })
    }

    /// key: None = 그대로, Some("") = 지우기
    pub fn set_ai(&mut self, key: Option<&str>, model: Option<&str>, endpoint: Option<&str>) {
        let enc = key.map(|k| if k.trim().is_empty() { String::new() } else { crate::dpapi::encrypt(k.trim()) });
        let (model, endpoint) = (model.map(String::from), endpoint.map(String::from));
        self.settings.update(|d| {
            if !d["ai"].is_object() {
                d["ai"] = json!({});
            }
            if let Some(e) = enc {
                d["ai"]["keyEnc"] = json!(e);
                d["ai"]["key"] = json!("");
            }
            if let Some(m) = model {
                d["ai"]["model"] = json!(m);
            }
            if let Some(e) = endpoint {
                d["ai"]["endpoint"] = json!(e);
            }
        });
        self.emit(Ev::Ui);
    }

    /// 계획(AI·규칙·채팅 AI) → 찬송가 파일·장절 확인 → 새 큐시트로 저장하고 연다. 반환: { id, name, count, warnings }
    pub fn plan_to_saved_cue(&mut self, plan: &Value) -> Result<Value, String> {
        // 장절 확인은 DB 를 바꿔 쓰므로 먼저 해 둔다
        let v = self.version_id();
        let mut refs: HashMap<String, Option<String>> = HashMap::new();
        for x in plan["items"].as_array().into_iter().flatten() {
            if let Some(r) = x["ref"].as_str() {
                let res = self.store.resolve_reference(&v, r);
                refs.insert(r.to_string(), if res["ok"] == json!(true) { res["passage"]["title"].as_str().map(String::from) } else { None });
            }
        }
        let core: &Core = self;
        let hymn = |n: i64, book: &str| {
            let hits: Vec<_> = crate::hymns::search(core, &n.to_string()).into_iter().filter(|h| h.number == Some(n)).collect();
            let want_sa = book.contains("구세군");
            let pick = hits
                .iter()
                .find(|h| if want_sa { h.folder.contains("구세군") } else { h.folder.contains("찬송") && !h.folder.contains("구세군") })
                .or_else(|| hits.iter().find(|h| !h.folder.contains("교독문")));
            pick.map(|h| (h.file.to_string_lossy().to_string(), h.title.clone()))
        };
        let check_ref = |r: &str| refs.get(r).cloned().flatten();
        let hymnal = |n: i64, book: &str| {
            if book.contains("교독문") {
                // 새찬송가 교독문 n 번 → 베들레헴 장 번호
                return crate::hymnal::search(core, &format!("교독문 {n}"), 5).into_iter().find_map(|h| h["number"].as_i64().and_then(|ch| crate::hymnal::get(core, "새찬송가", ch).ok())).filter(|b| b["kind"] == "reading");
            }
            crate::hymnal::get(core, if book.contains("찬미가") { "찬미가" } else { "새찬송가" }, n).ok()
        };
        let song = |name: &str| {
            let norm = |t: &str| t.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_lowercase();
            let q = norm(name);
            let hits = crate::songs::list_songs(&core.store.db, name);
            let s = hits.iter().find(|x| norm(x["title"].as_str().unwrap_or("")) == q).or_else(|| hits.iter().find(|x| norm(x["title"].as_str().unwrap_or("")).starts_with(&q))).or(if hits.len() == 1 { hits.first() } else { None });
            s.and_then(|x| crate::songs::get_song(&core.store.db, x["id"].as_str().unwrap_or("")))
        };
        let (cue, warnings) = bulletin::plan_to_cue(plan, &Resolvers { hymn: &hymn, check_ref: &check_ref, hymnal: &hymnal, song: &song })?;
        let c = Cue {
            id: short_id("c"),
            name: cue["name"].as_str().unwrap_or("새 예배").to_string(),
            service_date: cue["serviceDate"].as_str().unwrap_or("").to_string(),
            start_time: cue["startTime"].as_str().unwrap_or("").to_string(),
            items: cue["items"].as_array().cloned().unwrap_or_default(),
            memo: String::new(),
            pinned: false,
            template: false,
        };
        self.store.save_cue(&c).map_err(|e| e.to_string())?;
        self.cues = self.store.list_cues();
        self.open_cue(&c.id);
        Ok(json!({ "id": c.id, "name": c.name, "count": c.items.len(), "warnings": warnings }))
    }

    /// 붙여 넣은 예배 순서 글 → 큐시트 (AI 없이 규칙으로)
    pub fn quick_make_cue(&mut self, text: &str) -> Result<Value, String> {
        if text.trim().is_empty() {
            return Err(crate::i18n::tr("예배 순서를 붙여 넣으세요.").into());
        }
        let plan = bulletin::parse_order_text(text, &|s| crate::parser::looks_like_reference(s));
        self.plan_to_saved_cue(&plan)
    }

    /// 채팅 AI 의 답 → 큐시트
    pub fn chat_ai_import(&mut self, answer: &str) -> Result<Value, String> {
        let (plan, is_sample) = bulletin::parse_chat_answer(answer)?;
        if is_sample {
            return Err(crate::i18n::tr("샘플 내용 그대로입니다. 주보를 첨부해 다시 만들어 달라고 하세요.").into());
        }
        self.plan_to_saved_cue(&plan)
    }

    /// 주보 파일 → 글 (규칙 파서에 넣을 수 있게) 또는 AI 에 보낼 입력
    pub fn read_bulletin_file(path: &std::path::Path) -> Result<Input, String> {
        let buf = std::fs::read(path).map_err(|e| format!("파일을 읽지 못했습니다: {e}"))?;
        bulletin::read_bulletin(&buf, &path.to_string_lossy())
    }
}

/// 백그라운드에서 AI 로 주보 → 큐시트 (끝나면 알림·큐시트 열기)
pub fn ai_make_cue(shared: &crate::core::Shared, input: Input, extra: String) {
    let (key, model, endpoint) = {
        let c = shared.lock();
        let a = c.settings.get()["ai"].clone();
        (c.ai_key(), a["model"].as_str().unwrap_or("").to_string(), a["endpoint"].as_str().unwrap_or("").to_string())
    };
    let sh = shared.clone();
    shared.lock().notify(crate::i18n::tr("AI 가 주보를 읽는 중…"), "info");
    std::thread::spawn(move || {
        let r = bulletin::ask_ai(&key, &model, &endpoint, &input, &extra);
        let mut c = sh.lock();
        match r.and_then(|(plan, _usage, _m)| c.plan_to_saved_cue(&plan)) {
            Ok(v) => {
                let w = v["warnings"].as_array().map(|a| a.len()).unwrap_or(0);
                c.notify(crate::i18n::tr_args("'{1}' 큐시트를 만들었습니다 ({2}개 순서{3})", &[&(v["name"].as_str().unwrap_or("")), &(v["count"]), &(if w > 0 { format!(", 확인할 것 {w}개") } else { String::new() })]), "ok");
            }
            Err(e) => c.notify(e, "error"),
        }
    });
}

/// 웹 주소를 기본 브라우저로 (http/https 만)
pub fn open_link(url: &str) -> Result<(), String> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(crate::i18n::tr("웹 주소만 열 수 있습니다.").into());
    }
    open_with_shell(url)
}

/// 폴더·파일을 탐색기로
pub fn open_path(p: &std::path::Path) -> Result<(), String> {
    if !p.exists() {
        return Err(crate::i18n::tr("폴더를 찾을 수 없습니다.").into());
    }
    open_with_shell(&p.to_string_lossy())
}

fn open_with_shell(target: &str) -> Result<(), String> {
    #[cfg(windows)]
    let r = std::process::Command::new("explorer.exe").arg(target).spawn();
    #[cfg(target_os = "macos")]
    let r = std::process::Command::new("open").arg(target).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let r = std::process::Command::new("xdg-open").arg(target).spawn();
    r.map(|_| ()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quick_cue_saves() {
        let dir = std::env::temp_dir().join(format!("wp-trackd-{}", crate::util::now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        drop(rusqlite::Connection::open(dir.join("bible.db")).unwrap()); // 기존 자료를 복사해 오지 않게
        let mut c = Core::open(dir.clone(), dir.join("web"), None).unwrap();
        let r = c.quick_make_cue("2026-11-15 주일 예배\n찬송 8장\n대표기도  김철수\n광고\n예배 후").unwrap();
        assert_eq!(r["count"], 4);
        let cue = c.cue.clone().unwrap();
        assert_eq!(cue.service_date, "2026-11-15");
        // 찬송가 폴더가 없으니 베들레헴 가사(이 PC 에 있으면 찬양) 또는 제목으로
        assert!(cue.items[0]["type"] == "song" || cue.items[0]["type"] == "title");
        assert_eq!(cue.items[3]["type"], "blank");
        assert!(c.chat_ai_import(&bulletin::sample().to_string()).is_err());
        assert!(open_link("file:///c:/x").is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}

// ---------------- 송출할 때 (core.send_preview · send_deck 끝에서 부름) ----------------
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};

/// 예배당 창을 켜 달라는 요청 (조작 화면이 프레임마다 가져감)
static WANT_OUTPUT_ON: AtomicBool = AtomicBool::new(false);
/// 자동 동작을 마지막으로 실행한 송출 (sentAt) — 같은 송출에서 두 번 돌지 않게
static LAST_ACTIONS: AtomicI64 = AtomicI64::new(0);
/// 조작 화면이 알려 주는 예배당 창 상태 (outputStatus.desktop)
static DESKTOP: once_cell::sync::Lazy<parking_lot::Mutex<Value>> = once_cell::sync::Lazy::new(|| parking_lot::Mutex::new(Value::Null));

pub fn take_output_on_request() -> bool {
    WANT_OUTPUT_ON.swap(false, Ordering::SeqCst)
}
pub fn set_desktop_status(v: Value) {
    *DESKTOP.lock() = v;
}
pub fn desktop_status() -> Value {
    DESKTOP.lock().clone()
}

pub fn on_send(c: &mut Core) {
    let Some(l) = c.pres.live.clone() else { return };
    // 송출을 시작하면 예배당 모니터 창을 자동으로 켠다 (설정 output.autoOnSend)
    let auto = c.settings.get()["output"]["autoOnSend"].as_bool().unwrap_or(true);
    if auto && !DESKTOP.lock()["running"].as_bool().unwrap_or(false) {
        WANT_OUTPUT_ON.store(true, Ordering::SeqCst);
    }
    // 큐시트 순서의 '송출할 때 자동으로' (item.actions)
    let ci = l.deck.passage["cueIndex"].as_i64().unwrap_or(-1);
    if ci < 0 || LAST_ACTIONS.swap(l.sent_at, Ordering::SeqCst) == l.sent_at {
        return;
    }
    let it = c.cue.as_ref().and_then(|q| q.items.get(ci as usize)).cloned();
    if let Some(it) = it {
        if it["actions"].is_object() {
            run_item_actions(c, &it["actions"], 0);
        }
    }
}

/// 예배당 창을 껐다 켰을 때: 끄면 OBS 도 숨김(설정 output.linkObs), 켜면 다시 보임
pub fn output_toggled(c: &mut Core, before: bool, after: bool) {
    let link = c.settings.get()["output"]["linkObs"].as_bool().unwrap_or(true);
    let hold = if before && !after && link { Some(true) } else if !before && after { Some(false) } else { None };
    if let Some(h) = hold {
        if c.obs_hold != h {
            c.obs_hold = h;
            c.emit(Ev::Live);
        }
    }
    c.emit(Ev::Outputs);
}

/// Electron 판 cue.js runActions 와 같은 칸 (음악 재생은 조작 화면 몫이라 뺌)
pub fn run_item_actions(c: &mut Core, a: &Value, depth: u8) {
    let s = |k: &str| a[k].as_str().unwrap_or("").to_string();
    if !s("macro").is_empty() && depth < 2 {
        let id = s("macro");
        if let Err(e) = c.run_macro_depth(&id, depth) {
            c.notify(crate::i18n::tr_args("매크로: {1}", &[&e]), "warn");
        }
    }
    if let Some(m) = a["timer"].as_f64().filter(|x| *x > 0.0) {
        let _ = c.stage_timer("start", m);
    }
    if !s("stageMsg").is_empty() {
        c.stage_message(&s("stageMsg"));
    } else if a["clearStage"].as_bool().unwrap_or(false) {
        c.stage_message("");
    }
    if !s("logo").is_empty() {
        c.layers.logo = s("logo") == "on";
    }
    if !s("bg").is_empty() {
        // Electron toggleBackground({on}) 과 같이 직접 정한 배경은 다음 송출 때 저절로 되살리지 않는다
        c.layers.hide_bg = s("bg") == "hide";
        c.layers.bg_by_clear = false;
    }
    if !s("ovOn").is_empty() {
        let _ = c.toggle_overlay(&s("ovOn"), Some(true));
    }
    if s("ovOff") == "*" {
        let on: Vec<String> = c.settings.get()["overlays"].as_array().into_iter().flatten().filter(|o| o["on"] == true && o["id"] != a["ovOn"]).filter_map(|o| o["id"].as_str().map(String::from)).collect();
        for id in on {
            let _ = c.toggle_overlay(&id, Some(false));
        }
    } else if !s("ovOff").is_empty() {
        let _ = c.toggle_overlay(&s("ovOff"), Some(false));
    }
    match s("stagePin").as_str() {
        "" => {}
        "live" => c.stage_pin_set(None),
        "order" => c.stage_pin_set(Some(&json!({ "mode": "order", "name": "순서표" }))),
        p => c.stage_pin_set(Some(&json!({ "preset": p }))),
    }
    // 네트워크 쪽 (장비 · Companion · OBS) 은 백그라운드에서 — 송출이 멈추지 않게
    let devices = c.settings.get()["devices"].clone();
    let companion = c.settings.get()["companion"].clone();
    let obs = crate::obs::conf_from(c.settings.get());
    let devs: Vec<(String, String)> = a["dev"].as_array().into_iter().flatten().map(|x| (x["device"].as_str().unwrap_or("").to_string(), x["action"].as_str().unwrap_or("").to_string())).collect();
    let comp = s("companion");
    let mut obs_acts = vec![];
    if !s("obsScene").is_empty() {
        obs_acts.push(json!({ "action": "scene", "scene": s("obsScene") }));
    }
    if !s("obsStream").is_empty() {
        obs_acts.push(json!({ "action": "stream", "on": s("obsStream") == "start" }));
    }
    if !s("obsRecord").is_empty() {
        obs_acts.push(json!({ "action": "record", "on": s("obsRecord") == "start" }));
    }
    if devs.is_empty() && comp.is_empty() && obs_acts.is_empty() {
        c.emit(Ev::Live);
        c.emit(Ev::Ui);
        return;
    }
    let sh = crate::hymns::shared();
    std::thread::spawn(move || {
        let mut errs = vec![];
        for (d, act) in devs {
            if let Err(e) = crate::devices::run(&devices, &d, &act) {
                errs.push(format!("장비: {e}"));
            }
        }
        if !comp.is_empty() {
            if let Err(e) = crate::companion::press(&companion, &comp) {
                errs.push(format!("Companion: {e}"));
            }
        }
        for x in obs_acts {
            if let Err(e) = crate::obs::action(&obs, &x) {
                errs.push(e);
            }
        }
        if let (Some(sh), false) = (sh, errs.is_empty()) {
            sh.lock().notify(errs.join(" · "), "error");
        }
    });
    c.emit(Ev::Live);
    c.emit(Ev::Ui);
}

impl Core {
    /// OBS 설정 저장 (비밀번호는 DPAPI 로)
    pub fn set_obs(&mut self, enabled: bool, host: &str, port: u16, password: Option<&str>) {
        let enc = password.map(|p| if p.is_empty() { String::new() } else { crate::dpapi::encrypt(p) });
        let host = host.to_string();
        self.settings.update(|d| {
            if !d["obs"].is_object() {
                d["obs"] = json!({});
            }
            d["obs"]["enabled"] = json!(enabled);
            d["obs"]["host"] = json!(host);
            d["obs"]["port"] = json!(port);
            if let Some(e) = enc {
                d["obs"]["passwordEnc"] = json!(e);
                d["obs"]["password"] = json!("");
            }
        });
        self.emit(Ev::Ui);
    }
}

#[cfg(test)]
mod actions_tests {
    use super::*;
    #[test]
    fn link_obs_and_actions() {
        let dir = std::env::temp_dir().join(format!("wp-trackd-a-{}", crate::util::now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        drop(rusqlite::Connection::open(dir.join("bible.db")).unwrap());
        let mut c = Core::open(dir.clone(), dir.join("web"), None).unwrap();
        output_toggled(&mut c, true, false);
        assert!(c.obs_hold);
        output_toggled(&mut c, false, true);
        assert!(!c.obs_hold);
        run_item_actions(&mut c, &json!({ "logo": "on", "bg": "hide", "stageMsg": "마이크 켜기" }), 0);
        assert!(c.layers.logo && c.layers.hide_bg);
        assert_eq!(c.stage_payload()["message"], "마이크 켜기");
        let _ = std::fs::remove_dir_all(dir);
    }
}
