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
    /// 알림에 붙는 단추 하나 ("undo" 되돌리기 · "output-on" 예배당 화면 켜기). notify 는 지우고 notify_act 만 채운다
    /// 새로 넣은 PPT 의 분할 제안 대기열 (저장하지 않음)
    pub split_offers: Vec<String>,
    pub toast_act: Option<&'static str>,
    pub port: u16,
    pub b: crate::trackb::TrackB, // [트랙 B]
    pub flow: crate::flow::Flow, // [트랙 C] 자동 넘김·글자 자동 숨김
    /// 라이브러리에서 고른 것 넣기 (중복·연타 막기, 만드는 중인 찬송가 기다렸다 띄우기)
    pub pick: crate::pick::PickState,
    /// 송출하면서 '예배당 화면을 켜 달라' 고 요청한 횟수 (output.autoOnSend). 조작 화면은 trackd::take_output_on_request 로 받는다 — 이 값은 시험·점검용
    pub output_on_requests: u32,
}

/// 코어 잠금. lockcheck::Mutex 는 같은 스레드의 재진입을 멈춤 대신 로그+panic 으로 바꾼다
pub type Shared = Arc<crate::lockcheck::Mutex<Core>>;

/// 사용자 자료의 뿌리 폴더: Windows %APPDATA%, 맥 ~/Library/Application Support
pub fn user_data_root() -> PathBuf {
    #[cfg(windows)]
    if let Some(a) = std::env::var_os("APPDATA") {
        return PathBuf::from(a);
    }
    dirs::data_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// Electron 판(1.x) 자료 폴더 (읽기만). Electron 과 같은 규칙: BIBLECAST_DATA → %APPDATA%\WorshipCue\data.
/// 점검용 WP_LEGACY_DATA 가 있으면 그것을 먼저 쓴다 (실제 자료 대신 사본으로 이전을 시험할 때).
pub fn legacy_data_dir() -> Option<PathBuf> {
    for k in ["WP_LEGACY_DATA", "BIBLECAST_DATA"] {
        if let Some(v) = std::env::var_os(k).filter(|v| !v.is_empty()) {
            return Some(PathBuf::from(v));
        }
    }
    dirs::data_dir().map(|d| d.join("WorshipCue").join("data"))
}

/// 폴더 통째 복사 (하위 폴더 포함). 이미 같은 크기로 있는 파일은 건너뛴다. 실패한 파일 수를 돌려준다
fn copy_tree(src: &Path, dst: &Path) -> usize {
    let mut failed = 0;
    if std::fs::create_dir_all(dst).is_err() {
        return 1;
    }
    let Ok(rd) = std::fs::read_dir(src) else { return 1 };
    for e in rd.flatten() {
        let (from, to) = (e.path(), dst.join(e.file_name()));
        match e.file_type() {
            Ok(t) if t.is_dir() => failed += copy_tree(&from, &to),
            Ok(t) if t.is_file() => {
                let same = matches!((std::fs::metadata(&from), std::fs::metadata(&to)), (Ok(a), Ok(b)) if a.len() == b.len());
                if !same && std::fs::copy(&from, &to).is_err() {
                    failed += 1;
                }
            }
            _ => {}
        }
    }
    failed
}

/// 처음 실행: Electron 판 자료(%APPDATA%\WorshipCue\data)가 있으면 복사해 온다 (원본은 건드리지 않음)
pub fn prepare_data_dir(data_dir: &Path, seed_db: Option<&Path>) -> Result<Option<String>> {
    prepare_data_dir_from(data_dir, legacy_data_dir().as_deref(), seed_db)
}

/// 이전 순서: 설정·그림 파일을 먼저, 성경·큐시트·찬양 DB 는 임시 이름(bible.db.migrating)으로 만든 뒤 맨 끝에 bible.db 로 바꾼다.
/// bible.db 가 '다 옮겼음' 표시라서, 옮기는 도중 강제로 꺼져도(첫 실행은 창이 뜨기 전 몇 초 걸림) 다음에 켤 때 처음부터 다시 옮긴다.
/// DB 를 못 읽으면(손상·잠김) 프로그램이 아예 안 켜지지 않게 기본 성경으로 시작하고 경고로 알린다 (원본은 그대로 남는다).
pub fn prepare_data_dir_from(data_dir: &Path, old: Option<&Path>, seed_db: Option<&Path>) -> Result<Option<String>> {
    std::fs::create_dir_all(data_dir)?;
    let db = data_dir.join("bible.db");
    let mut kept = None;
    if db.exists() {
        // 이미 이 판 자료가 있음: 가져온 뒤 1.x 자료가 또 바뀌었나 (예전에 2.x 를 시험으로 켜 본 PC 등)
        match crate::firstrun::recheck(data_dir, old) {
            crate::firstrun::Recheck::Nothing => return Ok(None),
            crate::firstrun::Recheck::Ask(v) => {
                crate::firstrun::set_ask(Some(v));
                return Ok(None);
            }
            crate::firstrun::Recheck::Reimport(why) => {
                let dir = crate::firstrun::set_aside(data_dir).context("지금 자료 보관")?;
                log::info!("1.x 자료 다시 가져오기: {why} (지금 자료 보관: {})", dir.display());
                kept = Some(dir);
            }
        }
    }
    let same = |a: &Path, b: &Path| a.to_string_lossy().trim_end_matches(['\\', '/']).eq_ignore_ascii_case(b.to_string_lossy().trim_end_matches(['\\', '/']));
    let kept_note = kept.map(|d| format!(" · 전에 쓰던 이 판 자료는 {} 에 보관했습니다", d.display())).unwrap_or_default();
    let mut warn = None;
    if let Some(old) = old.filter(|o| o.join("bible.db").is_file() && !same(o, data_dir)) {
        for f in ["settings.json", "settings.backup.json"] {
            if old.join(f).is_file() {
                let _ = std::fs::copy(old.join(f), data_dir.join(f));
            }
        }
        let om = old.join("media");
        let failed = if om.is_dir() { copy_tree(&om, &data_dir.join("media")) } else { 0 };
        let tmp = data_dir.join("bible.db.migrating");
        let _ = std::fs::remove_file(&tmp);
        // 열려 있어도 안전하게: SQLite 온라인 백업으로 한 시점의 사본 (WAL 에만 있는 최근 변경도 들어감).
        // VACUUM INTO 는 실제 자료(163MB)에서 11초 걸려 첫 실행 창이 한참 안 떴다 → 쪽 단위 복사 + 진행률
        let r = crate::firstrun::copy_db(&old.join("bible.db"), &tmp)
            .context("기존 자료 복사")
            .and_then(|_| std::fs::rename(&tmp, &db).context("기존 자료 이름 바꾸기"));
        match r {
            Ok(()) => {
                crate::firstrun::write_marker(data_dir, old, "migrated");
                let more = if failed > 0 { format!(" · 그림 파일 {failed}개는 복사하지 못했습니다") } else { String::new() };
                return Ok(Some(format!("기존 워십프리젠터 자료를 복사해 왔습니다 ({}){more}{kept_note}", old.display())));
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                log::warn!("기존 자료 복사 실패: {e:#}");
                warn = Some(format!("기존 자료({})를 복사하지 못해 기본 성경으로 시작합니다: {e:#}", old.display()));
            }
        }
    }
    if let Some(seed) = seed_db.filter(|s| s.exists()) {
        let tmp = data_dir.join("bible.db.migrating");
        std::fs::copy(seed, &tmp)?;
        std::fs::rename(&tmp, &db)?;
        return Ok(Some(warn.unwrap_or_else(|| "기본 성경 자료로 시작합니다.".into())));
    }
    Ok(warn)
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
        crate::firstrun::mark("자료 폴더 준비");
        let media_dir = data_dir.join("media");
        std::fs::create_dir_all(&media_dir)?;
        // 손상으로 못 열면: 원본을 damaged/ 에 남기고 멀쩡한 최근 백업으로 (sync::recover_from_backup). 그 밖의 까닭은 예전처럼 오류
        let mut recovered = None;
        let store = match BibleStore::open(&data_dir.join("bible.db")) {
            Ok(s) => s,
            Err(e) => match crate::sync::recover_from_backup(&data_dir) {
                Ok(n) => {
                    recovered = Some(n);
                    BibleStore::open(&data_dir.join("bible.db"))?
                }
                Err(why) => {
                    log::warn!("자동 복구 안 함: {why:#}");
                    return Err(e);
                }
            },
        };
        let _ = crate::usage::ensure(&store.db); // [트랙 C] 사용 기록 표
        if let Some(old) = legacy_data_dir() {
            crate::firstrun::repoint_pasted(&store.db, &data_dir, &old); // [갈래 D] 1.x 폴더가 지워졌으면 붙여 넣은 PPT 를 이 판 복사본으로
        }
        let mut settings = Settings::load(&data_dir);
        crate::dpapi::adopt_electron_secrets(&mut settings, legacy_data_dir().as_deref()); // [갈래 D] 1.x 에서 잠가 둔 AI 키·OBS 비밀번호 이어받기
        crate::firstrun::mark("설정 읽기");
        let (tx, _) = broadcast::channel(256);
        let port = settings.get()["port"].as_u64().unwrap_or(37528) as u16;
        let mut c = Self {
            data_dir, media_dir, web_dir, store, settings, pres: Presentation::default(), layers: Layers::default(), pending_audio: None, obs_hold: false, obs_full: false, // [트랙 C] obs_full
           
            outputs: HashMap::new(), tx, cue: None, cues: vec![], toast: None, toast_act: None, split_offers: vec![], port, b: Default::default(), flow: Default::default(), pick: Default::default(), output_on_requests: 0, // [트랙 B] · [트랙 C] flow
        };
        c.trackb_init(); // [트랙 B]
        c.purge_old_trash(30); // 휴지통의 큐시트는 30일 뒤 저절로 비운다 (Electron core.js:123)
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
            // 복사 실패는 경고로 (기본 성경으로 시작했다는 것을 놓치지 않게)
            let kind = if n.contains("복사하지 못해") { "warn" } else { "ok" };
            if kind == "warn" {
                crate::firstrun::add_start_warning(n.clone());
            }
            c.notify(n, kind);
        }
        if let Some(n) = recovered {
            crate::firstrun::add_start_warning(n.clone());
            c.notify(n, "warn");
        }
        // 깨진 설정 파일로 켰으면 조용히 넘어가지 않고 알린다 (자료 이전 알림보다 중요하므로 나중에 덮어쓴다)
        if let Some(n) = c.settings.load_note.clone() {
            crate::firstrun::add_start_warning(n.clone());
            c.notify(n, "warn");
        }
        Ok(c)
    }

    #[track_caller]
    pub fn emit(&self, e: Ev) {
        // 점검용: WP_DEBUG_REPAINT=1 이면 알림을 보낸 자리를 세어 5초마다 로그에 (가만히 둘 때 다시 그리기 원인 찾기)
        if debug_emit() {
            static SEEN: once_cell::sync::Lazy<parking_lot::Mutex<(i64, HashMap<String, u32>)>> = once_cell::sync::Lazy::new(|| parking_lot::Mutex::new((now_ms(), HashMap::new())));
            let l = std::panic::Location::caller();
            let mut s = SEEN.lock();
            *s.1.entry(format!("{:?}@{}:{}", e, l.file(), l.line())).or_default() += 1;
            if now_ms() - s.0 > 5000 {
                log::info!("emit 5초: {:?}", s.1);
                s.1.clear();
                s.0 = now_ms();
            }
        }
        let _ = self.tx.send(e);
    }

    pub fn notify(&mut self, msg: impl Into<String>, kind: &'static str) {
        let msg: String = msg.into();
        // 오류·경고 알림은 8초 뒤 사라지고 다음 알림에 덮이므로 logs\ 에도 남긴다 (나중에 원인 찾기)
        match kind {
            "error" => log::error!("알림: {msg}"),
            "warn" => log::warn!("알림: {msg}"),
            _ => {}
        }
        self.toast = Some((msg, kind, now_ms()));
        self.toast_act = None;
        self.emit(Ev::Ui);
    }

    /// 단추 하나가 붙은 알림 (예: '순서를 삭제했습니다 [되돌리기]'). 단추 동작은 조작 화면이 act 이름으로 정한다
    pub fn notify_act(&mut self, msg: impl Into<String>, kind: &'static str, act: &'static str) {
        self.notify(msg, kind);
        self.toast_act = Some(act);
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

    /// 장절을 '다음(미리보기)' 에만 올린다 — 바로 송출 모드여도 송출하지 않는다 (설교 구절 따라가기). 반환: 본문 제목
    pub fn preview_passage_quiet(&mut self, reference: &str) -> Result<String, String> {
        let v = self.version_id();
        let r = self.store.resolve_reference(&v, reference);
        if r["ok"] != json!(true) {
            return Err(r["error"].as_str().unwrap_or("본문을 찾지 못했습니다.").to_string());
        }
        let passage = crate::flow::with_parallel(self, r["passage"].clone());
        let opts = self.split_opts(None);
        let slides = slides::build_bible(&passage, &opts);
        let p = self.passage_meta(&passage, reference, None);
        self.pres.set_preview(p, slides, 0);
        self.pick.want = None;
        self.pick.want_send = None;
        self.emit(Ev::Ui); // after_preview 를 부르지 않는다 (송출하지 않음)
        Ok(passage["title"].as_str().unwrap_or(reference).to_string())
    }

    /// 출력 화면에 오버레이(자막 등)가 바뀌었다고 알린다
    pub fn overlays_changed(&self) {
        self.emit(Ev::Live);
        self.emit(Ev::Ui);
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
                // Electron cue.js previewItem: 장절이 빈 성경 순서는 보일 것이 없다 — 제목 카드로 대신 내보내지 않는다
                return Err(crate::i18n::tr("장절을 입력하세요.").into());
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
                self.want_when_built(i, &e);
                self.notify(e, "warn");
                false
            }
        }
    }

    /// 찬송가 PPT 를 아직 만드는 중이라 못 띄운 순서: 다 만들면 미리보기 (바로 송출이면 송출, hymns::start_build).
    /// Electron 은 previewItem 이 loadHymn 을 기다린 뒤 directSend 한다.
    fn want_when_built(&mut self, i: usize, e: &str) {
        if e == crate::hymns::BUILDING || e == crate::i18n::tr(crate::hymns::BUILDING) {
            let cid = self.cue.as_ref().map(|c| c.id.clone()).unwrap_or_default();
            self.pick.want = Some((cid, i));
            self.pick.want_send = None;
        }
    }

    /// 순서를 미리보기하며 k 번째 장으로 (전체 격자·오른쪽 클릭 메뉴: Electron BCCue.previewAt = 순서는 holdDirect 로 조용히, 그 뒤 setPreviewIndex).
    /// 바로 송출이면 k 번째 장을 한 번만 송출한다.
    pub fn preview_item_at(&mut self, i: usize, k: usize) -> bool {
        match self.item_deck(i) {
            Ok(mut d) => {
                d.index = k.min(d.slides.len().saturating_sub(1));
                self.pres.preview = Some(d);
                self.after_preview();
                true
            }
            Err(e) => {
                self.want_when_built(i, &e);
                self.notify(e, "warn");
                false
            }
        }
    }

    /// 미리보기 대신 바로 송출 중인지 (머리 줄 '클릭하면 [바로 송출]', 설정 previewMode=false)
    pub fn direct_mode(&self) -> bool {
        self.settings.get()["previewMode"].as_bool() == Some(false)
    }

    /// 사용자가 고른 슬라이드 (본문 목록·격자·전체 보기·번호 단추·←→·PageUp/Down):
    /// 미리보기를 그 장으로 옮기고, '바로 송출' 이면 송출 (Electron control.js setPreviewIndex → directSend).
    /// 이미 그 장이 보이는 채로 송출 중이면 다시 보내지 않는다.
    pub fn pick_preview_index(&mut self, i: usize) {
        self.pres.set_preview_index(i);
        if self.direct_mode() {
            let same = match (&self.pres.preview, &self.pres.live) {
                (Some(p), Some(l)) => l.visible && l.deck.index == p.index && l.deck.slides.len() == p.slides.len() && l.deck.passage == p.passage,
                _ => false,
            };
            if !same {
                self.send_preview();
            }
        }
        self.emit(Ev::Ui);
    }

    /// 라이브러리·찬송가·넣기 단추에서 고른 것을 큐시트에 넣는다 (Electron cue.js addItem: 넣고 곧바로 previewItem → 바로 송출이면 송출).
    /// - 같은 찬송가·찬양·파일이 이미 있으면(force 가 아니면) 새로 넣지 않고 그 순서를 고른다 + 알림 [그래도 하나 더 넣기]
    /// - 같은 것을 짧은 시간(pick::RAPID) 안에 또 넣으려 하면 무시 (두 번 누르기·연타)
    pub fn add_picked_item(&mut self, it: Value, at: Option<usize>, force: bool) -> crate::pick::Picked {
        use crate::pick::{content_key, lib_key, Picked};
        let now = std::time::Instant::now();
        let ck = content_key(&it);
        if self.pick.rapid(&ck, now) {
            return Picked::Ignored;
        }
        if !force {
            if let Some(k) = lib_key(&it) {
                let found = self.cue.as_ref().and_then(|c| c.items.iter().position(|x| lib_key(x).as_deref() == Some(k.as_str())));
                if let Some(j) = found {
                    self.pick.mark(ck, now);
                    let title = self.cue.as_ref().and_then(|c| c.items.get(j)).and_then(|x| x["title"].as_str()).unwrap_or("").to_string();
                    self.preview_item(j);
                    self.pick.dup_pending = Some((it, at));
                    self.notify_act(crate::i18n::tr_args("'{1}' 은(는) 이미 {2}번 순서에 있어 그 순서를 골랐습니다.", &[&title, &(j + 1)]), "info", "add-dup");
                    return Picked::Existing(j);
                }
            }
        }
        self.pick.mark(ck, now);
        self.pick.dup_pending = None;
        self.add_item(it, at);
        let n = self.cue.as_ref().map(|c| c.items.len()).unwrap_or(0);
        let i = at.unwrap_or(n.saturating_sub(1)).min(n.saturating_sub(1));
        self.preview_new_item(i);
        Picked::Added(i)
    }

    /// 알림 [그래도 하나 더 넣기]
    pub fn add_dup_anyway(&mut self) -> Option<crate::pick::Picked> {
        let (mut it, at) = self.pick.dup_pending.take()?;
        it["id"] = json!(crate::util::short_id("i"));
        self.pick.last = None;
        Some(self.add_picked_item(it, at, true))
    }

    /// 새로 넣은 순서 미리보기 (바로 송출이면 송출). 내용이 아직 없는 빈 순서(새 찬양·빈 그림)는 조용히 넘어간다.
    fn preview_new_item(&mut self, i: usize) {
        match self.item_deck(i) {
            Ok(d) if !d.slides.is_empty() || d.passage.get("audioItem").is_some() => {
                self.pres.preview = Some(d);
                self.after_preview();
            }
            Ok(_) => {}
            Err(e) => {
                let ty = self.cue.as_ref().and_then(|c| c.items.get(i)).and_then(|x| x["type"].as_str()).unwrap_or("").to_string();
                self.want_when_built(i, &e);
                // 내용을 골라 넣은 찬송가(만드는 중·파일 없음)는 알리고, 빈 새 순서의 '보일 슬라이드가 없습니다' 는 알리지 않는다
                let has_content = crate::pick::lib_key(self.cue.as_ref().and_then(|c| c.items.get(i)).unwrap_or(&Value::Null)).is_some();
                if ty == "hymn" && has_content {
                    self.notify(e, "info");
                }
            }
        }
    }

    /// 순서를 두 번 눌러 송출 (Electron `cue.js sendItem`: `if (await previewItem(i)) await app.sendPreview()`).
    /// 미리보기가 실패하면 송출하지 않는다 — 전에 올려 둔 다른 큐시트·다른 내용의 미리보기가 LIVE 로 나가는 것을 막는다.
    /// 보조로 미리보기의 cueId·cueIndex 가 지금 큐시트의 이 순서와 같은지도 확인한다.
    pub fn send_item(&mut self, i: usize) -> bool {
        if !self.preview_item(i) {
            // 그림을 만드는 중이면 다 만든 뒤 이어서 송출 (그 사이 다른 송출이 없을 때만 — hymns::start_build)
            let cid = self.cue.as_ref().map(|c| c.id.clone()).unwrap_or_default();
            if self.pick.want == Some((cid, i)) {
                self.pick.want_send = Some(self.pres.version);
            }
            return false;
        }
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
        self.pick.want = None; // 다른 것을 띄웠으면 '만들면 띄우기' 는 버린다
        self.pick.want_send = None;
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
                // 다시 보내지는 않아도 예배당 화면이 꺼져 있으면 켠다 (방금 Esc 로 끈 뒤 두 번 눌러 송출한 경우)
                crate::trackd::on_send(self);
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
        // Electron core.js:544 — LIVE 순서의 큐시트 이름은 DB 에서 읽는다 (열린 큐시트와 다르더라도 비지 않게)
        let cue_name = p["cueId"].as_str().filter(|id| !id.is_empty()).map(|id| match self.cue.as_ref().filter(|c| c.id == id) { Some(c) => c.name.clone(), None => self.store.get_cue(id).map(|c| c.name).unwrap_or_default() }).unwrap_or_default(); // [트랙 C]
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
    /// 지금 예배당 화면에 송출 배경(layers.media) 밖의 배경이 있는가 (Electron screenPayload().bgs 의 slide · template 항목)
    /// - LIVE 슬라이드의 bg 가 색(#…)이 아닌 그림·영상·카메라
    /// - 예배당(main) 템플릿 배경이 image · video · camera
    fn has_slide_or_template_bg(&self) -> bool {
        let slide_bg = self.pres.live.as_ref().and_then(|l| l.deck.slides.get(l.deck.index)).and_then(|s| s["bg"].as_str()).map(|u| !u.is_empty() && !u.starts_with('#')).unwrap_or(false);
        if slide_bg {
            return true;
        }
        matches!(self.template_for("main")["background"]["type"].as_str(), Some("image" | "video" | "camera"))
    }

    /// (Electron core.js:1290-1297) 감추는 쪽이고 송출 배경(라이브러리에서 띄운 카메라·그림·영상)이 있으면 그것은 아예 끈다.
    /// 송출 배경맀 있었으면 거기서 끝 — 순서·템플릿 배경까지 숨기지(hideBg) 않는다.
    pub fn set_bg_hidden(&mut self, hide: bool) {
        if hide && self.layers.media.is_some() {
            self.layers.media = None;
            if !self.has_slide_or_template_bg() {
                self.emit(Ev::Live);
                self.emit(Ev::Ui);
                return;
            }
        }
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
            if item["type"] == "hymn" && item["range"].as_str().unwrap_or("").is_empty() {
                if let Some(id) = item["id"].as_str() { self.split_offers.push(id.to_string()); }
            }
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

    /// 설정을 파일로 — 파일 쓰기는 백그라운드에서 (조작 화면이 주기적으로 부르는 것)
    pub fn flush_bg(&mut self) {
        self.settings.save_bg();
        // 저장이 안 되고 있으면(디스크 가득·폴더 권한·백신이 막음) 사용자에게 한 번 알린다 — 모르고 예배 준비를 계속하면 다음에 켤 때 사라진다
        if let Some(e) = self.settings.take_save_error() {
            self.notify(crate::i18n::tr_args("설정을 저장하지 못했습니다: {1} — 지금 바꾼 내용이 다음에 켤 때 사라질 수 있습니다. 디스크 공간과 자료 폴더 권한을 확인하세요.", &[&e]), "error");
        }
    }

    /// 설정을 파일로 (바뀐 것이 있을 때만). 다 쓸 때까지 기다린다 — 끌 때·시험에서
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

    /// Electron core.js:1290-1297 toggleBackground: 배경을 끌 때 송출 배경(layers.media)이 있으면 그것만 끈다.
    /// 송출 배경맀 있었으면 hideBg 는 그대로, 순서·템플릿 배경도 있으면 media 를 끈 다음 hideBg 까지.
    #[test]
    fn toggle_bg_with_media_only_clears_media() {
        let mut c = core();
        c.cue = Some(cue("A", vec![title("A0")]));
        trigger::send_cue_item(&mut c, 0).unwrap();
        // 제목 순서는 bg 가 없고, 기본 템플릿 배경은 색/그라데이션 → 송출 배경만 있는 상황
        assert!(!c.has_slide_or_template_bg(), "기본 템플릿 배경은 그림·영상·카메라가 아니어야 이 테스트가 성립");
        c.layers.media = Some(json!({ "type": "camera", "deviceId": "cam1" }));
        c.toggle_bg();
        assert!(c.layers.media.is_none(), "송출 배경은 끈다");
        assert!(!c.layers.hide_bg, "송출 배경맀 있었으므로 순서·템플릿 배경은 숨기지 않는다");
        assert!(!c.layers.bg_by_clear);
        // 한 번 더 누르면 이제는 보통의 배경 숨기기
        c.toggle_bg();
        assert!(c.layers.hide_bg && !c.layers.bg_by_clear);
        c.toggle_bg();
        assert!(!c.layers.hide_bg);

        // 슬라이드에 그림 배경이 있으면: media 도 끄고 hideBg 도 숨김 (Electron: bgs 가 남아 있으므로 진행)
        if let Some(l) = c.pres.live.as_mut() {
            l.deck.slides[0]["bg"] = json!("/media/bg.jpg");
        }
        c.layers.media = Some(json!({ "type": "camera", "deviceId": "cam1" }));
        c.toggle_bg();
        assert!(c.layers.media.is_none());
        assert!(c.layers.hide_bg && !c.layers.bg_by_clear, "순서 배경이 있으므로 배경도 숨김");
        // 색 배경(#…)은 bgs 에 안 들어간다
        c.layers.hide_bg = false;
        if let Some(l) = c.pres.live.as_mut() {
            l.deck.slides[0]["bg"] = json!("#000000");
        }
        c.layers.media = Some(json!({ "type": "image", "url": "/media/x.png" }));
        c.toggle_bg();
        assert!(c.layers.media.is_none() && !c.layers.hide_bg);
        // '배경 다시 켜기'(hide=false) 는 media 를 건들지 않는다
        c.layers.hide_bg = true;
        c.layers.media = Some(json!({ "type": "image", "url": "/media/x.png" }));
        c.toggle_bg();
        assert!(!c.layers.hide_bg && c.layers.media.is_some());
    }

    /// 검토 1113-5 결함 3: 사용 기록의 큐시트 칸은 LIVE 순서의 cueId 로 DB 에서 읽는다 (Electron core.js:544)
    #[test]
    fn usage_log_cue_name_from_live_cue_not_open_cue() {
        let mut c = core();
        let a = cue("A", vec![json!({"type":"song", "title":"주 하나님 지으신 모든 세계", "lyrics":"주 하나님 지으신 모든 세계", "credit":"CCLI 1"})]);
        c.store.save_cue(&a).unwrap();
        c.cue = Some(a.clone());
        trigger::send_cue_item(&mut c, 0).unwrap();
        let deck_a = c.pres.live.as_ref().unwrap().deck.clone();
        crate::usage::clear(&c.store.db, "");
        // 다른 큐시트 B 를 열어 두고 LIVE 덱(A 의 순서)을 다시 송출 → 기록의 큐시트 칸은 'A'
        c.cue = Some(cue("B", vec![title("B0")]));
        c.send_deck(deck_a);
        let rows = crate::usage::list(&c.store.db, &crate::usage::Filter::default());
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].cue, "A", "열린 큐시트(B)가 아닌 LIVE 순서의 큐시트(A) 이름");
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

/// 점검용 환경 변수 WP_DEBUG_REPAINT (한 번만 읽음)
fn debug_emit() -> bool {
    static ON: once_cell::sync::Lazy<bool> = once_cell::sync::Lazy::new(|| std::env::var_os("WP_DEBUG_REPAINT").is_some());
    *ON
}

#[cfg(test)]
mod migrate_tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("wp-migrate-{tag}-{}", rand::random::<u64>()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Electron 판처럼 WAL 로 열려 있는 DB (마지막 변경이 아직 WAL 에만 있음)
    fn fake_electron(dir: &Path) -> rusqlite::Connection {
        let c = rusqlite::Connection::open(dir.join("bible.db")).unwrap();
        c.execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0; CREATE TABLE Cuesheets(id TEXT, items TEXT); INSERT INTO Cuesheets VALUES('c1','[]');").unwrap();
        c.execute("INSERT INTO Cuesheets VALUES('c2', ?)", [r#"[{"file":"C:\\x\\WorshipCue\\data\\pasted\\a.png"}]"#]).unwrap();
        std::fs::write(dir.join("settings.json"), r#"{"activeCueId":"c2","remote":{"port":37540}}"#).unwrap();
        std::fs::write(dir.join("settings.backup.json"), "{}").unwrap();
        std::fs::create_dir_all(dir.join("media").join("sub")).unwrap();
        std::fs::write(dir.join("media").join("a.png"), b"a").unwrap();
        std::fs::write(dir.join("media").join("sub").join("b.png"), b"bb").unwrap();
        c
    }

    #[test]
    fn copies_db_settings_and_media_tree_while_old_is_open() {
        let (old, new) = (tmp("old"), tmp("new"));
        let open = fake_electron(&old); // 켜져 있는 Electron 판 흉내
        let note = prepare_data_dir_from(&new, Some(&old), None).unwrap().unwrap();
        assert!(note.contains("복사해 왔습니다"), "{note}");
        let c = rusqlite::Connection::open(new.join("bible.db")).unwrap();
        let n: i64 = c.query_row("SELECT count(*) FROM Cuesheets", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 2, "WAL 에만 있던 큐시트도 옮겨야 함");
        assert!(std::fs::read_to_string(new.join("settings.json")).unwrap().contains("37540"));
        assert!(new.join("settings.backup.json").is_file());
        assert_eq!(std::fs::read(new.join("media").join("sub").join("b.png")).unwrap(), b"bb", "하위 폴더도 복사");
        assert!(!new.join("bible.db.migrating").exists());
        // 두 번째 실행은 아무것도 하지 않음 · 원본은 그대로
        assert!(prepare_data_dir_from(&new, Some(&old), None).unwrap().is_none());
        assert!(old.join("settings.json").is_file());
        drop(c);
        drop(open);
        let _ = std::fs::remove_dir_all(old);
        let _ = std::fs::remove_dir_all(new);
    }

    #[test]
    fn interrupted_migration_is_redone() {
        let (old, new) = (tmp("old2"), tmp("new2"));
        let open = fake_electron(&old);
        // 지난번에 옮기다 꺼진 흔적: 반쯤 쓴 임시 DB, 설정 없음
        std::fs::write(new.join("bible.db.migrating"), b"half").unwrap();
        prepare_data_dir_from(&new, Some(&old), None).unwrap();
        let c = rusqlite::Connection::open(new.join("bible.db")).unwrap();
        let n: i64 = c.query_row("SELECT count(*) FROM Cuesheets", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 2);
        assert!(new.join("settings.json").is_file());
        drop(c);
        drop(open);
        let _ = std::fs::remove_dir_all(old);
        let _ = std::fs::remove_dir_all(new);
    }

    #[test]
    fn broken_old_db_falls_back_to_seed_with_warning() {
        let (old, new, seed_dir) = (tmp("old3"), tmp("new3"), tmp("seed3"));
        std::fs::write(old.join("bible.db"), b"this is not sqlite").unwrap();
        let seed = seed_dir.join("bible.db");
        rusqlite::Connection::open(&seed).unwrap().execute_batch("CREATE TABLE t(x)").unwrap();
        let note = prepare_data_dir_from(&new, Some(&old), Some(&seed)).unwrap().unwrap();
        assert!(note.contains("복사하지 못해"), "{note}");
        assert!(new.join("bible.db").is_file(), "그래도 켜져야 함");
        let _ = std::fs::remove_dir_all(old);
        let _ = std::fs::remove_dir_all(new);
        let _ = std::fs::remove_dir_all(seed_dir);
    }

    #[test]
    fn same_folder_is_not_migrated_onto_itself() {
        let d = tmp("same");
        assert!(prepare_data_dir_from(&d, Some(&d), None).unwrap().is_none());
        let _ = std::fs::remove_dir_all(d);
    }
}

#[cfg(test)]
mod audit_e_corrupt_tests {
    use super::*;

    fn open_with(settings: &str) -> Core {
        let dir = std::env::temp_dir().join(format!("wp-audit-e-{}", crate::util::short_id("t")));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("bible.db"), b"").unwrap();
        std::fs::write(dir.join("settings.json"), settings).unwrap();
        Core::open(dir.clone(), dir, None).unwrap()
    }

    /// 손으로 고치다 망가진 settings.json (칸의 모양이 다름) 으로 켜고 흔한 조작을 해도 꺼지지 않는다
    #[test]
    fn wrong_typed_settings_do_not_panic() {
        let bad = r#"{"templates":"글자","activeTemplates":[1,2],"overlays":{"a":1},"messageTemplates":5,"stagePresets":"x","macros":null,
            "schedule":{"x":1},"cameras":[1],"history":"없음","imageFolders":7,"droppedFiles":{},"output":"꺼짐","obs":[],"remote":3,"shortcuts":"x",
            "hymnFolders":"C:\\없음","uiLang":12,"port":"가","preService":[],"stage":"x","ai":1,"sync":"y","lanOutput":[],"companion":"z","devices":{}}"#;
        let mut c = open_with(bad);
        for out in ["main", "overlay", "stage"] {
            let _ = c.output_payload(out);
            let _ = c.layers_payload(out);
        }
        let _ = c.stage_payload();
        let _ = c.output_status();
        let _ = c.search("요 3:16");
        let _ = c.search("사랑");
        c.new_cue("손상 점검").unwrap();
        c.add_item(new_item("title", "제목 🙂"), None);
        c.add_item(json!({ "type": "song", "title": 5, "lyrics": ["배열"], "order": {} }), None);
        c.add_item(json!({ "type": "모르는 것" }), Some(99));
        c.add_item(json!("순서가 아닌 글"), None);
        for i in 0..6 {
            c.preview_item(i);
            c.send_item(i);
            c.step(1);
            c.step(-1);
            c.goto_live(99);
        }
        c.toggle_clear();
        c.toggle_logo();
        c.toggle_bg();
        c.clear_all();
        c.set_template("main", "없는 템플릿");
        c.set_version("없는 번역본");
        let _ = c.open_cue("없는 큐시트");
        c.move_item(0, -5);
        c.move_item(99, 1);
        c.remove_item(99);
        crate::flow::tick(&mut c, crate::util::now_ms());
        // 설정을 고치는 기능들
        let _ = crate::templates::save_template(&mut c.settings, &json!({ "name": "새 템플릿" }));
        let _ = crate::templates::duplicate_template(&mut c.settings, "default", None);
        let _ = crate::templates::delete_template(&mut c.settings, "없는 것");
        let _ = crate::themes::apply_theme(&mut c, "red-shield", &["main", "overlay"]);
        c.settings.update(|d| {
            let _ = crate::overlays::save_overlay(d, &json!({ "type": "clock" }));
            let _ = crate::overlays::save_message_template(d, &json!({ "name": "a", "text": "b" }));
            let _ = crate::overlays::save_stage_preset(d, &json!({ "name": "a" }));
            let _ = crate::overlays::save_macro(d, &json!({ "name": "m" }));
            crate::overlays::delete_macro(d, "x");
            let _ = crate::schedule::save(d, &json!({ "time": "가:1", "days": "x" }));
            let _ = crate::schedule::save(d, &json!({ "time": "10:55", "days": [0], "macroId": "m" }));
            crate::cameras::set(d, "카메라 🎥", Some(&json!({ "mirror": "예" })));
            let _ = crate::library::add_folder(d, "image", std::path::Path::new("C:\\없는 폴더"), None);
            let _ = crate::library::add_dropped(d, &[std::path::PathBuf::from("C:\\없는 파일.png")]);
        });
        c.flush();
    }

    /// 완전히 깨진 settings.json (JSON 이 아님 · 빈 파일 · 배열) 도 기본값으로 켜진다
    #[test]
    fn unreadable_settings_fall_back_to_defaults() {
        for bad in ["", "{", "[1,2,3]", "\u{feff}null", "설정이 아님"] {
            let mut c = open_with(bad);
            assert!(!c.settings.templates().is_empty(), "{bad:?}");
            let _ = c.output_payload("main");
            c.new_cue("x").unwrap();
            c.add_item(new_item("blank", "빈 화면"), None);
            c.send_item(0);
        }
    }
}
