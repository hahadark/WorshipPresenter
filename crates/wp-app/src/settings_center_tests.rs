//! 설정 센터 합성 입력 시험: 실제 화면 전체(App::ui_body)를 headless 로 그리고 포인터·글자·글쇠를 넣는다.
//! 검색 → 항목, 변경 → 코어 값, 열기 단추 → 실제 창, 기본값 복원·취소·되돌리기 범위,
//! 바꾼 단축키가 실제로 듣는지, 1280×800 에서 창이 화면 안에 드는지를 본다.
use super::auditc_tests::{seen, H};
use super::dbl_tests::{click_at, full, idle, live};
use crate::settings_ui::items::Cat;
use crate::widgets;
use eframe::egui::{self, Event, Key, Pos2, RawInput, Rect, Vec2};
use serde_json::{json, Value};

/// 이 모듈 시험은 한 번에 하나씩: 열기 단추 시험이 프로그램 전체가 함께 쓰는 창 상태(도구·출력·동기화)를 잠깐 켠다
static ONE: parking_lot::Mutex<()> = parking_lot::Mutex::new(());

fn notice() -> Value {
    json!({ "id": "a", "type": "text", "title": "공지", "body": "첫째 장\n---\n둘째 장\n---\n셋째 장" })
}
fn open(cat: Cat) -> H {
    let mut h = H::new(vec![notice()]);
    h.app.tc.settings.open_cat(cat);
    idle(&mut h, 3);
    h
}
fn names() -> Vec<String> {
    widgets::HOVER_SEEN.with(|m| m.borrow().keys().filter(|k| k.starts_with("set-")).cloned().collect())
}
/// 그 위젯이 설정 창 안에 보일 때까지 마우스 휠로 스크롤하고, 누를 자리를 돌려준다
fn visible(h: &mut H, name: &str) -> Pos2 {
    for _ in 0..60 {
        idle(h, 1);
        let r = seen(name).unwrap_or_else(|| panic!("그린 자리 없음: {name} (있는 것: {:?})", names()));
        let win = seen("set-window").expect("설정 창");
        if matches!(name, "set-search" | "set-undo") {
            return r.center(); // 윗줄은 스크롤되지 않는다
        }
        let (top, bottom) = (win.top() + 84.0, win.bottom() - 16.0);
        let c = r.center();
        if c.y > top && c.y < bottom {
            idle(h, 2); // 부드러운 스크롤이 끝난 뒤의 자리
            let r = seen(name).unwrap();
            if r.center().y > top && r.center().y < bottom {
                return r.center();
            }
            continue;
        }
        let dy = if c.y >= bottom { -(c.y - bottom + 80.0).min(260.0) } else { (top - c.y + 80.0).min(260.0) };
        let p = Pos2::new(win.center().x + 120.0, win.center().y);
        full(h, vec![Event::PointerMoved(p), Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: egui::vec2(0.0, dy), phase: egui::TouchPhase::Move, source: egui::MouseWheelSource::Wheel, modifiers: Default::default() }]);
    }
    panic!("스크롤해도 보이지 않음: {name}");
}
fn press(h: &mut H, name: &str) {
    let p = visible(h, name);
    click_at(h, p);
    idle(h, 1);
}
fn key(h: &mut H, k: Key) {
    for pressed in [true, false] {
        full(h, vec![Event::Key { key: k, physical_key: None, pressed, repeat: false, modifiers: Default::default() }]);
    }
    idle(h, 1);
}
/// 글칸 안의 글을 모두 고른다 (다음에 치는 글이 그 자리를 대신한다)
fn select_all(h: &mut H) {
    let mods = egui::Modifiers { ctrl: true, command: true, ..Default::default() };
    for pressed in [true, false] {
        full(h, vec![Event::Key { key: Key::A, physical_key: None, pressed, repeat: false, modifiers: mods }]);
    }
}
fn typed(h: &mut H, s: &str) {
    full(h, vec![Event::Text(s.to_string())]);
}
fn get(h: &H, path: &[&str]) -> Value {
    crate::settings_ui::items::read(h.app.core.lock().settings.get(), path).clone()
}
fn put(h: &H, path: &[&str], v: Value) {
    crate::settings_ui::items::patch(&h.app.core, path, v);
}

