//! 갈래 C: 두 번 클릭(더블클릭) 합성 입력 시험. 원본 dblclick 처리기 13곳(docs/behavior-parity.md 1절)을
//! 실제 화면 전체(App::ui_body)를 headless 로 그리고 '첫 클릭 → 프레임 진행 → 같은 화면 좌표에 두 번째 클릭' 으로 확인한다.
//! 첫 클릭으로 배치가 바뀌어도(편집칸·미리보기·바로 송출) 같은 항목이면 두 번 클릭으로 쳐야 한다 (widgets::click2).
use super::auditc_tests::{seen, H};
use super::{Act, App, Snap};
use crate::center_ui::SlideView;
use crate::widgets::{self, Click};
use eframe::egui::{self, Event, PointerButton, Pos2, RawInput, Rect, Vec2};
use serde_json::{json, Value};

/// 화면 전체 한 프레임 (실제 App::ui_body: 메뉴·머리 줄·작업공간 패널·창 모두)
pub(super) fn full(h: &mut H, events: Vec<Event>) {
    h.time += 0.05;
    widgets::HOVER_SEEN.with(|m| m.borrow_mut().clear());
    let app = &mut h.app;
    let mut out = h.ctx.run_pass(
        RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1600.0, 2600.0))), time: Some(h.time), events, ..Default::default() },
        |ctx| {
            ctx.root_ui(|ui| app.ui_body(ui));
        },
    );
    out.textures_delta.clear();
}
pub(super) fn idle(h: &mut H, n: usize) {
    for _ in 0..n {
        full(h, vec![]);
    }
}
pub(super) fn at(h: &mut H, name: &str) -> Pos2 {
    idle(h, 3);
    seen(name).unwrap_or_else(|| panic!("그린 자리 없음: {name} (있는 것: {:?})", widgets::HOVER_SEEN.with(|m| m.borrow().keys().cloned().collect::<Vec<_>>()))).center()
}
pub(super) fn click_at(h: &mut H, p: Pos2) {
    full(h, vec![Event::PointerMoved(p)]);
    for pressed in [true, false] {
        full(h, vec![Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: Default::default() }]);
    }
}
/// 두 번 클릭: 첫 클릭 뒤 화면이 다시 그려지고(배치가 바뀔 수 있음) 같은 화면 좌표를 다시 누른다
pub(super) fn dbl_at(h: &mut H, p: Pos2) {
    click_at(h, p);
    idle(h, 1);
    click_at(h, p);
    idle(h, 1);
}
pub(super) fn dbl(h: &mut H, name: &str) {
    let p = at(h, name);
    dbl_at(h, p);
}
fn titles() -> Vec<Value> {
    vec![
        json!({ "id": "a", "type": "text", "title": "공지", "body": "첫째 장\n---\n둘째 장\n---\n셋째 장" }),
        json!({ "id": "b", "type": "text", "title": "광고", "body": "하나\n---\n둘" }),
        json!({ "id": "c", "type": "text", "title": "축도", "body": "끝" }),
    ]
}
/// LIVE 로 나간 것: (cueIndex, 슬라이드 번호)
pub(super) fn live(h: &H) -> Option<(i64, usize)> {
    let c = h.app.core.lock();
    c.pres.live.as_ref().map(|l| (l.deck.passage["cueIndex"].as_i64().unwrap_or(-1), l.deck.index))
}
fn version(h: &H) -> u64 {
    h.app.core.lock().pres.version as u64
}
fn items(h: &H) -> Vec<Value> {
    h.app.core.lock().cue.as_ref().unwrap().items.clone()
}

// ---------------- click2 자체 ----------------

thread_local! {
    static LOG: std::cell::RefCell<Vec<(usize, Click)>> = Default::default();
    static SHIFT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}
/// 줄 셋. SHIFT 가 켜지면 위에 띠가 생겨 줄이 한 칸씩 아래로 밀린다 (첫 클릭으로 편집칸이 열리는 것과 같은 상황)
fn rows(_: &mut App, _: &egui::Context, ui: &mut egui::Ui, _: &Snap, _: &mut Vec<Act>) {
    if SHIFT.with(|s| s.get()) {
        ui.allocate_exact_size(Vec2::new(200.0, 30.0), egui::Sense::hover());
    }
    ui.spacing_mut().item_spacing.y = 0.0;
    for i in 0..3 {
        let (r, resp) = ui.allocate_exact_size(Vec2::new(200.0, 30.0), egui::Sense::click());
        widgets::mark(&format!("r{i}"), r);
        let c = widgets::click2(ui, ("t-row", i), &resp);
        if c != Click::None {
            LOG.with(|l| l.borrow_mut().push((i, c)));
            if c == Click::Single {
                SHIFT.with(|s| s.set(true));
            }
        }
    }
}
fn log() -> Vec<(usize, Click)> {
    LOG.with(|l| l.borrow().clone())
}
fn reset() {
    LOG.with(|l| l.borrow_mut().clear());
    SHIFT.with(|s| s.set(false));
}

#[test]
fn click2_second_click_counts_even_when_first_click_shifts_the_list() {
    reset();
    let mut h = H::new(vec![]);
    h.frame(vec![], rows);
    h.frame(vec![], rows);
    let p = seen("r1").unwrap().center();
    h.click_at(p, rows); // 첫 클릭 → 줄이 30px 밀려 이 자리에는 0번 줄이 온다
    h.frame(vec![], rows);
    assert!(seen("r0").unwrap().contains(p), "시험 전제: 첫 클릭 뒤 그 자리에 다른 줄이 와 있다");
    h.click_at(p, rows);
    assert_eq!(log(), vec![(1, Click::Single), (1, Click::Double)], "밀려도 처음 누른 줄의 두 번 클릭 · 그 자리에 온 줄은 클릭을 받지 않는다");
}

#[test]
fn click2_slow_second_click_within_half_second_is_double_and_later_is_single() {
    reset();
    SHIFT.with(|s| s.set(true));
    let mut h = H::new(vec![]);
    h.frame(vec![], rows);
    let p = seen("r2").unwrap().center();
    h.click_at(p, rows);
    for _ in 0..4 {
        h.frame(vec![], rows); // 0.2초 쉼 → 두 번째 클릭이 0.35초 뒤에 끝난다 (egui 기본 0.3초로는 놓침)
    }
    h.click_at(p, rows);
    assert_eq!(log(), vec![(2, Click::Single), (2, Click::Double)]);
    // 세 번째 클릭은 새 한 번 클릭 · 0.5초를 넘기면 다시 한 번 클릭
    h.click_at(p, rows);
    for _ in 0..12 {
        h.frame(vec![], rows);
    }
    h.click_at(p, rows);
    assert_eq!(log()[2..], [(2, Click::Single), (2, Click::Single)]);
}

#[test]
fn click2_two_different_rows_are_two_single_clicks() {
    reset();
    SHIFT.with(|s| s.set(true));
    let mut h = H::new(vec![]);
    h.frame(vec![], rows);
    let (a, b) = (seen("r0").unwrap().center(), seen("r2").unwrap().center());
    h.click_at(a, rows);
    h.click_at(b, rows);
    assert_eq!(log(), vec![(0, Click::Single), (2, Click::Single)]);
}

// ---------------- 4) 큐시트 순서 줄 (cue.js:1266 — 의도적 차이: 두 번 = 송출) ----------------

#[test]
fn dbl_cue_row_sends_in_preview_mode() {
    let mut h = H::new(titles());
    let p = at(&mut h, "cue-row-1");
    click_at(&mut h, p);
    assert_eq!(live(&h), None, "한 번 = 미리보기만");
    assert_eq!(h.preview_title().as_deref(), Some("광고"));
    idle(&mut h, 1);
    click_at(&mut h, p);
    assert_eq!(live(&h), Some((1, 0)), "두 번 = 그 순서 송출");
}

