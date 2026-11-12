//! [트랙 A] PPT 차트 기본 그리기: 막대(세로·가로, 묶음·누적) · 꺾은선 · 영역 · 원 · 도넛
//! chartN.xml 의 캐시 값(numCache·strCache)만 읽는다 (내장 엑셀은 읽지 않음). 축 눈금·범례·제목은 간단히.

use super::svg::esc;
use roxmltree::{Document, Node};
use std::fmt::Write;

#[derive(Clone, Debug, Default)]
pub struct Series {
    pub name: String,
    pub vals: Vec<f64>,
    pub color: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Chart {
    pub kind: String, // bar · line · area · pie · doughnut
    pub horizontal: bool,
    pub stacked: bool,
    pub title: String,
    pub cats: Vec<String>,
    pub series: Vec<Series>,
    pub legend: bool,
    pub labels: bool,
    pub font_pt: f64,
    /// 테마 accent1~6 (#rrggbb)
    pub theme: Vec<String>,
}

fn ln<'a>(n: &Node<'a, 'a>) -> &'a str {
    n.tag_name().name()
}
fn kids<'a>(n: Node<'a, 'a>, name: &'a str) -> impl Iterator<Item = Node<'a, 'a>> {
    n.children().filter(move |c| c.is_element() && ln(c) == name)
}
fn kid<'a>(n: Option<Node<'a, 'a>>, name: &str) -> Option<Node<'a, 'a>> {
    n?.children().find(|c| c.is_element() && ln(c) == name)
}
fn find<'a>(n: Node<'a, 'a>, name: &str) -> Option<Node<'a, 'a>> {
    n.descendants().find(|c| c.is_element() && ln(c) == name)
}
fn val<'a>(n: Option<Node<'a, 'a>>) -> Option<&'a str> {
    n?.attribute("val")
}

/// 캐시 값: pt idx 자리에 맞춰
fn cache(n: Option<Node>) -> Vec<String> {
    let Some(n) = n else { return vec![] };
    let Some(c) = n.descendants().find(|c| c.is_element() && (ln(c) == "numCache" || ln(c) == "strCache" || ln(c) == "numLit" || ln(c) == "strLit")) else { return vec![] };
    let count = val(kid(Some(c), "ptCount")).and_then(|x| x.parse::<usize>().ok()).unwrap_or(0);
    let mut out = vec![String::new(); count];
    for p in kids(c, "pt") {
        let i = p.attribute("idx").and_then(|x| x.parse::<usize>().ok()).unwrap_or(out.len());
        let v = kid(Some(p), "v").and_then(|v| v.text()).unwrap_or("").to_string();
        if i >= out.len() {
            out.resize(i + 1, String::new());
        }
        out[i] = v;
    }
    out
}

fn rich_text(n: Option<Node>) -> String {
    let Some(n) = n else { return String::new() };
    n.descendants().filter(|c| c.is_element() && ln(c) == "t").filter_map(|t| t.text()).collect::<Vec<_>>().join("")
}

