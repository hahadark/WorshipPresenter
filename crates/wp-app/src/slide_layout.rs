//! 슬라이드 자리 계산 — 웹 출력(web/shared/render.js · render.css)의 규칙을 그대로 옮긴 순수 함수.
//! 모든 값은 템플릿 해상도(px) 기준. 조작 화면 그림(paint.rs)은 이 결과에 배율만 곱해 그린다.

use eframe::egui::{Pos2, Rect, Vec2};
use serde_json::Value;

fn num(v: &Value, d: f32) -> f32 {
    v.as_f64().map(|x| x as f32).unwrap_or(d)
}

pub fn stage_size(t: &Value) -> (f32, f32) {
    (num(&t["width"], 1920.0), num(&t["height"], 1080.0))
}

/// render.js contentRect: 안전영역 + 여백을 뺀 글 영역
pub fn content_rect(t: &Value) -> Rect {
    let (w, h) = stage_size(t);
    let safe = num(&t["safeArea"], 0.0);
    let (sx, sy) = (w * safe / 100.0, h * safe / 100.0);
    let p = &t["padding"];
    let (l, tp, r, b) = (num(&p["left"], 160.0), num(&p["top"], 120.0), num(&p["right"], 160.0), num(&p["bottom"], 120.0));
    Rect::from_min_size(Pos2::new(sx + l, sy + tp), Vec2::new(w - 2.0 * sx - l - r, h - 2.0 * sy - tp - b))
}

/// 가로 정렬 ('hanging' 은 왼쪽)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HAlign {
    Left,
    Center,
    Right,
}

pub fn halign(a: &str) -> HAlign {
    match a {
        "left" | "hanging" | "start" | "justify" => HAlign::Left,
        "right" | "end" => HAlign::Right,
        _ => HAlign::Center,
    }
}

/// 본문 덩어리(.bc-block)의 위쪽 y: .bc-area 는 세로 flex, justify-content = valign (넘치면 가운데·아래는 위로도 넘친다)
pub fn block_top(area: Rect, block_h: f32, valign: &str) -> f32 {
    match valign {
        "top" => area.top(),
        "bottom" => area.bottom() - block_h,
        _ => area.top() + (area.height() - block_h) / 2.0,
    }
}

/// 덩어리 안 한 줄의 x 기준점 (egui galley 의 x=0 이 정렬 기준: 왼쪽=왼끝, 가운데=가운데, 오른쪽=오른끝)
pub fn anchor_x(left: f32, width: f32, a: HAlign) -> f32 {
    match a {
        HAlign::Left => left,
        HAlign::Center => left + width / 2.0,
        HAlign::Right => left + width,
    }
}

/// 로고 (render.js buildStage): 높이 = size, 폭 = 그림 비율, 여백 margin 만큼 띄움. aspect = 폭/높이
pub fn logo_rect(t: &Value, aspect: f32) -> Option<Rect> {
    let lg = &t["logo"];
    if !lg["show"].as_bool().unwrap_or(false) || lg["image"].as_str().is_none_or(|x| x.is_empty()) {
        return None;
    }
    let (w, h) = stage_size(t);
    let m = num(&lg["margin"], 40.0);
    let hh = { let s = num(&lg["size"], 120.0); if s > 0.0 { s } else { 120.0 } };
    let ww = hh * aspect.max(0.01);
    let pos = lg["position"].as_str().unwrap_or("top-right");
    let known = matches!(pos, "top-left" | "top-center" | "top-right" | "bottom-left" | "bottom-center" | "bottom-right");
    let pos = if known { pos } else { "top-right" };
    let x = if pos.ends_with("left") { m } else if pos.ends_with("center") { (w - ww) / 2.0 } else { w - m - ww };
    let y = if pos.starts_with("bottom") { h - m - hh } else { m };
    Some(Rect::from_min_size(Pos2::new(x, y), Vec2::new(ww, hh)))
}

/// object-fit: cover / contain (가운데 맞춤). img = 그림 원래 크기
pub fn fit_rect(container: Rect, img: Vec2, cover: bool) -> Rect {
    if img.x <= 0.0 || img.y <= 0.0 {
        return container;
    }
    let s = if cover { (container.width() / img.x).max(container.height() / img.y) } else { (container.width() / img.x).min(container.height() / img.y) };
    Rect::from_center_size(container.center(), img * s)
}

