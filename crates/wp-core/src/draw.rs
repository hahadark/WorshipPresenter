//! 화면에 그리기 · 강조 (Electron core.js drawStroke · drawUndo · drawClear · drawSpot) [트랙 B]
//! 출력 화면(render.js paintDraw)이 SSE 'draw' 이벤트 { strokes: [{id, pts:[[x,y]], color, width, alpha}], spot: {x,y,r} } 를 그린다.
//! 좌표는 화면 너비·높이에 대한 0~1 비율 (width 는 화면 높이에 대한 비율).

use crate::core::{Core, Ev};
use anyhow::{bail, Result};
use serde_json::{json, Value};

#[derive(Clone, Debug, Default)]
pub struct Draw {
    pub strokes: Vec<Value>,
    pub spot: Option<Value>,
}

impl Draw {
    pub fn json(&self) -> Value {
        json!({ "strokes": self.strokes, "spot": self.spot })
    }
    pub fn is_empty(&self) -> bool {
        self.strokes.is_empty() && self.spot.is_none()
    }
}

fn clamp(v: &Value, lo: f64, hi: f64, d: f64) -> f64 {
    v.as_f64().filter(|x| x.is_finite()).unwrap_or(d).clamp(lo, hi)
}

fn is_hex_color(s: &str) -> bool {
    s.len() >= 4 && s.len() <= 9 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit())
}

/// 선 하나 정리 (같은 id 면 바꿈 — 끄는 동안 계속 보낸다)
pub fn norm_stroke(s: &Value) -> Result<Value> {
    let id: String = s["id"].as_str().unwrap_or("").chars().take(40).collect();
    if id.is_empty() {
        bail!("선 id 가 없습니다.");
    }
    let pts: Vec<Value> = s["pts"].as_array().into_iter().flatten().take(4000).map(|p| json!([clamp(&p[0], -0.1, 1.1, 0.0), clamp(&p[1], -0.1, 1.1, 0.0)])).collect();
    let color = s["color"].as_str().filter(|c| is_hex_color(c)).unwrap_or("#ff3b30");
    Ok(json!({ "id": id, "pts": pts, "color": color, "width": clamp(&s["width"], 0.001, 0.08, 0.006), "alpha": clamp(&s["alpha"], 0.1, 1.0, 1.0) }))
}

impl Core {
    fn draw_changed(&self) {
        self.emit(Ev::Draw);
        self.emit(Ev::Ui);
    }
    pub fn draw_stroke(&mut self, s: &Value) -> Result<usize> {
        let st = norm_stroke(s)?;
        let list = &mut self.b.draw.strokes;
        match list.iter().position(|x| x["id"] == st["id"]) {
            Some(k) => list[k] = st,
            None => list.push(st),
        }
        if list.len() > 300 {
            let n = list.len() - 300;
            list.drain(0..n);
        }
        let n = list.len();
        self.draw_changed();
        Ok(n)
    }
    pub fn draw_undo(&mut self) -> usize {
        self.b.draw.strokes.pop();
        self.draw_changed();
        self.b.draw.strokes.len()
    }
    pub fn draw_clear(&mut self) {
        self.b.draw = Draw::default();
        self.draw_changed();
    }
    /// 강조: 원 하나만 밝게 (나머지 어둡게). None 이면 끔
    pub fn draw_spot(&mut self, spot: Option<(f64, f64, f64)>) {
        self.b.draw.spot = spot.map(|(x, y, r)| json!({ "x": x.clamp(0.0, 1.0), "y": y.clamp(0.0, 1.0), "r": r.clamp(0.03, 0.6) }));
        self.draw_changed();
    }
    pub fn draw_json(&self) -> Value {
        self.b.draw.json()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stroke_norm() {
        let s = norm_stroke(&json!({ "id": "s1", "pts": [[0.5, 2.0], [-1, "x"]], "color": "red", "width": 1 })).unwrap();
        assert_eq!(s["pts"], json!([[0.5, 1.1], [-0.1, 0.0]]));
        assert_eq!(s["color"], "#ff3b30");
        assert_eq!(s["width"], 0.08);
        assert!(norm_stroke(&json!({ "pts": [] })).is_err());
        assert!(is_hex_color("#ffd479") && !is_hex_color("#xyz"));
    }
}

#[cfg(test)]
mod sse_tests {
    // [트랙 B] 출력 화면 접속(/events) → 그리면 'draw' 이벤트가 온다
    use crate::core::Core;
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn draw_event_reaches_output() {
        let dir = std::env::temp_dir().join(format!("wp-draw-{}-{}", crate::util::now_ms(), rand::random::<u32>()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("bible.db"), b"").unwrap();
        let mut c = Core::open(dir.clone(), dir, None).unwrap();
        c.port = 47610 + (rand::random::<u16>() % 300);
        let core = Arc::new(crate::lockcheck::Mutex::new(c));
        let port = crate::server::serve(core.clone()).await.unwrap();
        let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        s.write_all(format!("GET /events?client=main HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAccept: text/event-stream\r\n\r\n").as_bytes()).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        core.lock().draw_stroke(&serde_json::json!({ "id": "s1", "pts": [[0.1, 0.1], [0.5, 0.5]], "color": "#ffd60a" })).unwrap();
        let mut buf = vec![0u8; 65536];
        let mut got = String::new();
        let t0 = web_time::Instant::now();
        while t0.elapsed().as_secs() < 5 && !got.contains("event: draw") {
            if let Ok(Ok(n)) = tokio::time::timeout(std::time::Duration::from_millis(500), s.read(&mut buf)).await {
                got.push_str(&String::from_utf8_lossy(&buf[..n]));
            }
        }
        assert!(got.contains("event: render"), "{got}");
        assert!(got.contains("event: draw"), "{got}");
        assert!(got.contains("#ffd60a"));
    }
}
