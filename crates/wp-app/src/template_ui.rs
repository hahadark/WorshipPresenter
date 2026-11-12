//! [트랙 C] 출력 템플릿 편집 창 (Electron 설정 → 템플릿): 모든 칸을 고치고 바로 미리 보기, 저장·복제·삭제·출력에 지정

use crate::form::{check, color, num, pick, text};
use crate::paint::{self, paint_slide};
use eframe::egui::{self, Color32, RichText, Sense, Vec2};
use serde_json::{json, Value};
use wp_core::core::Shared;
use wp_core::templates;

#[derive(Default)]
pub struct TemplateUi {
    pub open: bool,
    sel: String,
    draft: Value,
    dirty: bool,
    msg: String,
}

fn sample() -> Value {
    json!({ "kind": "bible", "ref": "요한복음 3:16", "title": "요한복음 3:16-17", "parts": [
        { "book": 43, "chapter": 3, "verse": 16, "text": "하나님이 세상을 이처럼 사랑하사 독생자를 주셨으니 이는 그를 믿는 자마다 멸망하지 않고 영생을 얻게 하려 하심이라", "heading": "" }
    ] })
}

impl TemplateUi {
    pub fn show(&mut self, ctx: &egui::Context, core: &Shared, web_dir: &std::path::Path, data_dir: &std::path::Path) {
        if !self.open {
            return;
        }
        let (list, active_main, active_overlay) = {
            let c = core.lock();
            let s = c.settings.get();
            (c.settings.templates(), s["activeTemplates"]["main"].as_str().unwrap_or("").to_string(), s["activeTemplates"]["overlay"].as_str().unwrap_or("").to_string())
        };
        if self.draft.is_null() {
            self.sel = active_main.clone();
            self.draft = list.iter().find(|t| t["id"] == self.sel.as_str()).cloned().unwrap_or(Value::Null);
            self.dirty = false;
        }
        let mut open = self.open;
        egui::Window::new(wp_core::i18n::tr("출력 템플릿")).open(&mut open).default_size([1100.0, 760.0]).vscroll(false).show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                // 왼쪽: 템플릿 목록
                ui.vertical(|ui| {
                    ui.set_width(220.0);
                    egui::ScrollArea::vertical().id_salt("tpl-list").max_height(600.0).show(ui, |ui| {
                        for t in &list {
                            let id = t["id"].as_str().unwrap_or("");
                            let mut name = t["name"].as_str().unwrap_or("").to_string();
                            if id == active_main { name.push_str("  · 예배당"); }
                            if id == active_overlay { name.push_str("  · OBS"); }
                            if ui.selectable_label(self.sel == id, name).clicked() {
                                self.sel = id.to_string();
                                self.draft = t.clone();
                                self.dirty = false;
                                self.msg.clear();
                            }
                        }
                    });
                    ui.separator();
                    if ui.button(wp_core::i18n::tr("새 템플릿")).clicked() {
                        let mut t = wp_core::settings::TEMPLATE_BASE.clone();
                        t["id"] = json!("");
                        t["name"] = json!("새 템플릿");
                        self.draft = t;
                        self.sel.clear();
                        self.dirty = true;
                    }
                });
                ui.separator();
                if self.draft.is_null() {
                    return;
                }
                // 가운데: 칸들
                let mut ch = false;
                ui.vertical(|ui| {
                    ui.set_width(520.0);
                    egui::ScrollArea::vertical().id_salt("tpl-fields").max_height(640.0).show(ui, |ui| {
                        ch |= fields(ui, &mut self.draft);
                    });
                });
                self.dirty |= ch;
                ui.separator();
                // 오른쪽: 미리보기 + 단추
                ui.vertical(|ui| {
                    let w = ui.available_width().max(240.0);
                    let tw = self.draft["width"].as_f64().unwrap_or(1920.0) as f32;
                    let th = self.draft["height"].as_f64().unwrap_or(1080.0) as f32;
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, w * th / tw), Sense::hover());
                    let c = paint::Ctx { web_dir, data_dir };
                    paint_slide(ui, &c, rect, Some(&sample()), &self.draft, "개역개정", true, false, false);
                    ui.painter().rect_stroke(rect, 2.0, egui::Stroke::new(1.0, Color32::from_gray(80)), egui::StrokeKind::Outside);
                    ui.add_space(8.0);
                    ui.horizontal_wrapped(|ui| {
                        if ui.add_enabled(self.dirty, egui::Button::new(wp_core::i18n::tr("저장"))).clicked() {
                            let r = templates::save_template(&mut core.lock().settings, &self.draft);
                            match r {
                                Ok(t) => {
                                    self.sel = t["id"].as_str().unwrap_or("").to_string();
                                    self.draft = t;
                                    self.dirty = false;
                                    self.msg = wp_core::i18n::tr("저장했습니다.").into();
                                    let c = core.lock();
                                    c.emit(wp_core::core::Ev::Live);
                                    c.emit(wp_core::core::Ev::Ui);
                                }
                                Err(e) => self.msg = e,
                            }
                        }
                        if ui.button(wp_core::i18n::tr("복제")).clicked() && !self.sel.is_empty() {
                            match templates::duplicate_template(&mut core.lock().settings, &self.sel, None) {
                                Ok(t) => { self.sel = t["id"].as_str().unwrap_or("").into(); self.draft = t; self.dirty = false; self.msg = wp_core::i18n::tr("복제했습니다.").into(); }
                                Err(e) => self.msg = e,
                            }
                        }
                        if ui.button(RichText::new(wp_core::i18n::tr("삭제")).color(Color32::from_rgb(0xff, 0x8a, 0x8a))).clicked() && !self.sel.is_empty() {
                            match templates::delete_template(&mut core.lock().settings, &self.sel) {
                                Ok(()) => { self.sel.clear(); self.draft = Value::Null; self.msg = wp_core::i18n::tr("삭제했습니다.").into(); }
                                Err(e) => self.msg = e,
                            }
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        for (slot, label) in [("main", "예배당에 쓰기"), ("overlay", "OBS 자막에 쓰기"), ("overlayLyrics", "OBS 찬송가·찬양에 쓰기")] {
                            if ui.add_enabled(!self.sel.is_empty() && !self.dirty, egui::Button::new(label)).clicked() {
                                core.lock().set_template(slot, &self.sel);
                                self.msg = wp_core::i18n::tr_args("{1} 했습니다.", &[&label]);
                            }
                        }
                    });
                    if self.dirty { ui.label(RichText::new(wp_core::i18n::tr("저장하지 않은 고침이 있습니다")).small().color(Color32::from_rgb(0xff, 0xb0, 0x4a))); }
                    if !self.msg.is_empty() { ui.label(RichText::new(&self.msg).small()); }
                });
            });
        });
        self.open = open;
    }
}

