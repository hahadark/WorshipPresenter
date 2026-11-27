//! 사용자 요청 세 가지 + 중복 순서 합성 입력 시험
//!  1) 예배당 화면 창을 눌러 고르고 Esc → 예배당 화면 꺼짐 (조작 화면 상태도)
//!  2) 미리보기·LIVE 아래 슬라이드(절) 번호 단추 줄 접기·펼치기
//!  3) '바로 송출' 일 때 모든 클릭 경로에서 바로 송출 · 안내 문구
//!  4) 같은 찬송가·찬양을 다시 고르면 새 순서를 만들지 않음 · 짧은 시간 반복 추가 막기
use super::{Act, App, Snap};
use eframe::egui::{self, Event, PointerButton, Pos2, RawInput, Rect, Vec2};
use serde_json::{json, Value};
use wp_core::core::Core;

struct H {
    ctx: egui::Context,
    app: App,
    dir: std::path::PathBuf,
    time: f64,
}

type Draw = fn(&mut App, &egui::Context, &mut egui::Ui, &Snap, &mut Vec<Act>);

impl H {
    fn new(items: Vec<Value>) -> Self {
        let dir = std::env::temp_dir().join(format!("wp-fix3-{}-{}", std::process::id(), wp_core::util::short_id("t")));
        std::fs::create_dir_all(&dir).unwrap();
        // 빈 DB 를 먼저 만들어 사용자 원본 자동 복사를 막는다
        drop(wp_core::db::BibleStore::open(&dir.join("bible.db")).unwrap());
        let mut core = Core::open(dir.clone(), dir.clone(), None).unwrap();
        core.new_cue("합성입력 전용").unwrap();
        for it in items {
            core.add_item(it, None);
        }
        core.b.undo = Default::default();
        core.toast = None;
        let core = std::sync::Arc::new(wp_core::lockcheck::Mutex::new(core));
        Self { ctx: egui::Context::default(), app: App::for_test(core, dir.clone()), dir, time: 0.0 }
    }
    fn raw_frame(&mut self, events: Vec<Event>, f: Draw) -> Vec<Act> {
        self.time += 0.05;
        crate::widgets::HOVER_SEEN.with(|m| m.borrow_mut().clear());
        let snap = self.app.snap();
        let mut acts = vec![];
        let app = &mut self.app;
        let mut out = self.ctx.run_pass(
            RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1600.0, 960.0))), time: Some(self.time), events, ..Default::default() },
            |ctx| {
                ctx.root_ui(|root| {
                    egui::CentralPanel::default_margins().show(root, |ui| f(app, ctx, ui, &snap, &mut acts));
                });
            },
        );
        out.textures_delta.clear();
        acts
    }
    fn frame(&mut self, events: Vec<Event>, f: Draw) {
        let acts = self.raw_frame(events, f);
        self.app.apply(acts);
    }
    fn press(&mut self, p: Pos2, f: Draw) {
        self.frame(vec![Event::PointerMoved(p)], f);
        for pressed in [true, false] {
            self.frame(vec![Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: Default::default() }], f);
        }
        self.frame(vec![], f);
    }
    /// 두 번 그려 자리를 잡은 뒤 이름 붙은 자리를 실제 포인터로 누른다
    fn click(&mut self, name: &str, f: Draw) {
        self.frame(vec![], f);
        self.frame(vec![], f);
        let p = seen(name).unwrap_or_else(|| panic!("그린 자리 없음: {name}")).center();
        self.press(p, f);
    }
    fn click_acts(&mut self, name: &str, f: Draw) -> Vec<Act> {
        self.raw_frame(vec![], f);
        self.raw_frame(vec![], f);
        let p = seen(name).unwrap_or_else(|| panic!("그린 자리 없음: {name}")).center();
        let mut all = self.raw_frame(vec![Event::PointerMoved(p)], f);
        for pressed in [true, false] {
            all.extend(self.raw_frame(vec![Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: Default::default() }], f));
        }
        all
    }
    fn toast(&self) -> (String, Option<&'static str>) {
        let c = self.app.core.lock();
        (c.toast.as_ref().map(|t| t.0.clone()).unwrap_or_default(), c.toast_act)
    }
    fn items(&self) -> Vec<Value> {
        self.app.core.lock().cue.as_ref().unwrap().items.clone()
    }
    fn direct(&mut self) {
        self.app.core.lock().settings.update(|d| {
            d["previewMode"] = json!(false);
            d["output"]["autoOnSend"] = json!(false); // 시험에서는 예배당 창을 실제로 띄우지 않는다
        });
    }
    /// LIVE 로 나간 것: (cueIndex, 슬라이드 번호)
    fn live(&self) -> Option<(i64, usize)> {
        let c = self.app.core.lock();
        c.pres.live.as_ref().map(|l| (l.deck.passage["cueIndex"].as_i64().unwrap_or(-1), l.deck.index))
    }
}
impl Drop for H {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
fn seen(name: &str) -> Option<Rect> {
    crate::widgets::HOVER_SEEN.with(|m| m.borrow().get(name).copied())
}
fn toast(app: &mut App, ctx: &egui::Context, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
    // (나) 전문가 테마(기본)는 알림이 아래 상태 줄 안에 (toast_inline)
    if crate::theme::pal().bevel {
        ui.horizontal(|ui| {
            app.toast_inline(ui, s, acts);
        });
        return;
    }
    if let Some(a) = app.toast(ctx, s) {
        acts.push(a);
    }
}
fn bottom(app: &mut App, _: &egui::Context, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
    app.bottom_bar(ui, s, acts);
}
fn exit_frame(app: &mut App, ctx: &egui::Context, _: &mut egui::Ui, _: &Snap, _: &mut Vec<Act>) {
    crate::trackd_ui::output_exit(app, ctx);
}
fn titles() -> Vec<Value> {
    vec![
        json!({ "id": "a", "type": "text", "title": "공지", "body": "첫째 장\n---\n둘째 장\n---\n셋째 장" }),
        json!({ "id": "b", "type": "text", "title": "광고", "body": "하나\n---\n둘" }),
    ]
}

// ---------------- 1) 예배당 화면 Esc ----------------

#[test]
fn output_window_args_include_esc_close() {
    let o = crate::outwin::OutWin::for_test();
    let a = o.open_args(37528, 1, false);
    assert!(a.iter().any(|x| x == "--esc-close"), "예배당 창은 Esc 로 끌 수 있게 --esc-close 로 띄운다: {a:?}");
    assert!(!a.iter().any(|x| x == "--windowed"));
}

#[test]
fn esc_exit_turns_output_off_and_offers_turn_on() {
    let mut h = H::new(titles());
    h.app.out.fake_exit(Some(crate::outwin::EXIT_ESC));
    h.frame(vec![], exit_frame);
    let (msg, act) = h.toast();
    assert!(msg.contains("Esc"), "{msg}");
    assert_eq!(act, Some("output-on"), "알림의 [예배당 화면 켜기] 로 다시 켤 수 있다");
    assert!(!h.app.out.running());
    // [예배당 화면 켜기] → 예배당 화면 켜기 동작
    let acts = h.click_acts("toast-act", toast);
    assert!(acts.iter().any(|a| matches!(a, Act::ToastAct("output-on"))));
    // 한 번만 알린다
    h.app.core.lock().toast = None;
    h.frame(vec![], exit_frame);
    assert!(h.toast().0.is_empty());
    // 아래 줄 단추·상태: 꺼짐
    let s = h.app.snap();
    let items = h.app.status_items(&s);
    assert!(items.iter().any(|x| x.2 == "output" && x.1.contains("꺼짐")), "{:?}", items.iter().map(|x| x.1.clone()).collect::<Vec<_>>());
    let _ = bottom;
}

#[test]
fn closing_from_control_is_not_reported_as_exit() {
    let mut o = crate::outwin::OutWin::for_test();
    o.fake_exit(Some(0));
    o.close();
    assert!(o.take_exit().is_none(), "조작 화면에서 끈 것은 '창이 닫혔습니다' 알림을 내지 않는다");
}

// ---------------- 2) 슬라이드(절) 번호 단추 줄 접기 ----------------

fn pv_pane(app: &mut App, _: &egui::Context, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
    let d = s.preview.clone();
    app.deck_view(ui, "미리보기", d.as_ref(), false, None, s, acts);
}
fn lv_pane(app: &mut App, _: &egui::Context, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
    let d = s.live.as_ref().map(|l| l.deck.clone());
    app.deck_view(ui, "LIVE", d.as_ref(), true, s.live.as_ref(), s, acts);
}
fn fold_of(h: &H, k: &str) -> bool {
    h.app.core.lock().settings.get()["thumbsFold"][k].as_bool().unwrap_or(false)
}

#[test]
fn preview_chips_fold_and_unfold_with_button() {
    let mut h = H::new(titles());
    h.app.apply(vec![Act::PreviewItem(0)]);
    h.frame(vec![], pv_pane);
    h.frame(vec![], pv_pane);
    assert!(seen("pv-chip-2").is_some(), "처음에는 번호 단추 줄이 보인다");
    h.click("pv-thumbs-fold", pv_pane);
    assert!(fold_of(&h, "pv"), "접으면 설정에 기억");
    assert!(!fold_of(&h, "lv"), "미리보기·LIVE 는 따로");
    h.frame(vec![], pv_pane);
    assert!(seen("pv-chip-0").is_none(), "접히면 번호 단추가 안 보인다");
    assert!(seen("pv-thumbs-fold").is_some(), "접혀도 단추는 남는다");
    h.click("pv-thumbs-fold", pv_pane);
    assert!(!fold_of(&h, "pv"));
    h.frame(vec![], pv_pane);
    assert!(seen("pv-chip-1").is_some(), "다시 펼침");
}

#[test]
fn live_chips_fold_separately() {
    let mut h = H::new(titles());
    h.app.apply(vec![Act::PreviewItem(0), Act::SendPreview]);
    h.frame(vec![], lv_pane);
    h.frame(vec![], lv_pane);
    assert!(seen("lv-chip-1").is_some());
    h.click("lv-thumbs-fold", lv_pane);
    assert!(fold_of(&h, "lv") && !fold_of(&h, "pv"));
    h.frame(vec![], lv_pane);
    assert!(seen("lv-chip-1").is_none());
    h.click("lv-thumbs-fold", lv_pane);
    h.frame(vec![], lv_pane);
    assert!(seen("lv-chip-1").is_some(), "접혀도 남은 단추로 다시 펼친다");
}

// ---------------- 3) '바로 송출' 일 때 모든 클릭 경로에서 송출 ----------------

fn cue(app: &mut App, _: &egui::Context, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
    app.cue_panel(ui, s, acts);
}

#[test]
fn direct_mode_cue_row_click_goes_live() {
    let mut h = H::new(titles());
    h.direct();
    h.click("cue-row-1", cue);
    assert_eq!(h.live(), Some((1, 0)), "바로 송출: 큐시트 순서를 누르면 바로 나간다");
}

#[test]
fn preview_mode_cue_row_click_does_not_go_live() {
    let mut h = H::new(titles());
    h.click("cue-row-1", cue);
    assert_eq!(h.live(), None, "미리보기 방식: 누르면 미리보기만");
    assert!(h.app.core.lock().pres.preview.is_some());
}

#[test]
fn direct_mode_slide_chip_click_goes_live() {
    // 원인: 슬라이드(본문 목록·격자·번호 단추)를 누르면 Act::PreviewIndex → Core::set_preview_index 가 미리보기만 옮기고
    // 바로 송출을 하지 않았다 (Electron setPreviewIndex 는 directSend). 지금은 pick_preview_index.
    let mut h = H::new(titles());
    h.direct();
    h.app.apply(vec![Act::PreviewItem(0)]);
    assert_eq!(h.live(), Some((0, 0)));
    h.click("pv-chip-2", pv_pane);
    assert_eq!(h.live(), Some((0, 2)), "바로 송출: 슬라이드를 누르면 그 장이 나간다");
    // 미리보기 방식이면 그대로
    h.app.core.lock().settings.update(|d| d["previewMode"] = json!(true));
    h.click("pv-chip-1", pv_pane);
    assert_eq!(h.live(), Some((0, 2)));
    assert_eq!(h.app.core.lock().pres.preview.as_ref().unwrap().index, 1);
}

#[test]
fn direct_mode_page_down_goes_live() {
    let mut h = H::new(titles());
    h.direct();
    h.app.apply(vec![Act::PreviewItem(0), Act::PreviewStep(1)]);
    assert_eq!(h.live(), Some((0, 1)), "← → · PageDown 도 바로 송출");
}

#[test]
fn item_and_slide_pick_sends_once_at_that_slide() {
    // 전체 격자·오른쪽 클릭: PreviewItem(i) + PreviewIndex(k) → 첫 장이 잠깐 나갔다가 k 장이 나가지 않고 k 장만 한 번
    let mut h = H::new(titles());
    h.direct();
    let v0 = h.app.core.lock().pres.version;
    h.app.apply(vec![Act::PreviewItem(0), Act::PreviewIndex(2)]);
    assert_eq!(h.live(), Some((0, 2)));
    assert_eq!(h.app.core.lock().pres.version, v0 + 1, "송출은 한 번");
}

#[test]
fn direct_mode_send_with_output_off_shows_band() {
    let mut h = H::new(titles());
    h.direct();
    h.click("cue-row-0", cue);
    assert!(h.live().is_some());
    h.frame(vec![], lv_pane);
    h.frame(vec![], lv_pane);
    assert!(seen("live-output-on").is_some(), "송출은 됐지만 예배당 화면이 꺼져 있으면 LIVE 칸 띠 [지금 켜기]");
}

// ---------------- 4) 같은 찬송가 중복 넣기 · 연타 ----------------

fn hymn3() -> Value {
    json!({ "title": "3장 성부 성자와 성령", "file": "C:\\없는폴더\\새찬송가\\003 성부 성자와 성령.pps", "number": 3, "background": "default", "obsLyrics": "" })
}

#[test]
fn same_hymn_rapid_clicks_add_one_item() {
    // 리허설 큐시트에 2초 동안 같은 3장이 5개 생긴 경로: 찬송가 탭 결과 줄(hymns_ui) → Act::AddItem("hymn") (두 번 누르기 = 두 번)
    let mut h = H::new(titles());
    h.app.sel = Some(0);
    for _ in 0..5 {
        h.app.apply(vec![Act::AddItem("hymn", Some(hymn3()))]);
    }
    let n = h.items().iter().filter(|x| x["type"] == "hymn").count();
    assert_eq!(n, 1, "연타·두 번 누르기로 같은 찬송가가 여러 개 생기지 않는다");
    assert_eq!(h.app.sel, Some(1));
}

#[test]
fn same_hymn_again_selects_existing_and_offers_add_anyway() {
    let mut h = H::new(titles());
    h.app.apply(vec![Act::AddItem("hymn", Some(hymn3()))]);
    h.app.core.lock().pick.forget_last(); // 연타 시간이 지난 것처럼
    h.app.sel = Some(0);
    h.app.apply(vec![Act::AddItem("hymn", Some(hymn3()))]);
    assert_eq!(h.items().len(), 3, "새 순서를 만들지 않는다");
    assert_eq!(h.app.sel, Some(2), "이미 있는 3번 순서를 고른다");
    let (msg, act) = h.toast();
    assert!(msg.contains("이미 3번 순서"), "{msg}");
    assert_eq!(act, Some("add-dup"));
    // [그래도 하나 더 넣기]
    let acts = h.click_acts("toast-act", toast);
    assert!(acts.iter().any(|a| matches!(a, Act::ToastAct("add-dup"))));
    h.app.apply(acts);
    assert_eq!(h.items().iter().filter(|x| x["type"] == "hymn").count(), 2, "일부러 하나 더 넣을 수 있다");
    let ids: Vec<_> = h.items().iter().map(|x| x["id"].clone()).collect();
    let mut u = ids.clone();
    u.dedup();
    assert_eq!(ids.len(), u.len(), "더 넣은 순서는 새 id");
}

#[test]
fn library_song_pick_goes_live_in_direct_mode() {
    let mut h = H::new(titles());
    h.direct();
    h.app.sel = Some(1);
    let song = json!({ "title": "주 하나님 지으신 모든 세계", "lyrics": "주 하나님 지으신 모든 세계\n내 마음 속에 그리어 볼 때", "songId": "s-1" });
    h.app.apply(vec![Act::AddItem("song", Some(song.clone()))]);
    assert_eq!(h.items().len(), 3);
    assert_eq!(h.live().map(|x| x.0), Some(2), "Electron addItem → previewItem → directSend: 넣은 찬양이 바로 나간다");
    // 같은 찬양을 다시 고르면 넣지 않고 그 순서를 (바로 송출이면 송출)
    h.app.core.lock().pick.forget_last();
    h.app.apply(vec![Act::PreviewItem(0)]);
    assert_eq!(h.live().map(|x| x.0), Some(0));
    h.app.apply(vec![Act::AddItem("song", Some(song))]);
    assert_eq!(h.items().len(), 3);
    assert_eq!(h.live().map(|x| x.0), Some(2));
}

#[test]
fn preview_mode_pick_previews_new_item_without_sending() {
    let mut h = H::new(titles());
    h.app.sel = Some(1);
    h.app.apply(vec![Act::AddItem("text", Some(json!({ "title": "새 공지", "body": "내용" })))]);
    assert_eq!(h.live(), None);
    let pv = h.app.core.lock().pres.preview.as_ref().map(|p| p.passage["cueIndex"].as_i64());
    assert_eq!(pv, Some(Some(2)), "넣은 순서를 미리보기 (Electron addItem → previewItem)");
}

#[test]
fn empty_new_item_does_not_warn() {
    let mut h = H::new(titles());
    h.app.apply(vec![Act::AddItem("song", None)]);
    assert!(h.toast().0.is_empty(), "빈 새 순서는 '보일 슬라이드가 없습니다' 경고를 내지 않는다: {}", h.toast().0);
}
