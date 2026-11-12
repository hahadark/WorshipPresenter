//! 조작 화면의 미리보기·LIVE 그림: 슬라이드를 템플릿 모양대로 egui 로 직접 그린다.
//! (예배당·OBS 화면은 웹 출력이 그리고, 여기는 운영자가 보는 작은 화면)

use eframe::egui::{self, text::LayoutJob, Align, Align2, Color32, FontFamily, FontId, Pos2, Rect, Stroke, TextFormat, Vec2};
use serde_json::Value;
use std::path::{Path, PathBuf};
use wp_core::measure;

pub struct Ctx<'a> {
    pub web_dir: &'a Path,
    pub data_dir: &'a Path,
}

fn num(v: &Value, d: f32) -> f32 {
    v.as_f64().map(|x| x as f32).unwrap_or(d)
}

/// "#rrggbb" · "#rgb" · "rgba(r,g,b,a)" · "rgb(…)" → 색. 그라데이션은 첫 색
pub fn parse_color(s: &str) -> Option<Color32> {
    let s = s.trim();
    if let Some(h) = s.strip_prefix('#') {
        let h: String = h.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
        let p = |i: usize, n: usize| u8::from_str_radix(&h[i..i + n], 16).ok();
        return match h.len() {
            3 => Some(Color32::from_rgb(p(0, 1)? * 17, p(1, 1)? * 17, p(2, 1)? * 17)),
            6 => Some(Color32::from_rgb(p(0, 2)?, p(2, 2)?, p(4, 2)?)),
            8 => Some(Color32::from_rgba_unmultiplied(p(0, 2)?, p(2, 2)?, p(4, 2)?, p(6, 2)?)),
            _ => None,
        };
    }
    if s.starts_with("rgb") {
        let inner = s.split_once('(')?.1.split(')').next()?;
        let v: Vec<f32> = inner.split(',').filter_map(|x| x.trim().parse().ok()).collect();
        if v.len() >= 3 {
            let a = v.get(3).copied().unwrap_or(1.0);
            return Some(Color32::from_rgba_unmultiplied(v[0] as u8, v[1] as u8, v[2] as u8, (a * 255.0) as u8));
        }
    }
    if s.contains("gradient") {
        // 첫 색만
        let i = s.find('#').or_else(|| s.find("rgb"))?;
        return parse_color(&s[i..]);
    }
    None
}

fn color(v: &Value, d: Color32) -> Color32 {
    v.as_str().and_then(parse_color).unwrap_or(d)
}

/// 출력 화면 주소(/lib/file?p=… · /media/… · /shared/…) → egui 가 읽을 file:// 주소
pub fn image_uri(c: &Ctx, url: &str) -> Option<String> {
    let path: PathBuf = if let Some(q) = url.strip_prefix("/lib/file?p=") {
        PathBuf::from(percent_encoding::percent_decode_str(q).decode_utf8().ok()?.to_string())
    } else if let Some(f) = url.strip_prefix("/media/") {
        c.data_dir.join("media").join(f)
    } else if let Some(r) = url.strip_prefix("/hymn/") {
        // [트랙 A] 찬송가 캐시: 우리 것 → Electron 판
        let (key, file) = r.split_once('/')?;
        let mut dirs = vec![c.data_dir.join("hymn-cache")];
        if let Some(e) = wp_core::hymns::electron_data() { dirs.push(e.join("hymn-cache")); }
        dirs.into_iter().map(|d| d.join(key).join(file)).find(|p| p.is_file())?
    } else if let Some(f) = url.strip_prefix("/shared/") {
        c.web_dir.join("shared").join(f)
    } else if url.len() > 2 && url.as_bytes()[1] == b':' {
        PathBuf::from(url)
    } else {
        return None;
    };
    if !path.is_file() {
        return None;
    }
    Some(format!("file:///{}", path.to_string_lossy().replace('\\', "/").trim_start_matches('/')))
}

/// [트랙 A] 다른 모듈에서 쓰는 그림 그리기
pub fn draw_image_pub(ui: &egui::Ui, c: &Ctx, rect: Rect, url: &str, cover: bool) -> bool {
    draw_image(ui, c, rect, url, cover)
}

