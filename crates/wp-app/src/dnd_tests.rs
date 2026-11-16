//! 끌어다 놓기·붙여 넣기 합성 시험: egui RawInput 의 dropped_files / hovered_files 를 넣고 실제 조작 화면을 그려
//! 큐시트·라이브러리 설정이 원본(dropfiles.js)대로 바뀌는지 본다.
use super::auditc_tests::{seen, H};
use super::{Act, App, Snap};
use crate::dnd::{self, Zone};
use eframe::egui::{self, Event, Pos2, RawInput, Rect, Vec2};
use serde_json::{json, Value};
use std::path::PathBuf;


fn draw(app: &mut App, ctx: &egui::Context, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
    dnd::frame(app, ctx);
    app.cue_panel(ui, s, acts);
}
fn draw_lib(app: &mut App, ctx: &egui::Context, ui: &mut egui::Ui, _: &Snap, _: &mut Vec<Act>) {
    dnd::frame(app, ctx);
    app.library_panel(ui);
}

/// 포인터를 pos 에 두고(PointerMoved) 파일을 놓거나 끌어 오는 한 프레임
fn frame_files(h: &mut H, pos: Pos2, dropped: &[PathBuf], hovered: bool, f: super::auditc_tests::Draw) {
    h.time += 0.05;
    crate::widgets::HOVER_SEEN.with(|m| m.borrow_mut().clear());
    let snap = h.app.snap();
    let mut acts = vec![];
    let app = &mut h.app;
    let raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1600.0, 960.0))),
        time: Some(h.time),
        events: vec![Event::PointerMoved(pos)],
        dropped_files: dropped.iter().map(|p| std::sync::Arc::new(TestDrop(p.clone())) as egui::DroppedFileHandle).collect(),
        hovered_files: if hovered { vec![egui::HoveredFile { path: dropped.first().cloned(), ..Default::default() }] } else { vec![] },
        ..Default::default()
    };
    let mut out = h.ctx.run_pass(raw, |ctx| {
        ctx.root_ui(|root| {
            egui::CentralPanel::default_margins().show(root, |ui| f(app, ctx, ui, &snap, &mut acts));
        });
    });
    out.textures_delta.clear();
    h.app.apply(acts);
}

#[derive(Debug)]
struct TestDrop(PathBuf);
impl egui::DroppedFile for TestDrop {
    fn path(&self) -> &std::path::Path {
        &self.0
    }
    fn bytes(&self) -> Result<Vec<u8>, String> {
        std::fs::read(&self.0).map_err(|e| e.to_string())
    }
}

fn tmpdir() -> PathBuf {
    let d = std::env::temp_dir().join(format!("wp-dndtest-{}-{}", std::process::id(), wp_core::util::short_id("t")));
    std::fs::create_dir_all(&d).unwrap();
    d
}
fn types(h: &H) -> Vec<String> {
    h.app.core.lock().cue.as_ref().unwrap().items.iter().map(|x| x["type"].as_str().unwrap_or("").to_string()).collect()
}
fn titles(h: &H) -> Vec<String> {
    h.app.core.lock().cue.as_ref().unwrap().items.iter().map(|x| x["title"].as_str().unwrap_or("").to_string()).collect()
}
fn two_text() -> Vec<Value> {
    vec![wp_core::core::new_item("text", "가"), wp_core::core::new_item("text", "나")]
}

