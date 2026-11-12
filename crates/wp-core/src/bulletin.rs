//! [트랙 D] 주보 → 큐시트 (Electron 판 bulletin.js)
//! 파일 글자 꺼내기(hwp · hwpx · docx · txt, pdf·사진은 AI 로) · 규칙 파서 · AI(오픈라우터)·채팅 AI 프롬프트 · 계획 → 큐시트 항목

use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::{json, Value};
use std::io::Read;

pub const RECOMMENDED: [(&str, &str); 4] = [
    ("anthropic/claude-haiku-5.5", "Claude Haiku 5.5 — 추천 (빠르고 쌈, 한국어·표 정리 잘함)"),
    ("openai/gpt-6-luna", "GPT-6 Luna — 비슷한 가격대"),
    ("google/gemini-3.8-flash", "Gemini 3.8 Flash — PDF·사진 주보에 강함"),
    ("anthropic/claude-sonnet-5.5", "Claude Sonnet 5.5 — 복잡한 주보용 (더 정확, 약 20배 비쌈)"),
];
pub const DEFAULT_MODEL: &str = "anthropic/claude-haiku-5.5";
pub const DEFAULT_ENDPOINT: &str = "https://openrouter.ai/api/v1/chat/completions";

// ---------------- 글자 꺼내기 ----------------
/// 주보 파일에서 읽은 것: 글 또는 (AI 가 직접 읽을) 파일
#[derive(Debug, Clone)]
pub enum Input {
    Text(String),
    File { mime: String, base64: String, name: String },
}

/// 한글(HWP 5) 본문 글자
pub fn hwp_text(buf: &[u8]) -> Result<String, String> {
    let mut c = cfb::CompoundFile::open(std::io::Cursor::new(buf)).map_err(|_| "한글 파일 형식이 아닙니다.".to_string())?;
    let mut hdr = vec![];
    c.open_stream("/FileHeader").map_err(|_| "한글 파일 형식이 아닙니다.".to_string())?.read_to_end(&mut hdr).map_err(|e| e.to_string())?;
    if hdr.len() < 40 {
        return Err("한글 파일 형식이 아닙니다.".into());
    }
    let flags = u32::from_le_bytes([hdr[36], hdr[37], hdr[38], hdr[39]]);
    if flags & 2 != 0 {
        return Err("암호가 걸린 한글 파일입니다.".into());
    }
    let compressed = flags & 1 != 0;
    let mut names: Vec<String> = c
        .walk_storage("/BodyText")
        .map(|it| it.filter(|e| e.is_stream()).map(|e| e.path().to_string_lossy().replace('\\', "/")).collect())
        .unwrap_or_default();
    names.retain(|n| Regex::new(r"/Section\d+$").unwrap().is_match(n));
    names.sort_by_key(|n| n.rsplit("Section").next().and_then(|x| x.parse::<u32>().ok()).unwrap_or(0));
    const EXT: [u16; 21] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 11, 12, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23];
    let mut lines = vec![];
    for n in names {
        let mut b = vec![];
        if c.open_stream(&n).and_then(|mut s| s.read_to_end(&mut b)).is_err() {
            continue;
        }
        if compressed {
            b = miniz_oxide::inflate::decompress_to_vec(&b).map_err(|_| "한글 파일 압축을 풀지 못했습니다.".to_string())?;
        }
        let mut o = 0usize;
        let u32at = |b: &[u8], i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        while o + 4 <= b.len() {
            let h = u32at(&b, o);
            o += 4;
            let tag = h & 0x3ff;
            let mut size = (h >> 20) as usize;
            if size == 0xfff {
                if o + 4 > b.len() { break; }
                size = u32at(&b, o) as usize;
                o += 4;
            }
            if tag == 67 {
                let mut t = String::new();
                let end = (o + size).min(b.len());
                let mut i = o;
                let mut units: Vec<u16> = vec![];
                while i + 1 < end {
                    let ch = u16::from_le_bytes([b[i], b[i + 1]]);
                    if EXT.contains(&ch) {
                        i += 16;
                        if ch == 9 { units.push(9); }
                        continue;
                    }
                    if ch == 13 || ch == 10 { units.push(10) } else if ch >= 32 { units.push(ch) }
                    i += 2;
                }
                t.push_str(&String::from_utf16_lossy(&units));
                if !t.trim().is_empty() {
                    lines.push(t.trim().to_string());
                }
            }
            o += size;
        }
    }
    Ok(lines.join("\n"))
}

fn xml_unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