#[test]
fn dbl_cue_row_in_direct_mode_sends_once_and_stays_on_that_item() {
    let mut h = H::new(titles());
    h.direct();
    let p = at(&mut h, "cue-row-2");
    click_at(&mut h, p);
    assert_eq!(live(&h), Some((2, 0)), "바로 송출: 첫 클릭에 나간다");
    idle(&mut h, 1);
    click_at(&mut h, p);
    idle(&mut h, 1);
    assert_eq!(live(&h), Some((2, 0)), "두 번째 클릭이 다른 줄에 떨어지지 않는다");
    assert_eq!(h.app.sel, Some(2));
}

#[test]
fn dbl_cue_row_with_editor_open_still_sends() {
    // 고친 줄(편집칸이 열린 상태)에서 다른 줄을 두 번: 첫 클릭이 편집칸을 닫아 배치가 바뀐다
    let mut h = H::new(titles());
    idle(&mut h, 2);
    h.app.show_item_editor(0);
    h.app.apply(vec![Act::PreviewItem(0)]);
    dbl(&mut h, "cue-row-1");
    assert_eq!(live(&h), Some((1, 0)));
}

// ---------------- 3) 순서의 슬라이드 줄 (cue.js:945) ----------------

#[test]
fn dbl_slide_row_sends_that_slide() {
    let mut h = H::new(titles());
    idle(&mut h, 2);
    h.app.apply(vec![Act::PreviewItem(0)]);
    crate::center_ui::test_set_view(&h.ctx, SlideView::List, true);
    dbl(&mut h, "srow-2");
    assert_eq!(live(&h), Some((0, 2)));
}

// ---------------- 8·9) 격자 · 전체 격자 · 전체 목록 (grid.js:50, 151) ----------------

#[test]
fn dbl_grid_cell_sends_that_slide() {
    let mut h = H::new(titles());
    idle(&mut h, 2);
    h.app.apply(vec![Act::PreviewItem(0)]);
    crate::center_ui::test_set_view(&h.ctx, SlideView::Grid, true);
    dbl(&mut h, "cell-1");
    assert_eq!(live(&h), Some((0, 1)));
}

#[test]
fn dbl_all_grid_cell_of_another_item_sends_it() {
    let mut h = H::new(titles());
    idle(&mut h, 2);
    h.app.apply(vec![Act::PreviewItem(0)]);
    crate::center_ui::test_set_view(&h.ctx, SlideView::All, true);
    // cell-1 은 마지막에 그린 것 = 2번째 순서(광고)의 둘째 장
    dbl(&mut h, "cell-1");
    assert_eq!(live(&h), Some((1, 1)), "전체 격자: 다른 순서의 칸을 두 번 누르면 그 순서 그 장이 나간다 (첫 클릭으로 미리보기가 바뀌어도)");
}

#[test]
fn dbl_all_list_line_sends_it() {
    let mut h = H::new(titles());
    idle(&mut h, 2);
    h.app.apply(vec![Act::PreviewItem(0)]);
    crate::center_ui::test_set_view(&h.ctx, SlideView::AllList, true);
    dbl(&mut h, "cgl-1-1");
    assert_eq!(live(&h), Some((1, 1)));
}

// ---------------- 2) 검색 결과 줄 (control.js:917) ----------------

#[test]
fn dbl_search_result_row_previews_and_sends() {
    let mut h = H::new(titles());
    idle(&mut h, 2);
    h.app.search = "요3:16-17".into();
    h.app.apply(vec![Act::Search("요3:16-17".into())]);
    crate::center_ui::test_set_view(&h.ctx, SlideView::List, false);
    dbl(&mut h, "row-1");
    let c = h.app.core.lock();
    let l = c.pres.live.as_ref().expect("두 번 = 송출");
    let on_air = serde_json::to_string(&l.deck.slides[l.deck.index]).unwrap();
    assert!(on_air.contains("보내신"), "누른 절(17절)이 든 슬라이드가 나간다: {on_air}");
}

// ---------------- 1) 미리보기 아래 슬라이드 번호 (control.js:611) ----------------

#[test]
fn dbl_preview_chip_moves_and_sends() {
    let mut h = H::new(titles());
    idle(&mut h, 2);
    h.app.apply(vec![Act::PreviewItem(0)]);
    dbl(&mut h, "pv-chip-2");
    assert_eq!(live(&h), Some((0, 2)));
    let v = version(&h);
    idle(&mut h, 2);
    assert_eq!(version(&h), v);
    let _ = items(&h);
}

/// 첫 클릭 뒤 그 항목이 화면에서 밀리지 않는지 (밀리면 두 번째 클릭이 다른 것에 떨어진다)
fn stays(h: &mut H, name: &str) -> bool {
    let p = at(h, name);
    click_at(h, p);
    idle(h, 4);
    seen(name).is_some_and(|r| r.contains(p))
}

#[test]
fn first_click_in_all_view_does_not_scroll_the_clicked_cell_away() {
    // 화면보다 긴 큐시트 (스크롤이 생김). 미리보기 순서가 바뀌면 그 순서를 맨 위로 끌어올리던 것이 눌린 줄을 밀어냈다
    let many: Vec<Value> = (0..80).map(|i| json!({ "id": format!("m{i}"), "type": "text", "title": format!("순서{i}"), "body": "하나\n---\n둘\n---\n셋" })).collect();
    let mut h = H::new(many);
    idle(&mut h, 2);
    h.app.apply(vec![Act::PreviewItem(0)]);
    h.app.sel = Some(0); // 순서를 고른 상태(편집칸이 이미 열림). 고른 순서가 없을 때는 첫 클릭이 편집칸을 열어 밀린다 → dbl_all_* 시험
    crate::center_ui::test_set_view(&h.ctx, SlideView::AllList, true);
    idle(&mut h, 3);
    assert!(seen("cgl-79-0").is_some_and(|r| r.top() > 2600.0), "시험 전제: 목록이 화면보다 길다");
    assert!(stays(&mut h, "cgl-3-1"), "전체 목록에서 다른 순서의 줄을 눌러도 그 줄은 제자리");
    assert_eq!(h.preview_title().as_deref(), Some("순서3"));
    let p = seen("cgl-3-1").unwrap().center();
    idle(&mut h, 10);
    click_at(&mut h, p);
    assert_eq!(live(&h), None, "0.5초가 지난 뒤의 클릭은 한 번 클릭");
    dbl(&mut h, "cgl-4-1");
    assert_eq!(live(&h), Some((4, 1)));
}

