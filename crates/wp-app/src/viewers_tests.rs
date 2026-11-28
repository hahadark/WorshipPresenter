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

/// 홍보 캡처 중 발견: 예배 시간표 범위 밖(예배 전 · 뒤)에 무대 타이머를 켜면 시간표의 무대 줄이 비어 보였다
#[test]
fn timeline_shows_running_timer_outside_plan() {
    let mut h = H::new(notices());
    {
        let mut c = h.app.core.lock();
        c.settings.update(|d| d["rsProLayout"]["timeline"] = json!(true));
        // 지금부터 5시간 전에 시작해 10분이면 끝나는 계획 (지금 시각은 시간표 범위 밖)
        let now = (wp_core::util::now_ms() + wp_core::localtime::local_offset_ms()).div_euclid(60_000).rem_euclid(1440);
        let st = (now - 300).rem_euclid(1440);
        if let Some(q) = c.cue.as_mut() {
            q.start_time = format!("{:02}:{:02}", st / 60, st % 60);
            for it in q.items.iter_mut() {
                it["duration"] = json!(5);
            }
        }
        c.stage_timer("start", 5.0).unwrap();
    }
    idle(&mut h, 3);
    let r = seen("timeline-timer").expect("무대 타이머가 시간표 무대 줄에 보여야 함");
    let head = seen("pro-tl-fold").unwrap();
    assert!(r.right() <= head.right() + 30.0 && r.left() >= 0.0, "줄 안에 보임: {r:?}");
}

/// 홍보 캡처 중 발견: 다른 곳(분 단추 · 리모컨 · /api/trigger/timer)으로 켠 타이머의 끝 시각이 무대 '몇 시까지' 칸에 보여야 함.
/// 2026-11-28 부터 조작은 인스펙터 '무대' 탭 한 곳 — 무대 화면 창의 [무대 탭 열기] 로 그 탭을 연다
#[test]
fn stage_window_until_field_shows_actual_end() {
    let mut h = H::new(notices());
    h.app.core.lock().stage_timer("start", 30.0).unwrap();
    let t = h.app.core.lock().b.stage.timer.clone();
    let m = ((t.end_at + wp_core::localtime::local_offset_ms()).div_euclid(60_000)).rem_euclid(1440);
    crate::track_c::handle_menu(&mut h.app, "b:stage");
    let p = at(&mut h, "stage-win-open-tab");
    click_at(&mut h, p);
    idle(&mut h, 3);
    assert!(seen("stage-timer-at").is_some(), "인스펙터 무대 탭의 '몇 시까지' 칸");
    assert_eq!(h.app.tb.timer_at_for_test(), format!("{}:{:02}", m / 60, m % 60));
}

/// 주인 결정 (2026-11-28): 무대 화면 창은 미리 보기와 주소만, 조작(타이머 · 메시지 · 고정)은 인스펙터 '무대' 탭.
/// 창은 본문 판 오른쪽 아래에 떠서 뷰어(미리보기의 '다음' 슬라이드 줄)를 가리지 않는다
#[test]
fn stage_window_is_preview_and_address_only_and_clear_of_viewers() {
    let mut h = H::new(notices());
    h.app.core.lock().preview_item(0);
    idle(&mut h, 2);
    crate::track_c::handle_menu(&mut h.app, "b:stage");
    idle(&mut h, 4);
    let prev = seen("stage-win-preview").expect("무대 화면 미리 보기");
    assert!(seen("stage-win-copy").is_some(), "주소 복사 단추");
    for gone in ["stage-timer-big", "stage-timer-at", "stage-timer-at-go"] {
        assert!(seen(gone).is_none(), "창(과 글자 탭)에는 조작이 없다: {gone}");
    }
    let viewers = crate::widgets_win::viewers_rect(&h.ctx).expect("뷰어 자리");
    let body = crate::widgets_win::body_rect(&h.ctx).expect("본문 판 자리");
    assert!(prev.top() > viewers.bottom(), "뷰어 아래: {prev:?} / {viewers:?}");
    assert!(prev.top() > body.top() + crate::theme::HEAD_H, "본문 판 머리도 가리지 않음: {prev:?} / {body:?}");
    assert!(prev.right() <= body.right() && prev.center().x > body.center().x, "본문 판 오른쪽: {prev:?} / {body:?}");
    // 무대 탭으로 옮긴 조작은 하나도 사라지지 않았다
    h.app.pro_open_stage_tab(&h.ctx);
    idle(&mut h, 3);
    for keep in ["stage-timer-big", "stage-timer-at", "stage-timer-at-go"] {
        assert!(seen(keep).is_some(), "인스펙터 무대 탭: {keep}");
    }
}

/// 파일 → 새 큐시트 · Ctrl+N: 큐시트 판 폴더 단추의 '새 큐시트' 와 같은 동작 (Act::NewCue)
#[test]
fn file_menu_new_cue_and_ctrl_n() {
    let mut h = H::new(notices());
    idle(&mut h, 2);
    let before = h.app.core.lock().cues.len();
    let first = h.app.core.lock().cue.as_ref().map(|c| c.id.clone());
    crate::track_c::handle_menu(&mut h.app, "b:newcue");
    let (n1, id1) = { let c = h.app.core.lock(); (c.cues.len(), c.cue.as_ref().map(|c| c.id.clone())) };
    assert_eq!(n1, before + 1, "메뉴: 큐시트 하나 더");
    assert_ne!(id1, first, "새 큐시트가 열림");
    assert!(h.app.core.lock().cue.as_ref().unwrap().items.is_empty(), "폴더 단추의 '새 큐시트' 와 같은 빈 큐시트");
    idle(&mut h, 1);
    let m = Modifiers { ctrl: true, command: true, ..Default::default() };
    full(&mut h, vec![Event::ModifiersChanged(m), Event::Key { key: Key::N, physical_key: None, pressed: true, repeat: false, modifiers: m }]);
    full(&mut h, vec![Event::Key { key: Key::N, physical_key: None, pressed: false, repeat: false, modifiers: m }, Event::ModifiersChanged(Modifiers::NONE)]);
    assert_eq!(h.app.core.lock().cues.len(), before + 2, "Ctrl+N");
}