/// hwpx · docx (압축 XML) 글자
pub fn zip_xml_text(buf: &[u8], kind: &str) -> Result<String, String> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(buf)).map_err(|_| "압축 파일을 열지 못했습니다.".to_string())?;
    let mut files: Vec<String> = if kind == "hwpx" {
        let re = Regex::new(r"(?i)^Contents/section\d+\.xml$").unwrap();
        z.file_names().filter_map(|n| n.ok().map(|x| x.to_string())).filter(|n| re.is_match(n)).collect()
    } else {
        vec!["word/document.xml".into()]
    };
    files.sort();
    let (para, text) = if kind == "hwpx" { (r"<hp:p[ >][\s\S]*?</hp:p>", r"<hp:t[^>]*>([^<]*)</hp:t>") } else { (r"<w:p[ >][\s\S]*?</w:p>", r"<w:t[^>]*>([^<]*)</w:t>") };
    let (para, text) = (Regex::new(para).unwrap(), Regex::new(text).unwrap());
    let mut out = vec![];
    for f in files {
        let Ok(mut e) = z.by_name(&f) else { continue };
        let mut xml = String::new();
        if e.read_to_string(&mut xml).is_err() { continue; }
        for p in para.find_iter(&xml) {
            let t: String = text.captures_iter(p.as_str()).map(|m| xml_unescape(&m[1])).collect();
            if !t.trim().is_empty() {
                out.push(t.trim().to_string());
            }
        }
    }
    Ok(out.join("\n"))
}

/// 주보 파일 → 글 또는 파일(PDF·그림은 AI 가 직접 읽음)
pub fn read_bulletin(buf: &[u8], name: &str) -> Result<Input, String> {
    let ext = std::path::Path::new(name).extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "hwp" => Ok(Input::Text(hwp_text(buf)?)),
        "hwpx" => Ok(Input::Text(zip_xml_text(buf, "hwpx")?)),
        "docx" => Ok(Input::Text(zip_xml_text(buf, "docx")?)),
        "txt" | "md" | "srt" | "vtt" => Ok(Input::Text(String::from_utf8_lossy(buf).trim_start_matches('\u{feff}').to_string())),
        "pdf" | "png" | "jpg" | "jpeg" | "webp" => {
            let mime = match ext.as_str() { "pdf" => "application/pdf", "png" => "image/png", "webp" => "image/webp", _ => "image/jpeg" };
            let base = std::path::Path::new(name).file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
            Ok(Input::File { mime: mime.into(), base64: crate::dpapi::b64(buf), name: base })
        }
        _ => Err("읽을 수 없는 파일입니다 (hwp · hwpx · docx · pdf · txt · 사진).".into()),
    }
}

