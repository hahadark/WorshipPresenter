//! 화면 구성 편집 창을 headless 로 그리고, 그린 캔버스 좌표로 칸을 끌어 본다.
//! (옮기기 · 모서리 끌기 · 자동 저장 · 출력에 쓰기 · 실행 취소 · 다시 · 잠근 칸)
use super::*;
use eframe::egui::{Event, PointerButton, RawInput};
use wp_core::core::Core;

struct H {
    ctx: egui::Context,
    ui: SceneUi,
    core: Shared,
    dir: std::path::PathBuf,
    time: f64,
}

impl H {
    fn new(preset: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("wp-scene-{}-{}", std::process::id(), wp_core::util::short_id("t")));
        std::fs::create_dir_all(&dir).unwrap();
        // 빈 DB 를 먼저 만들어 사용자 원본 자동 복사를 막는다
        drop(wp_core::db::BibleStore::open(&dir.join("bible.db")).unwrap());
        let mut core = Core::open(dir.clone(), dir.clone(), None).unwrap();
        let sc = core.save_scene(&scenes::new_scene(preset, "", Some("camera:cam-a"))).unwrap();
        let mut h = H { ctx: egui::Context::default(), ui: SceneUi::default(), core: std::sync::Arc::new(wp_core::lockcheck::Mutex::new(core)), dir, time: 0.0 };
        h.ui.open_with(sc["id"].as_str().unwrap());
        h.frame(vec![]);
        h.frame(vec![]);
        h
    }
    fn frame(&mut self, events: Vec<Event>) {
        self.frame_mods(events, Modifiers::NONE);
    }
    fn frame_mods(&mut self, mut events: Vec<Event>, modifiers: Modifiers) {
        self.time += 0.05;
        events.insert(0, Event::ModifiersChanged(modifiers));
        let input = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1600.0, 960.0))), time: Some(self.time), events, ..Default::default() };
        let (dir, core) = (self.dir.clone(), self.core.clone());
        let mut out = self.ctx.run_pass(input, |ctx| self.ui.show(ctx, &core, &dir, &dir));
        out.textures_delta.clear();
    }
    fn wait(&mut self, secs: f64) {
        for _ in 0..((secs / 0.05).ceil() as usize + 1) {
            self.frame(vec![]);
        }
    }
    fn btn(&mut self, p: Pos2, pressed: bool) {
        self.frame(vec![Event::PointerMoved(p), Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: Default::default() }]);
    }
    fn drag(&mut self, from: Pos2, to: Pos2) {
        self.frame(vec![Event::PointerMoved(from)]);
        self.btn(from, true);
        for k in 1..=5 {
            self.frame(vec![Event::PointerMoved(from.lerp(to, k as f32 / 5.0))]);
        }
        self.btn(to, false);
        self.frame(vec![]);
    }
    fn canvas(&self) -> Rect {
        self.ui.canvas_seen.expect("캔버스를 그리지 않음")
    }
    fn at(&self, fx: f32, fy: f32) -> Pos2 {
        let c = self.canvas();
        Pos2::new(c.left() + c.width() * fx, c.top() + c.height() * fy)
    }
    fn layer(&self, src: &str) -> Value {
        self.ui.draft["layers"].as_array().unwrap().iter().find(|l| l["source"] == src).cloned().unwrap()
    }
    fn saved_layer(&self, src: &str) -> Value {
        let c = self.core.lock();
        c.scenes().into_iter().find(|s| s["id"] == self.ui.sel.as_str()).unwrap()["layers"].as_array().unwrap().iter().find(|l| l["source"] == src).cloned().unwrap()
    }
}

impl Drop for H {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn drag_pip_box_snaps_saves_and_undoes() {
    let mut h = H::new("pip");
    let r0 = scenes::rect_of(&h.layer("camera:cam-a"));
    // 작은 화면 칸(오른쪽 아래)을 왼쪽 위로 끈다 — 화면 왼쪽 가장자리 · 격자에 붙는다
    let from = h.at((r0[0] + r0[2] / 2.0) as f32, (r0[1] + r0[3] / 2.0) as f32);
    let to = h.at(0.15 + 0.003, 0.15);
    h.drag(from, to);
    let r1 = scenes::rect_of(&h.layer("camera:cam-a"));
    assert!(r1[0] < 0.05 && r1[1] < 0.05, "왼쪽 위로: {r0:?} → {r1:?}");
    assert!(r1[0].abs() < 1e-6, "왼쪽 가장자리에 붙음: {r1:?}");
    assert_eq!(h.ui.layer.as_deref(), h.layer("camera:cam-a")["id"].as_str(), "끈 칸이 골라짐");
    h.wait(0.5);
    assert_eq!(scenes::rect_of(&h.saved_layer("camera:cam-a")), r1, "자동 저장");
    assert_eq!(h.ui.undo.len(), 1, "끌기 한 번 = 실행 취소 한 칸");
    // 오른쪽 아래 모서리를 끌어 키운다
    let r = r1;
    let corner = h.at((r[0] + r[2]) as f32, (r[1] + r[3]) as f32);
    h.drag(corner, corner + Vec2::new(60.0, 40.0));
    let r2 = scenes::rect_of(&h.layer("camera:cam-a"));
    assert!(r2[2] > r1[2] + 0.02 && r2[3] > r1[3] + 0.02, "커짐: {r1:?} → {r2:?}");
    assert!((r2[0] - r1[0]).abs() < 1e-6 && (r2[1] - r1[1]).abs() < 1e-6, "왼쪽 위는 그대로");
    // Ctrl+Z 두 번 → 처음 자리, Ctrl+Y → 다시
    let p = h.at(0.5, 0.5);
    let key = |k: Key, m: Modifiers| Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: m };
    h.frame(vec![Event::PointerMoved(p), key(Key::Z, Modifiers::COMMAND)]);
    assert_eq!(scenes::rect_of(&h.layer("camera:cam-a")), r1);
    h.frame(vec![Event::PointerMoved(p), key(Key::Z, Modifiers::COMMAND)]);
    assert_eq!(scenes::rect_of(&h.layer("camera:cam-a")), r0, "처음 자리");
    h.frame(vec![Event::PointerMoved(p), key(Key::Y, Modifiers::COMMAND)]);
    assert_eq!(scenes::rect_of(&h.layer("camera:cam-a")), r1, "다시");
    h.wait(0.5);
    assert_eq!(scenes::rect_of(&h.saved_layer("camera:cam-a")), r1, "실행 취소 · 다시도 저장");
}

