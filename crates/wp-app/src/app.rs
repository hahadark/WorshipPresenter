//! 조작 화면 (egui)
//! 왼쪽: 큐시트(예배 순서)와 고르기·고치기 · 가운데: 성경 찾기 · 아래: 미리보기 | LIVE

use crate::outwin::OutWin;
use crate::paint::{self, paint_slide};
use eframe::egui::{self, Align, Color32, Key, KeyboardShortcut, Layout, Modifiers, RichText, Sense, Vec2};
use serde_json::{json, Value};
use std::time::Instant;
use wp_core::core::{new_item, Shared};
use wp_core::db::{CueSummary, Version};
use wp_core::presentation::{Deck, Live};

pub(crate) enum Act {
    /// 검색 실행 (Electron doSearch: Enter·후보 고르기). 장절이면 미리보기로 (바로 송출이면 송출)
    Search(String),
    /// 입력 중 목록만 (Electron #q input → 후보 목록). 미리보기·송출은 바꾸지 않는다
    SearchList(String),
    /// Ctrl+Enter (Electron sendNow): 검색칸 글이 지금 미리보기가 아니면 검색해 미리보기로 만든 뒤 송출
    SendNow(String),
    PreviewPassage(Value, String, Option<(i64, i64)>),
    PreviewRef(String, Option<(i64, i64)>),
    SendRef(String, Option<(i64, i64)>),
    PreviewItem(usize),
    SendItem(usize),
    SendPreview,
    PreviewIndex(usize),
    /// 순서를 미리보기하며 그 순서의 k 번째 장으로 (PreviewItem + PreviewIndex 를 합친 것: 바로 송출이면 k 번째 장만 한 번 송출, Electron BCCue.previewAt)
    PreviewItemAt(usize, usize),
    LiveGoto(usize),
    Step(i64),
    ToggleClear,
    ToggleLogo,
    ToggleBg,
    ClearAll,
    SetTemplate(&'static str, String),
    SetVersion(String),
    OpenCue(String),
    NewCue,
    AddItem(&'static str, Option<Value>),
    RemoveItem(usize),
    MoveItem(usize, i64),
    UpdateItem(usize, Value),
    TogglePreviewMode,
    OutputToggle,
    SetMonitor(usize),
    CuePreviewStep(i64),
    /// 미리보기 앞뒤 슬라이드 (control.js stepPreview): 순서 끝이면 다음·이전 순서로 (cueAdvance)
    PreviewStep(i64),
    /// 썸네일·목록의 마우스 올림 단추: (순서 번호, 슬라이드 ekey, 동작 edit·add·dup·del·skip·up·down)
    SlideOp(usize, String, &'static str),
    /// 고른 여러 장 지우기·건너뛰기 (slideedit.js selAction('del')): (순서 번호, ekey 들)
    SlideDelMany(usize, Vec<String>),
    /// 끌어서 여러 장 옮기기 (slideedit.js moveSlides): (순서 번호, 옮길 ekey 들, 놓을 자리 ekey, 그 뒤에)
    SlideMove(usize, Vec<String>, String, bool),
    /// 알림에 붙은 단추 ("undo" 되돌리기 · "output-on" 예배당 화면 켜기)
    ToastAct(&'static str),
    /// 알림을 눌러 닫기
    ToastDismiss,
    /// 되돌리기(false) · 다시 하기(true) — 알림은 코어 undo_items 가 띄운다
    Undo(bool),
}

pub struct App {
    pub(crate) core: Shared,
    pub(crate) web_dir: std::path::PathBuf,
    pub(crate) data_dir: std::path::PathBuf,
    pub(crate) search: String,
    pub(crate) result: Value,
    pub(crate) versions: Vec<Version>,
    pub(crate) sel: Option<usize>,
    pub(crate) edit: Option<(String, Value)>,
    pub(crate) out: OutWin,
    pub(crate) monitor: usize,
    editor_on: bool,
    editor_reveal: bool,
    views: MonitorViews,
    last_flush: Instant,
    pub(crate) focus_search: bool,
    pub(crate) tb: crate::trackb_ui::TbState, // [트랙 B]
    /// 점검용: --search 글 · --send · --shot 파일 (몇 초 뒤 화면을 그림 파일로 저장하고 끝냄)
    test: Option<(Instant, Option<String>, bool, Option<String>, u8)>,
    pub(crate) workspace: crate::workspace_ui::WorkspaceUi,
    pub(crate) tc: crate::track_c::TrackC, // [트랙 C]
}

pub(crate) struct Snap {
    pub(crate) preview: Option<Deck>,
    pub(crate) live: Option<Live>,
    pub(crate) cue_name: String,
    pub(crate) cue_id: String,
    pub(crate) items: Vec<Value>,
    pub(crate) cues: Vec<CueSummary>,
    pub(crate) templates: Vec<(String, String)>,
    pub(crate) tpl_main: Value,
    pub(crate) tpl_overlay: Value,
    pub(crate) preview_main: Value,
    pub(crate) preview_overlay: Value,
    pub(crate) active_main: String,
    pub(crate) active_overlay: String,
    pub(crate) version: String,
    pub(crate) preview_mode: bool,
    pub(crate) toast: Option<(String, &'static str, i64)>,
    pub(crate) toast_act: Option<&'static str>,
    pub(crate) outputs: Vec<(String, String)>,
    pub(crate) port: u16,
    pub(crate) logo: bool,
    pub(crate) hide_bg: bool,
    /// OBS 자막에 그림·악보 슬라이드를 우상단에 작게 (output.obsPip, 'OBS 전체' 가 아닐 때)
    pub(crate) obs_pip: bool,
    /// 화면 구성 (로드맵 2단계): [LIVE 예배당, LIVE OBS, 미리보기 예배당, 미리보기 OBS] — 기본이면 id 'full'
    pub(crate) scenes: [Value; 4],
    /// 칸 자리 표시에 쓸 소스 이름 (camera:… · src-… → 이름)
    pub(crate) source_names: Vec<(String, String)>,
    /// 추가 출력 (로드맵 3단계): 뷰어의 출력 고르기 [예배당 | OBS | 로비 …] 에 붙는다
    pub(crate) extra_views: Vec<ExtraView>,
    /// 순서마다 '출력 기본과 다른 화면 구성' 툴팁 글 (items 와 같은 차례, 비면 표시 없음)
    pub(crate) scene_marks: Vec<String>,
    /// 순서마다 '송출할 때 자동으로: 화면 구성 바꾸기' 요약 (로드맵 4단계, 시간표 구성 줄)
    pub(crate) scene_acts: Vec<String>,
}

/// 뷰어가 추가 출력을 그릴 때 쓸 것 (LIVE · 미리보기 따로)
pub(crate) struct ExtraView {
    pub(crate) id: String,
    pub(crate) name: String,
    /// 방송(OBS)처럼 그리는 출력 (그림 우상단)
    pub(crate) obs_like: bool,
    pub(crate) tpl_live: Value,
    pub(crate) tpl_pv: Value,
    pub(crate) scene_live: Value,
    pub(crate) scene_pv: Value,
}

/// 고른 것을 넣은 결과로 왼쪽 큐시트의 고른 순서를 맞춘다 (넣은 자리 또는 이미 있던 순서)
pub(crate) fn picked_sel<T>(sel: &mut Option<usize>, edit: &mut Option<T>, r: wp_core::pick::Picked) {
    use wp_core::pick::Picked;
    match r {
        Picked::Added(i) | Picked::Existing(i) => {
            *sel = Some(i);
            *edit = None;
        }
        Picked::Ignored => {}
    }
}

fn type_info(t: &str) -> (&'static str, Color32) {
    match t {
        "bible" => ("성경", crate::theme::type_ink("bible")),
        "song" => ("찬양", crate::theme::type_ink("song")),
        "hymn" => ("찬송", crate::theme::type_ink("hymn")),
        "text" => ("공지", crate::theme::type_ink("text")),
        "title" => ("제목", crate::theme::type_ink("title")),
        "blank" => ("빈 화면", crate::theme::type_ink("blank")),
        "image" => ("그림", crate::theme::type_ink("image")),
        "camera" => ("카메라", crate::theme::type_ink("camera")),
        "video" => ("영상", crate::theme::type_ink("camera")),
        "slideshow" => ("슬라이드쇼", crate::theme::type_ink("slideshow")),
        _ => ("순서", crate::theme::pal().muted),
    }
}

/// 미리보기 켜짐/꺼짐에 맞춰 미리보기 패널을 보이거나 닫는다 (배치 잠금과 관계없이, 원본 BCDock.show/hide 와 같음)
fn sync_preview_pane(ws: &mut wp_core::workspace::Workspace, on: bool) {
    use wp_core::workspace::Pane;
    if ws.visible(Pane::Preview) == on {
        return;
    }
    let locked = ws.locked;
    ws.locked = false;
    // 다시 켤 때는 원본 기본 배치처럼 LIVE 바로 위에 붙인다 (맨 오른쪽 끝에 새 열로 붙지 않게)
    if on && ws.layout.as_ref().is_some_and(|n| n.contains(Pane::Live)) {
        if let Some(n) = &mut ws.layout {
            n.insert(Pane::Preview, Pane::Live, 2);
        }
    } else {
        ws.toggle(Pane::Preview);
    }
    ws.locked = locked;
}

#[cfg(test)]
mod preview_pane_tests {
    use super::sync_preview_pane;
    use wp_core::workspace::{Pane, Workspace};
    #[test]
    fn preview_returns_above_live_not_as_new_column() {
        let mut ws = Workspace::default();
        sync_preview_pane(&mut ws, false);
        assert!(!ws.visible(Pane::Preview));
        sync_preview_pane(&mut ws, true);
        assert!(ws.visible(Pane::Preview));
        // LIVE 와 같은 묶음(위아래 분할) 안에 있어야 한다
        let g = ws.layout.as_ref().unwrap().group(Pane::Live);
        let json = serde_json::to_string(ws.layout.as_ref().unwrap()).unwrap();
        assert!(g.is_some(), "{json}");
        assert!(ws.floating.is_empty());
    }
}

impl App {
    pub fn new(core: Shared, web_dir: std::path::PathBuf, data_dir: std::path::PathBuf) -> Self {
        let versions = core.lock().store.list_versions();
        // 새 디자인 (나) 2026-11-27: 예전 기본 테마('어둡게')를 쓰던 자료는 한 번만 새 기본('전문가')으로 바꾼다.
        // 다른 테마를 직접 고른 사람은 그대로. 다시 '어둡게'를 고르면 그 뒤로는 바꾸지 않는다(ui.proDesign 표시).
        {
            let mut c = core.lock();
            if c.settings.get()["ui"]["proDesign"].is_null() {
                c.settings.update(|s| {
                    if !s["ui"].is_object() { s["ui"] = serde_json::json!({}); }
                    let t = s["ui"]["theme"].as_str().unwrap_or("").to_string();
                    if t.is_empty() || t == "dark" { s["ui"]["theme"] = serde_json::json!("pro"); }
                    s["ui"]["proDesign"] = serde_json::json!(1);
                });
            }
        }
        let out = OutWin::new();
        let saved = core.lock().settings.get()["rsOutputMonitor"].as_u64().map(|x| x as usize);
        let monitor = saved.filter(|m| out.monitors.iter().any(|x| x.index == *m)).unwrap_or_else(|| out.default_monitor());
        let mut workspace = crate::workspace_ui::WorkspaceUi::new(core.lock().settings.get());
        // 원본 dock.js:409 — 미리보기 끔(바로 송출)이면 미리보기 패널을 닫는다
        sync_preview_pane(&mut workspace.state, core.lock().settings.get()["previewMode"].as_bool().unwrap_or(true));
        Self { editor_on: true, editor_reveal: false, views: MonitorViews::default(), workspace, core, web_dir, data_dir, search: String::new(), result: Value::Null, versions, sel: None, edit: None, out, monitor, last_flush: Instant::now(), focus_search: true, test: test_args(), tb: Default::default() /* [트랙 B] */, tc: Default::default() /* [트랙 C] */ }
    }

    pub(crate) fn snap(&self) -> Snap {
        let c = self.core.lock();
        let s = c.settings.get();
        Snap {
            preview: c.pres.preview.clone(),
            live: c.pres.live.clone(),
            cue_name: c.cue.as_ref().map(|x| x.name.clone()).unwrap_or_default(),
            cue_id: c.cue.as_ref().map(|x| x.id.clone()).unwrap_or_default(),
            items: c.cue.as_ref().map(|x| x.items.clone()).unwrap_or_default(),
            cues: c.cues.clone(),
            templates: c.settings.templates().iter().map(|t| (t["id"].as_str().unwrap_or("").to_string(), wp_core::i18n::tr_name(t["name"].as_str().unwrap_or("")))).collect(),
            preview_main: c.template_for_deck("main", c.pres.preview.as_ref()),
            // 'OBS 전체'(obsFull) 면 OBS 화면도 예배당 템플릿으로 (core.output_payload 와 같게)
            preview_overlay: c.template_for_deck(if c.obs_full { "main" } else { "overlay" }, c.pres.preview.as_ref()),
            tpl_main: c.template_for("main"),
            tpl_overlay: c.template_for(if c.obs_full { "main" } else { "overlay" }),
            active_main: s["activeTemplates"]["main"].as_str().unwrap_or("").to_string(),
            active_overlay: s["activeTemplates"]["overlay"].as_str().unwrap_or("").to_string(),
            version: c.version_id(),
            preview_mode: s["previewMode"].as_bool().unwrap_or(true),
            toast: c.toast.clone(),
            toast_act: c.toast_act,
            outputs: c.outputs.values().map(|o| (o.client.clone(), o.kind.clone())).collect(),
            port: c.port,
            logo: c.layers.logo,
            hide_bg: c.layers.hide_bg,
            obs_pip: !c.obs_full && s["output"]["obsPip"].as_bool().unwrap_or(false),
            scenes: {
                let pv = c.pres.preview.as_ref();
                // 구성이 하나라도 있으면 (출력 · 순서 · 종류 지정은 있는 구성만 가리킨다) — 없으면 모두 기본 full
                let any = s["scenes"].as_array().is_some_and(|a| !a.is_empty());
                if any {
                    [c.scene_for("main"), c.scene_for("overlay"), c.scene_for_deck("main", pv), c.scene_for_deck("overlay", pv)]
                } else {
                    let f = || json!({ "id": wp_core::scenes::FULL });
                    [f(), f(), f(), f()]
                }
            },
            source_names: if s["scenes"].as_array().is_some_and(|a| !a.is_empty()) { crate::scene_ui::source_names(&c) } else { vec![] },
            scene_marks: if s["scenes"].as_array().is_some_and(|a| !a.is_empty()) { crate::sources_ui::scene_marks(&c) } else { vec![] },
            scene_acts: c.cue.as_ref().map(|q| q.items.iter().map(|it| wp_core::scenectl::action_summary(s, &it["actions"]["scene"]).join(" · ")).collect()).unwrap_or_default(),
            extra_views: {
                let pv = c.pres.preview.as_ref();
                let lv = c.pres.live.as_ref().map(|l| &l.deck);
                c.extra_outputs()
                    .iter()
                    .map(|o| {
                        let id = o["id"].as_str().unwrap_or("").to_string();
                        ExtraView {
                            name: o["name"].as_str().unwrap_or("").to_string(),
                            obs_like: o["kind"] == "overlay",
                            tpl_live: c.stage_template_for_deck(&id, lv),
                            tpl_pv: c.stage_template_for_deck(&id, pv),
                            scene_live: c.scene_for(&id),
                            scene_pv: c.scene_for_deck(&id, pv),
                            id,
                        }
                    })
                    .collect()
            },
        }
    }

    fn apply(&mut self, acts: Vec<Act>) {
        // 바로 송출 모드에서는 누른 것(순서·본문·슬라이드·라이브러리)이 송출 없이도 나간다 → LIVE 가 바뀌었는지로 본다
        let live_mark = |c: &wp_core::core::Core| c.pres.live.as_ref().map(|l| (l.sent_at, c.pres.version));
        let before = live_mark(&self.core.lock());
        let explicit = acts.iter().any(|a| matches!(a, Act::SendPreview | Act::SendItem(_) | Act::SendRef(..)));
        // PreviewItem(i) 바로 뒤 PreviewIndex(k) 는 하나로 (바로 송출에서 첫 장이 잠깐 나갔다 k 번째가 다시 나가지 않게)
        let mut merged = Vec::with_capacity(acts.len());
        let mut it = acts.into_iter().peekable();
        while let Some(a) = it.next() {
            if let Act::PreviewItem(i) = a {
                if let Some(Act::PreviewIndex(k)) = it.peek() {
                    let k = *k;
                    it.next();
                    merged.push(Act::PreviewItemAt(i, k));
                    continue;
                }
            }
            merged.push(a);
        }
        self.apply_acts(merged);
        let sent = explicit || live_mark(&self.core.lock()) != before;
        if sent {
            self.warn_output_off_after_send();
        }
    }

    fn apply_acts(&mut self, acts: Vec<Act>) {
        for a in acts {
            let mut c = self.core.lock();
            match a {
                Act::Search(q) => {
                    if q.trim().is_empty() {
                        self.result = Value::Null;
                        continue;
                    }
                    self.result = c.search(&q);
                    if self.result["ok"] == json!(true) && self.result["mode"] == "reference" {
                        let p = self.result["passage"].clone();
                        c.preview_passage(&p, &q, None);
                    }
                }
                Act::SearchList(q) => {
                    self.result = if q.trim().is_empty() { Value::Null } else { c.search(&q) };
                }
                Act::SendNow(q) => {
                    let q = q.trim().to_string();
                    let cur = c.pres.preview.as_ref().map(|p| p.passage["query"].as_str().unwrap_or("").trim().to_string());
                    if !q.is_empty() && cur.as_deref() != Some(q.as_str()) {
                        self.result = c.search(&q);
                        // 검색 실패·본문 검색이면 송출하지 않음
                        if self.result["ok"] != json!(true) || self.result["mode"] != "reference" { continue; }
                        let p = self.result["passage"].clone();
                        c.preview_passage(&p, &q, None);
                    }
                    c.send_preview();
                }
                Act::PreviewPassage(p, q, st) => c.preview_passage(&p, &q, st),
                Act::PreviewRef(q, st) => {
                    let v = c.version_id();
                    let r = c.store.resolve_reference(&v, &q);
                    if r["ok"] == json!(true) { c.preview_passage(&r["passage"].clone(), &q, st) }
                }
                Act::SendRef(q, st) => {
                    let v = c.version_id();
                    let r = c.store.resolve_reference(&v, &q);
                    if r["ok"] == json!(true) {
                        c.preview_passage(&r["passage"].clone(), &q, st);
                        c.send_preview();
                    }
                }
                Act::PreviewItem(i) => { c.preview_item(i); }
                Act::SendItem(i) => { c.send_item(i); } // 미리보기 실패·다른 큐시트의 미리보기면 송출하지 않음 (Electron sendItem)
                Act::SendPreview => c.send_preview(),
                Act::PreviewIndex(i) => c.pick_preview_index(i), // 바로 송출이면 송출 (Electron setPreviewIndex → directSend)
                Act::PreviewItemAt(i, k) => { c.preview_item_at(i, k); }
                Act::LiveGoto(i) => c.goto_live(i),
                Act::Step(d) => c.step(d),
                Act::ToggleClear => c.toggle_clear(),
                Act::ToggleLogo => c.toggle_logo(),
                Act::ToggleBg => c.toggle_bg(),
                Act::ClearAll => c.clear_all(),
                Act::SetTemplate(slot, id) => c.set_template(slot, &id),
                Act::SetVersion(id) => {
                    c.set_version(&id);
                    drop(c);
                    let q = self.search.clone();
                    if !q.trim().is_empty() {
                        self.apply(vec![Act::Search(q)]);
                    }
                    continue;
                }
                Act::OpenCue(id) => {
                    c.open_cue(&id);
                    self.sel = None;
                    self.edit = None;
                }
                Act::NewCue => {
                    let name = format!("{} 예배", &wp_core::util::iso_now()[..10]);
                    if let Err(e) = c.new_cue(&name) { c.notify(wp_core::i18n::tr_args("새 큐시트 실패: {1}", &[&e]), "error") }
                    self.sel = None;
                    self.edit = None;
                }
                Act::AddItem(ty, base) => {
                    let (label, _) = type_info(ty);
                    let mut it = new_item(ty, label);
                    if let Some(b) = base {
                        wp_core::settings::merge_into(&mut it, &b);
                    }
                    let at = self.sel.map(|s| s + 1);
                    // 넣고 곧바로 미리보기(바로 송출이면 송출) · 같은 찬송가·찬양은 있는 순서를 고름 · 연타 무시 (wp_core::pick)
                    picked_sel(&mut self.sel, &mut self.edit, c.add_picked_item(it, at, false));
                }
                Act::RemoveItem(i) => {
                    let name = c.cue.as_ref().and_then(|x| x.items.get(i)).map(|it| it["title"].as_str().map(str::trim).filter(|t| !t.is_empty()).unwrap_or(type_info(it["type"].as_str().unwrap_or("")).0).to_string());
                    c.remove_item(i);
                    if let Some(n) = name {
                        // 원본은 바로 지우고 알림이 없다. 잘못 지운 것을 바로 되살리도록 알림에 [되돌리기]
                        c.notify_act(wp_core::i18n::tr_args("'{1}' 순서를 삭제했습니다", &[&n]), "ok", "undo");
                    }
                    self.sel = None;
                    self.edit = None;
                }
                Act::MoveItem(i, d) => {
                    c.move_item(i, d);
                    let n = c.cue.as_ref().map(|x| x.items.len()).unwrap_or(0) as i64;
                    let j = (i as i64 + d).clamp(0, (n - 1).max(0)) as usize;
                    self.sel = Some(j);
                    self.edit = None;
                }
                Act::SlideOp(i, key, op) => {
                    use wp_core::i18n::tr;
                    if op == "edit" {
                        // grid.js .sg-edit → openPreviewSlide: 슬라이드 편집기를 그 장으로
                        self.sel = Some(i);
                        self.tc.slideedit.open_at(&key);
                        continue;
                    }
                    // slideedit.js quickOp: 편집기 차례(건너뛴 것 포함)로 동작을 적용한다
                    let list = match crate::slideedit_ui::editor_list(&mut c, i) {
                        Ok(Some(x)) => x,
                        Ok(None) => continue,
                        Err(e) => { c.notify(e, "warn"); continue; }
                    };
                    let keys: Vec<String> = list.1.iter().map(crate::center_ui::slide_key).collect();
                    match wp_core::slideedit::quick_op(&list.0, &keys, &key, op) {
                        Ok((a, pick)) => {
                            c.remember_items(); // 단추 한 번 = 실행 취소(Ctrl+Z) 한 번
                            c.b.typing = None;
                            if let Some(cue) = &mut c.cue { cue.items[i] = a; }
                            c.save_cue();
                            if c.preview_is_item(i) {
                                let idx = c.pres.preview.as_ref().map(|p| p.index).unwrap_or(0);
                                if let Ok(mut d) = c.item_deck(i) {
                                    d.index = idx.min(d.slides.len().saturating_sub(1));
                                    c.pres.preview = Some(d);
                                }
                            }
                            match op {
                                "del" => c.notify(tr("슬라이드를 지웠습니다. Ctrl+Z 로 되돌릴 수 있습니다."), "ok"),
                                "skip" => c.notify(tr("건너뛰기를 바꿨습니다 (건너뛴 슬라이드는 송출에서 빠집니다). Ctrl+Z 로 되돌릴 수 있습니다."), "ok"),
                                _ => {}
                            }
                            self.sel = Some(i);
                            self.edit = None;
                            if op == "add" {
                                // 새 슬라이드: 편집기에서 바로 글 쓰기 (Electron quickOp)
                                self.tc.slideedit.open_at(&pick);
                            }
                        }
                        Err(m) if !m.is_empty() => c.notify(tr(m), "info"),
                        Err(_) => {}
                    }
                }
                act @ (Act::SlideDelMany(..) | Act::SlideMove(..)) => {
                    let (i, del) = match &act { Act::SlideDelMany(i, d) | Act::SlideMove(i, d, _, _) => (*i, d.clone()), _ => continue };
                    if del.is_empty() { continue; }
                    use wp_core::i18n::{tr, tr_args};
                    // 편집기 차례(건너뛴 것 포함)로 적용한다 (SlideOp 와 같음)
                    let list = match crate::slideedit_ui::editor_list(&mut c, i) {
                        Ok(Some(x)) => x,
                        Ok(None) => continue,
                        Err(e) => { c.notify(e, "warn"); continue; }
                    };
                    let keys: Vec<String> = list.1.iter().map(crate::center_ui::slide_key).collect();
                    let chunk = wp_core::slideedit::is_chunk(list.0["type"].as_str().unwrap_or(""));
                    let (res, moved) = match &act {
                        Act::SlideMove(_, mv, target, after) => (wp_core::slideedit::move_keys(&list.0, &keys, mv, target, *after).map(|a| (a, mv.len())), true),
                        _ => (wp_core::slideedit::delete_keys(&list.0, &keys, &del), false),
                    };
                    // 옮긴 슬라이드의 새 키 (글 덩어리는 번호가 바뀜)
                    let new_keys = match &act {
                        Act::SlideMove(_, mv, target, after) => wp_core::slideedit::keys_after_move(chunk, &keys, mv, target, *after),
                        _ => vec![],
                    };
                    match res {
                        Ok((a, n)) => {
                            c.remember_items(); // 한 번 = 실행 취소(Ctrl+Z) 한 번
                            c.b.typing = None;
                            if let Some(cue) = &mut c.cue { cue.items[i] = a; }
                            c.save_cue();
                            if c.preview_is_item(i) {
                                let idx = c.pres.preview.as_ref().map(|p| p.index).unwrap_or(0);
                                if let Ok(mut d) = c.item_deck(i) {
                                    // 옮긴 슬라이드를 새 자리에서 미리보기
                                    let first = if moved { d.slides.iter().position(|s| new_keys.contains(&crate::center_ui::slide_key(s))) } else { None };
                                    d.index = first.unwrap_or(idx).min(d.slides.len().saturating_sub(1));
                                    c.pres.preview = Some(d);
                                }
                            }
                            let msg = if moved {
                                tr_args("슬라이드 {1}장의 차례를 바꿨습니다 (되돌리기 Ctrl+Z).", &[&n])
                            } else if chunk {
                                tr_args("슬라이드 {1}장을 지웠습니다.", &[&n])
                            } else {
                                tr_args("슬라이드 {1}장을 건너뛰기 (송출에서 빠짐) 했습니다.", &[&n])
                            };
                            c.notify(msg, "ok");
                            self.sel = Some(i);
                            self.edit = None;
                        }
                        Err(m) if !m.is_empty() => c.notify(tr(m), "info"),
                        Err(_) => {}
                    }
                }
                Act::UpdateItem(i, it) => {
                    c.remember_typing(it["id"].as_str().unwrap_or("")); // [트랙 B] 실행 취소 묶음
                    if let Some(cue) = &mut c.cue {
                        if i < cue.items.len() {
                            cue.items[i] = it;
                        }
                    }
                    c.save_cue();
                    let in_preview = c.preview_is_item(i); // 열린 큐시트의 이 순서를 보고 있을 때만 (cueId 도 비교)
                    if in_preview {
                        let idx = c.pres.preview.as_ref().map(|p| p.index).unwrap_or(0);
                        if let Ok(mut d) = c.item_deck(i) {
                            d.index = idx.min(d.slides.len().saturating_sub(1));
                            c.pres.preview = Some(d);
                        }
                    }
                }
                Act::TogglePreviewMode => {
                    let on = !c.settings.get()["previewMode"].as_bool().unwrap_or(true);
                    c.settings.update(|d| d["previewMode"] = json!(on));
                    // 원본 control.js:491 — 켜면 미리보기 패널을 열고, 끄면 닫는다
                    sync_preview_pane(&mut self.workspace.state, on);
                    c.notify(if on { wp_core::i18n::tr("미리보기를 켰습니다. 고른 본문은 미리보기에 놓이고 F9 로 송출합니다.") } else { wp_core::i18n::tr("미리보기를 껐습니다. 이제 클릭한 본문·순서·슬라이드가 바로 송출됩니다.") }, if on { "ok" } else { "info" });
                }
                Act::OutputToggle => {
                    let port = c.port;
                    drop(c);
                    let was = self.out.running(); if was { self.out.close() } else { self.out.open(port, self.monitor, false) } crate::trackd_ui::toggled(&self.core, was, !was); // [트랙 D] linkObs
                    continue;
                }
                Act::SetMonitor(m) => {
                    self.monitor = m;
                    c.settings.update(|d| d["rsOutputMonitor"] = json!(m));
                    let port = c.port;
                    drop(c);
                    if self.out.running() {
                        self.out.open(port, m, false);
                    }
                    continue;
                }
                Act::Undo(redo) => {
                    c.undo_items(redo);
                    self.edit = None;
                }
                Act::ToastDismiss => {
                    c.toast = None;
                    c.toast_act = None;
                }
                Act::ToastAct(a) => {
                    c.toast = None;
                    c.toast_act = None;
                    match a {
                        "undo" => {
                            c.undo_items(false);
                            self.edit = None;
                        }
                        "output-on" => {
                            drop(c);
                            if !self.out.running() {
                                self.apply(vec![Act::OutputToggle]);
                            }
                            continue;
                        }
                        // 이미 있는 찬송가·찬양을 일부러 하나 더
                        "add-dup" => {
                            if let Some(r) = c.add_dup_anyway() {
                                picked_sel(&mut self.sel, &mut self.edit, r);
                            }
                        }
                        _ => {}
                    }
                }
                Act::PreviewStep(d) => {
                    use wp_core::i18n::tr;
                    let Some((idx, n, ci, cid)) = c.pres.preview.as_ref().map(|p| (p.index as i64, p.slides.len() as i64, p.passage["cueIndex"].as_i64().unwrap_or(-1), p.passage["cueId"].as_str().unwrap_or("").to_string())) else { continue };
                    let ni = idx + d;
                    if ni >= 0 && ni < n { c.pick_preview_index(ni as usize); continue; }
                    // 큐시트 순서의 끝: 설정 '순서 끝에서 다음 순서로'(cueAdvance)가 켜져 있으면 다음·이전 순서 미리보기 (뒤로 가면 마지막 장)
                    if ci < 0 || c.settings.get()["cueAdvance"] == json!(false) { continue; }
                    let Some(len) = c.cue.as_ref().filter(|q| q.id == cid).map(|q| q.items.len() as i64) else { continue };
                    let j = ci + d;
                    if j < 0 { c.notify(tr("큐시트의 첫 순서입니다."), "info"); continue; }
                    if j >= len { c.notify(tr("큐시트의 마지막 순서입니다."), "info"); continue; }
                    self.sel = Some(j as usize);
                    self.edit = None;
                    if c.preview_item(j as usize) && d < 0 {
                        if let Some(last) = c.pres.preview.as_ref().map(|p| p.slides.len().saturating_sub(1)) { c.set_preview_index(last); }
                    }
                }
                Act::CuePreviewStep(d) => {

                    let n = c.cue.as_ref().map(|x| x.items.len()).unwrap_or(0) as i64;
                    if n == 0 { continue; }
                    let cur = self.sel.map(|x| x as i64).unwrap_or(-1);
                    let j = (cur + d).clamp(0, n - 1) as usize;
                    self.sel = Some(j);
                    self.edit = None;
                    c.preview_item(j);
                }
            }
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context, acts: &mut Vec<Act>, typing: bool) {
        self.tc.slideedit.pre_keys(ctx); // [갈래 C] 편집기에서 글 바로 고치는 중의 Esc = 그만두기
        // [트랙 C] 설정 → 단축키에서 바꾼 키를 쓴다 (없으면 기본값)
        let st = self.core.lock().settings.get()["shortcuts"].clone();
        let st = serde_json::json!({ "shortcuts": st });
        // 여러 장 고르기 키 (Esc·Delete·Ctrl+A·Ctrl+E) — 전역 Esc(보조 화면 끄기)보다 먼저
        if !typing { self.msel_keys(ctx, acts); }
        let mut on = |id: &str, def: &str| (!typing || crate::keys::safe_while_typing(&st, id, def)) && crate::keys::pressed(ctx, &st, id, def);
        if on("prevSlide", "F5") { acts.push(Act::Step(-1)) }
        if on("nextSlide", "F6") { acts.push(Act::Step(1)) }
        if on("sendPreview", "F9") { acts.push(Act::SendPreview) }
        if on("toggleClear", "F10") { acts.push(Act::ToggleClear) }
        if on("focusSearch", "Ctrl+K") { self.focus_search = true }
        // 슬라이드 편집기에서 글을 바로 고치는 중의 Ctrl+Enter 는 그 글 저장이다 (slideedit.js: keydown stopPropagation)
        if !self.tc.slideedit.inline_active() && on("sendNow", "Ctrl+Enter") { acts.push(Act::SendNow(self.search.clone())) }
        if on("cueNext", "Ctrl+ArrowDown") { acts.push(Act::CuePreviewStep(1)) }
        if on("cuePrev", "Ctrl+ArrowUp") { acts.push(Act::CuePreviewStep(-1)) }
        if on("toggleOutputWindow", "Ctrl+Shift+F") { acts.push(Act::OutputToggle) }
        // Esc 는 열린 즉석 자막 칸을 먼저 닫고, 닫을 것이 없을 때만 보조 화면 끄기 (control.js BCQuick.close)
        if !typing && self.tb.quick && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) { self.tb.quick = false }
        if on("outputOff", "Escape") && self.out.running() { acts.push(Act::OutputToggle) }
        // [트랙 C] 미리보기 앞뒤 절 · 장 처음
        if on("previewPrevVerse", "Alt+ArrowLeft") { wp_core::flow::preview_adjacent_verse(&mut self.core.lock(), -1) }
        if on("previewNextVerse", "Alt+ArrowRight") { wp_core::flow::preview_adjacent_verse(&mut self.core.lock(), 1) }
        if on("previewChapterStart", "Ctrl+Home") { wp_core::flow::preview_chapter_start(&mut self.core.lock()) }
        if on("cueEdit", "F2") {
            if let Some(i) = self.sel { self.show_item_editor(i); }
        }
        if on("toggleEditor", "Ctrl+Shift+E") {
            self.editor_on = !self.editor_on;
            if self.editor_on { self.editor_reveal = true; }
        }
        if !typing {
            let pv = self.core.lock().pres.preview.as_ref().map(|p| (p.index, p.slides.len()));
            if pv.is_some() {
                if on("previewNextSlide", "PageDown") { acts.push(Act::PreviewStep(1)) }
                if on("previewPrevSlide", "PageUp") { acts.push(Act::PreviewStep(-1)) }
                // control.js: ← → = 미리보기 앞뒤 슬라이드, Enter = 미리보기 송출 (글 쓰는 칸·창이 열려 있을 때·본문 목록에 키보드가 있을 때 Enter 는 빼고)
                if !self.tc.slideedit.keys_active() {
                    // 장 격자·송출 기록 창이 키보드를 갖고 있으면 ←→ 는 그쪽 것, 순서 목록까지 포함해 Enter 도 그쪽 것 (control.js:1654·1660 의 #chapterGrid · #cueList · 모달 제외)
                    let (nav_arrows, nav_enter) = (crate::cue_ui::nav_takes_arrows(ctx), crate::cue_ui::nav_takes_enter(ctx));
                    if !nav_arrows && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowLeft)) { acts.push(Act::PreviewStep(-1)) }
                    if !nav_arrows && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::ArrowRight)) { acts.push(Act::PreviewStep(1)) }
                    if !nav_enter && !crate::center_ui::results_focused(ctx) && ctx.input(|i| i.key_pressed(Key::Enter) && i.modifiers.is_none()) {
                        ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
                        acts.push(Act::SendPreview)
                    }
                }
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Space)) { acts.push(Act::Step(1)) }
            // [갈래 C] control.js jumpKey: 송출 중인 찬양에서 1~9 = 그 절, C = 후렴, B = 브릿지, E = 엔딩, P = 전주 의 첫 슬라이드로
            if !self.tc.slideedit.keys_active() {
                let typed: Option<String> = ctx.input(|i| if i.modifiers.ctrl || i.modifiers.alt || i.modifiers.command { None } else { i.events.iter().find_map(|e| if let egui::Event::Text(t) = e { Some(t.clone()) } else { None }) });
                if let Some(t) = typed.filter(|t| t.chars().count() == 1) {
                    let live = self.core.lock().pres.live.as_ref().map(|l| (wp_core::groups::live_labels(&l.deck.slides), l.deck.index));
                    if let Some((labels, cur)) = live.filter(|(l, _)| l.iter().any(|x| !x.is_empty())) {
                        match wp_core::groups::jump_target(&labels, cur, &t) {
                            Some(Some(k)) => acts.push(Act::LiveGoto(k)),
                            Some(None) => self.core.lock().notify(wp_core::i18n::tr_args("'{1}' 에 맞는 부분이 이 찬양에 없습니다.", &[&t]), "info"),
                            None => {}
                        }
                    }
                }
            }
        }
    }
    /// 큐시트 쪽 (Electron #cuePanel): 고르기 콤보 + 폴더 단추 · 순서 줄 · 넣기 단추 격자 · 도움말.
    /// 순서 옮기기·송출·지우기·편집칸은 오른쪽 클릭 메뉴와 본문 패널 편집칸에서 (원본 배치).
    pub(crate) fn cue_panel(&mut self, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
        let cues: Vec<(String, String)> = s.cues.iter().map(|c| (c.id.clone(), format!("{}{}{} ({})", if c.pinned { "[고정] " } else { "" }, if c.template { "[서식] " } else { "" }, c.name, c.count))).collect();
        let cue_label = cues.iter().find(|c| c.0 == s.cue_id).map(|c| c.1.clone()).unwrap_or_else(|| s.cue_name.clone());
        crate::cue_ui::head(self, ui, &s.cue_id, &cue_label, &cues, acts);
        ui.add_space(4.0);
        let live_idx = s.live.as_ref().filter(|l| l.deck.passage["cueId"] == json!(s.cue_id)).and_then(|l| l.deck.passage["cueIndex"].as_i64());
        let live_hidden = s.live.as_ref().is_some_and(|l| !l.visible);
        let pv_idx = s.preview.as_ref().filter(|p| p.passage["cueId"] == json!(s.cue_id)).and_then(|p| p.passage["cueIndex"].as_i64());
        // (나) 전문가용 표는 시각·분 칸이 늘 있다 (예전 테마는 설정 '예정 시각 보기' 일 때만)
        let pro = crate::theme::pal().bevel;
        let times = { let c = self.core.lock(); if pro || c.settings.get()["cueTimes"] == true { c.planned_times() } else { vec![] } };
        if pro {
            crate::cue_ui::table_head(ui);
        }
        let list_h = crate::cue_ui::list_height(ui);
        // .cue-list { flex: 1; padding: 2px 6px }
        egui::ScrollArea::vertical().id_salt("items").max_height(list_h).min_scrolled_height(list_h).auto_shrink([false, false]).show(ui, |ui| {
            // [갈래 C] 목록의 빈 자리 오른쪽 클릭 (ctxmenu.js cueEmptyMenu) — 줄보다 먼저 놓아 줄이 위에서 먼저 받는다
            let bg = ui.interact(ui.clip_rect(), ui.id().with("cue-empty"), Sense::click());
            crate::widgets::mark("cue-empty", ui.clip_rect());
            crate::ctx_ui::cue_empty_menu(self, &bg, acts);
            // 목록에 키보드가 있을 때의 ↑↓ · Alt+↑↓ · Enter · Delete (cue.js:1271)
            crate::cue_ui::list_keys(self, ui, ui.clip_rect(), s.items.len(), pv_idx, acts);
            let key_scroll = crate::cue_ui::take_scroll(ui.ctx());
            egui::Frame::new().inner_margin(if pro { egui::Margin::ZERO } else { egui::Margin::symmetric(6, 2) }).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for (i, it) in s.items.iter().enumerate() {
                    let st = crate::cue_ui::RowState {
                        selected: self.sel == Some(i),
                        pv: pv_idx == Some(i as i64),
                        air: live_idx == Some(i as i64),
                        hidden: live_hidden,
                        time: times.get(i).filter(|t| !t.is_empty() || pro).map(|t| (t.clone(), it["duration"].as_i64().unwrap_or(0))),
                        done: live_idx.is_some_and(|l| (i as i64) < l),
                        scene: s.scene_marks.get(i).cloned().unwrap_or_default(),
                    };
                    let out = crate::cue_ui::row(ui, i, it, &st);
                    if !pro { ui.add_space(2.0); } // .cue-item { margin-bottom: 2px }
                    let r = out.resp;
                    crate::widgets::mark(&format!("cue-row-{i}"), r.rect);
                    crate::dnd::zone(crate::dnd::Zone::CueRow(i), r.rect);
                    if key_scroll && self.sel == Some(i) { r.scroll_to_me(None); }
                    self.cue_row_dnd(ui, &r, i); // [트랙 B] 끌어서 옮기기
                    crate::parity_ui::cue_row_menu(self, &r, i, it, s.items.len(), acts); // [트랙 A] 오른쪽 클릭 메뉴
                    if out.send {
                        self.sel = Some(i);
                        acts.push(Act::SendItem(i));
                    } else if out.edit {
                        self.show_item_editor(i);
                        acts.push(Act::PreviewItem(i));
                    } else {
                        // 두 번 = 송출 (원본 cue.js:1266 은 미리보기만 — 사용자 지시로 의도적 차이, docs/behavior-parity.md).
                        // 첫 클릭으로 편집칸·미리보기가 바뀌어 줄이 밀려도 같은 순서면 두 번 클릭으로 친다 (widgets::click2)
                        match crate::widgets::click2(ui, ("cue-row", it["id"].as_str().unwrap_or(""), i), &r) {
                            crate::widgets::Click::Single => {
                                self.sel = Some(i);
                                self.edit = None;
                                acts.push(Act::PreviewItem(i));
                            }
                            crate::widgets::Click::Double => {
                                self.sel = Some(i);
                                acts.push(Act::SendItem(i));
                            }
                            crate::widgets::Click::None => {}
                        }
                    }
                }
                if s.items.is_empty() {
                    ui.add_space(12.0);
                    ui.label(crate::theme::rich(wp_core::i18n::tr("아래 버튼으로 예배 순서를 추가하세요."), 13.0, crate::theme::Weight::Regular, crate::theme::pal().muted));
                }
            });
        });
        crate::cue_ui::add_grid(self, ui, acts);
        let sel_item = self.sel.filter(|i| *i < s.items.len()).map(|i| (i, &s.items[i]));
        crate::cue_ui::windows(self, ui, sel_item, acts);
    }

    /// 송출 기록 창이 다시 띄운 본문을 검색칸·절 목록에 반영한다.
    pub(crate) fn take_history_loaded(&mut self) {
        if let Some((q, pg)) = self.tc.history.loaded.take() {
            self.search = q;
            self.result = json!({ "ok": true, "mode": "reference", "passage": pg });
        }
    }

    /// 우클릭 메뉴 등은 이 API로 선택과 본문 패널 노출을 함께 요청한다.
    pub(crate) fn show_item_editor(&mut self, i: usize) {
        self.sel = Some(i);
        self.edit = None;
        self.editor_on = true;
        self.editor_reveal = true;
    }

    fn reveal_editor_panel(&mut self) {
        use wp_core::workspace::Pane;
        if !self.workspace.state.visible(Pane::Main) { self.workspace.state.toggle(Pane::Main); }
        if let Some(n) = &mut self.workspace.state.layout { reveal_main(n); }
        for f in &mut self.workspace.state.floating {
            let mut n = f.node();
            reveal_main(&mut n);
            f.group = Some(n);
        }
    }

    pub(crate) fn main_panel(&mut self, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
        // 검색칸은 원본처럼 머리 줄(topbar_ui::search_bar)에 있다.
        let selected = self.sel.filter(|i| *i < s.items.len());
        // .center-head: '본문 | 주석' · '편집칸' · 제목 + 부가 글 · '목록 | 격자 | 전체 격자 | 전체 목록' (center_ui.rs)
        let mut editor_on = self.editor_on;
        ui.spacing_mut().item_spacing.y = 0.0;
        self.center_head(ui, s, &mut editor_on, acts);
        self.editor_on = editor_on;
        // 원본처럼 순서를 골랐을 때만 편집칸을 보인다. 높이는 기본 본문 패널의 46%(control.css .cue-editor max-height 46%),
        // 아래 구분선을 끌어 바꾸고(설정 rsEditorFrac 에 기억, 두 번 누르면 처음대로), 머리의 '크게' 로 본문 패널 전체를 쓴다 (평가 8번).
        let mut big = false;
        if let (true, Some(i)) = (self.editor_on, selected) {
            big = editor_big(ui.ctx());
            let avail = ui.available_height();
            let frac_id = egui::Id::new("editor-frac-drag");
            let saved = self.core.lock().settings.get()["rsEditorFrac"].as_f64().unwrap_or(EDITOR_FRAC as f64) as f32;
            let frac = ui.data(|d| d.get_temp::<f32>(frac_id)).unwrap_or(saved);
            let h = if big { avail } else { editor_height(avail, frac) };
            let w = ui.available_width();
            ui.allocate_ui(egui::vec2(w, h), |ui| {
                ui.set_max_height(h);
                if !big { ui.set_min_height(h); }
                self.editor(ui, i, &s.items[i], acts);
            });
            if !big {
                // 끌어서 높이를 바꾸는 구분선 (8px, 가운데 손잡이)
                let p = crate::theme::pal();
                let (r, resp) = ui.allocate_exact_size(egui::vec2(w, 8.0), Sense::click_and_drag());
                crate::widgets::mark("editor-split", r);
                let resp = resp.on_hover_text(wp_core::i18n::tr("끌어서 편집칸 높이 바꾸기 (두 번 누르면 처음 높이)"));
                let hot = resp.hovered() || resp.dragged();
                if hot { ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical); }
                let c = r.center();
                ui.painter().line_segment([egui::pos2(c.x - 20.0, c.y), egui::pos2(c.x + 20.0, c.y)], egui::Stroke::new(3.0, if hot { p.accent } else { p.line2 }));
                if resp.dragged() {
                    let nf = ((h + resp.drag_delta().y) / avail.max(1.0)).clamp(EDITOR_FRAC_MIN, EDITOR_FRAC_MAX);
                    ui.data_mut(|d| d.insert_temp(frac_id, nf));
                }
                if resp.drag_stopped() {
                    let nf = ui.data(|d| d.get_temp::<f32>(frac_id)).unwrap_or(frac);
                    self.core.lock().settings.update(|d| d["rsEditorFrac"] = json!(nf));
                    ui.data_mut(|d| d.remove::<f32>(frac_id));
                }
                if resp.double_clicked() {
                    self.core.lock().settings.update(|d| d["rsEditorFrac"] = json!(EDITOR_FRAC));
                    ui.data_mut(|d| d.remove::<f32>(frac_id));
                }
            }
        }
        if !big {
            self.center_body(ui, s, acts);
        }
    }

    /// 순서 편집칸 (Electron cue.js renderEditor): 머리 줄 카드 + 이름표-입력칸 2단. 입력하면 바로 저장
    fn editor(&mut self, ui: &mut egui::Ui, i: usize, it: &Value, acts: &mut Vec<Act>) {
        use crate::center_ui::{ce_area, ce_head, ce_note, ce_row, ce_select, ce_text};
        let ty = it["type"].as_str().unwrap_or("").to_string();
        let cams = if ty == "camera" { crate::camscan::known_cameras(&self.core.lock()) } else { vec![] }; // 연결된 카메라 포함 (화면 셋 공용 목록)
        let scan_bar = |ui: &mut egui::Ui, core: &Shared| crate::camscan::scan_bar(ui, core); // 카메라 순서: 카메라 찾기 줄
        let key = it["id"].as_str().unwrap_or("").to_string();
        if self.edit.as_ref().map(|e| &e.0) != Some(&key) {
            self.edit = Some((key.clone(), it.clone()));
        }
        let p = crate::theme::pal();
        let tr = wp_core::i18n::tr;
        // 고를 거리: 번역본 · 템플릿
        let mut versions = vec![(String::new(), tr("현재 번역본"))];
        versions.extend(self.versions.iter().map(|v| (v.id.clone(), v.name.clone())));
        let tpls: Vec<(String, String)> = self.core.lock().settings.templates().iter().map(|t| (t["id"].as_str().unwrap_or("").to_string(), wp_core::i18n::tr_name(t["name"].as_str().unwrap_or("")))).collect();
        // 장절 확인 글 ('✓ 시편 23:1-6 (6절)') — 장절·번역본이 바뀔 때만 다시 찾는다
        let ref_msg = if ty == "bible" {
            let (r, v) = self.edit.as_ref().map(|(_, e)| (e["ref"].as_str().unwrap_or("").to_string(), e["versionId"].as_str().unwrap_or("").to_string())).unwrap_or_default();
            let id = egui::Id::new(("ce-ref", &r, &v));
            match ui.data(|d| d.get_temp::<(bool, String)>(id)) {
                Some(m) => Some(m),
                None if r.trim().is_empty() => None,
                None => {
                    let mut c = self.core.lock();
                    let ver = if v.is_empty() || !c.store.has_version(&v) { c.version_id() } else { v.clone() };
                    let res = c.store.resolve_reference(&ver, &r);
                    drop(c);
                    let m = if res["ok"] == json!(true) {
                        let n = res["passage"]["verses"].as_array().map(|x| x.len()).unwrap_or(0);
                        (true, wp_core::i18n::tr_args("✓ {1} ({2}절)", &[&res["passage"]["title"].as_str().unwrap_or(""), &n]))
                    } else {
                        (false, res["error"].as_str().unwrap_or("").to_string())
                    };
                    ui.data_mut(|d| d.insert_temp(id, m.clone()));
                    Some(m)
                }
            }
        } else {
            None
        };
        let mut head = crate::center_ui::CeHead::default();
        let mut changed = false;
        // .cue-editor: panel2 바탕 · 안쪽 8px 14px 10px · 아래 2px pv (너비는 보이는 영역까지)
        let full = ui.available_width().min(ui.clip_rect().right() - ui.cursor().left());
        let frame = egui::Frame::new().fill(p.panel2).inner_margin(egui::Margin { left: 14, right: 14, top: 8, bottom: 10 }).show(ui, |ui| {
            ui.set_width(full - 28.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            head = ce_head(ui, i, &ty, it["title"].as_str().unwrap_or(""), "", editor_big(ui.ctx()));
            if ty == "camera" {
                scan_bar(ui, &self.core);
            }
            let Some((_, e)) = &mut self.edit else { return };
            let core = &self.core;
            egui::ScrollArea::vertical().id_salt("edit").auto_shrink([false, true]).show(ui, |ui| {
                let w = ui.available_width().min(ui.clip_rect().right() - ui.cursor().left()) - 62.0;
                changed |= ce_row(ui, &tr("제목"), |ui| ce_text(ui, e, "title", "순서 이름", w));
                match ty.as_str() {
                    "bible" => {
                        changed |= ce_row(ui, &tr("장절"), |ui| {
                            let mut ch = false;
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 6.0;
                                ch |= ce_text(ui, e, "ref", "예: 롬 8:18-25 · 시 23", (w - 146.0).max(80.0));
                                let mut v = e["versionId"].as_str().unwrap_or("").to_string();
                                if ce_select(ui, "ce-version", &mut v, &versions, 120.0) {
                                    e["versionId"] = json!(v);
                                    ch = true;
                                }
                            });
                            if let Some((ok, m)) = &ref_msg {
                                ce_note(ui, m, if *ok { p.ok_text } else { p.danger_text });
                            }
                            ch
                        });
                    }
                    "song" => {
                        changed |= ce_row(ui, &tr("가사"), |ui| {
                            let mut ch = ce_area(ui, e, "lyrics", "[1절]\n첫째 줄\n둘째 줄\n\n[후렴]\n…", 7);
                            ui.horizontal_wrapped(|ui| {
                                let mut stt = e["showTitle"].as_bool().unwrap_or(false);
                                if ui.checkbox(&mut stt, tr("화면에 제목 표시")).changed() {
                                    e["showTitle"] = json!(stt);
                                    ch = true;
                                }
                                ce_note(ui, &tr("· 빈 줄 = 다음 슬라이드 · [1절] 줄은 슬라이드 이름"), p.muted);
                                if let Some(i) = self.sel {
                                    if ui.add(crate::widgets::Btn::new(tr("가사 자동 정리…")).size(crate::widgets::Size::Sm).tooltip(tr("절 · 후렴 · 브리지로 나누고 줄 바꿈 · 띄어쓰기를 다듬어 고치기 전후를 나란히 봅니다"))).clicked() {
                                        crate::lyricsfix_ui::open_for(i, e);
                                    }
                                }
                            });
                            ch
                        });
                        changed |= ce_row(ui, &tr("부를 순서"), |ui| {
                            let mut ch = ce_text(ui, e, "order", "비우면 가사 순서대로 · 예: 1절 후렴 2절 후렴 후렴 (또는 1 C 2 C C)", w);
                            ch |= crate::media_ui::song_arrangements(ui, &mut core.lock(), e);
                            ch
                        });
                    }
                    "text" => {
                        changed |= ce_row(ui, &tr("내용"), |ui| {
                            let ch = ce_area(ui, e, "body", "광고 내용…\n---\n다음 장", 6);
                            ce_note(ui, &tr("'---' 줄 = 다음 슬라이드 · 교독문: 줄 앞에 인도자: · 회중: · 다같이: 를 쓰면 회중·다같이 줄은 굵게"), p.muted);
                            ch
                        });
                    }
                    "title" => {
                        changed |= ce_row(ui, &tr("부제"), |ui| ce_text(ui, e, "subtitle", "예: 홍길동 집사 · 설교 제목 · 본문", w));
                        let rows = if e["more"].as_str().is_some_and(|m| !m.is_empty()) { 3 } else { 1 };
                        changed |= ce_row(ui, &tr("추가 슬라이드"), |ui| ce_area(ui, e, "more", "다음 슬라이드 (첫 줄 = 제목, 둘째 줄 = 부제) · '---' 줄로 또 다음 · 비우면 한 장", rows));
                    }
                    "blank" => {
                        changed |= ce_row(ui, &tr("화면"), |ui| {
                            let mut m = e["mode"].as_str().unwrap_or("black").to_string();
                            let opts = [("black".to_string(), tr("검은 화면 (로고도 숨김)")), ("background".to_string(), tr("배경·로고만 보이기"))];
                            if ce_select(ui, "ce-blank", &mut m, &opts, 220.0) {
                                e["mode"] = json!(m);
                                return true;
                            }
                            false
                        });
                    }
                    "image" => {
                        changed |= ce_row(ui, &tr("이미지"), |ui| {
                            let mut ch = false;
                            ui.horizontal(|ui| {
                                let has = e["image"].as_str().is_some_and(|x| !x.is_empty());
                                if ui.add(crate::widgets::Btn::new(tr(if has { "바꾸기" } else { "고르기" })).icon("image")).clicked() {
                                    if let Some(f) = rfd::FileDialog::new().add_filter("그림", &["png", "jpg", "jpeg", "webp", "gif", "bmp"]).pick_file() {
                                        let enc = percent_encoding::utf8_percent_encode(&f.to_string_lossy(), percent_encoding::NON_ALPHANUMERIC).to_string();
                                        e["image"] = json!(format!("/lib/file?p={enc}"));
                                        if e["title"].as_str().is_none_or(|t| t.is_empty() || t == "그림") {
                                            e["title"] = json!(f.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default());
                                        }
                                        ch = true;
                                    }
                                }
                                ce_note(ui, &tr("등록한 그림 폴더 안의 그림만 예배당 화면에 보입니다"), p.muted);
                            });
                            ch
                        });
                    }
                    "hymn" => {
                        changed |= crate::hymns_ui::editor(ui, e, core); // [트랙 A]
                    }
                    _ => {
                        changed |= crate::parity_ui::editor(ui, e, &cams); // [트랙 A] 카메라 · 슬라이드쇼 · 영상
                    }
                }
                // 배경: 성경·찬양·공지·제목만 (이 순서에만 다른 이미지·영상)
                if matches!(ty.as_str(), "song" | "text" | "title" | "bible") {
                    changed |= ce_row(ui, &tr("배경"), |ui| {
                        let mut ch = false;
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            let zr = ui.available_rect_before_wrap();
                            crate::dnd::zone(crate::dnd::Zone::CueBg, egui::Rect::from_min_size(zr.min, egui::vec2(zr.width(), 30.0)));
                            if let Some(u) = crate::dnd::take_bg() { e["bg"] = json!(u); ch = true; } // 끌어다 놓은 그림·영상
                            let bg = e["bg"].as_str().unwrap_or("").to_string();
                            let name = percent_encoding::percent_decode_str(bg.rsplit('=').next().unwrap_or("")).decode_utf8_lossy().rsplit(['\\', '/']).next().unwrap_or("").to_string();
                            ui.label(crate::theme::rich(if bg.is_empty() { tr("템플릿 배경") } else { name }, 14.0, crate::theme::Weight::Regular, p.muted));
                            if ui.add(crate::widgets::Btn::new(tr("이미지·영상 고르기…")).icon("image")).clicked() {
                                if let Some(f) = rfd::FileDialog::new().add_filter("그림·영상", &["png", "jpg", "jpeg", "webp", "gif", "bmp", "mp4", "webm", "mov"]).pick_file() {
                                    let enc = percent_encoding::utf8_percent_encode(&f.to_string_lossy(), percent_encoding::NON_ALPHANUMERIC).to_string();
                                    e["bg"] = json!(format!("/lib/file?p={enc}"));
                                    ch = true;
                                }
                            }
                            if !bg.is_empty() && ui.add(crate::widgets::Btn::new(tr("지우기"))).clicked() {
                                e["bg"] = json!("");
                                ch = true;
                            }
                        });
                        ch
                    });
                }
                if ty != "audio" {
                    // 테마: 이 순서에만 다른 테마 (예배당·OBS 템플릿을 함께 바꿈)
                    changed |= ce_row(ui, &tr("테마"), |ui| {
                        let custom = e["tplMain"].as_str().is_some_and(|x| !x.is_empty()) || e["tplOverlay"].as_str().is_some_and(|x| !x.is_empty());
                        let mut cur = if custom { "custom".to_string() } else { String::new() };
                        let mut opts = vec![(String::new(), tr("기본 (전체 설정 따름)"))];
                        if custom {
                            opts.push(("custom".into(), tr("직접 고른 템플릿 (아래)")));
                        }
                        // 기본 테마 + 내 테마 (theme_ui: 테마 관리)
                        opts.extend(wp_core::themes::list_all(core.lock().settings.get()).iter().map(|t| (t["id"].as_str().unwrap_or("").to_string(), t["name"].as_str().unwrap_or("").to_string())));
                        if ce_select(ui, "ce-theme", &mut cur, &opts, 220.0) {
                            if cur.is_empty() {
                                e["tplMain"] = json!("");
                                e["tplOverlay"] = json!("");
                                return true;
                            }
                            if cur != "custom" {
                                if crate::theme_ui::apply_item_theme(e, core, &cur) {
                                    return true;
                                }
                            }
                        }
                        false
                    });
                    changed |= ce_row(ui, &tr("템플릿"), |ui| {
                        let mut ch = false;
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            for (k, label, salt) in [("tplMain", "예배당", "ce-tm"), ("tplOverlay", "OBS", "ce-to")] {
                                let l = tr(label);
                                let mut opts = vec![(String::new(), format!("{l}: {}", tr("기본 템플릿")))];
                                opts.extend(tpls.iter().map(|(id, n)| (id.clone(), format!("{l}: {n}"))));
                                let mut cur = e[k].as_str().unwrap_or("").to_string();
                                if ce_select(ui, salt, &mut cur, &opts, 200.0) {
                                    e[k] = json!(cur);
                                    ch = true;
                                }
                            }
                        });
                        ch
                    });
                    changed |= crate::flow_ui::item_fields(ui, e); // [트랙 C] 자동 넘김·글자 숨김
                    changed |= crate::actions_ui::item_fields(ui, e, core); // 배경 음악 · ⚡ 송출할 때 자동으로 (Electron cue.js actionsHtml)
                }
                changed |= ce_row(ui, &tr("메모"), |ui| ce_text(ui, e, "note", "조작자만 보는 메모 (예: 설교 후 바로)", w));
            });
        });
        let r = frame.response.rect;
        ui.painter().hline(r.x_range(), r.bottom() - 1.0, egui::Stroke::new(if p.bevel { 1.0 } else { 2.0 }, if p.bevel { p.line } else { p.pv }));
        if head.up && i > 0 {
            acts.push(Act::MoveItem(i, -1));
        }
        if head.down {
            acts.push(Act::MoveItem(i, 1));
        }
        if head.del {
            acts.push(Act::RemoveItem(i));
        }
        if head.close {
            self.editor_on = false;
            set_editor_big(ui.ctx(), false);
        }
        if head.big {
            let b = editor_big(ui.ctx());
            set_editor_big(ui.ctx(), !b);
        }
        if head.slide_edit {
            acts.push(Act::PreviewItem(i));
            self.tc.slideedit.open = true;
        }
        if head.dup {
            let mut c = self.core.lock();
            c.remember_items();
            c.add_item(wp_core::parity_a::duplicate_item(it), Some(i + 1));
            drop(c);
            self.sel = Some(i + 1);
            self.edit = None;
            return;
        }
        if changed {
            if let Some((_, e)) = &self.edit {
                // 저장본만 Electron cues.js normalizeItem 처럼 정규화 (편집 중인 e 는 글 입력이 끊기지 않게 그대로)
                let mut saved = e.clone();
                wp_core::actions::normalize_item_actions(&mut saved);
                acts.push(Act::UpdateItem(i, saved));
            }
        }
    }

    /// 검색 결과만 (검색칸은 머리 줄 topbar_ui::search_bar)
    pub(crate) fn search_panel(&mut self, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
        self.search_results(ui, s, acts);
    }

    /// 성경 검색 결과 (장절 본문 또는 낱말 찾기 목록) — 모양은 center_ui.rs (control.css .results .row)
    pub(crate) fn search_results(&mut self, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
        self.center_results(ui, s, acts);
    }

    /// 미리보기 · LIVE 패널 (Electron index.html .pane.preview / .pane.live, control.js renderPreview · renderLive · renderScreenLayers)
    pub(crate) fn deck_view(&mut self, ui: &mut egui::Ui, title: &str, deck: Option<&Deck>, is_live: bool, live: Option<&Live>, s: &Snap, acts: &mut Vec<Act>) {
        use crate::theme::{self, pal, Weight};
        use crate::widgets::{self, Badge, Btn};
        use wp_core::i18n::tr;
        let _ = title;
        let p = pal();
        let hidden = live.is_some_and(|l| !l.visible);
        // .pane { padding: 8px 12px 10px }
        egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 12, top: 8, bottom: 10 }).show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 0.0);
            // ---- .pane-head: 배지 · 제목 · (숨김 중) · 예배당 | OBS
            ui.horizontal(|ui| {
                ui.set_min_height(26.0);
                if p.bevel {
                    // (나) 뷰어 머리: '다음'(노랑) · '송출 중'(빨강) 굵은 글자만 (배지 없이)
                    let (t, c) = if is_live { (tr("송출 중"), p.live_text) } else { (tr("다음"), p.next) };
                    ui.add(egui::Label::new(theme::rich(t, 12.5, Weight::Bold, c)).selectable(false));
                } else if is_live { widgets::badge(ui, "LIVE", Badge::Live); } else { widgets::badge(ui, &tr("미리보기"), Badge::Pv); }
                let info = deck.map(|d| deck_info(d, is_live)).unwrap_or_default();
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    // 출력 고르기 (로드맵 3단계: 출력 목록을 따름 — 추가 출력이 없으면 예전처럼 [예배당 | OBS])
                    let view = if is_live { &mut self.views.live } else { &mut self.views.preview };
                    if !s.extra_views.iter().any(|x| &x.id == view) && view != "overlay" {
                        view.clear();
                    }
                    let main_label = tr("예배당");
                    let mut items: Vec<(String, &str)> = vec![(String::new(), main_label.as_str()), ("overlay".into(), "OBS")];
                    items.extend(s.extra_views.iter().map(|x| (x.id.clone(), x.name.as_str())));
                    widgets::segmented(ui, view, &items);
                    if hidden { hidden_badge(ui, &tr("숨김 중")); }
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        ui.add(egui::Label::new(theme::rich(info, if p.bevel { 12.0 } else { 14.0 }, if p.bevel { Weight::Regular } else { Weight::SemiBold }, if p.bevel { p.muted } else { p.text })).truncate().selectable(false));
                    });
                });
            });
            // 예배당 화면 꺼짐 띠 [지금 켜기] — (나) 에서는 화면 위에 겹쳐 그려 두 뷰어 크기를 같게 (아래 output_off_overlay)
            if is_live && !p.bevel { self.output_off_band(ui, deck.is_some(), acts); }
            ui.add_space(8.0); // .pane-head { margin-bottom: 8px }
            // ---- .screen: 16:9, 2px 테두리 (미리보기 pv · LIVE live · 숨김이면 점선), 둥글기 6
            let extra = s.extra_views.iter().find(|x| x.id == self.views.pick(is_live));
            let tpl = match (extra, is_live, self.views.obs(is_live)) {
                (Some(x), true, _) => &x.tpl_live, (Some(x), false, _) => &x.tpl_pv,
                (None, true, false) => &s.tpl_main, (None, true, true) => &s.tpl_overlay,
                (None, false, false) => &s.preview_main, (None, false, true) => &s.preview_overlay,
            };
            let c = paint::Ctx { web_dir: &self.web_dir, data_dir: &self.data_dir };
            let w = ui.available_width();
            let tw = tpl["width"].as_f64().unwrap_or(1920.0) as f32;
            let th = tpl["height"].as_f64().unwrap_or(1080.0) as f32;
            // 슬라이드(절) 번호 단추 줄 접기 (Electron control.js syncThumbsFold, 기억: 설정 thumbsFold { pv, lv })
            let fold_key = if is_live { "lv" } else { "pv" };
            let folded_saved = self.core.lock().settings.get()["thumbsFold"][fold_key].as_bool().unwrap_or(false);
            // (나) 뷰어가 낮으면(작은 창) 절 단추 줄을 저절로 접는다 — 같은 단추는 아래 본문 판에도 있다
            let auto_fold = p.bevel && ui.available_height() < 250.0;
            let folded = folded_saved || auto_fold;
            let reserve = if auto_fold { 44.0 } else if p.bevel { 44.0 + 34.0 + if folded { 0.0 } else { 74.0 } } else if is_live { 128.0 } else { 74.0 } - if folded && deck.is_some() && !p.bevel { 34.0 } else { 0.0 };
            let h = (w * th / tw).min(ui.available_height() - reserve).max(60.0);
            let w2 = h * tw / th;
            let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
            let rect = egui::Rect::from_center_size(rect.center(), Vec2::new(w2, h));
            let slide = deck.and_then(|d| d.slides.get(d.index));
            let version = deck.map(|d| d.passage["versionName"].as_str().unwrap_or("").to_string()).unwrap_or_default();
            let (visible, logo, hide_bg) = match live { Some(l) => (l.visible, s.logo, s.hide_bg), None if is_live => (true, s.logo, s.hide_bg), None => (true, false, false) };
            ui.painter().rect_filled(rect, crate::theme::cr(crate::theme::Rad::Md), Color32::BLACK);
            // 화면 구성 (로드맵 2단계): 고른 구성의 칸 자리 · 자르기대로 (기본 구성이면 paint_slide 그대로)
            let scene = match extra {
                Some(x) => if is_live { &x.scene_live } else { &x.scene_pv },
                None => &s.scenes[match (is_live, self.views.obs(is_live)) { (true, false) => 0, (true, true) => 1, (false, false) => 2, (false, true) => 3 }],
            };
            let obs_like = extra.map(|x| x.obs_like).unwrap_or(self.views.obs(is_live));
            let names = |id: &str| s.source_names.iter().find(|(k, _)| k == id).map(|(_, n)| n.clone()).unwrap_or_else(|| id.to_string());
            paint::paint_scene(ui, &c, rect.shrink(2.0), scene, slide, tpl, &version, visible, logo, hide_bg, &names);
            // OBS 자막 + 그림 우상단 켬: 그림·악보 슬라이드를 예배당 모양 그대로 오른쪽 위에 작게 (output index.html .bc-pip)
            if obs_like && s.obs_pip && visible && slide.is_some_and(|x| matches!(x["kind"].as_str(), Some("image" | "score"))) {
                let main_t = if is_live { &s.tpl_main } else { &s.preview_main };
                let mut sl = slide.cloned().unwrap_or_default();
                sl["caption"] = json!("");
                paint::paint_pip(ui, &c, rect.shrink(2.0), &sl, main_t);
            }
            // [갈래 C] 화면 오른쪽 클릭 메뉴 (ctxmenu.js previewMenu · liveMenu)
            let mresp = ui.interact(rect, ui.id().with(("pane-menu", is_live)), Sense::click());
            crate::widgets::mark(if is_live { "pane-live" } else { "pane-preview" }, rect);
            crate::ctx_ui::pane_menu(self, &mresp, is_live, acts);
            if deck.is_none() {
                let ph = if is_live { tr("송출 중인 본문 없음") } else { tr("검색한 본문이 여기에 표시됩니다") };
                // 화면 상자가 작으면(창이 작거나 미리보기·LIVE 를 위아래로 둘 때) 글이 상자 밖으로 넘쳤다 — 폭에 맞춰 줄인다
                let g = ui.painter().layout_no_wrap(ph.clone(), theme::font(14.0, Weight::Regular), p.muted2);
                let room = (rect.width() - 12.0).max(10.0);
                let g = if g.size().x > room { ui.painter().layout_no_wrap(ph, theme::font((14.0 * room / g.size().x).max(7.0), Weight::Regular), p.muted2) } else { g };
                ui.painter().galley(rect.center() - g.size() / 2.0, g, p.muted2);
            }
            let border = if is_live { p.live } else { p.pv };
            if hidden {
                dashed_rect(ui, rect.shrink(1.0), egui::Stroke::new(2.0, border));
            } else {
                ui.painter().rect_stroke(rect, crate::theme::cr(crate::theme::Rad::Md), egui::Stroke::new(if is_live && p.bevel { 3.0 } else { 2.0 }, border), egui::StrokeKind::Inside);
            }
            if is_live && p.bevel { self.output_off_overlay(ui, rect, deck.is_some(), acts); }
            // ---- .pane-tools: (◀ n/m ▶) · 다음: … · 그리기
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.set_min_height(30.0);
                if !is_live {
                    let n = deck.map(|d| d.slides.len()).unwrap_or(0);
                    let i = deck.map(|d| d.index).unwrap_or(0);
                    let why = if deck.is_none() { Some(tr("미리보기에 띄운 본문이 없습니다")) } else { None };
                    let prev_tip = why.clone().unwrap_or_else(|| if i == 0 { tr("첫 슬라이드입니다") } else { tr("미리보기 이전 슬라이드 (PageUp)") });
                    if ui.add(Btn::new("◀").height(28.0).enabled(deck.is_some() && i > 0).tooltip(prev_tip)).clicked() && i > 0 { acts.push(Act::PreviewIndex(i - 1)) }
                    count_label(ui, deck.map(|d| format!("{} / {}", d.index + 1, d.slides.len())).unwrap_or_default());
                    let next_tip = why.unwrap_or_else(|| if i + 1 >= n { tr("마지막 슬라이드입니다") } else { tr("미리보기 다음 슬라이드 (PageDown)") });
                    if ui.add(Btn::new("▶").height(28.0).enabled(deck.is_some() && i + 1 < n).tooltip(next_tip)).clicked() && i + 1 < n { acts.push(Act::PreviewIndex(i + 1)) }
                } else if let Some(d) = deck {
                    count_label(ui, format!("{} / {}", d.index + 1, d.slides.len()));
                }
                let draw_on = is_live && crate::system_ui::ST.lock().draw;
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    // 줄 오른쪽 끝 ▴/▾ (.pane-tools .thumbs-fold: 28px, muted · 접힌 때 accent). 접혀도 남아 다시 펼친다
                    let fb = Btn::icon_only(if folded { "chevron-down" } else { "chevron-up" })
                        .kind(widgets::Kind::Ghost)
                        .height(28.0)
                        .icon_color(if folded { p.accent } else { p.muted })
                        .tooltip(if folded { tr("슬라이드(절) 번호 단추 펼치기") } else { tr("슬라이드(절) 번호 단추 접기 (화면 자리 넓히기)") });
                    let fr = ui.add(fb);
                    widgets::mark(&format!("{fold_key}-thumbs-fold"), fr.rect);
                    if fr.clicked() {
                        self.core.lock().settings.update(|d| d["thumbsFold"][fold_key] = serde_json::json!(!folded_saved));
                        ui.ctx().request_repaint();
                    }
                    if is_live && ui.add(Btn::new(tr("그리기")).icon("brush").height(30.0).selected(draw_on).tooltip(tr("LIVE 화면에 그리기·강조 — 예배당·OBS 화면에 같이 보입니다"))).clicked() {
                        crate::system_ui::ST.lock().draw = true;
                    }
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        if let Some(d) = deck {
                            let continuous = is_live && self.core.lock().settings.get()["navMode"] == "continuous";
                            ui.add(egui::Label::new(next_hint(d, continuous)).truncate().selectable(false));
                        }
                    });
                });
            });
            // ---- .lv-layers: 지금 화면 (층마다 ✕) — 작은 창의 (나) 뷰어에서는 빼고 '지우기' 메뉴·오른쪽 클릭으로
            if is_live && !auto_fold {
                ui.add_space(2.0);
                let top = ui.cursor().top();
                ui.painter().hline(ui.max_rect().x_range(), top, egui::Stroke::new(1.0, p.line_soft));
                egui::Frame::new().inner_margin(egui::Margin::symmetric(6, 4)).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
                        ui.set_min_height(22.0);
                        ui.add(egui::Label::new(theme::rich(tr("지금 화면"), 11.0, Weight::Regular, p.muted)).selectable(false));
                        ui.add_space(2.0);
                        let layers = self.screen_layers(s, deck, live);
                        if layers.is_empty() {
                            ui.add(egui::Label::new(theme::rich(tr("비어 있음"), 12.0, Weight::Regular, p.muted)).selectable(false));
                        }
                        for ly in layers {
                            if layer_chip(ui, &ly).clicked() { self.layer_action(ly.act, s, acts); }
                        }
                    });
                });
            }
            // ---- .thumbs: 슬라이드 칩 (1. 3:16 …) — 접었으면 안 그린다 (.thumbs.folded { display: none })
            if let Some(d) = deck.filter(|_| !folded) {
                ui.add_space(4.0);
                // (나) 슬라이드 줄: 작은 그림 + 절 번호 한 줄 (가로로 굴림). 예전 테마는 둥근 칩 여러 줄
                let strip = p.bevel;
                let (wd, dd) = (self.web_dir.clone(), self.data_dir.clone());
                let c = paint::Ctx { web_dir: &wd, data_dir: &dd };
                let sa = if strip { egui::ScrollArea::horizontal().id_salt(format!("{}-slides", if is_live { "lv" } else { "pv" })).auto_shrink([false, true]) } else { egui::ScrollArea::vertical().id_salt(format!("{}-slides", if is_live { "lv" } else { "pv" })).max_height(76.0).auto_shrink([false, true]) };
                sa.show(ui, |ui| {
                    let mut body = |ui: &mut egui::Ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(if strip { 6.0 } else { 4.0 }, 4.0);
                        let item = crate::center_ui::deck_item(s, d);
                        for (k, sl) in d.slides.iter().enumerate() {
                            let label = chip_label(k, &slide_chip_text(sl));
                            let r = if strip { slide_tile(ui, &c, sl, tpl, &version, &label, k == d.index, k < d.index, is_live) } else { thumb_chip(ui, &label, k == d.index, is_live) }.on_hover_text(slide_snippet(sl));
                            if r.hovered() { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
                            widgets::mark(&format!("{fold_key}-chip-{k}"), r.rect);
                            // 마우스 올림 단추 (격자와 같은 규칙, 칩이 작아 칩 위에 띄운다)
                            let ops = crate::center_ui::slide_hover_ops(item.map(|x| x.1), sl, false);
                            if let Some(op) = widgets::hover_ops_float(ui, egui::Id::new((if is_live { "lv-chip" } else { "pv-chip" }, k)), r.rect, &ops) {
                                if let Some((i, _)) = item { acts.push(Act::SlideOp(i, crate::center_ui::slide_key(sl), op)); }
                            }
                            // control.js:611 번호를 두 번 누르면 그 장으로 옮기고 송출 (LIVE 쪽은 한 번에 그 장으로)
                            match widgets::click2(ui, (fold_key, "chip", k), &r) {
                                widgets::Click::Single => acts.push(if is_live { Act::LiveGoto(k) } else { Act::PreviewIndex(k) }),
                                widgets::Click::Double if is_live => acts.push(Act::LiveGoto(k)),
                                widgets::Click::Double => { acts.push(Act::PreviewIndex(k)); acts.push(Act::SendPreview) }
                                widgets::Click::None => {}
                            }
                            crate::parity2_ui::slide_menu(&self.core, &r, d, k, is_live, acts); // [트랙 A] 썸네일 오른쪽 클릭
                        }
                    };
                    if strip { ui.horizontal(|ui| body(ui)); } else { ui.horizontal_wrapped(|ui| body(ui)); }
                });
            }
        });
    }

    /// 지금 예배당 화면에 떠 있는 것 (Electron core.js screenPayload + control.js renderScreenLayers)
    fn screen_layers(&self, s: &Snap, deck: Option<&Deck>, live: Option<&Live>) -> Vec<Layer> {
        use wp_core::i18n::tr;
        let mut out = vec![];
        let c = self.core.lock();
        if let (Some(d), Some(l)) = (deck, live) {
            let slide = d.slides.get(d.index);
            let kind = slide.and_then(|x| x["kind"].as_str()).unwrap_or("text").to_string();
            let title = d.passage["itemTitle"].as_str().filter(|x| !x.is_empty()).or(d.passage["title"].as_str()).unwrap_or("");
            let name = if d.slides.len() > 1 { format!("{title} {}/{}", d.index + 1, d.slides.len()) } else { title.to_string() };
            let icon = match kind.as_str() { "video" => "clapperboard", "camera" => "camera", "image" => "image", "score" => "music-2", "blank" => "square", "slideshow" => "images", "title" => "tag", _ => "text" };
            out.push(if l.visible {
                Layer { act: LayerAct::Content, on: true, text: name, btn: "✕".into(), tip: tr(if kind == "video" { "영상 멈추고 지우기" } else { "슬라이드 지우기 (F10)" }), icon }
            } else {
                Layer { act: LayerAct::Content, on: false, text: format!("{name} · {}", tr("숨김")), btn: tr("보이기"), tip: tr("슬라이드 다시 보이기 (F10)"), icon }
            });
        }
        let bg_icon = |t: &str| match t { "camera" => "camera", "video" => "clapperboard", "color" => "palette", _ => "image" };
        if let Some(m) = &c.layers.media {
            let label = m["label"].as_str().unwrap_or("");
            let text = if label.is_empty() { tr("송출 배경") } else { format!("{}: {label}", tr("송출 배경")) };
            out.push(Layer { act: LayerAct::Media, on: true, text, btn: "✕".into(), tip: tr("송출 배경 끄기 (카메라는 닫힘)"), icon: bg_icon(m["type"].as_str().unwrap_or("")) });
        }
        if c.layers.hide_bg {
            out.push(Layer { act: LayerAct::BgOn, on: false, text: tr("배경 꺼짐"), btn: tr("다시 켜기"), tip: tr("배경(순서·템플릿 배경)을 다시 켭니다"), icon: "image" });
        } else {
            let kind_of = |u: &str| -> &'static str {
                let l = u.to_ascii_lowercase();
                let path = l.split(['?', '#']).next().unwrap_or("");
                if u.is_empty() { "" } else if u.starts_with('#') { "color" } else if u.starts_with("camera:") { "camera" } else if [".mp4", ".webm", ".mov", ".m4v", ".ogv", ".mkv"].iter().any(|e| path.ends_with(e)) { "video" } else { "image" }
            };
            if let (Some(d), Some(_)) = (deck, live) {
                let bg = d.slides.get(d.index).and_then(|x| x["bg"].as_str()).unwrap_or("");
                let k = kind_of(bg);
                if !k.is_empty() && k != "color" {
                    out.push(Layer { act: LayerAct::BgItem, on: true, text: tr("순서 배경"), btn: "✕".into(), tip: tr("이 순서에서 배경 빼기"), icon: bg_icon(k) });
                }
            }
            let b = &s.tpl_main["background"];
            let bt = b["type"].as_str().unwrap_or("");
            if matches!(bt, "image" | "video" | "camera") {
                let name = s.tpl_main["name"].as_str().unwrap_or("");
                let text = if name.is_empty() { tr("템플릿 배경") } else { format!("{}: {name}", tr("템플릿 배경")) };
                out.push(Layer { act: LayerAct::BgTpl, on: true, text, btn: "✕".into(), tip: tr("템플릿 배경 빼기 (단색으로, 이 템플릿을 쓰는 모든 순서)"), icon: bg_icon(bt) });
            }
        }
        let ov: Vec<String> = c.overlays().into_iter().filter(|o| o["on"] == true).map(|o| {
            o["name"].as_str().filter(|x| !x.is_empty()).or(o["text"].as_str().filter(|x| !x.is_empty())).map(String::from).unwrap_or_else(|| tr("오버레이"))
        }).collect();
        if !ov.is_empty() {
            out.push(Layer { act: LayerAct::Overlays, on: true, text: ov.join(", "), btn: "✕".into(), tip: tr("오버레이 끄기"), icon: "layers" });
        }
        if c.layers.logo {
            out.push(Layer { act: LayerAct::Logo, on: true, text: tr("로고 화면"), btn: "✕".into(), tip: tr("로고 화면 끄기"), icon: "flag" });
        }
        let dj = c.draw_json();
        if dj["strokes"].as_array().is_some_and(|a| !a.is_empty()) || !dj["spot"].is_null() {
            out.push(Layer { act: LayerAct::Draw, on: true, text: tr("그리기"), btn: "✕".into(), tip: tr("그린 것 지우기"), icon: "brush" });
        }
        out
    }

    /// 지금 화면 칩을 눌렀을 때 (control.js #lvLayers click)
    fn layer_action(&mut self, act: LayerAct, s: &Snap, acts: &mut Vec<Act>) {
        match act {
            LayerAct::Content => acts.push(Act::ToggleClear),
            LayerAct::Media => self.core.lock().set_background_media(None),
            LayerAct::BgOn => acts.push(Act::ToggleBg),
            LayerAct::Logo => acts.push(Act::ToggleLogo),
            LayerAct::Overlays => self.core.lock().toggle_overlays_off(),
            LayerAct::Draw => self.core.lock().draw_clear(),
            LayerAct::BgItem => {
                // 지금 송출 중인 순서(열린 큐시트)에서 배경 빼기
                let Some(l) = &s.live else { return };
                let (cid, idx) = (l.deck.passage["cueId"].as_str().unwrap_or(""), l.deck.passage["cueIndex"].as_i64().unwrap_or(-1));
                if idx < 0 || cid != s.cue_id { return; }
                if let Some(mut it) = s.items.get(idx as usize).cloned() {
                    it["bg"] = json!("");
                    acts.push(Act::UpdateItem(idx as usize, it));
                }
            }
            LayerAct::BgTpl => {
                // 템플릿 배경을 단색으로 (Electron app.removeTemplateBg)
                let mut t = s.tpl_main.clone();
                if t.is_null() { return; }
                let color = t["background"]["color"].as_str().filter(|x| !x.is_empty()).unwrap_or("#000000").to_string();
                t["background"]["type"] = json!("color");
                t["background"]["color"] = json!(color);
                let mut c = self.core.lock();
                let _ = wp_core::templates::save_template(&mut c.settings, &t);
                c.emit(wp_core::core::Ev::Live);
                c.emit(wp_core::core::Ev::Ui);
            }
        }
    }
}

