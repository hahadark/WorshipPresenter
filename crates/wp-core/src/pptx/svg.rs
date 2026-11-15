//! [트랙 A] 슬라이드 → SVG (Electron 판 pptxsvg.js 와 같은 일, 단 글은 foreignObject 대신 <text> 로 — resvg 가 그릴 수 있게)
//! 좌표 단위는 포인트(pt), viewBox = 슬라이드 크기

use super::{Fill, Para, Rgba, Shape, Slide, TextBody};
use crate::measure::text_width;
use std::fmt::Write;

pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn col(c: &Rgba) -> String {
    c.hex()
}

pub struct Ctx<'a> {
    /// 그림 부분(ppt/media/…) → href (파일 이름 또는 data:)
    pub embed: &'a dyn Fn(&str) -> Option<String>,
    pub defs: String,
    pub n: usize,
}

impl Ctx<'_> {
    fn id(&mut self) -> String {
        self.n += 1;
        format!("g{}", self.n)
    }
}

/// 채우기 → fill="…" 속성 (그라데이션은 defs 에)
fn fill_attr(f: &Option<Fill>, cx: &mut Ctx) -> String {
    match f {
        None | Some(Fill::None) | Some(Fill::Group) => "fill=\"none\"".into(),
        Some(Fill::Solid(c)) => format!("fill=\"{}\"{}", col(c), if c.a < 1.0 { format!(" fill-opacity=\"{:.3}\"", c.a) } else { String::new() }),
        Some(Fill::Grad { angle, radial, stops }) => {
            let id = cx.id();
            let st: String = stops.iter().map(|(p, c)| format!("<stop offset=\"{:.3}\" stop-color=\"{}\" stop-opacity=\"{:.3}\"/>", p, col(c), c.a)).collect();
            if *radial {
                let _ = write!(cx.defs, "<radialGradient id=\"{id}\">{st}</radialGradient>");
            } else {
                let a = angle.to_radians();
                let (dx, dy) = (a.cos() / 2.0, a.sin() / 2.0);
                let _ = write!(cx.defs, "<linearGradient id=\"{id}\" x1=\"{:.3}\" y1=\"{:.3}\" x2=\"{:.3}\" y2=\"{:.3}\">{st}</linearGradient>", 0.5 - dx, 0.5 - dy, 0.5 + dx, 0.5 + dy);
            }
            format!("fill=\"url(#{id})\"")
        }
        Some(Fill::Blip { .. }) => "fill=\"none\"".into(),
    }
}

/// 그림 (자르기 포함) — 상자 x,y,w,h 에 가득
fn image(href: &str, x: f64, y: f64, w: f64, h: f64, crop: Option<[f64; 4]>, alpha: f64) -> String {
    let op = if alpha < 1.0 { format!(" opacity=\"{alpha:.3}\"") } else { String::new() };
    match crop {
        Some([l, t, r, b]) => {
            // 잘린 부분을 키워서 상자 밖은 잘라 낸다
            let sw = (1.0 - l - r).max(0.01);
            let sh = (1.0 - t - b).max(0.01);
            let fw = w / sw;
            let fh = h / sh;
            format!("<svg x=\"{x:.2}\" y=\"{y:.2}\" width=\"{w:.2}\" height=\"{h:.2}\"{op}><image href=\"{}\" x=\"{:.2}\" y=\"{:.2}\" width=\"{fw:.2}\" height=\"{fh:.2}\" preserveAspectRatio=\"none\"/></svg>", esc(href), -l * fw, -t * fh)
        }
        None => format!("<image href=\"{}\" x=\"{x:.2}\" y=\"{y:.2}\" width=\"{w:.2}\" height=\"{h:.2}\" preserveAspectRatio=\"none\"{op}/>", esc(href)),
    }
}

