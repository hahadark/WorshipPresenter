//! 디자인 체계 부품 (2026-11-28 craft-main) — `widgets::…` 로 다시 내보낸다 (`use crate::widgets;` 하나로 씀).
//!
//! 규칙과 견본 그림은 docs/design-system.md, 모든 부품 · 모든 상태를 한 화면에 그리는 견본 창은 `wp-app --specimen`.
//!
//! 부품마다 두 겹이다:
//! - `paint_…(ui, rect, …, vis)` — 자리와 상태([`Vis`])를 받아 그리기만 한다 (견본 창이 상태를 강제로 그릴 때 · 직접 배치하는 화면 코드)
//! - `…(ui, …) -> Response` — 자리를 잡고, 실제 마우스 · 키보드 상태를 읽어 그린다
//!
//! 상태: 보통 · 올림(hover) · 눌림 · 초점 · 꺼짐 · 고름. 올림 · 눌림 · 고름 이동은 80~140ms 로 움직이고
//! (`theme::anim_bool`), 가만히 있으면 다시 그리지 않는다. 설정 '움직임 줄이기' 면 바로 바뀐다.
//!
//! 색 규칙: 빨강 = 송출 중에만 · 노랑 = 다음/고른 것에만 · 오류 = 주황 · 초점 = 밝은 무채색(`pal().focus`).
use crate::icons;
use crate::theme::{self, Ctl, Rad, Type, alpha, lerp, mix, pal, sp};
use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Galley, Id, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2};
use std::sync::Arc;

// ================================================================ 상태

/// 부품의 보이는 상태. `hover` · `press` 는 0~1 (움직이는 중이면 사이 값)
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vis {
    pub hover: f32,
    pub press: f32,
    /// 키보드 초점
    pub focus: bool,
    /// 고름 · 켜짐
    pub on: bool,
    /// 꺼짐 (누를 수 없음)
    pub off: bool,
}

impl Vis {
    pub const NORMAL: Vis = Vis { hover: 0.0, press: 0.0, focus: false, on: false, off: false };
    pub const HOVER: Vis = Vis { hover: 1.0, press: 0.0, focus: false, on: false, off: false };
    pub const PRESS: Vis = Vis { hover: 1.0, press: 1.0, focus: false, on: false, off: false };
    pub const FOCUS: Vis = Vis { hover: 0.0, press: 0.0, focus: true, on: false, off: false };
    pub const OFF: Vis = Vis { hover: 0.0, press: 0.0, focus: false, on: false, off: true };
    pub const ON: Vis = Vis { hover: 0.0, press: 0.0, focus: false, on: true, off: false };
    /// 견본 창의 여섯 상태 (이름, 상태)
    pub const ALL: [(&'static str, Vis); 6] = [("보통", Vis::NORMAL), ("올림", Vis::HOVER), ("눌림", Vis::PRESS), ("초점", Vis::FOCUS), ("꺼짐", Vis::OFF), ("고름", Vis::ON)];

    pub fn with_on(mut self, on: bool) -> Vis {
        self.on = on;
        self
    }

    /// 실제 반응에서 읽는다 (올림은 부드럽게, 눌림은 바로)
    pub fn of(ui: &Ui, resp: &Response, enabled: bool, on: bool) -> Vis {
        if !enabled {
            return Vis { off: true, on, ..Vis::NORMAL };
        }
        let down = resp.is_pointer_button_down_on();
        let hover = theme::anim_bool(ui.ctx(), resp.id.with("vis-h"), resp.hovered() || down, theme::T_FAST);
        let press = theme::anim_bool(ui.ctx(), resp.id.with("vis-p"), down, theme::T_FAST);
        Vis { hover, press, focus: resp.has_focus(), on, off: false }
    }
}

/// 키보드 초점 고리: 부품 바깥 1px 띄우고 2px (전문가 테마는 밝은 무채색, 옛 테마는 강조색)
pub fn focus_ring(ui: &Ui, rect: Rect, radius: u8) {
    let p = pal();
    ui.painter().rect_stroke(rect.expand(3.0), CornerRadius::same(radius.saturating_add(3)), Stroke::new(theme::LINE_EMPH, p.focus), StrokeKind::Inside);
}

/// 조절 칸 몸통: 바탕 + 가는 테두리 + 위 밝은 선(도드라짐) — 눌리면 어둡게 · 위 선 대신 안쪽 그늘.
/// `base` 는 보통 상태 바탕, `raised` 가 false 면 테두리 · 도드라짐 없이 바탕만 (조용한 단추)
pub fn paint_ctl(ui: &Ui, rect: Rect, base: Color32, vis: Vis, raised: bool) {
    let p = pal();
    let pt = ui.painter();
    let r = theme::cr(Rad::Md);
    let fill = if vis.off { theme::disabled(base, p.panel) } else { theme::state_fill(base, vis.hover, vis.press) };
    if fill.a() > 0 {
        pt.rect_filled(rect, r, fill);
    }
    if raised {
        if p.bevel {
            pt.rect_stroke(rect, r, Stroke::new(theme::LINE, if vis.off { alpha(p.ctl_line, 0.5) } else { p.ctl_line }), StrokeKind::Inside);
            let rad = theme::radius(Rad::Md) as f32;
            let x = rect.x_range().shrink(rad.min(rect.width() / 2.0 - 0.5));
            if !vis.off {
                // 위 1px: 보통은 밝은 선(도드라짐), 눌리면 어두운 선(들어감)
                let hi = lerp(alpha(Color32::WHITE, if p.dark { 0.09 } else { 0.55 }), Color32::from_black_alpha(if p.dark { 70 } else { 30 }), vis.press);
                pt.hline(x, rect.top() + 1.5, Stroke::new(1.0, hi));
            }
        } else {
            pt.rect_stroke(rect, r, Stroke::new(p.border_w, if vis.hover > 0.5 { p.line2 } else { p.line }), StrokeKind::Inside);
        }
    }
    if vis.focus && !vis.off {
        focus_ring(ui, rect, theme::radius(Rad::Md));
    }
}

fn layout(ui: &Ui, text: &str, font: FontId) -> Arc<Galley> {
    ui.painter().layout_no_wrap(text.to_string(), font, Color32::PLACEHOLDER)
}

/// 한 줄 글을 폭에 맞춰 줄인다 (넘치면 끝에 …). 돌려주는 것: (글, 줄였는가) — 줄였으면 툴팁으로 전문을 보일 것
pub fn elide(ui: &Ui, text: &str, font: FontId, color: Color32, max_w: f32) -> (Arc<Galley>, bool) {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_string(), font, color);
    job.wrap.max_width = max_w.max(8.0);
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job.wrap.overflow_character = Some('…');
    let g = ui.painter().layout_job(job);
    let cut = g.elided;
    (g, cut)
}

/// 폭에 맞춰 줄인 한 줄 글을 그린다. 줄였으면 true (부르는 쪽이 그 자리에 전문 툴팁을 단다)
pub fn text_elided(ui: &Ui, pos: Pos2, align: Align2, text: &str, font: FontId, color: Color32, max_w: f32) -> bool {
    let (g, cut) = elide(ui, text, font, color, max_w);
    let r = align.anchor_size(pos, g.size());
    ui.painter().galley(r.min, g, color);
    cut
}

// ================================================================ 숫자 폭 고정 글

/// 숫자 칸 폭이 같은 글 (tabular): 시각 · 분 · 쪽 수처럼 초마다 바뀌어도 흔들리지 않게.
/// Pretendard 의 숫자는 폭이 조금씩 다르다(1 이 좁다) → 숫자마다 가장 넓은 숫자 폭의 칸 가운데에 그린다.
pub struct Tnum {
    parts: Vec<(f32, Arc<Galley>)>,
    pub size: Vec2,
}

impl Tnum {
    pub fn new(ui: &Ui, text: &str, font: FontId, color: Color32) -> Tnum {
        let pt = ui.painter();
        let cell = "0123456789".chars().map(|c| pt.layout_no_wrap(c.to_string(), font.clone(), color).size().x).fold(0.0, f32::max).ceil();
        let (mut x, mut h) = (0.0_f32, 0.0_f32);
        let mut parts = Vec::with_capacity(text.chars().count());
        for ch in text.chars() {
            let g = pt.layout_no_wrap(ch.to_string(), font.clone(), color);
            let w = g.size().x;
            h = h.max(g.size().y);
            if ch.is_ascii_digit() {
                parts.push((x + (cell - w) / 2.0, g));
                x += cell;
            } else {
                parts.push((x, g));
                x += w;
            }
        }
        Tnum { parts, size: Vec2::new(x, h) }
    }