#[test]
fn 검색해서_결과를_누르면_그_항목으로_간다() {
    let _one = ONE.lock();
    let mut h = open(Cat::Send);
    press(&mut h, "set-search");
    typed(&mut h, "글자 크기");
    idle(&mut h, 2);
    assert!(seen("set-hit-ui.scale").is_some(), "이름으로 찾음: {:?}", names());
    assert!(seen("set-item-previewMode").is_none(), "찾는 동안에는 분류 쪽 대신 결과만 보인다");
    press(&mut h, "set-hit-ui.scale");
    assert_eq!(h.app.tc.settings.cat, Cat::Ui, "그 항목의 분류로");
    assert_eq!(h.app.tc.settings.focused(), Some("ui.scale"), "그 항목을 강조");
    assert!(h.app.tc.settings.query.is_empty(), "검색은 지워지고 분류 쪽이 보인다");
    assert!(seen("set-ctl-ui.scale").is_some(), "실제 조절 칸이 보인다");

    // 설명·관련 낱말로도 찾고, 접힌 고급 항목이면 펼쳐서 보여 준다
    press(&mut h, "set-search");
    typed(&mut h, "37528");
    idle(&mut h, 2);
    press(&mut h, "set-hit-port");
    assert_eq!(h.app.tc.settings.cat, Cat::Obs);
    visible(&mut h, "set-ctl-port");

    // 단축키는 기능 이름·키로 찾아 단축키 쪽의 그 줄로
    press(&mut h, "set-search");
    typed(&mut h, "F6");
    idle(&mut h, 2);
    press(&mut h, "set-hit-key-nextSlide");
    assert_eq!(h.app.tc.settings.cat, Cat::Keys);
    idle(&mut h, 2);
    assert!(seen("set-key-nextSlide").is_some());
    assert!(seen("set-key-prevSlide").is_none(), "그 기능만 걸러 보인다");

    // 없는 낱말: 결과 없음 (분류 쪽으로 새지 않는다)
    press(&mut h, "set-search");
    typed(&mut h, "없는낱말xyz");
    idle(&mut h, 2);
    assert!(names().iter().all(|n| !n.starts_with("set-hit-") && !n.starts_with("set-item-")), "{:?}", names());
}

#[test]
fn 바꾸면_그_칸만_코어에_쓰이고_되돌리기는_그_칸만_돌린다() {
    let _one = ONE.lock();
    let mut h = open(Cat::Send);
    assert_eq!(get(&h, &["cueEdgeHide"]), json!(false));
    idle(&mut h, 2); // 설정 창이 설정을 읽어 둔 뒤에
    // 그 사이 다른 창이 템플릿 이름과 다른 설정을 바꿨다 (설정 창이 옛 사본을 되쓰면 사라진다)
    h.app.core.lock().settings.update(|d| {
        d["templates"][0]["name"] = json!("다른 창에서 고친 이름");
        d["navMode"] = json!("continuous");
        d["obs"]["passwordEnc"] = json!("비밀");
        d["앞으로_생길_칸"] = json!({ "a": 1 });
    });
    let before = h.app.core.lock().settings.get().clone();
    press(&mut h, "set-ctl-cueEdgeHide");
    let mut want = before.clone();
    want["cueEdgeHide"] = json!(true);
    assert_eq!(h.app.core.lock().settings.get(), &want, "누른 칸 하나만 바뀐다");
    assert!(h.app.tc.settings.status_text().contains("적용됨"), "상태 글: {}", h.app.tc.settings.status_text());
    idle(&mut h, 2);
    assert!(seen("set-def-cueEdgeHide").is_some(), "화면에도 기본값과 다르다고 나오고 '기본값으로'가 생긴다");

    // 고르기 칸
    press(&mut h, "set-ctl-searchMode");
    press(&mut h, "set-opt-searchMode-text");
    assert_eq!(get(&h, &["searchMode"]), json!("text"));
    want["searchMode"] = json!("text");
    assert_eq!(h.app.core.lock().settings.get(), &want);

    // 되돌리기: 마지막으로 바꾼 것(검색 방식)만
    press(&mut h, "set-undo");
    want["searchMode"] = json!("auto");
    assert_eq!(h.app.core.lock().settings.get(), &want, "되돌리기는 방금 바꾼 칸만");
    assert!(seen("set-undo").is_none(), "되돌릴 것이 없으면 단추도 없다");

    // 항목 하나만 기본값으로
    press(&mut h, "set-def-cueEdgeHide");
    assert_eq!(h.app.core.lock().settings.get(), &before, "처음 상태 (다른 창이 바꾼 것은 그대로)");
}

