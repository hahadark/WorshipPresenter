//! [트랙 A] .pptx/.ppsx 를 PowerPoint 없이 직접 읽는다 (Electron 판 pptxparse.js 를 옮김 — 직접 쓴 파서 경로)
//! 글상자(문단·글 조각·크기·색·굵게·정렬·여백·줄 간격·글머리표) · 개체 틀 상속(레이아웃 → 마스터 → 마스터 글 스타일 → 기본 글 스타일)
//! · 테마 색·글꼴 · 채우기(단색·그라데이션·그림) · 선 · 그룹 좌표 · 그림(자르기) · 표 · 배경(슬라이드 → 레이아웃 → 마스터)
//! 그림 만들기는 svg.rs, 캐시·요소 꺼내기는 build.rs · layers.rs

pub mod build;
pub mod chart; // [트랙 A] 차트 기본 그리기
pub mod layers;
pub mod svg;

use roxmltree::{Document, Node};
use std::collections::HashMap;
use std::io::Read;

pub const EMU_PT: f64 = 12700.0;

// ---------------- 노드 도우미 (접두사 뺀 이름으로) ----------------
pub fn ln<'a>(n: &Node<'a, 'a>) -> &'a str {
    n.tag_name().name()
}
pub fn kid<'a>(n: Option<Node<'a, 'a>>, name: &str) -> Option<Node<'a, 'a>> {
    n?.children().find(|c| c.is_element() && ln(c) == name)
}
pub fn kids<'a>(n: Option<Node<'a, 'a>>, name: &'a str) -> Vec<Node<'a, 'a>> {
    n.map(|n| n.children().filter(|c| c.is_element() && ln(c) == name).collect()).unwrap_or_default()
}
pub fn find<'a>(n: Option<Node<'a, 'a>>, name: &str) -> Option<Node<'a, 'a>> {
    n?.descendants().find(|c| c.is_element() && ln(c) == name)
}
pub fn attr<'a>(n: Option<Node<'a, 'a>>, name: &str) -> Option<&'a str> {
    let n = n?;
    n.attributes().find(|a| a.name() == name).map(|a| a.value())
}
pub fn num(n: Option<Node>, name: &str, d: f64) -> f64 {
    attr(n, name).and_then(|v| v.parse::<f64>().ok()).unwrap_or(d)
}
pub fn text_of(n: Option<Node>) -> String {
    n.map(|n| n.descendants().filter(|c| c.is_text()).map(|c| c.text().unwrap_or("")).collect()).unwrap_or_default()
}

// ---------------- 패키지 ----------------
pub struct Pkg {
    pub files: HashMap<String, Vec<u8>>,
    xml: HashMap<String, String>,
}

fn join_path(from: &str, target: &str) -> String {
    if let Some(t) = target.strip_prefix('/') {
        return t.to_string();
    }
    let mut parts: Vec<&str> = from.split('/').collect();
    parts.pop();
    for seg in target.split('/') {
        match seg {
            ".." => {
                parts.pop();
            }
            "." | "" => {}
            s => parts.push(s),
        }
    }
    parts.join("/")
}

#[derive(Clone, Debug)]
pub struct Rel {
    pub ty: String,
    pub target: String,
    pub external: bool,
}

impl Pkg {
    pub fn open(bytes: &[u8]) -> Result<Self, String> {
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| format!("pptx(zip) 를 열 수 없습니다: {e}"))?;
        let mut files = HashMap::new();
        for i in 0..z.len() {
            let Ok(mut f) = z.by_index(i) else { continue };
            if f.is_dir() {
                continue;
            }
            let Ok(name) = f.name().map(|n| n.to_string()) else { continue };
            let mut buf = Vec::with_capacity(f.size() as usize);
            if f.read_to_end(&mut buf).is_ok() {
                files.insert(name, buf);
            }
        }
        Ok(Self { files, xml: HashMap::new() })
    }
    pub fn has(&self, name: &str) -> bool {
        self.files.contains_key(name)
    }
    pub fn raw(&self, name: &str) -> Option<&[u8]> {
        self.files.get(name).map(|v| v.as_slice())
    }
    /// XML 글 (같은 부분을 여러 번 읽으니 기억해 둔다)
    pub fn text(&mut self, name: &str) -> Option<String> {
        if let Some(s) = self.xml.get(name) {
            return Some(s.clone());
        }
        let b = self.files.get(name)?;
        let s = String::from_utf8_lossy(b).trim_start_matches('\u{feff}').to_string();
        self.xml.insert(name.to_string(), s.clone());
        Some(s)
    }
    pub fn rels(&mut self, part: &str) -> Vec<(String, Rel)> {
        let (dir, file) = part.rsplit_once('/').unwrap_or(("", part));
        let rel_name = if dir.is_empty() { format!("_rels/{file}.rels") } else { format!("{dir}/_rels/{file}.rels") };
        let Some(t) = self.text(&rel_name) else { return vec![] };
        let Ok(d) = Document::parse(&t) else { return vec![] };
        d.root_element()
            .children()
            .filter(|c| c.is_element() && ln(c) == "Relationship")
            .map(|r| {
                let ext = r.attribute("TargetMode") == Some("External");
                let tgt = r.attribute("Target").unwrap_or("");
                let ty = r.attribute("Type").unwrap_or("").rsplit('/').next().unwrap_or("").to_string();
                (r.attribute("Id").unwrap_or("").to_string(), Rel { ty, target: if ext { tgt.to_string() } else { join_path(part, tgt) }, external: ext })
            })
            .collect()
    }
    pub fn rel_target(&mut self, part: &str, id: &str) -> Option<String> {
        self.rels(part).into_iter().find(|(i, r)| i == id && !r.external).map(|(_, r)| r.target)
    }
    pub fn rel_by_type(&mut self, part: &str, ty: &str) -> Option<String> {
        self.rels(part).into_iter().find(|(_, r)| r.ty == ty && !r.external).map(|(_, r)| r.target)
    }
}

// ---------------- 색 ----------------
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}
impl Rgba {
    pub fn hex(&self) -> String {
        let c = |v: f64| v.round().clamp(0.0, 255.0) as u8;
        format!("#{:02x}{:02x}{:02x}", c(self.r), c(self.g), c(self.b))
    }
    fn from_hex(h: &str) -> Option<Self> {
        if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let p = |i| u8::from_str_radix(&h[i..i + 2], 16).unwrap() as f64;
        Some(Self { r: p(0), g: p(2), b: p(4), a: 1.0 })
    }
}

fn rgb_to_hsl(r: f64, g: f64, b: f64) -> (f64, f64, f64) {
    let (r, g, b) = (r / 255.0, g / 255.0, b / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if max == min {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 { d / (2.0 - max - min) } else { d / (max + min) };
    let h = if max == r { (g - b) / d + if g < b { 6.0 } else { 0.0 } } else if max == g { (b - r) / d + 2.0 } else { (r - g) / d + 4.0 };
    (h / 6.0, s, l)
}
fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (f64, f64, f64) {
    if s == 0.0 {
        return (l * 255.0, l * 255.0, l * 255.0);
    }
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let f = |t: f64| {
        let t = (t + 1.0) % 1.0;
        if t < 1.0 / 6.0 { p + (q - p) * 6.0 * t } else if t < 0.5 { q } else if t < 2.0 / 3.0 { p + (q - p) * (2.0 / 3.0 - t) * 6.0 } else { p }
    };
    (f(h + 1.0 / 3.0) * 255.0, f(h) * 255.0, f(h - 1.0 / 3.0) * 255.0)
}

fn prst_clr(v: &str) -> &'static str {
    match v {
        "black" => "000000", "white" => "FFFFFF", "red" => "FF0000", "green" => "008000", "blue" => "0000FF", "yellow" => "FFFF00",
        "gray" | "grey" | "medGray" => "808080", "darkGray" | "dkGray" => "A9A9A9", "lightGray" | "ltGray" => "D3D3D3", "orange" => "FFA500",
        "purple" => "800080", "navy" => "000080", "silver" => "C0C0C0", "maroon" => "800000", "olive" => "808000", "teal" => "008080",
        "aqua" | "cyan" => "00FFFF", "magenta" | "fuchsia" => "FF00FF", "lime" => "00FF00", "gold" => "FFD700", "pink" => "FFC0CB",
        "brown" => "A52A2A", "darkBlue" => "00008B", "darkRed" => "8B0000", "darkGreen" => "006400", "lightBlue" => "ADD8E6", _ => "000000",
    }
}

#[derive(Clone, Default, Debug)]
pub struct Theme {
    pub colors: HashMap<String, String>,
    pub major: (String, String),
    pub minor: (String, String),
    pub fills: Vec<String>,   // fmtScheme 채우기 노드 XML (참조용 — 단순화: 첫 색만)
    pub bg_fills: Vec<String>,
}

#[derive(Clone, Default, Debug)]
pub struct Ctx {
    pub part: String,
    pub theme: Theme,
    pub clr_map: HashMap<String, String>,
}

const COLOR_KIDS: [&str; 6] = ["srgbClr", "schemeClr", "sysClr", "prstClr", "scrgbClr", "hslClr"];
pub fn color_kid<'a>(n: Option<Node<'a, 'a>>) -> Option<Node<'a, 'a>> {
    n?.children().find(|c| c.is_element() && COLOR_KIDS.contains(&ln(c)))
}