    /// `pos` 를 `align` 기준점으로 그린다. 그린 자리를 돌려준다
    pub fn paint(self, ui: &Ui, pos: Pos2, align: Align2, color: Color32) -> Rect {
        let r = align.anchor_size(pos, self.size);
        for (dx, g) in self.parts {
            ui.painter().galley(Pos2::new(r.left() + dx, r.top()), g, color);
        }
        r
    }
}

/// 숫자 폭 고정 글을 바로 그린다
pub fn tnum(ui: &Ui, pos: Pos2, align: Align2, text: &str, font: FontId, color: Color32) -> Rect {
    Tnum::new(ui, text, font, color.to_owned()).paint(ui, pos, align, color)
}

// ================================================================ 팝업 단추

/// 팝업 단추(고르기 칸) 그리기: 왼쪽 값 · 오른쪽 위아래 꺾쇠. `open` 이면 입력칸 바탕 + 초점 테두리
pub fn paint_popup(ui: &Ui, rect: Rect, text: &str, vis: Vis, open: bool) {
    let p = pal();
    let mut v = vis;
    if open {
        v.focus = false;
    }
    paint_ctl(ui, rect, if open { p.sunken } else { p.raised }, v, true);
    if open {
        ui.painter().rect_stroke(rect, theme::cr(Rad::Md), Stroke::new(theme::LINE, p.focus), StrokeKind::Inside);
    }
    let fg = if vis.off { theme::disabled(p.text, p.panel) } else { p.text };
    let chev = 16.0;
    let room = rect.width() - sp::X8 - chev - sp::X4;
    let small = rect.height() < theme::ctl(Ctl::Md) - 0.5;
    let font = if small { Type::Small.font() } else { Type::Body.font() };
    text_elided(ui, Pos2::new(rect.left() + sp::X8, rect.center().y), Align2::LEFT_CENTER, text, font, fg, room);
    // 위아래 꺾쇠 (팝업 단추 표시)
    let c = Pos2::new(rect.right() - sp::X4 - chev / 2.0, rect.center().y);
    let st = Stroke::new(1.3, if vis.off { theme::disabled(p.text2, p.panel) } else { p.text2 });
    let pt = ui.painter();
    pt.add(egui::Shape::line(vec![Pos2::new(c.x - 3.0, c.y - 1.75), Pos2::new(c.x, c.y - 4.75), Pos2::new(c.x + 3.0, c.y - 1.75)], st));
    pt.add(egui::Shape::line(vec![Pos2::new(c.x - 3.0, c.y + 1.75), Pos2::new(c.x, c.y + 4.75), Pos2::new(c.x + 3.0, c.y + 1.75)], st));
}

/// 팝업 단추: 지금 값 글을 보이고, 누르면 부르는 쪽이 `egui::Popup::menu(&resp)` 로 목록을 연다.
/// `width` 가 None 이면 남은 폭 전체
pub fn popup_btn(ui: &mut Ui, id: impl std::hash::Hash + std::fmt::Debug, text: &str, width: Option<f32>, size: Ctl) -> Response {
    let w = width.unwrap_or_else(|| ui.available_width());
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, theme::ctl(size)), Sense::hover());
    let resp = ui.interact(rect, Id::new(("kit-popup", ui.id(), id)), Sense::click());
    if ui.is_rect_visible(rect) {
        let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&resp));
        paint_popup(ui, rect, text, Vis::of(ui, &resp, true, false), open);
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// 고르기: 팝업 단추 + 목록 (값이 바뀌면 true). 목록 줄은 메뉴와 같은 모양
pub fn select<T: PartialEq + Clone>(ui: &mut Ui, id: impl std::hash::Hash + std::fmt::Debug, current: &mut T, items: &[(T, String)], width: Option<f32>, size: Ctl) -> bool {
    let shown = items.iter().find(|x| x.0 == *current).map(|x| x.1.as_str()).unwrap_or("");
    let resp = popup_btn(ui, id, shown, width, size);
    let mut changed = false;
    egui::Popup::menu(&resp).frame(crate::menubar::drop_frame()).gap(2.0).show(|ui| {
        ui.set_min_width(resp.rect.width().max(120.0) - 8.0);
        ui.spacing_mut().item_spacing.y = 0.0;
        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
            for (v, label) in items {
                if crate::menubar::item_row(ui, None, label, "", Some(*v == *current)).clicked() {
                    if *v != *current {
                        *current = v.clone();
                        changed = true;
                    }
                    ui.close();
                }
            }
        });
    });
    changed
}