#[test]
fn 분류_기본값_복원은_묻고_그_분류만_돌리며_되돌릴_수_있다() {
    let _one = ONE.lock();
    let mut h = open(Cat::Send);
    h.app.core.lock().settings.update(|d| {
        d["previewMode"] = json!(false);
        d["navMode"] = json!("continuous");
        d["split"] = json!({ "mode": "count", "count": 5 });
        d["ui"]["scale"] = json!(100);
        d["ui"]["theme"] = json!("navy"); // 다른 분류
        d["output"]["cursorHidden"] = json!(false); // 다른 분류
        d["obs"] = json!({ "enabled": false, "host": "10.0.0.9", "port": 4455, "password": "", "passwordEnc": "비밀" });
        d["remote"]["pin"] = json!("1234");
        d["syncDir"] = json!("D:\\동기화");
        d["logoScreenImage"] = json!("/media/logo.png");
        d["uiLang"] = json!("ko");
        d["templates"][0]["name"] = json!("내 템플릿");
        d["activeTemplates"]["main"] = json!("overlay");
        d["모르는_칸"] = json!([1, 2]);
        d["shortcuts"]["nextSlide"] = json!("F8");
    });
    h.app.core.lock().settings.update(|d| d["output"]["autoOnSend"] = json!(false));
    let before = h.app.core.lock().settings.get().clone();
    let cues_before = h.app.core.lock().store.list_cues().len();

    // 그만두기: 아무것도 바뀌지 않는다
    press(&mut h, "set-reset");
    assert!(seen("set-reset-yes").is_some(), "한 번 더 묻는다");
    assert_eq!(h.app.core.lock().settings.get(), &before, "묻는 동안에는 그대로");
    press(&mut h, "set-reset-no");
    assert_eq!(h.app.core.lock().settings.get(), &before);
    assert!(seen("set-reset-yes").is_none());

    // 돌리기: 이 분류의 값만 기본값으로
    press(&mut h, "set-reset");
    press(&mut h, "set-reset-yes");
    let mut want = before.clone();
    want["previewMode"] = json!(true);
    want["navMode"] = json!("range");
    want["split"] = json!({ "mode": "auto", "count": 2 });
    assert_eq!(h.app.core.lock().settings.get(), &want, "다른 분류·비밀·주소·파일·템플릿·단축키·모르는 칸은 그대로");
    assert_eq!(h.app.core.lock().store.list_cues().len(), cues_before, "큐시트는 그대로");
    assert!(h.app.core.lock().cue.is_some(), "열어 둔 큐시트도 그대로");

    // 되돌리기: 복원 전으로 통째로
    press(&mut h, "set-undo");
    assert_eq!(h.app.core.lock().settings.get(), &before, "되돌리면 복원 전 값 그대로");
}