#[test]
fn first_click_keeps_rows_in_place_everywhere() {
    type Prep = fn(&mut H);
    let cases: Vec<(&str, Prep)> = vec![
        ("cue-row-1", |_| {}),
        ("cue-row-2", |h| h.direct()),
        ("cue-row-1", |h| { h.app.show_item_editor(0); h.app.apply(vec![Act::PreviewItem(0)]); }),
        ("srow-2", |h| { h.app.apply(vec![Act::PreviewItem(0)]); crate::center_ui::test_set_view(&h.ctx, SlideView::List, true); }),
        ("srow-2", |h| { h.direct(); h.app.apply(vec![Act::PreviewItem(0)]); crate::center_ui::test_set_view(&h.ctx, SlideView::List, true); }),
        ("cell-1", |h| { h.app.apply(vec![Act::PreviewItem(0)]); crate::center_ui::test_set_view(&h.ctx, SlideView::Grid, true); }),
        ("row-1", |h| { h.app.search = "요3:16-17".into(); h.app.apply(vec![Act::Search("요3:16-17".into())]); crate::center_ui::test_set_view(&h.ctx, SlideView::List, false); }),
        ("row-1", |h| { h.direct(); h.app.search = "요3:16-17".into(); h.app.apply(vec![Act::Search("요3:16-17".into())]); crate::center_ui::test_set_view(&h.ctx, SlideView::List, false); }),
        ("pv-chip-2", |h| { h.app.apply(vec![Act::PreviewItem(0)]); }),
    ];
    let mut h = H::new(titles());
    for (n, (name, prep)) in cases.into_iter().enumerate() {
        // 같은 자료로 다시 시작
        h.app.core.lock().settings.update(|d| d["previewMode"] = json!(true));
        h.app.core.lock().pres = Default::default();
        h.app.sel = None;
        h.app.edit = None;
        idle(&mut h, 2);
        prep(&mut h);
        assert!(stays(&mut h, name), "{n}번 경우: 첫 클릭 뒤 {name} 이 그 자리에 있어야 한다");
    }
}
// ---------------- 두 번 클릭으로 송출하면 꺼져 있던 예배당 화면이 켜진다 (core.js:549 output.autoOnSend) ----------------
// 화면 전체를 그리면 실제 예배당 창을 띄우므로(trackd_ui::frame) 패널만 그리고, 코어가 켜 달라고 요청했는지를 본다.

fn cue_only(app: &mut App, _: &egui::Context, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
    app.cue_panel(ui, s, acts);
}
fn center_only(app: &mut App, _: &egui::Context, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
    app.center_body(ui, s, acts);
}
fn pv_only(app: &mut App, _: &egui::Context, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
    let d = s.preview.clone();
    app.deck_view(ui, "미리보기", d.as_ref(), false, None, s, acts);
}
fn dbl_panel(h: &mut H, name: &str, f: super::auditc_tests::Draw) {
    h.frame(vec![], f);
    h.frame(vec![], f);
    let p = seen(name).unwrap_or_else(|| panic!("그린 자리 없음: {name}")).center();
    h.click_at(p, f);
    h.frame(vec![], f);
    h.click_at(p, f);
    h.frame(vec![], f);
}
fn asked(h: &H) -> u32 {
    h.app.core.lock().output_on_requests
}
fn auto_on(h: &mut H) {
    h.app.core.lock().settings.update(|d| d["output"]["autoOnSend"] = json!(true));
}

#[test]
fn double_click_send_asks_to_turn_output_on_on_every_path() {
    type Prep = fn(&mut H);
    let cases: Vec<(&str, super::auditc_tests::Draw, Prep)> = vec![
        ("cue-row-1", cue_only, |_| {}),
        ("srow-2", center_only, |h| { h.app.apply(vec![Act::PreviewItem(0)]); crate::center_ui::test_set_view(&h.ctx, SlideView::List, true); }),
        ("cell-1", center_only, |h| { h.app.apply(vec![Act::PreviewItem(0)]); crate::center_ui::test_set_view(&h.ctx, SlideView::Grid, true); }),
        ("cell-1", center_only, |h| { h.app.apply(vec![Act::PreviewItem(0)]); crate::center_ui::test_set_view(&h.ctx, SlideView::All, true); }),
        ("cgl-1-1", center_only, |h| { h.app.apply(vec![Act::PreviewItem(0)]); crate::center_ui::test_set_view(&h.ctx, SlideView::AllList, true); }),
        ("row-1", center_only, |h| { h.app.search = "요3:16-17".into(); h.app.apply(vec![Act::Search("요3:16-17".into())]); crate::center_ui::test_set_view(&h.ctx, SlideView::List, false); }),
        ("pv-chip-2", pv_only, |h| { h.app.apply(vec![Act::PreviewItem(0)]); }),
    ];
    for (n, (name, f, prep)) in cases.into_iter().enumerate() {
        let mut h = H::new(titles());
        prep(&mut h);
        auto_on(&mut h);
        assert_eq!(asked(&h), 0);
        dbl_panel(&mut h, name, f);
        assert!(live(&h).is_some(), "{n}번 경우({name}): 두 번 클릭 = 송출");
        assert!(asked(&h) >= 1, "{n}번 경우({name}): 송출하면 꺼져 있던 예배당 화면을 켜 달라고 요청한다");
    }
}

#[test]
fn direct_mode_first_click_asks_to_turn_output_on_and_second_click_too() {
    let mut h = H::new(titles());
    h.direct();
    auto_on(&mut h);
    h.click("cue-row-1", cue_only);
    assert_eq!(live(&h), Some((1, 0)));
    assert_eq!(asked(&h), 1, "바로 송출: 첫 클릭에 켜 달라고 요청");
    // 방금 보낸 것을 또 누르면 다시 보내지는 않지만(1.5초), 꺼져 있으면 또 켜 달라고 한다
    let v = version(&h);
    h.app.apply(vec![Act::SendItem(1)]);
    assert_eq!(version(&h), v, "같은 것을 다시 송출하지는 않는다");
    assert!(asked(&h) >= 2);
}

#[test]
fn auto_on_off_setting_is_respected() {
    let mut h = H::new(titles());
    dbl_panel(&mut h, "cue-row-0", cue_only);
    assert!(live(&h).is_some());
    assert_eq!(asked(&h), 0, "자동 켜기를 끄면 요청하지 않는다 (LIVE 칸 띠 [지금 켜기] 로 안내)");
}

// ---------------- 10) 라이브러리 탭마다: 한 번 = 고르기, 두 번 = 큐시트에 넣기 (library.js:84) ----------------

fn lib(app: &mut App, _: &egui::Context, ui: &mut egui::Ui, _: &Snap, acts: &mut Vec<Act>) {
    app.library_tabs_panel(ui, acts);
}
fn lib_tab(h: &H, tab: &str) {
    h.ctx.data_mut(|d| d.insert_temp(egui::Id::new("library-tab"), tab.to_string()));
}
fn types(h: &H) -> Vec<String> {
    items(h).iter().map(|x| x["type"].as_str().unwrap_or("").to_string()).collect()
}
fn count(h: &H, ty: &str) -> usize {
    types(h).iter().filter(|t| *t == ty).count()
}
/// 한 번 클릭 (그 뒤 0.5초 넘게 쉼)
fn single_panel(h: &mut H, name: &str, f: super::auditc_tests::Draw) {
    h.frame(vec![], f);
    h.frame(vec![], f);
    let p = seen(name).unwrap_or_else(|| panic!("그린 자리 없음: {name}")).center();
    h.click_at(p, f);
    for _ in 0..12 {
        h.frame(vec![], f);
    }
}

#[test]
fn dbl_library_song_adds_it_once() {
    let mut h = H::new(titles());
    h.app.core.lock().save_song(&json!({ "title": "은혜 아니면", "lyrics": "은혜 아니면 살아갈 수가 없네" })).unwrap();
    lib_tab(&h, "songs");
    single_panel(&mut h, "song-row-은혜 아니면", lib);
    assert_eq!(count(&h, "song"), 0, "한 번 = 고르기만");
    dbl_panel(&mut h, "song-row-은혜 아니면", lib);
    assert_eq!(count(&h, "song"), 1, "두 번 = 큐시트에 넣기 (한 번만)");
}

