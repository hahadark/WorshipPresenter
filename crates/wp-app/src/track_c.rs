//! [트랙 C] 조작 화면에 붙는 것을 한곳에서: 메뉴 줄 · 설정 · 템플릿 · 송출 기록 · 사용 기록 · 정보 · 번역본 가져오기 자리

use crate::app::{Act, App};
use crate::history_ui::HistoryUi;
use crate::menubar;
use crate::settings_ui::{apply_ui_prefs, SettingsUi};
use crate::template_ui::TemplateUi;
use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};

#[derive(Default)]
pub struct TrackC {
    pub settings: SettingsUi,
    pub templates: TemplateUi,
    pub history: HistoryUi,
    pub slideedit: crate::slideedit_ui::SlideEditUi,
    pub themes: crate::theme_ui::ThemeUi,
    pub import: crate::import_ui::ImportUi,
    pub commentary: crate::commentary_ui::CommentaryUi,
}

/// 메뉴에서 고른 트랙 C 항목
pub fn handle_menu(app: &mut App, id: &str) {
    match id {
        "w:workspaces" => app.workspace.open = true,
        "c:settings" => app.tc.settings.open = true,
        "c:templates" => app.tc.templates.open = true,
        "c:history" => app.tc.history.open = true,
        "c:usage" => app.tc.history.usage_open = true,
        "c:about" => app.tc.history.about_open = true,
        "c:slideedit" => app.tc.slideedit.open = true,
        "c:themes" => app.tc.themes.open = true,
        "c:obsfull" => { let mut c = app.core.lock(); let f = !c.obs_full; wp_core::themes::set_obs_view(&mut c, Some(f), None); c.notify(if f { wp_core::i18n::tr_s("OBS 자막: 슬라이드 그대로 (예배당 화면 모양)") } else { wp_core::i18n::tr_s("OBS 자막: 자막 템플릿") }, "ok"); }
        "c:obspip" => { let mut c = app.core.lock(); let p = !c.settings.get()["output"]["obsPip"].as_bool().unwrap_or(false); wp_core::themes::set_obs_view(&mut c, None, Some(p)); }
        "c:import-bible" => app.tc.import.open = true,
        "c:commentary" => app.tc.commentary.open = true,
        // 트랙 B 창 (예전 도구 줄)
        "b:songs" => app.tb.songs = true,
        "b:lib" => app.tb.lib = true,
        "b:overlays" => app.tb.overlays = true,
        "b:message" => app.tb.message = true,
        "b:quick" => app.tb.quick = true,
        "b:stage" => app.tb.stage = true,
        "b:macros" => app.tb.macros = true,
        "b:cues" => app.tb.cues = true,
        "b:print" => app.print_cue(),
        "b:undo" | "b:redo" => {
            app.core.lock().undo_items(id == "b:redo"); // 결과 알림은 코어가 띄운다
            app.edit = None;
        }
        // 트랙 D 도구 창 (예전 임시 도구 단추)
        "d:remote" => crate::tools_ui::open(crate::tools_ui::Tool::Remote),
        "d:trigger" => crate::tools_ui::open(crate::tools_ui::Tool::Trigger),
        "d:devices" => crate::tools_ui::open(crate::tools_ui::Tool::Devices),
        "d:lan" => crate::tools_ui::open(crate::tools_ui::Tool::Lan),
        d if d.starts_with("d:") && crate::trackd_ui::handle_menu(d) => {} // [트랙 D] 주보 · 출력 · OBS
        a if a.starts_with("a:") && crate::a_tools::handle_menu(a) => {} // [트랙 A] 설교 포인트 · 예배 전 화면 · 홈페이지 주보
        a if a.starts_with("a:help") && crate::parity_ui::handle_menu(a) => {} // [트랙 A] 도움말
        b if b.starts_with("b:") && crate::system_ui::handle_menu(app, b) => {} // [트랙 B] 동기화 · 점검 · 그리기 · 카메라
        _ => {}
    }
}

