//! 설교 중 성경 구절 따라가기 · 방송 실시간 자막 (음성 인식 글을 받아서 쓰는 쪽)
//!
//! - 따라가기: 받아 적은 글에서 장절(refscan::find_refs_spoken)을 찾으면 '다음(미리보기)' 칸에만 올린다. 절대 송출하지 않는다
//!   ('바로 송출' 모드여도 미리보기만 바꾼다 — Core::preview_passage_quiet).
//! - 자막: 받아 적은 글을 오버레이 배너 하나로 (기본은 방송 출력 /outputs/overlay 에만, 예배당 화면에는 나가지 않음).
//!   기존 회중 메시지·오버레이 길(overlays_for)에 한 줄 얹은 것 — 출력 웹 코드(web/output)는 고치지 않는다.

use crate::refscan::{find_refs_spoken, Found};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde_json::{json, Value};

/// 같은 장절을 다시 말해도 이 시간 안이면 다시 올리지 않는다
pub const REPEAT_MS: i64 = 90_000;

#[derive(Default)]
pub struct Follower {
    recent: Vec<(String, i64)>,
    /// 앞 조각의 끝 (책 이름과 장 번호가 두 조각으로 갈린 경우: '…요한복음' | '15장 5절')
    carry: String,
}

impl Follower {
    /// 새로 받아 적은 글 → 새로 찾은 장절 (앞 조각 끝과 이어 본다)
    pub fn feed(&mut self, text: &str, now: i64) -> Vec<Found> {
        let joined = format!("{} {}", self.carry, text);
        let skip = self.carry.len() + 1;
        let found = find_refs_spoken(&joined);
        self.recent.retain(|(_, t)| now - *t < REPEAT_MS);
        let mut out = vec![];
        for f in found {
            // 앞 조각 안에서 이미 끝난 것은 지난번에 봤다
            if f.end <= skip {
                continue;
            }
            if self.recent.iter().any(|(r, _)| *r == f.reference) {
                continue;
            }
            self.recent.push((f.reference.clone(), now));
            out.push(f);
        }
        let chars: Vec<char> = text.chars().collect();
        self.carry = chars[chars.len().saturating_sub(16)..].iter().collect();
        out
    }
}

// ---------------- 방송 자막 ----------------
#[derive(Clone, Debug)]
pub struct Caption {
    pub text: String,
    pub outputs: Vec<String>,
    pub size: f64,
    pub at: i64,
}

static CAPTION: Lazy<Mutex<Option<Caption>>> = Lazy::new(|| Mutex::new(None));

/// 자막 줄을 바꾼다 (빈 글이면 내림). outputs 기본 ["overlay"]
pub fn set_caption(text: &str, outputs: &[String], size: f64) {
    let t: String = text.trim().chars().take(240).collect();
    *CAPTION.lock() = if t.is_empty() { None } else { Some(Caption { text: t, outputs: outputs.to_vec(), size, at: crate::util::now_ms() }) };
}

pub fn caption() -> Option<Caption> {
    CAPTION.lock().clone()
}

/// 자막 줄은 마지막으로 바뀐 뒤 이 시간이 지나면 저절로 내린다 (말이 멈췄을 때 남지 않게)
pub const CAPTION_HOLD_MS: i64 = 8_000;

/// 실시간 자막 오버레이 id — 출력 화면 구성에서는 'caption' 소스 칸에 그린다 (로드맵 소스, AI 5번)
pub const CAPTION_ID: &str = "wpcaption";

/// 출력 화면(main · overlay)에 얹을 자막 배너
pub fn caption_overlay(output: &str) -> Vec<Value> {
    caption_overlay_if(|outs| outs.iter().any(|o| o == output))
}

/// 화면 구성에 'caption' 칸을 직접 넣은 출력: 자막을 어느 출력에 띄웠든 그 칸에 (띄운 동안만)
pub fn caption_overlay_any() -> Vec<Value> {
    caption_overlay_if(|outs| !outs.is_empty())
}

fn caption_overlay_if(want: impl Fn(&[String]) -> bool) -> Vec<Value> {
    let Some(c) = caption() else { return vec![] };
    if !want(&c.outputs) || crate::util::now_ms() - c.at > CAPTION_HOLD_MS {
        return vec![];
    }
    crate::overlays::normalize_overlay(&json!({ "id": CAPTION_ID, "type": "banner", "on": true, "name": "실시간 자막", "text": c.text, "outputs": c.outputs,
        "position": "bottom-center", "full": true, "size": if c.size > 0.0 { c.size } else { 40.0 }, "color": "#ffffff", "bg": "rgba(0,0,0,0.72)", "margin": 0 }))
    .map(|x| vec![x])
    .unwrap_or_default()
}

/// 자막 설정 (settings.stt.captions): 기본 꺼짐 · 방송 출력에만
pub fn caption_settings(s: &Value) -> (bool, Vec<String>, bool, f64) {
    let c = &s["stt"]["captions"];
    let outs: Vec<String> = c["outputs"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().filter(|o| *o == "main" || *o == "overlay").map(String::from)).collect()).unwrap_or_else(|| vec!["overlay".into()]);
    (c["on"].as_bool().unwrap_or(false), outs, c["translate"].as_bool().unwrap_or(false), c["size"].as_f64().unwrap_or(40.0))
}

/// 번역 프롬프트 (한 줄씩, 영어)
pub fn translate_system() -> String {
    "You translate live Korean church sermon captions into natural English for a livestream. Output only the English translation of the given line, no quotes or notes. Keep Bible references in English form (e.g. John 15:5).".into()
}

/// 자막 두 줄 (원문 + 번역) 또는 번역만
pub fn caption_text(ko: &str, en: Option<&str>, only_en: bool) -> String {
    match en.filter(|e| !e.trim().is_empty()) {
        Some(e) if only_en => e.trim().to_string(),
        Some(e) => format!("{}\n{}", ko.trim(), e.trim()),
        None => ko.trim().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn follower_dedupes_and_joins_split_chunks() {
        let mut f = Follower::default();
        let a = f.feed("오늘 본문은 요한복음", 0);
        assert!(a.is_empty());
        let b = f.feed("15장 5절 말씀입니다", 1000);
        assert_eq!(b.iter().map(|x| x.reference.as_str()).collect::<Vec<_>>(), vec!["요한복음 15:5"]);
        assert!(f.feed("다시 요한복음 15장 5절을 보면", 5000).is_empty(), "90초 안 같은 구절은 한 번만");
        assert_eq!(f.feed("요한복음 15장 5절", 5000 + REPEAT_MS).len(), 1, "시간이 지나면 다시");
        assert_eq!(f.feed("로마서 팔 장 이십팔 절", 200_000)[0].reference, "로마서 8:28");
    }
    #[test]
    fn caption_goes_to_overlay_only_by_default() {
        let s = json!({});
        let (on, outs, tr, _) = caption_settings(&s);
        assert!(!on && !tr);
        assert_eq!(outs, vec!["overlay".to_string()]);
        set_caption("하나님은 사랑이십니다", &outs, 0.0);
        assert_eq!(caption_overlay("overlay").len(), 1);
        assert!(caption_overlay("main").is_empty(), "예배당 화면에는 나가지 않음");
        assert_eq!(caption_overlay("overlay")[0]["text"], "하나님은 사랑이십니다");
        set_caption("", &outs, 0.0);
        assert!(caption_overlay("overlay").is_empty());
        assert_eq!(caption_text("가", Some("A"), false), "가\nA");
        assert_eq!(caption_text("가", Some("A"), true), "A");
        assert_eq!(caption_text("가", None, true), "가");
    }
}
