//! [트랙 A] 한 PPT 를 여러 순서로 나누기 (Electron 판 pptsplit.js)
//!  - 규칙으로 (AI 없이): 짧은 '순서 제목' 슬라이드 · 찬송가 장 번호 · 성경 장절
//!  - API 키로 (오픈라우터 · OpenAI 호환, 주보 AI 와 같은 키·모델)
//!  - 채팅 AI 로 (키 없이): 프롬프트 → ChatGPT·Claude·Gemini → 답 붙여 넣기
//! 결과: parts = [{ a, b, name, kind }] (슬라이드 번호 1부터, 빠짐없이 · 겹치지 않게 · 차례대로)

use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::{json, Value};

pub const KINDS: [&str; 10] = ["hymn", "song", "bible", "reading", "prayer", "sermon", "notice", "offering", "video", "title"];
const KIND_NAMES: [&str; 10] = ["찬송가", "찬양", "성경", "교독문", "기도", "설교·강의", "광고·소식", "봉헌", "영상", "제목·그 밖"];
pub const DEFAULT_ENDPOINT: &str = "https://openrouter.ai/api/v1/chat/completions";
pub const DEFAULT_MODEL: &str = "anthropic/claude-haiku-5.5";
const MAX_PARTS: usize = 200;
const MAX_NAME: usize = 40;

#[derive(Clone, Debug, PartialEq)]
pub struct Part {
    pub a: usize,
    pub b: usize,
    pub name: String,
    pub kind: String,
}

impl Part {
    pub fn json(&self) -> Value {
        json!({ "a": self.a, "b": self.b, "name": self.name, "kind": self.kind })
    }
}

pub fn slide_lines(t: &str) -> Vec<String> {
    let s = t.replace("\r\n", "\n").replace(['\r', '\u{b}'], "\n");
    let sp = Regex::new(r"[ \t\u{a0}]+").unwrap();
    s.split('\n').map(|l| sp.replace_all(l, " ").trim().to_string()).filter(|l| !l.is_empty()).collect()
}
fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}
fn clip(s: &str, n: usize) -> String {
    let t = one_line(s);
    if t.chars().count() <= n { t } else { format!("{}…", t.chars().take(n - 1).collect::<String>().trim_end()) }
}
static TIDY: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[\s\-–—·•*※:：]+|[\s\-–—·•:：]+$").unwrap());
fn tidy_name(s: &str) -> String {
    clip(&TIDY.replace_all(s, ""), MAX_NAME)
}

static SECTIONS: Lazy<Vec<(Regex, &'static str)>> = Lazy::new(|| {
    [
        (r"^(?:예배\s*(?:로|에)?의?\s*부름|예배\s*부름|부름|묵도|입례|개회|개회\s*(?:예배|선언|사|기도|찬송)|여는\s*(?:예배|찬양)|환영|인사|소개|오프닝)", "title"),
        (r"^(?:교독문|교독|성시\s*교독)", "reading"),
        (r"^(?:새\s*찬송가|구세군\s*찬송가|찬송가|찬송|송영|경배\s*찬송)", "hymn"),
        (r"^(?:찬양|특송|특별\s*찬양|경배와\s*찬양|헌금\s*송|봉헌\s*송|찬양\s*대|찬양\s*팀|축가)", "song"),
        (r"^(?:성경\s*봉독|성경\s*말씀|본문\s*말씀|말씀\s*봉독|봉독|성경|본문)", "bible"),
        (r"^(?:대표\s*기도|합심\s*기도|통성\s*기도|중보\s*기도|주\s*기도문?|기도)", "prayer"),
        (r"^(?:신앙\s*고백|사도\s*신경|교리\s*(?:고백|낭독)?)", "title"),
        (r"^(?:설교|말씀|강의|특강|강연|발제|주제\s*강의|간증)", "sermon"),
        (r"^(?:헌금|봉헌)", "offering"),
        (r"^(?:광고|교회\s*소식|알림|공지|소식|행사\s*안내|안내)", "notice"),
        (r"^(?:축도|폐회|파송|축복\s*기도|마침|폐회\s*(?:기도|선언|사)|기념\s*촬영|시상|임명|보고|회의|점심|식사|휴식|만찬)", "title"),
    ]
    .into_iter()
    .map(|(r, k)| (Regex::new(r).unwrap(), k))
    .collect()
});
static PART_NO: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(?:제\s*)?([1-9]\d?)\s*부(?:\s|$|[.:：)])").unwrap());
static ROLE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?:사관|목사|장로|집사|권사|전도사|교역자|정위|참위|부위|참령|부령|정령|사령관|서기장|교수|박사|형제|자매|청년|위원장?|부장|회장|님|팀|찬양대|성가대|일동|다같이|회중|인도자)$").unwrap());
static HYMN_NO: Lazy<Regex> = Lazy::new(|| Regex::new(r"(새\s*찬송가|구세군\s*찬송가|구세군|찬송가|찬송)?\s*(?:제\s*)?(\d{1,3})\s*장").unwrap());
static REF: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?:^|\s)(창|출|레|민|신|수|삿|룻|삼상|삼하|왕상|왕하|대상|대하|스|느|에|욥|시|잠|전|아|사|렘|애|겔|단|호|욜|암|옵|욘|미|나|합|습|학|슥|말|마|막|눅|요|행|롬|고전|고후|갈|엡|빌|골|살전|살후|딤전|딤후|딛|몬|히|약|벧전|벧후|요일|요이|요삼|유|계|[가-힣]{2,6}(?:복음|기|서|편|록|가))\s*(\d{1,3})\s*[:장]\s*(\d{1,3})").unwrap());
static LEAD: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[\s\d.)\-–·•*※]+").unwrap());

