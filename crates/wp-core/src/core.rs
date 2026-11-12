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
            data_dir, media_dir, web_dir, store, settings, pres: Presentation::default(), layers: Layers::default(), obs_hold: false, obs_full: false, // [트랙 C] obs_full
           
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
    pub fn template_for(&self, output: &str) -> Value {
        let s = self.settings.get();
        let tpls = self.settings.templates();
        let live = self.pres.live.as_ref();
        let over = live.and_then(|l| l.deck.passage["tpl"][output].as_str()).unwrap_or("").to_string();
        let exists = |id: &str| tpls.iter().any(|t| t["id"] == id);
        let mut id = if !over.is_empty() && exists(&over) { over.clone() } else { s["activeTemplates"][output].as_str().unwrap_or("").to_string() };
        if output == "overlay" && over.is_empty() {
            if let Some(l) = live {
                if is_lyrics(&l.deck.passage, l.deck.slides.get(l.deck.index)) {
                    let ly = s["activeTemplates"]["overlayLyrics"].as_str().unwrap_or("");
                    if !ly.is_empty() && exists(ly) {
                        id = ly.to_string();
                    }
                }
            }
        }
        tpls.iter().find(|t| t["id"] == id.as_str()).or(tpls.first()).cloned().unwrap_or(Value::Null)
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
        if slides.is_empty() { return Err("이 순서에 보일 슬라이드가 없습니다 (PPT 파일을 고르세요).".into()); } // [트랙 A]
        let meta = p.unwrap_or_else(|| {
            let mut m = json!({ "title": it["title"], "versionId": "", "versionName": "", "book": null, "first": null, "last": null, "query": "", "tpl": null, "auto": null, "hideAfter": 0, "edgeHide": false });
            self.fill_item_meta(&mut m, i, &it);
            m
        });
        Ok(Deck { passage: meta, slides, index: 0 })
    }

    pub fn preview_item(&mut self, i: usize) {
        match self.item_deck(i) {
            Ok(d) => {
                self.pres.preview = Some(d);
                self.after_preview();
            }
            Err(e) => self.notify(e, "warn"),
        }
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
    pub fn send_preview(&mut self) {
        self.layers.hide_bg = false;
        self.obs_hold = false;
        if self.pres.send_live(None) {
            self.add_history();
            crate::trackd::on_send(self); // [트랙 D] 예배당 창 자동 켜기 · 순서 자동 동작
            self.emit(Ev::Live);
        }
        self.emit(Ev::Ui);
    }

    pub fn send_deck(&mut self, d: Deck) {
        self.layers.hide_bg = false;
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
            self.layers.hide_bg = false;
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
        self.layers.hide_bg = !self.layers.hide_bg;
        self.emit(Ev::Live);
        self.emit(Ev::Ui);
    }

    /// 모두 지우기: 글자 숨김 + 배경 숨김 + 로고 끔
    pub fn clear_all(&mut self) {
        self.pres.clear();
        self.layers.hide_bg = true;
        self.layers.logo = false;
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
                self.notify(format!("큐시트 저장 실패: {e}"), "error");
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
