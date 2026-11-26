//! 템플릿 편집기 오른쪽 '속성' 칸 — 배경 · 글자 · 위치와 여백 · 장절·번역본·절 번호 · 로고 · 효과 · 고급.
//! 쉬운 말로 된 시각적 입력만 앞에 두고, 글로 적는 값(px·CSS·주소)은 '고급'에만 둔다 (docs/ux-rules.md).
//! 범위는 Electron tplcontrols.js(전체 편집) + settings.js 고급 칸과 같다. 저장 형식(templates)은 그대로.

use crate::form;
use crate::theme::{self, pal, Weight};
use crate::tpl_controls::{self as tc, b, f, s, set};
use crate::widgets::{Btn, Size};
use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use serde_json::{json, Value};
use std::path::Path;
use wp_core::i18n::tr;

/// 속성 묶음
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sec {
    Bg,
    Font,
    Layout,
    Ref,
    Logo,
    Effect,
    Adv,
}

impl Sec {
    pub const ALL: [Sec; 7] = [Sec::Bg, Sec::Font, Sec::Layout, Sec::Ref, Sec::Logo, Sec::Effect, Sec::Adv];
    pub fn id(self) -> &'static str {
        match self {
            Sec::Bg => "bg",
            Sec::Font => "font",
            Sec::Layout => "layout",
            Sec::Ref => "ref",
            Sec::Logo => "logo",
            Sec::Effect => "fx",
            Sec::Adv => "adv",
        }
    }
    pub fn title(self) -> &'static str {
        match self {
            Sec::Bg => "배경",
            Sec::Font => "글자",
            Sec::Layout => "위치와 여백",
            Sec::Ref => "장절 · 번역본 · 절 번호",
            Sec::Logo => "로고",
            Sec::Effect => "효과 (외곽선 · 그림자 · 상자)",
            Sec::Adv => "고급",
        }
    }
    fn icon(self) -> &'static str {
        match self {
            Sec::Bg => "image",
            Sec::Font => "text",
            Sec::Layout => "layout-grid",
            Sec::Ref => "book-open",
            Sec::Logo => "flag",
            Sec::Effect => "sparkles",
            Sec::Adv => "sliders-horizontal",
        }
    }
}

pub const BASE_FONTS: [&str; 15] = ["맑은 고딕", "나눔고딕", "나눔명조", "나눔스퀘어", "바탕", "돋움", "굴림", "궁서", "Pretendard", "Noto Sans KR", "Noto Serif KR", "Arial", "Georgia", "Times New Roman", "Segoe UI"];
const TEXT_COLORS: [&str; 6] = ["#ffffff", "#fff3c4", "#ffd479", "#9ec5ff", "#b8f2c8", "#000000"];
const REF_COLORS: [&str; 6] = ["#ffd479", "#ffffff", "#9ec5ff", "#ff9b9e", "#cccccc", "#000000"];
const BG_COLORS: [&str; 7] = ["#000000", "#0b1d3a", "#13261b", "#2a1414", "#1f1f1f", "#7a0e1f", "#ffffff"];
const BOX_COLORS: [&str; 5] = ["#000000", "#0b1d3a", "#ffffff", "#d8102f", "#1f1f1f"];
const LOGOS: [(&str, &str); 2] = [("/shared/assets/sa-shield.png", "레드 쉴드"), ("/shared/assets/sa-crest.png", "크레스트")];

/// 그라데이션 방향 (값, 이름)
pub const GRAD_DIRS: [(&str, &str); 4] = [("tb", "위 → 아래"), ("lr", "왼쪽 → 오른쪽"), ("diag", "대각선"), ("radial", "가운데에서 둥글게")];

/// 두 색 + 방향 → CSS (Electron 출력이 그대로 쓰는 background.gradient)
pub fn gradient_css(c1: &str, c2: &str, dir: &str) -> String {
    match dir {
        "lr" => format!("linear-gradient(90deg, {c1} 0%, {c2} 100%)"),
        "diag" => format!("linear-gradient(135deg, {c1} 0%, {c2} 100%)"),
        "radial" => format!("radial-gradient(circle at center, {c1} 0%, {c2} 100%)"),
        _ => format!("linear-gradient(180deg, {c1} 0%, {c2} 100%)"),
    }
}

/// CSS → (색1, 색2, 방향, 두 색으로 다시 만들어도 같은가)
pub fn parse_gradient(css: &str) -> (String, String, String, bool) {
    let hex = |c: Color32| format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b());
    let cols = crate::paint::hex_colors(css);
    let c1 = cols.first().map(|c| hex(*c)).unwrap_or_else(|| "#1b2a4a".into());
    let c2 = cols.last().filter(|_| cols.len() > 1).map(|c| hex(*c)).unwrap_or_else(|| "#000000".into());
    let dir = if css.contains("radial-gradient") {
        "radial"
    } else {
        match crate::paint::linear_angle(css).round() as i32 {
            90 => "lr",
            135 => "diag",
            _ => "tb",
        }
    };
    let simple = css.trim().is_empty() || gradient_css(&c1, &c2, dir).eq_ignore_ascii_case(css.trim());
    (c1, c2, dir.to_string(), simple)
}