fn is_hangul_start(s: &str) -> bool {
    s.chars().next().is_some_and(|c| ('\u{ac00}'..='\u{d7a3}').contains(&c))
}

/// 줄 머리가 순서 이름인가 ('설교' · '설교: 제목' · '찬송 288장' · '대표기도 홍길동 사관')
fn section_head(h: &str, re: &Regex) -> bool {
    let Some(m) = re.find(h) else { return false };
    let after = &h[m.end()..];
    if is_hangul_start(after) {
        return false;
    }
    let rest = after.trim();
    if rest.is_empty() || rest.starts_with([':', '：', '·', '-', '–', '—', '(', '（', '[', '|', '/']) || rest.starts_with(|c: char| c.is_ascii_digit()) {
        return true;
    }
    let toks: Vec<&str> = rest.split_whitespace().collect();
    if toks.len() == 1 && rest.chars().count() <= 12 {
        return true;
    }
    toks.len() <= 3 && ROLE.is_match(rest)
}

/// 슬라이드 하나 → (kind, key, name) — 새 순서가 시작될 만한 슬라이드
pub fn classify(lines: &[String]) -> Option<(String, String, String)> {
    let head = lines.first()?;
    let total: usize = lines.iter().map(|l| l.chars().filter(|c| !c.is_whitespace()).count()).sum();
    let short = total <= 60 && lines.len() <= 4;
    let top = lines.iter().take(2).cloned().collect::<Vec<_>>().join(" ");
    if let Some(m) = HYMN_NO.captures(&top) {
        let whole = m.get(0).unwrap();
        let after_raw = &top[whole.end()..];
        // '장' 바로 뒤에 한글이 이어지면 (장로 등) 아니다 · '3장 16절' 은 성경
        let next_is_hangul = is_hangul_start(after_raw);
        let verse_after = Regex::new(r"^\s*\d{1,3}\s*절").unwrap().is_match(after_raw);
        let head_num = Regex::new(r"^\s*(?:제\s*)?\d{1,3}\s*장").unwrap().is_match(head);
        if !next_is_hangul && !verse_after && !REF.is_match(&top) && (m.get(1).is_some() || head_num || short) {
            let after = after_raw.trim_start_matches(|c: char| c.is_whitespace() || ".:：-–".contains(c));
            let book = m.get(1).map(|b| b.as_str().split_whitespace().collect::<Vec<_>>().join(" ")).unwrap_or_default();
            let prefix = if !book.is_empty() && book != "찬송" { format!("{} ", if book == "구세군" { "구세군 찬송가".to_string() } else { book }) } else { "찬송 ".into() };
            let first = Regex::new(r"\s{2,}| / ").unwrap().split(after).next().unwrap_or("").to_string();
            let name = tidy_name(&format!("{prefix}{}장{}", &m[2], if first.is_empty() { String::new() } else { format!(" {first}") }));
            return Some(("hymn".into(), format!("h{}", &m[2]), name));
        }
    }
    if !short {
        return None;
    }
    if let Some(m) = PART_NO.captures(head) {
        return Some(("title".into(), format!("p{}", &m[1]), tidy_name(&lines.iter().take(2).cloned().collect::<Vec<_>>().join(" "))));
    }
    let h = LEAD.replace(head, "").to_string();
    for (re, kind) in SECTIONS.iter() {
        if !section_head(&h, re) {
            continue;
        }
        let name = tidy_name(&lines.iter().take(2).cloned().collect::<Vec<_>>().join(" · "));
        let key: String = h.chars().filter(|c| !c.is_whitespace()).take(12).collect();
        return Some((kind.to_string(), format!("{kind}:{key}"), name));
    }
    if let Some(m) = REF.captures(&top) {
        if total <= 40 {
            return Some(("bible".into(), format!("b{}{}", &m[1], &m[2]), tidy_name(&format!("성경봉독 {}", m[0].trim()))));
        }
    }
    None
}

