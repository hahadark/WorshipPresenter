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
    Search(String),
    PreviewPassage(Value, String, Option<(i64, i64)>),
    PreviewRef(String, Option<(i64, i64)>),
    SendRef(String, Option<(i64, i64)>),
    PreviewItem(usize),
    SendItem(usize),
    SendPreview,
    PreviewIndex(usize),
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
    cue_name: String,
    cue_id: String,
    items: Vec<Value>,
    cues: Vec<CueSummary>,
    templates: Vec<(String, String)>,
    pub(crate) tpl_main: Value,
    tpl_overlay: Value,
    preview_main: Value,
    preview_overlay: Value,
    active_main: String,
    active_overlay: String,
    version: String,
    preview_mode: bool,
    toast: Option<(String, &'static str, i64)>,
    outputs: Vec<(String, String)>,
    port: u16,
    logo: bool,
    hide_bg: bool,
}

fn type_info(t: &str) -> (&'static str, Color32) {
    match t {
        "bible" => ("성경", Color32::from_rgb(0x5b, 0x9c, 0xf6)),
        "song" => ("찬양", Color32::from_rgb(0xe0, 0x7b, 0xd6)),
        "hymn" => ("찬송", Color32::from_rgb(0xf0, 0x9a, 0x4a)),
        "text" => ("공지", Color32::from_rgb(0x58, 0xc2, 0x8a)),
        "title" => ("제목", Color32::from_rgb(0xc7, 0xa6, 0x4b)),
        "blank" => ("빈 화면", Color32::from_rgb(0x8a, 0x93, 0xa3)),
        "image" => ("그림", Color32::from_rgb(0x4f, 0xc3, 0xc9)),
        "camera" => ("카메라", Color32::from_rgb(0xd9, 0x6a, 0x6a)),
        "video" => ("영상", Color32::from_rgb(0xd9, 0x6a, 0x6a)),
        "slideshow" => ("슬라이드쇼", Color32::from_rgb(0x9b, 0x87, 0xf5)),
        _ => ("순서", Color32::GRAY),
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
    ws.toggle(Pane::Preview);
    ws.locked = locked;
}

impl App {
    pub fn new(core: Shared, web_dir: std::path::PathBuf, data_dir: std::path::PathBuf) -> Self {
        let versions = core.lock().store.list_versions();
        let out = OutWin::new();
        let saved = core.lock().settings.get()["rsOutputMonitor"].as_u64().map(|x| x as usize);
        let monitor = saved.filter(|m| out.monitors.iter().any(|x| x.index == *m)).unwrap_or_else(|| out.default_monitor());
        let mut workspace = crate::workspace_ui::WorkspaceUi::new(core.lock().settings.get());
        // 원본 dock.js:409 — 미리보기 끔(바로 송출)이면 미리보기 패널을 닫는다
        sync_preview_pane(&mut workspace.state, core.lock().settings.get()["previewMode"].as_bool().unwrap_or(true));
        Self { editor_on: true, editor_reveal: false, views: MonitorViews::default(), workspace, core, web_dir, data_dir, search: String::new(), result: Value::Null, versions, sel: None, edit: None, out, monitor, last_flush: Instant::now(), focus_search: true, test: test_args(), tb: Default::default() /* [트랙 B] */, tc: Default::default() /* [트랙 C] */ }
    }

    fn snap(&self) -> Snap {
        let c = self.core.lock();
        let s = c.settings.get();
        Snap {
            preview: c.pres.preview.clone(),
            live: c.pres.live.clone(),
            cue_name: c.cue.as_ref().map(|x| x.name.clone()).unwrap_or_default(),
            cue_id: c.cue.as_ref().map(|x| x.id.clone()).unwrap_or_default(),
            items: c.cue.as_ref().map(|x| x.items.clone()).unwrap_or_default(),
            cues: c.cues.clone(),
            templates: c.settings.templates().iter().map(|t| (t["id"].as_str().unwrap_or("").to_string(), t["name"].as_str().unwrap_or("").to_string())).collect(),
            preview_main: c.template_for_deck("main", c.pres.preview.as_ref()),
            preview_overlay: c.template_for_deck("overlay", c.pres.preview.as_ref()),
            tpl_main: c.template_for("main"),
            tpl_overlay: c.template_for("overlay"),
            active_main: s["activeTemplates"]["main"].as_str().unwrap_or("").to_string(),
            active_overlay: s["activeTemplates"]["overlay"].as_str().unwrap_or("").to_string(),
            version: c.version_id(),
            preview_mode: s["previewMode"].as_bool().unwrap_or(true),
            toast: c.toast.clone(),
            outputs: c.outputs.values().map(|o| (o.client.clone(), o.kind.clone())).collect(),
            port: c.port,
            logo: c.layers.logo,
            hide_bg: c.layers.hide_bg,
        }
    }

    fn apply(&mut self, acts: Vec<Act>) {
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
                Act::PreviewIndex(i) => c.set_preview_index(i),
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
                    c.add_item(it, at);
                    let n = c.cue.as_ref().map(|x| x.items.len()).unwrap_or(0);
                    self.sel = Some(at.unwrap_or(n.saturating_sub(1)).min(n.saturating_sub(1)));
                    self.edit = None;
                }
                Act::RemoveItem(i) => {
                    c.remove_item(i);
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
                    c.notify(if on { wp_core::i18n::tr("미리보기를 켰습니다. 고른 본문은 미리보기에 놓이고 F9 로 송출합니다.") } else { wp_core::i18n::tr("미리보기를 껌습니다. 이제 클릭한 본문·순서·슬라이드가 바로 송출됩니다.") }, if on { "ok" } else { "info" });
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
        // [트랙 C] 설정 → 단축키에서 바꾼 키를 쓴다 (없으면 기본값)
        let st = self.core.lock().settings.get()["shortcuts"].clone();
        let st = serde_json::json!({ "shortcuts": st });
        let mut on = |id: &str, def: &str| (!typing || crate::keys::safe_while_typing(&st, id, def)) && crate::keys::pressed(ctx, &st, id, def);
        if on("prevSlide", "F5") { acts.push(Act::Step(-1)) }
        if on("nextSlide", "F6") { acts.push(Act::Step(1)) }
        if on("sendPreview", "F9") { acts.push(Act::SendPreview) }
        if on("toggleClear", "F10") { acts.push(Act::ToggleClear) }
        if on("focusSearch", "Ctrl+K") { self.focus_search = true }
        if on("cueNext", "Ctrl+ArrowDown") { acts.push(Act::CuePreviewStep(1)) }
        if on("cuePrev", "Ctrl+ArrowUp") { acts.push(Act::CuePreviewStep(-1)) }
        if on("toggleOutputWindow", "Ctrl+Shift+F") { acts.push(Act::OutputToggle) }
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
            if let Some((i, n)) = pv {
                if on("previewNextSlide", "PageDown") && i + 1 < n { acts.push(Act::PreviewIndex(i + 1)) }
                if on("previewPrevSlide", "PageUp") && i > 0 { acts.push(Act::PreviewIndex(i - 1)) }
            }
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Space)) { acts.push(Act::Step(1)) }
        }
    }
    /// 큐시트 쪽 (Electron #cuePanel): 고르기 콤보 + 폴더 단추 · 순서 줄 · 넣기 단추 격자 · 도움말.
    /// 순서 옮기기·송출·지우기·편집칸은 오른쪽 클릭 메뉴와 본문 패널 편집칸에서 (원본 배치).
    pub(crate) fn cue_panel(&mut self, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
        let cues: Vec<(String, String)> = s.cues.iter().map(|c| (c.id.clone(), format!("{}{}{} ({})", if c.pinned { "📌 " } else { "" }, if c.template { "📄 " } else { "" }, c.name, c.count))).collect();
        let cue_label = cues.iter().find(|c| c.0 == s.cue_id).map(|c| c.1.clone()).unwrap_or_else(|| s.cue_name.clone());
        crate::cue_ui::head(self, ui, &s.cue_id, &cue_label, &cues, acts);
        ui.add_space(4.0);
        let live_idx = s.live.as_ref().filter(|l| l.deck.passage["cueId"] == json!(s.cue_id)).and_then(|l| l.deck.passage["cueIndex"].as_i64());
        let live_hidden = s.live.as_ref().is_some_and(|l| !l.visible);
        let pv_idx = s.preview.as_ref().filter(|p| p.passage["cueId"] == json!(s.cue_id)).and_then(|p| p.passage["cueIndex"].as_i64());
        let times = { let c = self.core.lock(); if c.settings.get()["cueTimes"] == true { c.planned_times() } else { vec![] } };
        let list_h = crate::cue_ui::list_height(ui);
        // .cue-list { flex: 1; padding: 2px 6px }
        egui::ScrollArea::vertical().id_salt("items").max_height(list_h).min_scrolled_height(list_h).auto_shrink([false, false]).show(ui, |ui| {
            egui::Frame::new().inner_margin(egui::Margin::symmetric(6, 2)).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for (i, it) in s.items.iter().enumerate() {
                    let st = crate::cue_ui::RowState {
                        selected: self.sel == Some(i),
                        pv: pv_idx == Some(i as i64),
                        air: live_idx == Some(i as i64),
                        hidden: live_hidden,
                        time: times.get(i).filter(|t| !t.is_empty()).map(|t| (t.clone(), it["duration"].as_i64().unwrap_or(0))),
                    };
                    let out = crate::cue_ui::row(ui, i, it, &st);
                    ui.add_space(2.0); // .cue-item { margin-bottom: 2px }
                    let r = out.resp;
                    self.cue_row_dnd(ui, &r, i); // [트랙 B] 끌어서 옮기기
                    crate::parity_ui::cue_row_menu(self, &r, i, it, s.items.len(), acts); // [트랙 A] 오른쪽 클릭 메뉴
                    if out.edit {
                        self.show_item_editor(i);
                        acts.push(Act::PreviewItem(i));
                    } else if r.clicked() {
                        self.sel = Some(i);
                        self.edit = None;
                        acts.push(Act::PreviewItem(i));
                    }
                    if r.double_clicked() && !out.edit {
                        acts.push(Act::SendItem(i));
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
        self.center_head(ui, s, &mut editor_on);
        self.editor_on = editor_on;
        // 원본처럼 순서를 골랐을 때만 편집칸을 보이고, 높이는 본문 패널의 절반 안쪽(control.css .cue-editor max-height 46%)으로 제한해 검색 결과가 가려지지 않게 한다.
        if let (true, Some(i)) = (self.editor_on, selected) {
            let h = (ui.available_height() * 0.46).max(160.0);
            ui.allocate_ui(egui::vec2(ui.available_width(), h), |ui| {
                ui.set_max_height(h);
                self.editor(ui, i, &s.items[i], acts);
            });
        }
        self.center_body(ui, s, acts);
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
        let tpls: Vec<(String, String)> = self.core.lock().settings.templates().iter().map(|t| (t["id"].as_str().unwrap_or("").to_string(), t["name"].as_str().unwrap_or("").to_string())).collect();
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
            head = ce_head(ui, i, &ty, "");
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
                        opts.extend(wp_core::themes::list_themes().iter().map(|t| (t["id"].as_str().unwrap_or("").to_string(), t["name"].as_str().unwrap_or("").to_string())));
                        if ce_select(ui, "ce-theme", &mut cur, &opts, 220.0) {
                            if cur.is_empty() {
                                e["tplMain"] = json!("");
                                e["tplOverlay"] = json!("");
                                return true;
                            }
                            if cur != "custom" {
                                if let Ok(ids) = wp_core::themes::ensure_theme(&mut core.lock(), &cur) {
                                    e["tplMain"] = ids["main"].clone();
                                    e["tplOverlay"] = ids["overlay"].clone();
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
        ui.painter().hline(r.x_range(), r.bottom() - 1.0, egui::Stroke::new(2.0, p.pv));
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
                if is_live { widgets::badge(ui, "LIVE", Badge::Live); } else { widgets::badge(ui, &tr("미리보기"), Badge::Pv); }
                let info = deck.map(|d| deck_info(d, is_live)).unwrap_or_default();
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let view = if is_live { &mut self.views.live_obs } else { &mut self.views.preview_obs };
                    let main_label = tr("예배당");
                    widgets::segmented(ui, view, &[(false, main_label.as_str()), (true, "OBS")]);
                    if hidden { hidden_badge(ui, &tr("숨김 중")); }
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        ui.add(egui::Label::new(theme::rich(info, 14.0, Weight::SemiBold, p.text)).truncate().selectable(false));
                    });
                });
            });
            ui.add_space(8.0); // .pane-head { margin-bottom: 8px }
            // ---- .screen: 16:9, 2px 테두리 (미리보기 pv · LIVE live · 숨김이면 점선), 둥글기 6
            let tpl = match (is_live, self.views.obs(is_live)) {
                (true, false) => &s.tpl_main, (true, true) => &s.tpl_overlay,
                (false, false) => &s.preview_main, (false, true) => &s.preview_overlay,
            };
            let c = paint::Ctx { web_dir: &self.web_dir, data_dir: &self.data_dir };
            let w = ui.available_width();
            let tw = tpl["width"].as_f64().unwrap_or(1920.0) as f32;
            let th = tpl["height"].as_f64().unwrap_or(1080.0) as f32;
            let reserve = if is_live { 128.0 } else { 74.0 };
            let h = (w * th / tw).min(ui.available_height() - reserve).max(60.0);
            let w2 = h * tw / th;
            let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
            let rect = egui::Rect::from_center_size(rect.center(), Vec2::new(w2, h));
            let slide = deck.and_then(|d| d.slides.get(d.index));
            let version = deck.map(|d| d.passage["versionName"].as_str().unwrap_or("").to_string()).unwrap_or_default();
            let (visible, logo, hide_bg) = match live { Some(l) => (l.visible, s.logo, s.hide_bg), None if is_live => (true, s.logo, s.hide_bg), None => (true, false, false) };
            ui.painter().rect_filled(rect, egui::CornerRadius::same(6), Color32::BLACK);
            paint_slide(ui, &c, rect.shrink(2.0), slide, tpl, &version, visible, logo, hide_bg);
            if deck.is_none() {
                let ph = if is_live { tr("송출 중인 본문 없음") } else { tr("검색한 본문이 여기에 표시됩니다") };
                ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, ph, theme::font(14.0, Weight::Regular), p.muted2);
            }
            let border = if is_live { p.live } else { p.pv };
            if hidden {
                dashed_rect(ui, rect.shrink(1.0), egui::Stroke::new(2.0, border));
            } else {
                ui.painter().rect_stroke(rect, egui::CornerRadius::same(6), egui::Stroke::new(2.0, border), egui::StrokeKind::Inside);
            }
            // ---- .pane-tools: (◀ n/m ▶) · 다음: … · 그리기
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.set_min_height(30.0);
                if !is_live {
                    let n = deck.map(|d| d.slides.len()).unwrap_or(0);
                    let i = deck.map(|d| d.index).unwrap_or(0);
                    if ui.add(Btn::new("◀").height(28.0).enabled(deck.is_some()).tooltip(tr("미리보기 이전 슬라이드 (PageUp)"))).clicked() && i > 0 { acts.push(Act::PreviewIndex(i - 1)) }
                    count_label(ui, deck.map(|d| format!("{} / {}", d.index + 1, d.slides.len())).unwrap_or_default());
                    if ui.add(Btn::new("▶").height(28.0).enabled(deck.is_some()).tooltip(tr("미리보기 다음 슬라이드 (PageDown)"))).clicked() && i + 1 < n { acts.push(Act::PreviewIndex(i + 1)) }
                } else if let Some(d) = deck {
                    count_label(ui, format!("{} / {}", d.index + 1, d.slides.len()));
                }
                let draw_on = is_live && crate::system_ui::ST.lock().draw;
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
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
            // ---- .lv-layers: 지금 화면 (층마다 ✕)
            if is_live {
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
            // ---- .thumbs: 슬라이드 칩 (1. 3:16 …)
            if let Some(d) = deck {
                ui.add_space(4.0);
                egui::ScrollArea::vertical().id_salt(format!("{}-slides", if is_live { "lv" } else { "pv" })).max_height(76.0).auto_shrink([false, true]).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
                        for (k, sl) in d.slides.iter().enumerate() {
                            let r = thumb_chip(ui, &chip_label(k, &slide_chip_text(sl)), k == d.index, is_live).on_hover_text(slide_snippet(sl));
                            if r.clicked() { acts.push(if is_live { Act::LiveGoto(k) } else { Act::PreviewIndex(k) }) }
                            if r.double_clicked() && !is_live { acts.push(Act::SendPreview) }
                            crate::parity2_ui::slide_menu(&self.core, &r, d, k, is_live, acts); // [트랙 A] 썸네일 오른쪽 클릭
                        }
                    });
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
    ui.painter().rect_filled(r, egui::CornerRadius::same(4), p.warn);
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
    ui.painter().rect(r, egui::CornerRadius::same(crate::theme::RADIUS), bg, egui::Stroke::new(p.border_w, border), egui::StrokeKind::Inside);
    ui.painter().galley(r.center() - g.size() / 2.0, g, p.text);
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
    let cr = egui::CornerRadius::same(12);
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
    let icol = if ly.on { p.danger_text } else { p.muted };
    crate::icons::paint(ui, egui::Rect::from_min_size(egui::Pos2::new(x, r.center().y - 7.0), Vec2::splat(14.0)), ly.icon, icol);
    x += 14.0 + 5.0;
    let pt = ui.painter();
    pt.galley(egui::Pos2::new(x, r.center().y - tg.size().y / 2.0), tg.clone(), fg);
    x += tg.size().x + 5.0;
    let xr = egui::Rect::from_min_size(egui::Pos2::new(x, r.center().y - 9.0), Vec2::new(xw, 18.0));
    let (xbg, xfg) = if ly.on && hovered { (p.live, Color32::WHITE) } else { (alpha(Color32::from_gray(127), 0.18), fg) };
    pt.rect_filled(xr, egui::CornerRadius::same(9), xbg);
    pt.galley(xr.center() - xg.size() / 2.0, xg, xfg);
    if hovered { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
    resp.on_hover_text(ly.tip.clone())
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
            if let Some(i) = std::env::args().position(|x| x == "--item").and_then(|i| std::env::args().nth(i + 1)).and_then(|x| x.parse().ok()) { self.sel = Some(i); acts.push(Act::PreviewItem(i)); }
            if send { acts.push(Act::SendPreview); }
            // 점검용 --edit-actions: 큐시트가 없으면 새로 만들고 제목 순서를 넣어 '⚡ 송출할 때 자동으로' 편집칸을 펼친다
            if std::env::args().any(|x| x == "--edit-actions") {
                if self.core.lock().cue.is_none() { acts.push(Act::NewCue); }
                acts.push(Act::AddItem("title", Some(json!({ "title": "설교", "subtitle": "점검", "music": "/lib/file?p=C%3A%5Cmusic%5Cintro.mp3", "musicVolume": 0.6,
                    "actions": { "timer": 40, "stageMsg": "설교 시작 — 휴대폰을 무음으로", "logo": "off", "bg": "show", "ovOff": "*", "stopMusic": true, "stagePin": "order", "obsScene": "설교", "obsRecord": "start", "companion": "1/0/2" } }))));
                self.editor_on = true;
                crate::actions_ui::set_open(true);
            }
            self.test.as_mut().unwrap().4 = 1;
        } else if stage == 1 && el > 3.5 {
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
            if shot.is_some() && el > 15.0 { ctx.send_viewport_cmd(egui::ViewportCommand::Close); }
        }
        ctx.request_repaint();
    }
}

