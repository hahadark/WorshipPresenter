//! 미리보기와 LIVE 를 따로 들고 있는 송출 엔진 (Electron 판 presentation.js)

use crate::util::now_ms;
use serde_json::{json, Value};

#[derive(Clone, Debug)]
pub struct Deck {
    pub passage: Value,
    pub slides: Vec<Value>,
    pub index: usize,
}

#[derive(Clone, Debug)]
pub struct Live {
    pub deck: Deck,
    pub visible: bool,
    pub sent_at: i64,
}

#[derive(Default)]
pub struct Presentation {
    pub version: u64,
    pub preview: Option<Deck>,
    pub live: Option<Live>,
}

pub enum Move {
    Ok,
    NoLive,
    Edge(&'static str),
}

impl Presentation {
    pub fn set_preview(&mut self, passage: Value, slides: Vec<Value>, index: usize) {
        let n = slides.len().max(1);
        self.preview = Some(Deck { passage, slides, index: index.min(n - 1) });
    }

    pub fn set_preview_index(&mut self, i: usize) {
        if let Some(p) = &mut self.preview {
            p.index = i.min(p.slides.len().saturating_sub(1));
        }
    }

    /// 미리보기(또는 주어진 묶음)를 LIVE 로
    pub fn send_live(&mut self, deck: Option<Deck>) -> bool {
        let Some(src) = deck.or_else(|| self.preview.clone()) else { return false };
        if src.slides.is_empty() {
            return false;
        }
        self.live = Some(Live { deck: src, visible: true, sent_at: now_ms() });
        self.bump();
        true
    }

    pub fn move_by(&mut self, d: i64) -> Move {
        let Some(l) = &mut self.live else { return Move::NoLive };
        let next = l.deck.index as i64 + d;
        if next < 0 || next >= l.deck.slides.len() as i64 {
            return Move::Edge(if d > 0 { "end" } else { "start" });
        }
        l.deck.index = next as usize;
        self.bump();
        Move::Ok
    }

    pub fn goto(&mut self, i: usize) {
        if let Some(l) = &mut self.live {
            l.deck.index = i.min(l.deck.slides.len().saturating_sub(1));
            self.bump();
        }
    }

    pub fn clear(&mut self) {
        if let Some(l) = &mut self.live {
            if l.visible {
                l.visible = false;
                self.bump();
            }
        }
    }

    pub fn restore(&mut self) {
        if let Some(l) = &mut self.live {
            if !l.visible {
                l.visible = true;
                self.bump();
            }
        }
    }

    pub fn toggle(&mut self) {
        match &self.live {
            Some(l) if l.visible => self.clear(),
            Some(_) => self.restore(),
            None => {}
        }
    }

    pub fn bump(&mut self) {
        self.version += 1;
    }

    /// 출력 화면·외부 연동이 쓰는 LIVE 상태 (getLiveState 와 같은 모양)
    pub fn live_state(&self) -> Value {
        match &self.live {
            None => json!({ "version": self.version, "hasLive": false, "visible": false, "index": -1, "count": 0, "slide": null, "nextSlide": null, "passage": null, "slides": [], "sentAt": null }),
            Some(l) => {
                let d = &l.deck;
                json!({
                    "version": self.version, "hasLive": true, "visible": l.visible, "index": d.index, "count": d.slides.len(),
                    "slide": d.slides.get(d.index), "nextSlide": d.slides.get(d.index + 1), "passage": d.passage,
                    "slides": d.slides.iter().map(|s| match s["label"].as_str() { Some(lb) if !lb.is_empty() => format!("¶{}", lb), _ => s["ref"].as_str().filter(|x| !x.is_empty()).or(s["kind"].as_str()).unwrap_or("").to_string() }).collect::<Vec<_>>(),
                    "sentAt": l.sent_at,
                })
            }
        }
    }
}
