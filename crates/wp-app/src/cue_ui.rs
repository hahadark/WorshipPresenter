//! 왼쪽 큐시트 패널 (Electron control/index.html #panelCue · cue.js render · control.js renderBooks · control.css .cue-* .book-*)
//!  - 맨 위 '큐시트 (예배 순서) | 성경 책' 넓은 분할 단추 (.side-tabs)
//!  - 큐시트: 고르기 콤보 + 폴더 단추(큐시트 관리) · 순서 줄 · 넣기 단추 격자(더보기 ▾) · 도움말
//!  - 성경 책: 책 찾기 · 구약/신약 책 목록 · 장 격자 (장을 누르면 절 목록만, 미리보기는 절을 눌렀을 때)

use crate::app::{Act, App};
use crate::icons;
use crate::theme::{self, pal, rich, Weight};
use crate::widgets::{self, Btn};
use eframe::egui::{self, Align2, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use serde_json::{json, Value};
use wp_core::i18n::{tr, tr_args};

const SIDE: &str = "cue-side"; // "cue" | "books"
const BOOK_Q: &str = "cue-book-q";
const BOOK_SEL: &str = "cue-book-sel";
const BOOK_FOCUS: &str = "cue-book-focus";
const BOTTOM_H: &str = "cue-bottom-h";
const HYMN_PICK: &str = "cue-hymn-pick";
const PPT_WIN: &str = "cue-ppt-win";

const NAV: &str = "nav-owner";
const DEL_SEL: &str = "cue-key-del-sel";
const SCROLL_SEL: &str = "cue-key-scroll";
const CHAP_CUR: &str = "cue-chap-cur";

fn id(k: &str) -> egui::Id {
    egui::Id::new(k)
}

// ---------------------------------------------------------------- 키보드가 있는 목록 (원본의 DOM 초점)

/// 방향키·Enter 를 받는 목록. 원본은 DOM 초점(#cueList · #chapterGrid 단추 · .history-list)으로 가르고,
/// egui 에서는 누른 줄이 초점을 갖지 않으므로 따로 적어 둔다 (본문 목록은 center_ui 의 kbd).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum Nav {
    #[default]
    None,
    /// 큐시트 순서 목록 (cue.js:1271)
    Cue,
    /// 성경 책의 장 격자 (control.js:390)
    Chapters,
    /// 송출 기록 창 (control.js:1364)
    History,
}

#[derive(Clone, Copy, Default)]
struct NavState {
    owner: Nav,
    /// 그 목록을 마지막으로 그린 패스. 목록이 화면에서 사라지면(탭 바꿈·패널 접기) 키를 계속 쥐고 있지 않게 한다
    stamp: u64,
}

/// 지금 키보드를 가진 목록. 글 쓰는 칸에 초점이 있거나 그 목록이 더 그려지지 않으면 None
pub(crate) fn nav_owner(ctx: &egui::Context) -> Nav {
    let st: NavState = ctx.data(|d| d.get_temp(id(NAV))).unwrap_or_default();
    if st.owner == Nav::None || ctx.egui_wants_keyboard_input() || ctx.cumulative_pass_nr() > st.stamp + 2 {
        return Nav::None;
    }
    st.owner
}

/// 이 목록이 키보드를 갖는다 (누르거나 창을 열었을 때)
pub(crate) fn nav_set(ctx: &egui::Context, n: Nav) {
    let stamp = ctx.cumulative_pass_nr();
    ctx.data_mut(|d| d.insert_temp(id(NAV), NavState { owner: n, stamp }));
}

/// 이 목록이 갖고 있었으면 놓는다 (밖을 누름 · 창 닫힘)
pub(crate) fn nav_release(ctx: &egui::Context, n: Nav) {
    ctx.data_mut(|d| {
        if d.get_temp::<NavState>(id(NAV)).is_some_and(|s| s.owner == n) {
            d.remove_temp::<NavState>(id(NAV));
        }
    });
}

/// 목록을 그릴 때마다: 아직 화면에 있다는 표시. 글 쓰는 칸으로 초점이 가면 놓는다 (원본 blur)
pub(crate) fn nav_touch(ctx: &egui::Context, n: Nav) {
    if ctx.egui_wants_keyboard_input() {
        nav_release(ctx, n);
        return;
    }
    let stamp = ctx.cumulative_pass_nr();
    ctx.data_mut(|d| {
        if let Some(s) = d.get_temp::<NavState>(id(NAV)).filter(|s| s.owner == n) {
            if stamp > s.stamp + 2 {
                d.remove_temp::<NavState>(id(NAV)); // 한동안 안 그렸다 다시 나타난 목록은 키보드를 되찾지 않는다
            } else {
                d.insert_temp(id(NAV), NavState { stamp, ..s });
            }
        }
    });
}

/// 전역 Enter(= 미리보기 송출)를 막을 목록이 키보드를 갖고 있는가 (control.js:1660 이 #cueList · #chapterGrid · 모달을 뺀다)
pub(crate) fn nav_takes_enter(ctx: &egui::Context) -> bool {
    nav_owner(ctx) != Nav::None
}

/// 전역 ←→(= 미리보기 앞뒤 슬라이드)를 막을 목록인가 (control.js:1654 가 #chapterGrid · 모달을 뺀다. #cueList 는 빼지 않는다)
pub(crate) fn nav_takes_arrows(ctx: &egui::Context) -> bool {
    matches!(nav_owner(ctx), Nav::Chapters | Nav::History)
}

// ---------------------------------------------------------------- 순서 목록 키

