//! [트랙 A] 설교 포인트 슬라이드 (Electron 판 sermonpoints.js)
//! 설교 원고(또는 자막) → 대지 3~4개와 핵심 문장 → 송출용 '제목' 순서 하나 (장마다 카드 한 장)
//!  - 규칙으로 (AI 없이): '첫째·둘째', '1. 2.', 'Ⅰ. Ⅱ.', '대지 1' 같은 표시를 찾는다
//!  - 오픈라우터 API · 채팅 AI (프롬프트 복사 → 답 붙여 넣기)
//!  - 데이터 모양은 설교쇼츠 편집기 '설교 요약' 과 같다: { title, verse, speaker, theme, points:[{heading, body}], application }

use crate::bulletin::{parse_json, DEFAULT_ENDPOINT, DEFAULT_MODEL};
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::{json, Value};

const ORDINAL_WORDS: [&str; 6] = ["첫째", "둘째", "셋째", "넷째", "다섯째", "여섯째"];
const ORDINAL_LONG: [&str; 6] = ["첫", "두", "세", "네", "다섯", "여섯"]; // + '번째'
const ROMAN: [&str; 6] = ["Ⅰ", "Ⅱ", "Ⅲ", "Ⅳ", "Ⅴ", "Ⅵ"];
const ROMAN_ASCII: [&str; 6] = ["I", "II", "III", "IV", "V", "VI"];
const CIRCLED: [&str; 6] = ["①", "②", "③", "④", "⑤", "⑥"];
const MAX_HEAD: usize = 40;
const MAX_BODY: usize = 80;

fn re(s: &str) -> Regex {
    Regex::new(s).unwrap()
}

/// 원고 정리: BOM · 자막 번호·시간 줄 지우기. 자막이면 한 문단으로 → (글, 자막인가)
pub fn normalize(text: &str) -> (String, bool) {
    let t = text.trim_start_matches('\u{feff}').replace('\r', "").replace('\u{a0}', " ");
    static SUB: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?m)^WEBVTT|\d{2}:\d{2}:\d{2}[,.]\d{3}\s*-->").unwrap());
    static TAG: Lazy<Regex> = Lazy::new(|| Regex::new(r"<[^>]+>").unwrap());
    static NUM: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*\d+\s*$").unwrap());
    static NOTE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(NOTE|STYLE)\b").unwrap());
    let is_sub = SUB.is_match(&t);
    let t = if is_sub {
        t.split('\n')
            .filter(|l| !NUM.is_match(l) && !l.contains("-->") && !l.starts_with("WEBVTT") && !NOTE.is_match(l))
            .map(|l| TAG.replace_all(l, "").trim().to_string())
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        t
    };
    (t.trim().to_string(), is_sub)
}

/// 문장 나누기 (마침표 뒤, 또는 부호가 없을 때 '…니다 …어요 …죠' 같은 끝말 뒤)
pub fn sentences(s: &str) -> Vec<String> {
    const ENDS: [&str; 10] = ["니다", "어요", "아요", "에요", "지요", "죠", "까요", "세요", "십시다", "합시다"];
    let mut out = vec![];
    let mut cur: Vec<&str> = vec![];
    for w in s.split_whitespace() {
        cur.push(w);
        let end = w.ends_with(['.', '!', '?', '。']) || ENDS.iter().any(|e| w.ends_with(e));
        if end {
            out.push(cur.join(" "));
            cur.clear();
        }
    }
    if !cur.is_empty() {
        out.push(cur.join(" "));
    }
    out
}

/// 글을 n 자 안쪽으로 (낱말 경계에서 자르고 …)
pub fn clip(s: &str, n: usize) -> String {
    let t = s.split_whitespace().collect::<Vec<_>>().join(" ");
    let chars: Vec<char> = t.chars().collect();
    if chars.len() <= n {
        return t;
    }
    let cut: String = chars[..n].iter().collect();
    let sp = cut.chars().rev().position(|c| c == ' ').map(|p| n - 1 - p);
    let base: String = match sp {
        Some(i) if i as f32 > n as f32 * 0.6 => cut.chars().take(i).collect(),
        _ => cut,
    };
    format!("{}…", base.trim_end_matches([',', '，', '·', ' ']))
}

