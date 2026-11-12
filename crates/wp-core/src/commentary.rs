//! [트랙 C] 주석(EPUB) → 절 단위 주석 (Electron commentary.js 를 그대로 옮김)
//! 대상: OCR 로 글자를 만든 EPUB 주석서. 본문 속 "1:1 태초에." 같은 절 표시로 항목을 나누고, 책 경계는
//!   ① 장 번호가 1장으로 되돌아가는 곳 ② 근처 쪽머리 제목(오타 섞인 책 이름) 유사도 ③ 성경 순서 를 함께 써서 정한다.
//! 위치는 모두 '글자 번호'(JS 문자열과 같은 감각)로 다룬다. 표: Commentaries · CommentaryEntries (Electron 과 같음)

use crate::books::BOOKS;
use anyhow::{anyhow, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

#[derive(Clone, Debug)]
pub struct Block {
    pub head: bool,
    pub text: String,
}

#[derive(Clone, Debug, Default)]
pub struct Entry {
    pub book: i64,
    pub sc: i64,
    pub sv: i64,
    pub ec: i64,
    pub ev: i64,
    pub title: String,
    pub text: String,
}

#[derive(Clone, Debug, Default)]
pub struct Analysis {
    pub title: String,
    pub entries: Vec<Entry>,
    pub articles: Vec<Entry>,
    /// 책마다 { id, name, entries, chapters, of }
    pub stats: Vec<Value>,
    pub marks: usize,
}

static HEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)&#x([0-9a-f]+);").unwrap());
static DEC: Lazy<Regex> = Lazy::new(|| Regex::new(r"&#(\d+);").unwrap());
fn decode(s: &str) -> String {
    let s = HEX.replace_all(s, |c: &regex::Captures| u32::from_str_radix(&c[1], 16).ok().and_then(char::from_u32).map(String::from).unwrap_or_default());
    let s = DEC.replace_all(&s, |c: &regex::Captures| c[1].parse::<u32>().ok().and_then(char::from_u32).map(String::from).unwrap_or_default());
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&nbsp;", " ").replace("&amp;", "&")
}

