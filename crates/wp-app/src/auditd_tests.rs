//! 갈래 D 점검: '1.x 자료 다시 가져오기' 메뉴와 묻는 창을 합성 입력(포인터 누름)으로 실제로 눌러 본다.
//! 그려진 글자 자리를 출력 도형에서 찾아 그 자리를 누른다 (사람이 누르는 것과 같은 egui 입력 경로).
use super::auditc_tests::H;
use super::{Act, App};
use eframe::egui::{self, Event, PointerButton, Pos2, RawInput, Rect, Vec2};
use serde_json::json;

fn draw(app: &mut App, ctx: &egui::Context, ui: &mut egui::Ui, acts: &mut Vec<Act>) {
    crate::track_c::menu_bar(app, ui, acts);
    let dir = app.data_dir.clone();
    crate::trackd_ui::migrate_ask(ctx, &dir);
}

struct D {
    h: H,
    time: f64,
}

impl D {
    /// 한 프레임 그리고, 그려진 글자(자리 포함)를 돌려준다
    fn frame(&mut self, events: Vec<Event>) -> Vec<(String, Rect)> {
        self.time += 0.05;
        let mut acts = vec![];
        let app = &mut self.h.app;
        let mut out = self.h.ctx.run_pass(
            RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1600.0, 960.0))), time: Some(self.time), events, ..Default::default() },
            |ctx| {
                ctx.root_ui(|root| draw(app, ctx, root, &mut acts));
            },
        );
        out.textures_delta.clear();
        self.h.app.apply(acts);
        let mut texts = vec![];
        fn walk(s: &egui::Shape, out: &mut Vec<(String, Rect)>) {
            match s {
                egui::Shape::Vec(v) => v.iter().for_each(|x| walk(x, out)),
                egui::Shape::Text(t) => out.push((t.galley.text().to_string(), t.visual_bounding_rect())),
                _ => {}
            }
        }
        for c in &out.shapes {
            walk(&c.shape, &mut texts);
        }
        texts
    }
    fn texts(&mut self) -> Vec<(String, Rect)> {
        self.frame(vec![]);
        self.frame(vec![])
    }
    fn has(&mut self, needle: &str) -> bool {
        self.texts().iter().any(|(t, _)| t.contains(needle))
    }
    /// 그 글자가 그려진 자리를 누른다
    fn click_text(&mut self, needle: &str) {
        let all = self.texts();
        let (_, r) = all.iter().find(|(t, _)| t.trim() == needle).or_else(|| all.iter().find(|(t, _)| t.contains(needle))).unwrap_or_else(|| panic!("화면에 '{needle}' 이(가) 없음. 보이는 글: {:?}", all.iter().map(|x| x.0.clone()).collect::<Vec<_>>()));
        let p = r.center();
        self.frame(vec![Event::PointerMoved(p)]);
        for pressed in [true, false] {
            self.frame(vec![Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: Default::default() }]);
        }
        self.frame(vec![]);
    }
}

