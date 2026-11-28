//! [트랙 C] 상단 메뉴 줄. 2026-11 '안 A' 구성: 파일 · 편집 · 보기 · 큐시트 · 화면 · 연결 · 송출 · AI · 도움말
//! (시안 · 옮김 표 · 영어 이름: docs/menu-2026-11/). 구성은 `menus()` 의 표 하나가 기준이다.
//! 다른 트랙도 `menus()` 의 목록에 항목을 더하고, 고른 id 를 자기 처리기에서 받으면 된다.
//! 항목 id 는 "트랙:동작" 처럼 짓는다 (예: "c:settings"). "x:" 는 상태에 따라 바뀌는 항목(track_c::menu_bar 가 채움),
//! "v:" 는 하위 메뉴 자리(동작 없음).

use crate::icons;
use crate::theme::{self, Weight, alpha, pal};
use eframe::egui::{self, Align2, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2};

/// 하위 메뉴
pub enum Sub {
    /// 보통 줄
    No,
    /// 표에 적은 줄들 (예: 언어)
    Items(Vec<Item>),
    /// 실행 중 자료로 채우는 줄들 (템플릿 목록 · OBS 주소) — `show_with` 의 `dynamic` 이 그린다
    Dynamic,
}

pub struct Item {
    pub id: &'static str,
    /// 이름
    pub label: &'static str,
    /// 오른쪽 흐린 설명 (없으면 "")
    pub desc: &'static str,
    pub key: String,
    /// 켜기·끄기 항목이면 Some(켜짐). 켜져 있으면 아이콘 자리에 ✓
    pub check: Option<bool>,
    pub sub: Sub,
}

pub struct Menu {
    pub label: &'static str,
    pub items: Vec<Item>,
}

fn it(id: &'static str, label: &'static str, key: &'static str) -> Item {
    Item { id, label, desc: "", key: key.to_string(), check: None, sub: Sub::No }
}
/// 이름 + 오른쪽 흐린 설명
fn itd(id: &'static str, label: &'static str, desc: &'static str, key: &'static str) -> Item {
    Item { desc, ..it(id, label, key) }
}
fn line() -> Item {
    it("-", "-", "")
}
fn sub(id: &'static str, label: &'static str, items: Vec<Item>) -> Item {
    Item { sub: Sub::Items(items), ..it(id, label, "") }
}
fn dynsub(id: &'static str, label: &'static str) -> Item {
    Item { sub: Sub::Dynamic, ..it(id, label, "") }
}

/// 메뉴 목록 (안 A). id "-" 는 구분선
pub fn menus() -> Vec<Menu> {
    #[cfg(target_arch = "wasm32")]
    {
        let mut m = menus_all();
        crate::web::menu::filter(&mut m); // [웹판] 브라우저로 할 수 없는 항목은 뺀다
        return m;
    }
    #[cfg(not(target_arch = "wasm32"))]
    menus_all()
}

