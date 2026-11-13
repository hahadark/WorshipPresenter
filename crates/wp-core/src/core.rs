//! 코어: DB · 설정 · 송출 상태 · 출력 화면에 보낼 데이터. 조작 화면(egui)과 웹 서버가 같이 쓴다.

use crate::db::{BibleStore, Cue, CueSummary};
use crate::presentation::{Deck, Presentation};
use crate::settings::Settings;
use crate::slides::{self, SplitOpts};
use crate::util::{iso_now, now_ms, short_id};
use anyhow::{Context, Result};
use parking_lot::Mutex;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::broadcast;

#[derive(Clone, Debug)]
pub enum Ev {
    /// 송출 화면을 다시 그려야 함
    Live,
    /// 조작 화면 상태가 바뀜 (미리보기 · 설정 · 큐시트)
    Ui,
    /// 출력 화면 접속 목록이 바뀜
    Outputs,
    /// 화면에 그리기 · 강조가 바뀜 (SSE 'draw') [트랙 B]
    Draw,
}

#[derive(Clone, Debug, Default)]
pub struct OutputClient {
    pub client: String,
    pub kind: String,
    pub connected_at: i64,
    pub ack: Option<Value>,
}

#[derive(Default, Clone)]
pub struct Layers {
    pub hide_bg: bool,
    /// '모두 지우기' 로 숨긴 배경인지 (Electron layers.bgByClear). 이것만 다음 송출 때 저절로 다시 보인다 — 손으로 끈 배경은 켤 때까지 그대로.
    pub bg_by_clear: bool,
    pub logo: bool,
    pub media: Option<Value>,
}

