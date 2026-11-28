//! Lucide 선 아이콘 (Electron 판 `web/shared/icons.js` 와 같은 한 벌, ISC 라이선스).
//!
//! 쓰는 법 (다음 작업자용):
//! - `icons::show(ui, "settings", 18.0, theme::pal().muted)` → 자리를 잡아 그리고 Response.
//! - `icons::paint(ui, rect, "x", color)` → 이미 잡은 사각형 안에 그림 (직접 그리는 위젯용).
//! - `icons::image("play", 18.0, color)` → `egui::Image` (Button::image 등에 넣을 때).
//! - `icons::for_type("bible")` → 큐시트 순서 종류 아이콘 이름 (icons.js `TYPE`).
//! - `icons::has(name)` · `icons::names()`.
//!
//! SVG 는 `stroke="currentColor"` 를 흰색으로 바꿔 만든 뒤 egui 의 tint(곱하기)로 원하는 색을 입힌다.
//! 만든 SVG 는 이름별로 한 번만 만들고(프로그램 끝까지 보관), 그림은 egui 그림 캐시(bytes://ic/…)가 맡는다.
#![allow(dead_code)] // 다음 단계 작업자가 쓸 공개 API (아직 안 쓰는 것이 있음)

use eframe::egui::{self, Color32, Rect, Response, Sense, Vec2};
use std::collections::HashMap;

static SRC: &str = include_str!("../../../web/shared/icons.js");
static CRAFT: &str = include_str!("../../../assets/icons/craft.json");
/// 고유 아이콘 한 벌(assets/icons/craft.json, 16px 격자 · 선 1.25)을 쓸지. **예전 Lucide 로 되돌리려면 이 한 줄을 false 로.**
/// 실행할 때만 잠깐 견주려면 환경변수 `WP_ICONS=lucide`. craft 에 없는 이름은 늘 Lucide(web/shared/icons.js)
const USE_CRAFT_ICONS: bool = true;
static LEGACY: once_cell::sync::Lazy<bool> = once_cell::sync::Lazy::new(|| !USE_CRAFT_ICONS || std::env::var("WP_ICONS").is_ok_and(|v| v == "lucide"));

/// 이름 → SVG 문서 (흰 선, 24×24, stroke-width 2, fill none)
static SVGS: once_cell::sync::Lazy<HashMap<&'static str, &'static [u8]>> = once_cell::sync::Lazy::new(|| {
    let mut result: HashMap<_, _> = parse(SRC)
        .into_iter()
        .map(|(name, body)| {
            let doc = format!(
                r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="#ffffff" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">{}</svg>"##,
                body.replace("currentColor", "#ffffff")
            );
            (name, &*Box::leak(doc.into_bytes().into_boxed_slice()))
        })
        .collect();
    if !*LEGACY {
        let paths: HashMap<&'static str, &'static str> = serde_json::from_str(CRAFT).expect("craft icon source");
        for (name, body) in paths {
            let doc = format!(r##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="#ffffff" stroke-width="1.25" stroke-linecap="round" stroke-linejoin="round">{body}</svg>"##);
            result.insert(name, &*Box::leak(doc.into_bytes().into_boxed_slice()));
        }
    }
    result
});

/// icons.js 의 `const PATHS = { 'name': '<path …/>', … }` 를 읽는다 (주석 `<!-- … -->` 는 뺀다)
fn parse(src: &'static str) -> Vec<(&'static str, String)> {
    let start = src.find("const PATHS").unwrap_or(0);
    let end = src[start..].find("\n  };").map(|e| start + e).unwrap_or(src.len());
    let mut out = vec![];
    for line in src[start..end].lines() {
        let l = line.trim();
        if !l.starts_with('\'') {
            continue;
        }
        let Some(k_end) = l[1..].find('\'') else { continue };
        let name = &l[1..1 + k_end];
        let rest = &l[1 + k_end + 1..];
        let (Some(a), Some(b)) = (rest.find('\''), rest.rfind('\'')) else { continue };
        if b <= a {
            continue;
        }
        let mut body = rest[a + 1..b].to_string();
        while let (Some(c0), Some(c1)) = (body.find("<!--"), body.find("-->")) {
            if c1 < c0 {
                break;
            }
            body.replace_range(c0..c1 + 3, "");
        }
        out.push((name, body));
    }
    out
}

/// 큐시트 순서 종류 → 아이콘 이름 (icons.js `TYPE`)
pub fn for_type(t: &str) -> &'static str {
    match t {
        "hymn" | "score" => "music-2",
        "bible" => "book-open",
        "song" => "music",
        "text" => "megaphone",
        "title" => "tag",
        "image" => "image",
        "video" => "clapperboard",
        "audio" => "volume-2",
        "slideshow" => "images",
        "camera" => "camera",
        "weblyrics" => "globe",
        "ppt" => "presentation",
        "folder" => "folder-open",
        "color" => "palette",
        _ => "square",
    }
}

pub fn has(name: &str) -> bool {
    SVGS.contains_key(name)
}

pub fn names() -> Vec<&'static str> {
    let mut v: Vec<_> = SVGS.keys().copied().collect();
    v.sort();
    v
}

/// SVG 문서 바이트 (없는 이름은 None)
pub fn svg(name: &str) -> Option<&'static [u8]> {
    SVGS.get(name).copied()
}

/// 아이콘 그림 (크기는 정사각형 한 변, 색은 tint). 없는 이름이면 'square'
pub fn image(name: &str, size: f32, color: Color32) -> egui::Image<'static> {
    let (key, bytes) = match SVGS.get_key_value(name) {
        Some((k, b)) => (*k, *b),
        None => ("square", svg("square").unwrap_or(b"<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\"/>")),
    };
    egui::Image::from_bytes(format!("bytes://ic/{key}.svg"), bytes).fit_to_exact_size(Vec2::splat(size)).tint(color)
}

/// rect 가운데에 아이콘을 그린다 (rect 보다 크지 않게)
pub fn paint(ui: &egui::Ui, rect: Rect, name: &str, color: Color32) {
    let s = rect.width().min(rect.height());
    image(name, s, color).paint_at(ui, Rect::from_center_size(rect.center(), Vec2::splat(s)));
}

/// 자리를 잡아 아이콘을 그린다 (누를 수 없음 — 누르는 단추는 widgets::icon_btn)
pub fn show(ui: &mut egui::Ui, name: &str, size: f32, color: Color32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    if ui.is_rect_visible(rect) {
        paint(ui, rect, name, color);
    }
    resp
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_icons_js() {
        let n = super::names();
        assert!(n.len() > 80, "아이콘 {}개", n.len());
        for k in ["x", "play", "skip-back", "skip-forward", "settings", "search", "text", "circle-help", "tag"] {
            let s = std::str::from_utf8(super::svg(k).unwrap()).unwrap();
            assert!(s.starts_with("<svg") && s.ends_with("</svg>") && !s.contains("<!--") && !s.contains("currentColor"), "{k}: {s}");
        }
        assert_eq!(super::for_type("bible"), "book-open");
    }
}