fn tidy_head(s: &str) -> String {
    static A: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[\s,，.:：)\]\-]+").unwrap());
    static B: Lazy<Regex> = Lazy::new(|| Regex::new(r"[.。]+$").unwrap());
    let t = A.replace(s, "");
    clip(B.replace(&t, "").trim(), MAX_HEAD)
}
fn tidy_body(s: &str) -> String {
    clip(s.trim(), MAX_BODY)
}

/// 원고 머리의 '제목: …' '본문: …' '설교자: …'
fn header_meta(lines: &[String]) -> (String, String, String) {
    static T: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(?:설교\s*)?제\s*목\s*[:：]\s*(.+)$").unwrap());
    static V: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(?:본\s*문|성\s*경|말\s*씀|봉\s*독)\s*[:：]\s*(.+)$").unwrap());
    static S: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(?:설교자|설교|말씀)\s*[:：]\s*(.+)$").unwrap());
    static Q: Lazy<Regex> = Lazy::new(|| Regex::new(r#"^[“"'‘「『]|[”"'’」』]$"#).unwrap());
    let (mut t, mut v, mut s) = (String::new(), String::new(), String::new());
    for l in lines.iter().take(15) {
        if t.is_empty() {
            if let Some(m) = T.captures(l) {
                t = Q.replace_all(&m[1], "").trim().to_string();
                continue;
            }
        }
        if v.is_empty() {
            if let Some(m) = V.captures(l) {
                v = m[1].trim().to_string();
                continue;
            }
        }
        if s.is_empty() {
            if let Some(m) = S.captures(l) {
                s = m[1].trim().to_string();
            }
        }
    }
    (t, v, s)
}

#[derive(Clone, Debug)]
struct Mark {
    fam: &'static str,
    n: usize, // 0 = '마지막으로' (바로 다음 번호)
    rest: String,
    i: usize,
}

/// 줄 머리의 대지 표시 → (fam, n, 나머지 글)
fn head_mark(line: &str) -> Option<(&'static str, usize, String)> {
    let l = line.trim();
    static TAIL: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*[,，.:：)]?\s*").unwrap());
    let boundary = |s: &str| s.is_empty() || s.starts_with([' ', '\t', ',', '，', '.', ':', '：', ')']);
    for i in 0..6 {
        let pat = format!(r"^(?:{}|{}\s*번째)", ORDINAL_WORDS[i], ORDINAL_LONG[i]);
        if let Some(m) = Regex::new(&pat).unwrap().find(l) {
            let after = &l[m.end()..];
            // 뒤에 '으로·로·는·은' 이 붙을 수 있다 ('첫째날' 은 아님: 뒤에 띄어쓰기·부호가 있어야)
            let suf = re(r"^\s*(?:으로|로|는|은)").find(after).map(|x| x.end());
            for cut in suf.into_iter().chain(std::iter::once(0)) {
                let rest = &after[cut..];
                if boundary(rest) {
                    let t = TAIL.find(rest).map(|x| x.end()).unwrap_or(0);
                    return Some(("word", i + 1, rest[t..].to_string()));
                }
            }
        }
    }
    if let Some(m) = re(r"^마지막으로[\s,，]").find(l) {
        return Some(("word", 0, l[m.end()..].to_string()));
    }
    if let Some(m) = re(r"^(?:대지|포인트|요점)\s*([1-6])\s*[.:：)]?\s*").captures(l) {
        return Some(("daeji", m[1].parse().unwrap(), l[m.get(0).unwrap().end()..].to_string()));
    }
    if let Some(m) = re(r"^([ⅠⅡⅢⅣⅤⅥ])\s*[.)]?\s*").captures(l) {
        return Some(("roman", ROMAN.iter().position(|x| *x == &m[1]).unwrap() + 1, l[m.get(0).unwrap().end()..].to_string()));
    }
    if let Some(m) = re(r"^(VI|IV|V|I{1,3})\s*[.)]\s*").captures(l) {
        return Some(("roman", ROMAN_ASCII.iter().position(|x| *x == &m[1]).unwrap() + 1, l[m.get(0).unwrap().end()..].to_string()));
    }
    if let Some(m) = re(r"^([①-⑥])\s*").captures(l) {
        return Some(("circled", CIRCLED.iter().position(|x| *x == &m[1]).unwrap() + 1, l[m.get(0).unwrap().end()..].to_string()));
    }
    if let Some(m) = re(r"^([1-6])\s*[.)]\s+").captures(l) {
        return Some(("num", m[1].parse().unwrap(), l[m.get(0).unwrap().end()..].to_string()));
    }
    None
}

/// 표시가 붙은 덩어리 중 1,2,3… 차례로 이어지는 가장 그럴듯한 묶음
fn best_run(marks: &[Mark]) -> Option<Vec<Mark>> {
    let mut best: Option<Vec<Mark>> = None;
    for fam in ["word", "roman", "daeji", "circled", "num"] {
        let mut run: Vec<Mark> = vec![];
        for x in marks.iter().filter(|x| x.fam == fam) {
            let want = run.len() + 1;
            if x.n == want || (x.n == 0 && !run.is_empty()) {
                run.push(x.clone());
            } else if x.n == 1 && run.len() < 2 {
                run.clear();
                run.push(x.clone());
            }
        }
        if run.len() >= 2 && best.as_ref().is_none_or(|b| run.len() > b.len()) {
            best = Some(run);
        }
        if best.is_some() && fam == "word" {
            break;
        }
    }
    best
}

fn point_of(rest: &str, follow: &str) -> Value {
    let first = sentences(rest);
    let mut heading = first.first().cloned().unwrap_or_default();
    let mut body = first.get(1).cloned().unwrap_or_default();
    let hc: Vec<char> = heading.chars().collect();
    if hc.len() > MAX_HEAD && hc[6.min(hc.len())..(MAX_HEAD + 6).min(hc.len())].iter().any(|c| *c == ',' || *c == '，') {
        if let Some(k) = hc.iter().position(|c| *c == ',' || *c == '，') {
            if k > 5 {
                if body.is_empty() {
                    body = hc[k + 1..].iter().collect::<String>().trim().to_string();
                }
                heading = hc[..k].iter().collect();
            }
        }
    }
    if body.is_empty() {
        body = sentences(follow).into_iter().find(|s| s.chars().filter(|c| !c.is_whitespace()).count() >= 8).unwrap_or_default();
    }
    json!({ "heading": tidy_head(&heading), "body": tidy_body(&body) })
}

/// 규칙으로 대지 찾기. meta: { title, verse, speaker } (비어 있으면 원고 머리에서)
pub fn find_points(text: &str, meta: &Value) -> Result<Value, String> {
    let (t, is_sub) = normalize(text);
    if t.is_empty() {
        return Err("설교 원고를 붙여 넣거나 파일을 불러오세요.".into());
    }
    let m = |k: &str| meta[k].as_str().unwrap_or("").to_string();
    if let Some(mut d) = try_answer(&t) {
        for k in ["title", "verse", "speaker"] {
            if d[k].as_str().is_none_or(|x| x.is_empty()) {
                d[k] = json!(m(k));
            }
        }
        d["found"] = json!("json");
        return Ok(d);
    }
    let lines: Vec<String> = t.split('\n').map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect();
    let (ht, hv, hs) = header_meta(&lines);
    let or = |a: String, b: String| if a.is_empty() { b } else { a };
    let mut out = json!({ "title": or(m("title"), ht), "verse": or(m("verse"), hv), "speaker": or(m("speaker"), hs), "theme": "", "points": [], "application": "", "found": "" });
    let mut blocks: Vec<(String, String)> = vec![];
    if !is_sub {
        let marks: Vec<Mark> = lines.iter().enumerate().filter_map(|(i, l)| head_mark(l).map(|(fam, n, rest)| Mark { fam, n, rest, i })).collect();
        if let Some(run) = best_run(&marks) {
            for (k, x) in run.iter().enumerate() {
                let end = run.get(k + 1).map(|y| y.i).unwrap_or(lines.len());
                blocks.push((x.rest.clone(), lines[x.i + 1..end].join(" ")));
            }
            out["found"] = json!("lines");
        }
    }
    if blocks.is_empty() {
        let ss = sentences(&t);
        let marks: Vec<Mark> = ss.iter().enumerate().filter_map(|(i, s)| head_mark(s).filter(|h| h.0 == "word").map(|(fam, n, rest)| Mark { fam, n, rest, i })).collect();
        if let Some(run) = best_run(&marks) {
            for (k, x) in run.iter().enumerate() {
                let end = run.get(k + 1).map(|y| y.i).unwrap_or((x.i + 8).min(ss.len()));
                blocks.push((x.rest.clone(), ss[x.i + 1..end].join(" ")));
            }
            out["found"] = json!("sentences");
        }
    }
    if blocks.is_empty() {
        return Err("대지 표시('첫째·둘째', '1. 2.', 'Ⅰ. Ⅱ.')를 찾지 못했습니다. 'AI 로 뽑기' 를 쓰거나 대지를 직접 입력하세요.".into());
    }
    out["points"] = json!(blocks.iter().take(6).map(|(r, f)| point_of(r, f)).filter(|p| p["heading"].as_str().is_some_and(|h| !h.is_empty())).collect::<Vec<_>>());
    static AP: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(?:적용|결론|맺음말|마치며)\s*[:：]?\s*").unwrap());
    if let Some(ap) = lines.iter().find(|l| AP.find(l).is_some_and(|m| m.end() < l.len())) {
        out["application"] = json!(tidy_body(&sentences(&AP.replace(ap, "")).into_iter().next().unwrap_or_default()));
    }
    Ok(out)
}

// ---------------- AI ----------------
pub const FORMAT: &str = r#"{
  "title": "설교 제목",
  "verse": "본문 장절 (예: 롬 1:16-17)",
  "speaker": "설교자 (모르면 빈 문자열)",
  "theme": "설교 전체의 핵심을 한 문장으로 (40자 안팎)",
  "points": [
    { "heading": "대지 제목 (20자 안팎, 예: 복음은 하나님의 능력입니다)", "body": "그 대지의 핵심 문장 한 줄 (50자 안팎)" }
  ],
  "application": "이번 주 삶의 적용 한 문장 (없으면 빈 문자열)"
}"#;
const RULES: &str = "원칙:
- 설교 원고에 실제로 있는 흐름과 대지만 씁니다. 원고에 '첫째·둘째' 같은 대지가 있으면 그 순서와 뜻을 그대로 따릅니다.
- 대지는 3~4개 (원고가 2개나 5개로 분명히 나뉘면 그대로).
- 예배 화면(큰 글씨)에 띄울 글입니다. heading 은 20자 안팎의 한 줄, body 는 50자 안팎의 한 문장으로 짧고 분명하게.
- 성경 본문 문장을 새로 만들지 않습니다. 장절은 \"롬 1:16-17\" 처럼만 씁니다.
- 원고가 자동 자막이면 띄어쓰기·맞춤법·교회 용어를 문맥에 맞게 바로잡습니다.
- 교회·설교자 이름은 주어진 대로 씁니다. 모든 글은 한국어 존댓말.";