/// 규칙으로 나누기. texts[k] = k+1 번째 슬라이드 글자
pub fn rule_split(texts: &[String], title: &str) -> Result<Vec<Part>, String> {
    let n = texts.len();
    if n == 0 {
        return Err("슬라이드가 없습니다.".into());
    }
    let mut parts: Vec<(Part, String)> = vec![];
    for (k, t) in texts.iter().enumerate() {
        let c = classify(&slide_lines(t));
        let start_new = match (&parts.last(), &c) {
            (None, _) => true,
            (Some((cur, key)), Some((kind, ckey, _))) => ckey != key && !(kind == "bible" && cur.kind == "bible"),
            _ => false,
        };
        if start_new {
            let (kind, key, name) = c.unwrap_or_default();
            parts.push((Part { a: k + 1, b: k + 1, name, kind }, key));
        } else {
            parts.last_mut().unwrap().0.b = k + 1;
        }
    }
    let mut out: Vec<Part> = parts.into_iter().map(|p| p.0).collect();
    for p in out.iter_mut() {
        if p.name.is_empty() {
            p.name = slide_lines(&texts[p.a - 1]).first().map(|f| tidy_name(f)).unwrap_or_else(|| format!("{} {}", if title.is_empty() { "PPT" } else { title }, if p.a == p.b { p.a.to_string() } else { format!("{}-{}", p.a, p.b) }));
        }
        if p.kind.is_empty() {
            p.kind = "title".into();
        }
    }
    let list: Vec<Value> = out.iter().map(|p| p.json()).collect();
    clean_parts(&list, n, title)
}

/// AI 답이나 고친 묶음 → 빠짐없이 · 겹치지 않게 · 차례대로
pub fn clean_parts(list: &[Value], count: usize, title: &str) -> Result<Vec<Part>, String> {
    let n = count;
    if n == 0 {
        return Err("슬라이드 수를 알 수 없습니다.".into());
    }
    let num = |v: &Value| -> Option<i64> { v.as_f64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok())).map(|x| x.round() as i64) };
    let rng = Regex::new(r"^\s*(\d+)\s*(?:[-~]\s*(\d+))?\s*$").unwrap();
    let mut parts: Vec<Part> = list
        .iter()
        .filter_map(|p| {
            if !p.is_object() {
                return None;
            }
            let pick = |ks: &[&str]| ks.iter().find_map(|k| num(&p[*k]));
            let mut a = pick(&["a", "from", "start", "first"]);
            let mut b = pick(&["b", "to", "end", "last"]);
            if a.is_none() {
                if let Some(m) = p["range"].as_str().and_then(|r| rng.captures(r)) {
                    a = m[1].parse().ok();
                    b = m.get(2).or(m.get(1)).and_then(|x| x.as_str().parse().ok());
                }
            }
            if a.is_none() {
                if let Some(s) = p["slides"].as_array() {
                    let v: Vec<i64> = s.iter().filter_map(num).collect();
                    if !v.is_empty() {
                        a = v.iter().min().copied();
                        b = v.iter().max().copied();
                    }
                }
            }
            let mut a = a?;
            let mut b = b.unwrap_or(a);
            if a > b {
                std::mem::swap(&mut a, &mut b);
            }
            let a = a.max(1) as usize;
            let b = (b.max(0) as usize).min(n);
            if a > n || b < a {
                return None;
            }
            let k = p["kind"].as_str().or(p["type"].as_str()).filter(|k| KINDS.contains(k)).unwrap_or("title");
            let name = ["name", "title", "label"].iter().find_map(|x| p[*x].as_str()).unwrap_or("");
            Some(Part { a, b, name: tidy_name(name), kind: k.into() })
        })
        .collect();
    parts.sort_by(|x, y| x.a.cmp(&y.a).then(y.b.cmp(&x.b)));
    let mut out: Vec<Part> = vec![];
    for mut p in parts {
        if let Some(prev) = out.last() {
            if p.a <= prev.b {
                p.a = prev.b + 1;
            }
        }
        if p.a > p.b {
            continue;
        }
        out.push(p);
    }
    if out.is_empty() {
        let t = tidy_name(title);
        return Ok(vec![Part { a: 1, b: n, name: if t.is_empty() { "전체".into() } else { t }, kind: "title".into() }]);
    }
    for i in 1..out.len() {
        if out[i].a > out[i - 1].b + 1 {
            out[i - 1].b = out[i].a - 1;
        }
    }
    if out[0].a > 1 {
        let e = out[0].a - 1;
        out.insert(0, Part { a: 1, b: e, name: "처음".into(), kind: "title".into() });
    }
    out.last_mut().unwrap().b = n;
    if out.len() > MAX_PARTS {
        out.truncate(MAX_PARTS);
        out.last_mut().unwrap().b = n;
    }
    for p in out.iter_mut() {
        if p.name.is_empty() {
            let t = tidy_name(title);
            p.name = format!("{} {}", if t.is_empty() { "PPT".into() } else { t }, if p.a == p.b { p.a.to_string() } else { format!("{}-{}", p.a, p.b) });
        }
    }
    Ok(out)
}

