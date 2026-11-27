//! 가사 자동 정리 — 붙여 넣은 가사를 절 · 후렴 · 브리지로 나누고 줄 바꿈 · 띄어쓰기를 다듬는다
//!
//! 규칙만으로 (AI 없이):
//!  1) 군더더기 줄 빼기: 코드(G  D/F#  Em) · 작사/작곡/Copyright/CCLI · '(x2)' 표시
//!  2) 이름표 맞추기: 'Verse 1' · 'V1' · '1.' · '1절:' → [1절], 'Chorus' · 'C' · '후렴:' → [후렴], 'Bridge' · '브릿지' → [브리지] …
//!  3) 띄어쓰기: 여러 칸 → 한 칸, 문장부호 앞 칸 빼기, 줄 끝 마침표·쉼표 빼기
//!  4) 긴 줄(18자 넘음) 은 가운데 가까운 띄어쓰기에서 둘로
//!  5) 이름표가 없으면 lyricsweb::arrange(되풀이 묶음 = 후렴) 로 나누고, 마지막 후렴 앞에 한 번만 나오는 묶음은 [브리지]
//! AI (선택): 같은 일을 맡기고 결과를 같은 꼴(가사 + 부를 순서)로 받는다. 화면은 고치기 전후를 나란히 보여 주고 받아들일 때만 바꾼다.

use crate::aihelp::{self, AiCfg};
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::Value;

pub const MAX_LINE: usize = 18;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Cleaned {
    pub lyrics: String,
    pub order: String,
    /// 무엇을 고쳤는지 (화면에 짧게)
    pub notes: Vec<String>,
}

fn re(s: &str) -> Regex {
    Regex::new(s).unwrap()
}

/// 코드 줄: 'G  D/F#  Em7' 처럼 코드 이름만
fn chord_line(l: &str) -> bool {
    static C: Lazy<Regex> = Lazy::new(|| re(r"^(?:[A-G](?:#|b)?(?:m|maj|min|sus|dim|aug|add)?\d*(?:/[A-G](?:#|b)?)?\s*)+$"));
    let t = l.trim();
    !t.is_empty() && C.is_match(t)
}

/// 이름표 줄 → 맞춘 이름 ('1절' · '후렴' · '브리지' …)
pub fn label_of(l: &str) -> Option<String> {
    let t = l.trim().trim_matches(['[', ']', '(', ')', ':', '：']).trim();
    let low = t.to_lowercase();
    static VERSE: Lazy<Regex> = Lazy::new(|| re(r"^(?:verse|v|절)\s*\.?\s*(\d)$"));
    static VERSE_KO: Lazy<Regex> = Lazy::new(|| re(r"^(\d)\s*(?:절|\.)$"));
    static CHORUS: Lazy<Regex> = Lazy::new(|| re(r"^(?:chorus|c|후렴|코러스|refrain)\s*(\d?)$"));
    if let Some(m) = VERSE.captures(&low).or_else(|| VERSE_KO.captures(&low)) {
        return Some(format!("{}절", &m[1]));
    }
    if let Some(m) = CHORUS.captures(&low) {
        return Some(if m[1].is_empty() || &m[1] == "1" { "후렴".into() } else { format!("후렴{}", &m[1]) });
    }
    match low.as_str() {
        "bridge" | "b" | "브릿지" | "브리지" => Some("브리지".into()),
        "pre-chorus" | "prechorus" | "pre chorus" | "pc" | "프리코러스" | "프리 코러스" => Some("프리코러스".into()),
        "ending" | "outro" | "엔딩" | "아웃트로" | "tag" | "태그" => Some("엔딩".into()),
        "intro" | "인트로" | "전주" | "간주" | "interlude" => Some("간주".into()),
        _ => None,
    }
}

/// 군더더기 줄인가 (작사·작곡·저작권·CCLI·'x2')
fn junk_line(l: &str) -> bool {
    static J: Lazy<Regex> = Lazy::new(|| re(r"(?i)^(?:작사|작곡|편곡|번역|가사|words|music|lyrics?)\s*[:：]|copyright|ⓒ|©|ccli|all rights reserved|^\(?\s*[x×]\s*\d\s*\)?$|^\(?\s*\d\s*[x×번]\s*\)?$"));
    static PAREN: Lazy<Regex> = Lazy::new(|| re(r"\((?:작사|작곡|편곡|words|music)[^)]*\)"));
    J.is_match(l.trim()) || PAREN.is_match(l)
}

