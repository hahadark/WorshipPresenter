//! 사용성 고치기(평가 3-1·3-2·3-4·3-5·3-7·3-8·2) 합성 입력 시험.
//! 실제 화면 함수를 headless 로 그리고, 그린 자리(widgets::mark)를 실제 포인터로 눌러 결과를 본다.
use super::{Act, App, Snap};
use eframe::egui::{self, Event, PointerButton, Pos2, RawInput, Rect, Vec2};
use serde_json::{json, Value};
use wp_core::core::Core;

struct H {
    ctx: egui::Context,
    app: App,
    dir: std::path::PathBuf,
    time: f64,
}

type Draw = fn(&mut App, &egui::Context, &mut egui::Ui, &Snap, &mut Vec<Act>);

impl H {
    fn new(items: Vec<Value>) -> Self {
        let dir = std::env::temp_dir().join(format!("wp-uxa-{}-{}", std::process::id(), wp_core::util::short_id("t")));
        std::fs::create_dir_all(&dir).unwrap();
        // 빈 DB 를 먼저 만들어 사용자 원본 자동 복사를 막는다
        drop(wp_core::db::BibleStore::open(&dir.join("bible.db")).unwrap());
        let mut core = Core::open(dir.clone(), dir.clone(), None).unwrap();
        core.new_cue("합성입력 전용").unwrap();
        for it in items {
            core.add_item(it, None);
        }
        core.b.undo = Default::default(); // 준비로 넣은 것은 되돌리기 기록에서 뺀다
        core.toast = None;
        let core = std::sync::Arc::new(parking_lot::Mutex::new(core));
        Self { ctx: egui::Context::default(), app: App::for_test(core, dir.clone()), dir, time: 0.0 }
    }
    fn frame(&mut self, events: Vec<Event>, f: Draw) {
        let acts = self.raw_frame(events, f);
        self.app.apply(acts);
    }
    /// 한 프레임 그리고 낸 Act 를 적용하지 않고 돌려준다 (출력 창을 실제로 띄우는 동작 확인용)
    fn raw_frame(&mut self, events: Vec<Event>, f: Draw) -> Vec<Act> {
        self.time += 0.05;
        crate::widgets::HOVER_SEEN.with(|m| m.borrow_mut().clear());
        let snap = self.app.snap();
        let mut acts = vec![];
        let app = &mut self.app;
        let mut out = self.ctx.run_pass(
            RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1600.0, 960.0))), time: Some(self.time), events, ..Default::default() },
            |ctx| {
                ctx.root_ui(|root| {
                    egui::CentralPanel::default_margins().show(root, |ui| f(app, ctx, ui, &snap, &mut acts));
                });
            },
        );
        out.textures_delta.clear();
        acts
    }
    /// 이름 붙은 자리를 눌렀을 때 나온 Act (적용하지 않음)
    fn click_acts(&mut self, name: &str, f: Draw) -> Vec<Act> {
        self.raw_frame(vec![], f);
        self.raw_frame(vec![], f);
        let p = seen(name).unwrap_or_else(|| panic!("그린 자리 없음: {name}")).center();
        let mut all = self.raw_frame(vec![Event::PointerMoved(p)], f);
        for pressed in [true, false] {
            all.extend(self.raw_frame(vec![Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: Default::default() }], f));
        }
        all
    }
    /// 두 번 그려 자리를 잡은 뒤 이름 붙은 자리를 실제 포인터로 누른다
    fn click(&mut self, name: &str, f: Draw) {
        self.frame(vec![], f);
        self.frame(vec![], f);
        let r = seen(name).unwrap_or_else(|| panic!("그린 자리 없음: {name}"));
        let p = r.center();
        self.frame(vec![Event::PointerMoved(p)], f);
        for pressed in [true, false] {
            self.frame(vec![Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: Default::default() }], f);
        }
        self.frame(vec![], f);
    }
    fn items(&self) -> Vec<Value> {
        self.app.core.lock().cue.as_ref().unwrap().items.clone()
    }
    fn toast(&self) -> (String, Option<&'static str>) {
        let c = self.app.core.lock();
        (c.toast.as_ref().map(|t| t.0.clone()).unwrap_or_default(), c.toast_act)
    }
}
impl Drop for H {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
fn seen(name: &str) -> Option<Rect> {
    crate::widgets::HOVER_SEEN.with(|m| m.borrow().get(name).copied())
}
fn cue(app: &mut App, _: &egui::Context, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
    app.cue_panel(ui, s, acts);
}
fn toast(app: &mut App, ctx: &egui::Context, _: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
    if let Some(a) = app.toast(ctx, s) {
        acts.push(a);
    }
}
fn two() -> Vec<Value> {
    vec![json!({ "id": "a", "type": "title", "title": "말씀 봉독" }), json!({ "id": "b", "type": "title", "title": "설교" })]
}

fn live(app: &mut App, _: &egui::Context, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
    let d = s.live.as_ref().map(|l| l.deck.clone());
    app.deck_view(ui, "LIVE", d.as_ref(), true, s.live.as_ref(), s, acts);
}
fn bottom(app: &mut App, _: &egui::Context, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
    app.bottom_bar(ui, s, acts);
}

// ---------------- 3-1 예배당 화면 꺼짐 ----------------

#[test]
fn live_pane_band_appears_only_when_live_and_output_off() {
    let mut h = H::new(two());
    h.frame(vec![], live);
    h.frame(vec![], live);
    assert!(seen("live-output-on").is_none(), "송출한 것이 없으면 띠도 없다");
    h.app.apply(vec![Act::PreviewItem(0), Act::SendPreview]);
    let acts = h.click_acts("live-output-on", live);
    assert!(acts.iter().any(|a| matches!(a, Act::OutputToggle)), "[지금 켜기] 는 예배당 화면을 켜야 함");
}

#[test]
fn send_with_output_off_and_no_auto_toasts_turn_on_button() {
    let mut h = H::new(two());
    h.app.core.lock().settings.update(|d| d["output"]["autoOnSend"] = json!(false));
    h.app.apply(vec![Act::PreviewItem(0), Act::SendPreview]);
    let (msg, act) = h.toast();
    assert!(msg.contains("예배당 화면이 꺼져"), "{msg}");
    assert_eq!(act, Some("output-on"));
    let acts = h.click_acts("toast-act", toast);
    assert!(acts.iter().any(|a| matches!(a, Act::ToastAct("output-on"))));
}

#[test]
fn status_dot_click_opens_output_settings() {
    let mut h = H::new(two());
    assert!(!crate::trackd_ui::is_open(crate::trackd_ui::Win::Output));
    h.click("status-output", bottom);
    assert!(crate::trackd_ui::is_open(crate::trackd_ui::Win::Output), "상태 점을 누르면 출력 설정 창");
}

#[test]
fn status_fit_keeps_most_important_first() {
    use crate::widgets::fit_status;
    // 넓으면 모두 글까지
    assert_eq!(fit_status(&[100.0, 80.0, 80.0], 1000.0, 14.0), vec![Some(true); 3]);
    // 좁으면 앞(중요한) 것만 글, 나머지는 점
    assert_eq!(fit_status(&[100.0, 80.0, 80.0], 9.0 * 3.0 + 28.0 + 105.0, 14.0), vec![Some(true), Some(false), Some(false)]);
    // 점도 모자라면 뒤부터 빠진다 — 첫 항목(예배당 화면)은 끝까지 남는다
    assert_eq!(fit_status(&[100.0, 80.0, 80.0], 20.0, 14.0), vec![Some(false), None, None]);
}

// ---------------- 3-2 · 2 되돌리기 ----------------

#[test]
fn remove_item_toasts_with_undo_button_that_restores() {
    let mut h = H::new(two());
    h.app.apply(vec![Act::RemoveItem(0)]);
    assert_eq!(h.items().len(), 1);
    let (msg, act) = h.toast();
    assert!(msg.contains("말씀 봉독") && msg.contains("삭제"), "지운 순서 이름이 알림에 있어야 함: {msg}");
    assert_eq!(act, Some("undo"));
    // 알림의 [되돌리기] 를 실제로 누른다
    h.click("toast-act", toast);
    assert_eq!(h.items().len(), 2, "알림 단추로 되살아나야 함");
    assert_eq!(h.items()[0]["title"], "말씀 봉독");
    let (msg, act) = h.toast();
    assert!(msg.starts_with("되돌렸습니다: '말씀 봉독' 순서 삭제"), "{msg}");
    assert_eq!(act, None);
}

#[test]
fn cue_head_undo_redo_buttons_show_label_and_work() {
    let mut h = H::new(two());
    // 할 것이 없으면 흐리게: 눌러도 아무 일 없음
    h.click("cue-undo", cue);
    assert_eq!(h.items().len(), 2);
    h.app.apply(vec![Act::MoveItem(1, -1)]);
    assert_eq!(h.items()[0]["title"], "설교");
    assert_eq!(h.app.core.lock().undo_label(false).as_deref(), Some("'설교' 순서 옮기기"));
    h.click("cue-undo", cue);
    assert_eq!(h.items()[0]["title"], "말씀 봉독", "↶ 로 옮기기가 되돌려져야 함");
    assert!(h.toast().0.starts_with("되돌렸습니다: '설교' 순서 옮기기"));
    assert_eq!(h.app.core.lock().undo_label(true).as_deref(), Some("'설교' 순서 옮기기"));
    h.click("cue-redo", cue);
    assert_eq!(h.items()[0]["title"], "설교", "↷ 로 다시 하기");
    assert!(h.toast().0.starts_with("다시 했습니다"));
}

#[test]
fn undo_with_nothing_left_says_so() {
    let mut h = H::new(two());
    h.app.apply(vec![Act::Undo(false)]);
    assert_eq!(h.toast().0, "더 되돌릴 것이 없습니다");
}

#[test]
fn toast_body_click_dismisses() {
    let mut h = H::new(two());
    h.app.core.lock().notify("시험 알림", "info");
    h.frame(vec![], toast);
    h.frame(vec![], toast);
    let r = h.ctx.memory(|m| m.area_rect(egui::Id::new("wp-toast"))).expect("알림이 그려져야 함");
    let p = r.left_center() + Vec2::new(24.0, 0.0);
    h.frame(vec![Event::PointerMoved(p)], toast);
    for pressed in [true, false] {
        h.frame(vec![Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: Default::default() }], toast);
    }
    assert!(h.app.core.lock().toast.is_none(), "알림을 누르면 닫혀야 함");
}