#[test]
fn 로고_화면_그림은_파일을_고르고_지운다() {
    let _one = ONE.lock();
    let mut h = open(Cat::Output);
    let dir = h.app.data_dir.clone();
    let src = dir.join("교회 로고.png");
    std::fs::write(&src, [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]).unwrap();
    h.app.tc.settings.test_pick = Some(src.clone());
    press(&mut h, "set-ctl-logoScreenImage");
    let url = get(&h, &["logoScreenImage"]).as_str().unwrap_or("").to_string();
    assert!(!url.is_empty(), "고른 그림이 설정에 들어간다 (상태: {})", h.app.tc.settings.status_text());
    assert!(!url.contains(":\\"), "글칸에 경로를 쓰는 대신 가져온 주소가 들어간다: {url}");
    press(&mut h, "set-clear-logoScreenImage");
    assert_eq!(get(&h, &["logoScreenImage"]), json!(""));
    assert!(src.exists(), "지우기는 설정만 비우고 그림 파일은 그대로 둔다");
}

#[test]
fn 바꾼_단축키가_바로_듣고_틀린_키는_저장되지_않는다() {
    let _one = ONE.lock();
    let mut h = open(Cat::Keys);
    h.app.sel = Some(0);
    h.app.core.lock().preview_item(0);
    // 기능 이름으로 걸러 그 줄만 보이게 (긴 목록에서 찾는 실제 사용 흐름)
    h.app.tc.settings.keys.filter = "F6".into();
    assert_eq!(get(&h, &["shortcuts", "nextSlide"]), json!("F6"));

    // 틀린 키: 저장하지 않고 까닭을 보여 준다
    press(&mut h, "set-key-nextSlide");
    select_all(&mut h);
    typed(&mut h, "Hyper+Q");
    key(&mut h, Key::Enter);
    idle(&mut h, 2);
    assert_eq!(get(&h, &["shortcuts", "nextSlide"]), json!("F6"), "틀린 키는 저장되지 않는다");
    assert!(seen("set-key-err-nextSlide").is_some(), "까닭이 보인다: {:?}", names());
    assert_eq!(h.app.tc.settings.keys.draft("nextSlide"), Some("Hyper+Q"), "쓴 글은 남아 고칠 수 있다");

    // 맞는 키 (소문자·띄어쓰기도 받아 한 모양으로 저장)
    h.app.tc.settings.keys.filter.clear();
    h.app.tc.settings.keys.clear_drafts();
    h.app.tc.settings.keys.filter = "F6".into();
    press(&mut h, "set-key-nextSlide");
    select_all(&mut h);
    typed(&mut h, "f8");
    key(&mut h, Key::Enter);
    idle(&mut h, 2);
    assert_eq!(get(&h, &["shortcuts", "nextSlide"]), json!("F8"));

    // 실제로 듣는지: 설정 창을 닫고 조작 화면에서
    h.app.tc.settings.open = false;
    idle(&mut h, 2);
    key(&mut h, Key::F9);
    assert_eq!(live(&h), Some((0, 0)), "F9 = 미리보기 송출");
    key(&mut h, Key::F8);
    assert_eq!(live(&h), Some((0, 1)), "새 키 F8 이 다음 슬라이드");
    key(&mut h, Key::F6);
    assert_eq!(live(&h), Some((0, 1)), "옛 키 F6 은 이제 듣지 않는다");

    // 다른 기능과 같은 키를 주면 충돌 요약에 나온다
    put(&h, &["shortcuts", "nextSlide"], json!("F5"));
    h.app.tc.settings.open_cat(Cat::Keys);
    h.app.tc.settings.keys.filter.clear();
    idle(&mut h, 3);
    assert!(seen("set-key-problems").is_some(), "겹치는 키 요약");

    // 하나만 기본값으로
    h.app.tc.settings.keys.filter = "다음 슬라이드 (Live)".into();
    press(&mut h, "set-key-def-nextSlide");
    assert_eq!(get(&h, &["shortcuts", "nextSlide"]), json!("F6"));
    h.app.tc.settings.open = false;
    idle(&mut h, 2);
    key(&mut h, Key::F6);
    assert_eq!(live(&h), Some((0, 2)), "기본값으로 돌리면 F6 이 다시 듣는다");
}