fn geom(sh: &Shape, attrs: &str) -> String {
    let (x, y, w, h) = (sh.x, sh.y, sh.w, sh.h);
    match sh.prst.as_str() {
        "ellipse" | "flowChartConnector" => format!("<ellipse cx=\"{:.2}\" cy=\"{:.2}\" rx=\"{:.2}\" ry=\"{:.2}\" {attrs}/>", x + w / 2.0, y + h / 2.0, w / 2.0, h / 2.0),
        "roundRect" | "flowChartAlternateProcess" => {
            let r = w.min(h) * sh.adj / 100000.0;
            format!("<rect x=\"{x:.2}\" y=\"{y:.2}\" width=\"{w:.2}\" height=\"{h:.2}\" rx=\"{r:.2}\" {attrs}/>")
        }
        "line" | "straightConnector1" | "bentConnector2" | "bentConnector3" => format!("<line x1=\"{x:.2}\" y1=\"{y:.2}\" x2=\"{:.2}\" y2=\"{:.2}\" {attrs}/>", x + w, y + h),
        "triangle" => format!("<polygon points=\"{:.2},{y:.2} {:.2},{:.2} {x:.2},{:.2}\" {attrs}/>", x + w / 2.0, x + w, y + h, y + h),
        "rtTriangle" => format!("<polygon points=\"{x:.2},{y:.2} {:.2},{:.2} {x:.2},{:.2}\" {attrs}/>", x + w, y + h, y + h),
        "diamond" => format!("<polygon points=\"{:.2},{y:.2} {:.2},{:.2} {:.2},{:.2} {x:.2},{:.2}\" {attrs}/>", x + w / 2.0, x + w, y + h / 2.0, x + w / 2.0, y + h, y + h / 2.0),
        "custom" => {
            let mut s = String::new();
            for (pw, ph, d, fill) in sh.custom.iter().flatten() {
                let sx = if *pw > 0.0 { w / pw } else { 1.0 };
                let sy = if *ph > 0.0 { h / ph } else { 1.0 };
                let a = if *fill { attrs.to_string() } else { attrs.replace(|_: char| false, "").split(' ').map(|p| if p.starts_with("fill=") { "fill=\"none\"" } else { p }).collect::<Vec<_>>().join(" ") };
                let _ = write!(s, "<path transform=\"translate({x:.2} {y:.2}) scale({sx:.5} {sy:.5})\" vector-effect=\"non-scaling-stroke\" d=\"{d}\" {a}/>");
            }
            s
        }
        _ => format!("<rect x=\"{x:.2}\" y=\"{y:.2}\" width=\"{w:.2}\" height=\"{h:.2}\" {attrs}/>"),
    }
}

fn transform(sh: &Shape) -> String {
    let (cx, cy) = (sh.x + sh.w / 2.0, sh.y + sh.h / 2.0);
    let mut t = String::new();
    if sh.rot != 0.0 {
        let _ = write!(t, "rotate({:.3} {cx:.2} {cy:.2}) ", sh.rot);
    }
    if sh.flip_h || sh.flip_v {
        let _ = write!(t, "translate({cx:.2} {cy:.2}) scale({} {}) translate({:.2} {:.2})", if sh.flip_h { -1 } else { 1 }, if sh.flip_v { -1 } else { 1 }, -cx, -cy);
    }
    t
}

fn is_hangul(s: &str) -> bool {
    s.chars().any(|c| ('\u{ac00}'..='\u{d7a3}').contains(&c) || ('\u{3131}'..='\u{318e}').contains(&c))
}

/// 글 조각 하나의 글꼴 목록 (한글이면 동아시아 글꼴 먼저)
pub fn font_list(latin: &str, ea: &str, text: &str) -> String {
    let mut v: Vec<&str> = vec![];
    if is_hangul(text) {
        v.extend([ea, latin]);
    } else {
        v.extend([latin, ea]);
    }
    let mut out: Vec<String> = v.into_iter().filter(|s| !s.is_empty() && !s.starts_with('+')).map(|s| format!("'{}'", s.replace('\'', ""))).collect();
    out.dedup();
    out.push("'Malgun Gothic'".into());
    out.push("sans-serif".into());
    out.join(", ")
}

/// PPT 가 쓰는 글꼴이 이 PC 에 있으면 그 글꼴로 폭을 잰다 (없으면 맑은 고딕 근사).
/// 예전에는 늘 맑은 고딕으로 재서, 폭이 다른 글꼴(구세군체 등)의 오른쪽·가운데 맞춤 글이 자리에서 벗어나고 줄바꿈이 달랐다.
fn run_width(s: &str, r: &super::Run) -> f64 {
    use ab_glyph::{Font, PxScale, ScaleFont};
    let Some(f) = run_font(s, r) else { return text_width(s, r.sz as f32, r.b, 0.0) as f64 };
    let em = f.units_per_em().unwrap_or(1000.0);
    let sf = f.as_scaled(PxScale::from(r.sz as f32 * f.height_unscaled() / em));
    s.chars()
        .map(|c| {
            let g = f.glyph_id(c);
            // 이 글꼴에 없는 글자는 대신 그려질 맑은 고딕 폭으로
            if g.0 == 0 { text_width(&c.to_string(), r.sz as f32, r.b, 0.0) as f64 } else { sf.h_advance(g) as f64 }
        })
        .sum()
}

