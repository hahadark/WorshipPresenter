//! [트랙 C] 상단 메뉴 줄 (Electron menubar.js 처럼 파일·큐시트·송출·보기·도구·도움말)
//! 다른 트랙도 `menus()` 의 목록에 항목을 더하고, 고른 id 를 자기 처리기에서 받으면 된다.
//! 항목 id 는 "트랙:동작" 처럼 짓는다 (예: "c:settings").

use crate::icons;
use crate::theme::{self, Weight, alpha, pal};
use eframe::egui::{self, Align2, CornerRadius, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2};

pub struct Item {
    pub id: &'static str,
    pub label: &'static str,
    pub key: String,
}

pub struct Menu {
    pub label: &'static str,
    pub items: Vec<Item>,
}

fn it(id: &'static str, label: &'static str, key: &'static str) -> Item {
    Item { id, label, key: key.to_string() }
}

/// 메뉴 목록. 빈 label("-") 은 구분선
pub fn menus() -> Vec<Menu> {
    vec![
        Menu { label: "파일", items: vec![
            it("c:import-bible", "성경 번역본 가져오기…", ""),
            it("c:usage", "사용 기록 (저작권 보고)", ""),
            it("d:bulletin", "주보로 큐시트 만들기 (AI)…", ""),
            it("b:sync", "여러 PC 동기화 · 백업", ""), // [트랙 B]
            it("-", "-", ""),
            it("c:settings", "설정", "Ctrl+,"),
        ] },
        Menu { label: "큐시트", items: vec![
            it("c:slideedit", "슬라이드 편집 (고른 순서)", "Ctrl+E"),
            it("b:cues", "큐시트 관리 (휴지통 · 복제 · 내보내기)", ""),
            it("b:conti", "찬양 콘티 가져오기 (콘티 스튜디오 .worship)…", ""),
            it("b:undo", "실행 취소", "Ctrl+Z"),
            it("b:redo", "다시 실행", "Ctrl+Y"),
            it("-", "-", ""),
            it("b:print", "큐시트 인쇄", "Ctrl+P"),
        ] },
        Menu { label: "송출", items: vec![
            it("c:history", "송출 기록", "Ctrl+H"),
            it("-", "-", ""),
            it("b:quick", "즉석 자막", "Ctrl+T"),
            it("b:message", "회중 메시지", "Ctrl+M"),
            it("b:overlays", "오버레이 (시계 · 카운트다운 · 배너)", ""),
            it("b:stage", "무대 화면", ""),
            it("b:macros", "매크로 · 예약 실행", "Alt+1~9"),
            it("b:draw", "화면에 그리기 · 강조", ""), // [트랙 B]
        ] },
        Menu { label: "보기", items: vec![
            it("w:workspaces", "작업공간 · 패널 배치", ""),
            it("c:commentary", "주석 보기 · 가져오기", ""),
            it("c:templates", "출력 템플릿 편집", ""),
            it("c:themes", "출력 테마 관리", ""),
            // Electron menubar.js 보기 > Language · 언어 (화면 언어 바꾸기)
            it("c:lang-ko", "Language · 언어: 한국어", ""),
            it("c:lang-en", "Language · 언어: English", ""),
            it("-", "-", ""),
            it("c:obsfull", "OBS 자막: 슬라이드 그대로 켜기·끄기", ""),
            it("c:obspip", "OBS 자막: 그림 슬라이드 작게 켜기·끄기", ""),
            it("d:output", "출력 설정 (예배당 화면 · 모니터 · OBS 숨김)", ""), // [트랙 D]
        ] },
        Menu { label: "도구", items: vec![
            it("b:songs", "찬양 라이브러리", ""),
            it("b:lib", "그림 · 음악 · 영상 라이브러리", ""),
            it("b:cameras", "카메라 설정", ""), // [트랙 B]
            it("-", "-", ""),
            it("d:remote", "휴대폰 리모컨", ""),
            it("d:trigger", "스트림덱 · 외부 단추", ""),
            it("d:devices", "Companion · 장비 (X32 · PTZ)", ""),
            it("d:lan", "내부망 출력 (다른 PC 의 OBS)", ""),
            it("d:obs", "OBS 원격 조작", ""), // [트랙 D]
            it("d:bulletin", "주보 → 큐시트 (AI)", ""), // [트랙 D]
            it("a:home", "홈페이지 주보 → 큐시트 · 광고", ""), // [트랙 A]
            it("a:points", "설교 포인트 슬라이드", ""), // [트랙 A]
            it("a:preservice", "예배 전 화면 만들기", ""), // [트랙 A]
        ] },
        Menu { label: "도움말", items: vec![
            it("b:diagnose", "연결 점검 (예배 전 점검)", ""), // [트랙 B]
            it("b:logbundle", "로그 묶음 저장…", ""), // [트랙 B]
            it("b:logs", "로그 폴더 열기", ""), // [트랙 B]
            it("-", "-", ""),
            it("a:help-manual", "사용 설명서", ""), // [트랙 A]
            it("a:help-readme", "그림으로 보는 사용법 (README)", ""), // [트랙 A]
            it("a:help-obs", "OBS 연결 안내", ""), // [트랙 A]
            it("a:help-keys", "단축키 보기", ""), // [트랙 A]
            it("a:help-news", "새 소식 (버전별 바뀐 점)", ""), // [트랙 A]
            it("a:help-issue", "문제 알리기 · 건의", ""), // [트랙 A]
            it("c:about", "정보 · 라이선스", ""),
            it("d:update", "새 버전 확인", ""), // [트랙 D]
            it("d:reimport", "1.x 자료 다시 가져오기…", ""), // [갈래 D]
        ] },
    ]
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
        "b:undo" => "undo-2",
        "b:redo" => "redo-2",
        "b:print" => "file-text",
        "c:history" => "history",
        "b:quick" => "text",
        "b:message" => "megaphone",
        "b:overlays" => "layers",
        "b:stage" => "presentation",
        "b:macros" => "sparkles",
        "b:draw" => "brush",
        "w:workspaces" => "columns-2",
        "c:commentary" => "book-open",
        "c:templates" => "palette",
        "c:themes" => "palette",
        "c:lang-ko" | "c:lang-en" => "languages",
        "c:obsfull" | "c:obspip" => "picture-in-picture-2",
        "d:output" => "monitor",
        "b:songs" => "music",
        "b:lib" => "library",
        "b:cameras" => "camera",
        "d:remote" => "smartphone",
        "d:trigger" => "gamepad-2",
        "d:devices" => "sliders-horizontal",
        "d:lan" => "radio",
        "d:obs" => "radio",
        "d:bulletin" => "clipboard-paste",
        "a:home" => "globe",
        "a:points" => "presentation",
        "a:preservice" => "images",
        "b:diagnose" => "list-checks",
        "b:logbundle" => "download",
        "b:logs" => "folder-open",
        "a:help-manual" => "book",
        "a:help-readme" => "image",
        "a:help-obs" => "video",
        "a:help-keys" => "keyboard",
        "a:help-news" => "sparkles",
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
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::same(6))
        .shadow(egui::Shadow { offset: [0, 10], blur: 30, spread: 0, color: alpha(egui::Color32::BLACK, if p.dark { 0.6 } else { 0.18 }) })
}

