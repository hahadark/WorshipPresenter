//! 조작 화면의 미리보기·LIVE 그림: 슬라이드를 템플릿 모양대로 egui 로 직접 그린다.
//! (예배당·OBS 화면은 웹 출력이 그리고, 여기는 운영자가 보는 작은 화면)

use eframe::egui::{self, text::LayoutJob, Align, Align2, Color32, FontFamily, FontId, Pos2, Rect, Stroke, TextFormat, Vec2};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use wp_core::measure;

pub struct Ctx<'a> {
    pub web_dir: &'a Path,
    pub data_dir: &'a Path,
}

/// 마지막으로 그린 슬라이드에서 본문·장절·로고·글 영역이 놓인 자리 (템플릿 편집 캔버스가 누른 곳을 알아낼 때 씀)
#[derive(Clone, Copy, Debug, Default)]
pub struct Hits {
    /// 글 영역 (여백 안쪽)
    pub area: Option<Rect>,
    /// 본문 글 덩어리
    pub body: Option<Rect>,
    /// 장절 줄
    pub reference: Option<Rect>,
    /// 로고 그림
    pub logo: Option<Rect>,
}

thread_local! {
    static HITS: std::cell::Cell<Hits> = std::cell::Cell::new(Hits::default());
    /// 바로 앞에 그린 글 덩어리의 맞춘 글자 크기 (템플릿 px, 점검용)
    static LAST_FIT: std::cell::Cell<f32> = const { std::cell::Cell::new(0.0) };
}

/// 바로 앞 paint_slide 가 고른 글자 크기 (점검 --parity 가 웹 fit 크기와 비교)
pub fn last_fit() -> f32 {
    LAST_FIT.with(|c| c.replace(0.0))
}

/// 바로 앞 paint_slide 가 기록한 자리를 꺼낸다 (꺼내면 비워짐)
pub fn take_hits() -> Hits {
    HITS.with(|h| h.replace(Hits::default()))
}

fn hit_set(f: impl FnOnce(&mut Hits)) {
    HITS.with(|h| {
        let mut v = h.get();
        f(&mut v);
        h.set(v);
    });
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
    image_uri_px(c, url, crate::imgcache::LARGE as f32)
}

/// 그릴 크기(물리 픽셀의 긴 변)를 알 때: 작은 그림(썸네일)은 줄여서 읽는다 (imgcache)
pub fn image_uri_px(c: &Ctx, url: &str, need_px: f32) -> Option<String> {
    let path = image_path(c, url)?;
    Some(crate::imgcache::uri_for(&path, need_px))
}

/// 주소 → 파일 경로. 파일이 있는지 확인은 2초 동안 기억한다 (슬라이드 수백 장 격자에서 프레임마다 디스크를 묻지 않게)
fn image_path(c: &Ctx, url: &str) -> Option<String> {
    type Memo = std::collections::HashMap<String, (Option<String>, std::time::Instant)>;
    static MEMO: once_cell::sync::Lazy<parking_lot::Mutex<Memo>> = once_cell::sync::Lazy::new(Default::default);
    let key = format!("{}|{url}", c.data_dir.display());
    if let Some((v, at)) = MEMO.lock().get(&key) {
        // 없던 파일은 더 자주 다시 본다 (찬송가 그림이 만들어지는 중일 수 있다)
        if at.elapsed().as_millis() < if v.is_some() { 2000 } else { 400 } {
            return v.clone();
        }
    }
    let v = image_path_now(c, url).map(|p| p.to_string_lossy().to_string());
    let mut m = MEMO.lock();
    if m.len() > 4000 {
        m.clear();
    }
    m.insert(key, (v.clone(), std::time::Instant::now()));
    v
}

fn image_path_now(c: &Ctx, url: &str) -> Option<PathBuf> {
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
    Some(path)
}

const IMG_OPTS: egui::TextureOptions = egui::TextureOptions { magnification: egui::TextureFilter::Linear, minification: egui::TextureFilter::Linear, wrap_mode: egui::TextureWrapMode::ClampToEdge, mipmap_mode: Some(egui::TextureFilter::Linear) };

/// [트랙 A] 다른 모듈에서 쓰는 그림 그리기
pub fn draw_image_pub(ui: &egui::Ui, c: &Ctx, rect: Rect, url: &str, cover: bool) -> bool {
    draw_image(ui, c, rect, url, cover)
}

fn draw_image(ui: &egui::Ui, c: &Ctx, rect: Rect, url: &str, cover: bool) -> bool {
    // 그릴 크기에 맞는 그림(썸네일은 512, 미리보기는 1920 까지)을 읽는다 — 메모리
    let need_px = rect.width().max(rect.height()) * ui.ctx().pixels_per_point();
    let Some(uri) = image_uri_px(c, url, need_px) else { return false };
    // 작게 줄인 그림도 거칠지 않게 (밉맵) — 브라우저 축소와 비슷하게
    let img = egui::Image::new(uri.clone()).texture_options(IMG_OPTS);
    let img = if cover { img.fit_to_exact_size(rect.size()) } else { img.fit_to_exact_size(rect.size()).maintain_aspect_ratio(true) };
    // 그림 비율 맞추기: 모양을 알면 가운데에 맞춰 넣는다
    match img.load_for_size(ui.ctx(), rect.size()) {
        Ok(poll) => {
            if let Some(sz) = poll.size() {
                let fitted = crate::slide_layout::fit_rect(rect, sz, cover); // object-fit: cover / contain
                let r = fitted.intersect(rect);
                let uv = if cover {
                    let full = fitted;
                    Rect::from_min_max(
                        Pos2::new((r.min.x - full.min.x) / full.width(), (r.min.y - full.min.y) / full.height()),
                        Pos2::new((r.max.x - full.min.x) / full.width(), (r.max.y - full.min.y) / full.height()),
                    )
                } else {
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0))
                };
                egui::Image::new(uri).texture_options(IMG_OPTS).uv(uv).paint_at(ui, r);
            } else {
                img.paint_at(ui, rect);
            }
            true
        }
        Err(e) => {
            // 같은 그림은 한 번만 로그에 (보이는 동안 프레임마다 쓰면 로그 파일이 금세 커진다)
            static SEEN: once_cell::sync::Lazy<parking_lot::Mutex<std::collections::HashSet<String>>> = once_cell::sync::Lazy::new(Default::default);
            let mut seen = SEEN.lock();
            if seen.len() < 500 && seen.insert(url.to_string()) {
                log::warn!("그림을 못 읽음 {url}: {e}");
            }
            false
        }
    }
}


/// linear-gradient 의 각도 (도). 'to right' = 90, 'to bottom right' = 135 …, 없으면 180
pub fn linear_angle(g: &str) -> f32 {
    let Some(i) = g.find("linear-gradient(") else { return 180.0 };
    let first = g[i + 16..].split(',').next().unwrap_or("").trim();
    if let Some(d) = first.strip_suffix("deg") {
        return d.trim().parse().unwrap_or(180.0);
    }
    match first {
        "to top" => 0.0,
        "to right" => 90.0,
        "to left" => 270.0,
        "to bottom right" | "to right bottom" => 135.0,
        "to bottom left" | "to left bottom" => 225.0,
        "to top right" | "to right top" => 45.0,
        "to top left" | "to left top" => 315.0,
        _ => 180.0,
    }
}

