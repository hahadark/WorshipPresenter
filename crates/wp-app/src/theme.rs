//! 조작 화면 테마 — Electron 판 `control.css` 의 색 변수(:root · html[data-theme=…])를 그대로 옮긴 것.
//!
//! 쓰는 법 (다음 작업자용):
//! - `theme::pal()` → 지금 테마의 [`Palette`] (`&'static`, 프레임마다 불러도 됨). 이름은 css 변수와 같다
//!   (`--line-soft` → `line_soft`, `--accent-strong` → `accent_strong`, color-mix 파생색 `pv_bg` …).
//! - `theme::font(14.0, Weight::SemiBold)` → 굵기별 Pretendard [`FontId`]. `theme::rich(…)` 는 RichText 한 번에.
//! - `theme::slide_family(bold)` → 미리보기·LIVE 슬라이드 그림용(맑은 고딕 먼저, 송출 화면 기본 글꼴과 같게).
//! - 수치: [`CTL_H`](34, 기본 단추·입력칸 높이) · [`RADIUS`](6) · [`RADIUS_LG`](10) · [`CARD_RADIUS`](12, 독 패널 카드).
//! - 순서 종류 색: [`type_color`] (css `--tc`) · [`type_ink`] (아이콘 색, 어두운 테마는 흰색과 섞음) · [`type_chip_bg`].
//! - 테마 바꾸기는 설정 `ui.theme` → `settings_ui::apply_ui_prefs` → [`apply`] 가 한다.
#![allow(dead_code)] // 다음 단계 작업자가 쓸 공개 API (아직 안 쓰는 것이 있음)

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Margin, RichText, Shadow, Stroke, TextStyle, Vec2};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

/// 기본 단추·입력칸 높이 (css `--ctl-h`) — 옛 테마(어둡게 · 밝게 · 고대비 · 따뜻하게 · 푸른 밤)
pub const CTL_H: f32 = 34.0;
/// 지금 테마의 보통 조절 칸 높이 = [`ctl`]`(Ctl::Md)`
pub fn ctl_h() -> f32 {
    ctl(Ctl::Md)
}

// ================================================================ 디자인 체계 토큰 (2026-11-28 craft-main)
// 규칙 · 견본 그림: docs/design-system.md · 견본 창: `wp-app --specimen` (specimen.rs)
// 숫자를 화면 코드에 직접 쓰지 말고 여기 이름으로: 간격 `sp::X8` · 높이 `ctl(Ctl::Md)` · 글자 `Type::Body.font()` ·
// 둥글기 `cr(Rad::Md)` · 선 `LINE` · 색 `pal().…` · 움직임 `anim_bool(…, T_FAST)`.

/// 간격 눈금 — 8 아래는 2px, 그 위는 4px 격자. 여백 · 틈은 이 값만 쓴다.
/// X2 붙은 것(아이콘과 글) · X4 한 묶음 안 · X6 줄 사이 · X8 조절 칸 사이 · X12 판 안쪽 여백 · X16 구획 사이 · X24 큰 묶음
pub mod sp {
    pub const X2: f32 = 2.0;
    pub const X4: f32 = 4.0;
    pub const X6: f32 = 6.0;
    pub const X8: f32 = 8.0;
    pub const X12: f32 = 12.0;
    pub const X16: f32 = 16.0;
    pub const X20: f32 = 20.0;
    pub const X24: f32 = 24.0;
    pub const X32: f32 = 32.0;
    /// 판 안쪽 좌우 여백 (판 머리 글 · 표 첫 칸 · 인스펙터 줄이 같은 세로선에서 시작)
    pub const PANE: f32 = 12.0;
}

/// 조절 칸 높이 단계
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ctl {
    /// 작게 22: 판 머리 안 구획 단추 · 인스펙터의 촘촘한 칸 · 꼬리표
    Sm,
    /// 보통 26: 단추 · 입력칸 · 팝업 단추 (기본)
    Md,
    /// 크게 32: 컨트롤 막대의 이전 · 송출 · 다음
    Lg,
}

/// 조절 칸 높이 (지금 테마). 옛 테마는 24 · 34 · 40
pub fn ctl(k: Ctl) -> f32 {
    match (k, pal().bevel) {
        (Ctl::Sm, true) => 22.0,
        (Ctl::Md, true) => 26.0,
        (Ctl::Lg, true) => 32.0,
        (Ctl::Sm, false) => 24.0,
        (Ctl::Md, false) => CTL_H,
        (Ctl::Lg, false) => 40.0,
    }
}

/// 위 컨트롤 막대 높이 (12 + 32 + 12)
pub const BAR_H: f32 = 56.0;
/// 판 머리 띠 높이
pub const HEAD_H: f32 = 28.0;
/// 표 · 목록 줄 높이
pub const ROW_H: f32 = 28.0;
/// 접는 구획 머리 높이
pub const SECTION_H: f32 = 24.0;
/// 아래 상태 줄 높이
pub const STATUS_H: f32 = 24.0;

/// 선 굵기: 가는 선 (구분선 · 테두리)
pub const LINE: f32 = 1.0;
/// 선 굵기: 강조 (다음 뷰어 테두리 · 초점 고리 · 끼움 선)
pub const LINE_EMPH: f32 = 2.0;
/// 선 굵기: 송출 중 뷰어 테두리 (화면에서 가장 굵은 선은 이것 하나)
pub const LINE_LIVE: f32 = 3.0;

/// 글자 크기 단계 — 크기 다섯(10.5 · 11.5 · 12.5 · 14 · 20)에 굵기를 얹은 여덟 가지. 이 밖의 크기를 쓰지 않는다
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Type {
    /// 10.5 Medium · 줄 14 — 아주 작은 이름표: 표시창 이름표 · 시간표 눈금 · 단축키 칩 · 슬라이드 번호
    Micro,
    /// 11.5 Regular · 줄 16 — 양식 이름표 · 부가 글 · 표의 둘째 글 · 상태 줄
    Small,
    /// 11.5 SemiBold · 줄 16 — 구획 머리 · 표 머리 · 뷰어 이름표('다음' · '송출 중')
    SmallStrong,
    /// 12.5 Regular · 줄 18 — 본문: 단추 · 표의 첫 글 · 입력칸 · 메뉴
    Body,
    /// 12.5 SemiBold · 줄 18 — 판 머리 제목 · 고른 줄 · 주된 단추 · 표시창의 순서 이름
    BodyStrong,
    /// 14 SemiBold · 줄 20 — 판 안의 제목 하나 (지금 고른 순서 이름) · 창 제목
    Title,
    /// 14 Regular · 줄 22 — 읽는 글: 가사 · 성경 본문 · 공지 내용 (본문 목록과 긴 입력칸)
    Read,
    /// 20 SemiBold · 줄 24 · 숫자 폭 고정 — 표시창의 시각
    Display,
}

