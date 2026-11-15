//! [트랙 C] 슬라이드 편집기 (Ctrl+E): 고른 큐시트 순서의 슬라이드마다 글상자 위치·꾸미기·배경,
//! 덧붙인 글상자·그림·도형, 넣기·복제·지우기·옮기기·건너뛰기·여기서 새 순서로 나누기 (오른쪽 클릭 메뉴에도)

use crate::app::Act;
use crate::paint::{self, paint_slide};
use eframe::egui::{self, Color32, RichText, Sense, Vec2};
use serde_json::{json, Value};
use wp_core::core::Shared;
use wp_core::slideedit::{self as se, is_chunk};

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
    pub fn pre_keys(&mut self, ctx: &egui::Context) {
        if self.inline_active() && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            self.inline_esc = true;
        }
    }
    #[cfg(test)]
    pub(crate) fn inline_target(&self) -> Option<String> {
        self.inline.as_ref().map(|n| n.target.clone())
    }
    /// 편집기 목록 키: Esc = 한 장만 (여러 장을 고른 중) · Ctrl+A = 모두 (slideedit.js 키 처리)
    pub fn list_keys(&mut self, ctx: &egui::Context) {
        use egui::{Key, Modifiers};
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
        if !self.open {
            return;
        }
        let mut open = self.open;
        crate::widgets::float_window(ctx, "slideedit", wp_core::i18n::tr("슬라이드 편집 (Ctrl+E)"), [980.0, 640.0]).open(&mut open).show(ctx, |ui| {
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

            ui.horizontal(|ui| {
                ui.label(RichText::new(it["title"].as_str().unwrap_or("")).strong());
                ui.label(RichText::new(wp_core::i18n::tr_args("슬라이드 {1}장", &[&(slides.len())])).small().color(Color32::GRAY));
                ui.checkbox(&mut self.common, wp_core::i18n::tr("모든 슬라이드에 공통(*)"));
                if multi_on {
                    // se-multi-note
                    let n = self.multi.len();
                    ui.label(RichText::new(wp_core::i18n::tr_args("고른 슬라이드 {1}장", &[&n])).strong().color(crate::theme::pal().accent));
                    ui.label(RichText::new(wp_core::i18n::tr_args("여기서 바꾸는 꾸미기·글상자·배경이 고른 {1}장에 함께 적용됩니다 (Esc: 한 장만)", &[&n])).small().color(crate::theme::pal().muted));
                }
            });
            ui.separator();
            egui::Panel::left("se-list").default_size(300.0).show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
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
                        if r.drag_started() { self.drag_slide = Some(k); }
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
            if ui.input(|i| i.pointer.any_released()) { self.drag_slide = None; }
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
                // 본문 영역: 요소보다 먼저 놓아 요소가 위에서 먼저 받는다
                let main_r = if e["box"].is_object() {
                    let b = |k: &str, d: f64| e["box"][k].as_f64().unwrap_or(d) as f32;
                    pct(b("x", 10.0), b("y", 10.0), b("w", 80.0), b("h", 80.0))
                } else {
                    let cr = crate::slide_layout::content_rect(&tpl);
                    pct(cr.left() / tw * 100.0, cr.top() / th * 100.0, cr.width() / tw * 100.0, cr.height() / th * 100.0)
                };
                let mut start: Option<Inline> = None;
                if !editing {
                    let mresp = ui.interact(main_r.intersect(rect), ui.id().with(("se-main", &key)), Sense::click());
                    crate::widgets::mark("se-main", main_r.intersect(rect));
                    if crate::widgets::click2(ui, ("se-main", item_id.as_str(), key.as_str()), &mresp) == crate::widgets::Click::Double {
                        if self.common {
                            core.lock().notify(wp_core::i18n::tr("글은 각 슬라이드에서 고칩니다 (왼쪽 목록에서 슬라이드를 고르세요)."), "info");
                        } else if !no_text_kind(slide) {
                            match se::main_text(&it, slide) {
                                Some(t) => start = Some(Inline { item_id: item_id.clone(), key: key.clone(), target: "main".into(), slide: slide.clone(), text: t.clone(), orig: t, step: 0 }),
                                None => core.lock().notify(wp_core::i18n::tr("성경·찬송가 본문은 여기서 고치지 않습니다. 위치·꾸미기만 바꿀 수 있습니다."), "info"),
                            }
                        }
                    }
                }
                let mut inline_rect = self.inline.as_ref().filter(|n| n.target == "main").map(|_| main_r);
                let mut inline_size = e["style"]["size"].as_f64().or(tpl["font"]["size"].as_f64().map(|s| s * 1080.0 / th as f64)).unwrap_or(60.0) as f32;
                if let Some(items) = e["items"].as_array_mut() {
                    for (j, x) in items.iter_mut().enumerate().rev() {
                        let v = |k: &str, d: f64| x[k].as_f64().unwrap_or(d) as f32;
                        let er = egui::Rect::from_min_size(rect.min + Vec2::new(v("x",0.0)*rect.width()/100.0,v("y",0.0)*rect.height()/100.0),Vec2::new(v("w",30.0)*rect.width()/100.0,v("h",20.0)*rect.height()/100.0));
                        if self.inline.as_ref().is_some_and(|n| n.target != "main" && x["id"].as_str() == Some(n.target.as_str())) {
                            inline_rect = Some(er);
                            inline_size = v("size", 60.0);
                        }
                        if editing { continue; } // 고치는 동안에는 상자를 끌거나 다시 열지 않는다
                        let response = ui.interact(er.intersect(rect),ui.id().with(("se-element",&key,j)),Sense::click_and_drag());
                        crate::widgets::mark(&format!("se-el-{j}"), er.intersect(rect));
                        // slideedit.js:1012 상자를 두 번 누르면 글 바로 고치기 (글상자만 · 잠긴 상자 제외)
                        if crate::widgets::click2(ui, ("se-el", item_id.as_str(), key.as_str(), j), &response) == crate::widgets::Click::Double && x["type"] == "text" && x["locked"] != true {
                            if let Some(id) = x["id"].as_str() {
                                let t = x["text"].as_str().unwrap_or("").to_string();
                                start = Some(Inline { item_id: item_id.clone(), key: key.clone(), target: id.into(), slide: Value::Null, text: t.clone(), orig: t, step: 0 });
                            }
                        }
                        #[cfg(test)]
                        self.observed.insert(format!("element-{j}"), response.rect);
                        if response.hovered() || response.dragged() { ui.painter().rect_stroke(er,0.0,egui::Stroke::new(1.0,Color32::LIGHT_BLUE),egui::StrokeKind::Inside); }
                        if response.dragged() {
                            let d=ui.input(|i| i.pointer.delta());
                            se::translate_item(x,(d.x/rect.width()*100.0) as f64,(d.y/rect.height()*100.0) as f64);
                        }
                    }
                }
                if start.is_some() {
                    self.inline = start;
                } else if let (Some(mut n), Some(r)) = (self.inline.clone(), inline_rect) {
                    // 그 자리에 글칸 (.se-inline): Esc = 그만두기 · Ctrl+Enter 또는 다른 곳 누름 = 저장
                    let r = r.intersect(rect);
                    let tid = ui.id().with("se-inline");
                    let fs = (inline_size * rect.height() / 1080.0).clamp(11.0, 40.0);
                    let resp = ui.put(r, egui::TextEdit::multiline(&mut n.text).id(tid).font(egui::FontId::proportional(fs)).desired_width(r.width()));
                    crate::widgets::mark("se-inline", r);
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
                ui.add_space(6.0);
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
                                items.push(se::new_item(t));
                                e["items"] = json!(items);
                            }
                        }
                    });
                    let mut del = None;
                    let n = e["items"].as_array().map(|a| a.len()).unwrap_or(0);
                    for j in 0..n {
                        let x = &mut e["items"][j];
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(wp_core::i18n::tr(match x["type"].as_str() { Some("text") => "글", Some("image") => "그림", Some("shape") => "도형", _ => "요소" })).small());
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
                    if let Some(j) = del { e["items"].as_array_mut().unwrap().remove(j); }
                    ui.add_space(6.0);
                    if ui.button(wp_core::i18n::tr("이 슬라이드 편집 지우기")).clicked() { e = json!({}); }
                });
            });
            if e != before {
                let mut a = new_it.clone().unwrap_or_else(|| it.clone());
                se::update_edit(&mut a, &key, |x| *x = e.clone());
                // 여러 장 함께 꾸미기: 바뀐 것만 다른 고른 슬라이드에 (slideedit.js propagate)
                if multi_on && self.multi.contains(&key) {
                    let others: Vec<String> = self.multi.iter().filter(|k| **k != key).cloned().collect();
                    se::propagate(&mut a, &others, &before, &e);
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
        self.open = open;
    }
}
