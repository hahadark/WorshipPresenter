//! 슬라이드 편집기 캔버스 보조: 고른 요소에 쓰는 글쇠 모으기 · 끌기 상태 · 크기 조절 손잡이 자리
//! (Electron control/slideedit.js 의 el keydown 처리 · onPointerDown · se-h 손잡이)

use eframe::egui::{self, Event, Key, Modifiers, Pos2, Rect, Vec2};
use wp_core::slideedit::PRect;

/// 편집기가 받은 글쇠 한 건 (전역 단축키보다 먼저 가져간다)
#[derive(Clone, Debug, PartialEq)]
pub(super) enum EdKey {
    /// Enter · F2: 고른 글(없으면 본문) 바로 고치기
    Edit,
    /// Delete · Backspace: 고른 덧붙인 요소 지우기
    Delete,
    /// 방향키 (요소를 고른 중): 0.5% · Shift 5% 옮기기
    Nudge(f64, f64),
    /// 방향키 (고른 요소 없음): 앞·뒤 슬라이드
    Nav(i32),
    /// Ctrl+D: 복제
    Dup,
    Copy,
    Cut,
    /// Ctrl+V 로 들어온 글
    Paste(String),
    /// 끌어다 놓은·붙여 넣은 그림 (주소 · 놓은 자리)
    Image(String, Option<Pos2>),
    /// Esc: 요소 고르기 풀기
    Deselect,
    /// Esc (고른 것이 없을 때): 편집기 닫기
    Close,
}

/// 글 쓰는 칸이 없을 때만 부른다. has_sel = 요소를 고른 중, multi_on = 여러 장 고른 중 (그 Esc 는 목록 처리에 맡긴다)
pub(super) fn collect(ctx: &egui::Context, has_sel: bool, multi_on: bool) -> Vec<EdKey> {
    let mut v = vec![];
    ctx.input_mut(|i| {
        if (has_sel || !multi_on) && i.consume_key(Modifiers::NONE, Key::Escape) {
            v.push(if has_sel { EdKey::Deselect } else { EdKey::Close });
        }
        let (alt, shift) = (i.modifiers.alt, i.modifiers.shift);
        // 클립보드: egui 는 Ctrl+C/X/V 를 글쇠가 아니라 사건으로 준다. Shift 가 눌린 붙여 넣기는 '종류 골라 붙여 넣기' 창 몫
        i.events.retain(|e| match e {
            Event::Copy if has_sel => {
                v.push(EdKey::Copy);
                false
            }
            Event::Cut if has_sel => {
                v.push(EdKey::Cut);
                false
            }
            Event::Paste(t) if !shift => {
                v.push(EdKey::Paste(t.clone()));
                false
            }
            _ => true,
        });
        if i.consume_key(Modifiers::COMMAND, Key::D) {
            v.push(EdKey::Dup);
        }
        if alt {
            return; // Alt+방향키 등은 전역 단축키(앞뒤 절) 그대로
        }
        if i.consume_key(Modifiers::NONE, Key::Enter) || i.consume_key(Modifiers::NONE, Key::F2) {
            v.push(EdKey::Edit);
        }
        if i.consume_key(Modifiers::NONE, Key::Delete) || i.consume_key(Modifiers::NONE, Key::Backspace) {
            v.push(EdKey::Delete);
        }
        let step = if shift { 5.0 } else { 0.5 };
        for (k, dx, dy) in [(Key::ArrowLeft, -1.0, 0.0), (Key::ArrowRight, 1.0, 0.0), (Key::ArrowUp, 0.0, -1.0), (Key::ArrowDown, 0.0, 1.0)] {
            if i.consume_key(Modifiers::NONE, k) {
                v.push(if has_sel { EdKey::Nudge(dx * step, dy * step) } else { EdKey::Nav((dx + dy) as i32) });
            }
        }
    });
    v
}

/// 끌고 있는 요소 (옮기기 또는 손잡이로 크기)
#[derive(Clone, Debug)]
pub(super) struct ElDrag {
    /// "main" 또는 요소 id
    pub id: String,
    /// "" = 옮기기, 그 밖 = 손잡이 이름
    pub handle: &'static str,
    pub start: PRect,
    pub origin: Pos2,
    /// 조금(0.4%) 넘게 움직여야 끌기로 친다
    pub moved: bool,
}

/// 손잡이 여덟 개: (이름, 가로 자리 0~1, 세로 자리 0~1)
pub(super) const HANDLES: [(&str, f32, f32); 8] = [("nw", 0.0, 0.0), ("n", 0.5, 0.0), ("ne", 1.0, 0.0), ("e", 1.0, 0.5), ("se", 1.0, 1.0), ("s", 0.5, 1.0), ("sw", 0.0, 1.0), ("w", 0.0, 0.5)];

pub(super) fn handle_rect(r: Rect, fx: f32, fy: f32) -> Rect {
    Rect::from_center_size(Pos2::new(r.left() + r.width() * fx, r.top() + r.height() * fy), Vec2::splat(10.0))
}

pub(super) fn handle_cursor(name: &str) -> egui::CursorIcon {
    match name {
        "nw" | "se" => egui::CursorIcon::ResizeNwSe,
        "ne" | "sw" => egui::CursorIcon::ResizeNeSw,
        "n" | "s" => egui::CursorIcon::ResizeVertical,
        _ => egui::CursorIcon::ResizeHorizontal,
    }
}