/// 메뉴 줄 맨 위 글자 단추 (.mb-top: 테두리·바탕 없음, 13px 500 text2, hover·열림 때 hover 바탕)
fn top_button(ui: &mut Ui, id: egui::Id, text: &str, open: bool) -> Response {
    let p = pal();
    let g = ui.painter().layout_no_wrap(text.to_string(), theme::font(TOP_FONT, Weight::Medium), p.text2);
    let size = Vec2::new(g.size().x + TOP_PAD.x * 2.0, (g.size().y + TOP_PAD.y * 2.0).max(24.0));
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let resp = ui.interact(rect, id, Sense::click());
    if ui.is_rect_visible(rect) {
        if open || resp.hovered() {
            ui.painter().rect_filled(rect, CornerRadius::same(4), p.hover);
        }
        ui.painter().galley(rect.center() - g.size() / 2.0, g, p.text2);
    }
    resp
}

/// 펼친 메뉴의 한 줄 (.mb-item: 18px 아이콘 칸 · 글 · 오른쪽 kbd, hover 때 sel 바탕).
/// `check` 가 Some 이면 아이콘 칸에 ✓ (원본의 '✓ ' 앞글자)
pub fn item_row(ui: &mut Ui, icon: Option<&str>, label: &str, key: &str, check: Option<bool>) -> Response {
    let p = pal();
    let w = ui.available_width().max(DROP_W - 12.0);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, ITEM_H), Sense::click());
    if !ui.is_rect_visible(rect) {
        return resp;
    }
    let hov = resp.hovered() || resp.has_focus();
    let pt = ui.painter();
    if hov {
        pt.rect_filled(rect, CornerRadius::same(4), p.sel);
    }
    let ic_rect = Rect::from_center_size(Pos2::new(rect.left() + 8.0 + 9.0, rect.center().y), Vec2::splat(16.0));
    let ic_col = if hov { p.text } else { p.muted };
    match (check, icon) {
        (Some(true), _) => icons::paint(ui, ic_rect, "check", p.text),
        (_, Some(name)) => icons::paint(ui, ic_rect, name, ic_col),
        _ => {}
    }
    let x = rect.left() + 8.0 + 18.0 + 9.0;
    pt.text(Pos2::new(x, rect.center().y), Align2::LEFT_CENTER, label, theme::font(ITEM_FONT, Weight::Regular), p.text);
    if !key.is_empty() {
        pt.text(Pos2::new(rect.right() - 10.0, rect.center().y), Align2::RIGHT_CENTER, key, theme::font(11.0, Weight::Regular), p.muted);
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

/// 메뉴 줄을 그리고, 누른 항목 id 를 돌려준다
pub fn show(ui: &mut Ui, menus: &[Menu]) -> Option<&'static str> {
    show_with(ui, menus, |_, _| {})
}

