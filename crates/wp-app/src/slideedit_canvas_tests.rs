//! 슬라이드 편집기 캔버스 합성 입력 시험: 실제 편집기(SlideEditUi::show)를 headless 로 그리고
//! 포인터·글쇠·클립보드 사건(egui Event)을 넣어 저장된 큐시트 순서를 본다. 실제 PC 클립보드는 건드리지 않는다
//! (복사는 egui 출력 명령 CopyText 로, 붙여 넣기는 Event::Paste 로).
//! 프레임 차례는 실제 화면(App::ui_body → shortcuts)과 같게: pre_keys → 목록 키 → (전역 단축키 자리) → 편집기 창.
//! '전역 단축키 자리'에서 아직 남아 있는 글쇠는 leaked 에 적는다 — 편집기가 가져가야 할 글쇠가 전역으로 새지 않는지 본다.
use super::*;
use eframe::egui::{Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect};
use wp_core::core::Core;

const WATCH: [Key; 9] = [Key::Enter, Key::F2, Key::Delete, Key::Backspace, Key::ArrowLeft, Key::ArrowRight, Key::ArrowUp, Key::ArrowDown, Key::Escape];

struct H {
    ctx: egui::Context,
    ui: SlideEditUi,
    core: Shared,
    dir: std::path::PathBuf,
    time: f64,
    /// 편집기가 가져가지 않아 전역 단축키까지 간 글쇠
    leaked: Vec<Key>,
    /// 클립보드에 쓰려고 한 글 (egui 출력 명령)
    copied: Vec<String>,
}
impl H {
    fn new(item: Value) -> Self {
        let dir = std::env::temp_dir().join(format!("wp-secanvas-{}-{}", std::process::id(), wp_core::util::short_id("t")));
        std::fs::create_dir_all(&dir).unwrap();
        // 빈 DB 를 먼저 만든다 (Core::open 이 사용자 자료를 복사해 오지 않게)
        drop(wp_core::db::BibleStore::open(&dir.join("bible.db")).unwrap());
        let mut core = Core::open(dir.clone(), dir.clone(), None).unwrap();
        core.new_cue("합성입력 전용").unwrap();
        core.add_item(item, None);
        core.b.undo = Default::default();
        core.b.typing = None;
        core.toast = None;
        let mut ui = SlideEditUi::default();
        ui.open = true;
        let mut h = Self { ctx: egui::Context::default(), ui, core: std::sync::Arc::new(wp_core::lockcheck::Mutex::new(core)), dir, time: 0.0, leaked: vec![], copied: vec![] };
        h.idle(4);
        h
    }
    fn frame_m(&mut self, mut events: Vec<Event>, mods: Modifiers) {
        events.insert(0, Event::ModifiersChanged(mods));
        self.time += 0.05;
        crate::widgets::HOVER_SEEN.with(|m| m.borrow_mut().clear());
        let input = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1600.0, 1100.0))), time: Some(self.time), events, ..Default::default() };
        let mut acts = vec![];
        let (ui, core, dir, leaked) = (&mut self.ui, &self.core, &self.dir, &mut self.leaked);
        let mut out = self.ctx.run_pass(input, |ctx| {
            ui.pre_keys(ctx);
            if !ctx.egui_wants_keyboard_input() {
                ui.list_keys(ctx);
            }
            for k in WATCH {
                if ctx.input(|i| i.key_pressed(k)) {
                    leaked.push(k);
                }
            }
            ui.show(ctx, core, Some(0), dir, dir, &mut acts);
        });
        out.textures_delta.clear();
        for c in &out.platform_output.commands {
            if let egui::OutputCommand::CopyText(t) = c {
                self.copied.push(t.clone());
            }
        }
        // 편집기가 낸 저장 요청을 실제 화면(App::apply_acts 의 UpdateItem)과 같은 코어 호출로 반영한다
        for a in acts {
            if let Act::UpdateItem(i, it) = a {
                let mut c = self.core.lock();
                c.remember_typing(it["id"].as_str().unwrap_or(""));
                c.cue.as_mut().unwrap().items[i] = it;
                c.save_cue();
            }
        }
    }
    fn frame(&mut self, events: Vec<Event>) {
        self.frame_m(events, Modifiers::NONE);
    }
    fn idle(&mut self, n: usize) {
        for _ in 0..n {
            self.frame(vec![]);
        }
    }
    fn seen(&mut self, name: &str) -> Rect {
        self.idle(2);
        crate::widgets::HOVER_SEEN.with(|m| m.borrow().get(name).copied()).unwrap_or_else(|| panic!("그린 자리 없음: {name} (있는 것: {:?})", crate::widgets::HOVER_SEEN.with(|m| m.borrow().keys().cloned().collect::<Vec<_>>())))
    }
    fn has(&mut self, name: &str) -> bool {
        self.idle(2);
        crate::widgets::HOVER_SEEN.with(|m| m.borrow().contains_key(name))
    }
    fn press(&mut self, p: Pos2, pressed: bool, mods: Modifiers) {
        self.frame_m(vec![Event::PointerMoved(p), Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: mods }], mods);
    }
    fn click_at(&mut self, p: Pos2) {
        self.frame(vec![Event::PointerMoved(p)]);
        self.press(p, true, Modifiers::NONE);
        self.press(p, false, Modifiers::NONE);
        self.idle(1);
    }
    fn click(&mut self, name: &str) {
        let p = self.seen(name).center();
        self.click_at(p);
    }
    fn drag(&mut self, from: Pos2, to: Pos2, mods: Modifiers) {
        self.frame_m(vec![Event::PointerMoved(from)], mods);
        self.press(from, true, mods);
        self.frame_m(vec![Event::PointerMoved(from.lerp(to, 0.35))], mods);
        self.frame_m(vec![Event::PointerMoved(to)], mods);
        self.press(to, false, mods);
        self.idle(2);
    }
    /// 캔버스 백분율만큼 끌기
    fn drag_pct(&mut self, from: Pos2, dx: f32, dy: f32, mods: Modifiers) {
        let c = self.seen("se-canvas");
        self.drag(from, from + Vec2::new(c.width() * dx / 100.0, c.height() * dy / 100.0), mods);
    }
    fn key(&mut self, k: Key, mods: Modifiers) {
        for pressed in [true, false] {
            self.frame_m(vec![Event::Key { key: k, physical_key: None, pressed, repeat: false, modifiers: mods }], mods);
        }
        self.idle(1);
    }
    fn item(&self) -> Value {
        self.core.lock().cue.as_ref().unwrap().items[0].clone()
    }
    fn items(&self, key: &str) -> Vec<Value> {
        self.item()["edits"][key]["items"].as_array().cloned().unwrap_or_default()
    }
    fn undo(&self) -> bool {
        self.core.lock().undo_items(false)
    }
    /// 저장까지 됐는지 (자동 저장): DB 의 큐시트가 메모리와 같다
    fn assert_saved(&self) {
        let c = self.core.lock();
        let cue = c.cue.as_ref().unwrap();
        assert_eq!(c.store.get_cue(&cue.id).unwrap().items[0], cue.items[0], "바뀐 것이 큐시트에 저장돼 있어야 한다");
    }
}
impl Drop for H {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
fn n(v: &Value) -> f64 {
    v.as_f64().unwrap_or(f64::NAN)
}
fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.11
}
fn tbox() -> Value {
    json!({ "id": "t1", "type": "text", "x": 20, "y": 70, "w": 60, "h": 20, "text": "글상자", "size": 60, "color": "#ffffff", "align": "center", "valign": "middle" })
}
fn shape(id: &str) -> Value {
    json!({ "id": id, "type": "shape", "x": 10, "y": 10, "w": 20, "h": 10, "fill": "#1d3557", "below": false })
}
fn notice(edits: Value) -> Value {
    json!({ "id": "a", "type": "text", "title": "공지", "body": "첫째 장\n---\n둘째 장\n---\n셋째 장\n---\n넷째 장", "edits": edits })
}