const FORMAT: &str = "{\n  \"parts\": [\n    { \"from\": 1, \"to\": 2, \"title\": \"표지\", \"kind\": \"title\" },\n    { \"from\": 3, \"to\": 8, \"title\": \"개회 찬송 (새찬송가 8장)\", \"kind\": \"hymn\" },\n    { \"from\": 9, \"to\": 9, \"title\": \"대표기도 · 홍길동 사관\", \"kind\": \"prayer\" }\n  ]\n}";

pub fn system() -> String {
    let kinds = KINDS.iter().zip(KIND_NAMES).map(|(k, n)| format!("{k}({n})")).collect::<Vec<_>>().join(" · ");
    format!("당신은 한국 교회의 예배 화면(PPT) 담당자입니다. 예배나 행사(수련회 · 사관회 · 세미나 등) 전체를 한 파일에 담은 PowerPoint 를 받아, 예배 송출 프로그램의 큐시트 순서로 나눕니다. 송출 담당자는 순서마다 한 번에 이어서 넘깁니다.\n답은 아래 형식의 JSON 하나만 씁니다.\n\n형식:\n{FORMAT}\n\n원칙:\n- 슬라이드 번호는 1부터입니다. 모든 슬라이드를 빠짐없이, 겹치지 않게, 차례대로 나눕니다 (다음 순서의 from = 앞 순서의 to + 1).\n- 한 순서는 진행에서 한 덩어리입니다: 찬송 한 곡, 찬양 한 곡, 기도, 성경봉독, 교독문, 설교(또는 강의) 전체, 광고, 영상 하나, 1부·2부 제목, 축도 등.\n- 같은 곡의 여러 절·후렴은 한 순서입니다. 곡이 바뀌면 새 순서입니다. 설교 중의 본문·요점 슬라이드는 설교 순서에 넣습니다.\n- 순서 사이의 빈(검은) 슬라이드·구분 슬라이드는 앞 순서 끝에 붙입니다. 맨 앞 표지는 '표지' 순서로 둡니다.\n- title 은 큐시트에 보일 짧은 이름(25자 안쪽)입니다. 슬라이드에 있는 순서 이름·곡 제목·장 번호·담당자를 그대로 씁니다. 슬라이드에 없는 이름·곡·장절을 지어내지 않습니다.\n- kind 는 {kinds} 중 하나입니다.\n- 모아보기 그림이 있으면 칸 왼쪽 위의 번호가 슬라이드 번호입니다. 글자와 그림을 함께 보고 판단합니다.")
}