impl Type {
    pub const ALL: [Type; 8] = [Type::Micro, Type::Small, Type::SmallStrong, Type::Body, Type::BodyStrong, Type::Title, Type::Read, Type::Display];
    pub fn size(self) -> f32 {
        match self {
            Type::Micro => 10.5,
            Type::Small | Type::SmallStrong => 11.5,
            Type::Body | Type::BodyStrong => 12.5,
            Type::Title | Type::Read => 14.0,
            Type::Display => 20.0,
        }
    }
    pub fn weight(self) -> Weight {
        match self {
            Type::Micro => Weight::Medium,
            Type::Small | Type::Body | Type::Read => Weight::Regular,
            Type::SmallStrong | Type::BodyStrong | Type::Title | Type::Display => Weight::SemiBold,
        }
    }
    /// 줄 높이 (여러 줄 글의 줄 사이 · 한 줄 글이 차지하는 높이)
    pub fn line(self) -> f32 {
        match self {
            Type::Micro => 14.0,
            Type::Small | Type::SmallStrong => 16.0,
            Type::Body | Type::BodyStrong => 18.0,
            Type::Title => 20.0,
            Type::Read => 22.0,
            Type::Display => 24.0,
        }
    }
    pub fn font(self) -> FontId {
        font(self.size(), self.weight())
    }
    /// 견본 창 · 문서에 쓰는 이름
    pub fn name(self) -> &'static str {
        match self {
            Type::Micro => "Micro",
            Type::Small => "Small",
            Type::SmallStrong => "SmallStrong",
            Type::Body => "Body",
            Type::BodyStrong => "BodyStrong",
            Type::Title => "Title",
            Type::Read => "Read",
            Type::Display => "Display",
        }
    }
}

/// 글자 단계로 RichText (`ui.label(theme::ty("글", Type::Small, pal().muted))`)
pub fn ty(text: impl Into<String>, t: Type, color: Color32) -> RichText {
    RichText::new(text).font(t.font()).color(color)
}

// ---------------------------------------------------------------- 움직임

/// 올림 · 눌림 (초)
pub const T_FAST: f32 = 0.09;
/// 고름 이동 · 접기/펴기 (초)
pub const T_MED: f32 = 0.13;

/// 설정 '움직임 줄이기' (`ui.reduceMotion`, 기본 꺼짐)
static REDUCE_MOTION: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn reduce_motion() -> bool {
    REDUCE_MOTION.load(Ordering::Relaxed)
}

/// 움직임 줄이기를 켜고 끈다 (egui 기본 움직임 — 접기 · 스크롤 · 팝업 — 도 같이). 같은 값이면 아무것도 안 한다
pub fn set_reduce_motion(ctx: &egui::Context, on: bool) {
    let want = if on { 0.0 } else { T_MED };
    if REDUCE_MOTION.swap(on, Ordering::Relaxed) == on && (ctx.global_style().animation_time - want).abs() < 0.001 {
        return;
    }
    ctx.all_styles_mut(|s| s.animation_time = want);
}

fn ease_out(t: f32) -> f32 {
    let u = 1.0 - t.clamp(0.0, 1.0);
    1.0 - u * u * u
}

/// 켜짐/꺼짐을 0~1 로 부드럽게 (올림 · 눌림 · 고름). 움직이는 동안에만 다시 그리기를 부른다 — 가만히 있으면 0번.
/// 움직임 줄이기면 바로 0 또는 1
pub fn anim_bool(ctx: &egui::Context, id: egui::Id, on: bool, secs: f32) -> f32 {
    if reduce_motion() {
        return if on { 1.0 } else { 0.0 };
    }
    ease_out(ctx.animate_bool_with_time(id, on, secs))
}

/// 값을 목표로 부드럽게 (구획 단추의 고른 칸 자리 · 밑줄 자리). 움직임 줄이기면 바로 목표
pub fn anim_value(ctx: &egui::Context, id: egui::Id, target: f32, secs: f32) -> f32 {
    if reduce_motion() {
        return target;
    }
    ctx.animate_value_with_time(id, target, secs)
}

// ---------------------------------------------------------------- 상태별 색

/// 눌림: 바탕을 조금 어둡게 (어두운 테마 · 밝은 테마 모두 '들어간' 쪽)
pub fn press(c: Color32) -> Color32 {
    if pal().dark { mix(c, 84.0, Color32::BLACK) } else { mix(c, 86.0, Color32::BLACK) }
}

/// 두 색 사이 (t = 0 → a, 1 → b). 투명도도 같이 섞는다
pub fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_premultiplied(f(a.r(), b.r()), f(a.g(), b.g()), f(a.b(), b.b()), f(a.a(), b.a()))
}

/// 누를 수 있는 바탕의 상태 색: 보통 → 올림(hover 0~1) → 눌림(press 0~1)
pub fn state_fill(base: Color32, hover: f32, pressed: f32) -> Color32 {
    let c = lerp(base, lift(base), hover);
    lerp(c, press(base), pressed)
}

/// 꺼진 것의 글 · 아이콘 색 (바탕색과 섞어 흐리게 — 투명도를 쓰지 않아 겹쳐도 얼룩이 없다)
pub fn disabled(c: Color32, ground: Color32) -> Color32 {
    mix(c, 38.0, ground)
}

/// 둥글기 단계. 숫자를 직접 쓰지 말고 [`cr`] · [`radius`] 로 (테마마다 값이 다르다: 전문가용은 더 각지게)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rad {
    /// 2: 작은 막대 · 손잡이 · 진행 막대
    Xs,
    /// 조절 칸 안에 들여 그리는 것 (전문가 3): 구획 단추의 고른 칸 · 꼬리표 · 단축키 칩
    Sm,
    /// 조절 칸 (전문가 4 한 가지): 단추 · 입력칸 · 팝업 단추 · 구획 단추 바탕
    Md,
    /// 떠 있는 것 (전문가 6): 메뉴 · 툴팁 · 펼침 목록
    Lg,
    /// 창 (전문가 6)
    Xl,
}

/// 둥글기 숫자 (지금 테마)
pub fn radius(k: Rad) -> u8 {
    radius_for(pal().id, k)
}

/// 둥글기 숫자 (그 테마)
pub fn radius_for(id: ThemeId, k: Rad) -> u8 {
    let pro = id == ThemeId::Pro || id == ThemeId::ProLight;
    match (k, pro) {
        (Rad::Xs, _) => 2,
        // 전문가 테마: 판은 0(둥글리지 않음), 조절 칸은 4 한 가지. Sm 3 은 조절 칸 안에 1px 들여 그리는 것(고른 칸 · 꼬리표)의 같은 중심 둥글기,
        // 떠 있는 것(메뉴 · 툴팁 · 창)만 6
        (Rad::Sm, true) => 3,
        (Rad::Sm, false) => 4,
        (Rad::Md, true) => 4,
        (Rad::Md, false) => RADIUS,
        (Rad::Lg, true) => 6,
        (Rad::Lg, false) => 8,
        (Rad::Xl, true) => 6,
        (Rad::Xl, false) => CARD_RADIUS,
    }
}

