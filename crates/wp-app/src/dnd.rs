//! 탐색기에서 파일·폴더 끌어다 놓기 · 복사한 파일 Ctrl+V · 편집기 캡처 그림 붙여 넣기
//! (Electron control/dropfiles.js · paste.js). 어느 칸이 무엇을 받는지는 그 칸이 그려질 때 zone() 으로 자리를 남긴다.
//! 놓는 자리는 '지난 프레임'에 남긴 자리로 가린다 (같은 프레임에 그리면서 가릴 수 없으므로).
use crate::app::App;
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Stroke};
use std::cell::RefCell;
use std::path::PathBuf;
use wp_core::dropfiles::{self as df, Kind};

/// 파일을 받는 칸
#[derive(Clone, Debug, PartialEq)]
pub enum Zone {
    /// 슬라이드 편집기 창 (그림·영상을 요소로)
    SlideEdit,
    /// 라이브러리 패널 (kind = "image" | "audio"): 폴더 → 등록
    Lib(String),
    /// 찬송가·PPT 탭: 폴더 → 찬송가 폴더 등록
    Hymns,
    /// 찬양 라이브러리: 찬양 파일·폴더 가져오기
    Songs,
    /// 순서 편집칸의 '배경' (성경·찬양·공지·제목)
    CueBg,
    /// 슬라이드쇼 순서 편집칸: 그림 → 그림 목록
    Slideshow,
    /// 큐시트의 i 번째 순서 (그 순서 다음에 넣기)
    CueRow(usize),
}

thread_local! {
    static CUR: RefCell<Vec<(Zone, Rect)>> = const { RefCell::new(Vec::new()) };
    static PREV: RefCell<Vec<(Zone, Rect)>> = const { RefCell::new(Vec::new()) };
}

/// 붙여 넣기·놓기가 화면 그리는 쪽에 넘기는 일 (순서 편집칸·편집기가 다음 프레임에 가져간다)
#[derive(Default)]
struct Pending {
    bg: Option<String>,
    slideshow: Vec<String>,
    se_images: Vec<(String, Option<Pos2>)>,
    /// (요청한 앱의 자료 폴더, 결과) — 여러 앱(시험)이 한 프로세스에 있어도 자기 것만 꺼낸다
    pasted: Vec<(PathBuf, Pasted)>,
}
/// 백그라운드 스레드가 넣는 Ctrl+V 결과만 전역 (pasted). 나머지는 화면 스레드에서만 넣고 빼므로 스레드마다 따로
/// (시험이 여러 스레드에서 동시에 돌 때 다른 시험의 편집기가 그림을 가져가지 않게)
static PENDING: parking_lot::Mutex<Pending> = parking_lot::Mutex::new(Pending { bg: None, slideshow: Vec::new(), se_images: Vec::new(), pasted: Vec::new() });
thread_local! {
    static LOCAL: RefCell<Pending> = const { RefCell::new(Pending { bg: None, slideshow: Vec::new(), se_images: Vec::new(), pasted: Vec::new() }) };
}
fn local<R>(f: impl FnOnce(&mut Pending) -> R) -> R {
    LOCAL.with(|l| f(&mut l.borrow_mut()))
}

/// Ctrl+V 로 읽어 온 것 (백그라운드 스레드가 넣는다)
pub enum Pasted {
    /// 탐색기에서 복사한 파일·폴더
    Files(Vec<PathBuf>, bool),
    /// 편집기가 열린 채 붙인 캡처 그림 (/media/… 주소)
    EditorImage(String),
}

pub fn zone(z: Zone, r: Rect) {
    CUR.with(|c| c.borrow_mut().push((z, r)));
}
pub fn push_pasted(data_dir: PathBuf, p: Pasted) {
    PENDING.lock().pasted.push((data_dir, p));
}
/// 시험용: 편집기가 받을 그림을 직접 넣는다
#[cfg(test)]
pub fn push_se_image(url: &str, at: Option<Pos2>) {
    local(|p| p.se_images.push((url.to_string(), at)));
}
/// 시험용: PENDING 은 전역이라 그것을 보는 시험은 차례로 돌린다
#[cfg(test)]
pub static TEST_LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());
pub fn take_bg() -> Option<String> {
    local(|p| p.bg.take())
}
pub fn take_slideshow() -> Vec<String> {
    local(|p| std::mem::take(&mut p.slideshow))
}
pub fn take_se_images() -> Vec<(String, Option<Pos2>)> {
    local(|p| std::mem::take(&mut p.se_images))
}

/// 이 자리에 걸치는 칸들 (위에 그려진 것부터)
fn zones_at(p: Option<Pos2>) -> Vec<Zone> {
    let Some(p) = p else { return vec![] };
    PREV.with(|v| v.borrow().iter().rev().filter(|(_, r)| r.contains(p)).map(|(z, _)| z.clone()).collect())
}

