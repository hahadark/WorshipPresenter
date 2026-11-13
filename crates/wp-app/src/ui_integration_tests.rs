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
            core: std::sync::Arc::new(parking_lot::Mutex::new(core)),
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
        editor_reveal: false,
        views: super::MonitorViews::default(),
        test: None,
        tb: Default::default(),
        tc: Default::default(),
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
