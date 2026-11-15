//! 가운데 본문 패널 — Electron `index.html #panelMain`(.center-head · #results · #cmView),
//! `grid.js`(목록 | 격자 | 전체 격자 | 전체 목록), `cue.js showSlideList`, `control.js showPassageList·markResults·renderCommentary` 의 모양과 동작.
//!
//! 상태(본문/주석 탭, 보기 방식, 썸네일 크기, 전체 보기 캐시)는 egui 임시 메모리에 둔다 (App 구조체를 늘리지 않으려고).
use crate::app::{Act, App, Snap};
use crate::icons;
use crate::paint;
use crate::theme::{self, pal, rich, Weight};
use crate::widgets::{self, Btn, Size};
use eframe::egui::{self, Color32, CornerRadius, Margin, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Vec2};
use serde_json::{json, Value};
use std::sync::Arc;
use wp_core::i18n::{tr, tr_args};
use wp_core::presentation::{Deck, Live};

/// 슬라이드 보기 방식 (grid.js mode: list · grid · all · alllist)
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum SlideView {
    #[default]
    List,
    Grid,
    All,
    AllList,
}

#[derive(Clone)]
struct AllSec {
    deck: Result<Deck, String>,
    tpl: Value,
}

#[derive(Clone, Default)]
pub(crate) struct CenterState {
    init: bool,
    /// '주석' 탭
    cm: bool,
    view: SlideView,
    /// 썸네일 크기 (150~420, 기본 240)
    size: f32,
    /// 가운데에 큐시트 순서의 슬라이드를 보이는 중 (검색 결과 대신)
    cue_mode: bool,
    seen_preview: String,
    seen_search: String,
    scroll_key: String,
    all_key: String,
    all: Arc<Vec<AllSec>>,
    cm_key: String,
    cm_rows: Arc<Vec<Value>>,
    cm_installed: bool,
    cm_open: Vec<usize>,
    /// 본문 목록에 키보드가 있음 (Electron #results 포커스: 줄을 누르거나 검색칸에서 ↓)
    kbd: bool,
    /// 화살표로 고른 줄 (app.results.sel)
    sel: Option<usize>,
    /// Shift 범위의 기준 줄과 범위 (app.results.anchor · range)
    anchor: Option<usize>,
    range: Option<(usize, usize)>,
    /// 목록이 바뀌면 고른 줄·범위를 초기화
    res_key: String,
    /// 고른 줄이 보이게 스크롤
    scroll_sel: bool,
    /// 본문 목록 자리 (밖을 누르면 키보드를 놓는다)
    body_rect: Option<Rect>,
    /// 여러 장 고르기 (slideedit.js M): 고른 순서 (큐시트 id, 순서 번호) · 고른 슬라이드(미리보기 차례) · Shift 기준
    ms_item: Option<(String, usize)>,
    ms: std::collections::BTreeSet<usize>,
    ms_anchor: Option<usize>,
    /// 끌어서 차례 바꾸기 (M.drag)
    drag: Option<SlideDrag>,
    /// 옮긴 뒤 새 자리에서 다시 고를 슬라이드 (순서 번호, ekey 들)
    ms_after: Option<(usize, Vec<String>)>,
    /// 점검용: --msel 0-2 (처음 미리보기한 순서에서 그 범위를 고른 채로)
    ms_init: Option<(usize, usize)>,
}

/// 끌어서 차례 바꾸기 중 (slideedit.js attachSel)
#[derive(Clone, Debug)]
struct SlideDrag {
    /// 순서 번호
    item: usize,
    /// 옮길 슬라이드 (미리보기 차례): 고른 것 중 하나를 끌면 고른 것 모두
    ks: Vec<usize>,
    /// 누른 자리 (8px 넘게 움직여야 끌기)
    start: Pos2,
    moved: bool,
    /// 놓을 자리 (슬라이드, 뒤에)
    to: Option<(usize, bool)>,
}

/// 미리보기가 열린 큐시트의 순서이면 (순서 번호, 미리보기 차례, 슬라이드 수)
fn pv_item(s: &Snap) -> Option<(usize, usize, usize)> {
    let d = s.preview.as_ref()?;
    deck_item(s, d).map(|(i, _)| (i, d.index, d.slides.len()))
}

/// 고른 것이 지금 미리보기 중인 순서의 것인가 (selValid)
fn ms_valid(st: &CenterState, s: &Snap) -> bool {
    match (pv_item(s), &st.ms_item) {
        (Some((i, _, _)), Some((id, j))) => *j == i && *id == s.cue_id,
        _ => false,
    }
}

/// 고르기를 한 장(또는 없음)으로 (selReset)
fn ms_reset(st: &mut CenterState, s: &Snap, k: Option<usize>) {
    st.ms_item = pv_item(s).map(|(i, _, _)| (s.cue_id.clone(), i));
    st.ms = k.into_iter().collect();
    st.ms_anchor = k;
}

/// 여러 장을 고른 중이면 (순서 번호, 고른 슬라이드 차례) (selMany)
fn ms_many(st: &CenterState, s: &Snap) -> Option<(usize, Vec<usize>)> {
    let (i, _, n) = pv_item(s)?;
    if !ms_valid(st, s) {
        return None;
    }
    let ks: Vec<usize> = st.ms.iter().copied().filter(|k| *k < n).collect();
    (ks.len() > 1).then_some((i, ks))
}

/// 목록·격자 누르기 (selClick): Shift = 기준부터 범위 · Ctrl = 더하기/빼기. true 면 고르기로 썼음 (미리보기 바꾸지 않음)
fn ms_click(st: &mut CenterState, s: &Snap, k: usize, m: egui::Modifiers) -> bool {
    let Some((_, pv, _)) = pv_item(s) else { return false };
    if !ms_valid(st, s) {
        ms_reset(st, s, Some(pv));
    }
    if m.shift {
        let a = st.ms_anchor.unwrap_or(k);
        st.ms = (a.min(k)..=a.max(k)).collect();
        return true;
    }
    if m.command {
        if st.ms.contains(&k) && st.ms.len() > 1 {
            st.ms.remove(&k);
        } else {
            st.ms.insert(k);
        }
        return true;
    }
    ms_reset(st, s, Some(k));
    false
}

/// 전체 격자·전체 목록 누르기 (selClickAll): 미리보기 중인 순서의 칸만 고르기, 다른 순서면 거기서 새로
fn ms_click_all(st: &mut CenterState, s: &Snap, i: usize, k: usize, m: egui::Modifiers) -> bool {
    if pv_item(s).map(|x| x.0) != Some(i) {
        st.ms_item = Some((s.cue_id.clone(), i));
        st.ms = [k].into_iter().collect();
        st.ms_anchor = Some(k);
        return false;
    }
    ms_click(st, s, k, m)
}

/// 고른 슬라이드 → 편집 키 (겹친 것은 하나로, 목록 차례대로)
fn ms_keys(d: &Deck, ks: &[usize]) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for k in ks {
        if let Some(sl) = d.slides.get(*k) {
            let key = slide_key(sl);
            if !out.contains(&key) {
                out.push(key);
            }
        }
    }
    out
}

/// 끌기 시작: 고른 것 중 하나면 고른 것 모두
fn drag_begin(ui: &egui::Ui, st: &mut CenterState, s: &Snap, item: usize, k: usize) {
    let m = ui.input(|i| i.modifiers);
    if m.shift || m.command || pv_item(s).map(|x| x.0) != Some(item) {
        return;
    }
    let start = ui.input(|i| i.pointer.press_origin().or(i.pointer.interact_pos())).unwrap_or_default();
    let ks = match ms_many(st, s) {
        Some((i, ks)) if i == item && ks.contains(&k) => ks,
        _ => vec![k],
    };
    st.drag = Some(SlideDrag { item, ks, start, moved: false, to: None });
}

/// 끌기 중: 놓을 자리(가장자리면 저절로 넘김)·표시선, 놓으면 차례 바꾸기.
/// `cells` 는 그 순서의 (슬라이드, 칸), `grid` 면 좌우로 앞뒤 (목록은 위아래)
fn drag_update(ui: &egui::Ui, st: &mut CenterState, s: &Snap, item: usize, cells: &[(usize, Rect)], grid: bool, acts: &mut Vec<Act>) {
    let Some(mut d) = st.drag.clone() else { return };
    if d.item != item {
        return;
    }
    let (pos, down) = ui.input(|i| (i.pointer.interact_pos(), i.pointer.primary_down()));
    if let Some(p) = pos.filter(|_| down) {
        if !d.moved && (p.x - d.start.x).abs() + (p.y - d.start.y).abs() >= 8.0 {
            d.moved = true;
        }
        if d.moved {
            let pl = pal();
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
            let pt = ui.painter();
            for (k, r) in cells {
                if d.ks.contains(k) {
                    // .drag-src { opacity: .4 }
                    pt.rect_filled(*r, CornerRadius::same(4), theme::alpha(pl.panel, 0.6));
                }
            }
            d.to = None;
            if let Some((k, r)) = cells.iter().find(|(k, r)| r.contains(p) && !d.ks.contains(k)) {
                let after = if grid { p.x >= r.center().x } else { p.y >= r.center().y };
                d.to = Some((*k, after));
                // .drop-before / .drop-after: 격자는 칸 옆 세로 막대, 목록은 줄 위·아래 선 (accent)
                let bar = if grid {
                    let x = if after { r.right() + 5.0 } else { r.left() - 5.0 };
                    Rect::from_min_max(Pos2::new(x - 2.0, r.top() - 3.0), Pos2::new(x + 2.0, r.bottom() + 3.0))
                } else {
                    let y = if after { r.bottom() } else { r.top() };
                    Rect::from_min_max(Pos2::new(r.left(), y - 1.5), Pos2::new(r.right(), y + 1.5))
                };
                pt.rect_filled(bar, CornerRadius::same(2), pl.accent);
                #[cfg(test)]
                widgets::HOVER_SEEN.with(|m| m.borrow_mut().insert("drop-mark".into(), bar));
            }
            // 가장자리로 끌면 저절로 넘김
            let clip = ui.clip_rect();
            if p.y > clip.bottom() - 30.0 {
                ui.scroll_with_delta(Vec2::new(0.0, -18.0));
            } else if p.y < clip.top() + 30.0 {
                ui.scroll_with_delta(Vec2::new(0.0, 18.0));
            }
            ui.ctx().request_repaint();
        }
        st.drag = Some(d);
        return;
    }
    // 놓음
    st.drag = None;
    let (true, Some((t, after)), Some(dk)) = (d.moved, d.to, s.preview.as_ref()) else { return };
    let mut ks = d.ks.clone();
    ks.sort();
    let moved = ms_keys(dk, &ks);
    let Some(target) = dk.slides.get(t).map(slide_key) else { return };
    if moved.is_empty() || moved.contains(&target) {
        return;
    }
    // 옮긴 뒤 새 키: 글 덩어리(찬양·공지·제목)는 미리보기 키가 곳 덩어리 번호다
    let chunk = s.items.get(item).is_some_and(|it| wp_core::slideedit::is_chunk(it["type"].as_str().unwrap_or("")));
    let all = ms_keys(dk, &(0..dk.slides.len()).collect::<Vec<_>>());
    let after_keys = wp_core::slideedit::keys_after_move(chunk, &all, &moved, &target, after);
    acts.push(Act::SlideMove(item, moved, target, after));
    st.ms_after = Some((item, after_keys));
}

/// 고른 슬라이드 메뉴 (ctxmenu.js slideMenu 의 selMany 갈래): 'edit' · 'del' · 'clear'
fn multi_menu(resp: &Response, n: usize, chunk: bool) -> Option<&'static str> {
    let mut out = None;
    resp.context_menu(|ui| {
        ui.set_min_width(230.0);
        ui.label(rich(tr_args("고른 슬라이드 {1}장", &[&n]), 12.0, Weight::SemiBold, pal().muted));
        if menu_btn(ui, Some("square-pen"), &tr_args("고른 {1}장 편집 (함께 꾸미기)", &[&n]), "Ctrl+E", true, false) {
            out = Some("edit");
        }
        let (ic, l) = if chunk { ("trash-2", tr_args("고른 {1}장 지우기", &[&n])) } else { ("eye-off", tr_args("고른 {1}장 건너뛰기", &[&n])) };
        if menu_btn(ui, Some(ic), &l, "Delete", true, true) {
            out = Some("del");
        }
        ui.separator();
        if menu_btn(ui, Some("x"), &tr("선택 해제"), "Esc", true, false) {
            out = Some("clear");
        }
    });
    out
}

/// 고른 슬라이드 지우기 확인 (글 순서: 가사·내용에서도 지워짐)
fn ms_confirm_id() -> egui::Id {
    egui::Id::new("msel-del-confirm")
}

/// 목록이 바뀌었는지 보는 열쇠 (검색 결과)
fn res_key_of(r: &Value, n: usize) -> String {
    format!("{}|{}|{}|{}", r["mode"], r["passage"]["title"], r["passage"]["versionName"], n)
}

/// 목록에서 누른 키 (control.js #results keydown)
enum ListKey {
    /// ↑↓: 고르기만 (미리보기 안 바꿈)
    Move(usize),
    /// Shift+↑↓: 범위 늘리기·줄이기
    Range(usize),
    /// Enter: 고른 줄 미리보기
    Activate(usize),
    /// 맨 위에서 ↑: 검색칸으로
    ToSearch,
}

/// 목록 키 읽기 (키보드가 목록에 있고 글 쓰는 칸이 아닐 때). n = 줄 수, range = Shift 범위를 쓰는 목록인가
fn list_keys(ctx: &egui::Context, st: &mut CenterState, n: usize, range: bool) -> Option<ListKey> {
    use egui::{Key, Modifiers};
    if !st.kbd || n == 0 || ctx.egui_wants_keyboard_input() {
        return None;
    }
    let cur = st.sel;
    let (down, up, sdown, sup, enter) = ctx.input_mut(|i| {
        (
            i.consume_key(Modifiers::NONE, Key::ArrowDown),
            i.consume_key(Modifiers::NONE, Key::ArrowUp),
            range && i.consume_key(Modifiers::SHIFT, Key::ArrowDown),
            range && i.consume_key(Modifiers::SHIFT, Key::ArrowUp),
            i.consume_key(Modifiers::NONE, Key::Enter),
        )
    });
    if sdown || sup {
        let s = cur.unwrap_or(0);
        if st.anchor.is_none() {
            st.anchor = Some(s);
        }
        let to = if sdown { (s + 1).min(n - 1) } else { s.saturating_sub(1) };
        return Some(ListKey::Range(to));
    }
    if down || up {
        st.anchor = None; // 일반 이동은 범위 기준점 초기화
    }
    if down {
        return Some(ListKey::Move(cur.map(|s| (s + 1).min(n - 1)).unwrap_or(0)));
    }
    if up {
        return Some(match cur {
            Some(s) if s > 0 => ListKey::Move(s - 1),
            _ => ListKey::ToSearch,
        });
    }
    if enter {
        return cur.filter(|s| *s < n).map(ListKey::Activate);
    }
    None
}

/// 검색칸에서 ↓: 키보드를 본문 목록으로 (고른 줄이 없으면 첫 줄)
pub(crate) fn focus_results(ctx: &egui::Context) {
    let mut st = load(ctx);
    st.kbd = true;
    if st.sel.is_none() {
        st.sel = Some(0);
    }
    st.scroll_sel = true;
    save(ctx, st);
}

/// 키보드가 본문 목록에 있는가 (있으면 전역 Enter=송출을 하지 않는다 — control.js 가 #results 를 뺀다)
pub(crate) fn results_focused(ctx: &egui::Context) -> bool {
    load(ctx).kbd
}

