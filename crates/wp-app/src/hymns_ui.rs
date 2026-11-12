//! [트랙 A] 찬송가·교독문 PPT: 폴더 등록·찾기 · 찬송가 순서 편집칸

use crate::app::{Act, App};
use eframe::egui::{self, Color32, RichText};
use serde_json::{json, Value};
use wp_core::hymns;

const Q: &str = "hymn-q";
const RES: &str = "hymn-res";
const OPEN: &str = "hymn-open";

/// 큐시트 아래 '찬송가 찾기' (번호·제목 → 누르면 순서로 넣기)
pub fn search_box(app: &mut App, ui: &mut egui::Ui, acts: &mut Vec<Act>) {
    let id = |k: &str| egui::Id::new(k);
    let mut open: bool = ui.data(|d| d.get_temp(id(OPEN)).unwrap_or(false));
    if ui.selectable_label(open, RichText::new("찬송가·교독문 PPT 찾기").color(Color32::from_rgb(0xf0, 0x9a, 0x4a))).clicked() {
        open = !open;
        ui.data_mut(|d| d.insert_temp(id(OPEN), open));
    }
    if !open {
        return;
    }
    let mut q: String = ui.data(|d| d.get_temp(id(Q)).unwrap_or_default());
    let mut res: Vec<Value> = ui.data(|d| d.get_temp(id(RES)).unwrap_or_default());
    let folders = hymns::folders(&app.core.lock());
    ui.horizontal(|ui| {
        let r = ui.add(egui::TextEdit::singleline(&mut q).hint_text("번호(10) 또는 제목").desired_width(ui.available_width() - 90.0));
        if r.changed() || (res.is_empty() && !folders.is_empty() && q.is_empty()) {
            res = hymns::search(&app.core.lock(), &q)
                .into_iter()
                .take(80)
                .map(|h| json!({ "file": h.file.to_string_lossy(), "number": h.number, "title": h.title, "folder": h.folder }))
                .collect();
        }
        if ui.button("폴더 추가…").clicked() {
            if let Some(dir) = rfd::FileDialog::new().set_title("찬송가·교독문 PPT 폴더").pick_folder() {
                let mut c = app.core.lock();
                match hymns::add_folder(&mut c, &dir, None) {
                    Ok(v) => c.notify(format!("'{}' 폴더를 넣었습니다 ({}개)", v["name"].as_str().unwrap_or(""), v["count"]), "ok"),
                    Err(e) => c.notify(e, "error"),
                }
                res.clear();
            }
        }
    });
    if folders.is_empty() {
        ui.label(RichText::new("찬송가 PPT(.pps .ppt .pptx) 폴더를 넣으세요.").small().color(Color32::GRAY));
    }
    ui.horizontal_wrapped(|ui| {
        for f in &folders {
            let name = f["name"].as_str().unwrap_or("");
            let warn = !f["exists"].as_bool().unwrap_or(false);
            ui.label(RichText::new(format!("{} {}개{}", name, f["count"], if warn { " (폴더 없음)" } else { "" })).small().color(if warn { Color32::from_rgb(0xff, 0x8a, 0x8a) } else { Color32::GRAY }));
            if ui.small_button("×").on_hover_text("폴더 빼기 (파일은 그대로)").clicked() {
                hymns::remove_folder(&mut app.core.lock(), f["path"].as_str().unwrap_or(""));
                res.clear();
            }
        }
    });
    egui::ScrollArea::vertical().id_salt("hymn-res").max_height(150.0).show(ui, |ui| {
        for h in &res {
            let n = h["number"].as_i64().map(|n| format!("{n}  ")).unwrap_or_default();
            let t = format!("{}{}  · {}", n, h["title"].as_str().unwrap_or(""), h["folder"].as_str().unwrap_or(""));
            if ui.selectable_label(false, t).on_hover_text("눌러서 큐시트에 넣기").clicked() {
                acts.push(Act::AddItem("hymn", Some(json!({ "title": h["title"], "file": h["file"], "number": h["number"].as_i64().unwrap_or(0), "background": "default", "obsLyrics": "" }))));
            }
        }
    });
    ui.data_mut(|d| {
        d.insert_temp(id(Q), q);
        d.insert_temp(id(RES), res);
    });
}

/// 찬송가 순서 편집칸 (파일 · 번호 · 배경 · 범위 · OBS 가사) → 바뀌었으면 true
pub fn editor(ui: &mut egui::Ui, e: &mut Value) -> bool {
    let mut changed = false;
    let file = e["file"].as_str().unwrap_or("").to_string();
    ui.label(RichText::new("PPT 파일").small());
    ui.horizontal(|ui| {
        let short = file.rsplit(['\\', '/']).next().unwrap_or("");
        ui.label(RichText::new(if short.is_empty() { "(없음)" } else { short }).small()).on_hover_text(&file);
        if ui.button("고르기…").clicked() {
            if let Some(p) = rfd::FileDialog::new().add_filter("PPT", &["pptx", "ppsx", "ppt", "pps"]).pick_file() {
                let (num, title, _) = hymns::parse_name(&p);
                e["file"] = json!(p.to_string_lossy());
                e["number"] = json!(num.unwrap_or(0));
                if e["title"].as_str().is_none_or(|t| t.is_empty() || t == "찬송") {
                    e["title"] = json!(title);
                }
                changed = true;
            }
        }
    });
    ui.label(RichText::new("등록한 찬송가 폴더·붙여 넣은 슬라이드 폴더 안의 파일만 열립니다").small().color(Color32::GRAY));
    ui.label(RichText::new("배경 (악보 그림일 때)").small());
    let mut bg = e["background"].as_str().unwrap_or("default").to_string();
    ui.horizontal_wrapped(|ui| {
        for (v, l) in [("default", "설정대로"), ("original", "파일 배경"), ("template", "템플릿 배경"), ("#ffffff", "흰색"), ("#000000", "검정")] {
            if ui.radio_value(&mut bg, v.to_string(), l).changed() {
                e["background"] = json!(bg.clone());
                changed = true;
            }
        }
    });
    ui.label(RichText::new("슬라이드 범위 (예: 5-9, 비우면 전부)").small());
    let mut r = e["range"].as_str().unwrap_or("").to_string();
    if ui.add(egui::TextEdit::singleline(&mut r).desired_width(f32::INFINITY)).changed() {
        e["range"] = json!(r);
        changed = true;
    }
    ui.label(RichText::new("OBS 자막 가사 (비우면 PPT 안의 글자. 빈 줄로 슬라이드마다)").small());
    let mut o = e["obsLyrics"].as_str().unwrap_or("").to_string();
    if ui.add(egui::TextEdit::multiline(&mut o).desired_rows(4).desired_width(f32::INFINITY)).changed() {
        e["obsLyrics"] = json!(o);
        changed = true;
    }
    changed
}