// ================================================================ 체크 · 라디오 · 스위치

/// 체크 상자 그리기 (14×14). 켜지면 고른 회색 바탕 + 밝은 체크
pub fn paint_check(ui: &Ui, rect: Rect, on: bool, vis: Vis) {
    let p = pal();
    let pt = ui.painter();
    let r = CornerRadius::same(3);
    let ground = if on { if p.bevel { mix(p.text2, 78.0, p.panel) } else { p.accent } } else { p.sunken };
    let fill = if vis.off { theme::disabled(ground, p.panel) } else { theme::state_fill(ground, vis.hover * 0.6, vis.press) };
    pt.rect_filled(rect, r, fill);
    let line = if on { Color32::TRANSPARENT } else if vis.off { alpha(p.line2, 0.5) } else { lerp(p.line2, p.muted, vis.hover) };
    pt.rect_stroke(rect, r, Stroke::new(theme::LINE, line), StrokeKind::Inside);
    if on {
        let c = if vis.off { theme::disabled(p.panel, ground) } else if p.bevel { if p.dark { hex_dark() } else { Color32::WHITE } } else { Color32::WHITE };
        let o = rect.min;
        let s = rect.width() / 14.0;
        let pts = vec![Pos2::new(o.x + 3.5 * s, o.y + 7.25 * s), Pos2::new(o.x + 6.0 * s, o.y + 9.75 * s), Pos2::new(o.x + 10.5 * s, o.y + 4.5 * s)];
        pt.add(egui::Shape::line(pts, Stroke::new(1.6 * s, c)));
    }
    if vis.focus && !vis.off {
        focus_ring(ui, rect, 3);
    }
}

/// 어두운 전문가 테마에서 밝은 바탕 위 표시(체크 · 스위치 손잡이)의 진한 색
fn hex_dark() -> Color32 {
    Color32::from_rgb(0x1a, 0x1a, 0x1c)
}

/// 라디오 그리기 (14×14)
pub fn paint_radio(ui: &Ui, rect: Rect, on: bool, vis: Vis) {
    let p = pal();
    let pt = ui.painter();
    let c = rect.center();
    let rad = rect.width() / 2.0;
    let ground = if on { if p.bevel { mix(p.text2, 78.0, p.panel) } else { p.accent } } else { p.sunken };
    let fill = if vis.off { theme::disabled(ground, p.panel) } else { theme::state_fill(ground, vis.hover * 0.6, vis.press) };
    pt.circle_filled(c, rad, fill);
    if !on {
        pt.circle_stroke(c, rad - 0.5, Stroke::new(theme::LINE, if vis.off { alpha(p.line2, 0.5) } else { lerp(p.line2, p.muted, vis.hover) }));
    } else {
        let dot = if vis.off { theme::disabled(p.panel, ground) } else if p.bevel && p.dark { hex_dark() } else { Color32::WHITE };
        pt.circle_filled(c, rad * 0.4, dot);
    }
    if vis.focus && !vis.off {
        pt.circle_stroke(c, rad + 2.0, Stroke::new(theme::LINE_EMPH, p.focus));
    }
}

/// 스위치 그리기 (28×16). `t` 는 0(꺼짐)~1(켜짐) — 손잡이 자리
pub fn paint_switch(ui: &Ui, rect: Rect, t: f32, vis: Vis) {
    let p = pal();
    let pt = ui.painter();
    let r = CornerRadius::same((rect.height() / 2.0) as u8);
    let on_c = if p.bevel { mix(p.text2, 78.0, p.panel) } else { p.accent };
    let ground = lerp(p.sunken, on_c, t);
    let fill = if vis.off { theme::disabled(ground, p.panel) } else { theme::state_fill(ground, vis.hover * 0.5, vis.press) };
    pt.rect_filled(rect, r, fill);
    pt.rect_stroke(rect, r, Stroke::new(theme::LINE, lerp(if vis.off { alpha(p.line2, 0.5) } else { p.line2 }, Color32::TRANSPARENT, t)), StrokeKind::Inside);
    let kr = rect.height() / 2.0 - 2.0;
    let x = egui::lerp((rect.left() + 2.0 + kr)..=(rect.right() - 2.0 - kr), t);
    let knob = if vis.off { theme::disabled(p.text2, p.panel) } else { lerp(p.muted, if p.bevel && p.dark { hex_dark() } else { Color32::WHITE }, t) };
    pt.circle_filled(Pos2::new(x, rect.center().y), kr, knob);
    if vis.focus && !vis.off {
        pt.rect_stroke(rect.expand(3.0), CornerRadius::same((rect.height() / 2.0 + 3.0) as u8), Stroke::new(theme::LINE_EMPH, p.focus), StrokeKind::Inside);
    }
}

/// 표시(14) + 글 한 줄의 자리와 반응. (표시 자리, 반응)
fn mark_row(ui: &mut Ui, text: &str, mark_w: f32, enabled: bool) -> (Rect, Response) {
    let p = pal();
    let g = (!text.is_empty()).then(|| layout(ui, text, Type::Body.font()));
    let h = theme::ctl(Ctl::Sm).max(ui.spacing().interact_size.y.min(theme::ctl(Ctl::Md)));
    let w = mark_w + g.as_ref().map(|g| sp::X6 + g.size().x).unwrap_or(0.0);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), if enabled { Sense::click() } else { Sense::hover() });
    if let Some(g) = g {
        let fg = if enabled { p.text } else { theme::disabled(p.text, p.panel) };
        ui.painter().galley(Pos2::new(rect.left() + mark_w + sp::X6, rect.center().y - g.size().y / 2.0), g, fg);
    }
    if enabled && resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    (Rect::from_center_size(Pos2::new(rect.left() + mark_w / 2.0, rect.center().y), Vec2::new(mark_w, mark_w.min(16.0))), resp)
}

/// 체크 상자 + 글. 바뀌면 `changed()`
pub fn checkbox(ui: &mut Ui, on: &mut bool, text: &str) -> Response {
    checkbox_enabled(ui, on, text, true)
}

pub fn checkbox_enabled(ui: &mut Ui, on: &mut bool, text: &str, enabled: bool) -> Response {
    let (m, mut resp) = mark_row(ui, text, 14.0, enabled);
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    paint_check(ui, Rect::from_center_size(m.center(), Vec2::splat(14.0)), *on, Vis::of(ui, &resp, enabled, *on));
    resp
}

