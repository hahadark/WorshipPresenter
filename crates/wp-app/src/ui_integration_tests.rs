//! 실제 편집기 위젯을 headless로 렌더하고 관찰한 Response 좌표로만 입력한다.
//! GUI/출력 프로세스 없이 실제 편집기·작업공간 입력과 저장을 검사한다.
use crate::slideedit_ui::SlideEditUi;
use eframe::egui::{self, Event, PointerButton, Pos2, RawInput, Rect, Vec2};
use serde_json::{Value, json};
use wp_core::core::{Core, Shared};

struct Harness {
    ctx: egui::Context,
    ui: SlideEditUi,
    core: Shared,
    dir: std::path::PathBuf,
    time: f64,
}
impl Harness {
    fn new(item: Value) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "wp-ui-{}-{}",
            std::process::id(),
            wp_core::util::short_id("test")
        ));
        std::fs::create_dir_all(&dir).unwrap();
        // 반드시 먼저 빈 DB를 생성한다. Core::open의 사용자 원본 자동 복사 방지.
        drop(wp_core::db::BibleStore::open(&dir.join("bible.db")).unwrap());
        let mut core = Core::open(dir.clone(), dir.clone(), None).unwrap();
        core.new_cue("합성입력 전용").unwrap();
        core.add_item(item, None); // 테스트 준비일 뿐 UI 입력 성공으로 세지 않는다.
        let mut ui = SlideEditUi::default();
        ui.open = true;
        let mut h = Self {
            ctx: egui::Context::default(),
            ui,
            core: std::sync::Arc::new(wp_core::lockcheck::Mutex::new(core)),
            dir,
            time: 0.0,
        };
        h.frame(vec![]);
        h.frame(vec![]);
        h
    }
    fn frame(&mut self, events: Vec<Event>) {
        self.time += 0.05;
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1600.0, 1100.0))),
            time: Some(self.time),
            events,
            ..Default::default()
        };
        let mut acts = vec![];
        let mut output = self.ctx.run_pass(input, |ctx| {
            self.ui
                .show(ctx, &self.core, Some(0), &self.dir, &self.dir, &mut acts)
        });
        output.textures_delta.clear(); // GPU 없는 테스트에서는 명시적으로 소비한다.
        // 실제 UI가 낸 액션만 production과 동일한 코어 저장 API로 전달한다.
        headless_app(self).apply(acts);
    }
    fn point(&self, name: &str) -> Pos2 {
        let rect = self
            .ui
            .observed
            .get(name)
            .unwrap_or_else(|| panic!("실제 위젯 좌표 없음: {name}"));
        assert!(rect.is_positive());
        rect.center()
    }
    fn button(&mut self, p: Pos2, button: PointerButton, pressed: bool) {
        self.frame(vec![
            Event::PointerMoved(p),
            Event::PointerButton {
                pos: p,
                button,
                pressed,
                modifiers: Default::default(),
            },
        ]);
    }
    fn click(&mut self, name: &str, button: PointerButton) {
        let p = self.point(name);
        self.frame(vec![Event::PointerMoved(p)]);
        self.button(p, button, true);
        self.button(p, button, false);
        self.frame(vec![]);
    }
    fn drag(&mut self, from: Pos2, to: Pos2) {
        self.frame(vec![Event::PointerMoved(from)]);
        self.button(from, PointerButton::Primary, true);
        self.frame(vec![Event::PointerMoved(from.lerp(to, 0.35))]);
        self.frame(vec![Event::PointerMoved(to)]);
        self.button(to, PointerButton::Primary, false);
        self.frame(vec![]);
    }
    fn item(&self) -> Value {
        self.core.lock().cue.as_ref().unwrap().items[0].clone()
    }
    fn assert_saved(&self) {
        let c = self.core.lock();
        let cue = c.cue.as_ref().unwrap();
        assert_eq!(c.store.get_cue(&cue.id).unwrap().items[0], cue.items[0]);
        // 별도 연결로도 커밋된 내용을 확인한다.
        let reopened = wp_core::db::BibleStore::open(&self.dir.join("bible.db")).unwrap();
        assert_eq!(reopened.get_cue(&cue.id).unwrap().items[0], cue.items[0]);
    }
}
impl Drop for Harness {
    fn drop(&mut self) {
        // SQLite 핸들이 살아 있어 Windows에서 삭제가 안 되는 경우도 허용한다.
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
#[test]
fn title_context_add_then_drag_and_persist() {
    let mut h =
        Harness::new(json!({"id":"title-test","type":"title","title":"첫째","more":"둘째"}));
    h.click("slide-0", PointerButton::Secondary);
    h.click("op-add", PointerButton::Primary);
    assert_eq!(
        wp_core::slides::title_pages(&h.item()),
        vec!["첫째", "새 슬라이드", "둘째"]
    );
    h.assert_saved();
    let from = h.point("slide-0");
    let to = h.point("slide-2");
    h.drag(from, to);
    assert_eq!(
        wp_core::slides::title_pages(&h.item()),
        vec!["새 슬라이드", "둘째", "첫째"]
    );
    h.assert_saved();
}
#[test]
fn element_pointer_drag_updates_xy_and_persists() {
    let mut h = Harness::new(
        json!({"id":"element-test","type":"title","title":"요소","edits":{"0":{"items":[{"id":"e","type":"shape","x":20.0,"y":20.0,"w":20.0,"h":20.0,"fill":"#ffffff"}]}}}),
    );
    let before = h.item()["edits"]["0"]["items"][0].clone();
    let from = h.point("element-0");
    h.drag(from, from + Vec2::new(45.0, 30.0));
    let after = h.item()["edits"]["0"]["items"][0].clone();
    assert!(
        after["x"].as_f64().unwrap() > before["x"].as_f64().unwrap(),
        "x가 실제 드래그로 증가해야 함: {after}"
    );
    assert!(
        after["y"].as_f64().unwrap() > before["y"].as_f64().unwrap(),
        "y가 실제 드래그로 증가해야 함: {after}"
    );
    h.assert_saved();
}

fn headless_app(h: &Harness) -> super::App {
    super::App {
        core: h.core.clone(),
        web_dir: h.dir.clone(),
        data_dir: h.dir.clone(),
        search: String::new(),
        result: Value::Null,
        versions: vec![],
        sel: None,
        edit: None,
        out: crate::outwin::OutWin::for_test(),
        monitor: 0,
        last_flush: std::time::Instant::now(),
        focus_search: false,
        editor_on: true,
        editor_force: None,
        editor_reveal: false,
        views: super::MonitorViews::default(),
        test: None,
        tb: Default::default(),
        tc: Default::default(),
        viewer_direct_both: false,
        workspace: crate::workspace_ui::WorkspaceUi::new(h.core.lock().settings.get()),
    }
}
fn workspace_frame(h: &mut Harness, app: &mut super::App, events: Vec<Event>) {
    h.time += 0.05;
    let snap = app.snap();
    let mut acts = vec![];
    let mut output = h.ctx.run_pass(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1600.0, 1100.0))),
            time: Some(h.time),
            events,
            ..Default::default()
        },
        |ctx| {
            ctx.root_ui(|root| {
                egui::CentralPanel::default_margins().show(root, |ui| {
                    app.workspace_content(ui, &snap, &mut acts);
                });
            });
        },
    );
    output.textures_delta.clear();
    app.apply(acts);
}
fn workspace_drag(h: &mut Harness, app: &mut super::App) {
    let from = app.workspace.observed["tab-Preview"].center();
    let to = app.workspace.observed["pane-Live"].center();
    workspace_frame(h, app, vec![Event::PointerMoved(from)]);
    workspace_frame(
        h,
        app,
        vec![Event::PointerButton {
            pos: from,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        }],
    );
    workspace_frame(h, app, vec![Event::PointerMoved(from.lerp(to, 0.2))]);
    workspace_frame(h, app, vec![Event::PointerMoved(to)]);
    workspace_frame(
        h,
        app,
        vec![Event::PointerButton {
            pos: to,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        }],
    );
    workspace_frame(h, app, vec![]);
}
#[test]
fn workspace_tab_drag_lock_and_settings_restore() {
    use wp_core::workspace::{Node, Pane};
    let mut h = Harness::new(json!({"id":"workspace-test","type":"title","title":"격리"}));
    let mut app = headless_app(&h);
    // 단순한 두 패널 배치는 준비 데이터다. 이후 변경은 실제 탭 위젯 입력만 사용.
    app.workspace.state.layout = Some(Node::split(
        true,
        0.5,
        Node::tabs(vec![Pane::Preview]),
        Node::tabs(vec![Pane::Live]),
    ));
    app.workspace.state.locked = true;
    workspace_frame(&mut h, &mut app, vec![]);
    workspace_frame(&mut h, &mut app, vec![]);
    let locked = app.workspace.state.layout.clone();
    workspace_drag(&mut h, &mut app);
    assert_eq!(
        app.workspace.state.layout, locked,
        "잠금 중 드래그는 배치를 바꾸지 않아야 함"
    );
    // 잠금 상태 설정 자체는 준비 작업이며 체크박스 클릭 검증이 아니다.
    app.workspace.state.locked = false;
    workspace_frame(&mut h, &mut app, vec![]);
    workspace_drag(&mut h, &mut app);
    assert_ne!(
        app.workspace.state.layout, locked,
        "실제 탭 드래그로 합쳐져야 함"
    );
    let saved = serde_json::to_value(&app.workspace.state).unwrap();
    app.core.lock().flush();
    let restored_core = Core::open(h.dir.clone(), h.dir.clone(), None).unwrap();
    let restored = crate::workspace_ui::WorkspaceUi::new(restored_core.settings.get());
    assert_eq!(serde_json::to_value(&restored.state).unwrap(), saved);
}