/// 순서 목록 키 (cue.js:1271 list keydown): ↑↓ 고르기만 · Alt+↑↓ 순서 옮기기 · Enter 미리보기(이미 미리보기 중이면 송출) · Delete 지우기.
/// 목록(줄·빈 자리)을 누르면 키보드를 갖고, 밖을 누르거나 글 쓰는 칸에 들어가면 놓는다. 메뉴가 열려 있으면 메뉴가 키를 받는다.
/// `pv_idx` = 열린 큐시트에서 지금 미리보기 중인 순서.
pub(crate) fn list_keys(app: &mut App, ui: &egui::Ui, list_rect: Rect, n: usize, pv_idx: Option<i64>, acts: &mut Vec<Act>) {
    use egui::{Key, Modifiers};
    let ctx = ui.ctx();
    // 지난 프레임에 Delete 로 지웠으면 고른 자리를 원본처럼 같은 자리(끝이었으면 새 끝)에 둔다 (removeItem: st.sel = min(i, len - 1))
    if let Some(j) = ctx.data_mut(|d| d.remove_temp::<usize>(id(DEL_SEL))) {
        app.sel = if n == 0 { None } else { Some(j.min(n - 1)) };
        app.edit = None;
    }
    if ui.input(|i| i.pointer.any_pressed()) {
        if ui.rect_contains_pointer(list_rect) {
            nav_set(ctx, Nav::Cue);
        } else {
            nav_release(ctx, Nav::Cue);
        }
    }
    nav_touch(ctx, Nav::Cue);
    if nav_owner(ctx) != Nav::Cue || egui::Popup::is_any_open(ctx) {
        return;
    }
    let sel = app.sel.filter(|s| *s < n);
    // Alt+↑↓ 먼저 (꾸밈 키 없는 ↑↓ 가 Alt 를 누른 것도 받으므로). Ctrl+↑↓ 는 전역 단축키가 이미 먹었다
    let (aup, adown) = ctx.input_mut(|i| (i.consume_key(Modifiers::ALT, Key::ArrowUp), i.consume_key(Modifiers::ALT, Key::ArrowDown)));
    if aup || adown {
        let d: i64 = if aup { -1 } else { 1 };
        if let Some(s) = sel {
            let to = s as i64 + d;
            if to >= 0 && (to as usize) < n {
                acts.push(Act::MoveItem(s, d));
                ctx.data_mut(|m| m.insert_temp(id(SCROLL_SEL), true));
            }
        }
        return;
    }
    let (up, down, enter, del) = ctx.input_mut(|i| {
        (
            i.consume_key(Modifiers::NONE, Key::ArrowUp),
            i.consume_key(Modifiers::NONE, Key::ArrowDown),
            i.consume_key(Modifiers::NONE, Key::Enter),
            i.consume_key(Modifiers::NONE, Key::Delete),
        )
    });
    if n == 0 {
        return;
    }
    if down || up {
        // 고르기만 바꾼다 — 미리보기·송출은 그대로 (st.sel ± 1; render(); forceEditor())
        app.sel = Some(match (sel, down) {
            (Some(s), true) => (s + 1).min(n - 1),
            (Some(s), false) => s.saturating_sub(1),
            (None, _) => 0,
        });
        app.edit = None;
        ctx.data_mut(|m| m.insert_temp(id(SCROLL_SEL), true));
    } else if enter {
        if let Some(s) = sel {
            if pv_idx == Some(s as i64) {
                acts.push(Act::SendPreview); // 이미 미리보기 중인 순서면 고른 슬라이드 송출
            } else {
                quiet_preview(app, s, acts);
            }
        }
    } else if del {
        if let Some(s) = sel {
            // 원본은 confirm() 으로 묻는다. Rust 판은 다른 지우기와 같이 바로 지우고 알림의 [되돌리기]·Ctrl+Z 로 되살린다
            acts.push(Act::RemoveItem(s));
            ctx.data_mut(|m| m.insert_temp(id(DEL_SEL), s));
        }
    }
}

/// 키로 고른 줄이 보이게 스크롤할 차례인가 (고른 줄을 그릴 때 한 번)
pub(crate) fn take_scroll(ctx: &egui::Context) -> bool {
    ctx.data_mut(|d| d.remove_temp::<bool>(id(SCROLL_SEL))).unwrap_or(false)
}

/// cue.js quietPreview: 순서를 미리보기에만 올린다 — '바로 송출' 모드여도 송출하지 않는다 (holdDirect)
fn quiet_preview(app: &mut App, i: usize, acts: &mut Vec<Act>) {
    let mut c = app.core.lock();
    if !c.direct_mode() {
        drop(c);
        acts.push(Act::PreviewItem(i));
        return;
    }
    match c.item_deck(i) {
        Ok(d) => {
            c.pres.preview = Some(d);
            c.emit(wp_core::core::Ev::Ui);
        }
        Err(e) => c.notify(e, "warn"),
    }
}

/// 처음 고른 쪽: 점검용 `--left-tab books` (예전 숫자 값은 큐시트)
fn side_default() -> String {
    let a: Vec<String> = std::env::args().collect();
    match a.iter().position(|x| x == "--left-tab").and_then(|i| a.get(i + 1)).map(|s| s.as_str()) {
        Some("books") => "books".into(),
        _ => "cue".into(),
    }
}

/// Ctrl+B: 왼쪽 패널을 '성경 책' 으로 바꾸고 책 찾기 칸에 초점 (Electron openBooks → showSide('books'))
pub fn open_books(ctx: &egui::Context) {
    ctx.data_mut(|d| {
        d.insert_temp(id(SIDE), "books".to_string());
        d.insert_temp(id(BOOK_FOCUS), true);
    });
}

/// 큐시트 쪽으로 (순서를 넣으면 Electron 은 showSide('cue'))
pub fn show_cue(ctx: &egui::Context) {
    ctx.data_mut(|d| d.insert_temp(id(SIDE), "cue".to_string()));
}

/// 큐시트 패널이 숨었거나 다른 탭 뒤에 있으면 앞으로 (Ctrl+B)
pub fn reveal_cue_pane(app: &mut App) {
    use wp_core::workspace::{Node, Pane};
    fn walk(n: &mut Node) {
        match n {
            Node::Tabs { panes, active, collapsed } if panes.contains(&Pane::Cue) => {
                *active = Pane::Cue;
                *collapsed = false;
            }
            Node::Split { a, b, .. } => {
                walk(a);
                walk(b);
            }
            _ => {}
        }
    }
    let st = &mut app.workspace.state;
    if !st.visible(Pane::Cue) {
        st.toggle(Pane::Cue);
    }
    if let Some(n) = &mut st.layout {
        walk(n);
    }
    for f in &mut st.floating {
        let mut n = f.node();
        walk(&mut n);
        f.group = Some(n);
    }
}

/// 패널 전체 (workspace_cue 가 부른다)
pub fn side(app: &mut App, ui: &mut egui::Ui, acts: &mut Vec<Act>, cue: impl FnOnce(&mut App, &mut egui::Ui, &mut Vec<Act>)) {
    let mut side: String = ui.data(|d| d.get_temp(id(SIDE))).unwrap_or_else(side_default);
    // .side-tabs { margin: 8px 8px 4px }
    ui.add_space(8.0);
    inset(ui, 8.0, |ui| {
        let r = widgets::segmented_wide(ui, &mut side, &[("cue".to_string(), &tr("큐시트 (예배 순서)")), ("books".to_string(), &tr("성경 책"))]);
        if r.changed() && side == "books" {
            ui.data_mut(|d| d.insert_temp(id(BOOK_FOCUS), true));
        }
    });
    ui.data_mut(|d| d.insert_temp(id(SIDE), side.clone()));
    ui.add_space(4.0);
    if side == "books" {
        books(app, ui);
    } else {
        cue(app, ui, acts);
    }
}

/// 좌우 여백을 둔 칸
fn inset<R>(ui: &mut egui::Ui, x: f32, f: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new().inner_margin(egui::Margin::symmetric(x as i8, 0)).show(ui, f).inner
}

// ---------------------------------------------------------------- 큐시트 머리