/// 고른 파일을 자료 폴더 media 로 복사하고 출력 주소('/media/…')를 돌려준다 (Electron /api/media 와 같은 자리)
pub fn import_media(data_dir: &Path, src: &Path, kind: &str) -> Result<String, String> {
    let dir = data_dir.join("media");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let ext = src.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_else(|| "png".into());
    let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    let name = format!("{kind}-{ms:x}.{ext}");
    std::fs::copy(src, dir.join(&name)).map_err(|e| e.to_string())?;
    Ok(format!("/media/{name}"))
}

fn font_name(family: &str) -> String {
    family.trim().trim_start_matches(['\'', '"']).split([',', '\'', '"']).next().unwrap_or("").trim().to_string()
}

fn font_family(name: &str) -> String {
    format!("'{}', 'Malgun Gothic', 'Apple SD Gothic Neo', sans-serif", name.replace('\'', ""))
}

/// 파일 이름만 (주소 끝)
fn file_label(url: &str) -> String {
    let u = percent_encoding::percent_decode_str(url).decode_utf8_lossy().to_string();
    u.rsplit(['/', '\\']).next().unwrap_or("").to_string()
}

/// 그림 썸네일 상자 (그림이 없으면 아이콘)
fn thumb(ui: &mut egui::Ui, pc: &crate::paint::Ctx, url: &str, size: Vec2, icon: &str) {
    let p = pal();
    let (r, _) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect(r, CornerRadius::same(6), p.sunken, Stroke::new(1.0, p.line), StrokeKind::Inside);
    if url.is_empty() || !crate::paint::draw_image_pub(ui, pc, r.shrink(1.0), url, false) {
        crate::icons::paint(ui, Rect::from_center_size(r.center(), Vec2::splat(22.0)), icon, p.muted2);
    }
}

/// 속성 칸 전체. focus 가 있으면 그 묶음만 펼치고 나머지는 접는다(한 번)
pub fn inspector(ui: &mut egui::Ui, t: &mut Value, focus: Option<Sec>, pc: &crate::paint::Ctx, data_dir: &Path, msg: &mut String) -> bool {
    let mut ch = false;
    for sec in Sec::ALL {
        let force = focus.map(|fs| fs == sec);
        let (head, body) = tc::section(ui, sec.id(), sec.icon(), sec.title(), force, |ui| match sec {
            Sec::Bg => background(ui, t, pc, data_dir, msg),
            Sec::Font => font(ui, t),
            Sec::Layout => layout(ui, t),
            Sec::Ref => reference(ui, t),
            Sec::Logo => logo(ui, t, pc, data_dir, msg),
            Sec::Effect => effects(ui, t),
            Sec::Adv => advanced(ui, t),
        });
        ch |= body.unwrap_or(false);
        if focus == Some(sec) {
            head.scroll_to_me(Some(egui::Align::TOP));
        }
    }
    ch
}