// ---------- EPUB ----------
static FULLPATH: Lazy<Regex> = Lazy::new(|| Regex::new(r#"full-path="([^"]+)""#).unwrap());
static ITEM: Lazy<Regex> = Lazy::new(|| Regex::new(r"<item\b[^>]*>").unwrap());
static ID_A: Lazy<Regex> = Lazy::new(|| Regex::new(r#"id="([^"]+)""#).unwrap());
static HREF_A: Lazy<Regex> = Lazy::new(|| Regex::new(r#"href="([^"]+)""#).unwrap());
static ITEMREF: Lazy<Regex> = Lazy::new(|| Regex::new(r#"<itemref\b[^>]*idref="([^"]+)""#).unwrap());
static DCTITLE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?s)<dc:title[^>]*>(.*?)</dc:title>").unwrap());
static BODY: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?is)<body[^>]*>(.*)</body>").unwrap());
static OPEN: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)<(h[1-6]|p|li|div)\b[^>]*>").unwrap());
static BR: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)<br\s*/?>").unwrap());
static TAGS: Lazy<Regex> = Lazy::new(|| Regex::new(r"<[^>]+>").unwrap());
static WS: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s+").unwrap());

fn pct_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) { out.push(v); i += 3; continue; }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into()
}

/// 본문 XHTML 블록 (JS 의 <(h1-6|p|li|div)>…</같은 태그> 를 차례로: 같은 태그의 첫 닫는 태그까지)
pub fn html_blocks(body: &str, out: &mut Vec<Block>) {
    let lower = body.to_ascii_lowercase();
    let mut pos = 0;
    while let Some(m) = OPEN.captures_at(body, pos) {
        let whole = m.get(0).unwrap();
        let tag = m[1].to_ascii_lowercase();
        let close = format!("</{tag}>");
        let Some(rel) = lower[whole.end()..].find(&close) else { pos = whole.end(); continue };
        let inner = &body[whole.end()..whole.end() + rel];
        let t = decode(&TAGS.replace_all(&BR.replace_all(inner, " "), ""));
        let t = WS.replace_all(&t, " ").trim().to_string();
        if !t.is_empty() { out.push(Block { head: tag.starts_with('h'), text: t }); }
        pos = whole.end() + rel + close.len();
    }
}

pub fn epub_blocks(file: &Path) -> Result<(String, Vec<Block>)> {
    let f = std::fs::File::open(file)?;
    let mut z = zip::ZipArchive::new(f).map_err(|_| anyhow!("ZIP(EPUB) 형식이 아닙니다."))?;
    let names: Vec<String> = z.file_names().filter_map(|n| n.ok().map(|c| c.to_string())).collect();
    let mut read = |name: &str| -> Option<String> {
        let mut e = z.by_name(name).ok()?;
        let mut s = vec![];
        e.read_to_end(&mut s).ok()?;
        Some(String::from_utf8_lossy(&s).into())
    };
    let container = read("META-INF/container.xml").unwrap_or_default();
    let opf_path = FULLPATH.captures(&container).map(|c| c[1].to_string()).or_else(|| names.iter().find(|n| n.ends_with(".opf")).cloned()).ok_or_else(|| anyhow!("EPUB 목차(.opf)가 없습니다."))?;
    let opf = read(&opf_path).ok_or_else(|| anyhow!("EPUB 목차를 읽을 수 없습니다."))?;
    let base = opf_path.rfind('/').map(|i| opf_path[..=i].to_string()).unwrap_or_default();
    let manifest: HashMap<String, String> = ITEM.find_iter(&opf).filter_map(|m| Some((ID_A.captures(m.as_str())?[1].to_string(), HREF_A.captures(m.as_str())?[1].to_string()))).collect();
    let spine: Vec<String> = ITEMREF.captures_iter(&opf).filter_map(|c| manifest.get(&c[1]).cloned()).collect();
    let title = DCTITLE.captures(&opf).map(|c| decode(&c[1]).trim().to_string()).unwrap_or_default();
    let mut blocks = vec![];
    for href in spine {
        if href.to_lowercase().contains("nav") { continue; }
        let html = read(&format!("{base}{}", pct_decode(&href))).unwrap_or_default();
        let body = BODY.captures(&html).map(|c| c[1].to_string()).unwrap_or(html);
        html_blocks(&body, &mut blocks);
    }
    Ok((title, blocks))
}

// ---------- 책 이름 유사도 (OCR 오타 허용) ----------
fn hangul(s: &str) -> Vec<char> {
    s.chars().filter(|c| ('가'..='힣').contains(c)).collect()
}
fn lcs(a: &[char], b: &[char]) -> usize {
    let mut dp = vec![0usize; b.len() + 1];
    for i in 1..=a.len() {
        let mut prev = 0;
        for j in 1..=b.len() {
            let tmp = dp[j];
            dp[j] = if a[i - 1] == b[j - 1] { prev + 1 } else { dp[j].max(dp[j - 1]) };
            prev = tmp;
        }
    }
    dp[b.len()]
}
/// 쪽머리 글자가 어느 책 이름에 가까운지 (0~1)
pub fn book_score(head: &str, name: &str) -> f64 {
    let h = hangul(head);
    let n: Vec<char> = name.chars().collect();
    if h.len() < 2 { return 0.0; }
    if h.iter().collect::<String>().contains(name) { return 1.0; }
    let l = lcs(&h[..h.len().min(n.len() + 2)], &n);
    l as f64 / n.len().max(2) as f64
}

// ---------- 절 표시 찾기 ----------
static MARK: Lazy<Regex> = Lazy::new(|| Regex::new(r"(^|[^0-9:가-힣A-Za-z(])([0-9]{1,3})\s?[:：]\s?([0-9]{1,3})(?:\s?[-–~]\s?([0-9]{1,3})(?:\s?[:：]\s?([0-9]{1,3}))?)?").unwrap());
static REF_BEFORE: Lazy<Regex> = Lazy::new(|| {
    let mut names: Vec<String> = BOOKS.iter().flat_map(|b| [b.name.clone(), b.short.clone()]).collect();
    names.sort_by(|a, b| b.chars().count().cmp(&a.chars().count()));
    Regex::new(&format!("(?:{})[ \\t]?$", names.iter().map(|n| regex::escape(n)).collect::<Vec<_>>().join("|"))).unwrap()
});
static AFTER_REF: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*[)\]절장편참]").unwrap());