pub fn system() -> String {
    format!("당신은 한국 교회의 예배 화면(PPT) 담당자입니다. 주일 설교 원고에서 예배 중 화면에 띄울 '설교 포인트'(대지와 핵심 문장)를 뽑습니다.\n답은 아래 형식의 JSON 하나만 씁니다.\n\n형식:\n{FORMAT}\n\n{RULES}")
}

fn meta_lines(meta: &Value) -> String {
    let g = |k: &str| meta[k].as_str().unwrap_or("").trim().to_string();
    [("설교 제목", g("title")), ("본문", g("verse")), ("설교자", g("speaker"))].iter().filter(|(_, v)| !v.is_empty()).map(|(k, v)| format!("{k}: {v}")).collect::<Vec<_>>().join("\n")
}

fn head60k(t: &str) -> String {
    t.chars().take(60000).collect()
}

/// 채팅 AI (키 없이) 에 붙여 넣을 프롬프트 (원고 포함)
pub fn chat_prompt(text: &str, meta: &Value, extra: &str) -> Result<String, String> {
    let (t, _) = normalize(text);
    if t.is_empty() {
        return Err("설교 원고를 먼저 붙여 넣으세요.".into());
    }
    let ml = meta_lines(meta);
    Ok(format!(
        "{}\n\n답하는 법:\n- JSON 하나만 ```json 코드 블록으로 답합니다. 앞뒤 설명은 쓰지 않습니다.{}\n\n{}[설교 원고]\n{}",
        system(),
        if extra.is_empty() { String::new() } else { format!("\n\n추가 요청: {extra}") },
        if ml.is_empty() { String::new() } else { format!("{ml}\n\n") },
        head60k(&t)
    ))
}