/// 놓을 자리에 따른 안내 글 · 그 자리의 칸 이름
fn hint_for(zs: &[Zone]) -> &'static str {
    if zs.contains(&Zone::SlideEdit) {
        return df::hint("slideedit");
    }
    for z in zs {
        match z {
            Zone::Songs => return df::hint("songs"),
            Zone::Hymns => return df::hint("hymns"),
            Zone::Lib(_) => return df::hint("lib"),
            Zone::CueBg => return df::hint("bg"),
            _ => {}
        }
    }
    df::hint("cue")
}

/// 한 프레임에 한 번, 조작 화면 맨 앞에서 부른다: 지난 프레임의 칸 자리 가져오기 · 끌고 있는 동안 안내 · 놓은 파일 처리 · 붙여 넣은 파일 처리
pub fn frame(app: &mut App, ctx: &egui::Context) {
    PREV.with(|p| CUR.with(|c| *p.borrow_mut() = std::mem::take(&mut *c.borrow_mut())));
    let (hovering, dropped, pos) = ctx.input(|i| {
        (
            !i.raw.hovered_files.is_empty(),
            i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect::<Vec<_>>(),
            i.pointer.latest_pos(),
        )
    });
    if hovering {
        overlay(ctx, pos);
    }
    if !dropped.is_empty() {
        handle(app, dropped, pos, false);
    }
    let mine = app.core.lock().data_dir.clone();
    let pasted: Vec<Pasted> = {
        let mut g = PENDING.lock();
        let (take, keep): (Vec<_>, Vec<_>) = std::mem::take(&mut g.pasted).into_iter().partition(|(d, _)| *d == mine);
        g.pasted = keep;
        take.into_iter().map(|(_, p)| p).collect()
    };
    for p in pasted {
        match p {
            Pasted::Files(paths, editor) => handle(app, paths, None, editor),
            Pasted::EditorImage(url) => local(|p| p.se_images.push((url, None))),
        }
    }
}

fn overlay(ctx: &egui::Context, pos: Option<Pos2>) {
    let zs = zones_at(pos);
    let sc = ctx.content_rect();
    let pt = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("dnd-overlay")));
    pt.rect_filled(sc, 0.0, Color32::from_black_alpha(110));
    // 큐시트 순서 위: 그 아래에 들어간다는 표시
    if let Some(Zone::CueRow(i)) = zs.iter().find(|z| matches!(z, Zone::CueRow(_))).filter(|_| !zs.contains(&Zone::SlideEdit)) {
        if let Some(r) = PREV.with(|v| v.borrow().iter().rev().find(|(z, _)| *z == Zone::CueRow(*i)).map(|(_, r)| *r)) {
            pt.line_segment([r.left_bottom(), r.right_bottom()], Stroke::new(3.0, Color32::from_rgb(0x4d, 0xa3, 0xff)));
        }
    }
    let text = format!("⬇ 여기에 놓기 — {}", wp_core::i18n::tr(hint_for(&zs)));
    let font = FontId::proportional(18.0);
    let g = pt.layout_no_wrap(text, font, Color32::WHITE);
    let r = Rect::from_center_size(sc.center_top() + egui::vec2(0.0, 80.0), g.size() + egui::vec2(40.0, 24.0));
    pt.rect(r, 8.0, Color32::from_rgb(0x1d, 0x3a, 0x5f), Stroke::new(2.0, Color32::from_rgb(0x4d, 0xa3, 0xff)), egui::StrokeKind::Inside);
    pt.galley(r.center() - g.size() / 2.0, g, Color32::WHITE);
    let _ = Align2::CENTER_CENTER;
}