fn draw_image(ui: &egui::Ui, c: &Ctx, rect: Rect, url: &str, cover: bool) -> bool {
    let Some(uri) = image_uri(c, url) else { return false };
    let img = egui::Image::new(uri);
    let img = if cover { img.fit_to_exact_size(rect.size()) } else { img.fit_to_exact_size(rect.size()).maintain_aspect_ratio(true) };
    // 그림 비율 맞추기: 모양을 알면 가운데에 맞춰 넣는다
    match img.load_for_size(ui.ctx(), rect.size()) {
        Ok(poll) => {
            if let Some(sz) = poll.size() {
                let s = if cover { (rect.width() / sz.x).max(rect.height() / sz.y) } else { (rect.width() / sz.x).min(rect.height() / sz.y) };
                let r = Rect::from_center_size(rect.center(), sz * s).intersect(rect);
                let uv = if cover {
                    let full = Rect::from_center_size(rect.center(), sz * s);
                    Rect::from_min_max(
                        Pos2::new((r.min.x - full.min.x) / full.width(), (r.min.y - full.min.y) / full.height()),
                        Pos2::new((r.max.x - full.min.x) / full.width(), (r.max.y - full.min.y) / full.height()),
                    )
                } else {
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0))
                };
                egui::Image::new(image_uri(c, url).unwrap()).uv(uv).paint_at(ui, r);
            } else {
                img.paint_at(ui, rect);
            }
            true
        }
        Err(e) => {
            log::warn!("그림을 못 읽음 {url}: {e}");
            #[cfg(debug_assertions)]
            eprintln!("그림을 못 읽음 {url}: {e}");
            false
        }
    }
}

/// CSS 그라데이션 근사: radial-gradient 는 가운데가 밝은 타원 빛, 'to top, #색 0 14px' 는 아래 띠
fn paint_gradient(p: &egui::Painter, rect: Rect, g: &str, th: f32) {
    let hex = regex_lite_hex(g);
    if let Some(i) = g.find("radial-gradient") {
        let part = &g[i..];
        let cols = regex_lite_hex(part);
        if let (Some(c1), Some(c2)) = (cols.first(), cols.get(1)) {
            let center = Pos2::new(rect.center().x, rect.top() + rect.height() * 0.35);
            let n = 18;
            for k in (0..n).rev() {
                let f = (k as f32 + 1.0) / n as f32;
                let col = lerp(*c1, *c2, f);
                p.add(egui::Shape::ellipse_filled(center, Vec2::new(rect.width() * 0.72 * f, rect.height() * 0.95 * f), col));
            }
        }
    } else if g.contains("linear-gradient") && hex.len() >= 2 && !g.contains("to top,") {
        let n = 24;
        for k in 0..n {
            let f0 = k as f32 / n as f32;
            let r = Rect::from_min_max(Pos2::new(rect.left(), rect.top() + rect.height() * f0), Pos2::new(rect.right(), rect.top() + rect.height() * (f0 + 1.0 / n as f32) + 1.0));
            p.rect_filled(r, 0.0, lerp(hex[0], hex[hex.len() - 1], f0));
        }
    }
    // 아래 띠: linear-gradient(to top, #D8102F 0 14px, transparent 14px)
    if let Some(i) = g.find("to top,") {
        let rest = &g[i + 7..];
        if let (Some(col), Some(px)) = (regex_lite_hex(rest).first().copied(), rest.split("px").next().and_then(|s| s.split_whitespace().last()).and_then(|x| x.parse::<f32>().ok())) {
            let hh = px * rect.height() / th;
            p.rect_filled(Rect::from_min_max(Pos2::new(rect.left(), rect.bottom() - hh), rect.max), 0.0, col);
        }
    }
}

fn regex_lite_hex(s: &str) -> Vec<Color32> {
    let mut out = vec![];
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'#' {
            let j = (i + 1..b.len()).take_while(|&k| (b[k] as char).is_ascii_hexdigit()).last().map(|k| k + 1).unwrap_or(i + 1);
            if let Some(c) = parse_color(&s[i..j]) { out.push(c); }
            i = j;
        } else {
            i += 1;
        }
    }
    out
}

fn lerp(a: Color32, b: Color32, f: f32) -> Color32 {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * f) as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