/// '로고 화면' (.bc-logoscreen img): 원래 크기, 단 폭 45%·높이 55% 를 넘지 않게 줄임, 가운데
pub fn logo_screen_rect(stage: Rect, img: Vec2) -> Rect {
    if img.x <= 0.0 || img.y <= 0.0 {
        return Rect::from_center_size(stage.center(), Vec2::splat(stage.height() * 0.4));
    }
    let s = (stage.width() * 0.45 / img.x).min(stage.height() * 0.55 / img.y).min(1.0);
    Rect::from_center_size(stage.center(), img * s)
}

/// 화면 가장자리에 고정된 장절 (fixedRefHtml): (줄 상자의 위 y 또는 아래 y, 위 고정인가)
pub fn fixed_ref_edge(t: &Value) -> (f32, bool) {
    let r = &t["reference"];
    let (_, h) = stage_size(t);
    let off = num(&r["offset"], 60.0);
    let sy = h * num(&t["safeArea"], 0.0) / 100.0;
    if r["position"] == "fixed-top" { (sy + off, true) } else { (h - sy - off, false) }
}

/// 꼬리말 (slide.footer): (왼쪽, 오른쪽, 아래, 글자 크기)
pub fn footer_box(t: &Value) -> (f32, f32, f32, f32) {
    let (w, h) = stage_size(t);
    (w * 0.04, w * 0.96, h * 0.965, (h * 0.024).round())
}

/// 카운트다운 상자 (countdownHtml): 오른쪽 60·아래 50 (1080 기준), 글자 30·78, 안쪽 여백 14×26
pub fn countdown_scale(t: &Value) -> f32 {
    let (_, h) = stage_size(t);
    (h / 1080.0 * 100.0).round() / 100.0
}

/// 내어쓰기 절 번호 칸 폭: calc(digits × 0.6 × scale em + 0.1em), 칸 사이 0.35em
pub fn hanging_cols(digits: usize, scale: f32, size: f32, show: bool) -> (f32, f32) {
    if !show {
        return (0.0, 0.0);
    }
    ((digits.max(1) as f32 * 0.6 * scale + 0.1) * size, 0.35 * size)
}

/// 투명 배경 체크 무늬 칸 (render.css .bc-checker: 64px 무늬 = 32px 칸)
pub const CHECKER_CELL: f32 = 32.0;

/// lyricsOnly: 이 출력에서 PPT·찬송가 슬라이드를 가사 글자만으로 보이는가
pub fn lyrics_only(t: &Value) -> bool {
    match t["pptMode"].as_str().filter(|x| !x.is_empty()) {
        Some(m) => m == "lyrics",
        None => t["background"]["type"] == "transparent",
    }
}