/// 큐시트 고르기 콤보 + 폴더 단추 (.cue-head { margin: 4px 8px; gap: 4px })
pub fn head(app: &mut App, ui: &mut egui::Ui, cue_id: &str, cue_label: &str, cues: &[(String, String)], acts: &mut Vec<Act>) {
    let p = pal();
    inset(ui, 8.0, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            // 단추 셋(↶ ↷ 폴더) + 간격 4px
            let w = (ui.available_width() - 3.0 * (theme::CTL_H + 4.0)).max(60.0);
            // select: sunken 바탕 · line 테두리 · 여백 6px 8px · 오른쪽 ▾
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, theme::CTL_H), Sense::click());
            let resp = resp.on_hover_text(tr("큐시트 고르기"));
            if ui.is_rect_visible(rect) {
                let pt = ui.painter();
                let border = if resp.hovered() { p.line2 } else { p.line };
                pt.rect(rect, CornerRadius::same(theme::RADIUS), p.sunken, Stroke::new(p.border_w, border), StrokeKind::Inside);
                let text = if cue_label.is_empty() { tr("(큐시트 없음)") } else { cue_label.to_string() };
                let g = one_line(ui, &text, theme::font(14.0, Weight::Regular), p.text, rect.width() - 8.0 - 24.0);
                pt.galley(Pos2::new(rect.left() + 9.0, rect.center().y - g.size().y / 2.0), g, p.text);
                // 두 겹 그라데이션 화살표 (오른쪽 13px·9px, 4px)
                let c = Pos2::new(rect.right() - 11.0, rect.center().y + 1.0);
                pt.add(egui::Shape::convex_polygon(vec![Pos2::new(c.x - 4.0, c.y - 2.0), Pos2::new(c.x + 4.0, c.y - 2.0), Pos2::new(c.x, c.y + 2.0)], p.muted, Stroke::NONE));
            }
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            egui::Popup::menu(&resp).width(w).show(|ui| {
                egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                    for (cid, label) in cues {
                        if ui.add(egui::Button::selectable(cid == cue_id, label)).clicked() {
                            acts.push(Act::OpenCue(cid.clone()));
                            ui.close();
                        }
                    }
                    if cues.is_empty() {
                        widgets::muted(ui, &tr("(큐시트 없음)"));
                    }
                });
            });
            // 되돌리기 · 다시 하기 (툴팁에 무엇이 되돌려지는지, 할 것이 없으면 흐리게 + 이유)
            let (u, r) = { let c = app.core.lock(); (c.undo_label(false), c.undo_label(true)) };
            if let Some(a) = undo_buttons(ui, u.as_deref(), r.as_deref()) {
                acts.push(Act::Undo(a));
            }
            // 큐시트 관리: 새로 만들기 · 복제 · 서식 · 메모 · 내보내기·가져오기 · 휴지통
            let fb = ui.add(Btn::icon_only("folder-open").tooltip(tr("큐시트 관리: 새로 만들기 · 다음 주로 복제 · 서식 · 메모 · 내보내기·가져오기 · 휴지통")));
            egui::Popup::menu(&fb).show(|ui| {
                ui.set_min_width(200.0);
                if ui.add(egui::Button::image_and_text(icons::image("plus", 15.0, p.muted), tr("새 큐시트"))).clicked() {
                    acts.push(Act::NewCue);
                    ui.close();
                }
                if ui.add(egui::Button::image_and_text(icons::image("folder-open", 15.0, p.muted), tr("큐시트 관리…"))).clicked() {
                    app.tb.cues = true;
                    ui.close();
                }
            });
        });
    });
}

/// 큐시트 머리의 ↶ ↷ 아이콘 단추 (34×34). 눌린 쪽: Some(false)=되돌리기 · Some(true)=다시 하기
pub(crate) fn undo_buttons(ui: &mut egui::Ui, undo: Option<&str>, redo: Option<&str>) -> Option<bool> {
    let mut out = None;
    let tip = |what: Option<&str>, verb: &str, key: &str, none: &str| match what {
        Some(w) => format!("{verb}: {w} ({key})"),
        None => tr(none),
    };
    let b = Btn::icon_only("undo-2").enabled(undo.is_some()).tooltip(tip(undo, &tr("되돌리기"), "Ctrl+Z", "되돌릴 것이 없습니다"));
    let r = ui.add(b);
    widgets::mark("cue-undo", r.rect);
    if r.clicked() {
        out = Some(false);
    }
    let b = Btn::icon_only("redo-2").enabled(redo.is_some()).tooltip(tip(redo, &tr("다시 하기"), "Ctrl+Y", "다시 할 것이 없습니다"));
    let r = ui.add(b);
    widgets::mark("cue-redo", r.rect);
    if r.clicked() {
        out = Some(true);
    }
    out
}

/// 한 줄로 자르고 넘치면 … (text-overflow: ellipsis)
fn one_line(ui: &egui::Ui, text: &str, font: egui::FontId, color: Color32, max_w: f32) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_string(), font, color);
    job.wrap = egui::text::TextWrapping { max_width: max_w.max(10.0), max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
    ui.painter().layout_job(job)
}

// ---------------------------------------------------------------- 순서 줄

/// 둘째 줄 (Electron cue.js detail)
pub fn detail(it: &Value) -> String {
    let s = |k: &str| it[k].as_str().unwrap_or("").to_string();
    let file_name = |f: &str| f.rsplit(['\\', '/']).next().unwrap_or("").split('?').next().unwrap_or("").to_string();
    match it["type"].as_str().unwrap_or("") {
        // Electron cues.js 정규화가 null 을 +null = 0 으로 저장해 붙여 넣은 PPT 가 '0장' 으로 보이던 것을 고침: 번호는 1 이상일 때만
        "hymn" => match it["number"].as_i64().or_else(|| it["number"].as_str().and_then(|x| x.trim().parse().ok())).filter(|n| *n > 0) {
            // 영어 화면: 성경의 "{1}장"(Chapter) 과 갈라 "Hymn 3" (예전에는 "Chapter 3")
            Some(n) if wp_core::i18n::is_en() => wp_core::i18n::tr_args("찬송가 {1}장", &[&n]),
            Some(n) => format!("{n}장"),
            None if s("file").replace('/', "\\").to_lowercase().contains("\\pasted\\") => tr("붙여 넣은 슬라이드"),
            None => tr("찬송가"),
        },
        "bible" => {
            let r = s("ref");
            if r.is_empty() { tr("(장절을 입력하세요)") } else { r }
        }
        "song" => {
            let first = s("lyrics")
                .lines()
                .map(|l| strip_fmt(&strip_chords(l)))
                .zip(s("lyrics").lines().map(str::to_string))
                .find(|(clean, raw)| {
                    let t = raw.trim();
                    !t.is_empty() && !(t.starts_with('[') && t.ends_with(']')) && !clean.trim().is_empty()
                })
                .map(|(c, _)| c.trim().to_string())
                .unwrap_or_default();
            if first.is_empty() { tr("(가사 없음)") } else { first }
        }
        "text" => {
            let first = strip_fmt(s("body").lines().find(|l| !l.trim().is_empty()).unwrap_or(""));
            if first.is_empty() { tr("(내용 없음)") } else { first }
        }
        "title" => s("subtitle"),
        "image" => if s("image").is_empty() { tr("(이미지를 고르세요)") } else { tr("이미지") },
        "blank" => if s("mode") == "background" { tr("배경·로고만") } else { tr("검은 화면") },
        "audio" => {
            let f = s("file");
            if f.is_empty() { tr("(음악을 고르세요)") } else { file_name(&f) }
        }
        "video" => {
            let f = s("file");
            if f.is_empty() { tr("(영상을 고르세요)") } else { format!("{}{}", file_name(&f), if it["loop"] == true { tr(" · 반복") } else { String::new() }) }
        }
        _ => String::new(),
    }
}

