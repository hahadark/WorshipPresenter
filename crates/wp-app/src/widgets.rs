//! 공용 위젯 — Electron 조작 화면(control.css)의 단추·분할 단추·배지·kbd 칩·패널 탭 머리를 egui 로 직접 그린다.
//! 색은 모두 `theme::pal()` 에서 가져오므로 조작 화면 테마를 바꾸면 같이 바뀐다.
//!
//! 쓰는 법 (다음 작업자용):
//! ```ignore
//! use crate::widgets::{Btn, Kind, Size};
//! if ui.add(Btn::new("이전").icon("skip-back").kbd("F5").size(Size::Big)).clicked() { … }
//! if ui.add(Btn::new("송출").icon("play").kbd("F9").kind(Kind::Send).size(Size::Big)).clicked() { … }
//! ui.add(Btn::new("모두 지우기").kind(Kind::Danger).size(Size::Tall));      // 빨간 테두리
//! ui.add(Btn::new("미리보기 끔 · 바로 송출").kind(Kind::Warn));             // 노랑
//! ui.add(Btn::new("슬라이드 편집").kind(Kind::Primary));                    // 파랑(accent)
//! ui.add(Btn::icon_only("settings").tooltip("설정"));                       // 34×34 아이콘 단추
//! ui.add(Btn::new("그리기").icon("brush").selected(on));                    // button.on (accent 테두리)
//! widgets::segmented(ui, &mut view, &[(0, "목록"), (1, "격자")]);           // view-tabs (작은 분할 단추)
//! widgets::segmented_wide(ui, &mut tab, &[(0, "큐시트 (예배 순서)"), (1, "성경 책")]); // side-tabs (가로 꽉)
//! widgets::badge(ui, "LIVE", Badge::Live); widgets::kbd(ui, "Ctrl+T");
//! let r = widgets::panel_tabs(ui, "center", &["본문 · 편집"], &mut active); // 패널 탭 머리 (34px)
//! widgets::card_frame().show(ui, |ui| …);                                   // 독 패널 카드
//! widgets::group_frame().show(ui, |ui| …);                                  // ft-group 묶음 테두리
//! ```
#![allow(dead_code)] // 다음 단계 작업자가 쓸 공개 API (아직 안 쓰는 것이 있음)

use crate::icons;
use crate::theme::{self, Weight, alpha, mix, pal};
use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Galley, Margin, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2, Widget};
use std::sync::Arc;

/// 단추 종류 (css 클래스)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Kind {
    /// button: panel2 바탕 · line 테두리
    #[default]
    Default,
    /// button.primary: accent 바탕 · 흰 글자 · 600
    Primary,
    /// .big.send: live 바탕 · 흰 글자 · 700 (송출)
    Send,
    /// button.danger (모두 지우기): live-line 테두리, hover 때 live-bg
    Danger,
    /// .studio.off / .clear-group .active: warn-bg 바탕 · warn 테두리 · 700
    Warn,
    /// .studio.on: pv-bg 바탕 · pv 테두리 · ok-text 글자
    Pv,
    /// button.ghost: 테두리·바탕 없음, hover 때 hover 바탕
    Ghost,
}

/// 단추 크기
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Size {
    /// 독 탭 머리 단추·칩: 높이 24, 글자 13, 여백 7
    Sm,
    /// 기본(--ctl-h): 높이 34, 글자 14, 여백 12
    #[default]
    Md,
    /// 송출 줄 .clear-group 단추: 높이 40, 글자 14, 여백 12, kbd 10px
    Tall,
    /// 송출 줄 .big: 높이 40, 글자 15, 600, 여백 16, kbd 11px
    Big,
}

impl Size {
    /// (높이, 글자 크기, 좌우 여백, 아이콘 크기, kbd 글자 크기, kbd 앞 간격)
    fn metrics(self) -> (f32, f32, f32, f32, f32, f32) {
        match self {
            Size::Sm => (24.0, 13.0, 7.0, 14.0, 10.0, 4.0),
            Size::Md => (theme::CTL_H, 14.0, 12.0, 16.0, 11.0, 4.0),
            Size::Tall => (40.0, 14.0, 12.0, 16.0, 10.0, 4.0),
            Size::Big => (40.0, 15.0, 16.0, 18.0, 11.0, 6.0),
        }
    }
}

/// 아이콘 + 글 + kbd 칩 단추. `ui.add(Btn::new(…))`
#[must_use = "ui.add(…) 로 그려야 합니다"]
pub struct Btn {
    text: String,
    icon: Option<String>,
    icon_end: Option<String>,
    kbd: Option<String>,
    kind: Kind,
    size: Size,
    selected: bool,
    enabled: bool,
    weight: Option<Weight>,
    min_width: f32,
    height: Option<f32>,
    square: bool,
    tooltip: Option<String>,
    text_color: Option<Color32>,
    icon_color: Option<Color32>,
    caret: bool,
}

impl Btn {
    pub fn new(text: impl Into<String>) -> Self {
        Btn { text: text.into(), icon: None, icon_end: None, kbd: None, kind: Kind::Default, size: Size::Md, selected: false, enabled: true, weight: None, min_width: 0.0, height: None, square: false, tooltip: None, text_color: None, icon_color: None, caret: false }
    }
    /// 아이콘만 있는 정사각형 단추 (.icon-btn: 34×34, 아이콘 18)
    pub fn icon_only(name: &str) -> Self {
        let mut b = Btn::new("");
        b.icon = Some(name.into());
        b.square = true;
        b
    }
    /// 글 앞 아이콘 (Lucide 이름)
    pub fn icon(mut self, name: &str) -> Self {
        self.icon = Some(name.into());
        self
    }
    /// 글 뒤 아이콘 (예: 다음 ▷|)
    pub fn icon_end(mut self, name: &str) -> Self {
        self.icon_end = Some(name.into());
        self
    }
    /// 글 뒤 작은 ▾ (펼침 메뉴 단추: 지우기 ▾)
    pub fn caret(mut self) -> Self {
        self.caret = true;
        self
    }
    /// 오른쪽 kbd 칩 (단축키)
    pub fn kbd(mut self, k: impl Into<String>) -> Self {
        let k = k.into();
        self.kbd = (!k.is_empty()).then_some(k);
        self
    }
    pub fn kind(mut self, k: Kind) -> Self {
        self.kind = k;
        self
    }
    pub fn size(mut self, s: Size) -> Self {
        self.size = s;
        self
    }
    /// button.on (accent 테두리)
    pub fn selected(mut self, on: bool) -> Self {
        self.selected = on;
        self
    }
    pub fn enabled(mut self, on: bool) -> Self {
        self.enabled = on;
        self
    }
    /// 글자 굵기 (기본: 종류·크기에 따라)
    pub fn weight(mut self, w: Weight) -> Self {
        self.weight = Some(w);
        self
    }
    pub fn min_width(mut self, w: f32) -> Self {
        self.min_width = w;
        self
    }
    pub fn height(mut self, h: f32) -> Self {
        self.height = Some(h);
        self
    }
    pub fn tooltip(mut self, t: impl Into<String>) -> Self {
        self.tooltip = Some(t.into());
        self
    }
    pub fn text_color(mut self, c: Color32) -> Self {
        self.text_color = Some(c);
        self
    }
    pub fn icon_color(mut self, c: Color32) -> Self {
        self.icon_color = Some(c);
        self
    }
}