// ---------------- 마우스 올림 아이콘 단추 (docs/ux-rules.md 3절) ----------------

type PanelFn = fn(&mut super::App, &mut egui::Ui, &super::Snap, &mut Vec<super::Act>);

/// 실제 App 화면 함수 하나를 한 프레임 그리고, 그 화면이 낸 Act 를 production 과 같은 apply 로 적용한다.
fn panel_frame(h: &mut Harness, app: &mut super::App, events: Vec<Event>, f: PanelFn) {
    h.time += 0.05;
    crate::widgets::HOVER_SEEN.with(|m| m.borrow_mut().clear());
    let snap = app.snap();
    let mut acts = vec![];
    let mut output = h.ctx.run_pass(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1600.0, 1100.0))),
            time: Some(h.time),
            events,
            ..Default::default()
        },
        |ctx| {
            ctx.root_ui(|root| {
                egui::CentralPanel::default_margins().show(root, |ui| f(app, ui, &snap, &mut acts));
            });
        },
    );
    output.textures_delta.clear();
    app.apply(acts);
}
fn seen(name: &str) -> Option<Rect> {
    crate::widgets::HOVER_SEEN.with(|m| m.borrow().get(name).copied())
}
/// 마우스를 올려 단추가 생기는지 보고, 그 단추를 실제 포인터로 누른다
fn hover_then_click(h: &mut Harness, app: &mut super::App, hover_at: Pos2, op: &str, f: PanelFn) {
    panel_frame(h, app, vec![Event::PointerMoved(hover_at)], f);
    panel_frame(h, app, vec![], f);
    let r = seen(op).unwrap_or_else(|| panic!("마우스를 올렸는데 '{op}' 단추가 없음"));
    assert!(r.width() >= 24.0 && r.height() >= 24.0, "단추는 24px 이상: {r:?}");
    let p = r.center();
    panel_frame(h, app, vec![Event::PointerMoved(p)], f);
    for pressed in [true, false] {
        panel_frame(h, app, vec![Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: Default::default() }], f);
    }
    panel_frame(h, app, vec![], f);
}
fn grid_body(app: &mut super::App, ui: &mut egui::Ui, s: &super::Snap, acts: &mut Vec<super::Act>) {
    app.center_body(ui, s, acts);
}
fn cue_body(app: &mut super::App, ui: &mut egui::Ui, s: &super::Snap, acts: &mut Vec<super::Act>) {
    app.cue_panel(ui, s, acts);
}