fn menus_all() -> Vec<Menu> {
    vec![
        Menu { label: "파일", items: vec![
            it("c:import-bible", "성경 번역본 가져오기…", ""),
            itd("b:conti", "찬양 콘티 가져오기…", "콘티 스튜디오 .worship", ""),
            it("d:reimport", "1.x 자료 다시 가져오기…", ""),
            line(),
            itd("b:pptx", "PPT로 내보내기…", "예배 순서대로", ""),
            it("b:print", "큐시트 인쇄", "Ctrl+P"),
            line(),
            it("b:sync", "여러 PC 동기화 · 백업", ""),
            itd("c:usage", "사용 기록", "저작권 보고", ""),
            line(),
            it("c:settings", "설정", "Ctrl+,"),
        ] },
        Menu { label: "편집", items: vec![
            it("b:undo", "실행 취소", "Ctrl+Z"),
            it("b:redo", "다시 실행", "Ctrl+Y"),
            line(),
            itd("c:slideedit", "슬라이드 편집", "고른 순서", "Ctrl+E"),
        ] },
        Menu { label: "보기", items: vec![
            itd("w:workspaces", "배치", "판 보이기 · 숨기기", ""),
            itd("w:viewers", "뷰어 바꾸기", "둘 다 · 송출 중만", "Ctrl+Shift+L"),
            line(),
            itd("c:commentary", "주석", "보기 · 가져오기", ""),
            line(),
            // 처음 쓰는 영어 사용자가 찾을 수 있게 이름은 두 말 그대로 (옮기지 않음)
            sub("v:lang", "Language · 언어", vec![it("c:lang-ko", "한국어", ""), it("c:lang-en", "English", "")]),
        ] },
        Menu { label: "큐시트", items: vec![
            itd("b:cues", "큐시트 관리", "휴지통 · 복제 · 내보내기", ""),
            line(),
            it("a:home", "홈페이지 주보 → 큐시트 · 광고", ""),
            it("a:points", "설교 포인트 슬라이드", ""),
            it("a:preservice", "예배 전 화면 만들기", ""),
            line(),
            it("b:songs", "찬양 라이브러리", ""),
            it("b:lib", "그림 · 음악 · 영상 라이브러리", ""),
        ] },
        Menu { label: "화면", items: vec![
            itd("d:output", "출력 설정", "예배당 화면 · 모니터 · OBS 숨김", ""),
            it("c:scenes", "화면 구성 편집…", ""),
            line(),
            dynsub("x:tpl-main", "예배당 화면 템플릿"),
            dynsub("x:tpl-overlay", "OBS 자막 템플릿"),
            it("c:templates", "템플릿 편집", ""),
            it("c:themes", "테마 관리", ""),
            line(),
            it("c:obsfull", "OBS 자막: 슬라이드 그대로", ""),
            it("c:obspip", "OBS 자막: 그림 슬라이드 작게", ""),
            line(),
            it("b:stage", "무대 화면", ""),
        ] },
        Menu { label: "연결", items: vec![
            it("d:obs", "OBS 원격 조작", ""),
            dynsub("x:obs-url", "OBS 주소 복사"),
            itd("d:lan", "내부망 출력", "다른 PC 의 OBS", ""),
            line(),
            it("d:remote", "휴대폰 리모컨", ""),
            itd("d:phonecam", "휴대폰 카메라", "QR 로 연결", ""),
            itd("d:webedit", "웹 편집기", "다른 컴퓨터에서 큐시트 고치기", ""),
            itd("d:cloud", "클라우드 연결", "교회 밖에서도 열기", ""), // 웹판 1단계 (wp_core::cloud)
            line(),
            it("d:trigger", "스트림덱 · 외부 단추", ""),
            itd("d:devices", "Companion · 장비", "X32 · PTZ", ""),
            line(),
            it("b:cameras", "카메라 설정", ""),
            it("d:camtest", "카메라 동시 열기 시험", ""),
        ] },
        Menu { label: "송출", items: vec![
            itd("x:preview-off", "미리보기 끄기", "클릭하면 바로 송출", ""),
            it("x:output-toggle", "예배당 화면 켜기·끄기", "Ctrl+Shift+F"),
            line(),
            it("b:quick", "즉석 자막", "Ctrl+T"),
            it("b:message", "회중 메시지", "Ctrl+M"),
            itd("b:names", "이름 자막", "설교자 · 기도자", ""),
            itd("b:overlays", "오버레이", "시계 · 카운트다운 · 배너", ""),
            it("b:draw", "화면에 그리기 · 강조", ""),
            line(),
            it("b:macros", "매크로 · 예약 실행", "Alt+1~9"),
            line(),
            it("c:history", "송출 기록", "Ctrl+H"),
        ] },
        Menu { label: "AI", items: vec![
            it("d:bulletin", "주보로 큐시트 만들기…", ""),
            it("ai:sermon", "설교 원고 → 슬라이드", ""),
            it("ai:lyrics", "가사 자동 정리", ""),
            it("ai:check", "예배 전 큐시트 점검", ""),
            line(),
            itd("ai:say", "말로 시키기", "찾기칸에 '>' 로도", ""),
            itd("ai:listen", "음성 인식", "구절 따라가기 · 실시간 자막", ""),
            line(),
            itd("ai:recap", "예배 후 정리", "시간 표시 · 유튜브 설명 · 요약", ""),
            line(),
            itd("ai:settings", "AI 설정", "키 · 제공자 · 동의", ""),
        ] },
        Menu { label: "도움말", items: vec![
            it("a:help-manual", "사용 설명서", ""),
            itd("a:help-readme", "그림으로 보는 사용법", "README", ""),
            it("a:help-obs", "OBS 연결 안내", ""),
            it("a:help-keys", "단축키 보기", ""),
            line(),
            itd("a:help-news", "새 소식", "버전별 바뀐 점", ""),
            it("d:update", "새 버전 확인", ""),
            line(),
            itd("b:diagnose", "연결 점검", "예배 전 점검", ""),
            it("b:logbundle", "로그 묶음 저장…", ""),
            it("b:logs", "로그 폴더 열기", ""),
            it("a:help-issue", "문제 알리기 · 건의", ""),
            line(),
            it("c:about", "정보 · 라이선스", ""),
        ] },
    ]
}

/// 메뉴 막대에 보이는 이름. 영어 화면에서 '화면' · '연결' 처럼 다른 자리에서도 쓰는 낱말은
/// 사전의 "메뉴: 화면" 을 먼저 본다 (Screens · Connections — docs/menu-2026-11/README.md '영어 이름')
pub fn title(label: &str) -> String {
    let k = format!("메뉴: {label}");
    let t = wp_core::i18n::tr(&k);
    if t == k { wp_core::i18n::tr(label) } else { t }
}

/// 표의 모든 줄 (하위 메뉴 안의 줄까지, 구분선 빼고)
pub fn flat(menus: &[Menu]) -> Vec<&Item> {
    fn walk<'a>(items: &'a [Item], out: &mut Vec<&'a Item>) {
        for i in items {
            if i.id == "-" {
                continue;
            }
            out.push(i);
            if let Sub::Items(s) = &i.sub {
                walk(s, out);
            }
        }
    }
    let mut out = Vec::new();
    for m in menus {
        walk(&m.items, &mut out);
    }
    out
}

/// 켜기·끄기 항목의 지금 상태를 넣는다 (✓ 표시)
pub fn set_check(menus: &mut [Menu], id: &str, on: bool) {
    for m in menus {
        for i in &mut m.items {
            if i.id == id {
                i.check = Some(on);
            }
        }
    }
}

/// 설정에서 바꾼 키와 메뉴 안내가 어긋나지 않게 한다.
pub fn menus_with_shortcuts(settings: &serde_json::Value) -> Vec<Menu> {
    let mut list = menus();
    for menu in &mut list {
        for item in &mut menu.items {
            let id = match item.id {
                "c:history" => Some("openHistory"),
                "b:quick" => Some("quickText"),
                "b:message" => Some("messages"),
                "b:print" => Some("printCue"),
                "w:viewers" => Some("toggleViewers"),
                "x:output-toggle" => Some("toggleOutputWindow"),
                _ => None,
            };
            if let Some(id) = id { item.key = crate::keys::label(settings, id, &item.key); }
        }
    }
    list
}

