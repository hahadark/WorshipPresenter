//! 예배 후 자동 정리 — 그날의 사용 기록(usage.rs, 송출 시각)과 받아 적은 글(stt 의 transcript)로 글을 만든다
//!  - 순서별 시간 표시 (유튜브 챕터 꼴 '0:00 예배로 부름') — 영상 시작 시각을 기준으로
//!  - 유튜브 설명 글 (예배 이름 · 본문 · 설교 · 찬양 목록과 저작권 표시 · 챕터)
//!  - 설교 요약 재료 (설교 제목 · 본문 · 설교 포인트 · 받아 적은 설교 글)
//!  - 쇼츠 후보 (시간 구간 목록만: 설교 중 성경 구절 · '첫째/둘째' 대지가 나온 곳)
//!  - 녹음 파일 요약: 녹음을 PC 안에서 글로 바꾼 뒤(stt) 그 '글' 만 AI 로 (키 · 동의 필요)

use crate::aihelp::{self, AiCfg};
use crate::usage::Row;
use serde_json::Value;

/// 한 줄 기록 (지역 시각 초, 종류, 제목, 저작권 표시)
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub sec: i64,
    pub kind: String,
    pub title: String,
    pub credit: String,
}

/// 받아 적은 한 줄 (지역 시각 초, 글)
#[derive(Debug, Clone, PartialEq)]
pub struct Said {
    pub sec: i64,
    pub text: String,
}

pub fn hms(sec: i64) -> String {
    format!("{:02}:{:02}:{:02}", sec / 3600, sec % 3600 / 60, sec % 60)
}

/// 영상 안 시각 (1:02:03 · 12:34 · 0:05)
pub fn yt(sec: i64) -> String {
    let s = sec.max(0);
    if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, s % 3600 / 60, s % 60) } else { format!("{}:{:02}", s / 60, s % 60) }
}

/// 'HH:MM' 또는 'HH:MM:SS' → 그날 초
pub fn parse_clock(s: &str) -> Option<i64> {
    let p: Vec<i64> = s.trim().split(':').map(|x| x.trim().parse().ok()).collect::<Option<Vec<_>>>()?;
    match p.as_slice() {
        [h, m] => Some(h * 3600 + m * 60),
        [h, m, s] => Some(h * 3600 + m * 60 + s),
        _ => None,
    }
}

/// 사용 기록(UTC ISO) → 그날 지역 시각 초 차례
pub fn events_of(rows: &[Row], offset_ms: i64) -> Vec<Event> {
    let mut v: Vec<Event> = rows
        .iter()
        .filter_map(|r| {
            let ms = crate::usage::iso_ms(&r.at)? + offset_ms;
            Some(Event { sec: ms.rem_euclid(86_400_000) / 1000, kind: r.kind.clone(), title: r.title.clone(), credit: r.credit.clone() })
        })
        .collect();
    v.sort_by_key(|e| e.sec);
    v
}

/// transcript 파일 ('HH:MM:SS<TAB>글') 읽기
pub fn parse_transcript(t: &str) -> Vec<Said> {
    t.lines()
        .filter_map(|l| {
            let (a, b) = l.split_once('\t')?;
            Some(Said { sec: parse_clock(a)?, text: b.trim().to_string() })
        })
        .filter(|s| !s.text.is_empty())
        .collect()
}

fn kind_ko(k: &str) -> &str {
    crate::usage::kind_name(k)
}

/// 순서별 시간 표시 (유튜브 챕터): start = 영상이 시작한 그날 초. 첫 줄은 늘 0:00, 10초 안에 바뀐 것은 합친다
pub fn chapters(events: &[Event], start: i64) -> String {
    let mut out: Vec<(i64, String)> = vec![];
    for e in events.iter().filter(|e| e.sec >= start - 60) {
        let at = (e.sec - start).max(0);
        let name = if e.kind == "bible" { format!("성경 {}", e.title) } else { e.title.clone() };
        if let Some(last) = out.last_mut() {
            if at - last.0 < 10 {
                last.1 = name;
                continue;
            }
            if last.1 == name {
                continue;
            }
        }
        out.push((at, name));
    }
    if let Some(f) = out.first_mut() {
        f.0 = 0;
    }
    out.iter().map(|(t, n)| format!("{} {}", yt(*t), n)).collect::<Vec<_>>().join("\n")
}

/// 설교 구간 (처음 '설교' 가 든 제목 순서부터 다음 순서까지) — 없으면 None
pub fn sermon_span(events: &[Event]) -> Option<(i64, i64, String)> {
    let i = events.iter().position(|e| e.title.contains("설교") || e.title.contains("말씀"))?;
    let end = events.get(i + 1).map(|e| e.sec).unwrap_or(events[i].sec + 40 * 60);
    Some((events[i].sec, end, events[i].title.clone()))
}