/// 단추 상태별 색 (바탕, 테두리, 글자)
fn colors(kind: Kind, hovered: bool, selected: bool) -> (Color32, Color32, Color32) {
    let p = pal();
    let white85 = |c: Color32| mix(c, 85.0, Color32::WHITE);
    let (bg, border, fg) = match kind {
        Kind::Default => if hovered { (p.hover, p.line2, p.text) } else { (p.panel2, p.line, p.text) },
        Kind::Primary => if hovered { (white85(p.accent), white85(p.accent), Color32::WHITE) } else { (p.accent, p.accent, Color32::WHITE) },
        Kind::Send => if hovered { (white85(p.live), white85(p.live), Color32::WHITE) } else { (p.live, p.live, Color32::WHITE) },
        Kind::Danger => if hovered { (p.live_bg, p.live_line, p.text) } else { (p.panel2, p.live_line, p.text) },
        Kind::Warn => (p.warn_bg, p.warn, p.text),
        Kind::Pv => (p.pv_bg, p.pv, p.ok_text),
        Kind::Ghost => if hovered { (p.hover, Color32::TRANSPARENT, p.text) } else { (Color32::TRANSPARENT, Color32::TRANSPARENT, p.text) },
    };
    let border = if selected && matches!(kind, Kind::Default | Kind::Ghost) { p.accent } else { border };
    (bg, border, fg)
}

fn layout(ui: &Ui, text: &str, font: FontId) -> Arc<Galley> {
    ui.painter().layout_no_wrap(text.to_string(), font, Color32::PLACEHOLDER)
}

/// kbd 칩 크기 (글 크기 → 칩 크기)
fn kbd_size(g: &Galley) -> Vec2 {
    Vec2::new(g.size().x + 8.0, g.size().y.max(14.0) + 2.0)
}

/// kbd 칩을 rect 에 그린다 (css kbd: muted 글자, line 테두리, 둥글기 4, 여백 0 4px)
fn paint_kbd(ui: &Ui, rect: Rect, g: Arc<Galley>, fg: Color32, border: Color32) {
    let pt = ui.painter();
    pt.rect_stroke(rect, CornerRadius::same(4), Stroke::new(1.0, border), StrokeKind::Inside);
    pt.galley(rect.center() - g.size() / 2.0, g, fg);
}

impl Widget for Btn {
    fn ui(self, ui: &mut Ui) -> Response {
        let p = pal();
        let (h0, fs, pad, ic, kfs, kgap) = self.size.metrics();
        let h = self.height.unwrap_or(h0);
        let ic = if self.square && self.size == Size::Md { 18.0 } else { ic };
        let weight = self.weight.unwrap_or(match (self.kind, self.size) {
            (Kind::Send, _) | (Kind::Warn, _) => Weight::Bold,
            (Kind::Primary, _) | (_, Size::Big) => Weight::SemiBold,
            _ => Weight::Regular,
        });
        let tg = (!self.text.is_empty()).then(|| layout(ui, &self.text, theme::font(fs, weight)));
        let kg = self.kbd.as_deref().map(|k| layout(ui, k, theme::font(kfs, weight)) /* kbd { font: inherit } */);
        // css: button { gap: 2px } · .big { gap: 4px } · 앞 아이콘 margin-right .4em (.big 은 margin-left 2px 도)
        //      .ic-end { margin-left: .3em } · kbd { margin-left: 4px } (.big kbd 6px) · .big.send { padding-right: 14px }
        let big = self.size == Size::Big;
        let fgap = if big { 4.0 } else { 2.0 };
        let (ic_l, ic_r) = if big { (2.0, 0.4 * fs) } else { (0.0, 0.4 * fs) };
        let (end_l, end_r) = if big { (2.0, 2.0) } else { (0.3 * fs, 0.0) };
        // 그릴 조각: (너비, 앞 여백) — 조각 사이에는 flex gap
        let mut parts: Vec<f32> = vec![];
        if self.icon.is_some() {
            parts.push(ic_l + ic + if tg.is_some() { ic_r } else { 0.0 });
        }
        if let Some(g) = &tg {
            parts.push(g.size().x);
        }
        if self.caret {
            parts.push(0.3 * fs + 7.0);
        }
        if self.icon_end.is_some() {
            parts.push(end_l + ic + end_r);
        }
        if let Some(g) = &kg {
            parts.push(kgap + kbd_size(g).x);
        }
        let w: f32 = parts.iter().sum::<f32>() + fgap * parts.len().saturating_sub(1) as f32;
        let (pad_l, pad_r) = if self.kind == Kind::Send && big { (pad, 14.0) } else { (pad, pad) };        let size = if self.square { Vec2::splat(h) } else { Vec2::new((w + pad_l + pad_r).max(self.min_width), h) };
        let sense = if self.enabled { Sense::click() } else { Sense::hover() };
        let (rect, mut resp) = ui.allocate_exact_size(size, sense);
        if ui.is_rect_visible(rect) {
            let hovered = self.enabled && resp.hovered();
            let (mut bg, mut border, mut fg) = colors(self.kind, hovered, self.selected);
            if let Some(c) = self.text_color {
                fg = c;
            }
            let mut icol = self.icon_color.unwrap_or(fg);
            if !self.enabled {
                for c in [&mut bg, &mut border, &mut fg, &mut icol] {
                    *c = c.gamma_multiply(0.5);
                }
            }
            // 누를 때 0.5px 내려감 (button:active)
            let r = if resp.is_pointer_button_down_on() { rect.translate(Vec2::new(0.0, 0.5)) } else { rect };
            let pt = ui.painter();
            pt.rect(r, CornerRadius::same(theme::RADIUS), bg, Stroke::new(p.border_w, border), StrokeKind::Inside);
            if resp.has_focus() {
                // :focus-visible { outline: 2px solid accent; outline-offset: 1px }
                pt.rect_stroke(r.expand(3.0), CornerRadius::same(theme::RADIUS + 3), Stroke::new(2.0, p.accent), StrokeKind::Inside);
            }
            let mut x = if self.square { r.center().x - ic / 2.0 } else { r.left() + pad_l + ((r.width() - pad_l - pad_r) - w).max(0.0) / 2.0 };
            let cy = r.center().y;
            if let Some(n) = &self.icon {
                x += ic_l;
                icons::paint(ui, Rect::from_min_size(Pos2::new(x, cy - ic / 2.0), Vec2::splat(ic)), n, icol);
                x += ic + if tg.is_some() { ic_r + fgap } else { 0.0 };
            }
            if let Some(g) = tg {
                let gw = g.size().x;
                pt.galley(Pos2::new(x, cy - g.size().y / 2.0), g, fg);
                x += gw + fgap;
            }
            if self.caret {
                // ▾ (작은 아래 삼각형, 가운데 높이)
                x += 0.3 * fs;
                let c = Pos2::new(x + 3.5, cy + 0.5);
                pt.add(egui::Shape::convex_polygon(vec![Pos2::new(c.x - 3.5, c.y - 2.0), Pos2::new(c.x + 3.5, c.y - 2.0), Pos2::new(c.x, c.y + 2.5)], fg, Stroke::NONE));
                x += 7.0 + fgap;
            }
            if let Some(n) = &self.icon_end {
                x += end_l;
                icons::paint(ui, Rect::from_min_size(Pos2::new(x, cy - ic / 2.0), Vec2::splat(ic)), n, icol);
                x += ic + end_r + fgap;
            }            if let Some(g) = kg {
                x += kgap;
                let ks = kbd_size(&g);
                let (kf, kb) = if self.kind == Kind::Send { (alpha(Color32::WHITE, 0.8), alpha(Color32::WHITE, 0.4)) } else { (p.muted, p.line) };
                let kf = if self.enabled { kf } else { kf.gamma_multiply(0.5) };
                paint_kbd(ui, Rect::from_min_size(Pos2::new(x, cy - ks.y / 2.0), ks), g, kf, kb);
            }
        }
        if self.enabled && resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if let Some(t) = self.tooltip {
            resp = resp.on_hover_text(t);
        }
        resp
    }
}