// ---------------------------------------------------------------- 미리보기 · LIVE 패널 조각 (deck_view 전용)

/// 패널 제목 (control.js: 미리보기 = 본문 제목 · 번역본, LIVE = 슬라이드 장절(순서면 순서 제목) · 번역본)
fn deck_info(d: &Deck, is_live: bool) -> String {
    let slide = d.slides.get(d.index);
    let sref = slide.and_then(|x| x["ref"].as_str()).unwrap_or("");
    let head = if is_live {
        let in_cue = d.passage["cueIndex"].as_i64().is_some_and(|i| i >= 0);
        let kind = slide.and_then(|x| x["kind"].as_str()).unwrap_or("");
        if in_cue && kind != "bible" { d.passage["itemTitle"].as_str().filter(|x| !x.is_empty()).unwrap_or(sref).to_string() } else { sref.to_string() }
    } else {
        d.passage["title"].as_str().unwrap_or("").to_string()
    };
    [head.as_str(), d.passage["versionName"].as_str().unwrap_or("")].iter().filter(|x| !x.is_empty()).cloned().collect::<Vec<_>>().join(" · ")
}

/// .hidden-badge (warn 바탕 · 검은 글자 · 700 · 12px · 여백 2px 8px)
fn hidden_badge(ui: &mut egui::Ui, text: &str) {
    let p = crate::theme::pal();
    let g = ui.painter().layout_no_wrap(text.to_string(), crate::theme::font(12.0, crate::theme::Weight::Bold), Color32::BLACK);
    let (r, _) = ui.allocate_exact_size(g.size() + Vec2::new(16.0, 4.0), Sense::hover());
    ui.painter().rect_filled(r, crate::theme::cr(crate::theme::Rad::Sm), p.warn);
    ui.painter().galley(r.center() - g.size() / 2.0, g, Color32::BLACK);
}

