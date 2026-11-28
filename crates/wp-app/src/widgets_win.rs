//! 창 · 대화상자 공용 부품 (craft-windows).
//!
//! `theme.rs` · `widgets.rs` 위에 얹는 얇은 층이다 — 색 · 둥글기 · 단추 모양은 모두 그쪽에서 가져오고,
//! 여기에는 "창을 창답게" 만드는 짜임만 둔다:
//!
//! ```ignore
//! use crate::widgets_win as win;
//! win::window(ctx, "names", tr("이름 자막"), [520.0, 560.0]).open(&mut open).show(ctx, |ui| {
//!     win::lead(ui, &tr("이름과 직분을 화면 아래에 잠깐 띄웁니다."));       // 창 맨 위 한 줄 설명
//!     win::section(ui, &tr("명단"));                                       // 구획 머리
//!     win::field(ui, &tr("이름"), |ui| ui.text_edit_singleline(&mut name)); // 오른쪽 맞춤 이름표 + 조절 칸
//!     win::hint(ui, &tr("비우면 직분만 나옵니다."));                         // 조절 칸 아래 짧은 도움말
//!     win::message(ui, &msg);                                              // ✓ · ✗ 로 시작하는 결과 글 → 아이콘 + 색
//!     win::footer(ui, |ui| {                                               // 아래 단추 줄 (오른쪽 맞춤)
//!         if ui.add(win::primary(tr("띄우기"))).clicked() { … }             //   주 단추가 맨 오른쪽
//!         if ui.add(win::secondary(tr("닫기"))).clicked() { … }             //   취소 · 닫기는 그 왼쪽
//!     });
//! });
//! ```
//!
//! 규칙 (docs/ui-craft-windows.md):
//! - 창마다 주 단추(Primary)는 하나, 아래 줄 맨 오른쪽. 취소 · 닫기는 그 왼쪽. 이름은 동사('만들기' · '띄우기' · '가져오기').
//! - 위험한 동작(지우기 · 덮어쓰기 · 되돌릴 수 없는 것)은 채우지 않은 단추 + 한 번 더 묻기 (`danger` + `widgets::confirm_delete`).
//! - Esc = 맨 위 창 닫기 (원본 control.js: 열린 창을 먼저 닫고, 닫을 것이 없을 때만 단축키). 바깥을 눌러도 안 닫히는 창
//!   (편집기 · 예배 중 띄워 두는 창, `keep_on_outside`)은 Esc 로도 닫지 않는다.
//! - 상태 셋: 비어 있음 `empty` · 하는 중 `busy` · 실패 `error` — 실패에는 다음에 할 일을 같이 적는다.
#![allow(dead_code)]

use crate::icons;
use crate::theme::{self, pal, Weight};
use crate::widgets::{self, Btn, Kind, Size};
use eframe::egui::{self, Color32, Id, Pos2, Rect, Response, RichText, Sense, Stroke, Ui, Vec2};

// ---------------------------------------------------------------- 창

/// 떠 있는 창 (`widgets::float_window` + 화면에 맞춘 처음 크기 + Esc 닫기).
/// `key` 는 창마다 다른 고정 이름. id 는 `widgets::float_window` 와 같다(`("wp-float", key)`) — 자리 · 크기 기억이 이어진다.
pub fn window<'a>(ctx: &egui::Context, key: &str, title: impl Into<String>, size: [f32; 2]) -> Win<'a> {
    let size = fit_size(ctx, size);
    let scr = ctx.content_rect();
    let id = Id::unique(("wp-float", key));
    // 처음 자리: 가운데 위쪽(왼쪽 큐시트는 비킴), 이미 열린 우리 창 수만큼 비껴 놓기. 넓은 창(화면 폭의 55% 넘음)은 가운데
    // (widgets::float_pos 의 열린 창 세기는 닫힌 창도 셀 때가 있어 창이 화면 가운데까지 내려가 떴다 — 여기서 직접 정함)
    let others = open_count(ctx, id);
    let step = widgets::FLOAT_STEP * (others % 6) as f32;
    let x = if size[0] > scr.width() * 0.55 { scr.center().x - size[0] / 2.0 } else { (scr.center().x - size[0] / 2.0).max(scr.left() + scr.width() * 0.22) };
    let x = (x + step).min(scr.right() - size[0] - 8.0).max(scr.left() + 8.0);
    let pos = Pos2::new(x, scr.top() + widgets::FLOAT_TOP + step);
    // 높이 한도는 내용 칸 기준이라 창 머리(제목 줄)만큼 더 뺀다 — 안 그러면 큰 창이 위 메뉴 막대를 덮는다
    let max_h = (scr.height() - widgets::FLOAT_TOP - 12.0 - TITLE_H).max(200.0);
    Win { inner: widgets::float_window(ctx, key, title, size).max_width((scr.width() - 24.0).max(320.0)).max_height(max_h).vscroll(true), id, open: None, modal: true, size, fit: true, max_h, pos: Some(pos) }
}

