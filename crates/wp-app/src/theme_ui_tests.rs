//! 출력 테마 관리 창을 headless 로 그리고, 그린 단추 자리만 눌러 본다 (합성 입력).
//! 미리보기는 상태를 바꾸지 않음 · 출력 범위별 적용과 되돌리기 · 저장/복제/이름/삭제 확인 · 처음 디자인으로 확인 · 가져오기/내보내기
use super::*;
use eframe::egui::{Event, PointerButton, RawInput};
use wp_core::core::Core;

struct H {
    ctx: egui::Context,
    ui: ThemeUi,
    core: Shared,
    dir: std::path::PathBuf,
    time: f64,
    screen: Vec2,
}

impl H {
    fn new() -> Self {
        Self::sized(Vec2::new(1700.0, 1400.0))
    }
    fn sized(screen: Vec2) -> Self {
        let dir = std::env::temp_dir().join(format!("wp-thm-{}-{}", std::process::id(), wp_core::util::short_id("t")));
        std::fs::create_dir_all(&dir).unwrap();
        // 빈 DB 를 먼저 만들어 사용자 원본 자동 복사를 막는다
        drop(wp_core::db::BibleStore::open(&dir.join("bible.db")).unwrap());
        let core = Core::open(dir.clone(), dir.clone(), None).unwrap();
        let mut h = H { ctx: egui::Context::default(), ui: ThemeUi::default(), core: std::sync::Arc::new(wp_core::lockcheck::Mutex::new(core)), dir, time: 0.0, screen };
        h.ui.open = true;
        h.frame(vec![]);
        h.frame(vec![]);
        h
    }
    fn frame(&mut self, events: Vec<Event>) {
        self.time += 0.05;
        widgets::HOVER_SEEN.with(|m| m.borrow_mut().clear());
        let input = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.screen)), time: Some(self.time), events, ..Default::default() };
        let (dir, core) = (self.dir.clone(), self.core.clone());
        let mut out = self.ctx.run_pass(input, |ctx| self.ui.show(ctx, &core, &dir, &dir));
        out.textures_delta.clear();
    }
    fn seen(&self, name: &str) -> Option<Rect> {
        widgets::HOVER_SEEN.with(|m| m.borrow().get(name).copied())
    }
    fn click(&mut self, name: &str) {
        self.frame(vec![]);
        let p = self.seen(name).unwrap_or_else(|| panic!("그린 자리 없음: {name} (있는 것: {:?})", widgets::HOVER_SEEN.with(|m| m.borrow().keys().cloned().collect::<Vec<_>>()))).center();
        let vis = self.seen(name).unwrap();
        assert!(vis.width() > 4.0 && vis.height() > 4.0 && Rect::from_min_size(Pos2::ZERO, self.screen).contains(p), "{name} 이(가) 가려졌거나 화면 밖: {vis:?}");
        self.frame(vec![Event::PointerMoved(p)]);
        for pressed in [true, false] {
            self.frame(vec![Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: Default::default() }]);
        }
        self.frame(vec![]);
        self.frame(vec![]);
    }
    fn type_text(&mut self, text: &str) {
        for ch in text.chars() {
            self.frame(vec![Event::Text(ch.to_string())]);
        }
    }
    fn settings(&self) -> Value {
        self.core.lock().settings.get().clone()
    }
    fn active(&self, slot: &str) -> String {
        self.settings()["activeTemplates"][slot].as_str().unwrap_or("").to_string()
    }
    fn customs(&self) -> Vec<Value> {
        self.settings()["customThemes"].as_array().cloned().unwrap_or_default()
    }
}