/// 점선 테두리 (.live.is-hidden .screen { border-style: dashed })
fn dashed_rect(ui: &egui::Ui, r: egui::Rect, st: egui::Stroke) {
    let pts = vec![r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
    for s in egui::Shape::dashed_line(&pts, st, 8.0, 5.0) { ui.painter().add(s); }
}

/// .count (muted, 가운데, 최소 48px)
fn count_label(ui: &mut egui::Ui, t: String) {
    let p = crate::theme::pal();
    let g = ui.painter().layout_no_wrap(t, crate::theme::font(14.0, crate::theme::Weight::Regular), p.muted);
    let (r, _) = ui.allocate_exact_size(Vec2::new(g.size().x.max(48.0), g.size().y), Sense::hover());
    ui.painter().galley(r.center() - g.size() / 2.0, g, p.muted);
}

/// 슬라이드 짧은 글 (control.js slideSnippet)
pub(crate) fn slide_snippet(s: &Value) -> String {
    use wp_core::i18n::tr;
    match s["kind"].as_str().unwrap_or("") {
        "title" => [s["ref"].as_str().unwrap_or(""), s["subtitle"].as_str().unwrap_or("")].iter().filter(|x| !x.is_empty()).cloned().collect::<Vec<_>>().join(" · "),
        "image" => tr("이미지"),
        "video" => tr("영상"),
        "blank" => tr(if s["mode"] == "background" { "배경만" } else { "검은 화면" }),
        _ => {
            let t: Vec<String> = s["parts"].as_array().into_iter().flatten().map(|p| strip_marks(p["text"].as_str().unwrap_or(""))).collect();
            let t = t.join(" ").split_whitespace().collect::<Vec<_>>().join(" ");
            if t.chars().count() > 46 { format!("{}…", t.chars().take(46).collect::<String>()) } else { t }
        }
    }
}

/// 꾸밈 표시(**굵게** 등)·코드([C] 같은 화음) 빼기 — BCGroup.stripFmt · stripChords 근사
fn strip_marks(t: &str) -> String {
    let mut out = String::with_capacity(t.len());
    let mut depth = 0;
    for ch in t.chars() {
        match ch {
            '[' => depth += 1,
            ']' if depth > 0 => depth -= 1,
            '*' | '_' | '~' | '{' | '}' => {}
            c if depth == 0 => out.push(if c == '\n' { ' ' } else { c }),
            _ => {}
        }
    }
    out
}

/// '다음: <b>요한복음 3:17</b> 하나님이 …' (control.js pvNextHint · lvNextHint)
fn next_hint(d: &Deck, continuous: bool) -> egui::text::LayoutJob {
    use crate::theme::{font, pal, Weight};
    use wp_core::i18n::tr;
    let p = pal();
    let mut job = egui::text::LayoutJob::default();
    let fmt = |w: Weight, c: Color32| egui::TextFormat { font_id: font(12.0, w), color: c, valign: Align::Center, ..Default::default() };
    match d.slides.get(d.index + 1) {
        Some(nx) => {
            job.append(&format!("{} ", tr("다음:")), 0.0, fmt(Weight::Regular, p.muted));
            job.append(nx["ref"].as_str().unwrap_or(""), 0.0, fmt(Weight::Bold, p.text2));
            job.append(&format!(" {}", slide_snippet(nx)), 0.0, fmt(Weight::Regular, p.muted));
        }
        None => job.append(&tr(if continuous { "범위 끝 — F6을 누르면 다음 절로 이어갑니다" } else { "마지막 슬라이드" }), 0.0, fmt(Weight::Regular, p.muted)),
    }
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job
}

/// 슬라이드 칩 글 (bible: '3:16' = 장절에서 책 이름 뺀 것, 찬양: 이름표)
fn slide_chip_text(s: &Value) -> String {
    if let Some(l) = s["label"].as_str().filter(|x| !x.is_empty()) { return l.to_string(); }
    if let Some(r) = s["ref"].as_str().filter(|x| !x.is_empty()) {
        return match r.split_once(char::is_whitespace) { Some((_, rest)) => rest.to_string(), None => r.to_string() };
    }
    slide_snippet(s).chars().take(12).collect()
}

/// control.js chipLabel: '1. 3:16' (글이 번호와 같으면 번호만)
fn chip_label(i: usize, t: &str) -> String {
    if t.is_empty() || t == (i + 1).to_string() { (i + 1).to_string() } else { format!("{}. {t}", i + 1) }
}

/// .thumbs button (12px, 여백 2px 8px). 고른 칸: 미리보기 sel 바탕 + accent 테두리 · LIVE live-bg 바탕 + live 테두리
fn thumb_chip(ui: &mut egui::Ui, text: &str, on: bool, is_live: bool) -> egui::Response {
    use crate::theme::{font, pal, Weight};
    let p = pal();
    let g = ui.painter().layout_no_wrap(text.to_string(), font(12.0, Weight::Regular), p.text);
    let (r, resp) = ui.allocate_exact_size(Vec2::new(g.size().x + 16.0 + 2.0, g.size().y.max(16.0) + 6.0), Sense::click());
    let (bg, border) = match (on, is_live) {
        (true, true) => (p.live_bg, p.live),
        (true, false) => (p.sel, p.accent),
        _ if resp.hovered() => (p.hover, p.line2),
        _ => (p.panel2, p.line),
    };
    ui.painter().rect(r, crate::theme::cr(crate::theme::Rad::Md), bg, egui::Stroke::new(p.border_w, border), egui::StrokeKind::Inside);
    ui.painter().galley(r.center() - g.size() / 2.0, g, p.text);
    if resp.hovered() { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
    resp
}

/// (나) 슬라이드 줄의 한 칸: 16:9 작은 그림(84×47) + 아래 번호. 지금 장 = 빨강(송출 중) / 노랑(다음) 테두리, 지난 장은 흐리게
#[allow(clippy::too_many_arguments)]
fn slide_tile(ui: &mut egui::Ui, c: &paint::Ctx, sl: &Value, tpl: &Value, version: &str, label: &str, on: bool, past: bool, is_live: bool) -> egui::Response {
    use crate::theme::{font, pal, Weight};
    let p = pal();
    let (tw, th) = (84.0, 47.0);
    let (r, resp) = ui.allocate_exact_size(Vec2::new(tw, th + 17.0), Sense::click());
    if ui.is_rect_visible(r) {
        let img = egui::Rect::from_min_size(r.min, Vec2::new(tw, th));
        ui.painter().rect_filled(img, 0.0, Color32::BLACK);
        paint_slide(ui, c, img, Some(sl), tpl, version, true, false, false);
        if past {
            ui.painter().rect_filled(img, 0.0, Color32::from_black_alpha(120));
        }
        let col = if is_live { p.live } else { p.next };
        if on {
            ui.painter().rect_stroke(img, 0.0, egui::Stroke::new(if is_live { 3.0 } else { 2.0 }, col), egui::StrokeKind::Outside);
        } else {
            ui.painter().rect_stroke(img, 0.0, egui::Stroke::new(1.0, if resp.hovered() { p.line2 } else { Color32::BLACK }), egui::StrokeKind::Outside);
        }
        let tc = if on { if is_live { p.live_text } else { p.next } } else if past { p.muted2 } else { p.muted };
        ui.painter().with_clip_rect(r).text(egui::Pos2::new(r.center().x, img.bottom() + 9.0), egui::Align2::CENTER_CENTER, label, font(10.5, if on { Weight::Bold } else { Weight::Regular }), tc);
    }
    if resp.hovered() { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
    resp
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LayerAct { Content, Media, BgOn, BgItem, BgTpl, Overlays, Logo, Draw }

/// 지금 화면 칩 하나 (renderScreenLayers chip)
struct Layer { act: LayerAct, on: bool, text: String, btn: String, tip: String, icon: &'static str }

/// .lv-layers .ly: 둥근 칩(12) · 아이콘 14 · 글 12px · 오른쪽 ✕ 동그라미. 켜진 층은 live 테두리, 꺼진 층은 점선·흐리게
fn layer_chip(ui: &mut egui::Ui, ly: &Layer) -> egui::Response {
    use crate::theme::{alpha, font, pal, Weight};
    let p = pal();
    let fg = if ly.on { p.text } else { p.text.gamma_multiply(0.8) };
    let mut tg = ui.painter().layout_no_wrap(ly.text.clone(), font(12.0, Weight::Regular), fg);
    if tg.size().x > 220.0 {
        let mut job = egui::text::LayoutJob::simple(ly.text.clone(), font(12.0, Weight::Regular), fg, 220.0);
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        tg = ui.painter().layout_job(job);
    }
    let xg = ui.painter().layout_no_wrap(ly.btn.clone(), font(12.0, Weight::Bold), fg);
    let xw = xg.size().x + 12.0;
    let h = 26.0; // line-height 20 + 여백 2·2 + 테두리
    let w = 8.0 + 14.0 + 5.0 + tg.size().x + 5.0 + xw + 4.0;
    let (r, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click());
    let hovered = resp.hovered();
    let pt = ui.painter();
    let bg = if hovered { p.hover } else { p.panel2 };
    let border = if ly.on { p.live } else if hovered { p.line2 } else { p.line };
    // (나): 둥근 알약 대신 각진 꼬리표 (둥글기 Sm), 켜진 층은 왼쪽 빨간 테두리 대신 같은 1px 빨강
    let cr = if p.bevel { crate::theme::cr(crate::theme::Rad::Sm) } else { crate::theme::cr(crate::theme::Rad::Xl) };
    if ly.on {
        pt.rect(r, cr, bg, egui::Stroke::new(p.border_w, border), egui::StrokeKind::Inside);
    } else {
        pt.rect_filled(r, cr, bg.gamma_multiply(0.8));
        let rr = r.shrink(0.5);
        let pts = vec![rr.left_top() + Vec2::new(12.0, 0.0), rr.right_top() - Vec2::new(12.0, 0.0)];
        for s in egui::Shape::dashed_line(&pts, egui::Stroke::new(1.0, border), 4.0, 3.0) { pt.add(s); }
        let pts = vec![rr.left_bottom() + Vec2::new(12.0, 0.0), rr.right_bottom() - Vec2::new(12.0, 0.0)];
        for s in egui::Shape::dashed_line(&pts, egui::Stroke::new(1.0, border), 4.0, 3.0) { pt.add(s); }
    }
    let mut x = r.left() + 8.0;
    let icol = if ly.on { p.live_text } else { p.muted }; // 나가는 중 = 송출 빨강 (오류 색 아님)
    crate::icons::paint(ui, egui::Rect::from_min_size(egui::Pos2::new(x, r.center().y - 7.0), Vec2::splat(14.0)), ly.icon, icol);
    x += 14.0 + 5.0;
    let pt = ui.painter();
    pt.galley(egui::Pos2::new(x, r.center().y - tg.size().y / 2.0), tg.clone(), fg);
    x += tg.size().x + 5.0;
    let xr = egui::Rect::from_min_size(egui::Pos2::new(x, r.center().y - 9.0), Vec2::new(xw, 18.0));
    let (xbg, xfg) = if ly.on && hovered { (p.live, Color32::WHITE) } else { (alpha(Color32::from_gray(127), 0.18), fg) };
    pt.rect_filled(xr, crate::theme::cr(crate::theme::Rad::Lg), xbg);
    pt.galley(xr.center() - xg.size() / 2.0, xg, xfg);
    if hovered { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
    resp.on_hover_text(ly.tip.clone())
}

/// 편집칸 높이: 본문 패널에서 차지하는 비율 (기본·최소·최대)
const EDITOR_FRAC: f32 = 0.46;
const EDITOR_FRAC_MIN: f32 = 0.2;
const EDITOR_FRAC_MAX: f32 = 0.85;

/// 비율 → 편집칸 높이 (너무 작거나 검색 결과를 다 덮지 않게)
fn editor_height(avail: f32, frac: f32) -> f32 {
    (avail * frac.clamp(EDITOR_FRAC_MIN, EDITOR_FRAC_MAX)).clamp(160.0_f32.min(avail), (avail - 80.0).max(160.0))
}

/// '크게': 편집칸이 본문 패널 전체를 쓰는지 (창을 닫으면 처음처럼)
pub(crate) fn editor_big(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(egui::Id::new("editor-big"))).unwrap_or(false)
}
fn set_editor_big(ctx: &egui::Context, on: bool) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("editor-big"), on));
}

static SHOT_DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn test_args() -> Option<(Instant, Option<String>, bool, Option<String>, u8)> {
    let a: Vec<String> = std::env::args().collect();
    let get = |n: &str| a.iter().position(|x| x == n).and_then(|i| a.get(i + 1).cloned());
    let shot = get("--shot");
    let search = get("--search");
    if shot.is_none() && search.is_none() {
        return None;
    }
    Some((Instant::now(), search, a.iter().any(|x| x == "--send"), shot, 0))
}

impl App {
    fn run_test(&mut self, ctx: &egui::Context, acts: &mut Vec<Act>) {
        let Some((t0, search, send, shot, stage)) = self.test.clone() else { return };
        let el = t0.elapsed().as_secs_f32();
        if stage == 0 && el > 1.0 {
            if let Some(q) = search { self.search = q.clone(); acts.push(Act::Search(q)); }
            // 점검용 --import-cue <파일>: 큐시트(.json)·찬양 콘티(.worship)를 가져와 연다 (--item 과 함께 쓰면 그 순서 미리보기)
            if let Some(p) = std::env::args().position(|x| x == "--import-cue").and_then(|i| std::env::args().nth(i + 1)) {
                let (n, first, errs, warns) = self.core.lock().import_cue_files_ex(&[std::path::PathBuf::from(&p)]);
                log::info!("점검: 큐시트 가져오기 {n}개, 오류 {errs:?}, 확인할 것 {warns:?}");
                if !first.is_empty() {
                    self.core.lock().open_saved_cue(&first);
                }
            }
            // 점검용 --pdf <파일>: PDF 를 쪽마다 그림으로 바꿔 슬라이드쇼 순서로 넣는다 (순서 넣기 → 더보기 → PDF… 와 같은 길)
            if let Some(f) = std::env::args().position(|x| x == "--pdf").and_then(|i| std::env::args().nth(i + 1)) {
                let dir = self.core.lock().data_dir.clone();
                let r = wp_core::pdf::render(&dir, std::path::Path::new(&f));
                log::info!("점검: PDF {f} → {:?}", r.as_ref().map(|v| v.len()));
                if let Ok(pages) = r {
                    if self.core.lock().cue.is_none() { acts.push(Act::NewCue); }
                    acts.push(Act::AddItem("slideshow", Some(wp_core::pdf::item("PDF", &pages))));
                }
            }
            // 점검용 --frame <열쇠>: 미리 만든 예배 틀(sunday · communion · wednesday · friday · dawn)로 새 큐시트
            if let Some(k) = std::env::args().position(|x| x == "--frame").and_then(|i| std::env::args().nth(i + 1)) {
                let r = self.core.lock().new_cue_from_frame(&k, false);
                log::info!("점검: 예배 틀 {k} → {r:?}");
            }
            // 점검용 --export-pptx <파일>: 열린 큐시트를 PPT 로 (큐시트 → PPT 파일로 내보내기와 같은 길)
            if let Some(p) = std::env::args().position(|x| x == "--export-pptx").and_then(|i| std::env::args().nth(i + 1)) {
                let r = self.core.lock().export_cue_pptx(std::path::Path::new(&p));
                log::info!("점검: PPT 내보내기 {r:?}");
            }
            if let Some(i) = std::env::args().position(|x| x == "--item").and_then(|i| std::env::args().nth(i + 1)).and_then(|x| x.parse().ok()) { self.sel = Some(i); acts.push(Act::PreviewItem(i)); }
            if send { acts.push(Act::SendPreview); }
            // 점검용 --outwin-esc-test: 예배당 창(wp-output)을 작은 창으로 실제로 띄우고, 창 안 페이지에서 Esc keydown 을 흉내 낸다 → 창이 꺼지고 상태가 '꺼짐' 으로
            if std::env::args().any(|x| x == "--outwin-esc-test") {
                let port = self.core.lock().port;
                self.out.extra_args = vec!["--selftest-esc".into()];
                self.out.open(port, self.monitor, true);
                log::info!("점검: 예배당 창 띄움 (running={})", self.out.running());
            }
            // 점검용 --edit-actions: 큐시트가 없으면 새로 만들고 제목 순서를 넣어 '⚡ 송출할 때 자동으로' 편집칸을 펼친다
            if std::env::args().any(|x| x == "--edit-actions") {
                if self.core.lock().cue.is_none() { acts.push(Act::NewCue); }
                acts.push(Act::AddItem("title", Some(json!({ "title": "설교", "subtitle": "점검", "music": "/lib/file?p=C%3A%5Cmusic%5Cintro.mp3", "musicVolume": 0.6,
                    "actions": { "timer": 40, "stageMsg": "설교 시작 — 휴대폰을 무음으로", "logo": "off", "bg": "show", "ovOff": "*", "stopMusic": true, "stagePin": "order", "obsScene": "설교", "obsRecord": "start", "companion": "1/0/2" } }))));
                self.editor_on = true;
                crate::actions_ui::set_open(true);
            }
            self.test.as_mut().unwrap().4 = 1;
        } else if stage == 1 && el > if std::env::args().any(|x| x == "--outwin-esc-test") { 10.0 } else { std::env::var("WP_SHOT_AT").ok().and_then(|v| v.parse().ok()).unwrap_or(3.5) } {
            if let Some(path) = shot.clone() {
                let c2 = ctx.clone();
                ctx.request_screenshot(move |img| {
                    let [w, h] = img.size;
                    let px: Vec<u8> = img.pixels.iter().flat_map(|p| p.to_array()).collect();
                    if let Some(buf) = image::RgbaImage::from_raw(w as u32, h as u32, px) { let _ = buf.save(&path); }
                    SHOT_DONE.store(true, std::sync::atomic::Ordering::SeqCst);
                    c2.request_repaint();
                });
            }
            self.test.as_mut().unwrap().4 = 2;
        } else if stage == 2 {
            if SHOT_DONE.load(std::sync::atomic::Ordering::SeqCst) { ctx.send_viewport_cmd(egui::ViewportCommand::Close); }
            if shot.is_some() && el > 15.0_f32.max(std::env::var("WP_SHOT_AT").ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0) + 10.0) { ctx.send_viewport_cmd(egui::ViewportCommand::Close); }
        }
        ctx.request_repaint();
    }
}

impl eframe::App for App {
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        wp_core::firstrun::frame_tick(); // 켜는 시간 로그 (처음 두 프레임만)
        // 글꼴 아틀라스(글자 그림 모음)는 폭이 '가장 큰 텍스처 변'이고 높이가 두 배씩 자란다.
        // 그래픽 카드 한계(8192)를 그대로 쓰면 슬라이드의 큰 한글 글자가 쌓일 때마다 수십 MB 씩 늘어
        // 8192×8192(256MB, 화면 쪽 사본까지 두 배)까지 간다. 4096 으로 묶으면 가득 찼을 때 egui 가 비우고 다시 채운다.
        static SIDE: once_cell::sync::Lazy<usize> = once_cell::sync::Lazy::new(|| std::env::var("WP_MAX_TEX").ok().and_then(|v| v.parse().ok()).unwrap_or(4096usize).clamp(1024, 16384));
        raw.max_texture_side = Some(raw.max_texture_side.unwrap_or(*SIDE).min(*SIDE));
        crate::system_ui::inject(ctx, raw); // [트랙 B] 점검용 --draw-test (켜지 않으면 아무것도 안 함)
        // 점검용 --hover X,Y : 그 자리에 마우스가 있는 것처럼 (마우스 올림 단추 스크린샷용, 매 프레임)
        static HOVER: once_cell::sync::Lazy<Option<egui::Pos2>> = once_cell::sync::Lazy::new(|| {
            let a: Vec<String> = std::env::args().collect();
            let v = a.iter().position(|x| x == "--hover").and_then(|i| a.get(i + 1))?;
            let (x, y) = v.split_once(',')?;
            Some(egui::pos2(x.trim().parse().ok()?, y.trim().parse().ok()?))
        });
        if let Some(p) = *HOVER {
            let _ = ctx;
            raw.events.push(egui::Event::PointerMoved(p));
        }
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // 예배 중 한 프레임의 오류(panic)로 프로그램이 꺼지면 예배당 창도 함께 꺼진다(작업 개체).
        // 그래서 그 프레임만 버리고 계속 돌아간다. 내용은 panic hook(logger) 이 logs\ 에 남긴다.
        // 점검용: WP_DEBUG_REPAINT=1 이면 다시 그리기를 부른 자리를 2초마다 로그에 (가만히 둘 때 CPU 점검)
        if std::env::var_os("WP_DEBUG_REPAINT").is_some() {
            static LAST: parking_lot::Mutex<Option<Instant>> = parking_lot::Mutex::new(None);
            static FRAMES: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            FRAMES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let mut l = LAST.lock();
            if l.is_none_or(|t| t.elapsed().as_secs_f32() > 2.0) {
                *l = Some(Instant::now());
                let n = FRAMES.swap(0, std::sync::atomic::Ordering::Relaxed);
                let (sum, max) = (FRAME_US_SUM.swap(0, std::sync::atomic::Ordering::Relaxed), FRAME_US_MAX.swap(0, std::sync::atomic::Ordering::Relaxed));
                let causes: Vec<String> = ui.ctx().repaint_causes().iter().take(6).map(|c| c.to_string()).collect();
                log::info!("다시 그리기 2초 {n}번, 프레임 평균 {:.1}ms · 최대 {:.1}ms, 부른 자리: {causes:?}", sum as f64 / 1000.0 / n.max(1) as f64, max as f64 / 1000.0);
            }
        }
        static FRAME_US_SUM: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        static FRAME_US_MAX: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let frame_t0 = Instant::now();
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.ui_body(ui)));
        let us = frame_t0.elapsed().as_micros() as u64;
        FRAME_US_SUM.fetch_add(us, std::sync::atomic::Ordering::Relaxed);
        FRAME_US_MAX.fetch_max(us, std::sync::atomic::Ordering::Relaxed);
        // 한 프레임이 0.25초를 넘으면(조작 화면이 끊김) 로그에 남긴다 — 예배 중 '멈칫' 원인 찾기
        if us > 250_000 {
            log::warn!("조작 화면 한 프레임이 {:.0}ms 걸림", us as f64 / 1000.0);
        }
        crate::imgcache::end_frame(ui.ctx()); // 그림 텍스처 상한 (오래 안 쓴 것 버리기)
        {
            // 점검용 WP_DEBUG_MEM: 처음 몇 프레임과 그 뒤 10초마다 메모리
            static F: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            static LAST: parking_lot::Mutex<Option<Instant>> = parking_lot::Mutex::new(None);
            let f = F.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if matches!(f, 0 | 1 | 5 | 30) {
                wp_core::util::mem_log(&format!("프레임 {f}"));
            }
            let mut l = LAST.lock();
            if l.is_none_or(|t| t.elapsed().as_secs() >= 10) {
                *l = Some(Instant::now());
                let per = |v: Vec<(String, usize)>| v.into_iter().filter(|x| x.1 >= 1 << 20).map(|x| format!("{}={:.0}MB", x.0.rsplit("::").next().unwrap_or(""), x.1 as f64 / 1048576.0)).collect::<Vec<_>>().join(" ");
                let lb: Vec<(String, usize)> = ui.ctx().loaders().bytes.lock().iter().map(|b| (b.id().to_string(), b.byte_size())).collect();
                let li: Vec<(String, usize)> = ui.ctx().loaders().image.lock().iter().map(|b| (b.id().to_string(), b.byte_size())).collect();
                let lt: Vec<(String, usize)> = ui.ctx().loaders().texture.lock().iter().map(|b| (b.id().to_string(), b.byte_size())).collect();
                let loaders = lb.iter().chain(li.iter()).chain(lt.iter()).map(|x| x.1).sum::<usize>();
                let loader_detail = format!("파일[{}] 풀어낸 그림[{}] 텍스처[{}]", per(lb), per(li), per(lt));
                let tm = ui.ctx().tex_manager();
                let tm = tm.read();
                let mut all: Vec<(usize, String)> = tm.allocated().map(|(_, m)| (m.bytes_used(), format!("{} {}x{}", m.name.chars().rev().take(28).collect::<String>().chars().rev().collect::<String>(), m.size[0], m.size[1]))).collect();
                all.sort_by_key(|x| std::cmp::Reverse(x.0));
                let total: usize = all.iter().map(|x| x.0).sum();
                let (n, lru_mb) = crate::imgcache::stats();
                wp_core::util::mem_log(&format!("프레임 {f} (10초마다) {loader_detail} · 그림 읽기 {:.0}MB · 텍스처 {}개 {:.0}MB · LRU {n}개 {lru_mb:.0}MB · 큰 것 {:?} · {}", loaders as f64 / 1048576.0, all.len(), total as f64 / 1048576.0, all.iter().take(4).map(|x| format!("{}={:.0}MB", x.1, x.0 as f64 / 1048576.0)).collect::<Vec<_>>(), wp_core::util::mem_regions_summary()));
            }
        }
        if r.is_err() {
            static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if n == 0 || n % 100 == 99 {
                self.core.lock().notify(wp_core::i18n::tr_s("조작 화면에 오류가 있었지만 송출은 계속됩니다. 도움말 > 로그 폴더에 기록했습니다."), "error");
            }
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
        }
    }
}