/// 글 조각을 SVG 글로: PPT 글꼴에 없는 글자(– 등)는 맑은 고딕으로 그리게 따로 싼다.
/// (그냥 두면 그리는 쪽이 아무 글꼴이나 골라 줄표가 굵고 짧게 나왔다 — PowerPoint 는 맑은 고딕으로 바꿔 그린다)
fn run_text(s: &str, r: &super::Run) -> String {
    use ab_glyph::Font;
    let Some(f) = run_font(s, r) else { return esc(s) };
    let mut out = String::new();
    let mut buf = String::new();
    let mut miss = false;
    let mut flush = |buf: &mut String, miss: bool, out: &mut String| {
        if buf.is_empty() {
            return;
        }
        if miss {
            let _ = write!(out, "<tspan font-family=\"'Malgun Gothic', sans-serif\">{}</tspan>", esc(buf));
        } else {
            out.push_str(&esc(buf));
        }
        buf.clear();
    };
    for c in s.chars() {
        let m = !c.is_whitespace() && f.glyph_id(c).0 == 0;
        if m != miss {
            flush(&mut buf, miss, &mut out);
            miss = m;
        }
        buf.push(c);
    }
    flush(&mut buf, miss, &mut out);
    out
}

/// 이 글 조각을 그릴 PPT 글꼴 (이 PC 에 있을 때만)
fn run_font(s: &str, r: &super::Run) -> Option<ab_glyph::FontArc> {
    static CACHE: once_cell::sync::Lazy<parking_lot::Mutex<std::collections::HashMap<(String, bool), Option<ab_glyph::FontArc>>>> = once_cell::sync::Lazy::new(Default::default);
    let names = if is_hangul(s) { [&r.ea, &r.latin] } else { [&r.latin, &r.ea] };
    names.into_iter().filter(|n| !n.is_empty() && !n.starts_with('+')).find_map(|n| {
        CACHE
            .lock()
            .entry((n.to_lowercase(), r.b))
            .or_insert_with(|| {
                let bold_name = format!("{n} Bold");
                let got = if r.b && !n.to_lowercase().ends_with(" bold") { super::build::font_bytes(&bold_name).or_else(|| super::build::font_bytes(n)) } else { super::build::font_bytes(n) };
                got.and_then(|(data, index)| ab_glyph::FontVec::try_from_vec_and_index(data, index).ok()).map(ab_glyph::FontArc::new)
            })
            .clone()
    })
}

struct Piece {    text: String,
    run: usize,
    w: f64,
}

/// 문단 하나를 폭에 맞춰 줄로 (어절 단위, 긴 어절은 글자 단위)
fn wrap_para(p: &Para, width: f64, wrap: bool) -> Vec<Vec<Piece>> {
    let mut lines: Vec<Vec<Piece>> = vec![vec![]];
    let mut cur = 0.0;
    let first_indent = (p.mar_l + p.indent).max(0.0);
    let mut limit = (width - first_indent).max(1.0);
    let mut push = |lines: &mut Vec<Vec<Piece>>, cur: &mut f64, limit: &mut f64, piece: Piece| {
        if wrap && *cur > 0.0 && *cur + piece.w > *limit + 0.5 && !piece.text.trim().is_empty() {
            lines.push(vec![]);
            *cur = 0.0;
            *limit = (width - p.mar_l).max(1.0);
            if piece.text.trim().is_empty() {
                return;
            }
        }
        *cur += piece.w;
        lines.last_mut().unwrap().push(piece);
    };
    for (ri, r) in p.runs.iter().enumerate() {
        if r.br {
            lines.push(vec![]);
            cur = 0.0;
            limit = (width - p.mar_l).max(1.0);
            continue;
        }
        // 공백을 붙인 어절들로
        let mut word = String::new();
        let flush = |word: &mut String, lines: &mut Vec<Vec<Piece>>, cur: &mut f64, limit: &mut f64| {
            if word.is_empty() {
                return;
            }
            let w = run_width(word, r);
            if wrap && w > (width - p.mar_l).max(1.0) {
                // 긴 어절: 글자 단위
                for ch in word.chars() {
                    let s = ch.to_string();
                    let cw = run_width(&s, r);
                    push(lines, cur, limit, Piece { text: s, run: ri, w: cw });
                }
            } else {
                push(lines, cur, limit, Piece { text: word.clone(), run: ri, w });
            }
            word.clear();
        };
        for ch in r.text.chars() {
            if ch == '\n' || ch == '\u{b}' {
                flush(&mut word, &mut lines, &mut cur, &mut limit);
                lines.push(vec![]);
                cur = 0.0;
                limit = (width - p.mar_l).max(1.0);
                continue;
            }
            word.push(ch);
            if ch == ' ' {
                flush(&mut word, &mut lines, &mut cur, &mut limit);
            }
        }
        flush(&mut word, &mut lines, &mut cur, &mut limit);
    }
    lines
}