/// 코드 표기 [C] [Am7] 지우기 (BCGroup.stripChords 근사)
fn strip_chords(l: &str) -> String {
    let mut out = String::new();
    let mut rest = l;
    while let Some(a) = rest.find('[') {
        let Some(b) = rest[a..].find(']') else { break };
        let inner = &rest[a + 1..a + b];
        let chord = !inner.is_empty() && inner.chars().next().is_some_and(|c| ('A'..='G').contains(&c)) && inner.chars().all(|c| c.is_ascii_alphanumeric() || "#b/+-()".contains(c));
        out.push_str(&rest[..a]);
        if !chord {
            out.push_str(&rest[a..=a + b]);
        }
        rest = &rest[a + b + 1..];
    }
    out.push_str(rest);
    out
}

/// 꾸밈 표기 **굵게** __밑줄__ ==강조== 등 지우기 (BCGroup.stripFmt 근사)
fn strip_fmt(l: &str) -> String {
    l.replace("**", "").replace("__", "").replace("==", "").replace("~~", "")
}

/// 줄 상태
pub struct RowState {
    pub selected: bool,
    pub pv: bool,
    pub air: bool,
    /// LIVE 인데 글자 숨김 (.air.hid → warn 띠)
    pub hidden: bool,
    pub time: Option<(String, i64)>,
}

/// 줄 한 개 결과
pub struct RowOut {
    pub resp: egui::Response,
    pub edit: bool,
    /// 마우스 올림 ▶ 단추: 이 순서 송출
    pub send: bool,
}

const NO_W: f32 = 18.0;
const ICO: f32 = 28.0;
const GAP: f32 = 6.0;
const EDIT_W: f32 = 28.0;
const EDIT_H: f32 = 24.0;
const TITLE_LH: f32 = 20.3; // 14px × 1.45
const SUB_LH: f32 = 17.4; // 12px × 1.45

/// 순서 한 줄 (.cue-item: 여백 7px 6px · 테두리 1px + 왼쪽 4px · 둥글기 10 · 아래 2px)
pub fn row(ui: &mut egui::Ui, i: usize, it: &Value, st: &RowState) -> RowOut {
    let p = pal();
    let ty = it["type"].as_str().unwrap_or("");
    let title = it["title"].as_str().unwrap_or("").to_string();
    let det = detail(it);
    let note = it["note"].as_str().unwrap_or("").trim().to_string();
    let w = ui.available_width();
    let mut body_h = TITLE_LH;
    if !det.is_empty() {
        body_h += SUB_LH;
    }
    if !note.is_empty() {
        body_h += SUB_LH;
    }
    let h = 1.0 + 7.0 + body_h.max(ICO) + 7.0 + 1.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click_and_drag());
    // 단추 위에 있어도 줄은 마우스 올림 상태 (docs/ux-rules.md 3절: 고른 줄만이 아니라 올린 줄에도 연필·송출)
    let hovered = ui.rect_contains_pointer(rect);
    let mut edit = false;
    let mut send = false;
    // 연필 단추 (.cue-edit: 고른 줄·마우스 올린 줄에서만 보임) · 그 왼쪽 ▶ 송출 (마우스 올린 줄)
    let inner_r = rect.right() - 1.0 - 6.0;
    let edit_rect = Rect::from_min_size(Pos2::new(inner_r - EDIT_W, rect.top() + 8.0), Vec2::new(EDIT_W, EDIT_H));
    let send_rect = edit_rect.translate(Vec2::new(-(EDIT_W + 4.0), 0.0));
    let sr = hovered.then(|| ui.interact(send_rect, resp.id.with("send"), Sense::click()).on_hover_text(tr(if ty == "audio" { "이 음악 재생 (두 번 누르기와 같음)" } else { "이 순서 바로 송출 (두 번 누르기와 같음)" })));
    if sr.as_ref().is_some_and(|r| r.clicked()) {
        send = true;
    }
    let er = ui.interact(edit_rect, resp.id.with("edit"), Sense::click()).on_hover_text(tr("이 순서 고치기 (F2)"));
    if er.clicked() {
        edit = true;
    }
    #[cfg(test)]
    if hovered {
        crate::widgets::HOVER_SEEN.with(|m| {
            let mut m = m.borrow_mut();
            m.insert(format!("cue-edit-{i}"), edit_rect);
            m.insert(format!("cue-send-{i}"), send_rect);
        });
    }
    if ui.is_rect_visible(rect) {
        let pt = ui.painter();
        let radius = CornerRadius::same(theme::RADIUS_LG);
        let bg = if st.air { p.live_bg } else if st.selected { p.sel } else if hovered || er.hovered() { p.hover } else { p.panel };
        let border = if st.selected { p.line2 } else { Color32::TRANSPARENT };
        pt.rect(rect, radius, bg, Stroke::new(1.0, border), StrokeKind::Inside);
        // 왼쪽 4px 띠: 미리보기 중이면 pv, 고른 줄이면 line2
        let band = if st.pv { Some(p.pv) } else if st.selected { Some(p.line2) } else { None };
        if let Some(c) = band {
            pt.with_clip_rect(Rect::from_min_max(rect.min, Pos2::new(rect.left() + 4.0, rect.bottom()))).rect_filled(rect, radius, c);
        }
        // LIVE: 오른쪽 안쪽 4px 띠 (글자 숨김이면 노랑)
        if st.air {
            let inner = rect.shrink(1.0);
            let c = if st.hidden { p.warn } else { p.live };
            pt.with_clip_rect(Rect::from_min_max(Pos2::new(inner.right() - 4.0, inner.top()), inner.max)).rect_filled(inner, CornerRadius::same(theme::RADIUS_LG - 1), c);
        }
        let top = rect.top() + 1.0 + 7.0;
        let mut x = rect.left() + 4.0 + 6.0;
        // 번호 (.cue-no: 12px muted, 오른쪽 맞춤, 위 2px)
        widgets::text_at(ui, Pos2::new(x + NO_W, top + 2.0), Align2::RIGHT_TOP, &(i + 1).to_string(), 12.0, Weight::Regular, p.muted);
        x += NO_W + GAP;
        // 종류 아이콘 칩 (28×28, 둥글기 8, 아이콘 16)
        let ico = Rect::from_min_size(Pos2::new(x, top), Vec2::splat(ICO));
        let (chip, ink) = if st.air { (p.live, Color32::WHITE) } else { (theme::type_chip_bg(ty, st.selected || st.pv), theme::type_ink(ty)) };
        pt.rect_filled(ico, CornerRadius::same(8), chip);
        icons::paint(ui, Rect::from_center_size(ico.center(), Vec2::splat(16.0)), icons::for_type(ty), ink);
        x += ICO + GAP;
        // 오른쪽: 연필 단추 자리 + 예정 시각
        let mut right = inner_r - EDIT_W - GAP - if hovered { EDIT_W + 4.0 } else { 0.0 };
        if let Some((t, dur)) = &st.time {
            let a = widgets::text_at(ui, Pos2::new(right - 4.0, top), Align2::RIGHT_TOP, t, 12.0, Weight::Regular, p.muted);
            if *dur > 0 {
                widgets::text_at(ui, Pos2::new(right - 4.0, a.bottom()), Align2::RIGHT_TOP, &crate::cue_ui::mins(*dur), 12.0, Weight::Regular, p.muted2);
            }
            right = a.left() - 4.0 - GAP;
        }
        let bw = (right - x).max(20.0);
        // 굵은 제목 · 흐린 둘째 줄 · 메모(gold)
        let tg = one_line(ui, &title, theme::font(14.0, Weight::Bold), p.text, bw);
        let ty0 = top + (TITLE_LH - tg.size().y) / 2.0;
        pt.galley(Pos2::new(x, ty0), tg, p.text);
        let mut y = top + TITLE_LH;
        if !det.is_empty() {
            let g = one_line(ui, &det, theme::font(12.0, Weight::Regular), p.muted, bw);
            pt.galley(Pos2::new(x, y + (SUB_LH - g.size().y) / 2.0), g, p.muted);
            y += SUB_LH;
        }
        if !note.is_empty() {
            let g = one_line(ui, &format!("📝 {note}"), theme::font(12.0, Weight::Regular), p.gold, bw);
            pt.galley(Pos2::new(x, y + (SUB_LH - g.size().y) / 2.0), g, p.gold);
        }
        if st.selected || hovered || er.hovered() {
            let (eb, ebd) = if er.hovered() { (p.hover, p.line2) } else { (p.panel2, p.line) };
            pt.rect(edit_rect, CornerRadius::same(theme::RADIUS), eb, Stroke::new(p.border_w, ebd), StrokeKind::Inside);
            icons::paint(ui, Rect::from_center_size(edit_rect.center(), Vec2::splat(16.0)), "pencil", p.text);
        }
        if let Some(sr) = &sr {
            let h = sr.hovered();
            let (eb, ebd) = if h { (p.live, p.live) } else { (p.panel2, p.line) };
            pt.rect(send_rect, CornerRadius::same(theme::RADIUS), eb, Stroke::new(p.border_w, ebd), StrokeKind::Inside);
            icons::paint(ui, Rect::from_center_size(send_rect.center(), Vec2::splat(16.0)), "play", if h { Color32::WHITE } else { p.text });
        }
    }
    if hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    RowOut { resp, edit, send }
}