/// 색 노드 → 색 (밝기·틴트·투명 보정 포함)
pub fn resolve_color(n: Option<Node>, ctx: Option<&Ctx>, ph: Option<Rgba>) -> Option<Rgba> {
    let n = n?;
    let v = n.attribute("val").unwrap_or("");
    let mut c = match ln(&n) {
        "srgbClr" => Rgba::from_hex(v)?,
        "sysClr" => Rgba::from_hex(n.attribute("lastClr").unwrap_or(if v == "window" { "FFFFFF" } else { "000000" }))?,
        "prstClr" => Rgba::from_hex(prst_clr(v))?,
        "scrgbClr" => {
            let g = |k| num(Some(n), k, 0.0) / 100000.0 * 255.0;
            Rgba { r: g("r"), g: g("g"), b: g("b"), a: 1.0 }
        }
        "hslClr" => {
            let (r, g, b) = hsl_to_rgb(num(Some(n), "hue", 0.0) / 21600000.0, num(Some(n), "sat", 0.0) / 100000.0, num(Some(n), "lum", 0.0) / 100000.0);
            Rgba { r, g, b, a: 1.0 }
        }
        "schemeClr" => {
            if v == "phClr" {
                ph?
            } else {
                let ctx = ctx?;
                let key = ctx.clr_map.get(v).map(|s| s.as_str()).unwrap_or(v);
                let hex = ctx.theme.colors.get(key).or(ctx.theme.colors.get(v))?;
                Rgba::from_hex(hex)?
            }
        }
        _ => return None,
    };
    for m in n.children().filter(|x| x.is_element()) {
        let val = num(Some(m), "val", 0.0) / 100000.0;
        match ln(&m) {
            "alpha" => c.a = val.clamp(0.0, 1.0),
            "tint" => {
                c.r = 255.0 - (255.0 - c.r) * val;
                c.g = 255.0 - (255.0 - c.g) * val;
                c.b = 255.0 - (255.0 - c.b) * val;
            }
            "shade" => {
                c.r *= val;
                c.g *= val;
                c.b *= val;
            }
            k @ ("lumMod" | "lumOff" | "satMod" | "hueMod") => {
                let (mut h, mut s, mut l) = rgb_to_hsl(c.r, c.g, c.b);
                match k {
                    "lumMod" => l *= val,
                    "lumOff" => l += val,
                    "satMod" => s *= val,
                    _ => h = (h * val) % 1.0,
                }
                let (r, g, b) = hsl_to_rgb(h, s.clamp(0.0, 1.0), l.clamp(0.0, 1.0));
                c.r = r;
                c.g = g;
                c.b = b;
            }
            "comp" | "inv" => {
                c.r = 255.0 - c.r;
                c.g = 255.0 - c.g;
                c.b = 255.0 - c.b;
            }
            _ => {}
        }
    }
    Some(c)
}

pub fn parse_theme(x: &str) -> Theme {
    let mut t = Theme::default();
    let Ok(d) = Document::parse(x) else { return t };
    let root = d.root_element();
    let te = kid(Some(root), "themeElements");
    if let Some(cs) = kid(te, "clrScheme") {
        for c in cs.children().filter(|c| c.is_element()) {
            if let Some(col) = resolve_color(color_kid(Some(c)), None, None) {
                t.colors.insert(ln(&c).to_string(), col.hex()[1..].to_string());
            }
        }
    }
    let fs = kid(te, "fontScheme");
    let font_of = |n: Option<Node>| -> (String, String) {
        let latin = attr(kid(n, "latin"), "typeface").unwrap_or("").to_string();
        let mut ea = attr(kid(n, "ea"), "typeface").unwrap_or("").to_string();
        if ea.is_empty() {
            if let Some(h) = kids(n, "font").into_iter().find(|f| f.attribute("script") == Some("Hang")) {
                ea = h.attribute("typeface").unwrap_or("").to_string();
            }
        }
        (latin, ea)
    };
    t.major = font_of(kid(fs, "majorFont"));
    t.minor = font_of(kid(fs, "minorFont"));
    let fmt = kid(te, "fmtScheme");
    let collect = |n: Option<Node>| -> Vec<String> { n.map(|n| n.children().filter(|c| c.is_element()).map(|c| x[c.range()].to_string()).collect()).unwrap_or_default() };
    t.fills = collect(kid(fmt, "fillStyleLst"));
    t.bg_fills = collect(kid(fmt, "bgFillStyleLst"));
    t
}

/// 테마 글꼴 이름('+mj-lt' '+mn-ea') 풀기
pub fn theme_font(name: &str, theme: &Theme, ea: bool) -> String {
    if !name.starts_with('+') {
        return name.to_string();
    }
    let (major, kind) = match name {
        "+mj-lt" => (true, "lt"),
        "+mj-ea" => (true, "ea"),
        "+mj-cs" => (true, "cs"),
        "+mn-lt" => (false, "lt"),
        "+mn-ea" => (false, "ea"),
        "+mn-cs" => (false, "cs"),
        _ => return String::new(),
    };
    let f = if major { &theme.major } else { &theme.minor };
    let r = if kind == "ea" || ea { &f.1 } else { &f.0 };
    if r.is_empty() { f.0.clone() } else { r.clone() }
}

// ---------------- 채우기 · 선 ----------------
#[derive(Clone, Debug)]
pub enum Fill {
    None,
    Solid(Rgba),
    Grad { angle: f64, radial: bool, stops: Vec<(f64, Rgba)> },
    Blip { part: Option<String>, crop: Option<[f64; 4]>, alpha: f64 },
    Group,
}

pub fn parse_fill(parent: Option<Node>, ctx: &Ctx, pkg: &mut Pkg, ph: Option<Rgba>) -> Option<Fill> {
    let parent = parent?;
    for c in parent.children().filter(|c| c.is_element()) {
        match ln(&c) {
            "noFill" => return Some(Fill::None),
            "solidFill" => return Some(resolve_color(color_kid(Some(c)), Some(ctx), ph).map(Fill::Solid).unwrap_or(Fill::None)),
            "gradFill" => {
                let mut stops: Vec<(f64, Rgba)> = kids(kid(Some(c), "gsLst"), "gs").into_iter().filter_map(|g| Some((num(Some(g), "pos", 0.0) / 100000.0, resolve_color(color_kid(Some(g)), Some(ctx), ph)?))).collect();
                stops.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal)); // NaN 이 든 깨진 파일
                if stops.is_empty() {
                    return Some(Fill::None);
                }
                let lin = kid(Some(c), "lin");
                return Some(Fill::Grad { angle: if lin.is_some() { num(lin, "ang", 0.0) / 60000.0 } else { 90.0 }, radial: kid(Some(c), "path").is_some(), stops });
            }
            "blipFill" => {
                let blip = kid(Some(c), "blip");
                let rid = attr(blip, "embed").unwrap_or("");
                let src = kid(Some(c), "srcRect");
                let crop = src.map(|s| [num(Some(s), "l", 0.0) / 100000.0, num(Some(s), "t", 0.0) / 100000.0, num(Some(s), "r", 0.0) / 100000.0, num(Some(s), "b", 0.0) / 100000.0]).filter(|c| c.iter().any(|v| *v != 0.0));
                let alpha = kid(blip, "alphaModFix").map(|a| num(Some(a), "amt", 100000.0) / 100000.0).unwrap_or(1.0);
                let part = if rid.is_empty() { None } else { pkg.rel_target(&ctx.part, rid) };
                // 그림 효과가 있으면 파트 이름 뒤에 붙여 둔다 — build 가 다시 칠한 그림을 캐시에 둔다 (build.rs apply_fx)
                let part = part.map(|p| match blip_fx(blip, ctx) {
                    Some(fx) => format!("{p}#fx:{fx}"),
                    None => p,
                });
                return Some(Fill::Blip { part, crop, alpha });
            }
            "pattFill" => {
                let fg = resolve_color(color_kid(kid(Some(c), "fgClr")), Some(ctx), ph).unwrap_or(Rgba { r: 0.0, g: 0.0, b: 0.0, a: 1.0 });
                return Some(Fill::Solid(fg));
            }
            "grpFill" => return Some(Fill::Group),
            _ => {}
        }
    }
    None
}

