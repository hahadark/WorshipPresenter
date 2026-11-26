//! [트랙 C] 송출 흐름: 자동 넘김(auto) · 글자 자동 숨김(hideAfter) · 처음/끝 한 번 더 숨김(edgeHide·cueEdgeHide)
//! · 연속 탐색(navMode continuous) · 미리보기 앞뒤 절 · 병행 번역(parallelVersionId → text2)
//! Electron core.js scheduleAuto · scheduleHide · slideStep · 'edge' 처리 · parallelVerses 를 옮김.

use crate::core::{Core, Ev, Shared};
use crate::parser::Range;
use crate::presentation::{Deck, Move};
use crate::slides;
use crate::util::now_ms;
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Default, Debug, Clone)]
pub struct Flow {
    /// 마지막으로 본 LIVE 판 (바뀌면 타이머를 다시 잡는다)
    seen: u64,
    auto_due: Option<i64>,
    hide_due: Option<i64>,
    /// 자동으로 글자를 숨긴 판 — 그다음 F5·F6 은 넘기면서 다시 보인다
    pub auto_hid_at: Option<u64>,
}

/// 0.2초마다 타이머 확인 (main 에서 한 번 부른다)
pub fn install(core: &Shared) {
    let c = core.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(200));
        tick(&mut c.lock(), now_ms());
    });
}

/// 초를 정한 회중 메시지가 만료되면 지우고 출력에 새 render 를 보낸다 (Electron core.js showMessage 의 setTimeout → message=null; emit('live')).
/// 출력 웹은 until 을 보지 않으므로 여기서 알려야 띠가 사라진다.
pub fn expire_message(c: &mut Core, now: i64) {
    if c.b.message.as_ref().is_some_and(|m| m.until > 0 && now > m.until) {
        c.b.message = None;
        c.emit(Ev::Live);
        c.emit(Ev::Ui);
    }
}

/// 이름 자막: 정한 시간이 지나면 저절로 내린다
pub fn expire_name_tag(c: &mut Core, now: i64) {
    let due = c.name_tag_on().and_then(|n| n["hideAt"].as_i64()).filter(|t| *t > 0 && now >= *t);
    if due.is_some() {
        c.hide_name_tag();
    }
}

fn live_visible(c: &Core) -> bool {
    c.pres.live.as_ref().is_some_and(|l| l.visible)
}

/// 타이머 확인 (시험하려고 now 를 받는다)
pub fn tick(c: &mut Core, now: i64) {
    expire_message(c, now);
    expire_name_tag(c, now);
    if c.finish_video_fade(now) { return; }
    let ver = c.pres.version;
    if ver != c.flow.seen {
        c.flow.seen = ver;
        c.flow.auto_due = None;
        c.flow.hide_due = None;
        if let Some(l) = c.pres.live.as_ref().filter(|l| l.visible) {
            let p = &l.deck.passage;
            if let Some(sec) = p["auto"]["sec"].as_f64().filter(|s| *s >= 1.0) {
                c.flow.auto_due = Some(now + (sec * 1000.0) as i64);
            }
            if let Some(sec) = p["hideAfter"].as_f64().filter(|s| *s >= 1.0) {
                c.flow.hide_due = Some(now + (sec * 1000.0) as i64);
            }
        }
    }
    if c.flow.hide_due.is_some_and(|d| now >= d) {
        c.flow.hide_due = None;
        c.flow.auto_due = None;
        if live_visible(c) {
            c.pres.clear();
            c.flow.auto_hid_at = Some(c.pres.version);
            c.flow.seen = c.pres.version;
            c.emit(Ev::Live);
            c.emit(Ev::Ui);
        }
        return;
    }
    if c.flow.auto_due.is_some_and(|d| now >= d) {
        c.flow.auto_due = None;
        if !live_visible(c) {
            return;
        }
        let Some((idx, n, lp)) = c.pres.live.as_ref().map(|l| (l.deck.index, l.deck.slides.len(), l.deck.passage["auto"]["loop"].as_bool().unwrap_or(false))) else { return };
        if idx + 1 < n {
            c.step(1);
        } else if lp {
            c.goto_live(0);
        } else {
            c.step(1); // 끝: 큐시트 순서면 다음 순서로
        }
    }
}