/// Shift 범위 → 그 절들만 담은 본문 (control.js rangePassage). (본문, 짧은 장절 '시23:2-4')
fn range_passage(full: &Value, a: usize, b: usize) -> Option<(Value, String)> {
    let verses: Vec<Value> = full["verses"].as_array()?.get(a..=b)?.to_vec();
    let (f, l) = (verses.first()?, verses.last()?);
    let n = |v: &Value, k: &str| v[k].as_i64().unwrap_or(0);
    let span = if n(f, "chapter") == n(l, "chapter") {
        if n(f, "verse") == n(l, "verse") { format!("{}:{}", n(f, "chapter"), n(f, "verse")) } else { format!("{}:{}-{}", n(f, "chapter"), n(f, "verse"), n(l, "verse")) }
    } else {
        format!("{}:{}-{}:{}", n(f, "chapter"), n(f, "verse"), n(l, "chapter"), n(l, "verse"))
    };
    let keys: Vec<(i64, i64)> = verses.iter().map(|v| (n(v, "chapter"), n(v, "verse"))).collect();
    let mut sub = full.clone();
    sub["title"] = json!(format!("{} {}", full["book"]["name"].as_str().unwrap_or(""), span));
    let short = format!("{}{}", full["book"]["short"].as_str().unwrap_or(""), span);
    sub["short"] = json!(short);
    sub["headings"] = json!(full["headings"].as_array().into_iter().flatten().filter(|h| keys.contains(&(n(h, "chapter"), n(h, "verse")))).cloned().collect::<Vec<_>>());
    sub["ranges"] = json!([{ "sc": n(f, "chapter"), "sv": n(f, "verse"), "ec": n(l, "chapter"), "ev": n(l, "verse") }]);
    sub["verses"] = json!(verses);
    Some((sub, short))
}

fn sid() -> egui::Id {
    egui::Id::new("center-panel-state")
}

fn load(ctx: &egui::Context) -> CenterState {
    let mut st: CenterState = ctx.data(|d| d.get_temp(sid())).unwrap_or_default();
    if !st.init {
        st.init = true;
        st.size = 240.0;
        st.cm_key = "\u{0}".into();
        // 점검용: --slide-view grid|all|alllist · --center-tab cm
        let args: Vec<String> = std::env::args().collect();
        let arg = |k: &str| args.iter().position(|x| x == k).and_then(|i| args.get(i + 1)).cloned();
        st.view = match arg("--slide-view").as_deref() {
            Some("grid") => SlideView::Grid,
            Some("all") => SlideView::All,
            Some("alllist") => SlideView::AllList,
            _ => SlideView::List,
        };
        st.cm = arg("--center-tab").as_deref() == Some("cm");
        st.ms_init = arg("--msel").and_then(|v| {
            let (a, b) = v.split_once('-')?;
            Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
        });
    }
    st
}

fn save(ctx: &egui::Context, st: CenterState) {
    ctx.data_mut(|d| d.insert_temp(sid(), st));
}

/// 시험용: 보기 방식을 정한다 (격자·전체 격자·목록)
#[cfg(test)]
pub(crate) fn test_set_view(ctx: &egui::Context, v: SlideView, cue_mode: bool) {
    let mut st = load(ctx);
    st.view = v;
    st.cue_mode = cue_mode;
    save(ctx, st);
}

/// 시험용: 여러 장 고르기 상태 (고른 슬라이드 차례)
#[cfg(test)]
pub(crate) fn test_msel(ctx: &egui::Context) -> Vec<usize> {
    load(ctx).ms.iter().copied().collect()
}

/// cue.js TYPE (종류 이름)
pub(crate) fn type_name(t: &str) -> &'static str {
    match t {
        "camera" => "카메라",
        "slideshow" => "슬라이드쇼",
        "video" => "영상",
        "audio" => "음악",
        "hymn" => "찬송가·PPT",
        "bible" => "성경",
        "song" => "찬양",
        "text" => "공지",
        "title" => "제목",
        "image" => "이미지",
        "blank" => "빈 화면",
        _ => "순서",
    }
}

fn cue_index(d: &Deck) -> Option<usize> {
    d.passage["cueIndex"].as_i64().filter(|i| *i >= 0).map(|i| i as usize)
}

/// 절 열쇠 (책이 있으면 책까지): markResults 의 book:chapter:verse
fn vkey(book: i64, ch: i64, v: i64) -> (i64, i64, i64) {
    (book, ch, v)
}

fn same_verse(a: (i64, i64, i64), b: (i64, i64, i64)) -> bool {
    a.1 == b.1 && a.2 == b.2 && (a.0 == 0 || b.0 == 0 || a.0 == b.0)
}

fn slide_keys(s: Option<&Value>) -> Vec<(i64, i64, i64)> {
    s.and_then(|s| s["parts"].as_array())
        .map(|ps| ps.iter().map(|p| vkey(p["book"].as_i64().unwrap_or(0), p["chapter"].as_i64().unwrap_or(0), p["verse"].as_i64().unwrap_or(0))).collect())
        .unwrap_or_default()
}