/// 그림 효과(duotone · grayscl · biLevel · lum · clrChange)를 글자로. PowerPoint 는 이 효과를 칠해서 보여 주는데
/// 예전에는 무시해서 흐린 단색 문장이 원래 색(노랑·빨강)으로 나왔다.
fn blip_fx(blip: Option<Node>, ctx: &Ctx) -> Option<String> {
    let mut out: Vec<String> = vec![];
    for c in blip?.children().filter(|c| c.is_element()) {
        match c.tag_name().name() {
            "grayscl" => out.push("gray".into()),
            "biLevel" => out.push(format!("bi={:.4}", num(Some(c), "thresh", 50000.0) / 100000.0)),
            "duotone" => {
                let cols: Vec<Rgba> = c.children().filter(|x| x.is_element()).filter_map(|x| resolve_color(Some(x), Some(ctx), None)).collect();
                if cols.len() == 2 {
                    out.push(format!("duo={},{}", &cols[0].hex()[1..], &cols[1].hex()[1..]));
                }
            }
            "lum" => {
                let (b, k) = (num(Some(c), "bright", 0.0) / 100000.0, num(Some(c), "contrast", 0.0) / 100000.0);
                if b != 0.0 || k != 0.0 {
                    out.push(format!("lum={b:.4},{k:.4}"));
                }
            }
            "clrChange" => {
                let from = resolve_color(color_kid(kid(Some(c), "clrFrom")), Some(ctx), None);
                let to = resolve_color(color_kid(kid(Some(c), "clrTo")), Some(ctx), None);
                if let (Some(f), Some(t)) = (from, to) {
                    out.push(format!("chg={},{},{:.3}", &f.hex()[1..], &t.hex()[1..], t.a));
                }
            }
            _ => {}
        }
    }
    (!out.is_empty()).then(|| out.join(";"))
}