/// 예정 시간 분 표기
pub fn mins(d: i64) -> String {
    wp_core::i18n::tr_args("{1}분", &[&d])
}

/// 순서 목록이 쓸 높이 (넣기 칸·도움말 높이는 지난 프레임에 잰 값)
pub fn list_height(ui: &egui::Ui) -> f32 {
    let bottom: f32 = ui.data(|d| d.get_temp(id(BOTTOM_H))).unwrap_or(160.0);
    // 목록 앞뒤 item_spacing 만큼 빼야 패널 몸통(바깥 스크롤)이 넘치지 않는다
    let view = ui.clip_rect().bottom() - ui.cursor().top() - 2.0 * ui.spacing().item_spacing.y - 1.0;
    (view - bottom).max(120.0)
}

// ---------------------------------------------------------------- 넣기 단추 · 도움말

/// 넣기 단추 한 칸 (.cue-add button: 여백 6px 4px · 12px · 아이콘 15 종류 색)
fn add_btn(ui: &mut egui::Ui, w: f32, ic: Option<&str>, ink: Color32, label: &str, more: bool, active: bool, tip: &str) -> egui::Response {
    let p = pal();
    let h = 30.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click());
    let hov = resp.hovered();
    if ui.is_rect_visible(rect) {
        let r = if resp.is_pointer_button_down_on() { rect.translate(Vec2::new(0.0, 0.5)) } else { rect };
        let (bg, bd) = if hov { (p.hover, p.line2) } else { (p.panel2, p.line) };
        let (bd, fg) = if active { (p.accent, p.accent) } else { (bd, p.text) };
        let pt = ui.painter();
        pt.rect(r, CornerRadius::same(theme::RADIUS), bg, Stroke::new(p.border_w, bd), StrokeKind::Inside);
        let mut g = ui.painter().layout_no_wrap(label.to_string(), theme::font(12.0, Weight::Regular), fg);
        let icw = if ic.is_some() { 15.0 + 5.0 } else { 0.0 };
        let endw = if more { 2.0 + 13.0 } else { 0.0 };
        // 단추보다 글이 넓으면(영어 'Announcement') 잘리지 않게 글자를 줄인다
        let room = (w - 10.0 - icw - endw).max(8.0);
        if g.size().x > room {
            g = ui.painter().layout_no_wrap(label.to_string(), theme::font((12.0 * room / g.size().x).max(8.0), Weight::Regular), fg);
        }
        let total = icw + g.size().x + endw;
        let mut x = r.center().x - total / 2.0;
        let cy = r.center().y;
        if let Some(n) = ic {
            let col = if more { if active { p.accent } else if hov { p.text } else { p.muted } } else { ink };
            icons::paint(ui, Rect::from_min_size(Pos2::new(x, cy - 7.5), Vec2::splat(15.0)), n, col);
            x += icw;
        }
        let gw = g.size().x;
        pt.galley(Pos2::new(x, cy - g.size().y / 2.0), g, fg);
        x += gw;
        if more {
            icons::paint(ui, Rect::from_min_size(Pos2::new(x + 2.0, cy - 6.5), Vec2::splat(13.0)), "chevron-down", if active { p.accent } else if hov { p.text } else { p.muted });
        }
    }
    if hov {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if tip.is_empty() { resp } else { resp.on_hover_text(tip) }
}