fn background(ui: &mut egui::Ui, t: &mut Value, pc: &crate::paint::Ctx, data_dir: &Path, msg: &mut String) -> bool {
    let mut ch = false;
    let ty0 = s(t, "background.type").to_string();
    let mut ty = if ty0.is_empty() { "color".to_string() } else { ty0.clone() };
    tc::row(ui, "종류", "", |ui| {
        ch |= tc::seg_widget(ui, &mut ty, &[("color", "단색"), ("gradient", "그라데이션"), ("image", "그림"), ("video", "영상"), ("transparent", "투명")]);
    });
    let ty0n = if ty0.is_empty() { "color" } else { ty0.as_str() };
    if ty != ty0n {
        set(t, "background.type", json!(ty));
        if ty == "transparent" {
            set(t, "clearHidesBackground", json!(true)); // Electron 과 같이: 투명이면 글자 숨김 때 배경도
        }
        if ty == "gradient" && s(t, "background.gradient").trim().is_empty() {
            let c1 = s(t, "background.color").to_string();
            let c1 = if c1.is_empty() || c1 == "#000000" { "#1b2a4a".to_string() } else { c1 };
            set(t, "background.gradient", json!(gradient_css(&c1, "#000000", "tb")));
        }
        ch = true;
    }
    match ty.as_str() {
        "color" => {
            ch |= tc::color(ui, t, "background.color", "색", &BG_COLORS, false);
        }
        "gradient" => {
            let css = s(t, "background.gradient").to_string();
            let (mut c1, mut c2, mut dir, simple) = parse_gradient(&css);
            if !simple {
                tc::note(ui, "테마에서 만든 꾸민 그라데이션입니다. 아래에서 바꾸면 두 색 그라데이션으로 바뀝니다 (원래 모양은 '고급'에 남아 있습니다).");
            }
            let mut g = json!({ "a": c1, "b": c2 });
            let mut gch = tc::color(ui, &mut g, "a", "시작 색", &["#1b2a4a", "#0b1d3a", "#7a0e1f", "#13261b", "#3a2a5a", "#000000"], false);
            gch |= tc::color(ui, &mut g, "b", "끝 색", &["#000000", "#1f1f1f", "#0b1d3a", "#2a1414", "#ffffff"], false);
            c1 = g["a"].as_str().unwrap_or("").to_string();
            c2 = g["b"].as_str().unwrap_or("").to_string();
            tc::row(ui, "방향", "", |ui| {
                if let Some(d) = grad_dirs(ui, &dir, &c1, &c2) {
                    dir = d;
                    gch = true;
                }
            });
            if gch {
                set(t, "background.gradient", json!(gradient_css(&c1, &c2, &dir)));
                set(t, "background.color", json!(c1));
                ch = true;
            }
        }
        "image" => {
            let url = s(t, "background.image").to_string();
            tc::row(ui, "그림", "", |ui| {
                thumb(ui, pc, &url, Vec2::new(96.0, 54.0), "image");
                ui.vertical(|ui| {
                    if ui.add(Btn::new(tr(if url.is_empty() { "그림 고르기…" } else { "다른 그림…" })).icon("folder-open").size(Size::Sm).tooltip(tr("png · jpg · webp 그림을 골라 배경으로 씁니다"))).clicked() {
                        if let Some(p) = rfd::FileDialog::new().add_filter(tr("그림"), &["png", "jpg", "jpeg", "webp", "gif", "bmp"]).pick_file() {
                            match import_media(data_dir, &p, "background") {
                                Ok(u) => {
                                    set(t, "background.image", json!(u));
                                    ch = true;
                                }
                                Err(e) => *msg = tr_err("그림을 가져오지 못했습니다", &e),
                            }
                        }
                    }
                    if !url.is_empty() {
                        ui.label(theme::rich(file_label(&url), 11.0, Weight::Regular, pal().muted2));
                    }
                });
            });
            if url.is_empty() {
                tc::note(ui, "아직 그림이 없습니다. '그림 고르기…'를 누르세요.");
            }
            ch |= tc::seg(ui, t, "background.imageFit", "맞춤", &[("cover", "화면 채우기"), ("contain", "전체 보이기")], "cover");
            ch |= tc::slider(ui, t, "background.dim", "어둡게", "그림 위에 검은 막을 덮어 글자가 잘 보이게", 0.0, 0.85, 0.05, 0.0, 100.0, "%");
        }
        "video" => {
            let url = s(t, "background.video").to_string();
            tc::row(ui, "영상", "", |ui| {
                thumb(ui, pc, "", Vec2::new(96.0, 54.0), "clapperboard");
                ui.vertical(|ui| {
                    if ui.add(Btn::new(tr(if url.is_empty() { "영상 고르기…" } else { "다른 영상…" })).icon("folder-open").size(Size::Sm).tooltip(tr("mp4 · webm 영상을 골라 소리 없이 반복 재생합니다"))).clicked() {
                        if let Some(p) = rfd::FileDialog::new().add_filter(tr("영상"), &["mp4", "m4v", "webm", "mov"]).pick_file() {
                            match import_media(data_dir, &p, "background") {
                                Ok(u) => {
                                    set(t, "background.video", json!(u));
                                    ch = true;
                                }
                                Err(e) => *msg = tr_err("영상을 가져오지 못했습니다", &e),
                            }
                        }
                    }
                    ui.label(theme::rich(if url.is_empty() { tr("아직 영상이 없습니다") } else { file_label(&url) }, 11.0, Weight::Regular, pal().muted2));
                });
            });
            tc::note(ui, "영상은 예배당 화면·OBS 에서만 움직입니다. 이 편집 화면은 색으로 대신 보여 줍니다.");
            ch |= tc::slider(ui, t, "background.dim", "어둡게", "영상 위에 검은 막을 덮어 글자가 잘 보이게", 0.0, 0.85, 0.05, 0.0, 100.0, "%");
        }
        "transparent" => {
            tc::note(ui, "배경 없이 글자만 내보냅니다. OBS 에서 카메라 화면 위에 자막으로 겹칠 때 씁니다.");
        }
        "camera" => {
            tc::note(ui, "카메라 화면을 배경으로 씁니다 (카메라 고르기는 '빠른 설정'에서).");
            ch |= tc::slider(ui, t, "background.dim", "어둡게", "", 0.0, 0.85, 0.05, 0.0, 100.0, "%");
        }
        _ => {}
    }
    ch |= tc::switch(ui, t, "clearHidesBackground", "숨김 때 배경도", "'글자 숨김'(F10)을 누르면 배경까지 함께 숨깁니다");
    ch
}

fn tr_err(what: &str, e: &str) -> String {
    format!("{}: {}", tr(what), e)
}