/// 위첨자 절 번호(.bc-vn: vertical-align .45em) 를 egui 한 줄 안에 올려 놓을 줄 높이.
/// egui 는 글자를 줄 높이 가운데에 두므로, 번호의 줄 높이를 줄여 위로 올린다.
/// line = 본문 줄 높이, fh_body/fh_num = 글꼴 높이, asc = 글꼴 높이 대비 ascent 비율, raise = 올릴 거리
pub fn superscript_line_height(line: f32, fh_body: f32, fh_num: f32, asc: f32, raise: f32) -> f32 {
    let top_body = 0.5 * (line - fh_body);
    let baseline = top_body + asc * fh_body;
    let top_num = baseline - raise - asc * fh_num;
    (2.0 * top_num + fh_num).clamp(fh_num.min(line), line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tpl() -> Value {
        json!({ "width": 1920, "height": 1080, "safeArea": 0, "padding": { "left": 200, "top": 140, "right": 200, "bottom": 140 },
            "logo": { "show": true, "image": "/shared/assets/sa-shield.png", "margin": 48, "size": 120, "position": "top-right" },
            "reference": { "position": "fixed-bottom", "offset": 60 } })
    }

    #[test]
    fn content_rect_is_padding_inside() {
        let r = content_rect(&tpl());
        assert_eq!((r.left(), r.top(), r.width(), r.height()), (200.0, 140.0, 1520.0, 800.0));
        let mut t = tpl();
        t["safeArea"] = json!(5);
        let r = content_rect(&t);
        assert_eq!((r.left(), r.top()), (296.0, 194.0));
        assert_eq!(r.width(), 1920.0 - 192.0 - 400.0);
    }

    #[test]
    fn nine_cells_place_block_like_flex() {
        // 9칸 '아래·오른쪽' : 덩어리 아래 = 영역 아래, 줄 기준점 = 영역 오른쪽
        let area = content_rect(&tpl());
        assert_eq!(block_top(area, 100.0, "bottom"), 840.0);
        assert_eq!(block_top(area, 100.0, "middle"), 490.0);
        assert_eq!(block_top(area, 100.0, "top"), 140.0);
        assert_eq!(anchor_x(area.left(), area.width(), halign("right")), 1720.0);
        assert_eq!(anchor_x(area.left(), area.width(), halign("center")), 960.0);
        assert_eq!(anchor_x(area.left(), area.width(), halign("hanging")), 200.0);
        // 넘치면 가운데 정렬은 위아래로 똑같이 넘친다
        assert_eq!(block_top(area, 1000.0, "middle"), 40.0);
    }

    #[test]
    fn logo_keeps_aspect_and_margin() {
        // 높이 120, 폭은 그림 비율 (방패 0.85) → 오른쪽 여백 48
        let r = logo_rect(&tpl(), 0.85).unwrap();
        assert!((r.right() - (1920.0 - 48.0)).abs() < 0.01 && r.top() == 48.0 && r.height() == 120.0);
        assert!((r.width() - 102.0).abs() < 0.01);
        let mut t = tpl();
        t["logo"]["position"] = json!("bottom-center");
        let r = logo_rect(&t, 2.0).unwrap();
        assert_eq!((r.left(), r.bottom()), (840.0, 1080.0 - 48.0));
        t["logo"]["show"] = json!(false);
        assert!(logo_rect(&t, 1.0).is_none());
    }

    #[test]
    fn fixed_reference_edges() {
        assert_eq!(fixed_ref_edge(&tpl()), (1020.0, false));
        let mut t = tpl();
        t["reference"]["position"] = json!("fixed-top");
        t["safeArea"] = json!(10);
        assert_eq!(fixed_ref_edge(&t), (108.0 + 60.0, true));
    }

    #[test]
    fn fit_cover_contain() {
        let c = Rect::from_min_size(Pos2::ZERO, Vec2::new(1920.0, 1080.0));
        let r = fit_rect(c, Vec2::new(1000.0, 1000.0), false);
        assert_eq!((r.width(), r.height(), r.left()), (1080.0, 1080.0, 420.0));
        let r = fit_rect(c, Vec2::new(1000.0, 1000.0), true);
        assert_eq!((r.width(), r.top()), (1920.0, -420.0));
        // 로고 화면: 작은 그림은 원래 크기
        assert_eq!(logo_screen_rect(c, Vec2::new(300.0, 300.0)).width(), 300.0);
        assert!((logo_screen_rect(c, Vec2::new(2000.0, 2000.0)).height() - 594.0).abs() < 0.01);
    }

    #[test]
    fn footer_and_hanging() {
        let (l, r, b, z) = footer_box(&tpl());
        assert!((l - 76.8).abs() < 0.01 && (r - 1843.2).abs() < 0.01 && (b - 1042.2).abs() < 0.01 && z == 26.0);
        let (nw, gap) = hanging_cols(2, 0.6, 72.0, true);
        assert!((nw - (2.0 * 0.36 + 0.1) * 72.0).abs() < 0.01 && (gap - 25.2).abs() < 0.01);
        assert_eq!(hanging_cols(2, 0.6, 72.0, false), (0.0, 0.0));
    }

    #[test]
    fn lyrics_only_rule() {
        assert!(lyrics_only(&json!({ "background": { "type": "transparent" } })));
        assert!(!lyrics_only(&json!({ "background": { "type": "color" } })));
        assert!(!lyrics_only(&json!({ "pptMode": "slide", "background": { "type": "transparent" } })));
        assert!(lyrics_only(&json!({ "pptMode": "lyrics", "background": { "type": "color" } })));
    }

    #[test]
    fn superscript_raises_number() {
        // 72px 글, 줄 높이 1.5, 번호 0.6배: 번호 줄 높이가 본문보다 작아 위로 올라간다
        let fh = 72.0 * 1.33;
        let lh = superscript_line_height(108.0, fh, fh * 0.6, 0.8, 0.45 * 72.0);
        assert!(lh < 108.0 && lh > 0.0);
    }
}