/// 넣기 단추 격자 + 도움말 (.cue-add { grid 3열 · 간격 5 · 여백 8 · 위 테두리 })
pub fn add_grid(app: &mut App, ui: &mut egui::Ui, acts: &mut Vec<Act>) {
    let p = pal();
    let direct = app.core.lock().direct_mode();
    let top = ui.cursor().top();
    let full = ui.available_width();
    let r0 = ui.cursor().left();
    ui.painter().hline(r0..=r0 + full, top, Stroke::new(1.0, p.line));
    ui.add_space(8.0);
    let more_id = id("cue-more-open");
    inset(ui, 8.0, |ui| {
        let cw = ((ui.available_width() - 10.0) / 3.0).floor();
        let main: [(&str, &str, &str); 6] = [
            ("hymn", "찬송가·PPT", "찬송가·교독문·찬양 PPT (폴더 검색 · 온라인에서 찾기)"),
            ("bible", "성경", ""),
            ("song", "찬양", ""),
            ("weblyrics", "가사", "웹(벅스·지니·멜론)에서 가사를 찾아 찬양 순서로 넣기 (Ctrl+L)"),
            ("text", "공지", ""),
            ("title", "제목", ""),
        ];
        ui.spacing_mut().item_spacing = Vec2::new(5.0, 5.0);
        for rowk in main.chunks(3) {
            ui.horizontal(|ui| {
                for (ty, label, tip) in rowk {
                    // '성경': 지금 검색한 장절이 있으면 그것으로 채워 넣는다는 것을 툴팁에
                    let tip = match (*ty, app.searched_bible_item()) {
                        ("bible", Some(it)) => tr_args("성경 순서 넣기: 검색한 '{1}' 로 채움 (고른 순서 다음)", &[&it["ref"].as_str().unwrap_or("")]),
                        ("bible", None) => tr("성경 순서 넣기 (먼저 장절을 검색하면 그 장절로 채움)"),
                        _ => tr(tip),
                    };
                    let r = add_btn(ui, cw, Some(icons::for_type(ty)), theme::type_ink(ty), &tr(label), false, false, &tip);
                    widgets::mark(&format!("add-{ty}"), r.rect);
                    if r.clicked() {
                        add(app, ui.ctx(), ty, acts);
                    }
                }
            });
        }
        let open: bool = ui.data(|d| d.get_temp(more_id)).unwrap_or(false);
        let mr = add_btn(ui, cw, Some("plus"), p.muted, &tr("더보기"), true, open, &tr("이미지 · 영상 · 음악 · 슬라이드쇼 · 카메라 · 빈 화면"));
        let pop = egui::Popup::menu(&mr).show(|ui| {
            ui.set_min_width(200.0);
            ui.label(rich(tr("순서 넣기"), 12.0, Weight::SemiBold, p.muted));
            for (ty, ic, label) in [
                ("image", "image", "이미지"),
                ("video", "clapperboard", "영상"),
                ("audio", "volume-2", "음악"),
                ("slideshow", "images", "슬라이드쇼"),
                ("camera", "camera", "카메라"),
                ("blank", "square", "빈 화면"),
                ("points", "presentation", "설교 포인트 슬라이드…"),
                ("preservice", "images", "예배 전 화면 자동 만들기…"),
            ] {
                let ink = if matches!(ty, "points" | "preservice") { p.muted } else { theme::type_ink(ty) };
                if ui.add(egui::Button::image_and_text(icons::image(ic, 15.0, ink), tr(label))).clicked() {
                    add(app, ui.ctx(), ty, acts);
                    ui.close();
                }
            }
        });
        ui.data_mut(|d| d.insert_temp(more_id, pop.is_some()));
    });
    ui.add_space(8.0);
    // .cue-help { 11.5px muted2 · 여백 0 10px 8px · 줄 높이 1.5 }
    inset(ui, 10.0, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let mut job = egui::text::LayoutJob::single_section(
            // 지금 방식에 맞춘 안내 (머리 줄 '클릭하면 [미리보기][바로 송출]')
            if direct { tr("클릭 = 바로 송출 (회중 화면에 바로 나감) · ← → 로 고르고 Enter(F9) = 송출 · Ctrl+↑↓ = 이전/다음 · 끌어서 순서 바꾸기") } else { tr("클릭 = 미리보기 (화면에는 안 나감) · ← → 로 고르고 Enter(F9) = 송출 · Ctrl+↑↓ = 이전/다음 · 끌어서 순서 바꾸기") },
            egui::TextFormat { font_id: theme::font(11.5, Weight::Regular), color: p.muted2, line_height: Some(17.25), ..Default::default() },
        );
        job.wrap.max_width = ui.available_width();
        ui.label(job);
    });
    ui.add_space(8.0);
    let h = ui.cursor().top() - top;
    ui.data_mut(|d| d.insert_temp(id(BOTTOM_H), h));
}

/// 넣기 단추 동작 (Electron cue.js #cueAdd click)
pub(crate) fn add(app: &mut App, ctx: &egui::Context, ty: &str, acts: &mut Vec<Act>) {
    match ty {
        "hymn" => {
            ctx.data_mut(|d| d.insert_temp(id(HYMN_PICK), true));
            show_cue(ctx);
        }
        "weblyrics" => crate::lyrics_ui::open(),
        "points" => {
            crate::a_tools::handle_menu("a:points");
        }
        "preservice" => {
            crate::a_tools::handle_menu("a:preservice");
        }
        "camera" => acts.push(Act::AddItem("camera", Some(json!({ "title": tr("카메라"), "deviceId": "", "label": "", "fit": "cover" })))),
        "slideshow" => acts.push(Act::AddItem("slideshow", Some(json!({ "title": tr("예배 전 화면"), "images": [], "body": "", "autoSec": 8, "autoLoop": true, "countdown": "", "countdownLabel": tr("예배 시작까지") })))),
        "video" => acts.push(Act::AddItem("video", Some(json!({ "title": tr("영상"), "file": "", "sound": true, "volume": 1, "endAction": "hold", "loop": false })))),
        "audio" => acts.push(Act::AddItem("audio", Some(json!({ "title": tr("음악"), "file": "" })))),
        "blank" => acts.push(Act::AddItem("blank", Some(json!({ "mode": "background" })))),
        // 지금 검색한 장절이 있으면 그것으로 채운다 (없으면 빈 성경 순서)
        "bible" => {
            if !app.add_searched_bible(acts) {
                acts.push(Act::AddItem("bible", None))
            }
        }
        "song" => acts.push(Act::AddItem("song", None)),
        "text" => acts.push(Act::AddItem("text", None)),
        "title" => acts.push(Act::AddItem("title", None)),
        "image" => acts.push(Act::AddItem("image", None)),
        _ => {}
    }
    let _ = app;
}

/// 찬송가·PPT 고르기 창 (Electron openHymnPicker) · PPT 도구 창 (오른쪽 클릭 '여러 순서로 나누기…')
pub fn windows(app: &mut App, ui: &mut egui::Ui, sel_item: Option<(usize, &Value)>, acts: &mut Vec<Act>) {
    let ctx = ui.ctx().clone();
    let mut open: bool = ctx.data(|d| d.get_temp(id(HYMN_PICK))).unwrap_or(false);
    if open {
        crate::widgets::float_window(&ctx, "cue-hymn-win", tr("찬송가·PPT 넣기"), [460.0, 520.0]).open(&mut open).show(&ctx, |ui| {
            crate::hymns_ui::tab(app, ui, acts);
        });
        ctx.data_mut(|d| d.insert_temp(id(HYMN_PICK), open));
    }
    // PPT 도구: 오른쪽 클릭 메뉴가 요청을 남기면 창을 연다 (ppt_box 가 요청을 받아 펼침)
    let Some((i, it)) = sel_item.filter(|(_, it)| crate::hymns_ui::can_open_ppt_tools(it)) else { return };
    let item_id = it["id"].as_str().unwrap_or("").to_string();
    let requested = ctx.data(|d| d.get_temp::<String>(egui::Id::new("ppt-tools-request"))).is_some_and(|v| v == item_id);
    let mut win: String = ctx.data(|d| d.get_temp(id(PPT_WIN))).unwrap_or_default();
    if requested {
        win = item_id.clone();
    }
    if win == item_id {
        let mut o = true;
        crate::widgets::float_window(&ctx, "cue-ppt-win", tr("PPT 도구"), [420.0, 360.0]).open(&mut o).show(&ctx, |ui| {
            crate::hymns_ui::ppt_box(app, ui, i, it);
        });
        if !o {
            win.clear();
        }
    }
    ctx.data_mut(|d| d.insert_temp(id(PPT_WIN), win));
}