pub fn slide_list(texts: &[String]) -> String {
    let n = texts.len().max(1);
    let per = (60000 / n).saturating_sub(8).clamp(40, 240);
    texts.iter().enumerate().map(|(k, t)| {
        let l = slide_lines(t);
        format!("[{}] {}", k + 1, if l.is_empty() { "(글자 없음)".into() } else { clip(&l.join(" / "), per) })
    }).collect::<Vec<_>>().join("\n")
}

fn user_text(texts: &[String], title: &str, extra: &str, sheets: usize) -> String {
    let empty = texts.iter().filter(|t| slide_lines(t).is_empty()).count();
    let mut head = vec![];
    if !title.is_empty() {
        head.push(format!("PPT 파일: {title}"));
    }
    head.push(format!("슬라이드 {}장{}.", texts.len(), if empty > 0 { format!(" (그 가운데 {empty}장은 글자가 없습니다{})", if sheets > 0 { " — 모아보기 그림에서 보세요" } else { "" }) } else { String::new() }));
    if sheets > 0 {
        head.push(format!("모아보기 그림 {sheets}장을 함께 보냅니다 (칸마다 왼쪽 위에 슬라이드 번호)."));
    }
    if !extra.is_empty() {
        head.push(format!("추가 요청: {extra}"));
    }
    format!("{}\n\n[슬라이드 글자]\n{}", head.join("\n"), slide_list(texts))
}

/// 채팅 AI 에 붙여 넣을 프롬프트
pub fn chat_prompt(texts: &[String], title: &str, extra: &str, sheets: usize) -> Result<String, String> {
    if texts.is_empty() {
        return Err("슬라이드가 없습니다.".into());
    }
    Ok(format!("{}\n\n답하는 법:\n- JSON 하나만 ```json 코드 블록으로 답합니다. 앞뒤 설명은 쓰지 않습니다.{}\n\n{}", system(), if sheets > 0 { "\n- 함께 올린 모아보기 그림도 봅니다." } else { "" }, user_text(texts, title, extra, sheets)))
}

fn parse_json(t: &str) -> Option<Value> {
    let t = t.trim().trim_start_matches("```json").trim_start_matches("```JSON").trim_start_matches("```").trim_end_matches("```").trim();
    serde_json::from_str(t).ok().or_else(|| {
        let (a, b) = (t.find('{')?, t.rfind('}')?);
        if b > a { serde_json::from_str(&t[a..=b]).ok() } else { None }
    })
}

/// 글에서 parts JSON 을 찾아 정리
pub fn read_parts(text: &str, count: usize, title: &str) -> Option<Vec<Part>> {
    let t = text.trim();
    if !t.contains('{') && !t.contains('[') {
        return None;
    }
    let re = Regex::new(r"```(?:json|JSON)?\s*([\s\S]*?)```").unwrap();
    let mut cands: Vec<String> = re.captures_iter(t).map(|m| m[1].to_string()).collect();
    cands.push(t.to_string());
    for b in cands {
        let o = parse_json(&b).or_else(|| {
            let (a, z) = (b.find('[')?, b.rfind(']')?);
            if z > a { serde_json::from_str(&b[a..=z]).ok() } else { None }
        });
        let Some(o) = o else { continue };
        let list = if o.is_array() { o } else { ["parts", "items", "cues", "순서"].iter().map(|k| o[*k].clone()).find(|v| v.is_array()).unwrap_or(Value::Null) };
        let Some(arr) = list.as_array().filter(|a| !a.is_empty()) else { continue };
        if let Ok(p) = clean_parts(arr, count, title) {
            if !p.is_empty() {
                return Some(p);
            }
        }
    }
    None
}

/// 채팅 AI 답 → parts
pub fn parse_answer(text: &str, count: usize, title: &str) -> Result<Vec<Part>, String> {
    read_parts(text, count, title).ok_or_else(|| "AI 답에서 순서 나누기 JSON 을 찾지 못했습니다. AI 에게 \"JSON 코드 블록 하나로 다시 답해 줘\" 라고 해 보세요.".into())
}