fn checker(p: &egui::Painter, rect: Rect) {
    p.rect_filled(rect, 0.0, Color32::from_rgb(0x1d, 0x21, 0x29));
    let s = (rect.width() / 24.0).max(6.0);
    let mut y = rect.top();
    let mut row = 0;
    while y < rect.bottom() {
        let mut x = rect.left() + if row % 2 == 0 { 0.0 } else { s };
        while x < rect.right() {
            p.rect_filled(Rect::from_min_size(Pos2::new(x, y), Vec2::splat(s)).intersect(rect), 0.0, Color32::from_rgb(0x2a, 0x2f, 0x38));
            x += 2.0 * s;
        }
        y += s;
        row += 1;
    }
}

fn family(weight: f32) -> FontFamily {
    if weight >= 600.0 { FontFamily::Name("bold".into()) } else { FontFamily::Proportional }
}

/// 슬라이드 하나를 rect 에 그린다. visible=false 면 글자는 숨기고 배경만 (LIVE 숨김 상태)
pub fn paint_slide(ui: &egui::Ui, c: &Ctx, rect: Rect, slide: Option<&Value>, t: &Value, version_name: &str, visible: bool, logo: bool, hide_bg: bool) {
    let p = ui.painter_at(rect);
    let tw = num(&t["width"], 1920.0);
    let th = num(&t["height"], 1080.0);
    let sc = rect.width() / tw;
    // 배경
    let bg = &t["background"];
    let bgt = bg["type"].as_str().unwrap_or("color");
    if hide_bg {
        p.rect_filled(rect, 0.0, Color32::BLACK);
    } else if bgt == "transparent" {
        checker(&p, rect);
    } else {
        p.rect_filled(rect, 0.0, color(&bg["color"], Color32::BLACK));
        if bgt == "gradient" {
            paint_gradient(&p, rect, bg["gradient"].as_str().unwrap_or(""), num(&t["height"], 1080.0));
        }
        if bgt == "image" {
            if let Some(u) = bg["image"].as_str().filter(|x| !x.is_empty()) {
                draw_image(ui, c, rect, u, bg["imageFit"].as_str() != Some("contain"));
            }
        }
        let dim = num(&bg["dim"], 0.0);
        if dim > 0.0 {
            p.rect_filled(rect, 0.0, Color32::from_black_alpha((dim.min(1.0) * 255.0) as u8));
        }
    }
    // 템플릿 로고 (배경과 함께 늘 보임)
    let lg = &t["logo"];
    if lg["show"].as_bool().unwrap_or(false) && !hide_bg {
        if let Some(u) = lg["image"].as_str().filter(|x| !x.is_empty()) {
            let sz = num(&lg["size"], 120.0) * sc;
            let m = num(&lg["margin"], 40.0) * sc;
            let pos = lg["position"].as_str().unwrap_or("top-right");
            let x = if pos.ends_with("left") { rect.left() + m } else if pos.ends_with("center") { rect.center().x - sz / 2.0 } else { rect.right() - m - sz };
            let y = if pos.starts_with("bottom") { rect.bottom() - m - sz } else { rect.top() + m };
            draw_image(ui, c, Rect::from_min_size(Pos2::new(x, y), Vec2::splat(sz)), u, false);
        }
    }
    let Some(s) = slide else { return };
    let kind = s["kind"].as_str().unwrap_or("bible");
    // 슬라이드 자체 배경 (순서별 배경 · 찬송가 배경)
    if let Some(sbg) = s["bg"].as_str().filter(|x| !x.is_empty()) {
        if !hide_bg {
            if let Some(col) = parse_color(sbg) {
                p.rect_filled(rect, 0.0, col);
            } else {
                draw_image(ui, c, rect, sbg, true);
            }
        }
    }
    if logo {
        p.rect_filled(rect, 0.0, Color32::BLACK);
        let lr = Rect::from_center_size(rect.center(), Vec2::splat(rect.height() * 0.4));
        if !draw_image(ui, c, lr, "/shared/assets/sa-shield.png", false) {
            p.text(rect.center(), Align2::CENTER_CENTER, "로고", FontId::proportional(rect.height() * 0.1), Color32::WHITE);
        }
        return;
    }
    if !visible {
        return;
    }
    match kind {
        "image" | "score" => {
            if kind == "score" {
                if let Some(col) = s["bg"].as_str().and_then(parse_color) {
                    p.rect_filled(rect, 0.0, col);
                }
            } else {
                p.rect_filled(rect, 0.0, Color32::BLACK);
            }
            let u = s["image"].as_str().unwrap_or("");
            if !draw_image(ui, c, rect, u, false) {
                p.text(rect.center(), Align2::CENTER_CENTER, format!("그림: {}", s["ref"].as_str().unwrap_or("")), FontId::proportional((rect.height() * 0.06).max(10.0)), Color32::GRAY);
            }
        }
        "blank" => {
            if s["mode"] != "background" {
                p.rect_filled(rect, 0.0, Color32::BLACK);
            }
        }
        "video" | "camera" => {
            p.rect_filled(rect, 0.0, Color32::from_rgb(10, 12, 16));
            let label = if kind == "video" { "영상" } else { "카메라" };
            p.text(rect.center(), Align2::CENTER_CENTER, format!("{} · {}", label, s["ref"].as_str().unwrap_or("")), FontId::proportional((rect.height() * 0.06).max(10.0)), Color32::GRAY);
        }
        "title" => paint_title(&p, rect, s, t, sc),
        _ => paint_text(&p, rect, s, t, sc, version_name),
    }
    // [트랙 A] 그림·악보·빈 화면 슬라이드의 편집 요소 (PPT 에서 꺼낸 글상자·그림·도형)
    if matches!(kind, "image" | "score" | "blank" | "video" | "camera") {
        if let Some(items) = s["edit"]["items"].as_array().filter(|a| !a.is_empty()) {
            crate::paint_edit::paint_items(ui, c, rect, items, true);
            crate::paint_edit::paint_items(ui, c, rect, items, false);
        }
    }
    if let Some(cd) = s["countdown"].as_object() {
        let txt = format!("{} {}", cd.get("label").and_then(|x| x.as_str()).unwrap_or(""), cd.get("target").and_then(|x| x.as_str()).unwrap_or(""));
        p.text(rect.center_bottom() - Vec2::new(0.0, rect.height() * 0.08), Align2::CENTER_BOTTOM, txt, FontId::proportional((rect.height() * 0.05).max(9.0)), Color32::WHITE);
    }
    let _ = th;
}

