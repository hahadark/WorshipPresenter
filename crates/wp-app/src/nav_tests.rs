//! 키보드 이동 점검 (docs/navigation-parity.md): 원본 Electron control.js·cue.js 의 목록 키를 합성 입력으로 본다.
//!  1 검색칸 Esc = 내용 모두 고르기 (control.js:1077)
//!  2 큐시트 순서 목록 ↑↓ · Alt+↑↓ · Enter · Delete (cue.js:1271)
//!  3 성경 장 격자 방향키 (control.js:390)
//!  4 송출 기록 목록 ↑↓ · Enter (control.js:1364)
//! 키 한 번이 한 프레임에 두 군데(전역 단축키와 목록)에서 처리되지 않는지도 함께 본다.
use super::auditc_tests::{seen, H};
use super::{Act, App, Snap};
use crate::cue_ui::{nav_owner, Nav};
use eframe::egui::{self, Event, Key, Modifiers, PointerButton, Vec2};
use serde_json::{json, Value};

fn title(id: &str, t: &str) -> Value {
    json!({ "id": id, "type": "title", "title": t })
}
fn abc() -> Vec<Value> {
    vec![title("a", "가"), title("b", "나"), title("c", "다")]
}
fn titles(h: &H) -> Vec<String> {
    h.app.core.lock().cue.as_ref().unwrap().items.iter().map(|it| it["title"].as_str().unwrap_or("").to_string()).collect()
}
fn live_visible(h: &H) -> Option<bool> {
    h.app.core.lock().pres.live.as_ref().map(|l| l.visible)
}

/// 실제 화면 차례: 전역 단축키 → 검색칸 → 큐시트 목록
fn search_and_cue(app: &mut App, ctx: &egui::Context, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
    let typing = ctx.egui_wants_keyboard_input();
    app.shortcuts(ctx, acts, typing);
    app.search_bar(ui, s, acts);
    app.cue_panel(ui, s, acts);
}
/// 전역 단축키 → 성경 책 쪽 (폭 300: 장 격자가 여러 줄이 되게)
fn books(app: &mut App, ctx: &egui::Context, ui: &mut egui::Ui, _: &Snap, acts: &mut Vec<Act>) {
    let typing = ctx.egui_wants_keyboard_input();
    app.shortcuts(ctx, acts, typing);
    ui.allocate_ui(Vec2::new(300.0, 900.0), |ui| crate::cue_ui::books(app, ui));
}
/// 전역 단축키 → 창들(송출 기록) → 큐시트 목록
fn hist(app: &mut App, ctx: &egui::Context, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
    let typing = ctx.egui_wants_keyboard_input();
    app.shortcuts(ctx, acts, typing);
    crate::track_c::windows(app, ctx, acts);
    app.take_history_loaded();
    app.cue_panel(ui, s, acts);
}
fn focus_search(h: &mut H) {
    h.app.focus_search = true;
    h.frame(vec![], search_and_cue);
    h.frame(vec![], search_and_cue);
}
fn search_focused(h: &H) -> bool {
    h.ctx.memory(|m| m.has_focus(egui::Id::new("search-box")))
}

// ---------------- 1. 검색칸 Esc ----------------

#[test]
fn nav_search_esc_selects_all_and_keeps_focus() {
    let mut h = H::new(vec![]);
    focus_search(&mut h);
    h.type_text("요3:16", search_and_cue);
    h.key(Key::Escape, Modifiers::NONE, search_and_cue);
    assert!(search_focused(&h), "Esc 뒤에도 검색칸에 초점이 남는다 (원본 select())");
    assert_eq!(h.app.search, "요3:16", "글은 그대로");
    h.type_text("x", search_and_cue);
    assert_eq!(h.app.search, "x", "모두 골라졌으므로 다음 글자가 전체를 바꾼다");
    // 두 번째 Esc 도 같다 (초점을 놓고 전역으로 넘어가지 않음)
    h.key(Key::Escape, Modifiers::NONE, search_and_cue);
    assert!(search_focused(&h));
}

