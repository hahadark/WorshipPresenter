//! [트랙 C] 출력 템플릿 편집 창 — docs/ux-rules.md 3절 '테마(템플릿) 편집'.
//! 3단: 왼쪽 템플릿 목록(그림 썸네일 · 쓰는 곳 배지 · 새로 · 복제 · 삭제 확인),
//! 가운데 큰 캔버스(실제 출력과 같은 그림 · 견본 성경/찬양/공지/제목 · 여백 테두리·로고 끌기 · 누르면 그 속성으로),
//! 오른쪽 속성 칸(tpl_inspect: 접는 묶음).
//! 바꾸면 바로 저장(save_template + Ev::Live) · '저장됨' 표시 · 실행 취소/다시 · '원래대로'(창을 열거나 템플릿을 고른 때 상태).
//! 저장 형식(settings.json templates)은 Electron 판과 같다.

use crate::paint::{self, paint_slide};
use crate::theme::{self, alpha, pal, Weight};
use crate::tpl_controls::{f, set};
use crate::tpl_inspect::{self, Sec};
use crate::widgets::{self, Btn, Kind, Size};
use eframe::egui::{self, Align2, Color32, CornerRadius, Key, KeyboardShortcut, Modifiers, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Vec2};
use serde_json::{json, Value};
use wp_core::core::Shared;
use wp_core::i18n::tr;
use wp_core::templates;

/// 자동 저장까지 기다리는 시간 (초) — 막대를 끄는 동안은 놓을 때까지 기다린다
const SAVE_DELAY: f64 = 0.35;
/// 쓰는 곳 (activeTemplates 칸, 이름, 아이콘)
const SLOTS: [(&str, &str, &str); 3] = [("main", "예배당", "monitor"), ("overlay", "OBS 자막", "radio"), ("overlayLyrics", "OBS 찬송가·찬양", "music")];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Edge {
    Top,
    Bottom,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    Edge(Edge),
    /// 로고: 누른 곳과 로고 왼쪽 위의 차이
    Logo(Vec2),
}

#[derive(Clone, Debug, PartialEq, Default)]
enum SaveState {
    #[default]
    Saved,
    Pending,
    Error(String),
}

#[derive(Default)]
pub struct TemplateUi {
    pub open: bool,
    sel: String,
    draft: Value,
    /// '원래대로' 기준 (창을 열거나 템플릿을 고른 때)
    orig: Value,
    undo: Vec<Value>,
    redo: Vec<Value>,
    /// 이어지는 조작(막대 끌기 등) 시작 전 상태 — 놓을 때 실행 취소 한 칸으로
    gesture: Option<Value>,
    changed_at: Option<f64>,
    save: SaveState,
    msg: String,
    sample: usize,
    focus: Option<Sec>,
    confirm_del: bool,
    drag: Option<Drag>,
    guides: bool,
    guides_set: bool,
    /// 다음에 열 때 고를 템플릿 (빠른 설정 '자세히 ▸')
    want: Option<String>,
    win_rect: Option<Rect>,
    /// 마지막 캔버스 자리와 그 위 본문·장절·로고 자리 (headless 시험이 이 좌표로 누른다)
    pub(crate) canvas_seen: Option<(Rect, paint::Hits)>,
    /// 캔버스에서 마지막으로 눌러 펼친 묶음
    pub(crate) last_focus: Option<Sec>,
}

static CHECKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 견본 슬라이드 (성경 · 찬양 · 공지 · 제목)
fn sample(k: usize) -> Value {
    match k {
        1 => json!({ "kind": "text", "ref": "찬양 제목", "title": "찬양 제목", "label": "1절", "parts": [{ "book": 0, "chapter": 0, "verse": 0, "text": "찬양 가사 첫 줄이 여기에 나옵니다\n둘째 줄도 이렇게 이어집니다" }] }),
        2 => json!({ "kind": "text", "ref": "광고", "title": "광고", "label": "공지", "parts": [{ "book": 0, "chapter": 0, "verse": 0, "text": "오늘 오후 2시 교사 모임이 있습니다\n본당 2층 소예배실" }] }),
        3 => json!({ "kind": "title", "ref": "주일 예배", "subtitle": "말씀과 찬양으로 드리는 예배", "label": "제목", "parts": [] }),
        _ => json!({ "kind": "bible", "ref": "요한복음 3:16", "title": "요한복음 3:16-17", "parts": [
            { "book": 43, "chapter": 3, "verse": 16, "text": "하나님이 세상을 이처럼 사랑하사 독생자를 주셨으니 이는 그를 믿는 자마다 멸망하지 않고 영생을 얻게 하려 하심이라", "heading": "하나님의 사랑" }
        ] }),
    }
}

