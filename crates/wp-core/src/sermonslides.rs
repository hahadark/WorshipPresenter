//! 설교 원고 → 슬라이드 묶음 (제목 · 본문 성경 · 설교 포인트 · 원고 속 성경 구절 · 인용문)
//!
//! 규칙만으로 (AI 없이): 원고 머리의 '제목: · 본문: · 설교자:' (md 는 '# 제목'), 대지는 sermonpoints.rs find_points,
//!   성경 구절은 refscan.rs (요3:16 · 로마서 8장 28절), 인용문은 '> …' 줄 · “…” 로 감싼 줄
//! AI (선택): 같은 모양의 JSON 을 받아 같은 길로 만든다 (원고 글만 보냄, 처음 쓸 때 동의)
//! 화면은 만든 묶음을 미리 보여 주고, 고른 것만 큐시트에 넣는다.

use crate::aihelp::{self, AiCfg};
use crate::core::new_item;
use crate::sermonpoints as sp;
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::{json, Value};

/// 넣을 후보 하나
#[derive(Debug, Clone)]
pub struct Proposal {
    /// title | bible | points | quote
    pub kind: &'static str,
    pub on: bool,
    /// 미리 보기 한 줄
    pub label: String,
    pub item: Value,
}

const MAX_REFS: usize = 12;
const MAX_QUOTES: usize = 8;

