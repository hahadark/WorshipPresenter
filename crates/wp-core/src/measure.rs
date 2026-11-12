//! 슬라이드 글이 템플릿 화면에 들어가는지 재기 (Electron 판은 실제 DOM 으로 쟀다)
//! render.css 규칙을 따라 근사: word-break keep-all(어절 단위) + overflow-wrap anywhere(긴 어절은 글자 단위),
//! 장절 줄 · 번역본 이름 · 소제목 · 절 번호 · 상자 여백을 더한 높이가 본문 영역 높이를 넘는지 본다.

use ab_glyph::{Font, FontArc, PxScale, ScaleFont};
use once_cell::sync::Lazy;
use serde_json::Value;

pub struct Fonts {
    regular: Option<FontArc>,
    bold: Option<FontArc>,
}

fn load(paths: &[&str]) -> Option<FontArc> {
    for p in paths {
        if let Ok(bytes) = std::fs::read(p) {
            if let Ok(f) = FontArc::try_from_vec(bytes) {
                return Some(f);
            }
        }
    }
    None
}

pub static FONTS: Lazy<Fonts> = Lazy::new(|| Fonts {
    regular: load(&[r"C:\Windows\Fonts\malgun.ttf", "/System/Library/Fonts/AppleSDGothicNeo.ttc", "/usr/share/fonts/truetype/nanum/NanumGothic.ttf"]),
    bold: load(&[r"C:\Windows\Fonts\malgunbd.ttf", r"C:\Windows\Fonts\malgun.ttf"]),
});

fn num(v: &Value, d: f32) -> f32 {
    v.as_f64().map(|x| x as f32).unwrap_or(d)
}

/// 글자 폭 (px)
pub fn text_width(s: &str, size: f32, bold: bool, letter: f32) -> f32 {
    let font = if bold { FONTS.bold.as_ref().or(FONTS.regular.as_ref()) } else { FONTS.regular.as_ref() };
    match font {
        Some(f) => {
            let sf = f.as_scaled(PxScale::from(size));
            s.chars().map(|c| sf.h_advance(f.glyph_id(c)) + letter).sum()
        }
        None => s
            .chars()
            .map(|c| if (c as u32) > 0x2e80 { size * 0.95 } else if c == ' ' { size * 0.3 } else { size * 0.55 } + letter)
            .sum(),
    }
}

/// 어절 단위 줄바꿈 → 줄 수 (첫 줄 앞에 indent 폭이 있으면 같이)
pub fn wrap_lines(text: &str, width: f32, size: f32, bold: bool, letter: f32) -> usize {
    if width <= 1.0 {
        return usize::MAX / 4;
    }
    let space = text_width(" ", size, bold, letter);
    let mut lines = 1usize;
    let mut cur = 0.0f32;
    for word in text.split(' ').filter(|w| !w.is_empty()) {
        let w = text_width(word, size, bold, letter);
        if cur > 0.0 && cur + space + w <= width + 0.5 {
            cur += space + w;
            continue;
        }
        if cur > 0.0 {
            lines += 1;
            cur = 0.0;
        }
        if w <= width + 0.5 {
            cur = w;
        } else {
            // 긴 어절: 글자 단위로 꺾는다
            for ch in word.chars() {
                let cw = text_width(&ch.to_string(), size, bold, letter);
                if cur + cw > width + 0.5 && cur > 0.0 {
                    lines += 1;
                    cur = 0.0;
                }
                cur += cw;
            }
        }
    }
    lines
}

/// 서식 표시 지우기: **굵게** · {크기:…} · {색:…} · 화음 [C] 등
pub fn strip_marks(s: &str) -> String {
    let s = s.replace("**", "");
    let re = regex::Regex::new(r"\{(크기|색|size|color):[^}]*\}|\{/[^}]*\}").unwrap();
    re.replace_all(&s, "").to_string()
}

/// 슬라이드의 장절 표시 글 (refText)
pub fn ref_text(slide: &Value, t: &Value) -> String {
    let mode = t["reference"]["mode"].as_str().unwrap_or("slide");
    let s = if mode == "passage" { slide["title"].as_str().filter(|x| !x.is_empty()).or(slide["ref"].as_str()) } else { slide["ref"].as_str() };
    s.unwrap_or("").to_string()
}