#[test]
fn editor_click_selects_box_empty_canvas_or_esc_deselects_and_esc_closes() {
    let mut h = H::new(notice(json!({ "0": { "items": [tbox()] } })));
    assert_eq!(h.ui.sel, "");
    assert!(!h.has("se-h-se"), "고른 것이 없으면 손잡이가 없다");
    h.click("se-el-0");
    assert_eq!(h.ui.sel, "t1", "덧붙인 글상자를 누르면 고른다");
    assert!(h.has("se-h-se") && h.has("se-h-nw"), "고른 상자에 크기 조절 손잡이가 나온다");
    h.click("se-main");
    assert_eq!(h.ui.sel, "main", "본문 글상자도 고른다");
    // 빈 곳 (본문·요소 바깥)
    let (canvas, main) = (h.seen("se-canvas"), h.seen("se-main"));
    let empty = canvas.left_top() + Vec2::new(3.0, 3.0);
    assert!(!main.contains(empty), "시험 전제: 캔버스 모서리는 본문 바깥 (본문 {main:?} · 캔버스 {canvas:?})");
    h.click_at(empty);
    assert_eq!(h.ui.sel, "", "빈 곳을 누르면 고르기가 풀린다");
    h.click("se-el-0");
    h.key(Key::Escape, Modifiers::NONE);
    assert_eq!(h.ui.sel, "", "Esc = 고르기 풀기");
    assert!(h.ui.open, "고른 것이 있을 때의 Esc 는 닫지 않는다");
    h.key(Key::Escape, Modifiers::NONE);
    assert!(!h.ui.open, "고른 것이 없을 때의 Esc = 편집기 닫기");
    assert!(h.leaked.is_empty(), "Esc 가 전역 단축키(예배당 화면 끄기)로 새지 않는다: {:?}", h.leaked);
    assert_eq!(h.items("0").len(), 1, "고르기만으로는 바뀌는 것이 없다");
}

