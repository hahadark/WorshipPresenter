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
    last_flush: Instant,
    pub(crate) focus_search: bool,
    pub(crate) tb: crate::trackb_ui::TbState, // [트랙 B]
    /// 점검용: --search 글 · --send · --shot 파일 (몇 초 뒤 화면을 그림 파일로 저장하고 끝냄)
    test: Option<(Instant, Option<String>, bool, Option<String>, u8)>,
    pub(crate) tc: crate::track_c::TrackC, // [트랙 C]
}

pub(crate) struct Snap {
    preview: Option<Deck>,
    live: Option<Live>,
    cue_name: String,
    cue_id: String,
    items: Vec<Value>,
    cues: Vec<CueSummary>,
    templates: Vec<(String, String)>,
    tpl_main: Value,
    tpl_overlay: Value,
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
        Self { core, web_dir, data_dir, search: String::new(), result: Value::Null, versions, sel: None, edit: None, out, monitor, last_flush: Instant::now(), focus_search: true, test: test_args(), tb: Default::default() /* [트랙 B] */, tc: Default::default() /* [트랙 C] */ }
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
                Act::PreviewItem(i) => c.preview_item(i),
                Act::SendItem(i) => {
                    c.preview_item(i);
                    if c.pres.preview.as_ref().is_some_and(|p| p.passage["cueIndex"] == json!(i)) {
                        c.send_preview();
                    }
                }
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
                    let in_preview = c.pres.preview.as_ref().is_some_and(|p| p.passage["cueIndex"] == json!(i));
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