/// 아이콘만 있는 단추 (34×34). 툴팁 필수로 받는다
pub fn icon_btn(ui: &mut Ui, name: &str, tooltip: &str) -> Response {
    ui.add(Btn::icon_only(name).tooltip(tooltip))
}

/// 혼자 쓰는 kbd 칩 (css kbd, 11px)
pub fn kbd(ui: &mut Ui, k: &str) -> Response {
    let p = pal();
    let g = layout(ui, k, theme::font(11.0, Weight::Regular));
    let (rect, resp) = ui.allocate_exact_size(kbd_size(&g), Sense::hover());
    paint_kbd(ui, rect, g, p.muted, p.line);
    resp
}

// ---------------------------------------------------------------- 배지

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Badge {
    /// .badge.onair — live 바탕, 빛 번짐
    Live,
    /// .badge.pv — pv 바탕
    Pv,
}

/// LIVE · 미리보기 배지 (css .badge: 11.5px 700, 글자 간격 .5px, 여백 2px 9px, 둥글기 4)
pub fn badge(ui: &mut Ui, text: &str, kind: Badge) -> Response {
    let p = pal();
    let mut job = egui::text::LayoutJob::default();
    job.append(text, 0.0, egui::TextFormat { font_id: theme::font(11.5, Weight::Bold), color: Color32::WHITE, extra_letter_spacing: 0.5, ..Default::default() });
    let g = ui.painter().layout_job(job);
    let size = Vec2::new(g.size().x + 18.0, g.size().y + 4.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    let pt = ui.painter();
    let bg = match kind {
        Badge::Live => p.live,
        Badge::Pv => p.pv,
    };
    if kind == Badge::Live {
        // box-shadow: 0 0 0 1px live 50%, 0 0 10px live 45%
        for i in 1..=5 {
            let a = 0.45 * (1.0 - i as f32 / 6.0) * 0.5;
            pt.rect_filled(rect.expand(i as f32 * 2.0), CornerRadius::same(4 + i as u8 * 2), alpha(p.live, a));
        }
        pt.rect_stroke(rect.expand(1.0), CornerRadius::same(5), Stroke::new(1.0, alpha(p.live, 0.5)), StrokeKind::Inside);
    }
    pt.rect_filled(rect, CornerRadius::same(4), bg);
    pt.galley(rect.center() - g.size() / 2.0, g, Color32::WHITE);
    resp
}

// ---------------------------------------------------------------- 분할 단추

/// 분할 단추 모양
#[derive(Clone, Copy, Debug)]
pub struct SegStyle {
    /// 글자 크기
    pub font: f32,
    /// 칸 안 여백 (가로, 세로)
    pub pad: Vec2,
    /// 가로를 꽉 채워 칸을 똑같이 나눔 (.side-tabs)
    pub fill: bool,
    /// 바깥 둥글기
    pub radius: u8,
}

impl SegStyle {
    /// .view-tabs: 12px, 여백 2px 8px, 둥글기 4
    pub const COMPACT: SegStyle = SegStyle { font: 12.0, pad: Vec2::new(8.0, 2.0), fill: false, radius: 4 };
    /// .side-tabs: 13px, 여백 5px 4px, 가로 꽉, 둥글기 6
    pub const WIDE: SegStyle = SegStyle { font: 13.0, pad: Vec2::new(4.0, 5.0), fill: true, radius: 6 };
}

/// 작은 분할 단추 (view-tabs). 바뀌면 resp.changed() == true
pub fn segmented<T: PartialEq + Clone>(ui: &mut Ui, current: &mut T, items: &[(T, &str)]) -> Response {
    segmented_with(ui, current, items, SegStyle::COMPACT)
}

/// 가로 꽉 찬 분할 단추 (side-tabs: '큐시트 (예배 순서) | 성경 책')
pub fn segmented_wide<T: PartialEq + Clone>(ui: &mut Ui, current: &mut T, items: &[(T, &str)]) -> Response {
    segmented_with(ui, current, items, SegStyle::WIDE)
}

pub fn segmented_with<T: PartialEq + Clone>(ui: &mut Ui, current: &mut T, items: &[(T, &str)], st: SegStyle) -> Response {
    let p = pal();
    let gal: Vec<(Arc<Galley>, Arc<Galley>)> = items
        .iter()
        .map(|(_, t)| (layout(ui, t, theme::font(st.font, Weight::Regular)), layout(ui, t, theme::font(st.font, Weight::SemiBold))))
        .collect();
    let h = gal.iter().map(|g| g.1.size().y).fold(0.0, f32::max) + 2.0 * st.pad.y + 2.0;
    let widths: Vec<f32> = gal.iter().map(|g| g.1.size().x + 2.0 * st.pad.x + 2.0).collect();
    let total = if st.fill { ui.available_width() } else { widths.iter().sum::<f32>() - (items.len().saturating_sub(1)) as f32 };
    let (rect, mut resp) = ui.allocate_exact_size(Vec2::new(total, h), Sense::hover());
    let n = items.len();
    let mut x = rect.left();
    let mut sel_rect = None;
    for (i, ((val, _), (g_reg, g_bold))) in items.iter().zip(gal).enumerate() {
        let w = if st.fill { (total + (n - 1) as f32) / n as f32 } else { widths[i] };
        let r = Rect::from_min_size(Pos2::new(x, rect.top()), Vec2::new(w, h));
        x += w - 1.0; // + button { margin-left: -1px }
        let id = resp.id.with(i);
        let cr = ui.interact(r, id, Sense::click());
        if cr.clicked() && *current != *val {
            *current = val.clone();
            resp.mark_changed();
        }
        if cr.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let on = *current == *val;
        let rr = st.radius;
        let corner = match (i == 0, i + 1 == n) {
            (true, true) => CornerRadius::same(rr),
            (true, false) => CornerRadius { nw: rr, sw: rr, ne: 0, se: 0 },
            (false, true) => CornerRadius { nw: 0, sw: 0, ne: rr, se: rr },
            _ => CornerRadius::ZERO,
        };
        let (bg, border) = if on { (p.sel, p.line2) } else if cr.hovered() { (p.hover, p.line2) } else { (p.panel2, p.line) };
        let pt = ui.painter();
        pt.rect(r, corner, bg, Stroke::new(p.border_w, border), StrokeKind::Inside);
        if on {
            sel_rect = Some((r, corner));
        }
        let mut g = if on { g_bold } else { g_reg };
        // 칸보다 글이 넓으면(영어 화면의 긴 이름) 잘리지 않게 글자를 줄인다
        let room = (w - 8.0).max(8.0);
        if g.size().x > room {
            let size = (st.font * room / g.size().x).max(8.0);
            g = layout(ui, items[i].1, theme::font(size, if on { Weight::SemiBold } else { Weight::Regular }));
        }
        let pt = ui.painter();
        pt.galley(r.center() - g.size() / 2.0, g, p.text);
    }
    // 고른 칸 테두리가 옆 칸 위로 오게 (z-index: 1)
    if let Some((r, c)) = sel_rect {
        ui.painter().rect_stroke(r, c, Stroke::new(p.border_w, p.line2), StrokeKind::Inside);
    }
    resp
}

// ---------------------------------------------------------------- 패널 탭 머리

/// 패널 탭 머리에서 일어난 일
#[derive(Debug)]
pub struct TabHead {
    /// 누른 탭 (이미 active 로 바뀜)
    pub clicked: Option<usize>,
    /// ✕ 를 누른 탭
    pub closed: Option<usize>,
    /// 오른쪽 ▾ (접기)
    pub fold: bool,
    /// 오른쪽 ⧉ (띄우기)
    pub popout: bool,
    /// 머리 줄 전체 사각형
    pub rect: Rect,
    /// 탭마다 Response (도킹: 끌기·오른쪽 클릭 메뉴용, 순서는 tabs 와 같음)
    pub tabs: Vec<Response>,
    /// 오른쪽 ▾ 단추 Response (펼침 메뉴를 붙일 때: `egui::Popup::menu(r)`)
    pub fold_resp: Option<Response>,
}

/// 탭 머리 높이 (--dv-tabs-and-actions-container-height)
pub const TAB_H: f32 = 34.0;

/// 독 패널 탭 머리: 제목 + ✕(활성 탭), 활성 탭 아래 accent 밑줄, 오른쪽 ▾ · ⧉ 단추, 아래 line-soft 선.
/// 가로는 ui 가 가진 너비 전체를 쓴다.
pub fn panel_tabs(ui: &mut Ui, id: &str, tabs: &[&str], active: &mut usize) -> TabHead {
    panel_tabs_ex(ui, id, tabs, active, true, true)
}

/// closable: 활성 탭에 ✕ · actions: 오른쪽 ▾ ⧉ 단추
pub fn panel_tabs_ex(ui: &mut Ui, id: &str, tabs: &[&str], active: &mut usize, closable: bool, actions: bool) -> TabHead {
    panel_tabs_dock(ui, id, tabs, active, closable, actions, false)
}

/// 도킹용: draggable 이면 탭을 끌 수 있다(Sense::click_and_drag). ▾ 단추는 fold_resp 로 메뉴를 붙인다
pub fn panel_tabs_dock(ui: &mut Ui, id: &str, tabs: &[&str], active: &mut usize, closable: bool, actions: bool, draggable: bool) -> TabHead {
    let p = pal();
    let id = egui::Id::new(("panel-tabs", id));
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), TAB_H), Sense::hover());
    let mut out = TabHead { clicked: None, closed: None, fold: false, popout: false, rect, tabs: vec![], fold_resp: None };
    ui.painter().rect_filled(rect, 0.0, p.panel);
    ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, p.line_soft));
    let mut x = rect.left() + 4.0; // padding: 0 4px
    let font = theme::font(13.0, Weight::SemiBold);
    for (i, t) in tabs.iter().enumerate() {
        let on = i == *active;
        let g = layout(ui, t, font.clone());
        let xw = if on && closable { 6.0 + 14.0 } else { 0.0 };
        let w = 8.0 + g.size().x + xw + 8.0;
        let r = Rect::from_min_size(Pos2::new(x, rect.top()), Vec2::new(w, TAB_H - 1.0));
        x += w;
        let resp = ui.interact(r, id.with(i), if draggable { Sense::click_and_drag() } else { Sense::click() });
        let close_r = Rect::from_center_size(Pos2::new(r.right() - 8.0 - 7.0, r.center().y), Vec2::splat(18.0));
        let close = (on && closable).then(|| ui.interact(close_r, id.with(("x", i)), Sense::click()));
        if close.as_ref().is_some_and(|c| c.clicked()) {
            out.closed = Some(i);
        } else if resp.clicked() {
            *active = i;
            out.clicked = Some(i);
        }
        let pt = ui.painter();
        let fg = if on { p.text } else if resp.hovered() { p.text2 } else { p.muted };
        pt.galley(Pos2::new(r.left() + 8.0, r.center().y - g.size().y / 2.0), g, fg);
        if let Some(c) = close {
            if c.hovered() {
                pt.rect_filled(close_r, CornerRadius::same(4), p.hover);
            }
            icons::paint(ui, close_r.shrink(2.0), "x", if c.hovered() { p.text } else { p.text2 });
        }
        if on {
            // ::after { left: 8px; right: 8px; bottom: 0; height: 2px; radius 2px 2px 0 0; accent }
            let u = Rect::from_min_max(Pos2::new(r.left() + 8.0, rect.bottom() - 2.0), Pos2::new(r.right() - 8.0, rect.bottom()));
            pt.rect_filled(u, CornerRadius { nw: 2, ne: 2, sw: 0, se: 0 }, p.accent);
        }
        out.tabs.push(resp);
    }
    if actions {
        // .dock-actions button { padding: 0 7px; font-size: 13px; line-height: 20px } → 약 26×22
        let bh = 22.0;
        let bw = 26.0;
        let mut rx = rect.right() - 4.0;
        for (k, which) in [(1, "pop"), (0, "fold")] {
            let r = Rect::from_min_size(Pos2::new(rx - bw, rect.center().y - bh / 2.0), Vec2::new(bw, bh));
            rx -= bw + 4.0;
            let resp = ui.interact(r, id.with(which), Sense::click());
            let (bg, border, fg) = colors(Kind::Default, resp.hovered(), false);
            let pt = ui.painter();
            pt.rect(r, CornerRadius::same(theme::RADIUS), bg, Stroke::new(p.border_w, border), StrokeKind::Inside);
            let c = r.center();
            if k == 0 {
                // ▾
                let s = 3.0;
                pt.add(egui::Shape::convex_polygon(vec![Pos2::new(c.x - s, c.y - s * 0.6), Pos2::new(c.x + s, c.y - s * 0.6), Pos2::new(c.x, c.y + s * 0.7)], fg, Stroke::NONE));
                out.fold |= resp.clicked();
                out.fold_resp = Some(resp); // 툴팁은 접을 수 있는지에 따라 workspace_ui 가 단다
            } else {
                // ⧉ 두 겹 네모
                let a = Rect::from_min_size(c + Vec2::new(-6.0, -6.0), Vec2::splat(8.0));
                let b = Rect::from_min_size(c + Vec2::new(-2.0, -2.0), Vec2::splat(8.0));
                pt.rect_stroke(a, CornerRadius::same(1), Stroke::new(1.2, fg), StrokeKind::Middle);
                pt.rect_stroke(b, CornerRadius::same(1), Stroke::new(1.2, fg), StrokeKind::Middle);
                out.popout |= resp.clicked();
                resp.on_hover_text(wp_core::i18n::tr("떠 있는 창으로 띄우기"));
            }
        }
    }
    out
}