/// 오픈라우터 요청 본문
pub fn request_body(model: &str, text: &str, meta: &Value, extra: &str) -> Result<Value, String> {
    let (t, _) = normalize(text);
    if t.is_empty() {
        return Err("설교 원고를 붙여 넣으세요.".into());
    }
    let ml = meta_lines(meta);
    let user = format!("{}[설교 원고]\n{}{}", if ml.is_empty() { String::new() } else { format!("{ml}\n\n") }, head60k(&t), if extra.is_empty() { String::new() } else { format!("\n\n추가 요청: {extra}") });
    Ok(json!({ "model": if model.is_empty() { DEFAULT_MODEL } else { model }, "messages": [{ "role": "system", "content": system() }, { "role": "user", "content": user }], "temperature": 0.2, "response_format": { "type": "json_object" } }))
}

/// 오픈라우터 응답 → { data, usage, model }
pub fn parse_response(status: u16, body: &Value, model: &str, meta: &Value) -> Result<Value, String> {
    if !(200..300).contains(&status) {
        return Err(format!("오픈라우터 오류 ({status}): {}", body["error"]["message"].as_str().unwrap_or("")));
    }
    let msg = &body["choices"][0]["message"]["content"];
    let raw = match msg {
        Value::String(s) if !s.is_empty() => s.clone(),
        Value::Array(a) => a.iter().filter_map(|c| c["text"].as_str()).collect(),
        _ => return Err("AI 응답이 비었습니다.".into()),
    };
    let mut d = clean_data(&parse_json(&raw)?);
    if d["points"].as_array().is_none_or(|a| a.is_empty()) {
        return Err("AI 답에 대지(points)가 없습니다.".into());
    }
    for k in ["title", "verse", "speaker"] {
        if d[k].as_str().is_none_or(|x| x.is_empty()) {
            d[k] = meta[k].clone();
        }
    }
    Ok(json!({ "data": d, "usage": body["usage"], "model": body["model"].as_str().unwrap_or(model) }))
}