#[test]
fn nav_search_esc_does_not_fire_global_escape_shortcut_while_typing() {
    // 전역 Esc 가 불렸는지 보려고 글자 숨김(toggleClear)을 Esc 에 둔다 — 보조 화면 끄기(outputOff)와 같은 길(on())을 탄다
    let mut h = H::new(vec![title("a", "가")]);
    h.app.core.lock().settings.update(|d| {
        d["shortcuts"]["toggleClear"] = json!("Escape");
        d["shortcuts"]["outputOff"] = json!("");
    });
    h.app.apply(vec![Act::SendItem(0)]);
    assert_eq!(live_visible(&h), Some(true));
    focus_search(&mut h);
    h.type_text("요3", search_and_cue);
    h.key(Key::Escape, Modifiers::NONE, search_and_cue);
    assert_eq!(live_visible(&h), Some(true), "글 쓰는 중의 Esc 는 전역 단축키로 가지 않는다");
    assert!(search_focused(&h));
    // 검색칸을 떠난 뒤의 Esc 는 전역 단축키 그대로, 한 번만
    h.ctx.memory_mut(|m| m.surrender_focus(egui::Id::new("search-box")));
    h.frame(vec![], search_and_cue);
    h.key(Key::Escape, Modifiers::NONE, search_and_cue);
    assert_eq!(live_visible(&h), Some(false), "칸 밖에서는 전역 Esc 가 한 번 동작");
}

// ---------------- 2. 큐시트 순서 목록 ----------------

#[test]
fn nav_cue_arrows_only_select_and_enter_previews_then_sends_once() {
    let mut h = H::new(abc());
    h.click("cue-row-0", search_and_cue);
    assert_eq!(nav_owner(&h.ctx), Nav::Cue, "줄을 누르면 목록이 키보드를 갖는다");
    assert_eq!((h.app.sel, h.preview_title().as_deref()), (Some(0), Some("가")));
    h.key(Key::ArrowDown, Modifiers::NONE, search_and_cue);
    assert_eq!(h.app.sel, Some(1), "↓ 한 번 = 한 줄 (두 번 처리되면 2)");
    assert_eq!(h.preview_title().as_deref(), Some("가"), "↑↓ 는 고르기만 — 미리보기는 그대로 (cue.js: st.sel 만 바꿈)");
    // Enter: 고른 순서가 미리보기가 아니면 미리보기만. 전역 Enter(= 미리보기 송출)가 함께 불리면 '가' 가 나간다
    h.key(Key::Enter, Modifiers::NONE, search_and_cue);
    assert_eq!(h.preview_title().as_deref(), Some("나"));
    assert!(h.live_title().is_none(), "첫 Enter 는 송출하지 않음: {:?}", h.live_title());
    h.key(Key::Enter, Modifiers::NONE, search_and_cue);
    assert_eq!(h.live_title().as_deref(), Some("나"), "이미 미리보기 중인 순서에서 Enter = 송출");
    // 끝에서 멈춤
    h.key(Key::ArrowDown, Modifiers::NONE, search_and_cue);
    h.key(Key::ArrowDown, Modifiers::NONE, search_and_cue);
    assert_eq!(h.app.sel, Some(2));
    h.key(Key::ArrowUp, Modifiers::NONE, search_and_cue);
    h.key(Key::ArrowUp, Modifiers::NONE, search_and_cue);
    h.key(Key::ArrowUp, Modifiers::NONE, search_and_cue);
    assert_eq!(h.app.sel, Some(0));
    assert_eq!(h.live_title().as_deref(), Some("나"), "고르기만으로 LIVE 가 바뀌지 않음");
}

#[test]
fn nav_cue_enter_is_quiet_in_direct_mode_but_click_still_sends() {
    let mut h = H::new(abc());
    h.direct();
    h.click("cue-row-0", search_and_cue);
    assert_eq!(h.live_title().as_deref(), Some("가"), "바로 송출 모드의 클릭 = 송출 (Rust 판의 의도적 차이는 그대로)");
    h.key(Key::ArrowDown, Modifiers::NONE, search_and_cue);
    assert_eq!(h.live_title().as_deref(), Some("가"), "↓ 는 바로 송출 모드에서도 송출하지 않음");
    h.key(Key::Enter, Modifiers::NONE, search_and_cue);
    assert_eq!(h.preview_title().as_deref(), Some("나"));
    assert_eq!(h.live_title().as_deref(), Some("가"), "Enter 의 미리보기는 조용히 (quietPreview: holdDirect)");
    h.key(Key::Enter, Modifiers::NONE, search_and_cue);
    assert_eq!(h.live_title().as_deref(), Some("나"), "두 번째 Enter = 송출");
}