#[test]
fn dbl_library_hymn_row_adds_once_and_single_click_only_selects() {
    let mut h = H::new(titles());
    let dir = h.app.core.lock().data_dir.join("찬송가");
    std::fs::create_dir_all(&dir).unwrap();
    for n in ["003 성부 성자와 성령.pps", "010 전능왕 오셔서.pps"] {
        std::fs::write(dir.join(n), b"x").unwrap();
    }
    wp_core::hymns::add_folder(&mut h.app.core.lock(), &dir, None).unwrap();
    lib_tab(&h, "hymns");
    single_panel(&mut h, "hymn-row-1", lib);
    assert_eq!(count(&h, "hymn"), 0, "한 번 = 고르기만 (원본과 같게 — 한 번에 넣으면 두 번 누르는 습관으로 같은 찬송가가 중복됐다)");
    dbl_panel(&mut h, "hymn-row-1", lib);
    assert_eq!(count(&h, "hymn"), 1, "두 번 = 넣기 (한 번만)");
    let it = items(&h).into_iter().find(|x| x["type"] == "hymn").unwrap();
    assert!(it["title"].as_str().unwrap().contains("전능왕"), "{it}");
    // ＋ 단추는 한 번에 넣기
    h.app.core.lock().pick.forget_last();
    single_panel(&mut h, "hymn-add-0", lib);
    assert_eq!(count(&h, "hymn"), 2);
}

#[test]
fn hymn_search_arrow_and_enter_add_selected_row() {
    let mut h = H::new(titles());
    let dir = h.app.core.lock().data_dir.join("찬송가");
    std::fs::create_dir_all(&dir).unwrap();
    for n in ["003 성부 성자와 성령.pps", "010 전능왕 오셔서.pps"] {
        std::fs::write(dir.join(n), b"x").unwrap();
    }
    wp_core::hymns::add_folder(&mut h.app.core.lock(), &dir, None).unwrap();
    lib_tab(&h, "hymns");
    h.click("hymn-q", lib);
    h.key(egui::Key::ArrowDown, egui::Modifiers::NONE, lib);
    h.key(egui::Key::Enter, egui::Modifiers::NONE, lib);
    h.frame(vec![], lib);
    let it = items(&h).into_iter().find(|x| x["type"] == "hymn").expect("Enter = 고른 줄 넣기");
    assert!(it["title"].as_str().unwrap().contains("전능왕"), "↓ 로 둘째 줄을 골랐다: {it}");
}

#[test]
fn dbl_library_music_row_adds_audio_item_once() {
    let mut h = H::new(titles());
    let dir = h.app.core.lock().data_dir.join("음악");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("전주.mp3"), b"x").unwrap();
    h.app.core.lock().add_lib_folder("audio", &dir).unwrap();
    lib_tab(&h, "audio");
    single_panel(&mut h, "lib-audio-0", lib);
    assert_eq!(count(&h, "audio"), 0);
    dbl_panel(&mut h, "lib-audio-0", lib);
    assert_eq!(count(&h, "audio"), 1, "두 번 = 음악 순서로 넣기");
    let it = items(&h).into_iter().find(|x| x["type"] == "audio").unwrap();
    assert_eq!(it["fadeOnLeave"], json!(true), "library.js:340 과 같은 값: {it}");
}

#[test]
fn dbl_library_camera_toggles_live_background() {
    let mut h = H::new(titles());
    lib_tab(&h, "cameras");
    single_panel(&mut h, "cam-default", lib);
    assert!(h.app.core.lock().layers.media.is_none(), "한 번 = 아무 일 없음");
    dbl_panel(&mut h, "cam-default", lib);
    assert_eq!(h.app.core.lock().layers.media.as_ref().map(|m| m["type"].clone()), Some(json!("camera")), "두 번 = 송출 배경으로 (library.js:381 liveMedia)");
    for _ in 0..12 {
        h.frame(vec![], lib);
    }
    dbl_panel(&mut h, "cam-default", lib);
    assert!(h.app.core.lock().layers.media.is_none(), "다시 두 번 = 끄기");
}

#[test]
fn dbl_library_image_cell_adds_image_item_once() {
    let mut h = H::new(titles());
    let dir = h.app.core.lock().data_dir.join("그림");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("배경.png"), b"x").unwrap();
    h.app.core.lock().add_lib_folder("image", &dir).unwrap();
    lib_tab(&h, "images");
    h.frame(vec![], lib);
    h.frame(vec![], lib);
    let name = widgets::HOVER_SEEN.with(|m| m.borrow().keys().find(|k| k.starts_with("lib-cell-")).cloned()).expect("그림 칸");
    // 칸 가운데 아래쪽 (모서리의 마우스 올림 단추를 피해)
    let p = seen(&name).unwrap().center() + Vec2::new(0.0, 10.0);
    h.click_at(p, lib);
    for _ in 0..12 {
        h.frame(vec![], lib);
    }
    assert_eq!(count(&h, "image"), 0, "한 번 = 아무것도 넣지 않음");
    h.click_at(p, lib);
    h.frame(vec![], lib);
    h.click_at(p, lib);
    h.frame(vec![], lib);
    assert_eq!(count(&h, "image"), 1, "두 번 = 그림 순서로 넣기 (한 번만)");
}

// ---------------- 12·13) 오른쪽 클릭 메뉴: '▶ 바로 송출' · '큐시트에 넣기 (더블클릭)' (ctxmenu.js:141, 205) ----------------

fn rclick(h: &mut H, name: &str, f: super::auditc_tests::Draw) {
    h.frame(vec![], f);
    h.frame(vec![], f);
    let p = seen(name).unwrap_or_else(|| panic!("그린 자리 없음: {name}")).center();
    h.frame(vec![Event::PointerMoved(p)], f);
    for pressed in [true, false] {
        h.frame(vec![Event::PointerButton { pos: p, button: PointerButton::Secondary, pressed, modifiers: Default::default() }], f);
    }
    h.frame(vec![], f);
    h.frame(vec![], f);
}
/// 열린 메뉴의 항목을 누른다
fn menu_pick(h: &mut H, item: &str, f: super::auditc_tests::Draw) {
    let p = seen(item).unwrap_or_else(|| panic!("메뉴 항목 없음: {item} (있는 것: {:?})", widgets::HOVER_SEEN.with(|m| m.borrow().keys().filter(|k| k.starts_with("menu")).cloned().collect::<Vec<_>>()))).center();
    h.click_at(p, f);
    h.frame(vec![], f);
    h.frame(vec![], f);
}
fn searched(h: &mut H) {
    h.app.search = "요3:16-17".into();
    h.app.apply(vec![Act::Search("요3:16-17".into())]);
    crate::center_ui::test_set_view(&h.ctx, SlideView::List, false);
}

#[test]
fn verse_menu_send_now_goes_live_like_double_click() {
    let mut h = H::new(titles());
    searched(&mut h);
    rclick(&mut h, "row-1", center_only);
    assert!(seen("menu-preview").is_some() && seen("menu-cue").is_some(), "본문 줄 메뉴: 미리보기 · 바로 송출 · 큐시트에 넣기");
    menu_pick(&mut h, "menu-send", center_only);
    let c = h.app.core.lock();
    let l = c.pres.live.as_ref().expect("▶ 바로 송출 = 송출");
    assert!(serde_json::to_string(&l.deck.slides[l.deck.index]).unwrap().contains("보내신"));
}

#[test]
fn verse_menu_add_to_cue_inserts_bible_item_after_selection() {
    let mut h = H::new(titles());
    searched(&mut h);
    h.app.sel = Some(0);
    rclick(&mut h, "row-0", center_only);
    menu_pick(&mut h, "menu-cue", center_only);
    let its = items(&h);
    assert_eq!(its.len(), 4);
    assert_eq!(its[1]["type"], json!("bible"), "고른 순서 다음에: {:?}", types(&h));
    assert!(its[1]["ref"].as_str().unwrap().contains("3:16"), "{}", its[1]);
    assert_eq!(live(&h), None, "넣기는 송출하지 않는다 (미리보기 방식)");
}