/// CSS 글에서 #색들을 차례대로
pub fn hex_colors(s: &str) -> Vec<Color32> {
    regex_lite_hex(s)
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


// ───────────────────────── 그림 크기 · 그라데이션 ─────────────────────────

/// 그림의 원래 크기 (아직 못 읽었으면 None — 다음 프레임에 다시)
fn image_size(ui: &egui::Ui, c: &Ctx, url: &str) -> Option<Vec2> {
    let uri = image_uri(c, url)?;
    egui::Image::new(uri).texture_options(IMG_OPTS).load_for_size(ui.ctx(), Vec2::splat(512.0)).ok()?.size()
}

type GradKey = (String, u32, u32, u32, u32);
static GRAD_TEX: once_cell::sync::Lazy<parking_lot::Mutex<std::collections::HashMap<GradKey, egui::TextureHandle>>> = once_cell::sync::Lazy::new(Default::default);

/// CSS 그라데이션(여러 겹)을 웹과 같은 규칙(css_bg)으로 그린 텍스처. size_t = 템플릿 px 크기, px = 화면 픽셀 크기
fn gradient_texture(ctx: &egui::Context, css: &str, size_t: Vec2, px: Vec2) -> Option<egui::TextureHandle> {
    let layers = crate::css_bg::parse_layers(css);
    if layers.is_empty() || size_t.x < 1.0 || size_t.y < 1.0 {
        return None;
    }
    // 크기를 8 픽셀 단위로 맞춰 텍스처를 다시 쓰고, 큰 화면도 1280 폭까지만 (그라데이션은 부드러워 충분)
    let pw = ((px.x.clamp(8.0, 1280.0) / 8.0).ceil() * 8.0) as u32;
    let ph = (((pw as f32) * size_t.y / size_t.x / 8.0).ceil() * 8.0).max(8.0) as u32;
    let key = (css.to_string(), size_t.x.round() as u32, size_t.y.round() as u32, pw, ph);
    let mut cache = GRAD_TEX.lock();
    if let Some(t) = cache.get(&key) {
        return Some(t.clone());
    }
    if cache.len() > 24 {
        cache.clear();
    }
    let px = crate::css_bg::rasterize(&layers, size_t.x, size_t.y, pw as usize, ph as usize);
    let img = egui::ColorImage::from_rgba_unmultiplied([pw as usize, ph as usize], &px);
    let tex = ctx.load_texture(format!("css-grad-{}", cache.len()), img, egui::TextureOptions::LINEAR);
    cache.insert(key, tex.clone());
    Some(tex)
}

/// rect 에 CSS 그라데이션을 (둥근 모서리로) 그린다. 못 읽으면 false
fn paint_css_gradient(p: &egui::Painter, rect: Rect, css: &str, size_t: Vec2, radius: f32, tint: Color32) -> bool {
    let ppp = p.ctx().pixels_per_point();
    let Some(tex) = gradient_texture(p.ctx(), css, size_t, rect.size() * ppp) else { return false };
    let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
    p.add(egui::epaint::RectShape::filled(rect, radius, tint).with_texture(tex.id(), uv));
    true
}

/// 투명 배경 체크 무늬 (render.css .bc-checker: 32px 칸, #2a2f38 / #1d2129)
fn checker(p: &egui::Painter, rect: Rect, sc: f32) {
    p.rect_filled(rect, 0.0, Color32::from_rgb(0x1d, 0x21, 0x29));
    let s = (crate::slide_layout::CHECKER_CELL * sc).max(2.0);
    let mut y = rect.top();
    let mut row = 0;
    while y < rect.bottom() {
        let mut x = rect.left() + if row % 2 == 0 { s } else { 0.0 }; // 웹: (0,0) 칸은 바탕색
        while x < rect.right() {
            p.rect_filled(Rect::from_min_size(Pos2::new(x, y), Vec2::splat(s)).intersect(rect), 0.0, Color32::from_rgb(0x2a, 0x2f, 0x38));
            x += 2.0 * s;
        }
        y += s;
        row += 1;
    }
}

/// 템플릿 글꼴 (font.family 첫 이름): 맑은 고딕·기본이면 슬라이드 기본 글꼴, 아니면 그 글꼴을 찾아 등록
fn tpl_family(ctx: &egui::Context, t: &Value, weight: f32) -> FontFamily {
    let bold = weight >= 600.0;
    let fam = t["font"]["family"].as_str().unwrap_or("");
    let first = fam.split(',').next().unwrap_or("").trim().trim_matches(['\'', '"']).trim();
    let plain = first.is_empty() || matches!(first.to_ascii_lowercase().as_str(), "malgun gothic" | "맑은 고딕" | "sans-serif" | "system-ui" | "serif" | "noto sans kr");
    if plain { crate::theme::slide_family(bold) } else { crate::paint_edit::family_for(ctx, first, bold) }
}

// ───────────────────────── 슬라이드 ─────────────────────────

/// 슬라이드 하나를 rect 에 그린다 (웹 출력 render.js buildStage + buildContent + fit 과 같은 규칙).
/// visible=false 면 글자는 숨기고 배경만 (LIVE 숨김 상태), logo = '로고 화면', hide_bg = 배경 끄기
pub fn paint_slide(ui: &egui::Ui, c: &Ctx, rect: Rect, slide: Option<&Value>, t: &Value, version_name: &str, visible: bool, logo: bool, hide_bg: bool) {
    let (tw, th) = crate::slide_layout::stage_size(t);
    // 템플릿 비율(예: 4:3)이 그릴 칸(16:9 썸네일 등)과 다르면 가운데에 맞추고 남는 곳은 검게 — 웹 출력 scaleStage 의 레터박스와 같다
    let rect = {
        let fit = letterbox(rect, tw / th.max(1.0));
        if fit != rect {
            ui.painter_at(rect).rect_filled(rect, 0.0, Color32::BLACK);
        }
        fit
    };
    let p = ui.painter_at(rect);
    let sc = rect.width() / tw;
    let to_screen = |r: Rect| Rect::from_min_size(rect.min + r.min.to_vec2() * sc, r.size() * sc);
    // 숨김 + '지우면 배경도 숨김' 템플릿: 배경층도 숨는다 (output.js render)
    let bg_off = hide_bg || (!visible && t["clearHidesBackground"].as_bool().unwrap_or(false));
    // ---- 배경층 (backgroundCss) + 로고
    let bg = &t["background"];
    let bgt = bg["type"].as_str().unwrap_or("color");
    if bg_off {
        p.rect_filled(rect, 0.0, Color32::BLACK);
    } else if bgt == "transparent" {
        checker(&p, rect, sc);
    } else {
        let base = color(&bg["color"], Color32::BLACK);
        let grad = bg["gradient"].as_str().unwrap_or("");
        if bgt == "gradient" && !grad.is_empty() {
            // background: <그라데이션> — 바탕색은 쓰지 않는다 (아래는 검은 창)
            p.rect_filled(rect, 0.0, Color32::BLACK);
            if !paint_css_gradient(&p, rect, grad, Vec2::new(tw, th), 0.0, Color32::WHITE) {
                p.rect_filled(rect, 0.0, base);
            }
        } else {
            p.rect_filled(rect, 0.0, base);
            if bgt == "image" {
                if let Some(u) = bg["image"].as_str().filter(|x| !x.is_empty()) {
                    draw_image(ui, c, rect, u, bg["imageFit"].as_str() != Some("contain"));
                }
            }
        }
        let dim = num(&bg["dim"], 0.0);
        if dim > 0.0 && matches!(bgt, "image" | "video" | "camera") {
            p.rect_filled(rect, 0.0, Color32::from_black_alpha((dim.min(1.0) * 255.0) as u8));
        }
    }
    // 로고: 배경층에 함께 (높이 = size, 폭 = 그림 비율). 투명 배경(OBS)에도 보인다
    if !bg_off {
        if let Some(u) = t["logo"]["image"].as_str().filter(|x| !x.is_empty()) {
            if t["logo"]["show"].as_bool().unwrap_or(false) {
                let aspect = image_size(ui, c, u).map(|s| s.x / s.y.max(1.0)).unwrap_or(1.0);
                if let Some(lr) = crate::slide_layout::logo_rect(t, aspect) {
                    let lr = to_screen(lr);
                    hit_set(|h| h.logo = Some(lr));
                    let op = num(&t["logo"]["opacity"], 1.0).clamp(0.0, 1.0);
                    if let Some(uri) = image_uri(c, u) {
                        egui::Image::new(uri).texture_options(IMG_OPTS).tint(Color32::WHITE.gamma_multiply(op)).paint_at(ui, lr);
                    }
                }
            }
        }
    }
    if let Some(s) = slide.filter(|_| visible) {
        paint_content(ui, c, &p, rect, s, t, sc, version_name, hide_bg);
    }
    // ---- 로고 화면 (모든 것 위): 검은 바탕 가운데 로고 (원래 크기, 폭 45%·높이 55% 까지)
    if logo {
        p.rect_filled(rect, 0.0, Color32::BLACK);
        let u = t["logo"]["image"].as_str().filter(|x| !x.is_empty()).unwrap_or("/shared/assets/sa-shield.png");
        let natural = image_size(ui, c, u).map(|s| s * sc);
        let lr = crate::slide_layout::logo_screen_rect(rect, natural.unwrap_or(Vec2::ZERO));
        if !draw_image(ui, c, lr, u, false) {
            p.text(rect.center(), Align2::CENTER_CENTER, "로고", FontId::proportional(rect.height() * 0.1), Color32::WHITE);
        }
    }
}

/// 본문층 (buildContent): 종류별
#[allow(clippy::too_many_arguments)]
fn paint_content(ui: &egui::Ui, c: &Ctx, p: &egui::Painter, rect: Rect, s: &Value, t: &Value, sc: f32, version_name: &str, hide_bg: bool) {
    let mut kind = s["kind"].as_str().unwrap_or("bible").to_string();
    let as_lyrics = crate::slide_layout::lyrics_only(t);
    let owned: Value;
    let mut s = s;
    // PPT·찬송가 그림 → 가사만 (OBS 자막 등): 가사가 없으면 아무것도 안 보인다
    if as_lyrics && (kind == "score" || kind == "image") {
        if let Some(cap) = s["caption"].as_str() {
            if cap.trim().is_empty() {
                return;
            }
            owned = json!({ "kind": "text", "ref": "", "label": s["label"], "parts": [{ "book": 0, "chapter": 0, "verse": 0, "text": cap.trim() }] });
            s = &owned;
            kind = "text".into();
        }
    }
    let ed = &s["edit"];
    let edit_on = ed.is_object() && (ed["allOutputs"].as_bool().unwrap_or(false) || !as_lyrics);
    let full_items = |ui: &egui::Ui| {
        if edit_on {
            if let Some(items) = ed["items"].as_array().filter(|a| !a.is_empty()) {
                crate::paint_edit::paint_items(ui, c, rect, items, true);
                crate::paint_edit::paint_items(ui, c, rect, items, false);
            }
        }
    };
    let label_font = FontId::proportional((rect.height() * 0.05).max(9.0));
    match kind.as_str() {
        "video" | "camera" => {
            // 영상·카메라: 화면 가득 검은 바탕 (실제 화면은 영상이 재생된다)
            p.rect_filled(rect, 0.0, Color32::BLACK);
            let label = if s["youtube"].is_object() { wp_core::i18n::tr_s("유튜브") } else if kind == "video" { wp_core::i18n::tr_s("영상") } else { wp_core::i18n::tr_s("카메라") };
            p.text(rect.center(), Align2::CENTER_CENTER, format!("{} · {}", label, s["ref"].as_str().unwrap_or("")), label_font, Color32::GRAY);
            full_items(ui);
        }
        "image" => {
            // .bc-imgslide img: object-fit contain, 뒤는 템플릿 배경이 비친다
            let u = s["image"].as_str().unwrap_or("");
            if !draw_image(ui, c, rect, u, false) {
                p.text(rect.center(), Align2::CENTER_CENTER, wp_core::i18n::tr_args("그림: {1}", &[&(s["ref"].as_str().unwrap_or(""))]), label_font, Color32::GRAY);
            }
            full_items(ui);
        }
        "score" => {
            // 찬송가: (배경 cover) + 투명 악보 그림 contain
            if let Some(sbg) = s["bg"].as_str().filter(|x| !x.is_empty()) {
                if let Some(col) = sbg.starts_with('#').then(|| parse_color(sbg)).flatten() {
                    p.rect_filled(rect, 0.0, col);
                } else {
                    draw_image(ui, c, rect, sbg, true);
                }
            }
            let u = s["image"].as_str().unwrap_or("");
            if !draw_image(ui, c, rect, u, false) {
                p.text(rect.center(), Align2::CENTER_CENTER, wp_core::i18n::tr_args("그림: {1}", &[&(s["ref"].as_str().unwrap_or(""))]), label_font, Color32::GRAY);
            }
            full_items(ui);
        }
        "blank" => {
            if s["mode"] != "background" {
                p.rect_filled(rect, 0.0, Color32::BLACK);
            }
            full_items(ui);
        }
        _ => {
            // 순서 자체 배경 (교독문 원래 배경 등): 화면 전체, 배경 끄기면 숨김
            if let Some(sbg) = s["bg"].as_str().filter(|x| !x.is_empty() && !as_lyrics && !hide_bg) {
                if sbg.starts_with('#') {
                    p.rect_filled(rect, 0.0, parse_color(sbg).unwrap_or(Color32::BLACK));
                } else {
                    p.rect_filled(rect, 0.0, Color32::BLACK);
                    if !sbg.starts_with("camera:") && !is_video(sbg) {
                        draw_image(ui, c, rect, sbg, true);
                    }
                }
            }
            let te = if edit_on { wp_core::slideedit::edited_template(t, ed) } else { t.clone() };
            let items = if edit_on { ed["items"].as_array().cloned().unwrap_or_default() } else { vec![] };
            if !items.is_empty() {
                crate::paint_edit::paint_items(ui, c, rect, &items, true);
            }
            let hide_main = edit_on && ed["hideMain"].as_bool().unwrap_or(false);
            paint_text_block(p, rect, s, &te, sc, version_name, &kind, hide_main);
            if !items.is_empty() {
                crate::paint_edit::paint_items(ui, c, rect, &items, false);
            }
        }
    }
    if let Some(cd) = s["countdown"].as_object().filter(|cd| cd.get("target").and_then(|x| x.as_str()).is_some_and(|x| !x.is_empty())) {
        paint_countdown(p, rect, t, sc, cd);
    }
}

fn is_video(u: &str) -> bool {
    let u = percent_encoding::percent_decode_str(u).decode_utf8_lossy().to_ascii_lowercase();
    let u = u.split('#').next().unwrap_or("");
    [".mp4", ".m4v", ".webm", ".mov"].iter().any(|e| u.ends_with(e))
}

/// 슬라이드쇼 카운트다운 (countdownHtml): 오른쪽 아래 둥근 검은 상자
fn paint_countdown(p: &egui::Painter, rect: Rect, t: &Value, sc: f32, cd: &serde_json::Map<String, Value>) {
    let k = crate::slide_layout::countdown_scale(t) * sc;
    p.ctx().request_repaint_after(std::time::Duration::from_millis(500)); // 카운트다운이 보이는 동안만 초마다
    let target = cd.get("target").and_then(|x| x.as_str()).unwrap_or("");
    let now = wp_core::util::now_ms() + wp_core::localtime::local_offset_ms();
    let day_ms = now.rem_euclid(86_400_000);
    let left = if let Some((h, m)) = target.split_once(':').and_then(|(h, m)| Some((h.parse::<i64>().ok()?, m.parse::<i64>().ok()?))) {
        ((h * 3600 + m * 60) * 1000 - day_ms).max(0) / 1000
    } else {
        target.parse::<f64>().map(|m| (m * 60.0) as i64).unwrap_or(300)
    };
    let txt = if left == 0 && cd.get("doneText").and_then(|x| x.as_str()).is_some_and(|x| !x.is_empty()) {
        cd["doneText"].as_str().unwrap_or("").to_string()
    } else if left >= 3600 {
        format!("{}:{:02}:{:02}", left / 3600, left / 60 % 60, left % 60)
    } else {
        format!("{:02}:{:02}", left / 60 % 60, left % 60)
    };
    let fam = crate::theme::slide_family(true);
    let big = p.layout_no_wrap(txt, FontId::new(78.0 * k, fam), Color32::WHITE);
    let label = cd.get("label").and_then(|x| x.as_str()).filter(|x| !x.is_empty());
    let small = label.map(|l| p.layout_no_wrap(l.to_string(), FontId::new(30.0 * k, crate::theme::slide_family(false)), Color32::from_white_alpha(217)));
    let inner_w = big.size().x.max(small.as_ref().map(|g| g.size().x).unwrap_or(0.0));
    let inner_h = 78.0 * k * 1.1 + small.as_ref().map(|g| g.size().y).unwrap_or(0.0);
    let bx = Rect::from_min_max(Pos2::new(rect.right() - 60.0 * k - inner_w - 52.0 * k, rect.bottom() - 50.0 * k - inner_h - 28.0 * k), Pos2::new(rect.right() - 60.0 * k, rect.bottom() - 50.0 * k));
    p.rect_filled(bx, 16.0 * k, Color32::from_black_alpha(158));
    let mut y = bx.top() + 14.0 * k;
    if let Some(g) = small {
        p.galley(Pos2::new(bx.center().x - g.size().x / 2.0, y), g.clone(), Color32::WHITE);
        y += g.size().y;
    }
    p.galley(Pos2::new(bx.center().x - big.size().x / 2.0, y + (78.0 * k * 1.1 - big.size().y) / 2.0), big, Color32::WHITE);
}

// ───────────────────────── 글 덩어리 (.bc-block) ─────────────────────────

/// 글 덩어리 안에 놓인 galley 하나 (덩어리 안쪽 왼쪽 위 기준, x 는 galley 의 정렬 기준점)
struct Piece {
    g: std::sync::Arc<egui::Galley>,
    pos: Pos2,
    role: Role,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Body,
    Ref,
    Other,
}

struct Laid {
    pieces: Vec<Piece>,
    height: f32,
    wide: bool,
}

/// 글꼴·색 설정 (화면 px)
#[derive(Clone)]
struct Fmt {
    size: f32,
    fam: FontFamily,
    color: Color32,
    line: f32,
    letter: f32,
}

impl Fmt {
    fn tf(&self) -> TextFormat {
        TextFormat { font_id: FontId::new(self.size.max(0.5), self.fam.clone()), color: self.color, line_height: Some(self.line.max(0.5)), extra_letter_spacing: self.letter, ..Default::default() }
    }
    fn with(&self, size_mul: f32, line_mul: f32) -> Fmt {
        Fmt { size: self.size * size_mul, line: self.line * line_mul, ..self.clone() }
    }
}

/// 맑은 고딕 등 한글 글꼴의 글꼴 높이 대비 ascent (윈도우 winAscent/(winAscent+winDescent) ≈ 2167/2703)
const ASCENT: f32 = 0.8017;

/// 서식 표시(**굵게** · {크기:150}…{/크기} · {색:#e63946}…{/색}) 를 조각으로 (groups.js fmtHtml)
fn fmt_runs(s: &str) -> Vec<(String, bool, Option<Color32>, f32)> {
    let re = regex_lite_fmt();
    let mut out = vec![];
    let (mut bold, mut cols, mut sizes): (bool, Vec<Color32>, Vec<f32>) = (false, vec![], vec![]);
    let mut last = 0;
    for m in re.find_iter(s) {
        if m.start() > last {
            out.push((s[last..m.start()].to_string(), bold, cols.last().copied(), sizes.last().copied().unwrap_or(1.0)));
        }
        let tok = m.as_str();
        if tok == "**" {
            bold = !bold;
        } else if let Some(c) = tok.strip_prefix("{색:").and_then(|x| x.strip_suffix('}')) {
            if let Some(c) = parse_color(c) { cols.push(c); }
        } else if tok == "{/색}" {
            cols.pop();
        } else if let Some(z) = tok.strip_prefix("{크기:").and_then(|x| x.strip_suffix('}')) {
            sizes.push(z.parse::<f32>().unwrap_or(100.0) / 100.0 * sizes.last().copied().unwrap_or(1.0));
        } else if tok == "{/크기}" {
            sizes.pop();
        }
        last = m.end();
    }
    if last < s.len() {
        out.push((s[last..].to_string(), bold, cols.last().copied(), sizes.last().copied().unwrap_or(1.0)));
    }
    out
}

fn regex_lite_fmt() -> &'static regex_lite_shim::Re {
    static RE: once_cell::sync::Lazy<regex_lite_shim::Re> = once_cell::sync::Lazy::new(regex_lite_shim::Re::new);
    &RE
}

