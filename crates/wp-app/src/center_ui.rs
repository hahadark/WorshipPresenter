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

/// grid.js slideText: 전체 목록·슬라이드 목록에 쓰는 글
fn slide_text(s: &Value, list: bool) -> String {
    let kind = s["kind"].as_str().unwrap_or("");
    let st = |k: &str| s[k].as_str().unwrap_or("").to_string();
    match kind {
        "title" => [st("ref"), st("subtitle")].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(if list { " · " } else { "\n" }),
        "image" | "score" => {
            let c = st("caption");
            if !c.is_empty() { c } else if kind == "score" { tr("악보") } else { tr("이미지") }
        }
        "video" => tr("영상"),
        "camera" => tr("카메라"),
        "blank" => tr(if s["mode"] == "background" { "배경·로고만" } else { "검은 화면" }),
        _ => {
            let parts = s["parts"].as_array().cloned().unwrap_or_default();
            let text = kind == "text";
            parts
                .iter()
                .map(|p| {
                    let t = strip_fmt(p["text"].as_str().unwrap_or(""));
                    if text || p["verse"].as_i64().unwrap_or(0) == 0 { t } else { format!("{} {}", p["verse"], t) }
                })
                .collect::<Vec<_>>()
                .join(if text { "\n" } else { " " })
                .trim()
                .to_string()
        }
    }
}

