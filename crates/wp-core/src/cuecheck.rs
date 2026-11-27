//! 예배 전 큐시트 점검 — 순서마다 고칠 곳을 찾아 목록으로 (누르면 그 순서로)
//!
//! 규칙만으로 (AI 없이, 무료): 빈 가사 · 틀린·없는 장절 · 빠진 템플릿 · 없는 파일 · 보일 것이 없는 순서(슬라이드 0장) · 중복 · 주보와 다른 순서
//! AI 키가 있으면 (선택): 가사·공지 글의 오타 · 어색한 줄 나눔 (글만 보냄, 처음 쓸 때 동의)
//!
//! 연결 점검(diagnose.rs check_cue)의 큐시트 항목은 요약 한 줄, 여기는 순서 번호가 붙은 자세한 목록이다.

use crate::aihelp::{self, AiCfg};
use crate::core::Core;
use serde_json::{json, Value};

/// 찾은 것 하나. index: 큐시트 순서 번호(0부터, 없으면 큐시트 전체), level: error|warn|info
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    pub index: Option<usize>,
    pub level: &'static str,
    pub kind: &'static str,
    pub msg: String,
    pub fix: String,
}

impl Finding {
    pub fn json(&self) -> Value {
        json!({ "index": self.index, "level": self.level, "kind": self.kind, "msg": self.msg, "fix": self.fix })
    }
}

fn f(index: Option<usize>, level: &'static str, kind: &'static str, msg: impl Into<String>, fix: impl Into<String>) -> Finding {
    Finding { index, level, kind, msg: msg.into(), fix: fix.into() }
}

/// 점검에 쓰는 바깥 확인 (시험에서 바꿔 끼운다)
pub struct Env<'a> {
    /// (장절, 번역본 id) → Ok(확인된 제목) | Err(까닭). None = 이 PC 에 성경 본문이 없어 확인 못 함
    pub check_ref: &'a dyn Fn(&str, &str) -> Option<Result<String, String>>,
    /// 파일 주소 → 없으면 경로
    pub missing: &'a dyn Fn(&str) -> Option<String>,
    /// 있는 템플릿 id
    pub templates: &'a [String],
}

fn s<'a>(it: &'a Value, k: &str) -> &'a str {
    it[k].as_str().unwrap_or("")
}
fn empty(it: &Value, k: &str) -> bool {
    s(it, k).trim().is_empty()
}

/// 순서 이름 ('3번 찬양 "주 은혜임을"')
pub fn label(i: usize, it: &Value) -> String {
    let t = s(it, "title").trim();
    if t.is_empty() { format!("{}번", i + 1) } else { format!("{}번 '{}'", i + 1, t) }
}

/// 중복을 가르는 열쇠 (같은 내용을 두 번 넣은 것만: 제목·빈 화면은 같아도 괜찮다)
fn dup_key(it: &Value) -> Option<String> {
    let norm = |x: &str| x.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_lowercase();
    match s(it, "type") {
        "bible" if !empty(it, "ref") => {
            // '요3:16' 과 '요한복음 3장 16절' 을 같게
            let r = match crate::parser::parse_reference(s(it, "ref")) {
                crate::parser::Parsed::Ok { book, ranges } => format!("{}:{:?}", book.id, ranges),
                _ => norm(s(it, "ref")),
            };
            Some(format!("bible|{r}"))
        }
        "hymn" if !empty(it, "file") => Some(format!("hymn|{}", norm(s(it, "file")))),
        "song" if !empty(it, "songId") => Some(format!("song|{}", s(it, "songId"))),
        "song" if !empty(it, "lyrics") => Some(format!("song|{}", norm(s(it, "lyrics")).chars().take(80).collect::<String>())),
        "text" if !empty(it, "body") => Some(format!("text|{}", norm(s(it, "body")))),
        "image" if !empty(it, "image") => Some(format!("image|{}", s(it, "image"))),
        "video" if !empty(it, "file") => Some(format!("video|{}", s(it, "file"))),
        _ => None,
    }
}