    fn cue_panel(&mut self, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
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
            self.editor(ui, i, &s.items[i], acts);
        }
    }

    fn editor(&mut self, ui: &mut egui::Ui, i: usize, it: &Value, acts: &mut Vec<Act>) {
        let key = it["id"].as_str().unwrap_or("").to_string();
        if self.edit.as_ref().map(|e| &e.0) != Some(&key) {
            self.edit = Some((key.clone(), it.clone()));
        }
        let cams = if it["type"] == "camera" { crate::parity_ui::known_cameras(&self.core.lock()) } else { vec![] }; // [트랙 A]
        let Some((_, e)) = &mut self.edit else { return };
        let mut changed = false;
        let mut field = |ui: &mut egui::Ui, e: &mut Value, k: &str, multi: bool, rows: usize, hint: &str| {
            let mut sv = e[k].as_str().unwrap_or("").to_string();
            let r = if multi {
                ui.add(egui::TextEdit::multiline(&mut sv).desired_rows(rows).desired_width(f32::INFINITY).hint_text(wp_core::i18n::tr(hint)))
            } else {
                ui.add(egui::TextEdit::singleline(&mut sv).desired_width(f32::INFINITY).hint_text(wp_core::i18n::tr(hint)))
            };
            if r.changed() {
                e[k] = json!(sv);
                true
            } else {
                false
            }
        };
        egui::ScrollArea::vertical().id_salt("edit").auto_shrink([false, false]).show(ui, |ui| {
            ui.label(RichText::new(wp_core::i18n::tr("제목")).small());
            changed |= field(ui, e, "title", false, 1, "순서 이름");
            match it["type"].as_str().unwrap_or("") {
                "bible" => {
                    ui.label(RichText::new(wp_core::i18n::tr("장절")).small());
                    changed |= field(ui, e, "ref", false, 1, "예: 요3:16-18, 시 23");
                }
                "song" => {
                    ui.label(RichText::new(wp_core::i18n::tr("가사 (빈 줄로 슬라이드 나눔, [1절] [후렴] 은 이름표)")).small());
                    changed |= field(ui, e, "lyrics", true, 12, "[1절]\n가사 첫 줄\n가사 둘째 줄\n\n[후렴]\n…");
                    ui.label(RichText::new(wp_core::i18n::tr("부를 순서 (예: 1 C 2 C)")).small());
                    changed |= field(ui, e, "order", false, 1, "비우면 가사 순서대로");
                    let mut st = e["showTitle"].as_bool().unwrap_or(false);
                    if ui.checkbox(&mut st, wp_core::i18n::tr("곡 제목 보이기")).changed() { e["showTitle"] = json!(st); changed = true; }
                }
                "text" => {
                    ui.label(RichText::new(wp_core::i18n::tr("본문 ('---' 줄로 장 나눔, '인도자:' '회중:' 교독 표시)")).small());
                    changed |= field(ui, e, "body", true, 12, "");
                }
                "title" => {
                    ui.label(RichText::new(wp_core::i18n::tr("부제")).small());
                    changed |= field(ui, e, "subtitle", false, 1, "");
                    ui.label(RichText::new(wp_core::i18n::tr("더 넣을 장 ('---' 로 나눔)")).small());
                    changed |= field(ui, e, "more", true, 4, "");
                }
                "blank" => {
                    let mut m = e["mode"].as_str().unwrap_or("black").to_string();
                    ui.horizontal(|ui| {
                        if ui.radio_value(&mut m, "black".to_string(), wp_core::i18n::tr("검은 화면")).changed() | ui.radio_value(&mut m, "background".to_string(), wp_core::i18n::tr("배경만")).changed() {
                            e["mode"] = json!(m);
                            changed = true;
                        }
                    });
                }
                "image" => {
                    ui.horizontal(|ui| {
                        if ui.button(wp_core::i18n::tr("그림 고르기…")).clicked() {
                            if let Some(p) = rfd::FileDialog::new().add_filter("그림", &["png", "jpg", "jpeg", "webp", "gif", "bmp"]).pick_file() {
                                let enc = percent_encoding::utf8_percent_encode(&p.to_string_lossy(), percent_encoding::NON_ALPHANUMERIC).to_string();
                                e["image"] = json!(format!("/lib/file?p={enc}"));
                                if e["title"].as_str().is_none_or(|t| t.is_empty() || t == "그림") {
                                    e["title"] = json!(p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default());
                                }
                                changed = true;
                            }
                        }
                        ui.label(RichText::new(wp_core::i18n::tr("등록한 그림 폴더 안의 그림만 예배당 화면에 보입니다")).small().color(Color32::GRAY));
                    });
                }
                "hymn" => {
                    changed |= crate::hymns_ui::editor(ui, e); // [트랙 A]
                }
                _ => {
                    changed |= crate::parity_ui::editor(ui, e, &cams); // [트랙 A] 카메라 · 슬라이드쇼 · 영상
                }
            }
            ui.label(RichText::new(wp_core::i18n::tr("메모 (무대 화면에 보임)")).small());
            changed |= field(ui, e, "note", false, 1, "");
            if it["type"] != "audio" { changed |= crate::flow_ui::item_fields(ui, e); } // [트랙 C] 자동 넘김·글자 숨김
            changed |= crate::theme_ui::item_templates(ui, e, &self.core); // [트랙 C] 순서별 템플릿·테마
        });
        if changed {
            acts.push(Act::UpdateItem(i, e.clone()));
        }
    }

    fn search_panel(&mut self, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
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
        let r = &self.result;
        egui::ScrollArea::vertical().id_salt("results").auto_shrink([false, false]).show(ui, |ui| {
            if r.is_null() {
                ui.label(RichText::new(wp_core::i18n::tr("장절을 쓰면 바로 미리보기에 뜨고, 낱말을 쓰면 본문에서 찾습니다.")).color(Color32::GRAY));
                return;
            }
            if r["ok"] != json!(true) {
                ui.label(RichText::new(r["error"].as_str().unwrap_or("")).color(Color32::from_rgb(0xff, 0xb0, 0x4a)));
                for c in r["candidates"].as_array().into_iter().flatten() {
                    let name = c["name"].as_str().unwrap_or("").to_string();
                    if ui.button(&name).clicked() {
                        self.search = format!("{} {}", c["short"].as_str().unwrap_or(&name), r["rest"].as_str().unwrap_or(""));
                        acts.push(Act::Search(self.search.clone()));
                    }
                }
                return;
            }
            if r["mode"] == "reference" {
                let p = &r["passage"];
                ui.label(RichText::new(format!("{} · {}", p["title"].as_str().unwrap_or(""), p["versionName"].as_str().unwrap_or(""))).strong().color(Color32::from_rgb(0xff, 0xd4, 0x79)));
                for v in p["verses"].as_array().into_iter().flatten() {
                    let (c, n) = (v["chapter"].as_i64().unwrap_or(0), v["verse"].as_i64().unwrap_or(0));
                    let resp = ui.add(egui::Label::new(RichText::new(format!("{}:{}  {}", c, n, v["text"].as_str().unwrap_or("")))).sense(Sense::click()).wrap());
                    crate::parity_ui::verse_menu(&resp, &format!("{} {}:{}", p["book"]["name"].as_str().unwrap_or(""), c, n), v["text"].as_str().unwrap_or("")); // [트랙 A]
                    if resp.clicked() { acts.push(Act::PreviewPassage(p.clone(), self.search.clone(), Some((c, n)))) }
                    if resp.double_clicked() { acts.push(Act::PreviewPassage(p.clone(), self.search.clone(), Some((c, n)))); acts.push(Act::SendPreview) }
                }
            } else {
                let rows = r["results"].as_array().cloned().unwrap_or_default();
                ui.label(RichText::new(wp_core::i18n::tr_args("'{1}' {2}건{3}", &[&(self.search.trim()), &(rows.len()), &(if r["truncated"] == json!(true) { wp_core::i18n::tr_s(" (더 있음)") } else { "" })])).strong());
                for v in rows {
                    let q = format!("{}{}:{}", v["bookShort"].as_str().unwrap_or(""), v["chapter"], v["verse"]);
                    let resp = ui.add(egui::Label::new(RichText::new(format!("{}  {}", v["ref"].as_str().unwrap_or(""), v["text"].as_str().unwrap_or("")))).sense(Sense::click()).wrap());
                    crate::parity_ui::verse_menu(&resp, v["ref"].as_str().unwrap_or(""), v["text"].as_str().unwrap_or("")); // [트랙 A]
                    if resp.clicked() { acts.push(Act::PreviewRef(q.clone(), None)) }
                    if resp.double_clicked() { acts.push(Act::SendRef(q, None)) }
                }
            }
        });
    }

    fn deck_view(&self, ui: &mut egui::Ui, title: &str, deck: Option<&Deck>, tpl: &Value, live: Option<&Live>, s: &Snap, acts: &mut Vec<Act>) {
        let c = paint::Ctx { web_dir: &self.web_dir, data_dir: &self.data_dir };
        let is_live = live.is_some();
        ui.horizontal(|ui| {
            let col = if is_live { Color32::from_rgb(0xff, 0x6b, 0x6b) } else { Color32::from_rgb(0x8f, 0xb8, 0xff) };
            ui.label(RichText::new(wp_core::i18n::tr(title)).strong().color(col));
            if let Some(d) = deck {
                let t = d.passage["itemTitle"].as_str().filter(|x| !x.is_empty()).or(d.passage["title"].as_str()).unwrap_or("");
                ui.label(RichText::new(format!("{}  [{}/{}]", t, d.index + 1, d.slides.len())).color(Color32::LIGHT_GRAY));
            }
            if let Some(l) = live {
                if !l.visible { ui.label(RichText::new(wp_core::i18n::tr("글자 숨김")).small().color(Color32::from_rgb(0xff, 0xb0, 0x4a))); }
            }
        });
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
        egui::Panel::left("cue").default_size(380.0).min_size(280.0).show(ui, |ui| {
            ui.add_space(4.0);
            // [트랙 A] 왼쪽 탭: 큐시트 | 찬송가 | 찬양 | 라이브러리 (찬양·라이브러리 자리는 트랙 B 가 채움)
            crate::hymns_ui::global(self, ui, &mut acts);
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
                1 => crate::hymns_ui::tab(self, ui, &mut acts),
                2 => self.songs_panel(ui), // [트랙 B]
                3 => self.library_panel(ui), // [트랙 B]
                _ => self.cue_panel(ui, &s, &mut acts),
            }
        });
        egui::Panel::top("decks").resizable(false).exact_size((ui.available_height() * 0.6).max(320.0)).show(ui, |ui| {
            ui.add_space(4.0);
            ui.columns(2, |cols| {
                self.deck_view(&mut cols[0], "미리보기", s.preview.as_ref(), &s.tpl_main, None, &s, &mut acts);
                let live_deck = s.live.as_ref().map(|l| &l.deck);
                self.deck_view(&mut cols[1], "LIVE (예배당)", live_deck, &s.tpl_main, s.live.as_ref(), &s, &mut acts);
            });
            ui.separator();
            ui.vertical_centered(|ui| {
            ui.horizontal(|ui| {
                let big = |t: &str| egui::Button::new(RichText::new(wp_core::i18n::tr(t)).size(15.0)).min_size(Vec2::new(78.0, 30.0));
                if ui.add(big("◀ 이전 F5")).clicked() { acts.push(Act::Step(-1)) }
                if ui.add(big("송출 F9").fill(Color32::from_rgb(0xb0, 0x2a, 0x2a))).clicked() { acts.push(Act::SendPreview) }
                if ui.add(big("다음 F6 ▶")).clicked() { acts.push(Act::Step(1)) }
                ui.separator();
                let hidden = s.live.as_ref().is_some_and(|l| !l.visible);
                if ui.add(big(if hidden { wp_core::i18n::tr_s("글자 보이기 F10") } else { wp_core::i18n::tr_s("글자 숨김 F10") }).selected(hidden)).clicked() { acts.push(Act::ToggleClear) }
                if ui.add(big("배경 끄기").selected(s.hide_bg)).clicked() { acts.push(Act::ToggleBg) }
                if ui.add(big("로고").selected(s.logo)).clicked() { acts.push(Act::ToggleLogo) }
                if ui.add(big("모두 지우기")).clicked() { acts.push(Act::ClearAll) }
            });
            });
            self.video_bar(ui); // [트랙 B] 영상 조작 줄
        });
        egui::CentralPanel::default_margins().show(ui, |ui| {
            self.search_panel(ui, &s, &mut acts);
        });
        self.trackb_windows(&ctx); // [트랙 B]
        let _ = &s.tpl_overlay;
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

impl Drop for App {
    fn drop(&mut self) {
        self.out.close();
        wp_core::sync::on_exit(&mut self.core.lock()); // [트랙 B] 끝날 때 동기화 파일 쓰기
        self.core.lock().flush();
    }
}