/// grid.js slideText: 전체 목록 한 줄의 글. 그림·영상·빈 화면처럼 글이 없는 슬라이드는 (아이콘, 이름)
/// — Electron 은 🖼·🎬·⬛ 이모지, 여기서는 같은 자리에 선 아이콘 (docs/design-parity.md: 이모지를 쓰지 않는다)
fn slide_text(s: &Value) -> (Option<&'static str>, String) {
    let kind = s["kind"].as_str().unwrap_or("");
    let st = |k: &str| s[k].as_str().unwrap_or("").to_string();
    match kind {
        "title" => (None, [st("ref"), st("subtitle")].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · ")),
        "image" | "score" => {
            let c = st("caption");
            if !c.is_empty() { (None, c) } else if kind == "score" { (Some(icons::for_type("score")), tr("악보")) } else { (Some(icons::for_type("image")), tr("이미지")) }
        }
        "video" => (Some(icons::for_type("video")), tr("영상")),
        "camera" => (Some(icons::for_type("camera")), tr("카메라")),
        "blank" => (Some("square"), tr(if s["mode"] == "background" { "배경·로고만" } else { "검은 화면" })),
        _ => {
            let parts = s["parts"].as_array().cloned().unwrap_or_default();
            let text = kind == "text";
            let plain = |t: &str| wp_core::groups::strip_fmt(&wp_core::groups::strip_chords(t));
            let t = parts
                .iter()
                .map(|p| {
                    let t = plain(p["text"].as_str().unwrap_or(""));
                    if text || p["verse"].as_i64().unwrap_or(0) == 0 { t } else { format!("{} {}", p["verse"], t) }
                })
                .collect::<Vec<_>>()
                .join(if text { "\n" } else { " " })
                .trim()
                .to_string();
            (None, t)
        }
    }
}

/// 슬라이드 목록(.srow) 한 줄의 내용 (cue.js showSlideList)
enum SrowBody {
    /// 찬송가·악보 그림: 작은 그림 + OBS 가사 (없으면 None → 흐린 안내)
    Thumb(String, Option<String>),
    /// 글 (아이콘은 그림·영상·빈 화면 자리표시)
    Text(Option<&'static str>, String),
}

fn srow_body(s: &Value, item_type: &str) -> SrowBody {
    let kind = s["kind"].as_str().unwrap_or("");
    let st = |k: &str| s[k].as_str().unwrap_or("").to_string();
    if kind == "score" || (kind == "image" && item_type == "hymn") {
        let src = Some(st("thumb")).filter(|x| !x.is_empty()).unwrap_or_else(|| st("image"));
        return SrowBody::Thumb(src, Some(st("caption")).filter(|x| !x.is_empty()));
    }
    match kind {
        // 공지·찬양 가사: 첫 글 조각 (서식 표시는 뺀다)
        "text" => SrowBody::Text(None, wp_core::groups::strip_fmt(s["parts"][0]["text"].as_str().unwrap_or(""))),
        "title" => SrowBody::Text(None, [st("ref"), st("subtitle")].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join("\n")),
        "image" => SrowBody::Text(Some(icons::for_type("image")), tr("이미지")),
        "video" => SrowBody::Text(Some(icons::for_type("video")), tr("영상")),
        "camera" => SrowBody::Text(Some(icons::for_type("camera")), tr("카메라")),
        _ => SrowBody::Text(Some("square"), tr(if s["mode"] == "background" { "배경·로고만" } else { "검은 화면" })),
    }
}

/// 이름표 색 (BCGroup.color — 성경 슬라이드는 없음)
fn group_color(s: &Value) -> Option<Color32> {
    (s["kind"] != "bible").then(|| wp_core::groups::color(s["label"].as_str().unwrap_or(""))).flatten().map(theme::hex)
}

/// 슬라이드 이름 (grid.js: 숫자뿐인 이름은 빼고, 성경은 장절)
fn slide_label(s: &Value) -> String {
    let l = s["label"].as_str().unwrap_or("");
    if !l.is_empty() && !l.chars().all(|c| c.is_ascii_digit()) {
        l.to_string()
    } else if s["kind"] == "bible" || s["kind"].is_null() {
        s["ref"].as_str().unwrap_or("").to_string()
    } else {
        String::new()
    }
}

// ------------------------------------------------------------------ 그리기 조각

/// 줄 너비: 남은 너비와 보이는 영역 중 작은 쪽 (다른 줄이 패널보다 넓어도 글이 잘리지 않게)
fn row_width(ui: &egui::Ui) -> f32 {
    ui.available_width().min(ui.clip_rect().right() - ui.cursor().left()).max(40.0)
}

/// 글 배치 (줄 높이·줄바꿈 너비)
fn job(ui: &egui::Ui, text: &str, size: f32, w: Weight, color: Color32, line_h: f32, wrap: f32) -> Arc<egui::Galley> {
    let mut j = egui::text::LayoutJob::single_section(text.to_string(), egui::TextFormat { font_id: theme::font(size, w), color, line_height: Some(size * line_h), ..Default::default() });
    j.wrap.max_width = wrap.max(20.0);
    ui.painter().layout_job(j)
}

/// 결과 줄 표시 (control.js markResults: in-pv · on-air · sel · in-range)
#[derive(Clone, Copy, Default)]
struct Mark {
    pv: bool,
    live: bool,
    /// 화살표로 고른 줄 (.sel)
    sel: bool,
    /// Shift 범위 안 (.in-range)
    range: bool,
}

/// .results .row 바탕과 띠: hover < sel, in-range(범위) — 범위 안에서 고른 줄은 sel. 왼쪽 4px pv 띠, 오른쪽 4px live 띠
fn paint_row_marks(ui: &egui::Ui, rect: Rect, m: Mark, hovered: bool) {
    let p = pal();
    let pt = ui.painter();
    let bg = if m.sel { Some(p.sel) } else if m.range { Some(p.accent_bg) } else if hovered { Some(p.hover) } else { None };
    if let Some(bg) = bg {
        pt.rect_filled(rect, CornerRadius::ZERO, bg);
    }
    if m.pv {
        pt.rect_filled(Rect::from_min_size(rect.min, Vec2::new(4.0, rect.height())), CornerRadius::ZERO, p.pv);
    }
    if m.live {
        pt.rect_filled(Rect::from_min_max(Pos2::new(rect.right() - 4.0, rect.top()), rect.max), CornerRadius::ZERO, p.live);
    }
}

/// .row 왼쪽: 4px 테두리 + 12px 안쪽 여백
const ROW_PAD_L: f32 = 16.0;

/// .results .row.prow — 절 번호 칸(3.2em, hl 13px 700) + 16px 본문(줄 높이 1.6). 왼쪽 pv 띠, 오른쪽 live 띠
fn verse_row(ui: &mut egui::Ui, vn: &str, text: &str, m: Mark) -> Response {
    let p = pal();
    let w = row_width(ui);
    let vn_w = 16.0 * 3.2;
    let x_text = ROW_PAD_L + vn_w;
    let g = job(ui, text, 16.0, Weight::Regular, p.text, 1.6, w - x_text - 14.0);
    let h = g.size().y + 14.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click());
    if ui.is_rect_visible(rect) {
        paint_row_marks(ui, rect, m, resp.hovered());
        let pt = ui.painter();
        let vg = pt.layout_no_wrap(vn.to_string(), theme::font(13.0, Weight::Bold), p.hl);
        let first_line = 16.0 * 1.6;
        // 기준선 맞춤 (align-items: baseline): 13px 글자의 아래를 16px 글자의 아래에 맞춘다
        let y = rect.top() + 7.0 + (first_line - vg.size().y) / 2.0 + 1.5;
        pt.galley(Pos2::new(rect.left() + ROW_PAD_L + vn_w - 10.0 - vg.size().x, y), vg, p.hl);
        pt.galley(Pos2::new(rect.left() + x_text, rect.top() + 7.0), g, p.text);
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// 낱말 강조 구간 (control.js highlight): 검색어의 글자 사이에 공백이 있어도 찾는다 ('하나님의 사랑' ↔ '하나님의사랑')
fn mark_spans(text: &str, terms: &[String]) -> Vec<(usize, usize)> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut hits: Vec<(usize, usize)> = vec![];
    for t in terms {
        let want: Vec<char> = t.chars().filter(|c| !c.is_whitespace()).collect();
        if want.is_empty() {
            continue;
        }
        let mut i = 0;
        while i < chars.len() {
            // i 에서 시작해 글자마다 맞추고, 글자 사이의 공백은 건너뛴다
            let (mut j, mut k) = (i, 0);
            while k < want.len() && j < chars.len() {
                if chars[j].1 == want[k] {
                    j += 1;
                    k += 1;
                } else if k > 0 && chars[j].1.is_whitespace() {
                    j += 1;
                } else {
                    break;
                }
            }
            if k == want.len() {
                let end = chars.get(j).map(|c| c.0).unwrap_or(text.len());
                hits.push((chars[i].0, end));
                i = j;
            } else {
                i += 1;
            }
        }
    }
    hits.sort();
    hits
}

/// .results .row — 낱말 찾기 결과: 금색 장절(13px 700) + 본문(찾은 낱말은 warn-bg 칠)
fn text_row(ui: &mut egui::Ui, r: &str, text: &str, terms: &[String], m: Mark) -> Response {
    let p = pal();
    let w = row_width(ui);
    let mut j = egui::text::LayoutJob::default();
    let lh = Some(16.0 * 1.6);
    j.append(r, 0.0, egui::TextFormat { font_id: theme::font(13.0, Weight::Bold), color: p.gold, line_height: lh, ..Default::default() });
    let base = egui::TextFormat { font_id: theme::font(16.0, Weight::Regular), color: p.text, line_height: lh, ..Default::default() };
    let mark = egui::TextFormat { background: p.warn_bg, ..base.clone() };
    let hits = mark_spans(text, terms);
    let mut at = 0;
    let mut first = true;
    for (a, b) in hits {
        if a < at {
            continue;
        }
        j.append(&text[at..a], if first { 8.0 } else { 0.0 }, base.clone());
        first = false;
        j.append(&text[a..b], 0.0, mark.clone());
        at = b;
    }
    j.append(&text[at..], if first { 8.0 } else { 0.0 }, base);
    j.wrap.max_width = (w - ROW_PAD_L - 14.0).max(20.0);
    let g = ui.painter().layout_job(j);
    let h = g.size().y + 14.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click());
    if ui.is_rect_visible(rect) {
        paint_row_marks(ui, rect, m, resp.hovered());
        ui.painter().galley(Pos2::new(rect.left() + ROW_PAD_L, rect.top() + 7.0), g, p.text);
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// .results .heading (장 머리·소제목: muted 13px 700)
fn heading(ui: &mut egui::Ui, text: &str) {
    ui.add_space(8.0);
    egui::Frame::new().inner_margin(Margin { left: 14, right: 14, top: 2, bottom: 2 }).show(ui, |ui| {
        ui.label(rich(text, 13.0, Weight::Bold, pal().muted));
    });
}

/// .results .empty (가운데 흐린 안내)
fn empty(ui: &mut egui::Ui, text: &str) {
    ui.add_space(30.0);
    ui.vertical_centered(|ui| {
        ui.label(rich(text, 14.0, Weight::Regular, pal().muted).line_height(Some(14.0 * 1.8)));
    });
}

/// .results .srow — 슬라이드 목록 한 줄: 이름(4.5em, hl 12px 700) + 15px 글(줄바꿈 그대로). 미리보기면 pv 띠 + pv-bg.
/// 이름표 색이 있으면(--gc) 이름이 그 색, 테두리 안쪽에 4px 그 색 띠. 찬송가·악보는 작은 그림(높이 90 이하, 너비 45% 이하, 흰 바탕) + 가사
#[allow(clippy::too_many_arguments)]
fn slide_row(ui: &mut egui::Ui, c: &paint::Ctx, label: &str, gc: Option<Color32>, body: &SrowBody, pv: bool, live: bool, msel: bool) -> Response {
    let p = pal();
    let w = row_width(ui);
    let lw = 15.0 * 4.5;
    let x_text = 14.0 + lw;
    let avail = (w - x_text - 14.0).max(40.0);
    // 내용 배치
    enum Lay {
        Thumb(Option<egui::Image<'static>>, Vec2, Arc<egui::Galley>),
        Text(Option<&'static str>, Arc<egui::Galley>),
    }
    let lay = match body {
        SrowBody::Thumb(src, cap) => {
            let img = paint::image_uri(c, src).map(egui::Image::new);
            let max = Vec2::new(avail * 0.45, 90.0);
            let nat = img.as_ref().and_then(|i| i.load_for_size(ui.ctx(), max).ok()).and_then(|t| t.size()).unwrap_or(Vec2::new(160.0, 90.0));
            let sc = (max.x / nat.x).min(max.y / nat.y).min(1.0);
            let isz = nat * sc;
            let g = match cap {
                Some(t) => job(ui, t, 16.0, Weight::Regular, p.text, 1.6, avail - isz.x - 16.0),
                None => job(ui, &tr("(가사 없음 — 편집칸의 OBS 가사에 입력)"), 16.0, Weight::Regular, p.muted, 1.6, avail - isz.x - 16.0),
            };
            Lay::Thumb(img, isz, g)
        }
        SrowBody::Text(icon, t) => {
            let iw = if icon.is_some() { 22.0 } else { 0.0 };
            Lay::Text(*icon, job(ui, t, 15.0, Weight::Regular, p.text, 1.6, avail - iw))
        }
    };
    let content_h = match &lay {
        Lay::Thumb(_, isz, g) => isz.y.max(g.size().y),
        Lay::Text(_, g) => g.size().y.max(15.0 * 1.6),
    };
    let h = content_h + 16.0 + 1.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click_and_drag());
    if ui.is_rect_visible(rect) {
        let pt = ui.painter();
        if msel {
            // #results .srow.msel: accent 16% 바탕
            pt.rect_filled(rect, CornerRadius::ZERO, theme::mix(p.accent, 16.0, p.panel));
        } else if pv {
            pt.rect_filled(rect, CornerRadius::ZERO, p.pv_bg);
        } else if resp.contains_pointer() {
            pt.rect_filled(rect, CornerRadius::ZERO, p.hover);
        }
        if pv {
            pt.rect_filled(Rect::from_min_size(rect.min, Vec2::new(4.0, h)), CornerRadius::ZERO, p.pv);
        }
        if let Some(gc) = gc {
            pt.rect_filled(Rect::from_min_size(rect.min + Vec2::new(4.0, 0.0), Vec2::new(4.0, h)), CornerRadius::ZERO, gc);
        }
        if live {
            pt.rect_filled(Rect::from_min_max(Pos2::new(rect.right() - 4.0, rect.top()), rect.max), CornerRadius::ZERO, p.live);
        }
        pt.hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, p.line_soft));
        let lc = gc.unwrap_or(p.hl);
        let lg = pt.layout(label.to_string(), theme::font(12.0, Weight::Bold), lc, lw - 12.0);
        pt.galley(Pos2::new(rect.left() + 14.0 + lw - 12.0 - lg.size().x, rect.top() + 8.0 + (12.0 * 1.6 - lg.size().y.min(12.0 * 1.6)) / 2.0 + 2.0), lg, lc);
        let top = rect.top() + 8.0;
        match lay {
            Lay::Thumb(img, isz, g) => {
                // .sboth: 그림과 가사를 세로 가운데로, 사이 16px
                let ir = Rect::from_min_size(Pos2::new(rect.left() + x_text, top + (content_h - isz.y) / 2.0), isz);
                pt.rect_filled(ir, CornerRadius::same(4), Color32::WHITE);
                if let Some(img) = img {
                    img.corner_radius(4).paint_at(ui, ir);
                }
                let gy = top + (content_h - g.size().y) / 2.0;
                ui.painter().galley(Pos2::new(ir.right() + 16.0, gy), g, p.text);
            }
            Lay::Text(icon, g) => {
                let mut x = rect.left() + x_text;
                if let Some(ic) = icon {
                    icons::paint(ui, Rect::from_min_size(Pos2::new(x, top + (15.0 * 1.6 - 16.0) / 2.0), Vec2::splat(16.0)), ic, p.muted);
                    x += 22.0;
                }
                ui.painter().galley(Pos2::new(x, top), g, p.text);
            }
        }
        if msel {
            // outline: 3px solid accent; outline-offset: -1px
            ui.painter().rect_stroke(rect.shrink(1.0), CornerRadius::ZERO, Stroke::new(3.0, p.accent), StrokeKind::Inside);
        }
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// 썸네일·목록에서 누른 것
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Hit {
    /// 칸을 누름 (번호, 두 번 누름)
    Pick(usize, bool),
    /// 마우스 올림 단추 (번호, 동작)
    Op(usize, &'static str),
    /// 칸을 끌기 시작 (번호)
    Drag(usize),
}

/// Electron slideedit.js EDITABLE: 슬라이드 편집기로 고칠 수 있는 순서 종류
const EDITABLE: &[&str] = &["song", "text", "title", "bible", "hymn", "image", "video", "slideshow", "camera", "blank"];

/// 슬라이드에 마우스를 올리면 뜨는 단추 (grid.js .sg-edit · .sg-ops, slideedit.js extraOp 규칙).
/// 글 순서(찬양·공지·제목)와 덧붙인 슬라이드: ＋ · 복제 · ✕ 지우기 / 그 밖(성경·찬송가·그림 등 지울 수 없는 것): ＋ · 건너뛰기.
/// 모두 ✎ 편집(오른쪽 위). 큐시트 순서가 아니거나(검색한 본문) 음악이면 없음. `all_right` 면 모두 오른쪽에 (목록 줄).
pub(crate) fn slide_hover_ops(it: Option<&Value>, s: &Value, all_right: bool) -> Vec<widgets::HoverOp> {
    use widgets::HoverOp;
    let Some(it) = it else { return vec![] };
    let ty = it["type"].as_str().unwrap_or("");
    if !EDITABLE.contains(&ty) {
        return vec![];
    }
    let mut v = vec![HoverOp::new("add", "plus", tr("이 다음에 새 슬라이드"))];
    if wp_core::slideedit::is_chunk(ty) || s["xid"].is_string() {
        v.push(HoverOp::new("dup", "copy", tr("복제 (꾸민 것도)")));
        v.push(HoverOp::new("del", "trash-2", tr("이 슬라이드 지우기 (Ctrl+Z 로 되돌리기)")).danger());
    } else {
        v.push(HoverOp::new("skip", "eye-off", tr("건너뛰기 (송출에서 빼기, 원래 내용은 그대로 · 슬라이드 편집에서 다시 보이기)")));
    }
    v.push(HoverOp::new("edit", "pencil", tr("이 슬라이드 편집 (Ctrl+E)")).right());
    if all_right {
        // 오른쪽 끝부터 놓으므로 거꾸로 (왼쪽→오른쪽: ＋ · 복제/건너뛰기 · 지우기 · ✎)
        v = v.into_iter().rev().map(|o| o.right()).collect();
    }
    v
}

/// 슬라이드 오른쪽 클릭 메뉴에서 고른 것
pub(crate) enum MenuAct {
    /// (다른 순서면 그 순서 번호, 슬라이드, 바로 송출)
    Preview(Option<usize>, usize, bool),
    /// (순서, 슬라이드, 편집 키, 동작 edit·add·dup·del)
    Op(usize, usize, String, &'static str),
    /// 여기서 새 순서로 나누기 (구역)
    Split(usize, usize),
    /// 이 순서 편집칸 (F2)
    ItemEditor(usize),
    /// 보기 방식 바꾸기
    View(SlideView),
}

/// 메뉴 한 줄 (선 아이콘 + 글 + 오른쪽 키 설명)
pub(crate) fn menu_btn(ui: &mut egui::Ui, icon: Option<&str>, label: &str, key: &str, enabled: bool, danger: bool) -> bool {
    let p = pal();
    let color = if danger { p.danger_text } else { p.text };
    let mut b = match icon {
        Some(ic) => egui::Button::image_and_text(icons::image(ic, 14.0, color), rich(label, 13.0, Weight::Regular, color)),
        None => egui::Button::new(rich(label, 13.0, Weight::Regular, color)),
    };
    if !key.is_empty() {
        b = b.shortcut_text(rich(key, 12.0, Weight::Regular, p.muted));
    }
    let r = ui.add_enabled(enabled, b);
    widgets::mark(&format!("menu:{label}"), r.rect);
    if r.clicked() {
        ui.close();
        return true;
    }
    false
}

/// 슬라이드 목록·전체 목록 줄의 오른쪽 클릭 메뉴 (ctxmenu.js slideMenu · #cueGrid 메뉴).
/// `all` 이 Some(순서 번호)이면 전체 보기 메뉴(머리 '7번 순서 · 슬라이드 2', 그 순서를 먼저 미리보기)
fn slide_menu(resp: &Response, k: usize, sl: &Value, item: Option<(usize, &Value)>, all: Option<usize>, view: SlideView) -> Option<MenuAct> {
    let mut out = None;
    resp.context_menu(|ui| {
        ui.set_min_width(230.0);
        let p = pal();
        let lab = sl["label"].as_str().filter(|l| !l.is_empty() && !l.chars().all(|c| c.is_ascii_digit()));
        let head = match all {
            Some(i) => tr_args("{1}번 순서 · 슬라이드 {2}", &[&(i + 1), &(k + 1)]),
            None => format!("{}{}", tr_args("슬라이드 {1}", &[&(k + 1)]), lab.map(|l| format!(" · {l}")).unwrap_or_default()),
        };
        ui.label(rich(head, 12.0, Weight::SemiBold, p.muted));
        if menu_btn(ui, None, &tr("미리보기"), &tr("클릭"), true, false) {
            out = Some(MenuAct::Preview(all, k, false));
        }
        if menu_btn(ui, Some("play"), &tr("바로 송출"), &tr("더블클릭"), true, false) {
            out = Some(MenuAct::Preview(all, k, true));
        }
        ui.separator();
        let ty = item.map(|x| x.1["type"].as_str().unwrap_or("")).unwrap_or("");
        let editable = item.is_some() && EDITABLE.contains(&ty);
        let chunk = matches!(ty, "song" | "text" | "title");
        let key = slide_key(sl);
        let i = item.map(|x| x.0).unwrap_or(0);
        if menu_btn(ui, Some("square-pen"), &tr("이 슬라이드 편집"), "Ctrl+E", editable, false) {
            out = Some(MenuAct::Op(i, k, key.clone(), "edit"));
        }
        if editable {
            if menu_btn(ui, Some("plus"), &tr("이 다음에 새 슬라이드"), "", true, false) {
                out = Some(MenuAct::Op(i, k, key.clone(), "add"));
            }
            if all.is_none() {
                if menu_btn(ui, Some("copy"), &tr("이 슬라이드 복제"), "", chunk || sl["xid"].is_string(), false) {
                    out = Some(MenuAct::Op(i, k, key.clone(), "dup"));
                }
                let del = chunk || sl["xid"].is_string();
                if menu_btn(ui, Some(if del { "trash-2" } else { "eye-off" }), &tr(if del { "이 슬라이드 지우기" } else { "이 슬라이드 건너뛰기" }), "", true, true) {
                    out = Some(MenuAct::Op(i, k, key.clone(), "del"));
                }
            }
        }
        if let Some((i, _)) = item {
            if all.is_none() && menu_btn(ui, Some("pencil"), &tr("이 순서 편집칸"), "F2", true, false) {
                out = Some(MenuAct::ItemEditor(i));
            }
            // 구역 나누기: 글 덩어리(찬양·공지)의 둘째 장부터
            let can_split = matches!(ty, "song" | "text") && sl["src"].as_u64().unwrap_or(k as u64) > 0;
            if menu_btn(ui, Some("columns-2"), &tr("여기서 새 순서로 나누기 (구역)"), "", can_split, false) {
                out = Some(MenuAct::Split(i, k));
            }
        }
        ui.separator();
        for (v, l) in [(SlideView::List, "목록"), (SlideView::Grid, "격자"), (SlideView::All, "전체 격자 (모든 순서)"), (SlideView::AllList, "전체 목록 (모든 순서)")] {
            if v != view && menu_btn(ui, None, &tr_args("{1} 로 보기", &[&tr(l)]), "", true, false) {
                out = Some(MenuAct::View(v));
            }
        }
    });
    out
}

/// 여기서 새 순서로 나누기 (slideedit.js splitAt — 썸네일 메뉴 parity2_ui 와 같은 방식)
fn split_item(core: &wp_core::core::Shared, i: usize, k: usize) {
    let mut c = core.lock();
    let Ok(d) = c.item_deck(i) else { return };
    let Some(it) = c.cue.as_ref().and_then(|q| q.items.get(i)).cloned() else { return };
    let key = d.slides.get(k).and_then(|s| s["src"].as_u64()).map(|x| x as usize).unwrap_or(k);
    let (new_it, back) = wp_core::slideedit::apply_op(&it, "split", key);
    c.remember_items();
    if let Some(cue) = &mut c.cue {
        if i < cue.items.len() {
            cue.items[i] = new_it;
            if let Some(b) = back {
                cue.items.insert(i + 1, b);
            }
        }
    }
    c.save_cue();
    if let Ok(mut deck) = c.item_deck(i) {
        deck.index = k.min(deck.slides.len().saturating_sub(1));
        c.pres.preview = Some(deck);
    }
    c.emit(wp_core::core::Ev::Ui);
}

/// 미리보기·송출 덱이 열린 큐시트의 순서이면 그 번호와 순서
pub(crate) fn deck_item<'a>(s: &'a Snap, d: &Deck) -> Option<(usize, &'a Value)> {
    let i = cue_index(d)?;
    if d.passage["cueId"].as_str() != Some(s.cue_id.as_str()) {
        return None;
    }
    s.items.get(i).map(|it| (i, it))
}

/// 슬라이드의 편집 키 (decorate 가 붙인 ekey)
pub(crate) fn slide_key(s: &Value) -> String {
    s["ekey"].as_str().map(String::from).unwrap_or_else(|| s["src"].as_u64().unwrap_or(0).to_string())
}

/// 슬라이드 썸네일 칸들 (.slide-grid / .cg-row). 누른 칸 또는 마우스 올림 단추
#[allow(clippy::too_many_arguments)]
fn thumb_cells(ui: &mut egui::Ui, id: egui::Id, c: &paint::Ctx, slides: &[Value], tpl: &Value, version: &str, pv: Option<usize>, live: Option<usize>, size: f32, ops: &dyn Fn(&Value) -> Vec<widgets::HoverOp>, msel: &dyn Fn(usize) -> bool, cells: &mut Vec<(usize, Rect)>) -> Option<Hit> {
    let p = pal();
    let gap = 10.0;
    let w = row_width(ui);
    let cols = (((w + gap) / (size + gap)).floor() as usize).max(1);
    let cw = (w - gap * (cols - 1) as f32) / cols as f32;
    let (tw, th) = (tpl["width"].as_f64().unwrap_or(1920.0) as f32, tpl["height"].as_f64().unwrap_or(1080.0) as f32);
    let sh = (cw - 6.0) * th / tw;
    let label_h = 12.0 * 1.45 + 6.0;
    let ch = 6.0 + sh + label_h;
    let mut out = None;
    for (row, chunk) in slides.chunks(cols).enumerate() {
        let (rr, _) = ui.allocate_exact_size(Vec2::new(w, ch), Sense::hover());
        for (j, s) in chunk.iter().enumerate() {
            let k = row * cols + j;
            let r = Rect::from_min_size(Pos2::new(rr.left() + j as f32 * (cw + gap), rr.top()), Vec2::new(cw, ch));
            let resp = ui.interact(r, id.with(k), Sense::click_and_drag());
            cells.push((k, r));
            #[cfg(test)]
            widgets::HOVER_SEEN.with(|m| m.borrow_mut().insert(format!("cell-{k}"), r));
            if resp.drag_started_by(egui::PointerButton::Primary) {
                out = Some(Hit::Drag(k));
            }
            match widgets::click2(ui, (id, "cell", k), &resp) {
                widgets::Click::Single => out = Some(Hit::Pick(k, false)),
                widgets::Click::Double => out = Some(Hit::Pick(k, true)),
                widgets::Click::None => {}
            }
            // 단추 위에 있어도 칸은 마우스 올림 상태로 (테두리·바탕 강조가 깜빡이지 않게)
            let hot = resp.contains_pointer();
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if !ui.is_rect_visible(r) {
                continue;
            }
            let pt = ui.painter();
            let ms = msel(k);
            // .sg-cell.msel: accent 16% 바탕
            pt.rect_filled(r, CornerRadius::same(6), if ms { theme::mix(p.accent, 16.0, p.sunken) } else if hot { p.hover } else { p.sunken });
            let screen = Rect::from_min_size(r.min + Vec2::splat(3.0), Vec2::new(cw - 6.0, sh));
            paint::paint_slide(ui, c, screen, Some(s), tpl, version, true, false, false); // logo=로고 화면(F 로고 켜기) 아님
            let lab = slide_label(s);
            let mut lj = egui::text::LayoutJob::default();
            lj.append(&format!("{}", k + 1), 0.0, egui::TextFormat { font_id: theme::font(12.0, Weight::Bold), color: p.hl, ..Default::default() });
            lj.append(&lab, 4.0, egui::TextFormat { font_id: theme::font(12.0, Weight::Regular), color: p.muted, ..Default::default() });
            lj.wrap = egui::text::TextWrapping::truncate_at_width(cw - 18.0);
            let lg = ui.painter().layout_job(lj);
            ui.painter().galley(Pos2::new(r.left() + 9.0, screen.bottom() + 3.0), lg, p.muted);
            let (is_pv, is_live) = (pv == Some(k), live == Some(k));
            let border = if is_live { p.live } else if is_pv { p.pv } else if hot { p.line2 } else { Color32::TRANSPARENT };
            ui.painter().rect_stroke(r, CornerRadius::same(6), Stroke::new(3.0, border), StrokeKind::Inside);
            if is_pv && is_live {
                ui.painter().rect_stroke(r.expand(3.0), CornerRadius::same(8), Stroke::new(2.0, p.pv), StrokeKind::Inside);
            }
            if ms {
                // outline: 3px solid accent
                ui.painter().rect_stroke(r, CornerRadius::same(6), Stroke::new(3.0, p.accent), StrokeKind::Inside);
            }
            if let Some(op) = widgets::hover_ops(ui, id.with(("ops", k)), r, &ops(s)) {
                out = Some(Hit::Op(k, op));
            }
        }
        ui.add_space(gap - ui.spacing().item_spacing.y);
    }
    out
}

// ------------------------------------------------------------------ App 쪽

impl App {
    /// 가운데 본문 패널 머리 줄(.center-head). `editor_on` 은 App 의 편집칸 켜짐 값
    /// 지금 검색한 장절로 만든 성경 순서 (장절 검색 결과가 없으면 None).
    /// 원본 ctxmenu.js '이 본문을 큐시트에 넣기': { title: '성경봉독', ref: 본문 제목 } — 번역본은 비워 두어(현재 번역본) 원본과 같게
    pub(crate) fn searched_bible_item(&self) -> Option<Value> {
        if self.result["mode"] != "reference" || self.result["ok"] != json!(true) {
            return None;
        }
        let title = self.result["passage"]["title"].as_str().map(str::trim).filter(|t| !t.is_empty()).map(String::from).unwrap_or_else(|| self.search.trim().to_string());
        (!title.is_empty()).then(|| json!({ "title": tr("성경 봉독"), "ref": title }))
    }

    /// 검색한 장절을 고른 순서 다음에 넣고 알린다 (본문 패널 머리 · 검색칸 오른쪽 클릭 · 왼쪽 '성경' 단추)
    pub(crate) fn add_searched_bible(&mut self, acts: &mut Vec<Act>) -> bool {
        let Some(it) = self.searched_bible_item() else { return false };
        let r = it["ref"].as_str().unwrap_or("").to_string();
        acts.push(Act::AddItem("bible", Some(it)));
        self.core.lock().notify(tr_args("'{1}' 을(를) 큐시트에 넣었습니다.", &[&r]), "ok");
        true
    }

    pub(crate) fn center_head(&mut self, ui: &mut egui::Ui, s: &Snap, editor_on: &mut bool, acts: &mut Vec<Act>) {
        let mut st = load(ui.ctx());
        // 가운데에 무엇을 보일지: 큐시트 순서를 미리보기로 올리면 그 슬라이드, 검색하면 검색 결과 (Electron 은 마지막에 그린 쪽)
        let pk = s.preview.as_ref().map(|d| format!("{}:{}:{}", d.passage["cueId"], d.passage["cueIndex"], d.passage["title"])).unwrap_or_default();
        if pk != st.seen_preview {
            st.seen_preview = pk;
            st.cue_mode = s.preview.as_ref().and_then(cue_index).is_some();
        }
        if self.search != st.seen_search {
            st.seen_search = self.search.clone();
            if !self.search.trim().is_empty() {
                st.cue_mode = false;
            }
        }
        self.cm_refresh(&mut st, s);
        let has_sel = self.sel.is_some_and(|i| i < self.core.lock().cue.as_ref().map(|c| c.items.len()).unwrap_or(0));
        let (title, info) = self.center_title(&st, s);
        let mut toggle_editor = false;
        // 여러 장을 고른 중이면 머리 줄 자리에 '고른 슬라이드 N장' 줄 (.msel-bar: accent 24% 바탕 · 아래 2px accent)
        let many = ms_many(&st, s);
        let mut bar_act: Option<(&'static str, bool)> = None;
        let fill = if many.is_some() { theme::mix(pal().accent, 24.0, pal().panel) } else { Color32::TRANSPARENT };
        // .center-head: padding 8px 12px · gap 8 · 아래 line
        let head = egui::Frame::new().fill(fill).inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
            if let Some((i, ks)) = &many {
                let n = ks.len();
                let chunk = s.items.get(*i).is_some_and(|it| wp_core::slideedit::is_chunk(it["type"].as_str().unwrap_or("")));
                ui.horizontal(|ui| {
                    ui.set_min_height(26.0);
                    ui.spacing_mut().item_spacing.x = 6.0;
                    let cid = ms_confirm_id();
                    if widgets::confirm_armed(ui, cid) {
                        if widgets::confirm_row(ui, cid, &tr_args("고른 슬라이드 {1}장을 지울까요? (가사·내용에서도 지워집니다)", &[&n])) {
                            bar_act = Some(("del", true));
                        }
                        return;
                    }
                    ui.label(rich(tr_args("고른 슬라이드 {1}장", &[&n]), 13.0, Weight::Bold, pal().text));
                    let b = ui.add(Btn::new(tr_args("고른 {1}장 편집", &[&n])).icon("square-pen").size(Size::Sm).height(26.0).kind(widgets::Kind::Primary).tooltip(tr("고른 슬라이드를 함께 꾸미기 (Ctrl+E)")));
                    widgets::mark("msel-edit", b.rect);
                    if b.clicked() {
                        bar_act = Some(("edit", false));
                    }
                    let (ic, l) = if chunk { ("trash-2", tr("지우기")) } else { ("eye-off", tr("건너뛰기")) };
                    let b = ui.add(Btn::new(&l).icon(ic).size(Size::Sm).height(26.0).tooltip(format!("{l} (Delete)")));
                    widgets::mark("msel-del", b.rect);
                    if b.clicked() {
                        bar_act = Some(("del", false));
                    }
                    let b = ui.add(Btn::new(tr("해제")).icon("x").size(Size::Sm).height(26.0).tooltip(tr("한 장만 (Esc)")));
                    widgets::mark("msel-clear", b.rect);
                    if b.clicked() {
                        bar_act = Some(("clear", false));
                    }
                    ui.add(egui::Label::new(rich(tr("Shift+클릭 범위 · Ctrl+클릭 더하기/빼기 · 끌어서 차례 바꾸기"), 11.0, Weight::Regular, pal().muted)).truncate());
                });
                return;
            }
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let cm_label = if st.cm_rows.is_empty() { tr("주석") } else { format!("{} ({})", tr("주석"), st.cm_rows.len()) };
                let body = tr("본문");
                let mut tab = st.cm;
                if widgets::segmented(ui, &mut tab, &[(false, body.as_str()), (true, cm_label.as_str())]).on_hover_text(tr("미리보기 중인 본문의 주석 (조작 화면에만 보이고 송출되지 않습니다)")).changed() {
                    st.cm = tab;
                }
                if has_sel && ui.add(Btn::new(tr("편집칸")).icon("pencil").size(Size::Sm).height(26.0).selected(*editor_on).tooltip(tr("고른 순서를 여기서 바로 고치는 칸 보이기·숨기기 (F2 · Ctrl+Shift+E)"))).clicked() {
                    toggle_editor = true;
                }
                // 제목(굵게 15px) + 흐린 부가 글 — 넘치면 제목을 줄인다
                // 검색한 장절을 보고 있을 때만 '＋ 큐시트에 넣기' (큐시트 순서를 보고 있을 때는 필요 없음)
                let can_add = !st.cue_mode && self.searched_bible_item().is_some();
                let add_w = if can_add { 140.0 } else { 0.0 };
                let view_w = 250.0 + add_w + if matches!(st.view, SlideView::Grid | SlideView::All) { 100.0 } else { 0.0 };
                let info_g = ui.painter().layout_no_wrap(info.clone(), theme::font(14.0, Weight::Regular), pal().muted);
                let room = (row_width(ui) - view_w - info_g.size().x - 16.0).max(60.0);
                ui.scope(|ui| {
                    ui.set_max_width(room);
                    ui.add(egui::Label::new(rich(&title, 15.0, Weight::Bold, pal().text)).truncate());
                });
                if !info.is_empty() {
                    ui.label(rich(&info, 14.0, Weight::Regular, pal().muted));
                }
                if can_add {
                    let b = ui.add(Btn::new(tr("큐시트에 넣기")).icon("plus").size(Size::Sm).height(26.0).kind(widgets::Kind::Primary).tooltip(tr("검색한 본문을 성경 순서로 큐시트에 넣습니다 (고른 순서 다음에)")));
                    widgets::mark("center-add-bible", b.rect);
                    if b.clicked() {
                        self.add_searched_bible(acts);
                    }
                }
                let (l, g, a, al) = (tr("목록"), tr("격자"), tr("전체 격자"), tr("전체 목록"));
                let mut v = st.view;
                if widgets::segmented(ui, &mut v, &[(SlideView::List, l.as_str()), (SlideView::Grid, g.as_str()), (SlideView::All, a.as_str()), (SlideView::AllList, al.as_str())]).on_hover_text(tr("슬라이드 보기 방식")).changed() {
                    st.view = v;
                    st.all_key.clear();
                }
                if matches!(st.view, SlideView::Grid | SlideView::All) {
                    ui.spacing_mut().slider_width = 90.0;
                    ui.add(egui::Slider::new(&mut st.size, 150.0..=420.0).step_by(10.0).show_value(false)).on_hover_text(tr("썸네일 크기 (격자 위에서 Ctrl + 휠로도)"));
                }
            });
        });
        let r = head.response.rect;
        if many.is_some() {
            ui.painter().hline(ui.max_rect().x_range(), r.bottom() - 0.5, Stroke::new(2.0, pal().accent));
        } else {
            ui.painter().hline(ui.max_rect().x_range(), r.bottom(), Stroke::new(1.0, pal().line));
        }
        if toggle_editor {
            *editor_on = !*editor_on;
        }
        if let Some((a, confirmed)) = bar_act {
            self.msel_do(ui.ctx(), &mut st, s, a, confirmed, acts);
        }
        save(ui.ctx(), st);
    }

    /// 가운데 본문: 목록(검색 결과·순서 슬라이드) / 격자 / 전체 격자·목록 / 주석
    pub(crate) fn center_body(&mut self, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
        let mut st = load(ui.ctx());
        // 목록 밖을 누르거나 글 쓰는 칸에 들어가면 키보드는 목록을 떠난다 (#results blur)
        let body = ui.max_rect();
        st.body_rect = Some(body);
        let outside = ui.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !body.contains(p)));
        if outside || ui.ctx().egui_wants_keyboard_input() {
            st.kbd = false;
        }
        if let (Some((a, b)), Some(_)) = (st.ms_init, pv_item(s)) {
            st.ms_init = None;
            ms_reset(&mut st, s, Some(a));
            st.ms = (a..=b).collect();
            st.kbd = true;
        }
        // 끌어 옮긴 뒤: 옮긴 슬라이드를 새 자리에서 고른 채로 (moveSlides 끝)
        if let Some((i, keys)) = st.ms_after.take() {
            if let (Some((j, _, _)), Some(d)) = (pv_item(s), s.preview.as_ref()) {
                if i == j {
                    let idx: Vec<usize> = d.slides.iter().enumerate().filter(|(_, sl)| keys.contains(&slide_key(sl))).map(|(k, _)| k).collect();
                    if let Some(f) = idx.first() {
                        ms_reset(&mut st, s, Some(*f));
                        if idx.len() > 1 {
                            st.ms = idx.into_iter().collect();
                        }
                    }
                }
            }
        }
        // 끌던 중 단추를 놓았는데 그 보기가 안 그려졌으면 끌기를 푼다
        if st.drag.is_some() && !ui.input(|i| i.pointer.primary_down() || i.pointer.any_released()) {
            st.drag = None;
        }
        if st.cm {
            self.cm_view(ui, &mut st, s);
        } else {
            let text_search = !st.cue_mode && self.result["ok"] == json!(true) && self.result["mode"] != "reference";
            let cue_open = self.core.lock().cue.is_some();
            let pv_is_cue = s.preview.as_ref().is_none_or(|d| cue_index(d).is_some());
            match st.view {
                SlideView::All | SlideView::AllList if cue_open && pv_is_cue && !text_search => self.all_view(ui, &mut st, s, acts),
                SlideView::Grid if s.preview.is_some() && !text_search => self.grid_view(ui, &mut st, s, acts),
                _ => {
                    if st.cue_mode && s.preview.as_ref().and_then(cue_index).is_some() {
                        self.item_list(ui, &mut st, s, acts)
                    } else {
                        // center_results 가 상태를 읽고 쓰므로 앞뒤로 맞춘다
                        save(ui.ctx(), st.clone());
                        self.search_results(ui, s, acts);
                        st = load(ui.ctx());
                    }
                }
            }
        }
        // 마우스 휠 + Ctrl 로 썸네일 크기 (grid.js)
        if matches!(st.view, SlideView::Grid | SlideView::All) && ui.ui_contains_pointer() {
            let z = ui.input(|i| i.zoom_delta());
            if (z - 1.0).abs() > 0.001 {
                st.size = (st.size * z).clamp(150.0, 420.0);
            }
        }
        save(ui.ctx(), st);
    }

    /// #resultTitle · #resultInfo
    fn center_title(&self, st: &CenterState, s: &Snap) -> (String, String) {
        if st.cue_mode {
            if let Some(d) = s.preview.as_ref().filter(|d| cue_index(d).is_some()) {
                let i = cue_index(d).unwrap_or(0);
                let t = d.passage["itemTitle"].as_str().unwrap_or("");
                let ty = d.passage["itemType"].as_str().unwrap_or("");
                if ty == "bible" {
                    let n = bible_verses(d).len();
                    return (format!("{}. {} — {}", i + 1, t, d.passage["title"].as_str().unwrap_or("")), tr_args("{1}절 · {2}", &[&n, &d.passage["versionName"].as_str().unwrap_or("")]));
                }
                if ty == "audio" {
                    return (format!("{}. {}", i + 1, t), tr("음악 · 화면은 바뀌지 않습니다"));
                }
                return (format!("{}. {}", i + 1, t), tr_args("{1} · 슬라이드 {2}장", &[&tr(type_name(ty)), &d.slides.len()]));
            }
        }
        let r = &self.result;
        if r.is_null() || r["ok"] != json!(true) {
            return (tr("검색 결과"), String::new());
        }
        if r["mode"] == "reference" {
            let p = &r["passage"];
            let n = p["verses"].as_array().map(|v| v.len()).unwrap_or(0);
            if let Some((a, b)) = st.range {
                // markResults: '선택 3절 (23:2-4) · Shift+클릭으로 범위 조정'
                let pt = s.preview.as_ref().and_then(|d| d.passage["title"].as_str()).unwrap_or("");
                let span = pt.split_once(char::is_whitespace).map(|x| x.1).unwrap_or("");
                return (p["title"].as_str().unwrap_or("").to_string(), tr_args("선택 {1}절 ({2}) · Shift+클릭으로 범위 조정", &[&(b - a + 1), &span]));
            }
            return (p["title"].as_str().unwrap_or("").to_string(), tr_args("{1}절 · {2}", &[&n, &p["versionName"].as_str().unwrap_or("")]));
        }
        let n = r["results"].as_array().map(|v| v.len()).unwrap_or(0);
        (tr_args("“{1}” 본문 검색", &[&self.search.trim()]), format!("{}{}{}", n, if r["truncated"] == json!(true) { "+" } else { "" }, tr("건")))
    }

    /// 검색 결과 본문 (장절 → 절 목록, 낱말 → 찾은 절 목록). app.rs search_results 가 부른다
    pub(crate) fn center_results(&mut self, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
        let r = self.result.clone();
        let pv_keys = slide_keys(s.preview.as_ref().and_then(|d| d.slides.get(d.index)));
        let live_keys = live_keys(s.live.as_ref());
        let mut st = load(ui.ctx());
        // 목록이 바뀌면 고른 줄·범위 초기화 (낱말 찾기는 첫 줄을 고른 채로 — showTextResults sel: 0)
        let reference = r["mode"] == "reference";
        let n_rows = if reference { r["passage"]["verses"].as_array().map(|v| v.len()) } else { r["results"].as_array().map(|v| v.len()) }.unwrap_or(0);
        let rk = res_key_of(&r, n_rows);
        if rk != st.res_key {
            st.res_key = rk;
            st.sel = (!reference && n_rows > 0).then_some(0);
            st.anchor = None;
            st.range = None;
        }
        // 미리보기가 범위 본문이 아니게 되면(다른 것을 미리보기) 범위 표시도 푼다
        if st.range.is_some() && s.preview.as_ref().is_none_or(|d| cue_index(d).is_some()) {
            st.range = None;
        }
        // 이 절이 지금 미리보기(검색한 본문 또는 범위) 안에 있으면 그 슬라이드 번호 (activateResult: inPreview → setPreviewIndex)
        let in_preview = |c: i64, v: i64| -> Option<usize> {
            let d = s.preview.as_ref().filter(|d| cue_index(d).is_none())?;
            d.slides.iter().any(|sl| slide_keys(Some(sl)).iter().any(|k| k.1 == c && k.2 == v)).then(|| wp_core::slides::index_of_verse(&d.slides, c, v))
        };
        let key = list_keys(ui.ctx(), &mut st, n_rows, reference);
        egui::ScrollArea::vertical().id_salt("results").auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.add_space(6.0);
            if r.is_null() {
                return;
            }
            if r["ok"] != json!(true) {
                egui::Frame::new().inner_margin(Margin::symmetric(14, 8)).show(ui, |ui| {
                    ui.label(rich(r["error"].as_str().unwrap_or(""), 14.0, Weight::Regular, pal().danger_text));
                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        for c in r["candidates"].as_array().into_iter().flatten() {
                            let name = c["name"].as_str().unwrap_or("").to_string();
                            if ui.add(Btn::new(&name).size(Size::Sm)).clicked() {
                                self.search = format!("{} {}", c["short"].as_str().unwrap_or(&name), r["rest"].as_str().unwrap_or(""));
                                acts.push(Act::Search(self.search.clone()));
                            }
                        }
                    });
                });
                return;
            }
            if r["mode"] == "reference" {
                let p = &r["passage"];
                // 치는 중인 장절은 목록만 보이고 미리보기는 Enter 때 (control.js 후보 '↵ 요한복음 3:16')
                let shown = s.preview.as_ref().is_some_and(|d| d.passage["query"].as_str().map(str::trim) == Some(self.search.trim()) && cue_index(d).is_none());
                if !shown && !self.search.trim().is_empty() {
                    let label = wp_core::i18n::tr_args("↵ {1} — Enter 로 미리보기", &[&p["title"].as_str().unwrap_or("")]);
                    egui::Frame::new().inner_margin(Margin::symmetric(14, 4)).show(ui, |ui| {
                        let b = ui.add(Btn::new(&label).size(Size::Sm)).on_hover_text(wp_core::i18n::tr("누르거나 검색칸에서 Enter: 이 본문을 미리보기로 (바로 송출 모드면 송출)"));
                        crate::widgets::mark("search-enter-hint", b.rect);
                        if b.clicked() {
                            acts.push(Act::Search(self.search.clone()));
                        }
                    });
                }
                let verses = p["verses"].as_array().cloned().unwrap_or_default();
                let book = p["book"]["id"].as_i64().or_else(|| p["book"].as_i64()).unwrap_or(0);
                let multi = verses.first().map(|v| &v["chapter"]) != verses.last().map(|v| &v["chapter"]);
                let mut heads = std::collections::HashMap::new();
                for h in p["headings"].as_array().into_iter().flatten() {
                    heads.insert((h["chapter"].as_i64().unwrap_or(0), h["verse"].as_i64().unwrap_or(0)), h["text"].as_str().unwrap_or("").to_string());
                }
                let mut last_ch = -1;
                let pv_first = verses.iter().position(|v| pv_keys.iter().any(|k| same_verse(*k, vkey(v["book"].as_i64().unwrap_or(book), v["chapter"].as_i64().unwrap_or(0), v["verse"].as_i64().unwrap_or(0)))));
                let scroll_key = format!("{}:{:?}", p["title"], pv_first);
                let cv = |i: usize| verses.get(i).map(|v| (v["chapter"].as_i64().unwrap_or(0), v["verse"].as_i64().unwrap_or(0))).unwrap_or((0, 0));
                // 누른 줄 미리보기 (activateResult)
                let activate = |i: usize, this: &mut Self, st: &mut CenterState, acts: &mut Vec<Act>| {
                    let (c, n) = cv(i);
                    match in_preview(c, n) {
                        Some(k) if st.range.is_some() || s.preview.as_ref().is_some_and(|d| d.passage["title"] == p["title"]) => acts.push(Act::PreviewIndex(k)),
                        _ => {
                            st.range = None;
                            acts.push(Act::PreviewPassage(p.clone(), this.search.clone(), Some((c, n))));
                        }
                    }
                };
                // Shift 범위: 목록은 그대로, 고른 절들만 미리보기 (selectRange)
                let select_range = |i: usize, this: &mut Self, st: &mut CenterState, acts: &mut Vec<Act>| {
                    let i = i.min(verses.len().saturating_sub(1));
                    let a = *st.anchor.get_or_insert(st.sel.unwrap_or(i));
                    let rg = (a.min(i), a.max(i));
                    st.range = Some(rg);
                    st.sel = Some(i);
                    st.scroll_sel = true;
                    if let Some((sub, short)) = range_passage(p, rg.0, rg.1) {
                        this.search = short.clone();
                        st.seen_search = short.clone(); // 검색칸 글만 바꿈 (keepList: 다시 찾지 않음)
                        acts.push(Act::PreviewPassage(sub, short, None));
                    }
                };
                match key {
                    Some(ListKey::Move(i)) => {
                        st.sel = Some(i);
                        st.scroll_sel = true;
                    }
                    Some(ListKey::Range(i)) => select_range(i, self, &mut st, acts),
                    Some(ListKey::Activate(i)) => activate(i, self, &mut st, acts),
                    Some(ListKey::ToSearch) => {
                        st.kbd = false;
                        self.focus_search = true;
                    }
                    None => {}
                }
                for (i, v) in verses.iter().enumerate() {
                    let (c, n) = (v["chapter"].as_i64().unwrap_or(0), v["verse"].as_i64().unwrap_or(0));
                    if multi && c != last_ch {
                        heading(ui, &tr_args("{1}장", &[&c]));
                    }
                    last_ch = c;
                    if let Some(h) = heads.get(&(c, n)) {
                        heading(ui, h);
                    }
                    let k = vkey(v["book"].as_i64().unwrap_or(book), c, n);
                    let vn = if multi { format!("{c}:{n}") } else { n.to_string() };
                    let m = Mark { pv: pv_keys.iter().any(|x| same_verse(*x, k)), live: live_keys.iter().any(|x| same_verse(*x, k)), sel: st.sel == Some(i), range: st.range.is_some_and(|(a, b)| i >= a && i <= b) };
                    let resp = verse_row(ui, &vn, v["text"].as_str().unwrap_or(""), m);
                    #[cfg(test)]
                    widgets::HOVER_SEEN.with(|h| h.borrow_mut().insert(format!("row-{i}"), resp.rect));
                    if Some(i) == pv_first && st.scroll_key != scroll_key {
                        resp.scroll_to_me(None);
                        st.scroll_key = scroll_key.clone();
                    }
                    if m.sel && st.scroll_sel {
                        resp.scroll_to_me(None);
                        st.scroll_sel = false;
                    }
                    let vref = format!("{} {}:{}", p["book"]["name"].as_str().unwrap_or(""), c, n);
                    match crate::ctx_ui::verse_menu(&resp, &format!("{} · {}", p["title"].as_str().unwrap_or(""), tr_args("{1}절", &[&vn])), &vref, v["text"].as_str().unwrap_or("")) {
                        Some(crate::ctx_ui::VerseAct::Preview) => {
                            st.sel = Some(i);
                            activate(i, self, &mut st, acts);
                        }
                        Some(crate::ctx_ui::VerseAct::Send) => {
                            st.sel = Some(i);
                            activate(i, self, &mut st, acts);
                            acts.push(Act::SendPreview);
                        }
                        Some(crate::ctx_ui::VerseAct::AddCue) => {
                            self.add_searched_bible(acts);
                        }
                        None => {}
                    }
                    let click = widgets::click2(ui, ("res-verse", c, n), &resp);
                    let shift = ui.input(|x| x.modifiers.shift);
                    if click == widgets::Click::Double && !shift {
                        // control.js:917 두 번 = 그 절이 든 슬라이드를 미리보기하고 송출
                        st.kbd = true;
                        activate(i, self, &mut st, acts);
                        acts.push(Act::SendPreview)
                    } else if click != widgets::Click::None {
                        st.kbd = true;
                        if shift {
                            select_range(i, self, &mut st, acts);
                        } else {
                            st.anchor = Some(i);
                            st.sel = Some(i);
                            // 범위 밖을 누르면 범위 해제(전체 본문을 그 절부터), 범위 안이면 범위 유지
                            if st.range.is_some_and(|(a, b)| i < a || i > b) {
                                st.range = None;
                                if let Some(t) = p["title"].as_str() {
                                    self.search = t.to_string();
                                    st.seen_search = t.to_string();
                                }
                                acts.push(Act::PreviewPassage(p.clone(), self.search.clone(), Some((c, n))));
                            } else {
                                activate(i, self, &mut st, acts);
                            }
                        }
                    }
                }
            } else {
                let rows = r["results"].as_array().cloned().unwrap_or_default();
                let terms: Vec<String> = r["terms"].as_array().map(|t| t.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_else(|| self.search.split_whitespace().map(String::from).collect());
                if rows.is_empty() {
                    empty(ui, &tr("검색 결과가 없습니다.\n다른 단어나 더 짧은 단어로 검색해 보세요."));
                }
                let q_of = |v: &Value| format!("{}{}:{}", v["bookShort"].as_str().unwrap_or(""), v["chapter"], v["verse"]);
                match key {
                    Some(ListKey::Move(i)) => {
                        st.sel = Some(i);
                        st.scroll_sel = true;
                    }
                    Some(ListKey::Activate(i)) => {
                        if let Some(v) = rows.get(i) {
                            acts.push(Act::PreviewRef(q_of(v), None));
                        }
                    }
                    Some(ListKey::ToSearch) => {
                        st.kbd = false;
                        self.focus_search = true;
                    }
                    _ => {}
                }
                for (i, v) in rows.iter().enumerate() {
                    let q = q_of(v);
                    let k = vkey(v["book"].as_i64().unwrap_or(0), v["chapter"].as_i64().unwrap_or(0), v["verse"].as_i64().unwrap_or(0));
                    let m = Mark { pv: pv_keys.iter().any(|x| same_verse(*x, k)), live: live_keys.iter().any(|x| same_verse(*x, k)), sel: st.sel == Some(i), range: false };
                    let resp = text_row(ui, &q, v["text"].as_str().unwrap_or(""), &terms, m);
                    if m.sel && st.scroll_sel {
                        resp.scroll_to_me(None);
                        st.scroll_sel = false;
                    }
                    let vref = v["ref"].as_str().unwrap_or("");
                    match crate::ctx_ui::verse_menu(&resp, vref, vref, v["text"].as_str().unwrap_or("")) {
                        Some(crate::ctx_ui::VerseAct::Preview) => {
                            st.sel = Some(i);
                            acts.push(Act::PreviewRef(q.clone(), None));
                        }
                        Some(crate::ctx_ui::VerseAct::Send) => {
                            st.sel = Some(i);
                            acts.push(Act::SendRef(q.clone(), None));
                        }
                        Some(crate::ctx_ui::VerseAct::AddCue) => {
                            // ctxmenu.js: 그 절을 미리보기한 뒤 { title: '성경봉독', ref: 본문 제목 } 을 넣는다
                            let r = if vref.is_empty() { q.clone() } else { vref.to_string() };
                            acts.push(Act::AddItem("bible", Some(json!({ "title": tr("성경 봉독"), "ref": r }))));
                            self.core.lock().notify(tr_args("'{1}' 을(를) 큐시트에 넣었습니다.", &[&r]), "ok");
                        }
                        None => {}
                    }
                    match widgets::click2(ui, ("res-text", &q), &resp) {
                        widgets::Click::Single => {
                            st.kbd = true;
                            st.sel = Some(i);
                            acts.push(Act::PreviewRef(q.clone(), None))
                        }
                        widgets::Click::Double => {
                            st.kbd = true;
                            st.sel = Some(i);
                            acts.push(Act::SendRef(q, None))
                        }
                        widgets::Click::None => {}
                    }
                }
                if r["truncated"] == json!(true) {
                    empty(ui, &tr("결과가 많아 일부만 표시합니다. 검색어를 더 구체적으로 입력하세요."));
                }
            }
            ui.add_space(6.0);
        });
        save(ui.ctx(), st);
    }

    /// 큐시트 순서를 미리보기 중: 성경은 절 목록(showPassageList), 나머지는 슬라이드 목록(showSlideList)
    fn item_list(&mut self, ui: &mut egui::Ui, st: &mut CenterState, s: &Snap, acts: &mut Vec<Act>) {
        let Some(d) = s.preview.as_ref() else { return };
        let live_same = s.live.as_ref().filter(|l| l.deck.passage["cueId"] == d.passage["cueId"] && l.deck.passage["cueIndex"] == d.passage["cueIndex"]).map(|l| l.deck.index);
        let pv_keys = slide_keys(d.slides.get(d.index));
        let lkeys = live_keys(s.live.as_ref());
        let scroll_key = format!("{}:{}:{}", d.passage["cueId"], d.passage["cueIndex"], d.index);
        let item = deck_item(s, d);
        let item_type = d.passage["itemType"].as_str().unwrap_or("").to_string();
        let c = paint::Ctx { web_dir: &self.web_dir, data_dir: &self.data_dir };
        // 성경 순서는 절 목록(showPassageList)이라 ↑↓·Enter 가 있다. 목록이 바뀌면 고른 줄 초기화
        let verses = if item_type == "bible" { bible_verses(d) } else { vec![] };
        let rk = format!("item|{}|{}|{}", d.passage["cueId"], d.passage["cueIndex"], verses.len());
        if rk != st.res_key {
            st.res_key = rk;
            st.sel = None;
            st.anchor = None;
            st.range = None;
        }
        // 성경 순서도 원본은 검색 본문과 같은 절 목록(showPassageList)이라 Shift+↑↓ 범위가 된다
        let key = list_keys(ui.ctx(), st, verses.len(), item_type == "bible");
        let mut menu_act: Option<MenuAct> = None;
        let mut multi_act: Option<&'static str> = None;
        let mut range_req: Option<usize> = None;
        let chunk = item.is_some_and(|x| wp_core::slideedit::is_chunk(x.1["type"].as_str().unwrap_or("")));
        egui::ScrollArea::vertical().id_salt("results-item").auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.add_space(6.0);
            if d.passage["itemType"] == "audio" {
                let f = d.passage["audioItem"]["file"].as_str().unwrap_or("");
                let name = f.rsplit(['\\', '/']).next().unwrap_or("").to_string();
                empty(ui, &format!("{}\n{}", if name.is_empty() { tr("(음악 파일을 고르세요)") } else { name }, tr("F9 = 재생 · 화면은 바뀌지 않습니다")));
                return;
            }
            if item_type == "bible" {
                let idx_of = |i: usize| verses.get(i).map(|v| wp_core::slides::index_of_verse(&d.slides, v.1, v.2));
                match key {
                    Some(ListKey::Move(i)) => {
                        st.sel = Some(i);
                        st.scroll_sel = true;
                    }
                    Some(ListKey::Activate(i)) => {
                        if let Some(k) = idx_of(i) {
                            acts.push(Act::PreviewIndex(k));
                        }
                    }
                    Some(ListKey::Range(i)) => range_req = Some(i),
                    Some(ListKey::ToSearch) => {
                        st.kbd = false;
                        self.focus_search = true;
                    }
                    None => {}
                }
                let multi = verses.first().map(|v| v.1) != verses.last().map(|v| v.1);
                let mut last_ch = -1;
                for (i, (k, ch, vn, text)) in verses.iter().enumerate() {
                    let (k, ch, vn) = (*k, *ch, *vn);
                    if multi && ch != last_ch {
                        heading(ui, &tr_args("{1}장", &[&ch]));
                    }
                    last_ch = ch;
                    let label = if multi { format!("{ch}:{vn}") } else { vn.to_string() };
                    let pv = pv_keys.iter().any(|x| same_verse(*x, k));
                    let m = Mark { pv, live: lkeys.iter().any(|x| same_verse(*x, k)), sel: st.sel == Some(i), range: false };
                    let resp = verse_row(ui, &label, text, m);
                    #[cfg(test)]
                    widgets::HOVER_SEEN.with(|h| h.borrow_mut().insert(format!("row-{i}"), resp.rect));
                    if pv && st.scroll_key != scroll_key {
                        resp.scroll_to_me(None);
                        st.scroll_key = scroll_key.clone();
                    }
                    if m.sel && st.scroll_sel {
                        resp.scroll_to_me(None);
                        st.scroll_sel = false;
                    }
                    let idx = wp_core::slides::index_of_verse(&d.slides, ch, vn);
                    let vref = format!("{} {}:{}", d.passage["book"]["name"].as_str().unwrap_or(""), ch, vn);
                    match crate::ctx_ui::verse_menu(&resp, &format!("{} · {}", d.passage["title"].as_str().unwrap_or(""), tr_args("{1}절", &[&label])), &vref, text) {
                        Some(crate::ctx_ui::VerseAct::Preview) => {
                            st.sel = Some(i);
                            acts.push(Act::PreviewIndex(idx));
                        }
                        Some(crate::ctx_ui::VerseAct::Send) => {
                            st.sel = Some(i);
                            acts.push(Act::PreviewIndex(idx));
                            acts.push(Act::SendPreview);
                        }
                        Some(crate::ctx_ui::VerseAct::AddCue) => {
                            let r = d.passage["title"].as_str().unwrap_or("").to_string();
                            if !r.is_empty() {
                                acts.push(Act::AddItem("bible", Some(json!({ "title": tr("성경 봉독"), "ref": r }))));
                                self.core.lock().notify(tr_args("'{1}' 을(를) 큐시트에 넣었습니다.", &[&r]), "ok");
                            }
                        }
                        None => {}
                    }
                    let click = widgets::click2(ui, ("cue-verse", ch, vn), &resp);
                    let shift = ui.input(|x| x.modifiers.shift);
                    if click == widgets::Click::Double && !shift {
                        st.kbd = true;
                        st.anchor = Some(i);
                        st.sel = Some(i);
                        acts.push(Act::PreviewIndex(idx));
                        acts.push(Act::SendPreview);
                    } else if click != widgets::Click::None {
                        st.kbd = true;
                        if shift {
                            // Shift+클릭: 처음 누른 절부터 여기까지를 범위로 미리보기 (control.js selectRange)
                            range_req = Some(i);
                        } else {
                            st.anchor = Some(i);
                            st.sel = Some(i);
                            acts.push(Act::PreviewIndex(idx));
                        }
                    }
                }
            } else {
                let many = ms_many(st, s);
                let mut cells: Vec<(usize, Rect)> = vec![];
                for (k, sl) in d.slides.iter().enumerate() {
                    let l = sl["label"].as_str().filter(|x| !x.is_empty()).map(String::from).unwrap_or_else(|| (k + 1).to_string());
                    let msel = many.as_ref().is_some_and(|(_, ks)| ks.contains(&k));
                    let resp = slide_row(ui, &c, &l, group_color(sl), &srow_body(sl, &item_type), k == d.index, live_same == Some(k), msel);
                    cells.push((k, resp.rect));
                    #[cfg(test)]
                    widgets::HOVER_SEEN.with(|h| h.borrow_mut().insert(format!("srow-{k}"), resp.rect));
                    if resp.drag_started_by(egui::PointerButton::Primary) {
                        if let Some((i, _)) = item {
                            drag_begin(ui, st, s, i, k);
                        }
                    }
                    if let Some(op) = widgets::hover_ops(ui, egui::Id::new(("srow", k)), resp.rect, &slide_hover_ops(item.map(|x| x.1), sl, true)) {
                        if let Some((i, _)) = item {
                            acts.push(Act::SlideOp(i, slide_key(sl), op));
                        }
                    }
                    // ctxmenu.js slideMenu (여러 장을 고른 중이면 고른 것 메뉴)
                    if let Some((_, ks)) = &many {
                        if let Some(a) = multi_menu(&resp, ks.len(), chunk) {
                            multi_act = Some(a);
                        }
                    } else if let Some(a) = slide_menu(&resp, k, sl, item, None, st.view) {
                        menu_act = Some(a);
                    }
                    if k == d.index && st.scroll_key != scroll_key {
                        resp.scroll_to_me(None);
                        st.scroll_key = scroll_key.clone();
                    }
                    let click = widgets::click2(ui, ("srow", slide_key(sl), k), &resp);
                    let mods = ui.input(|i| i.modifiers);
                    if click == widgets::Click::Double && !(mods.shift || mods.command) {
                        // cue.js:945 두 번 = 그 장 송출
                        st.kbd = true;
                        acts.push(Act::PreviewIndex(k));
                        acts.push(Act::SendPreview);
                    } else if click != widgets::Click::None {
                        st.kbd = true;
                        // Shift·Ctrl+클릭 = 여러 장 고르기, 그냥 클릭 = 그 슬라이드 미리보기
                        if !ms_click(st, s, k, mods) {
                            acts.push(Act::PreviewIndex(k));
                        }
                    }
                }
                if let Some((i, _)) = item {
                    drag_update(ui, st, s, i, &cells, false, acts);
                }
            }
            ui.add_space(6.0);
        });
        if let Some(a) = menu_act {
            self.menu_act(a, st, s, acts);
        }
        if let Some(a) = multi_act {
            self.msel_do(ui.ctx(), st, s, a, false, acts);
        }
        if let Some(i) = range_req {
            self.cue_bible_range(st, d, &verses, i, acts);
        }
    }

    /// 성경 순서 절 목록에서 Shift 범위 (control.js selectRange): 그 절들만 담은 본문을 미리보기로.
    /// 원본처럼 목록은 그대로(검색한 본문 목록으로 이어서) 두고 범위만 표시한다
    fn cue_bible_range(&mut self, st: &mut CenterState, d: &Deck, verses: &[((i64, i64, i64), i64, i64, String)], i: usize, acts: &mut Vec<Act>) {
        if verses.is_empty() {
            return;
        }
        let i = i.min(verses.len() - 1);
        let full = json!({
            "title": d.passage["title"], "book": d.passage["book"], "versionId": d.passage["versionId"], "versionName": d.passage["versionName"],
            "verses": verses.iter().map(|(k, c, v, t)| json!({ "book": k.0, "chapter": c, "verse": v, "text": t })).collect::<Vec<_>>(),
            "headings": [],
        });
        let a = *st.anchor.get_or_insert(st.sel.unwrap_or(i));
        let rg = (a.min(i), a.max(i));
        let Some((sub, short)) = range_passage(&full, rg.0, rg.1) else { return };
        self.result = json!({ "ok": true, "mode": "reference", "passage": full });
        // 검색 결과 목록이 되지만 고른 줄·범위는 그대로 (res_key 를 맞춰 초기화하지 않게)
        st.res_key = res_key_of(&self.result, verses.len());
        st.range = Some(rg);
        st.sel = Some(i);
        st.scroll_sel = true;
        st.cue_mode = false;
        self.search = short.clone();
        st.seen_search = short.clone();
        acts.push(Act::PreviewPassage(sub, short, None));
    }

    /// 고른 여러 장으로 하기 (slideedit.js selAction): 'edit' 함께 꾸미기 · 'del' 지우기/건너뛰기 · 'clear' 한 장만.
    /// 글 순서(가사·내용에서 지워짐)를 지울 때는 먼저 묻는다 (`confirmed` 면 이미 물었음)
    fn msel_do(&mut self, ctx: &egui::Context, st: &mut CenterState, s: &Snap, a: &str, confirmed: bool, acts: &mut Vec<Act>) {
        let Some((i, ks)) = ms_many(st, s) else { return };
        let Some(d) = s.preview.as_ref() else { return };
        match a {
            "clear" => ms_reset(st, s, Some(d.index)),
            "edit" => {
                self.sel = Some(i);
                self.tc.slideedit.open_many(&ms_keys(d, &ks));
            }
            "del" => {
                let chunk = s.items.get(i).is_some_and(|it| wp_core::slideedit::is_chunk(it["type"].as_str().unwrap_or("")));
                if chunk && !confirmed {
                    ctx.data_mut(|m| m.insert_temp(ms_confirm_id(), true));
                    return;
                }
                ctx.data_mut(|m| m.insert_temp(ms_confirm_id(), false));
                acts.push(Act::SlideDelMany(i, ms_keys(d, &ks)));
                st.ms_item = None;
                st.ms.clear();
                st.ms_anchor = None;
            }
            _ => {}
        }
    }

    /// 여러 장 고르기 키 (slideedit.js selKey): Ctrl+A 모두 · Esc 한 장만 · Delete 지우기 · Ctrl+E 함께 꾸미기.
    /// 전역 단축키(Esc = 보조 화면 끄기 등)보다 먼저 부른다. 슬라이드 편집기가 열려 있으면 편집기 목록이 먼저
    pub(crate) fn msel_keys(&mut self, ctx: &egui::Context, acts: &mut Vec<Act>) {
        use egui::{Key, Modifiers};
        let (esc, del, all, edit) = ctx.input(|i| (i.key_pressed(Key::Escape), i.key_pressed(Key::Delete), i.modifiers.command && i.key_pressed(Key::A), i.modifiers.command && !i.modifiers.shift && i.key_pressed(Key::E)));
        if !(esc || del || all || edit) {
            return;
        }
        if self.tc.slideedit.open {
            self.tc.slideedit.list_keys(ctx);
            return;
        }
        let s = self.snap();
        let mut st = load(ctx);
        let many = ms_many(&st, &s);
        if edit && many.is_some() {
            ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::E));
            self.msel_do(ctx, &mut st, &s, "edit", false, acts);
        } else if st.kbd {
            if all {
                if let Some((_, pv, n)) = pv_item(&s) {
                    ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, Key::A));
                    ms_reset(&mut st, &s, Some(pv));
                    st.ms = (0..n).collect();
                }
            } else if many.is_some() && esc {
                ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
                ctx.data_mut(|m| m.insert_temp(ms_confirm_id(), false));
                let pv = s.preview.as_ref().map(|d| d.index);
                ms_reset(&mut st, &s, pv);
            } else if many.is_some() && del {
                ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Delete));
                self.msel_do(ctx, &mut st, &s, "del", false, acts);
            }
        }
        save(ctx, st);
    }

    /// 오른쪽 클릭 메뉴에서 고른 것 적용
    fn menu_act(&mut self, a: MenuAct, st: &mut CenterState, s: &Snap, acts: &mut Vec<Act>) {
        match a {
            MenuAct::Preview(i, k, send) => {
                if let Some(i) = i {
                    self.sel = Some(i);
                    let same = s.preview.as_ref().is_some_and(|d| deck_item(s, d).map(|x| x.0) == Some(i));
                    if !same {
                        acts.push(Act::PreviewItem(i));
                    }
                }
                acts.push(Act::PreviewIndex(k));
                if send {
                    acts.push(Act::SendPreview);
                }
            }
            MenuAct::Op(i, k, key, op) => {
                self.sel = Some(i);
                let same = s.preview.as_ref().is_some_and(|d| deck_item(s, d).map(|x| x.0) == Some(i));
                if !same {
                    acts.push(Act::PreviewItem(i));
                }
                acts.push(Act::PreviewIndex(k));
                acts.push(Act::SlideOp(i, key, op));
            }
            MenuAct::Split(i, k) => {
                self.sel = Some(i);
                split_item(&self.core, i, k);
            }
            MenuAct::ItemEditor(i) => self.show_item_editor(i),
            MenuAct::View(v) => {
                st.view = v;
                st.all_key.clear();
            }
        }
    }

    /// 격자: 미리보기 중인 순서·본문의 슬라이드 썸네일 (초록 = 미리보기, 빨강 = 송출)
    fn grid_view(&mut self, ui: &mut egui::Ui, st: &mut CenterState, s: &Snap, acts: &mut Vec<Act>) {
        let Some(d) = s.preview.as_ref() else { return };
        let tpl = self.core.lock().template_for_deck("main", Some(d));
        let live = s.live.as_ref().filter(|l| {
            let lp = &l.deck.passage;
            if cue_index(d).is_some() { lp["cueId"] == d.passage["cueId"] && lp["cueIndex"] == d.passage["cueIndex"] } else { cue_index(&l.deck).is_none() && lp["title"] == d.passage["title"] && l.deck.slides.len() == d.slides.len() }
        });
        let c = paint::Ctx { web_dir: &self.web_dir, data_dir: &self.data_dir };
        let size = st.size;
        let item = deck_item(s, d);
        let ops = |sl: &Value| slide_hover_ops(item.map(|x| x.1), sl, false);
        let many = ms_many(st, s);
        let chunk = item.is_some_and(|x| wp_core::slideedit::is_chunk(x.1["type"].as_str().unwrap_or("")));
        let mut multi_act: Option<&'static str> = None;
        let mut menu_act: Option<MenuAct> = None;
        egui::ScrollArea::vertical().id_salt("slide-grid").auto_shrink([false, false]).show(ui, |ui| {
            egui::Frame::new().inner_margin(Margin::same(10)).show(ui, |ui| {
                let msel = |k: usize| many.as_ref().is_some_and(|(_, ks)| ks.contains(&k));
                let mut cells = vec![];
                let hit = thumb_cells(ui, egui::Id::new("sg"), &c, &d.slides, &tpl, d.passage["versionName"].as_str().unwrap_or(""), Some(d.index), live.map(|l| l.deck.index), size, &ops, &msel, &mut cells);
                // 여러 장을 고른 중 오른쪽 클릭 = 고른 것 메뉴
                if let Some((_, ks)) = &many {
                    for (k, r) in &cells {
                        let resp = ui.interact(*r, egui::Id::new("sg").with(*k), Sense::click_and_drag());
                        if let Some(a) = multi_menu(&resp, ks.len(), chunk) {
                            multi_act = Some(a);
                        }
                    }
                } else {
                    // 한 장 오른쪽 클릭 = 슬라이드 메뉴 (ctxmenu.js: #slideGrid .sg-cell → slideMenu)
                    for (k, r) in &cells {
                        let resp = ui.interact(*r, egui::Id::new("sg").with(*k), Sense::click_and_drag());
                        if let Some(a) = d.slides.get(*k).and_then(|sl| slide_menu(&resp, *k, sl, item, None, st.view)) {
                            menu_act = Some(a);
                        }
                    }
                }
                match hit {
                    Some(Hit::Pick(k, dbl)) => {
                        st.kbd = true;
                        let m = ui.input(|i| i.modifiers);
                        if dbl && !(m.shift || m.command) {
                            acts.push(Act::PreviewIndex(k));
                            acts.push(Act::SendPreview);
                        } else if !ms_click(st, s, k, m) {
                            acts.push(Act::PreviewIndex(k));
                        }
                    }
                    Some(Hit::Op(k, op)) => {
                        if let (Some((i, _)), Some(sl)) = (item, d.slides.get(k)) {
                            acts.push(Act::SlideOp(i, slide_key(sl), op));
                        }
                    }
                    Some(Hit::Drag(k)) => {
                        if let Some((i, _)) = item {
                            drag_begin(ui, st, s, i, k);
                        }
                    }
                    None => {}
                }
                if let Some((i, _)) = item {
                    drag_update(ui, st, s, i, &cells, true, acts);
                }
            });
        });
        if let Some(a) = multi_act {
            self.msel_do(ui.ctx(), st, s, a, false, acts);
        }
        if let Some(a) = menu_act {
            self.menu_act(a, st, s, acts);
        }
    }

    /// 전체 격자 · 전체 목록 (ProPresenter 방식): 큐시트의 모든 순서 슬라이드를 순서별 줄로
    fn all_view(&mut self, ui: &mut egui::Ui, st: &mut CenterState, s: &Snap, acts: &mut Vec<Act>) {
        let (cue_id, items) = {
            let c = self.core.lock();
            c.cue.as_ref().map(|q| (q.id.clone(), q.items.clone())).unwrap_or_default()
        };
        // 끝의 번호: 찬송가 PPT 그림 만들기가 끝날 때마다 바뀐다 — '만드는 중' 이던 순서가 다 만든 뒤 저절로 채워지게
        let key = format!("{}|{}|{}|{}", cue_id, serde_json::to_string(&items).unwrap_or_default(), serde_json::to_string(&s.tpl_main).unwrap_or_default().len(), wp_core::hymns::build_gen());
        if key != st.all_key {
            st.all_key = key;
            let mut c = self.core.lock();
            let secs: Vec<AllSec> = (0..items.len())
                .map(|i| {
                    let deck = c.item_deck(i);
                    let tpl = c.template_for_deck("main", deck.as_ref().ok());
                    AllSec { deck, tpl }
                })
                .collect();
            st.all = Arc::new(secs);
        }
        let secs = st.all.clone();
        let p = pal();
        let pv = s.preview.as_ref().filter(|d| d.passage["cueId"] == cue_id.as_str()).and_then(|d| cue_index(d).map(|i| (i, d.index)));
        let lv = s.live.as_ref().filter(|l| l.deck.passage["cueId"] == cue_id.as_str()).and_then(|l| cue_index(&l.deck).map(|i| (i, l.deck.index)));
        let c = paint::Ctx { web_dir: &self.web_dir, data_dir: &self.data_dir };
        let list = st.view == SlideView::AllList;
        let size = st.size;
        let scroll_key = format!("all:{:?}", pv.map(|x| x.0));
        let mut new_scroll = None;
        let mut menu_act: Option<MenuAct> = None;
        let mut multi_act: Option<&'static str> = None;
        egui::ScrollArea::vertical().id_salt("cue-grid").auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            egui::Frame::new().inner_margin(Margin { left: 10, right: 10, top: 4, bottom: 40 }).show(ui, |ui| {
                for (i, (it, sec)) in items.iter().zip(secs.iter()).enumerate() {
                    let is_pv = pv.map(|x| x.0) == Some(i);
                    let is_lv = lv.map(|x| x.0) == Some(i);
                    let top = ui.cursor().top();
                    let mut head_rect = Rect::NOTHING;
                    // .cg-sec: 왼쪽 3px 띠(미리보기 초록 · 송출 빨강), 안쪽 6px
                    let inner = egui::Frame::new().inner_margin(Margin { left: 9, right: 0, top: 0, bottom: 6 }).show(ui, |ui| {
                        // .cg-head: 14px 600, 번호 hl, 슬라이드 수 흐리게, 아래 line-soft
                        let n = sec.deck.as_ref().map(|d| d.slides.len()).unwrap_or(0);
                        let head_w = ui.available_width();
                        let (hr, hresp) = ui.allocate_exact_size(Vec2::new(head_w, CG_HEAD_H), Sense::click());
                        head_rect = hr;
                        if ui.is_rect_visible(hr) {
                            paint_cg_head(ui, hr, i, it, n, hresp.hovered());
                        }
                        let hresp = hresp.on_hover_text(if !self.core.lock().direct_mode() { tr("이 순서 미리보기") } else { tr("이 순서 바로 송출") });
                        if hresp.clicked() {
                            self.sel = Some(i);
                            acts.push(Act::PreviewItem(i));
                        }
                        if is_pv && st.scroll_key != scroll_key {
                            new_scroll = Some(hresp.rect);
                        }
                        match &sec.deck {
                            Ok(d) if !d.slides.is_empty() => {
                                let pvk = pv.filter(|x| x.0 == i).map(|x| x.1);
                                let lvk = lv.filter(|x| x.0 == i).map(|x| x.1);
                                // 여러 장 고르기는 미리보기 중인 순서의 칸만
                                let many = ms_many(st, s).filter(|x| x.0 == i);
                                let chunk = wp_core::slideedit::is_chunk(it["type"].as_str().unwrap_or(""));
                                let mut cells: Vec<(usize, Rect)> = vec![];
                                let hit = if list {
                                    ui.add_space(4.0);
                                    let mut hit = None;
                                    for (k, sl) in d.slides.iter().enumerate() {
                                        let msel = many.as_ref().is_some_and(|(_, ks)| ks.contains(&k));
                                        let r = all_line(ui, k, sl, pvk == Some(k), lvk == Some(k), msel);
                                        cells.push((k, r.rect));
                                        #[cfg(test)]
                                        widgets::HOVER_SEEN.with(|h| h.borrow_mut().insert(format!("cgl-{i}-{k}"), r.rect));
                                        if r.drag_started_by(egui::PointerButton::Primary) {
                                            hit = Some(Hit::Drag(k));
                                        }
                                        match widgets::click2(ui, ("cgl", i, k), &r) {
                                            widgets::Click::Single => hit = Some(Hit::Pick(k, false)),
                                            widgets::Click::Double => hit = Some(Hit::Pick(k, true)),
                                            widgets::Click::None => {}
                                        }
                                        if let Some(op) = widgets::hover_ops(ui, egui::Id::new(("cgl", i, k)), r.rect, &slide_hover_ops(Some(it), sl, true)) {
                                            hit = Some(Hit::Op(k, op));
                                        }
                                        if let Some((_, ks)) = &many {
                                            if let Some(a) = multi_menu(&r, ks.len(), chunk) {
                                                multi_act = Some(a);
                                            }
                                        } else if let Some(a) = slide_menu(&r, k, sl, Some((i, it)), Some(i), st.view) {
                                            menu_act = Some(a);
                                        }
                                        ui.add_space(2.0);
                                    }
                                    hit
                                } else {
                                    ui.add_space(8.0);
                                    let ops = |sl: &Value| slide_hover_ops(Some(it), sl, false);
                                    let msel = |k: usize| many.as_ref().is_some_and(|(_, ks)| ks.contains(&k));
                                    let hit = thumb_cells(ui, egui::Id::new(("cg", i)), &c, &d.slides, &sec.tpl, d.passage["versionName"].as_str().unwrap_or(""), pvk, lvk, size, &ops, &msel, &mut cells);
                                    // 전체 격자 칸 오른쪽 클릭 (ctxmenu.js: #cueGrid .sg-cell — 그 순서의 그 슬라이드)
                                    for (k, r) in &cells {
                                        let resp = ui.interact(*r, egui::Id::new(("cg", i)).with(*k), Sense::click_and_drag());
                                        if let Some((_, ks)) = &many {
                                            if let Some(a) = multi_menu(&resp, ks.len(), chunk) {
                                                multi_act = Some(a);
                                            }
                                        } else if let Some(a) = d.slides.get(*k).and_then(|sl| slide_menu(&resp, *k, sl, Some((i, it)), Some(i), st.view)) {
                                            menu_act = Some(a);
                                        }
                                    }
                                    hit
                                };
                                match hit {
                                    Some(Hit::Pick(k, dbl)) => {
                                        st.kbd = true;
                                        // 이 화면 안에서 누른 것은 이미 보고 있는 자리다 — 그 순서를 맨 위로 끌어올리지 않는다
                                        // (끌어올리면 칸이 수백 px 밀려 두 번째 클릭이 다른 칸에 떨어졌다)
                                        st.scroll_key = format!("all:{:?}", Some(i));
                                        let m = ui.input(|x| x.modifiers);
                                        // Shift·Ctrl+클릭 = 여러 장 고르기 (미리보기 중인 순서에서만, selClickAll)
                                        if dbl && !(m.shift || m.command) {
                                            self.sel = Some(i);
                                            acts.push(Act::PreviewItem(i));
                                            acts.push(Act::PreviewIndex(k));
                                            acts.push(Act::SendPreview);
                                        } else if !ms_click_all(st, s, i, k, m) {
                                            self.sel = Some(i);
                                            if pvk.is_none() {
                                                acts.push(Act::PreviewItem(i));
                                            }
                                            acts.push(Act::PreviewIndex(k));
                                        }
                                    }
                                    Some(Hit::Drag(k)) => drag_begin(ui, st, s, i, k),
                                    Some(Hit::Op(k, op)) => {
                                        st.scroll_key = format!("all:{:?}", Some(i));
                                        // grid.js: 그 슬라이드를 먼저 미리보기한 뒤 동작
                                        self.sel = Some(i);
                                        if pvk.is_none() {
                                            acts.push(Act::PreviewItem(i));
                                        }
                                        acts.push(Act::PreviewIndex(k));
                                        if let Some(sl) = d.slides.get(k) {
                                            acts.push(Act::SlideOp(i, slide_key(sl), op));
                                        }
                                    }
                                    None => {}
                                }
                                drag_update(ui, st, s, i, &cells, !list, acts);
                            }
                            // .cg-empty: padding 10px 4px, 13px 흐리게 (음악은 스피커 아이콘)
                            Ok(_) => {
                                ui.add_space(10.0);
                                ui.horizontal(|ui| {
                                    ui.add_space(4.0);
                                    if it["type"] == "audio" {
                                        icons::show(ui, "volume-2", 14.0, p.muted);
                                    }
                                    ui.label(rich(if it["type"] == "audio" { tr("음악 · 화면은 바뀌지 않습니다") } else { tr("(슬라이드 없음)") }, 13.0, Weight::Regular, p.muted));
                                });
                                ui.add_space(10.0);
                            }
                            Err(e) => {
                                ui.add_space(10.0);
                                ui.horizontal(|ui| {
                                    ui.add_space(4.0);
                                    ui.label(rich(e, 13.0, Weight::Regular, p.muted));
                                });
                                ui.add_space(10.0);
                            }
                        }
                    });
                    let band = if is_lv { p.live } else if is_pv { p.pv } else { Color32::TRANSPARENT };
                    let r = inner.response.rect;
                    ui.painter().rect_filled(Rect::from_min_max(Pos2::new(r.left(), top), Pos2::new(r.left() + 3.0, r.bottom())), CornerRadius::ZERO, band);
                    // .cg-head { position: sticky; top: 0 }: 순서 머리가 위로 넘어가면 그 순서가 끝날 때까지 맨 위에 붙어 있다
                    let clip = ui.clip_rect();
                    if head_rect.top() < clip.top() && r.bottom() > clip.top() {
                        let y = clip.top().min(r.bottom() - CG_HEAD_H);
                        let sticky = Rect::from_min_size(Pos2::new(head_rect.left(), y), head_rect.size());
                        let n = sec.deck.as_ref().map(|d| d.slides.len()).unwrap_or(0);
                        let sr = ui.interact(sticky, egui::Id::new(("cg-sticky", i)), Sense::click());
                        // 같은 층에 나중에 그리고 나중에 등록하므로 밑의 줄보다 위에 보이고 먼저 눌린다
                        paint_cg_head(ui, sticky, i, it, n, sr.hovered());
                        if sr.clicked() {
                            self.sel = Some(i);
                            acts.push(Act::PreviewItem(i));
                        }
                    }
                }
            });
            if let Some(r) = new_scroll {
                ui.scroll_to_rect(r, Some(egui::Align::Min));
                st.scroll_key = scroll_key.clone();
            }
        });
        if let Some(a) = menu_act {
            self.menu_act(a, st, s, acts);
        }
        if let Some(a) = multi_act {
            self.msel_do(ui.ctx(), st, s, a, false, acts);
        }
    }

    /// 주석 탭 자료 (미리보기 범위가 바뀔 때만 다시 읽음)
    fn cm_refresh(&mut self, st: &mut CenterState, s: &Snap) {
        let range = s.preview.as_ref().and_then(|d| {
            let first = d.slides.first()?["parts"].as_array()?.first()?.clone();
            let last = d.slides.last()?["parts"].as_array()?.last()?.clone();
            let b = first["book"].as_i64().filter(|b| *b > 0)?;
            Some((b, first["chapter"].as_i64()?, first["verse"].as_i64()?, last["chapter"].as_i64()?, last["verse"].as_i64()?))
        });
        let key = format!("{range:?}");
        if key == st.cm_key {
            return;
        }
        st.cm_key = key;
        st.cm_open.clear();
        let c = self.core.lock();
        st.cm_installed = !wp_core::commentary::list(&c.store.db).is_empty();
        st.cm_rows = Arc::new(match (range, st.cm_installed) {
            (Some((b, sc, sv, ec, ev)), true) => wp_core::commentary::commentary_for(&c.store.db, b, sc, sv, ec, ev, None),
            _ => vec![],
        });
    }

    /// 주석 탭 (control.js renderCommentary): 주석서마다 머리, 지금 슬라이드에 걸치는 항목은 초록 띠
    fn cm_view(&mut self, ui: &mut egui::Ui, st: &mut CenterState, s: &Snap) {
        let p = pal();
        let cur = s.preview.as_ref().map(|d| slide_keys(d.slides.get(d.index))).unwrap_or_default();
        let (smin, smax) = (cur.iter().map(|k| k.1 * 1000 + k.2).min().unwrap_or(0), cur.iter().map(|k| k.1 * 1000 + k.2).max().unwrap_or(-1));
        egui::ScrollArea::vertical().id_salt("cm-view").auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.add_space(6.0);
            if !st.cm_installed {
                empty(ui, &tr("설치된 주석이 없습니다.\n설정 → 성경 데이터에서 주석서(EPUB)를 가져오세요."));
                ui.vertical_centered(|ui| {
                    if ui.add(Btn::new(tr("주석 가져오기…")).size(Size::Sm)).clicked() {
                        self.tc.commentary.open = true;
                    }
                });
                return;
            }
            if s.preview.is_none() {
                empty(ui, &tr("본문을 검색하면 그 범위의 주석이 여기에 나옵니다."));
                return;
            }
            if st.cm_rows.is_empty() {
                empty(ui, &tr("이 범위에 해당하는 주석 항목이 없습니다."));
                return;
            }
            let mut last = String::new();
            let rows = st.cm_rows.clone();
            for (i, it) in rows.iter().enumerate() {
                let cname = it["cname"].as_str().unwrap_or("");
                if cname != last {
                    heading(ui, &tr_args("{1} · 조작 화면 참고용 (송출되지 않음)", &[&cname]));
                    last = cname.to_string();
                }
                let n = |k: &str| it[k].as_i64().unwrap_or(0);
                let (a, b) = (n("s"), n("e"));
                let hit = a <= smax && b >= smin;
                let span = if n("sc") == n("ec") { if n("sv") == n("ev") { format!("{}:{}", n("sc"), n("sv")) } else { format!("{}:{}-{}", n("sc"), n("sv"), n("ev")) } } else { format!("{}:{}-{}:{}", n("sc"), n("sv"), n("ec"), n("ev")) };
                let open = st.cm_open.contains(&i);
                let article = it["kind"] == "article";
                let w = row_width(ui);
                let mut rj = egui::text::LayoutJob::default();
                rj.append(&format!("{}{}", if article { tr("[글] ") } else { String::new() }, span), 0.0, egui::TextFormat { font_id: theme::font(13.0, Weight::Regular), color: p.gold, ..Default::default() });
                let t = it["title"].as_str().unwrap_or("");
                if !t.is_empty() {
                    rj.append(t, 6.0, egui::TextFormat { font_id: theme::font(14.0, Weight::Bold), color: p.text, ..Default::default() });
                }
                rj.wrap.max_width = w - 32.0;
                let rg = ui.painter().layout_job(rj);
                let mut tj = egui::text::LayoutJob::single_section(it["text"].as_str().unwrap_or("").to_string(), egui::TextFormat { font_id: theme::font(14.0, Weight::Regular), color: p.text2, line_height: Some(14.0 * 1.7), ..Default::default() });
                tj.wrap.max_width = w - 32.0;
                if !open {
                    tj.wrap.max_rows = 4;
                }
                let tg = ui.painter().layout_job(tj);
                let h = 8.0 + rg.size().y + 4.0 + tg.size().y + 10.0;
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click());
                if ui.is_rect_visible(rect) {
                    let pt = ui.painter();
                    let bg = if hit { p.pv_bg } else if article { p.panel2 } else if resp.hovered() { p.hover } else { Color32::TRANSPARENT };
                    pt.rect_filled(rect, CornerRadius::ZERO, bg);
                    if hit {
                        pt.rect_filled(Rect::from_min_size(rect.min, Vec2::new(4.0, h)), CornerRadius::ZERO, p.pv);
                    }
                    pt.hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, p.panel2));
                    let rh = rg.size().y;
                    pt.galley(Pos2::new(rect.left() + 18.0, rect.top() + 8.0), rg, p.gold);
                    pt.galley(Pos2::new(rect.left() + 18.0, rect.top() + 12.0 + rh), tg, p.text2);
                }
                if resp.clicked() {
                    if open { st.cm_open.retain(|x| *x != i) } else { st.cm_open.push(i) }
                }
            }
        });
    }
}