const ALIGN: &[(&str, &str)] = &[("left", "왼쪽"), ("center", "가운데"), ("right", "오른쪽"), ("hanging", "왼쪽 + 내어쓰기")];

/// 템플릿의 모든 칸 (templates.js base 의 칸 전부)
pub fn fields(ui: &mut egui::Ui, t: &mut Value) -> bool {
    let mut ch = false;
    let grid = |ui: &mut egui::Ui, id: &str, f: &mut dyn FnMut(&mut egui::Ui)| {
        egui::Grid::new(id).num_columns(4).spacing([8.0, 6.0]).min_col_width(70.0).show(ui, |ui| f(ui));
    };
    ui.heading(wp_core::i18n::tr("기본"));
    grid(ui, "g-basic", &mut |ui| {
        ch |= text(ui, t, &["name"], "이름", 200.0);
        ui.end_row();
        ch |= num(ui, t, &["width"], "가로(px)", 320.0, 7680.0, 1.0);
        ch |= num(ui, t, &["height"], "세로(px)", 180.0, 4320.0, 1.0);
        ui.end_row();
        ch |= num(ui, t, &["safeArea"], "안전 영역(%)", 0.0, 20.0, 0.1);
        ch |= check(ui, t, &["clearHidesBackground"], "글자 숨김 때 배경도 숨김");
        ui.end_row();
    });
    ui.heading(wp_core::i18n::tr("배경"));
    grid(ui, "g-bg", &mut |ui| {
        ch |= pick(ui, t, &["background", "type"], "종류", &[("color", "단색"), ("gradient", "그라데이션"), ("image", "그림"), ("video", "영상"), ("camera", "카메라"), ("transparent", "투명 (OBS)")]);
        ch |= color(ui, t, &["background", "color"], "색");
        ui.end_row();
        ch |= text(ui, t, &["background", "gradient"], "그라데이션(CSS)", 200.0);
        ch |= num(ui, t, &["background", "dim"], "어둡게(0~1)", 0.0, 1.0, 0.01);
        ui.end_row();
        ch |= text(ui, t, &["background", "image"], "그림 주소", 200.0);
        ch |= pick(ui, t, &["background", "imageFit"], "맞춤", &[("cover", "꽉 채움"), ("contain", "다 보이게")]);
        ui.end_row();
        ch |= text(ui, t, &["background", "video"], "영상 주소", 200.0);
        ui.end_row();
    });
    ui.heading(wp_core::i18n::tr("글꼴"));
    grid(ui, "g-font", &mut |ui| {
        ch |= text(ui, t, &["font", "family"], "글꼴", 200.0);
        ch |= color(ui, t, &["font", "color"], "색");
        ui.end_row();
        ch |= num(ui, t, &["font", "size"], "크기", 8.0, 400.0, 1.0);
        ch |= num(ui, t, &["font", "minSize"], "최소 크기", 8.0, 400.0, 1.0);
        ui.end_row();
        ch |= check(ui, t, &["font", "autoFit"], "넘치면 줄이기");
        ch |= num(ui, t, &["font", "weight"], "굵기", 100.0, 900.0, 100.0);
        ui.end_row();
        ch |= num(ui, t, &["font", "lineHeight"], "줄 간격", 0.8, 3.0, 0.01);
        ch |= num(ui, t, &["font", "letterSpacing"], "자간(px)", -10.0, 30.0, 0.1);
        ui.end_row();
        ch |= pick(ui, t, &["align"], "가로 정렬", ALIGN);
        ch |= pick(ui, t, &["valign"], "세로 정렬", &[("top", "위"), ("middle", "가운데"), ("bottom", "아래")]);
        ui.end_row();
    });
    ui.heading(wp_core::i18n::tr("여백(px)"));
    grid(ui, "g-pad", &mut |ui| {
        ch |= num(ui, t, &["padding", "top"], "위", 0.0, 4000.0, 1.0);
        ch |= num(ui, t, &["padding", "bottom"], "아래", 0.0, 4000.0, 1.0);
        ui.end_row();
        ch |= num(ui, t, &["padding", "left"], "왼쪽", 0.0, 4000.0, 1.0);
        ch |= num(ui, t, &["padding", "right"], "오른쪽", 0.0, 4000.0, 1.0);
        ui.end_row();
    });
    ui.heading(wp_core::i18n::tr("절 번호 · 장절 · 번역본 · 소제목"));
    grid(ui, "g-ref", &mut |ui| {
        ch |= check(ui, t, &["verseNumbers", "show"], "절 번호");
        ch |= check(ui, t, &["verseBreak"], "절마다 줄바꿈");
        ui.end_row();
        ch |= color(ui, t, &["verseNumbers", "color"], "절 번호 색");
        ch |= num(ui, t, &["verseNumbers", "scale"], "절 번호 크기(배)", 0.2, 2.0, 0.01);
        ui.end_row();
        ch |= check(ui, t, &["reference", "show"], "장절 보이기");
        ch |= pick(ui, t, &["reference", "mode"], "장절 범위", &[("slide", "슬라이드마다"), ("passage", "전체 범위 고정")]);
        ui.end_row();
        ch |= pick(ui, t, &["reference", "position"], "장절 자리", &[("top", "본문 위"), ("bottom", "본문 아래"), ("fixed-top", "화면 위 고정"), ("fixed-bottom", "화면 아래 고정")]);
        ch |= num(ui, t, &["reference", "offset"], "가장자리 거리", 0.0, 2000.0, 1.0);
        ui.end_row();
        ch |= num(ui, t, &["reference", "size"], "장절 크기", 8.0, 300.0, 1.0);
        ch |= color(ui, t, &["reference", "color"], "장절 색");
        ui.end_row();
        ch |= num(ui, t, &["reference", "weight"], "장절 굵기", 100.0, 900.0, 100.0);
        ch |= pick(ui, t, &["reference", "align"], "장절 정렬", ALIGN);
        ui.end_row();
        ch |= check(ui, t, &["versionLabel", "show"], "번역본 이름");
        ch |= num(ui, t, &["versionLabel", "size"], "번역본 크기", 8.0, 200.0, 1.0);
        ui.end_row();
        ch |= color(ui, t, &["versionLabel", "color"], "번역본 색");
        ch |= check(ui, t, &["headings", "show"], "소제목");
        ui.end_row();
        ch |= num(ui, t, &["headings", "size"], "소제목 크기", 8.0, 200.0, 1.0);
        ch |= color(ui, t, &["headings", "color"], "소제목 색");
        ui.end_row();
    });
    ui.heading(wp_core::i18n::tr("상자 · 외곽선 · 그림자"));
    grid(ui, "g-box", &mut |ui| {
        ch |= check(ui, t, &["box", "enabled"], "글 상자");
        ch |= color(ui, t, &["box", "color"], "상자 색");
        ui.end_row();
        ch |= num(ui, t, &["box", "opacity"], "상자 불투명", 0.0, 1.0, 0.01);
        ch |= num(ui, t, &["box", "radius"], "모서리", 0.0, 200.0, 1.0);
        ui.end_row();
        ch |= num(ui, t, &["box", "padding"], "상자 안 여백", 0.0, 300.0, 1.0);
        ch |= text(ui, t, &["box", "gradient"], "상자 그라데이션", 160.0);
        ui.end_row();
        ch |= num(ui, t, &["stroke", "width"], "외곽선 두께", 0.0, 20.0, 0.5);
        ch |= color(ui, t, &["stroke", "color"], "외곽선 색");
        ui.end_row();
        ch |= check(ui, t, &["shadow", "enabled"], "그림자");
        ch |= num(ui, t, &["shadow", "blur"], "흐림", 0.0, 100.0, 1.0);
        ui.end_row();
        ch |= num(ui, t, &["shadow", "x"], "가로", -100.0, 100.0, 1.0);
        ch |= num(ui, t, &["shadow", "y"], "세로", -100.0, 100.0, 1.0);
        ui.end_row();
        ch |= text(ui, t, &["shadow", "color"], "그림자 색", 160.0);
        ui.end_row();
    });
    ui.heading(wp_core::i18n::tr("넘김 · PPT · 로고"));
    grid(ui, "g-etc", &mut |ui| {
        ch |= pick(ui, t, &["animation", "type"], "넘김", &[("dissolve", "디졸브"), ("cut", "컷")]);
        ch |= num(ui, t, &["animation", "duration"], "시간(ms)", 0.0, 5000.0, 10.0);
        ui.end_row();
        ch |= pick(ui, t, &["pptMode"], "PPT·찬송가", &[("", "자동 (투명이면 가사만)"), ("image", "그림 그대로"), ("lyrics", "가사 글자만")]);
        ui.end_row();
        ch |= check(ui, t, &["logo", "show"], "로고");
        ch |= text(ui, t, &["logo", "image"], "로고 그림", 160.0);
        ui.end_row();
        ch |= pick(ui, t, &["logo", "position"], "로고 자리", &[("top-left", "왼쪽 위"), ("top-center", "가운데 위"), ("top-right", "오른쪽 위"), ("bottom-left", "왼쪽 아래"), ("bottom-right", "오른쪽 아래")]);
        ch |= num(ui, t, &["logo", "size"], "로고 크기", 10.0, 1000.0, 1.0);
        ui.end_row();
        ch |= num(ui, t, &["logo", "opacity"], "로고 불투명", 0.0, 1.0, 0.01);
        ch |= num(ui, t, &["logo", "margin"], "로고 여백", 0.0, 500.0, 1.0);
        ui.end_row();
    });
    ch
}
