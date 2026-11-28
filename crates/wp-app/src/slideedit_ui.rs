//! [트랙 C] 슬라이드 편집기 (Ctrl+E): 고른 큐시트 순서의 슬라이드마다 글상자 위치·꾸미기·배경,
//! 덧붙인 글상자·그림·도형, 넣기·복제·지우기·옮기기·건너뛰기·여기서 새 순서로 나누기 (오른쪽 클릭 메뉴에도)

use crate::app::Act;
use crate::paint::{self, paint_slide};
use eframe::egui::{self, Color32, RichText, Sense, Vec2};
use serde_json::{json, Value};
use wp_core::core::Shared;
use wp_core::slideedit::{self as se, is_chunk};

#[path = "slideedit_canvas.rs"]
mod canvas;
use canvas::{EdKey, ElDrag};
#[cfg(test)]
#[path = "slideedit_canvas_tests.rs"]
mod canvas_tests;

#[derive(Default)]
pub struct SlideEditUi {
    pub open: bool,
    /// 보이는 슬라이드 번호
    k: usize,
    /// 공통('*') 편집 중
    common: bool,
    drag_slide: Option<usize>,
    /// 다음에 열 때 고를 슬라이드 (ekey) — 격자의 ✎·＋ 단추가 정한다
    pending_key: Option<String>,
    /// 여러 장 고르기 (slideedit.js S.multi · S.anchor): 고른 ekey 들, Shift 기준
    pub(crate) multi: Vec<String>,
    anchor: Option<String>,
    /// Ctrl+A: 다음 프레임에 건너뛰지 않은 슬라이드 모두 고르기
    want_all: bool,
    /// [갈래 C] 캔버스에서 글 바로 고치는 중 (slideedit.js inlineEdit — 상자를 두 번 누르면)
    inline: Option<Inline>,
    /// 글 바로 고치는 중에 Esc 를 눌렀다 (전역 Esc 보다 먼저 받아 둔다)
    inline_esc: bool,
    /// 캔버스에서 고른 요소 (slideedit.js S.sel): "" = 없음 · "main" = 본문 글상자 · 그 밖 = 덧붙인 요소 id
    sel: String,
    /// 고른 요소가 속한 (순서 id, 편집 키) — 다른 순서·슬라이드로 가면 고르기를 푼다
    sel_ctx: (String, String),
    /// 전역 단축키보다 먼저 받아 둔 편집기 글쇠 (pre_keys → show)
    keys: Vec<EdKey>,
    /// 지난 프레임에 캔버스를 그렸다 (그렸을 때만 글쇠를 가져간다)
    live: bool,
    /// 마지막으로 누른 곳이 편집기 창 안이다 (열 때도 켠다). 메인 창·다른 창을 누르면 꺼지고 편집기는 글쇠를 받지 않는다
    active: bool,
    /// 편집기 창의 층 (어디를 눌렀는지 가린다) · 지난 프레임에 열려 있었나
    win_layer: Option<egui::LayerId>,
    was_open: bool,
    /// 글 바로 고치는 글칸에 지난 프레임까지 초점이 있었다 (그때만 Esc 를 가져간다. egui 는 Esc 가 오면 프레임 첫머리에 초점을 지우므로 따로 적어 둔다)
    inline_focus: bool,
    /// 요소 끌기 (옮기기·크기 조절 손잡이)
    drag: Option<ElDrag>,
    /// 복사한 요소와 그때 클립보드에 쓴 글 (붙여 넣는 글이 이것과 같으면 요소를 붙인다)
    clip: Option<(Value, String)>,
    /// 다음 바꿈은 실행 취소 한 번으로 따로 (요소 지우기·복제·붙여 넣기·끌기 뒤)
    undo_break: bool,
    /// 목록에서 Shift 누르고 끌어 여러 장 고르기 (slideedit.js S.lasso): 끌기 시작한 슬라이드 ekey
    lasso: Option<String>,
    #[cfg(test)]
    pub(crate) observed: std::collections::BTreeMap<String, egui::Rect>,
}

/// 캔버스 위 글 바로 고치기 한 건
#[derive(Clone)]
struct Inline {
    item_id: String,
    /// 편집 키 ('*' 또는 슬라이드 ekey)
    key: String,
    /// "main" = 본문, 그 밖 = 덧붙인 글상자 id
    target: String,
    /// 본문이면 그 슬라이드 (글 덩어리 번호·덧붙인 카드 id 를 알아야 저장한다)
    slide: Value,
    text: String,
    orig: String,
    /// 0 = 막 열림(초점 주기) · 1 = 모두 고르기 · 2 = 치는 중
    step: u8,
}

impl Inline {
    /// 고친 글을 순서에 반영한다. 바뀐 것이 없으면 false
    fn commit(&self, it: &mut Value) -> Result<bool, &'static str> {
        if self.text == self.orig {
            return Ok(false);
        }
        if self.target == "main" {
            se::set_main_text(it, &self.slide, &self.text)?;
        } else {
            let (id, text) = (self.target.clone(), self.text.clone());
            se::update_edit(it, &self.key, |e| {
                if let Some(x) = e["items"].as_array_mut().and_then(|a| a.iter_mut().find(|x| x["id"].as_str() == Some(id.as_str()))) {
                    x["text"] = json!(text);
                }
            });
        }
        Ok(true)
    }
}

/// 글이 없는 슬라이드 종류 (slideedit.js NOTEXT): 본문 글 고치기가 없다
fn no_text_kind(slide: &Value) -> bool {
    matches!(slide["kind"].as_str().unwrap_or(""), "image" | "score" | "video" | "camera" | "blank" | "slideshow" | "audio")
}

fn edit_key(slide: &Value) -> String {
    slide["ekey"].as_str().unwrap_or("0").to_string()
}

/// 편집기 목록에 보일 슬라이드 (건너뛴 것 포함, 글 순서는 원래 글 덩어리 단위)와 그 템플릿.
/// 격자의 마우스 올림 단추도 같은 차례(ekey)로 동작하도록 함께 쓴다 (Electron slideedit.js sources).
pub(crate) fn editor_list(c: &mut wp_core::core::Core, i: usize) -> Result<Option<(Value, Vec<Value>, Value)>, String> {
    let Some(it) = c.cue.as_ref().and_then(|q| q.items.get(i)).cloned() else { return Ok(None) };
    let d = c.item_deck(i)?;
    let tpl = c.template_for_deck("main", Some(&d));
    let ty = it["type"].as_str().unwrap_or("");
    let slides = if matches!(ty, "song" | "text") {
        // 편집 목록은 출력에서 나뉜 화면이 아니라 원래 글 덩어리 단위다.
        let song = ty == "song";
        let chunks = if song { wp_core::slides::lyrics_chunks(it["lyrics"].as_str().unwrap_or("")) } else { wp_core::slides::text_chunks(it["body"].as_str().unwrap_or(""), if it["roleLabels"].as_bool().unwrap_or(false) { 2 } else { 1 }) };
        let raw = chunks.into_iter().map(|c| json!({"kind":"text","ref":it["title"],"label":c.label,"src":c.src,"parts":[{"text":c.text}]})).collect();
        se::decorate(&it, raw, true)
    } else if !is_chunk(ty) {
        // 건너뛴 슬라이드도 보이게: 꾸미기 전 슬라이드를 다시 꾸민다 (잠깐 바꿔 끼워 만들고 되돌린다)
        let mut bare = it.clone();
        if let Some(o) = bare.as_object_mut() { o.remove("skip"); o.remove("slideOrder"); o.remove("extra"); if it["type"] != "hymn" { o.remove("edits"); } }
        let orig = std::mem::replace(&mut c.cue.as_mut().unwrap().items[i], bare);
        let raw = c.item_deck(i);
        c.cue.as_mut().unwrap().items[i] = orig;
        raw.map(|d2| se::decorate(&it, d2.slides, true)).unwrap_or(d.slides)
    } else {
        d.slides
    };
    Ok(Some((it, slides, tpl)))
}