#[derive(Clone, Copy, Debug)]
struct Mark { at: usize, end: usize, sc: i64, sv: i64, ec: i64, ev: i64 }

/// 긴 글 + 글자 번호 ↔ 바이트 위치
struct Text { s: String, ci: Vec<usize> }
impl Text {
    fn new(s: String) -> Self {
        let mut ci: Vec<usize> = s.char_indices().map(|(b, _)| b).collect();
        ci.push(s.len());
        Self { s, ci }
    }
    fn len(&self) -> usize { self.ci.len() - 1 }
    fn ch(&self, b: usize) -> usize { self.ci.partition_point(|&x| x < b) }
    fn slice(&self, a: usize, b: usize) -> &str {
        let n = self.len();
        let a = a.min(n);
        let b = b.min(n).max(a);
        &self.s[self.ci[a]..self.ci[b]]
    }
}

fn find_marks(t: &Text) -> Vec<Mark> {
    let mut out = vec![];
    for m in MARK.captures_iter(&t.s) {
        let whole = m.get(0).unwrap();
        // (?![\d:]) 대신: 바로 뒤가 숫자·콜론이면 버린다
        if t.s[whole.end()..].chars().next().is_some_and(|c| c.is_ascii_digit() || c == ':') { continue; }
        let at = t.ch(whole.start() + m[1].len());
        let end = t.ch(whole.end());
        let line = t.slice(at.saturating_sub(8), at);
        let before = &line[line.rfind('\n').map(|i| i + 1).unwrap_or(0)..];
        if REF_BEFORE.is_match(before) { continue; }
        if AFTER_REF.is_match(t.slice(end, end + 4)) { continue; }
        let n = |i: usize| m.get(i).and_then(|x| x.as_str().parse::<i64>().ok());
        let (sc, sv) = (n(2).unwrap(), n(3).unwrap());
        let (mut ec, mut ev) = (sc, sv);
        match (n(4), n(5)) { (Some(a), Some(b)) => { ec = a; ev = b; } (Some(a), None) => ev = a, _ => {} }
        if sv < 1 || (ev < sv && ec == sc) || ec < sc { continue; }
        out.push(Mark { at, end, sc, sv, ec, ev });
    }
    out
}

static SECTION: Lazy<Regex> = Lazy::new(|| Regex::new(r"\n§ [^\n]*").unwrap());
static TITLE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^([^.。\n]{1,30})[.。]\s*").unwrap());
static D2: Lazy<Regex> = Lazy::new(|| Regex::new(r"\d{2,}").unwrap());
static SOLO: Lazy<Regex> = Lazy::new(|| Regex::new(r"^([가-힣A-Za-z ·,]{1,24})$").unwrap());
static HAS_CV: Lazy<Regex> = Lazy::new(|| Regex::new(r"\d+\s?:\s?\d+").unwrap());
static ART: Lazy<Regex> = Lazy::new(|| Regex::new(r"[가-힣]{2,}\s?[가-힣]{2,}").unwrap());
static LEAD_SEC: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\n§ [^\n]*").unwrap());

fn cut_chars(s: &str, n: usize) -> String { s.chars().take(n).collect() }

