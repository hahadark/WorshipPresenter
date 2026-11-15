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
        let core = std::sync::Arc::new(wp_core::lockcheck::Mutex::new(core));
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

// ---------------- 3-4 검색한 본문을 큐시트에 ----------------

fn head(app: &mut App, _: &egui::Context, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
    let mut on = true;
    app.center_head(ui, s, &mut on, acts);
}
fn searched(h: &mut H) {
    // 장절 검색 결과 (시험 DB 에는 본문이 없어 결과 모양만 넣는다)
    h.app.search = "요3:16".into();
    h.app.result = json!({ "ok": true, "mode": "reference", "passage": { "title": "요한복음 3:16", "verses": [] } });
}

#[test]
fn center_head_add_button_inserts_searched_passage_after_selection() {
    let mut h = H::new(two());
    h.frame(vec![], head);
    h.frame(vec![], head);
    assert!(seen("center-add-bible").is_none(), "검색한 장절이 없으면 단추도 없다");
    searched(&mut h);
    h.app.sel = Some(0);
    h.click("center-add-bible", head);
    let items = h.items();
    assert_eq!(items.len(), 3);
    assert_eq!(items[1]["type"], "bible", "고른 순서(0) 다음에");
    assert_eq!(items[1]["ref"], "요한복음 3:16");
    assert!(h.toast().0.contains("요한복음 3:16"));
}

#[test]
fn left_bible_button_fills_searched_reference() {
    let mut h = H::new(two());
    h.click("add-bible", cue);
    assert_eq!(h.items().last().unwrap()["ref"].as_str().unwrap_or(""), "", "검색이 없으면 빈 성경 순서");
    searched(&mut h);
    h.app.sel = None;
    h.click("add-bible", cue);
    assert_eq!(h.items().last().unwrap()["ref"], "요한복음 3:16");
}

// ---------------- 3-5 OBS 주소 복사 ----------------

fn obs(_: &mut App, _: &egui::Context, ui: &mut egui::Ui, _: &Snap, _: &mut Vec<Act>) {
    crate::trackd_ui::obs_urls(ui, 37528);
}

#[test]
fn output_settings_obs_urls_copy_with_notice() {
    let mut h = H::new(two());
    h.frame(vec![], obs);
    for i in 0..3 {
        assert!(seen(&format!("obs-copy-{i}")).is_some(), "주소 {i} 의 [복사]");
    }
    h.click("obs-copy-1", obs);
    let msg = crate::widgets::take_copied(&h.ctx).expect("복사하면 알림이 남아야 함");
    assert!(msg.contains("투명 자막") && msg.contains("복사했습니다"), "{msg}");
    assert!(crate::widgets::take_copied(&h.ctx).is_none(), "한 번 꺼내면 지워짐");
}

// ---------------- 3-7 미리보기 / 바로 송출 ----------------

fn top(app: &mut App, _: &egui::Context, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
    app.top_bar(ui, s, acts);
}
fn band(app: &mut App, _: &egui::Context, ui: &mut egui::Ui, _: &Snap, acts: &mut Vec<Act>) {
    app.direct_band(ui, acts);
}
fn preview_mode(h: &H) -> bool {
    h.app.core.lock().settings.get()["previewMode"].as_bool().unwrap_or(true)
}

#[test]
fn studio_split_button_picks_mode_and_band_returns() {
    let mut h = H::new(two());
    assert!(preview_mode(&h));
    h.click("studio-preview", top);
    assert!(preview_mode(&h), "이미 고른 쪽을 눌러도 그대로 (토글이 아님)");
    h.click("studio-direct", top);
    assert!(!preview_mode(&h), "'바로 송출'을 고르면 미리보기 끔");
    h.click("studio-direct", top);
    assert!(!preview_mode(&h));
    h.click("direct-band-back", band);
    assert!(preview_mode(&h), "띠의 [미리보기로 돌아가기]");
}

// ---------------- 3-8 편집칸 높이 · 크게 · 순서 삭제 ----------------

fn main(app: &mut App, _: &egui::Context, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
    app.main_panel(ui, s, acts);
}
fn frac(h: &H) -> Option<f64> {
    h.app.core.lock().settings.get()["rsEditorFrac"].as_f64()
}

#[test]
fn editor_splitter_drag_changes_and_remembers_height() {
    let mut h = H::new(two());
    h.app.sel = Some(0);
    h.frame(vec![], main);
    h.frame(vec![], main);
    let r = seen("editor-split").expect("편집칸 아래 구분선");
    let from = r.center();
    let to = from + Vec2::new(0.0, 120.0);
    h.frame(vec![Event::PointerMoved(from)], main);
    h.frame(vec![Event::PointerButton { pos: from, button: PointerButton::Primary, pressed: true, modifiers: Default::default() }], main);
    for k in 1..=4 {
        h.frame(vec![Event::PointerMoved(from.lerp(to, k as f32 / 4.0))], main);
    }
    h.frame(vec![Event::PointerButton { pos: to, button: PointerButton::Primary, pressed: false, modifiers: Default::default() }], main);
    h.frame(vec![], main);
    let f = frac(&h).expect("놓으면 높이를 설정에 기억");
    assert!(f > 0.5, "아래로 끌면 커져야 함: {f}");
    h.frame(vec![], main);
    let r2 = seen("editor-split").unwrap();
    assert!(r2.center().y > from.y + 60.0, "다시 그려도 바뀐 높이 그대로");
}

#[test]
fn editor_big_button_fills_panel_and_back() {
    let mut h = H::new(two());
    h.app.sel = Some(0);
    h.click("editor-big", main);
    assert!(super::editor_big(&h.ctx));
    assert!(seen("editor-split").is_none(), "크게 보면 구분선·검색 결과가 없다");
    h.click("editor-big", main);
    assert!(!super::editor_big(&h.ctx));
    assert!(seen("editor-split").is_some());
}

#[test]
fn editor_delete_button_removes_with_undo_toast() {
    let mut h = H::new(two());
    h.app.sel = Some(1);
    h.click("editor-del", main);
    assert_eq!(h.items().len(), 1);
    let (msg, act) = h.toast();
    assert!(msg.contains("'설교' 순서를 삭제"), "{msg}");
    assert_eq!(act, Some("undo"));
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
