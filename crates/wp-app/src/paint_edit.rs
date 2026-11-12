//! [트랙 A] 슬라이드 편집(edit.items)의 글상자·그림·도형을 조작 화면 미리보기에 그린다.
//! PPT 요소 따로 꺼내기(pptBg)를 한 슬라이드는 배경 그림이 요소를 뺀 그림이라, 이것을 안 그리면 빈 화면처럼 보인다.
//! (웹 출력은 render.js editItemsHtml 이 그린다. 좌표는 화면 %, 글자 크기는 1080 높이 기준 px)
//! 트랙 C 의 슬라이드 편집기가 같은 모양을 쓰면 이 그리기를 그대로 써도 된다.

use crate::paint::{self, parse_color, Ctx};
use eframe::egui::{self, text::LayoutJob, Align, Color32, FontFamily, FontId, Pos2, Rect, TextFormat, Vec2};
use parking_lot::Mutex;
use serde_json::Value;
use std::collections::HashMap;

/// 글꼴 이름 → 등록 상태 (None = 찾을 수 없음)
static FONTS: Mutex<Option<HashMap<String, Option<FontFamily>>>> = Mutex::new(None);

fn num(v: &Value, d: f32) -> f32 {
    v.as_f64().map(|x| x as f32).unwrap_or(d)
}

/// PPT 글꼴을 egui 에 등록한다 (처음 본 이름은 이번 프레임은 기본 글꼴, 다음 프레임부터 그 글꼴)
fn family_for(ctx: &egui::Context, name: &str, bold: bool) -> FontFamily {
    let fallback = if bold { FontFamily::Name("bold".into()) } else { FontFamily::Proportional };
    let name = name.trim();
    if name.is_empty() {
        return fallback;
    }
    let mut g = FONTS.lock();
    let map = g.get_or_insert_with(HashMap::new);
    match map.get(name) {
        Some(Some(fam)) => {
            // 정말 등록됐는지 (등록은 다음 프레임에 반영된다)
            if ctx.fonts(|f| f.definitions().families.contains_key(fam)) { fam.clone() } else { fallback }
        }
        Some(None) => fallback,
        None => {
            let found = wp_core::pptx::build::font_bytes(name);
            match found {
                Some((bytes, index)) => {
                    let fam = FontFamily::Name(format!("ppt:{name}").into());
                    let mut data = egui::FontData::from_owned(bytes);
                    data.index = index;
                    ctx.add_font(egui::epaint::text::FontInsert::new(
                        &format!("ppt:{name}"),
                        data,
                        vec![egui::epaint::text::InsertFontFamily { family: fam.clone(), priority: egui::epaint::text::FontPriority::Highest }],
                    ));
                    map.insert(name.to_string(), Some(fam));
                }
                None => {
                    map.insert(name.to_string(), None);
                }
            }
            fallback
        }
    }
}

