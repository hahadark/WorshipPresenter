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
        self.plan_save(plan, true)
    }

    /// 위와 같되 `open=false` 면 저장만 하고 화면(열린 큐시트)은 바꾸지 않는다 — 결과 카드의 [이 큐시트 열기] 로 연다.
    /// 반환에 순서 목록 `items`(제목·부제·장절)이 들어 있다.
    pub fn plan_save(&mut self, plan: &Value, open: bool) -> Result<Value, String> {
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
        if open {
            self.open_cue(&c.id);
        } else {
            self.emit(Ev::Ui);
        }
        Ok(json!({ "id": c.id, "name": c.name, "count": c.items.len(), "warnings": warnings, "items": item_summary(&c.items) }))
    }

    /// 결과 카드의 [이 큐시트 열기]
    pub fn open_saved_cue(&mut self, id: &str) -> bool {
        if self.store.list_cues().iter().any(|c| c.id == id) {
            self.open_cue(id);
            true
        } else {
            false
        }
    }

    /// 화면용: AI 갈래 설정 (mode: quick|api|chat, 키 없고 저장값 없으면 quick, 있으면 api)
    pub fn ai_mode(&self) -> String {
        let has_key = !self.ai_key().is_empty();
        resolve_mode(self.settings.get()["ai"]["mode"].as_str().unwrap_or(""), has_key)
    }

    pub fn set_ai_mode(&mut self, mode: &str) {
        let m = mode.to_string();
        self.settings.update(|d| {
            if !d["ai"].is_object() {
                d["ai"] = json!({});
            }
            d["ai"]["mode"] = json!(m);
        });
    }

    /// '순서 붙여 넣기' 초안 (재시작해도 남게): (큐시트 이름, 글)
    pub fn quick_draft(&self) -> (String, String) {
        let a = &self.settings.get()["ai"];
        (a["quickName"].as_str().unwrap_or("").to_string(), a["quickDraft"].as_str().unwrap_or("").to_string())
    }

    pub fn set_quick_draft(&mut self, name: &str, text: &str) {
        let (n, t) = (name.to_string(), text.to_string());
        self.settings.update(|d| {
            if !d["ai"].is_object() {
                d["ai"] = json!({});
            }
            d["ai"]["quickName"] = json!(n);
            d["ai"]["quickDraft"] = json!(t);
        });
    }

    /// 붙여 넣은 예배 순서 글 → 큐시트 (AI 없이 규칙으로)
    pub fn quick_make_cue(&mut self, text: &str) -> Result<Value, String> {
        self.quick_make_cue_ex(text, "", true)
    }

    /// name 이 비어 있지 않으면 큐시트 이름으로 쓴다
    pub fn quick_make_cue_ex(&mut self, text: &str, name: &str, open: bool) -> Result<Value, String> {
        if text.trim().is_empty() {
            return Err(crate::i18n::tr("예배 순서를 붙여 넣으세요.").into());
        }
        let mut plan = bulletin::parse_order_text(text, &|s| crate::parser::looks_like_reference(s));
        if !name.trim().is_empty() {
            plan["name"] = json!(name.trim());
        }
        self.plan_save(&plan, open)
    }

    /// 채팅 AI 의 답 → 큐시트
    pub fn chat_ai_import(&mut self, answer: &str) -> Result<Value, String> {
        self.chat_ai_import_ex(answer, true)
    }

    pub fn chat_ai_import_ex(&mut self, answer: &str, open: bool) -> Result<Value, String> {
        let (plan, is_sample) = bulletin::parse_chat_answer(answer)?;
        if is_sample {
            return Err(crate::i18n::tr("샘플 내용 그대로입니다. 주보를 첨부해 다시 만들어 달라고 하세요.").into());
        }
        self.plan_save(&plan, open)
    }

    /// 주보 파일 → 글 (규칙 파서에 넣을 수 있게) 또는 AI 에 보낼 입력
    pub fn read_bulletin_file(path: &std::path::Path) -> Result<Input, String> {
        let buf = std::fs::read(path).map_err(|e| format!("파일을 읽지 못했습니다: {e}"))?;
        bulletin::read_bulletin(&buf, &path.to_string_lossy())
    }
}