/// .cg-head 높이 (padding 6px 4px + 14px 글 + 아래 줄)
const CG_HEAD_H: f32 = 32.0;

/// .cg-head: 번호(hl 700) · 종류 아이콘 · 제목(14px 600) · '· 슬라이드 N장'(12px 흐리게), panel 바탕 · 아래 line-soft
fn paint_cg_head(ui: &egui::Ui, hr: Rect, i: usize, it: &Value, n: usize, hovered: bool) {
    let p = pal();
    let pt = ui.painter();
    pt.rect_filled(hr, CornerRadius::ZERO, if hovered { p.hover } else { p.panel });
    pt.hline(hr.x_range(), hr.bottom() - 0.5, Stroke::new(1.0, p.line_soft));
    let mut x = hr.left() + 4.0;
    let g = pt.layout_no_wrap(format!("{}", i + 1), theme::font(14.0, Weight::Bold), p.hl);
    let gw = g.size().x;
    pt.galley(Pos2::new(x, hr.center().y - g.size().y / 2.0), g, p.hl);
    x += gw + 8.0;
    let ty = it["type"].as_str().unwrap_or("");
    icons::paint(ui, Rect::from_center_size(Pos2::new(x + 8.0, hr.center().y), Vec2::splat(16.0)), icons::for_type(ty), theme::type_color(ty).unwrap_or(p.muted));
    x += 22.0;
    let g = pt.layout_no_wrap(it["title"].as_str().unwrap_or("").to_string(), theme::font(14.0, Weight::SemiBold), p.text);
    let gw = g.size().x;
    pt.galley(Pos2::new(x, hr.center().y - g.size().y / 2.0), g, p.text);
    x += gw + 6.0;
    if n > 0 {
        let g = pt.layout_no_wrap(tr_args("· 슬라이드 {1}장", &[&n]), theme::font(12.0, Weight::Regular), p.muted);
        pt.galley(Pos2::new(x, hr.center().y - g.size().y / 2.0), g, p.muted);
    }
}