/// 서식 표시 찾기 (정규식 크레이트 없이): "**" · "{색:#…}" · "{/색}" · "{크기:NNN}" · "{/크기}"
mod regex_lite_shim {
    pub struct Re;
    pub struct M<'a> {
        s: &'a str,
        a: usize,
        b: usize,
    }
    impl<'a> M<'a> {
        pub fn start(&self) -> usize { self.a }
        pub fn end(&self) -> usize { self.b }
        pub fn as_str(&self) -> &'a str { &self.s[self.a..self.b] }
    }
    impl Re {
        pub fn new() -> Self { Re }
        pub fn find_iter<'a>(&self, s: &'a str) -> Vec<M<'a>> {
            let mut out = vec![];
            let mut i = 0;
            while i < s.len() {
                let r = &s[i..];
                let hit = if r.starts_with("**") {
                    Some(2)
                } else if r.starts_with("{/색}") {
                    Some("{/색}".len())
                } else if r.starts_with("{/크기}") {
                    Some("{/크기}".len())
                } else if r.starts_with("{색:#") || r.starts_with("{크기:") {
                    r.find('}').filter(|&e| e < 24).map(|e| e + 1)
                } else {
                    None
                };
                match hit {
                    Some(n) => {
                        out.push(M { s, a: i, b: i + n });
                        i += n;
                    }
                    None => i += r.chars().next().map(|c| c.len_utf8()).unwrap_or(1),
                }
            }
            out
        }
    }
}