/// edit.items 중 below 가 맞는 것만 rect(화면 전체)에 그린다
pub fn paint_items(ui: &egui::Ui, c: &Ctx, rect: Rect, items: &[Value], below: bool) {
    let p = ui.painter_at(rect);
    let k = rect.height() / 1080.0;
    for x in items.iter().filter(|x| x["below"].as_bool().unwrap_or(false) == below) {
        let r = Rect::from_min_size(
            rect.min + Vec2::new(num(&x["x"], 0.0) * rect.width() / 100.0, num(&x["y"], 0.0) * rect.height() / 100.0),
            Vec2::new(num(&x["w"], 10.0) * rect.width() / 100.0, num(&x["h"], 10.0) * rect.height() / 100.0),
        );
        let op = num(&x["opacity"], 1.0).clamp(0.0, 1.0);
        let radius = if x["ellipse"].as_bool().unwrap_or(false) { r.width().min(r.height()) / 2.0 } else { num(&x["radius"], 0.0) * k };
        match x["type"].as_str().unwrap_or("text") {
            "shape" => {
                let col = x["fill"].as_str().and_then(parse_color).unwrap_or(Color32::from_rgb(0x1d, 0x35, 0x57)).gamma_multiply(op);
                if x["ellipse"].as_bool().unwrap_or(false) {
                    p.add(egui::Shape::ellipse_filled(r.center(), r.size() / 2.0, col));
                } else {
                    p.rect_filled(r, radius, col);
                }
            }
            "image" => {
                if let Some(src) = x["src"].as_str().filter(|s| !s.is_empty()) {
                    if let Some(uri) = paint::image_uri(c, src) {
                        let image = egui::Image::new(uri);
                        if let Ok(poll) = image.load_for_size(ui.ctx(), r.size()) {
                            if let Some(size) = poll.size() {
                                let crop = &x["crop"];
                                let l = num(&crop["l"], 0.0).clamp(0.0,0.95);
                                let t = num(&crop["t"], 0.0).clamp(0.0,0.95);
                                let right = num(&crop["r"], 0.0).clamp(0.0,0.95);
                                let bottom = num(&crop["b"], 0.0).clamp(0.0,0.95);
                                let uv = if l+right < 0.96 && t+bottom < 0.96 { Rect::from_min_max(Pos2::new(l,t),Pos2::new(1.0-right,1.0-bottom)) } else { Rect::from_min_max(Pos2::ZERO,Pos2::new(1.0,1.0)) };
                                let cropped = size * uv.size();
                                let cover = x["fit"].as_str() != Some("contain");
                                let scale = if cover { (r.width()/cropped.x).max(r.height()/cropped.y) } else { (r.width()/cropped.x).min(r.height()/cropped.y) };
                                let full = Rect::from_center_size(r.center(),cropped*scale);
                                let dest = full.intersect(r);
                                let a = (dest.min-full.min)/full.size();
                                let b = (dest.max-full.min)/full.size();
                                let visible = Rect::from_min_max(uv.min+a*uv.size(),uv.min+b*uv.size());
                                image.uv(visible).tint(Color32::WHITE.gamma_multiply(op)).paint_at(ui,dest);
                            }
                        }
                    }
                }
            }
            "camera" => {
                p.rect_filled(r, radius, Color32::BLACK);
            }
            _ => paint_text(ui, &p, r, x, k, op),
        }
    }
}

fn paint_text(ui: &egui::Ui, p: &egui::Painter, r: Rect, x: &Value, k: f32, op: f32) {
    let text = x["text"].as_str().unwrap_or("");
    if let Some(fill) = x["fill"].as_str().and_then(parse_color) {
        p.rect_filled(r, num(&x["radius"], 0.0) * k, fill.gamma_multiply(op));
    }
    if text.is_empty() {
        return;
    }
    let pad = num(&x["padding"], 12.0) * k;
    let inner = r.shrink(pad.min(r.width() / 3.0).min(r.height() / 3.0));
    let bold = x["bold"].as_bool().unwrap_or(false) || x["font"].as_str().is_some_and(|f| f.to_lowercase().ends_with("bold"));
    let fam = family_for(ui.ctx(), x["font"].as_str().unwrap_or(""), bold);
    let color = x["color"].as_str().and_then(parse_color).unwrap_or(Color32::WHITE).gamma_multiply(op);
    let lh = num(&x["lineHeight"], 1.3);
    let halign = match x["align"].as_str().unwrap_or("center") {
        "left" => Align::LEFT,
        "right" => Align::RIGHT,
        _ => Align::Center,
    };
    let mk = |size: f32| {
        let mut job = LayoutJob::default();
        job.wrap.max_width = inner.width().max(4.0);
        job.halign = halign;
        job.append(text, 0.0, TextFormat { font_id: FontId::new(size.max(1.0), fam.clone()), color, line_height: Some(size * lh), italics: x["italic"].as_bool().unwrap_or(false), ..Default::default() });
        p.layout_job(job)
    };
    let mut size = num(&x["size"], 60.0) * k;
    let mut g = mk(size);
    // fit: 상자에 들어갈 때까지 글자를 줄인다 (render.js fitItems 와 같은 비율, 키우지는 않음)
    if x["fit"].as_bool().unwrap_or(false) {
        let min = size * 0.35;
        let mut n = 0;
        while n < 40 && size > min && (g.size().y > inner.height() + 1.0 || g.size().x > inner.width() + 1.0) {
            size = (size * 0.93).max(min);
            g = mk(size);
            n += 1;
        }
    }
    let y = match x["valign"].as_str().unwrap_or("middle") {
        "top" => inner.top(),
        "bottom" => inner.bottom() - g.size().y,
        _ => inner.center().y - g.size().y / 2.0,
    };
    let gx = match halign {
        Align::LEFT => inner.left(),
        Align::RIGHT => inner.right(),
        _ => inner.center().x,
    };
    p.with_clip_rect(r).galley(Pos2::new(gx, y), g, color);
}
