//! 빠른 설정 안쪽 — Electron `index.html .quick-head` 와 `tplcontrols.js render(…, { basic: true })` 의 모양·차례.
//! 글꼴 · 글자 크기(− 막대 + px) · 본문 색 동그라미(+무지개 고르기) · 배경 | 위치 3×3 · 내어쓰기 · 장절 위치 · 표시 · 로고 · 전환 · PPT 슬라이드
use crate::theme::{self, pal, rich, Weight};
use crate::widgets::{self, Btn, SegStyle, Size};
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, Vec2};
use serde_json::{json, Value};
use wp_core::i18n::tr;

const BASE_FONTS: [&str; 15] = ["맑은 고딕", "나눔고딕", "나눔명조", "나눔스퀘어", "바탕", "돋움", "굴림", "궁서", "Pretendard", "Noto Sans KR", "Noto Serif KR", "Arial", "Georgia", "Times New Roman", "Segoe UI"];
const TEXT_COLORS: [&str; 6] = ["#ffffff", "#fff3c4", "#ffd479", "#9ec5ff", "#b8f2c8", "#000000"];
const BG_COLORS: [&str; 7] = ["transparent", "#000000", "#0b1d3a", "#13261b", "#2a1414", "#1f1f1f", "#ffffff"];
const VALIGNS: [(&str, &str); 3] = [("top", "위"), ("middle", "가운데"), ("bottom", "아래")];
const ALIGNS: [(&str, &str); 3] = [("left", "왼쪽"), ("center", "가운데"), ("right", "오른쪽")];

/// 빠른 설정 조작에서 일어난 일
#[derive(Default)]
pub(crate) struct Out {
    /// 템플릿이 바뀜
    pub changed: bool,
    /// 확정(저장) — 막대를 끄는 중이면 false
    pub fin: bool,
    /// '자세히' 템플릿 편집기를 열어야 함 (이미지·영상·카메라·내 로고)
    pub more: bool,
}

/// 'Malgun Gothic', … → Malgun Gothic
pub(crate) fn font_name(family: &str) -> String {
    family.trim().trim_start_matches(['\'', '"']).split([',', '\'', '"']).next().unwrap_or("").trim().to_string()
}

fn font_family(name: &str) -> String {
    format!("'{}', 'Malgun Gothic', 'Apple SD Gothic Neo', sans-serif", name.replace('\'', ""))
}

fn get<'a>(t: &'a Value, path: &str) -> &'a Value {
    path.split('.').fold(t, |v, k| &v[k])
}

fn set(t: &mut Value, path: &str, v: Value) {
    let mut cur = t;
    let ks: Vec<&str> = path.split('.').collect();
    for k in &ks[..ks.len() - 1] {
        if !cur[*k].is_object() {
            cur[*k] = json!({});
        }
        cur = &mut cur[*k];
    }
    cur[ks[ks.len() - 1]] = v;
}

/// .tc-row (gap 6, 위아래 5, 높이 28 이상) + .tc-label (64px muted 12px)
fn tc_row<R>(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let r = ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.set_min_height(28.0);
        let (r, _) = ui.allocate_exact_size(Vec2::new(64.0, 28.0), Sense::hover());
        let g = ui.painter().layout(tr(label), theme::font(12.0, Weight::Regular), pal().muted, 64.0);
        ui.painter().galley(Pos2::new(r.left(), r.center().y - g.size().y / 2.0), g, pal().muted);
        add(ui)
    });
    ui.add_space(10.0);
    r.inner
}