#[test]
fn slide_row_and_grid_cell_menu_send_now() {
    for (view, name) in [(SlideView::List, "srow-2"), (SlideView::Grid, "cell-2"), (SlideView::AllList, "cgl-0-2")] {
        let mut h = H::new(titles());
        h.app.apply(vec![Act::PreviewItem(0)]);
        crate::center_ui::test_set_view(&h.ctx, view, true);
        rclick(&mut h, name, center_only);
        menu_pick(&mut h, "menu:바로 송출", center_only);
        assert_eq!(live(&h), Some((0, 2)), "{name}: 오른쪽 메뉴 '바로 송출'");
    }
}

#[test]
fn library_menu_add_to_cue_for_song_hymn_music_image() {
    let mut h = H::new(titles());
    h.app.core.lock().save_song(&json!({ "title": "은혜 아니면", "lyrics": "은혜 아니면 살아갈 수가 없네" })).unwrap();
    let root = h.app.core.lock().data_dir.clone();
    for (d, f) in [("찬송가", "010 전능왕 오셔서.pps"), ("음악", "전주.mp3"), ("그림", "배경.png")] {
        std::fs::create_dir_all(root.join(d)).unwrap();
        std::fs::write(root.join(d).join(f), b"x").unwrap();
    }
    wp_core::hymns::add_folder(&mut h.app.core.lock(), &root.join("찬송가"), None).unwrap();
    h.app.core.lock().add_lib_folder("audio", &root.join("음악")).unwrap();
    h.app.core.lock().add_lib_folder("image", &root.join("그림")).unwrap();
    for (tab, row, ty) in [("songs", "song-row-은혜 아니면", "song"), ("hymns", "hymn-row-0", "hymn"), ("audio", "lib-audio-0", "audio"), ("images", "lib-cell-배경.png", "image")] {
        lib_tab(&h, tab);
        h.frame(vec![], lib);
        h.frame(vec![], lib);
        let row = if tab == "images" { widgets::HOVER_SEEN.with(|m| m.borrow().keys().find(|k| k.starts_with("lib-cell-")).cloned()).unwrap_or_else(|| panic!("그림 칸 없음: {:?}", widgets::HOVER_SEEN.with(|m| m.borrow().keys().cloned().collect::<Vec<_>>()))) } else { row.to_string() };
        let row = row.as_str();
        rclick(&mut h, row, lib);
        menu_pick(&mut h, "menu-cue", lib);
        assert_eq!(count(&h, ty), 1, "{tab} 탭: 오른쪽 메뉴 '큐시트에 넣기 (더블클릭)'");
        h.app.core.lock().pick.forget_last();
    }
}

// ---------------- 7) 큐시트 관리 창 (cuemanager.js): 한 번 = 고르기, 두 번 = 그 큐시트 열기 (휴지통 보기 제외) ----------------

thread_local! {
    static CM_OPEN: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
}
fn cm(app: &mut App, ctx: &egui::Context, _: &mut egui::Ui, _: &Snap, _: &mut Vec<Act>) {
    let mut o = CM_OPEN.with(|c| c.get());
    app.cuemgr_window(ctx, &mut o);
    CM_OPEN.with(|c| c.set(o));
    // 복사 알림 등은 실제 화면처럼 꺼내 준다
    if let Some(m) = widgets::take_copied(ctx) {
        app.core.lock().notify(m, "ok");
    }
}
fn cm_is_open() -> bool {
    CM_OPEN.with(|c| c.get())
}
/// 큐시트 셋: '주일 예배'(열림) · '수요 예배' · '새벽 예배'
fn cm_h() -> H {
    CM_OPEN.with(|c| c.set(true));
    let mut h = H::new(titles());
    {
        let mut c = h.app.core.lock();
        let first = c.cue.as_ref().unwrap().id.clone();
        c.update_cue_meta_of(&first, &json!({ "name": "주일 예배", "serviceDate": "2026-11-15" })).unwrap();
        for (n, d) in [("수요 예배", "2026-11-11"), ("새벽 예배", "2026-11-10")] {
            c.new_cue(n).unwrap();
            let id = c.cue.as_ref().unwrap().id.clone();
            c.add_item(json!({ "id": format!("{n}-1"), "type": "title", "title": format!("{n} 순서") }), None);
            c.update_cue_meta_of(&id, &json!({ "serviceDate": d })).unwrap();
        }
        c.open_cue(&first);
        c.toast = None;
    }
    h.frame(vec![], cm);
    h.frame(vec![], cm);
    h
}
fn cur_name(h: &H) -> String {
    h.app.core.lock().cue.as_ref().map(|c| c.name.clone()).unwrap_or_default()
}
fn cue_names(h: &H, trash: bool) -> Vec<String> {
    let mut v: Vec<String> = h.app.core.lock().cue_list(trash).iter().map(|c| c["name"].as_str().unwrap_or("").to_string()).collect();
    v.sort();
    v
}

#[test]
fn dbl_cue_manager_row_opens_that_cue_and_single_click_only_selects() {
    let mut h = cm_h();
    assert!(seen("cm-row-주일 예배").is_some() && seen("cm-row-수요 예배").is_some() && seen("cm-row-새벽 예배").is_some(), "목록에 큐시트가 모두 보인다");
    single_panel(&mut h, "cm-row-수요 예배", cm);
    assert_eq!(cur_name(&h), "주일 예배", "한 번 = 고르기만");
    assert!(cm_is_open());
    dbl_panel(&mut h, "cm-row-새벽 예배", cm);
    assert_eq!(cur_name(&h), "새벽 예배", "두 번 = 그 큐시트 열기");
    assert!(!cm_is_open(), "열면 관리 창이 닫힌다");
}

#[test]
fn cue_manager_open_button_new_default_and_empty() {
    let mut h = cm_h();
    single_panel(&mut h, "cm-row-수요 예배", cm);
    h.click("cm-open", cm);
    assert_eq!(cur_name(&h), "수요 예배");
    assert!(!cm_is_open());
    CM_OPEN.with(|c| c.set(true));
    h.click("cm-new", cm);
    let c = h.app.core.lock().cue.clone().unwrap();
    assert_eq!(c.items.len(), 8, "＋ 새 큐시트 = 기본 예배 순서 (원본 sampleItems)");
    assert!(!cm_is_open());
    CM_OPEN.with(|c| c.set(true));
    h.click("cm-new-empty", cm);
    assert!(h.app.core.lock().cue.as_ref().unwrap().items.is_empty(), "빈 큐시트");
}

#[test]
fn cue_manager_search_views_pin_template_and_duplicate() {
    let mut h = cm_h();
    // 찾기: 순서 제목으로도
    h.click("cm-q", cm);
    h.type_text("새벽", cm);
    h.frame(vec![], cm);
    assert!(seen("cm-row-새벽 예배").is_some() && seen("cm-row-주일 예배").is_none(), "찾기는 이름으로 거른다");
    for _ in 0..2 {
        h.key(egui::Key::Backspace, egui::Modifiers::NONE, cm);
    }
    h.frame(vec![], cm);
    assert!(seen("cm-row-주일 예배").is_some());
    // 고정 · 서식
    single_panel(&mut h, "cm-row-수요 예배", cm);
    h.click("cm-pin", cm);
    h.click("cm-tpl", cm);
    let c = h.app.core.lock().cue_list(false).into_iter().find(|c| c["name"] == "수요 예배").unwrap();
    assert_eq!((c["pinned"].clone(), c["template"].clone()), (json!(true), json!(true)));
    h.click("cm-view-pinned", cm);
    h.frame(vec![], cm);
    assert!(seen("cm-row-수요 예배").is_some() && seen("cm-row-주일 예배").is_none(), "고정 보기");
    h.click("cm-view-all", cm);
    // 다음 주 순서로 복제: 날짜 +7, 열린 큐시트는 그대로
    single_panel(&mut h, "cm-row-주일 예배", cm);
    h.click("cm-dup-next", cm);
    let made = h.app.core.lock().cue_list(false).into_iter().find(|c| c["serviceDate"] == "2026-11-22").expect("다음 주 복사본");
    assert_eq!(made["name"], "주일 예배 (2026-11-22)");
    assert_eq!(made["count"], 3);
    assert_eq!(cur_name(&h), "주일 예배");
    assert!(cm_is_open());
    // 순서지 복사
    h.click("cm-text", cm);
    assert!(h.app.core.lock().toast.as_ref().is_some_and(|t| t.0.contains("순서지")), "순서지 복사 알림");
}