/// pre-line: 줄바꿈은 지키고 줄 안의 공백 여러 개는 하나로, 줄 앞뒤 공백은 지운다
fn pre_line(s: &str) -> String {
    s.split('\n').map(|l| l.split_whitespace().collect::<Vec<_>>().join(" ")).collect::<Vec<_>>().join("\n")
}

/// 본문 글 덩어리를 만든다 (partsHtml · hangingHtml · refHtml · 번역본 이름). 크기는 화면 px
#[allow(clippy::too_many_arguments)]
fn lay_block(p: &egui::Painter, s: &Value, t: &Value, size: f32, sc: f32, w: f32, version_name: &str, kind: &str, wrap: &mut Wrap) -> Laid {
    let mut counter = 0usize;
    let ctx = p.ctx();
    let f = &t["font"];
    let weight = num(&f["weight"], 500.0);
    let fcol = color(&f["color"], Color32::WHITE);
    let lh = num(&f["lineHeight"], 1.5);
    let letter = num(&f["letterSpacing"], 0.0) * sc;
    let align = crate::slide_layout::halign(t["align"].as_str().unwrap_or("center"));
    let body = Fmt { size, fam: tpl_family(ctx, t, weight), color: fcol, line: size * lh, letter };
    let mut pieces: Vec<Piece> = vec![];
    let mut y = 0.0f32;
    let mut wide = false;
    let to_align = |a: crate::slide_layout::HAlign| match a {
        crate::slide_layout::HAlign::Left => Align::LEFT,
        crate::slide_layout::HAlign::Right => Align::RIGHT,
        crate::slide_layout::HAlign::Center => Align::Center,
    };
    let new_job = |a: crate::slide_layout::HAlign, width: f32| {
        let mut j = LayoutJob::default();
        j.wrap.max_width = width.max(4.0);
        j.halign = to_align(a);
        j
    };
    let mut push = |pieces: &mut Vec<Piece>, y: &mut f32, job: LayoutJob, a: crate::slide_layout::HAlign, mt: f32, mb: f32, role: Role, x0: f32, width: f32| {
        // 줄바꿈 자리는 템플릿 크기(px)에서 정하고, 작은 화면에는 같은 자리에서 끊어 그린다 (작은 글자의 반올림으로 줄이 달라지지 않게)
        let idx = counter;
        counter += 1;
        // CSS 줄 상자 높이 = 줄마다 가장 큰 line-height (egui 는 줄 자리를 픽셀로 반올림해 합이 조금 달라진다)
        let line = job.sections.iter().filter_map(|s| s.format.line_height).fold(0.0f32, f32::max);
        let (g, rows) = match wrap {
            Wrap::Measure(v) => {
                // 한 줄로 늘어놓고 웹(CSS)처럼 끊는다: 빈칸 뒤에서만(keep-all), 줄 끝 빈칸은 폭에 넣지 않음, 끊을 데가 없으면 글자 사이(anywhere)
                let mut nj = job;
                let max_w = nj.wrap.max_width;
                nj.wrap.max_width = f32::INFINITY;
                let g = p.layout_job(nj);
                let br = css_breaks(&g, max_w);
                let rows = g.rows.len() + br.len();
                v.push(br);
                (g, rows)
            }
            Wrap::Apply(b) => {
                let g = p.layout_job(apply_breaks(job, b.get(idx).map(|x| x.as_slice()).unwrap_or(&[])));
                let rows = g.rows.len();
                (g, rows)
            }
        };
        let _ = &mut wide;        *y += mt;
        let x = crate::slide_layout::anchor_x(x0, width, a);
        let h = if line > 0.0 { rows as f32 * line } else { g.size().y };
        pieces.push(Piece { g, pos: Pos2::new(x, *y), role });
        *y += h + mb;
    };
    // ---- 장절 (위)
    let r = &t["reference"];
    let rt = measure::ref_text(s, t);
    let rpos = r["position"].as_str().unwrap_or("top");
    let ref_on = r["show"].as_bool().unwrap_or(false) && !rt.is_empty() && kind != "title";
    let rs = num(&r["size"], 44.0) * sc;
    let rfmt = Fmt { size: rs, fam: tpl_family(ctx, t, num(&r["weight"], 700.0)), color: color(&r["color"], Color32::from_rgb(255, 212, 121)), line: rs * 1.25, letter };
    let ralign = crate::slide_layout::halign(r["align"].as_str().filter(|x| !x.is_empty()).or(t["align"].as_str()).unwrap_or("center"));
    if ref_on && rpos == "top" {
        let mut j = new_job(ralign, w);
        j.append(&rt, 0.0, rfmt.tf());
        push(&mut pieces, &mut y, j, ralign, 0.0, 0.45 * rs, Role::Ref, 0.0, w);
    }
    // ---- 본문 (.bc-text)
    let parts = s["parts"].as_array().cloned().unwrap_or_default();
    if kind == "title" {
        // 제목 카드: .bc-t1 1.3em 굵게 줄 1.3 · .bc-t2 .62em 위 .5em 줄 1.4 (장절 색)
        let t1 = Fmt { size: size * 1.3, fam: tpl_family(ctx, t, 700.0), color: fcol, line: size * 1.3 * 1.3, letter };
        let mut j = new_job(align, w);
        j.append(s["ref"].as_str().unwrap_or(""), 0.0, t1.tf());
        push(&mut pieces, &mut y, j, align, 0.0, 0.0, Role::Body, 0.0, w);
        if let Some(sub) = s["subtitle"].as_str().filter(|x| !x.is_empty()) {
            let t2 = Fmt { size: size * 0.62, fam: body.fam.clone(), color: color(&r["color"], Color32::from_rgb(255, 212, 121)), line: size * 0.62 * 1.4, letter };
            let mut j = new_job(align, w);
            j.append(sub, 0.0, t2.tf());
            push(&mut pieces, &mut y, j, align, 0.5 * size * 0.62, 0.0, Role::Body, 0.0, w);
        }
    } else if kind == "text" {
        // 가사·공지: 줄바꿈 그대로 (pre-line), '>' 줄은 번역 줄(작게)
        let bold_fam = tpl_family(ctx, t, 700.0);
        let runs_into = |j: &mut LayoutJob, txt: &str, base: &Fmt| {
            // 줄마다 본문 글자 크기의 줄 높이(strut)를 둔다: 작은 글자만 있는 줄도 웹처럼 본문 줄 높이
            let strut = |j: &mut LayoutJob| if j.text.is_empty() || j.text.ends_with('\n') { j.append("\u{200B}", 0.0, base.tf()); };
            strut(j);
            for (seg, b, col, mul) in fmt_runs(txt).into_iter().flat_map(|(s, b, c, m)| s.split_inclusive('\n').map(|x| (x.to_string(), b, c, m)).collect::<Vec<_>>()) {
                let mut f = base.with(mul, 1.0);
                f.line = base.line.max(base.line * mul); // 큰 글자는 줄도 커진다
                if b { f.fam = bold_fam.clone(); }
                if let Some(c) = col { f.color = c; }
                let mut tf = f.tf();
                if mul < 0.999 {
                    // 작은 글자는 본문 글자와 밑줄(baseline)을 맞춘다 (egui 는 줄 가운데에 둔다)
                    let fb = p.fonts_mut(|x| x.row_height(&FontId::new(base.size.max(0.5), base.fam.clone())));
                    let fs = p.fonts_mut(|x| x.row_height(&tf.font_id));
                    tf.line_height = Some(crate::slide_layout::baseline_line_height(base.line, fb, fs, ASCENT));
                    tf.valign = Align::BOTTOM;
                }
                j.append(&seg, 0.0, tf);
                strut(j);
            }
        };
        let mut cur = new_job(align, w);
        let mut cur_has = false;
        for part in &parts {
            let txt = part["text"].as_str().unwrap_or("");
            let has_tr = txt.lines().any(|l| l.trim_start().starts_with('>'));
            if !has_tr {
                runs_into(&mut cur, &pre_line(txt), &body);
                cur_has = true;
                continue;
            }
            if cur_has {
                push(&mut pieces, &mut y, std::mem::replace(&mut cur, new_job(align, w)), align, 0.0, 0.0, Role::Body, 0.0, w);
                cur_has = false;
            }
            for l in txt.split('\n') {
                let mut j = new_job(align, w);
                if let Some(tr) = l.trim_start().strip_prefix('>') {
                    let trf = Fmt { color: fcol.gamma_multiply(0.85), ..body.with(0.7, 0.7) };
                    runs_into(&mut j, &pre_line(tr.strip_prefix(' ').unwrap_or(tr)), &trf);
                    push(&mut pieces, &mut y, j, align, 0.05 * trf.size, 0.25 * trf.size, Role::Body, 0.0, w);
                } else {
                    let l = pre_line(l);
                    runs_into(&mut j, if l.is_empty() { "\u{a0}" } else { &l }, &body);
                    push(&mut pieces, &mut y, j, align, 0.0, 0.0, Role::Body, 0.0, w);
                }
            }
        }
        if cur_has {
            push(&mut pieces, &mut y, cur, align, 0.0, 0.0, Role::Body, 0.0, w);
        }
    } else {
        // 성경: 절 번호(위첨자) · 소제목 · 병행 번역 · 내어쓰기
        let vn = &t["verseNumbers"];
        let show_num = vn["show"].as_bool().unwrap_or(true);
        let vsc = num(&vn["scale"], 0.6);
        let vcol = color(&vn["color"], Color32::from_rgb(158, 197, 255));
        let heads = &t["headings"];
        let hs = num(&heads["size"], 36.0) * sc;
        let hfmt = Fmt { size: hs, fam: body.fam.clone(), color: color(&heads["color"], Color32::LIGHT_GRAY), line: hs * 1.3, letter };
        let par = Fmt { color: fcol.gamma_multiply(0.86), ..body.with(0.72, 0.72) };
        let hanging = t["align"] == "hanging";
        let brk = t["verseBreak"].as_bool().unwrap_or(false);
        let fh = |f: &Fmt| p.fonts_mut(|x| x.row_height(&FontId::new(f.size.max(0.5), f.fam.clone())));
        let num_fam = tpl_family(ctx, t, 700.0);
        if hanging {
            let digits = parts.iter().map(|x| x["verse"].to_string().len()).max().unwrap_or(1);
            let (nw, gap) = crate::slide_layout::hanging_cols(digits, vsc, size, show_num);
            let tw = (w - nw - gap).max(4.0);
            let nfmt = Fmt { size: size * vsc, fam: num_fam.clone(), color: vcol, line: size * vsc * lh, letter };
            for part in &parts {
                let pi = part["partIndex"].as_i64().unwrap_or(0);
                let pc = part["partCount"].as_i64().unwrap_or(0);
                let h = part["heading"].as_str().unwrap_or("");
                if !h.is_empty() && heads["show"].as_bool().unwrap_or(false) && pi <= 1 {
                    let mut j = new_job(crate::slide_layout::HAlign::Left, w);
                    j.append(h, 0.0, hfmt.tf());
                    push(&mut pieces, &mut y, j, crate::slide_layout::HAlign::Left, 0.0, 0.3 * hs, Role::Body, 0.0, w);
                }
                let row_top = y;
                let mut txt = String::new();
                if pi > 1 { txt.push_str("… "); }
                txt.push_str(part["text"].as_str().unwrap_or(""));
                if pi > 0 && pi < pc { txt.push_str(" …"); }
                let mut j = new_job(crate::slide_layout::HAlign::Left, tw);
                j.append(&txt, 0.0, body.tf());
                push(&mut pieces, &mut y, j, crate::slide_layout::HAlign::Left, 0.0, 0.0, Role::Body, nw + gap, tw);
                if let Some(t2) = part["text2"].as_str().filter(|x| !x.is_empty()) {
                    let mut j = new_job(crate::slide_layout::HAlign::Left, tw);
                    j.append(t2, 0.0, par.tf());
                    push(&mut pieces, &mut y, j, crate::slide_layout::HAlign::Left, 0.12 * par.size, 0.0, Role::Body, nw + gap, tw);
                }
                if show_num && pi <= 1 {
                    // 번호 칸: 오른쪽 맞춤, 본문 첫 줄과 글자 밑줄(baseline)을 맞춘다
                    let g = p.layout_no_wrap(part["verse"].to_string(), FontId::new(nfmt.size.max(0.5), nfmt.fam.clone()), vcol);
                    let (fb, fnn) = (fh(&body), fh(&nfmt));
                    let base_txt = 0.5 * (body.line - fb) + ASCENT * fb;
                    let ny = row_top + base_txt - ASCENT * fnn;
                    pieces.push(Piece { pos: Pos2::new(nw - g.size().x, ny), g, role: Role::Body });
                }
            }
        } else {
            let (fb, fnn) = (fh(&body), fh(&body.with(vsc, vsc)));
            let sup_line = crate::slide_layout::superscript_line_height(body.line, fb, fnn, ASCENT, 0.45 * size * vsc); // .bc-vn vertical-align .45em (번호 자신의 글자 크기 기준)
            let mut nf = Fmt { size: size * vsc, fam: num_fam.clone(), color: vcol, line: sup_line, letter }.tf();
            nf.valign = Align::TOP;
            let mut cur = new_job(align, w);
            let mut cur_has = false;
            let flush = |pieces: &mut Vec<Piece>, y: &mut f32, cur: &mut LayoutJob, cur_has: &mut bool, push: &mut dyn FnMut(&mut Vec<Piece>, &mut f32, LayoutJob, crate::slide_layout::HAlign, f32, f32, Role, f32, f32)| {
                if *cur_has {
                    push(pieces, y, std::mem::replace(cur, new_job(align, w)), align, 0.0, 0.0, Role::Body, 0.0, w);
                    *cur_has = false;
                }
            };
            for part in &parts {
                let pi = part["partIndex"].as_i64().unwrap_or(0);
                let pc = part["partCount"].as_i64().unwrap_or(0);
                let h = part["heading"].as_str().unwrap_or("");
                if !h.is_empty() && heads["show"].as_bool().unwrap_or(false) && pi <= 1 {
                    flush(&mut pieces, &mut y, &mut cur, &mut cur_has, &mut push);
                    let mut j = new_job(align, w);
                    j.append(h, 0.0, hfmt.tf());
                    push(&mut pieces, &mut y, j, align, 0.0, 0.3 * hs, Role::Body, 0.0, w);
                }
                if brk {
                    flush(&mut pieces, &mut y, &mut cur, &mut cur_has, &mut push);
                } else if cur_has {
                    cur.append(" ", 0.0, body.tf());
                }
                let mut lead = 0.0;
                if show_num && pi <= 1 {
                    cur.append(&part["verse"].to_string(), 0.0, nf.clone());
                    lead = 0.25 * size * vsc; // .bc-vn margin-right .25em
                }
                let mut txt = String::new();
                if pi > 1 { txt.push_str("… "); }
                txt.push_str(part["text"].as_str().unwrap_or(""));
                if pi > 0 && pi < pc { txt.push_str(" …"); }
                cur.append(&txt, lead, body.tf());
                cur_has = true;
                if let Some(t2) = part["text2"].as_str().filter(|x| !x.is_empty()) {
                    flush(&mut pieces, &mut y, &mut cur, &mut cur_has, &mut push);
                    let mut j = new_job(align, w);
                    j.append(t2, 0.0, par.tf());
                    push(&mut pieces, &mut y, j, align, 0.12 * par.size, 0.0, Role::Body, 0.0, w);
                }
                if brk {
                    flush(&mut pieces, &mut y, &mut cur, &mut cur_has, &mut push);
                }
            }
            flush(&mut pieces, &mut y, &mut cur, &mut cur_has, &mut push);
        }
    }
    // ---- 장절 (아래) · 번역본 이름
    if ref_on && rpos == "bottom" {
        let mut j = new_job(ralign, w);
        j.append(&rt, 0.0, rfmt.tf());
        push(&mut pieces, &mut y, j, ralign, 0.45 * rs, 0.0, Role::Ref, 0.0, w);
    }
    let vl = &t["versionLabel"];
    if vl["show"].as_bool().unwrap_or(false) && !version_name.is_empty() && kind != "title" {
        let vs = num(&vl["size"], 28.0) * sc;
        let vf = Fmt { size: vs, fam: body.fam.clone(), color: color(&vl["color"], Color32::GRAY), line: vs * 1.2, letter };
        let mut j = new_job(align, w);
        j.append(version_name, 0.0, vf.tf());
        push(&mut pieces, &mut y, j, align, 0.4 * vs, 0.0, Role::Other, 0.0, w);
    }
    Laid { pieces, height: y, wide }
}