/// 둥글기 (지금 테마)
pub fn cr(k: Rad) -> CornerRadius {
    CornerRadius::same(radius(k))
}

/// 계산한 둥글기 숫자 → CornerRadius
pub fn cr_n(n: u8) -> CornerRadius {
    CornerRadius::same(n)
}
/// 단추·입력칸 둥글기 (css `--radius`)
pub const RADIUS: u8 = 6;
/// 창·큰 상자 둥글기 (css `--radius-lg`)
pub const RADIUS_LG: u8 = 10;
/// 독 패널 카드 둥글기 (.wc-dock .dv-groupview)
pub const CARD_RADIUS: u8 = 12;
/// 기본 글자 크기 (html font-size)
pub const BASE: f32 = 14.0;

/// 조작 화면 테마 (설정 `ui.theme` 값)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeId {
    Dark = 0,
    Light = 1,
    Contrast = 2,
    Warm = 3,
    Navy = 4,
    /// (나) 어두운 전문가용 — 2026-11-27 새 기본
    Pro = 5,
    /// (나) 의 밝은 판 — 같은 배치 · 같은 색 규칙(빨강 = 송출 중 · 노랑 = 다음 · 송출 단추 하나만 도드라짐), 2.1.0 다듬기
    ProLight = 6,
}

impl ThemeId {
    pub const ALL: [ThemeId; 7] = [ThemeId::Pro, ThemeId::ProLight, ThemeId::Dark, ThemeId::Light, ThemeId::Contrast, ThemeId::Warm, ThemeId::Navy];
    /// 설정 값 → 테마. Electron 과 같은 id(dark·light·contrast·warm·navy) + Rust 판 새 기본 pro. 예전 Rust 판 값 night 는 navy,
    /// 아직 옮기지 않은 manna(만나로)는 밝게로 본다. 모르는 값·빈 값은 새 기본(pro).
    pub fn from_setting(s: &str) -> ThemeId {
        match s {
            "dark" => ThemeId::Dark,
            "light" | "manna" | "mannaro" | "bright" => ThemeId::Light,
            "contrast" => ThemeId::Contrast,
            "warm" => ThemeId::Warm,
            "navy" | "night" => ThemeId::Navy,
            "pro-light" => ThemeId::ProLight,
            _ => ThemeId::Pro,
        }
    }
    pub fn id(self) -> &'static str {
        ["dark", "light", "contrast", "warm", "navy", "pro", "pro-light"][self as usize]
    }
    /// 화면에 보이는 이름 (Electron uitheme.js)
    pub fn name(self) -> &'static str {
        ["어둡게", "밝게", "고대비", "따뜻하게", "푸른 밤", "전문가 (기본)", "전문가 밝게"][self as usize]
    }
}

/// control.css 색 변수 한 벌
#[derive(Clone, Debug)]
pub struct Palette {
    pub id: ThemeId,
    pub dark: bool,
    pub bg: Color32,
    pub panel: Color32,
    pub panel2: Color32,
    pub line: Color32,
    pub line2: Color32,
    pub line_soft: Color32,
    pub text: Color32,
    pub text2: Color32,
    pub muted: Color32,
    pub muted2: Color32,
    pub sunken: Color32,
    pub hover: Color32,
    pub sel: Color32,
    pub menubar: Color32,
    pub hl: Color32,
    pub gold: Color32,
    pub ok_text: Color32,
    pub danger_text: Color32,
    pub accent: Color32,
    pub accent_strong: Color32,
    pub pv: Color32,
    pub live: Color32,
    pub warn: Color32,
    pub warn_strong: Color32,
    // color-mix(in srgb, X p%, var(--panel)) 파생색
    pub pv_bg: Color32,
    pub live_bg: Color32,
    pub live_bg2: Color32,
    pub live_bg3: Color32,
    pub live_line: Color32,
    pub warn_bg: Color32,
    pub accent_bg: Color32,
    /// 고대비 테마는 단추·입력칸 테두리가 2px
    pub border_w: f32,
    // ---- (나) 전문가용 배치의 자리 색 (다른 테마는 위 색에서 끌어 씀)
    /// 판 머리 띠 (.ph · .sh)
    pub head: Color32,
    /// 위 컨트롤 막대 (.cb)
    pub bar: Color32,
    /// 컨트롤 막대 위 1px 밝은 선
    pub bar_hi: Color32,
    /// 판 사이 1px 선 (.mid gap)
    pub edge: Color32,
    /// 막대 위 도드라진 단추 (.tpb)
    pub raised: Color32,
    /// 상태 표시창 바탕 · 글 · 이름표
    pub lcd: Color32,
    pub lcd_text: Color32,
    pub lcd_label: Color32,
    /// '다음' (고른 것) 색 — 노랑. 테두리·글자·표 줄 바탕
    pub next: Color32,
    pub next_bg: Color32,
    /// '송출 중' 줄 바탕 · 밝은 빨강 글자
    pub live_dk: Color32,
    pub live_text: Color32,
    /// 강조색 위 글자 (노랑 위는 검정)
    pub on_accent: Color32,
    /// 송출 단추 (전문가용: 어두운 막대 위 하나뿐인 흰 단추)
    pub send_bg: Color32,
    pub send_fg: Color32,
    /// 단추를 테두리 대신 위·아래 선으로 도드라지게 (전문가용)
    pub bevel: bool,
    // ---- 디자인 체계 색 역할 (2026-11-28)
    /// 키보드 초점 고리 · 입력칸 초점 테두리 · 글 커서. 전문가 테마는 밝은 무채색(빨강 · 노랑과 겹치지 않게), 옛 테마는 강조색
    pub focus: Color32,
    /// 조절 칸(단추 · 팝업 단추 · 입력칸)의 가는 테두리
    pub ctl_line: Color32,
    /// 막대(컨트롤 막대) 위 단추 바탕 — 판 위 단추(raised)보다 한 단계 밝다
    pub bar_raised: Color32,
    /// 오류 · 주의 띠 바탕 (주황을 판 색에 섞은 것)
    pub danger_bg: Color32,
}

/// `#rrggbb` / `#rgb` → 색 (상수 표만 쓰므로 틀리면 바로 드러나게 panic)
pub fn hex(s: &str) -> Color32 {
    let h = s.trim_start_matches('#');
    let p = |i: usize, n: usize| u8::from_str_radix(&h[i..i + n], 16).expect("색 값");
    match h.len() {
        3 => Color32::from_rgb(p(0, 1) * 17, p(1, 1) * 17, p(2, 1) * 17),
        _ => Color32::from_rgb(p(0, 2), p(2, 2), p(4, 2)),
    }
}

