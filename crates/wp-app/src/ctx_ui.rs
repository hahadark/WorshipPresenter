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

// ---------------- 패널 메뉴 (ctxmenu.js previewMenu · liveMenu · clearItems · cueEmptyMenu) ----------------
use crate::app::{Act, App};

/// 순서 종류 (ctxmenu.js TYPES): (종류, 이름, 아이콘)
pub(crate) const TYPES: [(&str, &str, &str); 11] = [
    ("hymn", "찬송가·PPT", "music-2"),
    ("bible", "성경", "book-open"),
    ("song", "찬양", "music"),
    ("text", "공지", "megaphone"),
    ("title", "제목", "tag"),
    ("image", "이미지", "image"),
    ("video", "영상", "clapperboard"),
    ("slideshow", "슬라이드쇼", "images"),
    ("camera", "카메라", "camera"),
    ("audio", "음악", "volume-2"),
    ("blank", "빈 화면", "square"),
];

/// '순서 넣기' 줄들: 클립보드에서 붙여 넣기 + 종류마다 (고른 순서 다음에 들어간다)
pub(crate) fn add_items(app: &mut App, ui: &mut egui::Ui, acts: &mut Vec<Act>) {
    if item(ui, "paste", Some("clipboard"), &tr("클립보드에서 붙여 넣기"), "Ctrl+V", true, false) {
        crate::parity2_ui::open_paste();
    }
    for (ty, label, ic) in TYPES {
        if item(ui, &format!("add-{ty}"), Some(ic), &tr(label), "", true, false) {
            let ctx = ui.ctx().clone();
            crate::cue_ui::add(app, &ctx, ty, acts);
        }
    }
}

/// 큐시트 목록의 빈 자리 (cueEmptyMenu): 순서 넣기 · 주보로 큐시트 만들기 · 큐시트 관리
pub(crate) fn cue_empty_menu(app: &mut App, resp: &Response, acts: &mut Vec<Act>) {
    resp.context_menu(|ui| {
        ui.set_min_width(240.0);
        head(ui, &tr("순서 넣기"));
        add_items(app, ui, acts);
        ui.separator();
        if item(ui, "bulletin", Some("sparkles"), &tr("주보로 큐시트 만들기…"), "", true, false) {
            crate::trackd_ui::handle_menu("d:bulletin");
        }
        if item(ui, "cues", Some("folder-open"), &tr("큐시트 관리 (새로·복제·내보내기)…"), "", true, false) {
            app.tb.cues = true;
        }
    });
}

/// 음악 멈춤 (control.js #bAudio: 줄이며 끄기)
fn music_fade(ms: u64) {
    crate::music_ui::MUSIC.with(|m| m.borrow_mut().fade_out(ms));
}

/// 화면 지우기 줄들 (clearItems): LIVE 메뉴와 아래 줄 메뉴가 함께 쓴다
fn clear_items(app: &mut App, ui: &mut egui::Ui, acts: &mut Vec<Act>) {
    let (hide_bg, ov_off, media, logo) = {
        let c = app.core.lock();
        (c.layers.hide_bg, !c.b.saved_overlays.is_empty(), c.layers.media.clone(), c.layers.logo)
    };
    if item(ui, "clear", None, &tr("글자 숨김·복원"), "F10", true, false) {
        acts.push(Act::ToggleClear);
    }
    if item(ui, "bg", None, &tr(if hide_bg { "배경 다시 켜기" } else { "배경 지우기" }), "", true, false) {
        acts.push(Act::ToggleBg);
    }
    if item(ui, "ovoff", None, &tr(if ov_off { "오버레이 다시 켜기" } else { "오버레이 끄기" }), "", true, false) {
        app.core.lock().toggle_overlays_off();
    }
    if item(ui, "audio", None, &tr("음악 멈춤"), "", true, false) {
        music_fade(2500);
    }
    if item(ui, "logo", None, &tr(if logo { "로고 화면 끄기" } else { "로고 화면" }), "", true, false) {
        acts.push(Act::ToggleLogo);
    }
    if item(ui, "all", None, &tr("모두 지우기"), "", true, true) {
        acts.push(Act::ClearAll);
        music_fade(2000);
    }
    match media {
        Some(m) => {
            let label = m["label"].as_str().filter(|x| !x.is_empty()).map(String::from).unwrap_or_else(|| tr("미디어"));
            if item(ui, "mediaoff", None, &wp_core::i18n::tr_args("송출 배경 끄기 ({1})", &[&label]), "", true, false) {
                app.core.lock().set_background_media(None);
            }
        }
        None => {
            if item(ui, "media", Some("camera"), &tr("송출 배경 (카메라·미디어)…"), "", true, false) {
                crate::track_c::handle_menu(app, "b:lib");
            }
        }
    }
}

