//! 슬라이드 만들기 (Electron 판 shared/slides.js · control/cue.js buildSlides 를 옮김)
//! 슬라이드는 출력 화면(render.js)이 그대로 쓰는 JSON 모양 (serde_json::Value) 으로 만든다.

use crate::measure::fits_at;
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::{json, Value};
use std::collections::HashMap;

pub struct SplitOpts {
    pub mode: String, // auto | count
    pub count: usize,
    pub templates: Vec<Value>,
}

fn f(v: &Value, d: f32) -> f32 {
    v.as_f64().map(|x| x as f32).unwrap_or(d)
}

pub fn ref_label(book: &str, parts: &[Value]) -> String {
    let a = &parts[0];
    let b = &parts[parts.len() - 1];
    let (ac, av, bc, bv) = (a["chapter"].as_i64().unwrap_or(0), a["verse"].as_i64().unwrap_or(0), b["chapter"].as_i64().unwrap_or(0), b["verse"].as_i64().unwrap_or(0));
    if ac == bc && av == bv {
        let pc = a["partCount"].as_i64().unwrap_or(0);
        let mut s = format!("{} {}:{}", book, ac, av);
        if pc > 1 {
            s.push_str(&format!(" ({}/{})", a["partIndex"].as_i64().unwrap_or(0), pc));
        }
        return s;
    }
    if ac == bc {
        return format!("{} {}:{}-{}", book, ac, av, bv);
    }
    format!("{} {}:{}-{}:{}", book, ac, av, bc, bv)
}

