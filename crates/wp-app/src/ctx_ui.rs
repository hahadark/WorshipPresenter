//! 오른쪽 클릭 메뉴 (Electron control/ctxmenu.js) 가운데 본문 줄 · 라이브러리 · 패널 메뉴.
//! 큐시트 순서 메뉴는 parity_ui::cue_row_menu, 슬라이드 메뉴는 center_ui::slide_menu 에 있다.
//! 메뉴에서 고른 것은 값으로 돌려주고, 부른 쪽이 그 줄을 '눌렀을 때 · 두 번 눌렀을 때' 와 같은 동작을 한다
//! (원본은 row.click() · dispatchEvent('dblclick') 로 같은 처리기를 탄다).
use crate::theme::{pal, rich, Weight};
use eframe::egui::{self, Response};
use wp_core::i18n::tr;

/// 메뉴 머리 (.ctx-head: 작은 흐린 제목)
pub(crate) fn head(ui: &mut egui::Ui, text: &str) {
    ui.add(egui::Label::new(rich(text, 12.0, Weight::SemiBold, pal().muted)).truncate());
}

/// 메뉴 한 줄 (.ctx-item: 선 아이콘 + 글 + 오른쪽 키 설명). 누르면 메뉴를 닫고 true.
/// 합성 입력 시험이 찾을 수 있게 자리를 "menu-<op>" 로 남긴다
pub(crate) fn item(ui: &mut egui::Ui, op: &str, icon: Option<&str>, label: &str, key: &str, enabled: bool, danger: bool) -> bool {
    let p = pal();
    let color = if danger { p.danger_text } else { p.text };
    let mut b = match icon {
        Some(ic) => egui::Button::image_and_text(crate::icons::image(ic, 14.0, color), rich(label, 13.0, Weight::Regular, color)),
        None => egui::Button::new(rich(label, 13.0, Weight::Regular, color)),
    };
    if !key.is_empty() {
        b = b.shortcut_text(rich(key, 12.0, Weight::Regular, p.muted));
    }
    let r = ui.add_enabled(enabled, b);
    crate::widgets::mark(&format!("menu-{op}"), r.rect);
    if r.clicked() {
        ui.close();
        return true;
    }
    false
}

/// 성경 본문 줄에서 고른 것 (ctxmenu.js verseMenu)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum VerseAct {
    /// 미리보기 (그 줄을 누른 것과 같음)
    Preview,
    /// ▶ 바로 송출 (그 줄을 두 번 누른 것과 같음)
    Send,
    /// 이 본문을 큐시트에 넣기 (고른 순서 다음)
    AddCue,
}

/// 성경 검색 결과·성경 순서의 절 줄: 미리보기 · ▶ 바로 송출 · 큐시트에 넣기 · 복사
pub(crate) fn verse_menu(resp: &Response, title: &str, reference: &str, text: &str) -> Option<VerseAct> {
    let mut out = None;
    resp.context_menu(|ui| {
        ui.set_min_width(250.0);
        head(ui, title);
        if item(ui, "preview", None, &tr("미리보기"), "Enter", true, false) {
            out = Some(VerseAct::Preview);
        }
        if item(ui, "send", Some("play"), &tr("바로 송출"), "Ctrl+Enter", true, false) {
            out = Some(VerseAct::Send);
        }
        ui.separator();
        if item(ui, "cue", Some("book-open"), &tr("이 본문을 큐시트에 넣기 (고른 순서 다음)"), "", true, false) {
            out = Some(VerseAct::AddCue);
        }
        if item(ui, "copy", Some("copy"), &tr("장절·본문 복사"), "", true, false) {
            crate::widgets::copy_notice(ui.ctx(), format!("{reference} {text}").trim().to_string(), tr("복사했습니다."));
        }
        if item(ui, "copyref", None, &tr("장절만 복사"), "", true, false) {
            crate::widgets::copy_notice(ui.ctx(), reference, tr("복사했습니다."));
        }
    });
    out
}

/// 라이브러리 메뉴 항목 하나 (그 줄·칸에 있는 단추와 같은 동작)
pub(crate) struct LibOp {
    pub op: &'static str,
    pub icon: Option<&'static str>,
    pub label: String,
    pub danger: bool,
}
impl LibOp {
    pub fn new(op: &'static str, icon: Option<&'static str>, label: impl Into<String>) -> Self {
        Self { op, icon, label: label.into(), danger: false }
    }
    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }
}

/// 라이브러리 줄·칸 (ctxmenu.js libraryMenu): 머리(이름) · '큐시트에 넣기 (고른 순서 다음)  더블클릭' · 그 줄의 단추들.
/// 고른 것의 op 를 돌려준다 ('큐시트에 넣기' 는 "cue")
pub(crate) fn lib_menu(resp: &Response, name: &str, can_add: bool, ops: &[LibOp]) -> Option<&'static str> {
    let mut out = None;
    resp.context_menu(|ui| {
        ui.set_min_width(240.0);
        let n: String = name.trim().chars().take(40).collect();
        head(ui, if n.is_empty() { "라이브러리" } else { &n });
        if can_add {
            if item(ui, "cue", Some("file-plus"), &tr("큐시트에 넣기 (고른 순서 다음)"), &tr("더블클릭"), true, false) {
                out = Some("cue");
            }
        }
        if !ops.is_empty() {
            ui.separator();
        }
        for o in ops {
            if item(ui, o.op, o.icon, &o.label, "", true, o.danger) {
                out = Some(o.op);
            }
        }
    });
    out
}