/// 창 머리(제목 줄 + 테두리 안 여백) 높이 어림
const TITLE_H: f32 = 48.0;

// ---- 뷰어를 가리지 않는 자리 (진행 중에 여는 창: 즉석 자막 · 무대 화면 · 이름 자막 · 회중 메시지 · 오버레이)

fn viewers_key() -> Id {
    Id::new("wp-win-viewers")
}

/// 주 화면이 이번 프레임에 뷰어(다음 · 송출 중)를 그린 자리를 적는다 (prolayout_ui). 진행용 창의 처음 자리를 정할 때 쓴다
pub fn note_viewers(ctx: &egui::Context, rect: Rect) {
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|d| d.insert_temp(viewers_key(), (rect, pass)));
}

/// 지금 보이는 뷰어 자리 (최근 두 프레임 안에 그린 것만)
pub fn viewers_rect(ctx: &egui::Context) -> Option<Rect> {
    let pass = ctx.cumulative_pass_nr();
    ctx.data(|d| d.get_temp::<(Rect, u64)>(viewers_key())).filter(|(_, at)| at + 2 >= pass).map(|(r, _)| r)
}

/// 뷰어를 가리지 않는 처음 자리 (왼쪽 위 모서리). 차례: 뷰어 바로 아래 → 뷰어 왼쪽 → 뷰어 오른쪽. 어디에도 넉넉히 안 들어가면 None (보통 자리)
pub fn beside_viewers(scr: Rect, viewers: Rect, size: [f32; 2]) -> Option<Pos2> {
    let (w, h) = (size[0], size[1]);
    let min_h = h.min(220.0);
    let gap = 8.0;
    // 1) 뷰어 아래 (본문 판 위): 뷰어 왼쪽 끝에 맞추고, 화면 오른쪽을 넘으면 안으로
    let top = viewers.bottom() + gap;
    if scr.bottom() - top - gap >= min_h {
        let x = (viewers.left() + gap).min(scr.right() - w - gap).max(scr.left() + gap);
        return Some(Pos2::new(x, top));
    }
    let y = (viewers.top()).max(scr.top() + widgets::FLOAT_TOP);
    // 2) 뷰어 왼쪽 ('송출 중만' 보기: 뷰어가 가운데 판 오른쪽 칸)
    if viewers.left() - scr.left() - 2.0 * gap >= w {
        return Some(Pos2::new(viewers.left() - gap - w, y));
    }
    // 3) 뷰어 오른쪽 (인스펙터 위)
    if scr.right() - viewers.right() - 2.0 * gap >= w {
        return Some(Pos2::new(viewers.right() + gap, y));
    }
    None
}

/// 처음 크기를 조작 화면 안에 맞춘다 (1000×640 창 · 배율 150% 에서도 창이 화면 밖으로 나가지 않게)
pub fn fit_size(ctx: &egui::Context, size: [f32; 2]) -> [f32; 2] {
    let scr = ctx.content_rect();
    let max_w = (scr.width() - 24.0).max(320.0);
    let max_h = (scr.height() - widgets::FLOAT_TOP - 12.0 - TITLE_H).max(200.0);
    [size[0].min(max_w), size[1].min(max_h)]
}

pub struct Win<'a> {
    inner: widgets::FloatWin<'a>,
    id: Id,
    open: Option<&'a mut bool>,
    modal: bool,
    size: [f32; 2],
    /// 내용에 맞춘 높이로 연다 (창 안에서 스크롤하는 보통 창). 칸을 창 높이에 맞춰 나누는 편집기(`vscroll(false)`)는 정한 높이 그대로
    fit: bool,
    /// 내용 높이 한도 (넘으면 창 안에서 스크롤)
    max_h: f32,
    /// 처음 열 때 놓을 자리 (한 번만 — 그 뒤로는 egui 가 기억한 자리)
    pos: Option<Pos2>,
}

