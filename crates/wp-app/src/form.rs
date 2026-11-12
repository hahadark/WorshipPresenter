//! [트랙 C] JSON 값(serde_json::Value)을 경로로 고치는 작은 입력칸 도우미 (템플릿·설정 창에서 같이 씀)

use crate::paint::parse_color;
use eframe::egui;
use serde_json::{json, Value};

pub fn at<'a>(v: &'a mut Value, path: &[&str]) -> &'a mut Value {
    let mut c = v;
    for p in path {
        c = &mut c[*p];
    }
    c
}

pub fn num(ui: &mut egui::Ui, v: &mut Value, path: &[&str], label: &str, lo: f64, hi: f64, speed: f64) -> bool {
    let cur = at(v, path);
    let mut x = cur.as_f64().unwrap_or(lo);
    ui.label(wp_core::i18n::tr(label));
    let r = ui.add(egui::DragValue::new(&mut x).range(lo..=hi).speed(speed));
    if r.changed() {
        *cur = if speed >= 1.0 { json!(x.round() as i64) } else { json!((x * 100.0).round() / 100.0) };
        true
    } else {
        false
    }
}

pub fn text(ui: &mut egui::Ui, v: &mut Value, path: &[&str], label: &str, width: f32) -> bool {
    let cur = at(v, path);
    let mut s = cur.as_str().unwrap_or("").to_string();
    ui.label(wp_core::i18n::tr(label));
    if ui.add(egui::TextEdit::singleline(&mut s).desired_width(width)).changed() {
        *cur = json!(s);
        true
    } else {
        false
    }
}

pub fn check(ui: &mut egui::Ui, v: &mut Value, path: &[&str], label: &str) -> bool {
    let cur = at(v, path);
    let mut b = cur.as_bool().unwrap_or(false);
    if ui.checkbox(&mut b, wp_core::i18n::tr(label)).changed() {
        *cur = json!(b);
        true
    } else {
        false
    }
}

/// 고르기: opts = [(값, 보일 이름)]
pub fn pick(ui: &mut egui::Ui, v: &mut Value, path: &[&str], label: &str, opts: &[(&str, &str)]) -> bool {
    let id = path.join(".");
    let cur = at(v, path);
    let s = cur.as_str().unwrap_or("").to_string();
    let shown = opts.iter().find(|o| o.0 == s).map(|o| o.1).unwrap_or(&s).to_string();
    let mut changed = false;
    ui.label(wp_core::i18n::tr(label));
    egui::ComboBox::from_id_salt(id).selected_text(wp_core::i18n::tr(&shown)).show_ui(ui, |ui| {
        for (val, name) in opts {
            if ui.selectable_label(s == *val, wp_core::i18n::tr(name)).clicked() {
                *cur = json!(val);
                changed = true;
            }
        }
    });
    changed
}

/// 색: 고르기 단추 + 글(#rrggbb, rgba(…) 그대로 쓸 수도 있음)
pub fn color(ui: &mut egui::Ui, v: &mut Value, path: &[&str], label: &str) -> bool {
    let cur = at(v, path);
    let mut s = cur.as_str().unwrap_or("").to_string();
    let c = parse_color(&s).unwrap_or(egui::Color32::BLACK);
    let mut rgb = [c.r(), c.g(), c.b()];
    let mut changed = false;
    ui.label(wp_core::i18n::tr(label));
    if ui.color_edit_button_srgb(&mut rgb).changed() {
        s = format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]);
        changed = true;
    }
    if ui.add(egui::TextEdit::singleline(&mut s).desired_width(110.0)).changed() {
        changed = true;
    }
    if changed {
        *cur = json!(s);
    }
    changed
}