/// F5·F6 (Core::step 이 부른다)
pub fn step(c: &mut Core, d: i64) {
    // 자동 숨김 뒤: 넘기면서 다시 보인다
    let auto_hid = c.pres.live.as_ref().is_some_and(|l| !l.visible) && c.flow.auto_hid_at == Some(c.pres.version);
    if auto_hid {
        c.flow.auto_hid_at = None;
    }
    match c.pres.move_by(d) {
        Move::Ok => {
            if auto_hid {
                c.pres.restore();
            }
            c.sync_video_state(); // [트랙 B]
            c.emit(Ev::Live);
        }
        Move::NoLive => {}
        Move::Edge(edge) => on_edge(c, edge),
    }
    c.emit(Ev::Ui);
}

fn on_edge(c: &mut Core, edge: &str) {
    let s = c.settings.get().clone();
    let Some(l) = c.pres.live.as_ref() else { return };
    let p = l.deck.passage.clone();
    let in_cue = p["cueId"].as_str().is_some_and(|x| !x.is_empty());
    // 처음·끝에서 한 번 더: 글자만 숨기고 멈춘다
    if l.visible && in_cue && (p["edgeHide"].as_bool().unwrap_or(false) || s["cueEdgeHide"].as_bool() == Some(true)) {
        c.pres.clear();
        c.emit(Ev::Live);
        return;
    }
    // 큐시트 순서면 다음(이전) 순서로
    let ci = p["cueIndex"].as_i64().unwrap_or(-1);
    if in_cue && ci >= 0 && s["cueAdvance"].as_bool().unwrap_or(true) {
        if let Err(e) = crate::trigger::advance_live_cue(c, if edge == "end" { 1 } else { -1 }) {
            c.notify(e, "warn");
        }
        return;
    }
    if s["navMode"] == "continuous" {
        extend_live(c, if edge == "end" { 1 } else { -1 });
    }
}

/// 성경 본문의 한쪽 끝 절 (dir>0 이면 마지막 슬라이드의 마지막 절)
fn edge_verse(deck: &Deck, dir: i64) -> Option<(i64, i64, i64)> {
    let s = if dir > 0 { deck.slides.last()? } else { deck.slides.first()? };
    let parts = s["parts"].as_array()?;
    let p = if dir > 0 { parts.last()? } else { parts.first()? };
    let b = p["book"].as_i64().filter(|b| *b > 0)?;
    Some((b, p["chapter"].as_i64()?, p["verse"].as_i64()?))
}

/// 한 절 → 슬라이드 (병행 번역 포함)
fn verse_slides(c: &mut Core, version: &str, book: i64, ch: i64, v: i64) -> Option<(Value, Vec<Value>)> {
    let r = c.store.resolve_ranges(version, book, &[Range { sc: ch, sv: Some(v), ec: ch, ev: Some(v) }]);
    if r["ok"] != json!(true) {
        return None;
    }
    let pass = with_parallel(c, r["passage"].clone());
    let opts = c.split_opts(None);
    let sl = slides::build_bible(&pass, &opts);
    Some((pass, sl))
}

/// 연속 탐색: LIVE 범위 끝(처음)에 다음(이전) 절을 붙이고 그리로 (책 경계에서 멈춤)
pub fn extend_live(c: &mut Core, dir: i64) -> bool {
    let Some(l) = c.pres.live.as_ref() else { return false };
    let version = l.deck.passage["versionId"].as_str().unwrap_or("").to_string();
    let Some((b, ch, v)) = edge_verse(&l.deck, dir) else { return false };
    let Some((nc, nv)) = c.store.adjacent_verse(&version, b, ch, v, dir) else {
        c.notify(if dir > 0 { "책의 마지막 절입니다." } else { "책의 첫 절입니다." }, "warn");
        return false;
    };
    let Some((pass, sl)) = verse_slides(c, &version, b, nc, nv) else { return false };
    if sl.is_empty() {
        return false;
    }
    let l = c.pres.live.as_mut().unwrap();
    let k = sl.len();
    if dir > 0 {
        l.deck.slides.extend(sl);
        l.deck.index = l.deck.slides.len() - k;
    } else {
        let mut v2 = sl;
        v2.extend(std::mem::take(&mut l.deck.slides));
        l.deck.slides = v2;
        l.deck.index = k - 1;
    }
    l.deck.passage["title"] = pass["title"].clone();
    c.pres.bump();
    c.emit(Ev::Live);
    c.emit(Ev::Ui);
    true
}