impl<'a> Win<'a> {
    /// 진행 중에 여는 창: 처음 뜨는 자리를 뷰어(다음 · 송출 중) 밖으로. 사용자가 옮긴 자리는 egui 가 기억한다.
    /// 뷰어 아래에 놓으면 높이를 화면 아래 끝까지로 묶는다 (넘치면 창 안에서 스크롤)
    pub fn live(mut self, ctx: &egui::Context) -> Self {
        let scr = ctx.content_rect();
        if let Some(pos) = viewers_rect(ctx).and_then(|v| beside_viewers(scr, v, self.size)) {
            self.pos = Some(pos);
        }
        let top = ctx.memory(|m| m.area_rect(self.id)).map(|r| r.top()).or(self.pos.map(|p| p.y)).unwrap_or(scr.top() + widgets::FLOAT_TOP);
        // 창 머리(약 36px)와 아래 여백을 뺀 내용 높이 한도
        let max_h = (scr.bottom() - top - 48.0).max(200.0);
        self.max_h = self.max_h.min(max_h);
        self.inner = self.inner.max_height(self.max_h);
        self
    }
    pub fn open(mut self, open: &'a mut bool) -> Self {
        self.open = Some(open);
        self
    }
    /// 바깥을 눌러도 · Esc 를 눌러도 닫지 않는다 (조작 화면과 함께 쓰는 창: 편집기 · 음성 인식 · 점검)
    pub fn keep_on_outside(mut self) -> Self {
        self.inner = self.inner.keep_on_outside();
        self.modal = false;
        self
    }
    pub fn resizable(mut self, v: bool) -> Self { self.inner = self.inner.resizable(v); self }
    pub fn vscroll(mut self, v: bool) -> Self { self.inner = self.inner.vscroll(v); self.fit = v; self }
    pub fn hscroll(mut self, v: bool) -> Self { self.inner = self.inner.hscroll(v); self }
    pub fn min_width(mut self, v: f32) -> Self { self.inner = self.inner.min_width(v); self }
    pub fn min_height(mut self, v: f32) -> Self { self.inner = self.inner.min_height(v); self }
    pub fn max_width(mut self, v: f32) -> Self { self.inner = self.inner.max_width(v); self }
    pub fn max_height(mut self, v: f32) -> Self { self.inner = self.inner.max_height(v); self.max_h = v; self }
    pub fn default_width(mut self, v: f32) -> Self { self.inner = self.inner.default_width(v); self }
    pub fn default_height(mut self, v: f32) -> Self { self.inner = self.inner.default_height(v); self.fit = false; self }
    pub fn collapsible(mut self, v: bool) -> Self { self.inner = self.inner.collapsible(v); self }
    pub fn id(&self) -> Id {
        self.id
    }
    pub fn show<R>(self, ctx: &egui::Context, add: impl FnOnce(&mut Ui) -> R) -> Option<egui::InnerResponse<Option<R>>> {
        let Win { inner, id, open, modal, fit, max_h, pos, .. } = self;
        let showing = open.as_ref().is_none_or(|o| **o);
        // 처음 열 때 한 번만 자리를 정한다. 닫힌 창에는 하지 않는다 (set_state 는 그 층을 '보임' 으로 친다)
        let placed = Id::new(("wp-win-placed", id));
        if showing && !ctx.data(|d| d.get_temp::<bool>(placed).unwrap_or(false)) {
            if let Some(pos) = pos.filter(|_| ctx.memory(|m| m.area_rect(id).is_none())) {
                ctx.memory_mut(|m| m.areas_mut().set_state(egui::LayerId::new(egui::Order::Middle, id), egui::AreaState { pivot_pos: Some(pos), pivot: egui::Align2::LEFT_TOP, size: None, interactable: true, last_became_visible_at: None }));
            }
            ctx.data_mut(|d| d.insert_temp(placed, true));
        }
        // 높이 한도는 창이 실제로 놓인 자리에서 화면 아래까지 (비껴 놓인 창 · 사용자가 내린 창도 아래가 잘리지 않게)
        let scr = ctx.content_rect();
        let top = ctx.memory(|m| m.area_rect(id)).map(|r| r.top()).or(pos.map(|p| p.y)).unwrap_or(scr.top() + widgets::FLOAT_TOP);
        let max_h = max_h.min((scr.bottom() - top - 12.0 - TITLE_H).max(200.0));
        let inner = inner.max_height(max_h);
        // 내용에 맞춘 높이: 창의 스크롤을 끄고 우리 스크롤(높이 한도 = 창 한도, 아래로는 내용만큼 줄어듦)을 넣는다.
        // egui 의 Resize 는 지난 프레임 내용 크기까지 저절로 늘어나므로, 처음 높이를 작게 주면 내용 높이에서 멈춘다 (빈 칸이 큰 창이 없게)
        let (inner, add): (_, Box<dyn FnOnce(&mut Ui) -> R + '_>) = if fit {
            (
                inner.vscroll(false).default_height(60.0),
                Box::new(move |ui: &mut Ui| egui::ScrollArea::vertical().max_height(max_h).min_scrolled_height(max_h).auto_shrink([false, true]).show(ui, |ui| add(ui)).inner),
            )
        } else {
            (inner, Box::new(add))
        };
        match open {
            Some(o) => {
                if *o {
                    seen(ctx, id, modal);
                    if take_close(ctx, id) {
                        *o = false;
                    }
                }
                inner.open(o).show(ctx, add)
            }
            None => {
                seen(ctx, id, false);
                inner.show(ctx, add)
            }
        }
    }
}