/// 편집기 목록 줄의 마우스 올림 단추 (Electron slideedit.js opsHtml: 글 순서·덧붙인 슬라이드는 위로·아래로·복제·지우기,
/// 그 밖 원래 슬라이드는 위로·아래로(성경 빼고)·건너뛰기/되살리기)
fn list_ops(ty: &str, s: &Value) -> Vec<crate::widgets::HoverOp> {
    use crate::widgets::HoverOp;
    use wp_core::i18n::tr;
    let mut v = vec![];
    let free = is_chunk(ty) || s["xid"].is_string();
    if free || ty != "bible" {
        v.push(HoverOp::new("up", "arrow-up", tr("위로 (차례 바꾸기)")));
        v.push(HoverOp::new("down", "arrow-down", tr("아래로 (차례 바꾸기)")));
    }
    if free {
        v.push(HoverOp::new("dup", "copy", tr("복제 (꾸민 것도)")));
        v.push(HoverOp::new("del", "trash-2", tr("이 슬라이드 지우기 (Ctrl+Z 로 되돌리기)")).danger());
    } else if s["skipped"].as_bool().unwrap_or(false) {
        v.push(HoverOp::new("skip", "eye", tr("다시 보이기 (송출에 넣기)")));
    } else {
        v.push(HoverOp::new("skip", "eye-off", tr("건너뛰기 (송출에서 빼기, 원래 내용은 그대로)")));
    }
    // 오른쪽 끝부터 놓으므로 거꾸로 (왼쪽→오른쪽: 위로 · 아래로 · 복제 · 지우기)
    v.into_iter().rev().map(|o| o.right()).collect()
}