/// 규칙 점검 (AI 없이)
pub fn check_items(items: &[Value], env: &Env) -> Vec<Finding> {
    let mut out = vec![];
    if items.is_empty() {
        out.push(f(None, "warn", "empty-cue", "큐시트에 순서가 없습니다.", "순서 넣기로 예배 순서를 만드세요."));
        return out;
    }
    let mut seen: std::collections::HashMap<String, usize> = Default::default();
    for (i, it) in items.iter().enumerate() {
        let n = label(i, it);
        let ty = s(it, "type");
        // 파일
        for k in ["file", "image", "bg", "music"] {
            if ty == "video" && k == "file" && crate::youtube::parse(s(it, k)).is_some() {
                continue;
            }
            if let Some(m) = it[k].as_str().and_then(|x| (env.missing)(x)) {
                out.push(f(Some(i), "error", "file", format!("{n}: 파일이 없습니다 ({m})"), "편집칸에서 파일을 다시 고르세요. 다른 PC 에서 옮겨 왔으면 같은 자리에 두세요."));
            }
        }
        for img in it["images"].as_array().into_iter().flatten().filter_map(|x| x.as_str()) {
            if let Some(m) = (env.missing)(img) {
                out.push(f(Some(i), "error", "file", format!("{n}: 슬라이드쇼 그림이 없습니다 ({m})"), "슬라이드쇼 그림을 다시 고르세요."));
            }
        }
        // 템플릿
        for (k, what) in [("tplMain", "예배당"), ("tplOverlay", "OBS")] {
            let id = s(it, k).trim();
            if !id.is_empty() && !env.templates.iter().any(|t| t == id) {
                out.push(f(Some(i), "warn", "template", format!("{n}: 고른 {what} 템플릿이 없습니다 ({id})"), "편집칸에서 템플릿을 다시 고르세요. 지금은 기본 템플릿으로 나갑니다."));
            }
        }
        // 내용
        match ty {
            "song" => {
                if empty(it, "lyrics") {
                    out.push(f(Some(i), "error", "lyrics", format!("{n}: 찬양 가사가 비어 있습니다"), "가사를 붙여 넣거나 찬양 라이브러리·웹에서 가사 찾기(Ctrl+L)로 채우세요."));
                } else {
                    let bad = crate::slides::unknown_order_tokens(&crate::slides::lyrics_chunks(s(it, "lyrics")), s(it, "order"));
                    if !bad.is_empty() {
                        out.push(f(Some(i), "warn", "order", format!("{n}: 부를 순서의 '{}' 가 가사에 없습니다", bad.join(", ")), "부를 순서를 가사의 이름표([1절]·[후렴])와 맞추세요."));
                    }
                }
            }
            "text" if empty(it, "body") => out.push(f(Some(i), "error", "zero", format!("{n}: 공지 내용이 비어 있습니다 (보일 것이 없음)"), "내용을 쓰거나 순서를 지우세요.")),
            "bible" => {
                if empty(it, "ref") {
                    out.push(f(Some(i), "error", "ref", format!("{n}: 성경 장절이 비어 있습니다"), "편집칸에 장절(예: 요3:16)을 넣으세요."));
                } else {
                    let r = s(it, "ref");
                    match crate::parser::parse_reference(r) {
                        crate::parser::Parsed::Err { error, .. } => out.push(f(Some(i), "error", "ref", format!("{n}: 장절을 읽을 수 없습니다 ({r} — {error})"), "장절을 '요3:16' 처럼 고치세요.")),
                        crate::parser::Parsed::Ok { book, ranges } => {
                            if let Some(x) = ranges.iter().find(|x| x.sc > book.chapters || x.ec > book.chapters || x.sc < 1) {
                                out.push(f(Some(i), "error", "ref", format!("{n}: {}에는 {}장이 없습니다 ({r}, 모두 {}장)", book.name, x.sc.max(x.ec), book.chapters), "장 번호를 확인하세요."));
                            } else if let Some(Err(e)) = (env.check_ref)(r, s(it, "versionId")) {
                                out.push(f(Some(i), "error", "ref", format!("{n}: 없는 장절입니다 ({r} — {e})"), "절 번호를 확인하세요."));
                            }
                        }
                    }
                }
            }
            "image" if empty(it, "image") => out.push(f(Some(i), "error", "zero", format!("{n}: 그림을 고르지 않았습니다 (보일 것이 없음)"), "편집칸에서 그림을 고르세요.")),
            "hymn" if empty(it, "file") => out.push(f(Some(i), "error", "zero", format!("{n}: 찬송가·PPT 파일을 고르지 않았습니다 (보일 것이 없음)"), "찬송가 번호로 찾거나 PPT 파일을 고르세요.")),
            "video" | "audio" if empty(it, "file") => out.push(f(Some(i), "error", "zero", format!("{n}: 파일을 고르지 않았습니다"), "편집칸에서 파일을 고르세요.")),
            "camera" if empty(it, "deviceId") => out.push(f(Some(i), "warn", "zero", format!("{n}: 카메라를 고르지 않았습니다"), "편집칸에서 카메라를 고르세요.")),
            "slideshow" if it["images"].as_array().is_none_or(|a| a.is_empty()) && empty(it, "body") => {
                out.push(f(Some(i), "warn", "zero", format!("{n}: 슬라이드쇼에 그림도 글도 없습니다 (배경만 나감)"), "그림을 더하거나 글을 쓰세요."))
            }
            "title" if empty(it, "title") && empty(it, "subtitle") => out.push(f(Some(i), "warn", "zero", format!("{n}: 제목도 부제도 비어 있습니다 (빈 카드)"), "제목을 쓰거나 '빈 화면' 순서로 바꾸세요.")),
            _ => {}
        }
        // 중복
        if let Some(k) = dup_key(it) {
            if let Some(&j) = seen.get(&k) {
                out.push(f(Some(i), "warn", "dup", format!("{n}: {}과 같은 내용입니다", label(j, &items[j])), "일부러 두 번 넣은 것이 아니면 하나를 지우세요."));
            } else {
                seen.insert(k, i);
            }
        }
    }
    out
}