/// css `color-mix(in srgb, a p%, b)` — p 는 0~100
pub fn mix(a: Color32, p: f32, b: Color32) -> Color32 {
    let t = (p / 100.0).clamp(0.0, 1.0);
    let f = |x: u8, y: u8| (x as f32 * t + y as f32 * (1.0 - t)).round() as u8;
    Color32::from_rgb(f(a.r(), b.r()), f(a.g(), b.g()), f(a.b(), b.b()))
}

/// 불투명 색에 투명도(0~1)를 준다 (css rgba(…, a), color-mix(… transparent))
pub fn alpha(c: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// 색 변수 25개 (css 순서: bg panel panel2 line line2 line-soft text text2 muted muted2 sunken hover sel menubar hl gold ok-text danger-text accent accent-strong pv live warn warn-strong)
fn build(id: ThemeId, v: [&str; 24]) -> Palette {
    let c: Vec<Color32> = v.iter().map(|x| hex(x)).collect();
    let panel = c[1];
    // 밝게 테마만 color-mix 비율이 다르다
    let (pv_p, l1, l2, l3, w_p, a_p) = if id == ThemeId::Light { (14.0, 12.0, 20.0, 30.0, 18.0, 14.0) } else { (22.0, 24.0, 38.0, 52.0, 24.0, 24.0) };
    Palette {
        id,
        dark: id != ThemeId::Light && id != ThemeId::ProLight,
        bg: c[0],
        panel,
        panel2: c[2],
        line: c[3],
        line2: c[4],
        line_soft: c[5],
        text: c[6],
        text2: c[7],
        muted: c[8],
        muted2: c[9],
        sunken: c[10],
        hover: c[11],
        sel: c[12],
        menubar: c[13],
        hl: c[14],
        gold: c[15],
        ok_text: c[16],
        danger_text: c[17],
        accent: c[18],
        accent_strong: c[19],
        pv: c[20],
        live: c[21],
        warn: c[22],
        warn_strong: c[23],
        pv_bg: mix(c[20], pv_p, panel),
        live_bg: mix(c[21], l1, panel),
        live_bg2: mix(c[21], l2, panel),
        live_bg3: mix(c[21], l3, panel),
        live_line: mix(c[21], 55.0, panel),
        warn_bg: mix(c[22], w_p, panel),
        accent_bg: mix(c[18], a_p, panel),
        border_w: if id == ThemeId::Contrast { 2.0 } else { 1.0 },
        head: c[2],
        bar: c[1],
        bar_hi: c[2],
        edge: c[0],
        raised: c[2],
        lcd: c[10],
        lcd_text: c[6],
        lcd_label: c[8],
        next: c[20],
        next_bg: mix(c[20], pv_p, panel),
        live_dk: mix(c[21], l2, panel),
        live_text: c[17],
        on_accent: Color32::WHITE,
        send_bg: c[21],
        send_fg: Color32::WHITE,
        bevel: false,
        focus: c[18],
        ctl_line: c[3],
        bar_raised: c[2],
        danger_bg: mix(c[17], 18.0, panel),
    }
}

/// (나) 어두운 전문가용 맥 — pro-main.html :root 값 그대로
fn build_pro() -> Palette {
    let mut p = build(ThemeId::Pro, [
        "#1f1f21", // bg (--base)
        "#2a2a2c", // panel
        "#323235", // panel2 (--head)
        "#3c3c3f", // line
        "#4a4a4e", // line2
        "#303033", // line-soft (표 줄 사이)
        "#e6e6e8", // text
        "#c4c4c8", // text2
        "#9d9da3", // muted (--sec)
        "#6c6c72", // muted2 (--ter)
        "#1c1c1e", // sunken (--field)
        "#3a3a3d", // hover
        "#58585c", // sel (.seg span.on)
        "#2c2c2e", // menubar (Windows 제목 아래 메뉴 줄)
        "#f0c419", // hl
        "#f0c419", // gold
        "#d6d6da", // ok-text (전문가용: 초록 대신 밝은 회색 — 색은 빨강 = 송출 중 · 노랑 = 다음 둘만, 성공은 체크 아이콘으로)
        "#f39a5a", // danger-text (전문가용: 오류는 주황 — 빨강은 송출 중에만)
        "#f0c419", // accent (노랑 하나)
        "#b8920f", // accent-strong
        "#f0c419", // pv = 다음 (노랑)
        "#e5372e", // live
        "#f0a03c", // warn (주황: 노랑 '다음' 과 구별)
        "#8a5a10", // warn-strong
    ]);
    p.head = hex("#323235");
    p.bar = hex("#3a3a3d");
    p.bar_hi = hex("#4a4a4e");
    p.edge = hex("#151516");
    p.raised = hex("#4a4a4e");
    p.lcd = hex("#181a1c");
    p.lcd_text = hex("#e3e9ea");
    p.lcd_label = hex("#7b878c");
    p.next = hex("#f0c419");
    p.next_bg = hex("#4a4220");
    p.pv_bg = hex("#4a4220");
    p.accent_bg = hex("#4a4220");
    p.live_dk = hex("#5a1d1a");
    p.live_text = hex("#ff6b62");
    p.on_accent = hex("#1a1a1a");
    p.send_bg = hex("#e8e8ea");
    p.send_fg = hex("#1a1a1a");
    p.bevel = true;
    // 판 위 단추는 판에서 두 단계만 올라온 회색(물러나 있게), 막대 위 이전 · 다음 단추는 한 단계 더 밝게
    p.raised = hex("#434346");
    p.bar_raised = hex("#505054");
    p.ctl_line = hex("#1b1b1d");
    p.focus = hex("#f2f2f4");
    p.danger_bg = hex("#4a3324");
    p
}

/// (나) 전문가 밝게 — 같은 배치 · 같은 자리 색 규칙을 밝은 회색 판에. 상태 표시창은 어두운 그대로(LCD), 송출 단추는 하나뿐인 진한 단추
fn build_pro_light() -> Palette {
    let mut p = build(ThemeId::ProLight, [
        "#d4d4d8", // bg (판 사이 바탕)
        "#ececee", // panel
        "#e2e2e5", // panel2 (--head)
        "#c3c3c8", // line
        "#a9a9af", // line2
        "#dcdce0", // line-soft (표 줄 사이)
        "#1d1d1f", // text
        "#3a3a3e", // text2
        "#5c5c63", // muted
        "#7d7d84", // muted2
        "#ffffff", // sunken (입력칸)
        "#dddde1", // hover
        "#c4c4c9", // sel (.seg span.on)
        "#e4e4e7", // menubar
        "#8a6a00", // hl (밝은 바탕 위 노랑 글은 읽히지 않아 진한 황토)
        "#8a6a00", // gold
        "#3a3a3e", // ok-text (성공은 회색 글 — 색은 빨강 · 노랑 둘만)
        "#b4520f", // danger-text (오류는 진한 주황 — 빨강은 송출 중에만)
        "#c49500", // accent (노랑 하나)
        "#a88200", // accent-strong
        "#c49500", // pv = 다음 (노랑, 밝은 바탕에서 보이게 조금 진하게)
        "#d42e26", // live
        "#d9801c", // warn (주황)
        "#8a5a10", // warn-strong
    ]);
    p.head = hex("#e2e2e5");
    p.bar = hex("#d6d6da");
    p.bar_hi = hex("#f2f2f4");
    p.edge = hex("#b9b9be");
    p.raised = hex("#f6f6f7");
    p.lcd = hex("#232628");
    p.lcd_text = hex("#e8eced");
    p.lcd_label = hex("#8c979c");
    p.next = hex("#c49500");
    p.next_bg = hex("#f5e6a6");
    p.pv_bg = hex("#f5e6a6");
    p.accent_bg = hex("#f5e6a6");
    p.live_dk = hex("#f6d3d0");
    p.live_text = hex("#b3221b");
    p.on_accent = hex("#1a1a1a");
    p.send_bg = hex("#1d1d1f");
    p.send_fg = hex("#f4f4f5");
    p.bevel = true;
    p.bar_raised = hex("#f6f6f7");
    p.ctl_line = hex("#b4b4ba");
    p.focus = hex("#1d1d1f");
    p.danger_bg = hex("#f6dcc6");
    p
}

static PALETTES: once_cell::sync::Lazy<[Palette; 7]> = once_cell::sync::Lazy::new(|| {
    [
        // :root (어둡게, 기본)
        build(ThemeId::Dark, ["#121519", "#1a1e25", "#222731", "#313846", "#465065", "#252b35", "#eef1f6", "#d3d9e4", "#a3abba", "#7c8597", "#0e1115", "#262d3a", "#2c3c5c", "#0b0d10", "#9ec5ff", "#ffd479", "#86e8ae", "#ff9ea1", "#4f8cff", "#2b5fa8", "#3aa76d", "#e5484d", "#f0b429", "#8a6a1c"]),
        // html[data-theme="light"]
        build(ThemeId::Light, ["#e9edf3", "#ffffff", "#f3f5f9", "#cbd2dd", "#a7b1c2", "#e2e6ed", "#182030", "#2b3445", "#535d70", "#737d90", "#e3e8ef", "#e4ecf8", "#cfe0ff", "#dfe4ec", "#1d5bbf", "#8a5a00", "#136b3a", "#b8232a", "#2f6fe0", "#2f6fe0", "#1d8a50", "#d22f36", "#c98a00", "#c98a00"]),
        // html[data-theme="contrast"]
        build(ThemeId::Contrast, ["#000", "#0a0a0a", "#161616", "#5e5e5e", "#9a9a9a", "#3a3a3a", "#fff", "#f2f2f2", "#d6d6d6", "#b0b0b0", "#000", "#2a2a2a", "#003f8f", "#000", "#8fd0ff", "#ffe45c", "#7dffb4", "#ffa3a6", "#3d9bff", "#0060d0", "#22d17a", "#ff4d55", "#ffc400", "#b38600"]),
        // html[data-theme="warm"] (warn-strong 은 :root 값을 물려받음)
        build(ThemeId::Warm, ["#191612", "#211d18", "#2a251f", "#3d362d", "#5d5244", "#2e2822", "#f4ebde", "#e2d6c4", "#b8ab96", "#8f826f", "#13100d", "#342d25", "#4a3a24", "#110e0b", "#f3c47c", "#ffd27a", "#a5e3a0", "#ffaa9e", "#e3963d", "#a8661f", "#6dbb6f", "#e65a4f", "#f0b429", "#8a6a1c"]),
        // html[data-theme="navy"]
        build(ThemeId::Navy, ["#0e1524", "#142035", "#1a2843", "#2a3b5e", "#40567f", "#1f2d4a", "#ebf1fd", "#d0dbf1", "#a5b5d4", "#7a8bab", "#0a101d", "#213256", "#284579", "#08101c", "#93c4ff", "#ffd479", "#8ae6b1", "#ffa0a3", "#5b9bff", "#2f63c0", "#3cb878", "#ee4f57", "#f0b429", "#8a6a1c"]),
        build_pro(),
        build_pro_light(),
    ]
});

static CURRENT: AtomicU8 = AtomicU8::new(ThemeId::Pro as u8);
/// 마지막으로 egui Style 에 적용한 테마 (255 = 아직 안 함)
static APPLIED: AtomicU8 = AtomicU8::new(255);

impl Palette {
    /// 입력칸 바탕 (.field)
    pub fn field_or_sunken(&self) -> Color32 {
        self.sunken
    }
}

/// 누를 수 있는 바탕의 '올라간' 색 (어두운 테마는 밝게, 밝은 테마는 조금 어둡게)
pub fn lift(c: Color32) -> Color32 {
    if pal().dark { mix(c, 80.0, Color32::WHITE) } else { mix(c, 90.0, Color32::BLACK) }
}

/// 진한 바탕(송출 중 · 다음 줄) 위 글 색: 어두운 테마는 흰색, 밝은 테마는 진한 글
pub fn strong_text() -> Color32 {
    let p = pal();
    if p.dark { Color32::WHITE } else { p.text }
}

/// 지금 조작 화면 테마의 색
pub fn pal() -> &'static Palette {
    &PALETTES[CURRENT.load(Ordering::Relaxed) as usize]
}

