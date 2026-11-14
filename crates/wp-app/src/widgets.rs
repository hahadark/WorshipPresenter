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
        let g = if on { g_bold } else { g_reg };
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
                out.fold_resp = Some(resp.on_hover_text(wp_core::i18n::tr("패널 메뉴 (접기 · 그룹)")));
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