/// 송출 중(글자 보임)인 슬라이드의 절 열쇠
fn live_keys(l: Option<&Live>) -> Vec<(i64, i64, i64)> {
    l.filter(|l| l.visible).map(|l| slide_keys(l.deck.slides.get(l.deck.index))).unwrap_or_default()
}

/// 성경 순서의 슬라이드 → 절 목록 (나눈 긴 절은 다시 합친다): (열쇠, 장, 절, 글)
fn bible_verses(d: &Deck) -> Vec<((i64, i64, i64), i64, i64, String)> {
    let mut out: Vec<((i64, i64, i64), i64, i64, String)> = vec![];
    for s in &d.slides {
        for p in s["parts"].as_array().into_iter().flatten() {
            let (b, c, v) = (p["book"].as_i64().unwrap_or(0), p["chapter"].as_i64().unwrap_or(0), p["verse"].as_i64().unwrap_or(0));
            let t = p["text"].as_str().unwrap_or("");
            if let Some(last) = out.last_mut().filter(|x| x.1 == c && x.2 == v) {
                if !last.3.ends_with(t) {
                    last.3.push(' ');
                    last.3.push_str(t);
                }
                continue;
            }
            out.push((vkey(b, c, v), c, v, t.to_string()));
        }
    }
    out
}

