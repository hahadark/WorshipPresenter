//! 템플릿 편집기 오른쪽 '속성' 칸에서 쓰는 시각적 입력 부품 (docs/ux-rules.md 3절).
//! 숫자 = 막대 + 숫자칸(범위 제한), 색 = 견본 동그라미 + 고르기, 정렬 = 아이콘 분할 단추, 위치 = 3×3 격자, 켜고 끄기 = 스위치.
//! 값은 템플릿 JSON 을 경로("font.size")로 바로 고친다. 바뀌면 true.
//! 색은 모두 theme::pal() 에서 가져온다.

use crate::theme::{self, pal, Weight};
use crate::widgets::{self, SegStyle};
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, Vec2};
use serde_json::{json, Value};
use wp_core::i18n::tr;

/// 이름표 칸 너비
pub const LABEL_W: f32 = 78.0;

pub fn get<'a>(t: &'a Value, path: &str) -> &'a Value {
    path.split('.').fold(t, |v, k| &v[k])
}

pub fn set(t: &mut Value, path: &str, v: Value) {
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

pub fn f(t: &Value, path: &str, d: f64) -> f64 {
    get(t, path).as_f64().unwrap_or(d)
}

pub fn s<'a>(t: &'a Value, path: &str) -> &'a str {
    get(t, path).as_str().unwrap_or("")
}

pub fn b(t: &Value, path: &str) -> bool {
    get(t, path).as_bool().unwrap_or(false)
}

/// 이름표 + 내용 한 줄 (이름표는 흐린 12px, 툴팁이 있으면 이름표에 붙인다)
pub fn row<R>(ui: &mut egui::Ui, label: &str, tip: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.set_min_height(28.0);
        let (r, resp) = ui.allocate_exact_size(Vec2::new(LABEL_W, 28.0), Sense::hover());
        let g = ui.painter().layout(tr(label), theme::font(12.5, Weight::Regular), pal().muted, LABEL_W - 4.0);
        ui.painter().galley(Pos2::new(r.left(), r.center().y - g.size().y / 2.0), g, pal().muted);
        if !tip.is_empty() {
            resp.on_hover_text(tr(tip));
        }
        add(ui)
    })
    .inner
}

/// 작은 설명 글 (흐림)
pub fn note(ui: &mut egui::Ui, text: &str) {
    ui.label(theme::rich(tr(text), 11.5, Weight::Regular, pal().muted2));
}

/// 막대 + 숫자칸. shown = 숫자칸에 보일 배수(예: 불투명 0~1 → ×100 해서 %). 바뀌면 true
#[allow(clippy::too_many_arguments)]
pub fn slider(ui: &mut egui::Ui, t: &mut Value, path: &str, label: &str, tip: &str, lo: f64, hi: f64, step: f64, def: f64, scale: f64, unit: &str) -> bool {
    let cur = f(t, path, def);
    let mut ch = false;
    row(ui, label, tip, |ui| {
        let mut v = cur * scale;
        let (slo, shi, sstep) = (lo * scale, hi * scale, step * scale);
        ui.spacing_mut().slider_width = (ui.available_width() - 72.0).max(60.0);
        let r = ui.add(egui::Slider::new(&mut v, slo..=shi).step_by(sstep).show_value(false));
        let dec = if sstep < 0.1 { 2 } else if sstep < 1.0 { 1 } else { 0 };
        let r2 = ui.add(egui::DragValue::new(&mut v).range(slo..=shi).speed(sstep.max(0.01)).fixed_decimals(dec).suffix(unit));
        if (r.changed() || r2.changed()) && (v / scale - cur).abs() > f64::EPSILON {
            let nv = (v / scale).clamp(lo, hi);
            let nv = if step >= 1.0 { json!(nv.round() as i64) } else { json!((nv * 1000.0).round() / 1000.0) };
            set(t, path, nv);
            ch = true;
        }
    });
    ch
}

/// 켜고 끄기 스위치 (이름표 왼쪽, 스위치 오른쪽)
pub fn switch(ui: &mut egui::Ui, t: &mut Value, path: &str, label: &str, tip: &str) -> bool {
    let mut on = b(t, path);
    let ch = row(ui, label, tip, |ui| switch_widget(ui, &mut on).changed());
    if ch {
        set(t, path, json!(on));
    }
    ch
}

