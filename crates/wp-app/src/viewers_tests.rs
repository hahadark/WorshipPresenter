//! 뷰어 하나만 보기 (2026-11-27): '둘 다' · '송출 중만' 보기 방식의 합성 입력 시험.
//! 실제 화면 전체(App::ui_body)를 headless 로 그리고 뷰어 머리 구획 단추 · 단축키 · 보기 메뉴 · 바로 송출 자동 전환을 확인한다.
use super::auditc_tests::{seen, H};
use super::dbl_tests::{at, click_at, full, idle};
use eframe::egui::{Event, Key, Modifiers};
use serde_json::json;

fn notices() -> Vec<serde_json::Value> {
    vec![
        json!({ "id": "a", "type": "text", "title": "공지", "body": "첫째 장\n---\n둘째 장\n---\n셋째 장" }),
        json!({ "id": "b", "type": "text", "title": "광고", "body": "하나\n---\n둘" }),
    ]
}
fn saved_mode(h: &H) -> serde_json::Value {
    h.app.core.lock().settings.get()["rsProLayout"]["viewerMode"].clone()
}
/// 미리보기 뷰어 · 송출 뷰어 · '다음' 줄이 이번 프레임에 그려졌는가
fn shown(h: &mut H) -> (bool, bool, bool) {
    idle(h, 3);
    (seen("pane-preview").is_some(), seen("pane-live").is_some(), seen("next-mini").is_some())
}
fn ctrl_shift_l(h: &mut H) {
    let m = Modifiers { ctrl: true, shift: true, command: true, ..Default::default() };
    full(h, vec![Event::ModifiersChanged(m), Event::Key { key: Key::L, physical_key: None, pressed: true, repeat: false, modifiers: m }]);
    full(h, vec![Event::Key { key: Key::L, physical_key: None, pressed: false, repeat: false, modifiers: m }, Event::ModifiersChanged(Modifiers::NONE)]);
}

#[test]
fn head_buttons_switch_to_live_only_and_back_and_persist() {
    let mut h = H::new(notices());
    h.app.core.lock().preview_item(0);
    assert_eq!(shown(&mut h), (true, true, false), "기본은 둘 다");
    let p = at(&mut h, "pro-viewer-live");
    click_at(&mut h, p);
    assert_eq!(saved_mode(&h), json!("live"), "고른 방식은 설정에 저장 (다음 실행에도)");
    assert_eq!(shown(&mut h), (false, true, true), "송출 중만: 미리보기 뷰어 대신 '다음' 줄");
    // 송출 뷰어는 가운데 판 오른쪽 칸, 본문·편집은 왼쪽에서 위부터
    let live = seen("pane-live").unwrap();
    let head = seen("pro-view-head").unwrap();
    assert!(live.left() >= head.left() - 1.0 && head.width() < 700.0, "송출 뷰어 칸은 오른쪽에 작게: {head:?}");
    assert!(seen("pv-thumbs-fold").is_none() && seen("pv-chip-0").is_none(), "미리보기 슬라이드 줄도 숨김");
    let p = at(&mut h, "pro-viewer-both");
    click_at(&mut h, p);
    assert_eq!(saved_mode(&h), json!("both"));
    assert_eq!(shown(&mut h), (true, true, false));
}

#[test]
fn shortcut_and_view_menu_toggle() {
    let mut h = H::new(notices());
    idle(&mut h, 2);
    ctrl_shift_l(&mut h);
    assert_eq!(saved_mode(&h), json!("live"), "Ctrl+Shift+L");
    crate::track_c::handle_menu(&mut h.app, "w:viewers");
    assert_eq!(saved_mode(&h), json!("both"), "보기 메뉴 '뷰어: 둘 다 · 송출 중만 바꾸기'");
    // 단축키 설정에서 바꾼 키를 따른다
    h.app.core.lock().settings.update(|d| d["shortcuts"]["toggleViewers"] = json!("F7"));
    idle(&mut h, 1);
    full(&mut h, vec![Event::Key { key: Key::F7, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }]);
    full(&mut h, vec![Event::Key { key: Key::F7, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE }]);
    assert_eq!(saved_mode(&h), json!("live"), "바꾼 키 F7");
    let menus = crate::menubar::menus_with_shortcuts(h.app.core.lock().settings.get());
    let it = menus.iter().flat_map(|m| &m.items).find(|i| i.id == "w:viewers").unwrap();
    assert_eq!(it.key, "F7", "메뉴의 단축키 안내도 바꾼 키");
}