/// .cg-line (전체 목록 한 줄): 번호 hl 12px · 이름 muted 12px · 글 14px(줄 높이 1.4, 세 줄까지), 2px 테두리
fn all_line(ui: &mut egui::Ui, k: usize, s: &Value, pv: bool, live: bool, msel: bool) -> Response {
    let p = pal();
    let w = row_width(ui);
    let kg = ui.painter().layout_no_wrap(format!("{}", k + 1), theme::font(12.0, Weight::Bold), p.hl);
    let lab = slide_label(s);
    // 이름표 색이 있으면(--gc) 이름을 그 색 600 굵기로
    let gc = group_color(s);
    let lg = (!lab.is_empty()).then(|| ui.painter().layout_no_wrap(lab, theme::font(12.0, if gc.is_some() { Weight::SemiBold } else { Weight::Regular }), gc.unwrap_or(p.muted)));
    let (icon, text) = slide_text(s);
    let x_text = 8.0 + 12.0 * 1.6 + 10.0 + lg.as_ref().map(|g| g.size().x + 10.0).unwrap_or(0.0) + if icon.is_some() { 20.0 } else { 0.0 };
    let mut tj = egui::text::LayoutJob::single_section(text, egui::TextFormat { font_id: theme::font(14.0, Weight::Regular), color: p.text, line_height: Some(14.0 * 1.4), ..Default::default() });
    tj.wrap.max_width = (w - x_text - 34.0).max(40.0);
    tj.wrap.max_rows = 3;
    let tg = ui.painter().layout_job(tj);
    let h = tg.size().y.max(14.0 * 1.4) + 10.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click_and_drag());
    if ui.is_rect_visible(rect) {
        let pt = ui.painter();
        pt.rect_filled(rect, CornerRadius::same(4), if msel { theme::mix(p.accent, 16.0, p.sunken) } else { p.sunken });
        let y0 = rect.top() + 5.0 + (14.0 * 1.4 - 12.0 * 1.3) / 2.0;
        pt.galley(Pos2::new(rect.left() + 8.0 + 12.0 * 1.6 - kg.size().x, y0), kg, p.hl);
        if let Some(g) = lg {
            let c = gc.unwrap_or(p.muted);
            pt.galley(Pos2::new(rect.left() + 8.0 + 12.0 * 1.6 + 10.0, y0), g, c);
        }
        if let Some(ic) = icon {
            icons::paint(ui, Rect::from_min_size(Pos2::new(rect.left() + x_text - 20.0, rect.top() + 5.0 + (14.0 * 1.4 - 14.0) / 2.0), Vec2::splat(14.0)), ic, p.muted);
        }
        let pt = ui.painter();
        pt.galley(Pos2::new(rect.left() + x_text, rect.top() + 5.0), tg, p.text);
        // .sg-cell: hover line2 · in-pv pv · on-air live(+1px) · 둘 다면 live 테두리 + 바깥 2px pv
        let border = if live { p.live } else if pv { p.pv } else if resp.contains_pointer() { p.line2 } else { Color32::TRANSPARENT };
        pt.rect_stroke(rect, CornerRadius::same(4), Stroke::new(2.0, border), StrokeKind::Inside);
        if live {
            pt.rect_stroke(rect, CornerRadius::same(4), Stroke::new(1.0, p.live), StrokeKind::Outside);
        }
        if pv && live {
            pt.rect_stroke(rect.expand(1.0), CornerRadius::same(5), Stroke::new(2.0, p.pv), StrokeKind::Outside);
        }
        if msel {
            // #cueGrid .msel: outline 3px accent
            pt.rect_stroke(rect, CornerRadius::same(4), Stroke::new(3.0, p.accent), StrokeKind::Inside);
        }
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

// ------------------------------------------------------------------ 편집칸 (cue.js renderEditor 모양)

/// 편집칸 머리 줄에서 누른 것
#[derive(Default)]
pub(crate) struct CeHead {
    pub up: bool,
    pub down: bool,
    pub slide_edit: bool,
    pub dup: bool,
    pub del: bool,
    pub close: bool,
    /// '크게' / '작게' 누름
    pub big: bool,
}

/// .ce-head: 종류 아이콘(26×26 sunken, hl) · 'N. 순서 이름' + 종류 꼬리표 · (빈 칸) · ▲ ▼ · 파란 '슬라이드 편집' · 복제 | 순서 삭제 | 크게 · ✕
pub(crate) fn ce_head(ui: &mut egui::Ui, i: usize, ty: &str, title: &str, saved: &str, big_on: bool) -> CeHead {
    let p = pal();
    let mut h = CeHead::default();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let (r, _) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::hover());
        ui.painter().rect_filled(r, CornerRadius::same(6), p.sunken);
        icons::paint(ui, Rect::from_center_size(r.center(), Vec2::splat(16.0)), icons::for_type(ty), p.hl);
        // 목록에서 누른 이름 그대로 ('6. 설교') + 종류 꼬리표 (평가 20번). 이름이 없으면 예전처럼 'N번 종류'
        let title = title.trim();
        if title.is_empty() {
            ui.label(rich(tr_args("{1}번 {2}", &[&(i + 1), &tr(type_name(ty))]), 14.0, Weight::Bold, p.text));
        } else {
            ui.add(egui::Label::new(rich(format!("{}. {title}", i + 1), 14.0, Weight::Bold, p.text)).truncate().selectable(false));
            ui.label(rich(tr(type_name(ty)), 12.0, Weight::Regular, p.muted));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // 오른쪽부터: ✕ · 크게 | 순서 삭제 | 복제 · 슬라이드 편집 · ▼ · ▲ · 저장 표시
            let sm = |t: &str| Btn::new(t).size(Size::Sm).height(26.0);
            h.close = ui.add(Btn::icon_only("x").size(Size::Sm).height(26.0).tooltip(tr("편집칸 닫기"))).clicked();
            let br = ui.add(sm(&if big_on { tr("작게") } else { tr("크게") }).icon("maximize").selected(big_on).tooltip(if big_on { tr("편집칸을 원래 높이로 (검색 결과가 다시 보임)") } else { tr("편집칸이 본문 패널 전체를 쓰게 (긴 가사·공지 고칠 때)") }));
            widgets::mark("editor-big", br.rect);
            h.big = br.clicked();
            widgets::vsep(ui, 20.0);
            // 되돌릴 수 있지만 큰 변화라 빨간 휴지통 + '순서 삭제', 복제와 구분선으로 떼어 둔다 (평가 9번)
            let dr = ui.add(sm(&tr("순서 삭제")).icon("trash-2").icon_color(p.live).kind(widgets::Kind::Danger).tooltip(tr("이 순서를 큐시트에서 삭제 (Delete) — 알림의 [되돌리기] 나 Ctrl+Z 로 되살림")));
            widgets::mark("editor-del", dr.rect);
            h.del = dr.clicked();
            widgets::vsep(ui, 20.0);
            h.dup = ui.add(sm(&tr("복제")).tooltip(tr("이 순서 복제"))).clicked();
            if ty != "audio" {
                h.slide_edit = ui.add(sm(&tr("슬라이드 편집")).icon("square-pen").kind(crate::widgets::Kind::Primary).tooltip(tr("슬라이드마다 글상자 위치·꾸미기, 글·그림·도형 덧붙이기 (Ctrl+E)"))).clicked();
            }
            h.down = ui.add(Btn::icon_only("chevron-down").size(Size::Sm).height(26.0).tooltip(tr("아래로 (Alt+↓)"))).clicked();
            h.up = ui.add(Btn::icon_only("chevron-up").size(Size::Sm).height(26.0).tooltip(tr("위로 (Alt+↑)"))).clicked();
            if !saved.is_empty() {
                ui.label(rich(saved, 12.0, Weight::Regular, p.muted));
            }
        });
    });
    ui.add_space(6.0);
    h
}