#[test]
fn nav_cue_alt_arrows_move_one_step_and_undo_restores() {
    let mut h = H::new(abc());
    h.click("cue-row-0", search_and_cue);
    h.key(Key::ArrowUp, Modifiers::ALT, search_and_cue);
    assert_eq!(titles(&h), ["가", "나", "다"], "맨 위에서 Alt+↑ 는 아무 일도 없음");
    h.key(Key::ArrowDown, Modifiers::ALT, search_and_cue);
    assert_eq!(titles(&h), ["나", "가", "다"], "Alt+↓ 한 번 = 한 칸 (고르기 이동으로도 겹쳐 처리되지 않음)");
    assert_eq!(h.app.sel, Some(1), "옮긴 순서를 계속 고른 채로");
    h.key(Key::ArrowDown, Modifiers::ALT, search_and_cue);
    h.key(Key::ArrowDown, Modifiers::ALT, search_and_cue);
    assert_eq!(titles(&h), ["나", "다", "가"], "맨 아래에서 멈춤");
    assert_eq!(h.app.sel, Some(2));
    h.app.apply(vec![Act::Undo(false)]);
    assert_eq!(titles(&h), ["나", "가", "다"], "되돌리기로 한 칸씩 되살아남");
}

#[test]
fn nav_cue_delete_removes_one_keeps_position_and_is_undoable() {
    let mut h = H::new(abc());
    h.click("cue-row-0", search_and_cue);
    h.key(Key::ArrowDown, Modifiers::NONE, search_and_cue);
    h.key(Key::Delete, Modifiers::NONE, search_and_cue);
    assert_eq!(titles(&h), ["가", "다"], "Delete 한 번 = 고른 순서 하나만");
    assert_eq!(h.app.sel, Some(1), "지운 자리의 다음 순서를 고른다 (removeItem: min(i, len-1))");
    assert_eq!(h.app.core.lock().toast_act, Some("undo"), "알림에 [되돌리기]");
    h.app.apply(vec![Act::Undo(false)]);
    assert_eq!(titles(&h), ["가", "나", "다"], "Ctrl+Z 로 되살아남");
    // 맨 끝을 지우면 새 끝으로
    h.frame(vec![], search_and_cue);
    h.app.sel = Some(2);
    h.key(Key::Delete, Modifiers::NONE, search_and_cue);
    assert_eq!(titles(&h), ["가", "나"]);
    assert_eq!(h.app.sel, Some(1));
}

#[test]
fn nav_cue_keys_do_nothing_while_typing_or_after_clicking_elsewhere() {
    let mut h = H::new(abc());
    h.click("cue-row-1", search_and_cue);
    assert_eq!(h.app.sel, Some(1));
    // 글 쓰는 중: Delete·↑↓·Alt+↓ 가 목록에 닿지 않는다
    focus_search(&mut h);
    assert_eq!(nav_owner(&h.ctx), Nav::None, "글 쓰는 칸에 들어가면 목록은 키보드를 놓는다");
    h.type_text("가나", search_and_cue);
    h.key(Key::Delete, Modifiers::NONE, search_and_cue);
    h.key(Key::ArrowUp, Modifiers::NONE, search_and_cue);
    h.key(Key::ArrowDown, Modifiers::ALT, search_and_cue);
    assert_eq!(titles(&h), ["가", "나", "다"], "글 쓰는 중에는 지우기·옮기기가 안 됨");
    assert_eq!(h.app.sel, Some(1));
    // 검색칸을 나와도(초점만 놓음) 목록을 다시 누르기 전에는 키를 받지 않는다
    h.ctx.memory_mut(|m| m.surrender_focus(egui::Id::new("search-box")));
    h.frame(vec![], search_and_cue);
    h.key(Key::Delete, Modifiers::NONE, search_and_cue);
    assert_eq!(titles(&h), ["가", "나", "다"], "목록에 키보드가 없으면 Delete 는 아무 일도 없음");
}