static PUNCT_END: Lazy<Regex> = Lazy::new(|| Regex::new(r#"[.,;:?!」』”’)]$"#).unwrap());

/// 성경 본문 → 슬라이드 (화면 공간에 맞춰 절을 묶고, 넘치면 한 절을 나눈다)
pub fn build_bible(passage: &Value, opts: &SplitOpts) -> Vec<Value> {
    let book = passage["book"]["name"].as_str().unwrap_or("").to_string();
    let title = passage["title"].as_str().unwrap_or("").to_string();
    let ver = passage["versionName"].as_str().unwrap_or("").to_string();
    let mut heads: HashMap<i64, String> = HashMap::new();
    for h in passage["headings"].as_array().into_iter().flatten() {
        let k = h["chapter"].as_i64().unwrap_or(0) * 1000 + h["verse"].as_i64().unwrap_or(0);
        let t = h["text"].as_str().unwrap_or("");
        heads.entry(k).and_modify(|s| { s.push_str(" · "); s.push_str(t) }).or_insert_with(|| t.to_string());
    }
    let parts: Vec<Value> = passage["verses"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|v| {
            let k = v["chapter"].as_i64().unwrap_or(0) * 1000 + v["verse"].as_i64().unwrap_or(0);
            let mut p = json!({ "book": v["book"], "chapter": v["chapter"], "verse": v["verse"], "text": v["text"], "heading": heads.get(&k).cloned().unwrap_or_default() });
            if let Some(t2) = v["text2"].as_str() { p["text2"] = json!(t2); }
            p
        })
        .collect();
    let mk = |ps: &[Value]| json!({ "kind": "bible", "ref": ref_label(&book, ps), "parts": ps, "title": title });
    let fits = |ps: &[Value], pref: bool| {
        let s = mk(ps);
        opts.templates.iter().all(|t| {
            let auto = t["font"]["autoFit"].as_bool().unwrap_or(true);
            let size = if pref || !auto { f(&t["font"]["size"], 72.0) } else { f(&t["font"]["minSize"], 44.0) };
            fits_at(&s, t, size, &ver)
        })
    };
    let split_verse = |p: &Value| -> Vec<Value> {
        let text = p["text"].as_str().unwrap_or("");
        let words: Vec<&str> = text.split_whitespace().collect();
        let mut pieces: Vec<String> = vec![];
        let mut start = 0usize;
        let mut guard = 0;
        while start < words.len() && guard < 200 {
            guard += 1;
            let probe = |k: usize| {
                let mut q = p.clone();
                q["text"] = json!(words[start..start + k].join(" "));
                q["partIndex"] = json!(2);
                q["partCount"] = json!(9);
                fits(&[q], false)
            };
            let (mut lo, mut hi) = (1usize, words.len() - start);
            if probe(hi) {
                pieces.push(words[start..].join(" "));
                break;
            }
            if !probe(1) {
                pieces.push(words[start].to_string());
                start += 1;
                continue;
            }
            while hi - lo > 1 {
                let mid = (lo + hi) / 2;
                if probe(mid) { lo = mid } else { hi = mid }
            }
            let mut k = lo;
            let min = ((lo as f32) * 0.6).ceil() as usize;
            let mut j = lo;
            while j >= min.max(1) {
                if PUNCT_END.is_match(words[start + j - 1]) {
                    k = j;
                    break;
                }
                j -= 1;
            }
            pieces.push(words[start..start + k].join(" "));
            start += k;
        }
        let n = pieces.len();
        pieces
            .into_iter()
            .enumerate()
            .map(|(i, t)| {
                let mut q = p.clone();
                q["text"] = json!(t);
                q["partIndex"] = json!(i + 1);
                q["partCount"] = json!(n);
                if i > 0 { q["heading"] = json!(""); }
                if i + 1 < n { if let Some(o) = q.as_object_mut() { o.remove("text2"); } }
                q
            })
            .collect()
    };
    let mut slides = vec![];
    let mut pack = |group: &[Value], slides: &mut Vec<Value>| {
        let mut cur: Vec<Value> = vec![];
        for p in group {
            if !cur.is_empty() {
                let mut t = cur.clone();
                t.push(p.clone());
                if fits(&t, true) {
                    cur = t;
                    continue;
                }
                slides.push(mk(&cur));
                cur.clear();
            }
            if fits(std::slice::from_ref(p), true) {
                cur = vec![p.clone()];
                continue;
            }
            if fits(std::slice::from_ref(p), false) {
                slides.push(mk(std::slice::from_ref(p)));
                continue;
            }
            for piece in split_verse(p) {
                slides.push(mk(&[piece]));
            }
        }
        if !cur.is_empty() {
            slides.push(mk(&cur));
        }
    };
    if opts.mode == "count" {
        let n = opts.count.max(1);
        for group in parts.chunks(n) {
            if fits(group, false) { slides.push(mk(group)) } else { pack(group, &mut slides) }
        }
    } else {
        pack(&parts, &mut slides);
    }
    slides
}

/// 해당 장·절이 들어 있는 첫 슬라이드
pub fn index_of_verse(slides: &[Value], chapter: i64, verse: i64) -> usize {
    slides
        .iter()
        .position(|s| s["parts"].as_array().is_some_and(|ps| ps.iter().any(|p| p["chapter"] == chapter && p["verse"] == verse)))
        .unwrap_or(0)
}

#[derive(Clone, Debug)]
pub struct Chunk {
    pub label: String,
    pub text: String,
    pub src: Option<usize>,
}

/// 가사·공지 글 → 슬라이드 (한 덩어리가 최소 글꼴로도 안 들어가면 줄 단위로 더 나눈다)
pub fn build_text(title: &str, show_title: bool, chunks: &[Chunk], opts: &SplitOpts) -> Vec<Value> {
    let r = if show_title { title } else { "" };
    let mk = |text: &str, label: &str, src: Option<usize>| {
        let mut s = json!({ "kind": "text", "ref": r, "title": r, "label": label, "parts": [{ "book": 0, "chapter": 0, "verse": 0, "text": text }] });
        if let Some(k) = src { s["src"] = json!(k); }
        s
    };
    let fits = |s: &Value| {
        opts.templates.iter().all(|t| {
            let auto = t["font"]["autoFit"].as_bool().unwrap_or(true);
            let size = if auto { f(&t["font"]["minSize"], 44.0) } else { f(&t["font"]["size"], 72.0) };
            fits_at(s, t, size, "")
        })
    };
    let mut slides = vec![];
    for (ci, c) in chunks.iter().enumerate() {
        let label = if c.label.is_empty() { (ci + 1).to_string() } else { c.label.clone() };
        let text = c.text.trim();
        if text.is_empty() {
            continue;
        }
        let whole = mk(text, &label, c.src);
        if fits(&whole) {
            slides.push(whole);
            continue;
        }
        let mut cur: Vec<&str> = vec![];
        let mut n = 0;
        for line in text.split('\n') {
            if !cur.is_empty() {
                let mut t = cur.clone();
                t.push(line);
                if !fits(&mk(&t.join("\n"), &label, c.src)) {
                    n += 1;
                    slides.push(mk(&cur.join("\n"), &format!("{}-{}", label, n), c.src));
                    cur.clear();
                }
            }
            cur.push(line);
        }
        if !cur.is_empty() {
            n += 1;
            slides.push(mk(&cur.join("\n"), &format!("{}-{}", label, n), c.src));
        }
    }
    slides
}

static LABEL_LINE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\[(.{1,20})\]$").unwrap());
static CHORD: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\[[A-G][#b♯♭]?(m|maj|min|sus|dim|aug|add|M)?\d*(sus\d*|add\d*)?(/[A-G][#b♯♭]?)?\]$").unwrap());
static BLANK_SPLIT: Lazy<Regex> = Lazy::new(|| Regex::new(r"\n\s*\n").unwrap());
static DASH_SPLIT: Lazy<Regex> = Lazy::new(|| Regex::new(r"\n\s*-{3,}\s*\n").unwrap());

fn is_label(l: &str) -> Option<String> {
    let t = l.trim();
    if CHORD.is_match(t) {
        return None;
    }
    LABEL_LINE.captures(t).map(|m| m[1].to_string())
}

/// 가사: 빈 줄로 슬라이드 구분, "[1절]" 만 있는 줄은 이름표
pub fn lyrics_chunks(lyrics: &str) -> Vec<Chunk> {
    let s = lyrics.replace('\r', "");
    let mut out = vec![];
    for block in BLANK_SPLIT.split(&s) {
        let mut label = String::new();
        let kept: Vec<&str> = block
            .split('\n')
            .filter(|l| match is_label(l) {
                Some(lb) => { label = lb; false }
                None => true,
            })
            .collect();
        let text = kept.join("\n").trim().to_string();
        if !text.is_empty() {
            let k = out.len();
            out.push(Chunk { label, text, src: Some(k) });
        }
    }
    out
}

static ROLE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*(인도자|사회자|집례자|리더|회중|성도|다\s?같이|함께|모두|일동)\s*[:：]\s*(.*)$").unwrap());
static PEOPLE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(회중|성도|다\s?같이|함께|모두|일동)$").unwrap());
static BOLD_WRAP: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\*\*([\s\S]*)\*\*$").unwrap());

/// 교독문 역할 표시 (회중·다같이 줄은 굵게)
pub fn role_text(text: &str, labels: bool) -> String {
    let fmt = |role: &str, body: &str| -> String {
        let plain = BOLD_WRAP.replace(body, "$1").to_string();
        let txt = if labels { if plain.is_empty() { format!("{}:", role) } else { format!("{}: {}", role, plain) } } else { plain };
        if PEOPLE.is_match(role) { format!("**{}**", txt) } else if labels { txt } else { body.to_string() }
    };
    let mut out = vec![];
    let mut cur: Option<String> = None;
    for line in text.split('\n') {
        if let Some(m) = ROLE_RE.captures(line) {
            let role: String = m[1].chars().filter(|c| !c.is_whitespace()).collect();
            let body = m[2].trim().to_string();
            if !body.is_empty() || labels {
                out.push(fmt(&role, &body));
            }
            cur = if body.is_empty() { Some(role) } else { None };
            continue;
        }
        if line.trim().is_empty() {
            cur = None;
            out.push(line.to_string());
            continue;
        }
        match &cur {
            Some(r) => {
                let s = fmt(r, line.trim());
                out.push(if labels { s.replacen(&format!("{}: ", r), "", 1) } else { s });
            }
            None => out.push(line.to_string()),
        }
    }
    out.join("\n")
}

/// 공지 글: '---' 줄로 슬라이드 구분. roles: 0 안 함 · 1 표시 · 2 역할 이름도
pub fn text_chunks(body: &str, roles: u8) -> Vec<Chunk> {
    let s = body.replace('\r', "");
    let mut out = vec![];
    for t in DASH_SPLIT.split(&s) {
        let t = if roles > 0 { role_text(t, roles == 2) } else { t.to_string() };
        let t = t.trim().to_string();
        if !t.is_empty() {
            let k = out.len();
            out.push(Chunk { label: String::new(), text: t, src: Some(k) });
        }
    }
    out
}

fn norm_label(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace() && !"[]()".contains(*c)).collect::<String>().to_lowercase()
}

fn alias_of(tok: &str) -> Vec<String> {
    let t = norm_label(tok);
    let v = |a: &[&str]| a.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    if !t.is_empty() && t.chars().all(|c| c.is_ascii_digit()) {
        return vec![format!("{}절", t), t];
    }
    match t.as_str() {
        "c" | "ch" | "chorus" | "후" | "후렴" => v(&["후렴", "chorus", "c"]),
        "b" | "br" | "bridge" | "브" | "브릿지" => v(&["브릿지", "브리지", "bridge", "b"]),
        "p" | "pre" | "prechorus" | "전" | "전주" => v(&["전주", "prechorus", "pre"]),
        "e" | "end" | "ending" | "엔" | "엔딩" | "마무리" => v(&["엔딩", "마무리", "ending", "end"]),
        "i" | "intro" | "인트로" => v(&["인트로", "intro"]),
        "t" | "tag" | "태그" => v(&["태그", "tag"]),
        _ => vec![t],
    }
}

/// 부를 순서 "1절 후렴 2절 후렴" 대로 가사 묶음을 늘어놓는다
/// 부를 순서에서 가사에 없는 이름 (arrange 와 같은 규칙: '1절' · 'v1' · '후' …) — 큐시트 점검용
pub fn unknown_order_tokens(chunks: &[Chunk], order: &str) -> Vec<String> {
    let labels: Vec<String> = chunks.iter().filter(|c| !c.label.is_empty()).map(|c| norm_label(&c.label)).collect();
    if labels.is_empty() {
        return vec![];
    }
    order
        .split(|c: char| c.is_whitespace() || ",·>→/|".contains(c))
        .filter(|x| !x.is_empty())
        .filter(|tok| {
            let names = alias_of(tok);
            !labels.iter().any(|l| names.contains(l) || names.iter().any(|n| l.starts_with(n.as_str())))
        })
        .map(String::from)
        .collect()
}

pub fn arrange(chunks: Vec<Chunk>, order: &str) -> Vec<Chunk> {
    let toks: Vec<&str> = order.split(|c: char| c.is_whitespace() || ",·>→/|".contains(c)).filter(|x| !x.is_empty()).collect();
    if toks.is_empty() {
        return chunks;
    }
    let mut groups: Vec<(String, Vec<Chunk>)> = vec![];
    for c in &chunks {
        if !c.label.is_empty() || groups.is_empty() {
            groups.push((c.label.clone(), vec![c.clone()]));
        } else {
            groups.last_mut().unwrap().1.push(c.clone());
        }
    }
    let mut out = vec![];
    for tok in toks {
        let names = alias_of(tok);
        let g = groups
            .iter()
            .find(|g| names.contains(&norm_label(&g.0)))
            .or_else(|| groups.iter().find(|g| !g.0.is_empty() && names.iter().any(|n| norm_label(&g.0).starts_with(n.as_str()))));
        if let Some(g) = g {
            out.extend(g.1.iter().cloned());
        }
    }
    if out.is_empty() { chunks } else { out }
}

fn s<'a>(it: &'a Value, k: &str) -> &'a str {
    it[k].as_str().unwrap_or("")
}

/// 제목 순서의 장들
pub fn title_pages(it: &Value) -> Vec<String> {
    let first = [s(it, "title"), s(it, "subtitle")].iter().filter(|x| !x.is_empty()).cloned().collect::<Vec<_>>().join("\n");
    let mut v = vec![if first.is_empty() { "제목".to_string() } else { first }];
    v.extend(text_chunks(s(it, "more"), 0).into_iter().map(|c| c.text));
    v
}

pub fn title_slide(page: &str, k: usize) -> Value {
    let mut lines = page.split('\n');
    let t = lines.next().unwrap_or("").trim().to_string();
    let sub = lines.collect::<Vec<_>>().join(" ").trim().to_string();
    json!({ "kind": "title", "ref": t, "subtitle": sub, "label": if k > 0 { format!("제목 {}", k + 1) } else { "제목".into() }, "parts": [], "src": k })
}

/// 큐시트 순서 하나 → 슬라이드 (성경은 passage 가 있어야 한다)
pub fn build_item(it: &Value, passage: Option<&Value>, opts: &SplitOpts) -> Vec<Value> {
    let ty = s(it, "type");
    // 음악은 화면이 아니므로 기본 제목카드를 만들지 않는다.
    if ty == "audio" { return vec![]; }
    let title = s(it, "title");
    let mut slides: Vec<Value> = match ty {
        "bible" => passage.map(|p| build_bible(p, opts)).unwrap_or_default(),
        "song" => build_text(title, it["showTitle"].as_bool().unwrap_or(false), &arrange(lyrics_chunks(s(it, "lyrics")), s(it, "order")), opts),
        "text" => build_text(title, true, &text_chunks(s(it, "body"), if it["roleLabels"].as_bool().unwrap_or(false) { 2 } else { 1 }), opts),
        "image" if !s(it, "image").is_empty() => {
            if it["score"].as_bool().unwrap_or(false) {
                vec![json!({ "kind": "score", "image": s(it, "image"), "bg": it["scoreBg"].as_str().unwrap_or("#ffffff"), "ref": title, "label": "악보", "parts": [] })]
            } else {
                vec![json!({ "kind": "image", "image": s(it, "image"), "ref": title, "label": "이미지", "parts": [] })]
            }
        }
        "camera" => vec![json!({ "kind": "camera", "camera": { "deviceId": s(it, "deviceId"), "label": s(it, "label"), "fit": it["fit"].as_str().unwrap_or("cover") }, "ref": title, "label": "카메라", "parts": [] })],
        "slideshow" => {
            let mut v: Vec<Value> = it["images"].as_array().into_iter().flatten().enumerate().map(|(k, img)| json!({ "kind": "image", "image": img, "ref": title, "label": format!("그림 {}", k + 1), "parts": [] })).collect();
            if !s(it, "body").trim().is_empty() {
                v.extend(build_text(title, false, &text_chunks(s(it, "body"), 0), opts));
            }
            if v.is_empty() {
                v.push(json!({ "kind": "blank", "mode": "background", "ref": title, "label": "배경", "parts": [] }));
            }
            if !s(it, "countdown").is_empty() {
                let lb = it["countdownLabel"].as_str().filter(|x| !x.is_empty()).unwrap_or("예배 시작까지");
                for x in v.iter_mut() { x["countdown"] = json!({ "target": s(it, "countdown"), "label": lb }); }
            }
            v
        }
        "video" if !s(it, "file").is_empty() => vec![json!({ "kind": "video", "image": s(it, "file"), "ref": title, "label": "영상", "parts": [], "caption": "",
            "video": { "volume": it["volume"].as_f64().unwrap_or(1.0), "sound": it["sound"].as_bool().unwrap_or(true), "loop": it["loop"].as_bool().unwrap_or(false), "endAction": it["endAction"].as_str().unwrap_or("hold") } })],
        "blank" => {
            let mode = it["mode"].as_str().unwrap_or("black");
            vec![json!({ "kind": "blank", "mode": mode, "ref": title, "label": if mode == "background" { "배경" } else { "검은 화면" }, "parts": [] })]
        }
        "title" => title_pages(it).iter().enumerate().map(|(k, p)| title_slide(p, k)).collect(),
        _ => vec![],
    };
    // 유튜브 주소면 출력 화면이 임베드로 그린다 (youtube.rs)
    if ty == "video" {
        if let (Some(x), Some((id, start))) = (slides.first_mut(), crate::youtube::parse(s(it, "file"))) {
            x["youtube"] = json!({ "id": id, "start": start });
            x["label"] = json!("유튜브");
        }
    }
    if slides.is_empty() {
        slides.push(json!({ "kind": "title", "ref": title, "subtitle": s(it, "subtitle"), "label": "제목", "parts": [], "src": 0 }));
    }
    let bg = s(it, "bg");
    if !bg.is_empty() {
        for x in slides.iter_mut() {
            if x["kind"] == "text" || x["kind"] == "title" { x["bg"] = json!(bg); }
        }
    }
    if ty == "song" {
        let show = s(it, "creditShow");
        let credit = s(it, "credit").trim();
        if !show.is_empty() && !credit.is_empty() {
            let idx: Vec<usize> = slides.iter().enumerate().filter(|(_, x)| x["kind"] == "text").map(|(i, _)| i).collect();
            if let (Some(&a), Some(&b)) = (idx.first(), idx.last()) {
                if show == "first" || show == "both" { slides[a]["footer"] = json!(credit); }
                if show == "last" || show == "both" { slides[b]["footer"] = json!(credit); }
            }
        }
    }
    slides
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn youtube_video_item() {
        let o = SplitOpts { mode: "count".into(), count: 1, templates: vec![] };
        let s = build_item(&json!({"type":"video", "title":"찬양 영상", "file":"https://youtu.be/dQw4w9WgXcQ?t=30", "endAction":"next"}), None, &o);
        assert_eq!((s[0]["kind"].as_str(), s[0]["youtube"]["id"].as_str(), s[0]["youtube"]["start"].as_u64()), (Some("video"), Some("dQw4w9WgXcQ"), Some(30)));
        assert_eq!(s[0]["video"]["endAction"], "next");
        let f = build_item(&json!({"type":"video", "title":"내 영상", "file":"file:///C:/a.mp4"}), None, &o);
        assert!(f[0].get("youtube").is_none(), "파일 영상은 그대로");
    }

    #[test]
    fn audio_has_no_visual_fallback() {
        assert!(build_item(&json!({"type":"audio", "title":"음악"}), None, &SplitOpts { mode: "count".into(), count: 1, templates: vec![] }).is_empty());
    }
    #[test]
    fn lyrics() {
        let c = lyrics_chunks("[1절]\n주 하나님 지으신\n모든 세계\n\n[후렴]\n주님의 높은 이름을\n\n[2절]\n숲속이나");
        assert_eq!(c.len(), 3);
        assert_eq!(c[0].label, "1절");
        let a = arrange(c, "1 C 2 C");
        assert_eq!(a.len(), 4);
        assert_eq!(a[3].label, "후렴");
    }
    #[test]
    fn roles() {
        assert_eq!(role_text("인도자: 여호와는\n회중: 나의 목자시니", false), "여호와는\n**나의 목자시니**");
    }
}