/// 미리보기: 지금 보는 슬라이드의 앞(뒤) 절로 (Alt+←/→)
pub fn preview_adjacent_verse(c: &mut Core, dir: i64) {
    let Some(p) = c.pres.preview.as_ref() else { return };
    let version = p.passage["versionId"].as_str().filter(|x| !x.is_empty()).map(String::from).unwrap_or_else(|| c.version_id());
    let Some(slide) = p.slides.get(p.index) else { return };
    let parts = slide["parts"].as_array().cloned().unwrap_or_default();
    let part = if dir > 0 { parts.last() } else { parts.first() };
    let Some((b, ch, v)) = part.and_then(|x| Some((x["book"].as_i64().filter(|b| *b > 0)?, x["chapter"].as_i64()?, x["verse"].as_i64()?))) else { return };
    let Some((nc, nv)) = c.store.adjacent_verse(&version, b, ch, v, dir) else {
        c.notify(if dir > 0 { "책의 마지막 절입니다." } else { "책의 첫 절입니다." }, "warn");
        return;
    };
    let r = c.store.resolve_ranges(&version, b, &[Range { sc: nc, sv: Some(nv), ec: nc, ev: Some(nv) }]);
    if r["ok"] == json!(true) {
        let q = r["passage"]["title"].as_str().unwrap_or("").to_string();
        c.preview_passage(&r["passage"].clone(), &q, None);
    }
}

/// 미리보기: 지금 절이 든 장 전체, 그 장 처음부터 (Ctrl+Home)
pub fn preview_chapter_start(c: &mut Core) {
    let Some(p) = c.pres.preview.as_ref() else { return };
    let version = p.passage["versionId"].as_str().filter(|x| !x.is_empty()).map(String::from).unwrap_or_else(|| c.version_id());
    let Some(part) = p.slides.get(p.index).and_then(|s| s["parts"].as_array()?.first().cloned()) else { return };
    let (Some(b), Some(ch)) = (part["book"].as_i64().filter(|b| *b > 0), part["chapter"].as_i64()) else { return };
    let r = c.store.get_chapter(&version, b, ch);
    if r["ok"] == json!(true) {
        let q = r["passage"]["title"].as_str().unwrap_or("").to_string();
        c.preview_passage(&r["passage"].clone(), &q, Some((ch, 1)));
    }
}

/// 병행 번역: 설정 parallelVersionId 가 있으면 같은 범위의 절을 text2 로 붙인다
pub fn with_parallel(c: &mut Core, mut passage: Value) -> Value {
    let pv = c.settings.str(&["parallelVersionId"]);
    let cur = passage["versionId"].as_str().unwrap_or("").to_string();
    if pv.is_empty() || pv == cur || !c.store.has_version(&pv) {
        return passage;
    }
    let Some(book) = passage["book"]["id"].as_i64() else { return passage };
    let ranges: Vec<Range> = passage["ranges"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| Some(Range { sc: r["sc"].as_i64()?, sv: r["sv"].as_i64(), ec: r["ec"].as_i64()?, ev: r["ev"].as_i64() }))
        .collect();
    if ranges.is_empty() {
        return passage;
    }
    let r = c.store.resolve_ranges(&pv, book, &ranges);
    if r["ok"] != json!(true) {
        return passage;
    }
    let map: HashMap<i64, String> = r["passage"]["verses"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|v| (v["chapter"].as_i64().unwrap_or(0) * 1000 + v["verse"].as_i64().unwrap_or(0), v["text"].as_str().unwrap_or("").to_string()))
        .collect();
    if let Some(vs) = passage["verses"].as_array_mut() {
        for v in vs {
            let k = v["chapter"].as_i64().unwrap_or(0) * 1000 + v["verse"].as_i64().unwrap_or(0);
            if let Some(t) = map.get(&k) {
                v["text2"] = json!(t);
            }
        }
    }
    let pn = r["passage"]["versionName"].as_str().unwrap_or("").to_string();
    passage["versionName"] = json!(format!("{} · {}", passage["versionName"].as_str().unwrap_or(""), pn));
    passage
}

/// 큐시트 순서의 자동 넘김 · 글자 숨김 · 처음/끝 숨김을 송출 정보에 (fill_item_meta 가 부른다)
pub fn item_meta(p: &mut Value, it: &Value) {
    let h = it["hideAfter"].as_f64().unwrap_or(0.0);
    p["hideAfter"] = json!(if h >= 1.0 { h.round().min(3600.0) } else { 0.0 });
    p["edgeHide"] = json!(it["edgeHide"].as_bool().unwrap_or(false));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn meta() {
        let mut p = json!({});
        item_meta(&mut p, &json!({ "hideAfter": 4.4, "edgeHide": true }));
        assert_eq!(p["hideAfter"], json!(4.0));
        assert_eq!(p["edgeHide"], json!(true));
        item_meta(&mut p, &json!({ "hideAfter": 0.3 }));
        assert_eq!(p["hideAfter"], json!(0.0));
    }
}