/// 미리보기 화면 (previewMenu) · LIVE 화면 (liveMenu)
pub(crate) fn pane_menu(app: &mut App, resp: &Response, is_live: bool, acts: &mut Vec<Act>) {
    resp.context_menu(|ui| {
        ui.set_min_width(250.0);
        if is_live {
            head(ui, &tr("LIVE (송출 중)"));
            if item(ui, "next", None, &tr("다음 슬라이드"), "F6", true, false) {
                acts.push(Act::Step(1));
            }
            if item(ui, "prev", None, &tr("이전 슬라이드"), "F5", true, false) {
                acts.push(Act::Step(-1));
            }
            ui.separator();
            clear_items(app, ui, acts);
            ui.separator();
            let running = app.out.running();
            if item(ui, "output", None, &tr(if running { "예배당 화면 끄기" } else { "예배당 화면 켜기" }), "Ctrl+Shift+F", true, false) {
                acts.push(Act::OutputToggle);
            }
            if item(ui, "stage", Some("monitor"), &tr("무대 화면 창 열기"), "", true, false) {
                crate::track_c::handle_menu(app, "b:stage");
            }
            if item(ui, "history", None, &tr("송출 기록"), "Ctrl+H", true, false) {
                crate::track_c::handle_menu(app, "c:history");
            }
            ui.separator();
            if item(ui, "draw", Some("brush"), &tr("화면에 그리기·강조"), "", true, false) {
                crate::track_c::handle_menu(app, "b:draw");
            }
            if item(ui, "drawclear", None, &tr("그린 것 지우기"), "", true, false) {
                app.core.lock().draw_clear();
            }
            return;
        }
        head(ui, &tr("미리보기"));
        let (has, cue) = {
            let c = app.core.lock();
            let p = c.pres.preview.as_ref();
            let cue = p.and_then(|d| {
                let i = d.passage["cueIndex"].as_i64().filter(|i| *i >= 0)? as usize;
                let open = c.cue.as_ref().is_some_and(|q| d.passage["cueId"] == q.id.as_str());
                open.then(|| (i, d.slides.get(d.index).map(crate::center_ui::slide_key).unwrap_or_default()))
            });
            (p.is_some(), cue)
        };
        if item(ui, "send", Some("play"), &tr("송출"), "F9", has, false) {
            acts.push(Act::SendPreview);
        }
        if item(ui, "prev", None, &tr("이전 슬라이드"), "PageUp", has, false) {
            acts.push(Act::PreviewStep(-1));
        }
        if item(ui, "next", None, &tr("다음 슬라이드"), "PageDown", has, false) {
            acts.push(Act::PreviewStep(1));
        }
        ui.separator();
        if item(ui, "cuenext", None, &tr("다음 순서 미리보기"), "Ctrl+↓", true, false) {
            acts.push(Act::CuePreviewStep(1));
        }
        if item(ui, "cueprev", None, &tr("이전 순서 미리보기"), "Ctrl+↑", true, false) {
            acts.push(Act::CuePreviewStep(-1));
        }
        if let Some((i, key)) = cue {
            if item(ui, "edit", Some("square-pen"), &tr("이 슬라이드 편집"), "Ctrl+E", true, false) {
                app.sel = Some(i);
                app.tc.slideedit.open_at(&key);
            }
        }
        ui.separator();
        let direct = app.core.lock().direct_mode();
        if item(ui, "mode", None, &tr(if direct { "미리보기 켜기 (거쳐서 송출)" } else { "미리보기 끄기 (바로 송출)" }), "", true, false) {
            acts.push(Act::TogglePreviewMode);
        }
    });
}

/// 아래 송출 줄의 빈 자리 (footer.bottom): 화면 지우기
pub(crate) fn footer_menu(app: &mut App, resp: &Response, acts: &mut Vec<Act>) {
    resp.context_menu(|ui| {
        ui.set_min_width(230.0);
        head(ui, &tr("화면 지우기"));
        clear_items(app, ui, acts);
    });
}