fn stats_of(entries: &[Entry]) -> Vec<Value> {
    BOOKS.iter().map(|b| {
        let es: Vec<&Entry> = entries.iter().filter(|e| e.book == b.id).collect();
        let mut chs: Vec<i64> = es.iter().map(|e| e.sc).collect();
        chs.sort();
        chs.dedup();
        json!({ "id": b.id, "name": b.name, "entries": es.len(), "chapters": chs.len(), "of": b.chapters })
    }).collect()
}

/// 블록 목록 → 주석 항목
pub fn parse_commentary(blocks: &[Block]) -> Analysis {
    let mut s = String::new();
    let mut heads: Vec<(usize, String)> = vec![];
    let mut nchars = 0usize;
    for b in blocks {
        if b.head { heads.push((nchars, b.text.clone())); }
        let piece = if b.head { format!("\n§ {}", b.text) } else { format!("\n{}", b.text) };
        nchars += piece.chars().count();
        s.push_str(&piece);
    }
    let t = Text::new(s);
    let marks = find_marks(&t);
    struct HB { at: usize, id: i64, s: f64, text: String }
    let head_books: Vec<HB> = heads.iter().filter_map(|(at, text)| {
        let mut best: Option<(f64, i64)> = None;
        for bk in BOOKS.iter() {
            let sc = book_score(text, &bk.name);
            if sc >= 0.67 && best.is_none_or(|b| sc > b.0) { best = Some((sc, bk.id)); }
        }
        best.map(|(sc, id)| HB { at: *at, id, s: sc, text: text.clone() })
    }).collect();
    let start_at = head_books.iter().find(|h| h.id == 1 && h.s >= 0.99).map(|h| h.at).unwrap_or(0);
    let ms: Vec<Mark> = marks.iter().copied().filter(|m| m.at >= start_at).collect();
    let seq: Vec<Mark> = match ms.iter().position(|m| m.sc == 1 && m.sv <= 2) { Some(i) => ms[i..].to_vec(), None => vec![] };
    if seq.is_empty() {
        return Analysis { stats: stats_of(&[]), marks: marks.len(), ..Default::default() };
    }
    // 장 번호가 1장으로 되돌아가는 곳을 책 경계 후보로
    let mut cuts = vec![0usize];
    for i in 1..seq.len() {
        let m = seq[i];
        if !(m.sc == 1 && m.sv <= 3) { continue; }
        let lo = (*cuts.last().unwrap()).max(i.saturating_sub(9));
        let mut prev: Vec<i64> = seq[lo..i].iter().map(|x| x.sc).collect();
        prev.sort();
        if prev.len() >= 3 && prev[prev.len() >> 1] >= 3 { cuts.push(i); }
    }
    cuts.push(seq.len());
    let mut k = cuts.len() as i64 - 2;
    while k >= 1 {
        let ku = k as usize;
        if cuts[ku + 1] - cuts[ku] < 12 || cuts[ku] - cuts[ku - 1] < 12 { cuts.remove(ku); }
        k -= 1;
    }
    // 구간 → 책 배정 (동적 계획법)
    let ns = cuts.len() - 1;
    struct Seg { votes: Vec<f64>, max_ch: i64, min_ch: i64, size: usize }
    let mut seg_info = vec![];
    for k in 0..ns {
        let from = seq[cuts[k]].at as i64;
        let to = if cuts[k + 1] < seq.len() { seq[cuts[k + 1]].at as i64 } else { t.len() as i64 };
        let mut votes = vec![0f64; 67];
        let (lo, hi2) = (from - 2500, (from + 500).max(to - 2500));
        for h in &head_books {
            let at = h.at as i64;
            if at < lo || at >= hi2 { continue; }
            let strong = if HAS_CV.is_match(&h.text) { 2.5 } else { 1.0 };
            votes[h.id as usize] += h.s * strong;
        }
        let mut cnt: HashMap<i64, usize> = HashMap::new();
        for m in &seq[cuts[k]..cuts[k + 1]] { *cnt.entry(m.sc).or_default() += 1; }
        let mut vals: Vec<i64> = seq[cuts[k]..cuts[k + 1]].iter().filter(|m| cnt[&m.sc] >= 2 && m.sc <= 150).map(|m| m.sc).collect();
        vals.sort();
        let q = |p: f64| if vals.is_empty() { 1 } else { vals[(vals.len() - 1).min((p * vals.len() as f64).floor() as usize)] };
        seg_info.push(Seg { votes, max_ch: q(0.9), min_ch: q(0.05), size: cuts[k + 1] - cuts[k] });
    }
    const NEG: f64 = -1e9;
    let mut dp = vec![vec![NEG; 67]; ns];
    let mut back = vec![vec![0usize; 67]; ns];
    let local = |k: usize, b: usize| -> f64 {
        let bk = &BOOKS[b - 1];
        let si = &seg_info[k];
        let mut sc = si.votes[b].min(10.0) * 1.5;
        let over = si.max_ch - bk.chapters;
        if over > 2 { sc -= 8.0 + over as f64; }
        else if over > 0 { sc -= 1.5 * over as f64; }
        else { sc -= 6.0 * (bk.chapters - si.max_ch) as f64 / bk.chapters as f64; }
        sc
    };
    for b in 1..=66 { dp[0][b] = local(0, b) - (b as f64 - 1.0) * 2.0; }
    for k in 1..ns {
        for b in 1..=66usize {
            let l = local(k, b);
            let (mut best, mut arg) = (NEG, 0usize);
            for p in b.saturating_sub(10).max(1)..=b {
                if dp[k - 1][p] == NEG { continue; }
                let cont = seg_info[k].min_ch >= 6;
                let pen = if p == b { if cont { 0.0 } else { 1.5 } } else { (b - p - 1) as f64 * 1.2 + if cont { 4.0 } else { 0.0 } };
                let v = dp[k - 1][p] - pen;
                if v > best { best = v; arg = p; }
            }
            if arg > 0 { dp[k][b] = best + l; back[k][b] = arg; }
        }
    }
    let mut end_b = 1;
    for b in 1..=66 { if dp[ns - 1][b] > dp[ns - 1][end_b] { end_b = b; } }
    let mut assign = vec![0usize; ns];
    let mut b = end_b;
    for k in (0..ns).rev() { assign[k] = b; b = back[k][b]; }
    // 구간 안에서 순서·장 수에 맞는 표시만 (한 책에 처음부터 시작하는 구간이 여럿이면 가장 큰 것)
    let mut primary: HashMap<usize, usize> = HashMap::new();
    for k in 0..ns {
        if seg_info[k].min_ch >= 6 { continue; }
        let bk = assign[k];
        if primary.get(&bk).is_none_or(|&p| seg_info[k].size > seg_info[p].size) { primary.insert(bk, k); }
    }
    let keep: Vec<bool> = (0..ns).map(|k| {
        let Some(&p) = primary.get(&assign[k]) else { return true };
        if k == p { return true; }
        if k < p { return false; }
        ((p + 1)..=k).all(|j| assign[j] == assign[k] && seg_info[j].min_ch >= 6)
    }).collect();
    let mut accepted: Vec<(Mark, i64)> = vec![];
    let (mut last_book, mut last_c, mut last_v) = (0usize, 0i64, 0i64);
    for k in 0..ns {
        if !keep[k] { continue; }
        let bk = &BOOKS[assign[k] - 1];
        if assign[k] != last_book { last_c = 0; last_v = 0; last_book = assign[k]; }
        for m in &seq[cuts[k]..cuts[k + 1]] {
            if m.sc > bk.chapters || m.ec > bk.chapters { continue; }
            if m.sc < last_c || (m.sc == last_c && m.sv < last_v - 2) { continue; }
            if last_c > 0 && m.sc > last_c + 3 { continue; }
            accepted.push((*m, bk.id));
            last_v = if m.sc != last_c { m.sv } else { last_v.max(m.sv) };
            last_c = m.sc;
        }
    }
    // 항목 글: 표시 다음부터 다음 표시까지, 짧은 첫 문장은 표제
    let mut entries = vec![];
    for (i, (a, book)) in accepted.iter().enumerate() {
        let end = if i + 1 < accepted.len() { accepted[i + 1].0.at } else { t.len().min(a.end + 4000) };
        let raw = t.slice(a.end, end);
        let mut body = WS.replace_all(&SECTION.replace_all(raw, " "), " ").trim().to_string();
        let mut title = String::new();
        if let Some(tm) = TITLE_RE.captures(&body).filter(|c| !D2.is_match(&c[1])) {
            title = tm[1].trim().to_string();
            body = body[tm.get(0).unwrap().end()..].to_string();
        } else if let Some(sm) = SOLO.captures(&body) {
            title = sm[1].trim().to_string();
            body.clear();
        }
        if body.is_empty() && title.is_empty() { continue; }
        entries.push(Entry { book: *book, sc: a.sc, sv: a.sv, ec: a.ec, ev: a.ev, title, text: cut_chars(&body, 6000) });
    }
    // 상자 글: 쪽머리가 아닌 긴 제목 + 문단 → 가장 가까운 앞 항목 자리에
    let mut articles = vec![];
    for (hat, htext) in &heads {
        let tt = htext.trim();
        let n = tt.chars().count();
        if !(5..=30).contains(&n) || tt.chars().any(|c| c.is_ascii_digit()) || BOOKS.iter().any(|b| book_score(tt, &b.name) >= 0.67) { continue; }
        if !ART.is_match(tt) { continue; }
        let Some((prev, book)) = accepted.iter().rev().find(|x| x.0.at < *hat) else { continue };
        let rest = t.slice(*hat + 3, t.len());
        let next = rest.find("\n§ ").map(|b| *hat + 3 + rest[..b].chars().count());
        let stop = match next { Some(nb) => nb.min(*hat + 5000), None => *hat + 5000 };
        let seg = t.slice(*hat, stop);
        let body = WS.replace_all(&LEAD_SEC.replace(seg, ""), " ").trim().to_string();
        if body.chars().count() < 80 { continue; }
        articles.push(Entry { book: *book, sc: prev.sc, sv: prev.sv, ec: prev.ec, ev: prev.ev, title: tt.to_string(), text: body });
    }
    let stats = stats_of(&entries);
    Analysis { title: String::new(), entries, articles, stats, marks: marks.len() }
}