/// 유튜브 설명 글
pub fn youtube_description(cue_name: &str, events: &[Event], start: i64) -> String {
    let mut s = String::new();
    s.push_str(cue_name.trim());
    s.push_str("\n\n");
    let bibles: Vec<&str> = {
        let mut v: Vec<&str> = vec![];
        for e in events.iter().filter(|e| e.kind == "bible") {
            if !v.contains(&e.title.as_str()) {
                v.push(&e.title);
            }
        }
        v
    };
    if !bibles.is_empty() {
        s.push_str(&format!("본문: {}\n", bibles.join(", ")));
    }
    if let Some((_, _, t)) = sermon_span(events) {
        s.push_str(&format!("{t}\n"));
    }
    let songs: Vec<&Event> = {
        let mut v: Vec<&Event> = vec![];
        for e in events.iter().filter(|e| matches!(e.kind.as_str(), "song" | "hymn")) {
            if !v.iter().any(|x| x.title == e.title) {
                v.push(e);
            }
        }
        v
    };
    if !songs.is_empty() {
        s.push_str("\n찬양\n");
        for e in songs {
            if e.credit.trim().is_empty() {
                s.push_str(&format!("· {}\n", e.title));
            } else {
                s.push_str(&format!("· {} ({})\n", e.title, e.credit.trim()));
            }
        }
    }
    let ch = chapters(events, start);
    if !ch.is_empty() {
        s.push_str("\n순서\n");
        s.push_str(&ch);
        s.push('\n');
    }
    s
}

/// 설교 요약 재료 (사람이나 AI 가 요약할 글)
pub fn sermon_material(cue_items: &[Value], events: &[Event], said: &[Said]) -> String {
    let mut s = String::new();
    let span = sermon_span(events);
    if let Some((_, _, t)) = &span {
        s.push_str(&format!("설교: {t}\n"));
    }
    for e in events.iter().filter(|e| e.kind == "bible") {
        s.push_str(&format!("본문: {}\n", e.title));
    }
    // 설교 포인트(제목 순서의 추가 슬라이드)
    for it in cue_items.iter().filter(|it| it["type"] == "title" && it["note"].as_str().unwrap_or("").contains("설교 포인트")) {
        s.push_str(&format!("\n설교 포인트 — {}\n{}\n", it["title"].as_str().unwrap_or(""), it["more"].as_str().unwrap_or("").replace("\n---\n", "\n")));
    }
    let words: Vec<&Said> = match &span {
        Some((a, b, _)) => said.iter().filter(|x| x.sec >= *a && x.sec < *b).collect(),
        None => said.iter().collect(),
    };
    if !words.is_empty() {
        s.push_str("\n받아 적은 설교 (음성 인식, 틀린 곳이 있을 수 있음)\n");
        for w in words {
            s.push_str(&format!("[{}] {}\n", hms(w.sec), w.text));
        }
    }
    s
}

/// 쇼츠 후보 (시간 구간만): 설교 중 성경 구절 · 대지 표시가 나온 곳 앞 5초 ~ 뒤 55초. 겹치면 합친다
pub fn shorts(events: &[Event], said: &[Said], start: i64) -> Vec<(i64, i64, String)> {
    let span = sermon_span(events);
    let in_sermon = |sec: i64| span.as_ref().is_none_or(|(a, b, _)| sec >= *a && sec < *b);
    let mut v: Vec<(i64, i64, String)> = vec![];
    for w in said.iter().filter(|w| in_sermon(w.sec)) {
        let refs = crate::refscan::find_refs_spoken(&w.text);
        let point = ["첫째", "둘째", "셋째", "넷째", "마지막으로"].iter().find(|p| w.text.contains(*p));
        let why = if let Some(r) = refs.first() { format!("성경 {}", r.reference) } else if let Some(p) = point { format!("대지 '{p}'") } else { continue };
        let (a, b) = ((w.sec - 5 - start).max(0), w.sec + 55 - start);
        match v.last_mut() {
            Some(last) if a <= last.1 => {
                last.1 = last.1.max(b);
                if !last.2.contains(&why) {
                    last.2 = format!("{} · {why}", last.2);
                }
            }
            _ => v.push((a, b, why)),
        }
    }
    if v.is_empty() {
        if let Some((a, _, t)) = span {
            v.push(((a - start).max(0), a - start + 60, format!("{t} 시작")));
        }
    }
    v
}

pub fn shorts_text(v: &[(i64, i64, String)]) -> String {
    v.iter().map(|(a, b, w)| format!("{} ~ {}  {}", yt(*a), yt(*b), w)).collect::<Vec<_>>().join("\n")
}

/// 녹음 파일을 글로 바꾼 결과(시각 붙은 줄) → AI 요약 (글만 보냄)
pub fn summary_system() -> String {
    "너는 한국 교회 방송실 도우미다. 받아 적은 설교 글(음성 인식이라 틀린 낱말이 있다)을 읽고 교회 홈페이지·유튜브에 올릴 요약을 쓴다.\n\
     - 3~5문장 요약, 대지 3개 안팎(한 줄씩), 기억할 한 문장.\n\
     - 글에 없는 내용은 지어내지 않는다. 성경 장절은 글에 나온 것만.\n\
     JSON 하나만: {\"summary\":\"요약\",\"points\":[\"대지\"],\"quote\":\"기억할 한 문장\"}"
        .into()
}