/// 놓은(붙여 넣은) 파일·폴더를 놓은 자리에 맞게 넣는다 (dropfiles.js addPaths).
/// pos = 놓은 자리 (붙여 넣기는 None) · editor = 붙여 넣기인데 슬라이드 편집기가 열려 있었다
pub fn handle(app: &mut App, paths: Vec<PathBuf>, pos: Option<Pos2>, editor: bool) {
    let zs = zones_at(pos);
    let list = df::describe(&paths);
    if list.is_empty() {
        return;
    }
    // 폴더 밖 파일도 예배당 화면에 보이고 열리게 '끌어다 놓은 파일' 로 기억한다 (core addDroppedFiles)
    let regs: Vec<PathBuf> = list.iter().filter(|d| df::is_cue_kind(d.kind)).map(|d| d.path.clone()).collect();
    if !regs.is_empty() {
        app.core.lock().add_dropped_files(&regs);
    }
    let note = |app: &App, m: String, k: &'static str| app.core.lock().notify(m, k);

    // 슬라이드 편집기: 그림·영상 덧붙이기
    if editor || zs.contains(&Zone::SlideEdit) {
        let media: Vec<_> = list.iter().filter(|d| df::is_media(d.kind)).collect();
        if media.is_empty() {
            return note(app, "슬라이드 편집기에는 그림·영상을 놓을 수 있습니다.".into(), "info");
        }
        local(|p| {
            for (k, m) in media.iter().enumerate() {
                let at = pos.map(|q| q + egui::vec2(20.0, 20.0) * k as f32);
                p.se_images.push((df::url_of(m), at));
            }
        });
        return;
    }
    // 찬양 라이브러리: 찬양 파일·폴더 가져오기
    if zs.contains(&Zone::Songs) {
        let fs: Vec<PathBuf> = list.iter().filter(|d| d.kind != Kind::Missing).map(|d| d.path.clone()).collect();
        app.import_song_paths(&fs);
        return;
    }
    // 폴더 → 라이브러리·찬송가 폴더 등록
    let folders: Vec<_> = list.iter().filter(|d| d.kind == Kind::Folder).collect();
    if !folders.is_empty() {
        let lib = zs.iter().find_map(|z| if let Zone::Lib(k) = z { Some(k.clone()) } else { None });
        if lib.is_none() && !zs.contains(&Zone::Hymns) {
            note(app, "폴더는 라이브러리 패널(그림·영상 / 음악)이나 찬송가 탭에 놓으면 등록됩니다.".into(), "info");
        }
        for f in folders {
            if let Some(kind) = &lib {
                let r = app.core.lock().add_lib_folder(kind, &f.path);
                let (m, k) = match r {
                    Ok(_) => (format!("'{}' 폴더를 {} 폴더로 등록했습니다.", f.name, if kind == "audio" { "음악" } else { "그림·영상" }), "ok"),
                    Err(e) => (e.to_string(), "error"),
                };
                note(app, m, k);
            } else if zs.contains(&Zone::Hymns) {
                let r = wp_core::hymns::add_folder(&mut app.core.lock(), &f.path, Some(&f.name));
                let (m, k) = match r {
                    Ok(v) => (format!("'{}' 폴더를 찬송가·PPT 폴더로 등록했습니다 ({}개)", v["name"].as_str().unwrap_or(&f.name), v["count"]), "ok"),
                    Err(e) => (e, "error"),
                };
                note(app, m, k);
            }
        }
    }
    let mut files: Vec<_> = list.iter().filter(|d| df::is_cue_kind(d.kind)).collect();
    // 슬라이드쇼 순서 편집칸에 그림들: 그림 추가
    if zs.contains(&Zone::Slideshow) {
        let imgs: Vec<String> = files.iter().filter(|d| d.kind == Kind::Image).map(|d| df::url_of(d)).collect();
        if !imgs.is_empty() {
            note(app, format!("슬라이드쇼에 그림 {}장을 넣었습니다.", imgs.len()), "ok");
            local(|p| p.slideshow.extend(imgs));
            files.retain(|d| d.kind != Kind::Image);
            if files.is_empty() {
                return;
            }
        }
    }
    // 순서 편집칸에 그림·영상 하나: 그 순서 배경
    if zs.contains(&Zone::CueBg) && files.len() == 1 && df::is_media(files[0].kind) {
        local(|p| p.bg = Some(df::url_of(files[0])));
        note(app, format!("배경: {}", files[0].name), "ok");
        return;
    }
    let bad = df::rejected(&list);
    let bad_msg = if bad.is_empty() { String::new() } else { format!("넣을 수 없는 파일: {} (PPT·그림·영상·음악·txt 만)", bad.join(", ")) };
    if files.is_empty() {
        if !bad_msg.is_empty() {
            note(app, bad_msg, "info");
        }
        return;
    }
    if app.core.lock().cue.is_none() {
        return note(app, "큐시트를 먼저 열거나 만드세요.".into(), "info");
    }
    // 큐시트 순서 위에 놓았으면 그 다음 자리에, 아니면 고른 순서 다음에 차례로
    if let Some(Zone::CueRow(i)) = zs.iter().find(|z| matches!(z, Zone::CueRow(_))) {
        app.sel = Some(*i);
    }
    let names: Vec<String> = files.iter().map(|d| d.name.clone()).collect();
    for d in &files {
        if let Some(it) = df::cue_item(d) {
            app.add_to_cue(it);
        }
    }
    note(app, format!("큐시트에 넣었습니다: {}{}", names.join(" · "), if bad_msg.is_empty() { String::new() } else { format!(" — {bad_msg}") }), "ok");
}