/// 스위치 그림 (36×20, 켜지면 accent)
pub fn switch_widget(ui: &mut egui::Ui, on: &mut bool) -> Response {
    let p = pal();
    let (r, mut resp) = ui.allocate_exact_size(Vec2::new(36.0, 20.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let k = ui.ctx().animate_bool(resp.id, *on);
    let bg = if *on { p.accent } else if resp.hovered() { p.line2 } else { p.line };
    ui.painter().rect_filled(r, CornerRadius::same(10), bg);
    let x = egui::lerp(r.left() + 10.0..=r.right() - 10.0, k);
    ui.painter().circle_filled(Pos2::new(x, r.center().y), 7.0, p.text);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.on_hover_text(tr(if *on { "켜짐 (누르면 끔)" } else { "꺼짐 (누르면 켬)" }))
}

/// 분할 단추로 고르기 (값, 보일 이름, 툴팁)
pub fn seg(ui: &mut egui::Ui, t: &mut Value, path: &str, label: &str, items: &[(&str, &str)], def: &str) -> bool {
    let cur0 = get(t, path).as_str().map(String::from).or_else(|| get(t, path).as_i64().map(|x| x.to_string())).unwrap_or_else(|| def.to_string());
    let mut cur = cur0.clone();
    let ch = row(ui, label, "", |ui| seg_widget(ui, &mut cur, items));
    if ch && cur != cur0 {
        let v = if get(t, path).is_number() { cur.parse::<i64>().map(|n| json!(n)).unwrap_or(json!(cur)) } else { json!(cur) };
        set(t, path, v);
        return true;
    }
    false
}

pub fn seg_widget(ui: &mut egui::Ui, cur: &mut String, items: &[(&str, &str)]) -> bool {
    let tr_items: Vec<(String, String)> = items.iter().map(|(v, l)| (v.to_string(), tr(l))).collect();
    let refs: Vec<(String, &str)> = tr_items.iter().map(|(v, l)| (v.clone(), l.as_str())).collect();
    widgets::segmented_with(ui, cur, &refs, SegStyle { font: 12.5, pad: Vec2::new(8.0, 4.0), fill: false, radius: 6 }).changed()
}

/// 색 견본 동그라미 (24px). transparent 는 체크무늬
pub fn swatch(ui: &mut egui::Ui, c: Color32, on: bool, tip: &str) -> Response {
    let p = pal();
    let (r, resp) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::click());
    let ctr = r.center();
    let pt = ui.painter();
    if c.a() < 255 {
        for y in 0..5 {
            for x in 0..5 {
                let rr = Rect::from_min_size(Pos2::new(r.left() + x as f32 * 5.0, r.top() + y as f32 * 5.0), Vec2::splat(5.0));
                if rr.center().distance(ctr) < 11.0 {
                    pt.rect_filled(rr, CornerRadius::ZERO, if (x + y) % 2 == 0 { p.muted2 } else { p.sunken });
                }
            }
        }
    }
    pt.circle_filled(ctr, 11.0, c);
    pt.circle_stroke(ctr, 11.0, Stroke::new(1.5, if on { p.accent } else if resp.hovered() { p.muted } else { p.line2 }));
    if on {
        pt.circle_stroke(ctr, 14.5, Stroke::new(2.0, p.accent));
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if tip.is_empty() { resp } else { resp.on_hover_text(tip) }
}

fn rainbow(ui: &mut egui::Ui, open: bool) -> Response {
    let p = pal();
    let (r, resp) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::click());
    let ctr = r.center();
    let n = 24;
    for k in 0..n {
        let a0 = k as f32 / n as f32 * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
        let a1 = (k + 1) as f32 / n as f32 * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
        let col: Color32 = egui::ecolor::Hsva::new(k as f32 / n as f32, 0.85, 1.0, 1.0).into();
        ui.painter().add(Shape::convex_polygon(vec![ctr, ctr + Vec2::angled(a0) * 11.0, ctr + Vec2::angled(a1) * 11.0], col, Stroke::NONE));
    }
    ui.painter().circle_stroke(ctr, 11.0, Stroke::new(1.5, if open { p.accent } else { p.line2 }));
    ui.painter().text(ctr, egui::Align2::CENTER_CENTER, "+", theme::font(14.0, Weight::Bold), Color32::WHITE);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.on_hover_text(tr("다른 색 고르기"))
}

/// 색 문자열 → (#rrggbb 소문자, 불투명도 0~1)
fn norm(c: &str) -> Option<(String, f32)> {
    let col = crate::paint::parse_color(c)?;
    let a = if c.trim_start().starts_with("rgba") { col.a() as f32 / 255.0 } else { 1.0 };
    Some((format!("#{:02x}{:02x}{:02x}", col.r(), col.g(), col.b()), a))
}

/// 색 고르기 줄: 미리 정한 견본 + 무지개(+)를 누르면 그 자리에서 고르기가 펼쳐진다.
/// with_alpha 면 rgba(…) 로 저장(그림자 색처럼 반투명이 필요한 것)
pub fn color(ui: &mut egui::Ui, t: &mut Value, path: &str, label: &str, presets: &[&str], with_alpha: bool) -> bool {
    let cur_s = s(t, path).to_string();
    let cur = norm(&cur_s);
    let mut out: Option<String> = None;
    let open_id = ui.scope_id().with(("tc-color", path));
    let mut open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);
    row(ui, label, "", |ui| {
        ui.spacing_mut().item_spacing.x = 5.0;
        let mut matched = false;
        for c in presets {
            let on = cur.as_ref().is_some_and(|(h, _)| h.eq_ignore_ascii_case(c));
            matched |= on;
            if swatch(ui, crate::paint::parse_color(c).unwrap_or(Color32::WHITE), on, c).clicked() {
                out = Some(if with_alpha { rgba_of(c, cur.as_ref().map(|x| x.1).unwrap_or(1.0)) } else { c.to_string() });
            }
        }
        if !matched {
            // 견본에 없는 지금 색도 동그라미로 보인다
            if let Some(col) = crate::paint::parse_color(&cur_s) {
                swatch(ui, col, true, &tr("지금 색"));
            }
        }
        if rainbow(ui, open).clicked() {
            open = !open;
        }
    });
    if open {
        ui.horizontal(|ui| {
            ui.add_space(LABEL_W + 6.0);
            let mut col = crate::paint::parse_color(&cur_s).unwrap_or(Color32::WHITE);
            let alpha = if with_alpha { egui::color_picker::Alpha::OnlyBlend } else { egui::color_picker::Alpha::Opaque };
            if egui::color_picker::color_picker_color32(ui, &mut col, alpha) {
                out = Some(if with_alpha {
                    let [r, g, b, a] = col.to_srgba_unmultiplied();
                    format!("rgba({},{},{},{:.2})", r, g, b, a as f32 / 255.0)
                } else {
                    format!("#{:02x}{:02x}{:02x}", col.r(), col.g(), col.b())
                });
            }
        });
    }
    ui.data_mut(|d| d.insert_temp(open_id, open));
    if let Some(c) = out {
        if c != cur_s {
            set(t, path, json!(c));
            return true;
        }
    }
    false
}

