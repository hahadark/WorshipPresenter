//! 슬라이드 편집기 글쇠 초점 통합 합성 시험: 실제 화면 전체(App::ui_body — shortcuts → 창들(show) 차례 그대로)를
//! headless 로 그리고, 편집기 창 안·밖을 누른 뒤의 글쇠가 어디로 가는지 본다.
//! 편집기는 떠 있는 창이라, 마지막으로 누른 곳이 편집기 안일 때만 요소·목록 글쇠를 받아야 한다.
use super::auditc_tests::{seen, H};
use super::dbl_tests::{at, click_at, dbl_at, full, idle, live};
use eframe::egui::{self, Event, Key, Pos2};
use serde_json::{json, Value};

fn notice() -> Value {
    let b = json!({ "id": "t1", "type": "text", "x": 20, "y": 70, "w": 60, "h": 20, "text": "글상자", "size": 60, "color": "#ffffff", "align": "center", "valign": "middle" });
    json!({ "id": "a", "type": "text", "title": "공지", "body": "첫째 장\n---\n둘째 장\n---\n셋째 장", "edits": { "0": { "items": [b] } } })
}
/// 순서 하나를 미리보기에 올리고 편집기를 연 화면
fn opened() -> H {
    let mut h = H::new(vec![notice()]);
    h.app.sel = Some(0);
    h.app.core.lock().preview_item(0);
    h.app.tc.slideedit.open_at("0");
    idle(&mut h, 5);
    h
}
fn key(h: &mut H, k: Key) {
    for pressed in [true, false] {
        full(h, vec![Event::Key { key: k, physical_key: None, pressed, repeat: false, modifiers: Default::default() }]);
    }
    idle(h, 1);
}
fn el(h: &H) -> Value {
    h.app.core.lock().cue.as_ref().unwrap().items[0]["edits"]["0"]["items"][0].clone()
}
fn pv(h: &H) -> usize {
    h.app.core.lock().pres.preview.as_ref().map(|p| p.index).unwrap_or(99)
}
/// 메인 창의 한 점: 미리보기 화면 안이면서 편집기 창에 가려지지 않은 곳
fn outside(h: &mut H) -> Pos2 {
    idle(h, 2);
    let pane = seen("pane-preview").expect("미리보기 화면");
    let ed = seen("se-canvas").expect("편집기 캔버스");
    // (나) 배치에서는 미리보기 뷰어가 작아 편집기 창에 다 가릴 수 있다 → '보기'·인스펙터 판 머리, 큐시트 표 머리(메인 창의 누를 것 없는 자리)도 본다
    let head = [seen("pro-view-head"), seen("pro-ins-head"), seen("pro-cue-head")].into_iter().flatten().map(|r| r.left_center() + egui::vec2(40.0, 0.0));
    for p in [pane.center(), pane.left_top() + egui::vec2(6.0, 6.0), pane.right_top() + egui::vec2(-6.0, 6.0), pane.left_bottom() + egui::vec2(6.0, -6.0), pane.right_bottom() - egui::vec2(6.0, 6.0)].into_iter().chain(head) {
        let l = h.ctx.layer_id_at(p);
        if l.is_some_and(|l| l.order == egui::Order::Background) {
            return p;
        }
    }
    panic!("시험 전제: 미리보기 화면에 편집기 창이 가리지 않는 곳이 있어야 한다 (미리보기 {pane:?} · 편집기 캔버스 {ed:?})");
}