/// 그라데이션 방향 단추 넷 (작은 견본 그림)
fn grad_dirs(ui: &mut egui::Ui, cur: &str, c1: &str, c2: &str) -> Option<String> {
    let p = pal();
    let a = crate::paint::parse_color(c1).unwrap_or(Color32::BLACK);
    let bcol = crate::paint::parse_color(c2).unwrap_or(Color32::BLACK);
    let mut out = None;
    ui.spacing_mut().item_spacing.x = 5.0;
    for (v, name) in GRAD_DIRS {
        let (r, resp) = ui.allocate_exact_size(Vec2::new(42.0, 28.0), Sense::click());
        let inner = r.shrink(3.0);
        let mut mesh = egui::Mesh::default();
        let n = 8;
        for yi in 0..=n {
            for xi in 0..=n {
                let fx = xi as f32 / n as f32;
                let fy = yi as f32 / n as f32;
                let k = match v {
                    "lr" => fx,
                    "diag" => (fx + fy) / 2.0,
                    "radial" => (((fx - 0.5).powi(2) + (fy - 0.5).powi(2)).sqrt() / 0.7071).min(1.0),
                    _ => fy,
                };
                let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * k) as u8;
                mesh.colored_vertex(Pos2::new(inner.left() + inner.width() * fx, inner.top() + inner.height() * fy), Color32::from_rgb(m(a.r(), bcol.r()), m(a.g(), bcol.g()), m(a.b(), bcol.b())));
            }
        }
        for yi in 0..n {
            for xi in 0..n {
                let i = (yi * (n + 1) + xi) as u32;
                let w = (n + 1) as u32;
                mesh.add_triangle(i, i + 1, i + w + 1);
                mesh.add_triangle(i, i + w + 1, i + w);
            }
        }
        ui.painter().add(mesh);
        let on = v == cur;
        ui.painter().rect_stroke(r, CornerRadius::same(5), Stroke::new(if on { 2.0 } else { 1.0 }, if on { p.accent } else if resp.hovered() { p.line2 } else { p.line }), StrokeKind::Inside);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.on_hover_text(tr(name)).clicked() && !on {
            out = Some(v.to_string());
        }
    }
    out
}

fn font(ui: &mut egui::Ui, t: &mut Value) -> bool {
    let mut ch = false;
    let cur = font_name(s(t, "font.family"));
    tc::row(ui, "글꼴", "", |ui| {
        egui::ComboBox::from_id_salt("tpl-font").width(ui.available_width() - 8.0).selected_text(&cur).show_ui(ui, |ui| {
            let mut names: Vec<&str> = BASE_FONTS.to_vec();
            if !cur.is_empty() && !names.iter().any(|n| *n == cur) {
                names.insert(0, &cur);
            }
            for n in names {
                if ui.selectable_label(n == cur, n).clicked() && n != cur {
                    set(t, "font.family", json!(font_family(n)));
                    ch = true;
                }
            }
        });
    });
    let before = f(t, "font.size", 72.0);
    if tc::slider(ui, t, "font.size", "크기", "본문 글자 크기", 20.0, 200.0, 2.0, 72.0, 1.0, "") {
        ch = true;
        let sz = f(t, "font.size", before);
        if f(t, "font.minSize", 44.0) > sz {
            set(t, "font.minSize", json!(sz as i64));
        }
    }
    ch |= tc::seg(ui, t, "font.weight", "굵기", &[("400", "보통"), ("500", "중간"), ("700", "굵게")], "500");
    ch |= tc::color(ui, t, "font.color", "색", &TEXT_COLORS, false);
    ch |= tc::slider(ui, t, "font.lineHeight", "줄 간격", "줄과 줄 사이 (글자 크기의 몇 배)", 1.0, 2.2, 0.05, 1.5, 1.0, "배");
    ch |= tc::slider(ui, t, "font.letterSpacing", "글자 간격", "글자와 글자 사이", -5.0, 20.0, 0.5, 0.0, 1.0, "");
    ch |= tc::switch(ui, t, "font.autoFit", "자동 줄이기", "글이 길어 화면을 넘치면 글자를 작게 합니다");
    if b(t, "font.autoFit") {
        let mx = f(t, "font.size", 72.0);
        ch |= tc::slider(ui, t, "font.minSize", "가장 작게", "자동으로 줄일 때 이보다 작게는 하지 않습니다", 8.0, mx.max(8.0), 1.0, 44.0, 1.0, "");
    }
    ch
}

/// 화면 비율 → (가로, 세로) px. 4:3 은 옛 프로젝터 (예배송출의 '4:3 프로젝터' 를 참고해 더함, 2026-11-27)
pub const ASPECTS: [(&str, &str, f64, f64); 2] = [("16:9", "16:9 (보통 TV)", 1920.0, 1080.0), ("4:3", "4:3 (옛 프로젝터)", 1440.0, 1080.0)];

