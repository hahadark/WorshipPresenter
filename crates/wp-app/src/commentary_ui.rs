//! [트랙 C] 주석 창: 미리보기 중인 성경 범위의 주석 보기 · 주석(EPUB) 가져오기 · 지우기

use eframe::egui::{self, Color32, RichText};
use serde_json::{json, Value};
use std::sync::Arc;
use wp_core::commentary::{self, Analysis};
use wp_core::core::Shared;

#[derive(Default)]
pub struct CommentaryUi {
    pub open: bool,
    tab: u8,
    only: String,
    busy: Option<std::sync::mpsc::Receiver<Result<Analysis, String>>>,
    analysis: Option<Arc<Analysis>>,
    meta: Value,
    msg: String,
}

/// 미리보기 슬라이드(없으면 미리보기 전체)의 성경 범위 → (책, 시작 장, 절, 끝 장, 절, 글)
fn preview_range(core: &Shared) -> Option<(i64, i64, i64, i64, i64, String)> {
    let c = core.lock();
    let p = c.pres.preview.as_ref()?;
    let parts = p.slides.get(p.index)?["parts"].as_array()?.clone();
    let a = parts.first()?;
    let b = parts.last()?;
    let book = a["book"].as_i64().filter(|x| *x > 0)?;
    let label = p.slides[p.index]["ref"].as_str().unwrap_or("").to_string();
    Some((book, a["chapter"].as_i64()?, a["verse"].as_i64()?, b["chapter"].as_i64()?, b["verse"].as_i64()?, label))
}