// ---------------------------------------------------------------- 틀 · 구분선 · 상태 점

/// 독 패널 카드 (panel 바탕, 둥글기 12, line-soft 테두리)
pub fn card_frame() -> egui::Frame {
    let p = pal();
    egui::Frame::new().fill(p.panel).corner_radius(CornerRadius::same(theme::CARD_RADIUS)).stroke(Stroke::new(1.0, p.line_soft))
}

/// .ft-group 묶음 (line 테두리, 둥글기 8, 안쪽 3px) — 송출 줄 '글자 숨김 · 지우기 · 모두 지우기'
pub fn group_frame() -> egui::Frame {
    let p = pal();
    egui::Frame::new().stroke(Stroke::new(1.0, p.line)).corner_radius(CornerRadius::same(8)).inner_margin(Margin::same(3))
}

/// 세로 구분선 (.sep: 1px line, 높이 h)
pub fn vsep(ui: &mut Ui, h: f32) {
    let (r, _) = ui.allocate_exact_size(Vec2::new(9.0, h), Sense::hover());
    ui.painter().vline(r.center().x, r.y_range(), Stroke::new(1.0, pal().line));
}

/// 출력 연결 상태 점 + 글 (.status .st: 12px muted, 9px 점)
pub fn status_dot(ui: &mut Ui, color: Color32, text: &str) -> Response {
    let p = pal();
    let g = layout(ui, text, theme::font(12.0, Weight::Regular));
    let (r, resp) = ui.allocate_exact_size(Vec2::new(9.0 + 5.0 + g.size().x, g.size().y.max(9.0)), Sense::hover());
    ui.painter().circle_filled(Pos2::new(r.left() + 4.5, r.center().y), 4.5, color);
    ui.painter().galley(Pos2::new(r.left() + 14.0, r.center().y - g.size().y / 2.0), g, p.muted);
    resp
}