pub struct Core {
    pub data_dir: PathBuf,
    pub media_dir: PathBuf,
    pub web_dir: PathBuf,
    pub store: BibleStore,
    pub settings: Settings,
    pub pres: Presentation,
    pub layers: Layers,
    /// 화면을 바꾸지 않는 음악 재생 요청. 조작 화면이 한 번 소비한다.
    pub pending_audio: Option<Value>,
    pub obs_hold: bool,
    pub obs_full: bool, // [트랙 C] OBS 자막에 '슬라이드 그대로'
    pub outputs: HashMap<u64, OutputClient>,
    pub tx: broadcast::Sender<Ev>,
    /// 열어 둔 큐시트
    pub cue: Option<Cue>,
    pub cues: Vec<CueSummary>,
    /// 화면 아래 알림 (글, 종류, 시각)
    pub toast: Option<(String, &'static str, i64)>,
    pub port: u16,
    pub b: crate::trackb::TrackB, // [트랙 B]
    pub flow: crate::flow::Flow, // [트랙 C] 자동 넘김·글자 자동 숨김
}

pub type Shared = Arc<Mutex<Core>>;

/// 처음 실행: Electron 판 자료(%APPDATA%\WorshipCue\data)가 있으면 복사해 온다 (원본은 건드리지 않음)
pub fn prepare_data_dir(data_dir: &Path, seed_db: Option<&Path>) -> Result<Option<String>> {
    std::fs::create_dir_all(data_dir)?;
    let db = data_dir.join("bible.db");
    if db.exists() {
        return Ok(None);
    }
    let old = dirs::data_dir().map(|d| d.join("WorshipCue").join("data"));
    if let Some(old) = old.filter(|o| o.join("bible.db").exists()) {
        // 열려 있어도 안전하게: SQLite VACUUM INTO 로 한 시점의 사본
        let src = rusqlite::Connection::open_with_flags(old.join("bible.db"), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        src.execute("VACUUM INTO ?", [db.to_string_lossy().to_string()]).context("기존 자료 복사")?;
        for f in ["settings.json"] {
            let _ = std::fs::copy(old.join(f), data_dir.join(f));
        }
        let om = old.join("media");
        if om.is_dir() {
            let nm = data_dir.join("media");
            std::fs::create_dir_all(&nm)?;
            for e in std::fs::read_dir(om)?.flatten() {
                let _ = std::fs::copy(e.path(), nm.join(e.file_name()));
            }
        }
        return Ok(Some(format!("기존 워십프리젠터 자료를 복사해 왔습니다 ({})", old.display())));
    }
    if let Some(seed) = seed_db.filter(|s| s.exists()) {
        std::fs::copy(seed, &db)?;
        return Ok(Some("기본 성경 자료로 시작합니다.".into()));
    }
    Ok(None)
}

fn is_lyrics(passage: &Value, slide: Option<&Value>) -> bool {
    if passage["lyrics"].as_bool().unwrap_or(false) || passage["itemType"] == "hymn" || passage["itemType"] == "song" {
        return true;
    }
    slide.is_some_and(|s| s["kind"] == "score" || (s["kind"] == "image" && s["caption"].is_string()))
}

impl Core {
    pub fn open(data_dir: PathBuf, web_dir: PathBuf, seed_db: Option<&Path>) -> Result<Self> {
        crate::sync::apply_pending_restore(&data_dir); // [트랙 B] 예약된 백업 되돌리기 (DB 열기 전)
        let note = prepare_data_dir(&data_dir, seed_db)?;
        let media_dir = data_dir.join("media");
        std::fs::create_dir_all(&media_dir)?;
        let store = BibleStore::open(&data_dir.join("bible.db"))?;
        let _ = crate::usage::ensure(&store.db); // [트랙 C] 사용 기록 표
        let settings = Settings::load(&data_dir);
        let (tx, _) = broadcast::channel(256);
        let port = settings.get()["port"].as_u64().unwrap_or(37528) as u16;
        let mut c = Self {
            data_dir, media_dir, web_dir, store, settings, pres: Presentation::default(), layers: Layers::default(), pending_audio: None, obs_hold: false, obs_full: false, // [트랙 C] obs_full
           
            outputs: HashMap::new(), tx, cue: None, cues: vec![], toast: None, port, b: Default::default(), flow: Default::default(), // [트랙 B] · [트랙 C] flow
        };
        c.trackb_init(); // [트랙 B]
        if c.version_id().is_empty() {
            if let Some(v) = c.store.list_versions().first() {
                let id = v.id.clone();
                c.settings.update(|d| d["versionId"] = json!(id));
            }
        }
        c.cues = c.store.list_cues();
        let active = c.settings.str(&["activeCueId"]);
        c.cue = if active.is_empty() { None } else { c.store.get_cue(&active) };
        if c.cue.is_none() {
            if let Some(first) = c.cues.first() {
                c.cue = c.store.get_cue(&first.id.clone());
            }
        }
        if let Some(n) = note {
            c.notify(n, "ok");
        }
        Ok(c)
    }

    pub fn emit(&self, e: Ev) {
        let _ = self.tx.send(e);
    }

    pub fn notify(&mut self, msg: impl Into<String>, kind: &'static str) {
        self.toast = Some((msg.into(), kind, now_ms()));
        self.emit(Ev::Ui);
    }

    pub fn version_id(&self) -> String {
        self.settings.str(&["versionId"])
    }

    // ---------------- 템플릿 · 출력 데이터 ----------------
    /// 실제 출력은 항상 LIVE 묶음을 기준으로 한다.
    pub fn template_for(&self, output: &str) -> Value {
        self.template_for_deck(output, self.pres.live.as_ref().map(|l| &l.deck))
    }

    /// 미리보기·편집기에서 LIVE 상태를 빌리지 않고 대상 묶음의 템플릿을 고른다.
    pub fn template_for_deck(&self, output: &str, deck: Option<&Deck>) -> Value {
        let s = self.settings.get();
        let tpls = self.settings.templates();
        let find = |id: &str| tpls.iter().find(|t| t["id"] == id);
        if let Some(t) = deck.and_then(|d| d.passage["tpl"][output].as_str()).and_then(find) {
            return t.clone();
        }
        if output == "overlay" && deck.is_some_and(|d| is_lyrics(&d.passage, d.slides.get(d.index))) {
            if let Some(t) = s["activeTemplates"]["overlayLyrics"].as_str().and_then(find) {
                return t.clone();
            }
        }
        s["activeTemplates"][output].as_str().and_then(find).or(tpls.first()).cloned().unwrap_or(Value::Null)
    }

    pub fn layers_payload(&self, output: &str) -> Value {
        let s = self.settings.get();
        let tpl = self.template_for(output);
        let logo = s["logoScreenImage"].as_str().filter(|x| !x.is_empty()).or(tpl["logo"]["image"].as_str().filter(|x| !x.is_empty())).unwrap_or("/shared/assets/sa-shield.png");
        let media = if output == "main" { self.layers.media.clone() } else { None };
        json!({ "hideBg": self.layers.hide_bg, "logo": self.layers.logo, "logoImage": logo, "overlaysOff": false, "media": media })
    }

    fn active_overlays(&self, output: &str) -> Vec<Value> {
        self.settings.get()["overlays"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|o| o["on"].as_bool().unwrap_or(false) && o["outputs"].as_array().is_some_and(|a| a.iter().any(|x| x == output)))
            .cloned()
            .collect()
    }

    /// 출력 화면(예배당 창 · OBS)에 보낼 데이터 (outputPayload 와 같은 모양)
    pub fn output_payload(&self, output: &str) -> Value {
        let live = self.pres.live_state();
        let s = self.settings.get();
        json!({
            "output": output,
            "version": live["version"],
            "hasLive": live["hasLive"],
            "visible": live["visible"].as_bool().unwrap_or(false) && !self.obs_hold,
            "held": self.obs_hold,
            "slide": live["slide"],
            "versionName": live["passage"]["versionName"].as_str().unwrap_or(""),
            "template": if output == "overlay" && self.obs_full { self.template_for("main") } else { self.template_for(output) }, // [트랙 C]
            "obsFull": output == "overlay" && self.obs_full, // [트랙 C]
            "pip": if output == "overlay" && !self.obs_full && s["output"]["obsPip"].as_bool().unwrap_or(false) { json!({ "template": self.template_for("main") }) } else { Value::Null },
            "overlays": (self.overlays_for(output)), // [트랙 B]
            "layers": self.layers_payload(output),
            "video": self.video_payload(), // [트랙 B]
            "cursorHidden": s["output"]["cursorHidden"].as_bool().unwrap_or(true),
            "cameras": s.get("cameras").cloned().unwrap_or(json!({})),
        })
    }

    pub fn stage_payload(&self) -> Value {
        let live = self.pres.live_state();
        let p = &live["passage"];
        let title = if p["cueIndex"].as_i64().unwrap_or(-1) >= 0 && p["itemTitle"].as_str().is_some_and(|x| !x.is_empty()) { p["itemTitle"].clone() } else { p["title"].clone() };
        json!({
            "output": "stage", "version": live["version"], "hasLive": live["hasLive"], "visible": live["visible"],
            "slide": live["slide"], "nextSlide": live["nextSlide"], "index": live["index"], "count": live["count"],
            "title": title, "versionName": p["versionName"], "note": p["note"].as_str().unwrap_or(""),
            "timer": (self.b.stage.timer_json()), "message": (self.b.stage.message), "pin": (self.stage_pin_json()), // [트랙 B]
        })
    }

    pub fn output_status(&self) -> Value {
        let list: Vec<Value> = self
            .outputs
            .iter()
            .filter(|(_, c)| c.client != "control")
            .map(|(id, c)| json!({ "id": id, "output": c.client, "kind": c.kind, "connectedAt": c.connected_at, "ack": c.ack }))
            .collect();
        json!({ "clients": list, "desktop": crate::trackd::desktop_status() }) // [트랙 D] desktop
    }

    pub fn split_opts(&self, item: Option<&Value>) -> SplitOpts {
        let s = self.settings.get();
        let tid = |slot: &str, key: &str| -> Value {
            let over = item.and_then(|i| i[key].as_str()).filter(|x| !x.is_empty());
            over.and_then(|o| self.settings.template(o)).unwrap_or_else(|| {
                let id = s["activeTemplates"][slot].as_str().unwrap_or("");
                self.settings.template(id).unwrap_or_else(|| self.settings.templates().first().cloned().unwrap_or(Value::Null))
            })
        };
        SplitOpts {
            mode: s["split"]["mode"].as_str().unwrap_or("auto").to_string(),
            count: s["split"]["count"].as_u64().unwrap_or(2) as usize,
            templates: vec![tid("main", "tplMain"), tid("overlay", "tplOverlay")],
        }
    }

    // ---------------- 검색 · 미리보기 ----------------
    /// 장절이면 본문, 아니면 낱말 검색
    pub fn search(&mut self, q: &str) -> Value {
        let v = self.version_id();
        let mode = self.settings.str(&["searchMode"]);
        if mode == "text" || (mode != "reference" && !crate::parser::looks_like_reference(q)) {
            let mut r = self.store.search_text(&v, q, 300);
            r["mode"] = json!("text");
            return r;
        }
        let mut r = self.store.resolve_reference(&v, q);
        r["mode"] = json!("reference");
        r
    }

    /// 성경 본문을 미리보기로 (startAt: 이 절이 든 슬라이드부터)
    pub fn preview_passage(&mut self, passage: &Value, query: &str, start: Option<(i64, i64)>) {
        let opts = self.split_opts(None);
        let passage = &crate::flow::with_parallel(self, passage.clone()); // [트랙 C] 병행 번역
        let slides = slides::build_bible(passage, &opts);
        let idx = start.map(|(c, v)| slides::index_of_verse(&slides, c, v)).unwrap_or(0);
        let p = self.passage_meta(passage, query, None);
        self.pres.set_preview(p, slides, idx);
        self.after_preview();
    }

    fn passage_meta(&self, passage: &Value, query: &str, item: Option<(usize, &Value)>) -> Value {
        let ranges = passage["ranges"].as_array().cloned().unwrap_or_default();
        let first = ranges.first().map(|r| json!({ "chapter": r["sc"], "verse": r["sv"] }));
        let last = ranges.last().map(|r| json!({ "chapter": r["ec"], "verse": r["ev"] }));
        let mut p = json!({
            "title": passage["title"], "versionId": passage["versionId"], "versionName": passage["versionName"], "book": passage["book"],
            "first": first, "last": last, "query": query, "cueId": "", "cueIndex": -1, "itemType": "bible", "lyrics": false, "credit": "", "itemTitle": "", "note": "", "tpl": null, "auto": null, "hideAfter": 0, "edgeHide": false,
        });
        if let Some((i, it)) = item {
            self.fill_item_meta(&mut p, i, it);
        }
        p
    }

    fn fill_item_meta(&self, p: &mut Value, i: usize, it: &Value) {
        p["cueId"] = json!(self.cue.as_ref().map(|c| c.id.clone()).unwrap_or_default());
        p["cueIndex"] = json!(i);
        p["itemType"] = it["type"].clone();
        p["itemTitle"] = it["title"].clone();
        p["note"] = json!(it["note"].as_str().unwrap_or(""));
        p["lyrics"] = json!(it["type"] == "song" || it["type"] == "hymn");
        p["credit"] = json!(it["credit"].as_str().unwrap_or(""));
        let (tm, to) = (it["tplMain"].as_str().unwrap_or(""), it["tplOverlay"].as_str().unwrap_or(""));
        if !tm.is_empty() || !to.is_empty() {
            p["tpl"] = json!({ "main": tm, "overlay": to });
        }
        let sec = it["autoSec"].as_f64().unwrap_or(0.0);
        if sec >= 2.0 {
            p["auto"] = json!({ "sec": sec.round().min(3600.0), "loop": it["autoLoop"].as_bool().unwrap_or(false) });
        }
        crate::flow::item_meta(p, it); // [트랙 C] hideAfter·edgeHide
    }

    /// 큐시트 순서 하나 → 슬라이드 (오류면 글)
    pub fn item_deck(&mut self, i: usize) -> Result<Deck, String> {
        let it = self.cue.as_ref().and_then(|c| c.items.get(i)).cloned().ok_or("순서를 찾을 수 없습니다.")?;
        let opts = self.split_opts(Some(&it));
        let ty = it["type"].as_str().unwrap_or("");
        let (passage, p) = if ty == "bible" {
            let r = it["ref"].as_str().unwrap_or("");
            if r.trim().is_empty() {
                (None, None)
            } else {
                let v = it["versionId"].as_str().filter(|x| !x.is_empty() && self.store.has_version(x)).map(String::from).unwrap_or_else(|| self.version_id());
                let res = self.store.resolve_reference(&v, r);
                if res["ok"] != json!(true) {
                    return Err(format!("{}: {}", r, res["error"].as_str().unwrap_or("본문을 찾지 못했습니다.")));
                }
                let pass = crate::flow::with_parallel(self, res["passage"].clone()); // [트랙 C] 병행 번역
                let meta = self.passage_meta(&pass, r, Some((i, &it)));
                (Some(pass), Some(meta))
            }
        } else {
            (None, None)
        };
        let slides = if ty == "hymn" { crate::hymns::build_slides(self, &it, &opts)? } else { crate::slideedit::decorate(&it, slides::build_item(&it, passage.as_ref(), &opts), false) }; // [트랙 A] · [트랙 C] 슬라이드 편집 꾸미기
        if slides.is_empty() && ty != "audio" { return Err(crate::i18n::tr("이 순서에 보일 슬라이드가 없습니다 (PPT 파일을 고르세요).").into()); } // [트랙 A]
        let mut meta = p.unwrap_or_else(|| {
            let mut m = json!({ "title": it["title"], "versionId": "", "versionName": "", "book": null, "first": null, "last": null, "query": "", "tpl": null, "auto": null, "hideAfter": 0, "edgeHide": false });
            self.fill_item_meta(&mut m, i, &it);
            m
        });
        if ty == "audio" { meta["audioItem"] = it; }
        Ok(Deck { passage: meta, slides, index: 0 })
    }

    /// 순서를 미리보기에 올린다. 실패(없는 PPT·틀린 장절·만드는 중)하면 알림만 내고 미리보기는 그대로 두며 false.
    /// Electron `cue.js previewItem` 처럼 호출한 쪽이 돌려준 값으로 송출 여부를 정한다.
    pub fn preview_item(&mut self, i: usize) -> bool {
        match self.item_deck(i) {
            Ok(d) => {
                self.pres.preview = Some(d);
                self.after_preview();
                true
            }
            Err(e) => {
                self.notify(e, "warn");
                false
            }
        }
    }

    /// 순서를 두 번 눌러 송출 (Electron `cue.js sendItem`: `if (await previewItem(i)) await app.sendPreview()`).
    /// 미리보기가 실패하면 송출하지 않는다 — 전에 올려 둔 다른 큐시트·다른 내용의 미리보기가 LIVE 로 나가는 것을 막는다.
    /// 보조로 미리보기의 cueId·cueIndex 가 지금 큐시트의 이 순서와 같은지도 확인한다.
    pub fn send_item(&mut self, i: usize) -> bool {
        if !self.preview_item(i) { return false; }
        if !self.preview_is_item(i) {
            self.notify(crate::i18n::tr("미리보기가 이 순서가 아니어서 송출하지 않았습니다."), "warn");
            return false;
        }
        self.send_preview();
        true
    }

    /// 미리보기가 '열린 큐시트의 i 번 순서' 인지 (cueId 와 cueIndex 모두 비교)
    pub fn preview_is_item(&self, i: usize) -> bool {
        let Some(p) = self.pres.preview.as_ref() else { return false };
        let cue_id = self.cue.as_ref().map(|c| c.id.as_str()).unwrap_or("");
        p.passage["cueIndex"] == json!(i) && p.passage["cueId"] == json!(cue_id)
    }

    fn after_preview(&mut self) {
        if !self.settings.get()["previewMode"].as_bool().unwrap_or(true) {
            self.send_preview();
        }
        self.emit(Ev::Ui);
    }

    pub fn set_preview_index(&mut self, i: usize) {
        self.pres.set_preview_index(i);
        self.emit(Ev::Ui);
    }

    // ---------------- LIVE ----------------
    /// Electron core.js unclearBg: '모두 지우기' 로 숨긴 배경만 되살린다 (송출 · 글자 다시 보이기 때)
    pub fn unclear_bg(&mut self) {
        if self.layers.bg_by_clear {
            self.layers.hide_bg = false;
            self.layers.bg_by_clear = false;
        }
    }

    pub fn send_preview(&mut self) {
        if let Some(d) = self.pres.preview.clone() {
            if self.request_audio(&d) { return; }
        }
        // [트랙 B] '미리보기 거쳐 송출' 을 끈 채 두 번 눌러 송출하면 고르기(=송출)와 송출이 겹친다 → 방금(1.5초 안) 같은 것을 보냈으면 다시 보내지 않음
        if let (Some(p), Some(l)) = (&self.pres.preview, &self.pres.live) {
            if l.visible && l.deck.index == p.index && l.deck.slides.len() == p.slides.len() && l.deck.passage == p.passage && crate::util::now_ms() - l.sent_at < 1500 {
                self.emit(Ev::Ui);
                return;
            }
        }
        self.unclear_bg();
        self.obs_hold = false;
        if self.pres.send_live(None) {
            self.add_history();
            crate::trackd::on_send(self); // [트랙 D] 예배당 창 자동 켜기 · 순서 자동 동작
            self.emit(Ev::Live);
        }
        self.emit(Ev::Ui);
    }

    /// 음악 원본 항목을 조작 화면에 전달하며 LIVE·기록·레이어는 유지한다.
    pub fn take_pending_audio(&mut self) -> Option<Value> { self.pending_audio.take() }

    fn request_audio(&mut self, d: &Deck) -> bool {
        if d.passage["itemType"] != "audio" { return false; }
        self.pending_audio = Some(d.passage["audioItem"].clone());
        self.emit(Ev::Ui);
        true
    }

    pub fn send_deck(&mut self, d: Deck) {
        if self.request_audio(&d) { return; }
        self.unclear_bg();
        self.obs_hold = false;
        if self.pres.send_live(Some(d)) {
            self.add_history();
            crate::trackd::on_send(self); // [트랙 D] 예배당 창 자동 켜기 · 순서 자동 동작
            self.emit(Ev::Live);
        }
        self.emit(Ev::Ui);
    }

    fn add_history(&mut self) {
        self.sync_video_state(); // [트랙 B]
        let Some(l) = &self.pres.live else { return };
        let p = l.deck.passage.clone();
        log::info!(target: "live", "송출: {} [{}/{}]", p["itemTitle"].as_str().filter(|x| !x.is_empty()).or(p["title"].as_str()).unwrap_or(""), l.deck.index + 1, l.deck.slides.len()); // [트랙 B] 로그
        let cue_name = self.cue.as_ref().filter(|c| p["cueId"] == c.id.as_str()).map(|c| c.name.clone()).unwrap_or_default(); // [트랙 C]
        crate::usage::record(&self.store.db, &p, &cue_name); // [트랙 C] 사용 기록
        self.settings.update(|d| {
            if !d["history"].is_array() {
                d["history"] = json!([]);
            }
            let h = d["history"].as_array_mut().unwrap();
            h.insert(0, json!({ "at": now_ms(), "title": p["title"], "query": p["query"], "versionId": p["versionId"], "versionName": p["versionName"] }));
            h.truncate(300);
        });
    }

    /// 다음/이전 슬라이드. 순서의 끝이면 (설정 cueAdvance) 다음 순서를 바로 송출
    pub fn step(&mut self, d: i64) {
        crate::flow::step(self, d); // [트랙 C] 자동 숨김 뒤 넘김 · 처음/끝 숨김 · 다음 순서 · 연속 탐색
    }

    pub fn goto_live(&mut self, i: usize) {
        self.pres.goto(i);
        self.sync_video_state(); // [트랙 B]
        self.emit(Ev::Live);
        self.emit(Ev::Ui);
    }

    pub fn toggle_clear(&mut self) {
        self.pres.toggle();
        if self.pres.live.as_ref().is_some_and(|l| l.visible) {
            self.unclear_bg();
        }
        self.emit(Ev::Live);
        self.emit(Ev::Ui);
    }

    pub fn toggle_logo(&mut self) {
        self.layers.logo = !self.layers.logo;
        self.emit(Ev::Live);
        self.emit(Ev::Ui);
    }

    pub fn toggle_bg(&mut self) {
        self.set_bg_hidden(!self.layers.hide_bg);
    }
    /// 배경 끄기/켜기 (Electron toggleBackground). 직접 정한 배경은 다음 송출 때 되살리지 않는다 (bgByClear=false).
    pub fn set_bg_hidden(&mut self, hide: bool) {
        self.layers.hide_bg = hide;
        self.layers.bg_by_clear = false;
        self.emit(Ev::Live);
        self.emit(Ev::Ui);
    }

    /// 모두 지우기: 글자 숨김 + 배경 숨김 + 로고 끔
    /// (Electron core.js clearAll) 배경 숨김은 bgByClear 로 표시해 다음 송출 때만 되살린다.
    /// + 송출 배경(카메라·그림·영상) 깔 + 켜진 오버레이 모두 깔(기억해 두고) + 재생 중 영상 일시정지 (음악은 조작 화면 몹)
    pub fn clear_all(&mut self) {
        if self.pres.live.as_ref().is_some_and(|l| l.visible) {
            self.pres.clear();
        }
        if !self.layers.hide_bg {
            self.layers.hide_bg = true;
            self.layers.bg_by_clear = true;
        }
        self.layers.logo = false;
        self.layers.media = None;
        let on: Vec<String> = self.settings.get()["overlays"].as_array().into_iter().flatten().filter(|o| o["on"] == true).filter_map(|o| o["id"].as_str().map(String::from)).collect();
        if !on.is_empty() {
            self.settings.update(|d| {
                for id in &on {
                    let _ = crate::overlays::toggle_overlay(d, id, Some(false));
                }
            });
            self.b.saved_overlays = on;
        }
        if let Some(v) = self.b.video.as_mut().filter(|v| v.playing) {
            let _ = v.control("pause", 0.0);
        }
        self.emit(Ev::Live);
        self.emit(Ev::Ui);
    }

    pub fn set_template(&mut self, slot: &str, id: &str) {
        let (slot, id) = (slot.to_string(), id.to_string());
        self.settings.update(|d| d["activeTemplates"][&slot] = json!(id));
        self.emit(Ev::Live);
        self.emit(Ev::Ui);
    }

    pub fn set_version(&mut self, id: &str) {
        let id = id.to_string();
        self.settings.update(|d| d["versionId"] = json!(id));
        self.emit(Ev::Ui);
    }

    // ---------------- 큐시트 ----------------
    pub fn open_cue(&mut self, id: &str) {
        self.cue = self.store.get_cue(id);
        let id = id.to_string();
        self.settings.update(|d| d["activeCueId"] = json!(id));
        self.emit(Ev::Ui);
    }

    pub fn new_cue(&mut self, name: &str) -> Result<()> {
        let cue = Cue { id: short_id("c"), name: name.to_string(), service_date: iso_now()[..10].to_string(), start_time: String::new(), items: vec![], memo: String::new(), pinned: false, template: false };
        self.store.save_cue(&cue)?;
        self.cues = self.store.list_cues();
        let id = cue.id.clone();
        self.open_cue(&id);
        Ok(())
    }

    pub fn save_cue(&mut self) {
        if let Some(c) = &self.cue {
            if let Err(e) = self.store.save_cue(c) {
                self.notify(crate::i18n::tr_args("큐시트 저장 실패: {1}", &[&e]), "error");
                return;
            }
            self.cues = self.store.list_cues();
        }
        self.emit(Ev::Ui);
    }

    pub fn add_item(&mut self, item: Value, at: Option<usize>) {
        self.remember_items(); // [트랙 B]
        if self.cue.is_none() {
            let _ = self.new_cue("새 큐시트");
        }
        if let Some(c) = &mut self.cue {
            let i = at.unwrap_or(c.items.len()).min(c.items.len());
            c.items.insert(i, item);
        }
        self.save_cue();
    }

    pub fn remove_item(&mut self, i: usize) {
        self.remember_items(); // [트랙 B]
        if let Some(c) = &mut self.cue {
            if i < c.items.len() {
                c.items.remove(i);
            }
        }
        self.save_cue();
    }

    pub fn move_item(&mut self, i: usize, d: i64) {
        self.remember_items(); // [트랙 B]
        if let Some(c) = &mut self.cue {
            let j = i as i64 + d;
            if j >= 0 && (j as usize) < c.items.len() && i < c.items.len() {
                c.items.swap(i, j as usize);
            }
        }
        self.save_cue();
    }

    /// 설정을 파일로 (바뀐 것이 있을 때만)
    pub fn flush(&mut self) {
        if let Err(e) = self.settings.save_if_dirty() {
            log::error!("설정 저장 실패: {e}");
        }
    }
}

/// 새 순서 기본값 (Electron 판 cues.js 와 같은 칸)
pub fn new_item(ty: &str, title: &str) -> Value {
    let mut it = json!({ "id": short_id("i"), "type": ty, "title": title, "note": "", "duration": 0, "music": "", "musicVolume": 0.8, "autoSec": 0, "autoLoop": false, "tplMain": "", "tplOverlay": "" });
    match ty {
        "bible" => { it["ref"] = json!(""); it["versionId"] = json!(""); it["bg"] = json!(""); }
        "song" => { it["lyrics"] = json!(""); it["showTitle"] = json!(false); it["credit"] = json!(""); it["creditShow"] = json!(""); it["songId"] = json!(""); it["order"] = json!(""); it["bg"] = json!(""); }
        "text" => { it["body"] = json!(""); it["bg"] = json!(""); }
        "title" => { it["subtitle"] = json!(""); it["more"] = json!(""); it["bg"] = json!(""); }
        "blank" => { it["mode"] = json!("black"); }
        "image" => { it["image"] = json!(""); }
        _ => {}
    }
    it
}

#[cfg(test)]
mod live_fix_tests {
    use super::*;
    use crate::{db::Cue, trigger, remote};

    // 기존 사용자 자료를 복사하지 않도록 빈 DB가 있는 고유 임시 폴더를 쓴다.
    fn core() -> Core {
        let dir = std::env::temp_dir().join(format!("wp-live-fix-{}", crate::util::short_id("t")));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("bible.db"), b"").unwrap();
        Core::open(dir.clone(), dir, None).unwrap()
    }
    fn cue(id: &str, items: Vec<Value>) -> Cue {
        Cue { id: id.into(), name: id.into(), service_date: String::new(), start_time: String::new(), items, memo: String::new(), pinned: false, template: false }
    }
    fn title(name: &str) -> Value { json!({"type":"title", "title":name}) }
    fn audio() -> Value { json!({"type":"audio", "title":"배경음악", "file":"test.mp3", "volume":0.4, "loop":true, "fadeOnLeave":true}) }

    #[test]
    fn mismatched_cue_rejects_triggers_and_f6_f5() {
        let mut c = core();
        c.cue = Some(cue("A", vec![title("A0"), title("A1")]));
        trigger::send_cue_item(&mut c, 0).unwrap();
        c.cue = Some(cue("B", vec![title("B0"), title("B1")]));
        let before = c.pres.live_state();
        let preview = c.pres.preview.as_ref().unwrap().passage.clone();
        for action in ["cue-next", "cue-prev"] {
            assert!(trigger::run(&mut c, action, &HashMap::new()).is_err());
        }
        c.step(1);
        c.step(-1);
        assert_eq!(c.pres.live_state(), before);
        assert_eq!(c.pres.preview.as_ref().unwrap().passage, preview);
        assert!(c.take_pending_audio().is_none());
    }

    #[test]
    fn matching_cue_trigger_forward_and_backward_last_slide() {
        let mut c = core();
        c.cue = Some(cue("A", vec![json!({"type":"text", "title":"A0", "body":"첫 장\n---\n둘째 장"}), title("A1")]));
        trigger::send_cue_item(&mut c, 0).unwrap();
        let n = c.pres.live.as_ref().unwrap().deck.slides.len();
        assert!(n > 1);
        trigger::run(&mut c, "cue-next", &HashMap::new()).unwrap();
        assert_eq!(c.pres.live_state()["passage"]["cueIndex"], 1);
        trigger::run(&mut c, "cue-prev", &HashMap::new()).unwrap();
        assert_eq!(c.pres.live.as_ref().unwrap().deck.index, n - 1);
    }

    #[test]
    fn audio_send_paths_preserve_screen_layers_and_history() {
        let mut c = core();
        c.cue = Some(cue("A", vec![title("화면"), audio()]));
        trigger::send_cue_item(&mut c, 0).unwrap();
        c.layers.hide_bg = true;
        c.layers.logo = true;
        c.obs_hold = true;
        let before = c.pres.live_state();
        let history = c.settings.get()["history"].clone();
        let payload = c.output_payload("main");
        let mut rx = c.tx.subscribe();
        trigger::send_cue_item(&mut c, 1).unwrap();
        assert_eq!(c.take_pending_audio(), Some(audio()));
        assert!(c.take_pending_audio().is_none());
        c.send_preview();
        assert_eq!(c.take_pending_audio(), Some(audio()));
        assert_eq!(c.pres.live_state(), before);
        assert_eq!(c.output_payload("main"), payload);
        assert_eq!(c.settings.get()["history"], history);
        while let Ok(e) = rx.try_recv() { assert!(!matches!(e, Ev::Live)); }
    }

    #[test]
    fn f6_plays_audio_then_visual_and_f5_skips_audio() {
        let mut c = core();
        c.cue = Some(cue("A", vec![title("앞"), audio(), title("뒤")]));
        trigger::send_cue_item(&mut c, 0).unwrap();
        c.step(1);
        assert_eq!(c.pres.live_state()["passage"]["cueIndex"], 2);
        assert_eq!(c.take_pending_audio(), Some(audio()));
        c.step(-1);
        assert_eq!(c.pres.live_state()["passage"]["cueIndex"], 0);
        assert!(c.take_pending_audio().is_none());
    }

    #[test]
    fn trailing_audio_keeps_visual_and_audio_without_live_stays_dark() {
        let mut c = core();
        c.cue = Some(cue("A", vec![title("화면"), audio()]));
        trigger::send_cue_item(&mut c, 1).unwrap();
        assert!(c.pres.live.is_none());
        assert_eq!(c.take_pending_audio(), Some(audio()));
        trigger::send_cue_item(&mut c, 0).unwrap();
        let before = c.pres.live_state();
        c.step(1);
        assert_eq!(c.pres.live_state(), before);
        assert_eq!(c.take_pending_audio(), Some(audio()));
    }

    #[test]
    fn remote_obs_full_matches_output_and_toggles_back() {
        let mut c = core();
        assert_eq!(remote::state_for(&c)["obs"]["full"], false);
        for on in [true, false] {
            remote::act(&mut c, &json!({"action":"obsFull", "on":on})).unwrap();
            assert_eq!(remote::state_for(&c)["obs"]["full"], on);
            assert_eq!(c.output_payload("overlay")["obsFull"], on);
        }
    }

    /// 검토 1113-3 결함 2: '모두 지우기' 는 Electron clearAll 처럼 오버레이·송출 배경·영상까지 깔
    #[test]
    fn clear_all_turns_off_overlays_media_and_pauses_video() {
        let mut c = core();
        c.cue = Some(cue("A", vec![title("A0")]));
        trigger::send_cue_item(&mut c, 0).unwrap();
        c.save_overlay(&json!({ "id": "clock1", "type": "clock" })).unwrap();
        c.save_overlay(&json!({ "id": "banner1", "type": "banner", "text": "환영" })).unwrap();
        c.toggle_overlay("clock1", Some(true)).unwrap();
        c.toggle_overlay("banner1", Some(true)).unwrap();
        c.layers.media = Some(json!({ "kind": "camera", "id": "cam1" }));
        c.layers.logo = true;
        c.b.video = Some(crate::library::Video::start("v1"));
        assert!(c.b.video.as_ref().unwrap().playing);

        c.clear_all();
        assert!(!c.pres.live.as_ref().unwrap().visible, "글자 숨김");
        assert!(c.layers.hide_bg && c.layers.bg_by_clear, "배경은 '모두 지우기' 로 숨김 표시");
        assert!(!c.layers.logo);
        assert!(c.layers.media.is_none(), "송출 배경(카메라) 끊김");
        assert!(c.overlays().iter().all(|o| o["on"] == false), "오버레이 모두 깔");
        assert_eq!(c.b.saved_overlays, vec!["clock1".to_string(), "banner1".to_string()], "켜져 있던 것을 기억");
        assert!(!c.b.video.as_ref().unwrap().playing, "영상 일시정지 (소리가 계속 나지 않게)");
        // 오버레이 다시 켜기(toggle_overlays_off 한 번 더) 는 기억해 둔 것을 되살린다
        c.toggle_overlays_off();
        assert!(c.overlays().iter().all(|o| o["on"] == true));
        // 이미 손으로 끈 배경은 '모두 지우기' 가 bgByClear 로 바꾸지 않는다 (Electron: if (!hideBg) 만)
        let mut c2 = core();
        c2.set_bg_hidden(true);
        c2.clear_all();
        assert!(c2.layers.hide_bg && !c2.layers.bg_by_clear);
    }

    /// 검토 1113-3 결함 3: 손으로 끈 배경은 다음 송출에도 꺼진 채, '모두 지우기' 로 꺼진 배경만 다음 송출 때 복구 (Electron unclearBg)
    #[test]
    fn manual_bg_off_survives_send_but_clear_all_bg_returns() {
        let mut c = core();
        c.cue = Some(cue("A", vec![title("A0"), title("A1"), title("A2")]));
        trigger::send_cue_item(&mut c, 0).unwrap();
        // 손으로 끈 배경 → 다음 순서 송출해도 꺼진 채
        c.toggle_bg();
        assert!(c.layers.hide_bg && !c.layers.bg_by_clear);
        trigger::send_cue_item(&mut c, 1).unwrap();
        assert!(c.layers.hide_bg, "직접 끈 배경은 송출해도 유지");
        c.toggle_clear();
        c.toggle_clear();
        assert!(c.layers.hide_bg, "글자 다시 보여도 직접 끈 배경은 유지");
        // 다시 켜고 '모두 지우기' → 다음 송출 때 배경 복구
        c.toggle_bg();
        assert!(!c.layers.hide_bg);
        c.clear_all();
        assert!(c.layers.hide_bg && c.layers.bg_by_clear);
        trigger::send_cue_item(&mut c, 2).unwrap();
        assert!(!c.layers.hide_bg && !c.layers.bg_by_clear, "'모두 지우기' 배경은 송출하면 돌아온다");
        // 순서 자동 동작 bg:'hide' 도 직접 끈 것으로 취급 (Electron toggleBackground)
        c.clear_all();
        crate::trackd::run_item_actions(&mut c, &json!({ "bg": "hide" }), 0);
        assert!(c.layers.hide_bg && !c.layers.bg_by_clear);
    }
}

#[cfg(test)]
mod template_deck_tests {
    use super::*;
    fn core() -> Core {
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.test-data").join(short_id("template"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("bible.db"), b"").unwrap();
        let mut c = Core::open(dir.clone(), dir, None).unwrap();
        c.settings.update(|s| {
            s["templates"] = json!([{"id":"A"},{"id":"B"},{"id":"O"},{"id":"L"}]);
            s["activeTemplates"] = json!({"main":"A","overlay":"O","overlayLyrics":"L"});
        });
        c
    }
    fn deck(tpl: Value, lyrics: bool) -> Deck {
        Deck { passage: json!({"tpl":tpl,"lyrics":lyrics}), slides: vec![json!({"kind":"text"})], index: 0 }
    }
    #[test]
    fn preview_and_editor_use_target_deck_without_changing_live_output() {
        let mut c = core();
        c.pres.send_live(Some(deck(json!({"main":"A","overlay":"O"}), false)));
        let before = c.output_payload("main");
        c.pres.preview = Some(deck(json!({"main":"B","overlay":"B"}), true));
        assert_eq!(c.template_for_deck("main", c.pres.preview.as_ref())["id"], "B");
        assert_eq!(c.template_for_deck("overlay", c.pres.preview.as_ref())["id"], "B");
        assert_eq!(c.template_for("main")["id"], "A");
        assert_eq!(c.template_for("overlay")["id"], "O");
        assert_eq!(c.output_payload("main"), before);
        std::fs::remove_dir_all(&c.data_dir).ok();
    }
    #[test]
    fn lyrics_overlay_fallback_and_empty_deck_are_safe() {
        let mut c = core();
        for tpl in [json!({}), json!({"overlay":"missing"})] {
            let d = deck(tpl, true);
            assert_eq!(c.template_for_deck("overlay", Some(&d))["id"], "L");
        }
        let mut d = deck(json!({}), false);
        d.slides = vec![json!({"kind":"image","caption":"가사"})];
        assert_eq!(c.template_for_deck("overlay", Some(&d))["id"], "L");
        d.index = usize::MAX;
        assert_eq!(c.template_for_deck("overlay", Some(&d))["id"], "O");
        assert_eq!(c.template_for_deck("overlay", None)["id"], "O");
        c.settings.update(|s| s["templates"] = json!([]));
        assert_eq!(c.template_for_deck("main", None), Value::Null);
        std::fs::remove_dir_all(&c.data_dir).ok();
    }
    #[test]
    fn live_lyrics_and_obs_full_keep_output_contract() {
        let mut c = core();
        c.pres.send_live(Some(deck(json!({"main":"B"}), true)));
        assert_eq!(c.output_payload("overlay")["template"]["id"], "L");
        c.obs_full = true;
        assert_eq!(c.output_payload("overlay")["template"]["id"], "B");
        assert_eq!(c.output_payload("main")["template"]["id"], "B");
        std::fs::remove_dir_all(&c.data_dir).ok();
    }
}