/// 특정 테마의 색 (테마 고르기 미리보기 카드 등)
pub fn palette_of(id: ThemeId) -> &'static Palette {
    &PALETTES[id as usize]
}

pub fn current() -> ThemeId {
    PALETTES[CURRENT.load(Ordering::Relaxed) as usize].id
}

/// 테마를 바꾸고 egui Style 에 적용한다. 같은 테마면 아무것도 안 한다 (프레임마다 불러도 됨)
pub fn apply(ctx: &egui::Context, id: ThemeId) {
    CURRENT.store(id as u8, Ordering::Relaxed);
    if APPLIED.swap(id as u8, Ordering::Relaxed) == id as u8 {
        return;
    }
    let p = pal();
    // 운영체제 밝기 따라가기를 끄고, 두 egui 테마 모두 같은 모양으로 둔다
    ctx.set_theme(if p.dark { egui::Theme::Dark } else { egui::Theme::Light });
    ctx.all_styles_mut(|s| style(s, p));
}

/// control.css 의 button / input / select / scrollbar 모양을 egui Style 로
pub fn style(s: &mut egui::Style, p: &Palette) {
    use egui::style::{Selection, WidgetVisuals, Widgets};
    let r = CornerRadius::same(radius_for(p.id, Rad::Md));
    let cr = |k: Rad| CornerRadius::same(radius_for(p.id, k));
    let bw = p.border_w;
    let wv = |bg: Color32, stroke: Color32, fg: Color32| WidgetVisuals { bg_fill: bg, weak_bg_fill: bg, bg_stroke: Stroke::new(bw, stroke), corner_radius: r, fg_stroke: Stroke::new(1.0, fg), expansion: 0.0 };
    let v = &mut s.visuals;
    *v = if p.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.widgets = Widgets {
        // 구분선·꺼진 글자 (separator 는 noninteractive.bg_stroke)
        noninteractive: WidgetVisuals { bg_fill: p.panel, weak_bg_fill: p.panel, bg_stroke: Stroke::new(1.0, p.line), corner_radius: r, fg_stroke: Stroke::new(1.0, p.text), expansion: 0.0 },
        // button { background: panel2; border: 1px solid line }
        inactive: wv(p.panel2, p.line, p.text),
        // button:hover { border-color: line2; background: hover }
        hovered: wv(p.hover, p.line2, p.text),
        // 누르는 중 · 포커스
        active: wv(p.hover, p.line2, p.text),
        // 펼친 고르기 칸 (select)
        open: wv(p.sunken, p.line2, p.text),
    };
    if p.bevel {
        // 전문가용: egui 기본 단추 · 고르기 칸 · 입력칸 (창 안의 ui.button · ComboBox · TextEdit · DragValue 가 모두 이 모양)
        //  보통 = 도드라진 회색 + 어두운 가는 테두리 · 올림 = 밝게 · 눌림/초점 = 밝게 + 초점 테두리 · 펼침 = 입력칸 바탕
        // (egui 는 '눌리는 중' 과 '키보드 초점' 을 같은 상태로 그린다 → 눌림을 어둡게 하면 초점이 꺼진 것처럼 보여 테두리로 나타낸다)
        let raised = p.raised;
        let hov = if p.dark { mix(raised, 80.0, Color32::WHITE) } else { mix(raised, 90.0, Color32::BLACK) };
        let md = cr(Rad::Md);
        v.widgets.inactive = WidgetVisuals { bg_fill: raised, weak_bg_fill: raised, bg_stroke: Stroke::new(LINE, p.ctl_line), corner_radius: md, fg_stroke: Stroke::new(1.0, p.text), expansion: 0.0 };
        v.widgets.hovered = WidgetVisuals { bg_fill: hov, weak_bg_fill: hov, bg_stroke: Stroke::new(LINE, p.ctl_line), corner_radius: md, fg_stroke: Stroke::new(1.0, p.text), expansion: 0.0 };
        v.widgets.active = WidgetVisuals { bg_fill: hov, weak_bg_fill: hov, bg_stroke: Stroke::new(LINE, p.focus), corner_radius: md, fg_stroke: Stroke::new(1.0, p.text), expansion: 0.0 };
        v.widgets.open = WidgetVisuals { bg_fill: p.field_or_sunken(), weak_bg_fill: p.field_or_sunken(), bg_stroke: Stroke::new(LINE, p.focus), corner_radius: md, fg_stroke: Stroke::new(1.0, p.text), expansion: 0.0 };
        v.widgets.noninteractive.corner_radius = md;
    }
    // 고른 것: button.on / .view-tabs .on (sel 바탕, 글자는 text). 포커스된 입력칸 테두리도 이 색을 쓴다(egui 제약)
    v.selection = Selection { bg_fill: p.sel, stroke: Stroke::new(1.0, p.text) };
    v.hyperlink_color = p.hl;
    if p.bevel {
        // 전문가 테마 (어둡게 · 밝게): 입력칸 초점 테두리 = 초점색, 고른 글 바탕 · 체크 · 라디오는 회색 한 벌, 링크도 회색 글
        // (노랑은 '다음', 빨강은 '송출 중' 에만 — egui 기본 푸른 선택색이 남지 않게)
        v.selection = Selection { bg_fill: p.sel, stroke: Stroke::new(LINE, p.focus) };
        v.hyperlink_color = p.text2;
    }
    v.faint_bg_color = p.panel2;
    v.extreme_bg_color = p.sunken; // 입력칸·스크롤 바탕
    v.text_edit_bg_color = Some(p.sunken); // select, input { background: sunken }
    v.code_bg_color = p.sunken;
    v.warn_fg_color = p.warn;
    v.error_fg_color = p.danger_text;
    v.weak_text_color = Some(p.muted);
    v.panel_fill = p.panel;
    v.window_fill = p.panel;
    v.window_stroke = Stroke::new(1.0, p.line);
    v.window_corner_radius = cr(Rad::Xl);
    v.menu_corner_radius = cr(Rad::Lg);
    // --shadow: 0 12px 32px rgba(0,0,0,.45)
    let sh = Shadow { offset: [0, 12], blur: 32, spread: 0, color: if p.dark { alpha(Color32::BLACK, 0.45) } else { Color32::from_rgba_unmultiplied(20, 30, 50, 46) } };
    v.window_shadow = sh;
    v.popup_shadow = sh;
    v.window_highlight_topmost = false;
    v.striped = false;
    v.slider_trailing_fill = true;
    v.indent_has_left_vline = false;
    // 글 커서: 전문가 테마는 초점색(노랑은 '다음' 에만) · 옛 테마는 강조색
    v.text_cursor.stroke = if p.bevel { Stroke::new(1.5, p.focus) } else { Stroke::new(2.0, p.accent) };
    // 툴팁: 멈춰 있을 때 0.45초 뒤, 폭 320 (긴 설명이 한 줄로 화면을 가로지르지 않게)
    s.interaction.tooltip_delay = 0.45;
    s.interaction.show_tooltips_only_when_still = true;

    // 글자 크기: 전문가 테마는 글자 단계(Type) 그대로 · 옛 테마는 html 14px 기준
    s.text_styles = if p.bevel {
        [
            (TextStyle::Small, Type::Small.font()),
            (TextStyle::Body, Type::Body.font()),
            (TextStyle::Button, Type::Body.font()),
            (TextStyle::Heading, Type::Title.font()),
            (TextStyle::Monospace, FontId::new(12.0, FontFamily::Monospace)),
        ]
        .into()
    } else {
        [
            (TextStyle::Small, FontId::new(12.0, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(BASE, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(BASE, FontFamily::Proportional)),
            (TextStyle::Heading, FontId::new(16.0, FontFamily::Name(BOLD.into()))),
            (TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace)),
        ]
        .into()
    };
    s.animation_time = if reduce_motion() { 0.0 } else { T_MED };

    let sp = &mut s.spacing;
    sp.item_spacing = Vec2::new(8.0, 6.0);
    sp.button_padding = Vec2::new(12.0, 6.0); // button { padding: 6px 12px }
    sp.interact_size = Vec2::new(40.0, 28.0);
    sp.window_margin = Margin::same(14); // .modal-body { padding: 14px }
    sp.menu_margin = Margin::same(6); // .mb-drop { padding: 6px }
    sp.icon_width = 16.0;
    sp.icon_width_inner = 9.0;
    sp.icon_spacing = 6.0;
    sp.combo_height = 320.0;
    if p.bevel {
        // 전문가용: 조절 칸 높이 보통(26) · 줄 사이 6 · 칸 사이 8. 인스펙터처럼 촘촘한 곳은 그 자리에서 Ctl::Sm 으로 낮춘다 (widgets::dense)
        let h = 26.0; // = ctl(Ctl::Md) — pal() 이 아직 이 테마가 아닐 수 있어 숫자로 (아래 시험 tokens_agree 가 지킨다)
        sp.item_spacing = Vec2::new(self::sp::X8, self::sp::X6);
        sp.button_padding = Vec2::new(self::sp::X8, 3.0);
        sp.interact_size = Vec2::new(36.0, h);
        sp.window_margin = Margin::same(12);
        sp.menu_margin = Margin::same(4);
        sp.icon_width = 14.0;
        sp.icon_width_inner = 8.0;
        sp.icon_spacing = self::sp::X6;
        sp.slider_rail_height = 4.0;
        sp.tooltip_width = 320.0;
        sp.indent = self::sp::X16;
    }
    // 얇은 스크롤바: 폭 10px, 손잡이는 3px 안쪽 여백(테두리 투명) → 보이는 막대 4px, 바탕 투명
    let sc = &mut sp.scroll;
    sc.floating = false;
    sc.bar_width = 4.0;
    sc.bar_inner_margin = 3.0;
    sc.bar_outer_margin = 3.0;
    sc.handle_min_length = 24.0;
    sc.foreground_color = true; // 손잡이 = 글자색 × 투명도 → line2 ~ muted2 쯤
    sc.dormant_background_opacity = 0.0;
    sc.active_background_opacity = 0.0;
    sc.interact_background_opacity = 0.0;
    sc.dormant_handle_opacity = 0.4; // 긴 창(출력 설정 등)에서 아래가 더 있다는 것이 보이게 (0.25 는 거의 안 보였음)
    sc.active_handle_opacity = 0.5;
    sc.interact_handle_opacity = 0.6;
}

// ---------------------------------------------------------------- 글꼴

/// 굵기별 글꼴 가족 이름 (FontFamily::Name)
pub const MEDIUM: &str = "medium";
pub const SEMIBOLD: &str = "semibold";
pub const BOLD: &str = "bold";
/// 슬라이드 그림용 (맑은 고딕 먼저): 송출 화면 기본 글꼴이 맑은 고딕이라 미리보기도 맞춘다
pub const SLIDE: &str = "slide";
pub const SLIDE_BOLD: &str = "slide-bold";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weight {
    /// 400
    Regular,
    /// 500
    Medium,
    /// 600
    SemiBold,
    /// 700 이상
    Bold,
}

impl Weight {
    pub fn family(self) -> FontFamily {
        match self {
            Weight::Regular => FontFamily::Proportional,
            Weight::Medium => FontFamily::Name(MEDIUM.into()),
            Weight::SemiBold => FontFamily::Name(SEMIBOLD.into()),
            Weight::Bold => FontFamily::Name(BOLD.into()),
        }
    }
    /// css font-weight 숫자 → 굵기
    pub fn from_css(w: f32) -> Weight {
        if w >= 700.0 { Weight::Bold } else if w >= 600.0 { Weight::SemiBold } else if w >= 500.0 { Weight::Medium } else { Weight::Regular }
    }
}

/// 크기·굵기 → FontId (Pretendard)
pub fn font(size: f32, w: Weight) -> FontId {
    FontId::new(size, w.family())
}

/// 글·크기·굵기·색을 한 번에
pub fn rich(text: impl Into<String>, size: f32, w: Weight, color: Color32) -> RichText {
    RichText::new(text).font(font(size, w)).color(color)
}

/// 슬라이드 미리보기·LIVE 그림용 글꼴 가족 (맑은 고딕 → Pretendard 순)
pub fn slide_family(bold: bool) -> FontFamily {
    FontFamily::Name(if bold { SLIDE_BOLD } else { SLIDE }.into())
}

/// Pretendard 4굵기(내장) + 맑은 고딕(빠진 글자 대체, 슬라이드 그림용)을 등록한다.
/// 시작할 때 한 번만 부른다. PPT 글꼴(paint_edit.rs)은 그 뒤에 ctx.add_font 로 덧붙는다.
pub fn install_fonts(ctx: &egui::Context) {
    ctx.set_fonts(font_definitions());
}

pub fn font_definitions() -> egui::FontDefinitions {
    let mut defs = egui::FontDefinitions::default();
    let base_prop: Vec<String> = defs.families[&FontFamily::Proportional].clone(); // egui 기본(영문·이모지·기호)
    let mut add = |name: &str, bytes: &'static [u8]| {
        defs.font_data.insert(name.into(), Arc::new(egui::FontData::from_static(bytes)));
    };
    add("pretendard", include_bytes!("../../../assets/fonts/Pretendard-Regular.ttf"));
    add("pretendard-medium", include_bytes!("../../../assets/fonts/Pretendard-Medium.ttf"));
    add("pretendard-semibold", include_bytes!("../../../assets/fonts/Pretendard-SemiBold.ttf"));
    add("pretendard-bold", include_bytes!("../../../assets/fonts/Pretendard-Bold.ttf"));
    let mut malgun = vec![];
    let mut malgun_bold = vec![];
    if let Some(r) = wp_core::measure::font_file(r"C:\Windows\Fonts\malgun.ttf") {
        defs.font_data.insert("malgun".into(), Arc::new(egui::FontData::from_static(r)));
        malgun.push("malgun".to_string());
    }
    if let Some(b) = wp_core::measure::font_file(r"C:\Windows\Fonts\malgunbd.ttf") {
        defs.font_data.insert("malgun-bold".into(), Arc::new(egui::FontData::from_static(b)));
        malgun_bold.push("malgun-bold".to_string());
    }
    // 맥: 맑은 고딕이 없으므로 Apple SD Gothic Neo (웹 출력이 쓰는 다음 글꼴과 같게)
    if malgun.is_empty() {
        if let Some((bytes, r, b)) = wp_core::measure::mac_korean_faces() {
            let mut fd = egui::FontData::from_static(bytes);
            fd.index = r;
            defs.font_data.insert("malgun".into(), Arc::new(fd));
            malgun.push("malgun".to_string());
            if let Some(bi) = b {
                let mut fb = egui::FontData::from_static(bytes);
                fb.index = bi;
                defs.font_data.insert("malgun-bold".into(), Arc::new(fb));
                malgun_bold.push("malgun-bold".to_string());
            }
        }
    }
    if malgun_bold.is_empty() {
        malgun_bold = malgun.clone();
    }
    // 가족 = [이 굵기의 Pretendard, 맑은 고딕(같은 굵기), egui 기본]
    let fam = |first: &str, mg: &[String]| -> Vec<String> {
        let mut v = vec![first.to_string()];
        v.extend(mg.iter().cloned());
        v.extend(base_prop.iter().filter(|x| !v.contains(x)).cloned().collect::<Vec<_>>());
        v
    };
    let regular = fam("pretendard", &malgun);
    defs.families.insert(FontFamily::Proportional, regular);
    defs.families.insert(FontFamily::Name(MEDIUM.into()), fam("pretendard-medium", &malgun));
    defs.families.insert(FontFamily::Name(SEMIBOLD.into()), fam("pretendard-semibold", &malgun_bold));
    defs.families.insert(FontFamily::Name(BOLD.into()), fam("pretendard-bold", &malgun_bold));
    // 슬라이드 그림: 맑은 고딕 먼저 (없으면 Pretendard)
    let mut slide = malgun.clone();
    slide.extend(fam("pretendard", &[]));
    defs.families.insert(FontFamily::Name(SLIDE.into()), slide);
    let mut slide_b = malgun_bold.clone();
    slide_b.extend(fam("pretendard-bold", &[]));
    defs.families.insert(FontFamily::Name(SLIDE_BOLD.into()), slide_b);
    // 고정폭: egui 기본 Hack 다음에 한글
    let mono = defs.families.get_mut(&FontFamily::Monospace).unwrap();
    mono.push("pretendard".into());
    mono.extend(malgun);
    defs
}