/// 지금 템플릿 크기가 어느 비율인가 (그 밖의 크기는 "")
pub fn aspect_of(w: f64, h: f64) -> &'static str {
    ASPECTS.iter().find(|a| ((w / h.max(1.0)) - a.2 / a.3).abs() < 0.02).map(|a| a.0).unwrap_or("")
}

fn layout(ui: &mut egui::Ui, t: &mut Value) -> bool {
    let mut ch = false;
    // 화면 비율: 예배당 프로젝터가 4:3 이면 4:3 으로 (글이 위아래 검은 띠 없이 화면을 다 씀)
    let mut asp = aspect_of(f(t, "width", 1920.0), f(t, "height", 1080.0)).to_string();
    let items: Vec<(&str, &str)> = ASPECTS.iter().map(|a| (a.0, a.1)).collect();
    let changed = tc::row(ui, "화면 비율", "예배당 TV·프로젝터의 모양. 옛 프로젝터(1024×768 등)는 4:3", |ui| tc::seg_widget(ui, &mut asp, &items));
    if changed {
        if let Some(a) = ASPECTS.iter().find(|a| a.0 == asp) {
            set(t, "width", json!(a.2 as i64));
            set(t, "height", json!(a.3 as i64));
            ch = true;
        }
    }
    let al = s(t, "align").to_string();
    let al = if al.is_empty() { "center".to_string() } else { al };
    tc::row(ui, "정렬", "", |ui| {
        if let Some(a) = tc::align_seg(ui, "body", &al, &["left", "center", "right", "hanging"]) {
            set(t, "align", json!(a));
            if a == "hanging" {
                set(t, "reference.align", json!("left"));
                set(t, "verseNumbers.show", json!(true));
            } else {
                set(t, "reference.align", json!(a));
            }
            ch = true;
        }
    });
    let va = s(t, "valign").to_string();
    let col = if al == "hanging" { "left".to_string() } else { al.clone() };
    let cur = format!("{}|{}", if va.is_empty() { "middle" } else { &va }, col);
    tc::row(ui, "놓을 자리", "본문이 화면 어디에 놓일지", |ui| {
        let cells: [[(&str, &str); 3]; 3] = [
            [("top|left", "위 · 왼쪽"), ("top|center", "위 · 가운데"), ("top|right", "위 · 오른쪽")],
            [("middle|left", "가운데 · 왼쪽"), ("middle|center", "한가운데"), ("middle|right", "가운데 · 오른쪽")],
            [("bottom|left", "아래 · 왼쪽"), ("bottom|center", "아래 · 가운데"), ("bottom|right", "아래 · 오른쪽")],
        ];
        let rows: Vec<&[(&str, &str)]> = cells.iter().map(|r| &r[..]).collect();
        if let Some(v) = tc::pos_grid(ui, "body-pos", &rows, &cur) {
            let (v, a) = v.split_once('|').unwrap_or(("middle", "center"));
            set(t, "valign", json!(v));
            // 내어쓰기 중에 왼쪽 칸을 누르면 내어쓰기 유지 (tplcontrols.js 와 같음)
            let na = if al == "hanging" && a == "left" { "hanging" } else { a };
            set(t, "align", json!(na));
            set(t, "reference.align", json!(a));
            ch = true;
        }
        ui.vertical(|ui| {
            tc::note(ui, "칸을 누르면 본문이 그 자리로 갑니다.");
        });
    });
    let (w, h) = (f(t, "width", 1920.0), f(t, "height", 1080.0));
    ui.add_space(4.0);
    tc::note(ui, "여백: 화면 가장자리에서 글까지의 거리. 캔버스의 점선 테두리를 끌어도 됩니다.");
    ch |= tc::slider(ui, t, "padding.top", "위 여백", "", 0.0, (h * 0.8).round(), 10.0, 120.0, 1.0, "");
    ch |= tc::slider(ui, t, "padding.bottom", "아래 여백", "", 0.0, (h * 0.8).round(), 10.0, 120.0, 1.0, "");
    ch |= tc::slider(ui, t, "padding.left", "왼쪽 여백", "", 0.0, (w * 0.45).round(), 10.0, 160.0, 1.0, "");
    ch |= tc::slider(ui, t, "padding.right", "오른쪽 여백", "", 0.0, (w * 0.45).round(), 10.0, 160.0, 1.0, "");
    ch |= tc::slider(ui, t, "safeArea", "안전 영역", "TV·프로젝터가 가장자리를 잘라 먹을 때 네 변을 화면의 몇 %만큼 더 띄웁니다", 0.0, 20.0, 0.5, 0.0, 1.0, "%");
    ch
}