impl App {
    fn ui_body(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        crate::dnd::frame(self, &ctx); // 끌어다 놓은 파일 · 붙여 넣은 파일 처리와 안내
        // 점검용 --panic-test ui|relock : 60번째 프레임 뒤에 한 번, 일부러 오류를 낸다.
        // (logs\ 에 남는지 · 조작 화면이 계속 도는지 · 예배당 출력이 유지되는지 확인)
        {
            static KIND: once_cell::sync::Lazy<Option<String>> = once_cell::sync::Lazy::new(|| {
                let a: Vec<String> = std::env::args().collect();
                a.iter().position(|x| x == "--panic-test").and_then(|i| a.get(i + 1).cloned())
            });
            static DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
            if let Some(k) = KIND.as_deref() {
                if ctx.cumulative_pass_nr() > 60 && !DONE.swap(true, std::sync::atomic::Ordering::SeqCst) {
                    if k == "relock" {
                        let _a = self.core.lock();
                        let _b = self.core.lock(); // 재진입: 멈추지 않고 panic 이어야 한다
                    }
                    panic!("점검용: 일부러 낸 오류 (--panic-test {k})");
                }
            }
        }
        let mut acts = vec![];
        let typing = ctx.egui_wants_keyboard_input();
        self.shortcuts(&ctx, &mut acts, typing);
        crate::parity2_ui::shortcuts(self, &ctx); // [트랙 A] Ctrl+B · Ctrl+Shift+V (위젯보다 먼저)
        self.trackb_shortcuts(&ctx); // [트랙 B]
        self.run_test(&ctx, &mut acts);
        if crate::parity_shot::run(self, ui) { return; } // 점검용 --parity: 슬라이드 그림만 창 전체에
        let s = self.snap();

        crate::track_c::menu_bar(self, ui, &mut acts); // [트랙 C] 메뉴 줄 (트랙 B·D 항목도 여기서 연다)
        crate::track_c::windows(self, &ctx, &mut acts); // [트랙 C] 설정·템플릿·기록 창
        // 시험에서는 띄우지 않는다: 이 PC 의 실제 1.x 자료를 보고 뜨는 전역 상태(firstrun::ASK)라 화면 전체 합성 시험을 가린다
        if !cfg!(test) { crate::trackd_ui::migrate_ask(&ctx, &self.data_dir); } // [갈래 D] 1.x 자료가 더 최근이면 다시 가져올지 묻기
        // 송출 기록에서 다시 띄운 본문: 검색칸 글과 절 목록도 그 본문으로 (control.js:1362 `$('#q').value = …; loadPassage`)
        self.take_history_loaded();
        let s = self.snap(); // [트랙 C] 창에서 바뀐 것 반영
        // (나) 위 컨트롤 막대 (bar 색 · 아래 1px edge) — probar_ui.rs
        {
            let p = crate::theme::pal();
            let tr_ = egui::Panel::top("top").frame(egui::Frame::new().fill(p.bar).inner_margin(egui::Margin::ZERO).stroke(egui::Stroke::NONE)).show(ui, |ui| {
                self.control_bar(ui, &s, &mut acts);
                ui.painter().hline(ui.max_rect().x_range(), ui.max_rect().bottom() - 0.5, egui::Stroke::new(1.0, p.edge));
            });
            // 알림 띠가 놓일 높이 (컨트롤 막대 바로 아래)
            ctx.data_mut(|d| d.insert_temp(egui::Id::new("pro-top-y"), tr_.response.rect.bottom()));
        }
        if !s.preview_mode {
            // 바로 송출 중: 주황 띠를 계속 보인다 (조작 실수 막기)
            let p = crate::theme::pal();
            egui::Panel::top("direct-band").frame(egui::Frame::new().fill(p.warn_bg).stroke(egui::Stroke::new(1.0, p.warn)).inner_margin(egui::Margin::symmetric(12, if p.bevel { 1 } else { 4 }))).show(ui, |ui| {
                self.direct_band(ui, &mut acts);
            });
        }
        // 아래 상태 줄: 미리보기/바로 송출 · 출력 연결 상태 · 영상·음악 조작 (예전 송출 줄의 단추는 위 컨트롤 막대로)
        {
            let p = crate::theme::pal();
            egui::Panel::bottom("status").frame(egui::Frame::new().fill(p.head).inner_margin(egui::Margin::symmetric(10, 2))).show(ui, |ui| {
                ui.painter().hline(ui.max_rect().x_range(), ui.max_rect().top() - 2.0, egui::Stroke::new(1.0, p.edge));
                self.status_strip(ui, &s, &mut acts);
            });
        }
        if let Some(a) = self.toast(&ctx, &s) { acts.push(a); }
        if std::mem::take(&mut self.editor_reveal) { self.reveal_editor_panel(); }
        self.workspace_window(&ctx); // '배치' 창 (보기 메뉴): 큐시트 판 · 인스펙터 · 시간표 보이기, 크기 처음대로
        self.pro_layout(ui, &s, &mut acts); // (나) 고정 배치 — prolayout_ui.rs (예전 독 작업공간 대신)
        if std::mem::take(&mut self.editor_reveal) {
            self.reveal_editor_panel();
            ctx.request_repaint();
        }
        self.trackb_windows(&ctx); // [트랙 B]
        crate::tools_ui::show(&ctx, &self.core); // [트랙 D]
        crate::ai_ui::show(self, &ctx); // AI 도우미 (설정 · 동의 · 예배 전 점검 …)
        crate::lyricsfix_ui::show(self, &ctx, &mut acts); // 가사 자동 정리
        crate::say_ui::show(self, &ctx, &mut acts); // 말로 시키기
        for id in crate::say_ui::take_pending() {
            crate::track_c::handle_menu(self, &id);
        }
        crate::a_tools::show(self, &ctx, &mut acts); // [트랙 A] 설교 포인트 · 예배 전 화면 · 홈페이지 주보
        crate::parity_ui::show(self, &ctx); // [트랙 A] 단축키 보기
        crate::parity2_ui::show(self, &ctx, &mut acts); // [트랙 A] 성경 책 목록 · 종류 골라 붙여 넣기
        crate::trackd_ui::frame(self, &ctx); // [트랙 D] 주보 · 출력 설정 · OBS 창, 예배당 창 자동 켜기
        if crate::tools_ui::take_off_request() { self.out.close(); } // [트랙 D] 리모컨 '끄기' → 예배당 창도

        if !acts.is_empty() {
            self.apply(acts);
            ctx.request_repaint();
        }
        // 복사한 뒤 '복사했습니다' 알림 (widgets::copy_notice 를 쓴 모든 복사 지점)
        if let Some(m) = crate::widgets::take_copied(&ctx) {
            self.core.lock().notify(m, "ok");
            ctx.request_repaint();
        }
        if self.last_flush.elapsed().as_secs_f32() > 1.0 {
            self.core.lock().flush_bg(); // 파일 쓰기는 백그라운드 (잠금을 쥔 채 쓰면 화면이 멈칫한다)
            self.last_flush = Instant::now();
        }
        // 아무 일도 없으면 2초에 한 번만 다시 그린다 (출력 창 상태 확인 · 설정 저장 같은 느린 점검용).
        // 알림이 떠 있으면 사라질 때에 맞춰 0.5초마다. 시계·타이머·카운트다운은 그것을 그리는 자리에서 따로 1초마다 부른다.
        // 코어가 바뀌면(Ev) main.rs 의 스레드가 바로 다시 그리게 하므로 반응은 느려지지 않는다.
        let toast_alive = s.toast.as_ref().is_some_and(|t| wp_core::util::now_ms() - t.2 < 12_500);
        ctx.request_repaint_after(std::time::Duration::from_millis(if toast_alive { 500 } else { 2000 }));
    }

}