/// 눌러서 여는 상태 점 (아래 상태 줄). `dot_only` 면 점만 그리고 글은 툴팁으로.
/// 마우스를 올리면 hover 바탕 · 손 모양 커서.
pub fn status_btn(ui: &mut Ui, color: Color32, text: &str, dot_only: bool, tip: &str) -> Response {
    let p = pal();
    let g = layout(ui, text, theme::font(12.0, Weight::Regular));
    let w = if dot_only { 9.0 } else { 9.0 + 5.0 + g.size().x };
    let (r, resp) = ui.allocate_exact_size(Vec2::new(w, g.size().y.max(9.0)), Sense::click());
    let hov = resp.hovered();
    if hov {
        ui.painter().rect_filled(r.expand2(Vec2::new(4.0, 3.0)), CornerRadius::same(4), p.hover);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    ui.painter().circle_filled(Pos2::new(r.left() + 4.5, r.center().y), 4.5, color);
    if !dot_only {
        ui.painter().galley(Pos2::new(r.left() + 14.0, r.center().y - g.size().y / 2.0), g, if hov { p.text } else { p.muted });
    }
    let tip = if dot_only { format!("{text} — {tip}") } else { tip.to_string() };
    resp.on_hover_text(tip)
}

/// 상태 항목을 중요한 순서(앞에서부터)로 자리에 맞춘다: 먼저 모두 점만, 자리가 남으면 중요한 것부터 글까지.
/// 점조차 모자라면 덜 중요한(뒤) 항목부터 뺀다. 돌려주는 것: 항목마다 None=안 보임 · Some(false)=점만 · Some(true)=점+글
pub fn fit_status(text_w: &[f32], avail: f32, gap: f32) -> Vec<Option<bool>> {
    const DOT: f32 = 9.0;
    let n = text_w.len();
    let mut out = vec![None; n];
    let mut used = 0.0;
    for o in out.iter_mut() {
        let add = DOT + if used > 0.0 { gap } else { 0.0 };
        if used + add > avail {
            break;
        }
        used += add;
        *o = Some(false);
    }
    for (i, o) in out.iter_mut().enumerate() {
        if *o == Some(false) && used + 5.0 + text_w[i] <= avail {
            used += 5.0 + text_w[i];
            *o = Some(true);
        }
    }
    out
}

/// 흐린 글 (css .muted)
pub fn muted(ui: &mut Ui, text: &str) -> Response {
    ui.label(egui::RichText::new(text).color(pal().muted))
}

/// 글을 가운데 맞춰 그린다 (직접 그리는 위젯용)
pub fn text_at(ui: &Ui, pos: Pos2, align: Align2, text: &str, size: f32, w: Weight, color: Color32) -> Rect {
    ui.painter().text(pos, align, text, theme::font(size, w), color)
}

// ---------------- 마우스 올림 아이콘 단추 (docs/ux-rules.md 3\uc808 '\uc378\ub124\uc77c\u00b7\ubaa9\ub85d') ----------------

/// 썸네일·목록 줄에 마우스를 올리면 뜨는 아이콘 단추 하나
#[derive(Clone, Debug)]
pub struct HoverOp {
    /// 동작 이름 (edit · add · dup · skip · del · up · down · send · cue · bg …)
    pub op: &'static str,
    /// Lucide 아이콘 이름
    pub icon: &'static str,
    /// 툴팁 (필수)
    pub tip: String,
    /// 마우스를 올리면 빨갛게 (지우기)
    pub danger: bool,
    /// 오른쪽 위 모서리에 둔다 (✎ 편집처럼). 아니면 왼쪽 위
    pub right: bool,
}

impl HoverOp {
    pub fn new(op: &'static str, icon: &'static str, tip: impl Into<String>) -> Self {
        Self { op, icon, tip: tip.into(), danger: false, right: false }
    }
    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }
    pub fn right(mut self) -> Self {
        self.right = true;
        self
    }
}