fn reference(ui: &mut egui::Ui, t: &mut Value) -> bool {
    let mut ch = false;
    ch |= tc::switch(ui, t, "reference.show", "장절 보이기", "요한복음 3:16 같은 장절 줄");
    if b(t, "reference.show") {
        ch |= tc::seg(ui, t, "reference.position", "자리", &[("top", "본문 위"), ("bottom", "본문 아래"), ("fixed-top", "화면 위"), ("fixed-bottom", "화면 아래")], "top");
        if s(t, "reference.position").starts_with("fixed") {
            let h = f(t, "height", 1080.0);
            ch |= tc::slider(ui, t, "reference.offset", "가장자리 거리", "화면 위·아래 끝에서 장절 줄까지", 0.0, (h * 0.4).round(), 5.0, 60.0, 1.0, "");
        }
        ch |= tc::seg(ui, t, "reference.mode", "내용", &[("slide", "슬라이드마다"), ("passage", "전체 범위")], "slide");
        ch |= tc::slider(ui, t, "reference.size", "크기", "", 14.0, 120.0, 2.0, 44.0, 1.0, "");
        ch |= tc::color(ui, t, "reference.color", "색", &REF_COLORS, false);
        ch |= tc::seg(ui, t, "reference.weight", "굵기", &[("400", "보통"), ("700", "굵게")], "700");
        let ra = s(t, "reference.align").to_string();
        tc::row(ui, "정렬", "", |ui| {
            if let Some(a) = tc::align_seg(ui, "ref", if ra.is_empty() { "center" } else { &ra }, &["left", "center", "right"]) {
                set(t, "reference.align", json!(a));
                ch = true;
            }
        });
    }
    ui.add_space(4.0);
    ch |= tc::switch(ui, t, "verseNumbers.show", "절 번호", "본문 앞 작은 절 번호");
    if b(t, "verseNumbers.show") {
        ch |= tc::color(ui, t, "verseNumbers.color", "번호 색", &REF_COLORS, false);
        ch |= tc::slider(ui, t, "verseNumbers.scale", "번호 크기", "본문 글자의 몇 배", 0.3, 1.2, 0.05, 0.6, 1.0, "배");
    }
    let hanging = s(t, "align") == "hanging";
    if hanging {
        tc::row(ui, "절마다 줄바꿈", "", |ui| tc::note(ui, "내어쓰기에서는 늘 켜져 있습니다"));
    } else {
        ch |= tc::switch(ui, t, "verseBreak", "절마다 줄바꿈", "절이 바뀔 때마다 새 줄에서 시작");
    }
    ui.add_space(4.0);
    ch |= tc::switch(ui, t, "versionLabel.show", "번역본 이름", "본문 아래에 '개역개정' 같은 번역본 이름");
    if b(t, "versionLabel.show") {
        ch |= tc::slider(ui, t, "versionLabel.size", "이름 크기", "", 8.0, 120.0, 1.0, 28.0, 1.0, "");
        ch |= tc::color(ui, t, "versionLabel.color", "이름 색", &["#bbbbbb", "#ffffff", "#ffd479", "#9ec5ff", "#000000"], false);
    }
    ch |= tc::switch(ui, t, "headings.show", "소제목", "성경의 단락 소제목 (번역본에 있을 때)");
    if b(t, "headings.show") {
        ch |= tc::slider(ui, t, "headings.size", "소제목 크기", "", 8.0, 120.0, 1.0, 36.0, 1.0, "");
        ch |= tc::color(ui, t, "headings.color", "소제목 색", &["#cccccc", "#ffffff", "#ffd479", "#9ec5ff", "#000000"], false);
    }
    ch
}