/// 오픈라우터로 뽑기 (실제 호출 — 다른 스레드에서)
pub fn ask_points(api_key: &str, model: &str, endpoint: &str, text: &str, meta: &Value, extra: &str) -> Result<Value, String> {
    if api_key.is_empty() {
        return Err("오픈라우터 API 키가 없습니다. 파일 → 주보로 큐시트 만들기 → API 키로 에서 넣거나, 채팅 AI 를 쓰세요.".into());
    }
    let body = request_body(model, text, meta, extra)?;
    let resp = ureq::post(if endpoint.is_empty() { DEFAULT_ENDPOINT } else { endpoint })
        .header("Authorization", &format!("Bearer {api_key}"))
        .header("HTTP-Referer", "https://github.com/hahadark/WorshipPresenter")
        .header("X-Title", "Worship Presenter")
        .config()
        .http_status_as_error(false)
        .build()
        .send_json(&body)
        .map_err(|e| format!("AI 에 연결하지 못했습니다: {e}"))?;
    let status = resp.status().as_u16();
    let v: Value = resp.into_body().read_json().unwrap_or(Value::Null);
    parse_response(status, &v, body["model"].as_str().unwrap_or(""), meta)
}

/// AI 답 · 설교쇼츠 '설교 요약' JSON → 정리된 데이터
pub fn clean_data(o: &Value) -> Value {
    let o = if o["summary"].is_object() { &o["summary"] } else { o };
    let s = |v: &Value, n: usize| -> String {
        let t = match v {
            Value::String(x) => x.clone(),
            Value::Null => String::new(),
            x => x.to_string(),
        };
        t.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(n).collect()
    };
    let pts = o["points"].as_array().or(o["대지"].as_array()).cloned().unwrap_or_default();
    let points: Vec<Value> = pts
        .iter()
        .take(6)
        .map(|p| match p {
            Value::String(x) => json!({ "heading": tidy_head(x), "body": "" }),
            _ => {
                let h = if p["heading"].is_null() { &p["title"] } else { &p["heading"] };
                let b = if p["body"].is_null() { &p["text"] } else { &p["body"] };
                json!({ "heading": tidy_head(&s(h, 200)), "body": tidy_body(&s(b, 400)) })
            }
        })
        .filter(|p| p["heading"].as_str().is_some_and(|h| !h.is_empty()))
        .collect();
    let verse = if o["verse"].is_null() { &o["ref"] } else { &o["verse"] };
    json!({ "title": s(&o["title"], 120), "verse": s(verse, 80), "speaker": s(&o["speaker"], 60), "theme": tidy_body(&s(&o["theme"], 300)), "points": points, "application": tidy_body(&s(&o["application"], 300)) })
}