#[test]
fn 모든_단축키_기본값은_묻고_바꾼_키만_돌리며_되돌릴_수_있다() {
    let _one = ONE.lock();
    let mut h = open(Cat::Keys);
    h.app.core.lock().settings.update(|d| {
        d["shortcuts"]["nextSlide"] = json!("F8");
        d["shortcuts"]["openHistory"] = json!("");
        d["shortcuts"]["앞으로_생길_동작"] = json!("Ctrl+J");
        d["globalShortcuts"] = json!(true);
        d["reservedKeys"] = json!("F1");
    });
    let before = h.app.core.lock().settings.get().clone();
    press(&mut h, "set-keys-reset");
    press(&mut h, "set-keys-reset-no");
    assert_eq!(h.app.core.lock().settings.get(), &before, "그만두면 그대로");
    press(&mut h, "set-keys-reset");
    press(&mut h, "set-keys-reset-yes");
    let mut want = before.clone();
    want["shortcuts"]["nextSlide"] = json!("F6");
    want["shortcuts"]["openHistory"] = json!("Ctrl+H");
    assert_eq!(h.app.core.lock().settings.get(), &want, "단축키만 — 모르는 동작·전역 단축키·예약 키는 그대로");
    press(&mut h, "set-undo");
    assert_eq!(h.app.core.lock().settings.get(), &before);
}

#[test]
fn 센터의_열기_단추가_실제_창을_연다() {
    let _one = ONE.lock();
    use crate::tools_ui::{self, Tool};
    use crate::trackd_ui::{self, Win};
    type Check = Box<dyn Fn(&mut H) -> bool>;
    // (분류, 항목, 열렸는지 보고 다시 닫기)
    let cases: Vec<(Cat, &str, Check)> = vec![
        (Cat::Output, "out-open", Box::new(|_| trackd_ui::close_for_test(Win::Output))),
        (Cat::Output, "templates", Box::new(|h| std::mem::take(&mut h.app.tc.templates.open))),
        (Cat::Output, "themes", Box::new(|h| std::mem::take(&mut h.app.tc.themes.open))),
        (Cat::Obs, "obs-remote", Box::new(|_| trackd_ui::close_for_test(Win::Obs))),
        (Cat::Obs, "lan", Box::new(|_| tools_ui::close_for_test(Tool::Lan))),
        (Cat::Ui, "workspaces", Box::new(|h| std::mem::take(&mut h.app.workspace.open))),
        (Cat::Data, "bible", Box::new(|h| std::mem::take(&mut h.app.tc.import.open))),
        (Cat::Data, "commentary", Box::new(|h| std::mem::take(&mut h.app.tc.commentary.open))),
        (Cat::Backup, "sync", Box::new(|_| std::mem::take(&mut crate::system_ui::ST.lock().sync))),
        (Cat::Backup, "cues", Box::new(|h| std::mem::take(&mut h.app.tb.cues))),
        (Cat::Backup, "usage", Box::new(|h| std::mem::take(&mut h.app.tc.history.usage_open))),
        (Cat::Remote, "remote", Box::new(|_| tools_ui::close_for_test(Tool::Remote))),
        (Cat::Remote, "trigger", Box::new(|_| tools_ui::close_for_test(Tool::Trigger))),
        (Cat::Remote, "devices", Box::new(|_| tools_ui::close_for_test(Tool::Devices))),
        (Cat::Remote, "cameras", Box::new(|_| std::mem::take(&mut crate::system_ui::ST.lock().cameras))),
        (Cat::Remote, "lan2", Box::new(|_| tools_ui::close_for_test(Tool::Lan))),
        (Cat::About, "update", Box::new(|_| trackd_ui::close_for_test(Win::Update))),
        (Cat::About, "about", Box::new(|h| std::mem::take(&mut h.app.tc.history.about_open))),
    ];
    let mut h = open(Cat::Output);
    for (cat, id, opened) in cases {
        h.app.tc.settings.open_cat(cat);
        if id == "lan" {
            let now = h.time;
            h.app.tc.settings.goto("lan", now); // 고급 설정 안: 검색으로 왔을 때처럼 펼친다
        }
        let p = visible(&mut h, &format!("set-open-{id}"));
        // 단추를 누르는 프레임에 창이 열린다. 다른 창이 설정 창을 가리지 않게 확인한 뒤 바로 닫는다
        click_at(&mut h, p);
        let ok = opened(&mut h);
        assert!(ok, "'{id}' 단추가 그 창을 열지 않았다 (누른 곳 {p:?} · 보이는 것 {:?})", names());
        assert!(h.app.tc.settings.open, "설정 창은 그대로 열려 있다");
    }
    // 주소 복사
    h.app.tc.settings.open_cat(Cat::Obs);
    press(&mut h, "set-ctl-url-overlay");
    let port = h.app.core.lock().port;
    assert!(h.app.tc.settings.status_text().contains(&format!("http://127.0.0.1:{port}/outputs/overlay")), "{}", h.app.tc.settings.status_text());
    // OBS 자막 '슬라이드 그대로' 는 보기 메뉴와 같은 길로 켜진다
    assert!(!h.app.core.lock().obs_full);
    press(&mut h, "set-ctl-obsfull");
    assert!(h.app.core.lock().obs_full);
}