// ---------------------------------------------------------------- 성경 책

/// '성경 책' 쪽 (#booksPanel: 책 찾기 · 책 목록 · 장 격자)
pub fn books(app: &mut App, ui: &mut egui::Ui) {
    let p = pal();
    let (structure, _vname) = {
        let mut c = app.core.lock();
        let v = c.version_id();
        (c.store.get_structure(&v), v)
    };
    let mut q: String = ui.data(|d| d.get_temp(id(BOOK_Q))).unwrap_or_default();
    // 점검용 --book 43 (처음 고른 책)
    let mut sel: Option<i64> = ui.data(|d| d.get_temp(id(BOOK_SEL))).unwrap_or_else(|| std::env::args().position(|x| x == "--book").and_then(|i| std::env::args().nth(i + 1)).and_then(|x| x.parse().ok()));
    let mut pick: Option<(i64, i64)> = None;
    // 장 격자에서 키보드가 있는 장 (책 번호, 장) — 원본의 초점 있는 장 단추
    let mut chap_cur: Option<(i64, i64)> = ui.data(|d| d.get_temp(id(CHAP_CUR))).unwrap_or(None);
    // #bookFilter { margin: 8px }
    ui.add_space(4.0);
    let mut enter = false;
    inset(ui, 8.0, |ui| {
        let r = ui.add(egui::TextEdit::singleline(&mut q).hint_text(tr("책 찾기 (Ctrl+B)")).desired_width(f32::INFINITY).font(theme::font(14.0, Weight::Regular)).margin(egui::Margin::symmetric(8, 6)));
        let focus = ui.data_mut(|d| d.remove_temp::<bool>(id(BOOK_FOCUS))).unwrap_or(false);
        if focus {
            r.request_focus();
            if let Some(mut s) = egui::TextEdit::load_state(ui.ctx(), r.id) {
                s.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(q.chars().count()))));
                s.store(ui.ctx(), r.id);
            }
        }
        enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    });
    ui.add_space(8.0);
    let f = q.trim().to_string();
    let list: Vec<&Value> = structure
        .iter()
        .filter(|b| f.is_empty() || b["name"].as_str().unwrap_or("").contains(&f) || b["short"].as_str().unwrap_or("").starts_with(&f))
        .collect();
    if enter {
        if let Some(b) = list.first() {
            sel = b["id"].as_i64();
        }
    }
    let book = sel.and_then(|bid| structure.iter().find(|x| x["id"] == bid));
    // .chapter-box { max-height: 45% }
    let view = ui.clip_rect().bottom() - ui.cursor().top();
    let chap_h = if book.is_some() { (view * 0.45).min(chapter_box_h(ui, book.unwrap())) } else { 0.0 };
    let list_h = (view - chap_h).max(80.0);
    // .book-list { padding: 0 6px 6px }
    egui::ScrollArea::vertical().id_salt("cue-books").max_height(list_h).min_scrolled_height(list_h).auto_shrink([false, false]).show(ui, |ui| {
        inset(ui, 6.0, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let mut last = "";
            for b in &list {
                let t = b["testament"].as_str().unwrap_or("");
                if t != last {
                    last = t;
                    // .grp { 12px muted · 여백 8px 6px 4px }
                    ui.add_space(8.0);
                    inset(ui, 6.0, |ui| {
                        ui.label(rich(tr(if t == "OT" { "구약" } else { "신약" }), 12.0, Weight::Regular, p.muted));
                    });
                    ui.add_space(4.0);
                }
                let bid = b["id"].as_i64().unwrap_or(0);
                if book_btn(ui, b["name"].as_str().unwrap_or(""), b["short"].as_str().unwrap_or(""), sel == Some(bid)).clicked() {
                    sel = Some(bid);
                }
            }
            if list.is_empty() {
                ui.add_space(8.0);
                inset(ui, 6.0, |ui| ui.label(rich(tr("없음"), 12.0, Weight::Regular, p.muted)));
            }
            ui.add_space(6.0);
        });
    });
    if let Some(b) = book {
        // .chapter-box { border-top: 1px line; padding: 8px }
        let top = ui.cursor().top();
        let x0 = ui.cursor().left();
        ui.painter().hline(x0..=x0 + ui.available_width(), top, Stroke::new(1.0, p.line));
        ui.add_space(8.0);
        egui::ScrollArea::vertical().id_salt("cue-chapters").max_height((chap_h - 9.0).max(40.0)).auto_shrink([false, true]).show(ui, |ui| {
            inset(ui, 8.0, |ui| {
                let n = b["chapters"].as_array().map(|a| a.len()).unwrap_or(0);
                // .chapter-title { 700 · 아래 6px }
                ui.label(rich(wp_core::i18n::tr_args("{1} ({2}장)", &[&b["name"].as_str().unwrap_or(""), &n]), 14.0, Weight::Bold, p.text));
                ui.add_space(6.0 - ui.spacing().item_spacing.y);
                // .chapter-grid { auto-fill minmax(36px, 1fr) · 간격 4 }
                // 스크롤 막대 자리를 빼고 계산 (예전에는 오른쪽 칸이 패널 밖으로 넘쳐 24·36·48장이 잘렸다)
                let aw = (ui.available_width() - 14.0).max(40.0);
                let cols = (((aw + 4.0) / 40.0).floor() as usize).max(1);
                let cw = (aw - 4.0 * (cols - 1) as f32) / cols as f32;
                ui.spacing_mut().item_spacing = Vec2::new(4.0, 4.0);
                let chs: Vec<i64> = b["chapters"].as_array().into_iter().flatten().map(|x| x.as_i64().unwrap_or(0)).collect();
                let bid = b["id"].as_i64().unwrap_or(0);
                // 장 격자 방향키 (control.js:390): ←→ ±1 · ↑↓ ±한 줄, 끝에서는 첫·끝 장에 멈춤. 초점만 옮기고 Enter 가 그 장을 연다 (단추의 기본 동작)
                chap_cur = chap_cur.filter(|c| c.0 == bid && c.1 >= 1 && c.1 as usize <= chs.len());
                let ctx = ui.ctx().clone();
                nav_touch(&ctx, Nav::Chapters);
                let mut moved = false;
                if let (Nav::Chapters, Some((_, cur)), false) = (nav_owner(&ctx), chap_cur, egui::Popup::is_any_open(&ctx)) {
                    use egui::{Key, Modifiers};
                    let none = ctx.input(|i| !i.modifiers.alt && !i.modifiers.command && !i.modifiers.ctrl);
                    let (r, l, d, u, enter) = if none {
                        ctx.input_mut(|i| (i.consume_key(Modifiers::NONE, Key::ArrowRight), i.consume_key(Modifiers::NONE, Key::ArrowLeft), i.consume_key(Modifiers::NONE, Key::ArrowDown), i.consume_key(Modifiers::NONE, Key::ArrowUp), i.consume_key(Modifiers::NONE, Key::Enter)))
                    } else {
                        (false, false, false, false, false)
                    };
                    let step = if r { 1 } else if l { -1 } else if d { cols as i64 } else if u { -(cols as i64) } else { 0 };
                    if step != 0 {
                        chap_cur = Some((bid, (cur + step).clamp(1, chs.len() as i64)));
                        moved = true;
                    } else if enter {
                        pick = Some((bid, cur));
                    }
                }
                let focus_ring = nav_owner(&ctx) == Nav::Chapters;
                let mut grid_rect = Rect::NOTHING;
                for row in chs.chunks(cols).enumerate() {
                    ui.horizontal(|ui| {
                        for (k, verses) in row.1.iter().enumerate() {
                            let ch = (row.0 * cols + k + 1) as i64;
                            let r = ui.add(Btn::new(ch.to_string()).min_width(cw).height(29.0).tooltip(wp_core::i18n::tr_args("{1}장 · {2}절", &[&ch, verses])));
                            widgets::mark(&format!("chap-{ch}"), r.rect);
                            grid_rect = grid_rect.union(r.rect);
                            if r.clicked() {
                                pick = Some((bid, ch));
                                chap_cur = Some((bid, ch)); // 누른 단추가 초점을 갖는다
                                nav_set(&ctx, Nav::Chapters);
                            }
                            if focus_ring && chap_cur == Some((bid, ch)) {
                                // :focus-visible 테두리
                                ui.painter().rect_stroke(r.rect.expand(1.0), CornerRadius::same(theme::RADIUS), Stroke::new(2.0, p.accent), StrokeKind::Outside);
                                if moved {
                                    r.scroll_to_me(None);
                                }
                            }
                        }
                    });
                }
                // 격자 밖을 누르면 키보드를 놓는다 (단추 blur)
                if ui.input(|i| i.pointer.any_pressed()) && !ui.rect_contains_pointer(grid_rect) {
                    nav_release(&ctx, Nav::Chapters);
                }
                ui.add_space(8.0);
            });
        });
    }
    ui.data_mut(|d| {
        d.insert_temp(id(BOOK_Q), q);
        d.insert_temp(id(BOOK_SEL), sel);
        d.insert_temp(id(CHAP_CUR), chap_cur);
    });
    if let Some((bid, ch)) = pick {
        open_chapter(app, &structure, bid, ch);
    }
}

