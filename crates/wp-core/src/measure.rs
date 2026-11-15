//! 슬라이드 글이 템플릿 화면에 들어가는지 재기 (Electron 판은 실제 DOM 으로 쟀다)
//!
//! 웹 출력(`web/shared/render.js` · `render.css`)이 브라우저에서 놓이는 규칙을 그대로 따라 계산한다
//! (2026-11 점검 A: 근사 → 규칙 옮김. `tools/flow-compare/split-batch.js` 로 브라우저와 대조):
//! - 글자 폭: 템플릿 글꼴 파일의 글리프 폭 + 커닝(kern), 글꼴 크기는 브라우저처럼 1/64px 로 맞춤,
//!   font-family 차례대로 글자가 있는 첫 글꼴 (맑은 고딕이 아니면 시스템 글꼴에서 찾음)
//! - 줄바꿈: `.bc-text` 는 word-break: keep-all (빈칸·붙임표·줄표·닫는 문장 부호 뒤) + overflow-wrap: anywhere
//!   (한 어절이 줄보다 길 때만 글자 사이), 줄 끝 빈칸은 폭에 넣지 않음. 장절·번역본 줄은 보통 줄바꿈(한글 글자 사이)
//! - 높이: 줄 수 × 글자 크기 × line-height (줄 안의 가장 큰 글자 기준), 덩어리 사이 여백은 CSS 여백 겹침대로
//! - 장절 줄(.bc-ref) · 번역본(.bc-version) · 소제목(.bc-heading) · 절 번호(.bc-vn) · 병행 번역(.bc-par) ·
//!   번역 줄(.bc-tr) · 내어쓰기(.bc-hang) · 글 서식(**굵게** {크기:150}) · 제목 카드 · 상자 여백

use ab_glyph::{Font, FontArc, FontVec, GlyphId};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

pub struct Fonts {
    regular: Option<FontArc>,
    bold: Option<FontArc>,
    light: Option<FontArc>,
    /// 맑은 고딕에 없는 글자(기호 등)를 대신할 글꼴
    fallback: Vec<FontArc>,
}

/// 글꼴 파일 내용을 한 번만 읽어 프로그램 끝까지 쥔다 (경로별). 재기(ab_glyph)와 조작 화면(egui)이 같은 것을 써서
/// 맑은 고딕 13MB × 2벌을 두 번씩 읽지 않는다.
pub fn font_file(path: &str) -> Option<&'static [u8]> {
    static FILES: Lazy<parking_lot::Mutex<std::collections::HashMap<String, Option<&'static [u8]>>>> = Lazy::new(Default::default);
    let mut m = FILES.lock();
    if let Some(v) = m.get(path) {
        return *v;
    }
    // 파일 매핑으로 (복사하지 않음). 안 되면 읽어서 쥔다
    let v: Option<&'static [u8]> = crate::pptx::build::map_font_file(std::path::Path::new(path)).or_else(|| std::fs::read(path).ok().map(|b| &*Box::leak(b.into_boxed_slice())));
    m.insert(path.to_string(), v);
    v
}

fn load(paths: &[&str]) -> Option<FontArc> {
    for p in paths {
        if let Some(bytes) = font_file(p) {
            if let Ok(f) = FontArc::try_from_slice(bytes) {
                return Some(f);
            }
        }
    }
    None
}

pub static FONTS: Lazy<Fonts> = Lazy::new(|| Fonts {
    regular: load(&[r"C:\Windows\Fonts\malgun.ttf", "/System/Library/Fonts/AppleSDGothicNeo.ttc", "/usr/share/fonts/truetype/nanum/NanumGothic.ttf"]),
    bold: load(&[r"C:\Windows\Fonts\malgunbd.ttf", r"C:\Windows\Fonts\malgun.ttf"]),
    light: load(&[r"C:\Windows\Fonts\malgunsl.ttf"]),
    fallback: [r"C:\Windows\Fonts\segoeui.ttf", r"C:\Windows\Fonts\seguisym.ttf"].iter().filter_map(|p| load(&[p])).collect(),
});

fn num(v: &Value, d: f32) -> f32 {
    v.as_f64().map(|x| x as f32).unwrap_or(d)
}

// ---------------------------------------------------------------------------------------------
// 글꼴 묶음
// ---------------------------------------------------------------------------------------------

/// CSS font-family 차례대로 늘어놓은 글꼴 (글자가 없으면 다음 글꼴에서 찾는다)
#[derive(Clone)]
pub struct Chain(Arc<Vec<FontArc>>);

static CHAINS: Lazy<Mutex<HashMap<(String, u16), Chain>>> = Lazy::new(|| Mutex::new(HashMap::new()));

fn is_malgun(name: &str) -> bool {
    let n = name.to_lowercase().replace(' ', "");
    n == "malgungothic" || n == "맑은고딕"
}

/// 맑은 고딕에서 굵기에 맞는 것 (CSS 글꼴 고르기: 400~500 은 보통, 500 초과는 굵게, 350 이하는 가늘게)
fn malgun(weight: u16) -> Option<FontArc> {
    let f = &*FONTS;
    if weight > 500 {
        f.bold.clone().or_else(|| f.regular.clone())
    } else if weight <= 350 {
        f.light.clone().or_else(|| f.regular.clone())
    } else {
        f.regular.clone()
    }
}