impl SlideEditUi {
    /// 편집기를 열고 그 슬라이드(ekey)를 고른다 (격자 ✎ · ＋)
    pub fn open_at(&mut self, key: &str) {
        self.open = true;
        self.active = true;
        self.common = false;
        self.pending_key = Some(key.to_string());
        self.multi.clear();
        self.anchor = None;
    }
    /// 고른 여러 장을 함께 꾸미기로 열기 (본문 목록·격자의 '고른 N장 편집', slideedit.js open(i, key, keys))
    pub fn open_many(&mut self, keys: &[String]) {
        let Some(first) = keys.first() else { return };
        self.open_at(first);
        if keys.len() > 1 {
            self.multi = keys.to_vec();
            self.anchor = Some(first.clone());
        }
    }
    /// 캔버스에서 글을 바로 고치는 중인가 (전역 Ctrl+Enter 송출을 막는다 — 원본은 keydown stopPropagation)
    pub fn inline_active(&self) -> bool {
        self.open && self.inline.is_some()
    }
    /// 전역 단축키보다 먼저: 글을 바로 고치는 중의 Esc 는 그만두기다 (예배당 화면 끄기 등으로 새지 않게)
    /// 편집기가 글쇠를 받는 중인가: 열려 있고, 마지막으로 누른 곳이 편집기 창 안
    pub fn keys_active(&self) -> bool {
        self.open && self.active
    }
    /// 누른 곳에 따라 편집기 글쇠 켜고 끄기 (지난 프레임의 창 배치로 가린다. 포인터 사건은 건드리지 않는다)
    fn track_focus(&mut self, ctx: &egui::Context) {
        if !self.open {
            self.active = false;
            return;
        }
        let Some(p) = ctx.input(|i| if i.pointer.any_pressed() { i.pointer.interact_pos() } else { None }) else { return };
        match ctx.layer_id_at(p) {
            Some(l) if Some(l) == self.win_layer => self.active = true,
            // 메뉴·팝업(편집기 오른쪽 클릭 메뉴 등)을 누른 것은 그대로 둔다
            Some(l) if matches!(l.order, egui::Order::Foreground | egui::Order::Tooltip) => {}
            _ => self.active = false,
        }
    }
    pub fn pre_keys(&mut self, ctx: &egui::Context) {
        self.track_focus(ctx);
        if self.inline_active() {
            // 글칸에 초점이 있을 때만 (메인 창을 누른 뒤의 Esc 는 전역 것)
            if self.inline_focus && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
                self.inline_esc = true;
            }
            return;
        }
        // 편집기 글쇠 (slideedit.js el keydown): 글 쓰는 칸에 있을 때는 그 칸이 받는다
        if !self.open || !self.active || !self.live || ctx.egui_wants_keyboard_input() {
            return;
        }
        let got = canvas::collect(ctx, !self.sel.is_empty(), self.multi.len() > 1);
        if got.contains(&EdKey::Close) {
            self.open = false;
        }
        self.keys.extend(got);
    }
    #[cfg(test)]
    pub(crate) fn selected(&self) -> &str {
        &self.sel
    }
    #[cfg(test)]
    pub(crate) fn inline_target(&self) -> Option<String> {
        self.inline.as_ref().map(|n| n.target.clone())
    }
    /// 편집기 목록 키: Esc = 한 장만 (여러 장을 고른 중) · Ctrl+A = 모두 (slideedit.js 키 처리)
    pub fn list_keys(&mut self, ctx: &egui::Context) {
        use egui::{Key, Modifiers};
        if !self.keys_active() {
            return;
        }
        if self.multi.len() > 1 && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            self.multi.clear();
        }
        if !self.common && ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::A)) {
            self.want_all = true;
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn show(&mut self, ctx: &egui::Context, core: &Shared, sel: Option<usize>, web_dir: &std::path::Path, data_dir: &std::path::Path, acts: &mut Vec<Act>) {
        #[cfg(test)]
        self.observed.clear();
        self.live = false;
        if self.inline.is_none() { self.inline_focus = false; }
        let mut keys = std::mem::take(&mut self.keys);
        if !self.open {
            self.drag = None;
            self.lasso = None;
            self.active = false;
            self.was_open = false;
            return;
        }
        if !std::mem::replace(&mut self.was_open, true) {
            self.active = true; // 막 열었다 = 편집기를 쓰려는 것
        }
        if std::mem::take(&mut self.undo_break) {
            core.lock().b.typing = None;
        }
        let mut open = self.open;
        let win = crate::widgets_win::window(ctx, "slideedit", wp_core::i18n::tr("슬라이드 편집 (Ctrl+E)"), [1100.0, 700.0]).open(&mut open).keep_on_outside().vscroll(false).show(ctx, |ui| {
            let Some(i) = sel else {
                ui.label(wp_core::i18n::tr("큐시트에서 순서를 먼저 고르세요."));
                return;
            };
            let (it, slides, tpl) = match editor_list(&mut core.lock(), i) {
                Ok(Some(x)) => x,
                Ok(None) => return,
                Err(e) => { ui.label(RichText::new(e).color(crate::theme::pal().warn)); return; }
            };
            if let Some(key) = self.pending_key.take() {
                if let Some(pos) = slides.iter().position(|s| edit_key(s) == key) { self.k = pos; }
            }
            if slides.is_empty() { return; }
            self.k = self.k.min(slides.len() - 1);
            // 방향키 (고른 요소 없음) = 앞·뒤 슬라이드 (slideedit.js: ←↑ 이전 · →↓ 다음)
            keys.retain(|k| {
                let n = match k {
                    EdKey::Nav(d) => (self.k as i64 + *d as i64).clamp(0, slides.len() as i64 - 1) as usize,
                    EdKey::Edge(last) => if *last { slides.len() - 1 } else { 0 },
                    _ => return true,
                };
                if self.inline.is_none() {
                                        if n != self.k {
                        self.k = n;
                        self.multi.clear();
                        self.sel.clear();
                    }
                }
                false
            });
            if std::mem::take(&mut self.want_all) {
                self.multi = slides.iter().filter(|s| !s["skipped"].as_bool().unwrap_or(false)).map(edit_key).collect();
                if !self.multi.contains(&edit_key(&slides[self.k])) {
                    if let Some(p) = slides.iter().position(|s| self.multi.first() == Some(&edit_key(s))) { self.k = p; }
                }
            }
            // 고른 것에 지금 슬라이드가 없으면 한 장만 (draw)
            let cur_key = edit_key(&slides[self.k]);
            self.multi.retain(|k| slides.iter().any(|s| &edit_key(s) == k));
            if self.common { self.multi.clear(); } else if !self.multi.contains(&cur_key) { self.multi = vec![cur_key.clone()]; self.anchor = Some(cur_key.clone()); }
            let multi_on = !self.common && self.multi.len() > 1;
            let ty = it["type"].as_str().unwrap_or("").to_string();
            let chunk = is_chunk(&ty);
            let mut new_it: Option<Value> = None;
            let mut split_back: Option<Value> = None;

            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(it["title"].as_str().unwrap_or("")).strong());
                ui.label(RichText::new(wp_core::i18n::tr_args("슬라이드 {1}장", &[&(slides.len())])).small().color(crate::theme::pal().muted));
                ui.checkbox(&mut self.common, wp_core::i18n::tr("모든 슬라이드에 공통(*)"));
                if multi_on {
                    // se-multi-note
                    let n = self.multi.len();
                    ui.label(RichText::new(wp_core::i18n::tr_args("고른 슬라이드 {1}장", &[&n])).strong().color(crate::theme::pal().accent));
                    ui.label(RichText::new(wp_core::i18n::tr_args("여기서 바꾸는 꾸미기·글상자·배경이 고른 {1}장에 함께 적용됩니다 (Esc: 한 장만)", &[&n])).small().color(crate::theme::pal().muted));
                }
            });
            ui.separator();
            let list_w = (ui.available_width() * 0.24).clamp(145.0, 260.0);
            egui::Panel::left("se-list").default_size(list_w).size_range(130.0..=280.0).show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    let clip = ui.clip_rect();
                    if self.lasso.is_some() && ui.input(|i| i.pointer.primary_down()) {
                        if let Some(p) = ui.input(|i| i.pointer.interact_pos()) {
                            let delta = lasso_scroll(clip, p);
                            if delta != 0.0 {
                                ui.scroll_with_delta(Vec2::new(0.0, delta));
                                ui.ctx().request_repaint();
                            }
                        }
                    }
                    for (k, s) in slides.iter().enumerate() {
                        let skipped = s["skipped"].as_bool().unwrap_or(false);
                        let lb = s["label"].as_str().filter(|x| !x.is_empty()).map(|x| format!("¶{x}")).or_else(|| s["ref"].as_str().map(String::from)).unwrap_or_default();
                        let mut txt = RichText::new(format!("{:>2}  {}", k + 1, lb));
                        if skipped { txt = txt.strikethrough().color(crate::theme::pal().muted2); }
                        if s["edit"].is_object() { txt = txt.color(crate::theme::pal().hl); }
                        // 줄 높이 30px: 마우스 올림 단추(26px)가 들어가게
                        let r = ui.add(egui::Button::selectable(k == self.k, txt).min_size(Vec2::new(ui.available_width(), 30.0)).sense(Sense::click_and_drag()));
                        if r.hovered() { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
                        let key_k = edit_key(s);
                        if multi_on && self.multi.contains(&key_k) {
                            // .se-slide.msel: accent 테두리 · accent 18% 바탕
                            let p = crate::theme::pal();
                            ui.painter().rect(r.rect, 4.0, crate::theme::alpha(p.accent, 0.18), egui::Stroke::new(1.5, p.accent), egui::StrokeKind::Inside);
                        }
                        // 끌어 놓을 자리 표시선 (줄 위·아래)
                        if self.drag_slide.is_some_and(|f| f != k) && r.contains_pointer() {
                            if let Some(p) = ui.input(|i| i.pointer.interact_pos()) {
                                let y = if p.y >= r.rect.center().y { r.rect.bottom() } else { r.rect.top() };
                                ui.painter().hline(r.rect.x_range(), y, egui::Stroke::new(3.0, crate::theme::pal().accent));
                            }
                        }
                        if let Some(op) = crate::widgets::hover_ops(ui, egui::Id::new(("se-ops", k)), r.rect, &list_ops(&ty, s)) {
                            let keys: Vec<String> = slides.iter().map(edit_key).collect();
                            match se::quick_op(&it, &keys, &edit_key(s), op) {
                                Ok((a, pick)) => {
                                    core.lock().b.typing = None; // 단추 한 번 = 실행 취소 한 번
                                    new_it = Some(a);
                                    self.pending_key = Some(pick);
                                }
                                Err(m) if !m.is_empty() => core.lock().notify(wp_core::i18n::tr(m), "info"),
                                Err(_) => {}
                            }
                        }
                        #[cfg(test)]
                        self.observed.insert(format!("slide-{k}"), r.rect);
                        // 끌기: 그냥 = 차례 옮기기 (Rust 판에서 넣은 것) · Shift = 끌어서 여러 장 고르기 (slideedit.js 올가미 — 원본은 그냥 끌기)
                        if r.drag_started() {
                            if ui.input(|i| i.modifiers.shift) && !self.common { self.lasso = Some(edit_key(s)); } else { self.drag_slide = Some(k); }
                        }
                        if let Some(from) = self.lasso.clone().filter(|_| ui.input(|i| i.pointer.primary_down() && i.pointer.interact_pos().is_some_and(|p| {
                            p.x >= clip.left() && p.x <= clip.right() && r.rect.contains(egui::pos2(p.x, p.y.clamp(clip.top() + 1.0, clip.bottom() - 1.0)))
                        }))) {
                            let keys: Vec<String> = slides.iter().map(edit_key).collect();
                            if let Some(a) = keys.iter().position(|x| x == &from) {
                                if a != k || self.multi.len() > 1 {
                                    self.anchor = Some(from);
                                    self.multi = keys[a.min(k)..=a.max(k)].to_vec();
                                    self.k = k;
                                }
                            }
                        }
                        if r.contains_pointer() && ui.input(|i| i.pointer.any_released()) {
                            if let Some(from) = self.drag_slide.take() {
                                // slideedit.js moveSlides: 고른 것 중 하나를 끌면 고른 것 모두, 줄 가운데보다 아래면 뒤에
                                let keys: Vec<String> = slides.iter().map(edit_key).collect();
                                let fk = keys[from].clone();
                                let moved = if multi_on && self.multi.contains(&fk) { self.multi.clone() } else { vec![fk.clone()] };
                                let after = ui.input(|i| i.pointer.interact_pos()).is_some_and(|p| p.y >= r.rect.center().y);
                                match se::move_keys(&it, &keys, &moved, &key_k, after) {
                                    Ok(a) => {
                                        new_it = Some(a);
                                        // 옮긴 것을 새 자리에서 고른 채로 (글 덩어리는 번호가 바뀜)
                                        let nk = se::keys_after_move(chunk, &keys, &moved, &key_k, after);
                                        if let Some(f) = nk.first() { self.pending_key = Some(f.clone()); }
                                        self.multi = if nk.len() > 1 { nk } else { vec![] };
                                    }
                                    Err(m) if !m.is_empty() => core.lock().notify(wp_core::i18n::tr(m), "info"),
                                    Err(_) => {}
                                }
                            }
                        }
                        if r.clicked() {
                            // 목록에서 고르기 (pickSlides): Shift = 기준부터 범위 · Ctrl = 더하기/빼기 · 그냥 = 한 장
                            let m = ui.input(|i| i.modifiers);
                            let keys: Vec<String> = slides.iter().map(edit_key).collect();
                            if self.common {
                                self.k = k;
                            } else if m.shift && self.anchor.as_ref().is_some_and(|a| keys.contains(a)) {
                                let a = keys.iter().position(|x| Some(x) == self.anchor.as_ref()).unwrap_or(k);
                                self.multi = keys[a.min(k)..=a.max(k)].to_vec();
                                self.k = k;
                            } else if m.command {
                                let mut ms = if self.multi.is_empty() { vec![edit_key(&slides[self.k])] } else { self.multi.clone() };
                                if ms.contains(&key_k) && ms.len() > 1 {
                                    ms.retain(|x| x != &key_k);
                                    if self.k == k { if let Some(p) = slides.iter().position(|x| Some(&edit_key(x)) == ms.first()) { self.k = p; } }
                                } else {
                                    ms.push(key_k.clone());
                                    self.k = k;
                                }
                                self.multi = ms;
                            } else {
                                self.multi = vec![key_k.clone()];
                                self.anchor = Some(key_k.clone());
                                self.k = k;
                            }
                        }
                        let src = s["src"].as_u64().unwrap_or(k as u64) as usize;
                        let base = s["base"].as_u64().map(|x| x as usize);
                        r.context_menu(|ui| {
                            let mut op = |name: &str, label: &str, ui: &mut egui::Ui| {
                                let response = ui.button(wp_core::i18n::tr(label));
                                #[cfg(test)]
                                self.observed.insert(format!("op-{name}"), response.rect);
                                if response.clicked() {
                                    let (a, b) = se::apply_op(&it, name, if chunk { src } else { base.unwrap_or(k) });
                                    new_it = Some(a);
                                    split_back = b;
                                    ui.close();
                                }
                            };
                            if chunk {
                                op("add", "뒤에 새 슬라이드 넣기", ui);
                                op("dup", "복제", ui);
                                op("up", "위로", ui);
                                op("down", "아래로", ui);
                                op("del", "지우기", ui);
                                if ty != "title" { op("split", "여기서 새 순서로 나누기", ui); }
                            } else if base.is_some() {
                                op("skip", if skipped { "다시 보이기" } else { "건너뛰기" }, ui);
                                let n = slides.iter().filter(|x| x["base"].is_number()).count();
                                let pos = slides.iter().take(k).filter(|x| x["base"].is_number()).count();
                                if ui.button(wp_core::i18n::tr("위로")).clicked() && pos > 0 { let mut a = it.clone(); se::move_order(&mut a, n, pos, pos - 1); new_it = Some(a); ui.close(); }
                                if ui.button(wp_core::i18n::tr("아래로")).clicked() && pos + 1 < n { let mut a = it.clone(); se::move_order(&mut a, n, pos, pos + 1); new_it = Some(a); ui.close(); }
                                if ui.button(wp_core::i18n::tr("뒤에 제목 카드 덧붙이기")).clicked() {
                                    let mut a = it.clone();
                                    let id = wp_core::util::to36(wp_core::util::now_ms() as u64);
                                    let mut ex = a["extra"].as_array().cloned().unwrap_or_default();
                                    ex.push(json!({ "id": &id[id.len().saturating_sub(8)..], "at": base.unwrap(), "text": "제목" }));
                                    a["extra"] = json!(ex);
                                    new_it = Some(a);
                                    ui.close();
                                }
                            } else if s["xid"].is_string() && ui.button(wp_core::i18n::tr("덧붙인 카드 지우기")).clicked() {
                                let mut a = it.clone();
                                let xid = s["xid"].clone();
                                let ex: Vec<Value> = a["extra"].as_array().cloned().unwrap_or_default().into_iter().filter(|e| e["id"] != xid).collect();
                                a["extra"] = json!(ex);
                                if let Some(edits) = a["edits"].as_object_mut() { edits.remove(&edit_key(s)); }
                                new_it = Some(a);
                                ui.close();
                            }
                        });
                    }
                });
            });
            if ui.input(|i| i.pointer.any_released() || !i.pointer.primary_down()) { self.drag_slide = None; self.lasso = None; }
            let slide = &slides[self.k];
            let key = if self.common { "*".to_string() } else { edit_key(slide) };
            let mut e = it["edits"][&key].clone();
            if !e.is_object() { e = json!({}); }
            let before = e.clone();
            egui::CentralPanel::default_margins().show(ui, |ui| {
                // 미리보기
                let w = ui.available_width().min(560.0);
                let tw = tpl["width"].as_f64().unwrap_or(1920.0) as f32;
                let th = tpl["height"].as_f64().unwrap_or(1080.0) as f32;
                let (rect, _) = ui.allocate_exact_size(Vec2::new(w, w * th / tw), Sense::hover());
                let mut show = slide.clone();
                if let Some(m) = wp_core::hymns::merge_edit(&{ let mut x = it["edits"].clone(); if x.is_object() { x[&key] = e.clone(); } else { x = json!({ &key: e.clone() }); } x }, &edit_key(slide)) { show["edit"] = m; }
                let c = paint::Ctx { web_dir, data_dir };
                paint_slide(ui, &c, rect, Some(&show), &tpl, "", true, false, false);
                // ---- [갈래 C] 글 바로 고치기 (slideedit.js:452 inlineEdit · 1012 dblclick) ----
                let item_id = it["id"].as_str().unwrap_or("").to_string();
                let pct = |x: f32, y: f32, w: f32, h: f32| egui::Rect::from_min_size(rect.min + Vec2::new(x * rect.width() / 100.0, y * rect.height() / 100.0), Vec2::new(w * rect.width() / 100.0, h * rect.height() / 100.0));
                // 다른 순서·슬라이드로 옮겨 갔으면 고치던 글을 저장하고 닫는다 (원본은 blur 때 저장)
                if let Some(n) = self.inline.clone().filter(|n| n.item_id != item_id || n.key != key || (n.target == "main" && edit_key(&n.slide) != edit_key(slide))) {
                    self.inline = None;
                    if n.item_id == item_id {
                        let mut a = new_it.clone().unwrap_or_else(|| it.clone());
                        match n.commit(&mut a) {
                            Ok(true) => new_it = Some(a),
                            Ok(false) => {}
                            Err(m) => core.lock().notify(wp_core::i18n::tr(m), "info"),
                        }
                    }
                }
                let editing = self.inline.is_some();
                // 고른 요소는 그 순서·슬라이드 것만 (다른 데로 가면 푼다)
                let ctx_now = (item_id.clone(), key.clone());
                if self.sel_ctx != ctx_now {
                    self.sel.clear();
                    self.drag = None;
                    self.sel_ctx = ctx_now;
                }
                let pal = crate::theme::pal();
                let full = |r: se::PRect| pct(r.x as f32, r.y as f32, r.w as f32, r.h as f32);
                // 본문 영역 (mainRect): 따로 옮긴 자리, 없으면 템플릿 본문 영역
                let main_p = if e["box"].is_object() {
                    se::PRect::of(&e["box"], se::PRect { x: 10.0, y: 10.0, w: 80.0, h: 80.0 })
                } else {
                    let cr = crate::slide_layout::content_rect(&tpl);
                    se::PRect { x: (cr.left() / tw * 100.0) as f64, y: (cr.top() / th * 100.0) as f64, w: (cr.width() / tw * 100.0) as f64, h: (cr.height() / th * 100.0) as f64 }
                };
                let main_r = full(main_p);
                // 본문 글상자가 있는 슬라이드인가 (hasMain: 그림·영상·카메라·빈 화면은 없음)
                let has_main = self.common || !no_text_kind(slide);
                // 캔버스 상자들. 겹침 차례는 원본과 같이: 글 뒤 요소 → 본문 → 글 앞 요소 (뒤에 놓은 것이 위에서 먼저 받는다)
                // 공통('*') 요소는 슬라이드에서 고르기만 된다 (원본 'locked' — '공통' 에서 고친다)
                struct CBox { id: String, r: se::PRect, own: Option<usize>, locked: bool, text: bool, name: &'static str }
                let mut boxes: Vec<CBox> = vec![];
                let commons: Vec<Value> = if key != "*" { it["edits"]["*"]["items"].as_array().cloned().unwrap_or_default() } else { vec![] };
                let owns: Vec<Value> = e["items"].as_array().cloned().unwrap_or_default();
                let cbox = |x: &Value, own: Option<usize>| CBox {
                    id: x["id"].as_str().unwrap_or("").to_string(),
                    r: se::PRect::of(x, se::PRect { x: 0.0, y: 0.0, w: 30.0, h: 20.0 }),
                    own,
                    locked: own.is_none() || x["locked"] == true,
                    text: x["type"] == "text",
                    name: match x["type"].as_str() { Some("text") => "글상자", Some("image") => "그림", Some("shape") => "도형", Some("camera") => "카메라", _ => "요소" },
                };
                for below in [true, false] {
                    for x in commons.iter().filter(|x| x["below"].as_bool().unwrap_or(false) == below) { boxes.push(cbox(x, None)); }
                    for (j, x) in owns.iter().enumerate().filter(|(_, x)| x["below"].as_bool().unwrap_or(false) == below) { boxes.push(cbox(x, Some(j))); }
                    if below && has_main {
                        boxes.push(CBox { id: "main".into(), r: main_p, own: None, locked: false, text: true, name: if e["hideMain"] == true { "본문 (숨김)" } else { "본문" } });
                    }
                }
                boxes.retain(|b| !b.id.is_empty());
                if !self.sel.is_empty() && !boxes.iter().any(|b| b.id == self.sel) { self.sel.clear(); } // 고른 것이 없어졌다
                let mut start: Option<Inline> = None;
                let mut want_edit: Option<String> = None; // 두 번 누름 · Enter · F2
                let (pdown, ppos, origin, mods) = ui.input(|i| (i.pointer.primary_down(), i.pointer.interact_pos(), i.pointer.press_origin(), i.modifiers));
                if !editing {
                    // 빈 곳을 누르면 고르기 풀기 (onPointerDown)
                    let bg = ui.interact(rect, ui.id().with(("se-canvas", &key)), Sense::click());
                    crate::widgets::mark("se-canvas", rect);
                    if bg.clicked() { self.sel.clear(); }
                    for b in &boxes {
                        let er = full(b.r);
                        let hit = er.intersect(rect);
                        let (response, c2) = if b.id == "main" {
                            let r = ui.interact(hit, ui.id().with(("se-main", &key)), Sense::click_and_drag());
                            crate::widgets::mark("se-main", hit);
                            let c = crate::widgets::click2(ui, ("se-main", item_id.as_str(), key.as_str()), &r);
                            (r, c)
                        } else if let Some(j) = b.own {
                            let r = ui.interact(hit, ui.id().with(("se-element", &key, j)), Sense::click_and_drag());
                            crate::widgets::mark(&format!("se-el-{j}"), hit);
                            #[cfg(test)]
                            self.observed.insert(format!("element-{j}"), r.rect);
                            let c = crate::widgets::click2(ui, ("se-el", item_id.as_str(), key.as_str(), j), &r);
                            (r, c)
                        } else {
                            let r = ui.interact(hit, ui.id().with(("se-common", &key, &b.id)), Sense::click());
                            crate::widgets::mark(&format!("se-common-{}", b.id), hit);
                            (r, crate::widgets::Click::None)
                        };
                        // 누르는 순간 고른다 (잠긴 것도 고르기는 된다)
                        if response.is_pointer_button_down_on() || response.clicked() { self.sel = b.id.clone(); }
                        // slideedit.js:1012 상자를 두 번 누르면 글 바로 고치기 (잠긴 상자 제외)
                        if c2 == crate::widgets::Click::Double && !b.locked { want_edit = Some(b.id.clone()); }
                        if response.drag_started_by(egui::PointerButton::Primary) && !b.locked {
                            core.lock().b.typing = None; // 끌기 한 번 = 실행 취소 한 번
                            self.drag = Some(ElDrag { id: b.id.clone(), handle: "", start: b.r, origin: origin.or(ppos).unwrap_or(er.center()), moved: false });
                        }
                        if (response.hovered() || response.dragged()) && self.sel != b.id {
                            ui.painter().rect_stroke(er, 0.0, egui::Stroke::new(1.0, Color32::LIGHT_BLUE), egui::StrokeKind::Inside);
                        }
                    }
                    // 크기 조절 손잡이 여덟 개: 고른 상자에만, 잠긴 것은 없다 (se-h)
                    if let Some(b) = boxes.iter().find(|b| b.id == self.sel && !b.locked) {
                        let er = full(b.r);
                        for (name, fx, fy) in canvas::HANDLES {
                            let hr = canvas::handle_rect(er, fx, fy);
                            let resp = ui.interact(hr, ui.id().with(("se-h", name)), Sense::drag());
                            crate::widgets::mark(&format!("se-h-{name}"), hr);
                            if resp.hovered() || resp.dragged() { ui.ctx().set_cursor_icon(canvas::handle_cursor(name)); }
                            if resp.drag_started_by(egui::PointerButton::Primary) {
                                core.lock().b.typing = None;
                                self.drag = Some(ElDrag { id: b.id.clone(), handle: name, start: b.r, origin: origin.or(ppos).unwrap_or(hr.center()), moved: false });
                            }
                        }
                    }
                } else {
                    self.drag = None;
                }
                let own_pos = |e: &Value, id: &str| e["items"].as_array().and_then(|a| a.iter().position(|x| x["id"].as_str() == Some(id)));
                // 끌기: 옮기기(가운데·가장자리에 붙음, Alt = 안 붙음) · 손잡이로 크기(Shift = 비율 유지) — onPointerMove
                let mut guides: (Option<f64>, Option<f64>) = (None, None);
                if let Some(mut d) = self.drag.take() {
                    if !pdown {
                        self.undo_break = true; // 끝났다 (onPointerUp)
                    } else {
                        if let Some(p) = ppos {
                            let dx = ((p.x - d.origin.x) / rect.width() * 100.0) as f64;
                            let dy = ((p.y - d.origin.y) / rect.height() * 100.0) as f64;
                            if !d.moved && dx.abs() + dy.abs() >= 0.4 { d.moved = true; }
                            if d.moved {
                                let nr = if d.handle.is_empty() {
                                    let (r, gv, gh) = se::drag_move(d.start, dx, dy, !mods.alt);
                                    guides = (gv, gh);
                                    r
                                } else {
                                    se::drag_resize(d.start, d.handle, dx, dy, mods.shift)
                                };
                                if d.id == "main" {
                                    nr.put(&mut e["box"]);
                                } else if let Some(j) = own_pos(&e, &d.id) {
                                    nr.put(&mut e["items"][j]);
                                }
                            }
                        }
                        self.drag = Some(d);
                    }
                }
                // 끌어다 놓은·붙여 넣은 그림 → 그림 요소 (slideedit.js dropImage)
                if !editing {
                    keys.extend(crate::dnd::take_se_images().into_iter().map(|(u, p)| EdKey::Image(u, p)));
                }
                // 편집기 글쇠 (slideedit.js el keydown): 고른 요소에
                for kx in keys.drain(..) {
                    if editing { break; }
                    let cut = kx == EdKey::Cut;
                    let own = own_pos(&e, &self.sel).filter(|j| e["items"][*j]["locked"] != true);
                    let mut changed = false;
                    match kx {
                        EdKey::Deselect => self.sel.clear(),
                        EdKey::Edit => want_edit = Some(if self.sel.is_empty() { "main".into() } else { self.sel.clone() }),
                        // 지우기는 이 슬라이드에 덧붙인 요소만 (delSelected): 본문 글상자·공통 요소는 안 지워진다
                        EdKey::Delete => {
                            if let Some(j) = own {
                                e["items"].as_array_mut().unwrap().remove(j);
                                self.sel.clear();
                                changed = true;
                            }
                        }
                        EdKey::Dup => {
                            if let Some(j) = own {
                                let c = se::dup_item(&e["items"][j]);
                                self.sel = c["id"].as_str().unwrap_or("").to_string();
                                e["items"].as_array_mut().unwrap().push(c);
                                changed = true;
                            }
                        }
                        EdKey::Nudge(dx, dy) => {
                            if self.sel == "main" {
                                if !e["box"].is_object() { main_p.put(&mut e["box"]); }
                                se::nudge(&mut e["box"], dx, dy);
                                changed = true;
                            } else if let Some(j) = own {
                                se::nudge(&mut e["items"][j], dx, dy);
                                changed = true;
                            }
                        }
                        // 요소 복사·잘라 내기 (Rust 판에서 넣은 것 — 원본은 Ctrl+D 복제만 있다). 클립보드에는 그 글(글상자) 또는 [종류] 를 쓴다
                        EdKey::Copy | EdKey::Cut => {
                            if let Some(j) = own {
                                let x = e["items"][j].clone();
                                let name = wp_core::i18n::tr(match x["type"].as_str() { Some("text") => "글상자", Some("image") => "그림", _ => "도형" });
                                let t = x["text"].as_str().map(str::trim).filter(|t| !t.is_empty()).map(String::from).unwrap_or_else(|| format!("[{name}]"));
                                ui.ctx().copy_text(t.clone());
                                self.clip = Some((x, t));
                                if cut {
                                    e["items"].as_array_mut().unwrap().remove(j);
                                    self.sel.clear();
                                    changed = true;
                                }
                            }
                        }
                        // 붙여 넣기: 방금 복사한 요소면 그 요소를, 그 밖의 글이면 새 글상자로 (paste.js → pasteText)
                        EdKey::Paste(t) => {
                            let same = self.clip.as_ref().is_some_and(|(_, c)| c.replace("\r\n", "\n").trim() == t.replace("\r\n", "\n").trim());
                            let new = if same {
                                let c = se::dup_item(&self.clip.as_ref().unwrap().0);
                                self.clip.as_mut().unwrap().0 = c.clone(); // 거듭 붙이면 조금씩 비켜서
                                Some(c)
                            } else {
                                se::paste_text_item(&t)
                            };
                            match new {
                                Some(c) => {
                                    self.sel = c["id"].as_str().unwrap_or("").to_string();
                                    let mut items = e["items"].as_array().cloned().unwrap_or_default();
                                    items.push(c);
                                    e["items"] = json!(items);
                                    changed = true;
                                }
                                None => core.lock().notify(wp_core::i18n::tr("클립보드에 붙여 넣을 글·그림이 없습니다."), "info"),
                            }
                        }
                        // 놓은 자리(없으면 가운데)에 30%x30% 그림 요소 (dropImage)
                        EdKey::Image(url, at) => {
                            let (w, h) = (30.0f64, 30.0f64);
                            let (x, y) = match at.filter(|p| rect.contains(*p)) {
                                Some(p) => ((((p.x - rect.left()) / rect.width() * 100.0) as f64 - w / 2.0).clamp(0.0, 100.0 - w), (((p.y - rect.top()) / rect.height() * 100.0) as f64 - h / 2.0).clamp(0.0, 100.0 - h)),
                                None => (35.0, 35.0),
                            };
                            let mut c = se::new_item("image");
                            c["src"] = json!(url);
                            c["x"] = json!((x * 10.0).round() / 10.0);
                            c["y"] = json!((y * 10.0).round() / 10.0);
                            c["w"] = json!(w);
                            c["h"] = json!(h);
                            self.sel = c["id"].as_str().unwrap_or("").to_string();
                            let mut items = e["items"].as_array().cloned().unwrap_or_default();
                            items.push(c);
                            e["items"] = json!(items);
                            changed = true;
                        }
                        EdKey::Nav(_) | EdKey::Edge(_) | EdKey::Close => {}
                    }
                    if changed {
                        core.lock().b.typing = None; // 글쇠 한 번 = 실행 취소 한 번 (begin)
                        self.undo_break = true;
                    }
                }
                // 글 바로 고치기 열기 (inlineEdit)
                if let Some(id) = want_edit.filter(|_| !editing) {
                    if id == "main" {
                        if self.common {
                            core.lock().notify(wp_core::i18n::tr("글은 각 슬라이드에서 고칩니다 (왼쪽 목록에서 슬라이드를 고르세요)."), "info");
                        } else if !no_text_kind(slide) {
                            match se::main_text(&it, slide) {
                                Some(t) => start = Some(Inline { item_id: item_id.clone(), key: key.clone(), target: "main".into(), slide: slide.clone(), text: t.clone(), orig: t, step: 0 }),
                                None => core.lock().notify(wp_core::i18n::tr("성경·찬송가 본문은 여기서 고치지 않습니다. 위치·꾸미기만 바꿀 수 있습니다."), "info"),
                            }
                        }
                    } else if let Some(b) = boxes.iter().find(|b| b.id == id && b.own.is_some() && b.text && !b.locked) {
                        if let Some(j) = own_pos(&e, &b.id) {
                            let t = e["items"][j]["text"].as_str().unwrap_or("").to_string();
                            start = Some(Inline { item_id: item_id.clone(), key: key.clone(), target: id.clone(), slide: Value::Null, text: t.clone(), orig: t, step: 0 });
                        }
                    }
                }
                // 고른 상자 테두리·이름표·손잡이 · 붙는 자리 안내선
                if !editing && start.is_none() {
                    let p = ui.painter_at(rect.expand(8.0));
                    if let Some(b) = boxes.iter().find(|b| b.id == self.sel) {
                        // 끌거나 방향키로 옮긴 그 프레임에도 따라오게 지금 값으로
                        let now = if b.id == "main" { if e["box"].is_object() { se::PRect::of(&e["box"], b.r) } else { b.r } } else { own_pos(&e, &b.id).map(|j| se::PRect::of(&e["items"][j], b.r)).unwrap_or(b.r) };
                        let er = full(now);
                        let col = if b.locked { pal.muted } else { pal.accent };
                        p.rect_stroke(er, 0.0, egui::Stroke::new(2.0, col), egui::StrokeKind::Inside);
                        let tag = if b.locked { format!("{} {}", wp_core::i18n::tr(b.name), wp_core::i18n::tr("(공통 — '공통' 에서 고치세요)")) } else { format!("{}", wp_core::i18n::tr(b.name)) };
                        let g = p.layout_no_wrap(tag, egui::FontId::proportional(11.0), Color32::WHITE);
                        let tr_ = egui::Rect::from_min_size(er.left_top(), g.size() + Vec2::new(8.0, 2.0));
                        p.rect_filled(tr_, 0.0, col);
                        p.galley(tr_.min + Vec2::new(4.0, 1.0), g, Color32::WHITE);
                        if !b.locked {
                            for (_, fx, fy) in canvas::HANDLES {
                                let hr = canvas::handle_rect(er, fx, fy);
                                p.rect(hr, 2.0, Color32::WHITE, egui::Stroke::new(1.5, pal.accent), egui::StrokeKind::Inside);
                            }
                        }
                    }
                    if let Some(gx) = guides.0 { p.vline(rect.left() + gx as f32 * rect.width() / 100.0, rect.y_range(), egui::Stroke::new(1.0, pal.accent)); }
                    if let Some(gy) = guides.1 { p.hline(rect.x_range(), rect.top() + gy as f32 * rect.height() / 100.0, egui::Stroke::new(1.0, pal.accent)); }
                }
                let mut inline_rect = self.inline.as_ref().filter(|n| n.target == "main").map(|_| main_r);
                let mut inline_size = e["style"]["size"].as_f64().or(tpl["font"]["size"].as_f64().map(|s| s * 1080.0 / th as f64)).unwrap_or(60.0) as f32;
                if let Some(x) = self.inline.as_ref().filter(|n| n.target != "main").and_then(|n| own_pos(&e, &n.target)).map(|j| &e["items"][j]) {
                    inline_rect = Some(full(se::PRect::of(x, se::PRect { x: 0.0, y: 0.0, w: 30.0, h: 20.0 })));
                    inline_size = x["size"].as_f64().unwrap_or(60.0) as f32;
                }
                self.live = true;
                if start.is_some() {
                    self.inline = start;
                } else if let (Some(mut n), Some(r)) = (self.inline.clone(), inline_rect) {
                    // 그 자리에 글칸 (.se-inline): Esc = 그만두기 · Ctrl+Enter 또는 다른 곳 누름 = 저장
                    let r = r.intersect(rect);
                    let tid = ui.id().with("se-inline");
                    let fs = (inline_size * rect.height() / 1080.0).clamp(11.0, 40.0);
                    let resp = ui.put(r, egui::TextEdit::multiline(&mut n.text).id(tid).font(egui::FontId::proportional(fs)).desired_width(r.width()));
                    crate::widgets::mark("se-inline", r);
                    self.inline_focus = resp.has_focus() || n.step < 2; // 막 열어 초점을 주는 중에도 편집기 것
                    let (esc, ctrl_enter) = ui.input(|i| (i.key_pressed(egui::Key::Escape), i.modifiers.command && i.key_pressed(egui::Key::Enter)));
                    let esc = esc | std::mem::take(&mut self.inline_esc);
                    let mut done: Option<bool> = None; // Some(true) = 저장
                    match n.step {
                        0 => { resp.request_focus(); n.step = 1; }
                        1 => {
                            // 열자마자 모두 고르기 (ta.select()) — 바로 치면 바뀐다
                            if let Some(mut s) = egui::TextEdit::load_state(ui.ctx(), tid) {
                                let end = n.text.chars().count();
                                s.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(end))));
                                s.store(ui.ctx(), tid);
                            }
                            n.step = 2;
                        }
                        _ => {
                            if ctrl_enter { done = Some(true); }
                            else if resp.lost_focus() || !resp.has_focus() { done = Some(!esc); }
                        }
                    }
                    if esc && n.step == 2 { done = Some(false); }
                    match done {
                        None => self.inline = Some(n),
                        Some(save) => {
                            self.inline = None;
                            if save {
                                let mut a = new_it.clone().unwrap_or_else(|| it.clone());
                                match n.commit(&mut a) {
                                    Ok(true) => new_it = Some(a),
                                    Ok(false) => {}
                                    Err(m) => core.lock().notify(wp_core::i18n::tr(m), "info"),
                                }
                            }
                        }
                    }
                } else if self.inline.is_some() {
                    self.inline = None; // 고치던 상자가 없어졌다
                }
                // se-hint
                ui.label(RichText::new(wp_core::i18n::tr("끌어서 옮기기 · 모서리로 크기 · 더블클릭: 글 고치기 · ←→↑↓: 슬라이드 이동 (상자를 고르면 미세 이동) · Ctrl+Z: 되돌리기")).small().color(crate::theme::pal().muted));
                ui.add_space(6.0);
                let sel_now = self.sel.clone();
                let mut pick_sel: Option<String> = None;
                egui::ScrollArea::vertical().id_salt("se-props").auto_shrink([false, false]).show(ui, |ui| {
                    if !self.common && (ty == "title" || slide["xid"].is_string()) {
                        let sk=edit_key(slide);
                        let mut text=if let Some(id)=sk.strip_prefix('x') {
                            it["extra"].as_array().into_iter().flatten().find(|e| e["id"].as_str()==Some(id)).and_then(|e| e["text"].as_str()).unwrap_or("").to_string()
                        } else { wp_core::slides::title_pages(&it).get(slide["src"].as_u64().unwrap_or(0) as usize).cloned().unwrap_or_default() };
                        ui.label(wp_core::i18n::tr("카드 본문 (첫 줄 제목, 나머지 부제)"));
                        if ui.add(egui::TextEdit::multiline(&mut text).desired_rows(2)).changed() {
                            let mut a=new_it.clone().unwrap_or_else(||it.clone());
                            se::set_card_text(&mut a,&sk,&text); new_it=Some(a);
                        }
                    }
                    ui.horizontal(|ui| {
                        let mut hm = e["hideMain"].as_bool().unwrap_or(false);
                        if ui.checkbox(&mut hm, wp_core::i18n::tr("본문 숨기기")).changed() { e["hideMain"] = json!(hm); }
                        let mut ao = e["allOutputs"].as_bool().unwrap_or(false);
                        if ui.checkbox(&mut ao, wp_core::i18n::tr("OBS 자막에도 적용")).changed() { e["allOutputs"] = json!(ao); }
                        ui.label(wp_core::i18n::tr("배경"));
                        let mut bg = e["bg"].as_str().unwrap_or("").to_string();
                        if ui.add(egui::TextEdit::singleline(&mut bg).hint_text(wp_core::i18n::tr("#색 또는 그림 주소")).desired_width(160.0)).changed() { e["bg"] = json!(bg); }
                    });
                    ui.horizontal(|ui| {
                        let mut has = e["box"].is_object();
                        if ui.checkbox(&mut has, wp_core::i18n::tr("글상자 위치 (화면 %)")).changed() {
                            e["box"] = if has { json!({ "x": 10, "y": 10, "w": 80, "h": 80 }) } else { Value::Null };
                        }
                        if has {
                            for k in ["x", "y", "w", "h"] {
                                let mut v = e["box"][k].as_f64().unwrap_or(0.0);
                                ui.label(k);
                                if ui.add(egui::DragValue::new(&mut v).range(-50.0..=200.0).speed(0.5)).changed() { e["box"][k] = json!(v); }
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(wp_core::i18n::tr("글자 크기"));
                        let mut sz = e["style"]["size"].as_f64().unwrap_or(0.0);
                        if ui.add(egui::DragValue::new(&mut sz).range(0.0..=400.0).speed(1.0)).on_hover_text(wp_core::i18n::tr("0 = 템플릿대로 (1080 화면 기준 px)")).changed() {
                            e["style"]["size"] = if sz >= 8.0 { json!(sz) } else { Value::Null };
                        }
                        ui.label(wp_core::i18n::tr("색"));
                        let mut col = e["style"]["color"].as_str().unwrap_or("").to_string();
                        if ui.add(egui::TextEdit::singleline(&mut col).hint_text("#ffffff").desired_width(70.0)).changed() { e["style"]["color"] = json!(col); }
                        let mut b = e["style"]["bold"].as_bool().unwrap_or(false);
                        if ui.checkbox(&mut b, wp_core::i18n::tr("굵게")).changed() { e["style"]["bold"] = json!(b); }
                        for (a, l) in [("left", "왼쪽"), ("center", "가운데"), ("right", "오른쪽")] {
                            if ui.selectable_label(e["style"]["align"] == a, wp_core::i18n::tr(l)).clicked() { e["style"]["align"] = json!(a); }
                        }
                        for (a, l) in [("top", "위"), ("middle", "중간"), ("bottom", "아래")] {
                            if ui.selectable_label(e["style"]["valign"] == a, wp_core::i18n::tr(l)).clicked() { e["style"]["valign"] = json!(a); }
                        }
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(wp_core::i18n::tr("덧붙인 요소")).strong());
                        for (t, l) in [("text", "+ 글상자"), ("image", "+ 그림"), ("shape", "+ 도형")] {
                            if ui.button(wp_core::i18n::tr(l)).clicked() {
                                let mut items = e["items"].as_array().cloned().unwrap_or_default();
                                let x = se::new_item(t);
                                pick_sel = x["id"].as_str().map(String::from); // 새로 넣은 것을 고른다 (addItem)
                                items.push(x);
                                e["items"] = json!(items);
                            }
                        }
                    });
                    let mut del = None;
                    let mut dup = None;
                    let n = e["items"].as_array().map(|a| a.len()).unwrap_or(0);
                    for j in 0..n {
                        let x = &mut e["items"][j];
                        ui.horizontal_wrapped(|ui| {
                            // 캔버스에서 고른 요소의 줄은 강조한다 · 이름을 누르면 캔버스에서 고른다
                            let on = !sel_now.is_empty() && x["id"].as_str() == Some(sel_now.as_str());
                            let mut name = RichText::new(wp_core::i18n::tr(match x["type"].as_str() { Some("text") => "글", Some("image") => "그림", Some("shape") => "도형", _ => "요소" })).small();
                            if on { name = name.strong().color(crate::theme::pal().accent); }
                            if ui.add(egui::Label::new(name).sense(Sense::click())).clicked() { pick_sel = x["id"].as_str().map(String::from); }
                            for k in ["x", "y", "w", "h"] {
                                let mut v = x[k].as_f64().unwrap_or(0.0);
                                if ui.add(egui::DragValue::new(&mut v).range(-50.0..=200.0).speed(0.5).prefix(format!("{k} "))).changed() { x[k] = json!(v); }
                            }
                            match x["type"].as_str() {
                                Some("text") => {
                                    let mut t = x["text"].as_str().unwrap_or("").to_string();
                                    if ui.add(egui::TextEdit::singleline(&mut t).desired_width(160.0)).changed() { x["text"] = json!(t); }
                                    let mut s = x["size"].as_f64().unwrap_or(60.0);
                                    if ui.add(egui::DragValue::new(&mut s).range(8.0..=400.0).prefix(wp_core::i18n::tr("크기 "))).changed() { x["size"] = json!(s); }
                                }
                                Some("image") => {
                                    if ui.button(wp_core::i18n::tr("그림 고르기…")).clicked() {
                                        if let Some(p) = rfd::FileDialog::new().add_filter(wp_core::i18n::tr("그림"), &["png", "jpg", "jpeg", "webp", "gif"]).pick_file() {
                                            let enc = percent_encoding::utf8_percent_encode(&p.to_string_lossy(), percent_encoding::NON_ALPHANUMERIC).to_string();
                                            x["src"] = json!(format!("/lib/file?p={enc}"));
                                        }
                                    }
                                }
                                _ => {
                                    let mut f = x["fill"].as_str().unwrap_or("").to_string();
                                    if ui.add(egui::TextEdit::singleline(&mut f).desired_width(70.0)).changed() { x["fill"] = json!(f); }
                                    let mut el = x["ellipse"].as_bool().unwrap_or(false);
                                    if ui.checkbox(&mut el, wp_core::i18n::tr("타원")).changed() { x["ellipse"] = json!(el); }
                                }
                            }
                            let mut opacity = x["opacity"].as_f64().unwrap_or(1.0);
                            if ui.add(egui::DragValue::new(&mut opacity).range(0.05..=1.0).speed(0.01).prefix(wp_core::i18n::tr("불투명도 "))).changed() { x["opacity"] = json!(opacity); }
                            let mut below = x["below"].as_bool().unwrap_or(false);
                            if ui.checkbox(&mut below, wp_core::i18n::tr("글 뒤")).changed() { x["below"] = json!(below); }
                            if ui.small_button(wp_core::i18n::tr("복제")).on_hover_text(wp_core::i18n::tr("복제 (Ctrl+D)")).clicked() { dup = Some(j); }
                            if ui.small_button("✕").clicked() { del = Some(j); }
                        });
                        ui.horizontal(|ui| {
                            if x["type"]=="text" {
                                let mut font=x["font"].as_str().unwrap_or("").to_string(); ui.label(wp_core::i18n::tr("글꼴"));
                                if ui.add(egui::TextEdit::singleline(&mut font).hint_text("맑은 고딕").desired_width(160.0)).changed() { x["font"]=json!(font); }
                            } else if x["type"]=="image" {
                                ui.label(wp_core::i18n::tr("자르기 (비율)"));
                                for (k,other,label) in [("l","r","왼쪽"),("t","b","위"),("r","l","오른쪽"),("b","t","아래")] {
                                    let mut v=x["crop"][k].as_f64().unwrap_or(0.0);
                                    let max=(0.959-x["crop"][other].as_f64().unwrap_or(0.0)).clamp(0.0,0.95);
                                    if ui.add(egui::DragValue::new(&mut v).range(0.0..=max).speed(0.01).prefix(wp_core::i18n::tr(label))).changed() { x["crop"][k]=json!(v); }
                                }
                            }
                        });
                    }
                    if let Some(j) = dup {
                        let c = se::dup_item(&e["items"][j]);
                        pick_sel = c["id"].as_str().map(String::from);
                        e["items"].as_array_mut().unwrap().push(c);
                    }
                    if let Some(j) = del { e["items"].as_array_mut().unwrap().remove(j); }
                    ui.add_space(6.0);
                    if ui.button(wp_core::i18n::tr("이 슬라이드 편집 지우기")).clicked() { e = json!({}); }
                });
                if let Some(id) = pick_sel { self.sel = id; }
            });
            if e != before {
                let mut a = new_it.clone().unwrap_or_else(|| it.clone());
                se::update_edit(&mut a, &key, |x| *x = e.clone());
                // 여러 장 함께 꾸미기: 바뀐 것만 다른 고른 슬라이드에 (slideedit.js propagate)
                if multi_on && self.multi.contains(&key) {
                    let others: Vec<String> = self.multi.iter().filter(|k| **k != key).cloned().collect();
                    se::propagate(&mut a, &others, &before, &e);
                    // 함께 지워서 빈 편집({"items":[]})이 남지 않게 정리한다 (slideedit.js clean)
                    for k in &others { se::update_edit(&mut a, k, |_| {}); }
                }
                new_it = Some(a);
            }
            if let Some(a) = new_it {
                acts.push(Act::UpdateItem(i, a));
            }
            if let Some(b) = split_back {
                let ty: &'static str = if b["type"] == "song" { "song" } else { "text" };
                acts.push(Act::AddItem(ty, Some(b)));
            }
        });
        if let Some(w) = win {
            self.win_layer = Some(w.response.layer_id);
            crate::dnd::zone(crate::dnd::Zone::SlideEdit, w.response.rect);
        }
        self.open = open;
    }
}

/// 중앙 목록처럼 가장자리 30px 에서 한 프레임에 18px 씩 넘긴다.
fn lasso_scroll(clip: egui::Rect, p: egui::Pos2) -> f32 {
    if p.x < clip.left() || p.x > clip.right() { return 0.0; }
    if p.y > clip.bottom() - 30.0 { -18.0 } else if p.y < clip.top() + 30.0 { 18.0 } else { 0.0 }
}
#[cfg(test)]
mod small_lasso_tests {
    use super::*;
    #[test]
    fn small_lasso_edge_scroll_both_directions() {
        let clip = egui::Rect::from_min_max(egui::pos2(10.0, 10.0), egui::pos2(300.0, 400.0));
        assert_eq!(lasso_scroll(clip, egui::pos2(100.0, 20.0)), 18.0);
        assert_eq!(lasso_scroll(clip, egui::pos2(100.0, 390.0)), -18.0);
        assert_eq!(lasso_scroll(clip, egui::pos2(100.0, 500.0)), -18.0);
        assert_eq!(lasso_scroll(clip, egui::pos2(100.0, 200.0)), 0.0);
        assert_eq!(lasso_scroll(clip, egui::pos2(500.0, 390.0)), 0.0);
    }
}