/// 글에서 포인트 JSON 을 찾으면 정리해서, 아니면 None
pub fn try_answer(text: &str) -> Option<Value> {
    let t = text.trim();
    if !t.contains(['{', '[']) || !(t.contains("\"points\"") || t.contains("\"heading\"") || t.contains("\"대지\"")) {
        return None;
    }
    static BLK: Lazy<Regex> = Lazy::new(|| Regex::new(r"```(?:json|JSON)?\s*([\s\S]*?)```").unwrap());
    let mut cands: Vec<String> = BLK.captures_iter(t).map(|m| m[1].to_string()).collect();
    cands.push(t.to_string());
    for b in cands {
        if let Ok(v) = parse_json(&b) {
            let d = clean_data(&v);
            if d["points"].as_array().is_some_and(|a| !a.is_empty()) {
                return Some(d);
            }
        }
    }
    None
}

/// 채팅 AI 답 → 데이터 (meta 로 빈 칸 채움)
pub fn parse_answer(text: &str, meta: &Value) -> Result<Value, String> {
    let mut d = try_answer(text).ok_or("AI 답에서 설교 포인트 JSON 을 찾지 못했습니다. AI 에게 \"JSON 코드 블록 하나로 다시 답해 줘\" 라고 해 보세요.")?;
    for k in ["title", "verse", "speaker"] {
        if d[k].as_str().is_none_or(|x| x.is_empty()) {
            d[k] = json!(meta[k].as_str().unwrap_or(""));
        }
    }
    Ok(d)
}

// ---------------- 큐시트 순서로 ----------------
fn one(s: &str) -> String {
    static D: Lazy<Regex> = Lazy::new(|| Regex::new(r"-{3,}").unwrap());
    static N: Lazy<Regex> = Lazy::new(|| Regex::new(r"\n+").unwrap());
    D.replace_all(&N.replace_all(s, " "), "—").trim().to_string()
}

/// 데이터 → '제목' 순서 하나 (첫 장 = 설교 제목 · 본문/설교자, 다음 장 = 핵심 · 대지들 · 적용)
/// numbering: word | num | none
pub fn to_cue_item(data: &Value, numbering: &str, theme: bool, application: bool) -> Result<Value, String> {
    let d = clean_data(data);
    let pts = d["points"].as_array().cloned().unwrap_or_default();
    if pts.is_empty() {
        return Err("대지가 하나도 없습니다.".into());
    }
    let numbering = if ["word", "num", "none"].contains(&numbering) { numbering } else { "word" };
    let g = |k: &str| d[k].as_str().unwrap_or("").to_string();
    let mut pages = vec![];
    if theme && !g("theme").is_empty() {
        pages.push(format!("{}\n오늘의 핵심", one(&g("theme"))));
    }
    for (i, p) in pts.iter().enumerate() {
        let lab = match numbering {
            "none" => String::new(),
            "num" => format!("{}. ", i + 1),
            _ => format!("{}, ", ORDINAL_WORDS[i.min(5)]),
        };
        let body = p["body"].as_str().unwrap_or("");
        pages.push(format!("{lab}{}{}", one(p["heading"].as_str().unwrap_or("")), if body.is_empty() { String::new() } else { format!("\n{}", one(body)) }));
    }
    if application && !g("application").is_empty() {
        pages.push(format!("{}\n이번 주 적용", one(&g("application"))));
    }
    let title = one(&g("title"));
    let sub = [one(&g("verse")), one(&g("speaker"))].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · ");
    Ok(json!({ "type": "title", "title": if title.is_empty() { "설교".to_string() } else { title }, "subtitle": sub, "more": pages.join("\n---\n"), "note": format!("설교 포인트 {}장", pages.len() + 1) }))
}