/// fillRef idx → 테마 채우기
fn theme_ref_fill(r: Option<Node>, ctx: &Ctx, pkg: &mut Pkg) -> Option<Fill> {
    let r = r?;
    let idx = num(Some(r), "idx", 0.0) as usize;
    if idx == 0 {
        return None;
    }
    let col = resolve_color(color_kid(Some(r)), Some(ctx), None);
    let list = if idx >= 1001 { &ctx.theme.bg_fills } else { &ctx.theme.fills };
    let xml = list.get(if idx >= 1001 { idx - 1001 } else { idx - 1 })?.clone();
    let wrapped = format!("<w xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">{xml}</w>");
    let d = Document::parse(&wrapped).ok()?;
    parse_fill(Some(d.root_element()), ctx, pkg, col)
}

#[derive(Clone, Debug)]
pub struct Line {
    pub w: f64,
    pub color: Rgba,
    pub dash: String,
}

fn parse_line(sp_pr: Option<Node>, style: Option<Node>, ctx: &Ctx, pkg: &mut Pkg) -> Option<Line> {
    let l = kid(sp_pr, "ln");
    let mut fill = if l.is_some() { parse_fill(l, ctx, pkg, None) } else { None };
    let w = attr(l, "w").and_then(|v| v.parse::<f64>().ok()).map(|v| v / EMU_PT);
    if fill.is_none() {
        if let Some(r) = kid(style, "lnRef").filter(|r| num(Some(*r), "idx", 0.0) > 0.0) {
            if let Some(c) = resolve_color(color_kid(Some(r)), Some(ctx), None) {
                fill = Some(Fill::Solid(c));
            }
        }
    }
    let color = match fill? {
        Fill::Solid(c) => c,
        Fill::Grad { stops, .. } => stops[0].1,
        _ => return None,
    };
    Some(Line { w: w.unwrap_or(0.75), color, dash: attr(kid(l, "prstDash"), "val").filter(|d| *d != "solid").unwrap_or("").to_string() })
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Xfrm {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub rot: f64,
    pub flip_h: bool,
    pub flip_v: bool,
    pub ch: Option<(f64, f64, f64, f64)>,
    pub present: bool,
}

fn parse_xfrm(n: Option<Node>) -> Option<Xfrm> {
    let x = kid(n, "xfrm")?;
    let off = kid(Some(x), "off");
    let ext = kid(Some(x), "ext");
    let cho = kid(Some(x), "chOff");
    let che = kid(Some(x), "chExt");
    Some(Xfrm {
        x: num(off, "x", 0.0),
        y: num(off, "y", 0.0),
        w: num(ext, "cx", 0.0),
        h: num(ext, "cy", 0.0),
        rot: num(Some(x), "rot", 0.0) / 60000.0,
        flip_h: attr(Some(x), "flipH") == Some("1"),
        flip_v: attr(Some(x), "flipV") == Some("1"),
        ch: match (cho, che) {
            (Some(a), Some(b)) => Some((num(Some(a), "x", 0.0), num(Some(a), "y", 0.0), num(Some(b), "cx", 0.0), num(Some(b), "cy", 0.0))),
            _ => None,
        },
        present: off.is_some() && ext.is_some(),
    })
}

fn map_rect(r: Xfrm, g: &Xfrm) -> Xfrm {
    let Some((cx, cy, cw, ch)) = g.ch.filter(|c| c.2 != 0.0 && c.3 != 0.0) else { return r };
    let sx = g.w / cw;
    let sy = g.h / ch;
    Xfrm { x: g.x + (r.x - cx) * sx, y: g.y + (r.y - cy) * sy, w: r.w * sx, h: r.h * sy, ..r }
}

// ---------------- 글 ----------------
#[derive(Clone, Debug, Default)]
pub struct Run {
    pub text: String,
    pub br: bool,
    pub sz: f64,
    pub b: bool,
    pub i: bool,
    pub u: bool,
    pub color: Option<Rgba>,
    pub latin: String,
    pub ea: String,
}

#[derive(Clone, Debug, Default)]
pub struct Para {
    pub lvl: usize,
    pub algn: String,
    pub ln_spc_pct: Option<f64>,
    pub ln_spc_pts: Option<f64>,
    pub spc_bef: f64,
    pub spc_aft: f64,
    pub mar_l: f64,
    pub indent: f64,
    pub bullet: String,
    pub runs: Vec<Run>,
    pub end_sz: f64,
}

#[derive(Clone, Debug, Default)]
pub struct BodyPr {
    pub anchor: String,
    pub wrap: bool,
    pub vert: String,
    pub l_ins: f64,
    pub r_ins: f64,
    pub t_ins: f64,
    pub b_ins: f64,
}

#[derive(Clone, Debug, Default)]
pub struct TextBody {
    pub body: BodyPr,
    pub paras: Vec<Para>,
    pub has_text: bool,
}

impl TextBody {
    /// 문단마다 한 줄 (글머리표 포함)
    pub fn lines(&self) -> Vec<String> {
        self.paras.iter().map(|p| format!("{}{}", if p.bullet.is_empty() { String::new() } else { format!("{} ", p.bullet) }, p.runs.iter().map(|r| r.text.as_str()).collect::<String>().trim_end_matches('\n'))).collect()
    }
}

/// 한 레벨의 스타일 (pPr·lvlNpPr XML 글 → 다시 파싱해서 쓴다)
type Levels = Vec<Option<String>>;

fn levels(n: Option<Node>, src: &str) -> Levels {
    let mut out: Levels = vec![None; 9];
    if let Some(n) = n {
        for c in n.children().filter(|c| c.is_element()) {
            let name = ln(&c);
            if let Some(d) = name.strip_prefix("lvl").and_then(|s| s.strip_suffix("pPr")).and_then(|s| s.parse::<usize>().ok()) {
                if (1..=9).contains(&d) {
                    out[d - 1] = Some(src[c.range()].to_string());
                }
            }
        }
    }
    out
}

const NS_WRAP: &str = "xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"";

#[derive(Default, Clone)]
struct ParaLayer {
    algn: Option<String>,
    mar_l: Option<f64>,
    indent: Option<f64>,
    ln_pct: Option<f64>,
    ln_pts: Option<f64>,
    bef: Option<f64>,
    aft: Option<f64>,
    bullet: Option<String>, // "" = 없음, "#num:scheme:start" = 번호
    def: Option<RunLayer>,
}

#[derive(Default, Clone)]
struct RunLayer {
    sz: Option<f64>,
    b: Option<bool>,
    i: Option<bool>,
    u: Option<bool>,
    color: Option<Rgba>,
    latin: Option<String>,
    ea: Option<String>,
}

fn run_layer(r: Option<Node>, ctx: &Ctx, pkg: &mut Pkg) -> Option<RunLayer> {
    let r = r?;
    let onoff = |v: &str| v == "1" || v == "true";
    let mut o = RunLayer::default();
    if let Some(v) = r.attribute("sz").and_then(|v| v.parse::<f64>().ok()) {
        o.sz = Some(v / 100.0);
    }
    o.b = r.attribute("b").map(onoff);
    o.i = r.attribute("i").map(onoff);
    o.u = r.attribute("u").map(|v| v != "none");
    match parse_fill(Some(r), ctx, pkg, None) {
        Some(Fill::Solid(c)) => o.color = Some(c),
        Some(Fill::Grad { stops, .. }) => o.color = Some(stops[0].1),
        _ => {}
    }
    o.latin = attr(kid(Some(r), "latin"), "typeface").map(String::from);
    o.ea = attr(kid(Some(r), "ea"), "typeface").map(String::from);
    Some(o)
}

fn para_layer(p: Option<Node>, ctx: &Ctx, pkg: &mut Pkg) -> Option<ParaLayer> {
    let p = p?;
    let mut o = ParaLayer { algn: p.attribute("algn").map(String::from), ..Default::default() };
    o.mar_l = p.attribute("marL").and_then(|v| v.parse::<f64>().ok()).map(|v| v / EMU_PT);
    o.indent = p.attribute("indent").and_then(|v| v.parse::<f64>().ok()).map(|v| v / EMU_PT);
    if let Some(l) = kid(Some(p), "lnSpc") {
        o.ln_pct = kid(Some(l), "spcPct").map(|x| num(Some(x), "val", 100000.0) / 100000.0);
        o.ln_pts = kid(Some(l), "spcPts").map(|x| num(Some(x), "val", 0.0) / 100.0);
    }
    // 문단 앞뒤 간격은 pt 로 (퍼센트는 글자 크기를 모르니 18pt 기준)
    let sp = |n: Option<Node>| n.map(|l| kid(Some(l), "spcPts").map(|x| num(Some(x), "val", 0.0) / 100.0).or(kid(Some(l), "spcPct").map(|x| num(Some(x), "val", 0.0) / 100000.0 * 18.0 * 1.2)).unwrap_or(0.0));
    o.bef = sp(kid(Some(p), "spcBef"));
    o.aft = sp(kid(Some(p), "spcAft"));
    if kid(Some(p), "buNone").is_some() {
        o.bullet = Some(String::new());
    } else if let Some(b) = kid(Some(p), "buChar") {
        o.bullet = Some(b.attribute("char").unwrap_or("•").to_string());
    } else if let Some(b) = kid(Some(p), "buAutoNum") {
        o.bullet = Some(format!("#num:{}:{}", b.attribute("type").unwrap_or("arabicPeriod"), b.attribute("startAt").unwrap_or("1")));
    } else if kid(Some(p), "buBlip").is_some() {
        o.bullet = Some("•".into());
    }
    o.def = run_layer(kid(Some(p), "defRPr"), ctx, pkg);
    Some(o)
}

fn layer_from(xml: &Option<String>, ctx: &Ctx, pkg: &mut Pkg) -> Option<ParaLayer> {
    let x = xml.as_ref()?;
    // 이름공간 선언이 빠진 조각이라 감싸서 읽는다
    let wrapped = format!("<w {NS_WRAP}>{x}</w>");
    let d = Document::parse(&wrapped).ok()?;
    let n = d.root_element().first_element_child();
    para_layer(n, ctx, pkg)
}

pub fn number_label(scheme: &str, n: i64) -> String {
    let roman = |mut n: i64| {
        let t = [(1000, "M"), (900, "CM"), (500, "D"), (400, "CD"), (100, "C"), (90, "XC"), (50, "L"), (40, "XL"), (10, "X"), (9, "IX"), (5, "V"), (4, "IV"), (1, "I")];
        let mut s = String::new();
        for (v, r) in t {
            while n >= v {
                s.push_str(r);
                n -= v;
            }
        }
        s
    };
    let body = if scheme.starts_with("alphaLc") {
        ((b'a' + ((n - 1).rem_euclid(26)) as u8) as char).to_string()
    } else if scheme.starts_with("alphaUc") {
        ((b'A' + ((n - 1).rem_euclid(26)) as u8) as char).to_string()
    } else if scheme.starts_with("romanLc") {
        roman(n).to_lowercase()
    } else if scheme.starts_with("romanUc") {
        roman(n)
    } else if scheme.starts_with("circleNum") && (1..=20).contains(&n) {
        char::from_u32(0x2460 + n as u32 - 1).unwrap().to_string()
    } else {
        n.to_string()
    };
    if scheme.ends_with("ParenBoth") {
        format!("({body})")
    } else if scheme.ends_with("ParenR") {
        format!("{body})")
    } else if scheme.ends_with("Period") {
        format!("{body}.")
    } else {
        body
    }
}

/// 글 몸통 → 문단들. chain = 가까운 스타일부터 (레이아웃 개체 틀 · 마스터 개체 틀 · 마스터 글 스타일 · 기본 글 스타일)
fn parse_text_body(tx: Option<Node>, src: &str, chain: &[Levels], body_chain: &[String], font_ref: Option<(String, Option<Rgba>)>, ctx: &Ctx, pkg: &mut Pkg) -> Option<TextBody> {
    let tx = tx?;
    let bp = kid(Some(tx), "bodyPr");
    // bodyPr 사슬 (이 도형 → 레이아웃 → 마스터)
    let mut bps: Vec<String> = vec![];
    if let Some(b) = bp {
        bps.push(src[b.range()].to_string());
    }
    bps.extend(body_chain.iter().cloned());
    let mut body = BodyPr { anchor: "t".into(), wrap: true, vert: "horz".into(), l_ins: 7.2, r_ins: 7.2, t_ins: 3.6, b_ins: 3.6 };
    let mut font_scale = 1.0;
    let mut ln_red = 0.0;
    {
        let mut seen = std::collections::HashSet::new();
        let mut fit_done = false;
        for x in bps.iter() {
            let w = format!("<w {NS_WRAP}>{x}</w>");
            let Ok(d) = Document::parse(&w) else { continue };
            let Some(b) = d.root_element().first_element_child() else { continue };
            for (k, slot) in [("anchor", 0), ("wrap", 1), ("vert", 2)] {
                if let Some(v) = b.attribute(k) {
                    if seen.insert(k) {
                        match slot {
                            0 => body.anchor = v.into(),
                            1 => body.wrap = v != "none",
                            _ => body.vert = v.into(),
                        }
                    }
                }
            }
            for (k, slot) in [("lIns", 0), ("rIns", 1), ("tIns", 2), ("bIns", 3)] {
                if let Some(v) = b.attribute(k).and_then(|v| v.parse::<f64>().ok()) {
                    if seen.insert(k) {
                        let v = v / EMU_PT;
                        match slot {
                            0 => body.l_ins = v,
                            1 => body.r_ins = v,
                            2 => body.t_ins = v,
                            _ => body.b_ins = v,
                        }
                    }
                }
            }
            if !fit_done {
                if let Some(na) = kid(Some(b), "normAutofit") {
                    font_scale = num(Some(na), "fontScale", 100000.0) / 100000.0;
                    ln_red = num(Some(na), "lnSpcReduction", 0.0) / 100000.0;
                    fit_done = true;
                } else if kid(Some(b), "spAutoFit").is_some() || kid(Some(b), "noAutofit").is_some() {
                    fit_done = true;
                }
            }
        }
    }
    let lst = levels(kid(Some(tx), "lstStyle"), src);
    let mut paras = vec![];
    let mut counters: HashMap<usize, i64> = HashMap::new();
    for p in kids(Some(tx), "p") {
        let ppr = kid(Some(p), "pPr");
        let lvl = (num(ppr, "lvl", 0.0) as usize).min(8);
        let mut layers: Vec<ParaLayer> = vec![];
        if let Some(l) = para_layer(ppr, ctx, pkg) {
            layers.push(l);
        }
        if let Some(l) = layer_from(&lst[lvl], ctx, pkg) {
            layers.push(l);
        }
        for c in chain {
            if let Some(l) = c.get(lvl).and_then(|x| layer_from(x, ctx, pkg)) {
                layers.push(l);
            }
        }
        macro_rules! pick {
            ($f:ident) => {
                layers.iter().find_map(|l| l.$f.clone())
            };
        }
        let defs: Vec<RunLayer> = layers.iter().filter_map(|l| l.def.clone()).collect();
        let mut runs = vec![];
        for r in p.children().filter(|c| c.is_element()) {
            let name = ln(&r);
            if name == "br" {
                runs.push(Run { text: "\n".into(), br: true, ..Default::default() });
                continue;
            }
            if name != "r" && name != "fld" {
                continue;
            }
            let text = text_of(kid(Some(r), "t"));
            if text.is_empty() {
                continue;
            }
            let own = run_layer(kid(Some(r), "rPr"), ctx, pkg);
            let all: Vec<&RunLayer> = own.iter().chain(defs.iter()).collect();
            macro_rules! g {
                ($f:ident) => {
                    all.iter().find_map(|l| l.$f.clone())
                };
            }
            let fr_idx = font_ref.as_ref().map(|f| f.0.as_str()).unwrap_or("");
            let latin = g!(latin).unwrap_or_else(|| if fr_idx == "major" { "+mj-lt".into() } else { "+mn-lt".into() });
            let ea = g!(ea).unwrap_or_else(|| if fr_idx == "major" { "+mj-ea".into() } else { "+mn-ea".into() });
            let color = g!(color).or(font_ref.as_ref().and_then(|f| f.1));
            let lat = theme_font(&latin, &ctx.theme, false);
            runs.push(Run {
                text,
                br: false,
                sz: g!(sz).unwrap_or(18.0) * font_scale,
                b: g!(b).unwrap_or(false),
                i: g!(i).unwrap_or(false),
                u: g!(u).unwrap_or(false),
                color,
                latin: if lat.is_empty() { "Arial".into() } else { lat.clone() },
                ea: { let e = theme_font(&ea, &ctx.theme, true); if e.is_empty() { lat } else { e } },
            });
        }
        let end_sz = run_layer(kid(Some(p), "endParaRPr"), ctx, pkg).and_then(|r| r.sz).or(defs.iter().find_map(|d| d.sz)).unwrap_or(18.0) * font_scale;
        let mut bullet = pick!(bullet).unwrap_or_default();
        if let Some(rest) = bullet.strip_prefix("#num:") {
            let (scheme, start) = rest.split_once(':').unwrap_or((rest, "1"));
            let c = counters.entry(lvl).or_insert(start.parse::<i64>().unwrap_or(1) - 1);
            *c += 1;
            bullet = number_label(scheme, *c);
        }
        if !runs.iter().any(|r| !r.text.trim().is_empty()) {
            bullet.clear();
        }
        let ln_pct = pick!(ln_pct).map(|v| (v - ln_red).max(0.5));
        paras.push(Para {
            lvl,
            algn: pick!(algn).unwrap_or_else(|| "l".into()),
            ln_spc_pct: ln_pct,
            ln_spc_pts: pick!(ln_pts),
            spc_bef: pick!(bef).unwrap_or(0.0),
            spc_aft: pick!(aft).unwrap_or(0.0),
            mar_l: pick!(mar_l).unwrap_or(0.0),
            indent: pick!(indent).unwrap_or(0.0),
            bullet,
            runs,
            end_sz,
        });
    }
    let has_text = paras.iter().any(|p| p.runs.iter().any(|r| !r.text.trim().is_empty()));
    Some(TextBody { body, paras, has_text })
}

// ---------------- 도형 ----------------
#[derive(Clone, Debug, Default)]
pub struct Ph {
    pub ty: String,
    pub idx: String,
}

#[derive(Clone, Debug)]
pub struct Table {
    pub cols: Vec<f64>,
    pub rows: Vec<(f64, Vec<(Option<TextBody>, Option<Fill>, bool)>)>,
    pub accent: Option<Rgba>,
}

#[derive(Clone, Debug)]
pub struct Shape {
    pub id: String,
    pub name: String,
    pub origin: &'static str, // slide · layout · master
    pub kind: String,         // sp · pic · cxnSp · table · chart · diagram · ole · graphic
    pub ph: Option<Ph>,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub rot: f64,
    pub flip_h: bool,
    pub flip_v: bool,
    pub in_rot_group: bool,
    pub prst: String,
    pub adj: f64,
    pub custom: Option<Vec<(f64, f64, String, bool)>>, // (w, h, path d, fill)
    pub fill: Option<Fill>,
    pub line: Option<Line>,
    pub text: Option<TextBody>,
    pub pic: Option<Fill>,
    pub table: Option<Table>,
    pub chart: Option<chart::Chart>,
}

#[derive(Clone, Debug)]
pub struct Slide {
    pub w: f64,
    pub h: f64,
    pub bg: Fill,
    pub shapes: Vec<Shape>,
}

/// 걸어 가며 모은 도형 (아직 상속 전): XML 조각은 글로 들고 있다
#[derive(Clone, Debug)]
struct RawShape {
    xml: String, // 도형 노드 XML
    origin: &'static str,
    xf: Option<Xfrm>,
    ph: Option<Ph>,
    rot_group: bool,
}

fn wrap_doc(x: &str) -> String {
    format!("<w {NS_WRAP}>{x}</w>")
}

fn walk(tree: Option<Node>, src: &str, origin: &'static str, group: Option<&Xfrm>, rot_group: bool, depth: usize, out: &mut Vec<RawShape>) {
    let Some(tree) = tree else { return };
    if depth > 12 {
        return;
    }
    for c in tree.children().filter(|c| c.is_element()) {
        let name = ln(&c);
        if name == "AlternateContent" {
            let fb = kid(Some(c), "Fallback").or(kid(Some(c), "Choice"));
            walk(fb, src, origin, group, rot_group, depth + 1, out);
            continue;
        }
        if !["sp", "pic", "cxnSp", "graphicFrame", "grpSp"].contains(&name) {
            continue;
        }
        let nv = c.children().find(|x| x.is_element() && ln(x).starts_with("nv") && ln(x).ends_with("Pr"));
        if attr(kid(nv, "cNvPr"), "hidden") == Some("1") {
            continue;
        }
        if name == "grpSp" {
            let Some(gx) = parse_xfrm(kid(Some(c), "grpSpPr")) else { continue };
            let mine = match group { Some(g) => map_rect(gx, g), None => gx };
            let g2 = Xfrm { ch: gx.ch, ..mine };
            walk(Some(c), src, origin, Some(&g2), rot_group || gx.rot != 0.0 || gx.flip_h || gx.flip_v, depth + 1, out);
            continue;
        }
        let xf = parse_xfrm(if name == "graphicFrame" { Some(c) } else { kid(Some(c), "spPr") });
        let xf = match (xf, group) {
            (Some(x), Some(g)) => Some(Xfrm { rot: x.rot, flip_h: x.flip_h, flip_v: x.flip_v, present: x.present, ..map_rect(x, g) }),
            (x, _) => x,
        };
        let ph = kid(kid(nv, "nvPr"), "ph").map(|p| Ph { ty: p.attribute("type").unwrap_or("").into(), idx: p.attribute("idx").unwrap_or("").into() });
        out.push(RawShape { xml: src[c.range()].to_string(), origin, xf, ph, rot_group });
    }
}

fn ph_kind(ph: &Option<Ph>) -> &'static str {
    let Some(p) = ph else { return "other" };
    if p.ty == "title" || p.ty == "ctrTitle" || (p.ty.is_empty() && p.idx == "0") {
        return "title";
    }
    if ["body", "subTitle", "obj", "tbl", "chart", "clipArt", "dgm", "media", "pic", "sldImg", "hdr"].contains(&p.ty.as_str()) || (p.ty.is_empty() && !p.idx.is_empty()) {
        return "body";
    }
    "other"
}

fn match_ph<'a>(ph: &Ph, list: &'a [RawShape]) -> Option<&'a RawShape> {
    let t = if ph.ty.is_empty() { "body" } else { ph.ty.as_str() };
    let norm = |s: &str| if s.is_empty() { "body".to_string() } else { s.to_string() };
    list.iter()
        .find(|s| s.ph.as_ref().is_some_and(|p| norm(&p.ty) == t && p.idx == ph.idx))
        .or_else(|| if ph.idx.is_empty() { None } else { list.iter().find(|s| s.ph.as_ref().is_some_and(|p| p.idx == ph.idx)) })
        .or_else(|| if ph.ty.is_empty() { None } else { list.iter().find(|s| s.ph.as_ref().is_some_and(|p| p.ty == ph.ty)) })
        .or_else(|| {
            let k = ph_kind(&Some(ph.clone()));
            if k == "other" { None } else { list.iter().find(|s| ph_kind(&s.ph) == k) }
        })
}