/// 장 칸이 다 보일 때 높이
fn chapter_box_h(ui: &egui::Ui, b: &Value) -> f32 {
    let n = b["chapters"].as_array().map(|a| a.len()).unwrap_or(0).max(1);
    let aw = ui.available_width() - 16.0;
    let cols = (((aw + 4.0) / 40.0).floor() as usize).max(1);
    let rows = n.div_ceil(cols) as f32;
    1.0 + 8.0 + TITLE_LH + 6.0 + rows * 33.0 + 8.0
}

/// 책 단추 한 줄 (.book-list button: 왼쪽 맞춤 · 여백 4px 8px · 둥글기 4 · hover/on → sel)
fn book_btn(ui: &mut egui::Ui, name: &str, short: &str, on: bool) -> egui::Response {
    let p = pal();
    let w = ui.available_width();
    let h = 4.0 + 20.3 + 4.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click());
    if ui.is_rect_visible(rect) {
        if on || resp.hovered() {
            ui.painter().rect_filled(rect, CornerRadius::same(4), p.sel);
        }
        let a = widgets::text_at(ui, Pos2::new(rect.left() + 8.0, rect.center().y), Align2::LEFT_CENTER, name, 14.0, Weight::Regular, p.text);
        if !short.is_empty() {
            widgets::text_at(ui, Pos2::new(a.right() + 4.0, rect.center().y + 1.0), Align2::LEFT_CENTER, short, 12.0, Weight::Regular, p.muted);
        }
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// 장을 누르면: 검색칸에 '요3' 처럼 넣고 절 목록만 띄운다 (미리보기는 절을 눌렀을 때)
fn open_chapter(app: &mut App, structure: &[Value], bid: i64, ch: i64) {
    let short = structure.iter().find(|x| x["id"] == bid).and_then(|b| b["short"].as_str()).unwrap_or("").to_string();
    let mut c = app.core.lock();
    let v = c.version_id();
    let r = c.store.get_chapter(&v, bid, ch);
    if r["ok"] == json!(true) {
        drop(c);
        let pg = r["passage"].clone();
        app.search = if short.is_empty() { pg["title"].as_str().unwrap_or("").to_string() } else { format!("{short}{ch}") };
        app.result = json!({ "ok": true, "mode": "reference", "passage": pg });
    } else {
        c.notify(r["error"].as_str().unwrap_or(&tr("본문을 찾지 못했습니다.")), "warn");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detail_follows_electron_rules() {
        assert_eq!(detail(&json!({"type": "song", "lyrics": "[1절]\n\n"})), "(가사 없음)");
        assert_eq!(detail(&json!({"type": "song", "lyrics": "[1절]\n[C]주 [G7]사랑\n"})), "주 사랑");
        assert_eq!(detail(&json!({"type": "hymn", "number": 0})), "찬송가");
        assert_eq!(detail(&json!({"type": "hymn", "number": 3})), "3장");
        // 영어 화면용 문구가 사전에 있고 성경 장(Chapter)과 다르다 (언어 전환은 전역이라 여기서 바꾸지 않는다)
        let en: serde_json::Value = serde_json::from_str(include_str!("../../wp-core/data/en.json")).unwrap();
        assert_eq!(en["찬송가 {1}장"], "Hymn {1}");
        assert_eq!(detail(&json!({"type": "hymn", "number": 0, "file": "C:\\Users\\a\\AppData\\Roaming\\WorshipCue\\data\\pasted\\clip-1.pptx"})), "붙여 넣은 슬라이드");
        assert_eq!(detail(&json!({"type": "hymn"})), "찬송가");
        assert_eq!(detail(&json!({"type": "text", "body": ""})), "(내용 없음)");
        assert_eq!(detail(&json!({"type": "text", "body": "\n**광고** 안내"})), "광고 안내");
        assert_eq!(detail(&json!({"type": "bible", "ref": ""})), "(장절을 입력하세요)");
        assert_eq!(detail(&json!({"type": "title", "subtitle": ""})), "");
        assert_eq!(detail(&json!({"type": "blank", "mode": "background"})), "배경·로고만");
    }
}