/// font-family 글 → 글꼴 묶음. 맑은 고딕이 맨 앞이면(기본 템플릿 전부) 시스템 글꼴 목록을 읽지 않는다.
///
/// 웹 출력(render.js withLocalFont)은 따옴표로 묶인 첫 글꼴 이름에 `@font-face { src: local("이름") }` 별칭을 만들어 맨 앞에 둔다.
/// local() 은 '전체 이름'이 같은 글꼴 파일 하나(보통 굵기)를 가리키므로, 굵기가 700 이어도 **보통 굵기 파일의 글자 폭**으로 놓이고
/// 굵게는 덧칠(합성)만 한다 — 글자 폭이 늘지 않는다 (엣지 실측). 그래서 묶음 맨 앞에 그 파일을 둔다.
pub fn chain_for(family: &str, weight: u16) -> Chain {
    let key = (family.to_string(), weight);
    if let Some(c) = CHAINS.lock().get(&key) {
        return c.clone();
    }
    let unquote = |s: &str| s.trim().trim_matches(|c| c == '\'' || c == '"').trim().to_string();
    let generic = |n: &str| matches!(n.to_lowercase().as_str(), "sans-serif" | "serif" | "monospace" | "system-ui" | "cursive" | "fantasy");
    let names: Vec<String> = family.split(',').map(unquote).filter(|s| !s.is_empty()).collect();
    let mut fonts: Vec<FontArc> = vec![];
    // 1) local() 별칭: 첫 이름이 따옴표로 묶여 있을 때만
    let quoted_first = family.trim_start().starts_with(['\'', '"']);
    let mut alias_malgun = false;
    if let Some(first) = names.first().filter(|n| quoted_first && n.chars().count() <= 60 && !generic(n)) {
        if is_malgun(first) {
            fonts.extend(FONTS.regular.clone());
            alias_malgun = true;
        } else if let Some((data, index)) = crate::pptx::build::font_local(first) {
            if let Ok(f) = FontVec::try_from_vec_and_index(data, index) {
                fonts.push(FontArc::new(f));
            }
        }
    }
    // 2) 이름 차례대로, 굵기에 맞는 글꼴
    let mut have_malgun = false;
    for name in &names {
        if is_malgun(name) || generic(name) {
            if !have_malgun {
                // 별칭이 이미 맑은 고딕(보통)이면 맑은 고딕에 있는 글자는 모두 거기서 놓인다 — 굵은 파일을 더 볼 일이 없다
                if !alias_malgun {
                    fonts.extend(malgun(weight));
                }
                have_malgun = true;
            }
            break; // 맑은 고딕 뒤의 글꼴은 맑은 고딕에 없는 글자에만 쓰인다 — 아래 대체 글꼴로 충분
        }
        if let Some((data, index)) = crate::pptx::build::font_face(name, weight) {
            if let Ok(f) = FontVec::try_from_vec_and_index(data, index) {
                fonts.push(FontArc::new(f));
            }
        }
    }
    if !have_malgun {
        fonts.extend(malgun(weight));
    }
    fonts.extend(FONTS.fallback.iter().cloned());
    let c = Chain(Arc::new(fonts));
    CHAINS.lock().insert(key, c.clone());
    c
}

/// 맑은 고딕 보통·굵은 파일 그대로 (PPT 그리기처럼 웹 출력 규칙과 상관없는 곳)
fn default_chain(bold: bool) -> Chain {
    chain_for("Malgun Gothic", if bold { 700 } else { 400 })
}

impl Chain {
    /// 글자가 있는 첫 글꼴과 글리프
    fn glyph(&self, c: char) -> Option<(usize, GlyphId)> {
        for (i, f) in self.0.iter().enumerate() {
            let g = f.glyph_id(c);
            if g.0 != 0 {
                return Some((i, g));
            }
        }
        self.0.first().map(|f| (0, f.glyph_id(c)))
    }

    /// 글자마다의 폭 (px). 커닝은 앞 글자 폭에 더한다 (브라우저의 글 다듬기와 같은 자리)
    fn advances(&self, s: &str, size: f32, letter: f32, out: &mut Vec<f32>) {
        let size = snap(size);
        let mut prev: Option<(usize, GlyphId)> = None;
        for c in s.chars() {
            let Some((fi, g)) = self.glyph(c) else {
                // 글꼴이 하나도 없는 환경(시험 서버 등): 어림값
                out.push(if (c as u32) > 0x2e80 { size * 0.95 } else if c == ' ' { size * 0.3 } else { size * 0.55 } + letter);
                continue;
            };
            let f = &self.0[fi];
            let k = size / f.units_per_em().unwrap_or(1000.0);
            if let (Some((pf, pg)), Some(last)) = (prev, out.last_mut()) {
                if pf == fi {
                    *last += f.kern_unscaled(pg, g) * k;
                }
            }
            out.push(f.h_advance_unscaled(g) * k + letter);
            prev = Some((fi, g));
        }
    }

    pub fn width(&self, s: &str, size: f32, letter: f32) -> f32 {
        let mut v = Vec::with_capacity(s.len());
        self.advances(s, size, letter, &mut v);
        v.iter().sum()
    }
}

/// 브라우저는 글꼴 크기를 1/64px 단위로 맞춘다
fn snap(size: f32) -> f32 {
    (size * 64.0).round() / 64.0
}

/// 글자 폭 (px) — 맑은 고딕 (PPT 그리기 등 템플릿과 상관없는 곳에서 씀)
pub fn text_width(s: &str, size: f32, bold: bool, letter: f32) -> f32 {
    default_chain(bold).width(s, size, letter)
}

// ---------------------------------------------------------------------------------------------
// 줄바꿈
// ---------------------------------------------------------------------------------------------

/// 줄에 놓이는 글자 하나 (절 번호처럼 통째로 놓이는 것은 c = '\u{fffc}')
#[derive(Clone, Copy)]
struct Ch {
    c: char,
    adv: f32,
    /// 이 글자의 글꼴 크기 (줄 높이 = 줄 안의 가장 큰 크기 × line-height). 줄 높이에 끼지 않으면 0
    size: f32,
}