/// 색 견본 동그라미 (.tc-sw: 24px, 2px line2 테두리 · 고르면 accent 고리)
fn swatch(ui: &mut egui::Ui, c: &str, on: bool) -> Response {
    let p = pal();
    let (r, resp) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::click());
    let pt = ui.painter();
    let ctr = r.center();
    if c == "transparent" {
        // 체크무늬 (투명)
        let cell = 5.0;
        for y in 0..5 {
            for x in 0..5 {
                let rr = Rect::from_min_size(Pos2::new(r.left() + x as f32 * cell, r.top() + y as f32 * cell), Vec2::splat(cell));
                if rr.center().distance(ctr) < 11.0 {
                    pt.rect_filled(rr, CornerRadius::ZERO, if (x + y) % 2 == 0 { p.muted2 } else { Color32::from_gray(0x22) });
                }
            }
        }
    } else {
        pt.circle_filled(ctr, 11.0, crate::paint::parse_color(c).unwrap_or(Color32::WHITE));
    }
    pt.circle_stroke(ctr, 11.0, Stroke::new(2.0, if on { p.accent } else { p.line2 }));
    if on {
        pt.circle_stroke(ctr, 15.0, Stroke::new(2.0, p.accent));
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.on_hover_text(if c == "transparent" { tr("투명 (OBS 합성용)") } else { c.to_string() })
}

/// 무지개 '+' 동그라미 (.tc-sw-pick: 다른 색 고르기)
fn rainbow(ui: &mut egui::Ui) -> Response {
    let p = pal();
    let (r, resp) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::click());
    let ctr = r.center();
    let n = 24;
    for k in 0..n {
        let a0 = k as f32 / n as f32 * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
        let a1 = (k + 1) as f32 / n as f32 * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
        let hue = k as f32 / n as f32;
        let col: Color32 = egui::ecolor::Hsva::new(hue, 1.0, 1.0, 1.0).into();
        let pts = vec![ctr, ctr + Vec2::angled(a0) * 11.0, ctr + Vec2::angled(a1) * 11.0];
        ui.painter().add(Shape::convex_polygon(pts, col, Stroke::NONE));
    }
    ui.painter().circle_stroke(ctr, 11.0, Stroke::new(2.0, p.line2));
    ui.painter().text(ctr, egui::Align2::CENTER_CENTER, "+", theme::font(14.0, Weight::Bold), Color32::WHITE);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.on_hover_text(tr("다른 색 고르기"))
}

/// 견본 줄: 고른 색(문자열)을 돌려준다
fn swatches(ui: &mut egui::Ui, id: &str, colors: &[&str], current: &str) -> Option<String> {
    let mut out = None;
    ui.spacing_mut().item_spacing.x = 5.0;
    for c in colors {
        if swatch(ui, c, current.eq_ignore_ascii_case(c)).clicked() {
            out = Some(c.to_string());
        }
    }
    let open_id = egui::Id::new(("tc-pick", id));
    let mut open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);
    if rainbow(ui).clicked() {
        open = !open;
    }
    if open {
        let mut col = crate::paint::parse_color(current).unwrap_or(Color32::WHITE);
        if egui::color_picker::color_picker_color32(ui, &mut col, egui::color_picker::Alpha::Opaque) {
            out = Some(format!("#{:02x}{:02x}{:02x}", col.r(), col.g(), col.b()));
        }
    }
    ui.data_mut(|d| d.insert_temp(open_id, open));
    out
}