#[test]
fn grid_hover_add_button_inserts_slide_and_undo_restores() {
    let mut h = Harness::new(json!({"id":"grid-test","type":"text","title":"공지","body":"첫째\n---\n둘째"}));
    h.ui.open = false;
    let mut app = headless_app(&h);
    app.core.lock().preview_item(0); // 준비: 그 순서를 미리보기
    crate::center_ui::test_set_view(&h.ctx, crate::center_ui::SlideView::Grid, false);
    panel_frame(&mut h, &mut app, vec![], grid_body);
    panel_frame(&mut h, &mut app, vec![], grid_body);
    assert!(seen("add").is_none(), "마우스를 올리기 전에는 단추가 없어야 함");
    let cell = seen("cell-0").expect("격자 칸이 그려져야 함");
    hover_then_click(&mut h, &mut app, cell.center() + Vec2::new(0.0, 20.0), "add", grid_body);
    assert_eq!(h.item()["body"], json!("첫째\n---\n새 슬라이드\n---\n둘째"));
    assert!(app.tc.slideedit.open, "＋ 다음에는 슬라이드 편집기가 새 장으로 열림 (Electron quickOp)");
    h.assert_saved();
    // 단추 한 번 = 실행 취소 한 번
    assert!(app.core.lock().undo_items(false));
    assert_eq!(h.item()["body"], json!("첫째\n---\n둘째"));
}