fn content_rect(rect: Rect, t: &Value, sc: f32) -> Rect {
    let tw = num(&t["width"], 1920.0);
    let th = num(&t["height"], 1080.0);
    let safe = num(&t["safeArea"], 0.0);
    let (sx, sy) = (tw * safe / 100.0, th * safe / 100.0);
    let pd = &t["padding"];
    Rect::from_min_max(
        rect.min + Vec2::new((sx + num(&pd["left"], 160.0)) * sc, (sy + num(&pd["top"], 120.0)) * sc),
        rect.min + Vec2::new((tw - sx - num(&pd["right"], 160.0)) * sc, (th - sy - num(&pd["bottom"], 120.0)) * sc),
    )
}

fn halign(a: &str) -> Align {
    match a {
        "left" | "hanging" => Align::LEFT,
        "right" => Align::RIGHT,
        _ => Align::Center,
    }
}

fn place(area: Rect, size: Vec2, t: &Value) -> Rect {
    let x = match halign(t["align"].as_str().unwrap_or("center")) {
        Align::LEFT => area.left(),
        Align::RIGHT => area.right() - size.x,
        _ => area.center().x - size.x / 2.0,
    };
    let y = match t["valign"].as_str().unwrap_or("middle") {
        "top" => area.top(),
        "bottom" => area.bottom() - size.y,
        _ => area.center().y - size.y / 2.0,
    };
    Rect::from_min_size(Pos2::new(x, y), size)
}