const OBJ: char = '\u{fffc}';
const EPS: f32 = 1.0 / 64.0;

/// 줄바꿈에서의 글자 갈래 (유니코드 줄바꿈 규칙 UAX #14 가운데 예배 글에 나오는 것, 엣지/크롬 실측에 맞춤)
#[derive(Clone, Copy, PartialEq)]
enum Lb {
    Sp,
    /// 한글·한자·가나
    Id,
    /// 그 밖의 글자
    Al,
    Nu,
    /// 여는 괄호·따옴표
    Op,
    /// 닫는 괄호·따옴표
    Cl,
    /// , . ; :
    Is,
    /// ! ?
    Ex,
    /// %
    Po,
    /// $ + ₩
    Pr,
    /// -
    Hy,
    /// –
    Ba,
    /// —
    B2,
    /// …
    In,
    /// /
    Sy,
    /// 곧은 따옴표
    Qu,
    /// 그 밖의 기호 (& = * @ # ~ · < > …)
    Sym,
    /// 절 번호처럼 통째로 놓이는 것
    Obj,
}

fn lb(c: char) -> Lb {
    match c {
        ' ' | '\t' | '\n' | '\r' => Lb::Sp,
        OBJ | '\u{a0}' => Lb::Obj,
        '(' | '[' | '{' | '「' | '『' | '《' | '〈' | '【' | '〔' | '“' | '‘' | '（' => Lb::Op,
        ')' | ']' | '}' | '」' | '』' | '》' | '〉' | '】' | '〕' | '”' | '’' | '、' | '。' | '）' | '，' => Lb::Cl,
        ',' | '.' | ';' | ':' => Lb::Is,
        '!' | '?' | '！' | '？' => Lb::Ex,
        '%' | '°' | '℃' => Lb::Po,
        '$' | '+' | '₩' | '\\' | '±' | '№' => Lb::Pr,
        '-' => Lb::Hy,
        '–' => Lb::Ba,
        '—' => Lb::B2,
        '…' | '‥' => Lb::In,
        '/' => Lb::Sy,
        '"' | '\'' => Lb::Qu,
        _ if c.is_ascii_digit() => Lb::Nu,
        _ if matches!(c as u32, 0x1100..=0x11ff | 0x2e80..=0x2fdf | 0x3040..=0x30ff | 0x3130..=0x318f | 0x3400..=0x4dbf | 0x4e00..=0x9fff | 0xac00..=0xd7a3 | 0xf900..=0xfaff | 0xff66..=0xff9f) => Lb::Id,
        _ if c.is_alphabetic() => Lb::Al,
        _ => Lb::Sym,
    }
}

/// i 번째 글자 앞에서 줄을 바꿀 수 있는가.
/// keep_all (word-break: keep-all): 한글끼리·한글과 글자/숫자 사이에서는 바꾸지 않는다 (기호·문장 부호 쪽은 그대로 바뀐다)
fn can_break(chs: &[Ch], i: usize, keep_all: bool) -> bool {
    use Lb::*;
    let (p, c) = (lb(chs[i - 1].c), lb(chs[i].c));
    if c == Sp {
        return false;
    }
    if p == Sp {
        return true;
    }
    if p == Obj || c == Obj || p == Qu || c == Qu {
        return false;
    }
    // 앞에서 못 끊는 것: 닫는 부호 · 쉼표 마침표 · 느낌표 물음표 · % · 말줄임표 · 빗금 · 붙임표 · 짧은 줄표
    if matches!(c, Cl | Is | Ex | Po | In | Sy | Hy | Ba) {
        return false;
    }
    // 뒤에서 못 끊는 것: 여는 부호 · $ +
    if matches!(p, Op | Pr) {
        return false;
    }
    match (p, c) {
        (B2, B2) => false,
        (B2, _) | (_, B2) => true,
        (Ba, _) | (In, _) => true,
        // 붙임표 뒤: 그 앞이 글자·숫자일 때 ('-5' 처럼 낱말 맨 앞이면 아님)
        (Hy, _) => i >= 2 && matches!(lb(chs[i - 2].c), Al | Nu | Id),
        (Sy, Id) => true,
        (Sy, _) => false,
        (Ex, Id) | (Ex, Op) | (Is, Id) | (Is, Op) | (Po, Id) | (Cl, Id) | (Cl, Op) => true,
        (Ex, Al) => chs[i - 1].c == '?',
        (Is, _) | (Ex, _) | (Po, _) | (Cl, _) => false,
        (Id, Op) | (Id, Pr) => true,
        (_, Op) | (_, Pr) => false,
        (Id, Id) | (Id, Al) | (Id, Nu) | (Al, Id) | (Nu, Id) => !keep_all,
        (Id, Sym) | (Sym, Id) => true,
        _ => false,
    }
}