#[test]
fn nav_cue_keys_are_blocked_while_row_menu_is_open() {
    let mut h = H::new(abc());
    h.click("cue-row-1", search_and_cue);
    let p = seen("cue-row-1").unwrap().center();
    for pressed in [true, false] {
        h.frame(vec![Event::PointerButton { pos: p, button: PointerButton::Secondary, pressed, modifiers: Default::default() }], search_and_cue);
    }
    h.frame(vec![], search_and_cue);
    assert!(egui::Popup::is_any_open(&h.ctx), "오른쪽 클릭 메뉴가 열림");
    h.key(Key::Delete, Modifiers::NONE, search_and_cue);
    h.key(Key::ArrowDown, Modifiers::ALT, search_and_cue);
    assert_eq!(titles(&h), ["가", "나", "다"], "메뉴가 열려 있는 동안 목록 키는 동작하지 않음");
}

// ---------------- 3. 성경 장 격자 ----------------

/// 시편 30장(장마다 1절)을 시험 성경에 더한다
fn with_psalms(h: &mut H) -> String {
    use wp_core::importer::{replace_version, Data, Row};
    let mut c = h.app.core.lock();
    let mut verses: Vec<Row> = (1..=30).map(|ch| Row { book: 19, chapter: ch, verse: 1, text: format!("시편 {ch}편") }).collect();
    verses.push(Row { book: 43, chapter: 3, verse: 16, text: "하나님이 세상을 이처럼 사랑하사".into() });
    replace_version(&c.store.db, &json!({ "id": "tst", "name": "시험", "source": "s", "license": "l" }), &Data { verses, ..Default::default() }).unwrap();
    c.store.reload_structure().unwrap();
    let v = c.version_id();
    let st = c.store.get_structure(&v);
    st.iter().find(|b| b["id"] == 19).and_then(|b| b["short"].as_str()).unwrap_or("").to_string()
}

#[test]
fn nav_chapter_grid_arrows_move_focus_and_enter_opens() {
    let mut h = H::new(vec![title("a", "가")]);
    let short = with_psalms(&mut h);
    assert!(!short.is_empty());
    h.app.apply(vec![Act::PreviewItem(0)]);
    h.ctx.data_mut(|d| d.insert_temp(egui::Id::new("cue-book-sel"), Some(19i64)));
    h.click("chap-2", books);
    assert_eq!(h.app.search, format!("{short}2"), "장을 누르면 검색칸에 '시2', 절 목록");
    assert_eq!(h.app.result["passage"]["verses"].as_array().map(|a| a.len()), Some(1));
    assert_eq!(nav_owner(&h.ctx), Nav::Chapters, "누른 장 단추가 키보드를 갖는다");
    assert_eq!(h.preview_title().as_deref(), Some("가"), "장을 여는 것은 절 목록만 — 미리보기는 그대로");
    let top = seen("chap-1").unwrap().top();
    let cols = (1..=30).filter(|k| seen(&format!("chap-{k}")).is_some_and(|r| (r.top() - top).abs() < 1.0)).count() as i64;
    assert!((2..30).contains(&cols), "여러 줄이어야 ↑↓ 를 볼 수 있다: {cols}");
    // → 는 초점만 옮긴다 (열지 않음). 전역 ←→(미리보기 앞뒤)·Enter(송출)는 불리지 않는다
    h.key(Key::ArrowRight, Modifiers::NONE, books);
    assert_eq!(h.app.search, format!("{short}2"), "방향키는 초점만");
    h.key(Key::Enter, Modifiers::NONE, books);
    assert_eq!(h.app.search, format!("{short}3"), "→ 한 번 = 한 칸, Enter = 그 장 열기");
    assert!(h.live_title().is_none(), "장 격자의 Enter 가 미리보기를 송출하면 안 됨");
    h.key(Key::ArrowDown, Modifiers::NONE, books);
    h.key(Key::Enter, Modifiers::NONE, books);
    assert_eq!(h.app.search, format!("{short}{}", 3 + cols), "↓ = 한 줄 아래");
    h.key(Key::ArrowUp, Modifiers::NONE, books);
    h.key(Key::ArrowLeft, Modifiers::NONE, books);
    h.key(Key::Enter, Modifiers::NONE, books);
    assert_eq!(h.app.search, format!("{short}2"), "↑ 한 줄 위 · ← 한 칸 앞");
    // 끝에서 멈춤 (원본: max(0, min(len-1, i+d)))
    h.key(Key::ArrowUp, Modifiers::NONE, books);
    h.key(Key::ArrowLeft, Modifiers::NONE, books);
    h.key(Key::Enter, Modifiers::NONE, books);
    assert_eq!(h.app.search, format!("{short}1"));
    for _ in 0..40 {
        h.key(Key::ArrowDown, Modifiers::NONE, books);
    }
    h.key(Key::Enter, Modifiers::NONE, books);
    assert_eq!(h.app.search, format!("{short}30"));
    assert!(h.live_title().is_none());
    assert_eq!(h.preview_title().as_deref(), Some("가"));
    assert_eq!(nav_owner(&h.ctx), Nav::Chapters, "장을 연 뒤에도 초점은 격자에 남는다 (절 목록으로 가지 않음)");
}