// ---- Esc: 맨 위 창 닫기

#[derive(Clone, Default)]
struct Reg {
    /// (창 id, 마지막으로 그린 프레임, Esc 로 닫아도 되는 창인지)
    wins: Vec<(Id, u64, bool)>,
    /// Esc 로 닫으라고 한 창 (id, 프레임)
    close: Option<(Id, u64)>,
}

fn reg_id() -> Id {
    Id::new("wp-win-reg")
}

/// 이번 프레임에 이 창을 그렸다고 적는다
pub fn seen(ctx: &egui::Context, id: Id, modal: bool) {
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|d| {
        let r: &mut Reg = d.get_temp_mut_or_default(reg_id());
        match r.wins.iter_mut().find(|w| w.0 == id) {
            Some(w) => *w = (id, pass, modal),
            None => r.wins.push((id, pass, modal)),
        }
        r.wins.retain(|w| w.1 + 2 >= pass);
    });
}

/// `egui::Window` 를 직접 만드는 창도 같은 규칙으로: 열려 있는 동안 매 프레임 부른다. Esc 로 닫혔으면 true
pub fn esc_closes(ctx: &egui::Context, id: Id) -> bool {
    seen(ctx, id, true);
    take_close(ctx, id)
}

fn take_close(ctx: &egui::Context, id: Id) -> bool {
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|d| {
        let r: &mut Reg = d.get_temp_mut_or_default(reg_id());
        match r.close {
            Some((cid, at)) if cid == id && at + 2 >= pass => {
                r.close = None;
                true
            }
            Some((_, at)) if at + 2 < pass => {
                r.close = None;
                false
            }
            _ => false,
        }
    })
}

/// 지금 열려 있는 우리 창 수 (`id` 는 빼고)
fn open_count(ctx: &egui::Context, id: Id) -> usize {
    let pass = ctx.cumulative_pass_nr();
    let reg: Reg = ctx.data(|d| d.get_temp(reg_id())).unwrap_or_default();
    reg.wins.iter().filter(|w| w.0 != id && w.1 + 2 >= pass).count()
}

/// 지금 열려 있는 창 가운데 맨 위 (Esc 로 닫아도 되는 것만)
fn top_modal(ctx: &egui::Context) -> Option<Id> {
    let pass = ctx.cumulative_pass_nr();
    let reg: Reg = ctx.data(|d| d.get_temp(reg_id())).unwrap_or_default();
    let live: Vec<(Id, bool)> = reg.wins.iter().filter(|w| w.1 + 2 >= pass).map(|w| (w.0, w.2)).collect();
    if live.is_empty() {
        return None;
    }
    // 겹친 차례(뒤 → 앞)에서 맨 앞에 있는 우리 창. 맨 앞 창이 Esc 로 닫지 않는 창(편집기)이면 아무것도 닫지 않는다
    let order: Vec<Id> = ctx.memory(|m| m.layer_ids().filter(|l| l.order == egui::Order::Middle).map(|l| l.id).collect());
    let top = order.iter().rev().find(|id| live.iter().any(|w| w.0 == **id))?;
    live.iter().find(|w| w.0 == *top).filter(|w| w.1).map(|w| w.0)
}

/// Esc 로 닫을 창이 열려 있는가 (Enter = 미리보기 송출 같은 전역 키를 창이 열린 동안 막을 때)
pub fn modal_open(ctx: &egui::Context) -> bool {
    top_modal(ctx).is_some()
}

/// 전역 단축키보다 먼저 부른다: Esc 가 눌렸고 닫을 창이 있으면 그 창을 닫으라고 적고 키를 먹는다 (true).
/// 글 쓰는 중(`typing`)에는 egui 가 Esc 로 초점을 먼저 놓으므로 받지 않는다. 펼침 목록 · 메뉴가 열려 있으면 그쪽이 닫힌다.
pub fn esc_first(ctx: &egui::Context, typing: bool) -> bool {
    if typing || egui::Popup::is_any_open(ctx) {
        return false;
    }
    if !ctx.input(|i| i.key_pressed(egui::Key::Escape) && i.modifiers.is_none()) {
        return false;
    }
    let Some(id) = top_modal(ctx) else { return false };
    ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|d| d.get_temp_mut_or_default::<Reg>(reg_id()).close = Some((id, pass)));
    ctx.request_repaint();
    true
}

/// 창 안에서: Enter = 주 동작 (글 쓰는 칸에 초점이 없을 때). 글칸의 Enter 는 그 칸의 `lost_focus()` 로 따로 받는다
pub fn enter(ui: &Ui) -> bool {
    !ui.ctx().egui_wants_keyboard_input() && !egui::Popup::is_any_open(ui.ctx()) && ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.is_none())
}

// ---------------------------------------------------------------- 점검 인자