#[test]
fn locked_layer_does_not_move_and_output_uses_scene() {
    let mut h = H::new("split");
    // 왼쪽 칸(카메라)을 잠근다
    let id = h.layer("camera:cam-a")["id"].as_str().unwrap().to_string();
    for l in h.ui.draft["layers"].as_array_mut().unwrap() {
        if l["id"] == id.as_str() {
            l["locked"] = json!(true);
        }
    }
    h.wait(0.5);
    let r0 = scenes::rect_of(&h.layer("camera:cam-a"));
    h.drag(h.at(0.25, 0.5), h.at(0.4, 0.7));
    assert_eq!(scenes::rect_of(&h.layer("camera:cam-a")), r0, "잠근 칸은 그대로");
    // 출력에 쓰면 출력 데이터의 구성이 이 구성 (칸 차례 + 맨 위 칸)
    let sid = h.ui.sel.clone();
    h.core.lock().set_output_scene("overlay", &sid).unwrap();
    let p = h.core.lock().output_payload("overlay");
    assert_eq!(p["scene"]["id"], sid.as_str());
    let ids: Vec<&str> = p["scene"]["layers"].as_array().unwrap().iter().map(|l| l["source"].as_str().unwrap()).collect();
    assert_eq!(ids[..3], ["background", "camera:cam-a", "live"]);
    assert!(ids.ends_with(&["message", "logo-screen", "draw", "pip"]));
    assert!(p["scene"]["layers"][1].get("locked").is_none(), "잠그기는 편집기 값 — 출력에 안 보냄");
    // 예배당은 그대로 기본
    assert_eq!(h.core.lock().output_payload("main")["scene"]["id"], "full");
}

/// 로드맵 3단계 ⑥: Alt+누름 = 겹친 칸을 위에서 아래로 차례로 고르기, 그 뒤 끌면 고른(가려진) 칸이 움직인다
#[test]
fn alt_click_picks_hidden_layer_and_drags_it() {
    let mut h = H::new("pip"); // 배경 · 미디어 · 순서 배경 · 송출 중인 순서 + 카메라(구석)
    // 송출 중인 순서 칸을 조금 줄여 둔다 (전체 화면 칸은 옮길 자리가 없다)
    for l in h.ui.draft["layers"].as_array_mut().unwrap() {
        if l["source"] == "live" {
            l["rect"] = json!([0.1, 0.1, 0.8, 0.8]);
        }
    }
    h.frame(vec![]);
    let cam = h.layer("camera:cam-a");
    let r = scenes::rect_of(&cam);
    let pt = h.at((r[0] + r[2] / 2.0) as f32, (r[1] + r[3] / 2.0) as f32);
    let alt_btn = |h: &mut H, pressed: bool| {
        let m = Modifiers::ALT;
        h.frame_mods(vec![Event::PointerMoved(pt), Event::PointerButton { pos: pt, button: PointerButton::Primary, pressed, modifiers: m }], m);
    };
    // 보통 누름 → 맨 위 (카메라)
    h.btn(pt, true);
    h.btn(pt, false);
    assert_eq!(h.ui.layer.as_deref(), cam["id"].as_str());
    // Alt+누름 → 그 아래 (송출 중인 순서) → 그 아래 (순서 배경 영상)
    alt_btn(&mut h, true);
    alt_btn(&mut h, false);
    let live_id = h.layer("live")["id"].as_str().unwrap().to_string();
    assert_eq!(h.ui.layer.as_deref(), Some(live_id.as_str()), "가려진 아래 칸");
    alt_btn(&mut h, true);
    alt_btn(&mut h, false);
    assert_eq!(h.ui.layer.as_deref(), h.layer("slide-bg")["id"].as_str(), "한 번 더 = 더 아래");
    // 다시 송출 중인 순서까지 돌려 놓고, 끌면 카메라가 아니라 그 칸이 움직인다
    for _ in 0..4 {
        alt_btn(&mut h, true);
        alt_btn(&mut h, false);
    }
    assert_eq!(h.ui.layer.as_deref(), Some(live_id.as_str()), "맨 아래 다음은 맨 위부터 다시");
    let cam_r0 = scenes::rect_of(&h.layer("camera:cam-a"));
    h.drag(pt, pt + Vec2::new(-200.0, -120.0));
    assert_eq!(scenes::rect_of(&h.layer("camera:cam-a")), cam_r0, "카메라는 그대로");
    let lr = scenes::rect_of(&h.layer("live"));
    assert!(lr[0] < 0.09 && lr[1] < 0.09, "고른 아래 칸이 옮겨짐: {lr:?}");
}