impl eframe::App for App {
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        crate::system_ui::inject(ctx, raw); // [트랙 B] 점검용 --draw-test (켜지 않으면 아무것도 안 함)
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let mut acts = vec![];
        let typing = ctx.egui_wants_keyboard_input();
        self.shortcuts(&ctx, &mut acts, typing);
        crate::parity2_ui::shortcuts(self, &ctx); // [트랙 A] Ctrl+B · Ctrl+Shift+V (위젯보다 먼저)
        self.trackb_shortcuts(&ctx); // [트랙 B]
        self.run_test(&ctx, &mut acts);
        let s = self.snap();

        crate::track_c::menu_bar(self, ui, &mut acts); // [트랙 C] 메뉴 줄 (트랙 B·D 항목도 여기서 연다)
        crate::track_c::windows(self, &ctx, &mut acts); // [트랙 C] 설정·템플릿·기록 창
        let s = self.snap(); // [트랙 C] 창에서 바뀐 것 반영
        // 머리 줄 (header.top: panel 바탕 · 아래 line) — topbar_ui.rs
        let pf = crate::theme::pal().panel;
        egui::Panel::top("top").frame(egui::Frame::new().fill(pf).inner_margin(topbar_ui::TOP_MARGIN)).show(ui, |ui| {
            self.top_bar(ui, &s, &mut acts);
        });
        // 아래 송출 줄 (footer.bottom: 창 맨 아래 전체 폭, panel 바탕 · 위 line) — topbar_ui.rs
        egui::Panel::bottom("status").frame(egui::Frame::new().fill(pf).inner_margin(topbar_ui::BOTTOM_MARGIN)).show(ui, |ui| {
            self.bottom_bar(ui, &s, &mut acts);
        });
        self.toast(&ctx, &s);
        if std::mem::take(&mut self.editor_reveal) { self.reveal_editor_panel(); }
        self.workspace_window(&ctx); // 작업공간 · 패널 배치 창 (고르기는 머리 줄)
        egui::CentralPanel::default_margins().show(ui, |ui| { self.workspace_content(ui, &s, &mut acts); });
        if std::mem::take(&mut self.editor_reveal) {
            self.reveal_editor_panel();
            ctx.request_repaint();
        }
        self.trackb_windows(&ctx); // [트랙 B]
        crate::tools_ui::show(&ctx, &self.core); // [트랙 D]
        crate::a_tools::show(self, &ctx, &mut acts); // [트랙 A] 설교 포인트 · 예배 전 화면 · 홈페이지 주보
        crate::parity_ui::show(self, &ctx); // [트랙 A] 단축키 보기
        crate::parity2_ui::show(self, &ctx, &mut acts); // [트랙 A] 성경 책 목록 · 종류 골라 붙여 넣기
        crate::trackd_ui::frame(self, &ctx); // [트랙 D] 주보 · 출력 설정 · OBS 창, 예배당 창 자동 켜기
        if crate::tools_ui::take_off_request() { self.out.close(); } // [트랙 D] 리모컨 '끄기' → 예배당 창도