impl Drop for H {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

const A: &str = "sa-redshield";

fn second_builtin() -> String {
    themes::list_themes()[1]["id"].as_str().unwrap().to_string()
}

#[test]
fn clicking_cards_and_samples_only_previews() {
    let mut h = H::new();
    let s0 = h.settings();
    let b = second_builtin();
    h.click(&format!("theme-card-{b}"));
    assert_eq!(h.ui.sel, b, "누른 테마가 미리보기로 골라진다");
    h.click("theme-sample-1");
    assert_eq!(h.ui.sample, 1);
    h.click("theme-sample-2");
    h.click("theme-filter-1");
    h.click("theme-filter-0");
    h.click(&format!("theme-card-{A}"));
    h.click("theme-slot-all");
    h.click("theme-slot-overlay");
    assert!(h.seen("theme-preview-main").is_some() && h.seen("theme-preview-overlay").is_some(), "출력별 미리보기를 그린다");
    assert_eq!(h.settings(), s0, "고르기·견본·거르기·출력 고르기는 설정을 바꾸지 않는다");
    assert!(h.ui.undo.is_none());
    // 찾기: 없는 이름이면 카드가 사라진다 (상태는 그대로)
    h.ui.query = "없는이름zz".into();
    h.frame(vec![]);
    assert!(h.seen(&format!("theme-card-{b}")).is_none());
    h.ui.query.clear();
    h.frame(vec![]);
    assert!(h.seen(&format!("theme-card-{b}")).is_some());
    assert_eq!(h.settings(), s0);
}

#[test]
fn apply_respects_chosen_outputs_and_undo_restores() {
    let mut h = H::new();
    let s0 = h.settings();
    h.click(&format!("theme-card-{A}"));
    assert_eq!(h.ui.slots, [true, true, false], "찬송가 칸은 따로 켜야 한다");
    // OBS 자막을 끄고 예배당에만 적용
    h.click("theme-slot-overlay");
    assert_eq!(h.ui.slots, [true, false, false]);
    assert_eq!(h.settings(), s0);
    h.click("theme-apply");
    assert_eq!(h.active("main"), "theme-sa-redshield-main");
    assert_eq!(h.active("overlay"), s0["activeTemplates"]["overlay"].as_str().unwrap(), "고르지 않은 출력은 그대로");
    assert_eq!(h.active("overlayLyrics"), s0["activeTemplates"]["overlayLyrics"].as_str().unwrap());
    assert!(h.ui.undo.is_some() && !h.ui.msg_err);
    // 되돌리기
    h.click("theme-undo");
    assert_eq!(h.settings(), s0, "되돌리면 적용 전과 같다");
    assert!(h.ui.undo.is_none());
    assert!(h.seen("theme-undo").is_none(), "되돌린 뒤에는 되돌리기 단추가 없다");
    // 모두 고르기 → 기본 테마는 예배당 + OBS 자막만 (찬송가 칸은 디자인이 없어 그대로)
    h.click("theme-slot-all");
    h.click("theme-apply");
    assert_eq!(h.active("main"), "theme-sa-redshield-main");
    assert_eq!(h.active("overlay"), "theme-sa-redshield-overlay");
    assert_eq!(h.active("overlayLyrics"), s0["activeTemplates"]["overlayLyrics"].as_str().unwrap());
}

#[test]
fn normal_apply_keeps_edits_and_reset_needs_confirm() {
    let mut h = H::new();
    h.click(&format!("theme-card-{A}"));
    h.click("theme-apply");
    assert!(h.seen("theme-reset").is_none(), "고친 것이 없으면 처음 디자인으로 단추가 없다");
    // 사용자가 템플릿 편집 창에서 고친 것처럼
    h.core.lock().settings.update(|d| d["templates"].as_array_mut().unwrap().iter_mut().find(|t| t["id"] == "theme-sa-redshield-main").unwrap()["font"]["size"] = json!(91));
    let size = |h: &H| h.core.lock().settings.template("theme-sa-redshield-main").unwrap()["font"]["size"].clone();
    // 다른 테마를 적용했다가 다시 적용해도 고친 것이 남는다
    let b = second_builtin();
    h.click(&format!("theme-card-{b}"));
    h.click("theme-apply");
    h.click(&format!("theme-card-{A}"));
    h.click("theme-apply");
    assert_eq!(h.active("main"), "theme-sa-redshield-main");
    assert_eq!(size(&h), json!(91), "일반 적용은 고친 디자인을 지우지 않는다");
    // 처음 디자인으로: 누르기만 해서는 안 바뀌고, 확인해야 바뀐다
    h.click("theme-reset");
    assert_eq!(h.ui.confirm, Some(Confirm::Reset));
    assert_eq!(size(&h), json!(91));
    h.click("theme-confirm-no");
    assert_eq!(h.ui.confirm, None);
    assert_eq!(size(&h), json!(91));
    h.click("theme-reset");
    h.click("theme-confirm-yes");
    assert_ne!(size(&h), json!(91), "확인한 뒤에야 처음 디자인으로");
    assert_eq!(h.active("main"), "theme-sa-redshield-main");
    h.click("theme-undo");
    assert_eq!(size(&h), json!(91), "원복도 되돌릴 수 있다");
}

#[test]
fn save_duplicate_rename_delete_with_confirm() {
    let mut h = H::new();
    let s0 = h.settings();
    // 기본 테마에는 이름 바꾸기·삭제·편집이 없다
    h.click(&format!("theme-card-{A}"));
    for n in ["theme-rename", "theme-delete", "theme-edit-main"] {
        assert!(h.seen(n).is_none(), "기본 테마에 {n} 이 있으면 안 됨");
    }
    // 지금 출력 디자인을 내 테마로 저장 (이름 입력)
    h.click("theme-save-current");
    assert!(h.ui.form.is_some());
    assert_eq!(h.settings(), s0, "이름을 받기 전에는 아무것도 저장하지 않는다");
    h.type_text("Q");
    assert!(h.ui.form.as_ref().unwrap().name.contains('Q'), "이름칸에 글이 들어간다");
    h.click("theme-form-ok");
    let cs = h.customs();
    assert_eq!(cs.len(), 1);
    let id1 = cs[0]["id"].as_str().unwrap().to_string();
    assert!(cs[0]["name"].as_str().unwrap().contains('Q'));
    assert_eq!(h.ui.sel, id1, "새 테마가 골라진다");
    assert_eq!(h.settings()["activeTemplates"], s0["activeTemplates"], "저장은 출력을 바꾸지 않는다");
    assert!(h.ui.form.is_none());
    // 그만두기는 아무것도 만들지 않는다
    h.click("theme-dup");
    h.click("theme-form-cancel");
    assert_eq!(h.customs().len(), 1);
    // 기본 테마를 복제해서 수정
    h.click(&format!("theme-card-{A}"));
    h.click("theme-dup");
    assert!(matches!(h.ui.form.as_ref().unwrap().kind, FormKind::Duplicate(_)));
    h.click("theme-form-ok");
    let cs = h.customs();
    assert_eq!(cs.len(), 2);
    let id2 = cs[1]["id"].as_str().unwrap().to_string();
    assert_eq!(h.ui.sel, id2);
    assert!(h.core.lock().settings.template("theme-sa-redshield-main").is_none(), "복제가 기본 테마 템플릿을 만들지 않는다");
    // 편집 요청: 새 id 의 템플릿을 한 번만 넘긴다
    h.click("theme-edit-main");
    let want = cs[1]["slots"]["main"].as_str().unwrap().to_string();
    assert_eq!(h.ui.take_edit_request(), Some(want.clone()));
    assert_eq!(h.ui.take_edit_request(), None);
    assert_ne!(want, "theme-sa-redshield-main");
    // 이름 바꾸기
    h.click("theme-rename");
    h.type_text("Z");
    h.click("theme-form-ok");
    assert!(h.customs()[1]["name"].as_str().unwrap().contains('Z'));
    assert_eq!(h.customs()[1]["id"], json!(id2), "이름을 바꿔도 id 는 그대로");
    // 내 테마 적용 → 삭제는 확인을 거친다
    h.click("theme-apply");
    assert_eq!(h.active("main"), want);
    let before = h.settings();
    h.click("theme-delete");
    assert_eq!(h.ui.confirm, Some(Confirm::Delete));
    assert_eq!(h.settings(), before, "확인 전에는 지우지 않는다");
    h.click("theme-confirm-no");
    assert_eq!(h.customs().len(), 2);
    h.click("theme-delete");
    h.click("theme-confirm-yes");
    assert_eq!(h.customs().len(), 1);
    assert_eq!(h.customs()[0]["id"], json!(id1));
    assert_eq!(h.active("main"), want, "지운 테마의 디자인을 출력이 쓰고 있으면 그대로 나간다");
    assert!(h.core.lock().settings.template(&want).is_some());
}

#[test]
fn export_import_through_ui() {
    let mut h = H::new();
    h.core.lock().settings.update(|d| d["secretForTest"] = json!("SECRET-VALUE"));
    h.click(&format!("theme-card-{A}"));
    let file = h.dir.join("out.wptheme.json");
    let core = h.core.clone();
    h.ui.export_to(&core, &file);
    assert!(!h.ui.msg_err, "{}", h.ui.msg);
    let txt = std::fs::read_to_string(&file).unwrap();
    assert!(!txt.contains("SECRET"));
    // 깨진 파일: 오류 글만, 설정은 그대로
    let s0 = h.settings();
    let bad = h.dir.join("bad.json");
    std::fs::write(&bad, "{ \"format\": \"다른 것\" }").unwrap();
    h.ui.import_from(&core, &bad);
    assert!(h.ui.msg_err);
    assert_eq!(h.settings(), s0);
    h.ui.import_from(&core, &h.dir.join("없는 파일.json"));
    assert!(h.ui.msg_err);
    assert_eq!(h.settings(), s0);
    // 정상 파일: 내 테마로 들어오고 출력은 그대로
    h.ui.import_from(&core, &file);
    assert!(!h.ui.msg_err, "{}", h.ui.msg);
    assert_eq!(h.customs().len(), 1);
    assert_eq!(h.ui.sel, h.customs()[0]["id"].as_str().unwrap());
    assert_eq!(h.settings()["activeTemplates"], s0["activeTemplates"]);
    h.frame(vec![]);
    assert!(h.seen(&format!("theme-card-{}", h.ui.sel)).is_some(), "가져온 테마 카드가 보인다");
    // 순서별 테마로도 쓸 수 있다
    let mut e = json!({});
    assert!(apply_item_theme(&mut e, &core, &h.ui.sel.clone()));
    assert_eq!(e["tplMain"], h.customs()[0]["slots"]["main"]);
}

#[test]
fn fits_small_screens() {
    for size in [Vec2::new(1280.0, 720.0), Vec2::new(900.0, 640.0)] {
        let mut h = H::sized(size);
        h.frame(vec![]);
        let names: Vec<(String, Rect)> = widgets::HOVER_SEEN.with(|m| m.borrow().iter().map(|(k, v)| (k.clone(), *v)).collect());
        assert!(names.iter().any(|(k, _)| k.starts_with("theme-card-")), "{size:?}: 카드가 그려져야 함");
        assert!(names.iter().any(|(k, _)| k == "theme-apply"), "{size:?}: 적용 단추가 그려져야 함");
        for (k, r) in names.into_iter().filter(|(_, r)| r.is_positive()) {
            assert!(r.left() >= 0.0 && r.right() <= size.x + 0.5, "{size:?}: {k} 이(가) 가로로 잘림 {r:?}");
        }
    }
}

#[test]
fn quick_apply_and_manage_controls_stay_visible_without_scrolling() {
    for size in [Vec2::new(1280.0, 720.0), Vec2::new(900.0, 640.0)] {
        let mut h = H::sized(size);
        for key in ["theme-quick-apply", "theme-quick-dup", "theme-quick-export", "theme-quick-slot-main"] {
            let r = h.seen(key).unwrap_or_else(|| panic!("{size:?}: 그린 자리 없음 {key}"));
            assert!(r.top() >= 0.0 && r.bottom() <= size.y && r.left() >= 0.0 && r.right() <= size.x, "{size:?}: {key} 이 화면 밖 {r:?}");
        }
        let b = second_builtin();
        h.ui.sel = b;
        h.frame(vec![]);
        h.click("theme-quick-apply");
        assert!(h.ui.undo.is_some(), "스크롤 없이 실제 적용되어야 함");
    }
}