/// BCGroup.stripFmt 근사: **굵게** · __밑줄__ · [코드] 같은 꾸밈 기호를 뺀다
fn strip_fmt(t: &str) -> String {
    t.replace("**", "").replace("__", "").replace("~~", "")
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

/// .results .row.prow — 절 번호 칸(3.2em, hl 13px 700) + 16px 본문(줄 높이 1.6). 왼쪽 pv 띠, 오른쪽 live 띠
fn verse_row(ui: &mut egui::Ui, vn: &str, text: &str, pv: bool, live: bool) -> Response {
    let p = pal();
    let w = row_width(ui);
    let vn_w = 16.0 * 3.2;
    let x_text = 12.0 + vn_w;
    let g = job(ui, text, 16.0, Weight::Regular, p.text, 1.6, w - x_text - 14.0);
    let h = g.size().y + 14.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click());
    if ui.is_rect_visible(rect) {
        let pt = ui.painter();
        if resp.hovered() {
            pt.rect_filled(rect, CornerRadius::ZERO, p.hover);
        }
        if pv {
            pt.rect_filled(Rect::from_min_size(rect.min, Vec2::new(4.0, h)), CornerRadius::ZERO, p.pv);
        }
        if live {
            pt.rect_filled(Rect::from_min_max(Pos2::new(rect.right() - 4.0, rect.top()), rect.max), CornerRadius::ZERO, p.live);
        }
        let vg = pt.layout_no_wrap(vn.to_string(), theme::font(13.0, Weight::Bold), p.hl);
        let first_line = 16.0 * 1.6;
        // 기준선 맞춤 (align-items: baseline): 13px 글자의 아래를 16px 글자의 아래에 맞춘다
        let y = rect.top() + 7.0 + (first_line - vg.size().y) / 2.0 + 1.5;
        pt.galley(Pos2::new(rect.left() + 12.0 + vn_w - 10.0 - vg.size().x, y), vg, p.hl);
        pt.galley(Pos2::new(rect.left() + x_text, rect.top() + 7.0), g, p.text);
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// .results .row — 낱말 찾기 결과: 금색 장절(13px 700) + 본문(찾은 낱말은 warn-bg 칠)
fn text_row(ui: &mut egui::Ui, r: &str, text: &str, terms: &[String], live: bool) -> Response {
    let p = pal();
    let w = row_width(ui);
    let mut j = egui::text::LayoutJob::default();
    let lh = Some(16.0 * 1.6);
    j.append(r, 0.0, egui::TextFormat { font_id: theme::font(13.0, Weight::Bold), color: p.gold, line_height: lh, ..Default::default() });
    let base = egui::TextFormat { font_id: theme::font(16.0, Weight::Regular), color: p.text, line_height: lh, ..Default::default() };
    let mark = egui::TextFormat { background: p.warn_bg, ..base.clone() };
    // 찾은 낱말 구간 (공백 무시 없이 단순 비교)
    let mut hits: Vec<(usize, usize)> = vec![];
    for t in terms.iter().map(|t| t.split_whitespace().collect::<String>()).filter(|t| !t.is_empty()) {
        let mut from = 0;
        while let Some(i) = text[from..].find(&t) {
            hits.push((from + i, from + i + t.len()));
            from += i + t.len();
        }
    }
    hits.sort();
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
    j.wrap.max_width = (w - 26.0).max(20.0);
    let g = ui.painter().layout_job(j);
    let h = g.size().y + 14.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click());
    if ui.is_rect_visible(rect) {
        let pt = ui.painter();
        if resp.hovered() {
            pt.rect_filled(rect, CornerRadius::ZERO, p.hover);
        }
        if live {
            pt.rect_filled(Rect::from_min_max(Pos2::new(rect.right() - 4.0, rect.top()), rect.max), CornerRadius::ZERO, p.live);
        }
        pt.galley(Pos2::new(rect.left() + 12.0, rect.top() + 7.0), g, p.text);
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

/// .results .srow — 슬라이드 목록 한 줄: 이름(4.5em, hl 12px 700) + 15px 글. 미리보기면 pv 띠 + pv-bg
fn slide_row(ui: &mut egui::Ui, label: &str, text: &str, pv: bool, live: bool) -> Response {
    let p = pal();
    let w = row_width(ui);
    let lw = 15.0 * 4.5;
    let g = job(ui, text, 15.0, Weight::Regular, p.text, 1.6, w - 14.0 - lw - 14.0);
    let h = g.size().y.max(15.0 * 1.6) + 16.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click());
    if ui.is_rect_visible(rect) {
        let pt = ui.painter();
        if pv {
            pt.rect_filled(rect, CornerRadius::ZERO, p.pv_bg);
        } else if resp.contains_pointer() {
            pt.rect_filled(rect, CornerRadius::ZERO, p.hover);
        }
        if pv {
            pt.rect_filled(Rect::from_min_size(rect.min, Vec2::new(4.0, h)), CornerRadius::ZERO, p.pv);
        }
        if live {
            pt.rect_filled(Rect::from_min_max(Pos2::new(rect.right() - 4.0, rect.top()), rect.max), CornerRadius::ZERO, p.live);
        }
        pt.hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, p.line_soft));
        let lg = pt.layout(label.to_string(), theme::font(12.0, Weight::Bold), p.hl, lw - 12.0);
        pt.galley(Pos2::new(rect.left() + 14.0 + lw - 12.0 - lg.size().x, rect.top() + 8.0 + (15.0 * 1.6 - 12.0 * 1.3) / 2.0), lg, p.hl);
        pt.galley(Pos2::new(rect.left() + 14.0 + lw, rect.top() + 8.0), g, p.text);
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
fn thumb_cells(ui: &mut egui::Ui, id: egui::Id, c: &paint::Ctx, slides: &[Value], tpl: &Value, version: &str, pv: Option<usize>, live: Option<usize>, size: f32, ops: &dyn Fn(&Value) -> Vec<widgets::HoverOp>) -> Option<Hit> {
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
            let resp = ui.interact(r, id.with(k), Sense::click());
            #[cfg(test)]
            widgets::HOVER_SEEN.with(|m| m.borrow_mut().insert(format!("cell-{k}"), r));
            if resp.clicked() {
                out = Some(Hit::Pick(k, false));
            }
            if resp.double_clicked() {
                out = Some(Hit::Pick(k, true));
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
            pt.rect_filled(r, CornerRadius::same(6), if hot { p.hover } else { p.sunken });
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
        // .center-head: padding 8px 12px · gap 8 · 아래 line
        let head = egui::Frame::new().inner_margin(Margin::symmetric(12, 8)).show(ui, |ui| {
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
        ui.painter().hline(ui.max_rect().x_range(), r.bottom(), Stroke::new(1.0, pal().line));
        if toggle_editor {
            *editor_on = !*editor_on;
        }
        save(ui.ctx(), st);
    }

    /// 가운데 본문: 목록(검색 결과·순서 슬라이드) / 격자 / 전체 격자·목록 / 주석
    pub(crate) fn center_body(&mut self, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
        let mut st = load(ui.ctx());
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
                        self.search_results(ui, s, acts)
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
                    let resp = verse_row(ui, &vn, v["text"].as_str().unwrap_or(""), pv_keys.iter().any(|x| same_verse(*x, k)), live_keys.iter().any(|x| same_verse(*x, k)));
                    if Some(i) == pv_first && st.scroll_key != scroll_key {
                        resp.scroll_to_me(None);
                        st.scroll_key = scroll_key.clone();
                    }
                    crate::parity_ui::verse_menu(&resp, &format!("{} {}:{}", p["book"]["name"].as_str().unwrap_or(""), c, n), v["text"].as_str().unwrap_or(""));
                    if resp.clicked() {
                        acts.push(Act::PreviewPassage(p.clone(), self.search.clone(), Some((c, n))))
                    }
                    if resp.double_clicked() {
                        acts.push(Act::PreviewPassage(p.clone(), self.search.clone(), Some((c, n))));
                        acts.push(Act::SendPreview)
                    }
                }
            } else {
                let rows = r["results"].as_array().cloned().unwrap_or_default();
                let terms: Vec<String> = r["terms"].as_array().map(|t| t.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_else(|| self.search.split_whitespace().map(String::from).collect());
                if rows.is_empty() {
                    empty(ui, &tr("검색 결과가 없습니다.\n다른 단어나 더 짧은 단어로 검색해 보세요."));
                }
                for v in &rows {
                    let q = format!("{}{}:{}", v["bookShort"].as_str().unwrap_or(""), v["chapter"], v["verse"]);
                    let k = vkey(v["book"].as_i64().unwrap_or(0), v["chapter"].as_i64().unwrap_or(0), v["verse"].as_i64().unwrap_or(0));
                    let resp = text_row(ui, &q, v["text"].as_str().unwrap_or(""), &terms, live_keys.iter().any(|x| same_verse(*x, k)));
                    crate::parity_ui::verse_menu(&resp, v["ref"].as_str().unwrap_or(""), v["text"].as_str().unwrap_or(""));
                    if resp.clicked() {
                        acts.push(Act::PreviewRef(q.clone(), None))
                    }
                    if resp.double_clicked() {
                        acts.push(Act::SendRef(q, None))
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
        egui::ScrollArea::vertical().id_salt("results-item").auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.add_space(6.0);
            if d.passage["itemType"] == "audio" {
                let f = d.passage["audioItem"]["file"].as_str().unwrap_or("");
                let name = f.rsplit(['\\', '/']).next().unwrap_or("").to_string();
                empty(ui, &format!("{}\n{}", if name.is_empty() { tr("(음악 파일을 고르세요)") } else { name }, tr("F9 = 재생 · 화면은 바뀌지 않습니다")));
                return;
            }
            if d.passage["itemType"] == "bible" {
                let verses = bible_verses(d);
                let multi = verses.first().map(|v| v.1) != verses.last().map(|v| v.1);
                let mut last_ch = -1;
                for (k, ch, vn, text) in verses {
                    if multi && ch != last_ch {
                        heading(ui, &tr_args("{1}장", &[&ch]));
                    }
                    last_ch = ch;
                    let label = if multi { format!("{ch}:{vn}") } else { vn.to_string() };
                    let pv = pv_keys.iter().any(|x| same_verse(*x, k));
                    let resp = verse_row(ui, &label, &text, pv, lkeys.iter().any(|x| same_verse(*x, k)));
                    if pv && st.scroll_key != scroll_key {
                        resp.scroll_to_me(None);
                        st.scroll_key = scroll_key.clone();
                    }
                    let idx = wp_core::slides::index_of_verse(&d.slides, ch, vn);
                    if resp.clicked() {
                        acts.push(Act::PreviewIndex(idx));
                    }
                    if resp.double_clicked() {
                        acts.push(Act::PreviewIndex(idx));
                        acts.push(Act::SendPreview);
                    }
                }
            } else {
                for (k, sl) in d.slides.iter().enumerate() {
                    let l = sl["label"].as_str().filter(|x| !x.is_empty()).map(String::from).unwrap_or_else(|| (k + 1).to_string());
                    let resp = slide_row(ui, &l, &slide_text(sl, false), k == d.index, live_same == Some(k));
                    if let Some(op) = widgets::hover_ops(ui, egui::Id::new(("srow", k)), resp.rect, &slide_hover_ops(deck_item(s, d).map(|x| x.1), sl, true)) {
                        if let Some((i, _)) = deck_item(s, d) {
                            acts.push(Act::SlideOp(i, slide_key(sl), op));
                        }
                    }
                    if k == d.index && st.scroll_key != scroll_key {
                        resp.scroll_to_me(None);
                        st.scroll_key = scroll_key.clone();
                    }
                    if resp.clicked() {
                        acts.push(Act::PreviewIndex(k));
                    }
                    if resp.double_clicked() {
                        acts.push(Act::PreviewIndex(k));
                        acts.push(Act::SendPreview);
                    }
                }
            }
            ui.add_space(6.0);
        });
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
        egui::ScrollArea::vertical().id_salt("slide-grid").auto_shrink([false, false]).show(ui, |ui| {
            egui::Frame::new().inner_margin(Margin::same(10)).show(ui, |ui| {
                match thumb_cells(ui, egui::Id::new("sg"), &c, &d.slides, &tpl, d.passage["versionName"].as_str().unwrap_or(""), Some(d.index), live.map(|l| l.deck.index), size, &ops) {
                    Some(Hit::Pick(k, dbl)) => {
                        acts.push(Act::PreviewIndex(k));
                        if dbl {
                            acts.push(Act::SendPreview);
                        }
                    }
                    Some(Hit::Op(k, op)) => {
                        if let (Some((i, _)), Some(sl)) = (item, d.slides.get(k)) {
                            acts.push(Act::SlideOp(i, slide_key(sl), op));
                        }
                    }
                    None => {}
                }
            });
        });
    }

    /// 전체 격자 · 전체 목록 (ProPresenter 방식): 큐시트의 모든 순서 슬라이드를 순서별 줄로
    fn all_view(&mut self, ui: &mut egui::Ui, st: &mut CenterState, s: &Snap, acts: &mut Vec<Act>) {
        let (cue_id, items) = {
            let c = self.core.lock();
            c.cue.as_ref().map(|q| (q.id.clone(), q.items.clone())).unwrap_or_default()
        };
        let key = format!("{}|{}|{}", cue_id, serde_json::to_string(&items).unwrap_or_default(), serde_json::to_string(&s.tpl_main).unwrap_or_default().len());
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
        egui::ScrollArea::vertical().id_salt("cue-grid").auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            egui::Frame::new().inner_margin(Margin { left: 10, right: 10, top: 4, bottom: 40 }).show(ui, |ui| {
                for (i, (it, sec)) in items.iter().zip(secs.iter()).enumerate() {
                    let is_pv = pv.map(|x| x.0) == Some(i);
                    let is_lv = lv.map(|x| x.0) == Some(i);
                    let top = ui.cursor().top();
                    // .cg-sec: 왼쪽 3px 띠(미리보기 초록 · 송출 빨강), 안쪽 6px
                    let inner = egui::Frame::new().inner_margin(Margin { left: 9, right: 0, top: 0, bottom: 6 }).show(ui, |ui| {
                        // .cg-head: 14px 600, 번호 hl, 슬라이드 수 흐리게, 아래 line-soft
                        let n = sec.deck.as_ref().map(|d| d.slides.len()).unwrap_or(0);
                        let head_w = ui.available_width();
                        let (hr, hresp) = ui.allocate_exact_size(Vec2::new(head_w, 32.0), Sense::click());
                        if ui.is_rect_visible(hr) {
                            let pt = ui.painter();
                            pt.rect_filled(hr, CornerRadius::ZERO, if hresp.hovered() { p.hover } else { p.panel });
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
                                let hit = if list {
                                    ui.add_space(4.0);
                                    let mut hit = None;
                                    for (k, sl) in d.slides.iter().enumerate() {
                                        let r = all_line(ui, k, sl, pvk == Some(k), lvk == Some(k));
                                        if r.clicked() {
                                            hit = Some(Hit::Pick(k, false));
                                        }
                                        if r.double_clicked() {
                                            hit = Some(Hit::Pick(k, true));
                                        }
                                        if let Some(op) = widgets::hover_ops(ui, egui::Id::new(("cgl", i, k)), r.rect, &slide_hover_ops(Some(it), sl, true)) {
                                            hit = Some(Hit::Op(k, op));
                                        }
                                        ui.add_space(2.0);
                                    }
                                    hit
                                } else {
                                    ui.add_space(8.0);
                                    let ops = |sl: &Value| slide_hover_ops(Some(it), sl, false);
                                    thumb_cells(ui, egui::Id::new(("cg", i)), &c, &d.slides, &sec.tpl, d.passage["versionName"].as_str().unwrap_or(""), pvk, lvk, size, &ops)
                                };
                                match hit {
                                    Some(Hit::Pick(k, dbl)) => {
                                        self.sel = Some(i);
                                        acts.push(Act::PreviewItem(i));
                                        acts.push(Act::PreviewIndex(k));
                                        if dbl {
                                            acts.push(Act::SendPreview);
                                        }
                                    }
                                    Some(Hit::Op(k, op)) => {
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
                            }
                            Ok(_) => {
                                ui.add_space(4.0);
                                ui.label(rich(if it["type"] == "audio" { tr("음악 · 화면은 바뀌지 않습니다") } else { tr("(슬라이드 없음)") }, 13.0, Weight::Regular, p.muted));
                            }
                            Err(e) => {
                                ui.add_space(4.0);
                                ui.label(rich(e, 13.0, Weight::Regular, p.muted));
                            }
                        }
                    });
                    let band = if is_lv { p.live } else if is_pv { p.pv } else { Color32::TRANSPARENT };
                    let r = inner.response.rect;
                    ui.painter().rect_filled(Rect::from_min_max(Pos2::new(r.left(), top), Pos2::new(r.left() + 3.0, r.bottom())), CornerRadius::ZERO, band);
                }
            });
            if let Some(r) = new_scroll {
                ui.scroll_to_rect(r, Some(egui::Align::Min));
                st.scroll_key = scroll_key.clone();
            }
        });
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
fn all_line(ui: &mut egui::Ui, k: usize, s: &Value, pv: bool, live: bool) -> Response {
    let p = pal();
    let w = row_width(ui);
    let kg = ui.painter().layout_no_wrap(format!("{}", k + 1), theme::font(12.0, Weight::Bold), p.hl);
    let lab = slide_label(s);
    let lg = (!lab.is_empty()).then(|| ui.painter().layout_no_wrap(lab, theme::font(12.0, Weight::Regular), p.muted));
    let x_text = 8.0 + 12.0 * 1.6 + 10.0 + lg.as_ref().map(|g| g.size().x + 10.0).unwrap_or(0.0);
    let mut tj = egui::text::LayoutJob::single_section(slide_text(s, true), egui::TextFormat { font_id: theme::font(14.0, Weight::Regular), color: p.text, line_height: Some(14.0 * 1.4), ..Default::default() });
    tj.wrap.max_width = (w - x_text - 34.0).max(40.0);
    tj.wrap.max_rows = 3;
    let tg = ui.painter().layout_job(tj);
    let h = tg.size().y.max(14.0 * 1.4) + 10.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click());
    if ui.is_rect_visible(rect) {
        let pt = ui.painter();
        pt.rect_filled(rect, CornerRadius::same(4), p.sunken);
        let y0 = rect.top() + 5.0 + (14.0 * 1.4 - 12.0 * 1.3) / 2.0;
        pt.galley(Pos2::new(rect.left() + 8.0 + 12.0 * 1.6 - kg.size().x, y0), kg, p.hl);
        if let Some(g) = lg {
            pt.galley(Pos2::new(rect.left() + 8.0 + 12.0 * 1.6 + 10.0, y0), g, p.muted);
        }
        pt.galley(Pos2::new(rect.left() + x_text, rect.top() + 5.0), tg, p.text);
        let border = if live { p.live } else if pv { p.pv } else if resp.contains_pointer() { p.line2 } else { Color32::TRANSPARENT };
        pt.rect_stroke(rect, CornerRadius::same(4), Stroke::new(2.0, border), StrokeKind::Inside);
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