/// 점검용 `--open-x <이름>[,<이름>]`: 메뉴 id 가 없는 창 · 상태(첫 실행 묻기 · 새 버전 있음 …)를 캡처하려고 연다
pub fn open_x(name: &str) -> bool {
    static V: once_cell::sync::Lazy<Vec<String>> = once_cell::sync::Lazy::new(|| {
        let a: Vec<String> = std::env::args().collect();
        a.iter().position(|x| x == "--open-x").and_then(|i| a.get(i + 1)).map(|v| v.split(',').map(String::from).collect()).unwrap_or_default()
    });
    V.iter().any(|x| x == name)
}

// ---------------------------------------------------------------- 글

/// 창 맨 위 한 줄 설명 (이 창이 무엇을 하는지). 12px · 둘째 글색 · 줄바꿈
pub fn lead(ui: &mut Ui, text: &str) -> Response {
    let r = ui.add(egui::Label::new(theme::ty(text, theme::Type::Body, pal().text2)).wrap());
    ui.add_space(theme::sp::X4);
    r
}

/// 조절 칸 아래 짧은 도움말. 11.5px · 흐린 글 · 줄바꿈
pub fn hint(ui: &mut Ui, text: &str) -> Response {
    ui.add(egui::Label::new(theme::ty(text, theme::Type::Small, pal().muted)).wrap())
}

/// 구획 머리 (인스펙터 띠와 같은 부품)
pub fn section(ui: &mut Ui, title: &str) {
    widgets::group_head(ui, title);
}

/// 구획 머리 + 오른쪽 끝 작은 조작 (개수 · 단추)
pub fn section_with(ui: &mut Ui, title: &str, right: impl FnOnce(&mut Ui)) {
    let p = pal();
    ui.add_space(6.0);
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 22.0), Sense::hover());
    ui.painter().text(Pos2::new(r.left(), r.center().y), egui::Align2::LEFT_CENTER, title, theme::font(11.5, Weight::SemiBold), p.text);
    ui.painter().hline(r.x_range(), r.bottom() - 0.5, Stroke::new(1.0, p.line));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(r.shrink2(Vec2::new(0.0, 1.0))).layout(egui::Layout::right_to_left(egui::Align::Center)));
    right(&mut child);
    ui.add_space(4.0);
}

/// 양식 한 줄 (오른쪽 맞춤 이름표 + 조절 칸) — `widgets::form_row`
pub fn field<R>(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    widgets::form_row(ui, label, add)
}

/// 좁은 편집기에서는 세 열을 탭으로 바꾼다. 창 크기를 다시 넓히면 원래 열 배치로 돌아간다.
pub fn editor_tabs(ui: &mut Ui, key: &str, labels: &[String], go: Option<usize>) -> usize {
    let id = Id::new(("wp-editor-tab", key));
    let mut tab = go.unwrap_or_else(|| ui.data(|d| d.get_temp::<usize>(id)).unwrap_or(1));
    let items: Vec<(usize, &str)> = labels.iter().enumerate().map(|(i, s)| (i, s.as_str())).collect();
    widgets::segmented_cells(ui, &mut tab, &items, widgets::SegStyle::COMPACT);
    ui.data_mut(|d| d.insert_temp(id, tab));
    ui.separator();
    tab
}

/// 이름표 칸 너비를 정해 둔 양식 줄 (한 창 안의 줄들이 같은 세로선에 맞게). 조절 칸은 가로로 이어 놓는다
pub fn field_w<R>(ui: &mut Ui, label_w: f32, label: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.horizontal(|ui| {
        widgets::form_label(ui, label_w, label, "");
        add(ui)
    })
    .inner
}

/// 값 한 줄 (이름표 + 읽기만 하는 값). 값은 본문 글색, 길면 자름 · 마우스 올리면 전체
pub fn value_row(ui: &mut Ui, label_w: f32, label: &str, value: &str) {
    ui.horizontal(|ui| {
        widgets::form_label(ui, label_w, label, "");
        ui.add(egui::Label::new(theme::rich(value, 12.5, Weight::Regular, pal().text)).truncate()).on_hover_text(value);
    });
}

// ---------------------------------------------------------------- 상태

/// 결과 글의 종류
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Ok,
    Error,
    Warn,
    Info,
    Busy,
}

/// 옛 코드의 결과 글('✓ …' · '✗ …' · '✔' · '✖' · '⚠')에서 종류와 글을 나눈다. 글꼴에 따라 색 이모지로 나오는 기호를 화면에 내지 않으려는 것
pub fn split_tone(msg: &str) -> (Tone, &str) {
    let m = msg.trim_start();
    for (pre, tone) in [("✓", Tone::Ok), ("✔", Tone::Ok), ("✗", Tone::Error), ("✖", Tone::Error), ("✕", Tone::Error), ("⚠", Tone::Warn), ("…", Tone::Busy)] {
        if let Some(rest) = m.strip_prefix(pre) {
            return (tone, rest.trim_start_matches('\u{fe0f}').trim_start());
        }
    }
    (Tone::Info, m)
}