/// 화면 크기를 정해 한 프레임
fn sized(h: &mut H, w: f32, ht: f32) {
    h.time += 0.05;
    widgets::HOVER_SEEN.with(|m| m.borrow_mut().clear());
    let app = &mut h.app;
    let mut out = h.ctx.run_pass(RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(w, ht))), time: Some(h.time), ..Default::default() }, |ctx| {
        ctx.root_ui(|ui| app.ui_body(ui));
    });
    out.textures_delta.clear();
}

#[test]
fn 화면_1280x800_에서도_창과_분류가_화면_안에_든다() {
    let _one = ONE.lock();
    for cat in Cat::ALL {
        let mut h = H::new(vec![notice()]);
        h.app.tc.settings.open_cat(cat);
        for _ in 0..4 {
            sized(&mut h, 1280.0, 800.0);
        }
        let win = seen("set-window").expect("설정 창");
        let scr = Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 800.0));
        assert!(scr.contains_rect(win), "{cat:?}: 창이 화면 밖으로 나감 {win:?}");
        assert!(win.width() >= 900.0 && win.width() <= 1010.0, "{cat:?}: 너비 {}", win.width());
        assert!(win.height() >= 600.0, "{cat:?}: 높이 {}", win.height());
        for c in Cat::ALL {
            let r = seen(&format!("set-cat-{}", c.key())).expect("분류 줄");
            assert!(win.contains_rect(r), "{cat:?}: 분류 '{}' 가 창 밖 {r:?} / {win:?}", c.key());
        }
        // 오른쪽 상세의 항목이 창 너비를 넘지 않는다 (가로로 잘리지 않음)
        let items: Vec<(String, Rect)> = widgets::HOVER_SEEN.with(|m| m.borrow().iter().filter(|(k, _)| k.starts_with("set-item-") || k.starts_with("set-ctl-") || k.starts_with("set-open-") || k.starts_with("set-key-")).map(|(k, r)| (k.clone(), *r)).collect());
        assert!(!items.is_empty(), "{cat:?}: 항목이 그려져야 한다");
        for (k, r) in items {
            assert!(r.left() >= win.left() + 190.0 && r.right() <= win.right() + 0.5, "{cat:?}: '{k}' 가 가로로 잘림 {r:?} / {win:?}");
        }
    }
}
