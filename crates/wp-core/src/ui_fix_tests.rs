//! 조작 화면 결함 수정(검토 1113) 회귀 테스트 — 송출 가드 · 고르기로 고른 폴더 밖 PPT · 순서 자동 동작 저장 형식
use crate::core::Core;
use crate::db::Cue;
use serde_json::{json, Value};

// 기존 사용자 자료를 복사하지 않도록 빈 DB가 있는 고유 임시 폴더를 쓴다.
fn core() -> Core {
    let dir = std::env::temp_dir().join(format!("wp-ui-fix-{}", crate::util::short_id("t")));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("bible.db"), b"").unwrap();
    Core::open(dir.clone(), dir, None).unwrap()
}
fn cue(id: &str, items: Vec<Value>) -> Cue {
    Cue { id: id.into(), name: id.into(), service_date: String::new(), start_time: String::new(), items, memo: String::new(), pinned: false, template: false }
}
fn title(name: &str) -> Value { json!({"type":"title", "title":name}) }
/// 미리보기를 만들 수 없는 순서 (파일이 없는 PPT)
fn broken_hymn(name: &str) -> Value { json!({"type":"hymn", "title":name, "file":"Z:\\없는폴더\\없는파일.pptx"}) }

#[test]
fn send_item_fails_when_preview_fails_and_keeps_live() {
    let mut c = core();
    c.cue = Some(cue("A", vec![title("A0"), title("A1"), title("A2")]));
    assert!(c.send_item(2));
    let live_before = c.pres.live_state();
    assert_eq!(live_before["passage"]["cueId"], "A");
    assert_eq!(live_before["passage"]["cueIndex"], 2);

    // 큐시트 B 를 연다 (Electron openCue 처럼 미리보기는 남겨 둔다)
    c.cue = Some(cue("B", vec![title("B0"), title("B1"), broken_hymn("B2 없는 PPT")]));
    assert_eq!(c.pres.preview.as_ref().unwrap().passage["cueId"], "A");

    // B 의 3번을 두 번 누름 → 미리보기 실패 → 송출하지 않는다 (A 의 3번이 다시 나가면 안 됨)
    let mut rx = c.tx.subscribe();
    assert!(!c.send_item(2));
    assert_eq!(c.pres.live_state(), live_before);
    while let Ok(e) = rx.try_recv() { assert!(!matches!(e, crate::core::Ev::Live), "실패한 미리보기 뒤에 송출 이벤트가 나갔다"); }
    // 미리보기는 그대로 A 의 것
    assert_eq!(c.pres.preview.as_ref().unwrap().passage["cueId"], "A");
}

#[test]
fn preview_is_item_compares_cue_id_and_index() {
    let mut c = core();
    c.cue = Some(cue("A", vec![title("A0"), title("A1")]));
    assert!(c.preview_item(1));
    assert!(c.preview_is_item(1));
    assert!(!c.preview_is_item(0));
    // 다른 큐시트로 바꾸면 같은 번호라도 아니다
    c.cue = Some(cue("B", vec![title("B0"), title("B1")]));
    assert!(!c.preview_is_item(1));
}

/// 고르기… 로 고른 등록 폴더 밖 PPT 는 Electron 처럼 droppedFiles 에 넣어야 is_allowed 가 연다 (검토 1113 찬송가 2번)
#[test]
fn picked_ppt_outside_folders_allowed_after_add_dropped_files() {
    let mut c = core();
    let p = std::path::PathBuf::from(r"Z:\바탕화면\임시 광고.pptx");
    let f = p.to_string_lossy().to_string();
    assert!(!crate::hymns::is_allowed(&c, &f), "등록 폴더 밖 파일은 원래 거부");
    c.add_dropped_files(std::slice::from_ref(&p));
    assert!(crate::hymns::is_allowed(&c, &f));
    // 대소문자·슬래시 차이도 norm 으로 같게 본다
    assert!(crate::hymns::is_allowed(&c, &f.replace('\\', "/").to_uppercase()));
}

#[test]
fn send_item_succeeds_on_other_cue_when_preview_ok() {
    let mut c = core();
    c.cue = Some(cue("A", vec![title("A0"), title("A1")]));
    assert!(c.send_item(1));
    c.cue = Some(cue("B", vec![title("B0"), title("B1")]));
    assert!(c.send_item(1));
    let l = c.pres.live_state();
    assert_eq!(l["passage"]["cueId"], "B");
    assert_eq!(l["passage"]["cueIndex"], 1);
    assert_eq!(l["passage"]["itemTitle"], "B1");
}

/// [점검 A] 장절이 빈 성경 순서는 Electron 처럼 보일 것이 없다 (제목 카드 '성경' 을 대신 내보내지 않는다).
#[test]
fn empty_ref_bible_item_is_not_sent_as_title_card() {
    let mut c = core();
    c.cue = Some(cue("A", vec![title("A0"), json!({"type":"bible", "title":"성경", "ref":""})]));
    assert!(c.send_item(0));
    assert!(c.item_deck(1).is_err());
    assert!(!c.send_item(1));
    assert_eq!(c.pres.live_state()["passage"]["cueIndex"], 0);
}