/// 띄어쓰기 다듬기 (한 줄)
pub fn tidy_line(l: &str) -> String {
    static SP: Lazy<Regex> = Lazy::new(|| re(r"\s+"));
    static BEFORE: Lazy<Regex> = Lazy::new(|| re(r"\s+([,.!?~])"));
    static AFTER: Lazy<Regex> = Lazy::new(|| re(r"([,!?])([^\s,.!?~\d])"));
    static TAIL: Lazy<Regex> = Lazy::new(|| re(r"[\s,.]+$"));
    static REP: Lazy<Regex> = Lazy::new(|| re(r"\(\s*[x×]\s*\d\s*\)|\s[x×]\s*\d$"));
    let t = REP.replace_all(l, "");
    let t = SP.replace_all(t.trim(), " ");
    let t = BEFORE.replace_all(&t, "$1");
    let t = AFTER.replace_all(&t, "$1 $2");
    TAIL.replace(&t, "").to_string()
}

/// 긴 줄을 가운데 가까운 띄어쓰기에서 둘로 (띄어쓰기가 없으면 그대로)
pub fn split_long(l: &str, max: usize) -> Vec<String> {
    let n = l.chars().count();
    if n <= max {
        return vec![l.to_string()];
    }
    let mid = n / 2;
    let cs: Vec<char> = l.chars().collect();
    let best = cs.iter().enumerate().filter(|(_, c)| **c == ' ').map(|(i, _)| i).min_by_key(|i| i.abs_diff(mid));
    match best {
        Some(i) => {
            let a: String = cs[..i].iter().collect();
            let b: String = cs[i + 1..].iter().collect();
            let mut out = split_long(a.trim(), max);
            out.extend(split_long(b.trim(), max));
            out
        }
        None => vec![l.to_string()],
    }
}

/// 이름표 블록들에서 부를 순서와 중복 없는 가사 (같은 이름의 같은 글은 한 번만)
fn dedupe_labeled(blocks: Vec<(String, Vec<String>)>) -> (String, String, usize) {
    let key = |b: &Vec<String>| b.iter().map(|l| l.chars().filter(|c| c.is_alphanumeric()).collect::<String>()).collect::<Vec<_>>().join("|");
    let mut seen: Vec<(String, String)> = vec![];
    let mut out: Vec<String> = vec![];
    let mut order: Vec<String> = vec![];
    let mut dropped = 0;
    for (lb, lines) in blocks {
        let k = key(&lines);
        if let Some((name, _)) = seen.iter().find(|(n, kk)| *kk == k && (n == &lb || lines.is_empty())) {
            order.push(name.clone());
            dropped += 1;
            continue;
        }
        if lines.is_empty() {
            // 이름표만 있고 글이 없으면 앞의 같은 이름을 다시 부르는 것
            if seen.iter().any(|(n, _)| *n == lb) {
                order.push(lb);
            }
            continue;
        }
        seen.push((lb.clone(), k));
        order.push(lb.clone());
        out.push(format!("[{lb}]\n{}", lines.join("\n")));
    }
    let repeated = order.len() > seen.len();
    (out.join("\n\n"), if repeated { order.join(" ") } else { String::new() }, dropped)
}

/// 마지막 후렴 앞에 한 번만 나오는 절을 [브리지] 로 (절이 셋 이상 이름이 있고, 그 절이 후렴 둘 사이에 있을 때)
fn mark_bridge(lyrics: &str, order: &str) -> (String, String, bool) {
    let toks: Vec<&str> = order.split_whitespace().collect();
    if toks.len() < 5 {
        return (lyrics.into(), order.into(), false);
    }
    let last_ch = toks.iter().rposition(|t| t.starts_with("후렴"));
    let Some(lc) = last_ch else { return (lyrics.into(), order.into(), false) };
    if lc < 2 || !toks[lc - 1].ends_with('절') || !toks[..lc - 1].iter().any(|t| t.starts_with("후렴")) {
        return (lyrics.into(), order.into(), false);
    }
    let cand = toks[lc - 1];
    if toks.iter().filter(|t| **t == cand).count() != 1 || cand == "1절" {
        return (lyrics.into(), order.into(), false);
    }
    let ly = lyrics.replace(&format!("[{cand}]"), "[브리지]");
    let ord = toks.iter().map(|t| if *t == cand { "브리지" } else { t }).collect::<Vec<_>>().join(" ");
    (ly, ord, true)
}