#[test]
fn live_only_keeps_preview_controls() {
    let mut h = H::new(notices());
    h.app.core.lock().settings.update(|d| d["rsProLayout"]["viewerMode"] = json!("live"));
    h.app.core.lock().preview_item(0);
    let p = at(&mut h, "next-next");
    click_at(&mut h, p);
    idle(&mut h, 1);
    assert_eq!(h.app.core.lock().pres.preview.as_ref().map(|d| d.index), Some(1), "'다음' 줄 ▶ = 미리보기 다음 슬라이드");
    let p = at(&mut h, "next-prev");
    click_at(&mut h, p);
    idle(&mut h, 1);
    assert_eq!(h.app.core.lock().pres.preview.as_ref().map(|d| d.index), Some(0));
    assert!(h.app.core.lock().pres.live.is_none(), "미리보기 조작은 송출하지 않는다");
    // PageDown · F9 도 그대로
    full(&mut h, vec![Event::Key { key: Key::PageDown, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }]);
    full(&mut h, vec![Event::Key { key: Key::F9, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }]);
    idle(&mut h, 1);
    assert_eq!(h.app.core.lock().pres.live.as_ref().map(|l| l.deck.index), Some(1), "PageDown 뒤 F9 로 송출");
}

#[test]
fn direct_mode_auto_live_only_and_back() {
    let mut h = H::new(notices());
    h.app.core.lock().preview_item(0);
    assert_eq!(shown(&mut h).0, true);
    h.direct();
    let (pv, lv, next) = shown(&mut h);
    assert!(!pv && lv && !next, "바로 송출: 송출 뷰어만, '다음' 줄도 없음");
    assert!(saved_mode(&h).is_null(), "저절로 바뀐 것은 저장하지 않음");
    // 바로 송출 중에 단추로 '둘 다' → 이번 바로 송출 동안만
    let p = at(&mut h, "pro-viewer-both");
    click_at(&mut h, p);
    assert_eq!(shown(&mut h).0, true, "바로 송출이어도 둘 다로 바꿀 수 있음");
    assert!(saved_mode(&h).is_null());
    // 미리보기 모드로 돌아오면 원래 방식(둘 다), 다시 바로 송출이면 또 저절로 하나만
    h.app.core.lock().settings.update(|d| d["previewMode"] = json!(true));
    assert_eq!(shown(&mut h).0, true);
    h.direct();
    assert_eq!(shown(&mut h).0, false, "다시 바로 송출이면 저절로 송출 중만");
    // 설정을 끄면 바로 송출에서도 고른 방식 그대로
    h.app.core.lock().settings.update(|d| d["ui"]["directLiveOnly"] = json!(false));
    assert_eq!(shown(&mut h).0, true, "설정 '바로 송출 모드에서는 자동으로 송출 뷰어만' 끔");
    // 원래 '송출 중만' 을 골라 둔 사람은 미리보기 모드로 돌아와도 송출 중만
    h.app.core.lock().settings.update(|d| { d["ui"]["directLiveOnly"] = json!(true); d["rsProLayout"]["viewerMode"] = json!("live"); d["previewMode"] = json!(true); });
    assert_eq!(shown(&mut h), (false, true, true));
}

#[test]
fn shortcut_listed_in_settings_and_defaults() {
    assert_eq!(wp_core::settings::DEFAULTS["shortcuts"]["toggleViewers"], json!(crate::prolayout_ui::VIEWERS_KEY));
    assert!(wp_core::settings::SHORTCUTS["labels"]["toggleViewers"].as_str().is_some_and(|s| !s.is_empty()));
    assert_eq!(wp_core::settings::DEFAULTS["ui"]["directLiveOnly"], json!(true));
    // 기본 단축키가 다른 기본 단축키와 겹치지 않음
    let d = &wp_core::settings::DEFAULTS["shortcuts"];
    let n = d.as_object().unwrap().values().filter(|v| v.as_str() == Some(crate::prolayout_ui::VIEWERS_KEY)).count();
    assert_eq!(n, 1);
    assert!(crate::settings_ui::items::ITEMS.iter().any(|i| i.id == "ui.directLiveOnly"));
}

#[test]
fn live_only_width_bounds() {
    use crate::prolayout_ui::live_only_width;
    assert_eq!(live_only_width(1000.0, None), 360.0);
    assert!(live_only_width(500.0, None) >= 250.0 && live_only_width(500.0, None) <= 300.0);
    assert_eq!(live_only_width(1000.0, Some(0.9)), 600.0, "비율 한도 0.6");
    assert!(live_only_width(2400.0, None) <= 864.0);
}