/// 라디오 + 글. 누르면 `clicked()` (값은 부르는 쪽이 바꾼다)
pub fn radio(ui: &mut Ui, selected: bool, text: &str) -> Response {
    let (m, resp) = mark_row(ui, text, 14.0, true);
    paint_radio(ui, Rect::from_center_size(m.center(), Vec2::splat(14.0)), selected, Vis::of(ui, &resp, true, selected));
    resp
}

/// 스위치 (+ 글). 바뀌면 `changed()`
pub fn switch(ui: &mut Ui, on: &mut bool, text: &str) -> Response {
    let (m, mut resp) = mark_row(ui, text, 28.0, true);
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let t = theme::anim_bool(ui.ctx(), resp.id.with("sw"), *on, theme::T_MED);
    paint_switch(ui, Rect::from_center_size(m.center(), Vec2::new(28.0, 16.0)), t, Vis::of(ui, &resp, true, *on));
    resp
}

// ================================================================ 슬라이더

/// 슬라이더 그리기: 4px 길 · 채운 쪽은 회색 · 14px 손잡이. `frac` 0~1
pub fn paint_slider(ui: &Ui, rect: Rect, frac: f32, vis: Vis) {
    let p = pal();
    let pt = ui.painter();
    let kr = 7.0;
    let y = rect.center().y;
    let (x0, x1) = (rect.left() + kr, rect.right() - kr);
    let x = egui::lerp(x0..=x1, frac.clamp(0.0, 1.0));
    let rail = Rect::from_min_max(Pos2::new(rect.left(), y - 2.0), Pos2::new(rect.right(), y + 2.0));
    pt.rect_filled(rail, theme::cr(Rad::Xs), if vis.off { theme::disabled(p.sunken, p.panel) } else { p.sunken });
    pt.rect_stroke(rail, theme::cr(Rad::Xs), Stroke::new(theme::LINE, alpha(p.ctl_line, if vis.off { 0.4 } else { 1.0 })), StrokeKind::Inside);
    let fill_c = if p.bevel { p.muted } else { p.accent };
    pt.rect_filled(Rect::from_min_max(rail.min, Pos2::new(x, rail.bottom())), theme::cr(Rad::Xs), if vis.off { theme::disabled(fill_c, p.panel) } else { fill_c });
    let knob = if p.bevel { if p.dark { mix(p.text, 92.0, p.panel) } else { Color32::WHITE } } else { p.text };
    let knob = if vis.off { theme::disabled(knob, p.panel) } else { theme::state_fill(knob, 0.0, vis.press * 0.6) };
    pt.circle_filled(Pos2::new(x, y), kr, knob);
    pt.circle_stroke(Pos2::new(x, y), kr - 0.5, Stroke::new(theme::LINE, if p.dark { alpha(Color32::BLACK, 0.55) } else { p.ctl_line }));
    if vis.hover > 0.01 && !vis.off {
        pt.circle_stroke(Pos2::new(x, y), kr + 1.5, Stroke::new(1.0, alpha(p.text, 0.28 * vis.hover)));
    }
    if vis.focus && !vis.off {
        pt.circle_stroke(Pos2::new(x, y), kr + 2.5, Stroke::new(theme::LINE_EMPH, p.focus));
    }
}

/// 슬라이더: 끌거나 눌러서 값, 초점이 있으면 ← → (한 걸음 `step`). 바뀌면 `changed()`, 놓으면 `drag_stopped()`
pub fn slider(ui: &mut Ui, value: &mut f64, range: std::ops::RangeInclusive<f64>, step: f64, width: f32) -> Response {
    let (lo, hi) = (*range.start(), *range.end());
    let (rect, mut resp) = ui.allocate_exact_size(Vec2::new(width, theme::ctl(Ctl::Sm)), Sense::click_and_drag());
    let snap = |v: f64| if step > 0.0 { (lo + ((v - lo) / step).round() * step).clamp(lo, hi) } else { v.clamp(lo, hi) };
    let mut v = *value;
    if resp.is_pointer_button_down_on() || resp.dragged() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let f = ((pos.x - rect.left() - 7.0) / (rect.width() - 14.0).max(1.0)).clamp(0.0, 1.0) as f64;
            v = snap(lo + f * (hi - lo));
        }
    }
    if resp.clicked() {
        resp.request_focus();
    }
    if resp.has_focus() {
        let st = if step > 0.0 { step } else { (hi - lo) / 100.0 };
        let d = ui.input(|i| i.key_pressed(egui::Key::ArrowRight) as i32 - i.key_pressed(egui::Key::ArrowLeft) as i32);
        if d != 0 {
            v = snap(v + d as f64 * st);
        }
    }
    if (v - *value).abs() > f64::EPSILON {
        *value = v;
        resp.mark_changed();
    }
    if ui.is_rect_visible(rect) {
        let f = if hi > lo { ((*value - lo) / (hi - lo)) as f32 } else { 0.0 };
        paint_slider(ui, rect, f, Vis::of(ui, &resp, true, false));
    }
    resp
}

// ================================================================ 입력칸

/// 입력칸 바탕 그리기: 들어간 바탕 + 가는 테두리. 올리면 테두리 밝게, 초점이면 초점색 테두리
pub fn paint_field(ui: &Ui, rect: Rect, vis: Vis) {
    let p = pal();
    let pt = ui.painter();
    let r = theme::cr(Rad::Md);
    pt.rect_filled(rect, r, if vis.off { theme::disabled(p.sunken, p.panel) } else { p.sunken });
    let line = if vis.off { alpha(p.ctl_line, 0.5) } else if vis.focus { p.focus } else { lerp(if p.bevel { p.ctl_line } else { p.line }, p.line2, vis.hover) };
    pt.rect_stroke(rect, r, Stroke::new(theme::LINE, line), StrokeKind::Inside);
    if p.bevel && p.dark && !vis.focus && !vis.off {
        // 들어간 느낌: 안쪽 위 1px 그늘
        let rad = theme::radius(Rad::Md) as f32;
        pt.hline(rect.x_range().shrink(rad), rect.top() + 1.5, Stroke::new(1.0, Color32::from_black_alpha(90)));
    }
}