/// 메뉴 줄 + 메뉴마다 끝에 붙이는 동적 항목(`extra(메뉴 이름, ui)`, 예: 보기 → 출력 템플릿 고르기)
pub fn show_with(ui: &mut Ui, menus: &[Menu], mut extra: impl FnMut(&str, &mut Ui)) -> Option<&'static str> {
    let mut picked = None;
    let ctx = ui.ctx().clone();
    ui.spacing_mut().interact_size.y = 24.0; // 줄 높이 = .mb-top 높이 (기본 34 이면 메뉴 줄이 두꺼워짐)
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0; // .menubar { gap: 0 }
        let ids: Vec<egui::Id> = menus.iter().map(|m| ui.id().with(("mb-top", m.label))).collect();
        let any_open = ids.iter().any(|id| egui::Popup::is_id_open(&ctx, id.with("popup")));
        for m in menus {
            if m.items.is_empty() {
                continue;
            }
            let label = wp_core::i18n::tr(m.label); // [트랙 D] 영어 화면
            let id = ui.id().with(("mb-top", m.label));
            let pid = id.with("popup");
            let open = egui::Popup::is_id_open(&ctx, pid);
            let resp = top_button(ui, id, &label, open);
            crate::widgets::mark(&format!("menu-{}", m.label), resp.rect); // 합성 입력 시험용 자리
            // 다른 메뉴가 열려 있으면 올리기만 해도 옮겨 연다 (원본 메뉴 줄 동작)
            if any_open && !open && resp.hovered() {
                egui::Popup::open_id(&ctx, pid);
            }
            // 점검용: --open-menu 보기 → 처음에 그 메뉴를 펼친 채로 (스크린샷 비교)
            if !TEST_OPENED.load(std::sync::atomic::Ordering::Relaxed)
                && std::env::args().position(|x| x == "--open-menu").and_then(|i| std::env::args().nth(i + 1)).is_some_and(|x| x == m.label)
                && ctx.cumulative_pass_nr() > 3
            {
                TEST_OPENED.store(true, std::sync::atomic::Ordering::Relaxed);
                egui::Popup::open_id(&ctx, pid);
            }
            // 펼침 너비: 가장 긴 줄에 맞춘다 (최소 290)
            let mut w: f32 = DROP_W - 12.0;
            for i in &m.items {
                if i.id == "-" {
                    continue;
                }
                let tw = ui.painter().layout_no_wrap(wp_core::i18n::tr(i.label), theme::font(ITEM_FONT, Weight::Regular), pal().text).size().x;
                let kw = if i.key.is_empty() { 0.0 } else { ui.painter().layout_no_wrap(i.key.to_string(), theme::font(11.0, Weight::Regular), pal().muted).size().x + 12.0 };
                w = w.max(35.0 + tw + kw + 18.0);
            }
            egui::Popup::menu(&resp).id(pid).frame(drop_frame()).gap(2.0).show(|ui| {
                ui.set_width(w);
                ui.spacing_mut().item_spacing.y = 0.0;
                for i in &m.items {
                    if i.id == "-" {
                        sep(ui);
                        continue;
                    }
                    // 화면 언어 항목은 지금 고른 것에 체크
                    let check = match i.id { "c:lang-ko" => Some(!wp_core::i18n::is_en()), "c:lang-en" => Some(wp_core::i18n::is_en()), _ => None };
                    let label = if i.id.starts_with("c:lang-") { i.label.to_string() } else { wp_core::i18n::tr(i.label) };
                    let row = item_row(ui, icon_of(i.id), &label, &i.key, check);
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
                extra(m.label, ui);
            });
        }
    });
    picked
}

/// 옆으로 펼치는 하위 메뉴 줄 (.mb-hassub: 오른쪽 chevron-right 14px). `content` 가 하위 메뉴 내용
pub fn sub_row<R>(ui: &mut Ui, icon: Option<&str>, label: &str, content: impl FnOnce(&mut Ui) -> R) -> Option<R> {
    let resp = item_row(ui, icon, label, "", None);
    let ar = Rect::from_center_size(Pos2::new(resp.rect.right() - 10.0 - 7.0, resp.rect.center().y), Vec2::splat(14.0));
    icons::paint(ui, ar, "chevron-right", pal().muted);
    egui::containers::menu::SubMenu::new()
        .show(ui, &resp, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.set_min_width(220.0); // .mb-sub { min-width: 220px }
            content(ui)
        })
        .map(|r| r.inner)
}