/// 글 몸통 → <text> 들. 상자 x,y,w,h (pt)
pub fn text_svg(t: &TextBody, x: f64, y: f64, w: f64, h: f64) -> String {
    let b = &t.body;
    let inner_w = (w - b.l_ins - b.r_ins).max(1.0);
    struct L {
        pieces: Vec<Piece>,
        h: f64,
        asc: f64,
        para: usize,
        first: bool,
    }
    let mut all: Vec<L> = vec![];
    let mut gaps: Vec<f64> = vec![];
    for (pi, p) in t.paras.iter().enumerate() {
        let lines = wrap_para(p, inner_w, b.wrap);
        let n = lines.len();
        for (li, l) in lines.into_iter().enumerate() {
            let sz = l.iter().map(|pc| p.runs[pc.run].sz).fold(0.0, f64::max).max(if l.is_empty() { p.end_sz } else { 0.0 }).max(1.0);
            let lh = match (p.ln_spc_pct, p.ln_spc_pts) {
                (_, Some(pts)) => pts,
                (Some(pct), _) => sz * 1.2 * pct,
                _ => sz * 1.2,
            };
            // 줄 간격을 %로 넓히면 PowerPoint 는 늘어난 만큼을 줄 위쪽에도 둔다 (첫 줄부터 아래로 내려감). 예전에는 1배로 막아 글이 위로 붙었다
            let k = lh / (sz * 1.2);
            let asc_k = if p.ln_spc_pct.is_some() && p.ln_spc_pts.is_none() && k > 1.0 { k } else { k.min(1.0).max(0.75) };
            gaps.push(if li == 0 && pi > 0 { p.spc_bef + t.paras[pi - 1].spc_aft } else { 0.0 });
            all.push(L { pieces: l, h: lh, asc: sz * 0.92 * asc_k, para: pi, first: li == 0 });
            let _ = n;
        }
    }
    let total: f64 = all.iter().map(|l| l.h).sum::<f64>() + gaps.iter().sum::<f64>();
    let avail = h - b.t_ins - b.b_ins;
    let mut cy = y + b.t_ins
        + match b.anchor.as_str() {
            "ctr" => (avail - total) / 2.0,
            "b" => avail - total,
            _ => 0.0,
        };
    let mut out = String::new();
    for (i, l) in all.iter().enumerate() {
        cy += gaps[i];
        let p = &t.paras[l.para];
        let base = cy + l.asc;
        cy += l.h;
        let mut pieces: Vec<&Piece> = l.pieces.iter().collect();
        while pieces.last().is_some_and(|pc| pc.text.trim().is_empty()) {
            pieces.pop();
        }
        if pieces.is_empty() {
            continue;
        }
        let lw: f64 = pieces.iter().map(|pc| pc.w).sum();
        // 첫 줄: 글머리표는 marL+indent 자리, 글은 marL (글머리표가 더 길면 그 뒤)
        let r0 = &p.runs[pieces[0].run];
        let bw = if l.first && !p.bullet.is_empty() { run_width(&p.bullet, r0) + r0.sz * 0.25 } else { 0.0 };
        let indent = if l.first {
            if p.bullet.is_empty() { p.mar_l + p.indent } else { p.mar_l.max(p.mar_l + p.indent + bw) }
        } else {
            p.mar_l
        };
        let left = x + b.l_ins + indent.max(0.0);
        let room = inner_w - indent.max(0.0);
        let mut px = match p.algn.as_str() {
            "ctr" | "dist" => left + (room - lw) / 2.0,
            "r" => left + room - lw,
            _ => left,
        };
        if l.first && !p.bullet.is_empty() {
            let bx = if p.algn == "l" || p.algn.is_empty() { x + b.l_ins + (p.mar_l + p.indent).max(0.0) } else { px - bw };
            let _ = write!(out, "<text x=\"{bx:.2}\" y=\"{base:.2}\" font-size=\"{:.2}\" font-family=\"{}\" fill=\"{}\">{}</text>", r0.sz, esc(&font_list(&r0.latin, &r0.ea, &p.bullet)), r0.color.map(|c| col(&c)).unwrap_or("#000000".into()), esc(&p.bullet));
        }
        // 같은 글 조각끼리 묶어 <text>
        let mut k = 0;
        while k < pieces.len() {
            let ri = pieces[k].run;
            let mut s = String::new();
            let mut sw = 0.0;
            while k < pieces.len() && pieces[k].run == ri {
                s.push_str(&pieces[k].text);
                sw += pieces[k].w;
                k += 1;
            }
            let r = &p.runs[ri];
            let fill = r.color.unwrap_or(Rgba { r: 0.0, g: 0.0, b: 0.0, a: 1.0 });
            let _ = write!(
                out,
                "<text x=\"{px:.2}\" y=\"{base:.2}\" font-size=\"{:.2}\" font-family=\"{}\"{}{}{} fill=\"{}\"{} xml:space=\"preserve\">{}</text>",
                r.sz,
                esc(&font_list(&r.latin, &r.ea, &s)),
                if r.b { " font-weight=\"bold\"" } else { "" },
                if r.i { " font-style=\"italic\"" } else { "" },
                if r.u { " text-decoration=\"underline\"" } else { "" },
                col(&fill),
                if fill.a < 1.0 { format!(" fill-opacity=\"{:.3}\"", fill.a) } else { String::new() },
                run_text(&s, r)
            );
            px += sw;
        }
    }
    out
}

