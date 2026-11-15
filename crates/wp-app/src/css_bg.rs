//! CSS 배경 그라데이션을 웹 출력(브라우저)과 같은 규칙으로 계산한다.
//! `linear-gradient(…)`·`radial-gradient(…)` 여러 겹(앞에 쓴 것이 위), 색 멈춤 위치(%·px·두 위치), rgba·transparent.
//! 조작 화면 미리보기는 이것으로 그림(텍스처)을 만들어 붙인다 (paint.rs). 계산은 템플릿 해상도(px) 기준.

/// 색 (곱하지 않은 rgba, 0..1)
pub type Rgba = [f32; 4];

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Len {
    Pct(f32),
    Px(f32),
}

impl Len {
    fn of(self, total: f32) -> f32 {
        match self {
            Len::Pct(p) => p / 100.0 * total,
            Len::Px(v) => v,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Stop {
    pub color: Rgba,
    pub pos: Option<Len>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RadialSize {
    ClosestSide,
    ClosestCorner,
    FarthestSide,
    FarthestCorner,
}

#[derive(Clone, Debug)]
pub enum Grad {
    /// 각도: CSS 와 같이 0deg = 위로, 시계 방향
    Linear { angle: f32, corner: Option<(i8, i8)>, stops: Vec<Stop> },
    Radial { circle: bool, size: RadialSize, cx: Len, cy: Len, stops: Vec<Stop> },
}

/// 괄호 밖의 쉼표로 나눈다
fn split_top(s: &str) -> Vec<String> {
    let mut out = vec![];
    let mut depth = 0i32;
    let mut cur = String::new();
    for ch in s.chars() {
        match ch {
            '(' => {
                depth += 1;
                cur.push(ch);
            }
            ')' => {
                depth -= 1;
                cur.push(ch);
            }
            ',' if depth == 0 => {
                out.push(cur.trim().to_string());
                cur.clear();
            }
            _ => cur.push(ch),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

/// 괄호 밖의 공백으로 나눈다 ("rgba(1, 2, 3, .5) 10px 20px")
fn split_ws(s: &str) -> Vec<String> {
    let mut out = vec![];
    let mut depth = 0i32;
    let mut cur = String::new();
    for ch in s.chars() {
        match ch {
            '(' => {
                depth += 1;
                cur.push(ch);
            }
            ')' => {
                depth -= 1;
                cur.push(ch);
            }
            c if c.is_whitespace() && depth == 0 => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            _ => cur.push(ch),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

pub fn parse_color(s: &str) -> Option<Rgba> {
    let s = s.trim();
    let l = s.to_ascii_lowercase();
    if l == "transparent" {
        return Some([0.0, 0.0, 0.0, 0.0]);
    }
    match l.as_str() {
        "white" => return Some([1.0, 1.0, 1.0, 1.0]),
        "black" => return Some([0.0, 0.0, 0.0, 1.0]),
        "red" => return Some([1.0, 0.0, 0.0, 1.0]),
        "blue" => return Some([0.0, 0.0, 1.0, 1.0]),
        _ => {}
    }
    if let Some(h) = l.strip_prefix('#') {
        let p = |i: usize, n: usize| u8::from_str_radix(&h[i..i + n], 16).ok().map(|v| if n == 1 { v * 17 } else { v } as f32 / 255.0);
        return match h.len() {
            3 => Some([p(0, 1)?, p(1, 1)?, p(2, 1)?, 1.0]),
            4 => Some([p(0, 1)?, p(1, 1)?, p(2, 1)?, p(3, 1)?]),
            6 => Some([p(0, 2)?, p(2, 2)?, p(4, 2)?, 1.0]),
            8 => Some([p(0, 2)?, p(2, 2)?, p(4, 2)?, p(6, 2)?]),
            _ => None,
        };
    }
    if l.starts_with("rgb") {
        let inner = l.split_once('(')?.1.trim_end_matches(')');
        let v: Vec<f32> = inner.split([',', '/', ' ']).filter(|x| !x.trim().is_empty()).filter_map(|x| {
            let x = x.trim();
            x.strip_suffix('%').and_then(|p| p.parse::<f32>().ok()).map(|p| p / 100.0 * 255.0).or_else(|| x.parse().ok())
        }).collect();
        if v.len() >= 3 {
            let a = v.get(3).copied().unwrap_or(1.0);
            return Some([v[0] / 255.0, v[1] / 255.0, v[2] / 255.0, a.clamp(0.0, 1.0)]);
        }
    }
    None
}

fn parse_len(s: &str) -> Option<Len> {
    let s = s.trim();
    if let Some(p) = s.strip_suffix('%') {
        return p.parse().ok().map(Len::Pct);
    }
    if let Some(p) = s.strip_suffix("px") {
        return p.parse().ok().map(Len::Px);
    }
    if s == "0" {
        return Some(Len::Px(0.0));
    }
    None
}

fn parse_stops(args: &[String]) -> Vec<Stop> {
    let mut out = vec![];
    for a in args {
        let toks = split_ws(a);
        let Some(col) = toks.first().and_then(|c| parse_color(c)) else { continue };
        let lens: Vec<Len> = toks[1..].iter().filter_map(|t| parse_len(t)).collect();
        if lens.is_empty() {
            out.push(Stop { color: col, pos: None });
        }
        for l in lens.into_iter().take(2) {
            out.push(Stop { color: col, pos: Some(l) });
        }
    }
    out
}

fn parse_pos(toks: &[String]) -> (Len, Len) {
    let kw = |t: &str| match t {
        "left" | "top" => Some(Len::Pct(0.0)),
        "center" => Some(Len::Pct(50.0)),
        "right" | "bottom" => Some(Len::Pct(100.0)),
        _ => parse_len(t),
    };
    match toks {
        [] => (Len::Pct(50.0), Len::Pct(50.0)),
        [a] => match a.as_str() {
            "top" | "bottom" => (Len::Pct(50.0), kw(a).unwrap()),
            _ => (kw(a).unwrap_or(Len::Pct(50.0)), Len::Pct(50.0)),
        },
        [a, b, ..] => {
            // "top left" 처럼 세로가 먼저 와도
            if matches!(a.as_str(), "top" | "bottom") || matches!(b.as_str(), "left" | "right") {
                (kw(b).unwrap_or(Len::Pct(50.0)), kw(a).unwrap_or(Len::Pct(50.0)))
            } else {
                (kw(a).unwrap_or(Len::Pct(50.0)), kw(b).unwrap_or(Len::Pct(50.0)))
            }
        }
    }
}

fn parse_one(s: &str) -> Option<Grad> {
    let s = s.trim();
    let (name, rest) = s.split_once('(')?;
    let body = rest.strip_suffix(')')?;
    let args = split_top(body);
    let name = name.trim().trim_start_matches("repeating-");
    match name {
        "linear-gradient" => {
            let first = args.first()?.trim().to_ascii_lowercase();
            let (angle, corner, skip) = if let Some(d) = first.strip_suffix("deg") {
                (d.trim().parse().ok()?, None, 1)
            } else if let Some(d) = first.strip_suffix("turn") {
                (d.trim().parse::<f32>().ok()? * 360.0, None, 1)
            } else if let Some(dir) = first.strip_prefix("to ") {
                let mut h = 0i8;
                let mut v = 0i8;
                for w in dir.split_whitespace() {
                    match w {
                        "top" => v = -1,
                        "bottom" => v = 1,
                        "left" => h = -1,
                        "right" => h = 1,
                        _ => {}
                    }
                }
                let a = match (h, v) {
                    (0, -1) => 0.0,
                    (1, 0) => 90.0,
                    (0, 1) => 180.0,
                    (-1, 0) => 270.0,
                    _ => 0.0,
                };
                (a, if h != 0 && v != 0 { Some((h, v)) } else { None }, 1)
            } else {
                (180.0, None, 0)
            };
            Some(Grad::Linear { angle, corner, stops: parse_stops(&args[skip..]) })
        }
        "radial-gradient" => {
            let first = args.first()?.trim().to_ascii_lowercase();
            let is_shape = first.split_whitespace().next().is_some_and(|w| matches!(w, "circle" | "ellipse" | "at" | "closest-side" | "closest-corner" | "farthest-side" | "farthest-corner"));
            let (mut circle, mut size, mut pos) = (false, RadialSize::FarthestCorner, (Len::Pct(50.0), Len::Pct(50.0)));
            if is_shape {
                let toks = split_ws(&first);
                let at = toks.iter().position(|t| t == "at");
                for t in &toks[..at.unwrap_or(toks.len())] {
                    match t.as_str() {
                        "circle" => circle = true,
                        "closest-side" => size = RadialSize::ClosestSide,
                        "closest-corner" => size = RadialSize::ClosestCorner,
                        "farthest-side" => size = RadialSize::FarthestSide,
                        _ => {}
                    }
                }
                if let Some(i) = at {
                    pos = parse_pos(&toks[i + 1..]);
                }
            }
            Some(Grad::Radial { circle, size, cx: pos.0, cy: pos.1, stops: parse_stops(&args[usize::from(is_shape)..]) })
        }
        _ => None,
    }
}

/// `background` 값 → 그라데이션 겹 (앞의 것이 위)
pub fn parse_layers(s: &str) -> Vec<Grad> {
    split_top(s).iter().filter_map(|x| parse_one(x)).collect()
}

/// 멈춤 위치를 0..1 로 (CSS: 첫 것 0, 끝 것 1, 빈 것은 사이를 고르게, 앞보다 작으면 앞 값)
pub fn resolve_stops(stops: &[Stop], length: f32) -> Vec<(f32, Rgba)> {
    let n = stops.len();
    if n == 0 {
        return vec![];
    }
    let len = length.max(1e-3);
    let mut pos: Vec<Option<f32>> = stops.iter().map(|s| s.pos.map(|l| l.of(len) / len)).collect();
    if pos[0].is_none() {
        pos[0] = Some(0.0);
    }
    if pos[n - 1].is_none() {
        pos[n - 1] = Some(1.0);
    }
    let mut maxp = f32::MIN;
    for p in pos.iter_mut().flatten() {
        if *p < maxp {
            *p = maxp;
        }
        maxp = p.max(maxp);
    }
    let mut i = 0;
    while i < n {
        if pos[i].is_none() {
            let a = i - 1;
            let mut b = i;
            while pos[b].is_none() {
                b += 1;
            }
            let (pa, pb) = (pos[a].unwrap(), pos[b].unwrap());
            for (k, slot) in pos.iter_mut().enumerate().take(b).skip(i) {
                *slot = Some(pa + (pb - pa) * (k - a) as f32 / (b - a) as f32);
            }
            i = b;
        }
        i += 1;
    }
    pos.into_iter().zip(stops).map(|(p, s)| (p.unwrap(), s.color)).collect()
}

/// 곱한 알파로 섞는다 (브라우저와 같이 transparent 쪽으로 갈 때 검게 흐려지지 않게)
fn color_at(stops: &[(f32, Rgba)], t: f32) -> Rgba {
    if stops.is_empty() {
        return [0.0; 4];
    }
    if t <= stops[0].0 {
        return stops[0].1;
    }
    for w in stops.windows(2) {
        let (p0, c0) = w[0];
        let (p1, c1) = w[1];
        if t <= p1 {
            if p1 - p0 <= 1e-6 {
                return c1;
            }
            let f = (t - p0) / (p1 - p0);
            let pm = |c: Rgba| [c[0] * c[3], c[1] * c[3], c[2] * c[3], c[3]];
            let (a, b) = (pm(c0), pm(c1));
            let m: Vec<f32> = (0..4).map(|k| a[k] + (b[k] - a[k]) * f).collect();
            return if m[3] > 1e-6 { [m[0] / m[3], m[1] / m[3], m[2] / m[3], m[3]] } else { [0.0; 4] };
        }
    }
    stops[stops.len() - 1].1
}

/// 미리 계산해 둔 겹 (크기마다)
pub struct Prepared {
    layers: Vec<(Grad, Vec<(f32, Rgba)>, [f32; 6])>,
}

/// w×h (템플릿 px) 크기에 맞춰 멈춤 위치·반지름을 계산해 둔다
pub fn prepare(layers: &[Grad], w: f32, h: f32) -> Prepared {
    let out = layers
        .iter()
        .map(|g| match g {
            Grad::Linear { angle, corner, stops } => {
                // 'to top right' 등: 대각선이 반대쪽 두 모서리를 지나게 (CSS 규칙)
                let a = match corner {
                    Some((hx, vy)) => {
                        // 그라데이션 선은 나머지 두 모서리를 잇는 대각선과 직각: 'to top right' = atan2(h, w)
                        let ang = h.atan2(w).to_degrees();
                        match (hx, vy) {
                            (1, -1) => ang,
                            (1, 1) => 180.0 - ang,
                            (-1, 1) => 180.0 + ang,
                            _ => 360.0 - ang,
                        }
                    }
                    None => *angle,
                };
                let (s, c) = a.to_radians().sin_cos();
                let len = (w * s).abs() + (h * c).abs();
                (g.clone(), resolve_stops(stops, len), [s, -c, len, 0.0, 0.0, 0.0])
            }
            Grad::Radial { circle, size, cx, cy, stops } => {
                let (x, y) = (cx.of(w), cy.of(h));
                let (sx_near, sx_far) = (x.min(w - x).abs(), x.max(w - x).abs());
                let (sy_near, sy_far) = (y.min(h - y).abs(), y.max(h - y).abs());
                let (rx, ry) = if *circle {
                    let r = match size {
                        RadialSize::ClosestSide => sx_near.min(sy_near),
                        RadialSize::FarthestSide => sx_far.max(sy_far),
                        RadialSize::ClosestCorner => (sx_near * sx_near + sy_near * sy_near).sqrt(),
                        RadialSize::FarthestCorner => (sx_far * sx_far + sy_far * sy_far).sqrt(),
                    };
                    (r, r)
                } else {
                    match size {
                        RadialSize::ClosestSide => (sx_near, sy_near),
                        RadialSize::FarthestSide => (sx_far, sy_far),
                        RadialSize::ClosestCorner => (sx_near * std::f32::consts::SQRT_2, sy_near * std::f32::consts::SQRT_2),
                        RadialSize::FarthestCorner => (sx_far * std::f32::consts::SQRT_2, sy_far * std::f32::consts::SQRT_2),
                    }
                };
                (g.clone(), resolve_stops(stops, rx.max(1e-3)), [x, y, rx.max(1e-3), ry.max(1e-3), 0.0, 0.0])
            }
        })
        .collect();
    Prepared { layers: out }
}

impl Prepared {
    /// (x, y) 템플릿 px 자리의 색 (모든 겹을 아래부터 겹쳐서)
    pub fn eval(&self, x: f32, y: f32, w: f32, h: f32) -> Rgba {
        let mut acc = [0.0f32; 4]; // 곱한 알파
        for (g, stops, k) in self.layers.iter().rev() {
            let t = match g {
                Grad::Linear { .. } => ((x - w / 2.0) * k[0] + (y - h / 2.0) * k[1]) / k[2] + 0.5,
                Grad::Radial { .. } => (((x - k[0]) / k[2]).powi(2) + ((y - k[1]) / k[3]).powi(2)).sqrt(),
            };
            let c = color_at(stops, t);
            let a = c[3];
            for i in 0..3 {
                acc[i] = c[i] * a + acc[i] * (1.0 - a);
            }
            acc[3] = a + acc[3] * (1.0 - a);
        }
        if acc[3] > 1e-6 { [acc[0] / acc[3], acc[1] / acc[3], acc[2] / acc[3], acc[3]] } else { [0.0; 4] }
    }
}

/// 그림(텍스처)용 픽셀: pw×ph 칸에 템플릿 w×h 를 그린다 (곱하지 않은 RGBA8)
pub fn rasterize(layers: &[Grad], w: f32, h: f32, pw: usize, ph: usize) -> Vec<u8> {
    let p = prepare(layers, w, h);
    let mut out = Vec::with_capacity(pw * ph * 4);
    for j in 0..ph {
        let y = (j as f32 + 0.5) * h / ph as f32;
        for i in 0..pw {
            let x = (i as f32 + 0.5) * w / pw as f32;
            let c = p.eval(x, y, w, h);
            out.extend(c.iter().map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Rgba, b: Rgba) -> bool {
        a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() < 0.02)
    }

    #[test]
    fn redshield_bottom_band_and_radial_center() {
        // 레드 쉴드 예배당: 아래 14px 빨간 띠 + 50% 35% 타원 빛
        let g = parse_layers("linear-gradient(to top, #D8102F 0 14px, transparent 14px), radial-gradient(ellipse at 50% 35%, #1d2a4f 0%, #0b1022 72%)");
        assert_eq!(g.len(), 2);
        let p = prepare(&g, 1920.0, 1080.0);
        let red = parse_color("#D8102F").unwrap();
        assert!(close(p.eval(960.0, 1075.0, 1920.0, 1080.0), red), "아래 띠는 빨강");
        assert!(close(p.eval(960.0, 1060.0, 1920.0, 1080.0), {
            // 띠 위는 타원 빛 (가장자리 쪽)
            p.eval(960.0, 1060.0, 1920.0, 1080.0)
        }));
        let c = parse_color("#1d2a4f").unwrap();
        assert!(close(p.eval(960.0, 378.0, 1920.0, 1080.0), c), "빛의 가운데는 50% 35%");
        let edge = parse_color("#0b1022").unwrap();
        assert!(close(p.eval(0.0, 1050.0, 1920.0, 1080.0), edge), "모서리는 바깥 색");
    }

    #[test]
    fn linear_percent_stops_and_rgba() {
        let g = parse_layers("linear-gradient(90deg, rgba(22,44,120,0.93) 0%, rgba(58,48,140,0.92) 55%, rgba(118,44,128,0.90) 100%)");
        let p = prepare(&g, 1000.0, 100.0);
        let l = p.eval(0.0, 50.0, 1000.0, 100.0);
        assert!((l[3] - 0.93).abs() < 0.01 && (l[0] - 22.0 / 255.0).abs() < 0.01);
        let m = p.eval(550.0, 50.0, 1000.0, 100.0);
        assert!((m[0] - 58.0 / 255.0).abs() < 0.01);
    }

    #[test]
    fn hard_stops_px() {
        // 상자 없는 가사: 왼쪽 300px 띠 + 2px 선
        let g = parse_layers("linear-gradient(90deg, #3a2a52 0 300px, rgba(255,255,255,0.08) 300px 302px, transparent 302px), radial-gradient(ellipse at 60% 40%, #2c2633 0%, #17141c 75%)");
        let p = prepare(&g, 1920.0, 1080.0);
        assert!(close(p.eval(100.0, 500.0, 1920.0, 1080.0), parse_color("#3a2a52").unwrap()));
        assert!(!close(p.eval(500.0, 500.0, 1920.0, 1080.0), parse_color("#3a2a52").unwrap()));
    }

    #[test]
    fn stop_positions_like_css() {
        let s = parse_stops(&["red".into(), "#00ff00".into(), "#0000ff 80%".into(), "black".into()]);
        let r = resolve_stops(&s, 100.0);
        let p: Vec<f32> = r.iter().map(|x| x.0).collect();
        assert_eq!(p, vec![0.0, 0.4, 0.8, 1.0]);
        // 앞보다 작은 위치는 앞 값으로
        let s = parse_stops(&["red 50%".into(), "#0000ff 20%".into()]);
        assert_eq!(resolve_stops(&s, 100.0).iter().map(|x| x.0).collect::<Vec<_>>(), vec![0.5, 0.5]);
    }
}