/// 한 줄 입력칸 (보통 높이). `icon` 을 주면 앞에 아이콘(찾기칸). 바뀌면 `changed()`
pub fn text_field(ui: &mut Ui, text: &mut String, hint: &str, width: f32, icon: Option<&str>) -> Response {
    text_field_sized(ui, text, hint, width, icon, Ctl::Md, true)
}

pub fn text_field_sized(ui: &mut Ui, text: &mut String, hint: &str, width: f32, icon: Option<&str>, size: Ctl, enabled: bool) -> Response {
    let p = pal();
    let h = theme::ctl(size);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, h), Sense::hover());
    let bg = ui.painter().add(egui::Shape::Noop);
    let lead = if icon.is_some() { sp::X8 + 14.0 + sp::X6 } else { sp::X8 };
    let inner = Rect::from_min_max(Pos2::new(rect.left() + lead, rect.top() + 1.0), Pos2::new(rect.right() - sp::X8, rect.bottom() - 1.0));
    let font = if size == Ctl::Sm { Type::Small.font() } else { Type::Body.font() };
    let resp = ui
        .scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
            ui.add_enabled(
                enabled,
                egui::TextEdit::singleline(text)
                    .frame(egui::Frame::NONE)
                    .margin(egui::Margin::ZERO)
                    .hint_text(egui::RichText::new(hint).color(p.muted2).font(font.clone()))
                    .font(font)
                    .text_color(p.text)
                    .vertical_align(egui::Align::Center)
                    .desired_width(inner.width()),
            )
        })
        .inner;
    let hover = theme::anim_bool(ui.ctx(), resp.id.with("vis-h"), ui.rect_contains_pointer(rect) && enabled, theme::T_FAST);
    let vis = Vis { hover, press: 0.0, focus: resp.has_focus(), on: false, off: !enabled };
    // 바탕은 글보다 아래에 (자리 먼저 잡아 둔 Noop 에 넣는다)
    let mut shapes = vec![];
    {
        let r = theme::cr(Rad::Md);
        shapes.push(egui::Shape::rect_filled(rect, r, if vis.off { theme::disabled(p.sunken, p.panel) } else { p.sunken }));
        let line = if vis.off { alpha(p.ctl_line, 0.5) } else if vis.focus { p.focus } else { lerp(if p.bevel { p.ctl_line } else { p.line }, p.line2, vis.hover) };
        shapes.push(egui::Shape::rect_stroke(rect, r, Stroke::new(theme::LINE, line), StrokeKind::Inside));
        if p.bevel && p.dark && !vis.focus && !vis.off {
            let rad = theme::radius(Rad::Md) as f32;
            shapes.push(egui::Shape::hline(rect.x_range().shrink(rad), rect.top() + 1.5, Stroke::new(1.0, Color32::from_black_alpha(90))));
        }
    }
    ui.painter().set(bg, egui::Shape::Vec(shapes));
    if let Some(ic) = icon {
        icons::paint(ui, Rect::from_center_size(Pos2::new(rect.left() + sp::X8 + 7.0, rect.center().y), Vec2::splat(14.0)), ic, if vis.off { theme::disabled(p.muted, p.panel) } else { p.muted });
    }
    resp
}

// ================================================================ 표 · 목록 줄

/// 줄의 뜻 (송출 중 · 다음이 고름보다 먼저)
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RowVis {
    pub hover: f32,
    /// 고른 줄 (키보드 · 편집 대상)
    pub selected: bool,
    /// 송출 중
    pub live: bool,
    /// 다음 (미리보기에 올라 있음)
    pub next: bool,
    /// 지나간 줄
    pub done: bool,
    /// 이 줄에 키보드 초점
    pub focus: bool,
}

/// 줄 글 색 (첫 글, 둘째 글, 아이콘)
pub struct RowInk {
    pub text: Color32,
    pub sub: Color32,
    pub icon: Color32,
    /// 첫 글 굵게
    pub strong: bool,
}

/// 표/목록 줄 바탕을 그리고 글 색을 돌려준다.
/// 송출 중 = 어두운 빨강 바탕 + 왼쪽 3px 빨강 · 다음 = 노랑 기운 바탕 + 왼쪽 3px 노랑 · 고름 = 회색 바탕 · 올림 = 살짝 밝게 · 지나감 = 흐린 글
pub fn paint_row(ui: &Ui, rect: Rect, v: RowVis) -> RowInk {
    let p = pal();
    let pt = ui.painter();
    let strong = theme::strong_text();
    let bar = |c: Color32| pt.rect_filled(Rect::from_min_max(rect.min, Pos2::new(rect.left() + 3.0, rect.bottom())), 0.0, c);
    let ink = if v.live {
        pt.rect_filled(rect, 0.0, lerp(p.live_dk, theme::lift(p.live_dk), v.hover * 0.5));
        bar(p.live);
        RowInk { text: strong, sub: if p.dark { mix(strong, 72.0, p.live_dk) } else { p.text2 }, icon: strong, strong: true }
    } else if v.next {
        pt.rect_filled(rect, 0.0, lerp(p.next_bg, theme::lift(p.next_bg), v.hover * 0.5));
        bar(p.next);
        RowInk { text: strong, sub: if p.dark { mix(strong, 72.0, p.next_bg) } else { p.text2 }, icon: strong, strong: true }
    } else if v.selected {
        let base = if p.bevel { mix(p.sel, 55.0, p.panel) } else { p.sel };
        pt.rect_filled(rect, 0.0, lerp(base, theme::lift(base), v.hover * 0.5));
        RowInk { text: p.text, sub: p.text2, icon: p.text, strong: true }
    } else {
        if v.hover > 0.01 {
            pt.rect_filled(rect, 0.0, alpha(p.hover, v.hover));
        }
        if v.done {
            RowInk { text: p.muted, sub: p.muted2, icon: p.muted2, strong: false }
        } else {
            RowInk { text: p.text, sub: p.muted, icon: p.muted, strong: false }
        }
    };
    if v.focus {
        pt.rect_stroke(rect.shrink(0.5), 0.0, Stroke::new(theme::LINE, p.focus), StrokeKind::Inside);
    }
    ink
}

// ================================================================ 탭 (밑줄)

