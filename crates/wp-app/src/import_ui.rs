//! [트랙 C] 성경 번역본 창 (파일 → 성경 번역본 가져오기): 번역본 목록·삭제, 파일 분석 보고서 → 정보 입력 → 가져오기

use eframe::egui::{self, Color32, RichText};
use serde_json::{json, Value};
use std::sync::Arc;
use wp_core::core::Shared;
use wp_core::importer::{self, Analysis};

#[derive(Default)]
pub struct ImportUi {
    pub open: bool,
    analysis: Option<Arc<Analysis>>,
    busy: Option<std::sync::mpsc::Receiver<Analysis>>,
    meta: Value,
    msg: String,
    confirm_del: Option<String>,
}

impl ImportUi {
    pub fn show(&mut self, ctx: &egui::Context, core: &Shared) {
        if !self.open {
            return;
        }
        if let Some(rx) = &self.busy {
            if let Ok(a) = rx.try_recv() {
                let stem = a.report["file"].as_str().unwrap_or("").split('.').next().unwrap_or("").to_lowercase();
                let id: String = stem.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').take(32).collect();
                self.meta = json!({
                    "id": if id.len() >= 2 { id } else { String::new() },
                    "name": a.meta["name"].as_str().unwrap_or(""), "language": a.meta["language"].as_str().filter(|x| !x.is_empty()).unwrap_or("ko"),
                    "copyright": a.meta["copyright"].as_str().unwrap_or(""), "source": a.meta["source"].as_str().unwrap_or(""), "license": "",
                });
                self.analysis = Some(Arc::new(a));
                self.busy = None;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(200));
            }
        }
        let mut open = self.open;
        egui::Window::new("성경 번역본").open(&mut open).default_size([620.0, 560.0]).show(ctx, |ui| {
            ui.label(RichText::new("가지고 있는 번역본").strong());
            let versions = core.lock().store.list_versions();
            let cur = core.lock().version_id();
            egui::Grid::new("ver-list").num_columns(3).striped(true).show(ui, |ui| {
                for v in &versions {
                    ui.label(format!("{}{}", v.name, if v.id == cur { "  (쓰는 중)" } else { "" }));
                    ui.label(RichText::new(format!("{} · {}절", v.id, v.verse_count)).small().color(Color32::GRAY));
                    if self.confirm_del.as_deref() == Some(&v.id) {
                        ui.horizontal(|ui| {
                            if ui.button(RichText::new("정말 지우기").color(Color32::from_rgb(0xff, 0x8a, 0x8a))).clicked() {
                                let mut c = core.lock();
                                self.msg = match importer::delete_version(&mut c, &v.id) { Ok(()) => format!("{} 을(를) 지웠습니다.", v.name), Err(e) => format!("지우기 실패: {e}") };
                                self.confirm_del = None;
                            }
                            if ui.button("그만두기").clicked() { self.confirm_del = None; }
                        });
                    } else if ui.button("지우기").clicked() {
                        self.confirm_del = Some(v.id.clone());
                    }
                    ui.end_row();
                }
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(RichText::new("가져오기").strong());
                if ui.add_enabled(self.busy.is_none(), egui::Button::new("파일 고르기…")).clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("성경 자료", &["json", "csv", "tsv", "txt", "xml", "bdb"]).pick_file() {
                        let (tx, rx) = std::sync::mpsc::channel();
                        self.busy = Some(rx);
                        self.analysis = None;
                        self.msg.clear();
                        std::thread::spawn(move || { let _ = tx.send(importer::analyze_file(&p)); });
                    }
                }
                ui.label(RichText::new("JSON · CSV/TSV · Zefania XML · 베들레헴 .bdb").small().color(Color32::GRAY));
            });
            if self.busy.is_some() { ui.spinner(); }
            if let Some(a) = self.analysis.clone() {
                let r = &a.report;
                ui.label(format!("{} — {} {}", r["file"].as_str().unwrap_or(""), r["format"].as_str().unwrap_or(""), r["encoding"].as_str().unwrap_or("")));
                let st = &r["stats"];
                ui.label(format!("구절 {} · 책 {} · 표제 {} · 각주 {}", st["verses"], st["books"], st["headings"], st["notes"]));
                egui::ScrollArea::vertical().id_salt("imp-rep").max_height(160.0).show(ui, |ui| {
                    for (k, col) in [("errors", Color32::from_rgb(0xff, 0x8a, 0x8a)), ("warnings", Color32::from_rgb(0xff, 0xb0, 0x4a)), ("notes", Color32::GRAY)] {
                        for m in r[k].as_array().into_iter().flatten() {
                            ui.label(RichText::new(m.as_str().unwrap_or("")).small().color(col));
                        }
                    }
                });
                egui::Grid::new("imp-meta").num_columns(2).min_col_width(90.0).show(ui, |ui| {
                    for (k, l) in [("id", "번역본 ID (영문)"), ("name", "이름"), ("language", "언어 (ko · en …)"), ("copyright", "저작권"), ("source", "데이터 출처"), ("license", "사용 허가 (라이선스)")] {
                        ui.label(l);
                        let mut s = self.meta[k].as_str().unwrap_or("").to_string();
                        if ui.add(egui::TextEdit::singleline(&mut s).desired_width(320.0)).changed() { self.meta[k] = json!(s); }
                        ui.end_row();
                    }
                });
                let ok = r["errors"].as_array().is_some_and(|e| e.is_empty());
                if ui.add_enabled(ok, egui::Button::new("가져오기")).on_disabled_hover_text("검증 오류가 있으면 가져올 수 없습니다").clicked() {
                    let mut c = core.lock();
                    self.msg = match importer::commit(&mut c, &a, &self.meta) {
                        Ok(n) => { self.analysis = None; format!("가져왔습니다: {}절", n) }
                        Err(e) => e.join("\n"),
                    };
                }
            }
            if !self.msg.is_empty() { ui.label(RichText::new(&self.msg).color(Color32::from_rgb(0x8a, 0xe0, 0xa8))); }
        });
        self.open = open;
    }
}