#[test]
fn cue_manager_service_frame_makes_and_opens_a_cue() {
    let mut h = cm_h();
    h.click("cm-frame", cm);
    h.frame(vec![], cm);
    h.click("cm-frame-friday", cm);
    h.frame(vec![], cm);
    assert!(cur_name(&h).ends_with("금요 기도회"), "만든 큐시트가 열린다: {}", cur_name(&h));
    assert!(!cm_is_open(), "만들면 창을 닫는다");
    assert_eq!(items(&h).iter().filter(|i| i["type"] == "song").count(), 3);
    // 서식 목록에 넣기: 열린 큐시트는 그대로
    CM_OPEN.with(|c| c.set(true));
    h.click("cm-frame", cm);
    h.frame(vec![], cm);
    h.click("cm-frame-all", cm);
    h.frame(vec![], cm);
    let tpls: Vec<Value> = h.app.core.lock().cue_list(false).into_iter().filter(|c| c["template"] == true).collect();
    assert_eq!(tpls.len(), 5, "다섯 틀이 서식으로");
    assert!(cur_name(&h).ends_with("금요 기도회"));
}

#[test]
fn cue_manager_rename_saves_when_leaving_the_field() {
    let mut h = cm_h();
    single_panel(&mut h, "cm-row-수요 예배", cm);
    h.click("cm-name", cm);
    h.type_text(" 2부", cm);
    assert!(cue_names(&h, false).contains(&"수요 예배".to_string()), "치는 중에는 아직 저장하지 않는다");
    h.click("cm-memo", cm); // 다른 칸으로
    h.frame(vec![], cm);
    assert!(cue_names(&h, false).contains(&"수요 예배 2부".to_string()), "칸을 떠나면 저장: {:?}", cue_names(&h, false));
}

#[test]
fn cue_manager_trash_restore_and_no_open_from_trash_view() {
    let mut h = cm_h();
    // 열린 큐시트를 휴지통으로 (한 번 더 묻는다)
    single_panel(&mut h, "cm-row-주일 예배", cm);
    h.click("confirm-first", cm);
    assert_eq!(cue_names(&h, true).len(), 0, "첫 클릭은 묻기만");
    h.click("confirm-yes", cm);
    assert_eq!(cue_names(&h, true), ["주일 예배"]);
    assert_ne!(cur_name(&h), "주일 예배", "열려 있던 것을 지우면 다른 큐시트가 열린다");
    assert!(!cur_name(&h).is_empty());
    // 휴지통 보기: 두 번 눌러도 열지 않는다
    h.click("cm-view-trash", cm);
    let before = cur_name(&h);
    dbl_panel(&mut h, "cm-row-주일 예배", cm);
    assert_eq!(cur_name(&h), before, "휴지통의 큐시트는 두 번 눌러도 열리지 않는다");
    assert!(cm_is_open());
    // 되살리기 → 전체 보기로
    h.click("cm-restore", cm);
    assert!(cue_names(&h, true).is_empty());
    assert!(cue_names(&h, false).contains(&"주일 예배".to_string()));
    h.frame(vec![], cm);
    assert!(seen("cm-row-주일 예배").is_some() && seen("cm-row-수요 예배").is_some(), "되살리면 전체 보기로 돌아온다");
}

#[test]
fn cue_manager_check_all_then_bulk_trash_and_bulk_restore() {
    let mut h = cm_h();
    h.click("cm-check-all", cm);
    h.click("cm-bulk-first", cm);
    h.click("cm-bulk-yes", cm);
    assert_eq!(cue_names(&h, true).len(), 3, "고른 것 모두 휴지통으로");
    assert!(h.app.core.lock().cue.is_none(), "남은 큐시트가 없으면 열린 것도 없다");
    h.click("cm-view-trash", cm);
    h.click("cm-check-all", cm);
    h.click("cm-restore-sel", cm);
    assert_eq!(cue_names(&h, false).len(), 3);
}

// ---------------- 5) PPT 나누기 창 썸네일 (cue.js:545-557): 누르면 시작 · Shift 끝 · 두 번 = 그 슬라이드에서 새 순서 (이미 시작이면 합침) ----------------

fn sp(app: &mut App, ctx: &egui::Context, _: &mut egui::Ui, _: &Snap, _: &mut Vec<Act>) {
    app.pptsplit_window(ctx);
}
/// 슬라이드 6장짜리 PPT 순서 하나('주일 PPT')와 나누기 창
fn sp_h() -> H {
    let mut h = H::new(vec![json!({ "id": "p", "type": "hymn", "title": "주일 PPT", "file": "주일.pptx", "edits": { "0": { "items": [] } }, "skip": [1] }), json!({ "id": "z", "type": "title", "title": "끝" })]);
    let it = items(&h)[0].clone();
    let texts: Vec<String> = ["예배로 부름", "", "찬송 10장\n전능왕 오셔서", "가사", "설교", "축도"].iter().map(|s| s.to_string()).collect();
    let all: Vec<Value> = (0..6).map(|k| json!({ "kind": "image", "image": "", "ref": "주일 PPT", "label": (k + 1).to_string(), "parts": [] })).collect();
    crate::pptsplit_ui::open_with(&h.ctx, &it, all, texts);
    h.frame(vec![], sp);
    h.frame(vec![], sp);
    h
}
fn sp_parts(h: &H) -> Vec<(usize, usize, String)> {
    crate::pptsplit_ui::parts_now(&h.ctx)
}
fn sp_ranges(h: &H) -> Vec<(usize, usize)> {
    sp_parts(h).iter().map(|p| (p.0, p.1)).collect()
}
fn rest(h: &mut H) {
    for _ in 0..12 {
        h.frame(vec![], sp);
    }
}

#[test]
fn dbl_ppt_split_thumbnail_toggles_split_there_and_again_merges() {
    let mut h = sp_h();
    for n in 1..=6 {
        assert!(seen(&format!("sp-th-{n}")).is_some(), "썸네일 {n} 이 보인다");
    }
    dbl_panel(&mut h, "sp-th-3", sp);
    assert_eq!(sp_parts(&h), [(1, 2, "예배로 부름".to_string()), (3, 6, "찬송 10장".to_string())], "두 번 = 그 슬라이드에서 새 순서 (이름은 슬라이드 첫 줄)");
    assert_eq!(crate::pptsplit_ui::range_now(&h.ctx), "3", "첫 클릭은 시작 고르기");
    rest(&mut h);
    dbl_panel(&mut h, "sp-th-5", sp);
    assert_eq!(sp_ranges(&h), [(1, 2), (3, 4), (5, 6)]);
    rest(&mut h);
    dbl_panel(&mut h, "sp-th-3", sp);
    assert_eq!(sp_ranges(&h), [(1, 4), (5, 6)], "이미 시작인 슬라이드를 두 번 = 앞 순서와 합침");
    assert_eq!(items(&h).len(), 2, "'나누기' 를 누르기 전에는 큐시트가 그대로");
}