/// 밑줄 탭 한 줄 (창 안의 쪽 바꾸기). 고른 탭 아래 2px 선이 옆으로 미끄러진다. 바뀌면 `changed()`
pub fn tabs<T: PartialEq + Clone>(ui: &mut Ui, current: &mut T, items: &[(T, &str)]) -> Response {
    let p = pal();
    let h = theme::HEAD_H;
    let gal: Vec<Arc<Galley>> = items.iter().map(|(_, t)| layout(ui, t, Type::BodyStrong.font())).collect();
    let widths: Vec<f32> = gal.iter().map(|g| g.size().x + 2.0 * sp::X12).collect();
    let (rect, mut resp) = ui.allocate_exact_size(Vec2::new(ui.available_width().max(widths.iter().sum()), h), Sense::hover());
    ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(theme::LINE, p.line));
    let mut x = rect.left();
    let mut sel = None;
    for (i, ((val, _), g)) in items.iter().zip(gal).enumerate() {
        let r = Rect::from_min_size(Pos2::new(x, rect.top()), Vec2::new(widths[i], h));
        x += widths[i];
        let cr = ui.interact(r, resp.id.with(i), Sense::click());
        if cr.clicked() && *current != *val {
            *current = val.clone();
            resp.mark_changed();
        }
        let on = *current == *val;
        let hov = theme::anim_bool(ui.ctx(), cr.id.with("h"), cr.hovered(), theme::T_FAST);
        if cr.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let fg = if on { p.text } else { lerp(p.muted, p.text2, hov) };
        ui.painter().galley(r.center() - g.size() / 2.0, g, fg);
        if cr.has_focus() {
            focus_ring(ui, r.shrink2(Vec2::new(4.0, 5.0)), theme::radius(Rad::Md));
        }
        if on {
            sel = Some(r);
        }
    }
    if let Some(r) = sel {
        let ax = theme::anim_value(ui.ctx(), resp.id.with("ux"), r.left() + sp::X8, theme::T_MED);
        let aw = theme::anim_value(ui.ctx(), resp.id.with("uw"), r.width() - 2.0 * sp::X8, theme::T_MED);
        ui.painter().rect_filled(Rect::from_min_size(Pos2::new(ax, rect.bottom() - 2.0), Vec2::new(aw, 2.0)), 0.0, if p.bevel { p.text } else { p.accent });
    }
    resp
}

// ================================================================ 툴팁 (+ 단축키)

/// 툴팁: 설명 글 + 오른쪽에 단축키 칩. 단축키가 비면 글만
pub fn tip(resp: Response, text: &str, kbd: &str) -> Response {
    if kbd.is_empty() {
        return resp.on_hover_text(text);
    }
    let (text, kbd) = (text.to_string(), kbd.to_string());
    resp.on_hover_ui(move |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = sp::X8;
            ui.label(text.as_str());
            crate::widgets::kbd(ui, &kbd);
        });
    })
}

/// "설명 (단축키)" 꼴 글에서 맨 끝 괄호의 단축키를 떼어 [`tip`] 으로 단다 (괄호 안이 단축키 꼴이 아니면 글 그대로)
pub fn tip_auto(resp: Response, text: &str) -> Response {
    match split_kbd(text) {
        Some((t, k)) => tip(resp, t, k),
        None => resp.on_hover_text(text),
    }
}

/// "이전 슬라이드 (F5)" → ("이전 슬라이드", "F5"). 단축키 꼴(F1~F12 · Ctrl/Alt/Shift+… · PageUp 등 한 낱말)일 때만
pub fn split_kbd(text: &str) -> Option<(&str, &str)> {
    let t = text.trim_end();
    if !t.ends_with(')') {
        return None;
    }
    let open = t.rfind('(')?;
    let k = &t[open + 1..t.len() - 1];
    let is_key = |s: &str| {
        let s = s.trim();
        let fkey = s.len() >= 2 && s.len() <= 3 && s.starts_with('F') && s[1..].chars().all(|c| c.is_ascii_digit());
        fkey || s.contains('+') || matches!(s, "PageUp" | "PageDown" | "Space" | "Enter" | "Esc" | "Delete" | "Home" | "End" | "Tab")
    };
    is_key(k).then(|| (t[..open].trim_end(), k))
}

// ================================================================ 판 사이 손잡이

/// 판 사이 손잡이: 가는 선 하나(판 사이 어두운 선). 올리면 가운데 밝은 선이 나타나고 끌 수 있다.
/// `vertical_bar` = 세로 띠(좌우로 끎). 돌려주는 반응의 `dragged()` · `interact_pointer_pos()` 를 쓴다
pub fn splitter(ui: &Ui, rect: Rect, id: Id, vertical_bar: bool) -> Response {
    let p = pal();
    let resp = ui.interact(rect, id, Sense::click_and_drag());
    let hot = resp.hovered() || resp.dragged();
    let t = theme::anim_bool(ui.ctx(), id.with("hot"), hot, theme::T_FAST);
    let pt = ui.painter();
    pt.rect_filled(rect, 0.0, p.edge);
    if t > 0.01 {
        let c = alpha(p.text2, 0.55 * t);
        if vertical_bar {
            pt.vline(rect.center().x, rect.y_range(), Stroke::new(theme::LINE_EMPH, c));
        } else {
            pt.hline(rect.x_range(), rect.center().y, Stroke::new(theme::LINE_EMPH, c));
        }
    }
    // 가운데 잡는 자리 표시 (점 셋)
    let c = rect.center();
    let dot = lerp(p.muted2, p.text, t);
    for k in -1..=1 {
        let d = k as f32 * 4.0;
        let q = if vertical_bar { Pos2::new(c.x, c.y + d) } else { Pos2::new(c.x + d, c.y) };
        pt.circle_filled(q, 1.0, dot);
    }
    if hot {
        ui.ctx().set_cursor_icon(if vertical_bar { egui::CursorIcon::ResizeHorizontal } else { egui::CursorIcon::ResizeVertical });
    }
    resp
}

// ================================================================ 알림 띠

/// 알림 종류. 색은 왼쪽 3px 띠와 아이콘에만 (바탕은 판 색에 살짝) — 오류는 주황, 빨강은 쓰지 않는다
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Info,
    Ok,
    Warn,
    Error,
}

impl Tone {
    pub fn from_kind(kind: &str) -> Tone {
        match kind {
            "error" => Tone::Error,
            "warn" => Tone::Warn,
            "ok" => Tone::Ok,
            _ => Tone::Info,
        }
    }
    /// (띠 · 아이콘 색, 바탕, 아이콘 이름)
    pub fn colors(self) -> (Color32, Color32, &'static str) {
        let p = pal();
        match self {
            Tone::Info => (p.muted, mix(p.line2, 30.0, p.head), "info"),
            Tone::Ok => (p.text2, mix(p.line2, 30.0, p.head), "check"),
            Tone::Warn => (p.warn, mix(p.warn, 16.0, p.head), "flag"),
            Tone::Error => (p.danger_text, if p.bevel { p.danger_bg } else { p.live_bg }, "circle-x"),
        }
    }
}