/// slide 가 템플릿 t 에서 size(px) 글꼴로 넘치지 않는가
pub fn fits_at(slide: &Value, t: &Value, size: f32, version_name: &str) -> bool {
    let tw = num(&t["width"], 1920.0);
    let th = num(&t["height"], 1080.0);
    let safe = num(&t["safeArea"], 0.0);
    let (sx, sy) = (tw * safe / 100.0, th * safe / 100.0);
    let p = &t["padding"];
    let mut w = tw - 2.0 * sx - num(&p["left"], 160.0) - num(&p["right"], 160.0);
    let area_h = th - 2.0 * sy - num(&p["top"], 120.0) - num(&p["bottom"], 120.0);
    let mut h = 0.0f32;
    if t["box"]["enabled"].as_bool().unwrap_or(false) {
        let bp = num(&t["box"]["padding"], 32.0);
        w -= 2.0 * bp;
        h += 2.0 * bp;
    }
    let f = &t["font"];
    let bold = num(&f["weight"], 500.0) >= 600.0;
    let lh = num(&f["lineHeight"], 1.5);
    let letter = num(&f["letterSpacing"], 0.0);
    let line = size * lh;

    // 장절 줄 (본문에 붙는 위·아래만, 화면 가장자리 고정은 따로 자리)
    let r = &t["reference"];
    let rt = ref_text(slide, t);
    if r["show"].as_bool().unwrap_or(false) && matches!(r["position"].as_str(), Some("top") | Some("bottom")) && !rt.is_empty() {
        let rs = num(&r["size"], 44.0);
        let n = wrap_lines(&rt, w, rs, num(&r["weight"], 700.0) >= 600.0, 0.0);
        h += n as f32 * rs * 1.25 + rs * 0.45;
    }
    let vl = &t["versionLabel"];
    if vl["show"].as_bool().unwrap_or(false) && !version_name.is_empty() {
        let vs = num(&vl["size"], 28.0);
        h += vs * 1.2 + vs * 0.4;
    }

    let kind = slide["kind"].as_str().unwrap_or("bible");
    let parts = slide["parts"].as_array().cloned().unwrap_or_default();
    if kind == "text" {
        let mut n = 0usize;
        for part in &parts {
            let txt = strip_marks(part["text"].as_str().unwrap_or(""));
            for l in txt.split('\n') {
                let l = l.trim_start_matches('>').trim();
                n += if l.is_empty() { 1 } else { wrap_lines(l, w, size, bold, letter) };
            }
        }
        h += n as f32 * line;
    } else {
        let vn = &t["verseNumbers"];
        let show_num = vn["show"].as_bool().unwrap_or(true);
        let scale = num(&vn["scale"], 0.6);
        let heads = &t["headings"];
        let verse_break = t["verseBreak"].as_bool().unwrap_or(false) || t["align"] == "hanging";
        let mut flow = String::new();
        let mut n_lines = 0usize;
        let flush = |flow: &mut String, n: &mut usize| {
            if !flow.trim().is_empty() {
                *n += wrap_lines(flow.trim(), w, size, bold, letter);
            }
            flow.clear();
        };
        for part in &parts {
            let pi = part["partIndex"].as_i64().unwrap_or(0);
            let pc = part["partCount"].as_i64().unwrap_or(0);
            let heading = part["heading"].as_str().unwrap_or("");
            if !heading.is_empty() && heads["show"].as_bool().unwrap_or(false) && (pi == 0 || pi == 1) {
                flush(&mut flow, &mut n_lines);
                let hs = num(&heads["size"], 36.0);
                h += wrap_lines(heading, w, hs, false, 0.0) as f32 * hs * 1.3 + hs * 0.3;
            }
            let mut piece = String::new();
            if show_num && (pi == 0 || pi == 1) {
                // 절 번호는 작은 글자 → 본문 글자 폭으로 환산해 자리만 차지
                let digits = part["verse"].to_string().len();
                let nw = digits as f32 * 0.6 * scale * size + 0.25 * size;
                let per = text_width("가", size, bold, letter).max(1.0);
                piece.push_str(&"가".repeat(((nw / per).ceil() as usize).max(1)));
            }
            if pi > 1 {
                piece.push_str("… ");
            }
            piece.push_str(part["text"].as_str().unwrap_or(""));
            if pi > 0 && pi < pc {
                piece.push_str(" …");
            }
            if let Some(t2) = part["text2"].as_str().filter(|x| !x.is_empty()) {
                piece.push(' ');
                piece.push_str(t2);
            }
            if verse_break {
                flush(&mut flow, &mut n_lines);
                flow.push_str(&piece);
                flush(&mut flow, &mut n_lines);
            } else {
                if !flow.is_empty() {
                    flow.push(' ');
                }
                flow.push_str(&piece);
            }
        }
        flush(&mut flow, &mut n_lines);
        h += n_lines as f32 * line;
    }
    if slide["footer"].as_str().is_some_and(|x| !x.is_empty()) {
        h += size * 0.5 * 1.4;
    }
    h <= area_h + 0.5
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
