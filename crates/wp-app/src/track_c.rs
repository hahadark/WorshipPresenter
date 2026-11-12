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
}

/// 메뉴에서 고른 트랙 C 항목
pub fn handle_menu(app: &mut App, id: &str) {
    let t = &mut app.tc;
    match id {
        "c:settings" => t.settings.open = true,
        "c:templates" => t.templates.open = true,
        "c:history" => t.history.open = true,
        "c:usage" => t.history.usage_open = true,
        "c:about" => t.history.about_open = true,
        "c:import-bible" => app.core.lock().notify("성경 번역본 가져오기는 다음 차례에 옮깁니다.", "warn"),
        _ => {}
    }
}

/// 상단 메뉴 줄 (top 패널 위). 다른 트랙 항목 id 는 돌려준다
pub fn menu_bar(app: &mut App, ui: &mut egui::Ui) -> Option<&'static str> {
    let picked = egui::Panel::top("menubar").show(ui, |ui| menubar::show(ui, &menubar::menus())).inner;
    if let Some(id) = picked {
        if id.starts_with("c:") {
            handle_menu(app, id);
            return None;
        }
    }
    picked
}

/// 창들 · 단축키 · 조작 화면 테마
pub fn windows(app: &mut App, ctx: &egui::Context, acts: &mut Vec<Act>) {
    if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::CTRL, Key::H))) {
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
    // 점검용: --open-c settings|templates|history|usage|about
    if let Some(w) = std::env::args().position(|x| x == "--open-c").and_then(|i| std::env::args().nth(i + 1)) {
        if !OPENED.swap(true, std::sync::atomic::Ordering::SeqCst) {
            handle_menu(app, &format!("c:{w}"));
        }
    }
}

static OPENED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