/// 줄바꿈: Measure = 템플릿 크기에서 끊긴 자리를 모은다, Apply = 그 자리에서 끊는다 (조각 차례대로)
enum Wrap {
    Measure(Vec<Vec<usize>>),
    Apply(Vec<Vec<usize>>),
}

/// 한 줄로 늘어놓은 galley 를 폭 max_w 로 끊을 자리 (글 안의 바이트 위치 = 다음 줄이 시작하는 곳).
/// CSS word-break: keep-all + overflow-wrap: anywhere + white-space: normal 과 같은 규칙
fn css_breaks(g: &egui::Galley, max_w: f32) -> Vec<usize> {
    let text = g.job.text.as_str();
    let byte_of = |ci: usize| text.char_indices().nth(ci).map(|(b, _)| b).unwrap_or(text.len());
    let mut out = vec![];
    let mut base = 0usize;
    for row in g.rows.iter() {
        let gs = &row.row.glyphs;
        let mut start = 0usize;
        let mut cand: Option<usize> = None;
        for i in 0..gs.len() {
            let ch = gs[i].chr;
            if i > start && matches!(gs[i - 1].chr, ' ' | '-' | '\u{2014}' | '/') && ch != ' ' {
                cand = Some(i);
            }
            if ch == ' ' {
                continue; // 줄 끝 빈칸은 넘쳐도 된다
            }
            let right = gs[i].pos.x + gs[i].advance_width;
            if i > start && right - gs[start].pos.x > max_w + 0.01 {
                let b = cand.filter(|&c| c > start).unwrap_or(i);
                out.push(byte_of(base + b));
                start = b;
                cand = None;
            }
        }
        base += row.char_count_including_newline().0;
    }
    out
}
/// 정해진 자리에서 줄을 바꾼 job (끊긴 자리 앞의 빈칸은 줄바꿈으로 바꾸고, 그 밖에는 줄바꿈을 끼운다). 저절로 줄바꿈은 끈다
fn apply_breaks(mut job: LayoutJob, breaks: &[usize]) -> LayoutJob {
    job.wrap.max_width = f32::INFINITY;
    for &b in breaks.iter().rev() {
        let bytes = job.text.as_bytes();
        if b > 0 && b <= bytes.len() && bytes[b - 1] == b' ' {
            job.text.replace_range(b - 1..b, "\n");
        } else if b < bytes.len() && bytes[b] == b' ' {
            job.text.replace_range(b..b + 1, "\n");
        } else if b <= job.text.len() && job.text.is_char_boundary(b) {
            job.text.insert(b, '\n');
            for sec in &mut job.sections {
                if sec.byte_range.start.0 >= b && sec.byte_range.start.0 > 0 {
                    sec.byte_range.start.0 += 1;
                }
                if sec.byte_range.end.0 >= b {
                    sec.byte_range.end.0 += 1;
                }
            }
        }
    }
    job
}