/// 마우스 올림 단추 크기 (피츠의 법칙: 24px 이상)
pub const HOVER_BTN: f32 = 26.0;

#[cfg(test)]
thread_local! {
    /// 합성 입력 시험용: 이번 프레임에 그린 마우스 올림 단추 자리 (동작 이름 → 자리, 마지막에 그린 것)
    pub(crate) static HOVER_SEEN: std::cell::RefCell<std::collections::BTreeMap<String, Rect>> = Default::default();
}

/// 클립보드에 복사하고 알림을 남긴다 (모든 복사 지점 공용). 알림은 다음에 App 이 `take_copied` 로 꺼내 띄운다.
/// core 를 받지 않는 메뉴·창에서도 쓸 수 있게 egui 임시 자료에 둔다
pub fn copy_notice(ctx: &egui::Context, text: impl Into<String>, msg: impl Into<String>) {
    ctx.copy_text(text.into());
    let m: String = msg.into();
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("wp-copied"), m));
}

/// `copy_notice` 가 남긴 알림 글 (한 번 꺼내면 지워짐)
pub fn take_copied(ctx: &egui::Context) -> Option<String> {
    ctx.data_mut(|d| d.remove_temp::<String>(egui::Id::new("wp-copied")))
}

/// 한 번·두 번 클릭 판정 결과 (`click2`)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Click {
    None,
    /// 첫 클릭 (또는 두 번 클릭 간격을 넘긴 클릭)
    Single,
    /// 같은 항목을 0.5초 안에 다시 누름 — 한 번 클릭 동작은 다시 하지 않는다
    Double,
}

/// 두 번 클릭으로 치는 간격 (윈도우 기본값 500ms. egui 기본 0.3초는 천천히 누르는 사람에게 짧다)
pub const DBL_SECS: f64 = 0.5;
/// 두 번째 클릭이 첫 클릭 자리에서 벗어나도 되는 거리
const DBL_DIST: f32 = 10.0;

#[derive(Clone, Copy)]
struct LastClick {
    key: egui::Id,
    t: f64,
    pos: Pos2,
    /// 두 번 클릭으로 쓴 프레임 (0 = 아직 안 씀)
    used: u64,
}

/// 항목 키로 하는 한 번·두 번 클릭 판정. egui 의 `double_clicked()` 대신 쓴다.
///
/// egui 는 두 번째 클릭이 같은 위젯에 떨어져야 두 번 클릭으로 친다. 첫 클릭으로 화면이 바뀌면
/// (편집칸이 열려 목록이 밀림 · 바로 송출로 다시 배치 · 스크롤 따라가기) 두 번째 클릭이 다른 줄에 떨어져
/// "두 번 눌러도 안 켜진다". 여기서는 '첫 클릭 때 누른 항목(key)' 을 기억해 두고, 0.5초 안에 거의 같은 자리를
/// 다시 누르면 그 항목이 어디로 밀렸든 그 항목의 두 번 클릭으로 친다. 그때 그 자리에 온 다른 줄은 클릭을 받지 않는다.
///
/// - `Single`: 한 번 클릭 동작을 한다 (고르기·미리보기)
/// - `Double`: 두 번 클릭 동작만 한다 (송출·넣기). 한 번 클릭 동작이 또 실행되지 않으므로 넣기가 두 번 되지 않는다
///
/// key 는 화면 전체에서 겹치지 않게 (`("cue-row", id)` 처럼 자리 이름과 항목 키).
pub fn click2(ui: &Ui, key: impl std::hash::Hash + std::fmt::Debug, resp: &Response) -> Click {
    let ctx = ui.ctx();
    let key = egui::Id::new(key);
    let sid = egui::Id::new("wp-click2");
    let (now, any, pos) = ctx.input(|i| (i.time, i.pointer.primary_clicked(), i.pointer.interact_pos()));
    if !any {
        return Click::None;
    }
    let frame = ctx.cumulative_pass_nr() + 1;
    let last: Option<LastClick> = ctx.data(|d| d.get_temp(sid));
    if let (Some(l), Some(p)) = (last, pos) {
        if l.used == frame {
            return Click::None; // 이번 클릭은 이미 다른 줄의 두 번 클릭으로 썼다
        }
        if l.used == 0 && now - l.t <= DBL_SECS && p.distance(l.pos) <= DBL_DIST {
            if l.key != key {
                return Click::None; // 첫 클릭 항목의 두 번 클릭이다 (밀려서 이 자리에 온 줄은 받지 않음)
            }
            // 제자리면 실제로 클릭을 받았어야 하고(위에 뜬 단추·창이 받았으면 아님), 밀려났으면 그대로 인정
            if resp.clicked() || !resp.rect.contains(p) {
                ctx.data_mut(|d| d.insert_temp(sid, LastClick { used: frame, ..l }));
                return Click::Double;
            }
            return Click::None;
        }
    }
    if resp.clicked() {
        if let Some(p) = pos {
            ctx.data_mut(|d| d.insert_temp(sid, LastClick { key, t: now, pos: p, used: 0 }));
        }
        return Click::Single;
    }
    Click::None
}

/// 합성 입력 시험용: 방금 그린 확인 단추(confirm-first·confirm-yes) 자리를 다른 이름으로도 남긴다.
/// 한 화면에 확인 단추가 둘 이상일 때 어느 것인지 가려 누르기 위해서다 (시험 빌드가 아니면 아무것도 안 함)
#[allow(unused_variables)]
pub fn mark_confirm(name: &str) {
    #[cfg(test)]
    HOVER_SEEN.with(|m| {
        let mut m = m.borrow_mut();
        for k in ["first", "yes"] {
            if let Some(r) = m.remove(&format!("confirm-{k}")) {
                m.insert(format!("{name}-{k}"), r);
            }
        }
    });
}

/// 합성 입력 시험용: 이름 붙은 위젯 자리를 남긴다 (시험 빌드가 아니면 아무것도 안 함)
#[allow(unused_variables)]
pub fn mark(name: &str, rect: Rect) {
    #[cfg(test)]
    HOVER_SEEN.with(|m| m.borrow_mut().insert(name.to_string(), rect));
}