/// chartN.xml → Chart (모르는 차트면 None)
pub fn parse(xml: &str) -> Option<Chart> {
    let d = Document::parse(xml).ok()?;
    let root = d.root_element();
    let chart = find(root, "chart")?;
    let plot = kid(Some(chart), "plotArea")?;
    let ty = plot.children().find(|c| c.is_element() && ln(c).ends_with("Chart"))?;
    let kind = match ln(&ty) {
        "barChart" | "bar3DChart" => "bar",
        "lineChart" | "line3DChart" | "scatterChart" => "line",
        "areaChart" | "area3DChart" => "area",
        "pieChart" | "pie3DChart" | "ofPieChart" => "pie",
        "doughnutChart" => "doughnut",
        _ => return None,
    };
    let mut c = Chart { kind: kind.into(), ..Default::default() };
    c.horizontal = val(kid(Some(ty), "barDir")) == Some("bar");
    c.stacked = matches!(val(kid(Some(ty), "grouping")), Some("stacked") | Some("percentStacked"));
    let deleted = val(kid(Some(chart), "autoTitleDeleted")) == Some("1");
    c.title = rich_text(kid(Some(chart), "title"));
    for s in kids(ty, "ser") {
        let name = cache(kid(Some(s), "tx")).into_iter().next().unwrap_or_default();
        if c.cats.is_empty() {
            c.cats = cache(kid(Some(s), "cat").or(kid(Some(s), "xVal")));
        }
        let vals = cache(kid(Some(s), "val").or(kid(Some(s), "yVal"))).iter().map(|v| v.parse::<f64>().unwrap_or(0.0)).collect();
        let color = kid(Some(s), "spPr").and_then(|sp| find(sp, "srgbClr")).and_then(|x| x.attribute("val")).map(|h| format!("#{h}"));
        c.series.push(Series { name, vals, color });
    }
    if c.title.is_empty() && !deleted && c.series.len() == 1 && kind != "pie" && kind != "doughnut" {
        // 제목이 없고 지우지도 않았으면 PowerPoint 는 계열 이름을 제목으로 보인다
        if kid(Some(chart), "title").is_some() {
            c.title = c.series[0].name.clone();
        }
    }
    c.legend = kid(Some(chart), "legend").is_some();
    c.labels = ty.descendants().any(|x| x.is_element() && ln(&x) == "showVal" && x.attribute("val") == Some("1"));
    c.font_pt = root
        .descendants()
        .find(|x| x.is_element() && ln(x) == "defRPr" && x.attribute("sz").is_some())
        .and_then(|x| x.attribute("sz"))
        .and_then(|x| x.parse::<f64>().ok())
        .map(|x| x / 100.0)
        .unwrap_or(10.0);
    if c.series.is_empty() {
        return None;
    }
    Some(c)
}

/// 기본 색 (Office 테마 accent1~6 · 테마 색이 있으면 그것)
pub fn palette(theme: &[String]) -> Vec<String> {
    let def = ["#4472c4", "#ed7d31", "#a5a5a5", "#ffc000", "#5b9bd5", "#70ad47"];
    (0..6).map(|i| theme.get(i).cloned().unwrap_or_else(|| def[i].to_string())).collect()
}

fn nice_max(v: f64) -> (f64, f64) {
    if v <= 0.0 {
        return (1.0, 0.2);
    }
    let raw = v / 5.0;
    let mag = 10f64.powf(raw.log10().floor());
    let step = [1.0, 2.0, 2.5, 5.0, 10.0].iter().map(|m| m * mag).find(|s| *s >= raw).unwrap_or(10.0 * mag);
    ((v / step).ceil() * step, step)
}

fn fmt_num(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 { format!("{}", v.round() as i64) } else { format!("{v:.1}") }
}

fn text(x: f64, y: f64, s: &str, size: f64, anchor: &str, color: &str) -> String {
    format!("<text x=\"{x:.2}\" y=\"{y:.2}\" font-size=\"{size:.2}\" font-family=\"'Malgun Gothic', sans-serif\" fill=\"{color}\" text-anchor=\"{anchor}\">{}</text>", esc(s))
}