#[test]
fn editor_delete_removes_own_element_only_not_main_box_or_common_element_and_undo_restores() {
    let mut common = shape("c1");
    common["x"] = json!(70);
    let mut h = H::new(notice(json!({ "*": { "items": [common] }, "0": { "items": [tbox()], "box": { "x": 10, "y": 20, "w": 80, "h": 40 } } })));
    let before = h.item();
    // 본문 글상자: Delete 로 지워지지 않는다
    h.click("se-main");
    assert_eq!(h.ui.sel, "main");
    h.key(Key::Delete, Modifiers::NONE);
    assert_eq!(h.item(), before, "본문 글상자는 Delete 로 지워지지 않는다");
    // 공통 요소: 슬라이드에서는 고르기만 (손잡이 없음 · 끌어도 안 움직임 · Delete·방향키·Ctrl+D 안 됨)
    h.click("se-common-c1");
    assert_eq!(h.ui.sel, "c1", "공통 요소도 고르기는 된다");
    assert!(!h.has("se-h-se"), "공통(잠긴) 요소에는 손잡이가 없다");
    let p = h.seen("se-common-c1").center();
    h.drag_pct(p, 8.0, 6.0, Modifiers::NONE);
    h.key(Key::Delete, Modifiers::NONE);
    h.key(Key::ArrowRight, Modifiers::NONE);
    h.key(Key::D, Modifiers::COMMAND);
    h.key(Key::Enter, Modifiers::NONE);
    assert_eq!(h.item(), before, "공통 요소는 슬라이드에서 옮기거나 지우거나 복제할 수 없다");
    assert!(h.ui.inline.is_none());
    // 덧붙인 요소: Delete 로 지운다 (Backspace 도)
    h.click("se-el-0");
    h.key(Key::Delete, Modifiers::NONE);
    assert!(h.items("0").is_empty(), "고른 덧붙인 요소는 Delete 로 지운다");
    assert_eq!(h.ui.sel, "");
    assert_eq!(h.item()["edits"]["*"], before["edits"]["*"], "공통 것은 그대로");
    assert_eq!((n(&h.item()["edits"]["0"]["box"]["x"]), n(&h.item()["edits"]["0"]["box"]["h"])), (10.0, 40.0), "본문 자리도 그대로");
    h.assert_saved();
    assert!(h.undo(), "되돌릴 것이 있다");
    assert_eq!(h.items("0").len(), 1, "되돌리기 한 번으로 지운 요소가 돌아온다");
    assert_eq!(h.items("0")[0]["id"], "t1");
    h.idle(2);
    h.click("se-el-0");
    h.key(Key::Backspace, Modifiers::NONE);
    assert!(h.items("0").is_empty(), "Backspace 도 지우기");
    assert!(h.leaked.is_empty(), "Delete·방향키·Enter 가 전역 단축키로 새지 않는다: {:?}", h.leaked);
}