fn paint_text(p: &egui::Painter, rect: Rect, s: &Value, t: &Value, sc: f32, version_name: &str) {
    let area = content_rect(rect, t, sc);
    let f = &t["font"];
    let size = measure::fit_size(s, t, version_name) * sc;
    let fam = family(num(&f["weight"], 500.0));
    let fcol = color(&f["color"], Color32::WHITE);
    let lh = num(&f["lineHeight"], 1.5);
    let boxed = t["box"]["enabled"].as_bool().unwrap_or(false);
    let bpad = if boxed { num(&t["box"]["padding"], 32.0) * sc } else { 0.0 };
    let wrap = (area.width() - 2.0 * bpad).max(10.0);
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap;
    job.halign = halign(t["align"].as_str().unwrap_or("center"));
    let fmt = |sz: f32, col: Color32, fam: FontFamily, lh_mul: f32| TextFormat { font_id: FontId::new(sz.max(1.0), fam), color: col, line_height: Some(sz * lh_mul), ..Default::default() };

    let r = &t["reference"];
    let rt = measure::ref_text(s, t);
    let pos = r["position"].as_str().unwrap_or("top");
    let show_ref = r["show"].as_bool().unwrap_or(false) && !rt.is_empty();
    let rs = num(&r["size"], 44.0) * sc;
    let rfmt = fmt(rs, color(&r["color"], Color32::from_rgb(255, 212, 121)), family(num(&r["weight"], 700.0)), 1.25);
    if show_ref && pos == "top" {
        job.append(&rt, 0.0, rfmt.clone());
        job.append("\n", 0.0, fmt(rs * 0.45, fcol, fam.clone(), 1.0));
    }
    let parts = s["parts"].as_array().cloned().unwrap_or_default();
    let body = fmt(size, fcol, fam.clone(), lh);
    if s["kind"] == "text" {
        let txt = parts.iter().map(|x| measure::strip_marks(x["text"].as_str().unwrap_or(""))).collect::<Vec<_>>().join("\n");
        let txt = txt.lines().map(|l| l.trim_start_matches('>').trim_start()).collect::<Vec<_>>().join("\n");
        job.append(&txt, 0.0, body.clone());
    } else {
        let vn = &t["verseNumbers"];
        let show_num = vn["show"].as_bool().unwrap_or(true);
        let vcol = color(&vn["color"], Color32::from_rgb(158, 197, 255));
        let vsc = num(&vn["scale"], 0.6);
        let brk = t["verseBreak"].as_bool().unwrap_or(false) || t["align"] == "hanging";
        let heads = &t["headings"];
        for (i, part) in parts.iter().enumerate() {
            let pi = part["partIndex"].as_i64().unwrap_or(0);
            let pc = part["partCount"].as_i64().unwrap_or(0);
            if i > 0 {
                job.append(if brk { "\n" } else { " " }, 0.0, body.clone());
            }
            let h = part["heading"].as_str().unwrap_or("");
            if !h.is_empty() && heads["show"].as_bool().unwrap_or(false) && pi <= 1 {
                let hs = num(&heads["size"], 36.0) * sc;
                job.append(h, 0.0, fmt(hs, color(&heads["color"], Color32::LIGHT_GRAY), FontFamily::Proportional, 1.3));
                job.append("\n", 0.0, fmt(hs * 0.3, fcol, fam.clone(), 1.0));
            }
            if show_num && pi <= 1 {
                let mut nf = fmt(size * vsc, vcol, FontFamily::Name("bold".into()), lh / vsc);
                nf.valign = Align::TOP;
                job.append(&format!("{} ", part["verse"]), 0.0, nf);
            }
            let mut txt = String::new();
            if pi > 1 { txt.push_str("… "); }
            txt.push_str(part["text"].as_str().unwrap_or(""));
            if pi > 0 && pi < pc { txt.push_str(" …"); }
            job.append(&txt, 0.0, body.clone());
            if let Some(t2) = part["text2"].as_str().filter(|x| !x.is_empty()) {
                job.append(&format!("\n{}", t2), 0.0, fmt(size * 0.8, fcol.gamma_multiply(0.85), fam.clone(), lh));
            }
        }
    }
    if show_ref && pos == "bottom" {
        job.append("\n", 0.0, fmt(rs * 0.45, fcol, fam.clone(), 1.0));
        job.append(&rt, 0.0, rfmt.clone());
    }
    let vl = &t["versionLabel"];
    if vl["show"].as_bool().unwrap_or(false) && !version_name.is_empty() {
        let vs = num(&vl["size"], 28.0) * sc;
        job.append(&format!("\n{}", version_name), 0.0, fmt(vs, color(&vl["color"], Color32::GRAY), FontFamily::Proportional, 1.2));
    }
    if let Some(ft) = s["footer"].as_str().filter(|x| !x.is_empty()) {
        job.append(&format!("\n{}", ft), 0.0, fmt(size * 0.45, fcol.gamma_multiply(0.8), FontFamily::Proportional, 1.4));
    }
    let galley = p.layout_job(job);
    let size_v = galley.size() + Vec2::splat(2.0 * bpad);
    let block = place(area, Vec2::new(if boxed { area.width().min(size_v.x.max(wrap + 2.0 * bpad)) } else { size_v.x }, size_v.y), t);
    if boxed {
        let bx = &t["box"];
        let col = color(&bx["gradient"], color(&bx["color"], Color32::BLACK));
        let op = num(&bx["opacity"], 0.6);
        p.rect_filled(block, num(&bx["radius"], 16.0) * sc, col.gamma_multiply(op));
    }
    let inner = block.shrink(bpad);
    // 줄 정렬은 galley 가 wrap 폭 안에서 한다 → 시작 x 는 정렬에 맞춰
    let gx = match halign(t["align"].as_str().unwrap_or("center")) {
        Align::LEFT => inner.left(),
        Align::RIGHT => inner.right(),
        _ => inner.center().x, // 가운데 정렬 galley 는 x=0 이 가운데
    };
    let origin = Pos2::new(gx, inner.top());
    if t["shadow"]["enabled"].as_bool().unwrap_or(false) {
        let off = Vec2::new(num(&t["shadow"]["x"], 0.0), num(&t["shadow"]["y"], 3.0)) * sc;
        p.galley_with_override_text_color(origin + off + Vec2::splat(0.5), galley.clone(), Color32::from_black_alpha(170));
    }
    let sw = num(&t["stroke"]["width"], 0.0) * sc;
    if sw > 0.2 {
        let scol = color(&t["stroke"]["color"], Color32::BLACK);
        for d in [Vec2::new(sw, 0.0), Vec2::new(-sw, 0.0), Vec2::new(0.0, sw), Vec2::new(0.0, -sw)] {
            p.galley_with_override_text_color(origin + d, galley.clone(), scol);
        }
    }
    p.galley(origin, galley, fcol);
    // 화면 가장자리에 고정된 장절
    if show_ref && pos.starts_with("fixed") {
        let off = num(&r["offset"], 60.0) * sc;
        let y = if pos == "fixed-top" { rect.top() + off } else { rect.bottom() - off };
        let al = halign(r["align"].as_str().or(t["align"].as_str()).unwrap_or("center"));
        let (x, a2) = match al {
            Align::LEFT => (area.left(), if pos == "fixed-top" { Align2::LEFT_TOP } else { Align2::LEFT_BOTTOM }),
            Align::RIGHT => (area.right(), if pos == "fixed-top" { Align2::RIGHT_TOP } else { Align2::RIGHT_BOTTOM }),
            _ => (rect.center().x, if pos == "fixed-top" { Align2::CENTER_TOP } else { Align2::CENTER_BOTTOM }),
        };
        p.text(Pos2::new(x, y), a2, rt, rfmt.font_id.clone(), rfmt.color);
    }
    let _ = Stroke::NONE;
}

