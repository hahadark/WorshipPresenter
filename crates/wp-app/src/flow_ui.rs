//! [트랙 C] 순서 편집칸: 자동 넘김 · 글자 자동 숨김 · 처음/끝 한 번 더 숨김

use eframe::egui::{self, RichText};
use serde_json::{json, Value};

/// 바뀌었으면 true
pub fn item_fields(ui: &mut egui::Ui, e: &mut Value) -> bool {
    let mut ch = false;
    ui.label(RichText::new(wp_core::i18n::tr("송출 흐름")).small());
    ui.horizontal(|ui| {
        let mut sec = e["autoSec"].as_f64().unwrap_or(0.0);
        ui.label(wp_core::i18n::tr("자동 넘김"));
        if ui.add(egui::DragValue::new(&mut sec).range(0.0..=3600.0).speed(1.0).suffix(wp_core::i18n::tr(" 초"))).changed() {
            e["autoSec"] = json!(sec.round());
            ch = true;
        }
        let mut lp = e["autoLoop"].as_bool().unwrap_or(false);
        if ui.checkbox(&mut lp, wp_core::i18n::tr("끝에서 처음으로")).changed() {
            e["autoLoop"] = json!(lp);
            ch = true;
        }
    });
    ui.horizontal(|ui| {
        let mut h = e["hideAfter"].as_f64().unwrap_or(0.0);
        ui.label(wp_core::i18n::tr("글자 숨김"));
        if ui.add(egui::DragValue::new(&mut h).range(0.0..=3600.0).speed(1.0).suffix(wp_core::i18n::tr(" 초 뒤"))).on_hover_text(wp_core::i18n::tr("0 = 끔 · 숨긴 뒤 F6 은 다음 슬라이드를 보이며 넘김")).changed() {
            e["hideAfter"] = json!(h.round());
            ch = true;
        }
        let mut eh = e["edgeHide"].as_bool().unwrap_or(false);
        if ui.checkbox(&mut eh, wp_core::i18n::tr("처음·끝에서 한 번 더 누르면 숨김")).on_hover_text(wp_core::i18n::tr("마지막 슬라이드에서 F6(첫 슬라이드에서 F5)을 한 번 더 누르면 글자만 숨기고, 그다음에 누르면 다음(이전) 순서로")).changed() {
            e["edgeHide"] = json!(eh);
            ch = true;
        }
    });
    ui.label(RichText::new(wp_core::i18n::tr("자동 넘김 2초 이상만 동작 · 0 = 끔")).small().color(crate::theme::pal().muted));
    ch
}