#[test]
fn editor_arrows_nudge_selected_box_shift_is_big_step_and_without_selection_change_slide() {
    let mut h = H::new(notice(json!({ "0": { "items": [tbox()] } })));
    h.click("se-el-0");
    h.key(Key::ArrowRight, Modifiers::NONE);
    assert_eq!((n(&h.items("0")[0]["x"]), n(&h.items("0")[0]["y"])), (20.5, 70.0), "→ = 0.5% 오른쪽");
    h.key(Key::ArrowDown, Modifiers::SHIFT);
    assert_eq!((n(&h.items("0")[0]["x"]), n(&h.items("0")[0]["y"])), (20.5, 75.0), "Shift+↓ = 5% 아래");
    h.key(Key::ArrowLeft, Modifiers::NONE);
    h.key(Key::ArrowUp, Modifiers::NONE);
    assert_eq!((n(&h.items("0")[0]["x"]), n(&h.items("0")[0]["y"])), (20.0, 74.5));
    assert_eq!(n(&h.items("0")[0]["w"]), 60.0, "크기는 그대로");
    h.assert_saved();
    // 글쇠 한 번 = 되돌리기 한 번
    assert!(h.undo());
    assert_eq!((n(&h.items("0")[0]["x"]), n(&h.items("0")[0]["y"])), (20.0, 75.0), "되돌리기는 마지막 한 번만 무른다");
    h.idle(2);
    // 본문 글상자: 자리(box)가 없으면 템플릿 본문 영역에서 만들어 옮긴다
    assert!(h.item()["edits"]["0"]["box"].is_null());
    h.click("se-main");
    h.key(Key::ArrowRight, Modifiers::SHIFT);
    let b = h.item()["edits"]["0"]["box"].clone();
    assert!(b.is_object(), "본문을 방향키로 옮기면 본문 자리(box)가 생긴다: {b}");
    let x0 = n(&b["x"]);
    h.key(Key::ArrowLeft, Modifiers::NONE);
    assert!(near(n(&h.item()["edits"]["0"]["box"]["x"]), x0 - 0.5));
    assert_eq!(h.ui.k, 0, "상자를 고른 중에는 슬라이드가 바뀌지 않는다");
    // 고른 것이 없으면 방향키 = 앞·뒤 슬라이드
    h.key(Key::Escape, Modifiers::NONE);
    let before = h.item();
    h.key(Key::ArrowDown, Modifiers::NONE);
    assert_eq!(h.ui.k, 1, "↓ = 다음 슬라이드");
    h.key(Key::ArrowRight, Modifiers::NONE);
    assert_eq!(h.ui.k, 2, "→ = 다음 슬라이드");
    h.key(Key::ArrowUp, Modifiers::NONE);
    h.key(Key::ArrowLeft, Modifiers::NONE);
    h.key(Key::ArrowLeft, Modifiers::NONE);
    assert_eq!(h.ui.k, 0, "←·↑ = 이전 슬라이드 (처음에서 멈춤)");
    assert_eq!(h.item(), before, "슬라이드만 바꾸면 저장되는 것이 없다");
    assert!(h.leaked.is_empty(), "방향키가 전역 단축키(미리보기 넘기기·큐시트 이동)로 새지 않는다: {:?}", h.leaked);
}

