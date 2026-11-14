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

fn item_sub(it: &Value) -> String {
    match it["type"].as_str().unwrap_or("") {
        "bible" => it["ref"].as_str().unwrap_or("").to_string(),
        "hymn" => {
            let n = it["number"].as_i64().unwrap_or(0);
            if n > 0 { format!("{}장", n) } else { it["file"].as_str().map(|f| f.rsplit(['\\', '/']).next().unwrap_or("").to_string()).unwrap_or_default() }
        }
        "song" => it["lyrics"].as_str().unwrap_or("").lines().find(|l| !l.trim().is_empty() && !l.trim().starts_with('[')).unwrap_or("").chars().take(24).collect(),
        "text" => it["body"].as_str().unwrap_or("").lines().next().unwrap_or("").chars().take(24).collect(),
        "title" => it["subtitle"].as_str().unwrap_or("").to_string(),
        _ => String::new(),
    }
}

impl App {
    pub fn new(core: Shared, web_dir: std::path::PathBuf, data_dir: std::path::PathBuf) -> Self {
        let versions = core.lock().store.list_versions();
        let out = OutWin::new();
        let saved = core.lock().settings.get()["rsOutputMonitor"].as_u64().map(|x| x as usize);
        let monitor = saved.filter(|m| out.monitors.iter().any(|x| x.index == *m)).unwrap_or_else(|| out.default_monitor());
        let workspace = crate::workspace_ui::WorkspaceUi::new(core.lock().settings.get());
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
    fn top_bar(&mut self, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(wp_core::i18n::tr("워십프리젠터")).strong().size(16.0));
            ui.label(RichText::new(wp_core::i18n::tr("Rust 판 미리보기")).small().color(Color32::from_rgb(0xff, 0xb0, 0x4a)));
            ui.separator();
            ui.label(wp_core::i18n::tr("번역본"));
            let vname = self.versions.iter().find(|v| v.id == s.version).map(|v| v.name.clone()).unwrap_or_default();
            egui::ComboBox::from_id_salt("ver").selected_text(vname).width(120.0).show_ui(ui, |ui| {
                for v in &self.versions {
                    if ui.selectable_label(v.id == s.version, &v.name).clicked() {
                        acts.push(Act::SetVersion(v.id.clone()));
                    }
                }
            });
            ui.separator();
            for (slot, label, active) in [("main", "예배당", &s.active_main), ("overlay", "OBS 자막", &s.active_overlay)] {
                ui.label(wp_core::i18n::tr(label));
                let name = s.templates.iter().find(|t| &t.0 == active).map(|t| t.1.clone()).unwrap_or_default();
                egui::ComboBox::from_id_salt(slot).selected_text(name).width(170.0).show_ui(ui, |ui| {
                    for (id, n) in &s.templates {
                        if ui.selectable_label(id == active, n).clicked() {
                            acts.push(Act::SetTemplate(slot, id.clone()));
                        }
                    }
                });
            }
            ui.separator();
            if ui.selectable_label(s.preview_mode, wp_core::i18n::tr("미리보기 거쳐 송출")).on_hover_text(wp_core::i18n::tr("끄면 누른 본문·순서를 바로 송출합니다")).clicked() {
                acts.push(Act::TogglePreviewMode);
            }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let running = self.out.running();
                let btn = egui::Button::new(if running { wp_core::i18n::tr_s("예배당 화면 끄기") } else { wp_core::i18n::tr_s("예배당 화면 켜기") }).fill(if running { Color32::from_rgb(0x2e, 0x7d, 0x4f) } else { Color32::from_rgb(0x33, 0x38, 0x44) });
                if ui.add(btn).on_hover_text("Ctrl+Shift+F").clicked() {
                    acts.push(Act::OutputToggle);
                }
                let mname = self.out.monitors.iter().find(|m| m.index == self.monitor).map(|m| format!("{} {}×{}{}", m.name, m.size.0, m.size.1, if m.primary { wp_core::i18n::tr_s(" (주)") } else { "" })).unwrap_or_else(|| "모니터".into());
                egui::ComboBox::from_id_salt("mon").selected_text(mname).show_ui(ui, |ui| {
                    for m in self.out.monitors.clone() {
                        let t = format!("{} {}×{}{}", m.name, m.size.0, m.size.1, if m.primary { wp_core::i18n::tr_s(" (주 모니터)") } else { "" });
                        if ui.selectable_label(m.index == self.monitor, t).clicked() {
                            acts.push(Act::SetMonitor(m.index));
                        }
                    }
                });
                ui.menu_button(wp_core::i18n::tr("OBS 주소"), |ui| {
                    for (label, path) in [("예배당 화면 그대로", "/outputs/main"), ("투명 자막", "/outputs/overlay"), ("무대 화면", "/outputs/stage")] {
                        let url = format!("http://127.0.0.1:{}{}", s.port, path);
                        if ui.button(wp_core::i18n::tr_args("{1}: {2}  (복사)", &[&label, &url])).clicked() {
                            ui.ctx().copy_text(url);
                        }
                    }
                });
            });
        });
    }

    pub(crate) fn cue_panel(&mut self, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(wp_core::i18n::tr("큐시트")).strong());
            egui::ComboBox::from_id_salt("cue").selected_text(if s.cue_name.is_empty() { "(없음)".to_string() } else { s.cue_name.clone() }).width(ui.available_width() - 70.0).show_ui(ui, |ui| {
                for c in &s.cues {
                    let t = format!("{}{}  ·  {} · {}개", if c.pinned { "📌 " } else { "" }, c.name, c.service_date, c.count);
                    if ui.selectable_label(c.id == s.cue_id, t).clicked() {
                        acts.push(Act::OpenCue(c.id.clone()));
                    }
                }
            });
            if ui.button(wp_core::i18n::tr("새로")).clicked() {
                acts.push(Act::NewCue);
            }
        });
        ui.add_space(4.0);
        let live_idx = s.live.as_ref().filter(|l| l.deck.passage["cueId"] == json!(s.cue_id)).and_then(|l| l.deck.passage["cueIndex"].as_i64());
        let pv_idx = s.preview.as_ref().filter(|p| p.passage["cueId"] == json!(s.cue_id)).and_then(|p| p.passage["cueIndex"].as_i64());
        let list_h = (ui.available_height() * 0.52).max(160.0);
        egui::ScrollArea::vertical().id_salt("items").max_height(list_h).auto_shrink([false, false]).show(ui, |ui| {
            for (i, it) in s.items.iter().enumerate() {
                let (label, col) = type_info(it["type"].as_str().unwrap_or(""));
                let selected = self.sel == Some(i);
                let is_live = live_idx == Some(i as i64);
                let is_pv = pv_idx == Some(i as i64);
                let fill = if is_live { Color32::from_rgb(0x5a, 0x1f, 0x24) } else if selected { Color32::from_rgb(0x26, 0x33, 0x4a) } else { Color32::TRANSPARENT };
                let resp = egui::Frame::new().fill(fill).corner_radius(6.0).inner_margin(egui::Margin::symmetric(6, 4)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{:>2}", i + 1)).monospace().color(Color32::GRAY));
                        egui::Frame::new().fill(col.gamma_multiply(0.25)).corner_radius(4.0).inner_margin(egui::Margin::symmetric(5, 1)).show(ui, |ui| {
                            ui.label(RichText::new(wp_core::i18n::tr(label)).small().color(col));
                        });
                        ui.label(RichText::new(it["title"].as_str().unwrap_or("")).strong());
                        let sub = item_sub(it);
                        if !sub.is_empty() {
                            ui.label(RichText::new(sub).small().color(Color32::GRAY));
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if is_live { ui.label(RichText::new("LIVE").small().strong().color(Color32::from_rgb(0xff, 0x6b, 0x6b))); }
                            else if is_pv { ui.label(RichText::new(wp_core::i18n::tr("미리보기")).small().color(Color32::from_rgb(0x8f, 0xb8, 0xff))); }
                        });
                    });
                });
                let r = resp.response.interact(Sense::click_and_drag()); // [트랙 B] 끌어서 옮기기
                self.cue_row_dnd(ui, &r, i); // [트랙 B]
                crate::parity_ui::cue_row_menu(self, &r, i, it, s.items.len(), acts); // [트랙 A] 오른쪽 클릭 메뉴
                if r.clicked() {
                    self.sel = Some(i);
                    self.edit = None;
                    acts.push(Act::PreviewItem(i));
                }
                if r.double_clicked() {
                    acts.push(Act::SendItem(i));
                }
            }
            if s.items.is_empty() {
                ui.label(RichText::new(wp_core::i18n::tr("아래 단추로 예배 순서를 넣으세요.")).color(Color32::GRAY));
            }
        });
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            ui.label(wp_core::i18n::tr("넣기:"));
            for ty in ["bible", "song", "text", "title", "blank", "image"] {
                let (label, col) = type_info(ty);
                if ui.add(egui::Button::new(RichText::new(wp_core::i18n::tr(label)).color(col))).clicked() {
                    acts.push(Act::AddItem(ty, None));
                }
            }
            crate::parity_ui::add_buttons(ui, acts); // [트랙 A] 카메라 · 슬라이드쇼
        });
        if let Some(i) = self.sel.filter(|i| *i < s.items.len()) {
            ui.horizontal(|ui| {
                if ui.button("▲").clicked() { acts.push(Act::MoveItem(i, -1)) }
                if ui.button("▼").clicked() { acts.push(Act::MoveItem(i, 1)) }
                if ui.button(wp_core::i18n::tr("송출")).clicked() { acts.push(Act::SendItem(i)) }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button(RichText::new(wp_core::i18n::tr("지우기")).color(Color32::from_rgb(0xff, 0x8a, 0x8a))).clicked() { acts.push(Act::RemoveItem(i)) }
                });
            });
            crate::hymns_ui::ppt_box(self, ui, i, &s.items[i]); // [트랙 A]
            if ui.button(wp_core::i18n::tr("편집칸 고치기 (F2)")).clicked() { self.show_item_editor(i); }
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
        // 원본은 검색칸이 맨 위 도구줄에 있어 편집칸을 켜도 늘 보인다 → 검색칸을 먼저 그린다.
        self.search_bar(ui, s, acts);
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

    pub(crate) fn search_panel(&mut self, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
        self.search_bar(ui, s, acts);
        self.search_results(ui, s, acts);
    }

    /// 성경 검색칸 줄 (입력·송출·큐시트에 넣기)
    pub(crate) fn search_bar(&mut self, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
        ui.horizontal(|ui| {
            let id = egui::Id::new("search-box");
            let r = ui.add(egui::TextEdit::singleline(&mut self.search).id(id).hint_text(wp_core::i18n::tr("장절(요3:16, 시 23) 또는 낱말 — Ctrl+K")).desired_width(ui.available_width() - 190.0).font(egui::TextStyle::Heading));
            if self.focus_search {
                r.request_focus();
                self.focus_search = false;
            }
            if r.changed() {
                acts.push(Act::Search(self.search.clone()));
            }
            if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                if ui.input(|i| i.modifiers.ctrl) { acts.push(Act::SendPreview) }
                else if self.result["mode"] == "reference" && self.result["ok"] == json!(true) && !s.preview_mode { acts.push(Act::SendPreview) }
            }
            if ui.button(wp_core::i18n::tr("송출 (Ctrl+Enter)")).clicked() { acts.push(Act::SendPreview) }
            if ui.button(wp_core::i18n::tr("큐시트에 넣기")).clicked() && self.result["mode"] == "reference" && self.result["ok"] == json!(true) {
                let title = self.result["passage"]["title"].clone();
                acts.push(Act::AddItem("bible", Some(json!({ "title": "성경 봉독", "ref": self.search.trim(), "subtitle": title }))));
            }
        });
    }

    /// 성경 검색 결과 (장절 본문 또는 낱말 찾기 목록) — 모양은 center_ui.rs (control.css .results .row)
    pub(crate) fn search_results(&mut self, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
        self.center_results(ui, s, acts);
    }

    pub(crate) fn deck_view(&mut self, ui: &mut egui::Ui, title: &str, deck: Option<&Deck>, is_live: bool, live: Option<&Live>, s: &Snap, acts: &mut Vec<Act>) {

        ui.horizontal(|ui| {
            let col = if is_live { Color32::from_rgb(0xff, 0x6b, 0x6b) } else { Color32::from_rgb(0x8f, 0xb8, 0xff) };
            ui.label(RichText::new(wp_core::i18n::tr(title)).strong().color(col));
            let view = if is_live { &mut self.views.live_obs } else { &mut self.views.preview_obs };
            ui.selectable_value(view, false, wp_core::i18n::tr("예배당"));
            ui.selectable_value(view, true, "OBS");
            if let Some(d) = deck {
                let t = d.passage["itemTitle"].as_str().filter(|x| !x.is_empty()).or(d.passage["title"].as_str()).unwrap_or("");
                ui.label(RichText::new(format!("{}  [{}/{}]", t, d.index + 1, d.slides.len())).color(Color32::LIGHT_GRAY));
            }
            if let Some(l) = live {
                if !l.visible { ui.label(RichText::new(wp_core::i18n::tr("글자 숨김")).small().color(Color32::from_rgb(0xff, 0xb0, 0x4a))); }
            }
        });
        let tpl = match (is_live, self.views.obs(is_live)) {
            (true, false) => &s.tpl_main, (true, true) => &s.tpl_overlay,
            (false, false) => &s.preview_main, (false, true) => &s.preview_overlay,
        };
        let c = paint::Ctx { web_dir: &self.web_dir, data_dir: &self.data_dir };
        let w = ui.available_width();
        let tw = tpl["width"].as_f64().unwrap_or(1920.0) as f32;
        let th = tpl["height"].as_f64().unwrap_or(1080.0) as f32;
        let h = (w * th / tw).min(ui.available_height() - 150.0).max(60.0);
        let w2 = h * tw / th;
        let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
        let rect = egui::Rect::from_center_size(rect.center(), Vec2::new(w2, h));
        let slide = deck.and_then(|d| d.slides.get(d.index));
        let version = deck.map(|d| d.passage["versionName"].as_str().unwrap_or("").to_string()).unwrap_or_default();
        let (visible, logo, hide_bg) = match live { Some(l) => (l.visible, s.logo, s.hide_bg), None => (true, false, false) };
        paint_slide(ui, &c, rect, slide, tpl, &version, visible, logo, hide_bg);
        ui.painter().rect_stroke(rect, 2.0, egui::Stroke::new(if is_live { 2.0 } else { 1.0 }, if is_live { Color32::from_rgb(0xc0, 0x39, 0x2b) } else { Color32::from_gray(70) }), egui::StrokeKind::Outside);
        ui.add_space(4.0);
        if let Some(d) = deck {
            egui::ScrollArea::vertical().id_salt(format!("{title}-slides")).max_height(64.0).auto_shrink([false, true]).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (k, sl) in d.slides.iter().enumerate() {
                        let lb = sl["label"].as_str().filter(|x| !x.is_empty()).map(|x| format!("¶{x}")).or_else(|| sl["ref"].as_str().map(|r| r.rsplit(' ').next().unwrap_or(r).to_string())).unwrap_or_else(|| (k + 1).to_string());
                        let b = egui::Button::new(RichText::new(lb).small()).selected(k == d.index);
                        let r = ui.add(b);
                        if r.clicked() { acts.push(if is_live { Act::LiveGoto(k) } else { Act::PreviewIndex(k) }) }
                        if r.double_clicked() && !is_live { acts.push(Act::SendPreview) }
                        crate::parity2_ui::slide_menu(&self.core, &r, d, k, is_live, acts); // [트랙 A] 썸네일 오른쪽 클릭
                    }
                });
            });
        }
    }
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

        crate::track_c::menu_bar(self, ui); // [트랙 C] 메뉴 줄 (트랙 B·D 항목도 여기서 연다)
        crate::track_c::windows(self, &ctx, &mut acts); // [트랙 C] 설정·템플릿·기록 창
        let s = self.snap(); // [트랙 C] 창에서 바뀐 것 반영
        egui::Panel::top("top").show(ui, |ui| {
            ui.add_space(4.0);
            self.top_bar(ui, &s, &mut acts);
            ui.add_space(4.0);
        });
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                let n = |c: &str| s.outputs.iter().filter(|o| o.0 == c).count();
                ui.label(RichText::new(wp_core::i18n::tr_args("출력 연결  예배당 {1} · OBS 자막 {2} · 무대 {3}", &[&(n("main")), &(n("overlay")), &(n("stage"))])).small());
                ui.separator();
                ui.label(RichText::new(format!("http://127.0.0.1:{}", s.port)).small().color(Color32::GRAY));
                // [트랙 D] OBS 연결/방송/녹화/끊김 (설정에서 켰을 때만, 캐시는 obs 폴러가 채움)
                if let Some((t, warn)) = crate::trackd_ui::obs_status_line(self.core.lock().settings.get()) { ui.separator(); ui.label(RichText::new(t).small().color(if warn { Color32::from_rgb(0xff, 0x8a, 0x8a) } else { Color32::from_rgb(0x8a, 0xe0, 0xa8) })); }
                if !self.out.error.is_empty() {
                    ui.separator();
                    ui.label(RichText::new(&self.out.error).small().color(Color32::from_rgb(0xff, 0x8a, 0x8a)));
                }
                if let Some((msg, kind, at)) = &s.toast {
                    if wp_core::util::now_ms() - at < 8000 {
                        ui.separator();
                        let col = match *kind { "error" => Color32::from_rgb(0xff, 0x8a, 0x8a), "warn" => Color32::from_rgb(0xff, 0xb0, 0x4a), _ => Color32::from_rgb(0x8a, 0xe0, 0xa8) };
                        ui.label(RichText::new(msg).small().color(col));
                    }
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new(wp_core::i18n::tr("F5 이전 · F6 다음 · F9 송출 · F10 숨김 · Ctrl+↑↓ 순서")).small().color(Color32::GRAY));
                });
            });
        });
        if std::mem::take(&mut self.editor_reveal) { self.reveal_editor_panel(); }
        self.workspace_bar(ui);
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
            ui.add_space(4.0);
            // [트랙 A] 왼쪽 탭: 큐시트 | 찬송가 | 찬양 | 라이브러리 (찬양·라이브러리 자리는 트랙 B 가 채움)
            crate::hymns_ui::global(self, ui, acts);
            let tab_id = egui::Id::new("left-tab");
            let mut tab: u8 = ui.data(|d| d.get_temp(tab_id)).unwrap_or_else(|| std::env::args().position(|x| x == "--left-tab").and_then(|i| std::env::args().nth(i + 1)).and_then(|x| x.parse().ok()).unwrap_or(0));
            ui.horizontal(|ui| {
                for (k, label) in [(0u8, "큐시트"), (1, "찬송가"), (2, "찬양"), (3, "라이브러리")] {
                    if ui.selectable_label(tab == k, RichText::new(wp_core::i18n::tr(label)).strong()).clicked() { tab = k; }
                }
            });
            ui.data_mut(|d| d.insert_temp(tab_id, tab));
            ui.separator();
            match tab {
                1 => crate::hymns_ui::tab(self, ui, acts),
                2 => self.songs_panel(ui), // [트랙 B]
                3 => self.library_panel(ui), // [트랙 B]
                _ => self.cue_panel(ui, s, acts),
            }

 }
 pub(crate) fn workspace_controls(&mut self,ui:&mut egui::Ui,s:&Snap,acts:&mut Vec<Act>){
            ui.separator();
            // 아래 송출 줄 (Electron footer.bottom): 이전 F5 · 다음 F6 · 송출 F9 | 글자 숨김 F10 · 지우기 ▾ · 모두 지우기
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                use crate::widgets::{Btn, Kind, Size};
                use wp_core::i18n::tr;
                ui.spacing_mut().item_spacing.x = 6.0; // .ft-group { gap: 6px }
                if ui.add(Btn::new(tr("이전")).icon("skip-back").kbd("F5").size(Size::Big).tooltip(tr("이전 슬라이드 (F5)"))).clicked() { acts.push(Act::Step(-1)) }
                if ui.add(Btn::new(tr("다음")).icon_end("skip-forward").kbd("F6").size(Size::Big).tooltip(tr("다음 슬라이드 (F6)"))).clicked() { acts.push(Act::Step(1)) }
                if ui.add(Btn::new(tr("송출")).icon("play").kbd("F9").kind(Kind::Send).size(Size::Big).tooltip(tr("미리보기를 송출 (F9)"))).clicked() { acts.push(Act::SendPreview) }
                ui.add_space(4.0); // .bottom { gap: 10px }
                crate::widgets::group_frame().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0; // .clear-group { gap: 4px }
                        let hidden = s.live.as_ref().is_some_and(|l| !l.visible);
                        // 숨긴 동안은 .clear-group button.active (노랑)
                        let b = Btn::new(if hidden { tr("글자 보이기") } else { tr("글자 숨김") }).kbd("F10").size(Size::Tall).kind(if hidden { Kind::Warn } else { Kind::Default });
                        if ui.add(b.tooltip(tr("글자(슬라이드)만 숨기기·다시 보이기 (F10)"))).clicked() { acts.push(Act::ToggleClear) }
                        let more_on = s.hide_bg || s.logo;
                        let more = ui.add(Btn::new(tr("지우기 ▾").trim_end_matches(" ▾")).caret().size(Size::Tall).kind(if more_on { Kind::Warn } else { Kind::Default }).tooltip(tr("배경 지우기 · 로고 화면")));
                        egui::Popup::menu(&more).show(|ui| {
                            ui.set_min_width(160.0);
                            if ui.add(egui::Button::selectable(s.hide_bg, tr("배경 지우기"))).clicked() { acts.push(Act::ToggleBg); ui.close(); }
                            if ui.add(egui::Button::selectable(s.logo, tr("로고 화면"))).clicked() { acts.push(Act::ToggleLogo); ui.close(); }
                        });
                        if ui.add(Btn::new(tr("모두 지우기")).kind(Kind::Danger).size(Size::Tall).tooltip(tr("글자·배경·로고를 모두 지움"))).clicked() { acts.push(Act::ClearAll) }
                    });
                });
            });            self.video_bar(ui); // [트랙 B] 영상 조작 줄

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