#[test]
fn ppt_split_click_start_shift_click_end_then_add_range_with_name() {
    let mut h = sp_h();
    single_panel(&mut h, "sp-th-2", sp);
    assert_eq!(crate::pptsplit_ui::range_now(&h.ctx), "2");
    assert!(sp_parts(&h).is_empty(), "한 번 클릭은 나누지 않는다");
    // Shift 를 누른 채 4번
    let p = seen("sp-th-4").unwrap().center();
    let shift = egui::Modifiers::SHIFT;
    h.frame_m(vec![Event::PointerMoved(p)], shift, sp);
    for pressed in [true, false] {
        h.frame_m(vec![Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: shift }], shift, sp);
    }
    rest(&mut h);
    assert_eq!(crate::pptsplit_ui::range_now(&h.ctx), "2-4");
    h.click("sp-name", sp);
    h.type_text("찬송", sp);
    h.click("sp-add", sp);
    assert_eq!(sp_parts(&h), [(2, 4, "찬송".to_string())]);
    assert_eq!(crate::pptsplit_ui::range_now(&h.ctx), "5", "더한 뒤에는 다음 슬라이드가 시작");
    // 이름 칸에서 Enter 로도 더한다 (이름이 비면 '제목 범위')
    h.click("sp-name", sp);
    h.key(egui::Key::Enter, egui::Modifiers::NONE, sp);
    assert_eq!(sp_parts(&h), [(2, 4, "찬송".to_string()), (5, 5, "주일 PPT 5".to_string())]);
}

#[test]
fn ppt_split_rule_button_merge_remove_clear_and_go_replaces_the_item() {
    let mut h = sp_h();
    h.click("sp-rule", sp);
    assert!(sp_parts(&h).len() >= 2, "규칙으로: {:?}", sp_parts(&h));
    h.click("sp-clear", sp);
    assert!(sp_parts(&h).is_empty());
    rest(&mut h);
    dbl_panel(&mut h, "sp-th-3", sp);
    rest(&mut h);
    dbl_panel(&mut h, "sp-th-5", sp);
    rest(&mut h);
    h.click("sp-merge-2", sp); // 3번째(5-6)를 앞(3-4)과 합침
    assert_eq!(sp_ranges(&h), [(1, 2), (3, 6)]);
    rest(&mut h);
    dbl_panel(&mut h, "sp-th-6", sp);
    assert_eq!(sp_ranges(&h), [(1, 2), (3, 5), (6, 6)]);
    rest(&mut h); // 줄이 늘어 창 크기가 자리 잡을 때까지
    h.click("sp-rm-1", sp); // 가운데(3-5)는 넣지 않는다
    assert_eq!(sp_ranges(&h), [(1, 2), (6, 6)]);
    rest(&mut h);
    h.click("sp-go", sp);
    let v = items(&h);
    assert_eq!(v.len(), 3, "순서 하나가 둘로 바뀐다 (+ 원래 있던 '끝')");
    assert_eq!((v[0]["title"].as_str().unwrap(), v[0]["range"].as_str().unwrap()), ("예배로 부름", "1-2"));
    assert_eq!((v[1]["title"].as_str().unwrap(), v[1]["range"].as_str().unwrap()), ("축도", "6"));
    assert_eq!(v[2]["title"], "끝");
    for x in &v[..2] {
        assert_eq!(x["file"], "주일.pptx");
        assert!(x.get("edits").is_none() && x.get("skip").is_none(), "슬라이드 번호에 묶인 편집·건너뛰기는 옮기지 않는다");
    }
    assert_ne!(v[0]["id"], v[1]["id"]);
    assert!(!crate::pptsplit_ui::is_open(&h.ctx), "나누면 창이 닫힌다");
}

// ---------------- 11) 슬라이드 편집기 상자 두 번 = 글 바로 고치기 (slideedit.js:1012 · inlineEdit, 잠긴 상자 제외) ----------------

fn sedit(app: &mut App, ctx: &egui::Context, _: &mut egui::Ui, _: &Snap, acts: &mut Vec<Act>) {
    let typing = ctx.egui_wants_keyboard_input();
    app.shortcuts(ctx, acts, typing); // 전역 Ctrl+Enter(바로 송출)가 글 저장을 가로채지 않는지 함께 본다
    let (core, sel, wd, dd) = (app.core.clone(), app.sel, app.web_dir.clone(), app.data_dir.clone());
    app.tc.slideedit.show(ctx, &core, sel, &wd, &dd, acts);
}
fn se_h(items: Vec<Value>) -> H {
    let mut h = H::new(items);
    h.app.sel = Some(0);
    h.app.tc.slideedit.open_at("0");
    for _ in 0..4 {
        h.frame(vec![], sedit);
    }
    h
}
fn se_target(h: &H) -> Option<String> {
    h.app.tc.slideedit.inline_target()
}
fn boxed(extra: Value) -> Value {
    let mut b = json!({ "id": "t1", "type": "text", "x": 20, "y": 70, "w": 60, "h": 20, "text": "글상자", "size": 60, "color": "#ffffff", "align": "center", "valign": "middle" });
    for (k, v) in extra.as_object().unwrap() {
        b[k] = v.clone();
    }
    json!({ "id": "a", "type": "text", "title": "공지", "body": "첫째 장\n---\n둘째 장", "edits": { "0": { "items": [b] } } })
}

#[test]
fn dbl_editor_main_text_edits_in_place_and_ctrl_enter_saves_without_sending() {
    let mut h = se_h(vec![json!({ "id": "a", "type": "text", "title": "공지", "body": "첫째 장\n---\n둘째 장" })]);
    assert!(se_target(&h).is_none());
    single_panel(&mut h, "se-main", sedit);
    assert!(se_target(&h).is_none(), "한 번 클릭으로는 열리지 않는다");
    dbl_panel(&mut h, "se-main", sedit);
    assert_eq!(se_target(&h).as_deref(), Some("main"), "본문을 두 번 누르면 그 자리에서 글 고치기");
    h.frame(vec![], sedit);
    h.frame(vec![], sedit);
    assert!(seen("se-inline").is_some(), "글칸이 본문 자리에 나온다");
    h.type_text("새 글", sedit); // 열자마자 모두 골라져 있어 바로 치면 바뀐다
    assert_eq!(items(&h)[0]["body"], "첫째 장\n---\n둘째 장", "치는 중에는 저장하지 않는다");
    h.key(egui::Key::Enter, egui::Modifiers::COMMAND, sedit);
    h.frame(vec![], sedit);
    assert_eq!(items(&h)[0]["body"], "새 글\n---\n둘째 장", "Ctrl+Enter = 저장 (그 장만 바뀐다)");
    assert!(se_target(&h).is_none());
    assert!(h.live_title().is_none(), "글 저장의 Ctrl+Enter 가 송출로 새지 않는다");
}