/// 글자 그림자 (text-shadow x y blur color): 흐림은 둥글게 여러 번 겹쳐 그려 비슷하게
fn draw_shadowed(p: &egui::Painter, pos: Pos2, g: &std::sync::Arc<egui::Galley>, t: &Value, sc: f32, base: Color32) {
    let sh = &t["shadow"];
    if sh["enabled"].as_bool().unwrap_or(false) {
        let col = sh["color"].as_str().and_then(parse_color).unwrap_or(Color32::from_black_alpha(204));
        let off = Vec2::new(num(&sh["x"], 0.0), num(&sh["y"], 3.0)) * sc;
        let sigma = num(&sh["blur"], 8.0) * sc / 2.0;
        if sigma < 0.6 {
            p.galley_with_override_text_color(pos + off, g.clone(), col);
        } else {
            let a = col.a() as f32 / 255.0;
            let per = 1.0 - (1.0 - a * 0.75).powf(1.0 / 9.0);
            let c1 = Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), (per * 255.0) as u8);
            let c2 = Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), (per * 0.5 * 255.0) as u8);
            p.galley_with_override_text_color(pos + off, g.clone(), c1);
            for k in 0..8 {
                let (s, c) = (k as f32 * std::f32::consts::FRAC_PI_4).sin_cos();
                p.galley_with_override_text_color(pos + off + Vec2::new(c, s) * sigma * 0.7, g.clone(), c1);
                p.galley_with_override_text_color(pos + off + Vec2::new(c, s) * sigma * 1.6, g.clone(), c2);
            }
        }
    }
    // -webkit-text-stroke (폭×2, 바깥 절반이 보임)
    let sw = num(&t["stroke"]["width"], 0.0) * sc;
    if sw > 0.2 {
        let scol = color(&t["stroke"]["color"], Color32::BLACK);
        for k in 0..12 {
            let (s, c) = (k as f32 * std::f32::consts::TAU / 12.0).sin_cos();
            p.galley_with_override_text_color(pos + Vec2::new(c, s) * sw, g.clone(), scol);
        }
    }
    p.galley(pos, g.clone(), base);
}