/// 글자 열을 width 폭에 놓았을 때의 높이. strut: 빈 줄·작은 글자만 있는 줄의 기준 크기
fn layout(chs: &[Ch], width: f32, strut: f32, lh: f32, keep_all: bool, anywhere: bool) -> f32 {
    // 빈칸 정리 (white-space: normal): 이어진 빈칸은 하나, 앞뒤 빈칸은 없앰
    let mut v: Vec<Ch> = Vec::with_capacity(chs.len());
    for ch in chs {
        let mut ch = *ch;
        if ch.c == '\t' || ch.c == '\n' || ch.c == '\r' {
            ch.c = ' ';
        }
        if ch.c == ' ' && v.last().is_none_or(|p| p.c == ' ') {
            continue;
        }
        v.push(ch);
    }
    while v.last().is_some_and(|p| p.c == ' ') {
        v.pop();
    }
    if v.is_empty() {
        return strut * lh;
    }
    let mut total = 0.0f32;
    let mut line_w = 0.0f32; // 지금 줄에 놓인 폭 (끝 빈칸 포함)
    let mut line_size = strut;
    let new_line = |total: &mut f32, line_size: &mut f32| {
        *total += *line_size * lh;
        *line_size = strut;
    };
    let mut a = 0usize;
    while a < v.len() {
        // [a, b): 줄을 바꿀 수 없는 한 덩어리 (+ 뒤따르는 빈칸)
        let mut b = a + 1;
        while b < v.len() && !can_break(&v, b, keep_all) {
            b += 1;
        }
        let mut e = b;
        while e > a && v[e - 1].c == ' ' {
            e -= 1;
        }
        let cw: f32 = v[a..e].iter().map(|x| x.adv).sum();
        let sw: f32 = v[e..b].iter().map(|x| x.adv).sum();
        let seg_size = v[a..e].iter().map(|x| x.size).fold(0.0f32, f32::max);
        if line_w + cw <= width + EPS {
            line_w += cw + sw;
            line_size = line_size.max(seg_size);
        } else {
            if line_w > 0.0 {
                new_line(&mut total, &mut line_size);
            }
            if cw <= width + EPS || !anywhere {
                line_w = cw + sw;
                line_size = line_size.max(seg_size);
            } else {
                // 줄보다 긴 어절: 글자 사이에서 꺾는다 (overflow-wrap: anywhere)
                let mut x = 0.0f32;
                for ch in &v[a..e] {
                    if x > 0.0 && x + ch.adv > width + EPS {
                        new_line(&mut total, &mut line_size);
                        x = 0.0;
                    }
                    x += ch.adv;
                    line_size = line_size.max(ch.size);
                }
                line_w = x + sw;
            }
        }
        a = b;
    }
    total + line_size * lh
}

/// 어절 단위 줄바꿈 → 줄 수 (맑은 고딕, keep-all)
pub fn wrap_lines(text: &str, width: f32, size: f32, bold: bool, letter: f32) -> usize {
    if width <= 1.0 {
        return usize::MAX / 4;
    }
    let mut chs = vec![];
    push_text(&mut chs, &default_chain(bold), text, size, letter, size);
    (layout(&chs, width, size, 1.0, true, true) / size).round().max(1.0) as usize
}

fn push_text(out: &mut Vec<Ch>, chain: &Chain, s: &str, size: f32, letter: f32, line_size: f32) {
    let mut adv = Vec::with_capacity(s.len());
    chain.advances(s, size, letter, &mut adv);
    out.extend(s.chars().zip(adv).map(|(c, adv)| Ch { c, adv, size: line_size }));
}

// ---------------------------------------------------------------------------------------------
// 글 서식 (web/shared/groups.js)
// ---------------------------------------------------------------------------------------------

/// 서식 표시 지우기: **굵게** · {크기:…} · {색:…} 등
pub fn strip_marks(s: &str) -> String {
    static RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\{(크기|색|size|color):[^}]*\}|\{/[^}]*\}").unwrap());
    RE.replace_all(&s.replace("**", ""), "").to_string()
}