fn rgba_of(hex: &str, a: f32) -> String {
    let c = crate::paint::parse_color(hex).unwrap_or(Color32::BLACK);
    format!("rgba({},{},{},{:.2})", c.r(), c.g(), c.b(), a)
}

/// 정렬 아이콘 (줄 세 개를 왼쪽·가운데·오른쪽으로, 내어쓰기는 번호 + 들여 쓴 줄)
fn paint_align_icon(ui: &egui::Ui, r: Rect, kind: &str, col: Color32) {
    let pt = ui.painter();
    let ws = [16.0, 10.0, 14.0];
    for (i, w) in ws.iter().enumerate() {
        let y = r.top() + 4.0 + i as f32 * 5.0;
        let (x0, w) = match kind {
            "left" => (r.left() + 2.0, *w),
            "right" => (r.right() - 2.0 - w, *w),
            "hanging" => (r.left() + 6.0, if i == 0 { 12.0 } else { *w - 2.0 }),
            _ => (r.center().x - w / 2.0, *w),
        };
        pt.rect_filled(Rect::from_min_size(Pos2::new(x0, y), Vec2::new(w, 2.0)), CornerRadius::same(1), col);
    }
    if kind == "hanging" {
        pt.rect_filled(Rect::from_min_size(Pos2::new(r.left() + 1.0, r.top() + 3.5), Vec2::new(3.0, 3.0)), CornerRadius::same(1), col);
    }
}

/// 정렬 아이콘 분할 단추. kinds = ["left","center","right"(,"hanging")]
pub fn align_seg(ui: &mut egui::Ui, salt: &str, cur: &str, kinds: &[&str]) -> Option<String> {
    let p = pal();
    let (w, h) = (34.0, 26.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w * kinds.len() as f32 - (kinds.len() - 1) as f32, h), Sense::hover());
    let mut out = None;
    for (i, k) in kinds.iter().enumerate() {
        let r = Rect::from_min_size(Pos2::new(rect.left() + i as f32 * (w - 1.0), rect.top()), Vec2::new(w, h));
        let resp = ui.interact(r, ui.scope_id().with(("align-seg", salt, i)), Sense::click());
        let on = *k == cur;
        let corner = match (i == 0, i + 1 == kinds.len()) {
            (true, _) => CornerRadius { nw: 6, sw: 6, ne: 0, se: 0 },
            (_, true) => CornerRadius { nw: 0, sw: 0, ne: 6, se: 6 },
            _ => CornerRadius::ZERO,
        };
        let (bg, border) = if on { (p.sel, p.accent) } else if resp.hovered() { (p.hover, p.line2) } else { (p.panel2, p.line) };
        ui.painter().rect(r, corner, bg, Stroke::new(1.0, border), StrokeKind::Inside);
        paint_align_icon(ui, Rect::from_center_size(r.center(), Vec2::new(18.0, 16.0)), k, if on { p.text } else { p.text2 });
        let tip = match *k {
            "left" => "왼쪽 맞춤",
            "right" => "오른쪽 맞춤",
            "hanging" => "왼쪽 + 내어쓰기 (절 번호를 왼쪽 칸에 두고 둘째 줄부터 본문 시작에 맞춤)",
            _ => "가운데 맞춤",
        };
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.on_hover_text(tr(tip)).clicked() && !on {
            out = Some(k.to_string());
        }
    }
    out
}