pub fn analyze_epub(file: &Path) -> Result<Analysis> {
    let (title, blocks) = epub_blocks(file)?;
    let mut r = parse_commentary(&blocks);
    r.title = title;
    Ok(r)
}

/// 분석 요약 (Electron analyzeCommentary 가 돌려주는 값)
pub fn summary(a: &Analysis) -> Value {
    let covered = a.stats.iter().filter(|s| s["entries"].as_u64().unwrap_or(0) > 0).count();
    let weak: Vec<Value> = a.stats.iter().filter(|s| (s["chapters"].as_f64().unwrap_or(0.0) / s["of"].as_f64().unwrap_or(1.0)) < 0.6).map(|s| s["name"].clone()).collect();
    json!({ "title": a.title, "entries": a.entries.len(), "articles": a.articles.len(), "books": covered, "weak": weak, "stats": a.stats })
}

// ---------- DB ----------
pub fn list(db: &Connection) -> Vec<Value> {
    let Ok(mut st) = db.prepare("SELECT id, name, COALESCE(source,''), COALESCE(license,''), COALESCE(imported_at,''), COALESCE(entry_count,0) FROM Commentaries ORDER BY name") else { return vec![] };
    st.query_map([], |r| Ok(json!({ "id": r.get::<_, String>(0)?, "name": r.get::<_, String>(1)?, "source": r.get::<_, String>(2)?, "license": r.get::<_, String>(3)?, "imported_at": r.get::<_, String>(4)?, "entry_count": r.get::<_, i64>(5)? })))
        .map(|it| it.flatten().collect()).unwrap_or_default()
}