/// 도형 하나 → SVG 조각
pub fn shape_svg(sh: &Shape, cx: &mut Ctx) -> String {
    let mut body = String::new();
    if let Some(Fill::Blip { part, crop, alpha }) = &sh.pic {
        if let Some(h) = part.as_deref().and_then(|p| (cx.embed)(p)) {
            body.push_str(&image(&h, sh.x, sh.y, sh.w, sh.h, *crop, *alpha));
        }
    }
    if let Some(tb) = &sh.table {
        let mut yy = sh.y;
        for (ri, (rh, cells)) in tb.rows.iter().enumerate() {
            let mut xx = sh.x;
            for (ci, (t, f, merged)) in cells.iter().enumerate() {
                let cw = tb.cols.get(ci).copied().unwrap_or(0.0);
                if !merged {
                    let fill = f.clone().or_else(|| tb.accent.map(|a| if ri == 0 { Fill::Solid(a) } else { Fill::Solid(Rgba { a: if ri % 2 == 1 { 0.2 } else { 0.08 }, ..a }) }));
                    let fa = fill_attr(&fill, cx);
                    let _ = write!(body, "<rect x=\"{xx:.2}\" y=\"{yy:.2}\" width=\"{cw:.2}\" height=\"{rh:.2}\" {fa} stroke=\"#ffffff\" stroke-width=\"0.75\"/>");
                    if let Some(t) = t {
                        let mut t = t.clone();
                        if ri == 0 && tb.accent.is_some() {
                            for p in t.paras.iter_mut() {
                                for r in p.runs.iter_mut() {
                                    if r.color.is_none() {
                                        r.color = Some(Rgba { r: 255.0, g: 255.0, b: 255.0, a: 1.0 });
                                        r.b = true;
                                    }
                                }
                            }
                        }
                        t.body.l_ins = 7.2;
                        t.body.r_ins = 7.2;
                        body.push_str(&text_svg(&t, xx, yy, cw, *rh));
                    }
                }
                xx += cw;
            }
            yy += rh;
        }
    }
    if sh.kind != "pic" && sh.kind != "table" && !sh.prst.is_empty() {
        let mut attrs = fill_attr(&sh.fill, cx);
        if let Some(l) = &sh.line {
            let _ = write!(attrs, " stroke=\"{}\" stroke-width=\"{:.2}\"{}", col(&l.color), l.w.max(0.25), if l.dash.is_empty() { String::new() } else { format!(" stroke-dasharray=\"{:.1} {:.1}\"", l.w * 4.0, l.w * 3.0) });
        }
        let has_body = sh.fill.is_some() || sh.line.is_some();
        if let Some(Fill::Blip { part, crop, alpha }) = &sh.fill {
            if let Some(h) = part.as_deref().and_then(|p| (cx.embed)(p)) {
                body.push_str(&image(&h, sh.x, sh.y, sh.w, sh.h, *crop, *alpha));
            }
        }
        if has_body && !matches!(sh.fill, Some(Fill::Blip { .. })) {
            body.push_str(&geom(sh, &attrs));
        } else if sh.line.is_some() {
            body.push_str(&geom(sh, &attrs.replace(&fill_attr(&sh.fill, &mut Ctx { embed: cx.embed, defs: String::new(), n: 0 }), "fill=\"none\"")));
        }
    }
    // [트랙 A] 차트
    if let Some(c) = &sh.chart {
        body.push_str(&super::chart::chart_svg(c, sh.x, sh.y, sh.w, sh.h, &c.theme));
    }
    if let Some(t) = sh.text.as_ref().filter(|t| t.has_text) {
        body.push_str(&text_svg(t, sh.x, sh.y, sh.w, sh.h));
    }
    if body.is_empty() {
        return body;
    }
    let tr = transform(sh);
    if tr.is_empty() { body } else { format!("<g transform=\"{tr}\">{body}</g>") }
}