/// 차트 → SVG 조각 (상자 x,y,w,h pt)
pub fn chart_svg(c: &Chart, x: f64, y: f64, w: f64, h: f64, theme: &[String]) -> String {
    let pal = palette(theme);
    let colr = |i: usize, s: &Series| s.color.clone().unwrap_or_else(|| pal[i % pal.len()].clone());
    let fs = c.font_pt.clamp(6.0, 40.0);
    let ink = "#595959";
    let mut out = String::new();
    let mut top = y + fs * 0.6;
    if !c.title.is_empty() {
        out.push_str(&text(x + w / 2.0, top + fs * 1.3, &c.title, fs * 1.3, "middle", "#404040"));
        top += fs * 2.2;
    }
    let pie = c.kind == "pie" || c.kind == "doughnut";
    // 범례 항목: 원은 분류, 그 밖은 계열 (계열이 하나면 생략)
    let legend: Vec<(String, String)> = if pie {
        c.cats.iter().enumerate().map(|(i, n)| (n.clone(), pal[i % pal.len()].clone())).collect()
    } else if c.series.len() > 1 {
        c.series.iter().enumerate().map(|(i, s)| (s.name.clone(), colr(i, s))).collect()
    } else {
        vec![]
    };
    let show_legend = c.legend && !legend.is_empty();
    let bottom = y + h - if show_legend { fs * 2.2 } else { fs * 0.6 };
    if show_legend {
        let item_w: Vec<f64> = legend.iter().map(|(n, _)| fs * 1.2 + crate::measure::text_width(n, fs as f32, false, 0.0) as f64 + fs).collect();
        let total: f64 = item_w.iter().sum();
        let mut lx = x + (w - total).max(0.0) / 2.0;
        let ly = y + h - fs * 1.2;
        for ((n, col), iw) in legend.iter().zip(item_w) {
            let _ = write!(out, "<rect x=\"{lx:.2}\" y=\"{:.2}\" width=\"{:.2}\" height=\"{:.2}\" fill=\"{col}\"/>", ly - fs * 0.7, fs * 0.7, fs * 0.7);
            out.push_str(&text(lx + fs, ly, n, fs, "start", ink));
            lx += iw;
        }
    }
    if pie {
        let vals = &c.series[0].vals;
        let sum: f64 = vals.iter().filter(|v| **v > 0.0).sum();
        if sum <= 0.0 {
            return out;
        }
        let r = ((bottom - top).min(w) / 2.0 - fs * 0.5).max(4.0);
        let (cx, cy) = (x + w / 2.0, (top + bottom) / 2.0);
        let mut a0 = -std::f64::consts::FRAC_PI_2;
        for (i, v) in vals.iter().enumerate().filter(|(_, v)| **v > 0.0) {
            let a1 = a0 + v / sum * std::f64::consts::TAU;
            let col = c.series[0].color.clone().filter(|_| vals.len() == 1).unwrap_or_else(|| pal[i % pal.len()].clone());
            if (a1 - a0) >= std::f64::consts::TAU - 1e-6 {
                let _ = write!(out, "<circle cx=\"{cx:.2}\" cy=\"{cy:.2}\" r=\"{r:.2}\" fill=\"{col}\" stroke=\"#ffffff\" stroke-width=\"1\"/>");
            } else {
                let large = if a1 - a0 > std::f64::consts::PI { 1 } else { 0 };
                let _ = write!(
                    out,
                    "<path d=\"M{cx:.2},{cy:.2} L{:.2},{:.2} A{r:.2},{r:.2} 0 {large} 1 {:.2},{:.2} Z\" fill=\"{col}\" stroke=\"#ffffff\" stroke-width=\"1\"/>",
                    cx + r * a0.cos(), cy + r * a0.sin(), cx + r * a1.cos(), cy + r * a1.sin()
                );
            }
            if c.labels {
                let m = (a0 + a1) / 2.0;
                out.push_str(&text(cx + r * 0.65 * m.cos(), cy + r * 0.65 * m.sin() + fs * 0.35, &fmt_num(*v), fs, "middle", "#ffffff"));
            }
            a0 = a1;
        }
        if c.kind == "doughnut" {
            let _ = write!(out, "<circle cx=\"{cx:.2}\" cy=\"{cy:.2}\" r=\"{:.2}\" fill=\"#ffffff\"/>", r * 0.5);
        }
        return out;
    }
    // 축이 있는 차트
    let n = c.cats.len().max(c.series.iter().map(|s| s.vals.len()).max().unwrap_or(0)).max(1);
    let max = if c.stacked {
        (0..n).map(|k| c.series.iter().map(|s| s.vals.get(k).copied().unwrap_or(0.0).max(0.0)).sum::<f64>()).fold(0.0, f64::max)
    } else {
        c.series.iter().flat_map(|s| s.vals.iter().copied()).fold(0.0, f64::max)
    };
    let (vmax, step) = nice_max(max);
    let lab_w = fs * 0.6 * fmt_num(vmax).len() as f64 + fs * 0.6;
    let cat_h = fs * 1.8;
    let (px, py, pw, ph) = if c.horizontal {
        let cw = c.cats.iter().map(|s| crate::measure::text_width(s, fs as f32, false, 0.0) as f64).fold(0.0, f64::max) + fs * 0.6;
        (x + cw, top, w - cw - fs, bottom - top - cat_h)
    } else {
        (x + lab_w, top, w - lab_w - fs * 0.6, bottom - top - cat_h)
    };
    if pw <= 2.0 || ph <= 2.0 {
        return out;
    }
    // 눈금선과 값 눈금
    let mut v = 0.0;
    while v <= vmax + step * 0.01 {
        let f = v / vmax;
        if c.horizontal {
            let gx = px + pw * f;
            let _ = write!(out, "<line x1=\"{gx:.2}\" y1=\"{py:.2}\" x2=\"{gx:.2}\" y2=\"{:.2}\" stroke=\"#d9d9d9\" stroke-width=\"0.75\"/>", py + ph);
            out.push_str(&text(gx, py + ph + fs * 1.3, &fmt_num(v), fs, "middle", ink));
        } else {
            let gy = py + ph - ph * f;
            let _ = write!(out, "<line x1=\"{px:.2}\" y1=\"{gy:.2}\" x2=\"{:.2}\" y2=\"{gy:.2}\" stroke=\"#d9d9d9\" stroke-width=\"0.75\"/>", px + pw);
            out.push_str(&text(px - fs * 0.4, gy + fs * 0.35, &fmt_num(v), fs, "end", ink));
        }
        v += step;
    }
    let band = if c.horizontal { ph } else { pw } / n as f64;
    for (k, cat) in c.cats.iter().enumerate() {
        let mid = band * (k as f64 + 0.5);
        if c.horizontal {
            out.push_str(&text(px - fs * 0.4, py + ph - mid + fs * 0.35, cat, fs, "end", ink));
        } else {
            out.push_str(&text(px + mid, py + ph + fs * 1.3, cat, fs, "middle", ink));
        }
    }
    let ns = c.series.len().max(1);
    match c.kind.as_str() {
        "bar" => {
            let gap = band * 0.25;
            let bw = if c.stacked { band - 2.0 * gap } else { (band - 2.0 * gap) / ns as f64 };
            let mut acc = vec![0.0; n];
            for (si, s) in c.series.iter().enumerate() {
                let col = colr(si, s);
                for (k, v) in s.vals.iter().enumerate().take(n) {
                    let v = v.max(0.0);
                    let base = if c.stacked { acc[k] } else { 0.0 };
                    let off = gap + if c.stacked { 0.0 } else { bw * si as f64 };
                    let (len, b0) = (v / vmax, base / vmax);
                    let (rx, ry, rw, rh) = if c.horizontal {
                        (px + pw * b0, py + ph - band * (k as f64 + 1.0) + off, pw * len, bw)
                    } else {
                        (px + band * k as f64 + off, py + ph - ph * (b0 + len), bw, ph * len)
                    };
                    let _ = write!(out, "<rect x=\"{rx:.2}\" y=\"{ry:.2}\" width=\"{rw:.2}\" height=\"{rh:.2}\" fill=\"{col}\"/>");
                    if c.labels {
                        let (tx, ty, a) = if c.horizontal { (rx + rw + fs * 0.3, ry + rh / 2.0 + fs * 0.35, "start") } else { (rx + rw / 2.0, ry - fs * 0.3, "middle") };
                        out.push_str(&text(tx, ty, &fmt_num(v), fs, a, ink));
                    }
                    acc[k] += v;
                }
            }
        }
        _ => {
            let mut acc = vec![0.0; n];
            for (si, s) in c.series.iter().enumerate() {
                let col = colr(si, s);
                let pts: Vec<(f64, f64)> = s
                    .vals
                    .iter()
                    .enumerate()
                    .take(n)
                    .map(|(k, v)| {
                        let base = if c.stacked { acc[k] } else { 0.0 };
                        (px + band * (k as f64 + 0.5), py + ph - ph * ((base + v) / vmax))
                    })
                    .collect();
                let d = pts.iter().enumerate().map(|(i, (a, b))| format!("{}{a:.2},{b:.2}", if i == 0 { "M" } else { "L" })).collect::<String>();
                if c.kind == "area" {
                    if let (Some(f), Some(l)) = (pts.first(), pts.last()) {
                        let _ = write!(out, "<path d=\"{d} L{:.2},{:.2} L{:.2},{:.2} Z\" fill=\"{col}\" fill-opacity=\"0.85\"/>", l.0, py + ph, f.0, py + ph);
                    }
                } else {
                    let _ = write!(out, "<path d=\"{d}\" fill=\"none\" stroke=\"{col}\" stroke-width=\"{:.2}\" stroke-linejoin=\"round\"/>", (fs * 0.2).max(1.5));
                    for (a, b) in &pts {
                        let _ = write!(out, "<circle cx=\"{a:.2}\" cy=\"{b:.2}\" r=\"{:.2}\" fill=\"{col}\"/>", (fs * 0.25).max(2.0));
                    }
                }
                if c.labels {
                    for ((a, b), v) in pts.iter().zip(s.vals.iter()) {
                        out.push_str(&text(*a, b - fs * 0.5, &fmt_num(*v), fs, "middle", ink));
                    }
                }
                for (k, v) in s.vals.iter().enumerate().take(n) {
                    acc[k] += v;
                }
            }
        }
    }
    let _ = write!(out, "<line x1=\"{px:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\" stroke=\"#bfbfbf\" stroke-width=\"0.75\"/>", py + ph, px + pw, py + ph);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bar_and_pie() {
        let bar = r#"<c:chartSpace xmlns:c="c" xmlns:a="a"><c:chart><c:title><c:tx><c:rich><a:p><a:r><a:t>출석</a:t></a:r></a:p></c:rich></c:tx></c:title><c:plotArea><c:barChart><c:barDir val="col"/><c:grouping val="clustered"/><c:ser><c:tx><c:strRef><c:strCache><c:ptCount val="1"/><c:pt idx="0"><c:v>출석</c:v></c:pt></c:strCache></c:strRef></c:tx><c:cat><c:strRef><c:strCache><c:ptCount val="2"/><c:pt idx="0"><c:v>1부</c:v></c:pt><c:pt idx="1"><c:v>2부</c:v></c:pt></c:strCache></c:strRef></c:cat><c:val><c:numRef><c:numCache><c:ptCount val="2"/><c:pt idx="0"><c:v>120</c:v></c:pt><c:pt idx="1"><c:v>85</c:v></c:pt></c:numCache></c:numRef></c:val></c:ser></c:barChart></c:plotArea></c:chart></c:chartSpace>"#;
        let c = parse(bar).unwrap();
        assert_eq!(c.kind, "bar");
        assert_eq!(c.cats, vec!["1부", "2부"]);
        assert_eq!(c.series[0].vals, vec![120.0, 85.0]);
        assert_eq!(c.title, "출석");
        let svg = chart_svg(&c, 0.0, 0.0, 400.0, 300.0, &[]);
        assert_eq!(svg.matches("<rect").count(), 2);
        let pie = bar.replace("barChart", "pieChart");
        let p = parse(&pie).unwrap();
        assert_eq!(p.kind, "pie");
        assert_eq!(chart_svg(&p, 0.0, 0.0, 400.0, 300.0, &[]).matches("<path").count(), 2);
        assert_eq!(nice_max(120.0), (125.0, 25.0));
    }
}