fn tone_look(t: Tone) -> (&'static str, Color32) {
    let p = pal();
    match t {
        Tone::Ok => ("check", p.text2), // 성공은 회색 (디자인 체계)
        Tone::Error => ("circle-x", p.danger_text),
        Tone::Warn => ("flag", p.warn),
        Tone::Info => ("info", p.muted),
        Tone::Busy => ("clock", p.muted),
    }
}

/// 결과 한 줄: 아이콘 + 글 (종류 색). 빈 글이면 아무것도 그리지 않는다
pub fn status(ui: &mut Ui, tone: Tone, text: &str) {
    if text.trim().is_empty() {
        return;
    }
    let (icon, col) = tone_look(tone);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 5.0;
        if tone == Tone::Busy {
            ui.add(egui::Spinner::new().size(13.0).color(col));
        } else {
            let (r, _) = ui.allocate_exact_size(Vec2::new(14.0, 17.0), Sense::hover());
            icons::paint(ui, Rect::from_center_size(r.center(), Vec2::splat(14.0)), icon, col);
        }
        ui.add(egui::Label::new(theme::rich(text, 12.0, Weight::Regular, if tone == Tone::Info { pal().text2 } else { col })).wrap());
    });
}

/// 옛 결과 글(✓ · ✗ 로 시작)을 그대로 받아 아이콘 + 색으로 그린다
pub fn message(ui: &mut Ui, msg: &str) {
    let (tone, text) = split_tone(msg);
    status(ui, tone, text);
}

/// 하는 중: 돌아가는 표시 + 글
pub fn busy(ui: &mut Ui, text: &str) {
    status(ui, Tone::Busy, text);
}

/// 실패: 무엇이 안 됐는지 + (있으면) 다음에 할 일 한 줄
pub fn error(ui: &mut Ui, text: &str, next: &str) {
    status(ui, Tone::Error, text);
    if !next.trim().is_empty() {
        ui.horizontal(|ui| {
            ui.add_space(19.0);
            hint(ui, next);
        });
    }
}

/// 빈 상태: 흐린 아이콘 + 한 줄 제목 + 다음에 할 일 (디자인 체계 `widgets::empty`). 목록 자리에 왼쪽 위로 맞춘다
pub fn empty(ui: &mut Ui, icon: &str, title: &str, next: &str) {
    widgets::empty(ui, icon, title, next, None);
}

/// 빈 상태 + 할 일 단추 하나. 눌렀으면 true
pub fn empty_action(ui: &mut Ui, icon: &str, title: &str, next: &str, action: &str) -> bool {
    widgets::empty(ui, icon, title, next, Some(action))
}

// ---------------------------------------------------------------- 단추 · 아래 줄

/// 창의 주 단추 (하나만). 최소 너비를 줘서 '열기' 같은 짧은 이름도 누르기 쉽게
pub fn primary(text: impl Into<String>) -> Btn {
    Btn::new(text).kind(Kind::Primary).min_width(76.0)
}

/// 보조 단추 (취소 · 닫기 · 그 밖)
pub fn secondary(text: impl Into<String>) -> Btn {
    Btn::new(text).min_width(64.0)
}

/// 위험한 동작의 첫 단추: 채우지 않음 (누르면 한 번 더 묻는 쪽에서 쓴다)
pub fn danger(text: impl Into<String>) -> Btn {
    Btn::new(text).kind(Kind::Danger)
}

/// 작은 보조 단추 (줄 안 · 구획 머리 옆)
pub fn small(text: impl Into<String>) -> Btn {
    Btn::new(text).size(Size::Sm)
}

/// 창 아래 단추 줄: 위에 얇은 선, 오른쪽 맞춤. **먼저 넣은 것이 맨 오른쪽** — 주 단추를 먼저, 취소를 그다음에 넣는다.
/// 왼쪽에 둘 것(지우기 · 상태 글)은 `footer_split` 으로.
pub fn footer<R>(ui: &mut Ui, right: impl FnOnce(&mut Ui) -> R) -> R {
    footer_split(ui, |_| {}, right)
}

/// 아래 단추 줄 + 왼쪽 칸 (위험한 동작 · 상태 글은 왼쪽, 주 단추 · 취소는 오른쪽)
pub fn footer_split<R>(ui: &mut Ui, left: impl FnOnce(&mut Ui), right: impl FnOnce(&mut Ui) -> R) -> R {
    let p = pal();
    ui.add_space(8.0);
    let w = ui.available_width();
    let (line, _) = ui.allocate_exact_size(Vec2::new(w, 1.0), Sense::hover());
    ui.painter().hline(line.x_range(), line.center().y, Stroke::new(1.0, p.line));
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.set_min_height(theme::ctl_h());
        left(ui);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| right(ui)).inner
    })
    .inner
}