#[test]
fn editor_selected_then_main_window_click_enter_and_arrows_go_to_main_not_editor() {
    let mut h = opened();
    let p = at(&mut h, "se-el-0");
    click_at(&mut h, p);
    assert_eq!(h.app.tc.slideedit.selected(), "t1");
    assert!(h.app.tc.slideedit.keys_active(), "편집기 안을 누르면 편집기가 글쇠를 받는다");
    let out = outside(&mut h);
    click_at(&mut h, out);
    idle(&mut h, 12);
    assert!(!h.app.tc.slideedit.keys_active(), "메인 창을 누르면 편집기 글쇠가 꺼진다");
    assert!(h.app.tc.slideedit.open, "창은 그대로 떠 있다");
    let before = el(&h);
    // → 는 메인 창의 미리보기 넘기기로 간다 (편집기 요소는 안 움직인다)
    assert_eq!(pv(&h), 0);
    key(&mut h, Key::ArrowRight);
    assert_eq!(el(&h), before, "방향키가 편집기 요소 옮기기로 새지 않는다");
    assert_eq!(pv(&h), 1, "방향키는 메인 창의 미리보기 다음 슬라이드");
    // Enter 는 메인 창의 미리보기 송출로 간다 (편집기 글 고치기가 열리지 않는다)
    assert!(live(&h).is_none());
    key(&mut h, Key::Enter);
    assert!(h.app.tc.slideedit.inline_target().is_none(), "Enter 가 편집기 글 고치기로 새지 않는다");
    assert_eq!(live(&h), Some((0, 1)), "Enter 는 메인 창의 미리보기 송출");
    // Esc 도 편집기 것이 아니다 (고르기 풀기·닫기가 일어나지 않는다)
    key(&mut h, Key::Escape);
    assert_eq!(h.app.tc.slideedit.selected(), "t1");
    assert!(h.app.tc.slideedit.open);
    assert_eq!(el(&h), before);
}

#[test]
fn main_window_click_then_editor_click_arrows_move_the_element_again() {
    let mut h = opened();
    let out = outside(&mut h);
    click_at(&mut h, out);
    idle(&mut h, 12);
    assert!(!h.app.tc.slideedit.keys_active());
    let p0 = pv(&h);
    // 편집기 캔버스의 요소를 다시 누르면 편집기가 글쇠를 받는다
    let p = at(&mut h, "se-el-0");
    click_at(&mut h, p);
    assert!(h.app.tc.slideedit.keys_active());
    assert_eq!(h.app.tc.slideedit.selected(), "t1");
    key(&mut h, Key::ArrowRight);
    assert_eq!(el(&h)["x"].as_f64(), Some(20.5), "방향키 = 고른 요소 0.5% 옮기기");
    assert_eq!(pv(&h), p0, "그 방향키가 메인 창 미리보기를 넘기지 않는다");
    assert!(live(&h).is_none());
    // 편집기 목록을 누른 것도 편집기 안: 고른 것이 없으니 ↓ = 편집기의 다음 슬라이드 (미리보기는 그대로)
    click_at(&mut h, out);
    idle(&mut h, 12);
    key(&mut h, Key::Escape);
    assert_eq!(h.app.tc.slideedit.selected(), "t1", "밖을 누른 뒤의 Esc 는 편집기 것이 아니다");
    let canvas = at(&mut h, "se-canvas");
    let main = seen("se-main").unwrap();
    let empty = seen("se-canvas").unwrap().left_top() + egui::vec2(3.0, 3.0);
    assert!(!main.contains(empty) && canvas != empty);
    click_at(&mut h, empty);
    assert!(h.app.tc.slideedit.keys_active());
    assert_eq!(h.app.tc.slideedit.selected(), "", "캔버스 빈 곳 = 고르기 풀기");
}

#[test]
fn inline_edit_then_main_window_click_escape_does_not_reach_editor() {
    let mut h = opened();
    let out = outside(&mut h); // 글 고치는 중에는 캔버스 표시가 없어 먼저 잡아 둔다
    let p = at(&mut h, "se-el-0");
    dbl_at(&mut h, p);
    assert_eq!(h.app.tc.slideedit.inline_target().as_deref(), Some("t1"), "두 번 누르면 글 바로 고치기");
    idle(&mut h, 2);
    for ch in ["은", "혜"] {
        full(&mut h, vec![Event::Text(ch.into())]);
    }
    // 글 고치는 중에 메인 창을 누른다 → 글칸이 초점을 잃어 저장되고, 편집기 글쇠는 꺼진다
    click_at(&mut h, out);
    idle(&mut h, 12);
    assert!(h.app.tc.slideedit.inline_target().is_none());
    assert_eq!(el(&h)["text"], "은혜", "다른 곳을 누르면 저장 (원본 blur)");
    assert!(!h.app.tc.slideedit.keys_active());
    let before = el(&h);
    key(&mut h, Key::Escape);
    assert!(h.app.tc.slideedit.open, "그 뒤의 Esc 가 편집기 닫기로 새지 않는다");
    assert_eq!(h.app.tc.slideedit.selected(), "t1", "고르기 풀기로도 새지 않는다");
    assert_eq!(el(&h), before, "글도 그대로 (그만두기로 처리되지 않는다)");
}