// ---------------- 주보와 비교 ----------------

fn norm_title(t: &str) -> String {
    t.chars().filter(|c| c.is_alphanumeric()).collect::<String>()
}

/// 주보 순서 하나가 큐시트 순서와 같은가 (종류·장절·찬송 번호·제목)
fn same(plan: &Value, it: &Value) -> bool {
    let pt = s(plan, "type");
    let ty = s(it, "type");
    if pt == "bible" && ty == "bible" {
        return dup_key(plan) == dup_key(it) || norm_title(s(plan, "ref")) == norm_title(s(it, "ref"));
    }
    if pt == "hymn" {
        if let Some(n) = plan["hymnNumber"].as_i64() {
            let num = n.to_string();
            let hay = format!("{} {} {}", s(it, "title"), s(it, "file"), s(it, "subtitle"));
            // '8장' · '008' · '새찬송가 8장' 같은 꼴 — 숫자 앞뒤가 다른 숫자가 아닐 때
            let digits: Vec<char> = hay.chars().collect();
            let mut k = 0;
            while k < digits.len() {
                if digits[k].is_ascii_digit() {
                    let st = k;
                    while k < digits.len() && digits[k].is_ascii_digit() {
                        k += 1;
                    }
                    let n2: String = digits[st..k].iter().collect();
                    if n2.trim_start_matches('0') == num {
                        return true;
                    }
                } else {
                    k += 1;
                }
            }
            return false;
        }
    }
    let (a, b) = (norm_title(s(plan, "title")), norm_title(s(it, "title")));
    !a.is_empty() && (a == b || (a.chars().count() >= 2 && (b.contains(&a) || a.contains(&b) && b.chars().count() >= 2)))
}