/// 글 덩어리 + 상자 + 고정 장절 + 꼬리말 (buildContentInner 의 글 부분 + fit)
#[allow(clippy::too_many_arguments)]
fn paint_text_block(p: &egui::Painter, rect: Rect, s: &Value, t: &Value, sc: f32, version_name: &str, kind: &str, hide_main: bool) {
    let area_t = crate::slide_layout::content_rect(t);
    let area = Rect::from_min_size(rect.min + area_t.min.to_vec2() * sc, area_t.size() * sc);
    let f = &t["font"];
    let bx = &t["box"];
    let boxed = bx["enabled"].as_bool().unwrap_or(false);
    let bpad = if boxed { num(&bx["padding"], 32.0) * sc } else { 0.0 };
    let w = (area.width() - 2.0 * bpad).max(4.0);
    // ---- fit: 설정 크기 → 넘치면 (autoFit) 최소 크기까지 정수로 나눠 찾기
    // 크기 맞춤·줄바꿈은 템플릿 해상도(px)에서 한다 (웹 fit 과 같은 수치) → 화면에는 배율만 곱해서
    let bpad_t = if boxed { num(&bx["padding"], 32.0) } else { 0.0 };
    let w_t = (area_t.width() - 2.0 * bpad_t).max(4.0);
    let over = |l: &Laid| l.height + 2.0 * bpad_t > area_t.height() + 0.5 || l.wide;
    let lay = |z: f32| {
        let mut m = Wrap::Measure(vec![]);
        let l = lay_block(p, s, t, z, 1.0, w_t, version_name, kind, &mut m);
        (l, match m { Wrap::Measure(v) => v, Wrap::Apply(_) => vec![] })
    };
    let full = num(&f["size"], 72.0);
    let mut chosen = full;
    let mut measured = lay(full);
    if over(&measured.0) && f["autoFit"].as_bool().unwrap_or(true) {
        let (mut lo, mut hi) = (num(&f["minSize"], 44.0).floor() as i32, full.floor() as i32);
        measured = lay(lo as f32);
        chosen = lo as f32;
        if !over(&measured.0) {
            while hi - lo > 1 {
                let mid = (lo + hi) / 2;
                if over(&lay(mid as f32).0) { hi = mid } else { lo = mid }
            }
            chosen = lo as f32;
            measured = lay(chosen);
        }
    }
    LAST_FIT.with(|c| c.set(chosen));
    let laid = lay_block(p, s, t, chosen * sc, sc, w, version_name, kind, &mut Wrap::Apply(measured.1));
    let block_h = laid.height + 2.0 * bpad;
    let top = crate::slide_layout::block_top(area, block_h, t["valign"].as_str().unwrap_or("middle"));
    let block = Rect::from_min_size(Pos2::new(area.left(), top), Vec2::new(area.width(), block_h));
    let inner = block.shrink(bpad);
    let fcol = color(&f["color"], Color32::WHITE);
    let mut body_rect: Option<Rect> = None;
    let mut ref_rect: Option<Rect> = None;
    if !hide_main {
        if boxed {
            let radius = num(&bx["radius"], 16.0) * sc;
            let grad = bx["gradient"].as_str().unwrap_or("");
            let painted = !grad.is_empty() && paint_css_gradient(p, block, grad, block.size() / sc.max(0.001), radius, Color32::WHITE);
            if !painted {
                let col = color(&bx["color"], Color32::BLACK);
                p.rect_filled(block, radius, col.gamma_multiply(num(&bx["opacity"], 0.6).clamp(0.0, 1.0)));
            }
        }
        for pc in &laid.pieces {
            let origin = inner.min + pc.pos.to_vec2();
            let gr = pc.g.rect.translate(origin.to_vec2());
            match pc.role {
                Role::Ref => ref_rect = Some(gr),
                Role::Body => body_rect = Some(body_rect.map_or(gr, |b| b.union(gr))),
                Role::Other => {}
            }
            draw_shadowed(p, origin, &pc.g, t, sc, fcol);
        }
    }
    // ---- 화면 가장자리에 고정된 장절 (글 영역 폭, 줄 높이 = 본문 줄 배수)
    let r = &t["reference"];
    let rt = measure::ref_text(s, t);
    let rpos = r["position"].as_str().unwrap_or("top");
    if r["show"].as_bool().unwrap_or(false) && !rt.is_empty() && rpos.starts_with("fixed") && kind != "title" && !hide_main {
        let rs = num(&r["size"], 44.0) * sc;
        let ralign = crate::slide_layout::halign(r["align"].as_str().filter(|x| !x.is_empty()).or(t["align"].as_str()).unwrap_or("center"));
        let fam = tpl_family(p.ctx(), t, num(&r["weight"], 700.0));
        let mut j = LayoutJob::default();
        j.wrap.max_width = area.width().max(4.0);
        j.halign = match ralign { crate::slide_layout::HAlign::Left => Align::LEFT, crate::slide_layout::HAlign::Right => Align::RIGHT, _ => Align::Center };
        j.append(&rt, 0.0, TextFormat { font_id: FontId::new(rs.max(0.5), fam), color: color(&r["color"], Color32::from_rgb(255, 212, 121)), line_height: Some(rs * num(&f["lineHeight"], 1.5)), extra_letter_spacing: num(&f["letterSpacing"], 0.0) * sc, ..Default::default() });
        let g = p.layout_job(j);
        let (edge, is_top) = crate::slide_layout::fixed_ref_edge(t);
        let y = rect.top() + edge * sc - if is_top { 0.0 } else { g.size().y };
        let x = crate::slide_layout::anchor_x(area.left(), area.width(), ralign);
        let origin = Pos2::new(x, y);
        ref_rect = Some(g.rect.translate(origin.to_vec2()));
        let rc = color(&r["color"], Color32::from_rgb(255, 212, 121));
        draw_shadowed(p, origin, &g, t, sc, rc);
    }
    // ---- 꼬리말 (slide.footer): 화면 아래 4% 여백, 작은 글자
    if let Some(ft) = s["footer"].as_str().filter(|x| !x.is_empty() && !crate::slide_layout::lyrics_only(t)) {
        let (l, rr, bottom, fsz) = crate::slide_layout::footer_box(t);
        let right = t["align"] == "right";
        let mut j = LayoutJob::default();
        j.wrap.max_width = ((rr - l) * sc).max(4.0);
        j.halign = if right { Align::RIGHT } else { Align::LEFT };
        j.append(ft, 0.0, TextFormat { font_id: FontId::new((fsz * sc).max(0.5), tpl_family(p.ctx(), t, 400.0)), color: fcol.gamma_multiply(0.78), ..Default::default() });
        let g = p.layout_job(j);
        let origin = Pos2::new(rect.left() + if right { rr } else { l } * sc, rect.top() + bottom * sc - g.size().y);
        if t["shadow"]["enabled"].as_bool().unwrap_or(false) {
            for k in 0..8 {
                let (sn, cs) = (k as f32 * std::f32::consts::FRAC_PI_4).sin_cos();
                p.galley_with_override_text_color(origin + Vec2::new(0.0, sc) + Vec2::new(cs, sn) * 1.2 * sc, g.clone(), Color32::from_black_alpha(60));
            }
        }
        p.galley(origin, g, fcol.gamma_multiply(0.78));
    }
    hit_set(|h| {
        h.area = Some(area);
        h.body = Some(if boxed { block } else { body_rect.unwrap_or(block) });
        h.reference = ref_rect;
    });
    let _ = Stroke::NONE;
}


