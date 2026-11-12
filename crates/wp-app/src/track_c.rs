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
}

/// 메뉴에서 고른 트랙 C 항목
pub fn handle_menu(app: &mut App, id: &str) {
    match id {
        "c:settings" => app.tc.settings.open = true,
        "c:templates" => app.tc.templates.open = true,
        "c:history" => app.tc.history.open = true,
        "c:usage" => app.tc.history.usage_open = true,
        "c:about" => app.tc.history.about_open = true,
        "c:slideedit" => app.tc.slideedit.open = true,
        "c:themes" => app.tc.themes.open = true,
        "c:obsfull" => { let mut c = app.core.lock(); let f = !c.obs_full; wp_core::themes::set_obs_view(&mut c, Some(f), None); c.notify(if f { "OBS 자막: 슬라이드 그대로 (예배당 화면 모양)" } else { "OBS 자막: 자막 템플릿" }, "ok"); }
        "c:obspip" => { let mut c = app.core.lock(); let p = !c.settings.get()["output"]["obsPip"].as_bool().unwrap_or(false); wp_core::themes::set_obs_view(&mut c, None, Some(p)); }
        "c:import-bible" => app.tc.import.open = true,
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
            app.core.lock().undo_items(id == "b:redo");
            app.edit = None;
        }
        // 트랙 D 도구 창 (예전 임시 도구 단추)
        "d:remote" => crate::tools_ui::open(crate::tools_ui::Tool::Remote),
        "d:trigger" => crate::tools_ui::open(crate::tools_ui::Tool::Trigger),
        "d:devices" => crate::tools_ui::open(crate::tools_ui::Tool::Devices),
        "d:lan" => crate::tools_ui::open(crate::tools_ui::Tool::Lan),
        d if d.starts_with("d:") && crate::trackd_ui::handle_menu(d) => {} // [트랙 D] 주보 · 출력 · OBS
        a if a.starts_with("a:") && crate::a_tools::handle_menu(a) => {} // [트랙 A] 설교 포인트 · 예배 전 화면 · 홈페이지 주보
        b if b.starts_with("b:") && crate::system_ui::handle_menu(app, b) => {} // [트랙 B] 동기화 · 점검 · 그리기 · 카메라
        _ => {}
    }
}

/// 상단 메뉴 줄 (top 패널 위). 다른 트랙 항목 id 는 돌려준다
pub fn menu_bar(app: &mut App, ui: &mut egui::Ui) {
    let picked = egui::Panel::top("menubar").show(ui, |ui| menubar::show(ui, &menubar::menus())).inner;
    if let Some(id) = picked {
        handle_menu(app, id);
    }
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
    app.tc.themes.show(ctx, &core);
    app.tc.import.show(ctx, &core);
    // 점검용: --open-c settings|templates|history|usage|about
    if let Some(w) = std::env::args().position(|x| x == "--open-c").and_then(|i| std::env::args().nth(i + 1)) {
        if !OPENED.swap(true, std::sync::atomic::Ordering::SeqCst) {
            handle_menu(app, &format!("c:{w}"));
        }
    }
}

static OPENED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
