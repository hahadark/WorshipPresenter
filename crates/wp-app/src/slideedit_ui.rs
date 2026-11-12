//! [트랙 C] 슬라이드 편집기 (Ctrl+E): 고른 큐시트 순서의 슬라이드마다 글상자 위치·꾸미기·배경,
//! 덧붙인 글상자·그림·도형, 넣기·복제·지우기·옮기기·건너뛰기·여기서 새 순서로 나누기 (오른쪽 클릭 메뉴에도)

use crate::app::Act;
use crate::paint::{self, paint_slide};
use eframe::egui::{self, Color32, RichText, Sense, Vec2};
use serde_json::{json, Value};
use wp_core::core::Shared;
use wp_core::slideedit::{self as se, is_chunk};

#[derive(Default)]
pub struct SlideEditUi {
    pub open: bool,
    /// 보이는 슬라이드 번호
    k: usize,
    /// 공통('*') 편집 중
    common: bool,
}

fn edit_key(slide: &Value) -> String {
    slide["ekey"].as_str().unwrap_or("0").to_string()
}

impl SlideEditUi {
    #[allow(clippy::too_many_arguments)]
    pub fn show(&mut self, ctx: &egui::Context, core: &Shared, sel: Option<usize>, web_dir: &std::path::Path, data_dir: &std::path::Path, acts: &mut Vec<Act>) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        egui::Window::new(wp_core::i18n::tr("슬라이드 편집 (Ctrl+E)")).open(&mut open).default_size([980.0, 640.0]).show(ctx, |ui| {
            let Some(i) = sel else {
                ui.label(wp_core::i18n::tr("큐시트에서 순서를 먼저 고르세요."));
                return;
            };
            let (it, slides, tpl) = {
                let mut c = core.lock();
                let Some(it) = c.cue.as_ref().and_then(|q| q.items.get(i)).cloned() else { return };
                // 건너뛴 슬라이드도 보이게: 꾸미기 전 슬라이드를 다시 꾸민다
                let slides = match c.item_deck(i) {
                    Ok(d) if !is_chunk(it["type"].as_str().unwrap_or("")) && it["type"] != "hymn" => {
                        let mut bare = it.clone();
                        if let Some(o) = bare.as_object_mut() { o.remove("skip"); o.remove("slideOrder"); o.remove("extra"); o.remove("edits"); }
                        // 꾸미기 전 슬라이드: 잠깐 바꿔 끼워 만들고 되돌린다
                        let orig = std::mem::replace(&mut c.cue.as_mut().unwrap().items[i], bare);
                        let raw = c.item_deck(i);
                        c.cue.as_mut().unwrap().items[i] = orig;
                        raw.map(|d2| se::decorate(&it, d2.slides, true)).unwrap_or(d.slides)
                    }
                    Ok(d) => d.slides,
                    Err(e) => { ui.label(RichText::new(e).color(Color32::from_rgb(0xff, 0xb0, 0x4a))); return; }
                };
                (it, slides, c.template_for("main"))
            };
            if slides.is_empty() { return; }
            self.k = self.k.min(slides.len() - 1);
            let ty = it["type"].as_str().unwrap_or("").to_string();
            let chunk = matches!(ty.as_str(), "song" | "text");
            let mut new_it: Option<Value> = None;
            let mut split_back: Option<Value> = None;

            ui.horizontal(|ui| {
                ui.label(RichText::new(it["title"].as_str().unwrap_or("")).strong());
                ui.label(RichText::new(wp_core::i18n::tr_args("슬라이드 {1}장", &[&(slides.len())])).small().color(Color32::GRAY));
                ui.checkbox(&mut self.common, wp_core::i18n::tr("모든 슬라이드에 공통(*)"));
            });
            ui.separator();
            egui::Panel::left("se-list").default_size(220.0).show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    for (k, s) in slides.iter().enumerate() {
                        let skipped = s["skipped"].as_bool().unwrap_or(false);
                        let lb = s["label"].as_str().filter(|x| !x.is_empty()).map(|x| format!("¶{x}")).or_else(|| s["ref"].as_str().map(String::from)).unwrap_or_default();
                        let mut txt = RichText::new(format!("{:>2}  {}", k + 1, lb));
                        if skipped { txt = txt.strikethrough().color(Color32::GRAY); }
                        if s["edit"].is_object() { txt = txt.color(Color32::from_rgb(0x8f, 0xb8, 0xff)); }
                        let r = ui.selectable_label(k == self.k, txt);
                        if r.clicked() { self.k = k; }
                        let src = s["src"].as_u64().unwrap_or(k as u64) as usize;
                        let base = s["base"].as_u64().map(|x| x as usize);
                        r.context_menu(|ui| {
                            let mut op = |name: &str, label: &str, ui: &mut egui::Ui| {
                                if ui.button(label).clicked() {
                                    let (a, b) = se::apply_op(&it, name, if chunk { src } else { base.unwrap_or(k) });
                                    new_it = Some(a);
                                    split_back = b;
                                    ui.close();
                                }
                            };
                            if chunk {
                                op("add", "뒤에 새 슬라이드 넣기", ui);
                                op("dup", "복제", ui);
                                op("up", "위로", ui);
                                op("down", "아래로", ui);
                                op("del", "지우기", ui);
                                op("split", "여기서 새 순서로 나누기", ui);
                            } else if base.is_some() {
                                op("skip", if skipped { wp_core::i18n::tr_s("다시 보이기") } else { wp_core::i18n::tr_s("건너뛰기") }, ui);
                                let n = slides.iter().filter(|x| x["base"].is_number()).count();
                                let pos = slides.iter().take(k).filter(|x| x["base"].is_number()).count();
                                if ui.button(wp_core::i18n::tr("위로")).clicked() && pos > 0 { let mut a = it.clone(); se::move_order(&mut a, n, pos, pos - 1); new_it = Some(a); ui.close(); }
                                if ui.button(wp_core::i18n::tr("아래로")).clicked() && pos + 1 < n { let mut a = it.clone(); se::move_order(&mut a, n, pos, pos + 1); new_it = Some(a); ui.close(); }
                                if ui.button(wp_core::i18n::tr("뒤에 제목 카드 덧붙이기")).clicked() {
                                    let mut a = it.clone();
                                    let id = wp_core::util::to36(wp_core::util::now_ms() as u64);
                                    let mut ex = a["extra"].as_array().cloned().unwrap_or_default();
                                    ex.push(json!({ "id": &id[id.len().saturating_sub(8)..], "at": base.unwrap(), "text": "제목" }));
                                    a["extra"] = json!(ex);
                                    new_it = Some(a);
                                    ui.close();
                                }
                            } else if s["xid"].is_string() && ui.button(wp_core::i18n::tr("덧붙인 카드 지우기")).clicked() {
                                let mut a = it.clone();
                                let xid = s["xid"].clone();
                                let ex: Vec<Value> = a["extra"].as_array().cloned().unwrap_or_default().into_iter().filter(|e| e["id"] != xid).collect();
                                a["extra"] = json!(ex);
                                new_it = Some(a);
                                ui.close();
                            }
                        });
                    }
                });
            });
            let slide = &slides[self.k];
            let key = if self.common { "*".to_string() } else { edit_key(slide) };
            let mut e = it["edits"][&key].clone();
            if !e.is_object() { e = json!({}); }
            let before = e.clone();
            egui::CentralPanel::default_margins().show(ui, |ui| {
                // 미리보기
                let w = ui.available_width().min(560.0);
                let tw = tpl["width"].as_f64().unwrap_or(1920.0) as f32;
                let th = tpl["height"].as_f64().unwrap_or(1080.0) as f32;
                let (rect, _) = ui.allocate_exact_size(Vec2::new(w, w * th / tw), Sense::hover());
                let mut show = slide.clone();
                if let Some(m) = wp_core::hymns::merge_edit(&{ let mut x = it["edits"].clone(); if x.is_object() { x[&key] = e.clone(); } else { x = json!({ &key: e.clone() }); } x }, &edit_key(slide)) { show["edit"] = m; }
                let c = paint::Ctx { web_dir, data_dir };
                paint_slide(ui, &c, rect, Some(&show), &tpl, "", true, false, false);
                ui.add_space(6.0);
                egui::ScrollArea::vertical().id_salt("se-props").auto_shrink([false, false]).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let mut hm = e["hideMain"].as_bool().unwrap_or(false);
                        if ui.checkbox(&mut hm, wp_core::i18n::tr("본문 숨기기")).changed() { e["hideMain"] = json!(hm); }
                        let mut ao = e["allOutputs"].as_bool().unwrap_or(false);
                        if ui.checkbox(&mut ao, wp_core::i18n::tr("OBS 자막에도 적용")).changed() { e["allOutputs"] = json!(ao); }
                        ui.label(wp_core::i18n::tr("배경"));
                        let mut bg = e["bg"].as_str().unwrap_or("").to_string();
                        if ui.add(egui::TextEdit::singleline(&mut bg).hint_text(wp_core::i18n::tr("#색 또는 그림 주소")).desired_width(160.0)).changed() { e["bg"] = json!(bg); }
                    });
                    ui.horizontal(|ui| {
                        let mut has = e["box"].is_object();
                        if ui.checkbox(&mut has, wp_core::i18n::tr("글상자 위치 (화면 %)")).changed() {
                            e["box"] = if has { json!({ "x": 10, "y": 10, "w": 80, "h": 80 }) } else { Value::Null };
                        }
                        if has {
                            for k in ["x", "y", "w", "h"] {
                                let mut v = e["box"][k].as_f64().unwrap_or(0.0);
                                ui.label(k);
                                if ui.add(egui::DragValue::new(&mut v).range(-50.0..=200.0).speed(0.5)).changed() { e["box"][k] = json!(v); }
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(wp_core::i18n::tr("글자 크기"));
                        let mut sz = e["style"]["size"].as_f64().unwrap_or(0.0);
                        if ui.add(egui::DragValue::new(&mut sz).range(0.0..=400.0).speed(1.0)).on_hover_text(wp_core::i18n::tr("0 = 템플릿대로 (1080 화면 기준 px)")).changed() {
                            e["style"]["size"] = if sz >= 8.0 { json!(sz) } else { Value::Null };
                        }
                        ui.label(wp_core::i18n::tr("색"));
                        let mut col = e["style"]["color"].as_str().unwrap_or("").to_string();
                        if ui.add(egui::TextEdit::singleline(&mut col).hint_text("#ffffff").desired_width(70.0)).changed() { e["style"]["color"] = json!(col); }
                        let mut b = e["style"]["bold"].as_bool().unwrap_or(false);
                        if ui.checkbox(&mut b, wp_core::i18n::tr("굵게")).changed() { e["style"]["bold"] = json!(b); }
                        for (a, l) in [("left", "왼쪽"), ("center", "가운데"), ("right", "오른쪽")] {
                            if ui.selectable_label(e["style"]["align"] == a, l).clicked() { e["style"]["align"] = json!(a); }
                        }
                        for (a, l) in [("top", "위"), ("middle", "중간"), ("bottom", "아래")] {
                            if ui.selectable_label(e["style"]["valign"] == a, l).clicked() { e["style"]["valign"] = json!(a); }
                        }
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(wp_core::i18n::tr("덧붙인 요소")).strong());
                        for (t, l) in [("text", "+ 글상자"), ("image", "+ 그림"), ("shape", "+ 도형")] {
                            if ui.button(l).clicked() {
                                let mut items = e["items"].as_array().cloned().unwrap_or_default();
                                items.push(se::new_item(t));
                                e["items"] = json!(items);
                            }
                        }
                    });
                    let mut del = None;
                    let n = e["items"].as_array().map(|a| a.len()).unwrap_or(0);
                    for j in 0..n {
                        let x = &mut e["items"][j];
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(match x["type"].as_str() { Some("text") => "글", Some("image") => "그림", Some("shape") => "도형", _ => "요소" }).small());
                            for k in ["x", "y", "w", "h"] {
                                let mut v = x[k].as_f64().unwrap_or(0.0);
                                if ui.add(egui::DragValue::new(&mut v).range(-50.0..=200.0).speed(0.5).prefix(format!("{k} "))).changed() { x[k] = json!(v); }
                            }
                            match x["type"].as_str() {
                                Some("text") => {
                                    let mut t = x["text"].as_str().unwrap_or("").to_string();
                                    if ui.add(egui::TextEdit::singleline(&mut t).desired_width(160.0)).changed() { x["text"] = json!(t); }
                                    let mut s = x["size"].as_f64().unwrap_or(60.0);
                                    if ui.add(egui::DragValue::new(&mut s).range(8.0..=400.0).prefix("크기 ")).changed() { x["size"] = json!(s); }
                                }
                                Some("image") => {
                                    if ui.button(wp_core::i18n::tr("그림 고르기…")).clicked() {
                                        if let Some(p) = rfd::FileDialog::new().add_filter("그림", &["png", "jpg", "jpeg", "webp", "gif"]).pick_file() {
                                            let enc = percent_encoding::utf8_percent_encode(&p.to_string_lossy(), percent_encoding::NON_ALPHANUMERIC).to_string();
                                            x["src"] = json!(format!("/lib/file?p={enc}"));
                                        }
                                    }
                                }
                                _ => {
                                    let mut f = x["fill"].as_str().unwrap_or("").to_string();
                                    if ui.add(egui::TextEdit::singleline(&mut f).desired_width(70.0)).changed() { x["fill"] = json!(f); }
                                    let mut el = x["ellipse"].as_bool().unwrap_or(false);
                                    if ui.checkbox(&mut el, wp_core::i18n::tr("타원")).changed() { x["ellipse"] = json!(el); }
                                }
                            }
                            let mut below = x["below"].as_bool().unwrap_or(false);
                            if ui.checkbox(&mut below, wp_core::i18n::tr("글 뒤")).changed() { x["below"] = json!(below); }
                            if ui.small_button("✕").clicked() { del = Some(j); }
                        });
                    }
                    if let Some(j) = del { e["items"].as_array_mut().unwrap().remove(j); }
                    ui.add_space(6.0);
                    if ui.button(wp_core::i18n::tr("이 슬라이드 편집 지우기")).clicked() { e = json!({}); }
                });
            });
            if e != before {
                let mut a = it.clone();
                se::update_edit(&mut a, &key, |x| *x = e.clone());
                new_it = Some(a);
            }
            if let Some(a) = new_it {
                acts.push(Act::UpdateItem(i, a));
            }
            if let Some(b) = split_back {
                let ty: &'static str = if b["type"] == "song" { "song" } else { "text" };
                acts.push(Act::AddItem(ty, Some(b)));
            }
        });
        self.open = open;
    }
}