#[cfg(test)]
mod configured_key_tests {
    #[test]
    fn menus_show_configured_keys_and_leave_fixed_keys_alone() {
        let settings = serde_json::json!({"shortcuts":{"openHistory":"Ctrl+J","quickText":"F8","messages":"","printCue":"Ctrl+Shift+P"}});
        let list = super::menus_with_shortcuts(&settings);
        let key = |id: &str| list.iter().flat_map(|m| &m.items).find(|i| i.id == id).unwrap().key.as_str();
        assert_eq!(key("c:history"), "Ctrl+J");
        assert_eq!(key("b:quick"), "F8");
        assert_eq!(key("b:message"), "");
        assert_eq!(key("b:print"), "Ctrl+Shift+P");
        assert_eq!(key("c:settings"), "Ctrl+,");
    }
}

/// 메뉴 항목 아이콘 (Electron menubar.js 의 `ic` 와 같은 Lucide 이름). 원본에 없는 항목은 가까운 것으로
pub fn icon_of(id: &str) -> Option<&'static str> {
    Some(match id {
        "c:import-bible" => "book-open",
        "c:usage" => "chart-column",
        "b:sync" => "refresh-cw",
        "c:settings" => "settings",
        "c:slideedit" => "square-pen",
        "b:cues" => "folder-open",
        "b:conti" => "download",
        "b:pptx" => "file-text",
        "b:names" => "tag",
        "d:phonecam" => "camera",
        "d:webedit" => "pencil",
        "d:cloud" => "globe",
        "b:undo" => "undo-2",
        "b:redo" => "redo-2",
        "b:print" => "file-text",
        "c:history" => "history",
        "b:quick" => "text",
        "b:message" => "megaphone",
        "b:overlays" => "layers",
        "b:stage" => "presentation",
        "b:macros" => "layers",
        "b:draw" => "brush",
        "w:workspaces" => "columns-2",
        "w:viewers" => "monitor",
        "c:commentary" => "book-open",
        "c:templates" => "palette",
        "c:themes" => "palette",
        "v:lang" => "languages",
        "c:obsfull" | "c:obspip" => "picture-in-picture-2",
        "d:output" | "x:output-toggle" => "monitor",
        "c:scenes" => "layout-grid",
        "x:tpl-main" | "x:tpl-overlay" => "palette",
        "x:obs-url" => "copy",
        "b:songs" => "music",
        "b:lib" => "library",
        "b:cameras" => "camera",
        "d:camtest" => "camera",
        "d:remote" => "smartphone",
        "d:trigger" => "gamepad-2",
        "d:devices" => "sliders-horizontal",
        "d:lan" => "radio",
        "d:obs" => "radio",
        "d:bulletin" => "clipboard-paste",
        "a:home" => "globe",
        "a:points" => "presentation",
        "a:preservice" => "images",
        "ai:check" => "list-checks",
        "ai:sermon" => "file-text",
        "ai:listen" => "mic",
        "ai:say" => "keyboard",
        "ai:recap" => "history",
        "ai:lyrics" => "list-music",
        "ai:settings" => "settings",
        "b:diagnose" => "list-checks",
        "b:logbundle" => "download",
        "b:logs" => "folder-open",
        "a:help-manual" => "book",
        "a:help-readme" => "image",
        "a:help-obs" => "video",
        "a:help-keys" => "keyboard",
        "a:help-news" => "newspaper",
        "a:help-issue" => "bug",
        "c:about" => "info",
        "d:update" => "download",
        "d:reimport" => "download",
        _ => return None,
    })
}

// control.css .menubar / .mb-top / .mb-drop / .mb-item (2차 다듬기 값: 989~1001줄)
const TOP_PAD: Vec2 = Vec2::new(11.0, 4.0);
const TOP_FONT: f32 = 13.0;
const ITEM_FONT: f32 = 13.0;
const ITEM_H: f32 = 28.0;
const DROP_W: f32 = 290.0;

/// 펼친 메뉴 틀 (.mb-drop: panel2 바탕 · line 테두리 · 둥글기 8 · 안쪽 6 · 그림자)
pub fn drop_frame() -> egui::Frame {
    let p = pal();
    egui::Frame::new()
        .fill(p.panel2)
        .stroke(Stroke::new(1.0, p.line))
        .corner_radius(crate::theme::cr(crate::theme::Rad::Lg))
        .inner_margin(egui::Margin::same(6))
        .shadow(egui::Shadow { offset: [0, 10], blur: 30, spread: 0, color: alpha(egui::Color32::BLACK, if p.dark { 0.6 } else { 0.18 }) })
}

/// 메뉴 줄 맨 위 글자 단추 (.mb-top: 테두리·바탕 없음, 13px 500 text2, hover·열림 때 hover 바탕)
fn top_button(ui: &mut Ui, id: egui::Id, text: &str, open: bool, pad: f32) -> Response {
    let p = pal();
    let g = ui.painter().layout_no_wrap(text.to_string(), theme::font(TOP_FONT, Weight::Medium), p.text2);
    let size = Vec2::new(g.size().x + pad * 2.0, (g.size().y + TOP_PAD.y * 2.0).max(24.0));
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let resp = ui.interact(rect, id, Sense::click());
    if ui.is_rect_visible(rect) {
        if open || resp.hovered() {
            ui.painter().rect_filled(rect, crate::theme::cr(crate::theme::Rad::Sm), p.hover);
        }
        ui.painter().galley(rect.center() - g.size() / 2.0, g, p.text2);
    }
    resp
}