/// 오픈라우터(또는 OpenAI 호환)로 나누기 — 오래 걸리니 코어 잠금 밖(다른 스레드)에서 부른다
/// sheets: [(from, to, data:image/…)] 모아보기 그림
pub fn ask_split(api_key: &str, model: &str, endpoint: &str, texts: &[String], title: &str, extra: &str, sheets: &[(usize, usize, String)]) -> Result<Value, String> {
    if api_key.is_empty() {
        return Err("오픈라우터 API 키가 없습니다. 주보 AI 설정에서 넣거나, 채팅 AI 를 쓰세요.".into());
    }
    if texts.is_empty() {
        return Err("슬라이드가 없습니다.".into());
    }
    let imgs: Vec<&(usize, usize, String)> = sheets.iter().filter(|s| s.2.starts_with("data:image/")).take(40).collect();
    let ut = user_text(texts, title, extra, imgs.len());
    let user: Value = if imgs.is_empty() {
        json!(ut)
    } else {
        let mut c = vec![json!({ "type": "text", "text": ut })];
        for s in imgs {
            c.push(json!({ "type": "text", "text": format!("모아보기: 슬라이드 {}-{}", s.0, s.1) }));
            c.push(json!({ "type": "image_url", "image_url": { "url": s.2 } }));
        }
        json!(c)
    };
    let model = if model.is_empty() { DEFAULT_MODEL } else { model };
    let body = json!({ "model": model, "messages": [{ "role": "system", "content": system() }, { "role": "user", "content": user }], "temperature": 0.1, "response_format": { "type": "json_object" } });
    let resp = ureq::post(if endpoint.is_empty() { DEFAULT_ENDPOINT } else { endpoint })
        .header("Authorization", &format!("Bearer {api_key}"))
        .header("HTTP-Referer", "https://github.com/hahadark/WorshipPresenter")
        .header("X-Title", "Worship Presenter")
        .config()
        .http_status_as_error(false)
        .build()
        .send_json(&body)
        .map_err(|e| format!("AI 에 연결하지 못했습니다: {e}"))?;
    let status = resp.status();
    let v: Value = resp.into_body().read_json().unwrap_or(Value::Null);
    if !status.is_success() {
        return Err(format!("오픈라우터 오류 ({}): {}", status.as_u16(), v["error"]["message"].as_str().unwrap_or("")));
    }
    let msg = &v["choices"][0]["message"]["content"];
    let raw = match msg {
        Value::String(s) => s.clone(),
        Value::Array(a) => a.iter().filter_map(|c| c["text"].as_str()).collect(),
        _ => return Err("AI 응답이 비었습니다.".into()),
    };
    let parts = read_parts(&raw, texts.len(), title).ok_or("AI 답에서 순서 나누기(parts)를 찾지 못했습니다. 다시 해 보거나 다른 모델을 고르세요.")?;
    Ok(json!({ "parts": parts.iter().map(|p| p.json()).collect::<Vec<_>>(), "usage": v["usage"], "model": v["model"].as_str().unwrap_or(model) }))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }
    #[test]
    fn rules() {
        let t = s(&["2026 사관회", "개회 찬송\n새찬송가 8장", "거룩 거룩 거룩 전능하신 주님", "이른 아침 우리 주를 찬송합니다", "대표기도 홍길동 사관", "성경봉독\n요한복음 3:16", "하나님이 세상을 이처럼 사랑하사", "광고"]);
        let p = rule_split(&t, "").unwrap();
        let ab: Vec<(usize, usize)> = p.iter().map(|x| (x.a, x.b)).collect();
        assert_eq!(ab, vec![(1, 1), (2, 4), (5, 5), (6, 7), (8, 8)]);
        assert_eq!(p[1].kind, "hymn");
        assert_eq!(p[2].kind, "prayer");
        assert_eq!(p[4].kind, "notice");
        // 가사 안의 낱말에는 속지 않음
        assert!(classify(&s(&["찬양하세 주님을"])).is_none());
    }
    #[test]
    fn clean_and_parse() {
        let p = parse_answer("```json\n{\"parts\":[{\"from\":2,\"to\":3,\"title\":\"찬송\",\"kind\":\"hymn\"},{\"from\":3,\"to\":5,\"title\":\"설교\",\"kind\":\"sermon\"}]}\n```", 6, "").unwrap();
        let ab: Vec<(usize, usize)> = p.iter().map(|x| (x.a, x.b)).collect();
        assert_eq!(ab, vec![(1, 1), (2, 3), (4, 6)]);
        assert_eq!(p[0].name, "처음");
        assert!(chat_prompt(&s(&["a"]), "", "", 0).unwrap().contains("[1] a"));
    }
}