/// 주보(규칙 파서·AI 계획의 items)와 큐시트를 비교: 빠진 순서 · 차례가 다른 순서
pub fn compare_bulletin(plan_items: &[Value], items: &[Value]) -> Vec<Finding> {
    let mut out = vec![];
    let plan: Vec<&Value> = plan_items.iter().filter(|p| s(p, "type") != "blank").collect();
    let mut matched: Vec<Option<usize>> = vec![];
    let mut used = vec![false; items.len()];
    for p in &plan {
        let m = items.iter().enumerate().position(|(j, it)| !used[j] && same(p, it));
        if let Some(j) = m {
            used[j] = true;
        }
        matched.push(m);
    }
    let what = |p: &Value| {
        let mut t = s(p, "title").to_string();
        if let Some(n) = p["hymnNumber"].as_i64() {
            t = format!("{t} {n}장");
        }
        if !empty(p, "ref") {
            t = format!("{t} {}", s(p, "ref"));
        }
        t.trim().to_string()
    };
    for (k, p) in plan.iter().enumerate() {
        if matched[k].is_none() {
            // 앞에 맞춘 순서 바로 뒤에 넣으면 된다고 알려 준다
            let after = matched[..k].iter().rev().flatten().next().copied();
            let at = after.map(|a| format!(" ({} 뒤)", label(a, &items[a]))).unwrap_or_default();
            out.push(f(after, "warn", "bulletin", format!("주보의 '{}' 가 큐시트에 없습니다{at}", what(p)), "순서 넣기로 더하거나, 주보가 바뀐 것인지 확인하세요."));
        }
    }
    // 차례: 맞춘 순서들이 큐시트에서 거꾸로 놓인 곳
    let mut last: Option<usize> = None;
    for (k, m) in matched.iter().enumerate() {
        if let Some(j) = *m {
            if let Some(l) = last {
                if j < l {
                    out.push(f(Some(j), "warn", "bulletin", format!("{}: 주보에서는 {} 다음인데 큐시트에서는 앞에 있습니다", label(j, &items[j]), label(l, &items[l])), format!("주보 차례: '{}'", what(plan[k]))));
                }
            }
            last = Some(last.map_or(j, |l| l.max(j)));
        }
    }
    out
}

// ---------------- AI (오타 · 줄 나눔) ----------------

pub fn ai_system() -> String {
    "너는 한국 교회 예배 방송실의 교정 도우미다. 화면에 띄울 찬양 가사와 공지 글에서 오타, 맞춤법·띄어쓰기 실수, 어색한 줄 나눔(낱말이 두 줄로 갈림, 한 줄이 너무 김, 한 장에 줄이 너무 많음)만 찾는다.\n\
     가사의 시적 표현·옛말(예: '주의 은혜라', '하나님께 영광')과 고유명사는 고치지 않는다. 확실한 것만 적고, 없으면 빈 목록.\n\
     반드시 JSON 하나만: {\"issues\":[{\"index\":순서번호,\"line\":\"문제가 있는 줄 그대로\",\"problem\":\"무엇이 문제인지 짧게\",\"suggestion\":\"고친 줄\"}]}".to_string()
}

/// AI 에 보낼 글: 가사·공지 순서만 (번호는 1부터 화면과 같게). 반환 (글, 보낸 순서 수)
pub fn ai_input(items: &[Value]) -> (String, usize) {
    let mut s_out = String::new();
    let mut n = 0;
    for (i, it) in items.iter().enumerate() {
        let body = match s(it, "type") {
            "song" => s(it, "lyrics"),
            "text" => s(it, "body"),
            _ => continue,
        };
        if body.trim().is_empty() {
            continue;
        }
        n += 1;
        let clipped: String = body.chars().take(4000).collect();
        s_out.push_str(&format!("### 순서 {} ({}) {}\n{}\n\n", i + 1, if s(it, "type") == "song" { "찬양 가사" } else { "공지" }, s(it, "title"), clipped));
    }
    (s_out, n)
}

/// AI 답 → 찾은 것 (번호는 1부터 → 0부터, 없는 번호는 버림)
pub fn parse_ai(v: &Value, count: usize) -> Vec<Finding> {
    v["issues"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|x| {
            let idx = x["index"].as_i64().or_else(|| x["index"].as_str().and_then(|t| t.trim().parse().ok()))?;
            if idx < 1 || idx as usize > count {
                return None;
            }
            let line = x["line"].as_str().unwrap_or("").trim().chars().take(80).collect::<String>();
            let problem = x["problem"].as_str().unwrap_or("").trim().to_string();
            if problem.is_empty() {
                return None;
            }
            let sug = x["suggestion"].as_str().unwrap_or("").trim().to_string();
            Some(f(Some(idx as usize - 1), "info", "ai", format!("{}번: {problem} — '{line}'", idx), if sug.is_empty() { String::new() } else { format!("고친 줄: {sug}") }))
        })
        .collect()
}

pub fn ask_ai(cfg: &AiCfg, items: &[Value]) -> Result<(Vec<Finding>, Value), String> {
    let (text, n) = ai_input(items);
    if n == 0 {
        return Ok((vec![], Value::Null));
    }
    let (v, u) = aihelp::chat_json(cfg, &ai_system(), &text, 2000)?;
    Ok((parse_ai(&v, items.len()), u))
}