/// OBS 자막의 '그림 우상단'(output index.html .bc-pip): 위 3%·오른쪽 2%·폭 30%, 둥글기 1%, 그림자
pub fn paint_pip(ui: &egui::Ui, c: &Ctx, rect: Rect, slide: &Value, main_t: &Value) {
    let (tw, th) = crate::slide_layout::stage_size(main_t);
    let w = rect.width() * 0.30;
    let r = Rect::from_min_size(Pos2::new(rect.right() - rect.width() * 0.02 - w, rect.top() + rect.height() * 0.03), Vec2::new(w, w * th / tw.max(1.0)));
    let p = ui.painter_at(rect);
    for k in 1..=4 {
        let k = k as f32;
        p.rect_filled(r.translate(Vec2::new(0.0, rect.width() * 0.005)).expand(rect.width() * 0.004 * k), rect.width() * 0.01 + k, Color32::from_black_alpha(22));
    }
    paint_slide(ui, c, r, Some(slide), main_t, "", true, false, false);
}

/// 칸 안에 가로:세로 = aspect 인 가장 큰 사각형 (가운데). 비율이 거의 같으면(0.5% 안) 칸 그대로
pub fn letterbox(rect: Rect, aspect: f32) -> Rect {
    if !(aspect.is_finite() && aspect > 0.0) || rect.height() <= 0.0 {
        return rect;
    }
    let cur = rect.width() / rect.height();
    if (cur / aspect - 1.0).abs() < 0.005 {
        return rect;
    }
    let size = if cur > aspect { Vec2::new(rect.height() * aspect, rect.height()) } else { Vec2::new(rect.width(), rect.width() / aspect) };
    Rect::from_center_size(rect.center(), size)
}

#[cfg(test)]
mod letterbox_tests {
    use super::*;
    #[test]
    fn letterbox_fits_four_three_in_wide() {
        let wide = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(320.0, 180.0));
        assert_eq!(letterbox(wide, 16.0 / 9.0), wide, "같은 비율이면 그대로");
        let r = letterbox(wide, 4.0 / 3.0);
        assert_eq!(r.height(), 180.0);
        assert!((r.width() - 240.0).abs() < 0.01 && (r.center().x - wide.center().x).abs() < 0.01);
        let tall = Rect::from_min_size(Pos2::ZERO, Vec2::new(300.0, 300.0));
        let r = letterbox(tall, 16.0 / 9.0);
        assert!((r.width() - 300.0).abs() < 0.01 && (r.height() - 168.75).abs() < 0.01);
    }
    #[test]
    fn aspect_names() {
        assert_eq!(crate::tpl_inspect::aspect_of(1920.0, 1080.0), "16:9");
        assert_eq!(crate::tpl_inspect::aspect_of(1440.0, 1080.0), "4:3");
        assert_eq!(crate::tpl_inspect::aspect_of(1024.0, 768.0), "4:3");
        assert_eq!(crate::tpl_inspect::aspect_of(1080.0, 1920.0), "");
    }
}