fn logo(ui: &mut egui::Ui, t: &mut Value, pc: &crate::paint::Ctx, data_dir: &Path, msg: &mut String) -> bool {
    let mut ch = false;
    let show = b(t, "logo.show");
    let img = s(t, "logo.image").to_string();
    let cur = if show && !img.is_empty() { if LOGOS.iter().any(|l| l.0 == img) { img.clone() } else { "custom".to_string() } } else { String::new() };
    let mut pick = cur.clone();
    tc::row(ui, "로고", "", |ui| {
        tc::seg_widget(ui, &mut pick, &[("", "없음"), (LOGOS[0].0, LOGOS[0].1), (LOGOS[1].0, LOGOS[1].1), ("custom", "내 로고")]);
    });
    let defaults = |t: &mut Value| {
        for (k, v) in [("position", json!("top-right")), ("size", json!(120)), ("margin", json!(40)), ("opacity", json!(1))] {
            if t["logo"][k].is_null() {
                t["logo"][k] = v;
            }
        }
    };
    if pick != cur {
        match pick.as_str() {
            "" => set(t, "logo.show", json!(false)),
            "custom" => {
                if let Some(p) = rfd::FileDialog::new().add_filter(tr("그림"), &["png", "jpg", "jpeg", "webp", "gif", "svg"]).pick_file() {
                    match import_media(data_dir, &p, "logo") {
                        Ok(u) => {
                            defaults(t);
                            set(t, "logo.image", json!(u));
                            set(t, "logo.show", json!(true));
                        }
                        Err(e) => *msg = tr_err("로고를 가져오지 못했습니다", &e),
                    }
                }
            }
            u => {
                defaults(t);
                set(t, "logo.image", json!(u));
                set(t, "logo.show", json!(true));
            }
        }
        ch = true;
    }
    if !(b(t, "logo.show") && !s(t, "logo.image").is_empty()) {
        tc::note(ui, "교회 로고를 화면 모서리에 늘 띄웁니다. 투명 배경 png 를 권합니다.");
        return ch;
    }
    let img = s(t, "logo.image").to_string();
    tc::row(ui, "그림", "", |ui| {
        thumb(ui, pc, &img, Vec2::new(54.0, 54.0), "flag");
        if cur == "custom" && ui.add(Btn::new(tr("다른 그림…")).icon("folder-open").size(Size::Sm).tooltip(tr("다른 로고 그림 고르기"))).clicked() {
            if let Some(p) = rfd::FileDialog::new().add_filter(tr("그림"), &["png", "jpg", "jpeg", "webp", "gif", "svg"]).pick_file() {
                match import_media(data_dir, &p, "logo") {
                    Ok(u) => {
                        set(t, "logo.image", json!(u));
                        ch = true;
                    }
                    Err(e) => *msg = tr_err("로고를 가져오지 못했습니다", &e),
                }
            }
        }
    });
    let lp = s(t, "logo.position").to_string();
    tc::row(ui, "자리", "", |ui| {
        let cells: [[(&str, &str); 3]; 2] = [
            [("top-left", "왼쪽 위"), ("top-center", "가운데 위"), ("top-right", "오른쪽 위")],
            [("bottom-left", "왼쪽 아래"), ("bottom-center", "가운데 아래"), ("bottom-right", "오른쪽 아래")],
        ];
        let rows: Vec<&[(&str, &str)]> = cells.iter().map(|r| &r[..]).collect();
        if let Some(v) = tc::pos_grid(ui, "logo-pos", &rows, if lp.is_empty() { "top-right" } else { &lp }) {
            set(t, "logo.position", json!(v));
            ch = true;
        }
        tc::note(ui, "캔버스에서 로고를 끌어도 됩니다.");
    });
    ch |= tc::slider(ui, t, "logo.size", "크기", "", 40.0, 400.0, 5.0, 120.0, 1.0, "");
    ch |= tc::slider(ui, t, "logo.opacity", "진하기", "", 0.1, 1.0, 0.05, 1.0, 100.0, "%");
    ch |= tc::slider(ui, t, "logo.margin", "가장자리 거리", "", 0.0, 200.0, 5.0, 40.0, 1.0, "");
    ch
}

fn effects(ui: &mut egui::Ui, t: &mut Value) -> bool {
    let mut ch = false;
    ch |= tc::slider(ui, t, "stroke.width", "외곽선", "글자 둘레 선의 두께 (0 이면 없음)", 0.0, 12.0, 0.5, 0.0, 1.0, "");
    if f(t, "stroke.width", 0.0) > 0.0 {
        ch |= tc::color(ui, t, "stroke.color", "외곽선 색", &["#000000", "#ffffff", "#0b1d3a", "#7a0e1f"], false);
    }
    ui.add_space(4.0);
    ch |= tc::switch(ui, t, "shadow.enabled", "그림자", "글자 뒤 그림자 (밝은 배경에서 잘 읽히게)");
    if b(t, "shadow.enabled") {
        ch |= tc::slider(ui, t, "shadow.blur", "번짐", "", 0.0, 40.0, 1.0, 8.0, 1.0, "");
        ch |= tc::slider(ui, t, "shadow.x", "가로 거리", "", -30.0, 30.0, 1.0, 0.0, 1.0, "");
        ch |= tc::slider(ui, t, "shadow.y", "세로 거리", "", -30.0, 30.0, 1.0, 3.0, 1.0, "");
        ch |= tc::color(ui, t, "shadow.color", "그림자 색", &["#000000", "#0b1d3a", "#ffffff"], true);
    }
    ui.add_space(4.0);
    ch |= tc::switch(ui, t, "box.enabled", "글 상자", "본문 뒤에 반투명 상자를 깝니다 (OBS 자막에 좋음)");
    if b(t, "box.enabled") {
        let had_grad = !s(t, "box.gradient").trim().is_empty();
        if had_grad {
            tc::note(ui, "테마의 그라데이션 상자입니다. 색을 고르면 그 색 하나로 바뀝니다.");
        }
        if tc::color(ui, t, "box.color", "상자 색", &BOX_COLORS, false) {
            if had_grad {
                set(t, "box.gradient", json!(""));
            }
            ch = true;
        }
        ch |= tc::slider(ui, t, "box.opacity", "진하기", "", 0.0, 1.0, 0.05, 0.6, 100.0, "%");
        ch |= tc::slider(ui, t, "box.radius", "모서리", "모서리 둥글기", 0.0, 80.0, 1.0, 16.0, 1.0, "");
        ch |= tc::slider(ui, t, "box.padding", "안 여백", "상자 테두리와 글 사이", 0.0, 120.0, 2.0, 32.0, 1.0, "");
    }
    ui.add_space(4.0);
    let anim = s(t, "animation.type").to_string();
    let mut a2 = if anim == "dissolve" || anim == "fade" || anim.is_empty() { "dissolve".to_string() } else { "cut".to_string() };
    let a0 = a2.clone();
    tc::row(ui, "화면 전환", "슬라이드가 바뀌는 방식", |ui| {
        tc::seg_widget(ui, &mut a2, &[("dissolve", "서서히 (디졸브)"), ("cut", "바로 (컷)")]);
    });
    if a2 != a0 {
        set(t, "animation.type", json!(a2));
        ch = true;
    }
    if a2 == "dissolve" {
        ch |= tc::slider(ui, t, "animation.duration", "전환 시간", "", 100.0, 2000.0, 50.0, 400.0, 0.001, "초");
    }
    let ppt0 = s(t, "pptMode").to_string();
    let mut ppt = if ppt0.is_empty() { if s(t, "background.type") == "transparent" { "lyrics".to_string() } else { "image".to_string() } } else { ppt0.clone() };
    let p_before = ppt.clone();
    tc::row(ui, "PPT·찬송가", "PPT·찬송가 슬라이드를 어떻게 보일지", |ui| {
        tc::seg_widget(ui, &mut ppt, &[("image", "그림 그대로"), ("lyrics", "가사 글자만")]);
    });
    if ppt != p_before {
        set(t, "pptMode", json!(ppt));
        ch = true;
    }
    ch
}