/// − 막대 + 72px (.tc-step · range · .tc-val). (바뀐 값, 확정)
fn slider(ui: &mut egui::Ui, value: f64, lo: f64, hi: f64, step: f64, unit: &str) -> Option<(f64, bool)> {
    let mut out = None;
    let step_btn = |ui: &mut egui::Ui, t: &str| ui.add(Btn::new(t).size(Size::Sm).min_width(26.0));
    if step_btn(ui, "−").clicked() {
        out = Some((((value - step) * 100.0).round() / 100.0).clamp(lo, hi));
    }
    let mut v = value;
    let w = (ui.available_width() - 26.0 - 54.0 - 12.0).max(60.0);
    ui.spacing_mut().slider_width = w;
    let r = ui.add(egui::Slider::new(&mut v, lo..=hi).step_by(step).show_value(false));
    let mut fin = true;
    if r.changed() {
        out = Some(v);
        fin = !r.dragged();
    }
    if r.drag_stopped() {
        out = Some(v);
        fin = true;
    }
    if step_btn(ui, "+").clicked() {
        out = Some((((value + step) * 100.0).round() / 100.0).clamp(lo, hi));
    }
    let shown = if unit.is_empty() { format!("{:.2}", out.unwrap_or(value)) } else { format!("{}{}", out.unwrap_or(value), unit) };
    let (r, _) = ui.allocate_exact_size(Vec2::new(54.0, 24.0), Sense::hover());
    ui.painter().text(r.right_center(), egui::Align2::RIGHT_CENTER, shown, theme::font(12.0, Weight::Regular), pal().text);
    out.map(|v| (v, fin))
}

/// .tc-seg-sm (작은 분할 단추, 고르면 sel + accent 테두리)
fn seg(ui: &mut egui::Ui, cur: &mut String, items: &[(&str, &str)]) -> bool {
    let tr_items: Vec<(String, String)> = items.iter().map(|(v, l)| (v.to_string(), tr(l))).collect();
    let refs: Vec<(String, &str)> = tr_items.iter().map(|(v, l)| (v.clone(), l.as_str())).collect();
    widgets::segmented_with(ui, cur, &refs, SegStyle { font: 12.0, pad: Vec2::new(8.0, 2.0), fill: false, radius: 6 }).changed()
}

/// 위치 3×3 (.tc-pos: sunken 상자, 칸 34×22, 사이 3, 안쪽 5). 누른 (세로, 가로)
fn pos_grid(ui: &mut egui::Ui, valign: &str, col_align: &str) -> Option<(&'static str, &'static str)> {
    let p = pal();
    let (cw, ch, gap, pad) = (34.0, 22.0, 3.0, 5.0);
    let size = Vec2::new(cw * 3.0 + gap * 2.0 + pad * 2.0 + 2.0, ch * 3.0 + gap * 2.0 + pad * 2.0 + 2.0);
    let (r, _) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect(r, CornerRadius::same(6), p.sunken, Stroke::new(1.0, p.line), StrokeKind::Inside);
    let mut out = None;
    for (y, (v, vl)) in VALIGNS.iter().enumerate() {
        for (x, (a, al)) in ALIGNS.iter().enumerate() {
            let cr = Rect::from_min_size(Pos2::new(r.left() + 1.0 + pad + x as f32 * (cw + gap), r.top() + 1.0 + pad + y as f32 * (ch + gap)), Vec2::new(cw, ch));
            let resp = ui.interact(cr, egui::Id::new(("tc-pos", y, x)), Sense::click()).on_hover_text(format!("{} · {}", tr(vl), tr(al)));
            let on = *v == valign && *a == col_align;
            let (bg, border) = if on { (p.sel, p.accent) } else if resp.hovered() { (p.hover, p.line2) } else { (p.panel2, p.line) };
            ui.painter().rect(cr, CornerRadius::same(3), bg, Stroke::new(1.0, border), StrokeKind::Inside);
            // 안쪽 막대 (높이 4, 너비 60%, 왼쪽·가운데·오른쪽 맞춤)
            let bw = cw * 0.6;
            let bx = match *a {
                "left" => cr.left() + 3.0,
                "right" => cr.right() - 3.0 - bw,
                _ => cr.center().x - bw / 2.0,
            };
            ui.painter().rect_filled(Rect::from_min_size(Pos2::new(bx, cr.center().y - 2.0), Vec2::new(bw, 4.0)), CornerRadius::same(2), if on { p.text } else { p.muted2 });
            if resp.clicked() {
                out = Some((*v, *a));
            }
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
        }
    }
    out
}