static CM_ID: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)^[a-z0-9_-]{2,32}$").unwrap());

/// 주석 한 권을 한 트랜잭션으로 바꾼다 → (항목 수, 상자 글 수)
pub fn commit(db: &Connection, meta: &Value, a: &Analysis) -> Result<(usize, usize), String> {
    let id = meta["id"].as_str().unwrap_or("").trim().to_string();
    if !CM_ID.is_match(&id) { return Err("주석 ID는 영문·숫자 2~32자로 입력하세요.".into()); }
    let name = meta["name"].as_str().unwrap_or("").trim().to_string();
    if name.is_empty() { return Err("주석 이름을 입력하세요.".into()); }
    let run = || -> rusqlite::Result<()> {
        let tx = db.unchecked_transaction()?;
        tx.execute("DELETE FROM CommentaryEntries WHERE commentary_id = ?", [&id])?;
        tx.execute("INSERT OR REPLACE INTO Commentaries (id, name, source, license, imported_at, entry_count) VALUES (?, ?, ?, ?, ?, ?)",
            params![id, name, meta["source"].as_str().unwrap_or(""), meta["license"].as_str().unwrap_or(""), crate::util::iso_now(), (a.entries.len() + a.articles.len()) as i64])?;
        {
            let mut ins = tx.prepare("INSERT INTO CommentaryEntries (commentary_id, book_id, s, e, kind, title, text, seq) VALUES (?, ?, ?, ?, ?, ?, ?, ?)")?;
            let mut seq = 0i64;
            for (kind, list) in [("verse", &a.entries), ("article", &a.articles)] {
                for x in list.iter() {
                    ins.execute(params![id, x.book, x.sc * 1000 + x.sv, x.ec * 1000 + x.ev, kind, x.title, x.text, seq])?;
                    seq += 1;
                }
            }
        }
        tx.commit()
    };
    run().map_err(|e| e.to_string())?;
    Ok((a.entries.len(), a.articles.len()))
}

