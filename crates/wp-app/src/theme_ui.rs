//! [트랙 C] 테마 고르기 창 (보기 → 테마): 구세군·방송 스타일 프리셋 10개를 예배당·OBS 자막에 적용

use eframe::egui::{self, Color32, RichText};
use wp_core::core::Shared;
use wp_core::themes;

#[derive(Default)]
pub struct ThemeUi {
    pub open: bool,
}

impl ThemeUi {
    pub fn show(&mut self, ctx: &egui::Context, core: &Shared) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        egui::Window::new(wp_core::i18n::tr("테마 고르기")).open(&mut open).default_size([560.0, 520.0]).show(ctx, |ui| {
            ui.label(RichText::new(wp_core::i18n::tr("적용하면 그 테마의 템플릿을 기본값으로 다시 만들어 출력에 지정합니다 (고친 같은 테마 템플릿은 되돌아갑니다).")).small().color(Color32::GRAY));
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                let mut group = String::new();
                for t in themes::list_themes() {
                    let g = t["group"].as_str().unwrap_or("").to_string();
                    if g != group {
                        ui.add_space(6.0);
                        ui.label(RichText::new(&g).strong());
                        group = g;
                    }
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new(t["name"].as_str().unwrap_or("")).strong());
                            ui.label(RichText::new(t["desc"].as_str().unwrap_or("")).small().color(Color32::GRAY));
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let id = t["id"].as_str().unwrap_or("");
                            let mut go = |outs: &[&str]| {
                                let mut c = core.lock();
                                match themes::apply_theme(&mut c, id, outs) {
                                    Ok(_) => c.notify(wp_core::i18n::tr_args("테마 적용: {1}", &[&(t["name"].as_str().unwrap_or(""))]), "ok"),
                                    Err(e) => c.notify(e, "error"),
                                }
                            };
                            if ui.button(wp_core::i18n::tr("OBS 자막만")).clicked() { go(&["overlay"]); }
                            if ui.button(wp_core::i18n::tr("예배당만")).clicked() { go(&["main"]); }
                            if ui.button(wp_core::i18n::tr("둘 다 적용")).clicked() { go(&[]); }
                        });
                    });
                    ui.separator();
                }
            });
        });
        self.open = open;
    }
}

/// 순서 편집칸: 이 순서에만 쓸 템플릿 (예배당 · OBS 자막) · 테마로 한 번에
pub fn item_templates(ui: &mut egui::Ui, e: &mut serde_json::Value, core: &Shared) -> bool {
    let tpls: Vec<(String, String)> = core.lock().settings.templates().iter().map(|t| (t["id"].as_str().unwrap_or("").to_string(), t["name"].as_str().unwrap_or("").to_string())).collect();
    let mut ch = false;
    ui.label(RichText::new(wp_core::i18n::tr("이 순서에만 쓸 템플릿 (비우면 기본)")).small());
    for (key, label, salt) in [("tplMain", "예배당", "it-tm"), ("tplOverlay", "OBS 자막", "it-to")] {
        ui.horizontal(|ui| {
            ui.label(wp_core::i18n::tr(label));
            let cur = e[key].as_str().unwrap_or("").to_string();
            let name = tpls.iter().find(|t| t.0 == cur).map(|t| t.1.clone()).unwrap_or_else(|| wp_core::i18n::tr("(기본)"));
            egui::ComboBox::from_id_salt(salt).selected_text(name).width(220.0).show_ui(ui, |ui| {
                if ui.selectable_label(cur.is_empty(), wp_core::i18n::tr("(기본)")).clicked() { e[key] = serde_json::json!(""); ch = true; }
                for (id, n) in &tpls {
                    if ui.selectable_label(*id == cur, n).clicked() { e[key] = serde_json::json!(id); ch = true; }
                }
            });
        });
    }
    ui.horizontal(|ui| {
        ui.label(wp_core::i18n::tr("테마로"));
        egui::ComboBox::from_id_salt("it-theme").selected_text(wp_core::i18n::tr("고르기…")).width(220.0).show_ui(ui, |ui| {
            for t in themes::list_themes() {
                if ui.selectable_label(false, t["name"].as_str().unwrap_or("")).clicked() {
                    if let Ok(ids) = themes::ensure_theme(&mut core.lock(), t["id"].as_str().unwrap_or("")) {
                        e["tplMain"] = ids["main"].clone();
                        e["tplOverlay"] = ids["overlay"].clone();
                        ch = true;
                    }
                }
            }
        });
    });
    ch
}