impl App {
 pub(crate) fn workspace_cue(&mut self, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
            crate::hymns_ui::global(self, ui, acts); // [트랙 A] Ctrl+V 붙여 넣기 받기 (어느 쪽이든)
            // 원본 #panelCue: '큐시트 (예배 순서) | 성경 책'. 찬송가·찬양·라이브러리는 라이브러리 패널 탭에 있다.
            crate::cue_ui::side(self, ui, acts, |app, ui, acts| app.cue_panel(ui, s, acts));
 }
}

impl Drop for App {
    fn drop(&mut self) {
        self.out.close();
        crate::camscan::kill_all(); // 내가 띄운 카메라 도우미·미리보기 창만
        wp_core::sync::on_exit(&mut self.core.lock()); // [트랙 B] 끝날 때 동기화 파일 쓰기
        self.core.lock().flush();
    }
}

#[cfg(test)]
#[path = "ui_integration_tests.rs"]
mod ui_integration_tests;
#[cfg(test)]
#[path = "ux_a_tests.rs"]
mod ux_a_tests;
#[cfg(test)]
#[path = "fix3_tests.rs"]
mod fix3_tests;
#[cfg(test)]
#[path = "auditc_tests.rs"]
mod auditc_tests;
#[cfg(test)]
#[path = "dnd_tests.rs"]
mod dnd_tests;
#[cfg(test)]
#[path = "dbl_tests.rs"]
mod dbl_tests;
#[cfg(test)]
#[path = "auditd_tests.rs"]
mod auditd_tests;
#[cfg(test)]
#[path = "audite_tests.rs"]
mod audite_tests;
#[cfg(test)]
#[path = "slideedit_focus_tests.rs"]
mod slideedit_focus_tests;
#[cfg(test)]
#[path = "nav_tests.rs"]
mod nav_tests;
#[cfg(test)]
#[path = "settings_center_tests.rs"]
mod settings_center_tests; // 설정 센터: 검색 · 변경 · 열기 단추 · 기본값 복원 · 단축키
#[cfg(test)]
#[path = "settings_theme_link_tests.rs"]
mod settings_theme_link_tests;
#[cfg(test)]
impl App {
    /// 시험용 App (출력 창·점검 인자 없이)
    pub(crate) fn for_test(core: Shared, dir: std::path::PathBuf) -> Self {
        let workspace = crate::workspace_ui::WorkspaceUi::new(core.lock().settings.get());
        Self { editor_on: true, editor_reveal: false, views: MonitorViews::default(), workspace, core, web_dir: dir.clone(), data_dir: dir, search: String::new(), result: Value::Null, versions: vec![], sel: None, edit: None, out: crate::outwin::OutWin::for_test(), monitor: 0, last_flush: Instant::now(), focus_search: false, test: None, tb: Default::default(), tc: Default::default() }
    }
}
/// 머리 줄 · 아래 송출 줄 · 상태 · 알림 (Snap 의 private 필드를 읽도록 자식 모듈)
#[path = "topbar_ui.rs"]
mod topbar_ui;
/// 두 모니터의 보기 전환은 실제 송출 설정을 변경하지 않는다.
/// 뷰어마다 고른 출력 id ("" = 예배당, "overlay" = OBS, 추가 출력 out-…)
#[derive(Default)]
struct MonitorViews { preview: String, live: String }
impl MonitorViews {
    fn pick(&self, live: bool) -> &str { if live { &self.live } else { &self.preview } }
    fn obs(&self, live: bool) -> bool { self.pick(live) == "overlay" }
}
fn reveal_main(n: &mut wp_core::workspace::Node) {
    use wp_core::workspace::{Node, Pane};
    match n {
        Node::Tabs { panes, active, collapsed } if panes.contains(&Pane::Main) => { *active = Pane::Main; *collapsed = false; }
        Node::Split { a, b, .. } => { reveal_main(a); reveal_main(b); }
        _ => {}
    }
}
#[cfg(test)]
mod preview_edit_tests {
    use super::*;
    #[test]
    fn monitor_views_are_independent() {
        let mut v = MonitorViews::default();
        assert!(!v.obs(false) && !v.obs(true));
        v.preview = "overlay".into();
        assert!(v.obs(false) && !v.obs(true));
        v.live = "overlay".into(); v.preview = "out-lobby".into();
        assert!(!v.obs(false) && v.obs(true));
        assert_eq!(v.pick(false), "out-lobby");
    }
    #[test]
    fn editor_reveals_inactive_collapsed_main_tab() {
        use wp_core::workspace::{Node, Pane};
        let mut n = Node::Tabs { panes: vec![Pane::Cue, Pane::Main], active: Pane::Cue, collapsed: true };
        reveal_main(&mut n);
        assert!(n.rendered(Pane::Main));
    }
}