impl CommentaryUi {
    pub fn show(&mut self, ctx: &egui::Context, core: &Shared) {
        if !self.open {
            return;
        }
        if let Some(rx) = &self.busy {
            match rx.try_recv() {
                Ok(Ok(a)) => {
                    self.meta = json!({ "id": "", "name": a.title, "source": "", "license": "" });
                    self.analysis = Some(Arc::new(a));
                    self.busy = None;
                }
                Ok(Err(e)) => { self.msg = e; self.busy = None; }
                Err(_) => ctx.request_repaint_after(std::time::Duration::from_millis(200)),
            }
        }
        let mut open = self.open;
        egui::Window::new("주석").open(&mut open).default_size([560.0, 600.0]).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, 0, "주석 보기");
                ui.selectable_value(&mut self.tab, 1, "주석 가져오기·지우기");
            });
            ui.separator();
            let list = commentary::list(&core.lock().store.db);
            if self.tab == 0 {
                if list.is_empty() {
                    ui.label(RichText::new("가져온 주석이 없습니다. '주석 가져오기'에서 EPUB 주석서를 넣으세요.").color(Color32::GRAY));
                    return;
                }
                ui.horizontal(|ui| {
                    ui.label("주석서");
                    let name = list.iter().find(|c| c["id"] == self.only.as_str()).and_then(|c| c["name"].as_str()).unwrap_or("모두").to_string();
                    egui::ComboBox::from_id_salt("cm-only").selected_text(name).show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.only, String::new(), "모두");
                        for c in &list { ui.selectable_value(&mut self.only, c["id"].as_str().unwrap_or("").to_string(), c["name"].as_str().unwrap_or("")); }
                    });
                });
                let Some((b, sc, sv, ec, ev, label)) = preview_range(core) else {
                    ui.label(RichText::new("성경 본문을 미리보기에 띄우면 그 범위의 주석이 여기 나옵니다.").color(Color32::GRAY));
                    return;
                };
                ui.label(RichText::new(label).strong().color(Color32::from_rgb(0xff, 0xd4, 0x79)));
                let only = (!self.only.is_empty()).then_some(self.only.as_str());
                let rows = commentary::commentary_for(&core.lock().store.db, b, sc, sv, ec, ev, only);
                if rows.is_empty() { ui.label(RichText::new("이 범위의 주석이 없습니다.").color(Color32::GRAY)); }
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    let mut cur = String::new();
                    for r in rows {
                        let cn = r["cname"].as_str().unwrap_or("").to_string();
                        if cn != cur { ui.add_space(6.0); ui.label(RichText::new(&cn).strong()); cur = cn; }
                        let range = if r["sc"] == r["ec"] && r["sv"] == r["ev"] { format!("{}:{}", r["sc"], r["sv"]) } else if r["sc"] == r["ec"] { format!("{}:{}-{}", r["sc"], r["sv"], r["ev"]) } else { format!("{}:{}-{}:{}", r["sc"], r["sv"], r["ec"], r["ev"]) };
                        let art = r["kind"] == "article";
                        ui.label(RichText::new(format!("{}{}  {}", if art { "[글] " } else { "" }, range, r["title"].as_str().unwrap_or(""))).color(Color32::from_rgb(0x8f, 0xb8, 0xff)));
                        ui.add(egui::Label::new(r["text"].as_str().unwrap_or("")).wrap());
                        ui.separator();
                    }
                });
            } else {
                for c in &list {
                    ui.horizontal(|ui| {
                        ui.label(format!("{}  ({}항목)", c["name"].as_str().unwrap_or(""), c["entry_count"]));
                        if ui.small_button("지우기").clicked() {
                            let id = c["id"].as_str().unwrap_or("").to_string();
                            self.msg = match commentary::delete(&core.lock().store.db, &id) { Ok(()) => "지웠습니다.".into(), Err(e) => e.to_string() };
                        }
                    });
                }
                ui.separator();
                if ui.add_enabled(self.busy.is_none(), egui::Button::new("EPUB 주석서 고르기…")).clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("EPUB", &["epub"]).pick_file() {
                        let (tx, rx) = std::sync::mpsc::channel();
                        self.busy = Some(rx);
                        self.analysis = None;
                        self.msg.clear();
                        std::thread::spawn(move || { let _ = tx.send(commentary::analyze_epub(&p).map_err(|e| e.to_string())); });
                    }
                }
                ui.label(RichText::new("OCR 로 만든 주석서는 절 표시(1:1)로 항목을 나눕니다. 결과는 참고용입니다.").small().color(Color32::GRAY));
                if self.busy.is_some() { ui.spinner(); }
                if let Some(a) = self.analysis.clone() {
                    let s = commentary::summary(&a);
                    ui.label(format!("{} — 항목 {} · 글 {} · 책 {}권", s["title"].as_str().unwrap_or(""), s["entries"], s["articles"], s["books"]));
                    let weak: Vec<&str> = s["weak"].as_array().into_iter().flatten().filter_map(|x| x.as_str()).collect();
                    if !weak.is_empty() && weak.len() < 66 { ui.label(RichText::new(format!("장이 많이 빠진 책: {}", weak.join(", "))).small().color(Color32::from_rgb(0xff, 0xb0, 0x4a))); }
                    egui::Grid::new("cm-meta").num_columns(2).show(ui, |ui| {
                        for (k, l) in [("id", "주석 ID (영문)"), ("name", "이름"), ("source", "출처"), ("license", "사용 허가")] {
                            ui.label(l);
                            let mut v = self.meta[k].as_str().unwrap_or("").to_string();
                            if ui.add(egui::TextEdit::singleline(&mut v).desired_width(300.0)).changed() { self.meta[k] = json!(v); }
                            ui.end_row();
                        }
                    });
                    if ui.button("가져오기").clicked() {
                        self.msg = match commentary::commit(&core.lock().store.db, &self.meta, &a) {
                            Ok((n, m)) => { self.analysis = None; format!("가져왔습니다: 항목 {n} · 글 {m}") }
                            Err(e) => e,
                        };
                    }
                }
            }
            if !self.msg.is_empty() { ui.label(RichText::new(&self.msg).color(Color32::from_rgb(0x8a, 0xe0, 0xa8))); }
        });
        self.open = open;
    }
}