/// 반투명 바탕 아이콘 단추 하나 (control.css .sg-ops button: rgba(15,18,24,.85) 바탕 · 흰 글자 · 지우기는 빨강)
pub fn overlay_btn(ui: &Ui, id: egui::Id, rect: Rect, icon: &str, tip: &str, danger: bool) -> Response {
    let p = pal();
    let resp = ui.interact(rect, id, Sense::click());
    let hov = resp.hovered();
    let bg = if hov && danger { p.live } else if hov { p.accent_strong } else { alpha(p.sunken, 0.86) };
    let pt = ui.painter();
    pt.rect_filled(rect, CornerRadius::same(5), bg);
    pt.rect_stroke(rect, CornerRadius::same(5), Stroke::new(1.0, alpha(p.line2, 0.9)), StrokeKind::Inside);
    icons::paint(ui, rect.shrink(5.5), icon, if hov { Color32::WHITE } else { p.text2 });
    if hov {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.on_hover_text(tip)
}

/// 칸(`cell`) 안 모서리에 단추들을 그린다 — 마우스가 칸 위에 있을 때만. 누른 동작 이름을 돌려준다.
/// 칸의 클릭 반응(ui.interact)을 먼저 만든 뒤 부르면 단추가 그 위에 놓여 단추만 눌린다.
pub fn hover_ops(ui: &Ui, id: egui::Id, cell: Rect, ops: &[HoverOp]) -> Option<&'static str> {
    if ops.is_empty() || !ui.rect_contains_pointer(cell) {
        return None;
    }
    let s = HOVER_BTN.min(cell.height() - 4.0).max(16.0);
    let inset = if cell.height() >= s + 12.0 { 6.0 } else { ((cell.height() - s) / 2.0).max(0.0) };
    let (mut lx, mut rx) = (cell.left() + inset, cell.right() - inset);
    let y = cell.top() + inset;
    let mut out = None;
    for o in ops {
        let r = if o.right {
            rx -= s;
            let r = Rect::from_min_size(Pos2::new(rx, y), Vec2::splat(s));
            rx -= 4.0;
            r
        } else {
            let r = Rect::from_min_size(Pos2::new(lx, y), Vec2::splat(s));
            lx += s + 4.0;
            r
        };
        #[cfg(test)]
        HOVER_SEEN.with(|m| m.borrow_mut().insert(o.op.to_string(), r));
        if overlay_btn(ui, id.with(o.op), r, o.icon, &o.tip, o.danger).clicked() {
            out = Some(o.op);
        }
    }
    out
}

/// 작은 칩·줄처럼 단추를 안에 넣기 어려운 것: 칩 오른쪽 위에 떠 있는 단추 줄 (앞 레이어).
/// 칩과 단추 줄 어디에 마우스가 있어도 보인다.
pub fn hover_ops_float(ui: &Ui, id: egui::Id, anchor: Rect, ops: &[HoverOp]) -> Option<&'static str> {
    if ops.is_empty() {
        return None;
    }
    let s = HOVER_BTN;
    let w = ops.len() as f32 * (s + 4.0) - 4.0;
    // 칩 위로 겹치게 (칩에서 단추로 옮겨 가는 동안 끊기지 않게)
    let bar = Rect::from_min_size(Pos2::new((anchor.right() - w).max(anchor.left() - w * 0.5), anchor.top() - s + 6.0), Vec2::new(w, s));
    let hot = ui.ctx().pointer_hover_pos().is_some_and(|q| anchor.contains(q) || bar.expand(2.0).contains(q));
    if !hot || !ui.is_rect_visible(anchor) {
        return None;
    }
    let mut out = None;
    egui::Area::new(id.with("float")).order(egui::Order::Foreground).fixed_pos(bar.min).interactable(true).show(ui.ctx(), |ui| {
        let (_, _) = ui.allocate_exact_size(bar.size(), Sense::hover());
        for (j, o) in ops.iter().enumerate() {
            let r = Rect::from_min_size(Pos2::new(bar.left() + j as f32 * (s + 4.0), bar.top()), Vec2::splat(s));
            #[cfg(test)]
            HOVER_SEEN.with(|m| m.borrow_mut().insert(o.op.to_string(), r));
            if overlay_btn(ui, id.with(o.op), r, o.icon, &o.tip, o.danger).clicked() {
                out = Some(o.op);
            }
        }
    });
    out
}

// ---------------- 되돌릴 수 없는 삭제 확인 (docs/ux-rules.md 1절 5 '실수를 막는다') ----------------
// import_ui.rs 번역본 지우기와 같은 '줄 안 확인' 방식: 처음 누르면 그 자리에 '정말 지우기 · 그만두기'가 나온다.
// 상태는 egui 임시 메모리(id)에 두므로 창마다 따로 필드를 만들 필요가 없다.

/// 이 확인이 걸려 있나 (첫 단추를 눌러 '정말 지우기'를 기다리는 중)
pub fn confirm_armed(ui: &Ui, id: egui::Id) -> bool {
    ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(false)
}

/// 확인을 건다 (아이콘·칩 단추처럼 첫 단추를 직접 그리는 곳에서 부른다)
pub fn confirm_arm(ui: &Ui, id: egui::Id, on: bool) {
    ui.data_mut(|d| d.insert_temp(id, on));
}

/// 확인 줄: 걸려 있으면 `question` + [정말 지우기] [그만두기] 를 그리고 정말 지우기를 누르면 true.
/// 걸려 있지 않으면 아무것도 그리지 않는다.
pub fn confirm_row(ui: &mut Ui, id: egui::Id, question: &str) -> bool {
    if !confirm_armed(ui, id) {
        return false;
    }
    let p = pal();
    let mut yes = false;
    ui.horizontal_wrapped(|ui| {
        icons::show(ui, "trash-2", 16.0, p.danger_text);
        ui.label(egui::RichText::new(question).color(p.danger_text));
        let r = ui.add(Btn::new(wp_core::i18n::tr("정말 지우기")).kind(Kind::Danger).size(Size::Sm));
        #[cfg(test)]
        HOVER_SEEN.with(|m| m.borrow_mut().insert("confirm-yes".into(), r.rect));
        if r.clicked() {
            yes = true;
            confirm_arm(ui, id, false);
        }
        if ui.add(Btn::new(wp_core::i18n::tr("그만두기")).size(Size::Sm)).clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            confirm_arm(ui, id, false);
        }
    });
    yes
}

/// 지우기 단추 + 줄 안 확인을 한 번에. 정말 지우기를 누른 프레임에 true.
/// `label` 은 첫 단추 글('모두 지우기' 등), `question` 은 무엇이 어떻게 지워지는지 ("N줄이 지워지고 되살릴 수 없습니다").
pub fn confirm_delete(ui: &mut Ui, id: egui::Id, label: &str, question: &str) -> bool {
    if confirm_armed(ui, id) {
        return confirm_row(ui, id, question);
    }
    let r = ui.add(Btn::new(label).icon("trash-2").kind(Kind::Danger).size(Size::Sm).tooltip(wp_core::i18n::tr("누르면 한 번 더 묻습니다")));
    #[cfg(test)]
    HOVER_SEEN.with(|m| m.borrow_mut().insert("confirm-first".into(), r.rect));
    if r.clicked() {
        confirm_arm(ui, id, true);
    }
    false
}

// ---------------- 떠 있는 창 기본 자리 (휴리스틱 평가 14번) ----------------
// egui 기본은 모든 창이 왼쪽 위(큐시트·메뉴줄 위)에 열린다. 여기서 만든 창은
// 처음 열 때 조작 화면 가운데 위쪽(메뉴줄·검색줄 아래, 왼쪽 큐시트 오른쪽)에 열리고,
// 이미 열려 있는 떠 있는 창 수만큼 오른쪽 아래로 비껴서 제목줄이 겹치지 않는다.
// 한 번 연 뒤 옮긴 자리는 egui 가 기억한다(닫았다 열어도 그 자리).

