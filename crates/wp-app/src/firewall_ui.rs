//! 방화벽 줄: 연결 점검 · 휴대폰 리모컨 · 내부망 출력 · 휴대폰 카메라 창에 같은 줄을 넣는다.
//! 확인은 다른 스레드에서 (PowerShell 1~3초), 열기는 관리자 승인 창을 띄운다.

use eframe::egui::{self, Color32, RichText};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde_json::Value;
use wp_core::i18n::tr;

#[derive(Default)]
struct St {
    busy: bool,
    result: Option<Value>,
    note: String,
}

static ST: Lazy<Mutex<St>> = Lazy::new(Default::default);

fn exe() -> std::path::PathBuf {
    std::env::current_exe().unwrap_or_default()
}

/// 확인 (이미 확인 중이면 그대로)
pub fn check(ctx: &egui::Context) {
    {
        let mut s = ST.lock();
        if s.busy {
            return;
        }
        s.busy = true;
    }
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let r = wp_core::firewall::status(&exe());
        let mut s = ST.lock();
        s.result = Some(r);
        s.busy = false;
        ctx.request_repaint();
    });
}

fn open(ctx: &egui::Context) {
    {
        let mut s = ST.lock();
        if s.busy {
            return;
        }
        s.busy = true;
        s.note = tr("관리자 승인 창에서 [예]를 누르세요…");
    }
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let r = wp_core::firewall::open(&exe());
        let st = wp_core::firewall::status(&exe());
        let mut s = ST.lock();
        s.note = match r {
            Ok(()) if st["state"] == "ok" => tr("방화벽을 열었습니다. 같은 Wi-Fi 의 휴대폰·PC 에서 다시 열어 보세요."),
            Ok(()) => tr("규칙은 만들었지만 아직 열림으로 보이지 않습니다. 다시 확인을 눌러 보세요."),
            Err(e) => e,
        };
        s.result = Some(st);
        s.busy = false;
        ctx.request_repaint();
    });
}

/// 방화벽 줄 하나 (처음 그릴 때 저절로 확인)
pub fn row(ui: &mut egui::Ui) {
    // 맥: 확인·열기는 Windows 방화벽 전용이라 안내만 (전에는 '열려 있음' 으로 잘못 보였다)
    if !cfg!(windows) {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(tr("방화벽")).strong());
            ui.label(RichText::new(tr("휴대폰·다른 PC 에서 안 열리면 시스템 설정 → 네트워크 → 방화벽 → 옵션에서 워십프리젠터의 들어오는 연결을 허용하세요.")).small());
        });
        return;
    }
    let (busy, result, note) = {
        let s = ST.lock();
        (s.busy, s.result.clone(), s.note.clone())
    };
    if result.is_none() && !busy {
        check(ui.ctx());
    }
    ui.horizontal_wrapped(|ui| {
        let state = result.as_ref().and_then(|r| r["state"].as_str()).unwrap_or("");
        let (mark, col) = match state {
            "ok" => ("✔", crate::theme::pal().ok_text),
            "blocked" => ("✖", crate::theme::pal().danger_text),
            "none" => ("!", crate::theme::pal().warn),
            _ => ("…", crate::theme::pal().muted),
        };
        ui.label(RichText::new(mark).strong().color(col));
        ui.label(RichText::new(tr("방화벽")).strong());
        let text = if busy && result.is_none() { tr("확인하는 중…") } else { result.as_ref().map(wp_core::firewall::describe).unwrap_or_default() };
        ui.label(RichText::new(text).small());
        if state != "ok" && !state.is_empty() {
            if ui
                .add_enabled(!busy, egui::Button::new(tr("방화벽 열기")))
                .on_hover_text(tr("관리자 승인 한 번으로, 같은 내부망(Wi-Fi·유선)의 휴대폰·PC 만 이 프로그램에 들어오게 허용합니다. 인터넷 쪽에서는 그대로 막힙니다."))
                .clicked()
            {
                open(ui.ctx());
            }
        }
        if ui.add_enabled(!busy, egui::Button::new(tr("다시 확인")).small()).clicked() {
            ST.lock().note.clear();
            check(ui.ctx());
        }
    });
    if !note.is_empty() {
        ui.label(RichText::new(note).small().color(Color32::LIGHT_GRAY));
    }
}