fn title_item(title: &str, verse: &str, speaker: &str) -> Value {
    let mut it = new_item("title", if title.trim().is_empty() { "설교" } else { title.trim() });
    it["subtitle"] = json!([verse.trim(), speaker.trim()].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · "));
    it["note"] = json!("설교 원고에서");
    it
}

fn bible_item(r: &str, title: &str) -> Value {
    let mut it = new_item("bible", title);
    it["ref"] = json!(r);
    it
}

/// 원고 속 인용문: '> …' 줄, 줄 전체가 따옴표 “…” 인 것 (15~200자)
pub fn quotes(text: &str) -> Vec<(usize, String)> {
    static Q: Lazy<Regex> = Lazy::new(|| Regex::new(r#"^[“"「『](.{8,200})[”"」』]\s*(?:[-—–]\s*(.{1,40}))?$"#).unwrap());
    let mut out = vec![];
    let mut pos = 0usize;
    let mut block: Vec<String> = vec![];
    let mut block_at = 0;
    let flush = |block: &mut Vec<String>, at: usize, out: &mut Vec<(usize, String)>| {
        if !block.is_empty() {
            let t = block.join("\n");
            if t.chars().count() >= 8 {
                out.push((at, t));
            }
            block.clear();
        }
    };
    for line in text.split('\n') {
        let l = line.trim();
        if let Some(b) = l.strip_prefix('>') {
            if block.is_empty() {
                block_at = pos;
            }
            block.push(b.trim().to_string());
        } else {
            flush(&mut block, block_at, &mut out);
            if let Some(m) = Q.captures(l) {
                let src = m.get(2).map(|x| format!("\n— {}", x.as_str().trim())).unwrap_or_default();
                out.push((pos, format!("{}{src}", m[1].trim())));
            }
        }
        pos += line.len() + 1;
    }
    flush(&mut block, block_at, &mut out);
    out
}

/// md 첫 줄 '# 제목'
fn md_title(text: &str) -> String {
    text.lines().map(|l| l.trim()).find(|l| !l.is_empty()).and_then(|l| l.strip_prefix('#')).map(|l| l.trim_start_matches('#').trim().to_string()).unwrap_or_default()
}

/// 원고 머리의 본문 장절 (ex. '본문: 요 15:1-8') → parser 가 읽는 꼴
fn main_ref(verse: &str) -> Option<String> {
    let v = verse.trim();
    if v.is_empty() {
        return None;
    }
    if let crate::parser::Parsed::Ok { .. } = crate::parser::parse_reference(v) {
        return Some(v.to_string());
    }
    crate::refscan::find_refs(v).into_iter().next().map(|f| f.reference)
}

/// 같은 장절인지 (요3:16 = 요한복음 3:16)
fn same_ref(a: &str, b: &str) -> bool {
    use crate::parser::{parse_reference, Parsed};
    match (parse_reference(a), parse_reference(b)) {
        (Parsed::Ok { book: x, ranges: r1 }, Parsed::Ok { book: y, ranges: r2 }) => x.id == y.id && r1 == r2,
        _ => a == b,
    }
}

/// 자료(제목·본문·설교자·대지·구절·인용) → 후보 목록
pub fn build(title: &str, verse: &str, speaker: &str, points: Option<&Value>, refs: &[(usize, String)], quotes: &[(usize, String)]) -> Vec<Proposal> {
    let mut out = vec![];
    out.push(Proposal { kind: "title", on: true, label: format!("제목 카드: {}{}", if title.trim().is_empty() { "설교" } else { title.trim() }, if verse.trim().is_empty() { String::new() } else { format!(" · {}", verse.trim()) }), item: title_item(title, verse, speaker) });
    let main = main_ref(verse);
    if let Some(r) = &main {
        out.push(Proposal { kind: "bible", on: true, label: format!("본문 성경: {r}"), item: bible_item(r, "성경봉독") });
    }
    if let Some(d) = points {
        if let Ok(mut it) = sp::to_cue_item(d, "word", true, true) {
            it["title"] = json!(if title.trim().is_empty() { "설교 포인트".to_string() } else { format!("{} — 설교 포인트", title.trim()) });
            let n = d["points"].as_array().map(|a| a.len()).unwrap_or(0);
            out.push(Proposal { kind: "points", on: true, label: format!("설교 포인트: 대지 {n}개 ({})", it["note"].as_str().unwrap_or("")), item: it });
        }
    }
    // 원고 순서대로 구절 · 인용
    let mut rest: Vec<(usize, Proposal)> = vec![];
    let mut seen: Vec<String> = main.iter().cloned().collect();
    for (at, r) in refs {
        if seen.iter().any(|s| same_ref(s, r)) || seen.len() > MAX_REFS {
            continue;
        }
        seen.push(r.clone());
        rest.push((*at, Proposal { kind: "bible", on: true, label: format!("원고 속 성경: {r}"), item: bible_item(r, r) }));
    }
    for (at, q) in quotes.iter().take(MAX_QUOTES) {
        let mut it = new_item("text", "인용");
        it["body"] = json!(q);
        let first: String = q.lines().next().unwrap_or("").chars().take(40).collect();
        rest.push((*at, Proposal { kind: "quote", on: true, label: format!("인용문: {first}"), item: it }));
    }
    rest.sort_by_key(|x| x.0);
    out.extend(rest.into_iter().map(|x| x.1));
    out
}

/// 규칙으로: 원고 → 후보
pub fn rule_plan(text: &str, meta: &Value) -> Result<(Vec<Proposal>, String), String> {
    let (t, _) = sp::normalize(text);
    if t.trim().is_empty() {
        return Err(crate::i18n::tr("설교 원고를 붙여 넣거나 파일을 불러오세요.").into());
    }
    let pts = sp::find_points(&t, meta);
    let (mut title, verse, speaker) = match &pts {
        Ok(d) => (d["title"].as_str().unwrap_or("").to_string(), d["verse"].as_str().unwrap_or("").to_string(), d["speaker"].as_str().unwrap_or("").to_string()),
        Err(_) => {
            let g = |k: &str| meta[k].as_str().unwrap_or("").to_string();
            let lines: Vec<String> = t.lines().map(|l| l.trim().to_string()).collect();
            let (a, b, c) = header(&lines);
            (if g("title").is_empty() { a } else { g("title") }, if g("verse").is_empty() { b } else { g("verse") }, if g("speaker").is_empty() { c } else { g("speaker") })
        }
    };
    if title.is_empty() {
        title = md_title(&t);
    }
    let refs: Vec<(usize, String)> = crate::refscan::find_refs(&t).into_iter().map(|f| (f.start, f.reference)).collect();
    let qs = quotes(&t);
    let note = match &pts {
        Ok(_) => String::new(),
        Err(e) => e.clone(),
    };
    Ok((build(&title, &verse, &speaker, pts.as_ref().ok(), &refs, &qs), note))
}

/// 원고 머리 (sermonpoints 의 것과 같은 규칙, 대지를 못 찾았을 때만 씀)
fn header(lines: &[String]) -> (String, String, String) {
    static T: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(?:설교\s*)?제\s*목\s*[:：]\s*(.+)$").unwrap());
    static V: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(?:본\s*문|성\s*경|말\s*씀)\s*[:：]\s*(.+)$").unwrap());
    static S: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(?:설교자|설교)\s*[:：]\s*(.+)$").unwrap());
    let find = |re: &Regex| lines.iter().take(15).find_map(|l| re.captures(l).map(|m| m[1].trim().trim_matches(['“', '”', '"']).to_string())).unwrap_or_default();
    (find(&T), find(&V), find(&S))
}

// ---------------- AI ----------------
pub fn ai_system() -> String {
    format!(
        "당신은 한국 교회의 예배 화면 담당자입니다. 설교 원고를 읽고 예배 중 띄울 슬라이드 재료를 뽑습니다. JSON 하나만 답합니다.\n\
         형식:\n{{\n  \"title\": \"설교 제목\", \"verse\": \"본문 장절 (예: 요 15:1-8)\", \"speaker\": \"설교자\",\n  \"theme\": \"핵심 한 문장\",\n  \"points\": [{{\"heading\":\"대지 (20자 안팎)\",\"body\":\"핵심 문장 (50자 안팎)\"}}],\n  \"application\": \"적용 한 문장\",\n  \"refs\": [\"원고에서 읽거나 인용하는 다른 성경 구절 장절만, 나오는 순서대로 (예: 롬 8:28)\"],\n  \"quotes\": [{{\"text\":\"원고에 실제로 있는 인용문 그대로\",\"source\":\"말한 사람·책 (없으면 빈 문자열)\"}}]\n}}\n\
         원칙: 원고에 없는 내용은 만들지 않습니다. 성경 본문 문장을 새로 쓰지 않습니다(장절만). 대지는 원고의 흐름대로 3~4개. 모든 글은 한국어."
    )
}

/// AI 답 → 후보 (장절은 parser 로 확인한 것만)
pub fn from_ai(v: &Value, meta: &Value, text: &str) -> Vec<Proposal> {
    let g = |k: &str| {
        let m = meta[k].as_str().unwrap_or("").trim();
        if m.is_empty() { v[k].as_str().unwrap_or("").trim().to_string() } else { m.to_string() }
    };
    let pts = if v["points"].as_array().is_some_and(|a| !a.is_empty()) { Some(sp::clean_data(v)) } else { None };
    // 장절 자리는 원고에서 찾아 순서를 맞춘다 (못 찾으면 끝에)
    let found = crate::refscan::find_refs(text);
    let refs: Vec<(usize, String)> = v["refs"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| r.as_str())
        .filter_map(|r| match crate::parser::parse_reference(r) {
            crate::parser::Parsed::Ok { .. } => {
                let at = found.iter().find(|f| same_ref(&f.reference, r)).map(|f| f.start).unwrap_or(usize::MAX - 1);
                Some((at, r.trim().to_string()))
            }
            _ => crate::refscan::find_refs(r).into_iter().next().map(|f| (usize::MAX - 1, f.reference)),
        })
        .collect();
    let qs: Vec<(usize, String)> = v["quotes"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|q| {
            let t = q["text"].as_str().or(q.as_str())?.trim().to_string();
            if t.chars().count() < 6 {
                return None;
            }
            let src = q["source"].as_str().unwrap_or("").trim();
            let at = text.find(&t.chars().take(12).collect::<String>()).unwrap_or(usize::MAX);
            Some((at, if src.is_empty() { t } else { format!("{t}\n— {src}") }))
        })
        .collect();
    build(&g("title"), &g("verse"), &g("speaker"), pts.as_ref(), &refs, &qs)
}

pub fn ask_ai(cfg: &AiCfg, text: &str, meta: &Value) -> Result<(Vec<Proposal>, Value), String> {
    let (t, _) = sp::normalize(text);
    if t.trim().is_empty() {
        return Err(crate::i18n::tr("설교 원고를 붙여 넣거나 파일을 불러오세요.").into());
    }
    let mut user = String::new();
    for (k, l) in [("title", "설교 제목"), ("verse", "본문"), ("speaker", "설교자")] {
        if let Some(x) = meta[k].as_str().filter(|x| !x.trim().is_empty()) {
            user.push_str(&format!("{l}: {x}\n"));
        }
    }
    user.push_str("설교 원고:\n");
    user.push_str(&t.chars().take(60000).collect::<String>());
    let (v, u) = aihelp::chat_json(cfg, &ai_system(), &user, 3000)?;
    Ok((from_ai(&v, meta, &t), u))
}

/// 고른 후보 → 큐시트에 넣을 순서들
pub fn chosen_items(list: &[Proposal]) -> Vec<Value> {
    list.iter().filter(|p| p.on).map(|p| {
        let mut it = p.item.clone();
        it["id"] = json!(crate::util::short_id("i"));
        it
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    const MS: &str = "# 포도나무와 가지\n본문: 요 15:1-8\n설교자: 홍길동 사관\n\n예수님은 나는 포도나무요 너희는 가지라 하셨습니다 (요15:5).\n첫째, 붙어 있어야 삽니다. 가지는 스스로 열매를 맺을 수 없습니다.\n로마서 8장 28절에서 바울은 말합니다.\n> 우리가 알거니와 하나님을 사랑하는 자 곧 그의 뜻대로 부르심을 입은 자들에게는\n둘째, 열매로 압니다. 열매가 나무를 보여 줍니다.\n“기도는 영혼의 호흡이다” — 어느 성도\n셋째, 아버지께 영광이 됩니다.\n다시 요한복음 15장 5절을 봅시다.";

    #[test]
    fn rule_plan_builds_bundle() {
        let (list, note) = rule_plan(MS, &json!({})).unwrap();
        assert!(note.is_empty(), "{note}");
        let kinds: Vec<&str> = list.iter().map(|p| p.kind).collect();
        assert_eq!(&kinds[..3], &["title", "bible", "points"], "{kinds:?}");
        assert_eq!(list[0].item["title"], "포도나무와 가지");
        assert_eq!(list[0].item["subtitle"], "요 15:1-8 · 홍길동 사관");
        assert_eq!(list[1].item["ref"], "요 15:1-8");
        assert!(list[2].label.contains("대지 3개"), "{}", list[2].label);
        // 원고 속 구절: 요15:5 (본문 안이지만 다른 범위) · 롬 8:28, 같은 요 15:5 는 한 번만
        let refs: Vec<&str> = list.iter().filter(|p| p.kind == "bible").skip(1).map(|p| p.item["ref"].as_str().unwrap()).collect();
        assert_eq!(refs, vec!["요한복음 15:5", "로마서 8:28"]);
        let qs: Vec<&Proposal> = list.iter().filter(|p| p.kind == "quote").collect();
        assert_eq!(qs.len(), 2);
        assert!(qs[1].item["body"].as_str().unwrap().contains("— 어느 성도"));
        // 원고 차례: 롬 8:28 → 인용(>) → “기도는…”
        let pos = |k: &str| list.iter().position(|p| p.label.contains(k)).unwrap();
        assert!(pos("로마서 8:28") < pos("우리가 알거니와") && pos("우리가 알거니와") < pos("기도는"));
        let items = chosen_items(&list);
        assert_eq!(items.len(), list.len());
    }

    #[test]
    fn no_points_still_gives_title_and_refs() {
        let (list, note) = rule_plan("제목: 은혜\n오늘은 시편 23편 1절을 묵상합니다.", &json!({})).unwrap();
        assert!(!note.is_empty());
        assert_eq!(list[0].item["title"], "은혜");
        assert!(list.iter().any(|p| p.item["ref"] == "시편 23:1"));
        assert!(rule_plan("  ", &json!({})).is_err());
    }

    #[test]
    fn ai_answer_with_mock() {
        let _g = crate::aihelp::TEST_LOCK.lock();
        crate::aihelp::set_mock(Some(Box::new(|_| {
            Ok(json!({"choices":[{"message":{"content":"{\"title\":\"포도나무\",\"verse\":\"요 15:1-8\",\"speaker\":\"\",\"theme\":\"붙어 있으면 삽니다\",\"points\":[{\"heading\":\"붙어 있어야 삽니다\",\"body\":\"가지는 홀로 못 삽니다\"},{\"heading\":\"열매로 압니다\",\"body\":\"\"}],\"application\":\"\",\"refs\":[\"롬 8:28\",\"엉터리 99:1\",\"요 15:1-8\"],\"quotes\":[{\"text\":\"기도는 영혼의 호흡이다\",\"source\":\"어느 성도\"}]}"}}]}))
        })));
        let r = ask_ai(&AiCfg::default(), MS, &json!({"speaker":"홍길동"}));
        crate::aihelp::set_mock(None);
        let (list, _) = r.unwrap();
        assert_eq!(list[0].item["subtitle"], "요 15:1-8 · 홍길동");
        assert!(list.iter().any(|p| p.kind == "points"));
        let refs: Vec<&str> = list.iter().filter(|p| p.kind == "bible").map(|p| p.item["ref"].as_str().unwrap()).collect();
        assert_eq!(refs, vec!["요 15:1-8", "롬 8:28"], "본문과 같은 것·엉터리는 빠짐");
        assert!(list.iter().any(|p| p.kind == "quote" && p.item["body"].as_str().unwrap().ends_with("— 어느 성도")));
    }
}

impl crate::core::Core {
    /// 여러 순서를 한 번에 넣는다 (되돌리기 한 번으로 모두 빠짐). 반환: 넣은 첫 자리
    pub fn add_items(&mut self, items: Vec<Value>, at: Option<usize>) -> usize {
        self.remember_items();
        if self.cue.is_none() {
            let _ = self.new_cue("새 큐시트");
        }
        let mut first = 0;
        if let Some(c) = &mut self.cue {
            let i = at.unwrap_or(c.items.len()).min(c.items.len());
            first = i;
            for (k, it) in items.into_iter().enumerate() {
                c.items.insert(i + k, it);
            }
        }
        self.save_cue();
        first
    }
}
