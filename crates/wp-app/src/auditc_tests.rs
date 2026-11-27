//! 갈래 C 점검: 조작 화면 동작을 원본(Electron control.js·quicktext.js)과 맞춘 것의 합성 입력 시험.
//! 실제 화면 함수를 headless 로 그리고 키·글자·포인터 사건을 넣어 결과(미리보기·LIVE·목록)를 본다.
use super::{Act, App, Snap};
use eframe::egui::{self, Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2};
use serde_json::{json, Value};
use wp_core::core::Core;

pub(super) struct H {
    pub ctx: egui::Context,
    pub app: App,
    dir: std::path::PathBuf,
    pub(super) time: f64,
}

pub(super) type Draw = fn(&mut App, &egui::Context, &mut egui::Ui, &Snap, &mut Vec<Act>);

impl H {
    /// 요한복음 3:16~17 만 든 시험 성경(tst)과 순서들
    pub fn new(items: Vec<Value>) -> Self {
        let dir = std::env::temp_dir().join(format!("wp-auditc-{}-{}", std::process::id(), wp_core::util::short_id("t")));
        std::fs::create_dir_all(&dir).unwrap();
        drop(wp_core::db::BibleStore::open(&dir.join("bible.db")).unwrap());
        let mut core = Core::open(dir.clone(), dir.clone(), None).unwrap();
        {
            use wp_core::importer::{replace_version, Data, Row};
            let row = |v: i64, t: &str| Row { book: 43, chapter: 3, verse: v, text: t.into() };
            let d = Data { verses: vec![row(16, "하나님이 세상을 이처럼 사랑하사"), row(17, "하나님이 그 아들을 세상에 보내신 것은")], ..Default::default() };
            replace_version(&core.store.db, &json!({ "id": "tst", "name": "시험", "source": "s", "license": "l" }), &d).unwrap();
            core.store.reload_structure().unwrap();
        }
        core.set_version("tst");
        core.new_cue("합성입력 전용").unwrap();
        for it in items {
            core.add_item(it, None);
        }
        core.b.undo = Default::default();
        core.toast = None;
        core.settings.update(|d| d["output"]["autoOnSend"] = json!(false));
        // (나) 고정 배치: 이 합성 시험들은 본문 판(목록·격자)을 누르는 시험이라 시간표를 접고 본문 판을 넓게 둔다
        // (예전 독 배치에서 본문 패널이 차지하던 높이와 비슷하게)
        core.settings.update(|d| d["rsProLayout"] = json!({ "timeline": false, "viewers": 0.3 }));
        let core = std::sync::Arc::new(wp_core::lockcheck::Mutex::new(core));
        Self { ctx: egui::Context::default(), app: App::for_test(core, dir.clone()), dir, time: 0.0 }
    }
    pub fn frame_m(&mut self, mut events: Vec<Event>, mods: Modifiers, f: Draw) {
        events.insert(0, Event::ModifiersChanged(mods));
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
        self.app.apply(acts);
    }
    pub fn frame(&mut self, events: Vec<Event>, f: Draw) {
        self.frame_m(events, Modifiers::NONE, f);
    }
    pub fn key(&mut self, k: Key, mods: Modifiers, f: Draw) {
        self.frame_m(vec![Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: mods }], mods, f);
        self.frame_m(vec![Event::Key { key: k, physical_key: None, pressed: false, repeat: false, modifiers: mods }], mods, f);
    }
    /// 한 글자씩 친다 (글자마다 한 프레임)
    pub fn type_text(&mut self, s: &str, f: Draw) {
        for ch in s.chars() {
            self.frame(vec![Event::Text(ch.to_string())], f);
        }
    }
    pub fn click_at(&mut self, p: Pos2, f: Draw) {
        self.frame(vec![Event::PointerMoved(p)], f);
        for pressed in [true, false] {
            self.frame(vec![Event::PointerButton { pos: p, button: PointerButton::Primary, pressed, modifiers: Default::default() }], f);
        }
    }
    pub fn click(&mut self, name: &str, f: Draw) {
        self.frame(vec![], f);
        self.frame(vec![], f);
        let p = seen(name).unwrap_or_else(|| panic!("그린 자리 없음: {name}")).center();
        self.click_at(p, f);
        self.frame(vec![], f);
    }
    pub fn live_title(&self) -> Option<String> {
        self.app.core.lock().pres.live.as_ref().map(|l| l.deck.passage["title"].as_str().unwrap_or("").to_string())
    }
    pub fn preview_title(&self) -> Option<String> {
        self.app.core.lock().pres.preview.as_ref().map(|p| p.passage["title"].as_str().unwrap_or("").to_string())
    }
    pub fn direct(&mut self) {
        self.app.core.lock().settings.update(|d| d["previewMode"] = json!(false));
    }
}
impl Drop for H {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
pub(super) fn seen(name: &str) -> Option<Rect> {
    crate::widgets::HOVER_SEEN.with(|m| m.borrow().get(name).copied())
}

/// 전역 단축키 + 검색칸 (실제 화면 차례: 단축키를 위젯보다 먼저)
fn search(app: &mut App, ctx: &egui::Context, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
    let typing = ctx.egui_wants_keyboard_input();
    app.shortcuts(ctx, acts, typing);
    app.search_bar(ui, s, acts);
}
fn focus_search(h: &mut H) {
    h.app.focus_search = true;
    h.frame(vec![], search);
    h.frame(vec![], search);
}

// ---------------- 검색칸: 치는 중에는 송출·미리보기가 바뀌지 않음 (control.js #q input → 후보 목록만) ----------------

#[test]
fn typing_reference_in_direct_mode_does_not_go_live_until_enter() {
    let mut h = H::new(vec![]);
    h.direct();
    focus_search(&mut h);
    h.type_text("요3:16", search);
    assert_eq!(h.app.search, "요3:16");
    assert!(h.live_title().is_none(), "치는 중인 '요3:1' 이 예배당에 나가면 안 됨: {:?}", h.live_title());
    assert!(h.preview_title().is_none(), "미리보기도 Enter 전에는 그대로");
    assert_eq!(h.app.result["ok"], json!(true), "목록(결과)은 치는 대로 보인다");
    h.key(Key::Enter, Modifiers::NONE, search);
    assert_eq!(h.live_title().as_deref(), Some("요한복음 3:16"), "Enter = 검색 → 바로 송출 모드면 송출");
}

#[test]
fn typing_keeps_current_cue_preview_and_enter_replaces_it() {
    let mut h = H::new(vec![json!({ "id": "a", "type": "title", "title": "설교" })]);
    h.app.apply(vec![Act::PreviewItem(0)]);
    let before = h.preview_title();
    focus_search(&mut h);
    h.type_text("요3:16", search);
    assert_eq!(h.preview_title(), before, "치는 동안 고른 순서의 미리보기를 덮지 않음");
    h.key(Key::Enter, Modifiers::NONE, search);
    assert_eq!(h.preview_title().as_deref(), Some("요한복음 3:16"));
    assert!(h.live_title().is_none(), "미리보기 모드의 Enter 는 송출하지 않음 (doSearch)");
}

#[test]
fn ctrl_enter_send_now_searches_then_sends() {
    let mut h = H::new(vec![]);
    focus_search(&mut h);
    h.type_text("요3:17", search);
    assert!(h.live_title().is_none());
    h.key(Key::Enter, Modifiers::COMMAND, search);
    assert_eq!(h.live_title().as_deref(), Some("요한복음 3:17"), "Ctrl+Enter (sendNow) = 검색해 미리보기로 만든 뒤 송출");
}

#[test]
fn ctrl_enter_with_failed_search_does_not_send_old_preview() {
    let mut h = H::new(vec![json!({ "id": "a", "type": "title", "title": "설교" })]);
    h.app.apply(vec![Act::PreviewItem(0)]);
    focus_search(&mut h);
    h.type_text("없는책3:1", search);
    h.key(Key::Enter, Modifiers::COMMAND, search);
    assert!(h.live_title().is_none(), "검색이 실패하면 예전 미리보기를 송출하지 않음");
}

// ---------------- 즉석 자막 (quicktext.js) ----------------

fn quick(app: &mut App, ctx: &egui::Context, _: &mut egui::Ui, _: &Snap, acts: &mut Vec<Act>) {
    let typing = ctx.egui_wants_keyboard_input();
    app.shortcuts(ctx, acts, typing);
    app.trackb_shortcuts(ctx);
    app.quick_window(ctx);
}

#[test]
fn quick_text_ctrl_t_opens_enter_sends_clears_and_remembers() {
    let mut h = H::new(vec![]);
    h.frame(vec![], quick);
    h.key(Key::T, Modifiers::COMMAND, quick);
    assert!(h.app.tb.quick, "Ctrl+T 로 열기");
    h.frame(vec![], quick);
    h.type_text("예배 후 전 교인 식사가 있습니다", quick);
    h.key(Key::Enter, Modifiers::NONE, quick);
    assert_eq!(h.live_title().as_deref(), Some("즉석 자막"), "Enter 로 바로 송출");
    let live_text = h.app.core.lock().pres.live.as_ref().unwrap().deck.slides[0]["parts"][0]["text"].as_str().unwrap_or("").to_string();
    assert!(live_text.contains("전 교인 식사"), "{live_text}");
    assert_eq!(h.app.tb.quick_text(), "", "보낸 뒤 입력칸은 비움");
    let dir = h.app.core.lock().data_dir.clone();
    assert_eq!(crate::trackb_ui::quick_recent(&dir), vec!["예배 후 전 교인 식사가 있습니다".to_string()], "최근 목록에 남음");
    // Esc 로 닫기 (보조 화면은 그대로)
    h.frame(vec![], quick);
    h.key(Key::Escape, Modifiers::NONE, quick);
    assert!(!h.app.tb.quick, "Esc 로 닫기");
    assert!(h.live_title().is_some(), "닫아도 송출은 그대로");
}

#[test]
fn quick_text_shift_enter_is_newline_and_recent_click_resends() {
    let mut h = H::new(vec![]);
    h.app.tb.quick = true;
    h.frame(vec![], quick);
    h.frame(vec![], quick);
    h.type_text("첫 줄", quick);
    h.key(Key::Enter, Modifiers::SHIFT, quick);
    h.type_text("둘째 줄", quick);
    assert!(h.live_title().is_none(), "Shift+Enter 는 송출하지 않음");
    assert_eq!(h.app.tb.quick_text(), "첫 줄\n둘째 줄");
    h.key(Key::Enter, Modifiers::NONE, quick);
    assert_eq!(h.live_title().as_deref(), Some("즉석 자막"));
    // 다른 것을 띄운 뒤 최근 목록을 누르면 다시 송출
    h.app.core.lock().clear_all();
    h.app.core.lock().pres.live = None;
    h.click("quick-recent-0", quick);
    assert_eq!(h.live_title().as_deref(), Some("즉석 자막"), "최근 목록을 누르면 다시 송출");
    h.click("quick-forget-0", quick);
    let dir = h.app.core.lock().data_dir.clone();
    assert!(crate::trackb_ui::quick_recent(&dir).is_empty(), "✕ 로 목록에서 지우기");
}

#[test]
fn quick_text_long_text_splits_and_to_cue_uses_first_line() {
    let mut h = H::new(vec![]);
    h.app.tb.quick = true;
    h.frame(vec![], quick);
    assert!(h.app.quick_send("하나\n---\n둘"));
    assert_eq!(h.app.core.lock().pres.live.as_ref().unwrap().deck.slides.len(), 2, "--- 로 나눈 글은 두 장");
    h.app.tb.set_quick_text("이번 주 일정은 다음과 같습니다 꼭 참석해 주세요\n수요예배");
    h.click("quick-cue", quick);
    let items = h.app.core.lock().cue.as_ref().unwrap().items.clone();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["type"], "text");
    assert_eq!(items[0]["title"].as_str().unwrap().chars().count(), 21, "첫 줄 20자 + …: {}", items[0]["title"]);
    assert!(items[0]["body"].as_str().unwrap().contains("수요예배"));
}

#[test]
fn custom_shortcuts_for_quick_text_are_respected() {
    let mut h = H::new(vec![]);
    h.app.core.lock().settings.update(|d| d["shortcuts"]["quickText"] = json!("Ctrl+J"));
    h.frame(vec![], quick);
    h.key(Key::T, Modifiers::COMMAND, quick);
    assert!(!h.app.tb.quick, "바꾼 뒤에는 Ctrl+T 로 열리지 않음");
    h.key(Key::J, Modifiers::COMMAND, quick);
    assert!(h.app.tb.quick, "설정한 Ctrl+J 로 열림");
    h.frame(vec![], quick);
    h.app.core.lock().settings.update(|d| d["shortcuts"]["quickText"] = json!("Ctrl+J"));
    // 글칸 밖에서 한 번 더 누르면 닫힘 (toggle)
    h.ctx.memory_mut(|m| m.request_focus(egui::Id::new("nothing")));
    h.key(Key::J, Modifiers::COMMAND, quick);
    assert!(!h.app.tb.quick, "다시 누르면 닫힘 (BCQuick.toggle)");
}