#[test]
fn editor_ctrl_d_duplicates_and_copy_cut_paste_work_through_egui_clipboard_events() {
    let mut h = H::new(notice(json!({ "0": { "items": [tbox()] } })));
    // 고른 것이 없으면 Ctrl+D·복사는 아무 일도 없다
    h.key(Key::D, Modifiers::COMMAND);
    h.frame(vec![Event::Copy]);
    assert_eq!(h.items("0").len(), 1);
    assert!(h.copied.is_empty());
    // Ctrl+D = 복제 (2% 비켜서, 새 것을 고른다)
    h.click("se-el-0");
    h.key(Key::D, Modifiers::COMMAND);
    let v = h.items("0");
    assert_eq!(v.len(), 2, "Ctrl+D = 복제");
    assert_eq!((n(&v[1]["x"]), n(&v[1]["y"]), v[1]["text"].as_str()), (22.0, 72.0, Some("글상자")));
    assert_ne!(v[1]["id"], v[0]["id"]);
    assert_eq!(h.ui.sel, v[1]["id"].as_str().unwrap(), "복제한 것을 고른다");
    assert!(h.undo(), "복제도 되돌리기 한 번");
    assert_eq!(h.items("0").len(), 1);
    h.idle(2);
    // 복사 → 붙여 넣기 (같은 글이 들어오면 그 요소를 붙인다)
    h.click("se-el-0");
    h.frame(vec![Event::Copy]);
    assert_eq!(h.copied, vec!["글상자".to_string()], "글상자를 복사하면 그 글이 클립보드로 간다");
    assert_eq!(h.items("0").len(), 1, "복사만으로는 바뀌지 않는다");
    h.frame(vec![Event::Paste("글상자".into())]);
    h.idle(1);
    let v = h.items("0");
    assert_eq!(v.len(), 2, "붙여 넣기 = 복사한 요소 하나 더");
    assert_eq!((n(&v[1]["x"]), n(&v[1]["y"]), n(&v[1]["w"]), v[1]["align"].as_str()), (22.0, 72.0, 60.0, Some("center")), "꾸민 것까지 그대로, 조금 비켜서");
    h.frame(vec![Event::Paste("글상자".into())]);
    h.idle(1);
    assert_eq!(n(&h.items("0")[2]["x"]), 24.0, "거듭 붙이면 더 비켜서");
    // 잘라 내기 = 복사 + 지우기
    let cut_id = h.ui.sel.clone();
    h.frame(vec![Event::Cut]);
    h.idle(1);
    assert_eq!(h.items("0").len(), 2, "잘라 내면 고른 요소가 없어진다");
    assert!(h.items("0").iter().all(|x| x["id"].as_str() != Some(cut_id.as_str())));
    assert_eq!(h.copied.len(), 2);
    // 다른 글을 붙이면 새 글상자 (원본 pasteText)
    h.frame(vec![Event::Paste("주님의 은혜\r\n둘째 줄".into())]);
    h.idle(1);
    let v = h.items("0");
    assert_eq!(v.len(), 3);
    assert_eq!((v[2]["text"].as_str(), n(&v[2]["x"]), n(&v[2]["w"]), v[2]["fit"].as_bool()), (Some("주님의 은혜\n둘째 줄"), 10.0, 80.0, Some(true)), "클립보드 글 = 새 글상자");
    assert_eq!(h.ui.sel, v[2]["id"].as_str().unwrap());
    h.assert_saved();
    // Shift 가 눌린 붙여 넣기(Ctrl+Shift+V)는 '종류 골라 붙여 넣기' 창 몫이라 편집기가 가져가지 않는다
    h.frame_m(vec![Event::Paste("안 붙는 글".into())], Modifiers::SHIFT | Modifiers::COMMAND);
    h.idle(1);
    assert_eq!(h.items("0").len(), 3);
}