/// 알림 띠가 돌려주는 것
#[derive(Default)]
pub struct NoticeOut {
    pub action: bool,
    pub closed: bool,
    pub rect: Option<Rect>,
}

/// 알림 띠 한 줄 (판 안 · 창 안): 왼쪽 색 띠 + 아이콘 + 글(길면 줄바꿈) + [할 일] + 닫기
pub fn notice(ui: &mut Ui, tone: Tone, text: &str, action: Option<&str>, closable: bool) -> NoticeOut {
    let p = pal();
    let (col, bg, icon) = tone.colors();
    let mut out = NoticeOut::default();
    let fr = egui::Frame::new().fill(bg).corner_radius(theme::cr(Rad::Md)).inner_margin(egui::Margin { left: 12, right: 8, top: 5, bottom: 5 }).show(ui, |ui| {
        // 위 맞춤: 글이 두 줄이 되어도 아이콘 · 단추는 첫 줄에 (한 줄 높이 = 작은 단추 22)
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = sp::X8;
            let (slot, _) = ui.allocate_exact_size(Vec2::new(14.0, theme::ctl(Ctl::Sm)), Sense::hover());
            icons::paint(ui, Rect::from_center_size(slot.center(), Vec2::splat(14.0)), icon, col);
            let side = action.map(|a| layout(ui, a, Type::Small.font()).size().x + 2.0 * sp::X8 + sp::X8).unwrap_or(0.0) + if closable { 22.0 + sp::X8 } else { 0.0 };
            let w = (ui.available_width() - side).max(60.0);
            ui.allocate_ui_with_layout(Vec2::new(w, 0.0), egui::Layout::top_down(egui::Align::LEFT), |ui| {
                ui.set_max_width(w);
                ui.add_space((theme::ctl(Ctl::Sm) - Type::Body.line()) / 2.0);
                ui.add(egui::Label::new(theme::ty(text, Type::Body, p.text)).wrap().selectable(false));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                if closable && ui.add(crate::widgets::Btn::icon_only("x").kind(crate::widgets::Kind::Ghost).size(crate::widgets::Size::Sm).tooltip(wp_core::i18n::tr("닫기"))).clicked() {
                    out.closed = true;
                }
                if let Some(a) = action {
                    if ui.add(crate::widgets::Btn::new(a).size(crate::widgets::Size::Sm)).clicked() {
                        out.action = true;
                    }
                }
            });
        });
    });
    let r = fr.response.rect;
    ui.painter().rect_filled(Rect::from_min_max(r.min, Pos2::new(r.left() + 3.0, r.bottom())), CornerRadius { nw: theme::radius(Rad::Md), sw: theme::radius(Rad::Md), ne: 0, se: 0 }, col);
    out.rect = Some(r);
    out
}

/// 넘치는 줄의 끝을 바탕색으로 흐리게 (24px). `left` 면 왼쪽 끝
pub fn fade_edge(ui: &Ui, rect: Rect, left: bool, ground: Color32) {
    let w = 24.0_f32.min(rect.width() / 4.0);
    let (x0, x1) = if left { (rect.left(), rect.left() + w) } else { (rect.right() - w, rect.right()) };
    let (c0, c1) = if left { (ground, Color32::TRANSPARENT) } else { (Color32::TRANSPARENT, ground) };
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(Pos2::new(x0, rect.top()), c0);
    mesh.colored_vertex(Pos2::new(x1, rect.top()), c1);
    mesh.colored_vertex(Pos2::new(x1, rect.bottom()), c1);
    mesh.colored_vertex(Pos2::new(x0, rect.bottom()), c0);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    ui.painter().add(mesh);
}

// ================================================================ 빈 상태

/// 빈 상태: 흐린 아이콘 + 한 줄 제목 + 안내 글 + (할 일 단추). 판 가운데가 아니라 위쪽에 왼쪽 맞춤으로 둔다
/// (가운데 큰 그림과 문장은 '웹 페이지' 처럼 보인다). 단추를 눌렀으면 true
pub fn empty(ui: &mut Ui, icon: &str, title: &str, hint: &str, action: Option<&str>) -> bool {
    let p = pal();
    let mut clicked = false;
    egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 12, top: 16, bottom: 12 }).show(ui, |ui| {
        ui.spacing_mut().item_spacing = Vec2::new(sp::X8, sp::X6);
        ui.horizontal(|ui| {
            icons::show(ui, icon, 16.0, p.muted2);
            ui.add(egui::Label::new(theme::ty(title, Type::BodyStrong, p.text2)).selectable(false));
        });
        if !hint.is_empty() {
            ui.add(egui::Label::new(theme::ty(hint, Type::Small, p.muted)).wrap().selectable(false));
        }
        if let Some(a) = action {
            ui.add_space(sp::X2);
            clicked = ui.add(crate::widgets::Btn::new(a)).clicked();
        }
    });
    clicked
}

// ================================================================ 진행 표시

/// 진행 막대 (4px). `frac` 이 None 이면 얼마나 걸릴지 모르는 일 — 막대가 좌우로 오간다 (보이는 동안만 다시 그림)
pub fn progress(ui: &mut Ui, frac: Option<f32>, width: f32) -> Response {
    let p = pal();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 4.0), Sense::hover());
    if ui.is_rect_visible(rect) {
        paint_progress(ui, rect, frac, ui.input(|i| i.time));
        if frac.is_none() && !theme::reduce_motion() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
        }
    }
    let _ = p;
    resp
}

/// 진행 막대 그리기 (`time` 은 모르는 일의 오가는 자리 — 견본 창은 고정 값을 준다)
pub fn paint_progress(ui: &Ui, rect: Rect, frac: Option<f32>, time: f64) {
    let p = pal();
    let pt = ui.painter();
    pt.rect_filled(rect, theme::cr(Rad::Xs), p.sunken);
    let c = if p.bevel { p.text2 } else { p.accent };
    match frac {
        Some(f) => {
            let w = rect.width() * f.clamp(0.0, 1.0);
            if w > 0.5 {
                pt.rect_filled(Rect::from_min_size(rect.min, Vec2::new(w, rect.height())), theme::cr(Rad::Xs), c);
            }
        }
        None => {
            let seg = rect.width() * 0.3;
            let t = if theme::reduce_motion() { 0.35 } else { (time * 0.9).fract() as f32 };
            let x = rect.left() - seg + (rect.width() + seg) * t;
            let r = Rect::from_min_max(Pos2::new(x.max(rect.left()), rect.top()), Pos2::new((x + seg).min(rect.right()), rect.bottom()));
            if r.width() > 0.5 {
                pt.rect_filled(r, theme::cr(Rad::Xs), c);
            }
        }
    }
}