fn paint_title(p: &egui::Painter, rect: Rect, s: &Value, t: &Value, sc: f32) {
    let area = content_rect(rect, t, sc);
    let f = &t["font"];
    let size = num(&f["size"], 72.0) * sc;
    let fcol = color(&f["color"], Color32::WHITE);
    let mut job = LayoutJob::default();
    job.wrap.max_width = area.width().max(10.0);
    job.halign = Align::Center;
    let t1 = size * 1.3;
    job.append(s["ref"].as_str().unwrap_or(""), 0.0, TextFormat { font_id: FontId::new(t1.max(1.0), FontFamily::Name("bold".into())), color: fcol, line_height: Some(t1 * 1.3), ..Default::default() });
    if let Some(sub) = s["subtitle"].as_str().filter(|x| !x.is_empty()) {
        let t2 = size * 0.62;
        job.append(&format!("\n{}", sub), 0.0, TextFormat { font_id: FontId::proportional(t2.max(1.0)), color: fcol.gamma_multiply(0.9), line_height: Some(t2 * 1.6), ..Default::default() });
    }
    let g = p.layout_job(job);
    let o = Pos2::new(area.center().x, area.center().y - g.size().y / 2.0);
    if t["shadow"]["enabled"].as_bool().unwrap_or(false) {
        p.galley_with_override_text_color(o + Vec2::new(0.0, 2.0 * sc + 0.5), g.clone(), Color32::from_black_alpha(170));
    }
    p.galley(o, g, fcol);
}