/// 메뉴 이름 끝의 괄호 설명 '이름 (설명)' → (이름, 설명). 설명은 메뉴 오른쪽에 흐린 글로 (2.1.0 다듬기: 긴 메뉴 이름 줄이기)
pub fn split_desc(label: &str) -> (&str, Option<&str>) {
    if let Some(body) = label.strip_suffix(')') {
        if let Some(i) = body.rfind(" (") {
            let (name, desc) = (&body[..i], &body[i + 2..]);
            if !name.trim().is_empty() && !desc.trim().is_empty() && !desc.contains('(') {
                return (name, Some(desc));
            }
        }
    }
    (label, None)
}

/// 설명 글 크기 · 이름과 설명 사이
const DESC_FONT: f32 = 11.5;
const DESC_GAP: f32 = 16.0;

/// 펼친 메뉴의 한 줄 (.mb-item: 18px 아이콘 칸 · 글 · 오른쪽 흐린 설명 · kbd, hover 때 sel 바탕).
/// `check` 가 Some 이면 아이콘 칸에 ✓ (원본의 '✓ ' 앞글자). 이름 끝의 괄호는 흐린 설명으로 나눈다
pub fn item_row(ui: &mut Ui, icon: Option<&str>, label: &str, key: &str, check: Option<bool>) -> Response {
    let (name, desc) = split_desc(label);
    item_row_desc(ui, icon, name, desc, key, check)
}

/// `item_row` 와 같지만 이름과 흐린 설명을 따로 받는다 (메뉴 막대의 표: 이름이 '…' 로 끝나도 설명을 붙일 수 있게)
pub fn item_row_desc(ui: &mut Ui, icon: Option<&str>, name: &str, desc: Option<&str>, key: &str, check: Option<bool>) -> Response {
    let p = pal();
    let w = ui.available_width().max(DROP_W - 12.0);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, ITEM_H), Sense::click());
    if !ui.is_rect_visible(rect) {
        return resp;
    }
    let hov = resp.hovered() || resp.has_focus();
    let pt = ui.painter();
    if hov {
        pt.rect_filled(rect, crate::theme::cr(crate::theme::Rad::Sm), p.sel);
    }
    let ic_rect = Rect::from_center_size(Pos2::new(rect.left() + 8.0 + 9.0, rect.center().y), Vec2::splat(16.0));
    let ic_col = if hov { p.text } else { p.muted };
    match (check, icon) {
        (Some(true), _) => icons::paint(ui, ic_rect, "check", p.text),
        (_, Some(name)) => icons::paint(ui, ic_rect, name, ic_col),
        _ => {}
    }
    let x = rect.left() + 8.0 + 18.0 + 9.0;
    pt.text(Pos2::new(x, rect.center().y), Align2::LEFT_CENTER, name, theme::font(ITEM_FONT, Weight::Regular), p.text);
    let mut right = rect.right() - 10.0;
    if !key.is_empty() {
        let r = pt.text(Pos2::new(right, rect.center().y), Align2::RIGHT_CENTER, key, theme::font(11.0, Weight::Regular), p.muted);
        right = r.left() - 12.0;
    }
    if let Some(d) = desc {
        pt.text(Pos2::new(right, rect.center().y), Align2::RIGHT_CENTER, d, theme::font(DESC_FONT, Weight::Regular), if hov { p.muted } else { p.muted2 });
    }
    resp
}

/// 구분선 (.mb-sep: 1px line, 바깥 5px 4px)
pub fn sep(ui: &mut Ui) {
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 11.0), Sense::hover());
    ui.painter().hline((r.left() + 4.0)..=(r.right() - 4.0), r.center().y, Stroke::new(1.0, pal().line));
}

/// 작은 제목 (.mb-head: 11px 700 muted)
pub fn head(ui: &mut Ui, text: &str) {
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 22.0), Sense::hover());
    ui.painter().text(Pos2::new(r.left() + 10.0, r.bottom() - 4.0), Align2::LEFT_BOTTOM, text, theme::font(11.0, Weight::Bold), pal().muted);
}

static TEST_OPENED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static TEST_SUB_OPENED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 점검용 인자 값 (--open-menu 보기 · --open-sub v:lang)
fn test_arg(name: &str) -> Option<String> {
    std::env::args().position(|x| x == name).and_then(|i| std::env::args().nth(i + 1))
}

/// 표의 이름 · 설명을 화면 글로. 언어 줄(Language · 언어 · 한국어 · English)은 어느 화면에서도 그대로
fn item_text(i: &Item) -> (String, Option<String>) {
    if i.id == "v:lang" || i.id.starts_with("c:lang-") {
        return (i.label.to_string(), None);
    }
    (wp_core::i18n::tr(i.label), (!i.desc.is_empty()).then(|| wp_core::i18n::tr(i.desc)))
}

/// 켜기·끄기 표시: 표에 넣은 상태, 화면 언어는 지금 고른 것
fn item_check(i: &Item) -> Option<bool> {
    match i.id {
        "c:lang-ko" => Some(!wp_core::i18n::is_en()),
        "c:lang-en" => Some(wp_core::i18n::is_en()),
        _ => i.check,
    }
}

/// 메뉴 줄을 그리고, 누른 항목 id 를 돌려준다
pub fn show(ui: &mut Ui, menus: &[Menu]) -> Option<&'static str> {
    show_with(ui, menus, |_, _| {})
}