#[test]
fn editor_enter_and_f2_open_inline_text_edit_for_selected_text_box_or_main() {
    let mut h = H::new(notice(json!({ "0": { "items": [tbox(), shape("s1")] } })));
    h.click("se-el-0");
    h.key(Key::Enter, Modifiers::NONE);
    assert_eq!(h.ui.inline_target().as_deref(), Some("t1"), "Enter = 고른 글상자 글 바로 고치기");
    h.idle(2);
    for ch in ["새", " ", "글"] {
        h.frame(vec![Event::Text(ch.into())]);
    }
    h.key(Key::Enter, Modifiers::COMMAND);
    h.idle(1);
    assert!(h.ui.inline.is_none());
    assert_eq!(h.items("0")[0]["text"], "새 글", "Ctrl+Enter = 저장");
    // 글 고치는 중의 Ctrl+Enter 는 여기서 가져가지 않는다 (전역 쪽이 inline_active() 로 막는다 — dbl_tests 가 본다)
    assert_eq!(h.leaked, vec![Key::Enter]);
    h.leaked.clear();
    h.idle(12);
    // 도형은 글이 없다
    h.click("se-el-1");
    assert_eq!(h.ui.sel, "s1");
    h.key(Key::Enter, Modifiers::NONE);
    h.key(Key::F2, Modifiers::NONE);
    assert!(h.ui.inline.is_none(), "도형·그림은 Enter·F2 로 글 고치기가 열리지 않는다");
    // 고른 것이 없으면 본문
    h.key(Key::Escape, Modifiers::NONE);
    assert_eq!(h.ui.sel, "");
    h.key(Key::F2, Modifiers::NONE);
    assert_eq!(h.ui.inline_target().as_deref(), Some("main"), "F2 (고른 것 없음) = 본문 글 바로 고치기");
    h.idle(2);
    h.key(Key::Escape, Modifiers::NONE);
    h.idle(1);
    assert!(h.ui.inline.is_none(), "글 고치는 중의 Esc = 그만두기");
    assert!(h.ui.open, "그 Esc 로 편집기가 닫히지는 않는다");
    assert_eq!(h.item()["body"], "첫째 장\n---\n둘째 장\n---\n셋째 장\n---\n넷째 장");
    assert!(!h.leaked.contains(&Key::F2) && !h.leaked.iter().any(|k| *k == Key::Enter), "Enter·F2 가 전역(미리보기 송출·순서 고치기)으로 새지 않는다: {:?}", h.leaked);
}

#[test]
fn editor_resize_handles_change_size_and_position_shift_keeps_ratio_and_drag_is_one_undo() {
    let mut h = H::new(notice(json!({ "0": { "items": [tbox()] } })));
    h.click("se-el-0");
    let rect = |h: &H| {
        let x = &h.items("0")[0];
        (n(&x["x"]), n(&x["y"]), n(&x["w"]), n(&x["h"]))
    };
    let is = |a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)| near(a.0, b.0) && near(a.1, b.1) && near(a.2, b.2) && near(a.3, b.3);
    // 오른쪽 아래 손잡이: 너비·높이만
    let p = h.seen("se-h-se").center();
    h.drag_pct(p, 10.0, 5.0, Modifiers::NONE);
    assert!(is(rect(&h), (20.0, 70.0, 70.0, 25.0)), "se 손잡이: {:?}", rect(&h));
    assert_eq!(h.ui.sel, "t1", "크기를 바꿔도 고른 채로");
    h.assert_saved();
    // 왼쪽 위 손잡이: 자리도 같이 (오른쪽 아래 끝은 그대로)
    let p = h.seen("se-h-nw").center();
    h.drag_pct(p, 5.0, 5.0, Modifiers::NONE);
    assert!(is(rect(&h), (25.0, 75.0, 65.0, 20.0)), "nw 손잡이: {:?}", rect(&h));
    // 되돌리기: 끌기 한 번이 한 번
    assert!(h.undo());
    assert!(is(rect(&h), (20.0, 70.0, 70.0, 25.0)), "되돌리기 한 번 = 마지막 끌기만: {:?}", rect(&h));
    h.idle(2);
    // Shift = 처음 비율 유지 (70:25)
    let p = h.seen("se-h-e").center();
    h.drag_pct(p, -20.0, 0.0, Modifiers::SHIFT);
    let r = rect(&h);
    assert!(near(r.2, 50.0) && near(r.3, 50.0 / (70.0 / 25.0)), "Shift 끌기는 비율을 지킨다: {r:?}");
    // 가장 작게 2%
    let p = h.seen("se-h-w").center();
    h.drag_pct(p, 70.0, 0.0, Modifiers::NONE);
    let r = rect(&h);
    assert!(near(r.2, 2.0) && near(r.0 + r.2, 70.0), "너비는 2% 아래로 안 줄고 오른쪽 끝을 지킨다: {r:?}");
    // 본문 글상자도 손잡이로: 본문 자리(box)가 생긴다
    h.click("se-main");
    assert!(h.item()["edits"]["0"]["box"].is_null());
    let m0 = h.seen("se-main");
    let p = h.seen("se-h-se").center();
    h.drag_pct(p, -10.0, -10.0, Modifiers::NONE);
    let b = h.item()["edits"]["0"]["box"].clone();
    let canvas = h.seen("se-canvas");
    let w0 = (m0.width() / canvas.width() * 100.0) as f64;
    assert!(b.is_object() && (n(&b["w"]) - (w0 - 10.0)).abs() < 0.3, "본문 너비가 10% 줄어든다: {b} (처음 {w0})");
    h.assert_saved();
}