// ---------------- AI 프롬프트 ----------------
pub const FORMAT: &str = r#"{
  "name": "큐시트 이름 (예: 2026-10-11 주일성결예배)",
  "serviceDate": "YYYY-MM-DD 또는 빈 문자열",
  "startTime": "HH:MM (예배 시작 시각, 모르면 빈 문자열)",
  "items": [
    { "type": "title|hymn|bible|song|text|blank",
      "title": "화면 제목 (예: 묵도, 기도, 설교)",
      "subtitle": "제목 화면 아래 줄 (예: 김명수 정교 / 설교 제목 · 설교자)",
      "hymnNumber": 31, "hymnbook": "새찬송가 또는 구세군",
      "ref": "성경 장절 (예: 마 6:9-13)",
      "body": "공지 내용 (슬라이드는 '---' 줄로 나눔)",
      "note": "조작자 메모 (일어서기 *, 담당자 등)",
      "duration": 0 }
  ]
}"#;
pub const RULES: &str = "규칙:
- 찬송(가) 순서는 type \"hymn\" 과 hymnNumber. '예배로의 부름'처럼 찬송가를 부르는 순서도 hymn (title 은 '예배로의 부름').
- 성경봉독·본문은 type \"bible\" 과 ref (책 이름은 한국어 약칭 가능).
- 찬양팀·찬양대처럼 곡이 정해지지 않은 찬양은 type \"title\" (subtitle 에 찬양팀 이름).
- 교회소식·광고는 type \"text\" 하나로, body 에 항목마다 '---' 로 나눠 요약 (한 장에 3~4줄).
- 설교는 type \"title\", subtitle 에 \"“설교 제목” · 설교자\".
- 일어서는 순서(* 표시)는 note 에 \"*일어서서\".
- 기도·봉헌·축도 등 나머지는 type \"title\" 과 담당자 subtitle.
- 마지막에 { \"type\": \"blank\", \"title\": \"예배 후\" } 를 넣는다.
- 주보에 없는 내용은 지어내지 않는다.";

pub fn system_prompt() -> String {
    format!("너는 한국 교회 예배 방송실을 돕는 도우미다. 주보에서 '예배 순서'(주일 낮/오전 예배를 우선)를 찾아 송출 큐시트 JSON 으로 만든다.\n반드시 아래 JSON 하나만 출력한다 (설명·코드블록 없이).\n{FORMAT}\n{RULES}")
}

pub const SAMPLE_NAME: &str = "워십프리젠터-큐시트-샘플.json";

/// 채팅 AI 에 형식을 보여 주는 샘플 (가상 이름·예시)
pub fn sample() -> Value {
    json!({
        "_안내": "워십프리젠터 큐시트 형식 예시입니다. 주보의 예배 순서로 이 형식 그대로 새로 만들어 주세요. (이 파일의 내용은 예시일 뿐입니다)",
        "name": "2026-10-18 주일 예배", "serviceDate": "2026-10-18", "startTime": "11:00",
        "items": [
            { "type": "title", "title": "예배로 부름", "subtitle": "인도: 홍길동 목사", "note": "" },
            { "type": "hymn", "title": "찬송", "hymnNumber": 8, "hymnbook": "새찬송가", "note": "*일어서서" },
            { "type": "title", "title": "신앙고백", "subtitle": "사도신경", "note": "*일어서서" },
            { "type": "title", "title": "대표기도", "subtitle": "김철수 장로" },
            { "type": "bible", "title": "성경봉독", "ref": "시 23:1-6" },
            { "type": "title", "title": "찬양", "subtitle": "찬양대" },
            { "type": "title", "title": "설교", "subtitle": "“선한 목자” · 홍길동 목사", "duration": 30 },
            { "type": "hymn", "title": "찬송", "hymnNumber": 310, "hymnbook": "새찬송가" },
            { "type": "text", "title": "교회 소식", "body": "다음 주일은 추수감사주일로 지킵니다\n---\n수요예배: 저녁 7시 30분 본당\n---\n새가족 환영회: 예배 후 2층 교육관" },
            { "type": "title", "title": "축도", "subtitle": "홍길동 목사", "note": "*일어서서" },
            { "type": "blank", "title": "예배 후" }
        ]
    })
}

/// 채팅 AI(ChatGPT·Claude·Gemini) 에 붙여 넣을 프롬프트
pub fn chat_prompt(extra: &str) -> String {
    let mut s = format!("[워십프리젠터 큐시트 만들기]\n너는 한국 교회 예배 방송실을 돕는 도우미다. 첨부한 주보에서 '예배 순서'(주일 낮/오전 예배를 우선)를 찾아, 송출 프로그램 워십프리젠터 가 불러올 큐시트 JSON 을 만들어 줘.\n첨부한 샘플 파일({SAMPLE_NAME})과 똑같은 형식으로 만든다. 샘플의 내용(이름·장절·찬송 번호)은 예시일 뿐이니 쓰지 말고, 주보 내용으로만 채운다.\n\n형식:\n{FORMAT}\n{RULES}\n- \"_안내\" 항목은 넣지 않는다.\n\n답하는 법:\n- JSON 하나만 ```json 코드 블록으로 답한다. 앞뒤 설명은 짧게.\n- 파일을 만들 수 있으면 '워십프리젠터-큐시트.json' 파일로 줘도 된다.\n- 주보에서 읽기 어려운 부분은 지어내지 말고 note 에 \"확인 필요\" 라고 적는다.");
    if !extra.trim().is_empty() {
        s.push_str(&format!("\n\n추가 요청: {}", extra.trim()));
    }
    s
}

/// 채팅 AI 바로 가기 주소
pub const CHAT_LINKS: [(&str, &str); 3] = [("챗GPT", "https://chatgpt.com/"), ("클로드", "https://claude.ai/new"), ("제미나이", "https://gemini.google.com/app")];

/// 응답 글에서 JSON 꺼내기 (코드블록·앞뒤 설명이 붙어도)
pub fn parse_json(text: &str) -> Result<Value, String> {
    let t = text.trim();
    let t = Regex::new(r"(?i)^```(?:json)?\s*").unwrap().replace(t, "").to_string();
    let t = Regex::new(r"```\s*$").unwrap().replace(&t, "").to_string();
    if let Ok(v) = serde_json::from_str::<Value>(&t) {
        return Ok(v);
    }
    if let (Some(a), Some(b)) = (t.find('{'), t.rfind('}')) {
        if b > a {
            if let Ok(v) = serde_json::from_str::<Value>(&t[a..=b]) {
                return Ok(v);
            }
        }
    }
    Err("AI 응답을 해석하지 못했습니다.".into())
}

/// 채팅 AI 의 답 → (계획, 샘플 그대로인가)
pub fn parse_chat_answer(text: &str) -> Result<(Value, bool), String> {
    let t = text.trim_start_matches('\u{feff}').trim();
    if t.is_empty() {
        return Err("AI 답을 붙여 넣거나 결과 파일을 불러오세요.".into());
    }
    let re = Regex::new(r"```(?:json|JSON)?\s*([\s\S]*?)```").unwrap();
    let mut cands: Vec<String> = re.captures_iter(t).map(|m| m[1].to_string()).collect();
    cands.push(t.to_string());
    let mut plan = None;
    for b in cands {
        if let Ok(p) = parse_json(&b) {
            if p["items"].is_array() || p.is_array() || p["cue"]["items"].is_array() {
                plan = Some(p);
                break;
            }
        }
    }
    let mut plan = plan.ok_or("AI 답에서 큐시트 JSON 을 찾지 못했습니다. AI 에게 \"JSON 코드 블록 하나로 다시 답해 줘\" 라고 해 보세요.")?;
    if plan.is_array() {
        plan = json!({ "items": plan });
    }
    if plan["cue"]["items"].is_array() {
        plan = plan["cue"].clone();
    }
    if let Some(o) = plan.as_object_mut() {
        o.remove("_안내");
    }
    if plan["items"].as_array().is_none_or(|a| a.is_empty()) {
        return Err("예배 순서(items)가 비어 있습니다.".into());
    }
    let is_sample = plan["items"] == sample()["items"];
    Ok((plan, is_sample))
}

// ---------------- 규칙으로 바로 (AI 없이) ----------------
static ORDER_SPLIT: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s*(?:/|\||·|•|→|➔)\s*").unwrap());
static BOOKS_KO: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(창|출|레|민|신|수|삿|룻|삼상|삼하|왕상|왕하|대상|대하|스|느|에|욥|시|잠|전|아|사|렘|애|겔|단|호|욜|암|옵|욘|미|나|합|습|학|슥|말|마|막|눅|요|행|롬|고전|고후|갈|엡|빌|골|살전|살후|딤전|딤후|딛|몬|히|약|벧전|벧후|요일|요이|요삼|유|계)").unwrap());
static DATE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(\d{4})[.\-/년]\s*(\d{1,2})[.\-/월]\s*(\d{1,2})일?").unwrap());
static TIME: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?:^|\D)([01]?\d|2[0-3])[:시]\s*([0-5]\d)분?(\d)?").unwrap());
static NUMBERING: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(?:\d+[.)]|[①-⑳]|[-•*·]\s)\s*").unwrap());
static STAND: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\*|[*※]\s*$|\(일어서서\)|일어서서|[*※]\s*일어").unwrap());
static STAND_STRIP: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\*+\s*|\s*[*※]+\s*$|\s*\(?일어서서\)?").unwrap());
static MEMO: Lazy<Regex> = Lazy::new(|| Regex::new(r"[(（]([^()（）]*)[)）]\s*$").unwrap());
static READING: Lazy<Regex> = Lazy::new(|| Regex::new(r"교독문\s*(?:제\s*)?(\d+)\s*(?:번|장)?\s*(.*)$").unwrap());
static HYMN: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(.*?)(새\s*찬송가|구세군\s*찬송가|구세군|통일\s*찬송가|찬송가|찬송)?\s*(?:제\s*)?(\d{1,3})\s*장\s*(.*)$").unwrap());
static BIBLE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(성경\s*봉독|본문\s*말씀|본문|말씀\s*봉독|봉독|성경)\s*[:：\-–]?\s*(.+)$").unwrap());
static NOTICE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(교회\s*소식|광고|알림|공지)\s*[:：\-–]?\s*(.*)$").unwrap());
static SONG: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(찬양|특송|헌금\s*송|봉헌\s*송|경배와\s*찬양|찬양\s*콘티|콘티)\s*[:：\-–]?\s*(.+)$").unwrap());
static SERMON: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(설교|말씀)\s*[:：\-–]?\s*(.*)$").unwrap());
static QUOTE: Lazy<Regex> = Lazy::new(|| Regex::new(r#"^[“"'‘「『]?(.+?)[”"'’」』]\s*[,·\-–]?\s*(.*)$"#).unwrap());
static SP1: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(.+?)\s*(?:[:：]|\s[-–]\s|\t|\s{2,})\s*(.+)$").unwrap());
static SP2: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(\S+)\s+(\S.*)$").unwrap());

fn ws_strip(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

/// 붙여 넣은 예배 순서 글 → 큐시트 계획 (AI 와 같은 JSON). looks_like_ref: 장절인지 확인
pub fn parse_order_text(text: &str, looks_like_ref: &dyn Fn(&str) -> bool) -> Value {
    let mut name = String::new();
    let mut date = String::new();
    let mut start = String::new();
    let mut items: Vec<Value> = vec![];
    let src = text.replace('\r', "").replace('\u{a0}', " ");
    let mut lines: Vec<String> = vec![];
    for raw in src.split('\n') {
        let l = raw.trim();
        if l.is_empty() {
            continue;
        }
        let starts_digit = l.chars().next().is_some_and(|c| c.is_ascii_digit());
        if ORDER_SPLIT.is_match(l) && !starts_digit {
            lines.extend(ORDER_SPLIT.split(l).map(|p| p.trim().to_string()).filter(|p| !p.is_empty()));
        } else {
            lines.push(l.to_string());
        }
    }
    for l0 in lines {
        let mut l = l0.clone();
        let dm = DATE.captures(&l);
        // 시각: 바로 뒤에 숫자가 이어지면 시각이 아님 (JS 의 (?!\d))
        let tm = TIME.captures_iter(&l).find(|c| c.get(3).is_none()).map(|c| (c[1].to_string(), c[2].to_string()));
        if let Some(d) = &dm {
            if date.is_empty() {
                date = format!("{}-{:0>2}-{:0>2}", &d[1], &d[2], &d[3]);
            }
        }
        if let Some((h, m)) = &tm {
            if start.is_empty() && !looks_like_ref(&l) {
                start = format!("{:0>2}:{}", h, m);
            }
        }
        let title_like = dm.is_some() || Regex::new(r"예배$|예배\s*순서|주보").unwrap().is_match(&l);
        if title_like && items.is_empty() && !Regex::new(r"찬송|성경|교독|찬양|기도|광고").unwrap().is_match(&l) {
            if name.is_empty() {
                let n = Regex::new(r"[\s|·•]*(예배\s*)?순서$").unwrap().replace(&l, "").to_string();
                name = Regex::new(r"\(?\s*\d{1,2}[:시]\s*[0-5]\d분?\s*\)?").unwrap().replacen(&n, 1, "").trim().to_string();
            }
            continue;
        }
        l = NUMBERING.replace(&l, "").to_string();
        let mut note = String::new();
        if STAND.is_match(&l) {
            note = "*일어서서".into();
            l = STAND_STRIP.replace_all(&l, "").trim().to_string();
        }
        let memo = MEMO.captures(&l).map(|c| (c[0].to_string(), c[1].to_string()));
        let mut it = json!({ "type": "title", "title": "", "subtitle": "", "note": note });
        if let Some(m) = READING.captures(&l) {
            it = json!({ "type": "reading", "title": "교독문", "hymnNumber": m[1].parse::<i64>().unwrap_or(0), "subtitle": m[2].trim(), "note": note });
        } else if let Some(m) = HYMN.captures(&l).filter(|m| m.get(2).is_some() || Regex::new(r"찬송|부름|송영|경배").unwrap().is_match(&m[1]) || m[1].trim().is_empty()) {
            let bk = m.get(2).map(|x| x.as_str()).unwrap_or("");
            let book = if bk.contains("구세군") { "구세군" } else { "새찬송가" };
            let head = Regex::new(r"[:：\-–]\s*$").unwrap().replace(&m[1], "").trim().to_string();
            let title = if !head.is_empty() && !head.starts_with("찬송") { head } else { "찬송".into() };
            it = json!({ "type": "hymn", "title": title, "hymnNumber": m[3].parse::<i64>().unwrap_or(0), "hymnbook": book, "subtitle": m[4].trim(), "note": note });
        } else if let Some(m) = BIBLE.captures(&l).filter(|m| looks_like_ref(&m[2])) {
            it = json!({ "type": "bible", "title": ws_strip(&m[1]), "ref": m[2].trim(), "subtitle": "", "note": note });
        } else if looks_like_ref(&l) && BOOKS_KO.is_match(&ws_strip(&l)) && l.chars().any(|c| c.is_ascii_digit()) {
            it = json!({ "type": "bible", "title": "성경봉독", "ref": l, "subtitle": "", "note": note });
        } else if let Some(m) = NOTICE.captures(&l) {
            it = json!({ "type": "text", "title": ws_strip(&m[1]), "body": m[2].trim(), "subtitle": "", "note": note });
        } else if let Some(m) = SONG.captures(&l).filter(|m| !Regex::new(r"찬양대|찬양팀\s*$").unwrap().is_match(&m[2])) {
            for s in Regex::new(r"\s*[,，、]\s*").unwrap().split(&m[2]).map(|s| s.trim()).filter(|s| !s.is_empty()) {
                items.push(json!({ "type": "song", "title": ws_strip(&m[1]), "song": s, "note": note }));
            }
            continue;
        } else if let Some(m) = SERMON.captures(&l) {
            let t = m[2].trim().to_string();
            let quoted = Regex::new(r#"[“"'‘「『]"#).unwrap().is_match(&t);
            let sub = match QUOTE.captures(&t) {
                Some(q) if quoted => {
                    let s = format!("“{}” · {}", &q[1], &q[2]);
                    s.strip_suffix(" · ").unwrap_or(&s).to_string()
                }
                _ => t,
            };
            it["title"] = json!(&m[1]);
            it["subtitle"] = json!(sub);
        } else if Regex::new(r"^(예배\s*후|폐회|끝)$").unwrap().is_match(&l) {
            it = json!({ "type": "blank", "title": l, "subtitle": "", "note": note });
        } else if let Some(sp) = SP1.captures(&l).or_else(|| SP2.captures(&l)) {
            it["title"] = json!(sp[1].trim());
            it["subtitle"] = json!(sp[2].trim());
        } else {
            it["title"] = json!(l);
        }
        if let Some((whole, inner)) = memo {
            let sub = it["subtitle"].as_str().unwrap_or("").to_string();
            if it["type"] == "title" && sub.ends_with(whole.trim()) {
                it["subtitle"] = json!(sub[..sub.len() - whole.trim().len()].trim());
                let n = [it["note"].as_str().unwrap_or(""), inner.trim()].iter().filter(|x| !x.is_empty()).cloned().collect::<Vec<_>>().join(" / ");
                it["note"] = json!(n);
            }
        }
        items.push(it);
    }
    if name.is_empty() {
        name = if date.is_empty() { "새 예배".into() } else { format!("{date} 예배") };
    }
    json!({ "name": name, "serviceDate": date, "startTime": start, "items": items })
}

/// 계획 → 큐시트 항목을 만들 때 쓰는 찾기 함수들
pub struct Resolvers<'a> {
    /// (번호, 찬송가 책) → (파일, 제목)
    pub hymn: &'a dyn Fn(i64, &str) -> Option<(String, String)>,
    /// 장절 → 확인된 제목
    pub check_ref: &'a dyn Fn(&str) -> Option<String>,
    /// 베들레헴 가사 { kind: hymn|reading, … }
    pub hymnal: &'a dyn Fn(i64, &str) -> Option<Value>,
    /// 라이브러리 찬양 { title, lyrics, order, credit }
    pub song: &'a dyn Fn(&str) -> Option<Value>,
}

/// AI·규칙이 만든 순서 → 큐시트 항목 (찬송가 파일 찾기 · 장절 확인). 반환: (cue {name, serviceDate, startTime, items}, 경고)
pub fn plan_to_cue(plan: &Value, r: &Resolvers) -> Result<(Value, Vec<String>), String> {
    let mut items = vec![];
    let mut warnings = vec![];
    let s = |v: &Value, k: &str| v[k].as_str().map(|x| x.to_string()).or_else(|| v[k].as_i64().map(|n| n.to_string())).unwrap_or_default();
    for x in plan["items"].as_array().into_iter().flatten() {
        let title = s(x, "title").trim().to_string();
        let mut base = crate::core::new_item("title", if title.is_empty() { "순서" } else { &title });
        base["note"] = json!(s(x, "note"));
        base["duration"] = json!(x["duration"].as_f64().unwrap_or(0.0));
        let ty = x["type"].as_str().unwrap_or("");
        let num = x["hymnNumber"].as_i64().or_else(|| x["hymnNumber"].as_str().and_then(|v| v.parse().ok())).unwrap_or(0);
        let mut put = |ty: &str, extra: Value| {
            let mut it = crate::core::new_item(ty, base["title"].as_str().unwrap_or(""));
            it["note"] = base["note"].clone();
            it["duration"] = base["duration"].clone();
            crate::settings::merge_into(&mut it, &extra);
            items.push(it);
        };
        let plain_title = !title.is_empty() && !Regex::new(r"^\d+장").unwrap().is_match(&title) && !title.starts_with("찬송");
        if ty == "hymn" && num > 0 {
            let book = s(x, "hymnbook");
            let label = format!("{num}장");
            if let Some((file, ht)) = (r.hymn)(num, &book) {
                let t = if plain_title { format!("{title} · {label} {ht}") } else { format!("{label} {ht}") };
                put("hymn", json!({ "title": t, "file": file, "number": num, "background": "default" }));
            } else if let Some(bh) = (r.hymnal)(num, &book).filter(|b| b["kind"] == "hymn") {
                let bt = bh["title"].as_str().unwrap_or("");
                let t = if plain_title { format!("{title} · {bt}") } else { bt.to_string() };
                put("song", json!({ "title": t, "lyrics": bh["lyrics"], "order": bh["order"].as_str().unwrap_or(""), "credit": bh["credit"].as_str().unwrap_or("") }));
            } else {
                let note = [base["note"].as_str().unwrap_or(""), "찬송가 파일 없음 — 찬송가 PPT 찾기로 넣으세요"].iter().filter(|x| !x.is_empty()).cloned().collect::<Vec<_>>().join(" / ");
                put("title", json!({ "title": format!("{} · {label}", if title.is_empty() { "찬송" } else { &title }), "subtitle": "", "note": note }));
                warnings.push(format!("{label}: 찬송가 폴더에서 찾지 못함"));
            }
        } else if ty == "bible" && !s(x, "ref").is_empty() {
            let rf = s(x, "ref");
            let ok = (r.check_ref)(&rf);
            put("bible", json!({ "ref": rf, "title": if title.is_empty() { "성경봉독".into() } else { title.clone() } }));
            if ok.is_none() {
                warnings.push(format!("'{rf}': 장절을 확인하세요"));
            }
        } else if ty == "reading" && num > 0 {
            match (r.hymnal)(num, "교독문").filter(|b| b["kind"] == "reading") {
                Some(b) => put("text", json!({ "title": b["title"], "body": b["body"] })),
                None => {
                    put("title", json!({ "title": format!("교독문 {num}"), "subtitle": s(x, "subtitle") }));
                    warnings.push(format!("교독문 {num}: 베들레헴 찬송가 가사가 없습니다 (베들레헴에서 가져오기)"));
                }
            }
        } else if ty == "song" && !s(x, "song").is_empty() && s(x, "lyrics").is_empty() {
            let name = s(x, "song");
            match (r.song)(&name) {
                Some(sg) => put("song", json!({ "title": sg["title"], "lyrics": sg["lyrics"].as_str().unwrap_or(""), "order": sg["order"].as_str().unwrap_or(""), "credit": sg["credit"].as_str().unwrap_or(""), "songId": sg["id"].as_str().unwrap_or("") })),
                None => {
                    put("title", json!({ "title": format!("{} · {name}", if title.is_empty() { "찬양" } else { &title }), "subtitle": "" }));
                    warnings.push(format!("'{name}': 라이브러리에 없는 찬양 — 가사 찾기로 넣으세요"));
                }
            }
        } else if ty == "text" {
            put("text", json!({ "body": s(x, "body"), "title": if title.is_empty() { "광고".into() } else { title.clone() } }));
        } else if ty == "song" {
            put("song", json!({ "lyrics": s(x, "lyrics"), "title": if title.is_empty() { "찬양".into() } else { title.clone() } }));
        } else if ty == "blank" {
            put("blank", json!({ "mode": "background", "title": if title.is_empty() { "빈 화면".into() } else { title.clone() } }));
        } else {
            put("title", json!({ "subtitle": s(x, "subtitle") }));
        }
    }
    if items.is_empty() {
        return Err("주보에서 예배 순서를 찾지 못했습니다.".into());
    }
    let name = s(plan, "name").trim().to_string();
    let date = s(plan, "serviceDate");
    let start = s(plan, "startTime");
    let date_ok = Regex::new(r"^\d{4}-\d{2}-\d{2}$").unwrap().is_match(&date);
    let time_ok = Regex::new(r"^\d{1,2}:\d{2}$").unwrap().is_match(&start);
    Ok((json!({ "name": if name.is_empty() { "새 예배 (AI)".into() } else { name }, "serviceDate": if date_ok { date } else { String::new() }, "startTime": if time_ok { start } else { String::new() }, "items": items }), warnings))
}

/// 오픈라우터(또는 OpenAI 호환) 요청 본문
pub fn ai_request_body(model: &str, input: &Input, extra: &str) -> Value {
    let mut content = vec![];
    match input {
        Input::Text(t) => content.push(json!({ "type": "text", "text": format!("주보 내용:\n{}", t.chars().take(60000).collect::<String>()) })),
        Input::File { mime, base64, name } => {
            let url = format!("data:{mime};base64,{base64}");
            if mime == "application/pdf" {
                content.push(json!({ "type": "file", "file": { "filename": name, "file_data": url } }));
            } else {
                content.push(json!({ "type": "image_url", "image_url": { "url": url } }));
            }
            content.push(json!({ "type": "text", "text": "위 주보에서 예배 순서를 큐시트 JSON 으로 만들어 줘." }));
        }
    }
    if !extra.trim().is_empty() {
        content.push(json!({ "type": "text", "text": format!("추가 요청: {}", extra.trim()) }));
    }
    json!({ "model": if model.is_empty() { DEFAULT_MODEL } else { model }, "messages": [{ "role": "system", "content": system_prompt() }, { "role": "user", "content": content }], "temperature": 0.1, "response_format": { "type": "json_object" } })
}

/// 응답 본문 → (계획, 사용량, 모델)
pub fn parse_ai_response(body: &Value, model: &str) -> Result<(Value, Value, String), String> {
    let msg = &body["choices"][0]["message"]["content"];
    let text = match msg {
        Value::String(s) => s.clone(),
        Value::Array(a) => a.iter().filter_map(|c| c["text"].as_str()).collect::<Vec<_>>().join(""),
        _ => return Err("AI 응답이 비었습니다.".into()),
    };
    if text.trim().is_empty() {
        return Err("AI 응답이 비었습니다.".into());
    }
    Ok((parse_json(&text)?, body["usage"].clone(), body["model"].as_str().unwrap_or(model).to_string()))
}

/// 오픈라우터에 묻는다 (오래 걸리니 백그라운드에서 부를 것)
pub fn ask_ai(api_key: &str, model: &str, endpoint: &str, input: &Input, extra: &str) -> Result<(Value, Value, String), String> {
    if api_key.is_empty() {
        return Err("오픈라우터 API 키를 넣으세요.".into());
    }
    let ep = if endpoint.is_empty() { DEFAULT_ENDPOINT } else { endpoint };
    let body = ai_request_body(model, input, extra);
    let mut resp = ureq::post(ep)
        .header("Authorization", &format!("Bearer {api_key}"))
        .header("HTTP-Referer", "https://github.com/hahadark/WorshipPresenter")
        .header("X-Title", "Worship Presenter")
        .config()
        .http_status_as_error(false)
        .build()
        .send_json(&body)
        .map_err(|e| format!("오픈라우터 연결 실패: {e}"))?;
    let status = resp.status().as_u16();
    let v: Value = resp.body_mut().read_json().unwrap_or(Value::Null);
    if !(200..300).contains(&status) {
        return Err(format!("오픈라우터 오류 ({status}): {}", v["error"]["message"].as_str().unwrap_or("")));
    }
    parse_ai_response(&v, model)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn refy(s: &str) -> bool {
        crate::parser::looks_like_reference(s)
    }
    #[test]
    fn order_text() {
        let p = parse_order_text(
            "2026년 11월 15일 주일 예배 순서 (11:00)\n1. 예배로 부름 - 홍길동 목사\n찬송 8장 *\n대표기도  김철수 장로\n성경봉독 요 3:16-18\n찬양 은혜 아니면, 주님 다시 오실 때까지\n설교 “선한 목자” 홍길동 목사\n교독문 13\n광고\n축도 (일어서서)\n예배 후",
            &refy,
        );
        assert_eq!(p["serviceDate"], "2026-11-15");
        assert_eq!(p["startTime"], "11:00");
        let it = p["items"].as_array().unwrap();
        assert_eq!(it[0]["title"], "예배로 부름");
        assert_eq!(it[0]["subtitle"], "홍길동 목사");
        assert_eq!(it[1]["type"], "hymn");
        assert_eq!(it[1]["hymnNumber"], 8);
        assert_eq!(it[1]["note"], "*일어서서");
        assert_eq!(it[2]["subtitle"], "김철수 장로");
        assert_eq!(it[3]["type"], "bible");
        assert_eq!(it[3]["ref"], "요 3:16-18");
        assert_eq!(it[4]["type"], "song");
        assert_eq!(it[5]["song"], "주님 다시 오실 때까지");
        assert_eq!(it[6]["subtitle"], "“선한 목자” · 홍길동 목사");
        assert_eq!(it[7]["type"], "reading");
        assert_eq!(it[8]["type"], "text");
        assert_eq!(it[9]["note"], "*일어서서");
        assert_eq!(it[10]["type"], "blank");
    }
    #[test]
    fn slash_line() {
        let p = parse_order_text("찬송 8장 / 기도 / 성경봉독 시 23", &refy);
        let it = p["items"].as_array().unwrap();
        assert_eq!(it.len(), 3);
        assert_eq!(it[2]["ref"], "시 23");
    }
    #[test]
    fn chat_answer() {
        let (p, s) = parse_chat_answer("여기 있습니다\n```json\n{\"name\":\"x\",\"items\":[{\"type\":\"title\",\"title\":\"기도\"}]}\n```").unwrap();
        assert_eq!(p["items"][0]["title"], "기도");
        assert!(!s);
        let (_, s2) = parse_chat_answer(&sample().to_string()).unwrap();
        assert!(s2);
        assert!(parse_chat_answer("없음").is_err());
    }
    #[test]
    fn plan_cue() {
        let plan = json!({ "name": "주일", "serviceDate": "2026-11-15", "startTime": "11:00", "items": [
            { "type": "hymn", "title": "찬송", "hymnNumber": 8 }, { "type": "hymn", "title": "예배로 부름", "hymnNumber": 9 },
            { "type": "bible", "ref": "요3:16" }, { "type": "song", "song": "없는곡" }, { "type": "blank", "title": "예배 후" } ] });
        let hymn = |n: i64, _: &str| if n == 8 { Some(("C:/h/008.pps".into(), "거룩 거룩 거룩".into())) } else { None };
        let check = |_: &str| Some("요한복음 3:16".into());
        let hymnal = |_: i64, _: &str| None;
        let song = |_: &str| None;
        let (cue, w) = plan_to_cue(&plan, &Resolvers { hymn: &hymn, check_ref: &check, hymnal: &hymnal, song: &song }).unwrap();
        let it = cue["items"].as_array().unwrap();
        assert_eq!(it[0]["type"], "hymn");
        assert_eq!(it[0]["title"], "8장 거룩 거룩 거룩");
        assert_eq!(it[1]["type"], "title");
        assert_eq!(it[1]["title"], "예배로 부름 · 9장");
        assert_eq!(it[2]["type"], "bible");
        assert_eq!(it[4]["mode"], "background");
        assert_eq!(w.len(), 2);
    }
    #[test]
    fn ai_body_and_response() {
        let b = ai_request_body("", &Input::Text("찬송 8장".into()), "");
        assert_eq!(b["model"], DEFAULT_MODEL);
        assert_eq!(b["messages"][0]["role"], "system");
        let resp = json!({ "choices": [{ "message": { "content": "```json\n{\"items\":[]}\n```" } }], "usage": { "total_tokens": 10 } });
        let (p, u, _) = parse_ai_response(&resp, "m").unwrap();
        assert!(p["items"].is_array());
        assert_eq!(u["total_tokens"], 10);
    }
    #[test]
    fn docx_text() {
        use std::io::Write;
        let mut buf = std::io::Cursor::new(vec![]);
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            z.start_file("word/document.xml", zip::write::SimpleFileOptions::default()).unwrap();
            z.write_all("<w:document><w:p ><w:r><w:t>찬송 &amp; 8장</w:t></w:r></w:p><w:p ><w:t>기도</w:t></w:p></w:document>".as_bytes()).unwrap();
            z.finish().unwrap();
        }
        let t = zip_xml_text(buf.get_ref(), "docx").unwrap();
        assert_eq!(t, "찬송 & 8장\n기도");
    }
}