/// 메뉴 줄. 표의 `Sub::Dynamic` 줄(템플릿 목록 · OBS 주소)은 `dynamic(항목 id, ui)` 가 하위 메뉴 안을 그린다
pub fn show_with(ui: &mut Ui, menus: &[Menu], mut dynamic: impl FnMut(&str, &mut Ui)) -> Option<&'static str> {
    let mut picked = None;
    let ctx = ui.ctx().clone();
    let metrics_path = test_arg("--menu-metrics");
    let mut metrics = Vec::new();
    if metrics_path.is_some() { ctx.data_mut(|d| d.remove::<Rect>(egui::Id::new("menu-sub-metrics"))); }
    ui.spacing_mut().interact_size.y = 24.0; // 줄 높이 = .mb-top 높이 (기본 34 이면 메뉴 줄이 두꺼워짐)
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0; // .menubar { gap: 0 }
        let ids: Vec<egui::Id> = menus.iter().map(|m| ui.id().with(("mb-top", m.label))).collect();
        let any_open = ids.iter().any(|id| egui::Popup::is_id_open(&ctx, id.with("popup")));
        // 영어 · 좁은 창 · 150%에서도 아홉 이름을 유지한다. 글자 크기는 그대로, 좌우 여백만 줄인다.
        let labels_width: f32 = menus.iter().map(|m| ui.painter().layout_no_wrap(title(m.label), theme::font(TOP_FONT, Weight::Medium), pal().text2).size().x).sum();
        let pad = ((ui.available_width() - labels_width) / (2.0 * menus.len().max(1) as f32)).clamp(2.0, TOP_PAD.x);
        for m in menus {
            if m.items.is_empty() {
                continue;
            }
            let label = title(m.label); // [트랙 D] 영어 화면
            let id = ui.id().with(("mb-top", m.label));
            let pid = id.with("popup");
            let open = egui::Popup::is_id_open(&ctx, pid);
            let resp = top_button(ui, id, &label, open, pad);
            if metrics_path.is_some() { metrics.push(serde_json::json!({"kind":"top","label":m.label,"rect":[resp.rect.min.x,resp.rect.min.y,resp.rect.max.x,resp.rect.max.y]})); }
            crate::widgets::mark(&format!("menu-{}", m.label), resp.rect); // 합성 입력 시험용 자리
            // 다른 메뉴가 열려 있으면 올리기만 해도 옮겨 연다 (원본 메뉴 줄 동작)
            if any_open && !open && resp.hovered() {
                egui::Popup::open_id(&ctx, pid);
            }
            // 점검용: --open-menu 보기 → 처음에 그 메뉴를 펼친 채로 (스크린샷 비교)
            if !TEST_OPENED.load(std::sync::atomic::Ordering::Relaxed) && test_arg("--open-menu").is_some_and(|x| x == m.label) && ctx.cumulative_pass_nr() > 3 {
                TEST_OPENED.store(true, std::sync::atomic::Ordering::Relaxed);
                egui::Popup::open_id(&ctx, pid);
            }
            // 펼침 너비: 가장 긴 줄에 맞춘다 (최소 290)
            let mut w: f32 = DROP_W - 12.0;
            let width = |s: &str, size: f32| ui.painter().layout_no_wrap(s.to_string(), theme::font(size, Weight::Regular), pal().text).size().x;
            for i in &m.items {
                if i.id == "-" {
                    continue;
                }
                let (name, desc) = item_text(i);
                let tw = width(&name, ITEM_FONT) + desc.map(|d| DESC_GAP + width(&d, DESC_FONT)).unwrap_or(0.0);
                let kw = if i.key.is_empty() { 0.0 } else { width(&i.key, 11.0) + 12.0 };
                let arrow = if matches!(i.sub, Sub::No) { 0.0 } else { 14.0 };
                w = w.max(35.0 + tw + kw + arrow + 18.0);
            }
            let popup = egui::Popup::menu(&resp).id(pid).frame(drop_frame()).gap(2.0).show(|ui| {
                ui.set_width(w);
                ui.spacing_mut().item_spacing.y = 0.0;
                for i in &m.items {
                    if i.id == "-" {
                        sep(ui);
                        continue;
                    }
                    let (name, desc) = item_text(i);
                    match &i.sub {
                        Sub::No => {
                            let row = item_row_desc(ui, icon_of(i.id), &name, desc.as_deref(), &i.key, item_check(i));
                            crate::widgets::mark(&format!("menuitem-{}", i.id), row.rect);
                            // 이름은 원본 menubar.js 그대로('실행 취소'), 무엇을 되돌리는지는 툴팁으로 (글 입력 되돌리기와 구분)
                            let row = match i.id {
                                "b:undo" => row.on_hover_text(wp_core::i18n::tr("큐시트 순서 변경 되돌리기 (Ctrl+Z)")),
                                "b:redo" => row.on_hover_text(wp_core::i18n::tr("큐시트 순서 변경 다시 하기 (Ctrl+Y)")),
                                _ => row,
                            };
                            if row.clicked() {
                                picked = Some(i.id);
                                ui.close();
                            }
                        }
                        Sub::Items(rows) => {
                            sub_row_id(ui, i.id, icon_of(i.id), &name, |ui| {
                                for s in rows {
                                    let (name, desc) = item_text(s);
                                    let row = item_row_desc(ui, icon_of(s.id), &name, desc.as_deref(), &s.key, item_check(s));
                                    crate::widgets::mark(&format!("menuitem-{}", s.id), row.rect);
                                    if row.clicked() {
                                        picked = Some(s.id);
                                        ui.close();
                                    }
                                }
                            });
                        }
                        Sub::Dynamic => {
                            sub_row_id(ui, i.id, icon_of(i.id), &name, |ui| dynamic(i.id, ui));
                        }
                    }
                }
            });
            if let Some(popup) = popup {
                if metrics_path.is_some() { let r=popup.response.rect; metrics.push(serde_json::json!({"kind":"popup","label":m.label,"rect":[r.min.x,r.min.y,r.max.x,r.max.y]})); }
            }
        }
    });
    if let Some(path) = metrics_path {
        if let Some(r) = ctx.data(|d| d.get_temp::<Rect>(egui::Id::new("menu-sub-metrics"))) {
            metrics.push(serde_json::json!({"kind":"submenu","rect":[r.min.x,r.min.y,r.max.x,r.max.y]}));
        }
        let r=ctx.content_rect();
        let _=std::fs::write(path, serde_json::json!({"viewport":[r.min.x,r.min.y,r.max.x,r.max.y],"pixelsPerPoint":ctx.pixels_per_point(),"items":metrics}).to_string());
    }
    picked
}