#[test]
fn dbl_editor_text_box_edits_in_place_escape_cancels_click_away_saves() {
    let mut h = se_h(vec![boxed(json!({}))]);
    let text = |h: &H| items(h)[0]["edits"]["0"]["items"][0]["text"].as_str().unwrap_or("").to_string();
    dbl_panel(&mut h, "se-el-0", sedit);
    assert_eq!(se_target(&h).as_deref(), Some("t1"), "글상자를 두 번 누르면 그 글상자 글 고치기");
    h.frame(vec![], sedit);
    h.frame(vec![], sedit);
    h.type_text("버릴 글", sedit);
    h.key(egui::Key::Escape, egui::Modifiers::NONE, sedit);
    h.frame(vec![], sedit);
    assert!(se_target(&h).is_none());
    assert_eq!(text(&h), "글상자", "Esc = 그만두기");
    for _ in 0..12 {
        h.frame(vec![], sedit);
    }
    dbl_panel(&mut h, "se-el-0", sedit);
    h.frame(vec![], sedit);
    h.frame(vec![], sedit);
    h.type_text("환영합니다", sedit);
    // 다른 곳(본문 위쪽)을 누르면 저장
    let away = seen("se-main").map(|r| r.center_top() + Vec2::new(0.0, 12.0));
    let away = away.unwrap_or_else(|| seen("se-inline").unwrap().center_top() - Vec2::new(0.0, 120.0));
    h.click_at(away, sedit);
    h.frame(vec![], sedit);
    assert!(se_target(&h).is_none());
    assert_eq!(text(&h), "환영합니다", "다른 곳을 누르면 저장");
    assert_eq!(items(&h)[0]["body"], "첫째 장\n---\n둘째 장", "본문은 그대로");
}

#[test]
fn dbl_editor_locked_box_image_slide_and_bible_text_do_not_open_inline_edit() {
    // 잠긴 상자
    let mut h = se_h(vec![boxed(json!({ "locked": true }))]);
    dbl_panel(&mut h, "se-el-0", sedit);
    assert!(se_target(&h).is_none(), "잠긴 상자는 두 번 눌러도 열리지 않는다");
    // 성경 본문은 여기서 고치지 않는다 (알림만)
    let mut h = se_h(vec![json!({ "id": "b", "type": "bible", "title": "요한복음 3:16", "ref": "요3:16", "reference": "요3:16", "version": "tst" })]);
    dbl_panel(&mut h, "se-main", sedit);
    assert!(se_target(&h).is_none(), "성경 본문은 글 고치기가 열리지 않는다");
    assert!(h.app.core.lock().toast.as_ref().is_some_and(|t| t.0.contains("성경")), "왜 안 되는지 알린다");
}
// ---------------- 오른쪽 클릭 메뉴: 미리보기·LIVE 화면 · 큐시트 빈 자리 · 순서 메모 (ctxmenu.js previewMenu · liveMenu · cueEmptyMenu · cueItemMenu) ----------------

fn rclick_full(h: &mut H, name: &str) {
    let p = at(h, name);
    rclick_full_at(h, p);
}
fn rclick_full_at(h: &mut H, p: Pos2) {
    full(h, vec![Event::PointerMoved(p)]);
    for pressed in [true, false] {
        full(h, vec![Event::PointerButton { pos: p, button: PointerButton::Secondary, pressed, modifiers: Default::default() }]);
    }
    idle(h, 2);
}
fn pick_full(h: &mut H, item: &str) {
    let p = seen(item).unwrap_or_else(|| panic!("메뉴 항목 없음: {item} (있는 것: {:?})", widgets::HOVER_SEEN.with(|m| m.borrow().keys().filter(|k| k.starts_with("menu")).cloned().collect::<Vec<_>>()))).center();
    click_at(h, p);
    idle(h, 2);
}

#[test]
fn preview_pane_menu_sends_and_live_pane_menu_hides_text() {
    let mut h = H::new(titles());
    h.app.core.lock().preview_item(0);
    idle(&mut h, 3);
    assert!(live(&h).is_none());
    rclick_full(&mut h, "pane-preview");
    pick_full(&mut h, "menu-next");
    assert_eq!(h.app.core.lock().pres.preview.as_ref().map(|d| d.index), Some(1), "미리보기 메뉴 '다음 슬라이드'");
    rclick_full(&mut h, "pane-preview");
    pick_full(&mut h, "menu-send");
    assert_eq!(live(&h), Some((0, 1)), "미리보기 메뉴 '송출' = F9");
    rclick_full(&mut h, "pane-live");
    pick_full(&mut h, "menu-clear");
    assert_eq!(h.app.core.lock().pres.live.as_ref().map(|l| l.visible), Some(false), "LIVE 메뉴 '글자 숨김·복원' = F10");
    rclick_full(&mut h, "pane-live");
    pick_full(&mut h, "menu-next");
    assert_eq!(live(&h), Some((0, 2)), "LIVE 메뉴 '다음 슬라이드' = F6");
}

#[test]
fn cue_list_empty_area_menu_adds_item_and_opens_cue_manager() {
    let mut h = H::new(titles());
    idle(&mut h, 3);
    let area = seen("cue-empty").expect("큐시트 목록");
    let last = seen("cue-row-2").expect("마지막 순서 줄");
    let p = Pos2::new(area.center().x, (last.bottom() + 30.0).min(area.bottom() - 4.0));
    assert!(p.y > last.bottom(), "시험 전제: 목록 아래에 빈 자리가 있다");
    rclick_full_at(&mut h, p);
    pick_full(&mut h, "menu-add-title");
    assert_eq!(items(&h).len(), 4, "빈 자리 메뉴 '제목' = 순서 하나 넣기");
    assert_eq!(items(&h).iter().filter(|x| x["type"] == "title").count(), 1);
    idle(&mut h, 12);
    let area = seen("cue-empty").unwrap();
    let last = seen("cue-row-3").unwrap();
    rclick_full_at(&mut h, Pos2::new(area.center().x, (last.bottom() + 30.0).min(area.bottom() - 4.0)));
    pick_full(&mut h, "menu-cues");
    assert!(h.app.tb.cues, "빈 자리 메뉴 '큐시트 관리' = 관리 창 열기");
}

#[test]
fn cue_row_menu_note_is_saved_on_the_item() {
    let mut h = H::new(titles());
    idle(&mut h, 3);
    rclick_full(&mut h, "cue-row-1");
    let p = seen("menu-note").expect("메모 칸").center();
    click_at(&mut h, p);
    for ch in "일어서서".chars() {
        full(&mut h, vec![Event::Text(ch.to_string())]);
    }
    pick_full(&mut h, "menu-note-ok");
    assert_eq!(items(&h)[1]["note"], "일어서서");
    assert_eq!(items(&h)[0]["note"].as_str().unwrap_or(""), "", "다른 순서는 그대로");
}
// ---------------- 송출 중 이름표로 건너뛰기 (control.js jumpKey): 1~9 = 그 절 · C = 후렴 ----------------

fn keys_only(app: &mut App, ctx: &egui::Context, _: &mut egui::Ui, _: &Snap, acts: &mut Vec<Act>) {
    let typing = ctx.egui_wants_keyboard_input();
    app.shortcuts(ctx, acts, typing);
}

#[test]
fn live_song_number_and_c_keys_jump_to_that_verse_and_chorus() {
    let mut h = H::new(vec![json!({ "id": "s", "type": "song", "title": "은혜", "lyrics": "[1절]\n하나\n\n[후렴]\n후렴 가사\n\n[2절]\n둘\n\n[후렴]\n후렴 가사" })]);
    h.app.core.lock().send_item(0);
    h.frame(vec![], keys_only);
    assert_eq!(live(&h), Some((0, 0)));
    h.type_text("2", keys_only);
    assert_eq!(live(&h), Some((0, 2)), "2 = 2절 첫 슬라이드");
    h.type_text("c", keys_only);
    assert_eq!(live(&h), Some((0, 3)), "C = 지금 다음의 후렴");
    h.type_text("ㅊ", keys_only);
    assert_eq!(live(&h), Some((0, 1)), "한글 자판 ㅊ 도 후렴 · 끝이면 처음부터");
    h.type_text("1", keys_only);
    assert_eq!(live(&h), Some((0, 0)));
    h.type_text("7", keys_only);
    assert_eq!(live(&h), Some((0, 0)), "없는 절은 그대로");
    assert!(h.app.core.lock().toast.as_ref().is_some_and(|t| t.0.contains("7")), "없다고 알린다");
}