/// 위치 격자 (rows × cols). 칸마다 (값, 툴팁). 누른 칸의 값
pub fn pos_grid(ui: &mut egui::Ui, salt: &str, cells: &[&[(&str, &str)]], cur: &str) -> Option<String> {
    let p = pal();
    let (cw, chh, gap, pad) = (30.0, 20.0, 3.0, 5.0);
    let cols = cells.first().map(|r| r.len()).unwrap_or(0) as f32;
    let rows = cells.len() as f32;
    let size = Vec2::new(cw * cols + gap * (cols - 1.0) + pad * 2.0, chh * rows + gap * (rows - 1.0) + pad * 2.0);
    let (r, _) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect(r, CornerRadius::same(6), p.sunken, Stroke::new(1.0, p.line), StrokeKind::Inside);
    let mut out = None;
    for (y, line) in cells.iter().enumerate() {
        for (x, (v, tip)) in line.iter().enumerate() {
            let cr = Rect::from_min_size(Pos2::new(r.left() + pad + x as f32 * (cw + gap), r.top() + pad + y as f32 * (chh + gap)), Vec2::new(cw, chh));
            let resp = ui.interact(cr, ui.scope_id().with((salt, y, x)), Sense::click());
            let on = *v == cur;
            let (bg, border) = if on { (p.accent, p.accent) } else if resp.hovered() { (p.hover, p.line2) } else { (p.panel2, p.line) };
            ui.painter().rect(cr, CornerRadius::same(3), bg, Stroke::new(1.0, border), StrokeKind::Inside);
            // 칸 안 작은 막대: 그 칸에 글이 놓이는 모양
            let bar_x = match x {
                0 => cr.left() + 4.0,
                _ if x as f32 == cols - 1.0 => cr.right() - 14.0,
                _ => cr.center().x - 5.0,
            };
            ui.painter().rect_filled(Rect::from_min_size(Pos2::new(bar_x, cr.center().y - 1.0), Vec2::new(10.0, 2.0)), CornerRadius::same(1), if on { Color32::WHITE } else { p.muted2 });
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.on_hover_text(tr(tip)).clicked() {
                out = Some(v.to_string());
            }
        }
    }
    out
}

/// 묶음 머리 (접기). force = Some(true/false) 면 이번에 그렇게 펼치거나 접는다
/// (머리 줄 Response, 펼쳐 있으면 내용이 돌려준 값)
pub fn section<R>(ui: &mut egui::Ui, id: &str, icon: &str, title: &str, force: Option<bool>, add: impl FnOnce(&mut egui::Ui) -> R) -> (Response, Option<R>) {
    let p = pal();
    let sid = ui.make_persistent_id(("tpl-sec", id));
    let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), sid, id == "bg" || id == "font");
    if let Some(o) = force {
        state.set_open(o);
    }
    // 머리 줄: 가로 꽉 찬 단추 (▸/▾ · 묶음 아이콘 · 제목), 마우스를 올리면 hover 바탕
    let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::click());
    if resp.clicked() {
        state.toggle(ui);
    }
    let open = state.is_open();
    let pt = ui.painter();
    if resp.hovered() {
        pt.rect_filled(r, CornerRadius::same(6), p.hover);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    crate::icons::paint(ui, Rect::from_center_size(Pos2::new(r.left() + 10.0, r.center().y), Vec2::splat(14.0)), if open { "chevron-down" } else { "chevron-right" }, p.muted);
    crate::icons::paint(ui, Rect::from_center_size(Pos2::new(r.left() + 30.0, r.center().y), Vec2::splat(16.0)), icon, if open { p.accent } else { p.muted });
    pt.text(Pos2::new(r.left() + 44.0, r.center().y), egui::Align2::LEFT_CENTER, tr(title), theme::font(14.0, Weight::SemiBold), p.text);
    pt.hline(r.x_range(), r.bottom(), Stroke::new(1.0, p.line_soft));
    let resp = resp.on_hover_text(tr(if open { "누르면 접기" } else { "누르면 펼치기" }));
    let body = state.show_body_unindented(ui, |ui| {
        ui.add_space(4.0);
        let r = add(ui);
        ui.add_space(8.0);
        r
    });
    (resp, body.map(|b| b.inner))
}