#[test]
fn nav_chapter_grid_keys_need_focus() {
    let mut h = H::new(vec![]);
    let short = with_psalms(&mut h);
    h.ctx.data_mut(|d| d.insert_temp(egui::Id::new("cue-book-sel"), Some(19i64)));
    h.frame(vec![], books);
    h.frame(vec![], books);
    // 장을 누르기 전에는 방향키·Enter 가 격자에 닿지 않는다
    h.key(Key::ArrowRight, Modifiers::NONE, books);
    h.key(Key::Enter, Modifiers::NONE, books);
    assert_eq!(h.app.search, "");
    h.click("chap-5", books);
    assert_eq!(h.app.search, format!("{short}5"));
    // 책 찾기 칸에 글을 쓰는 동안에도 닿지 않는다
    h.ctx.data_mut(|d| d.insert_temp(egui::Id::new("cue-book-focus"), true));
    h.frame(vec![], books);
    h.frame(vec![], books);
    assert_eq!(nav_owner(&h.ctx), Nav::None);
    h.key(Key::ArrowRight, Modifiers::NONE, books);
    assert_eq!(h.app.search, format!("{short}5"));
}

// ---------------- 4. 송출 기록 ----------------

fn open_history(h: &mut H) {
    h.app.tc.history.open = true;
    h.frame(vec![], hist);
    h.frame(vec![], hist);
}

/// 기록: [0] 설교(제목 순서) · [1] 요한복음 3:16(큐시트의 성경 순서). 그 뒤 큐시트에서 성경 순서를 지워 번호가 밀린 상태
fn stale_history() -> H {
    let mut h = H::new(vec![json!({ "id": "a", "type": "bible", "title": "성경봉독", "ref": "요3:16" }), title("b", "설교")]);
    h.app.apply(vec![Act::SendItem(0)]);
    assert_eq!(h.live_title().as_deref(), Some("요한복음 3:16"));
    h.app.core.lock().pres.live.as_mut().unwrap().sent_at = 0; // 1.5초 겹침 방지에 걸리지 않게
    h.app.apply(vec![Act::SendItem(1)]);
    assert_eq!(h.live_title().as_deref(), Some("설교"));
    h.app.core.lock().remove_item(0); // 이제 0번 순서 = 설교
    h.app.apply(vec![Act::PreviewItem(0)]);
    {
        let mut c = h.app.core.lock();
        c.pres.live = None;
        c.toast = None;
        let hist = c.settings.get()["history"].clone();
        assert_eq!(hist[0]["title"], "설교");
        assert_eq!(hist[1]["title"], "요한복음 3:16");
    }
    h
}