/// 갈래 이름 정리: 저장값이 이상하면 키 유무로 (키 없음 → 순서 붙여 넣기, 있음 → API 키)
pub fn resolve_mode(saved: &str, has_key: bool) -> String {
    match saved {
        "quick" | "api" | "chat" => saved.to_string(),
        _ => if has_key { "api" } else { "quick" }.to_string(),
    }
}

/// 큐시트 순서 목록 요약 (결과 카드용): [{ title, subtitle, ref }]
pub fn item_summary(items: &[Value]) -> Vec<Value> {
    items.iter().map(|x| json!({ "title": x["title"].as_str().unwrap_or(""), "subtitle": x["subtitle"].as_str().unwrap_or(""), "ref": x["ref"].as_str().unwrap_or("") })).collect()
}

/// 채팅 AI 결과 파일(.json/.txt/.md) 바이트 → 글 (UTF-8 머리표 제거, 너무 크면 거절)
pub fn read_answer_bytes(buf: &[u8]) -> Result<String, String> {
    if buf.len() > 2_000_000 {
        return Err(crate::i18n::tr("결과 파일이 너무 큽니다.").into());
    }
    let b = buf.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(buf);
    let t = String::from_utf8_lossy(b).to_string();
    if t.trim().is_empty() {
        return Err(crate::i18n::tr("결과 파일이 비어 있습니다.").into());
    }
    Ok(t)
}

/// AI API 호출 한 건의 진행 상태 (한 번에 하나만). 시험할 수 있게 시계를 밖에서 받는다.
#[derive(Default, Debug)]
pub struct AiJob {
    pub started: Option<web_time::Instant>,
    pub done: Option<Result<Value, String>>,
}

impl AiJob {
    /// 시작 시도 — 이미 돌고 있으면 false (중복 방지)
    pub fn try_begin(&mut self) -> bool {
        if self.started.is_some() {
            return false;
        }
        self.started = Some(web_time::Instant::now());
        self.done = None;
        true
    }
    pub fn running(&self) -> bool {
        self.started.is_some()
    }
    pub fn elapsed_secs(&self) -> u64 {
        self.started.map(|t| t.elapsed().as_secs()).unwrap_or(0)
    }
    /// 끝남: 결과에 걸린 시간을 붙이고 '돌고 있음' 을 푼다
    pub fn finish(&mut self, r: Result<Value, String>) {
        let secs = self.elapsed_secs();
        self.started = None;
        self.done = Some(r.map(|mut v| {
            v["secs"] = json!(secs);
            v
        }));
    }
    pub fn take_done(&mut self) -> Option<Result<Value, String>> {
        self.done.take()
    }
}

static AI_JOB: once_cell::sync::Lazy<parking_lot::Mutex<AiJob>> = once_cell::sync::Lazy::new(|| parking_lot::Mutex::new(AiJob::default()));

pub fn ai_job_running() -> bool {
    AI_JOB.lock().running()
}
pub fn ai_job_secs() -> u64 {
    AI_JOB.lock().elapsed_secs()
}
pub fn ai_job_take() -> Option<Result<Value, String>> {
    AI_JOB.lock().take_done()
}

/// usage.cost(오픈라우터가 주는 달러 값)가 있으면 글로
pub fn cost_text(usage: &Value) -> Option<String> {
    let c = usage["cost"].as_f64().or_else(|| usage["cost"].as_str().and_then(|s| s.parse().ok()))?;
    Some(format!("${c:.4}"))
}

/// 백그라운드에서 AI 로 주보 → 큐시트를 만들어 **저장만** 한다 (열지 않음). 결과는 `ai_job_take()` 로 가져가 카드로 보여 준다.
/// 이미 돌고 있으면 false.
pub fn ai_make_cue_job(shared: &crate::core::Shared, input: Input, extra: String) -> bool {
    if !AI_JOB.lock().try_begin() {
        return false;
    }
    let (key, model, endpoint) = {
        let c = shared.lock();
        let a = c.settings.get()["ai"].clone();
        (c.ai_key(), a["model"].as_str().unwrap_or("").to_string(), a["endpoint"].as_str().unwrap_or("").to_string())
    };
    let sh = shared.clone();
    crate::bg::spawn(move || {
        let r = bulletin::ask_ai(&key, &model, &endpoint, &input, &extra);
        let out = r.and_then(|(plan, usage, m)| {
            let mut v = sh.lock().plan_save(&plan, false)?;
            v["model"] = json!(m);
            if let Some(c) = cost_text(&usage) {
                v["cost"] = json!(c);
            }
            Ok(v)
        });
        AI_JOB.lock().finish(out);
    });
    true
}