// ---------------- 코어 연결 ----------------
impl Core {
    /// 열린 큐시트 규칙 점검 (+ 주보 글이 있으면 비교)
    pub fn cue_check(&mut self, bulletin_text: &str) -> Vec<Finding> {
        let Some(cue) = self.cue.clone() else {
            return vec![f(None, "info", "no-cue", "열린 큐시트가 없습니다.", "큐시트를 먼저 여세요.")];
        };
        let ver = self.version_id();
        let templates: Vec<String> = self.settings.templates().iter().filter_map(|t| t["id"].as_str().map(String::from)).collect();
        // 장절 확인은 DB 를 쓰므로 미리 해 둔다
        let mut refs: std::collections::HashMap<(String, String), Option<Result<String, String>>> = Default::default();
        for it in &cue.items {
            if it["type"] == "bible" && !empty(it, "ref") {
                let r = s(it, "ref").to_string();
                let vid = Some(s(it, "versionId")).filter(|x| !x.is_empty() && self.store.has_version(x)).map(String::from).unwrap_or_else(|| ver.clone());
                let res = if !self.store.has_version(&vid) {
                    None
                } else {
                    let x = self.store.resolve_reference(&vid, &r);
                    Some(if x["ok"] == json!(true) { Ok(x["passage"]["title"].as_str().unwrap_or("").to_string()) } else { Err(x["error"].as_str().unwrap_or("본문을 찾지 못했습니다.").to_string()) })
                };
                refs.insert((r, s(it, "versionId").to_string()), res);
            }
        }
        let core: &Core = self;
        let check_ref = |r: &str, v: &str| refs.get(&(r.to_string(), v.to_string())).cloned().flatten();
        let missing = |x: &str| crate::diagnose::missing_file(core, x);
        let env = Env { check_ref: &check_ref, missing: &missing, templates: &templates };
        let mut out = check_items(&cue.items, &env);
        if !bulletin_text.trim().is_empty() {
            let plan = crate::bulletin::parse_order_text(bulletin_text, &|x| crate::parser::looks_like_reference(x));
            let pi = plan["items"].as_array().cloned().unwrap_or_default();
            if pi.is_empty() {
                out.push(f(None, "info", "bulletin", "주보 글에서 예배 순서를 찾지 못했습니다.", "한 줄에 순서 하나씩 붙여 넣으세요 (예: 찬송 8장 · 성경봉독 요3:16)."));
            } else {
                out.extend(compare_bulletin(&pi, &cue.items));
            }
        }
        out.sort_by_key(|x| (x.index.unwrap_or(usize::MAX), match x.level { "error" => 0, "warn" => 1, _ => 2 }));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn env_ok<'a>(t: &'a [String]) -> Env<'a> {
        Env { check_ref: &|r, _| if r.contains("3:99") { Some(Err("절이 없습니다".into())) } else { Some(Ok(r.into())) }, missing: &|x| (x.starts_with("C:\\없는")).then(|| x.to_string()), templates: t }
    }

    #[test]
    fn rules_find_each_kind() {
        let t = vec!["tpl-a".to_string()];
        let items = vec![
            json!({"type":"song","title":"빈 찬양","lyrics":""}),
            json!({"type":"bible","title":"본문","ref":"요3:16"}),
            json!({"type":"bible","title":"없는 절","ref":"요3:99"}),
            json!({"type":"bible","title":"없는 장","ref":"요 30:1"}),
            json!({"type":"bible","title":"같은 본문","ref":"요한복음 3장 16절"}),
            json!({"type":"image","title":"그림","image":"C:\\없는\\a.png","tplMain":"tpl-x"}),
            json!({"type":"text","title":"광고","body":"  "}),
            json!({"type":"title","title":"","subtitle":""}),
            json!({"type":"bible","title":"엉터리","ref":"가나다 3:1"}),
            json!({"type":"song","title":"순서","lyrics":"[1절]\n가\n\n[후렴]\n나","order":"1절, 2절, 후렴"}),
        ];
        let r = check_items(&items, &env_ok(&t));
        let has = |i: usize, kind: &str| r.iter().any(|x| x.index == Some(i) && x.kind == kind);
        assert!(has(0, "lyrics"));
        assert!(!r.iter().any(|x| x.index == Some(1)), "맞는 장절은 조용히: {r:?}");
        assert!(has(2, "ref"));
        assert!(r.iter().any(|x| x.index == Some(3) && x.msg.contains("30장이 없습니다")), "{r:?}");
        assert!(has(4, "dup"), "같은 장절 다른 꼴도 중복");
        assert!(has(5, "file") && has(5, "template"));
        assert!(has(6, "zero") && has(7, "zero"));
        assert!(has(8, "ref"));
        assert!(r.iter().any(|x| x.index == Some(9) && x.kind == "order" && x.msg.contains("2절")), "{r:?}");
        assert!(check_items(&[], &env_ok(&t))[0].kind == "empty-cue");
    }

    #[test]
    fn youtube_and_titles_are_not_missing_or_dup() {
        let t: Vec<String> = vec![];
        let items = vec![json!({"type":"title","title":"기도"}), json!({"type":"title","title":"기도"}), json!({"type":"video","title":"영상","file":"https://youtu.be/dQw4w9WgXcQ"})];
        let env = Env { check_ref: &|_, _| None, missing: &|x| Some(x.to_string()), templates: &t };
        let r = check_items(&items, &env);
        assert!(r.is_empty(), "{r:?}");
    }

    #[test]
    fn bulletin_missing_and_order() {
        let plan = vec![
            json!({"type":"title","title":"예배로 부름"}),
            json!({"type":"hymn","title":"찬송","hymnNumber":8}),
            json!({"type":"bible","title":"성경봉독","ref":"시 23:1-6"}),
            json!({"type":"title","title":"설교"}),
            json!({"type":"title","title":"축도"}),
            json!({"type":"blank","title":"예배 후"}),
        ];
        let items = vec![
            json!({"type":"title","title":"예배로 부름"}),
            json!({"type":"title","title":"설교"}),
            json!({"type":"hymn","title":"찬송 8장","file":"C:\\찬송가\\008.pptx"}),
            json!({"type":"bible","title":"성경봉독","ref":"시편 23:1-6"}),
        ];
        let r = compare_bulletin(&plan, &items);
        assert!(r.iter().any(|x| x.msg.contains("축도") && x.msg.contains("없습니다")), "{r:?}");
        assert!(!r.iter().any(|x| x.msg.contains("성경봉독") && x.msg.contains("없습니다")), "시 = 시편: {r:?}");
        assert!(r.iter().any(|x| x.kind == "bulletin" && x.msg.contains("앞에 있습니다")), "{r:?}");
        // 같으면 조용히
        assert!(compare_bulletin(&plan[..3], &[items[0].clone(), items[2].clone(), items[3].clone()]).is_empty());
    }

    #[test]
    fn ai_input_and_parse() {
        let items = vec![json!({"type":"title","title":"x"}), json!({"type":"song","title":"찬양","lyrics":"주 은헤 임을\n고백 합니다"}), json!({"type":"text","title":"광고","body":"수요 예배"})];
        let (t, n) = ai_input(&items);
        assert_eq!(n, 2);
        assert!(t.contains("### 순서 2 (찬양 가사)") && t.contains("### 순서 3 (공지)"));
        let v = json!({"issues":[{"index":2,"line":"주 은헤 임을","problem":"오타 '은헤'","suggestion":"주 은혜임을"},{"index":9,"problem":"없는 번호"},{"index":"3","line":"","problem":""}]});
        let r = parse_ai(&v, items.len());
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].index, Some(1));
        assert!(r[0].fix.contains("주 은혜임을"));
    }

    #[test]
    fn ai_flow_with_mock() {
        let _g = crate::aihelp::TEST_LOCK.lock();
        crate::aihelp::set_mock(Some(Box::new(|b| {
            assert!(b["messages"][1]["content"].as_str().unwrap().contains("은헤"));
            Ok(json!({"choices":[{"message":{"content":"{\"issues\":[{\"index\":1,\"line\":\"은헤\",\"problem\":\"오타\",\"suggestion\":\"은혜\"}]}"}}]}))
        })));
        let r = ask_ai(&AiCfg::default(), &[json!({"type":"song","title":"a","lyrics":"은헤"})]);
        crate::aihelp::set_mock(None);
        let (r, _) = r.unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].kind, "ai");
    }
}