#[test]
fn reimport_menu_and_ask_dialog_by_synthetic_clicks() {
    use wp_core::firstrun as fr;
    let mut d = D { h: H::new(vec![]), time: 100.0 };
    let dir = d.h.app.data_dir.clone();
    fr::set_no_restart_for_test(true);
    fr::write_instance_pid(&dir); // 메뉴가 쓸 자료 폴더
    fr::set_ask(None);

    // 1) 메뉴: 도움말 → '1.x 자료 다시 가져오기…' → 묻는 창이 뜬다
    assert!(!d.has("지금 자료 그대로 쓰기"));
    d.click_text("도움말");
    d.click_text("1.x 자료 다시 가져오기…");
    let a = fr::ask().expect("메뉴를 누르면 묻는 창이 떠야 함");
    // 이 PC 에 1.x 자료가 있으면 "menu"(읽기만 한다), 없으면 "none"
    if a["reason"] == "none" {
        assert!(d.has("찾지 못했습니다"));
        d.click_text("닫기");
    } else {
        assert_eq!(a["reason"], "menu");
        assert!(d.has("지금 자료는 지우지 않고"), "보관 안내가 보여야 함");
        d.click_text("지금 자료 그대로 쓰기");
        assert!(!dir.join(fr::MARKER).exists(), "메뉴에서 연 창을 그냥 닫으면 표시 파일을 쓰지 않는다");
    }
    assert!(fr::ask().is_none(), "창이 닫혀야 함");
    assert!(!dir.join(fr::REQUEST).exists());

    // 2) 첫 화면에서 묻기(양쪽 다 바뀜) → '지금 자료 그대로 쓰기' → 표시 파일을 남기고 닫힌다
    let old = dir.join("fake-1x");
    std::fs::create_dir_all(&old).unwrap();
    let ask = json!({ "reason": "both", "from": old.to_string_lossy(), "dataDir": dir.to_string_lossy(), "electronCues": 9, "electronSongs": 3, "rustCues": 7, "rustSongs": 2, "electronChanged": 1794700000000i64, "rustChanged": 1794600000000i64 });
    fr::set_ask(Some(ask.clone()));
    assert!(d.has("양쪽에서 자료가 바뀌었습니다"));
    assert!(d.has("9 · 3") && d.has("7 · 2"), "양쪽 큐시트·찬양 개수가 보여야 함");
    d.click_text("지금 자료 그대로 쓰기");
    assert!(fr::ask().is_none());
    let m = fr::read_marker(&dir).expect("표시 파일");
    assert_eq!(m["how"], "kept");

    // 3) '1.x 자료 다시 가져오기' 단추 → 요청 파일을 남긴다 (다음에 켜질 때 지금 자료를 보관하고 다시 가져온다)
    fr::set_ask(Some(ask));
    d.click_text("1.x 자료 다시 가져오기 (프로그램이 다시 켜집니다)");
    assert!(dir.join(fr::REQUEST).is_file(), "요청 파일이 있어야 함");
    assert!(fr::ask().is_none());
    fr::set_no_restart_for_test(false);
}

/// 포터블·개발 실행(실행 파일 옆에 제거 프로그램이 없음)에서는 업데이트 창이 '스스로 바꾸지 않는다'는 안내를 보이고 받기·설치를 막는다
#[test]
fn update_window_explains_portable_copy() {
    let h = H::new(vec![]);
    let core = h.app.core.clone();
    assert!(!wp_core::updates::installed_copy(), "시험 실행 파일 옆에는 제거 프로그램이 없다");
    let st = wp_core::updates::status(core.lock().settings.get());
    assert_eq!(st["portable"], true);
    assert_eq!(st["canDownload"], false);
    assert_eq!(st["canInstall"], false);
    assert!(wp_core::updates::download(&core).is_err(), "포터블에서는 받기를 거절해야 함");
    assert!(wp_core::updates::install_now(&core).is_err(), "포터블에서는 설치를 거절해야 함");
    let mut texts: Vec<String> = vec![];
    for _ in 0..2 {
        let mut out = h.ctx.run_pass(RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1600.0, 960.0))), ..Default::default() }, |ctx| {
            ctx.root_ui(|ui| crate::trackd_ui::update_ui(ui, &core));
        });
        out.textures_delta.clear();
        fn walk(s: &egui::Shape, out: &mut Vec<String>) {
            match s {
                egui::Shape::Vec(v) => v.iter().for_each(|x| walk(x, out)),
                egui::Shape::Text(t) => out.push(t.galley.text().to_string()),
                _ => {}
            }
        }
        texts.clear();
        for c in &out.shapes {
            walk(&c.shape, &mut texts);
        }
    }
    assert!(texts.iter().any(|t| t.contains("포터블")), "포터블 안내가 보여야 함: {texts:?}");
}
