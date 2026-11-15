//! 사용자 요청 세 가지 + 중복 순서 합성 입력 시험
//!  1) 예배당 화면 창을 눌러 고르고 Esc → 예배당 화면 꺼짐 (조작 화면 상태도)
//!  2) 미리보기·LIVE 아래 슬라이드(절) 번호 단추 줄 접기·펼치기
//!  3) '바로 송출' 일 때 모든 클릭 경로에서 바로 송출 · 안내 문구
//!  4) 같은 찬송가·찬양을 다시 고르면 새 순서를 만들지 않음 · 짧은 시간 반복 추가 막기
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
        let dir = std::env::temp_dir().join(format!("wp-fix3-{}-{}", std::process::id(), wp_core::util::short_id("t")));
        std::fs::create_dir_all(&dir).unwrap();
        // 빈 DB 를 먼저 만들어 사용자 원본 자동 복사를 막는다
        drop(wp_core::db::BibleStore::open(&dir.join("bible.db")).unwrap());
        let mut core = Core::open(dir.clone(), dir.clone(), None).unwrap();
        core.new_cue("합성입력 전용").unwrap();
        for it in items {
            core.add_item(it, None);
        }
        core.b.undo = Default::default();
        core.toast = None;
        let core = std::sync::Arc::new(parking_lot::Mutex::new(core));
        Self { ctx: egui::Context::default(), app: App::for_test(core, dir.clone()), dir, time: 0.0 }
    }
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
    fn frame(&mut self, events: Vec<Event>, f: Draw) {
        let acts = self.raw_frame(events, f);
        self.app.apply(acts);
    }
    fn press(&mut self, p: Pos2, f: Draw) {
        self.frame(vec![Event::PointerMoved(p)], f);
        for pressed in [true, false] {
            self.frame(vec![Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: Default::default() }], f);
        }
        self.frame(vec![], f);
    }
    /// 두 번 그려 자리를 잡은 뒤 이름 붙은 자리를 실제 포인터로 누른다
    fn click(&mut self, name: &str, f: Draw) {
        self.frame(vec![], f);
        self.frame(vec![], f);
        let p = seen(name).unwrap_or_else(|| panic!("그린 자리 없음: {name}")).center();
        self.press(p, f);
    }
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
    fn toast(&self) -> (String, Option<&'static str>) {
        let c = self.app.core.lock();
        (c.toast.as_ref().map(|t| t.0.clone()).unwrap_or_default(), c.toast_act)
    }
    fn items(&self) -> Vec<Value> {
        self.app.core.lock().cue.as_ref().unwrap().items.clone()
    }
    fn direct(&mut self) {
        self.app.core.lock().settings.update(|d| {
            d["previewMode"] = json!(false);
            d["output"]["autoOnSend"] = json!(false); // 시험에서는 예배당 창을 실제로 띄우지 않는다
        });
    }
    /// LIVE 로 나간 것: (cueIndex, 슬라이드 번호)
    fn live(&self) -> Option<(i64, usize)> {
        let c = self.app.core.lock();
        c.pres.live.as_ref().map(|l| (l.deck.passage["cueIndex"].as_i64().unwrap_or(-1), l.deck.index))
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
fn toast(app: &mut App, ctx: &egui::Context, _: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
    if let Some(a) = app.toast(ctx, s) {
        acts.push(a);
    }
}
fn bottom(app: &mut App, _: &egui::Context, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
    app.bottom_bar(ui, s, acts);
}
fn exit_frame(app: &mut App, ctx: &egui::Context, _: &mut egui::Ui, _: &Snap, _: &mut Vec<Act>) {
    crate::trackd_ui::output_exit(app, ctx);
}
fn titles() -> Vec<Value> {
    vec![
        json!({ "id": "a", "type": "text", "title": "공지", "body": "첫째 장\n---\n둘째 장\n---\n셋째 장" }),
        json!({ "id": "b", "type": "text", "title": "광고", "body": "하나\n---\n둘" }),
    ]
}

// ---------------- 1) 예배당 화면 Esc ----------------

#[test]
fn output_window_args_include_esc_close() {
    let o = crate::outwin::OutWin::for_test();
    let a = o.open_args(37528, 1, false);
    assert!(a.iter().any(|x| x == "--esc-close"), "예배당 창은 Esc 로 끌 수 있게 --esc-close 로 띄운다: {a:?}");
    assert!(!a.iter().any(|x| x == "--windowed"));
}

#[test]
fn esc_exit_turns_output_off_and_offers_turn_on() {
    let mut h = H::new(titles());
    h.app.out.fake_exit(Some(crate::outwin::EXIT_ESC));
    h.frame(vec![], exit_frame);
    let (msg, act) = h.toast();
    assert!(msg.contains("Esc"), "{msg}");
    assert_eq!(act, Some("output-on"), "알림의 [예배당 화면 켜기] 로 다시 켤 수 있다");
    assert!(!h.app.out.running());
    // [예배당 화면 켜기] → 예배당 화면 켜기 동작
    let acts = h.click_acts("toast-act", toast);
    assert!(acts.iter().any(|a| matches!(a, Act::ToastAct("output-on"))));
    // 한 번만 알린다
    h.app.core.lock().toast = None;
    h.frame(vec![], exit_frame);
    assert!(h.toast().0.is_empty());
    // 아래 줄 단추·상태: 꺼짐
    let s = h.app.snap();
    let items = h.app.status_items(&s);
    assert!(items.iter().any(|x| x.2 == "output" && x.1.contains("꺼짐")), "{:?}", items.iter().map(|x| x.1.clone()).collect::<Vec<_>>());
    let _ = bottom;
}

#[test]
fn closing_from_control_is_not_reported_as_exit() {
    let mut o = crate::outwin::OutWin::for_test();
    o.fake_exit(Some(0));
    o.close();
    assert!(o.take_exit().is_none(), "조작 화면에서 끈 것은 '창이 닫혔습니다' 알림을 내지 않는다");
}