/// 원고 파일 → 글 (hwp · hwpx · docx · txt · md · srt · vtt)
pub fn read_text_file(path: &std::path::Path) -> Result<String, String> {
    let name = path.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let buf = std::fs::read(path).map_err(|e| format!("파일을 읽지 못했습니다: {e}"))?;
    if ext == "srt" || ext == "vtt" {
        return Ok(String::from_utf8_lossy(&buf).trim_start_matches('\u{feff}').to_string());
    }
    match crate::bulletin::read_bulletin(&buf, &name)? {
        crate::bulletin::Input::Text(t) => Ok(t),
        _ => Err("PDF·사진에서는 글을 꺼낼 수 없습니다. 원고를 복사해 붙여 넣거나 hwp·docx·txt 파일을 고르세요.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const SAMPLE: &str = "제목: 복음의 능력\n본문: 롬 1:16-17\n설교자: 홍길동 사관\n\n서론입니다. 오늘 말씀을 봅니다.\n첫째, 복음은 하나님의 능력입니다. 구원을 주시는 능력이 복음에 있습니다.\n이 능력은 믿는 자에게 나타납니다.\n둘째로 복음에는 하나님의 의가 나타납니다. 우리를 의롭다 하십니다.\n셋째, 의인은 믿음으로 삽니다.\n믿음으로 시작해 믿음으로 걸어가는 삶입니다.\n적용: 이번 주 복음을 한 사람에게 전합시다.";

    #[test]
    fn rule_points() {
        let d = find_points(SAMPLE, &json!({})).unwrap();
        assert_eq!(d["found"], "lines");
        assert_eq!(d["title"], "복음의 능력");
        assert_eq!(d["verse"], "롬 1:16-17");
        let p = d["points"].as_array().unwrap();
        assert_eq!(p.len(), 3);
        assert_eq!(p[0]["heading"], "복음은 하나님의 능력입니다");
        assert_eq!(p[0]["body"], "구원을 주시는 능력이 복음에 있습니다.");
        assert_eq!(p[2]["heading"], "의인은 믿음으로 삽니다");
        assert_eq!(p[2]["body"], "믿음으로 시작해 믿음으로 걸어가는 삶입니다.");
        assert_eq!(d["application"], "이번 주 복음을 한 사람에게 전합시다.");
        let it = to_cue_item(&d, "word", true, true).unwrap();
        assert_eq!(it["type"], "title");
        assert_eq!(it["subtitle"], "롬 1:16-17 · 홍길동 사관");
        assert!(it["more"].as_str().unwrap().starts_with("첫째, 복음은 하나님의 능력입니다\n구원을"));
        assert_eq!(it["note"], "설교 포인트 5장");
        // '첫째날' 은 대지가 아니다
        assert!(head_mark("첫째날 아침에").is_none());
        assert_eq!(head_mark("첫 번째는, 사랑입니다").unwrap().2, "사랑입니다");
    }

    #[test]
    fn subtitles_and_ai() {
        let srt = "1\n00:00:01,000 --> 00:00:03,000\n첫째 기도는 호흡입니다\n\n2\n00:00:04,000 --> 00:00:06,000\n숨 쉬듯 기도합시다\n\n3\n00:00:07,000 --> 00:00:09,000\n둘째 기도는 대화입니다\n";
        let d = find_points(srt, &json!({ "title": "기도" })).unwrap();
        assert_eq!(d["found"], "sentences");
        assert_eq!(d["points"].as_array().unwrap().len(), 2);
        // 가짜 AI 응답
        let fake = json!({ "model": "x/y", "choices": [{ "message": { "content": "```json\n{\"title\":\"\",\"points\":[{\"heading\":\"하나.\",\"body\":\"몸\"}]}\n```" } }] });
        let r = parse_response(200, &fake, "m", &json!({ "title": "원래 제목" })).unwrap();
        assert_eq!(r["data"]["title"], "원래 제목");
        assert_eq!(r["data"]["points"][0]["heading"], "하나");
        assert!(parse_response(401, &json!({ "error": { "message": "bad key" } }), "m", &json!({})).unwrap_err().contains("401"));
        assert!(chat_prompt(SAMPLE, &json!({ "title": "T" }), "").unwrap().contains("설교 제목: T"));
        assert_eq!(clip("가나다라 마바사아 자차카타", 9), "가나다라 마바사아…");
    }
}