/// 메뉴줄(약 28) + 검색줄(약 60) 아래부터
pub const FLOAT_TOP: f32 = 96.0;
/// 비껴 놓는 간격 (제목줄 높이보다 조금 크게)
pub const FLOAT_STEP: f32 = 36.0;

/// 지금까지 이 방식으로 연 창들의 id (열려 있는지 세는 데 쓴다)
fn float_ids(ctx: &egui::Context) -> Vec<egui::Id> {
    ctx.data(|d| d.get_temp::<Vec<egui::Id>>(egui::Id::unique("wp-float-ids"))).unwrap_or_default()
}

/// 새 창의 기본 왼쪽 위 자리. `id` 창이 이미 자리를 가졌으면 그 자리는 egui 가 쓰므로 계산만 한다.
pub fn float_pos(ctx: &egui::Context, id: egui::Id, width: f32) -> Pos2 {
    let scr = ctx.content_rect();
    let mut ids = float_ids(ctx);
    let open = ctx.memory(|m| ids.iter().filter(|x| **x != id).filter(|x| m.areas().is_visible(&egui::LayerId::new(egui::Order::Middle, **x))).count());
    if !ids.contains(&id) {
        ids.push(id);
        ctx.data_mut(|d| d.insert_temp(egui::Id::unique("wp-float-ids"), ids));
    }
    // 왼쪽 큐시트(화면 너비 약 22%)를 가리지 않게, 될 수 있으면 가운데
    let left_min = scr.left() + scr.width() * 0.22;
    let x = (scr.center().x - width / 2.0).max(left_min);
    let step = FLOAT_STEP * (open % 8) as f32;
    let x = (x + step).min((scr.right() - width - 8.0).max(scr.left()));
    Pos2::new(x, scr.top() + FLOAT_TOP + step)
}

/// 떠 있는 창을 만든다: id 고정 · 기본 크기 · 기본 자리(위 규칙). `.open(&mut 열림)` 은 부르는 쪽에서.
/// `key` 는 창마다 다른 고정 이름 (제목은 번역되어 바뀔 수 있으므로 따로 받는다).
pub fn float_window<'a>(ctx: &egui::Context, key: &str, title: impl Into<String>, size: [f32; 2]) -> egui::Window<'a> {
    let id = egui::Id::unique(("wp-float", key));
    let pos = float_pos(ctx, id, size[0]);
    egui::Window::new(title.into()).id(id).default_size(size).default_pos(pos)
}

#[cfg(test)]
mod float_tests {
    use super::*;

    fn pass(ctx: &egui::Context, open: &mut [bool; 2], seen: &mut [Option<Rect>; 2]) {
        let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1600.0, 960.0))), ..Default::default() };
        let mut out = ctx.run_pass(input, |ctx| {
            for (i, key) in ["a", "b"].iter().enumerate() {
                if !open[i] {
                    continue;
                }
                let r = float_window(ctx, key, format!("창 {key}"), [440.0, 300.0]).show(ctx, |ui| { ui.label("내용"); });
                seen[i] = r.map(|r| r.response.rect);
            }
        });
        out.textures_delta.clear(); // GPU 없는 시험: 텍스처 변경을 소비한다
    }

    #[test]
    fn 떠있는_창은_큐시트와_메뉴줄을_가리지_않고_겹쳐_열리지_않는다() {
        let ctx = egui::Context::default();
        let mut open = [true, false];
        let mut seen = [None, None];
        for _ in 0..3 { pass(&ctx, &mut open, &mut seen); }
        let a = seen[0].expect("첫 창");
        assert!(a.top() >= FLOAT_TOP - 1.0, "메뉴줄 아래: {a:?}");
        assert!(a.left() >= 1600.0 * 0.22 - 1.0, "큐시트 오른쪽: {a:?}");
        open[1] = true;
        for _ in 0..3 { pass(&ctx, &mut open, &mut seen); }
        let b = seen[1].expect("둘째 창");
        assert!((b.top() - a.top()) >= FLOAT_STEP - 1.0, "제목줄이 겹치지 않게 비껴 열림: {a:?} {b:?}");
    }

    #[test]
    fn 줄_안_확인은_두_번_눌러야_지운다() {
        let ctx = egui::Context::default();
        let id = egui::Id::unique("t-confirm");
        let mut got = false;
        let mut out = ctx.run_pass(Default::default(), |ctx| {
            egui::Area::new(egui::Id::unique("t-confirm-area")).show(ctx, |ui| {
                assert!(!confirm_armed(ui, id));
                got |= confirm_row(ui, id, "물음"); // 걸리지 않았으면 아무 일 없음
                confirm_arm(ui, id, true);
                assert!(confirm_armed(ui, id));
            });
        });
        out.textures_delta.clear();
        assert!(!got);
    }
    /// 한 프레임: confirm_delete 를 그리고 정말 지우기가 눌렸는지
    fn confirm_pass(ctx: &egui::Context, t: f64, events: Vec<egui::Event>) -> bool {
        let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))), time: Some(t), events, ..Default::default() };
        let mut yes = false;
        let mut out = ctx.run_pass(input, |ctx| {
            egui::Area::new(egui::Id::unique("t-confirm-area2")).fixed_pos(Pos2::new(10.0, 10.0)).show(ctx, |ui| {
                yes = confirm_delete(ui, egui::Id::unique("t-confirm2"), "모두 지우기", "N줄이 지워집니다.");
            });
        });
        out.textures_delta.clear();
        yes
    }

    fn click_at(ctx: &egui::Context, t: &mut f64, p: Pos2) -> bool {
        let mut got = false;
        *t += 0.05;
        got |= confirm_pass(ctx, *t, vec![egui::Event::PointerMoved(p)]);
        for (pressed, dt) in [(true, 0.05), (false, 0.05)] {
            *t += dt;
            got |= confirm_pass(ctx, *t, vec![egui::Event::PointerMoved(p), egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() }]);
        }
        *t += 0.05;
        got |= confirm_pass(ctx, *t, vec![]);
        got
    }

    #[test]
    fn 지우기_단추는_첫_클릭에_묻고_두번째에_지운다() {
        let ctx = egui::Context::default();
        let mut t = 0.0;
        for _ in 0..2 {
            t += 0.05;
            confirm_pass(&ctx, t, vec![]);
        }
        let first = HOVER_SEEN.with(|m| m.borrow().get("confirm-first").copied()).expect("첫 단추");
        assert!(!click_at(&ctx, &mut t, first.center()), "첫 클릭으로는 지우지 않는다");
        let yes = HOVER_SEEN.with(|m| m.borrow().get("confirm-yes").copied()).expect("정말 지우기 단추가 나와야 함");
        assert!(click_at(&ctx, &mut t, yes.center()), "정말 지우기를 누르면 지운다");
        // 지운 뒤에는 다시 첫 단추 상태
        assert!(!confirm_pass(&ctx, t + 0.1, vec![]));
    }
}