fn slot_color(slot: &str) -> Color32 {
    let p = pal();
    match slot {
        "main" => p.accent,
        "overlay" => p.pv,
        _ => p.gold,
    }
}

/// 점선 사각형
fn dashed_rect(pt: &egui::Painter, r: Rect, stroke: Stroke) {
    let pts = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
    pt.extend(Shape::dashed_line(&pts, stroke, 6.0, 4.0));
}

impl TemplateUi {
    /// 이 템플릿을 골라 연다 (빠른 설정 '자세히 ▸')
    pub fn open_with(&mut self, id: &str) {
        if !self.open || self.sel != id {
            self.want = Some(id.to_string());
        }
        self.open = true;
    }

    /// 마우스가 이 창 위에 있으면 Ctrl+Z 는 창 안 실행 취소 (큐시트 되돌리기가 받지 않게)
    pub fn owns_undo(&self, ctx: &egui::Context) -> bool {
        self.open && self.win_rect.zip(ctx.pointer_latest_pos()).is_some_and(|(r, p)| r.contains(p))
    }

    fn load(&mut self, t: &Value) {
        self.sel = t["id"].as_str().unwrap_or("").to_string();
        self.draft = t.clone();
        self.orig = t.clone();
        self.undo.clear();
        self.redo.clear();
        self.gesture = None;
        self.changed_at = None;
        self.save = SaveState::Saved;
        self.confirm_del = false;
        self.drag = None;
    }

    /// 지금 고친 것을 저장하고 출력(미리보기·LIVE)에 다시 보낸다
    fn flush(&mut self, core: &Shared) {
        if self.changed_at.is_none() || self.draft.is_null() {
            return;
        }
        let r = templates::save_template(&mut core.lock().settings, &self.draft);
        match r {
            Ok(t) => {
                self.draft["id"] = t["id"].clone();
                self.sel = t["id"].as_str().unwrap_or("").to_string();
                self.changed_at = None;
                self.save = SaveState::Saved;
                let c = core.lock();
                c.emit(wp_core::core::Ev::Live);
                c.emit(wp_core::core::Ev::Ui);
            }
            Err(e) => {
                self.changed_at = None;
                self.save = SaveState::Error(e);
            }
        }
    }