/// 내어쓰기 단추 (.tc-hang: 1·2 번호와 줄 아이콘 + '내어쓰기 꺼짐/켜짐', 켜지면 pv-bg · pv 테두리)
fn hang_btn(ui: &mut egui::Ui, on: bool) -> Response {
    let p = pal();
    let label = tr(if on { "내어쓰기 켜짐" } else { "내어쓰기 꺼짐" });
    let g = ui.painter().layout_no_wrap(label, theme::font(14.0, Weight::Regular), p.text);
    let w = 12.0 + 37.0 + 8.0 + g.size().x + 12.0;
    let (r, resp) = ui.allocate_exact_size(Vec2::new(w, 46.0), Sense::click());
    let (bg, border) = if on { (p.pv_bg, p.pv) } else if resp.hovered() { (p.hover, p.line2) } else { (p.panel2, p.line) };
    let pt = ui.painter();
    pt.rect(r, CornerRadius::same(theme::RADIUS), bg, Stroke::new(1.0, border), StrokeKind::Inside);
    // 아이콘: [1][긴 줄] / [  ][짧은 줄] / [2][긴 줄]
    let x0 = r.left() + 12.0;
    let cy = r.center().y;
    let small = theme::font(9.0, Weight::Bold);
    pt.text(Pos2::new(x0 + 8.0, cy - 8.0), egui::Align2::RIGHT_CENTER, "1", small.clone(), p.hl);
    pt.text(Pos2::new(x0 + 8.0, cy + 8.0), egui::Align2::RIGHT_CENTER, "2", small, p.hl);
    for (dy, ww) in [(-8.0, 26.0), (0.0, 18.0), (8.0, 26.0)] {
        pt.rect_filled(Rect::from_min_size(Pos2::new(x0 + 11.0, cy + dy - 1.5), Vec2::new(ww, 3.0)), CornerRadius::same(2), p.text2);
    }
    pt.galley(Pos2::new(x0 + 37.0 + 8.0, cy - g.size().y / 2.0), g, p.text);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.on_hover_text(tr("절 번호를 왼쪽 칸에 두고, 줄이 바뀌어도 본문 시작 위치에 맞춥니다"))
}

/// 그라데이션 견본 (.tc-sw-grad: 34×24, 둥글기 6) — CSS 그라데이션의 처음·끝 색으로 근사
fn grad_swatch(ui: &mut egui::Ui, css: &str, on: bool) -> Response {
    let p = pal();
    let (r, resp) = ui.allocate_exact_size(Vec2::new(34.0, 24.0), Sense::click());
    let cols: Vec<Color32> = css.split(|c: char| c == ' ' || c == ',' || c == '(' || c == ')').filter(|s| s.starts_with('#')).filter_map(crate::paint::parse_color).collect();
    let (a, b) = (cols.first().copied().unwrap_or(p.panel2), cols.last().copied().unwrap_or(p.panel2));
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(r.left_top(), a);
    mesh.colored_vertex(r.right_top(), a);
    mesh.colored_vertex(r.right_bottom(), b);
    mesh.colored_vertex(r.left_bottom(), b);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    ui.painter().add(mesh);
    ui.painter().rect_stroke(r, CornerRadius::same(6), Stroke::new(2.0, if on { p.accent } else { p.line2 }), StrokeKind::Inside);
    resp.on_hover_text(tr("테마 배경 (그라데이션)"))
}

/// 체크 상자 (.tc-toggle: 13px)
fn toggle(ui: &mut egui::Ui, on: bool, label: &str) -> Option<bool> {
    let mut v = on;
    ui.checkbox(&mut v, rich(tr(label), 13.0, Weight::Regular, pal().text)).changed().then_some(v)
}