static FMT: Lazy<Regex> = Lazy::new(|| Regex::new(r"\*\*|\{색:(#[0-9a-fA-F]{3,8})\}|\{/색\}|\{크기:(\d{2,3})\}|\{/크기\}").unwrap());
static CHORD: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\[([A-G](?:#|b|♯|♭)?(?:maj|min|m|M|sus|dim|aug|add|°|ø)?\d{0,2}(?:(?:sus|add|maj|b|#|\+|-)\d{1,2})*(?:/[A-G](?:#|b|♯|♭)?)?)\]").unwrap());
static MULTI_SP: Lazy<Regex> = Lazy::new(|| Regex::new(r"[ \t]{2,}").unwrap());
static TR_LINE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^\s*>").unwrap());
static TR_HEAD: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*>\s?").unwrap());

/// 코드 지우기 (코드만 있던 줄은 없앤다) — groups.js stripChords
pub fn strip_chords(t: &str) -> String {
    if !CHORD.is_match(t) {
        return t.to_string();
    }
    t.split('\n')
        .filter_map(|l| {
            let had = CHORD.is_match(l);
            let s = MULTI_SP.replace_all(&CHORD.replace_all(l, ""), " ").trim_end().to_string();
            if had && s.trim().is_empty() { None } else { Some(s) }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// 서식 조각: (글, 크기 배수, 굵게) — groups.js fmtHtml 과 같은 열고 닫기 규칙
fn fmt_runs(t: &str) -> Vec<(String, f32, bool)> {
    #[derive(Clone, Copy, PartialEq)]
    enum K {
        B,
        C,
        S(f32),
    }
    let mut open: Vec<K> = vec![];
    let mut out = vec![];
    let mut last = 0;
    let state = |open: &[K]| {
        let mut scale = 1.0f32;
        let mut bold = false;
        for k in open {
            match k {
                K::B => bold = true,
                K::S(v) => scale *= v / 100.0,
                K::C => {}
            }
        }
        (scale, bold)
    };
    let close = |open: &mut Vec<K>, pred: &dyn Fn(&K) -> bool| {
        if let Some(at) = open.iter().rposition(pred) {
            open.remove(at);
        }
    };
    for m in FMT.captures_iter(t) {
        let whole = m.get(0).unwrap();
        if whole.start() > last {
            let (s, b) = state(&open);
            out.push((t[last..whole.start()].to_string(), s, b));
        }
        last = whole.end();
        let tok = whole.as_str();
        if tok == "**" {
            if open.contains(&K::B) { close(&mut open, &|k| *k == K::B) } else { open.push(K::B) }
        } else if m.get(1).is_some() {
            open.push(K::C);
        } else if tok == "{/색}" {
            close(&mut open, &|k| *k == K::C);
        } else if let Some(n) = m.get(2) {
            open.push(K::S(n.as_str().parse::<f32>().unwrap_or(100.0).clamp(30.0, 400.0)));
        } else {
            close(&mut open, &|k| matches!(k, K::S(_)));
        }
    }
    if last < t.len() {
        let (s, b) = state(&open);
        out.push((t[last..].to_string(), s, b));
    }
    out
}

// ---------------------------------------------------------------------------------------------
// 슬라이드 높이
// ---------------------------------------------------------------------------------------------

/// 슬라이드의 장절 표시 글 (refText)
pub fn ref_text(slide: &Value, t: &Value) -> String {
    let mode = t["reference"]["mode"].as_str().unwrap_or("slide");
    let s = if mode == "passage" { slide["title"].as_str().filter(|x| !x.is_empty()).or(slide["ref"].as_str()) } else { slide["ref"].as_str() };
    s.unwrap_or("").to_string()
}

/// 병행 번역 글자 크기 (web/shared/render.css .bc-par font-size)
const PAR_SCALE: f32 = 0.72;
/// 번역 줄('> …') 글자 크기 (.bc-tr)
const TR_SCALE: f32 = 0.7;

/// 세로로 쌓이는 덩어리 하나 (위·아래 여백은 이웃 덩어리와 겹친다)
struct Blk {
    h: f32,
    mt: f32,
    mb: f32,
}

fn stack(blocks: &[Blk]) -> f32 {
    let mut h = 0.0;
    let mut prev_mb = 0.0f32;
    for (i, b) in blocks.iter().enumerate() {
        h += if i == 0 { b.mt } else { prev_mb.max(b.mt) } + b.h;
        prev_mb = b.mb;
    }
    h + prev_mb
}

/// 가사만 보이는 템플릿인가 (render.js lyricsOnly)
fn lyrics_only(t: &Value) -> bool {
    match t["pptMode"].as_str().filter(|x| !x.is_empty()) {
        Some(m) => m == "lyrics",
        None => t["background"]["type"] == "transparent",
    }
}

/// slide 가 템플릿 t 에서 size(px) 글꼴로 넘치지 않는가
pub fn fits_at(slide: &Value, t: &Value, size: f32, version_name: &str) -> bool {
    let (h, area_h) = content_height(slide, t, size, version_name);
    // 브라우저: block.offsetHeight(정수로 맞춘 높이) > area.clientHeight + 0.5 이면 넘침
    h.round() <= area_h.round() + 0.5
}

/// (글 덩어리 높이, 본문 영역 높이) — 웹의 `.bc-block` 높이와 `.bc-area` clientHeight 에 해당
pub fn content_height(slide: &Value, t: &Value, size: f32, version_name: &str) -> (f32, f32) {
    let tw = num(&t["width"], 1920.0);
    let th = num(&t["height"], 1080.0);
    let safe = num(&t["safeArea"], 0.0);
    let (sx, sy) = (tw * safe / 100.0, th * safe / 100.0);
    let p = &t["padding"];
    let mut w = tw - 2.0 * sx - num(&p["left"], 160.0) - num(&p["right"], 160.0);
    let area_h = th - 2.0 * sy - num(&p["top"], 120.0) - num(&p["bottom"], 120.0);

    let mut kind = slide["kind"].as_str().unwrap_or("bible");
    // 그림·악보·영상·카메라·빈 화면은 글 덩어리가 없다. 가사만 보이는 템플릿에서 가사(caption)가 있는 그림은 글 슬라이드로 본다
    let caption = slide["caption"].as_str().filter(|_| lyrics_only(t) && matches!(kind, "score" | "image"));
    let caption_parts;
    let (parts, slide_ref): (&[Value], &str) = if let Some(c) = caption {
        if c.trim().is_empty() {
            return (0.0, area_h);
        }
        kind = "text";
        caption_parts = vec![serde_json::json!({ "text": c.trim() })];
        (&caption_parts, "")
    } else {
        if matches!(kind, "image" | "blank" | "score" | "video" | "camera") {
            return (0.0, area_h);
        }
        (slide["parts"].as_array().map(|v| v.as_slice()).unwrap_or(&[]), "\0")
    };

    let mut pad = 0.0f32;
    if t["box"]["enabled"].as_bool().unwrap_or(false) {
        let bp = num(&t["box"]["padding"], 32.0);
        w -= 2.0 * bp;
        pad = 2.0 * bp;
    }
    if w <= 1.0 {
        return (f32::MAX / 4.0, area_h);
    }
    let f = &t["font"];
    let family = f["family"].as_str().unwrap_or("Malgun Gothic");
    let weight = num(&f["weight"], 500.0) as u16;
    let lh = num(&f["lineHeight"], 1.5);
    let letter = num(&f["letterSpacing"], 0.0);
    let base = chain_for(family, weight);
    let bold = chain_for(family, 700);
    let mut blocks: Vec<Blk> = vec![];

    if kind == "title" {
        // 제목 카드: 큰 제목(1.3em 굵게, 줄 높이 1.3) + 부제(0.62em, 위 여백 0.5em, 줄 높이 1.4). 장절 줄 없음
        let s1 = size * 1.3;
        let mut c1 = vec![];
        push_text(&mut c1, &bold, slide["ref"].as_str().unwrap_or(""), s1, letter, s1);
        blocks.push(Blk { h: layout(&c1, w, s1, 1.3, true, true), mt: 0.0, mb: 0.0 });
        if let Some(sub) = slide["subtitle"].as_str().filter(|x| !x.is_empty()) {
            let s2 = size * 0.62;
            let mut c2 = vec![];
            push_text(&mut c2, &base, sub, s2, letter, s2);
            blocks.push(Blk { h: layout(&c2, w, s2, 1.4, true, true), mt: s2 * 0.5, mb: 0.0 });
        }
        return (stack(&blocks) + pad, area_h);
    }

    // 장절 줄 (본문에 붙는 위·아래만. 화면 가장자리 고정은 따로 자리라 높이에 들지 않는다)
    let r = &t["reference"];
    let rt = if slide_ref == "\0" { ref_text(slide, t) } else { slide_ref.to_string() };
    let ref_pos = r["position"].as_str().unwrap_or("");
    let ref_blk = |top: bool| -> Option<Blk> {
        if !r["show"].as_bool().unwrap_or(false) || rt.is_empty() || ref_pos != if top { "top" } else { "bottom" } {
            return None;
        }
        let rs = num(&r["size"], 44.0);
        let mut c = vec![];
        push_text(&mut c, &chain_for(family, num(&r["weight"], 700.0) as u16), &rt, rs, letter, rs);
        let h = layout(&c, w, rs, 1.25, false, false);
        Some(if top { Blk { h, mt: 0.0, mb: rs * 0.45 } } else { Blk { h, mt: rs * 0.45, mb: 0.0 } })
    };
    blocks.extend(ref_blk(true));

    if kind == "text" {
        text_blocks(&mut blocks, parts, w, size, lh, letter, &base, &bold);
    } else {
        bible_blocks(&mut blocks, parts, t, w, size, lh, letter, &base, &bold);
    }

    blocks.extend(ref_blk(false));
    let vl = &t["versionLabel"];
    if vl["show"].as_bool().unwrap_or(false) && !version_name.is_empty() {
        let vs = num(&vl["size"], 28.0);
        let mut c = vec![];
        push_text(&mut c, &base, version_name, vs, letter, vs);
        blocks.push(Blk { h: layout(&c, w, vs, 1.2, false, false), mt: vs * 0.4, mb: 0.0 });
    }
    // 꼬리말(.bc-footer)은 화면 아래에 따로 놓여(absolute) 글 덩어리 높이에 들지 않는다
    (stack(&blocks) + pad, area_h)
}

/// 가사·공지 (.bc-pre · 번역 줄 .bc-tr)
#[allow(clippy::too_many_arguments)]
fn text_blocks(blocks: &mut Vec<Blk>, parts: &[Value], w: f32, size: f32, lh: f32, letter: f32, base: &Chain, bold: &Chain) {
    // 서식 조각을 글자 열로 (줄바꿈 '\n' 은 그대로 둔다)
    let runs_to_chs = |text: &str, scale0: f32| -> Vec<Ch> {
        let mut chs = vec![];
        for (s, scale, b) in fmt_runs(text) {
            let sz = size * scale0 * scale;
            push_text(&mut chs, if b { bold } else { base }, &s, sz, letter, sz);
        }
        chs
    };
    let mut flow: Vec<Ch> = vec![]; // 번역 줄이 없는 부분들은 한 흐름으로 이어진다
    let flush = |flow: &mut Vec<Ch>, blocks: &mut Vec<Blk>| {
        if flow.is_empty() {
            return;
        }
        // white-space: pre-line — 줄바꿈은 지키고, 줄마다 따로 놓는다.
        // 덩어리 맨 끝의 줄바꿈 하나는 줄을 만들지 않는다 (브라우저: "가\n" 은 한 줄, "가\n\n" 은 두 줄)
        let end = if flow.last().is_some_and(|c| c.c == '\n') { flow.len() - 1 } else { flow.len() };
        let h: f32 = flow[..end].split(|c| c.c == '\n').map(|line| layout(line, w, size, lh, true, true)).sum();
        blocks.push(Blk { h, mt: 0.0, mb: 0.0 });
        flow.clear();
    };
    for part in parts {
        let t = strip_chords(part["text"].as_str().unwrap_or(""));
        if !TR_LINE.is_match(&t) {
            flow.extend(runs_to_chs(&t, 1.0));
            continue;
        }
        flush(&mut flow, blocks);
        // 번역 줄('> Amazing grace')은 원래 줄 밑에 작게: 줄마다 따로 선 덩어리
        for l in t.split('\n') {
            if TR_HEAD.is_match(l) {
                let ts = size * TR_SCALE;
                let chs = runs_to_chs(&TR_HEAD.replace(l, ""), TR_SCALE);
                blocks.push(Blk { h: layout(&chs, w, ts, lh, true, true), mt: ts * 0.05, mb: ts * 0.25 });
            } else {
                blocks.push(Blk { h: layout(&runs_to_chs(l, 1.0), w, size, lh, true, true), mt: 0.0, mb: 0.0 });
            }
        }
    }
    flush(&mut flow, blocks);
    if parts.is_empty() {
        blocks.push(Blk { h: 0.0, mt: 0.0, mb: 0.0 });
    }
}

/// 성경: 절 번호 · 소제목 · 병행 번역 · 절마다 줄바꿈 · 내어쓰기
#[allow(clippy::too_many_arguments)]
fn bible_blocks(blocks: &mut Vec<Blk>, parts: &[Value], t: &Value, w: f32, size: f32, lh: f32, letter: f32, base: &Chain, bold: &Chain) {
    let vn = &t["verseNumbers"];
    let show_num = vn["show"].as_bool().unwrap_or(true);
    let scale = num(&vn["scale"], 0.6);
    let heads = &t["headings"];
    let hanging = t["align"] == "hanging";
    let verse_break = t["verseBreak"].as_bool().unwrap_or(false);
    let mut flow: Vec<Ch> = vec![];
    let flush = |flow: &mut Vec<Ch>, blocks: &mut Vec<Blk>, width: f32| {
        if flow.iter().any(|c| c.c != ' ') {
            blocks.push(Blk { h: layout(flow, width, size, lh, true, true), mt: 0.0, mb: 0.0 });
        }
        flow.clear();
    };
    // 내어쓰기: [번호 칸 | 본문] 두 칸
    let digits = parts.iter().map(|p| verse_str(p).len()).max().unwrap_or(1).max(1) as f32;
    let (num_w, gap) = if hanging && show_num { ((digits * 0.6 * scale + 0.1) * size, 0.35 * size) } else { (0.0, 0.0) };
    let text_w = if hanging { (w - num_w - gap).max(1.0) } else { w };

    for (i, part) in parts.iter().enumerate() {
        let pi = part["partIndex"].as_i64().unwrap_or(0);
        let pc = part["partCount"].as_i64().unwrap_or(0);
        let first = pi == 0 || pi == 1;
        let heading = part["heading"].as_str().unwrap_or("");
        if !heading.is_empty() && heads["show"].as_bool().unwrap_or(false) && first {
            flush(&mut flow, blocks, text_w);
            let hs = num(&heads["size"], 36.0);
            let mut c = vec![];
            push_text(&mut c, base, heading, hs, letter, hs);
            // 내어쓰기에서는 본문 밖(.bc-text 바로 아래 div)이라 전체 폭
            blocks.push(Blk { h: layout(&c, w, hs, 1.3, true, true), mt: 0.0, mb: hs * 0.3 });
        }
        if hanging || verse_break {
            flush(&mut flow, blocks, text_w);
        } else if i > 0 {
            flow.push(Ch { c: ' ', adv: base.width(" ", size, letter), size });
        }
        if show_num && first && !hanging {
            // 절 번호(.bc-vn): 작은 굵은 글자 + 오른쪽 여백 0.25em, 줄 높이에는 끼지 않는다(line-height: 0)
            let ns = size * scale;
            flow.push(Ch { c: OBJ, adv: bold.width(&verse_str(part), ns, letter) + 0.25 * ns, size: 0.0 });
        }
        let mut text = String::new();
        if pi > 1 {
            text.push_str("… ");
        }
        text.push_str(part["text"].as_str().unwrap_or(""));
        if pi > 0 && pi < pc {
            text.push_str(" …");
        }
        push_text(&mut flow, base, &text, size, letter, size);
        // 병행 번역(.bc-par): 따로 선 줄(display:block) — 글자 0.72배, 위 여백 0.12em
        if let Some(t2) = part["text2"].as_str().filter(|x| !x.is_empty()) {
            flush(&mut flow, blocks, text_w);
            let ps = size * PAR_SCALE;
            let mut c = vec![];
            push_text(&mut c, base, t2, ps, letter, ps);
            blocks.push(Blk { h: layout(&c, text_w, ps, lh, true, true), mt: ps * 0.12, mb: 0.0 });
        }
        if hanging || verse_break {
            flush(&mut flow, blocks, text_w);
        }
    }
    flush(&mut flow, blocks, text_w);
}

fn verse_str(p: &Value) -> String {
    match &p["verse"] {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        v => v.to_string(),
    }
}

/// autoFit 이면 최소 크기까지 줄여 가며 들어가는 가장 큰 글꼴 (화면 그리기용)
pub fn fit_size(slide: &Value, t: &Value, version_name: &str) -> f32 {
    let size = num(&t["font"]["size"], 72.0);
    if fits_at(slide, t, size, version_name) || !t["font"]["autoFit"].as_bool().unwrap_or(true) {
        return size;
    }
    let (mut lo, mut hi) = (num(&t["font"]["minSize"], 44.0) as i32, size as i32);
    if !fits_at(slide, t, lo as f32, version_name) {
        return lo as f32;
    }
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        if fits_at(slide, t, mid as f32, version_name) { lo = mid } else { hi = mid }
    }
    lo as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tpl() -> Value {
        json!({ "width": 1920, "height": 1080, "safeArea": 0, "padding": { "left": 120, "right": 120, "top": 100, "bottom": 100 },
            "box": { "enabled": false }, "font": { "family": "'Malgun Gothic', sans-serif", "size": 64, "minSize": 64, "weight": 700, "lineHeight": 1.45, "letterSpacing": 0, "autoFit": true },
            "reference": { "show": false }, "versionLabel": { "show": false }, "verseNumbers": { "show": false }, "headings": { "show": false } })
    }

    /// [점검 A] 병행 번역은 0.72배 작은 글자로 따로 선 줄 — 본문 크기로 이어 재지 않는다
    #[test]
    fn parallel_text_is_measured_as_smaller_block() {
        let t = tpl();
        let en = "And we know that in all things God works for the good of those who love him, who have been called according to his purpose. ".repeat(5);
        let with = json!({ "kind": "bible", "parts": [{ "book": 45, "chapter": 8, "verse": 28, "text": "우리가 알거니와", "text2": en.trim() }] });
        // 같은 글을 본문에 이어 붙인 것 (옛 계산과 같은 높이)
        let joined = json!({ "kind": "bible", "parts": [{ "book": 45, "chapter": 8, "verse": 28, "text": format!("우리가 알거니와 {}", en.trim()) }] });
        assert!(!fits_at(&joined, &t, 64.0, ""), "본문 크기로는 넘쳐야 시험이 뜻이 있다");
        assert!(fits_at(&with, &t, 64.0, ""), "병행 번역은 작은 글자라 들어간다");
        // 그래도 자리는 차지한다: 병행 번역이 아주 길면 넘친다
        let long = json!({ "kind": "bible", "parts": [{ "book": 45, "chapter": 8, "verse": 28, "text": "우리가 알거니와", "text2": en.repeat(3) }] });
        assert!(!fits_at(&long, &t, 64.0, ""));
    }

    #[test]
    fn em_width() {
        // 맑은 고딕 한글 한 글자 폭은 글꼴 크기(em)와 같다
        let w = text_width("가", 100.0, false, 0.0);
        if FONTS.regular.is_some() { assert!((w - 100.0).abs() < 0.01, "가 폭 {w}"); }
    }

    /// 브라우저 실측값 (엣지, 맑은 고딕 굵게 64px): 커닝이 들어간 폭
    #[test]
    fn kerning_matches_browser() {
        if FONTS.bold.is_none() { return; }
        assert!((text_width("AVATAR", 64.0, true, 0.0) - 240.0).abs() < 0.3, "{}", text_width("AVATAR", 64.0, true, 0.0));
        assert!((text_width(" ", 64.0, true, 0.0) - 22.5).abs() < 0.01);
    }

    #[test]
    fn keep_all_breaks_only_after_spaces() {
        let c = default_chain(false);
        let mk = |s: &str| { let mut v = vec![]; push_text(&mut v, &c, s, 100.0, 0.0, 100.0); v };
        // 250px 폭: '가나다 라마바' → 어절 단위 두 줄 (글자 사이에서 꺾지 않음)
        assert_eq!(layout(&mk("가나다 라마바"), 350.0, 100.0, 1.0, true, true), 200.0);
        // 줄 끝 빈칸은 폭에 넣지 않는다: '가나다 ' 는 300px 에 들어간다
        assert_eq!(layout(&mk("가나다 라"), 300.0, 100.0, 1.0, true, true), 200.0);
        // 줄보다 긴 어절은 글자 사이에서 (overflow-wrap: anywhere)
        assert_eq!(layout(&mk("가나다라마"), 250.0, 100.0, 1.0, true, true), 300.0);
        // 보통 줄바꿈(장절 줄): 한글 글자 사이에서 꺾인다
        assert_eq!(layout(&mk("가나다 라마바"), 350.0, 100.0, 1.0, false, false), 200.0);
        assert_eq!(layout(&mk("가나다라마"), 250.0, 100.0, 1.0, false, false), 300.0);
    }

    #[test]
    fn fmt_runs_follow_groups_js() {
        let r = fmt_runs("가 **나** {크기:150}다{/크기} 라");
        assert_eq!(r.iter().map(|x| (x.0.as_str(), x.1, x.2)).collect::<Vec<_>>(), vec![("가 ", 1.0, false), ("나", 1.0, true), (" ", 1.0, false), ("다", 1.5, false), (" 라", 1.0, false)]);
        assert_eq!(strip_chords("[C]주님 [G/B]사랑\n[Am] [F]\n끝"), "주님 사랑\n끝");
    }

    /// 큰 글자가 섞인 줄은 그 줄만 높아진다 · 번역 줄은 0.7배 + 여백
    #[test]
    fn text_slide_heights() {
        let t = tpl();
        let h = |text: &str| content_height(&json!({ "kind": "text", "ref": "", "parts": [{ "text": text }] }), &t, 64.0, "").0;
        let line = 64.0 * 1.45;
        assert!((h("가나다\n라마바") - 2.0 * line).abs() < 0.01);
        assert!((h("가나다\n\n라마바") - 3.0 * line).abs() < 0.01, "빈 줄도 한 줄");
        // [점검 A] 맨 끝 줄바꿈 하나는 줄이 아니다 — 줄 단위로 나눌 때 빈 줄이 앞 장 끝에 붙는다 (웹과 같은 나눔 위치)
        assert!((h("가나다\n") - line).abs() < 0.01);
        assert!((h("가나다\n\n") - 2.0 * line).abs() < 0.01);
        assert!((h("\n가나다") - 2.0 * line).abs() < 0.01);
        assert!((h("{크기:150}가나다{/크기}\n라마바") - 2.5 * line).abs() < 0.01);
        let tr = 64.0 * 0.7;
        assert!((h("가나다\n> abc") - (line + tr * 0.05 + tr * 1.45 + tr * 0.25)).abs() < 0.01);
        // 꼬리말은 높이에 들지 않는다
        let with_footer = content_height(&json!({ "kind": "text", "ref": "", "footer": "CCLI", "parts": [{ "text": "가" }] }), &t, 64.0, "").0;
        assert!((with_footer - line).abs() < 0.01);
    }

    #[test]
    fn hanging_uses_narrower_text_column() {
        let mut t = tpl();
        t["verseNumbers"] = json!({ "show": true, "scale": 0.6 });
        let s = json!({ "kind": "bible", "parts": [{ "verse": 16, "text": "가".repeat(26) + " 나" }] });
        let normal = content_height(&s, &t, 64.0, "").0;
        t["align"] = json!("hanging");
        let hang = content_height(&s, &t, 64.0, "").0;
        // 1680px 폭: 26글자(1664px) 한 줄 + '나' → 내어쓰기는 번호 칸만큼 좁아 글자 사이에서 한 번 더 꺾인다
        assert!(hang >= normal);
    }
}