#[test]
fn grid_hover_skip_on_bible_like_item_and_edit_opens_editor() {
    // 그림 순서: 지울 수 없는 종류라 ✕ 대신 건너뛰기(눈) · 복제 없음
    let mut h = Harness::new(json!({"id":"img-test","type":"slideshow","title":"그림","images":["a.png","b.png"]}));
    h.ui.open = false;
    let mut app = headless_app(&h);
    app.core.lock().preview_item(0);
    crate::center_ui::test_set_view(&h.ctx, crate::center_ui::SlideView::Grid, false);
    panel_frame(&mut h, &mut app, vec![], grid_body);
    panel_frame(&mut h, &mut app, vec![], grid_body);
    let cell = seen("cell-1").expect("둘째 칸");
    panel_frame(&mut h, &mut app, vec![Event::PointerMoved(cell.center())], grid_body);
    assert!(seen("del").is_none() && seen("dup").is_none(), "그림 원래 슬라이드는 지우기·복제 대신 건너뛰기");
    hover_then_click(&mut h, &mut app, cell.center(), "skip", grid_body);
    assert_eq!(h.item()["skip"], json!([1]));
    h.assert_saved();
    let cell = seen("cell-0").expect("첫째 칸");
    hover_then_click(&mut h, &mut app, cell.center(), "edit", grid_body);
    assert!(app.tc.slideedit.open, "✎ 는 슬라이드 편집기를 연다");
    assert_eq!(app.sel, Some(0));
}

#[test]
fn editor_list_hover_delete_button() {
    let mut h = Harness::new(json!({"id":"se-del","type":"text","title":"공지","body":"하나\n---\n둘\n---\n셋"}));
    let p = h.point("slide-1");
    h.frame(vec![Event::PointerMoved(p)]);
    h.frame(vec![]);
    let r = seen("del").expect("편집기 목록 줄에 마우스를 올리면 지우기 단추");
    assert!(seen("up").is_some() && seen("down").is_some() && seen("dup").is_some());
    let q = r.center();
    h.frame(vec![Event::PointerMoved(q)]);
    h.button(q, PointerButton::Primary, true);
    h.button(q, PointerButton::Primary, false);
    h.frame(vec![]);
    assert_eq!(h.item()["body"], json!("하나\n---\n셋"));
    h.assert_saved();
}