fn advanced(ui: &mut egui::Ui, t0: &mut Value) -> bool {
    // form:: 칸은 없는 열쇠를 null 로 만들어 넣으므로 사본에서 고치고, 바뀐 때만 되돌려 쓴다
    let mut copy = t0.clone();
    let t = &mut copy;
    let mut ch = false;
    tc::note(ui, "직접 값을 적는 칸입니다. 보통은 위의 묶음으로 충분합니다.");
    egui::Grid::new("tpl-adv").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
        ch |= form::num(ui, t, &["width"], "화면 가로(px)", 320.0, 7680.0, 1.0);
        ui.end_row();
        ch |= form::num(ui, t, &["height"], "화면 세로(px)", 180.0, 4320.0, 1.0);
        ui.end_row();
        ch |= form::pick(ui, t, &["background", "type"], "배경 종류", &[("color", "단색"), ("gradient", "그라데이션"), ("image", "그림"), ("video", "영상"), ("camera", "카메라"), ("transparent", "투명 (OBS)")]);
        ui.end_row();
        ch |= form::text(ui, t, &["background", "gradient"], "그라데이션 CSS", 190.0);
        ui.end_row();
        ch |= form::text(ui, t, &["background", "image"], "배경 그림 주소", 190.0);
        ui.end_row();
        ch |= form::text(ui, t, &["background", "video"], "배경 영상 주소", 190.0);
        ui.end_row();
        ch |= form::text(ui, t, &["font", "family"], "글꼴 CSS", 190.0);
        ui.end_row();
        ch |= form::num(ui, t, &["font", "weight"], "굵기 (100~900)", 100.0, 900.0, 100.0);
        ui.end_row();
        ch |= form::text(ui, t, &["logo", "image"], "로고 그림 주소", 190.0);
        ui.end_row();
        ch |= form::text(ui, t, &["box", "gradient"], "상자 그라데이션 CSS", 190.0);
        ui.end_row();
        ch |= form::text(ui, t, &["shadow", "color"], "그림자 색 CSS", 190.0);
        ui.end_row();
        ch |= form::num(ui, t, &["animation", "duration"], "전환 시간(ms)", 0.0, 5000.0, 10.0);
        ui.end_row();
    });
    if ch {
        *t0 = copy;
    }
    ch
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gradient_roundtrip() {
        for d in ["tb", "lr", "diag", "radial"] {
            let css = gradient_css("#112233", "#000000", d);
            let (a, b, dir, simple) = parse_gradient(&css);
            assert_eq!((a.as_str(), b.as_str(), dir.as_str(), simple), ("#112233", "#000000", d, true), "{css}");
        }
        // 테마의 꾸민 그라데이션은 '단순하지 않음'
        let (_, _, _, simple) = parse_gradient("radial-gradient(ellipse at 50% 35%, #1b2a4a 0%, #000 70%), linear-gradient(to top, #D8102F 0 14px, transparent 14px)");
        assert!(!simple);
    }

    #[test]
    fn media_copy() {
        let dir = std::env::temp_dir().join(format!("wp-tpl-media-{}", std::process::id()));
        let src = dir.join("a.PNG");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&src, b"x").unwrap();
        let u = import_media(&dir, &src, "logo").unwrap();
        assert!(u.starts_with("/media/logo-") && u.ends_with(".png"));
        assert!(dir.join("media").join(u.trim_start_matches("/media/")).is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