        if !acts.is_empty() {
            self.apply(acts);
            ctx.request_repaint();
        }
        if self.last_flush.elapsed().as_secs_f32() > 1.0 {
            self.core.lock().flush();
            self.last_flush = Instant::now();
        }
        // 알림이 사라질 때·출력 창 상태를 위해 가끔 다시 그림
        ctx.request_repaint_after(std::time::Duration::from_millis(500));
    }

}

impl App {
 pub(crate) fn workspace_cue(&mut self, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
            crate::hymns_ui::global(self, ui, acts); // [트랙 A] Ctrl+V 붙여 넣기 받기 (어느 쪽이든)
            // 원본 #panelCue: '큐시트 (예배 순서) | 성경 책'. 찬송가·찬양·라이브러리는 라이브러리 패널 탭에 있다.
            crate::cue_ui::side(self, ui, acts, |app, ui, acts| app.cue_panel(ui, s, acts));
 }
 /// 아래 송출 줄 앞부분 (Electron footer.bottom): 이전 F5 · 다음 F6 · 송출 F9 | 글자 숨김 F10 · 지우기 ▾ · 모두 지우기.
 /// 가로 줄 안에서 부른다 (topbar_ui::bottom_bar).
 pub(crate) fn workspace_controls(&mut self,ui:&mut egui::Ui,s:&Snap,acts:&mut Vec<Act>){
            {
                use crate::widgets::{Btn, Kind, Size};
                use wp_core::i18n::tr;
                // 묶음(48)과 높이를 맞추려 위아래 4px (가로 줄 가운데 맞춤이 먹지 않아 틀로)
                egui::Frame::new().inner_margin(egui::Margin::symmetric(0, 4)).show(ui, |ui| { ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0; // .ft-group { gap: 6px }
                if ui.add(Btn::new(tr("이전")).icon("skip-back").kbd("F5").size(Size::Big).tooltip(tr("이전 슬라이드 (F5)"))).clicked() { acts.push(Act::Step(-1)) }
                if ui.add(Btn::new(tr("다음")).icon_end("skip-forward").kbd("F6").size(Size::Big).tooltip(tr("다음 슬라이드 (F6)"))).clicked() { acts.push(Act::Step(1)) }
                if ui.add(Btn::new(tr("송출")).icon("play").kbd("F9").kind(Kind::Send).size(Size::Big).tooltip(tr("미리보기를 송출 (F9)"))).clicked() { acts.push(Act::SendPreview) }
                }); });
                crate::widgets::group_frame().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0; // .clear-group { gap: 4px }
                        let hidden = s.live.as_ref().is_some_and(|l| !l.visible);
                        // 숨긴 동안은 .clear-group button.active (노랑)
                        let b = Btn::new(if hidden { tr("글자 보이기") } else { tr("글자 숨김") }).kbd("F10").size(Size::Tall).kind(if hidden { Kind::Warn } else { Kind::Default });
                        if ui.add(b.tooltip(tr("글자(슬라이드)만 숨기기·다시 보이기 (F10)"))).clicked() { acts.push(Act::ToggleClear) }
                        let more_on = s.hide_bg || s.logo;
                        let more = ui.add(Btn::new(tr("지우기 ▾").trim_end_matches(" ▾")).caret().size(Size::Tall).kind(if more_on { Kind::Warn } else { Kind::Default }).tooltip(tr("배경 지우기 · 로고 화면")));
                        egui::Popup::menu(&more).frame(crate::menubar::drop_frame()).gap(2.0).show(|ui| {
                            ui.spacing_mut().item_spacing.y = 0.0;
                            if crate::menubar::item_row(ui, Some("image"), &tr("배경 지우기"), "", s.hide_bg.then_some(true)).clicked() { acts.push(Act::ToggleBg); ui.close(); }
                            if crate::menubar::item_row(ui, Some("flag"), &tr("로고 화면"), "", s.logo.then_some(true)).clicked() { acts.push(Act::ToggleLogo); ui.close(); }
                        });
                        if ui.add(Btn::new(tr("모두 지우기")).kind(Kind::Danger).size(Size::Tall).tooltip(tr("글자·배경·로고를 모두 지움"))).clicked() { acts.push(Act::ClearAll) }
                    });
                });
            }

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
/// 머리 줄 · 아래 송출 줄 · 상태 · 알림 (Snap 의 private 필드를 읽도록 자식 모듈)
#[path = "topbar_ui.rs"]
mod topbar_ui;
/// 두 모니터의 보기 전환은 실제 송출 설정을 변경하지 않는다.
#[derive(Default)]
struct MonitorViews { preview_obs: bool, live_obs: bool }
impl MonitorViews {
    fn obs(&self, live: bool) -> bool { if live { self.live_obs } else { self.preview_obs } }
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
        v.preview_obs = true;
        assert!(v.obs(false) && !v.obs(true));
        v.live_obs = true; v.preview_obs = false;
        assert!(!v.obs(false) && v.obs(true));
    }
    #[test]
    fn editor_reveals_inactive_collapsed_main_tab() {
        use wp_core::workspace::{Node, Pane};
        let mut n = Node::Tabs { panes: vec![Pane::Cue, Pane::Main], active: Pane::Cue, collapsed: true };
        reveal_main(&mut n);
        assert!(n.rendered(Pane::Main));
    }
}