/// 상단 메뉴 줄 (top 패널 위, control.css .menubar: menubar 바탕 · 아래 line 1px · 안쪽 3px 6px).
/// 정적 항목은 menubar::menus(), 상태에 따라 바뀌는 항목(미리보기 끄기 ✓·예배당 화면·출력 템플릿·OBS 주소)은 여기서 덧붙인다.
pub fn menu_bar(app: &mut App, ui: &mut egui::Ui, acts: &mut Vec<Act>) {
    use wp_core::i18n::tr;
    let p = crate::theme::pal();
    let (preview_off, templates, active_main, active_overlay, port) = {
        let c = app.core.lock();
        let s = c.settings.get();
        let tpls: Vec<(String, String)> = c.settings.templates().iter().map(|t| (t["id"].as_str().unwrap_or("").to_string(), t["name"].as_str().unwrap_or("").to_string())).collect();
        (s["previewMode"].as_bool() == Some(false), tpls, s["activeTemplates"]["main"].as_str().unwrap_or("").to_string(), s["activeTemplates"]["overlay"].as_str().unwrap_or("").to_string(), c.port)
    };
    let frame = egui::Frame::new().fill(p.menubar).inner_margin(egui::Margin::symmetric(6, 3));
    let picked = egui::Panel::top("menubar").frame(frame).show(ui, |ui| {
        menubar::show_with(ui, &menubar::menus(), |m, ui| match m {
            "송출" => {
                menubar::sep(ui);
                // 원본 송출 메뉴: '미리보기 끄기 (클릭하면 바로 송출)' ✓ · '보조 화면(예배당) 켜기·끄기' (Rust 판은 '예배당 화면' 으로 통일)
                if menubar::item_row(ui, None, &tr("미리보기 끄기 (클릭하면 바로 송출)"), "", Some(preview_off)).clicked() { acts.push(Act::TogglePreviewMode); ui.close(); }
                if menubar::item_row(ui, Some("monitor"), &tr("예배당 화면 켜기·끄기"), "Ctrl+Shift+F", None).clicked() { acts.push(Act::OutputToggle); ui.close(); }
            }
            "보기" => {
                // Rust 판 머리 줄에 있던 출력 템플릿 고르기 · OBS 주소 (원본은 빠른 설정 · 설정 → 출력)
                menubar::sep(ui);
                menubar::head(ui, &tr("출력"));
                for (slot, label, active) in [("main", "예배당 화면 템플릿", &active_main), ("overlay", "OBS 자막 템플릿", &active_overlay)] {
                    menubar::sub_row(ui, Some("palette"), &tr(label), |ui| {
                        for (id, n) in &templates {
                            if menubar::item_row(ui, None, n, "", Some(id == active)).clicked() { acts.push(Act::SetTemplate(slot, id.clone())); ui.close(); }
                        }
                    });
                }
                menubar::sub_row(ui, Some("copy"), &tr("OBS 주소 복사"), |ui| {
                    for (label, path) in [("예배당 화면 그대로", "/outputs/main"), ("투명 자막", "/outputs/overlay"), ("무대 화면", "/outputs/stage")] {
                        let url = format!("http://127.0.0.1:{port}{path}");
                        if menubar::item_row(ui, None, &tr(label), &url, None).clicked() { crate::widgets::copy_notice(ui.ctx(), url, obs_copied_msg(label)); ui.close(); }
                    }
                });
            }
            _ => {}
        })
    }).inner;
    if let Some(id) = picked {
        handle_menu(app, id);
    }
}

/// OBS 주소를 복사한 뒤 알림 (보기 메뉴 · 출력 설정 창 공용)
pub fn obs_copied_msg(label: &str) -> String {
    wp_core::i18n::tr_args("'{1}' 주소를 복사했습니다. OBS 브라우저 소스의 주소 칸에 붙여 넣으세요.", &[&wp_core::i18n::tr(label)])
}

/// 창들 · 단축키 · 조작 화면 테마
pub fn windows(app: &mut App, ctx: &egui::Context, acts: &mut Vec<Act>) {
    let sc = serde_json::json!({ "shortcuts": app.core.lock().settings.get()["shortcuts"].clone() });
    if crate::keys::pressed(ctx, &sc, "openHistory", "Ctrl+H") {
        app.tc.history.open = !app.tc.history.open;
    }
    if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::CTRL, Key::Comma))) {
        app.tc.settings.open = true;
    }
    let s = app.core.lock().settings.get().clone();
    apply_ui_prefs(ctx, &s);
    let core = app.core.clone();
    app.tc.settings.show(ctx, &core);
    app.tc.templates.show(ctx, &core, &app.web_dir, &app.data_dir);
    app.tc.history.show(ctx, &core, acts);
    if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::E))) { app.tc.slideedit.open = !app.tc.slideedit.open; }
    let (sel, wd, dd) = (app.sel, app.web_dir.clone(), app.data_dir.clone());
    app.tc.slideedit.show(ctx, &core, sel, &wd, &dd, acts);
    app.tc.themes.show(ctx, &core, &app.web_dir, &app.data_dir);
    app.tc.import.show(ctx, &core);
    app.tc.commentary.show(ctx, &core);
    // 점검용: --open-c settings|templates|history|usage|about
    if let Some(w) = std::env::args().position(|x| x == "--open-c").and_then(|i| std::env::args().nth(i + 1)) {
        if !OPENED.swap(true, std::sync::atomic::Ordering::SeqCst) {
            handle_menu(app, &format!("c:{w}"));
        }
    }
}

static OPENED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