#[test]
fn editor_drag_moves_box_snaps_to_center_and_edges_alt_disables_snap() {
    let mut h = H::new(notice(json!({ "0": { "items": [shape("s1")] } })));
    let pos = |h: &H| (n(&h.items("0")[0]["x"]), n(&h.items("0")[0]["y"]));
    // 오른쪽 가장자리가 가운데 선(50%) 근처면 거기 붙는다: 10 + 20.6 → 오른쪽 끝 50.6 → 50
    let p = h.seen("se-el-0").center();
    h.drag_pct(p, 20.6, 0.0, Modifiers::NONE);
    assert_eq!(pos(&h), (30.0, 10.0), "오른쪽 가장자리가 가운데 선에 붙는다");
    // 상자 가운데가 50% 근처면 가운데 맞춤: 30 + 10.6 → 가운데 50.6 → 50
    let p = h.seen("se-el-0").center();
    h.drag_pct(p, 10.6, 0.0, Modifiers::NONE);
    assert_eq!(pos(&h), (40.0, 10.0), "가운데에 붙는다");
    assert_eq!(h.ui.sel, "s1", "끌면 고른다");
    // 왼쪽 가장자리가 50 근처면 거기 붙는다: 40 + 9.5 = 49.5 → 50
    let p = h.seen("se-el-0").center();
    h.drag_pct(p, 9.5, 0.0, Modifiers::NONE);
    assert_eq!(pos(&h).0, 50.0, "가장자리도 붙는다");
    // Alt = 안 붙음
    let p = h.seen("se-el-0").center();
    h.drag_pct(p, -0.5 - 10.0, 3.0, Modifiers::ALT);
    let (x, y) = pos(&h);
    assert!(near(x, 39.5) && near(y, 13.0), "Alt 를 누르면 붙지 않는다: {x} {y}");
    assert_eq!((n(&h.items("0")[0]["w"]), n(&h.items("0")[0]["h"])), (20.0, 10.0), "옮겨도 크기는 그대로");
    h.assert_saved();
    assert!(h.undo());
    assert_eq!(pos(&h).0, 50.0, "끌기 한 번 = 되돌리기 한 번");
}