/// 슬라이드 → SVG 문서. skip(i) 가 참인 도형은 뺀다 (요소를 꺼낸 배경 그림)
pub fn slide_svg(s: &Slide, embed: &dyn Fn(&str) -> Option<String>, skip: &dyn Fn(usize) -> bool) -> String {
    let mut cx = Ctx { embed, defs: String::new(), n: 0 };
    let mut body = String::new();
    // 배경
    match &s.bg {
        Fill::Blip { part, crop, alpha } => {
            body.push_str(&format!("<rect width=\"{:.2}\" height=\"{:.2}\" fill=\"#ffffff\"/>", s.w, s.h));
            if let Some(h) = part.as_deref().and_then(embed) {
                body.push_str(&image(&h, 0.0, 0.0, s.w, s.h, *crop, *alpha));
            }
        }
        f => {
            let fa = fill_attr(&Some(f.clone()), &mut cx);
            body.push_str(&format!("<rect width=\"{:.2}\" height=\"{:.2}\" {fa}/>", s.w, s.h));
        }
    }
    for (i, sh) in s.shapes.iter().enumerate() {
        if skip(i) {
            continue;
        }
        body.push_str(&shape_svg(sh, &mut cx));
    }
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" viewBox=\"0 0 {w:.2} {h:.2}\" width=\"{w:.2}\" height=\"{h:.2}\" preserveAspectRatio=\"xMidYMid meet\"><defs>{}</defs>{body}</svg>",
        cx.defs,
        w = s.w,
        h = s.h
    )
}

/// 요소 하나짜리 그림 (선·그라데이션·특수 모양) → (SVG, 상자)
pub fn element_svg(sh: &Shape, embed: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    let mut cx = Ctx { embed, defs: String::new(), n: 0 };
    let mut s2 = sh.clone();
    s2.text = None; // 글은 따로 글상자로 꺼낸다
    let pad = sh.line.as_ref().map(|l| l.w).unwrap_or(0.0) + 1.0;
    let body = shape_svg(&s2, &mut cx);
    if body.is_empty() {
        return None;
    }
    Some(format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{:.2} {:.2} {:.2} {:.2}\" width=\"{:.2}\" height=\"{:.2}\"><defs>{}</defs>{body}</svg>",
        sh.x - pad,
        sh.y - pad,
        sh.w + 2.0 * pad,
        sh.h + 2.0 * pad,
        sh.w + 2.0 * pad,
        sh.h + 2.0 * pad,
        cx.defs
    ))
}