// ---------------------------------------------------------------- 순서 종류 색

/// 순서 종류별 색 (css `--tc`, 모든 테마 공통). 모르는 종류는 None (→ muted)
pub fn type_color(t: &str) -> Option<Color32> {
    Some(hex(match t {
        "bible" => "#2563eb",
        "hymn" | "score" | "hymns" => "#4f46e5",
        "song" | "songs" => "#7c3aed",
        "weblyrics" => "#0891b2",
        "text" => "#d97706",
        "title" => "#0f766e",
        "image" | "images" => "#0d9488",
        "video" => "#db2777",
        "audio" => "#ea580c",
        "slideshow" => "#0e7490",
        "camera" | "cameras" => "#e11d48",
        "ppt" | "folder" => "#c2410c",
        "color" | "templates" => "#c026d3",
        "overlays" => "#2563eb",
        "blank" => "#64748b",
        _ => return None,
    }))
}

/// 종류 아이콘 색 (css `--ink`): 어두운 테마는 종류 색 62% + 흰색, 밝게는 그대로. 고대비는 muted
pub fn type_ink(t: &str) -> Color32 {
    let p = pal();
    // 고대비 · 전문가(기본) 테마는 종류 아이콘을 흐린 글 색 한 가지로 (빨강=송출 · 노랑=다음 외에는 색을 쓰지 않는다)
    if p.id == ThemeId::Contrast || p.id == ThemeId::Pro || p.id == ThemeId::ProLight {
        return p.muted;
    }
    let tc = type_color(t).unwrap_or(p.muted);
    if p.dark { mix(tc, 62.0, Color32::WHITE) } else { tc }
}