pub fn delete(db: &Connection, id: &str) -> Result<()> {
    let tx = db.unchecked_transaction()?;
    tx.execute("DELETE FROM CommentaryEntries WHERE commentary_id = ?", [id])?;
    tx.execute("DELETE FROM Commentaries WHERE id = ?", [id])?;
    tx.commit()?;
    Ok(())
}

/// 범위(장·절)와 겹치는 주석 항목 (여러 장에 걸친 단락 주석도 함께)
pub fn commentary_for(db: &Connection, book: i64, sc: i64, sv: i64, ec: i64, ev: i64, commentary_id: Option<&str>) -> Vec<Value> {
    let (s, e) = (sc * 1000 + sv, ec * 1000 + ev);
    let extra = if commentary_id.is_some() { " AND x.commentary_id = ?" } else { "" };
    let sql = format!("SELECT c.id, c.name, x.book_id, x.s, x.e, x.kind, COALESCE(x.title,''), x.text FROM CommentaryEntries x JOIN Commentaries c ON c.id = x.commentary_id
        WHERE x.book_id = ? AND x.s <= ? AND x.e >= ?{extra} ORDER BY c.name, (x.e - x.s) > 999 DESC, x.s, x.seq LIMIT 400");
    let Ok(mut st) = db.prepare(&sql) else { return vec![] };
    let map = |r: &rusqlite::Row| {
        let (s, e): (i64, i64) = (r.get(3)?, r.get(4)?);
        Ok(json!({ "cid": r.get::<_, String>(0)?, "cname": r.get::<_, String>(1)?, "book": r.get::<_, i64>(2)?, "s": s, "e": e, "kind": r.get::<_, String>(5)?, "title": r.get::<_, String>(6)?, "text": r.get::<_, String>(7)?,
            "sc": s / 1000, "sv": s % 1000, "ec": e / 1000, "ev": e % 1000 }))
    };
    let rows: rusqlite::Result<Vec<Value>> = match commentary_id {
        Some(c) => st.query_map(params![book, e, s, c], map).map(|it| it.flatten().collect()),
        None => st.query_map(params![book, e, s], map).map(|it| it.flatten().collect()),
    };
    rows.unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn blocks(src: &[(bool, &str)]) -> Vec<Block> { src.iter().map(|(h, t)| Block { head: *h, text: t.to_string() }).collect() }
    #[test]
    fn score_and_marks() {
        assert_eq!(book_score("창세기 3:2-14", "창세기"), 1.0);
        assert!(book_score("창세가", "창세기") >= 0.66);
        let t = Text::new("\n1:1 태초에. 하나님이 출 3:14 참조\n2:3절 끝 1:2 땅이".into());
        let m = find_marks(&t);
        assert_eq!(m.len(), 2);
        assert_eq!((m[0].sc, m[0].sv), (1, 1));
        assert_eq!((m[1].sc, m[1].sv), (1, 2));
    }
    #[test]
    fn parse_simple_book() {
        let mut src = vec![(true, "창세기")];
        let texts: Vec<String> = (1..=20).map(|v| format!("1:{v} 제목{v}. 설명 글 {v} 입니다")).collect();
        for t in &texts { src.push((false, t.as_str())); }
        let a = parse_commentary(&blocks(&src));
        assert_eq!(a.entries.len(), 20);
        assert_eq!(a.entries[0].book, 1);
        assert_eq!(a.entries[0].title, "제목1");
        assert_eq!(a.entries[0].text, "설명 글 1 입니다");
    }
    #[test]
    fn html() {
        let mut v = vec![];
        html_blocks("<h2>창세기</h2><p>1:1 태<b>초</b>에&nbsp;하나님이</p><div>x</div>", &mut v);
        assert_eq!(v.len(), 3);
        assert!(v[0].head);
        assert_eq!(v[1].text, "1:1 태초에 하나님이");
    }
    #[test]
    fn db_roundtrip() {
        let f = std::env::temp_dir().join(format!("wp-cm-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&f);
        let st = crate::db::BibleStore::open(&f).unwrap();
        let a = Analysis { entries: vec![Entry { book: 43, sc: 3, sv: 16, ec: 3, ev: 17, title: "사랑".into(), text: "글".into() }], ..Default::default() };
        commit(&st.db, &json!({ "id": "cm1", "name": "시험 주석" }), &a).unwrap();
        assert_eq!(list(&st.db).len(), 1);
        assert_eq!(commentary_for(&st.db, 43, 3, 16, 3, 16, None).len(), 1);
        assert_eq!(commentary_for(&st.db, 43, 4, 1, 4, 2, None).len(), 0);
        delete(&st.db, "cm1").unwrap();
        assert!(list(&st.db).is_empty());
    }
}

#[cfg(test)]
mod cmp {
    /// Electron parseCommentary 결과와 비교 (tmp\cm-cmp.json 이 있을 때만)
    #[test]
    #[ignore]
    fn same_as_electron() {
        let Ok(s) = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../tmp/cm-cmp.json")) else { return };
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        let blocks: Vec<super::Block> = v["blocks"].as_array().unwrap().iter().map(|b| super::Block { head: b["head"].as_bool().unwrap(), text: b["text"].as_str().unwrap().into() }).collect();
        let a = super::parse_commentary(&blocks);
        let books: Vec<serde_json::Value> = a.stats.iter().filter(|s| s["entries"].as_u64().unwrap() > 0).map(|s| serde_json::json!([s["id"], s["entries"], s["chapters"]])).collect();
        println!("rust: {} {} {:?}", a.entries.len(), a.articles.len(), books);
        assert_eq!(a.entries.len() as u64, v["n"].as_u64().unwrap());
        assert_eq!(a.articles.len() as u64, v["a"].as_u64().unwrap());
        assert_eq!(serde_json::json!(books), v["books"]);
        assert_eq!(a.entries[0].title, v["titles"][0][0].as_str().unwrap());
        assert_eq!(a.entries[0].text, v["titles"][0][1].as_str().unwrap());
    }
}
