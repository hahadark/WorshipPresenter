//! 갈래 E 점검: 화면 언어 메뉴(보기 → Language · 언어)를 합성 입력으로 실제 눌러 바뀌는지 본다.
//! 화면 언어는 프로세스 전체 값(i18n::set_lang)이라, 한국어 문구를 보는 다른 시험과 같이 돌면 서로 흔들린다.
//! 그래서 평소에는 건너뛰고 따로 돌린다: cargo test -p wp-app --offline -- --ignored lang_menu --test-threads=1
use super::auditc_tests::{seen, H};
use super::{Act, App, Snap};
use eframe::egui;

fn menu(app: &mut App, _ctx: &egui::Context, ui: &mut egui::Ui, _s: &Snap, acts: &mut Vec<Act>) {
    crate::track_c::menu_bar(app, ui, acts);
}

#[test]
#[ignore = "화면 언어가 프로세스 전체 값이라 따로 돌림 (--ignored lang_menu --test-threads=1)"]
fn lang_menu_click_switches_to_english_and_back() {
    let mut h = H::new(vec![]);
    wp_core::i18n::set_lang("ko");
    let lang = |h: &H| h.app.core.lock().settings.get()["uiLang"].as_str().unwrap_or("").to_string();

    h.click("menu-보기", menu);
    assert!(seen("menuitem-c:lang-en").is_some(), "보기 메뉴를 누르면 언어 항목이 보여야 함");
    h.click("menuitem-c:lang-en", menu);
    assert_eq!(lang(&h), "en", "English 를 누르면 설정 uiLang 이 en");
    assert!(wp_core::i18n::is_en(), "누른 바로 그 프레임부터 영어 화면");
    h.frame(vec![], menu);
    assert!(seen("menuitem-c:lang-en").is_none(), "고르면 메뉴가 닫힘");
    assert_eq!(wp_core::i18n::tr("설정"), "Settings");

    // 영어 화면에서도 메뉴 자리 이름(내부 이름)은 그대로라 다시 눌러 한국어로 돌아올 수 있다
    h.click("menu-보기", menu);
    h.click("menuitem-c:lang-ko", menu);
    assert_eq!(lang(&h), "ko");
    assert!(!wp_core::i18n::is_en());
    assert_eq!(wp_core::i18n::tr("설정"), "설정");
}