struct Master {
    ctx: Ctx,
    bg: Option<Fill>,
    shapes: Vec<RawShape>,
    styles: HashMap<&'static str, Levels>,
}
struct Layout {
    ctx: Ctx,
    bg: Option<Fill>,
    shapes: Vec<RawShape>,
    master: Option<String>,
    show_master: bool,
}

pub struct Doc {
    pub pkg: Pkg,
    pub w: f64,
    pub h: f64,
    pub slides: Vec<String>,
    default_text: Levels,
    masters: HashMap<String, Master>,
    layouts: HashMap<String, Layout>,
}

fn parse_bg(c_sld: Option<Node>, ctx: &Ctx, pkg: &mut Pkg) -> Option<Fill> {
    let bg = kid(c_sld, "bg")?;
    if let Some(pr) = kid(Some(bg), "bgPr") {
        return parse_fill(Some(pr), ctx, pkg, None);
    }
    theme_ref_fill(kid(Some(bg), "bgRef"), ctx, pkg)
}

fn clr_map(n: Option<Node>) -> HashMap<String, String> {
    n.map(|n| n.attributes().map(|a| (a.name().to_string(), a.value().to_string())).collect()).unwrap_or_default()
}

impl Doc {
    pub fn open(bytes: &[u8]) -> Result<Self, String> {
        let mut pkg = Pkg::open(bytes)?;
        let pres_part = pkg.rel_by_type("", "officeDocument").unwrap_or_else(|| "ppt/presentation.xml".into());
        let txt = pkg.text(&pres_part).ok_or("pptx 파일 형식이 아닙니다 (presentation.xml 없음).")?;
        let d = Document::parse(&txt).map_err(|e| e.to_string())?;
        let root = d.root_element();
        let sz = kid(Some(root), "sldSz");
        let w = num(sz, "cx", 9144000.0) / EMU_PT;
        let h = num(sz, "cy", 6858000.0) / EMU_PT;
        let default_text = levels(kid(Some(root), "defaultTextStyle"), &txt);
        let ids: Vec<String> = kids(kid(Some(root), "sldIdLst"), "sldId").into_iter().filter_map(|s| s.attributes().find(|a| a.name() == "id" && a.namespace().is_some()).map(|a| a.value().to_string())).collect();
        let mut slides = vec![];
        for id in ids {
            let Some(part) = pkg.rel_target(&pres_part, &id) else { continue };
            let Some(t) = pkg.text(&part) else { continue };
            if let Ok(x) = Document::parse(&t) {
                if x.root_element().attribute("show") == Some("0") {
                    continue; // 숨긴 슬라이드
                }
            }
            slides.push(part);
        }
        Ok(Self { pkg, w, h, slides, default_text, masters: HashMap::new(), layouts: HashMap::new() })
    }

