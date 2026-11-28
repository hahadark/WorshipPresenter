//! 움직이는 배경의 조작 화면 그림 (템플릿 background.motion). 웹 출력은 web/shared/render.js motionFrame 이 캔버스로 움직이고,
//! 여기서는 같은 수식으로 한 장면(t 초)을 그린다 — 미리보기·LIVE·썸네일·템플릿 편집기에서 어떤 배경인지 보이게.
//! 색 · 자리 · 크기 수식은 render.js 와 한 줄씩 맞춘다 (바꾸면 둘 다).
use eframe::egui::{self, epaint::Mesh, Color32, Pos2, Rect};

/// (값, 화면 이름) — 템플릿 편집기 선택지
pub const KINDS: [(&str, &str); 5] = [("", "없음"), ("aurora", "오로라"), ("waves", "물결"), ("stars", "별빛"), ("bokeh", "빛망울")];

pub fn is_motion(kind: &str) -> bool {
    KINDS.iter().skip(1).any(|k| k.0 == kind)
}

/// render.js mulberry 와 같은 의사 난수 (같은 씨앗이면 같은 별 자리)
struct Mulberry(u32);
impl Mulberry {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x6D2B_79F5);
        let a = self.0;
        let mut t = (a ^ (a >> 15)).wrapping_mul(1 | a);
        t = (t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t))) ^ t;
        ((t ^ (t >> 14)) as f64 / 4_294_967_296.0) as f32
    }
}

fn hex(h: &str, a: f32) -> Color32 {
    let n = u32::from_str_radix(h.trim_start_matches('#'), 16).unwrap_or(0xffffff);
    Color32::from_rgba_unmultiplied((n >> 16) as u8, (n >> 8) as u8, n as u8, (a.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// 가운데 a → 가장자리 0 으로 옅어지는 원 (캔버스 createRadialGradient 두 멈춤과 같은 모양)
fn blob(mesh: &mut Mesh, c: Pos2, r: f32, h: &str, a: f32, clip: Rect) {
    if r <= 0.0 || !clip.expand(r).contains(c) {
        return;
    }
    let base = mesh.vertices.len() as u32;
    mesh.colored_vertex(c, hex(h, a));
    const N: u32 = 40;
    for k in 0..N {
        let ang = k as f32 / N as f32 * std::f32::consts::TAU;
        mesh.colored_vertex(c + egui::vec2(ang.cos(), ang.sin()) * r, hex(h, 0.0));
    }
    for k in 0..N {
        mesh.add_triangle(base, base + 1 + k, base + 1 + (k + 1) % N);
    }
}

/// 한 장면. rect = 화면에 그릴 칸(템플릿 전체), t = 초
pub fn paint(p: &egui::Painter, rect: Rect, kind: &str, t: f32) {
    let (w, h) = (rect.width(), rect.height());
    let o = rect.min;
    let at = |x: f32, y: f32| Pos2::new(o.x + x, o.y + y);
    let mut mesh = Mesh::default();
    match kind {
        "aurora" => {
            for (i, (c, cx, cy)) in [("#22d3a6", 0.25, 0.30), ("#6d5dfc", 0.70, 0.35), ("#1fb6ff", 0.45, 0.75)].into_iter().enumerate() {
                let fi = i as f32;
                let x = w * cx + w * 0.18 * (t * 0.07 * (fi + 1.0) + fi * 2.0).sin();
                let y = h * cy + h * 0.12 * (t * 0.05 * (fi + 1.0) + fi).cos();
                blob(&mut mesh, at(x, y), w * 0.4, c, 0.45, rect);
            }
        }
        "waves" => {
            for i in 0..3 {
                let fi = i as f32;
                let (base, amp, len) = (h * (0.58 + 0.1 * fi), h * (0.05 - 0.01 * fi), w * (0.9 - 0.2 * fi));
                let ph = t * (0.35 + 0.15 * fi) + fi * 1.7;
                let col = hex(["#3b82f6", "#22d3ee", "#1d4ed8"][i], 0.22);
                let y_of = |x: f32| base + amp * ((std::f32::consts::TAU * x) / len + ph).sin();
                const STEPS: usize = 96;
                for k in 0..STEPS {
                    let (x0, x1) = (w * k as f32 / STEPS as f32, w * (k + 1) as f32 / STEPS as f32);
                    let b = mesh.vertices.len() as u32;
                    for (x, y) in [(x0, y_of(x0)), (x1, y_of(x1)), (x1, h), (x0, h)] {
                        mesh.colored_vertex(at(x, y), col);
                    }
                    mesh.add_triangle(b, b + 1, b + 2);
                    mesh.add_triangle(b, b + 2, b + 3);
                }
            }
        }
        "stars" => {
            blob(&mut mesh, at(w * 0.3, h * 0.4), w * 0.45, "#7c3aed", 0.28, rect);
            blob(&mut mesh, at(w * 0.72, h * 0.62), w * 0.4, "#2563eb", 0.24, rect);
            p.add(egui::Shape::mesh(std::mem::take(&mut mesh)));
            let mut rnd = Mulberry(7);
            for _ in 0..220 {
                let (x, y) = (rnd.next() * w, rnd.next() * h);
                let r = (0.6 + rnd.next() * 1.6) * h / 1080.0;
                let (f, ph) = (0.4 + rnd.next() * 1.2, rnd.next() * std::f32::consts::TAU);
                let a = 0.35 + 0.65 * (t * f + ph).sin().abs();
                p.circle_filled(at((x + t * w * 0.004) % w, y), r.max(0.5), Color32::from_white_alpha((a * 255.0) as u8));
            }
        }
        "bokeh" => {
            let mut rnd = Mulberry(11);
            let cols = ["#ffd28a", "#ff9e6b", "#ffe9b8"];
            for i in 0..16 {
                let (x0, y0) = (rnd.next() * w, rnd.next() * h);
                let r = w * (0.03 + rnd.next() * 0.06);
                let a = 0.18 + rnd.next() * 0.22;
                let sp = h * (0.01 + rnd.next() * 0.02);
                let span = h + r * 2.0;
                let y = (y0 - t * sp + r).rem_euclid(span) - r;
                blob(&mut mesh, at(x0 + (t * 0.2 + i as f32).sin() * w * 0.01, y), r, cols[i % 3], a, rect);
            }
        }
        _ => {}
    }
    if !mesh.is_empty() {
        p.add(egui::Shape::mesh(mesh));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn same_random_as_render_js() {
        // node 로 render.js 의 mulberry(7) 를 세 번 부른 값
        let mut r = Mulberry(7);
        let v: Vec<f32> = (0..3).map(|_| r.next()).collect();
        let js = [0.011_704_753_f32, 0.061_958_257, 0.976_907_6];
        for (a, b) in v.iter().zip(js.iter()) {
            assert!((a - b).abs() < 1e-6, "{v:?} vs {js:?}");
        }
        assert!(is_motion("aurora") && !is_motion("") && !is_motion("fire"));
    }
}