/// .ce-form 한 줄: 52px 이름표(낱말 사이에서만 줄바꿈) + 나머지 칸
pub(crate) fn ce_row<R>(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let p = pal();
    let lw = if wp_core::i18n::is_en() { 88.0 } else { 52.0 };
    let r = ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        let g = ui.painter().layout(label.to_string(), theme::font(14.0, Weight::Regular), p.text2, lw);
        let (r, _) = ui.allocate_exact_size(Vec2::new(lw, theme::CTL_H.max(g.size().y)), Sense::hover());
        ui.painter().galley(Pos2::new(r.left(), r.top() + (theme::CTL_H - g.size().y).max(0.0) / 2.0), g, p.text2);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            add(ui)
        })
        .inner
    });
    ui.add_space(6.0);
    r.inner
}

/// 이름표 칸 하나짜리 글 입력 (높이 --ctl-h)
pub(crate) fn ce_text(ui: &mut egui::Ui, e: &mut Value, k: &str, hint: &str, width: f32) -> bool {
    let mut sv = e[k].as_str().unwrap_or("").to_string();
    let r = ui.add(egui::TextEdit::singleline(&mut sv).desired_width(width).min_size(Vec2::new(0.0, theme::CTL_H - 8.0)).margin(Margin::symmetric(10, 6)).font(theme::font(14.0, Weight::Regular)).hint_text(tr(hint)));
    if r.changed() {
        e[k] = json!(sv);
        true
    } else {
        false
    }
}