    fn load_master(&mut self, part: &str) {
        if self.masters.contains_key(part) {
            return;
        }
        let pkg = &mut self.pkg;
        let theme = pkg.rel_by_type(part, "theme").and_then(|t| pkg.text(&t)).map(|t| parse_theme(&t)).unwrap_or_default();
        let txt = pkg.text(part).unwrap_or_else(|| "<x/>".into());
        let Ok(d) = Document::parse(&txt) else { return };
        let root = d.root_element();
        let ctx = Ctx { part: part.to_string(), theme, clr_map: clr_map(kid(Some(root), "clrMap")) };
        let c_sld = kid(Some(root), "cSld");
        let bg = parse_bg(c_sld, &ctx, pkg);
        let mut shapes = vec![];
        walk(kid(c_sld, "spTree"), &txt, "master", None, false, 0, &mut shapes);
        let tx = kid(Some(root), "txStyles");
        let mut styles = HashMap::new();
        styles.insert("title", levels(kid(tx, "titleStyle"), &txt));
        styles.insert("body", levels(kid(tx, "bodyStyle"), &txt));
        styles.insert("other", levels(kid(tx, "otherStyle"), &txt));
        self.masters.insert(part.to_string(), Master { ctx, bg, shapes, styles });
    }

    fn load_layout(&mut self, part: &str) {
        if self.layouts.contains_key(part) {
            return;
        }
        let mp = self.pkg.rel_by_type(part, "slideMaster").filter(|m| self.pkg.has(m));
        if let Some(m) = &mp {
            self.load_master(m);
        }
        let master = mp.as_ref().and_then(|m| self.masters.get(m));
        let pkg = &mut self.pkg;
        let txt = pkg.text(part).unwrap_or_else(|| "<x/>".into());
        let Ok(d) = Document::parse(&txt) else { return };
        let root = d.root_element();
        let ovr = kid(kid(Some(root), "clrMapOvr"), "overrideClrMapping");
        let ctx = Ctx {
            part: part.to_string(),
            theme: master.map(|m| m.ctx.theme.clone()).unwrap_or_default(),
            clr_map: if ovr.is_some() { clr_map(ovr) } else { master.map(|m| m.ctx.clr_map.clone()).unwrap_or_default() },
        };
        let c_sld = kid(Some(root), "cSld");
        let bg = parse_bg(c_sld, &ctx, pkg);
        let mut shapes = vec![];
        walk(kid(c_sld, "spTree"), &txt, "layout", None, false, 0, &mut shapes);
        let show_master = root.attribute("showMasterSp") != Some("0");
        self.layouts.insert(part.to_string(), Layout { ctx, bg, shapes, master: mp, show_master });
    }