#[test]
fn cue_row_hover_shows_pencil_and_send() {
    let mut h = Harness::new(json!({"id":"cue-send","type":"title","title":"환영합니다"}));
    h.ui.open = false;
    let mut app = headless_app(&h);
    app.sel = None; // 고른 줄이 아니어도 마우스를 올리면 보여야 함
    panel_frame(&mut h, &mut app, vec![], cue_body);
    panel_frame(&mut h, &mut app, vec![], cue_body);
    assert!(seen("cue-send-0").is_none());
    // 첫 줄 자리는 연필 단추 자리로 알 수 없으니, 패널 위쪽부터 내려가며 줄을 찾는다
    let mut at = None;
    for y in (40..600).step_by(8) {
        let p = Pos2::new(300.0, y as f32);
        panel_frame(&mut h, &mut app, vec![Event::PointerMoved(p)], cue_body);
        if seen("cue-send-0").is_some() { at = Some(p); break; }
    }
    let at = at.expect("순서 줄에 마우스를 올리면 ▶ 단추가 생겨야 함");
    assert!(seen("cue-edit-0").is_some());
    assert!(app.snap().live.is_none());
    hover_then_click(&mut h, &mut app, at, "cue-send-0", cue_body);
    let live = app.snap().live.expect("▶ 를 누르면 송출");
    assert_eq!(live.deck.passage["cueIndex"], json!(0));
}

// ---------------- 본문 목록 보기 (Electron cue.js showSlideList · control.js 키보드) ----------------

/// 목록 보기 + 전역 단축키(← → Enter)를 함께 한 프레임 (production 의 ui() 와 같은 차례: 단축키 먼저)
fn list_body(app: &mut super::App, ui: &mut egui::Ui, s: &super::Snap, acts: &mut Vec<super::Act>) {
    let typing = ui.ctx().egui_wants_keyboard_input();
    app.shortcuts(ui.ctx(), acts, typing);
    app.center_body(ui, s, acts);
}
fn key(k: egui::Key) -> Vec<Event> {
    vec![Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() }]
}
fn list_click(h: &mut Harness, app: &mut super::App, name: &str, times: usize) {
    let p = seen(name).unwrap_or_else(|| panic!("목록 줄이 그려져야 함: {name}")).center();
    h.time += 1.0; // 앞의 누르기와 이어져 세 번 누르기로 세지 않게
    panel_frame(h, app, vec![Event::PointerMoved(p)], list_body);
    for _ in 0..times {
        for pressed in [true, false] {
            panel_frame(h, app, vec![Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: Default::default() }], list_body);
        }
    }
    panel_frame(h, app, vec![], list_body);
}
fn list_harness() -> (Harness, super::App) {
    let mut h = Harness::new(json!({"id":"list-test","type":"text","title":"광고","body":"첫째 소식\n---\n둘째 소식\n---\n셋째 소식"}));
    h.ui.open = false;
    let app = headless_app(&h);
    app.core.lock().preview_item(0); // 준비: 그 순서를 미리보기 (큐시트에서 누른 것과 같음)
    crate::center_ui::test_set_view(&h.ctx, crate::center_ui::SlideView::List, true);
    (h, app)
}

#[test]
fn list_click_previews_and_double_click_sends() {
    let (mut h, mut app) = list_harness();
    panel_frame(&mut h, &mut app, vec![], list_body);
    panel_frame(&mut h, &mut app, vec![], list_body);
    // 누르기 = 그 슬라이드 미리보기 (송출 안 됨)
    list_click(&mut h, &mut app, "srow-1", 1);
    assert_eq!(app.snap().preview.as_ref().unwrap().index, 1);
    assert!(app.snap().live.is_none(), "한 번 누르기는 송출하지 않음");
    // 두 번 누르기 = 그 슬라이드 송출
    list_click(&mut h, &mut app, "srow-2", 2);
    let live = app.snap().live.expect("두 번 누르면 송출");
    assert_eq!(live.deck.index, 2);
    assert_eq!(live.deck.passage["cueIndex"], json!(0));
}

