//! [트랙 C] 테마 고르기 창 (보기 → 테마 · 빠른 설정 '테마'): 구세군·방송 스타일 프리셋 10개.
//! docs/ux-rules.md: 테마마다 실제 모양 그림 카드(격자), 마우스 올림 강조, 누르면 오른쪽에 적용 미리보기
//! (예배당 화면 · OBS 자막), 적용 단추(둘 다 · 예배당만 · OBS 자막만). 지금 쓰는 테마는 배지로.

use crate::paint::{self, paint_slide};
use crate::theme::{self, alpha, pal, Weight};
use crate::widgets::{Btn, Kind, Size};
use eframe::egui::{self, Align2, CornerRadius, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Vec2};
use serde_json::{json, Value};
use wp_core::core::Shared;
use wp_core::i18n::tr;
use wp_core::themes;

#[derive(Default)]
pub struct ThemeUi {
    pub open: bool,
    /// 고른(미리 보는) 테마 id
    sel: String,
    msg: String,
}

/// 미리보기 견본 (성경 한 절)
fn sample() -> Value {
    json!({ "kind": "bible", "ref": "요한복음 3:16", "title": "요한복음 3:16", "parts": [
        { "book": 43, "chapter": 3, "verse": 16, "text": "하나님이 세상을 이처럼 사랑하사 독생자를 주셨으니 이는 그를 믿는 자마다 멸망하지 않고 영생을 얻게 하려 하심이라" }
    ] })
}

const CARD_W: f32 = 196.0;
const THUMB_H: f32 = 110.0;