    /// 슬라이드 하나 → 배경 + 도형들 (뒤 → 앞: 마스터 장식 · 레이아웃 장식 · 슬라이드 도형)
    pub fn slide(&mut self, k: usize) -> Result<Slide, String> {
        let part = self.slides.get(k).cloned().ok_or("슬라이드 번호가 맞지 않습니다")?;
        let lp = self.pkg.rel_by_type(&part, "slideLayout").filter(|l| self.pkg.has(l));
        if let Some(l) = &lp {
            self.load_layout(l);
        }
        let layout = lp.as_ref().and_then(|l| self.layouts.get(l));
        let master = layout.and_then(|l| l.master.as_ref()).and_then(|m| self.masters.get(m));
        let txt = self.pkg.text(&part).ok_or("슬라이드 XML 없음")?;
        let d = Document::parse(&txt).map_err(|e| e.to_string())?;
        let root = d.root_element();
        let ovr = kid(kid(Some(root), "clrMapOvr"), "overrideClrMapping");
        let theme = master.map(|m| m.ctx.theme.clone()).unwrap_or_default();
        let ctx = Ctx { part: part.clone(), theme, clr_map: if ovr.is_some() { clr_map(ovr) } else { layout.map(|l| l.ctx.clr_map.clone()).unwrap_or_default() } };
        let c_sld = kid(Some(root), "cSld");
        let mut own = vec![];
        walk(kid(c_sld, "spTree"), &txt, "slide", None, false, 0, &mut own);
        let show_master = root.attribute("showMasterSp") != Some("0") && layout.is_none_or(|l| l.show_master);
        // 나머지를 위해 값만 꺼내 둔다 (빌림을 풀려고)
        let lay_shapes = layout.map(|l| l.shapes.clone()).unwrap_or_default();
        let lay_ctx = layout.map(|l| l.ctx.clone());
        let lay_bg = layout.and_then(|l| l.bg.clone());
        let mas_shapes = master.map(|m| m.shapes.clone()).unwrap_or_default();
        let mas_ctx = master.map(|m| m.ctx.clone());
        let mas_bg = master.and_then(|m| m.bg.clone());
        let mas_styles = master.map(|m| m.styles.clone()).unwrap_or_default();
        let pkg = &mut self.pkg;
        let bg = parse_bg(c_sld, &ctx, pkg).or(lay_bg).or(mas_bg).unwrap_or(Fill::Solid(Rgba { r: 255.0, g: 255.0, b: 255.0, a: 1.0 }));
        let mut shapes = vec![];
        if show_master {
            if let Some(mc) = &mas_ctx {
                for s in mas_shapes.iter().filter(|s| s.ph.is_none()) {
                    if let Some(sh) = finish(s, mc, pkg, &[], &[], None, None, self.w) {
                        shapes.push(sh);
                    }
                }
            }
        }
        if let Some(lc) = &lay_ctx {
            for s in lay_shapes.iter().filter(|s| s.ph.is_none()) {
                if let Some(sh) = finish(s, lc, pkg, &[], &[], None, None, self.w) {
                    shapes.push(sh);
                }
            }
        }
        for s in own.iter() {
            let mut s = s.clone();
            let mut chain: Vec<Levels> = vec![];
            let mut body_chain: Vec<String> = vec![];
            let mut inherit_fill: Option<String> = None;
            let mut font_ref_xml: Option<String> = None;
            if let Some(ph) = s.ph.clone() {
                let lay = match_ph(&ph, &lay_shapes);
                let mas = match_ph(lay.and_then(|l| l.ph.as_ref()).unwrap_or(&ph), &mas_shapes);
                if s.xf.is_none_or(|x| !x.present) {
                    if let Some(src) = lay.filter(|l| l.xf.is_some_and(|x| x.present)).or(mas.filter(|m| m.xf.is_some_and(|x| x.present))) {
                        s.xf = src.xf;
                    }
                }
                for r in [lay, mas].into_iter().flatten() {
                    let w = wrap_doc(&r.xml);
                    if let Ok(dd) = Document::parse(&w) {
                        let n = dd.root_element().first_element_child();
                        let tb = kid(n, "txBody");
                        if tb.is_some() {
                            chain.push(levels(kid(tb, "lstStyle"), &w));
                            if let Some(b) = kid(tb, "bodyPr") {
                                body_chain.push(w[b.range()].to_string());
                            }
                        }
                        if inherit_fill.is_none() {
                            if let Some(sp) = kid(n, "spPr").filter(|sp| ["solidFill", "gradFill", "blipFill", "noFill"].iter().any(|k| kid(Some(*sp), k).is_some())) {
                                inherit_fill = Some(w[sp.range()].to_string());
                            }
                        }
                        if font_ref_xml.is_none() {
                            if let Some(fr) = kid(kid(n, "style"), "fontRef") {
                                font_ref_xml = Some(w[fr.range()].to_string());
                            }
                        }
                    }
                }
            }
            if let Some(st) = mas_styles.get(ph_kind(&s.ph)) {
                chain.push(st.clone());
            }
            chain.push(self.default_text.clone());
            if let Some(sh) = finish(&s, &ctx, pkg, &chain, &body_chain, inherit_fill, font_ref_xml, self.w) {
                shapes.push(sh);
            }
        }
        Ok(Slide { w: self.w, h: self.h, bg, shapes })
    }
}

#[allow(clippy::too_many_arguments)]
fn finish(s: &RawShape, ctx: &Ctx, pkg: &mut Pkg, chain: &[Levels], body_chain: &[String], inherit_fill: Option<String>, font_ref_xml: Option<String>, _sw: f64) -> Option<Shape> {
    let xf = s.xf?;
    let w = wrap_doc(&s.xml);
    let d = Document::parse(&w).ok()?;
    let n = d.root_element().first_element_child()?;
    let kind0 = ln(&n).to_string();
    let nv = n.children().find(|x| x.is_element() && ln(x).starts_with("nv") && ln(x).ends_with("Pr"));
    let cnv = kid(nv, "cNvPr");
    let sp_pr = kid(Some(n), "spPr");
    let style = kid(Some(n), "style");
    let pt = |v: f64| v / EMU_PT;
    let mut o = Shape {
        id: attr(cnv, "id").unwrap_or("").into(),
        name: attr(cnv, "name").unwrap_or("").into(),
        origin: s.origin,
        kind: kind0.clone(),
        ph: s.ph.clone(),
        x: pt(xf.x),
        y: pt(xf.y),
        w: pt(xf.w),
        h: pt(xf.h),
        rot: xf.rot,
        flip_h: xf.flip_h,
        flip_v: xf.flip_v,
        in_rot_group: s.rot_group,
        prst: String::new(),
        adj: 16667.0,
        custom: None,
        fill: None,
        line: None,
        text: None,
        pic: None,
        table: None,
        chart: None,
    };
    if sp_pr.is_some() {
        if let Some(p) = kid(sp_pr, "prstGeom") {
            o.prst = p.attribute("prst").unwrap_or("rect").into();
            if let Some(g) = kids(kid(Some(p), "avLst"), "gd").into_iter().find(|g| g.attribute("name") == Some("adj")) {
                o.adj = g.attribute("fmla").unwrap_or("").trim_start_matches("val").trim().parse().unwrap_or(16667.0);
            }
        } else if let Some(c) = kid(sp_pr, "custGeom") {
            o.prst = "custom".into();
            o.custom = Some(parse_cust(Some(c)));
        } else if kind0 == "sp" || kind0 == "cxnSp" {
            o.prst = "rect".into();
        }
        let mut fill = parse_fill(sp_pr, ctx, pkg, None);
        if fill.is_none() {
            if let Some(x) = &inherit_fill {
                let ww = wrap_doc(x);
                if let Ok(dd) = Document::parse(&ww) {
                    fill = parse_fill(dd.root_element().first_element_child(), ctx, pkg, None);
                }
            }
        }
        if fill.is_none() && kind0 != "pic" {
            fill = theme_ref_fill(kid(style, "fillRef"), ctx, pkg);
        }
        o.fill = fill.filter(|f| !matches!(f, Fill::None | Fill::Group));
        o.line = parse_line(sp_pr, style, ctx, pkg);
        if kind0 == "cxnSp" && o.line.is_none() {
            o.line = Some(Line { w: 0.75, color: Rgba { r: 0.0, g: 0.0, b: 0.0, a: 1.0 }, dash: String::new() });
        }
    }
    if kind0 == "pic" {
        o.pic = parse_fill(Some(n), ctx, pkg, None).filter(|f| matches!(f, Fill::Blip { .. }));
        if o.pic.is_none() {
            // pic 의 blipFill 은 도형 바로 아래
            let bf = kid(Some(n), "blipFill");
            if let Some(bf) = bf {
                let ww = format!("<w {NS_WRAP}>{}</w>", &w[bf.range()]);
                if let Ok(dd) = Document::parse(&ww) {
                    o.pic = parse_fill(Some(dd.root_element()), ctx, pkg, None);
                }
            }
        }
    }
    if kind0 == "graphicFrame" {
        let gd = kid(kid(Some(n), "graphic"), "graphicData");
        let uri = attr(gd, "uri").unwrap_or("");
        if uri.ends_with("/table") {
            o.kind = "table".into();
            o.table = parse_table(kid(gd, "tbl"), &w, ctx, pkg, chain);
        } else if uri.ends_with("/chart") {
            o.kind = "chart".into();
            // 차트 파트의 캐시 값으로 그린다
            let rid = attr(kid(gd, "chart"), "id").unwrap_or("");
            if let Some(xml) = pkg.rel_target(&ctx.part, rid).and_then(|t| pkg.text(&t)) {
                o.chart = chart::parse(&xml).map(|mut c| {
                    c.theme = (1..=6).filter_map(|i| ctx.theme.colors.get(&format!("accent{i}")).map(|h| format!("#{}", h.trim_start_matches('#')))).collect();
                    c
                });
            }
        } else if uri.ends_with("/diagram") {
            o.kind = "diagram".into();
        } else if uri.contains("ole") {
            o.kind = "ole".into();
            if let Some(p) = find(gd, "pic") {
                let bf = kid(Some(p), "blipFill");
                if let Some(bf) = bf {
                    let ww = format!("<w {NS_WRAP}>{}</w>", &w[bf.range()]);
                    if let Ok(dd) = Document::parse(&ww) {
                        o.pic = parse_fill(Some(dd.root_element()), ctx, pkg, None);
                    }
                }
            }
        } else {
            o.kind = "graphic".into();
        }
    }
    if o.kind != "table" {
        let fr = font_ref_xml.and_then(|x| {
            let ww = wrap_doc(&x);
            let dd = Document::parse(&ww).ok()?;
            let f = dd.root_element().first_element_child()?;
            Some((f.attribute("idx").unwrap_or("").to_string(), resolve_color(color_kid(Some(f)), Some(ctx), None)))
        });
        let fr = kid(style, "fontRef").map(|f| (f.attribute("idx").unwrap_or("").to_string(), resolve_color(color_kid(Some(f)), Some(ctx), None))).or(fr);
        o.text = parse_text_body(kid(Some(n), "txBody"), &w, chain, body_chain, fr, ctx, pkg);
    }
    Some(o)
}