#[test]
fn list_keyboard_arrows_step_preview_and_enter_sends() {
    let (mut h, mut app) = list_harness();
    app.core.lock().add_item(json!({"id":"next","type":"title","title":"다음 순서"}), None);
    app.core.lock().preview_item(0);
    panel_frame(&mut h, &mut app, vec![], list_body);
    panel_frame(&mut h, &mut app, vec![], list_body);
    // 목록에 키보드가 없을 때 Enter = 미리보기 송출 (control.js 전역 Enter)
    panel_frame(&mut h, &mut app, key(egui::Key::Enter), list_body);
    assert_eq!(app.snap().live.expect("Enter 로 송출").deck.index, 0);
    // → = 다음 슬라이드 미리보기
    panel_frame(&mut h, &mut app, key(egui::Key::ArrowRight), list_body);
    assert_eq!(app.snap().preview.as_ref().unwrap().index, 1);
    panel_frame(&mut h, &mut app, key(egui::Key::ArrowLeft), list_body);
    assert_eq!(app.snap().preview.as_ref().unwrap().index, 0);
    // 줄을 누르면 키보드는 목록에: Enter 는 목록이 쓰므로 송출하지 않음 (#results 는 전역 Enter 에서 빠짐)
    list_click(&mut h, &mut app, "srow-2", 1);
    panel_frame(&mut h, &mut app, key(egui::Key::Enter), list_body);
    assert_eq!(app.snap().live.as_ref().unwrap().deck.index, 0, "목록에 키보드가 있으면 Enter 로 송출하지 않음");
    // 마지막 장에서 → = 다음 순서 미리보기 (stepPreview → previewStep)
    panel_frame(&mut h, &mut app, key(egui::Key::ArrowRight), list_body);
    let pv = app.snap().preview.clone().unwrap();
    assert_eq!(pv.passage["cueIndex"], json!(1), "순서 끝에서 → 는 다음 순서");
    assert_eq!(app.sel, Some(1));
    // ← = 이전 순서의 마지막 장
    panel_frame(&mut h, &mut app, key(egui::Key::ArrowLeft), list_body);
    let pv = app.snap().preview.clone().unwrap();
    assert_eq!((pv.passage["cueIndex"].clone(), pv.index), (json!(0), 2));
}

// ---------------- 여러 장 고르기 · 끌어서 차례 바꾸기 (slideedit.js attachSel · selAction · moveSlides) ----------------

/// 머리 줄('고른 N장' 줄)과 본문을 함께 (production 처럼 단축키 먼저)
fn head_body(app: &mut super::App, ui: &mut egui::Ui, s: &super::Snap, acts: &mut Vec<super::Act>) {
    let typing = ui.ctx().egui_wants_keyboard_input();
    app.shortcuts(ui.ctx(), acts, typing);
    let mut on = false;
    app.center_head(ui, s, &mut on, acts);
    app.center_body(ui, s, acts);
}
/// panel_frame + 누른 꾸밈 키 (Shift·Ctrl)
fn frame_m(h: &mut Harness, app: &mut super::App, mut events: Vec<Event>, mods: egui::Modifiers) {
    events.insert(0, Event::ModifiersChanged(mods));
    h.time += 0.05;
    crate::widgets::HOVER_SEEN.with(|m| m.borrow_mut().clear());
    let snap = app.snap();
    let mut acts = vec![];
    let mut output = h.ctx.run_pass(
        RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1600.0, 1100.0))), time: Some(h.time), events, ..Default::default() },
        |ctx| {
            ctx.root_ui(|root| {
                egui::CentralPanel::default_margins().show(root, |ui| head_body(app, ui, &snap, &mut acts));
            });
        },
    );
    output.textures_delta.clear();
    app.apply(acts);
}
fn mclick(h: &mut Harness, app: &mut super::App, name: &str, mods: egui::Modifiers) {
    let p = seen(name).unwrap_or_else(|| panic!("그려져야 함: {name}")).center();
    h.time += 1.0; // 두 번 누르기로 세지 않게
    frame_m(h, app, vec![Event::PointerMoved(p)], mods);
    for pressed in [true, false] {
        frame_m(h, app, vec![Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: mods }], mods);
    }
    frame_m(h, app, vec![], egui::Modifiers::NONE);
}
fn mkey(h: &mut Harness, app: &mut super::App, k: egui::Key, mods: egui::Modifiers) {
    frame_m(h, app, vec![Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: mods }], mods);
    frame_m(h, app, vec![], egui::Modifiers::NONE);
}
/// 실제 포인터로 끌어 놓기. 놓기 전에 표시선이 보였는지 돌려준다
fn drag_drop(h: &mut Harness, app: &mut super::App, from: Pos2, to: Pos2) -> bool {
    let none = egui::Modifiers::NONE;
    h.time += 1.0;
    frame_m(h, app, vec![Event::PointerMoved(from)], none);
    frame_m(h, app, vec![Event::PointerButton { pos: from, button: PointerButton::Primary, pressed: true, modifiers: none }], none);
    frame_m(h, app, vec![Event::PointerMoved(from.lerp(to, 0.3))], none);
    frame_m(h, app, vec![Event::PointerMoved(to)], none);
    frame_m(h, app, vec![Event::PointerMoved(to)], none);
    let mark = seen("drop-mark").is_some();
    frame_m(h, app, vec![Event::PointerButton { pos: to, button: PointerButton::Primary, pressed: false, modifiers: none }], none);
    frame_m(h, app, vec![], none);
    frame_m(h, app, vec![], none);
    mark
}
const SHIFT: egui::Modifiers = egui::Modifiers::SHIFT;
const CTRL: egui::Modifiers = egui::Modifiers::COMMAND;