    fn apply_history(&mut self, back: bool) {
        let (from, to) = if back { (&mut self.undo, &mut self.redo) } else { (&mut self.redo, &mut self.undo) };
        if let Some(v) = from.pop() {
            to.push(std::mem::replace(&mut self.draft, v));
            self.gesture = None;
            self.changed_at = Some(0.0);
            self.save = SaveState::Pending;
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, core: &Shared, web_dir: &std::path::Path, data_dir: &std::path::Path) {
        if !self.open {
            if self.changed_at.is_some() {
                self.flush(core);
            }
            self.draft = Value::Null;
            self.win_rect = None;
            return;
        }
        if !self.guides_set {
            self.guides = true;
            self.guides_set = true;
        }
        let (list, active) = {
            let c = core.lock();
            let st = c.settings.get();
            let a = &st["activeTemplates"];
            let main = a["main"].as_str().unwrap_or("").to_string();
            let ov = a["overlay"].as_str().unwrap_or("").to_string();
            let lyr = a["overlayLyrics"].as_str().filter(|x| !x.is_empty()).map(String::from).unwrap_or_else(|| ov.clone());
            (c.settings.templates(), [main, ov, lyr])
        };
        if let Some(w) = self.want.take() {
            self.flush(core);
            if let Some(t) = list.iter().find(|t| t["id"] == w.as_str()) {
                self.load(t);
            }
        }
        // 점검용: --tpl-check <테마 id>[:main|overlay][:묶음] — 테마 템플릿을 (저장하지 않고) 열어 그 묶음을 펼친다
        if !CHECKED.swap(true, std::sync::atomic::Ordering::SeqCst) {
            if let Some(arg) = std::env::args().position(|x| x == "--tpl-check").and_then(|i| std::env::args().nth(i + 1)) {
                let mut it = arg.split(':');
                let (tid, slot, sec) = (it.next().unwrap_or(""), it.next().unwrap_or("main"), it.next().unwrap_or(""));
                if let Some(th) = wp_core::themes::list_themes().iter().find(|x| x["id"] == tid) {
                    self.load(&wp_core::settings::normalize_template(&th[slot]));
                }
                self.focus = Sec::ALL.into_iter().find(|x| x.id() == sec);
            }
        }
        if self.draft.is_null() {
            let id = active[0].clone();
            if let Some(t) = list.iter().find(|t| t["id"] == id.as_str()).or(list.first()) {
                self.load(&t.clone());
            }
        }
        let t_now = ctx.input(|i| i.time);
        let pc = paint::Ctx { web_dir, data_dir };
        let before = self.draft.clone();
        let mut open = self.open;
        let screen = ctx.content_rect();
        let size = Vec2::new((screen.width() - 40.0).clamp(900.0, 1560.0), (screen.height() - crate::widgets::FLOAT_TOP - 12.0).clamp(520.0, 880.0));
        // 자리는 다른 떠 있는 창과 같은 규칙 (widgets::float_window: 메뉴·검색줄 아래, 격치면 비껴 놓기)
        let win = crate::widgets::float_window(ctx, "tpl-editor", tr("출력 템플릿"), [size.x, size.y])
            .open(&mut open)
            .collapsible(false)
            .vscroll(false)
            .show(ctx, |ui| {
                let h = ui.available_height().max(480.0);
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 14.0;
                    ui.allocate_ui_with_layout(Vec2::new(236.0, h), egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.set_min_size(Vec2::new(236.0, h));
                        self.list_column(ui, core, &list, &active, &pc);
                    });
                    let cw = (ui.available_width() - 372.0 - 14.0).max(360.0);
                    ui.allocate_ui_with_layout(Vec2::new(cw, h), egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.set_min_size(Vec2::new(cw, h));
                        if !self.draft.is_null() {
                            self.center_column(ui, core, &active, &pc);
                        }
                    });
                    ui.allocate_ui_with_layout(Vec2::new(372.0, h), egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.set_min_size(Vec2::new(360.0, h));
                        if !self.draft.is_null() {
                            ui.label(theme::rich(tr("속성"), 13.0, Weight::SemiBold, pal().muted));
                            egui::ScrollArea::vertical().id_salt("tpl-inspect").auto_shrink([false, false]).show(ui, |ui| {
                                ui.set_width(352.0);
                                let focus = self.focus.take();
                                tpl_inspect::inspector(ui, &mut self.draft, focus, &pc, data_dir, &mut self.msg);
                            });
                        }
                    });
                });
            });
        self.win_rect = win.as_ref().map(|w| w.response.rect);
        self.open = open;
        // 바뀐 것: 실행 취소 기록 · 자동 저장
        let any_down = ctx.input(|i| i.pointer.any_down());
        if self.draft != before && !before.is_null() && before["id"] == self.draft["id"] {
            if self.gesture.is_none() {
                self.gesture = Some(before);
            }
            self.changed_at = Some(t_now);
            self.save = SaveState::Pending;
            self.redo.clear();
        }
        if !any_down {
            if let Some(g) = self.gesture.take() {
                if g != self.draft {
                    self.undo.push(g);
                    if self.undo.len() > 200 {
                        self.undo.remove(0);
                    }
                }
            }
        }
        if self.owns_undo(ctx) && !ctx.egui_wants_keyboard_input() {
            let redo = ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z)) || i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Y)));
            if redo {
                self.apply_history(false);
            } else if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Z))) {
                self.apply_history(true);
            }
        }
        if let Some(t0) = self.changed_at {
            if !any_down && t_now - t0 >= SAVE_DELAY {
                self.flush(core);
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(120));
            }
        }
        if !self.open {
            self.flush(core);
        }
    }

    /// 왼쪽: 템플릿 목록
    fn list_column(&mut self, ui: &mut egui::Ui, core: &Shared, list: &[Value], active: &[String; 3], pc: &paint::Ctx) {
        let p = pal();
        ui.label(theme::rich(tr("템플릿"), 13.0, Weight::SemiBold, p.muted));
        let list_h = (ui.available_height() - 130.0).max(120.0);
        let mut pick: Option<Value> = None;
        egui::ScrollArea::vertical().id_salt("tpl-list").max_height(list_h).auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            let smp = sample(0);
            for t in list {
                let id = t["id"].as_str().unwrap_or("");
                let on = self.sel == id;
                // 고른 것은 고치는 중인 모양(draft)으로
                let shown = if on { &self.draft } else { t };
                let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 66.0), Sense::click());
                let bg = if on { p.sel } else if resp.hovered() { p.hover } else { Color32::TRANSPARENT };
                ui.painter().rect_filled(r, CornerRadius::same(6), bg);
                if on {
                    ui.painter().rect_filled(Rect::from_min_size(r.min, Vec2::new(3.0, r.height())), CornerRadius::same(2), p.accent);
                }
                let tw = f(shown, "width", 1920.0) as f32;
                let th = f(shown, "height", 1080.0) as f32;
                let tr_h = 54.0;
                let tr_w = (tr_h * tw / th).min(100.0);
                let thumb = Rect::from_min_size(Pos2::new(r.left() + 8.0, r.center().y - tr_h / 2.0), Vec2::new(tr_w, tr_h));
                paint_slide(ui, pc, thumb, Some(&smp), shown, "개역개정", true, false, false);
                ui.painter().rect_stroke(thumb, CornerRadius::same(2), Stroke::new(1.0, p.line), StrokeKind::Outside);
                // 이름 + 배지
                let x0 = thumb.right() + 8.0;
                let name = shown["name"].as_str().unwrap_or("");
                let mut job = egui::text::LayoutJob::simple(name.to_string(), theme::font(13.0, if on { Weight::SemiBold } else { Weight::Regular }), p.text, r.right() - x0 - 4.0);
                job.wrap.max_rows = 2;
                let g = ui.painter().layout_job(job);
                ui.painter().galley(Pos2::new(x0, r.top() + 7.0), g, p.text);
                let mut bx = x0;
                let by = r.bottom() - 21.0;
                for (k, (slot, label, _)) in SLOTS.iter().enumerate() {
                    // 찬송가 칸을 따로 정하지 않았으면(자막과 같음) 배지를 겹쳐 달지 않는다
                    if active[k] != id || (k == 2 && active[2] == active[1]) {
                        continue;
                    }
                    let col = slot_color(slot);
                    let gl = ui.painter().layout_no_wrap(tr(label), theme::font(10.5, Weight::SemiBold), col);
                    let br = Rect::from_min_size(Pos2::new(bx, by), Vec2::new(gl.size().x + 10.0, gl.size().y + 3.0));
                    if br.right() > r.right() {
                        break;
                    }
                    ui.painter().rect(br, CornerRadius::same(4), alpha(col, 0.16), Stroke::new(1.0, alpha(col, 0.55)), StrokeKind::Inside);
                    ui.painter().galley(br.center() - gl.size() / 2.0, gl, col);
                    bx = br.right() + 4.0;
                }
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if resp.clicked() && !on {
                    pick = Some(t.clone());
                }
            }
        });
        if let Some(t) = pick {
            self.flush(core);
            self.load(&t);
        }
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
            if ui.add(Btn::new(tr("새 템플릿")).icon("plus").size(Size::Sm).tooltip(tr("기본 모양으로 새 템플릿을 만듭니다"))).clicked() {
                self.flush(core);
                let mut t = wp_core::settings::TEMPLATE_BASE.clone();
                t["id"] = json!("");
                t["name"] = json!(tr("새 템플릿"));
                match templates::save_template(&mut core.lock().settings, &t) {
                    Ok(t) => {
                        self.load(&t);
                        self.msg = tr("새 템플릿을 만들었습니다.");
                        core.lock().emit(wp_core::core::Ev::Ui);
                    }
                    Err(e) => self.msg = e,
                }
            }
            if ui.add(Btn::new(tr("복제")).icon("copy").size(Size::Sm).enabled(!self.sel.is_empty()).tooltip(tr("이 템플릿을 복사해 새로 만듭니다"))).clicked() {
                self.flush(core);
                match templates::duplicate_template(&mut core.lock().settings, &self.sel, None) {
                    Ok(t) => {
                        self.load(&t);
                        self.msg = tr("복제했습니다.");
                        core.lock().emit(wp_core::core::Ev::Ui);
                    }
                    Err(e) => self.msg = e,
                }
            }
            let used: Vec<String> = SLOTS.iter().enumerate().filter(|(k, _)| active[*k] == self.sel).map(|(_, s)| tr(s.1)).collect();
            let can_del = used.is_empty() && list.len() > 1;
            let why = if !used.is_empty() {
                wp_core::i18n::tr_args("지금 {1}에 쓰는 중이라 지울 수 없습니다. 다른 템플릿을 먼저 쓰세요.", &[&used.join(", ")])
            } else if list.len() <= 1 {
                tr("마지막 템플릿은 지울 수 없습니다")
            } else {
                tr("이 템플릿 지우기 (한 번 더 묻습니다)")
            };
            if ui.add(Btn::new(tr("삭제")).icon("trash-2").size(Size::Sm).enabled(can_del).tooltip(why)).clicked() {
                self.confirm_del = true;
            }
        });
        if self.confirm_del {
            ui.add_space(4.0);
            widgets::group_frame().show(ui, |ui| {
                let name = self.draft["name"].as_str().unwrap_or("").to_string();
                ui.label(theme::rich(wp_core::i18n::tr_args("'{1}' 템플릿을 지울까요? 되돌릴 수 없습니다.", &[&name]), 12.5, Weight::Regular, p.text));
                ui.horizontal(|ui| {
                    if ui.add(Btn::new(tr("지우기")).icon("trash-2").kind(Kind::Danger).size(Size::Sm)).clicked() {
                        self.changed_at = None;
                        let r = templates::delete_template(&mut core.lock().settings, &self.sel);
                        match r {
                            Ok(()) => {
                                self.msg = tr("삭제했습니다.");
                                self.draft = Value::Null;
                                self.sel.clear();
                                core.lock().emit(wp_core::core::Ev::Ui);
                            }
                            Err(e) => self.msg = e,
                        }
                        self.confirm_del = false;
                    }
                    if ui.add(Btn::new(tr("취소")).size(Size::Sm)).clicked() {
                        self.confirm_del = false;
                    }
                });
            });
        }
        if !self.msg.is_empty() {
            ui.add_space(4.0);
            ui.label(theme::rich(&self.msg, 12.0, Weight::Regular, p.muted));
        }
    }

    /// 가운데: 이름 · 실행 취소 · 저장 상태 / 견본 고르기 / 캔버스 / 쓰는 곳
    fn center_column(&mut self, ui: &mut egui::Ui, core: &Shared, active: &[String; 3], pc: &paint::Ctx) {
        let p = pal();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            let mut name = self.draft["name"].as_str().unwrap_or("").to_string();
            let r = ui.add(egui::TextEdit::singleline(&mut name).font(theme::font(15.0, Weight::SemiBold)).desired_width((ui.available_width() - 340.0).max(140.0)).hint_text(tr("템플릿 이름")));
            r.on_hover_text(tr("템플릿 이름"));
            if name != self.draft["name"].as_str().unwrap_or("") {
                self.draft["name"] = json!(name);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // 저장 상태
                let (icon, text, col) = match &self.save {
                    SaveState::Saved => ("check", tr("저장됨"), p.ok_text),
                    SaveState::Pending => ("clock", tr("저장 중…"), p.muted),
                    SaveState::Error(_) => ("circle-x", tr("저장 못 함"), p.danger_text),
                };
                let tip = match &self.save {
                    SaveState::Error(e) => e.clone(),
                    _ => tr("바꾸면 바로 저장되고 미리보기·LIVE 에도 바로 보입니다"),
                };
                let resp = ui.horizontal(|ui| {
                    ui.label(theme::rich(text, 12.5, Weight::Medium, col));
                    crate::icons::show(ui, icon, 15.0, col);
                });
                resp.response.on_hover_text(tip);
                ui.add_space(6.0);
                let changed_from_orig = self.draft != self.orig;
                let tip = tr(if changed_from_orig { "이 템플릿을 고른 때 모양으로 되돌립니다 (실행 취소로 다시 올 수 있음)" } else { "아직 바꾼 것이 없습니다" });
                if ui.add(Btn::new(tr("원래대로")).icon("rotate-ccw").size(Size::Sm).enabled(changed_from_orig).tooltip(tip)).clicked() {
                    let o = self.orig.clone();
                    self.undo.push(std::mem::replace(&mut self.draft, o));
                    self.redo.clear();
                    self.changed_at = Some(0.0);
                    self.save = SaveState::Pending;
                }
                if ui.add(Btn::icon_only("redo-2").size(Size::Sm).enabled(!self.redo.is_empty()).tooltip(tr("다시 실행 (Ctrl+Y)"))).clicked() {
                    self.apply_history(false);
                }
                if ui.add(Btn::icon_only("undo-2").size(Size::Sm).enabled(!self.undo.is_empty()).tooltip(tr("실행 취소 (Ctrl+Z)"))).clicked() {
                    self.apply_history(true);
                }
            });
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(theme::rich(tr("견본"), 12.5, Weight::Regular, p.muted));
            let items = [(0usize, tr("성경")), (1, tr("찬양")), (2, tr("공지")), (3, tr("제목"))];
            let refs: Vec<(usize, &str)> = items.iter().map(|(k, l)| (*k, l.as_str())).collect();
            widgets::segmented_with(ui, &mut self.sample, &refs, widgets::SegStyle { font: 12.5, pad: Vec2::new(10.0, 4.0), fill: false, radius: 6 }).on_hover_text(tr("어떤 슬라이드로 보며 고칠지"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(Btn::new(tr("안내선")).icon(if self.guides { "eye" } else { "eye-off" }).size(Size::Sm).selected(self.guides).tooltip(tr("글 영역 점선 테두리와 안전 영역 보이기/숨기기"))).clicked() {
                    self.guides = !self.guides;
                }
            });
        });
        ui.add_space(6.0);
        // 캔버스
        let tw = f(&self.draft, "width", 1920.0) as f32;
        let th = f(&self.draft, "height", 1080.0) as f32;
        let avail = Vec2::new(ui.available_width(), (ui.available_height() - 92.0).max(200.0));
        let cw = avail.x.min(avail.y * tw / th);
        let (outer, _) = ui.allocate_exact_size(Vec2::new(avail.x, cw * th / tw), Sense::hover());
        let rect = Rect::from_center_size(outer.center(), Vec2::new(cw, cw * th / tw));
        let _ = paint::take_hits();
        let smp = sample(self.sample);
        paint_slide(ui, pc, rect, Some(&smp), &self.draft, "개역개정", true, false, false);
        let hits = paint::take_hits();
        ui.painter().rect_stroke(rect, 2.0, Stroke::new(1.0, p.line2), StrokeKind::Outside);
        self.canvas_seen = Some((rect, hits));
        self.canvas(ui, rect, hits);
        ui.add_space(10.0);
        // 쓰는 곳
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.label(theme::rich(tr("이 템플릿을 쓸 곳"), 12.5, Weight::Regular, p.muted));
            for (k, (slot, label, icon)) in SLOTS.iter().enumerate() {
                let using = active[k] == self.sel;
                let tip = if using { wp_core::i18n::tr_args("지금 {1}에 쓰고 있습니다", &[&tr(label)]) } else { wp_core::i18n::tr_args("누르면 {1}에 바로 이 템플릿을 씁니다", &[&tr(label)]) };
                let text = if using { format!("{} ✓", tr(label)) } else { tr(label) };
                if ui.add(Btn::new(text).icon(icon).size(Size::Sm).selected(using).tooltip(tip)).clicked() && !using {
                    self.flush(core);
                    core.lock().set_template(slot, &self.sel);
                    self.msg = wp_core::i18n::tr_args("{1}에 이 템플릿을 씁니다.", &[&tr(label)]);
                }
            }
        });
        ui.add_space(4.0);
        ui.label(theme::rich(tr("캔버스에서 장절·본문·로고를 누르면 오른쪽에 그 설정이 열립니다. 점선 테두리를 끌면 여백, 로고를 끌면 자리가 바뀝니다."), 11.5, Weight::Regular, p.muted2));
    }

    /// 캔버스 위 직접 조작: 여백 테두리 끌기 · 로고 끌기 · 누르면 속성 묶음 펼치기 · 안전 영역
    fn canvas(&mut self, ui: &mut egui::Ui, rect: Rect, hits: paint::Hits) {
        let p = pal();
        let guides = self.guides;
        let t = &mut self.draft;
        let tw = f(t, "width", 1920.0) as f32;
        let th = f(t, "height", 1080.0) as f32;
        let sc = rect.width() / tw;
        let safe = f(t, "safeArea", 0.0) as f32;
        let (sx, sy) = (tw * safe / 100.0, th * safe / 100.0);
        let pt = ui.painter_at(rect.expand(2.0));
        // 안전 영역: 템플릿에 정한 것이 있으면 그것, 없으면 화면 끝 5% 안내선
        if guides {
            let (gs, label) = if safe > 0.0 { (safe, tr("안전 영역")) } else { (5.0, tr("화면 끝 5% 안내선")) };
            let sr = rect.shrink2(Vec2::new(rect.width() * gs / 100.0, rect.height() * gs / 100.0));
            dashed_rect(&pt, sr, Stroke::new(1.0, alpha(p.warn, 0.55)));
            pt.text(sr.left_top() + Vec2::new(4.0, 3.0), Align2::LEFT_TOP, label, theme::font(10.5, Weight::Regular), alpha(p.warn, 0.85));
        }
        // 누르기: 배경 → 본문 → 장절 → 로고 → 여백 테두리 순으로 위에 쌓인다
        let mut clicked: Option<Sec> = None;
        let mut hover_box: Option<Rect> = None;
        let bg_resp = ui.interact(rect, ui.scope_id().with("tpl-canvas-bg"), Sense::click());
        if bg_resp.on_hover_text(tr("배경 — 누르면 배경 설정")).clicked() {
            clicked = Some(Sec::Bg);
        }
        for (r, sec, id, tip) in [(hits.body, Sec::Font, "tpl-canvas-body", "본문 — 누르면 글자 설정"), (hits.reference, Sec::Ref, "tpl-canvas-ref", "장절 — 누르면 장절 설정")] {
            if let Some(r) = r {
                let r = r.intersect(rect);
                let resp = ui.interact(r, ui.scope_id().with(id), Sense::click());
                if resp.hovered() {
                    hover_box = Some(r);
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if resp.on_hover_text(tr(tip)).clicked() {
                    clicked = Some(sec);
                }
            }
        }
        // 로고 끌기
        if let Some(lr) = hits.logo {
            let resp = ui.interact(lr, ui.scope_id().with("tpl-canvas-logo"), Sense::click_and_drag());
            if resp.hovered() || resp.dragged() {
                ui.ctx().set_cursor_icon(if resp.dragged() { egui::CursorIcon::Grabbing } else { egui::CursorIcon::Grab });
                hover_box = Some(lr);
            }
            if resp.drag_started() {
                if let Some(pp) = resp.interact_pointer_pos() {
                    self.drag = Some(Drag::Logo(pp - lr.min));
                }
            }
            if let (Some(Drag::Logo(off)), Some(pp)) = (self.drag, ui.ctx().pointer_latest_pos()) {
                // 놓일 자리: 가로 셋 · 세로 둘 가운데 가장 가까운 곳
                let c = pp - off + lr.size() / 2.0;
                let col = if c.x < rect.left() + rect.width() / 3.0 { "left" } else if c.x > rect.right() - rect.width() / 3.0 { "right" } else { "center" };
                let row = if c.y < rect.center().y { "top" } else { "bottom" };
                let zone = Rect::from_min_size(
                    Pos2::new(rect.left() + rect.width() / 3.0 * match col { "left" => 0.0, "center" => 1.0, _ => 2.0 }, if row == "top" { rect.top() } else { rect.center().y }),
                    Vec2::new(rect.width() / 3.0, rect.height() / 2.0),
                );
                pt.rect_filled(zone, 0.0, alpha(p.accent, 0.10));
                pt.text(zone.center(), Align2::CENTER_CENTER, tr("여기에 놓기"), theme::font(12.0, Weight::SemiBold), alpha(p.text, 0.85));
                let ghost = Rect::from_min_size(pp - off, lr.size());
                pt.rect(ghost, CornerRadius::same(4), alpha(p.accent, 0.25), Stroke::new(1.5, p.accent), StrokeKind::Inside);
                if resp.drag_stopped() {
                    set(t, "logo.position", json!(format!("{row}-{col}")));
                    self.drag = None;
                    clicked = Some(Sec::Logo);
                }
            }
            if resp.on_hover_text(tr("로고 — 끌어서 자리를 옮기거나 누르면 로고 설정")).clicked() {
                clicked = Some(Sec::Logo);
            }
        }
        // 글 영역 테두리 (여백) 끌기
        if let Some(area) = hits.area {
            let dragging_edge = matches!(self.drag, Some(Drag::Edge(_)));
            if guides || dragging_edge {
                dashed_rect(&pt, area, Stroke::new(1.0, alpha(p.accent, 0.85)));
            }
            let grip = 10.0;
            let edges = [
                (Edge::Top, Rect::from_min_max(Pos2::new(area.left() + grip, area.top() - grip / 2.0), Pos2::new(area.right() - grip, area.top() + grip / 2.0))),
                (Edge::Bottom, Rect::from_min_max(Pos2::new(area.left() + grip, area.bottom() - grip / 2.0), Pos2::new(area.right() - grip, area.bottom() + grip / 2.0))),
                (Edge::Left, Rect::from_min_max(Pos2::new(area.left() - grip / 2.0, area.top() + grip), Pos2::new(area.left() + grip / 2.0, area.bottom() - grip))),
                (Edge::Right, Rect::from_min_max(Pos2::new(area.right() - grip / 2.0, area.top() + grip), Pos2::new(area.right() + grip / 2.0, area.bottom() - grip))),
            ];
            for (e, er) in edges {
                let resp = ui.interact(er, ui.scope_id().with(("tpl-edge", e as u8)), Sense::drag());
                if resp.drag_started() {
                    self.drag = Some(Drag::Edge(e));
                }
                let active = self.drag == Some(Drag::Edge(e));
                if resp.hovered() || active {
                    ui.ctx().set_cursor_icon(if matches!(e, Edge::Top | Edge::Bottom) { egui::CursorIcon::ResizeVertical } else { egui::CursorIcon::ResizeHorizontal });
                    let line = match e {
                        Edge::Top => [area.left_top(), area.right_top()],
                        Edge::Bottom => [area.left_bottom(), area.right_bottom()],
                        Edge::Left => [area.left_top(), area.left_bottom()],
                        Edge::Right => [area.right_top(), area.right_bottom()],
                    };
                    pt.line_segment(line, Stroke::new(2.5, p.accent));
                }
                if guides || resp.hovered() || active {
                    pt.rect(Rect::from_center_size(er.center(), Vec2::splat(8.0)), CornerRadius::same(2), p.accent, Stroke::new(1.0, p.text), StrokeKind::Inside);
                }
                if active {
                    if let Some(pp) = ui.ctx().pointer_latest_pos() {
                        let (top, bottom, left, right) = (f(t, "padding.top", 120.0) as f32, f(t, "padding.bottom", 120.0) as f32, f(t, "padding.left", 160.0) as f32, f(t, "padding.right", 160.0) as f32);
                        let snap = |v: f32| (v / 5.0).round() * 5.0;
                        let min_gap = 80.0;
                        let (path, v) = match e {
                            Edge::Top => ("padding.top", snap((pp.y - rect.top()) / sc - sy).min(th - 2.0 * sy - bottom - min_gap)),
                            Edge::Bottom => ("padding.bottom", snap((rect.bottom() - pp.y) / sc - sy).min(th - 2.0 * sy - top - min_gap)),
                            Edge::Left => ("padding.left", snap((pp.x - rect.left()) / sc - sx).min(tw - 2.0 * sx - right - min_gap)),
                            Edge::Right => ("padding.right", snap((rect.right() - pp.x) / sc - sx).min(tw - 2.0 * sx - left - min_gap)),
                        };
                        let v = v.max(0.0);
                        if (f(t, path, 0.0) as f32 - v).abs() >= 1.0 {
                            set(t, path, json!(v.round() as i64));
                        }
                        let label = wp_core::i18n::tr_args("여백 {1}", &[&(v.round() as i64).to_string()]);
                        let g = pt.layout_no_wrap(label, theme::font(12.0, Weight::SemiBold), p.text);
                        let br = Rect::from_min_size(pp + Vec2::new(12.0, -26.0), g.size() + Vec2::new(10.0, 4.0));
                        pt.rect_filled(br, CornerRadius::same(4), alpha(p.panel, 0.92));
                        pt.galley(br.min + Vec2::new(5.0, 2.0), g, p.text);
                    }
                    if resp.drag_stopped() || !ui.ctx().input(|i| i.pointer.any_down()) {
                        self.drag = None;
                        clicked = Some(Sec::Layout);
                    }
                }
                resp.on_hover_text(tr("끌어서 여백 조절"));
            }
        }
        if let Some(r) = hover_box {
            if self.drag.is_none() {
                pt.rect_stroke(r.expand(3.0), CornerRadius::same(4), Stroke::new(1.5, alpha(p.hl, 0.9)), StrokeKind::Outside);
            }
        }
        if !ui.ctx().input(|i| i.pointer.any_down()) && matches!(self.drag, Some(Drag::Logo(_))) {
            self.drag = None;
        }
        if let Some(sec) = clicked {
            self.focus = Some(sec);
            self.last_focus = Some(sec);
        }
    }
}

#[cfg(test)]
#[path = "template_ui_tests.rs"]
mod tests;