fn parse_cust(c: Option<Node>) -> Vec<(f64, f64, String, bool)> {
    let mut out = vec![];
    for p in kids(kid(c, "pathLst"), "path") {
        let (w, h) = (num(Some(p), "w", 0.0), num(Some(p), "h", 0.0));
        let mut d = String::new();
        let (mut cx, mut cy) = (0.0, 0.0);
        let pt = |n: Node| (num(Some(n), "x", 0.0), num(Some(n), "y", 0.0));
        for c in p.children().filter(|c| c.is_element()) {
            match ln(&c) {
                "moveTo" | "lnTo" => {
                    if let Some(t) = kid(Some(c), "pt") {
                        (cx, cy) = pt(t);
                        d.push_str(&format!("{}{} {} ", if ln(&c) == "moveTo" { "M" } else { "L" }, cx, cy));
                    }
                }
                "cubicBezTo" | "quadBezTo" => {
                    let pts: Vec<(f64, f64)> = kids(Some(c), "pt").into_iter().map(pt).collect();
                    let cmd = if ln(&c) == "cubicBezTo" { "C" } else { "Q" };
                    if !pts.is_empty() {
                        d.push_str(cmd);
                        for (x, y) in &pts {
                            d.push_str(&format!("{x} {y} "));
                        }
                        (cx, cy) = *pts.last().unwrap();
                    }
                }
                "arcTo" => {
                    let (wr, hr) = (num(Some(c), "wR", 0.0), num(Some(c), "hR", 0.0));
                    let st = num(Some(c), "stAng", 0.0) / 60000.0;
                    let swa = num(Some(c), "swAng", 0.0) / 60000.0;
                    let rad = |a: f64| a.to_radians();
                    let ox = cx - wr * rad(st).cos();
                    let oy = cy - hr * rad(st).sin();
                    let ex = ox + wr * rad(st + swa).cos();
                    let ey = oy + hr * rad(st + swa).sin();
                    d.push_str(&format!("A{} {} 0 {} {} {} {} ", wr, hr, if swa.abs() > 180.0 { 1 } else { 0 }, if swa > 0.0 { 1 } else { 0 }, ex, ey));
                    cx = ex;
                    cy = ey;
                }
                "close" => d.push_str("Z "),
                _ => {}
            }
        }
        out.push((w, h, d, p.attribute("fill") != Some("none")));
    }
    out
}

fn parse_table(tbl: Option<Node>, src: &str, ctx: &Ctx, pkg: &mut Pkg, chain: &[Levels]) -> Option<Table> {
    let tbl = tbl?;
    let cols = kids(kid(Some(tbl), "tblGrid"), "gridCol").into_iter().map(|c| num(Some(c), "w", 0.0) / EMU_PT).collect();
    let style_id = text_of(kid(kid(Some(tbl), "tblPr"), "tableStyleId"));
    let accent = if style_id.is_empty() { None } else { ctx.theme.colors.get("accent1").and_then(|h| Rgba::from_hex(h)) };
    let def: Vec<Levels> = chain.last().cloned().into_iter().collect();
    let rows = kids(Some(tbl), "tr")
        .into_iter()
        .map(|tr| {
            let cells = kids(Some(tr), "tc")
                .into_iter()
                .map(|tc| {
                    let fill = parse_fill(kid(Some(tc), "tcPr"), ctx, pkg, None).filter(|f| !matches!(f, Fill::None));
                    let tb = parse_text_body(kid(Some(tc), "txBody"), src, &def, &[], None, ctx, pkg);
                    let merged = tc.attribute("hMerge") == Some("1") || tc.attribute("vMerge") == Some("1");
                    (tb, fill, merged)
                })
                .collect();
            (num(Some(tr), "h", 0.0) / EMU_PT, cells)
        })
        .collect();
    Some(Table { cols, rows, accent })
}

/// 슬라이드의 글 (위 → 아래, 왼 → 오른쪽. 쪽 번호·바닥글·날짜 자리 뺌)
pub fn slide_text(s: &Slide) -> String {
    let mut boxes: Vec<(f64, f64, String)> = vec![];
    for sh in s.shapes.iter().filter(|x| x.origin == "slide") {
        if sh.ph.as_ref().is_some_and(|p| ["sldNum", "ftr", "dt"].contains(&p.ty.as_str())) {
            continue;
        }
        if let Some(t) = sh.text.as_ref().filter(|t| t.has_text) {
            boxes.push((sh.y, sh.x, t.lines().join("\n")));
        }
        if let Some(tb) = &sh.table {
            let t = tb.rows.iter().map(|(_, cs)| cs.iter().filter(|c| !c.2).filter_map(|c| c.0.as_ref()).map(|t| t.lines().join(" ")).collect::<Vec<_>>().join(" | ")).collect::<Vec<_>>().join("\n");
            boxes.push((sh.y, sh.x, t));
        }
    }
    boxes.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal).then(a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)));
    boxes.into_iter().map(|b| b.2).filter(|t| !t.trim().is_empty()).collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn colors_and_labels() {
        let x = r#"<a:x xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:srgbClr val="FF0000"><a:lumMod val="50000"/></a:srgbClr></a:x>"#;
        let d = Document::parse(x).unwrap();
        let c = resolve_color(color_kid(Some(d.root_element())), None, None).unwrap();
        assert_eq!(c.hex(), "#800000");
        assert_eq!(number_label("arabicPeriod", 3), "3.");
        assert_eq!(number_label("romanUcParenR", 4), "IV)");
        assert_eq!(join_path("ppt/slides/slide1.xml", "../media/image1.png"), "ppt/media/image1.png");
    }
}