/// 여러 줄 입력 (.ce-form textarea: 줄 높이 1.55)
pub(crate) fn ce_area(ui: &mut egui::Ui, e: &mut Value, k: &str, hint: &str, rows: usize) -> bool {
    let mut sv = e[k].as_str().unwrap_or("").to_string();
    let r = ui.add(egui::TextEdit::multiline(&mut sv).desired_rows(rows).desired_width(f32::INFINITY).margin(Margin::symmetric(10, 6)).font(theme::font(14.0, Weight::Regular)).hint_text(tr(hint)));
    if r.changed() {
        e[k] = json!(sv);
        true
    } else {
        false
    }
}

/// 흐린 작은 안내 (.note / .ok-text)
pub(crate) fn ce_note(ui: &mut egui::Ui, text: &str, color: Color32) {
    ui.label(rich(text, 12.0, Weight::Regular, color));
}

/// 콤보(select) 하나: 고른 값이 바뀌면 true
pub(crate) fn ce_select(ui: &mut egui::Ui, id: &str, cur: &mut String, opts: &[(String, String)], width: f32) -> bool {
    let shown = opts.iter().find(|o| o.0 == *cur).map(|o| o.1.clone()).unwrap_or_else(|| opts.first().map(|o| o.1.clone()).unwrap_or_default());
    let before = cur.clone();
    egui::ComboBox::from_id_salt(id).selected_text(shown).width(width).height(360.0).show_ui(ui, |ui| {
        for (v, l) in opts {
            ui.selectable_value(cur, v.clone(), l);
        }
    });
    *cur != before
}