pub fn summary_text(v: &Value) -> String {
    let mut s = v["summary"].as_str().unwrap_or("").trim().to_string();
    let pts: Vec<&str> = v["points"].as_array().into_iter().flatten().filter_map(|x| x.as_str()).collect();
    if !pts.is_empty() {
        s.push_str("\n\n");
        for (i, p) in pts.iter().enumerate() {
            s.push_str(&format!("{}. {}\n", i + 1, p.trim()));
        }
    }
    if let Some(q) = v["quote"].as_str().filter(|q| !q.trim().is_empty()) {
        s.push_str(&format!("\n“{}”", q.trim()));
    }
    s.trim().to_string()
}

pub fn ask_summary(cfg: &AiCfg, transcript: &str, title: &str) -> Result<(String, Value), String> {
    if transcript.trim().is_empty() {
        return Err(crate::i18n::tr("받아 적은 글이 없습니다.").into());
    }
    let user = format!("설교 제목: {title}\n받아 적은 설교:\n{}", transcript.chars().take(50000).collect::<String>());
    let (v, u) = aihelp::chat_json(cfg, &summary_system(), &user, 1500)?;
    Ok((summary_text(&v), u))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn ev(sec: i64, kind: &str, title: &str, credit: &str) -> Event {
        Event { sec, kind: kind.into(), title: title.into(), credit: credit.into() }
    }
    fn day() -> Vec<Event> {
        let h = 11 * 3600;
        vec![
            ev(h, "title", "예배로 부름", ""),
            ev(h + 3, "title", "예배로 부름 2", ""),
            ev(h + 120, "song", "주 은혜임을", "작사 홍길동 · CCLI 123"),
            ev(h + 600, "bible", "요한복음 15:1-8", ""),
            ev(h + 700, "title", "설교 — 포도나무와 가지", ""),
            ev(h + 2500, "hymn", "새찬송가 8장", ""),
        ]
    }
    #[test]
    fn chapters_from_start() {
        let c = chapters(&day(), 11 * 3600 - 30);
        assert_eq!(c.lines().next(), Some("0:00 예배로 부름 2"), "10초 안에 바뀐 것은 합침: {c}");
        assert!(c.contains("2:30 주 은혜임을") && c.contains("10:30 성경 요한복음 15:1-8") && c.contains("42:10 새찬송가 8장"), "{c}");
        assert_eq!(parse_clock("10:59:30"), Some(10 * 3600 + 59 * 60 + 30));
        assert_eq!(yt(3725), "1:02:05");
    }
    #[test]
    fn description_and_material() {
        let d = youtube_description("2026-11-29 주일 예배", &day(), 11 * 3600);
        assert!(d.starts_with("2026-11-29 주일 예배\n\n본문: 요한복음 15:1-8\n설교 — 포도나무와 가지\n"), "{d}");
        assert!(d.contains("· 주 은혜임을 (작사 홍길동 · CCLI 123)") && d.contains("\n순서\n0:00"), "{d}");
        let said = parse_transcript("11:12:00\t오늘 요한복음 15장 5절을 봅니다\n11:20:00\t첫째, 붙어 있어야 삽니다\n10:00:00\t예배 전 소리\n잘못된 줄");
        assert_eq!(said.len(), 3);
        let items = vec![json!({"type":"title","title":"포도나무","note":"설교 포인트 4장","more":"첫째, 붙어 있어야\n---\n둘째, 열매"})];
        let m = sermon_material(&items, &day(), &said);
        assert!(m.contains("설교 포인트 — 포도나무\n첫째, 붙어 있어야\n둘째, 열매") && m.contains("[11:12:00] 오늘 요한복음") && !m.contains("예배 전 소리"), "{m}");
        let sh = shorts(&day(), &said, 11 * 3600);
        assert_eq!(sh.len(), 2);
        assert_eq!(sh[0].0, 715);
        assert!(sh[0].2.contains("요한복음 15:5") && sh[1].2.contains("첫째"));
        assert!(shorts_text(&sh).starts_with("11:55 ~ 12:55"));
        // 받아 적은 글이 없으면 설교 시작 구간 하나
        assert_eq!(shorts(&day(), &[], 11 * 3600).len(), 1);
    }
    #[test]
    fn ai_summary_mock() {
        let _g = crate::aihelp::TEST_LOCK.lock();
        crate::aihelp::set_mock(Some(Box::new(|b| {
            assert!(b["messages"][1]["content"].as_str().unwrap().contains("포도나무"));
            Ok(json!({"choices":[{"message":{"content":"{\"summary\":\"붙어 있으면 삽니다.\",\"points\":[\"붙어 있기\",\"열매\"],\"quote\":\"주 안에 거하라\"}"}}]}))
        })));
        let r = ask_summary(&AiCfg::default(), "[11:00:00] 포도나무", "포도나무");
        crate::aihelp::set_mock(None);
        assert_eq!(r.unwrap().0, "붙어 있으면 삽니다.\n\n1. 붙어 있기\n2. 열매\n\n“주 안에 거하라”");
    }
}
