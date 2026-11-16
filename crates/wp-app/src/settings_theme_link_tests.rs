//! 설정 센터·테마 관리와 기존 편집 창의 통합 연결 확인.
use super::auditc_tests::H;
use super::{Act, App, Snap};
use crate::theme_ui::Do;
use eframe::egui;
use serde_json::json;

fn full(app: &mut App, _ctx: &egui::Context, ui: &mut egui::Ui, _s: &Snap, _a: &mut Vec<Act>) {
    app.ui_body(ui);
}

#[test]
fn theme_design_edit_opens_the_requested_template_without_changing_outputs() {
    let mut h = H::new(vec![json!({"id":"a","type":"title","title":"예배"})]);
    let (theme_id, template_id, active) = {
        let mut c = h.app.core.lock();
        let id = wp_core::themes::save_current_as(&mut c, "편집 연결 시험", "").unwrap();
        let all = wp_core::themes::list_all(c.settings.get());
        let t = all.iter().find(|t| t["id"] == id.as_str()).unwrap();
        (id, t["slots"]["main"]["id"].as_str().unwrap().to_string(), c.settings.get()["activeTemplates"].clone())
    };
    h.app.tc.themes.open = true;
    let core = h.app.core.clone();
    h.app.tc.themes.run(&core, Do::Select(theme_id));
    h.app.tc.themes.run(&core, Do::Edit(template_id.clone()));
    h.frame(vec![], full);
    assert!(h.app.tc.templates.open, "테마의 편집 요청이 실제 편집 창으로 전달되어야 함");
    assert!(!h.app.tc.themes.open, "관리 창이 편집 창을 가리지 않아야 함");
    h.frame(vec![], full);
    h.frame(vec![], full);
    assert_eq!(h.app.tc.templates.selected_id(), template_id);
    assert_eq!(h.app.core.lock().settings.get()["activeTemplates"], active, "편집 창을 연 것만으로 출력 적용은 바뀌면 안 됨");
}

#[test]
fn quick_duplicate_makes_a_copy_and_uses_it_for_that_output_only() {
    let mut h = H::new(vec![json!({"id":"a","type":"title","title":"예배"})]);
    let (src, before_count, obs_before, src_tpl) = {
        let c = h.app.core.lock();
        let s = c.settings.get();
        let src = s["activeTemplates"]["main"].as_str().unwrap().to_string();
        let tpl = s["templates"].as_array().unwrap().iter().find(|t| t["id"] == src.as_str()).unwrap().clone();
        (src, s["templates"].as_array().unwrap().len(), s["activeTemplates"]["obs"].clone(), tpl)
    };
    let copy = h.app.quick_duplicate("main", &src).expect("복제 결과");
    let new_id = copy["id"].as_str().unwrap().to_string();
    assert_ne!(new_id, src, "새 id 여야 함");
    let c = h.app.core.lock();
    let s = c.settings.get();
    assert_eq!(s["templates"].as_array().unwrap().len(), before_count + 1);
    assert_eq!(s["activeTemplates"]["main"], new_id.as_str(), "복사본이 이 출력에 걸려야 함");
    assert_eq!(s["activeTemplates"]["obs"], obs_before, "다른 출력은 그대로");
    let kept = s["templates"].as_array().unwrap().iter().find(|t| t["id"] == src.as_str()).unwrap();
    assert_eq!(kept, &src_tpl, "원래 템플릿은 바뀌지 않아야 함");
    assert_eq!(copy["font"], src_tpl["font"], "모양은 같게 복사");
    assert!(copy["name"].as_str().unwrap().contains("복사본"));
    assert!(h.app.tb.note.contains("이 출력에 걸었습니다"), "알림: {}", h.app.tb.note);
}

fn settings_rect(h: &H) -> egui::Rect {
    h.ctx.memory(|m| m.area_rect(egui::Id::unique(("wp-float", "settings-center")))).expect("설정 창 자리")
}

#[test]
fn clicking_main_window_outside_closes_settings_but_inside_keeps_it() {
    let mut h = H::new(vec![json!({"id":"a","type":"title","title":"예배"})]);
    h.app.tc.settings.open = true;
    for _ in 0..3 {
        h.frame(vec![], full);
    }
    let r = settings_rect(&h);
    // 창 안을 누르면 그대로
    h.click_at(r.center(), full);
    h.frame(vec![], full);
    assert!(h.app.tc.settings.open, "창 안 누름에는 닫히면 안 됨");
    // 창 바깥의 조작 화면 바탕을 누르면 닫힘
    let out = if r.left() > 60.0 { egui::pos2(r.left() - 40.0, r.center().y) } else { egui::pos2(r.right() + 40.0, r.center().y) };
    h.click_at(out, full);
    h.frame(vec![], full);
    assert!(!h.app.tc.settings.open, "바깥 누름에 닫혀야 함 (누른 곳 {out:?}, 창 {r:?})");
}

#[test]
fn window_opened_by_the_same_press_is_not_closed_and_slide_editor_stays() {
    let mut h = H::new(vec![json!({"id":"a","type":"title","title":"예배"})]);
    h.frame(vec![], full);
    // 누르는 순간 창이 열려도(누름이 창 바깥이어도) 그 누름으로는 닫히지 않는다
    h.app.tc.settings.open = true;
    let p = egui::pos2(30.0, 900.0);
    h.frame(vec![egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() }], full);
    h.frame(vec![egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() }], full);
    h.frame(vec![], full);
    assert!(h.app.tc.settings.open, "연 그 누름에 닫히면 안 됨");
}