/// 창 바닥에 붙는 단추 줄 (위 내용이 길어 스크롤해도 늘 보임). 내용을 그리기 **전에** 부른다
pub fn footer_pinned<R>(ui: &mut Ui, id: &str, left: impl FnOnce(&mut Ui), right: impl FnOnce(&mut Ui) -> R) -> R {
    let p = pal();
    egui::Panel::bottom(Id::new(("wp-win-foot", id)))
        .frame(egui::Frame::new().inner_margin(egui::Margin { left: 0, right: 0, top: 7, bottom: 0 }))
        .resizable(false)
        .show_separator_line(false)
        .show(ui, |ui| {
            let top = ui.max_rect().top() - 7.0;
            ui.painter().hline(ui.max_rect().x_range(), top + 0.5, Stroke::new(1.0, p.line));
            ui.horizontal(|ui| {
                ui.set_min_height(theme::ctl_h());
                left(ui);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| right(ui)).inner
            })
            .inner
        })
        .inner
}

// ---------------------------------------------------------------- 목록 줄

/// 고를 수 있는 목록 줄 하나의 바탕 (고른 줄 = 고름색 + 왼쪽 3px 막대, 마우스 올림 = hover). 높이 `h`, 가로 꽉.
/// 내용은 돌려받은 자리에 직접 그린다. 부모 커서는 줄 아래로만 간다 (`ui.put` 과 달리 겹치지 않음)
pub fn list_row(ui: &mut Ui, h: f32, selected: bool) -> (Rect, Response) {
    let p = pal();
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), h), Sense::click());
    if ui.is_rect_visible(rect) {
        let bg = if selected { p.sel } else if resp.hovered() { p.hover } else { Color32::TRANSPARENT };
        ui.painter().rect_filled(rect, theme::cr(theme::Rad::Md), bg);
        if selected {
            ui.painter().rect_filled(Rect::from_min_size(rect.min, Vec2::new(3.0, rect.height())), theme::cr(theme::Rad::Xs), sel_mark());
        }
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    (rect, resp)
}

/// 고른 것 표시 색: 전문가 테마는 회색(노랑 = '다음' 에만), 옛 테마는 강조색
pub fn sel_mark() -> Color32 {
    let p = pal();
    if p.bevel { p.text2 } else { p.accent }
}

/// 고른 것의 테두리 · 손잡이 색 (색 견본 · 캔버스의 고른 칸 · 편집 안내선): 전문가 테마는 밝은 무채색(노랑 = '다음' 에만), 옛 테마는 강조색
pub fn sel_line() -> Color32 {
    theme::pick() // 디자인 체계의 '고른 것' 색과 같은 것
}

/// 자리(`rect`)에 위젯 하나를 놓되 부모의 커서를 건드리지 않는다.
/// `ui.put` 은 커서를 그 자리 아래로 옮겨서, 줄 가운데에 작은 칸을 놓으면 다음 줄이 위로 당겨져 겹친다 (큐시트 관리 목록의 첫 줄이 가려지던 까닭)
pub fn place<R>(ui: &mut Ui, rect: Rect, add: impl FnOnce(&mut Ui) -> R) -> R {
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::centered_and_justified(egui::Direction::TopDown)));
    add(&mut child)
}

/// 작은 꼬리표 (목록 줄 안 '열림' · '쓰는 중' · '고정'): 테두리만, 무채색
pub fn tag_at(ui: &Ui, pos: Pos2, text: &str) -> Rect {
    let p = pal();
    let g = ui.painter().layout_no_wrap(text.to_string(), theme::font(10.5, Weight::SemiBold), p.text2);
    let r = Rect::from_min_size(pos, g.size() + Vec2::new(10.0, 3.0));
    ui.painter().rect_stroke(r, theme::cr(theme::Rad::Xs), Stroke::new(1.0, p.line2), egui::StrokeKind::Inside);
    ui.painter().galley(r.center() - g.size() / 2.0, g, p.text2);
    r
}