/// 규칙으로 정리
pub fn clean(text: &str) -> Cleaned {
    let mut notes: Vec<String> = vec![];
    let src = text.replace('\r', "").replace('\u{a0}', " ").replace('\u{feff}', "");
    let (mut chords, mut junk, mut split, mut spaced) = (0, 0, 0, 0);
    let mut lines: Vec<String> = vec![];
    for raw in src.split('\n') {
        if chord_line(raw) {
            chords += 1;
            continue;
        }
        if junk_line(raw) {
            junk += 1;
            continue;
        }
        if raw.trim().is_empty() {
            lines.push(String::new());
            continue;
        }
        if let Some(lb) = label_of(raw) {
            lines.push(format!("[{lb}]"));
            continue;
        }
        // '1절: 가사…' 처럼 이름표와 글이 한 줄
        static INLINE: Lazy<Regex> = Lazy::new(|| re(r"^\s*\[?((?:verse|chorus|bridge|v|c)\s*\d?|\d\s*절|후렴|브릿지|브리지)\]?\s*[:：.]\s*(.+)$"));
        let (lb, body) = match INLINE.captures(&raw.to_lowercase()) {
            Some(m) if label_of(&m[1]).is_some() => {
                let start = raw.len() - m[2].len();
                (label_of(&m[1]), raw[start..].to_string())
            }
            _ => (None, raw.to_string()),
        };
        if let Some(l) = lb {
            lines.push(format!("[{l}]"));
        }
        let t = tidy_line(&body);
        if t != body.trim() {
            spaced += 1;
        }
        let parts: Vec<String> = split_long(&t, MAX_LINE).into_iter().map(|x| tidy_line(&x)).collect();
        if parts.len() > 1 {
            split += 1;
        }
        lines.extend(parts);
    }
    let has_labels = lines.iter().any(|l| l.starts_with('['));
    let (mut lyrics, mut order) = if has_labels {
        // 이름표대로 묶기
        let mut blocks: Vec<(String, Vec<String>)> = vec![];
        let mut cur: Option<(String, Vec<String>)> = None;
        let mut verse_n = 0;
        for l in &lines {
            if let Some(lb) = l.strip_prefix('[').and_then(|x| x.strip_suffix(']')) {
                if let Some(c) = cur.take() {
                    blocks.push(c);
                }
                cur = Some((lb.to_string(), vec![]));
            } else if !l.is_empty() {
                match cur.as_mut() {
                    Some(c) => c.1.push(l.clone()),
                    None => {
                        verse_n += 1;
                        cur = Some((format!("{verse_n}절"), vec![l.clone()]));
                    }
                }
            }
        }
        if let Some(c) = cur.take() {
            blocks.push(c);
        }
        let (ly, ord, dropped) = dedupe_labeled(blocks);
        if dropped > 0 {
            notes.push(format!("되풀이한 묶음 {dropped}개는 한 번만 두고 부를 순서로"));
        }
        (ly, ord)
    } else {
        let joined = lines.join("\n");
        let (ly, ord) = crate::lyricsweb::arrange(&joined, 2);
        notes.push("이름표가 없어 되풀이되는 묶음으로 절 · 후렴을 나눔".into());
        (ly, ord)
    };
    let (ly, ord, bridged) = mark_bridge(&lyrics, &order);
    if bridged {
        notes.push("마지막 후렴 앞 한 번만 나오는 묶음을 [브리지] 로".into());
        lyrics = ly;
        order = ord;
    }
    for (n, what) in [(chords, "코드 줄"), (junk, "작사·작곡·저작권 줄"), (split, "긴 줄 나눔"), (spaced, "띄어쓰기·문장부호 다듬음")] {
        if n > 0 {
            notes.push(format!("{what} {n}"));
        }
    }
    Cleaned { lyrics: lyrics.trim().to_string(), order, notes }
}

/// 고치기 전후 나란히 보기용: 줄마다 (전, 후, 같은가)
pub fn side_by_side(before: &str, after: &str) -> Vec<(String, String, bool)> {
    let a: Vec<&str> = before.lines().collect();
    let b: Vec<&str> = after.lines().collect();
    let n = a.len().max(b.len());
    (0..n)
        .map(|i| {
            let x = a.get(i).copied().unwrap_or("");
            let y = b.get(i).copied().unwrap_or("");
            (x.to_string(), y.to_string(), x.trim() == y.trim())
        })
        .collect()
}

// ---------------- AI ----------------
pub fn ai_system() -> String {
    format!(
        "너는 한국 교회 예배 화면에 띄울 찬양 가사를 정리한다. 가사 낱말은 바꾸지 말고(오타·띄어쓰기만 바로잡음) 구조만 정리한다.\n\
         - 절은 [1절] [2절], 후렴은 [후렴], 브리지는 [브리지], 프리코러스는 [프리코러스], 엔딩은 [엔딩] 이름표 줄로.\n\
         - 같은 후렴을 여러 번 쓰지 말고 한 번만 두고, 부르는 차례는 order 에 '1절 후렴 2절 후렴 브리지 후렴' 처럼.\n\
         - 한 줄은 {MAX_LINE}자 안팎, 한 묶음은 2~4줄. 코드·작사·작곡·저작권 줄은 뺀다.\n\
         JSON 하나만: {{\"lyrics\":\"정리한 가사\",\"order\":\"부를 순서\",\"notes\":[\"고친 것 짧게\"]}}"
    )
}