/// (예전 방식) 백그라운드에서 AI 로 주보 → 큐시트 (끝나면 알림·큐시트 열기)
pub fn ai_make_cue(shared: &crate::core::Shared, input: Input, extra: String) {
    let (key, model, endpoint) = {
        let c = shared.lock();
        let a = c.settings.get()["ai"].clone();
        (c.ai_key(), a["model"].as_str().unwrap_or("").to_string(), a["endpoint"].as_str().unwrap_or("").to_string())
    };
    let sh = shared.clone();
    shared.lock().notify(crate::i18n::tr("AI 가 주보를 읽는 중…"), "info");
    crate::bg::spawn(move || {
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
    #[cfg(target_arch = "wasm32")]
    let r: std::io::Result<()> = { let _ = target; Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "웹판")) }; // [웹판]
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
        c.output_on_requests += 1;
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
    if a["scene"].is_object() {
        c.run_scene_action(&a["scene"]); // 화면 구성 바꾸기 (출력 기본, 남음)
    }
    match s("stagePin").as_str() {
        "" => {}
        "live" => { let _ = c.stage_pin_set(None); }
        "order" => { let _ = c.stage_pin_set(Some(&json!({ "mode": "order", "name": "순서표" }))); }
        // 프리셋 id → settings.stagePresets 에서 찾아 고정. 없으면 고정은 그대로 두고 알림 (Electron stagePinSet 은 오류)
        p => {
            if let Err(e) = c.stage_pin_set(Some(&json!({ "preset": p }))) {
                c.notify(e.to_string(), "warn");
            }
        }
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
    crate::bg::spawn(move || {
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

    /// 검토 1113-3 결함 5: 순서 자동 동작 stagePin = 프리셋 id 는 고정을 푸는 게 아니라 그 프리셋으로 고정 (Electron stagePinSet {preset})
    #[test]
    fn stage_pin_preset_action_pins_preset_and_unknown_keeps_pin() {
        let dir = std::env::temp_dir().join(format!("wp-trackd-pin-{}-{}", crate::util::now_ms(), rand::random::<u32>()));
        std::fs::create_dir_all(&dir).unwrap();
        drop(rusqlite::Connection::open(dir.join("bible.db")).unwrap());
        let mut c = Core::open(dir.clone(), dir.join("web"), None).unwrap();
        let pid = c.save_stage_preset(&json!({ "id": "sermon1", "name": "설교 요약", "mode": "text", "text": "1. 은혜\n2. 응답" })).unwrap();
        assert_eq!(pid, "sermon1");
        // 순서표로 고정된 상태에서 프리셋 동작 → 프리셋으로 바뀜
        run_item_actions(&mut c, &json!({ "stagePin": "order" }), 0);
        assert_eq!(c.b.stage.pin.as_ref().unwrap()["mode"], "order");
        run_item_actions(&mut c, &json!({ "stagePin": "sermon1" }), 0);
        let pin = c.b.stage.pin.clone().expect("프리셋으로 고정되어야 함 (풀리면 안 됨)");
        assert_eq!(pin["mode"], "text");
        assert_eq!(pin["name"], "설교 요약");
        assert_eq!(pin["text"], "1. 은혜\n2. 응답");
        assert_eq!(pin["id"], "sermon1");
        assert_eq!(c.stage_payload()["pin"]["mode"], "text");
        // 없는 프리셋: 고정은 그대로, 오류 알림
        run_item_actions(&mut c, &json!({ "stagePin": "nope" }), 0);
        assert_eq!(c.b.stage.pin.as_ref().unwrap()["id"], "sermon1", "못 찾으면 고정을 바꾸지 않는다");
        assert!(c.stage_pin_set(Some(&json!({ "preset": "nope" }))).is_err());
        // 'live' 는 풀기
        run_item_actions(&mut c, &json!({ "stagePin": "live" }), 0);
        assert!(c.b.stage.pin.is_none());
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[cfg(test)]
mod ai_card_tests {
    use super::*;

    fn core() -> Core {
        let dir = std::env::temp_dir().join(format!("wp-aicard-{}-{}", crate::util::now_ms(), crate::util::short_id("t")));
        std::fs::create_dir_all(&dir).unwrap();
        drop(rusqlite::Connection::open(dir.join("bible.db")).unwrap());
        Core::open(dir.clone(), dir.join("web"), None).unwrap()
    }

    #[test]
    fn mode_defaults_and_is_remembered() {
        assert_eq!(resolve_mode("", false), "quick");
        assert_eq!(resolve_mode("", true), "api");
        assert_eq!(resolve_mode("chat", true), "chat");
        assert_eq!(resolve_mode("zzz", false), "quick");
        let mut c = core();
        assert_eq!(c.ai_mode(), "quick");
        c.set_ai_mode("chat");
        assert_eq!(c.ai_mode(), "chat");
    }

    #[test]
    fn custom_model_and_draft_are_saved() {
        let mut c = core();
        c.set_ai(None, Some("vendor/my-model:free"), None);
        assert_eq!(c.ai_settings()["model"], "vendor/my-model:free");
        c.set_quick_draft("11월 15일", "찬송 8장\n광고");
        assert_eq!(c.quick_draft(), ("11월 15일".to_string(), "찬송 8장\n광고".to_string()));
    }

    #[test]
    fn duplicate_ai_run_is_blocked() {
        let mut j = AiJob::default();
        assert!(j.try_begin());
        assert!(j.running());
        assert!(!j.try_begin(), "실행 중에는 다시 시작할 수 없다");
        j.finish(Ok(json!({ "name": "x" })));
        assert!(!j.running());
        let r = j.take_done().unwrap().unwrap();
        assert!(r["secs"].is_u64());
        assert!(j.take_done().is_none());
        assert!(j.try_begin(), "끝나면 다시 가능");
    }

    #[test]
    fn card_save_does_not_open_cue() {
        let mut c = core();
        assert!(c.cue.is_none());
        let r = c.quick_make_cue_ex("2026-11-15 주일 예배\n대표기도  김철수\n광고", "내 큐시트", false).unwrap();
        assert!(c.cue.is_none(), "카드 단계에서는 화면의 큐시트가 바뀌지 않아야 한다");
        assert_eq!(r["name"], "내 큐시트");
        assert_eq!(r["items"].as_array().unwrap().len(), r["count"].as_u64().unwrap() as usize);
        let id = r["id"].as_str().unwrap().to_string();
        assert!(!c.open_saved_cue("없는-id"));
        assert!(c.open_saved_cue(&id));
        assert_eq!(c.cue.as_ref().unwrap().id, id);
    }

    #[test]
    fn answer_file_parsing() {
        let t = read_answer_bytes(b"\xEF\xBB\xBF{\"items\":[]}").unwrap();
        assert!(t.starts_with('{'));
        assert!(read_answer_bytes(b"  \n").is_err());
        assert!(read_answer_bytes(&vec![b'a'; 2_000_001]).is_err());
        let mut c = core();
        let ans = "```json\n{\"name\":\"파일 큐시트\",\"items\":[{\"type\":\"title\",\"title\":\"예배 후\"}]}\n```";
        let r = c.chat_ai_import_ex(&read_answer_bytes(ans.as_bytes()).unwrap(), false).unwrap();
        assert_eq!(r["name"], "파일 큐시트");
        assert!(c.cue.is_none());
    }

    #[test]
    fn cost_text_only_with_cost() {
        assert_eq!(cost_text(&json!({ "cost": 0.01234 })).unwrap(), "$0.0123");
        assert!(cost_text(&json!({ "total_tokens": 10 })).is_none());
    }
}