#[test]
fn drop_files_on_cue_adds_items_after_selected_in_order() {
    let _g = dnd::TEST_LOCK.lock();
    let d = tmpdir();
    for n in ["a.png", "b.mp3", "c.zip", "d.txt"] {
        std::fs::write(d.join(n), "x").unwrap();
    }
    let mut h = H::new(two_text());
    h.frame(vec![], draw);
    h.app.sel = Some(0);
    // 끌고 오는 중에는 안내 덮개를 그려도 오류가 없다
    frame_files(&mut h, Pos2::new(60.0, 400.0), &[d.join("a.png")], true, draw);
    frame_files(&mut h, Pos2::new(60.0, 800.0), &[d.join("a.png"), d.join("b.mp3"), d.join("c.zip"), d.join("d.txt")], false, draw);
    assert_eq!(types(&h), vec!["text", "image", "audio", "song", "text"], "zip 은 넣지 않고, 고른 순서(0) 다음에 차례로");
    assert_eq!(titles(&h)[1..4], ["a", "b", "d"]);
    let dropped = h.app.core.lock().settings.get()["droppedFiles"].as_array().map(|a| a.len()).unwrap_or(0);
    assert_eq!(dropped, 3, "넣은 파일은 '끌어다 놓은 파일' 로 기억 (예배당 화면에 보이게)");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn drop_on_a_cue_row_inserts_right_after_that_row() {
    let _g = dnd::TEST_LOCK.lock();
    let d = tmpdir();
    std::fs::write(d.join("a.png"), "x").unwrap();
    let mut h = H::new(two_text());
    h.frame(vec![], draw);
    h.frame(vec![], draw);
    h.app.sel = Some(1); // 고른 것은 마지막이지만, 놓은 곳은 첫 줄
    let p = seen("cue-row-0").expect("첫 줄이 그려져야 함").center();
    frame_files(&mut h, p, &[d.join("a.png")], false, draw);
    assert_eq!(types(&h), vec!["text", "image", "text"]);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn folder_on_library_panel_registers_it() {
    let _g = dnd::TEST_LOCK.lock();
    let d = tmpdir();
    let pics = d.join("사진");
    std::fs::create_dir_all(&pics).unwrap();
    std::fs::write(pics.join("a.png"), "x").unwrap();
    let mut h = H::new(two_text());
    h.frame(vec![], draw_lib);
    h.frame(vec![], draw_lib);
    frame_files(&mut h, Pos2::new(100.0, 100.0), &[pics.clone()], false, draw_lib);
    let n = wp_core::library::folders(h.app.core.lock().settings.get(), "image").len();
    assert_eq!(n, 1, "그림·영상 폴더로 등록");
    // 라이브러리 밖(큐시트 쪽)에 폴더를 놓으면 등록하지 않는다
    let before = types(&h).len();
    frame_files(&mut h, Pos2::new(100.0, 100.0), &[pics.clone()], false, draw);
    assert_eq!(wp_core::library::folders(h.app.core.lock().settings.get(), "image").len(), 1);
    assert_eq!(types(&h).len(), before);
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn bg_slideedit_and_slideshow_zones_follow_dropfiles_js() {
    let _g = dnd::TEST_LOCK.lock();
    let d = tmpdir();
    std::fs::write(d.join("a.png"), "x").unwrap();
    std::fs::write(d.join("b.png"), "x").unwrap();
    std::fs::write(d.join("c.mp3"), "x").unwrap();
    let mut h = H::new(two_text());
    let _ = (dnd::take_bg(), dnd::take_se_images(), dnd::take_slideshow());
    // 순서 배경 칸: 그림 하나 → 배경 (칸 자리는 한 프레임 뒤에 쓰이므로 두 프레임)
    let nop: super::auditc_tests::Draw = |app, ctx, _, _, _| { dnd::frame(app, ctx); dnd::zone(Zone::CueBg, Rect::from_min_size(Pos2::new(10.0, 10.0), Vec2::new(200.0, 30.0))); };
    h.frame(vec![], nop);
    frame_files(&mut h, Pos2::new(50.0, 20.0), &[d.join("a.png")], false, nop);
    let bg = dnd::take_bg().expect("배경으로 받음");
    assert!(bg.starts_with("/lib/file?p="));
    assert_eq!(types(&h), vec!["text", "text"], "큐시트에는 넣지 않는다");
    // 그림 둘이면 배경이 아니라 큐시트에
    h.frame(vec![], nop);
    frame_files(&mut h, Pos2::new(50.0, 20.0), &[d.join("a.png"), d.join("b.png")], false, nop);
    assert!(dnd::take_bg().is_none());
    assert_eq!(types(&h).iter().filter(|t| *t == "image").count(), 2);
    // 슬라이드 편집기: 그림·영상만 요소로, 음악은 안내만
    let se: super::auditc_tests::Draw = |app, ctx, _, _, _| { dnd::frame(app, ctx); dnd::zone(Zone::SlideEdit, Rect::from_min_size(Pos2::new(10.0, 10.0), Vec2::new(500.0, 400.0))); };
    h.frame(vec![], se);
    let n = types(&h).len();
    frame_files(&mut h, Pos2::new(100.0, 100.0), &[d.join("a.png"), d.join("c.mp3")], false, se);
    let imgs = dnd::take_se_images();
    assert_eq!(imgs.len(), 1);
    assert_eq!(types(&h).len(), n, "편집기에 놓은 것은 큐시트에 넣지 않는다");
    // 슬라이드쇼 편집칸: 그림은 그림 목록으로
    let ss: super::auditc_tests::Draw = |app, ctx, _, _, _| { dnd::frame(app, ctx); dnd::zone(Zone::Slideshow, Rect::from_min_size(Pos2::new(10.0, 10.0), Vec2::new(500.0, 400.0))); };
    h.frame(vec![], ss);
    frame_files(&mut h, Pos2::new(100.0, 100.0), &[d.join("a.png"), d.join("b.png")], false, ss);
    assert_eq!(dnd::take_slideshow().len(), 2);
    // 복사한 파일 붙여 넣기 (편집기가 열린 채): 같은 규칙으로 요소 대기열에
    dnd::push_pasted(h.app.core.lock().data_dir.clone(), dnd::Pasted::Files(vec![d.join("b.png")], true));
    h.frame(vec![], se);
    assert_eq!(dnd::take_se_images().len(), 1);
    // 복사한 파일 붙여 넣기 (큐시트): 고른 순서 다음에
    let m = types(&h).len();
    dnd::push_pasted(h.app.core.lock().data_dir.clone(), dnd::Pasted::Files(vec![d.join("c.mp3")], false));
    h.frame(vec![], draw);
    assert_eq!(types(&h).len(), m + 1);
    assert_eq!(types(&h).iter().filter(|t| *t == "audio").count(), 1);
    // 편집기 캡처 그림 붙여 넣기
    dnd::push_pasted(h.app.core.lock().data_dir.clone(), dnd::Pasted::EditorImage("/media/clip-1.png".into()));
    h.frame(vec![], se);
    assert_eq!(dnd::take_se_images(), vec![("/media/clip-1.png".to_string(), None)]);
    let _ = json!(null);
    let _ = std::fs::remove_dir_all(&d);
}