pub fn from_ai(v: &Value) -> Result<Cleaned, String> {
    let lyrics = v["lyrics"].as_str().unwrap_or("").trim().to_string();
    if lyrics.is_empty() {
        return Err(crate::i18n::tr("AI 응답이 비었습니다.").into());
    }
    Ok(Cleaned { lyrics, order: v["order"].as_str().unwrap_or("").trim().to_string(), notes: v["notes"].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(String::from)).take(8).collect() })
}

pub fn ask_ai(cfg: &AiCfg, text: &str) -> Result<(Cleaned, Value), String> {
    if text.trim().is_empty() {
        return Err(crate::i18n::tr("가사를 붙여 넣으세요.").into());
    }
    let (v, u) = aihelp::chat_json(cfg, &ai_system(), &text.chars().take(8000).collect::<String>(), 3000)?;
    Ok((from_ai(&v)?, u))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn labels_and_lines() {
        assert_eq!(label_of("Verse 1").as_deref(), Some("1절"));
        assert_eq!(label_of("[V2]").as_deref(), Some("2절"));
        assert_eq!(label_of("2.").as_deref(), Some("2절"));
        assert_eq!(label_of("Chorus:").as_deref(), Some("후렴"));
        assert_eq!(label_of("(브릿지)").as_deref(), Some("브리지"));
        assert_eq!(label_of("주 은혜"), None);
        assert!(chord_line("G   D/F#  Em7  C"));
        assert!(!chord_line("Amen 아멘"));
        assert_eq!(tidy_line("  주   은혜  임을 ,  고백 합니다 . "), "주 은혜 임을, 고백 합니다");
        assert_eq!(split_long("나의 모든 행실을 주는 다 아시오니 주님 앞에 나아갑니다", 18), vec!["나의 모든 행실을 주는 다", "아시오니 주님 앞에 나아갑니다"]);
    }
    #[test]
    fn labeled_song_is_deduped_with_order() {
        let src = "작사: 홍길동\nG  D  Em\nVerse 1\n주 은혜임을  고백합니다\n내 삶의 모든 것\n\nChorus\n주님만 바라봅니다\n\n2절:\n주 사랑임을 노래합니다\n\nChorus\n주님만 바라봅니다\n\nCopyright ⓒ 2020";
        let c = clean(src);
        assert_eq!(c.lyrics, "[1절]\n주 은혜임을 고백합니다\n내 삶의 모든 것\n\n[후렴]\n주님만 바라봅니다\n\n[2절]\n주 사랑임을 노래합니다");
        assert_eq!(c.order, "1절 후렴 2절 후렴");
        let t = clean("주 은혜임을 (작사·작곡: 홍길동)
[1절]
주님만 바라봅니다 ,주님만 의지합니다");
        assert_eq!(t.lyrics, "[1절]
주님만 바라봅니다
주님만 의지합니다", "제목·작사 줄은 빼고, 나눈 줄 끝 쉼표도 뺌");
        assert!(c.notes.iter().any(|n| n.contains("코드 줄 1")) && c.notes.iter().any(|n| n.contains("저작권 줄 2")), "{:?}", c.notes);
    }
    #[test]
    fn unlabeled_song_gets_sections_and_bridge() {
        let src = "첫째 절 첫 줄\n첫째 절 둘째 줄\n\n후렴 첫 줄 노래\n후렴 둘째 줄 노래\n\n둘째 절 첫 줄\n둘째 절 둘째 줄\n\n후렴 첫 줄 노래\n후렴 둘째 줄 노래\n\n다리 첫 줄 높이\n다리 둘째 줄 높이\n\n후렴 첫 줄 노래\n후렴 둘째 줄 노래";
        let c = clean(src);
        assert!(c.lyrics.contains("[후렴]") && c.lyrics.contains("[브리지]"), "{}", c.lyrics);
        assert_eq!(c.lyrics.matches("후렴 첫 줄 노래").count(), 1);
        assert_eq!(c.order, "1절 후렴 2절 후렴 브리지 후렴");
        let s = side_by_side(src, &c.lyrics);
        assert!(s.iter().any(|x| !x.2));
    }
    #[test]
    fn ai_mock() {
        let _g = crate::aihelp::TEST_LOCK.lock();
        crate::aihelp::set_mock(Some(Box::new(|_| Ok(serde_json::json!({"choices":[{"message":{"content":"{\"lyrics\":\"[1절]\\n가\",\"order\":\"1절\",\"notes\":[\"오타 1\"]}"}}]})))));
        let r = ask_ai(&AiCfg::default(), "가");
        crate::aihelp::set_mock(None);
        let (c, _) = r.unwrap();
        assert_eq!(c.lyrics, "[1절]\n가");
        assert_eq!(c.notes, vec!["오타 1"]);
    }
}
