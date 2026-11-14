//! 템플릿 편집 창을 headless 로 그리고, 그린 캔버스 좌표로만 누르고 끌어 본다.
//! (여백 테두리 끌기 · 로고 끌기 · 캔버스 누르면 속성 묶음 · 자동 저장 · 실행 취소 · 원래대로)
use super::*;
use crate::tpl_controls::s;
use eframe::egui::{Event, PointerButton, RawInput};
use wp_core::core::Core;

struct H {
    ctx: egui::Context,
    ui: TemplateUi,
    core: Shared,
    dir: std::path::PathBuf,
    time: f64,
}

impl H {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("wp-tpl-{}-{}", std::process::id(), wp_core::util::short_id("t")));
        std::fs::create_dir_all(&dir).unwrap();
        // 빈 DB 를 먼저 만들어 사용자 원본 자동 복사를 막는다
        drop(wp_core::db::BibleStore::open(&dir.join("bible.db")).unwrap());
        let core = Core::open(dir.clone(), dir.clone(), None).unwrap();
        let mut h = H { ctx: egui::Context::default(), ui: TemplateUi::default(), core: std::sync::Arc::new(parking_lot::Mutex::new(core)), dir, time: 0.0 };
        h.ui.open = true;
        h.frame(vec![]);
        h.frame(vec![]);
        h
    }
    fn frame(&mut self, events: Vec<Event>) {
        self.time += 0.05;
        let input = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1600.0, 960.0))), time: Some(self.time), events, ..Default::default() };
        let (dir, core) = (self.dir.clone(), self.core.clone());
        let mut out = self.ctx.run_pass(input, |ctx| self.ui.show(ctx, &core, &dir, &dir));
        out.textures_delta.clear();
    }
    fn wait(&mut self, secs: f64) {
        let n = (secs / 0.05).ceil() as usize + 1;
        for _ in 0..n {
            self.frame(vec![]);
        }
    }
    fn btn(&mut self, p: Pos2, pressed: bool) {
        self.frame(vec![Event::PointerMoved(p), Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: Default::default() }]);
    }
    fn click(&mut self, p: Pos2) {
        self.frame(vec![Event::PointerMoved(p)]);
        self.btn(p, true);
        self.btn(p, false);
        self.frame(vec![]);
    }
    fn drag(&mut self, from: Pos2, to: Pos2) {
        self.frame(vec![Event::PointerMoved(from)]);
        self.btn(from, true);
        for k in 1..=4 {
            self.frame(vec![Event::PointerMoved(from.lerp(to, k as f32 / 4.0))]);
        }
        self.btn(to, false);
        self.frame(vec![]);
    }
    fn hits(&self) -> (Rect, paint::Hits) {
        self.ui.canvas_seen.expect("캔버스를 그리지 않음")
    }
    /// 설정에 저장된 지금 템플릿
    fn saved(&self) -> Value {
        let c = self.core.lock();
        c.settings.templates().into_iter().find(|t| t["id"] == self.ui.sel.as_str()).unwrap()
    }
}

impl Drop for H {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn edge_drag_saves_and_undo_restores() {
    let mut h = H::new();
    let top0 = f(&h.ui.draft, "padding.top", 0.0);
    let (_, hits) = h.hits();
    let area = hits.area.expect("글 영역");
    let from = Pos2::new(area.center().x, area.top());
    h.drag(from, from + Vec2::new(0.0, 40.0));
    let top1 = f(&h.ui.draft, "padding.top", 0.0);
    assert!(top1 > top0 + 20.0, "위 여백이 늘어야 함: {top0} → {top1}");
    assert_eq!(h.ui.last_focus, Some(Sec::Layout), "여백을 끌면 '위치와 여백' 묶음");
    h.wait(0.5);
    assert_eq!(f(&h.saved(), "padding.top", 0.0), top1, "자동 저장");
    assert_eq!(h.ui.save, SaveState::Saved);
    assert_eq!(h.ui.undo.len(), 1, "끌기 한 번 = 실행 취소 한 칸");
    // Ctrl+Z (마우스가 창 위에 있을 때)
    let p = area.center();
    h.frame(vec![Event::PointerMoved(p), Event::Key { key: Key::Z, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::COMMAND }]);
    assert_eq!(f(&h.ui.draft, "padding.top", 0.0), top0, "실행 취소");
    h.wait(0.5);
    assert_eq!(f(&h.saved(), "padding.top", 0.0), top0, "실행 취소도 저장");
    assert_eq!(h.ui.redo.len(), 1);
}

#[test]
fn logo_drag_moves_position_and_canvas_click_focuses() {
    let mut h = H::new();
    set(&mut h.ui.draft, "logo.show", json!(true));
    set(&mut h.ui.draft, "logo.image", json!("/shared/assets/sa-shield.png"));
    set(&mut h.ui.draft, "logo.position", json!("top-right"));
    h.wait(0.5);
    let (rect, hits) = h.hits();
    let lr = hits.logo.expect("로고 자리");
    h.drag(lr.center(), Pos2::new(rect.left() + rect.width() * 0.12, rect.bottom() - rect.height() * 0.12));
    assert_eq!(s(&h.ui.draft, "logo.position"), "bottom-left");
    assert_eq!(h.ui.last_focus, Some(Sec::Logo));
    h.wait(0.5);
    assert_eq!(s(&h.saved(), "logo.position"), "bottom-left");
    // 장절 줄을 누르면 장절 묶음
    let (_, hits) = h.hits();
    h.click(hits.reference.expect("장절 줄").center());
    assert_eq!(h.ui.last_focus, Some(Sec::Ref));
    // 본문을 누르면 글자 묶음
    let (_, hits) = h.hits();
    h.click(hits.body.expect("본문").center());
    assert_eq!(h.ui.last_focus, Some(Sec::Font));
    // '원래대로'는 처음 상태(로고 없음)로
    let orig = h.ui.orig.clone();
    assert_ne!(h.ui.draft, orig);
}

#[test]
fn reopen_with_selects_template_and_revert_restores_original() {
    let mut h = H::new();
    let list = h.core.lock().settings.templates();
    let other = list.iter().find(|t| t["id"] != h.ui.sel.as_str()).unwrap()["id"].as_str().unwrap().to_string();
    h.ui.open_with(&other);
    h.frame(vec![]);
    assert_eq!(h.ui.sel, other);
    let orig = h.ui.draft.clone();
    set(&mut h.ui.draft, "font.size", json!(120));
    h.ui.changed_at = Some(0.0); // 창 밖에서 바꾼 값이므로 저장 대기로 표시
    h.wait(0.5);
    assert_eq!(f(&h.saved(), "font.size", 0.0), 120.0);
    // '원래대로' (창 안 상태 되돌리기)
    let o = h.ui.orig.clone();
    h.ui.undo.push(std::mem::replace(&mut h.ui.draft, o));
    h.ui.changed_at = Some(0.0);
    h.wait(0.5);
    assert_eq!(f(&h.saved(), "font.size", 0.0), f(&orig, "font.size", 0.0));
}