#[test]
fn list_multi_select_shift_ctrl_esc_and_delete_with_confirm() {
    let (mut h, mut app) = list_harness();
    frame_m(&mut h, &mut app, vec![], egui::Modifiers::NONE);
    frame_m(&mut h, &mut app, vec![], egui::Modifiers::NONE);
    mclick(&mut h, &mut app, "srow-0", egui::Modifiers::NONE);
    assert!(seen("msel-edit").is_none(), "한 장만 고르면 '고른 N장' 줄이 없다");
    // Shift+클릭 = 범위 (미리보기는 그대로)
    mclick(&mut h, &mut app, "srow-2", SHIFT);
    assert_eq!(crate::center_ui::test_msel(&h.ctx), vec![0, 1, 2]);
    assert_eq!(app.snap().preview.as_ref().unwrap().index, 0, "Shift+클릭은 미리보기를 바꾸지 않음");
    assert!(seen("msel-edit").is_some() && seen("msel-del").is_some() && seen("msel-clear").is_some(), "'고른 3장' 줄과 단추");
    // Ctrl+클릭 = 빼기
    mclick(&mut h, &mut app, "srow-1", CTRL);
    assert_eq!(crate::center_ui::test_msel(&h.ctx), vec![0, 2]);
    // Esc = 한 장만 (보조 화면 끄기 같은 전역 Esc 보다 먼저)
    mkey(&mut h, &mut app, egui::Key::Escape, egui::Modifiers::NONE);
    assert_eq!(crate::center_ui::test_msel(&h.ctx), vec![0]);
    assert!(seen("msel-edit").is_none());
    // Ctrl+A = 모두
    mkey(&mut h, &mut app, egui::Key::A, CTRL);
    assert_eq!(crate::center_ui::test_msel(&h.ctx), vec![0, 1, 2]);
    // 다시 둘만 고르고 Delete: 글 순서는 내용에서 지워지므로 먼저 묻는다
    mclick(&mut h, &mut app, "srow-1", egui::Modifiers::NONE);
    mclick(&mut h, &mut app, "srow-2", SHIFT);
    assert_eq!(crate::center_ui::test_msel(&h.ctx), vec![1, 2]);
    mkey(&mut h, &mut app, egui::Key::Delete, egui::Modifiers::NONE);
    assert_eq!(h.item()["body"], json!("첫째 소식\n---\n둘째 소식\n---\n셋째 소식"), "묻기 전에는 지우지 않음");
    let yes = seen("confirm-yes").expect("'정말 지우기' 확인").center();
    frame_m(&mut h, &mut app, vec![Event::PointerMoved(yes)], egui::Modifiers::NONE);
    for pressed in [true, false] {
        frame_m(&mut h, &mut app, vec![Event::PointerButton { pos: yes, button: PointerButton::Primary, pressed, modifiers: Default::default() }], egui::Modifiers::NONE);
    }
    frame_m(&mut h, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(h.item()["body"], json!("첫째 소식"));
    h.assert_saved();
    assert!(crate::center_ui::test_msel(&h.ctx).len() <= 1, "지운 뒤 고르기는 풀림");
}

#[test]
fn list_drag_moves_slide_with_drop_line() {
    let (mut h, mut app) = list_harness();
    frame_m(&mut h, &mut app, vec![], egui::Modifiers::NONE);
    frame_m(&mut h, &mut app, vec![], egui::Modifiers::NONE);
    let from = seen("srow-0").unwrap().center();
    let r2 = seen("srow-2").unwrap();
    let mark = drag_drop(&mut h, &mut app, from, r2.center() + Vec2::new(0.0, r2.height() / 4.0));
    assert!(mark, "끄는 동안 놓을 자리 표시선");
    assert_eq!(h.item()["body"], json!("둘째 소식\n---\n셋째 소식\n---\n첫째 소식"));
    h.assert_saved();
    let pv = app.snap().preview.clone().unwrap();
    assert_eq!(pv.index, 2, "옮긴 슬라이드를 새 자리에서 미리보기");
}

#[test]
fn grid_drag_moves_selected_slides_together() {
    let (mut h, mut app) = list_harness();
    crate::center_ui::test_set_view(&h.ctx, crate::center_ui::SlideView::Grid, true);
    frame_m(&mut h, &mut app, vec![], egui::Modifiers::NONE);
    frame_m(&mut h, &mut app, vec![], egui::Modifiers::NONE);
    mclick(&mut h, &mut app, "cell-0", egui::Modifiers::NONE);
    mclick(&mut h, &mut app, "cell-1", CTRL);
    assert_eq!(crate::center_ui::test_msel(&h.ctx), vec![0, 1]);
    let from = seen("cell-0").unwrap().center();
    let c2 = seen("cell-2").unwrap();
    assert!(drag_drop(&mut h, &mut app, from, c2.center() + Vec2::new(c2.width() / 4.0, 0.0)));
    // 고른 두 장이 셋째 뒤로 (목록 차례대로)
    assert_eq!(h.item()["body"], json!("셋째 소식\n---\n첫째 소식\n---\n둘째 소식"));
    h.assert_saved();
    frame_m(&mut h, &mut app, vec![], egui::Modifiers::NONE);
    assert_eq!(crate::center_ui::test_msel(&h.ctx), vec![1, 2], "옮긴 두 장을 새 자리에서 고른 채로");
}

#[test]
fn drag_is_blocked_for_song_with_singing_order() {
    let mut h = Harness::new(json!({"id":"song-order","type":"song","title":"찬양","lyrics":"[1절]\n하나\n\n[2절]\n둘","order":"2절 1절"}));
    h.ui.open = false;
    let mut app = headless_app(&h);
    app.core.lock().preview_item(0);
    crate::center_ui::test_set_view(&h.ctx, crate::center_ui::SlideView::List, true);
    frame_m(&mut h, &mut app, vec![], egui::Modifiers::NONE);
    frame_m(&mut h, &mut app, vec![], egui::Modifiers::NONE);
    let before = h.item();
    let from = seen("srow-0").expect("슬라이드 줄").center();
    let r1 = seen("srow-1").expect("둘째 줄");
    drag_drop(&mut h, &mut app, from, r1.center() + Vec2::new(0.0, r1.height() / 4.0));
    assert_eq!(h.item(), before, "부를 순서가 정해진 찬양은 차례를 바꾸지 않음");
    let toast = h.core.lock().toast.as_ref().map(|t| t.0.clone()).unwrap_or_default();
    assert!(toast.contains("부를 순서"), "까닭을 알림: {toast}");
}