impl ThemeUi {
    pub fn show(&mut self, ctx: &egui::Context, core: &Shared, web_dir: &std::path::Path, data_dir: &std::path::Path) {
        if !self.open {
            return;
        }
        let p = pal();
        let pc = paint::Ctx { web_dir, data_dir };
        let (act_main, act_ov) = {
            let c = core.lock();
            let a = &c.settings.get()["activeTemplates"];
            (a["main"].as_str().unwrap_or("").to_string(), a["overlay"].as_str().unwrap_or("").to_string())
        };
        let list = themes::list_themes();
        // 처음에는 지금 쓰는 테마(없으면 첫 테마)를 고른다
        if self.sel.is_empty() || !list.iter().any(|t| t["id"] == self.sel.as_str()) {
            self.sel = list.iter().find(|t| t["main"]["id"] == act_main.as_str()).or(list.first()).and_then(|t| t["id"].as_str()).unwrap_or("").to_string();
        }
        let smp = sample();
        let mut open = self.open;
        let screen = ctx.content_rect();
        egui::Window::new(tr("테마 고르기"))
            .open(&mut open)
            .default_size([(screen.width() - 80.0).clamp(760.0, 1260.0), (screen.height() - 120.0).clamp(480.0, 760.0)])
            .pivot(Align2::CENTER_CENTER)
            .default_pos(screen.center())
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label(RichText::new(tr("테마를 누르면 오른쪽에서 예배당 화면과 OBS 자막 모양을 미리 봅니다. '적용'을 눌러야 실제 출력이 바뀝니다.")).size(12.5).color(p.muted));
                ui.add_space(6.0);
                let h = ui.available_height().max(360.0);
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 16.0;
                    let gw = (ui.available_width() - 380.0 - 16.0).max(CARD_W + 20.0);
                    // 왼쪽: 카드 격자
                    ui.allocate_ui_with_layout(Vec2::new(gw, h), egui::Layout::top_down(egui::Align::Min), |ui| {
                        egui::ScrollArea::vertical().id_salt("theme-grid").auto_shrink([false, false]).max_height(h).show(ui, |ui| {
                            let mut group = String::new();
                            let mut idx = 0usize;
                            while idx < list.len() {
                                let g = list[idx]["group"].as_str().unwrap_or("").to_string();
                                if g != group {
                                    ui.add_space(4.0);
                                    ui.label(theme::rich(&g, 13.0, Weight::SemiBold, p.muted));
                                    group = g.clone();
                                }
                                // 같은 묶음의 카드를 한 줄씩
                                let in_group: Vec<&Value> = list[idx..].iter().take_while(|t| t["group"].as_str().unwrap_or("") == g).collect();
                                idx += in_group.len();
                                ui.horizontal_wrapped(|ui| {
                                    ui.spacing_mut().item_spacing = Vec2::new(12.0, 12.0);
                                    for t in in_group {
                                        let id = t["id"].as_str().unwrap_or("");
                                        let using_main = t["main"]["id"] == act_main.as_str();
                                        let using_ov = t["overlay"]["id"] == act_ov.as_str();
                                        if self.card(ui, &pc, &smp, t, self.sel == id, using_main, using_ov).clicked() {
                                            self.sel = id.to_string();
                                            self.msg.clear();
                                        }
                                    }
                                });
                                ui.add_space(8.0);
                            }
                        });
                    });
                    // 오른쪽: 미리보기 + 적용
                    ui.allocate_ui_with_layout(Vec2::new(380.0, h), egui::Layout::top_down(egui::Align::Min), |ui| {
                        let Some(t) = list.iter().find(|t| t["id"] == self.sel.as_str()) else { return };
                        ui.label(theme::rich(t["name"].as_str().unwrap_or(""), 16.0, Weight::Bold, p.text));
                        ui.label(theme::rich(t["desc"].as_str().unwrap_or(""), 12.5, Weight::Regular, p.muted));
                        ui.add_space(8.0);
                        for (key, label) in [("main", "예배당 화면"), ("overlay", "OBS 자막")] {
                            ui.label(theme::rich(tr(label), 12.0, Weight::Medium, p.muted));
                            let tpl = wp_core::settings::normalize_template(&t[key]);
                            let w = ui.available_width().min(380.0);
                            let (r, _) = ui.allocate_exact_size(Vec2::new(w, w * 9.0 / 16.0), Sense::hover());
                            paint_slide(ui, &pc, r, Some(&smp), &tpl, "개역개정", true, false, false);
                            ui.painter().rect_stroke(r, CornerRadius::same(3), Stroke::new(1.0, p.line2), StrokeKind::Outside);
                            ui.add_space(8.0);
                        }
                        let id = t["id"].as_str().unwrap_or("").to_string();
                        let name = t["name"].as_str().unwrap_or("").to_string();
                        let go = |outs: &[&str], msg: &mut String| {
                            let mut c = core.lock();
                            match themes::apply_theme(&mut c, &id, outs) {
                                Ok(_) => {
                                    *msg = wp_core::i18n::tr_args("'{1}' 테마를 적용했습니다.", &[&name]);
                                    c.notify(wp_core::i18n::tr_args("테마 적용: {1}", &[&name]), "ok");
                                }
                                Err(e) => {
                                    *msg = e.clone();
                                    c.notify(e, "error");
                                }
                            }
                        };
                        if ui.add(Btn::new(tr("예배당과 OBS 자막에 적용")).icon("check").kind(Kind::Primary).min_width(ui.available_width()).tooltip(tr("두 출력 모두 이 테마로 바꿉니다"))).clicked() {
                            go(&[], &mut self.msg);
                        }
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            let half = (ui.available_width() - 6.0) / 2.0;
                            if ui.add(Btn::new(tr("예배당만")).icon("monitor").size(Size::Sm).min_width(half).tooltip(tr("예배당 화면만 이 테마로 바꿉니다"))).clicked() {
                                go(&["main"], &mut self.msg);
                            }
                            if ui.add(Btn::new(tr("OBS 자막만")).icon("radio").size(Size::Sm).min_width(half).tooltip(tr("OBS 자막만 이 테마로 바꿉니다"))).clicked() {
                                go(&["overlay"], &mut self.msg);
                            }
                        });
                        ui.add_space(6.0);
                        ui.label(theme::rich(tr("적용하면 이 테마의 템플릿을 처음 모양으로 다시 만듭니다 (이 테마 템플릿을 고친 것은 되돌아갑니다). 고치려면 적용 뒤 '출력 템플릿'에서 바꾸세요."), 11.5, Weight::Regular, p.muted2));
                        if !self.msg.is_empty() {
                            ui.add_space(4.0);
                            ui.label(theme::rich(&self.msg, 12.5, Weight::Medium, p.ok_text));
                        }
                    });
                });
            });
        self.open = open;
    }

    /// 테마 카드: 실제 모양 썸네일 + 이름 + 쓰는 곳 배지. 마우스를 올리면 강조, 고르면 accent 테두리
    #[allow(clippy::too_many_arguments)]
    fn card(&self, ui: &mut egui::Ui, pc: &paint::Ctx, smp: &Value, t: &Value, selected: bool, using_main: bool, using_ov: bool) -> egui::Response {
        let p = pal();
        let (r, resp) = ui.allocate_exact_size(Vec2::new(CARD_W, THUMB_H + 46.0), Sense::click());
        let hovered = resp.hovered();
        let bg = if selected { p.sel } else if hovered { p.hover } else { p.panel2 };
        let border = if selected { p.accent } else if hovered { p.line2 } else { p.line };
        let lift = if hovered && !selected { -1.0 } else { 0.0 };
        let r = r.translate(Vec2::new(0.0, lift));
        ui.painter().rect(r, CornerRadius::same(8), bg, Stroke::new(if selected { 2.0 } else { 1.0 }, border), StrokeKind::Inside);
        let thumb = Rect::from_min_size(r.min + Vec2::new(6.0, 6.0), Vec2::new(CARD_W - 12.0, THUMB_H - 6.0));
        let tpl = wp_core::settings::normalize_template(&t["main"]);
        paint_slide(ui, pc, thumb, Some(smp), &tpl, "개역개정", true, false, false);
        ui.painter().text(Pos2::new(r.left() + 10.0, thumb.bottom() + 8.0), Align2::LEFT_TOP, t["name"].as_str().unwrap_or(""), theme::font(13.0, if selected { Weight::SemiBold } else { Weight::Regular }), p.text);
        // 쓰는 곳 배지
        let mut x = r.left() + 10.0;
        let y = thumb.bottom() + 26.0;
        for (on, label, col) in [(using_main, "예배당", p.accent), (using_ov, "OBS 자막", p.pv)] {
            if !on {
                continue;
            }
            let g = ui.painter().layout_no_wrap(tr(label), theme::font(10.5, Weight::SemiBold), col);
            let br = Rect::from_min_size(Pos2::new(x, y), Vec2::new(g.size().x + 10.0, g.size().y + 3.0));
            ui.painter().rect(br, CornerRadius::same(4), alpha(col, 0.16), Stroke::new(1.0, alpha(col, 0.55)), StrokeKind::Inside);
            ui.painter().galley(br.center() - g.size() / 2.0, g, col);
            x = br.right() + 4.0;
        }
        if hovered {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let tip = format!("{}\n{}", t["desc"].as_str().unwrap_or(""), tr("누르면 오른쪽에서 미리 봅니다"));
        resp.on_hover_text(tip)
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