/// 흐린 작은 글 (목록 부제 · 개수)
pub fn sub(text: impl Into<String>) -> RichText {
    theme::rich(text, 11.5, Weight::Regular, pal().muted)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 결과_글의_기호를_종류로_나눈다() {
        assert_eq!(split_tone("✓ 저장했습니다."), (Tone::Ok, "저장했습니다."));
        assert_eq!(split_tone("✗ 파일을 읽지 못했습니다"), (Tone::Error, "파일을 읽지 못했습니다"));
        assert_eq!(split_tone("⚠\u{fe0f} 확인할 것"), (Tone::Warn, "확인할 것"));
        assert_eq!(split_tone("그냥 글"), (Tone::Info, "그냥 글"));
    }

    fn pass(ctx: &egui::Context, open: &mut [bool; 2], keep: [bool; 2], events: Vec<egui::Event>) -> bool {
        let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1600.0, 960.0))), events, ..Default::default() };
        let mut took = false;
        let mut out = ctx.run_pass(input, |ctx| {
            took = esc_first(ctx, false);
            for (i, key) in ["wa", "wb"].iter().enumerate() {
                let mut w = window(ctx, key, format!("창 {key}"), [300.0, 200.0]).open(&mut open[i]);
                if keep[i] {
                    w = w.keep_on_outside();
                }
                w.show(ctx, |ui| {
                    ui.label("내용");
                });
            }
        });
        out.textures_delta.clear();
        took
    }

    fn esc() -> Vec<egui::Event> {
        vec![egui::Event::Key { key: egui::Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() }]
    }

    #[test]
    fn esc_는_맨_위_창부터_하나씩_닫고_닫을_창이_없으면_받지_않는다() {
        let ctx = egui::Context::default();
        let mut open = [true, true];
        for _ in 0..3 {
            pass(&ctx, &mut open, [false, false], vec![]);
        }
        assert!(modal_open(&ctx));
        assert!(pass(&ctx, &mut open, [false, false], esc()), "창이 열려 있으면 Esc 를 창이 받는다");
        pass(&ctx, &mut open, [false, false], vec![]);
        assert_eq!(open.iter().filter(|o| **o).count(), 1, "한 번에 하나만 닫는다: {open:?}");
        for _ in 0..3 {
            pass(&ctx, &mut open, [false, false], vec![]);
        }
        assert!(pass(&ctx, &mut open, [false, false], esc()));
        for _ in 0..4 {
            pass(&ctx, &mut open, [false, false], vec![]);
        }
        assert_eq!(open, [false, false]);
        assert!(!modal_open(&ctx));
        assert!(!pass(&ctx, &mut open, [false, false], esc()), "닫을 창이 없으면 Esc 는 전역 단축키(예배당 화면 끄기)로 간다");
    }

    #[test]
    fn 바깥을_눌러도_안_닫히는_창은_esc_로도_닫지_않는다() {
        let ctx = egui::Context::default();
        let mut open = [true, false];
        for _ in 0..3 {
            pass(&ctx, &mut open, [true, false], vec![]);
        }
        assert!(!modal_open(&ctx));
        assert!(!pass(&ctx, &mut open, [true, false], esc()));
        pass(&ctx, &mut open, [true, false], vec![]);
        assert!(open[0], "편집기 · 음성 인식 창은 Esc 로 닫히지 않는다");
    }

    #[test]
    fn 보통_창은_내용_높이에_맞춰_열린다() {
        let ctx = egui::Context::default();
        let mut open = true;
        let mut rects = vec![];
        for _ in 0..8 {
            let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1600.0, 960.0))), ..Default::default() };
            let mut out = ctx.run_pass(input, |ctx| {
                let w = window(ctx, "fit-a", "짧은 창", [400.0, 700.0]).open(&mut open);
                w.show(ctx, |ui| {
                    for i in 0..5 {
                        ui.label(format!("줄 {i}"));
                    }
                });
                window(ctx, "fit-b", "긴 창", [400.0, 700.0]).show(ctx, |ui| {
                    for i in 0..200 {
                        ui.label(format!("줄 {i}"));
                    }
                });
            });
            out.textures_delta.clear();
            rects.push((ctx.memory(|m| m.area_rect(Id::unique(("wp-float", "fit-a")))), ctx.memory(|m| m.area_rect(Id::unique(("wp-float", "fit-b"))))));
        }
        eprintln!("{rects:?}");
        let (a, b) = rects.last().cloned().unwrap();
        let (a, b) = (a.unwrap(), b.unwrap());
        assert!(a.height() < 200.0, "짧은 창은 내용만큼: {a:?}");
        assert!(b.height() > 600.0 && b.bottom() <= 950.0 && b.top() >= widgets::FLOAT_TOP, "긴 창은 화면 한도까지 (메뉴 막대를 덮지 않게): {b:?}");
    }

    #[test]
    fn 처음_크기는_좁은_화면에_맞춘다() {
        let ctx = egui::Context::default();
        let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1000.0, 640.0))), ..Default::default() };
        let mut got = [0.0, 0.0];
        let mut out = ctx.run_pass(input, |ctx| got = fit_size(ctx, [1200.0, 900.0]));
        out.textures_delta.clear();
        assert!(got[0] <= 1000.0 - 24.0 && got[1] <= 640.0 - widgets::FLOAT_TOP - 16.0, "{got:?}");
    }
}