/// 큐시트 순서 아이콘 칩 바탕 (css .cue-ico: 18% 섞기, 고른 줄이면 sel 과 26%)
pub fn type_chip_bg(t: &str, selected: bool) -> Color32 {
    let p = pal();
    let tc = type_color(t).unwrap_or(p.muted);
    if selected { mix(tc, 26.0, p.sel) } else { mix(tc, 18.0, p.panel) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_mix_matches_css() {
        // color-mix(in srgb, #3aa76d 22%, #1a1e25)
        let d = palette_of(ThemeId::Dark);
        assert_eq!(d.pv_bg, Color32::from_rgb(33, 60, 53));
        assert_eq!(d.bg, hex("#121519"));
        assert_eq!(palette_of(ThemeId::Contrast).bg, Color32::BLACK);
        assert!(!palette_of(ThemeId::Light).dark);
    }

    #[test]
    fn setting_ids() {
        assert_eq!(ThemeId::from_setting("night"), ThemeId::Navy);
        assert_eq!(ThemeId::from_setting("navy").id(), "navy");
        assert_eq!(ThemeId::from_setting("???"), ThemeId::Pro);
        assert_eq!(ThemeId::from_setting("dark"), ThemeId::Dark);
        assert_eq!(ThemeId::from_setting("pro").id(), "pro");
    }

    #[test]
    fn pro_light_keeps_pro_rules_on_light_ground() {
        // 2.1.0 다듬기 2차: 전문가 밝게 — 같은 배치(bevel) · 밝은 바탕 · 빨강 = 송출 중 · 노랑 = 다음 · 송출 단추만 진하게
        let id = ThemeId::from_setting("pro-light");
        assert_eq!(id, ThemeId::ProLight);
        assert_eq!(id.id(), "pro-light");
        assert!(ThemeId::ALL.contains(&id));
        let p = palette_of(id);
        assert!(p.bevel && !p.dark);
        let lum = |c: Color32| 0.2126 * c.r() as f32 + 0.7152 * c.g() as f32 + 0.0722 * c.b() as f32;
        assert!(lum(p.panel) > 200.0 && lum(p.text) < 60.0, "밝은 판 · 진한 글");
        assert!(lum(p.send_bg) < 60.0 && lum(p.send_fg) > 200.0, "송출 단추 하나만 진하게");
        assert!(p.live.r() > 180 && p.live.g() < 80, "빨강 = 송출 중");
        assert!(p.next.r() > 150 && p.next.g() > 120 && p.next.b() < 40, "노랑 = 다음");
        // 옛 밝게 · 고대비는 그대로
        assert!(!palette_of(ThemeId::Light).bevel && !palette_of(ThemeId::Contrast).bevel);
        assert_eq!(radius_for(id, Rad::Md), radius_for(ThemeId::Pro, Rad::Md));
    }

    #[test]
    fn tokens_agree() {
        // style() 가 숫자로 쓴 전문가 조절 칸 높이(26)가 ctl(Ctl::Md) 와 같아야 한다
        let mut st = egui::Style::default();
        let p = palette_of(ThemeId::Pro);
        style(&mut st, &p);
        assert_eq!(st.spacing.interact_size.y, 26.0);
        // 글자 단계: 크기 다섯 가지, 줄 높이는 크기보다 크다
        let mut sizes: Vec<u32> = Type::ALL.iter().map(|t| (t.size() * 10.0) as u32).collect();
        sizes.sort();
        sizes.dedup();
        assert_eq!(sizes.len(), 5);
        for t in Type::ALL {
            assert!(t.line() > t.size(), "{}", t.name());
        }
        // 4px 격자 (2 · 6 은 촘촘한 보정만)
        for v in [sp::X4, sp::X8, sp::X12, sp::X16, sp::X20, sp::X24, sp::X32, sp::PANE] {
            assert_eq!(v % 4.0, 0.0);
        }
        assert_eq!(radius_for(ThemeId::Pro, Rad::Md), 4);
    }
}