/// tplcontrols.js render(basic) — tpl 을 바로 고친다
pub(crate) fn basic_controls(ui: &mut egui::Ui, t: &mut Value) -> Out {
    let mut out = Out::default();
    let emit = |o: &mut Out, fin: bool| {
        o.changed = true;
        o.fin |= fin;
    };
    let w = ui.available_width().min(ui.clip_rect().right() - ui.cursor().left());
    let two = w >= 330.0 * 2.0 + 16.0;
    let col_w = if two { (w - 16.0) / 2.0 } else { w };
    let hanging = t["align"] == "hanging";
    let col_align = if hanging { "left".to_string() } else { t["align"].as_str().unwrap_or("center").to_string() };
    let valign = t["valign"].as_str().unwrap_or("middle").to_string();
    let section1 = |ui: &mut egui::Ui, t: &mut Value, out: &mut Out| {
        // 글꼴
        tc_row(ui, "글꼴", |ui| {
            let cur = font_name(t["font"]["family"].as_str().unwrap_or(""));
            let mut names: Vec<String> = BASE_FONTS.iter().map(|s| s.to_string()).collect();
            if !cur.is_empty() && !names.contains(&cur) {
                names.insert(0, cur.clone());
            }
            let mut sel = cur.clone();
            egui::ComboBox::from_id_salt("tc-font").selected_text(rich(&cur, 15.0, Weight::Regular, pal().text)).width(ui.available_width()).height(360.0).show_ui(ui, |ui| {
                for n in &names {
                    ui.selectable_value(&mut sel, n.clone(), n);
                }
            });
            if sel != cur {
                t["font"]["family"] = json!(font_family(&sel));
                emit(out, true);
            }
        });
        tc_row(ui, "글자 크기", |ui| {
            if let Some((v, fin)) = slider(ui, t["font"]["size"].as_f64().unwrap_or(72.0), 20.0, 200.0, 2.0, "px") {
                t["font"]["size"] = json!(v.round() as i64);
                if t["font"]["minSize"].as_f64().is_some_and(|m| m > v) {
                    t["font"]["minSize"] = json!(v.round() as i64);
                }
                emit(out, fin);
            }
        });
        tc_row(ui, "본문 색", |ui| {
            if let Some(c) = swatches(ui, "font.color", &TEXT_COLORS, t["font"]["color"].as_str().unwrap_or("")) {
                t["font"]["color"] = json!(c);
                emit(out, true);
            }
        });
        // 배경: [테마 그라데이션] 단색·투명 견본 · 카메라 · 영상 · 이미지
        tc_row(ui, "배경", |ui| {
            ui.horizontal_wrapped(|ui| {
                let ty = t["background"]["type"].as_str().unwrap_or("color").to_string();
                if let Some(g) = t["background"]["gradient"].as_str().filter(|g| !g.is_empty()).map(String::from) {
                    if grad_swatch(ui, &g, ty == "gradient").clicked() {
                        t["background"]["type"] = json!("gradient");
                        emit(out, true);
                    }
                }
                let cur = if ty == "transparent" { "transparent".to_string() } else if ty == "color" { t["background"]["color"].as_str().unwrap_or("").to_string() } else { String::new() };
                if let Some(c) = swatches(ui, "background", &BG_COLORS, &cur) {
                    if c == "transparent" {
                        t["background"]["type"] = json!("transparent");
                        t["clearHidesBackground"] = json!(true);
                    } else {
                        t["background"]["type"] = json!("color");
                        t["background"]["color"] = json!(c);
                    }
                    emit(out, true);
                }
                for (k, label, tip) in [("camera", "카메라", "웹캠·캡처보드 화면을 배경으로"), ("video", "영상", "라이브러리의 영상을 배경으로 (소리 없이 반복 재생)"), ("image", "이미지", "배경 이미지 올리기 (png · jpg · webp)")] {
                    let ic = match k { "camera" => "camera", "video" => "clapperboard", _ => "image" };
                    if ui.add(Btn::new(tr(label)).icon(ic).size(Size::Sm).height(28.0).selected(ty == k).tooltip(tr(tip))).clicked() {
                        out.more = true;
                    }
                }
            });
        });
    };
    let section2 = |ui: &mut egui::Ui, t: &mut Value, out: &mut Out| {
        // 위치 3×3 + 내어쓰기 + 'ㅇㅇ · ㅇㅇ'
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            if let Some((v, a)) = pos_grid(ui, &valign, &col_align) {
                t["valign"] = json!(v);
                t["align"] = json!(if hanging && a == "left" { "hanging" } else { a });
                t["reference"]["align"] = json!(a);
                emit(out, true);
            }
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                if hang_btn(ui, hanging).clicked() {
                    if hanging {
                        t["align"] = json!("left");
                    } else {
                        t["align"] = json!("hanging");
                        t["reference"]["align"] = json!("left");
                        t["verseNumbers"]["show"] = json!(true);
                    }
                    emit(out, true);
                }
                let vl = VALIGNS.iter().find(|x| x.0 == valign).map(|x| tr(x.1)).unwrap_or_default();
                let al = if hanging { tr("왼쪽 + 내어쓰기") } else { ALIGNS.iter().find(|x| x.0 == col_align).map(|x| tr(x.1)).unwrap_or_default() };
                ui.label(rich(format!("{vl} · {al}"), 12.0, Weight::Regular, pal().muted));
            });
        });
        ui.add_space(6.0);
        tc_row(ui, "장절 위치", |ui| {
            let mut cur = t["reference"]["position"].as_str().unwrap_or("top").to_string();
            if seg(ui, &mut cur, &[("top", "위"), ("bottom", "아래"), ("fixed-top", "위 고정"), ("fixed-bottom", "아래 고정")]) {
                t["reference"]["position"] = json!(cur);
                emit(out, true);
            }
        });
        tc_row(ui, "", |ui| {
            if let Some(v) = toggle(ui, t["reference"]["show"].as_bool().unwrap_or(true), "장절 표시") {
                t["reference"]["show"] = json!(v);
                emit(out, true);
            }
            if let Some(v) = toggle(ui, t["reference"]["mode"] == "passage", "전체 범위 고정") {
                t["reference"]["mode"] = json!(if v { "passage" } else { "slide" });
                emit(out, true);
            }
            if let Some(v) = toggle(ui, t["verseNumbers"]["show"].as_bool().unwrap_or(true), "절 번호") {
                t["verseNumbers"]["show"] = json!(v);
                emit(out, true);
            }
        });
        // 로고: 없음 · 레드 쉴드 · 크레스트 · 내 로고 올리기… (+ 위치 · 크기)
        let logos = [("", "없음"), ("/shared/assets/sa-shield.png", "레드 쉴드"), ("/shared/assets/sa-crest.png", "크레스트")];
        let cur_logo = if t["logo"]["show"].as_bool().unwrap_or(false) { t["logo"]["image"].as_str().unwrap_or("").to_string() } else { String::new() };
        let custom = !cur_logo.is_empty() && !logos.iter().any(|l| l.0 == cur_logo);
        tc_row(ui, "로고", |ui| {
            let mut cur = if custom { "custom".to_string() } else { cur_logo.clone() };
            let mut items: Vec<(&str, &str)> = logos.to_vec();
            items.push(("custom", if custom { "내 로고" } else { "내 로고 올리기…" }));
            if seg(ui, &mut cur, &items) {
                if cur == "custom" {
                    out.more = true;
                } else if cur.is_empty() {
                    set(t, "logo.show", json!(false));
                    emit(out, true);
                } else {
                    for (k, v) in [("position", json!("top-right")), ("size", json!(120)), ("margin", json!(40)), ("opacity", json!(1))] {
                        if get(t, &format!("logo.{k}")).is_null() {
                            set(t, &format!("logo.{k}"), v);
                        }
                    }
                    set(t, "logo.image", json!(cur));
                    set(t, "logo.show", json!(true));
                    emit(out, true);
                }
            }
        });
        if !cur_logo.is_empty() {
            tc_row(ui, "로고 위치", |ui| {
                let mut cur = t["logo"]["position"].as_str().unwrap_or("top-right").to_string();
                if seg(ui, &mut cur, &[("top-left", "↖"), ("top-center", "↑"), ("top-right", "↗"), ("bottom-left", "↙"), ("bottom-center", "↓"), ("bottom-right", "↘")]) {
                    set(t, "logo.position", json!(cur));
                    emit(out, true);
                }
            });
            tc_row(ui, "로고 크기", |ui| {
                if let Some((v, fin)) = slider(ui, t["logo"]["size"].as_f64().unwrap_or(120.0), 40.0, 400.0, 5.0, "px") {
                    set(t, "logo.size", json!(v.round() as i64));
                    emit(out, fin);
                }
            });
        }
        // 전환: 디졸브 · 컷 (+ 디졸브 시간)
        let anim = if matches!(t["animation"]["type"].as_str(), Some("dissolve" | "fade")) { "dissolve" } else { "cut" };
        tc_row(ui, "전환", |ui| {
            let mut cur = anim.to_string();
            if seg(ui, &mut cur, &[("dissolve", "디졸브"), ("cut", "컷")]) {
                set(t, "animation.type", json!(cur));
                emit(out, true);
            }
        });
        if anim == "dissolve" {
            tc_row(ui, "디졸브 시간", |ui| {
                if let Some((v, fin)) = slider(ui, t["animation"]["duration"].as_f64().unwrap_or(400.0), 100.0, 2000.0, 50.0, "ms") {
                    set(t, "animation.duration", json!(v.round() as i64));
                    emit(out, fin);
                }
            });
        }
        // PPT 슬라이드: 그림 그대로 / 가사만
        tc_row(ui, "PPT 슬라이드", |ui| {
            let mode = t["pptMode"].as_str().filter(|x| !x.is_empty()).map(String::from).unwrap_or_else(|| if t["background"]["type"] == "transparent" { "lyrics".into() } else { "image".into() });
            let mut cur = mode.clone();
            if seg(ui, &mut cur, &[("image", "그림 그대로"), ("lyrics", "가사만")]) {
                t["pptMode"] = json!(cur);
                emit(out, true);
            }
        });
    };
    if two {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 16.0;
            ui.vertical(|ui| {
                ui.set_width(col_w);
                section1(ui, t, &mut out);
            });
            ui.vertical(|ui| {
                ui.set_width(col_w);
                section2(ui, t, &mut out);
            });
        });
    } else {
        section1(ui, t, &mut out);
        section2(ui, t, &mut out);
    }
    out
}

/// .quick-head 의 '빠른 설정' 이름표 (13px 700)
pub(crate) fn head_label(ui: &mut egui::Ui) {
    ui.label(rich(tr("빠른 설정"), 13.0, Weight::Bold, pal().text));
}

/// 콤보(select) — 고른 값이 바뀌면 true
pub(crate) fn select(ui: &mut egui::Ui, id: &str, cur: &mut String, opts: &[(String, String)], width: f32, tip: &str) -> bool {
    let shown = opts.iter().find(|o| o.0 == *cur).map(|o| o.1.clone()).unwrap_or_default();
    let before = cur.clone();
    egui::ComboBox::from_id_salt(id).selected_text(shown).width(width).height(400.0).show_ui(ui, |ui| {
        for (v, l) in opts {
            ui.selectable_value(cur, v.clone(), l);
        }
    }).response.on_hover_text(tr(tip));
    *cur != before
}

/// 머리 줄 아래 구분선 (.quick-head border-bottom)
pub(crate) fn rule(ui: &mut egui::Ui) {
    let r = ui.max_rect();
    let y = ui.cursor().top();
    ui.painter().hline(r.x_range(), y, Stroke::new(1.0, pal().line));
}