#[test]
fn nav_history_arrows_and_enter_replay_the_recorded_passage_not_a_cue_index() {
    let mut h = stale_history();
    open_history(&mut h);
    assert_eq!(nav_owner(&h.ctx), Nav::History, "창을 열면 목록이 키보드를 갖는다");
    assert_eq!(h.app.tc.history.sel, 0);
    h.key(Key::ArrowDown, Modifiers::NONE, hist);
    assert_eq!(h.app.tc.history.sel, 1, "↓ 한 번 = 한 줄");
    assert_eq!(h.app.sel, None, "기록 창이 키를 갖는 동안 큐시트 목록은 움직이지 않는다");
    h.key(Key::ArrowDown, Modifiers::NONE, hist);
    assert_eq!(h.app.tc.history.sel, 1, "끝에서 멈춤");
    h.key(Key::Enter, Modifiers::NONE, hist);
    assert!(!h.app.tc.history.open, "Enter = 다시 띄우고 창 닫기 (load → closeModal)");
    assert_eq!(h.preview_title().as_deref(), Some("요한복음 3:16"));
    let pv = h.app.core.lock().pres.preview.as_ref().unwrap().passage.clone();
    assert!(pv["cueIndex"].as_i64().unwrap_or(-1) < 0 && pv["cueId"].as_str().unwrap_or("").is_empty(), "예전 순서 번호를 달고 오지 않는다: {pv}");
    assert_eq!(h.app.search, "요3:16", "검색칸에 그때의 검색 글");
    assert_eq!(h.app.result["passage"]["title"], "요한복음 3:16", "절 목록도 그 본문");
    assert!(h.live_title().is_none(), "미리보기 모드: Enter 는 미리보기만 (전역 Enter 가 겹쳐 송출하지 않음)");
    h.frame(vec![], hist);
    assert_eq!(nav_owner(&h.ctx), Nav::None, "창이 닫히면 키보드를 놓는다");
}

#[test]
fn nav_history_replay_in_direct_mode_sends_the_passage_itself() {
    let mut h = stale_history();
    h.direct();
    open_history(&mut h);
    h.key(Key::ArrowDown, Modifiers::NONE, hist);
    h.key(Key::Enter, Modifiers::NONE, hist);
    assert_eq!(h.live_title().as_deref(), Some("요한복음 3:16"), "바로 송출 모드: 다시 띄우면 송출 (loadPassage → directSend)");
    let live = h.app.core.lock().pres.live.as_ref().unwrap().deck.passage.clone();
    assert!(live["cueIndex"].as_i64().unwrap_or(-1) < 0 && live["cueId"].as_str().unwrap_or("").is_empty(), "지금 0번 순서(설교)가 아니라 기록된 본문이 나간다: {live}");
}

#[test]
fn nav_history_enter_on_non_bible_row_sends_nothing_and_keeps_old_preview() {
    let mut h = stale_history();
    h.direct();
    h.app.core.lock().pres.live = None;
    open_history(&mut h);
    // 첫 줄 = '설교' (성경이 아님) — 미리보기에는 다른 것(설교 순서)이 올라가 있다
    h.key(Key::Enter, Modifiers::NONE, hist);
    assert!(h.live_title().is_none(), "찾지 못한 기록에서 Enter 가 올려 둔 미리보기를 송출하면 안 됨: {:?}", h.live_title());
    assert_eq!(h.preview_title().as_deref(), Some("설교"), "미리보기는 그대로");
    assert_eq!(h.app.core.lock().toast.as_ref().map(|t| t.1), Some("warn"), "찾지 못했다는 알림");
    assert!(!h.app.tc.history.open, "원본처럼 창은 닫힌다");
}

#[test]
fn nav_history_click_and_double_click_use_recorded_title() {
    let mut h = stale_history();
    open_history(&mut h);
    h.click("hist-row-1", hist);
    assert_eq!(h.preview_title().as_deref(), Some("요한복음 3:16"), "한 번 누름 = 다시 띄움");
    assert!(h.live_title().is_none());
    assert!(h.app.tc.history.open, "누를 때는 창을 닫지 않는다 (두 번 눌러 송출하도록 — Rust 판의 차이)");
    assert_eq!(h.app.tc.history.sel, 1, "누른 줄을 고른다");
    let p = seen("hist-row-1").unwrap().center();
    h.click_at(p, hist);
    h.click_at(p, hist);
    assert_eq!(h.live_title().as_deref(), Some("요한복음 3:16"), "두 번 누름 = 송출");
}