/// 옆으로 펼치는 하위 메뉴 줄 (.mb-hassub: 오른쪽 chevron-right 14px). `content` 가 하위 메뉴 내용
pub fn sub_row<R>(ui: &mut Ui, icon: Option<&str>, label: &str, content: impl FnOnce(&mut Ui) -> R) -> Option<R> {
    sub_row_id(ui, "", icon, label, content)
}

/// `sub_row` + 시험용 자리 이름("menuitem-<id>") · 점검용 `--open-sub <id>` (그 하위 메뉴를 펼친 채로)
fn sub_row_id<R>(ui: &mut Ui, id: &str, icon: Option<&str>, label: &str, content: impl FnOnce(&mut Ui) -> R) -> Option<R> {
    let resp = item_row_desc(ui, icon, label, None, "", None);
    let ar = Rect::from_center_size(Pos2::new(resp.rect.right() - 10.0 - 7.0, resp.rect.center().y), Vec2::splat(14.0));
    icons::paint(ui, ar, "chevron-right", pal().muted);
    if !id.is_empty() {
        crate::widgets::mark(&format!("menuitem-{id}"), resp.rect);
        if !TEST_SUB_OPENED.load(std::sync::atomic::Ordering::Relaxed) && test_arg("--open-sub").is_some_and(|x| x == id) {
            TEST_SUB_OPENED.store(true, std::sync::atomic::Ordering::Relaxed);
            let sub = egui::containers::menu::SubMenu::id_from_widget_id(resp.id);
            egui::containers::menu::MenuState::from_ui(ui, |state, _| state.open_item = Some(sub));
        }
    }
    let popup = egui::containers::menu::SubMenu::new()
        .show(ui, &resp, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.set_min_width(220.0); // .mb-sub { min-width: 220px }
            // 템플릿이 많아도 하위 메뉴가 창 밖으로 늘어나지 않는다.
            egui::ScrollArea::vertical().max_height(ui.ctx().content_rect().height() - 32.0).show(ui, content).inner
        });
    if let Some(popup) = &popup {
        if test_arg("--menu-metrics").is_some() { ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("menu-sub-metrics"), popup.response.rect)); }
    }
    popup.map(|r| r.inner)
}

#[cfg(test)]
mod desc_tests {
    use super::split_desc;
    #[test]
    fn menu_desc_splits_trailing_paren_only() {
        assert_eq!(split_desc("웹 편집기 (다른 컴퓨터에서 큐시트 고치기)"), ("웹 편집기", Some("다른 컴퓨터에서 큐시트 고치기")));
        assert_eq!(split_desc("말로 시키기 (찾기칸에 '>' 로도)"), ("말로 시키기", Some("찾기칸에 '>' 로도")));
        assert_eq!(split_desc("주보로 큐시트 만들기 (AI)…"), ("주보로 큐시트 만들기 (AI)…", None));
        assert_eq!(split_desc("설정"), ("설정", None));
        assert_eq!(split_desc("Usage log (copyright report)"), ("Usage log", Some("copyright report")));
    }
}