#[test]
fn editor_list_shift_drag_selects_a_range_without_moving_slides() {
    let mut h = H::new(notice(json!({})));
    let before = h.item();
    h.idle(2);
    let p = |h: &H, k: usize| h.ui.observed[&format!("slide-{k}")].center();
    let (a, b) = (p(&h, 0), p(&h, 2));
    h.drag(a, b, Modifiers::SHIFT);
    assert_eq!(h.ui.multi, vec!["0".to_string(), "1".into(), "2".into()], "Shift 누르고 끌면 지나간 슬라이드까지 고른다");
    assert_eq!(h.item(), before, "고르기만 한다 (차례는 안 바뀐다)");
    // 거꾸로 끌어도 범위
    let (a, b) = (p(&h, 3), p(&h, 1));
    h.drag(a, b, Modifiers::SHIFT);
    assert_eq!(h.ui.multi, vec!["1".to_string(), "2".into(), "3".into()]);
    assert_eq!(h.item(), before);
    // 그냥 끌기는 그대로 차례 옮기기 (기존 동작)
    h.key(Key::Escape, Modifiers::NONE);
    let (a, b) = (p(&h, 0), p(&h, 1));
    h.drag(a, b, Modifiers::NONE);
    assert!(h.item()["body"].as_str().unwrap().starts_with("둘째 장"), "그냥 끌기 = 차례 옮기기: {}", h.item()["body"]);
}

#[test]
fn editor_element_keys_apply_to_all_selected_slides_together() {
    let mut h = H::new(notice(json!({})));
    h.ui.open_many(&["0".to_string(), "1".to_string()]);
    h.idle(3);
    assert_eq!(h.ui.multi.len(), 2);
    h.frame(vec![Event::Paste("함께".into())]);
    h.idle(1);
    let (a, b) = (h.items("0"), h.items("1"));
    assert_eq!((a.len(), b.len()), (1, 1), "붙여 넣은 글상자가 고른 두 장에 함께 들어간다");
    assert_eq!((a[0]["id"].as_str(), a[0]["text"].as_str(), b[0]["text"].as_str()), (b[0]["id"].as_str(), Some("함께"), Some("함께")), "같은 요소(id)로 들어간다");
    h.key(Key::ArrowRight, Modifiers::NONE);
    assert_eq!((n(&h.items("0")[0]["x"]), n(&h.items("1")[0]["x"])), (10.5, 10.5), "방향키 이동도 함께");
    h.key(Key::Delete, Modifiers::NONE);
    assert!(h.item().get("edits").is_none(), "지우기도 함께 (빈 편집은 없어진다): {}", h.item());
    assert!(h.items("2").is_empty());
}

/// 끌어다 놓은·붙여 넣은 그림은 놓은 자리(없으면 가운데)에 그림 요소로 들어가고 저장된다 (slideedit.js dropImage)
#[test]
fn dropped_and_pasted_images_become_image_elements() {
    let _g = crate::dnd::TEST_LOCK.lock();
    let _ = crate::dnd::take_se_images();
    let mut h = H::new(notice(json!({})));
    let c = h.seen("se-canvas");
    // 캔버스 한가운데에 놓기: 30% 그림의 중심이 놓은 자리 → x·y = 35
    crate::dnd::push_se_image("/lib/file?p=a.png", Some(c.center()));
    h.idle(2);
    // 자리 없음(붙여 넣기)도 35
    crate::dnd::push_se_image("/media/clip-1.png", None);
    h.idle(2);
    // 왼쪽 위 모서리 바깥쪽에 놓으면 캔버스 밖이라 35
    let all: Vec<Value> = h.item()["edits"].as_object().unwrap().values().flat_map(|e| e["items"].as_array().cloned().unwrap_or_default()).collect();
    let imgs: Vec<&Value> = all.iter().filter(|x| x["type"] == "image").collect();
    assert_eq!(imgs.len(), 2, "{all:?}");
    assert_eq!(imgs[0]["src"], "/lib/file?p=a.png");
    assert!(near(n(&imgs[0]["x"]), 35.0) && near(n(&imgs[0]["y"]), 35.0) && near(n(&imgs[0]["w"]), 30.0), "{:?}", imgs[0]);
    assert_eq!(imgs[1]["src"], "/media/clip-1.png");
    assert!(near(n(&imgs[1]["x"]), 35.0));
    h.assert_saved();
    // 실행 취소 한 번에 하나씩
    assert!(h.undo());
}