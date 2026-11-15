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