/// 메뉴 구성(안 A) 시험: 예전 메뉴(2.1.1 까지, 메뉴 6개 · 70줄)의 모든 동작이 새 메뉴에 꼭 한 번씩 있다.
/// docs/menu-2026-11/check.js 가 시안에서 하던 검사를 실제 표(`menus()`)에 한다.
#[cfg(test)]
mod layout_tests {
    use super::{flat, icon_of, menus, Item, Sub};
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn order_names_descriptions_and_separators_match_approved_plan_a() {
        let approved: serde_json::Value = serde_json::from_str(include_str!("../../../docs/menu-2026-11/plan-a.json")).unwrap();
        let list = menus();
        assert_eq!(list.len(), approved.as_array().unwrap().len());
        for (menu, expected) in list.iter().zip(approved.as_array().unwrap()) {
            assert_eq!(menu.label, expected["label"].as_str().unwrap());
            let rows = expected["items"].as_array().unwrap();
            assert_eq!(menu.items.len(), rows.len(), "{} 줄 수", menu.label);
            for (item, row) in menu.items.iter().zip(rows) {
                assert_eq!((item.id, item.label, item.desc, item.key.as_str()),
                    (row["id"].as_str().unwrap(), row["label"].as_str().unwrap(), row["desc"].as_str().unwrap(), row["key"].as_str().unwrap()), "{}: {}", menu.label, item.id);
                if let Sub::Items(children) = &item.sub {
                    assert_eq!(children.iter().map(|i| i.id).collect::<Vec<_>>(), row["sub"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect::<Vec<_>>());
                }
            }
        }
    }

    /// 예전 메뉴 (origin/rust a311ce8 의 menubar.rs menus() + track_c.rs menu_bar 덧붙임): (메뉴, id, 단축키).
    /// 'd:bulletin' 은 파일 · 도구 두 자리에 있었다(같은 창).
    const OLD: &[(&str, &str, &str)] = &[
        ("파일", "c:import-bible", ""), ("파일", "c:usage", ""), ("파일", "d:bulletin", ""), ("파일", "b:sync", ""), ("파일", "c:settings", "Ctrl+,"),
        ("큐시트", "c:slideedit", "Ctrl+E"), ("큐시트", "b:cues", ""), ("큐시트", "b:conti", ""), ("큐시트", "b:undo", "Ctrl+Z"), ("큐시트", "b:redo", "Ctrl+Y"),
        ("큐시트", "b:print", "Ctrl+P"), ("큐시트", "b:pptx", ""),
        ("송출", "c:history", "Ctrl+H"), ("송출", "b:quick", "Ctrl+T"), ("송출", "b:message", "Ctrl+M"), ("송출", "b:names", ""), ("송출", "b:overlays", ""),
        ("송출", "b:stage", ""), ("송출", "b:macros", "Alt+1~9"), ("송출", "b:draw", ""), ("송출", "x:preview-off", ""), ("송출", "x:output-toggle", "Ctrl+Shift+F"),
        ("보기", "w:workspaces", ""), ("보기", "w:viewers", "Ctrl+Shift+L"), ("보기", "c:commentary", ""), ("보기", "c:templates", ""), ("보기", "c:themes", ""),
        ("보기", "c:lang-ko", ""), ("보기", "c:lang-en", ""), ("보기", "c:obsfull", ""), ("보기", "c:obspip", ""), ("보기", "d:output", ""),
        ("보기", "x:tpl-main", ""), ("보기", "x:tpl-overlay", ""), ("보기", "c:scenes", ""), ("보기", "x:obs-url", ""),
        ("도구", "b:songs", ""), ("도구", "b:lib", ""), ("도구", "b:cameras", ""), ("도구", "d:camtest", ""), ("도구", "d:remote", ""), ("도구", "d:trigger", ""),
        ("도구", "d:devices", ""), ("도구", "d:lan", ""), ("도구", "d:phonecam", ""), ("도구", "d:webedit", ""), ("도구", "d:obs", ""), ("도구", "d:bulletin", ""),
        ("도구", "a:home", ""), ("도구", "a:points", ""), ("도구", "a:preservice", ""), ("도구", "ai:check", ""), ("도구", "ai:sermon", ""), ("도구", "ai:lyrics", ""),
        ("도구", "ai:recap", ""), ("도구", "ai:say", ""), ("도구", "ai:listen", ""), ("도구", "ai:settings", ""),
        ("도움말", "b:diagnose", ""), ("도움말", "b:logbundle", ""), ("도움말", "b:logs", ""), ("도움말", "a:help-manual", ""), ("도움말", "a:help-readme", ""),
        ("도움말", "a:help-obs", ""), ("도움말", "a:help-keys", ""), ("도움말", "a:help-news", ""), ("도움말", "a:help-issue", ""), ("도움말", "c:about", ""),
        ("도움말", "d:update", ""), ("도움말", "d:reimport", ""),
    ];

    #[test]
    fn old_table_is_the_70_rows_of_2_1_1() {
        assert_eq!(OLD.len(), 70);
        let per = |m: &str| OLD.iter().filter(|o| o.0 == m).count();
        assert_eq!([per("파일"), per("큐시트"), per("송출"), per("보기"), per("도구"), per("도움말")], [5, 7, 10, 14, 22, 12]);
    }

    #[test]
    fn menu_bar_is_plan_a_nine_menus() {
        let names: Vec<&str> = menus().iter().map(|m| m.label).collect();
        assert_eq!(names, ["파일", "편집", "보기", "큐시트", "화면", "연결", "송출", "AI", "도움말"]);
    }

    #[test]
    fn every_old_action_is_in_the_new_menu_exactly_once() {
        let list = menus();
        let mut count: BTreeMap<&str, usize> = BTreeMap::new();
        for i in flat(&list) {
            *count.entry(i.id).or_default() += 1;
        }
        let old: BTreeSet<&str> = OLD.iter().map(|o| o.1).collect();
        assert_eq!(old.len(), 69, "예전 동작은 69가지 (70줄 중 주보 → 큐시트가 두 번)");
        let missing: Vec<&&str> = old.iter().filter(|id| !count.contains_key(**id)).collect();
        assert!(missing.is_empty(), "새 메뉴에서 빠진 동작: {missing:?}");
        let dup: Vec<(&&str, &usize)> = count.iter().filter(|(_, n)| **n != 1).collect();
        assert!(dup.is_empty(), "두 자리에 있는 동작: {dup:?}");
        // 새로 생긴 것: 하위 메뉴 자리 하나(동작 없음) + 클라우드 연결 (웹판 1단계, 2.1.1 뒤에 더함)
        let added: Vec<&&str> = count.keys().filter(|id| !old.contains(**id)).collect();
        assert_eq!(added, [&"d:cloud", &"v:lang"], "새로 생긴 항목");
    }

    #[test]
    fn shortcuts_are_unchanged() {
        let list = menus();
        let new: BTreeMap<&str, &str> = flat(&list).into_iter().map(|i| (i.id, i.key.as_str())).collect();
        for (_, id, key) in OLD {
            assert_eq!(new[id], *key, "{id} 단축키");
        }
        let keys: Vec<&str> = new.values().copied().filter(|k| !k.is_empty()).collect();
        assert_eq!(keys.len(), 11, "단축키가 붙은 항목 수: {keys:?}");
    }

    #[test]
    fn shape_matches_the_proposal() {
        let list = menus();
        // 시안의 '줄 · 묶음(구분선 + 1)' (a.png 의 메뉴 이름 옆 숫자)
        let want = [("파일", 8, 4), ("편집", 3, 2), ("보기", 4, 3), ("큐시트", 6, 3), ("화면", 9, 4), ("연결", 11, 4), ("송출", 9, 4), ("AI", 8, 4), ("도움말", 11, 4)];
        for (m, (name, rows, groups)) in list.iter().zip(want) {
            assert_eq!(m.label, name);
            let seps = m.items.iter().filter(|i| i.id == "-").count();
            assert_eq!((m.items.len() - seps, seps + 1), (rows, groups), "{name} 줄 · 묶음");
            assert!(m.items.len() - seps <= 11, "{name}: 가장 긴 메뉴 11줄");
            // 구분선은 맨 앞 · 맨 끝 · 연달아 오지 않는다
            assert!(m.items.first().unwrap().id != "-" && m.items.last().unwrap().id != "-", "{name} 구분선 자리");
            assert!(!m.items.windows(2).any(|w| w[0].id == "-" && w[1].id == "-"), "{name} 구분선 둘");
        }
        assert_eq!(list.iter().map(|m| m.items.iter().filter(|i| i.id != "-").count()).sum::<usize>(), 69, "메뉴 줄 합계 (안 A 68 + 클라우드 연결)");
        // 하위 메뉴: 지금 있던 3개 + 언어, 모두 한 단계
        let subs: Vec<&Item> = list.iter().flat_map(|m| &m.items).filter(|i| !matches!(i.sub, Sub::No)).collect();
        assert_eq!(subs.iter().map(|i| i.id).collect::<Vec<_>>(), ["v:lang", "x:tpl-main", "x:tpl-overlay", "x:obs-url"]);
        for s in subs {
            if let Sub::Items(rows) = &s.sub {
                assert!(rows.iter().all(|r| matches!(r.sub, Sub::No)), "{} 하위 메뉴는 한 단계만", s.id);
            }
            assert!(s.key.is_empty() && s.desc.is_empty());
        }
        // 자리: 시안의 옮김 표에서 몇 군데
        let menu_of = |id: &str| list.iter().find(|m| flat(std::slice::from_ref(*m)).iter().any(|i| i.id == id)).map(|m| m.label).unwrap();
        for (id, menu) in [("d:bulletin", "AI"), ("b:stage", "화면"), ("d:reimport", "파일"), ("b:print", "파일"), ("b:undo", "편집"), ("c:slideedit", "편집"), ("x:obs-url", "연결"), ("x:preview-off", "송출"), ("c:lang-en", "보기"), ("b:songs", "큐시트"), ("b:diagnose", "도움말")] {
            assert_eq!(menu_of(id), menu, "{id}");
        }
    }

    #[test]
    fn every_row_has_an_icon_and_a_short_name() {
        let list = menus();
        for i in flat(&list) {
            // 미리보기 끄기는 ✓ 만, 언어 줄은 고른 것에 ✓ 만 (시안 그림과 같음)
            let no_icon = matches!(i.id, "x:preview-off" | "c:lang-ko" | "c:lang-en");
            assert_eq!(icon_of(i.id).is_none(), no_icon, "{} 아이콘", i.id);
            assert!(!i.label.contains('(') && !i.label.contains(')'), "{}: 괄호 설명은 desc 로", i.label);
            assert!(!i.label.contains("켜기·끄기") || i.id == "x:output-toggle", "{}: 켜기·끄기는 ✓ 로", i.label);
        }
    }

    /// 영어 화면: 메뉴 이름 · 항목 이름 · 흐린 설명이 모두 사전에 있다 (한글이 없는 글은 그대로 써도 됨)
    #[test]
    fn english_dictionary_has_every_menu_text() {
        let hangul = |s: &str| s.chars().any(|c| ('가'..='힣').contains(&c));
        let en = |s: &str| wp_core::i18n::en_of(s).filter(|e| !e.is_empty() && !hangul(e));
        let list = menus();
        let mut missing = Vec::new();
        for m in &list {
            let title = format!("메뉴: {}", m.label);
            if hangul(m.label) && en(&title).or(en(m.label)).is_none() {
                missing.push(m.label);
            }
        }
        for i in flat(&list) {
            if i.id == "v:lang" || i.id.starts_with("c:lang-") {
                continue; // 언어 줄은 옮기지 않는다
            }
            for s in [i.label, i.desc] {
                if hangul(s) && en(s).is_none() {
                    missing.push(s);
                }
            }
        }
        assert!(missing.is_empty(), "en.json 에 없는 메뉴 글: {missing:#?}");
        // 뜻이 겹치지 않게 고른 메뉴 이름 (docs/menu-2026-11/README.md '영어 이름')
        let title = |k: &str| en(&format!("메뉴: {k}")).or(en(k)).unwrap_or(k).to_string();
        let names: Vec<String> = list.iter().map(|m| title(m.label)).collect();
        assert_eq!(names, ["File", "Edit", "View", "Cue sheet", "Screens", "Connections", "Go live", "AI", "Help"]);
    }
}