/// 작은 돌림 표시 (14px) + 글 — '불러오는 중…'. 보이는 동안만 다시 그린다
pub fn loading(ui: &mut Ui, text: &str) {
    let p = pal();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = sp::X8;
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
        if ui.is_rect_visible(rect) {
            paint_spinner(ui, rect, ui.input(|i| i.time));
            if !theme::reduce_motion() {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
            }
        }
        if !text.is_empty() {
            ui.add(egui::Label::new(theme::ty(text, Type::Small, p.muted)).selectable(false));
        }
    });
}

/// 돌림 표시 그리기: 흐린 고리 위에 밝은 호 1/4
pub fn paint_spinner(ui: &Ui, rect: Rect, time: f64) {
    let p = pal();
    let pt = ui.painter();
    let c = rect.center();
    let r = rect.width() / 2.0 - 1.5;
    pt.circle_stroke(c, r, Stroke::new(1.5, p.line2));
    let a0 = if theme::reduce_motion() { -std::f32::consts::FRAC_PI_2 } else { (time * 5.0) as f32 % std::f32::consts::TAU };
    let pts: Vec<Pos2> = (0..=8).map(|i| { let a = a0 + i as f32 / 8.0 * std::f32::consts::FRAC_PI_2; Pos2::new(c.x + r * a.cos(), c.y + r * a.sin()) }).collect();
    pt.add(egui::Shape::line(pts, Stroke::new(1.5, p.text)));
}

// ================================================================ 구획 머리 (접는 것)

/// 접는 구획 머리 (인스펙터): 높이 [`theme::SECTION_H`], 판 끝까지 띠, ▸ 가 돌아 ▾, 굵은 작은 글. 펼쳐져 있으면 true.
/// 상태는 egui 임시 자료(`id`)에 둔다. 초점이 있으면 Enter · Space 로 접고 편다
pub fn section_head(ui: &mut Ui, id: Id, title: &str, default_open: bool) -> bool {
    let mut open = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(default_open);
    let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), theme::SECTION_H), Sense::click());
    // 띠는 판 끝까지 (안쪽 여백을 넘어) 그린다
    let clip = ui.clip_rect();
    let band = Rect::from_min_max(Pos2::new(clip.left().max(r.left() - sp::PANE), r.top()), Pos2::new(clip.right(), r.bottom()));
    let hov = theme::anim_bool(ui.ctx(), resp.id.with("h"), resp.hovered(), theme::T_FAST);
    let t = theme::anim_bool(ui.ctx(), id.with("open-t"), open, theme::T_MED);
    paint_section_head(ui, band, title, t, hov, resp.has_focus());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.clicked() {
        open = !open;
        ui.data_mut(|d| d.insert_temp(id, open));
    }
    open
}

/// 접는 구획 머리 그리기 (`open_t` 0 = 접힘 ▸ · 1 = 펼침 ▾)
pub fn paint_section_head(ui: &Ui, band: Rect, title: &str, open_t: f32, hover: f32, focus: bool) {
    let p = pal();
    let pt = ui.painter().with_clip_rect(ui.clip_rect());
    let base = mix(p.head, 70.0, p.panel);
    pt.rect_filled(band, 0.0, lerp(base, p.hover, hover));
    pt.hline(band.x_range(), band.top() + 0.5, Stroke::new(theme::LINE, p.line));
    pt.hline(band.x_range(), band.bottom() - 0.5, Stroke::new(theme::LINE, p.edge));
    // 꺾쇠: ▸ (접힘) 에서 90도 돌아 ▾ (펼침)
    let c = Pos2::new(band.left() + sp::PANE + 3.0, band.center().y);
    let a = open_t * std::f32::consts::FRAC_PI_2;
    let rot = |x: f32, y: f32| Pos2::new(c.x + x * a.cos() - y * a.sin(), c.y + x * a.sin() + y * a.cos());
    pt.add(egui::Shape::line(vec![rot(-1.5, -3.5), rot(2.0, 0.0), rot(-1.5, 3.5)], Stroke::new(1.5, lerp(p.muted, p.text2, hover))));
    pt.text(Pos2::new(band.left() + sp::PANE + 14.0, band.center().y), Align2::LEFT_CENTER, title, Type::SmallStrong.font(), p.text);
    if focus {
        pt.rect_stroke(band.shrink(1.0), 0.0, Stroke::new(theme::LINE, p.focus), StrokeKind::Inside);
    }
}

/// 촘촘한 칸(인스펙터)에서 egui 기본 조절 칸(고르기 · 숫자 · 단추)을 작게(22)로
pub fn dense(ui: &mut Ui) {
    let h = theme::ctl(Ctl::Sm);
    let s = ui.spacing_mut();
    s.interact_size.y = h;
    s.button_padding.y = 2.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_kbd_takes_only_shortcuts() {
        assert_eq!(split_kbd("이전 슬라이드 (F5)"), Some(("이전 슬라이드", "F5")));
        assert_eq!(split_kbd("예배당 화면 켜기 (Ctrl+Shift+F)"), Some(("예배당 화면 켜기", "Ctrl+Shift+F")));
        assert_eq!(split_kbd("미리보기 다음 슬라이드 (PageDown)"), Some(("미리보기 다음 슬라이드", "PageDown")));
        // 설명 괄호는 단축키가 아니다
        assert_eq!(split_kbd("모두 지우기 (큐시트 순서는 그대로)"), None);
        assert_eq!(split_kbd("슬라이드 지우기"), None);
        assert_eq!(split_kbd("배치 (판 보이기)"), None);
    }

    #[test]
    fn tone_from_kind() {
        assert_eq!(Tone::from_kind("error"), Tone::Error);
        assert_eq!(Tone::from_kind("warn"), Tone::Warn);
        assert_eq!(Tone::from_kind("ok"), Tone::Ok);
        assert_eq!(Tone::from_kind("info"), Tone::Info);
        assert_eq!(Tone::from_kind(""), Tone::Info);
    }
}
