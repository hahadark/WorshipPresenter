//! 웹에서 가사 찾기 (Electron lyricsweb.js 를 옮김): 벅스 · 지니 · 멜론 검색 결과와 곡 페이지에서 가사를 읽는다 [트랙 B]
//! - 사용자가 검색하고 고른 곡만 한 곡씩 (여러 곡을 자동으로 모으지 않음)
//! - 사이트 구조가 바뀌면 못 읽을 수 있다 → 그때는 가사를 복사해 붙여 넣기
//! - arrange(): 받은 가사를 [1절] [후렴] 구간과 슬라이드로 정리

use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0 Safari/537.36";
pub const SITES: [(&str, &str); 3] = [("bugs", "벅스"), ("genie", "지니"), ("melon", "멜론")];

macro_rules! re {
    ($s:expr) => {{
        static R: Lazy<Regex> = Lazy::new(|| Regex::new($s).unwrap());
        &*R
    }};
}

pub fn decode(s: &str) -> String {
    let s = re!(r"(?s)<!--.*?-->").replace_all(s, "");
    let s = re!(r"(?i)<br\s*/?>").replace_all(&s, "\n");
    let s = re!(r"<[^>]+>").replace_all(&s, "");
    let s = s.replace("&nbsp;", " ").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"");
    let s = re!(r"&#0*39;|&apos;").replace_all(&s, "'");
    let s = re!(r"&#(\d+);").replace_all(&s, |c: &regex::Captures| c[1].parse::<u32>().ok().and_then(char::from_u32).map(String::from).unwrap_or_default());
    let s = re!(r"(?i)&#x([0-9a-f]+);").replace_all(&s, |c: &regex::Captures| u32::from_str_radix(&c[1], 16).ok().and_then(char::from_u32).map(String::from).unwrap_or_default());
    s.replace("&amp;", "&")
}
fn one(s: &str) -> String {
    re!(r"\s+").replace_all(&decode(s), " ").trim().to_string()
}
fn pick(s: &str, re: &Regex) -> String {
    re.captures(s).map(|c| one(&c[1])).unwrap_or_default()
}

/// 접속 실패 갈래: "blocked" = 사이트가 자동 검색을 막음(429·403·503) · "net" = 연결·응답 없음
#[derive(Debug, Clone, PartialEq)]
pub struct Fail {
    pub kind: &'static str,
    pub msg: String,
}

impl std::fmt::Display for Fail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.msg)
    }
}

fn fail_of(e: &ureq::Error) -> Fail {
    match e {
        ureq::Error::StatusCode(c @ (403 | 429 | 503)) => Fail { kind: "blocked", msg: format!("HTTP {c}") },
        ureq::Error::StatusCode(c) => Fail { kind: "net", msg: format!("HTTP {c}") },
        ureq::Error::Timeout(_) => Fail { kind: "net", msg: crate::i18n::tr("응답 없음") },
        ureq::Error::HostNotFound | ureq::Error::ConnectionFailed => Fail { kind: "net", msg: crate::i18n::tr("인터넷 연결을 확인하세요") },
        e => Fail { kind: "net", msg: e.to_string() },
    }
}

fn get(url: &str) -> Result<String, Fail> {
    let resp = ureq::get(url)
        .header("User-Agent", UA)
        .header("Accept-Language", "ko-KR,ko;q=0.9")
        .config()
        .timeout_global(Some(std::time::Duration::from_secs(12)))
        .build()
        .call()
        .map_err(|e| fail_of(&e))?;
    resp.into_body().read_to_string().map_err(|e| fail_of(&e))
}

/// 검색 화면을 받았는데 곡이 0개일 때 까닭: "none" 정말 없음 · "blocked" 봇 확인 화면 · "changed" 결과 화면을 못 읽음(사이트가 바뀜 듯)
pub fn why_empty(html: &str) -> &'static str {
    let low = html.to_lowercase();
    if ["captcha", "자동입력 방지", "비정상적인 접근", "비정상적인 트래픽", "로봇이 아닙니다", "are you a robot", "too many requests"].iter().any(|m| low.contains(m)) {
        return "blocked";
    }
    // 멜론 "검색 결과가 없습니다." · 지니 "검색어의 철자나 맞춤법 등을 확인" (2026-11-25 실제 화면에서 확인)
    if re!(r"검색\s*결과가?\s*없|검색된\s*[^<>]{0,10}없|철자나\s*맞춤법|일치하는\s*[^<>]{0,10}없").is_match(html) {
        return "none";
    }
    "changed"
}

fn enc(q: &str) -> String {
    let mut o = String::new();
    for b in q.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            o.push(b as char)
        } else {
            o.push_str(&format!("%{:02X}", b))
        }
    }
    o
}

/// 검색 결과 HTML → [{site,id,title,artist,album}] (네트워크와 떼어 시험할 수 있게)
pub fn parse_search(site: &str, html: &str) -> Vec<Value> {
    let mut out = vec![];
    let mut push = |id: String, title: String, artist: String, album: String| {
        if !id.is_empty() && !title.is_empty() {
            out.push(json!({ "site": site, "id": id, "title": title, "artist": artist, "album": album }));
        }
    };
    match site {
        "bugs" => {
            for row in html.split("<tr ").skip(1) {
                if !row.contains("rowType=\"track\"") {
                    continue;
                }
                push(
                    pick(row, re!(r#"trackId="(\d+)""#)),
                    pick(row, re!(r#"(?s)<p class="title".*?title="([^"]*)""#)),
                    pick(row, re!(r#"(?s)<p class="artist">.*?title="([^"]*)""#)),
                    pick(row, re!(r#"class="album" title="([^"]*)""#)),
                );
            }
        }
        "genie" => {
            for row in html.split("<tr class=\"list\" songid=\"").skip(1) {
                push(
                    pick(row, re!(r#"^(\d+)""#)),
                    pick(row, re!(r#"class="title ellipsis" title="([^"]*)""#)),
                    pick(row, re!(r#"(?s)class="artist ellipsis"[^>]*>(.*?)</a>"#)),
                    pick(row, re!(r#"(?s)class="albumtitle ellipsis"[^>]*>(.*?)</a>"#)),
                );
            }
        }
        "melon" => {
            for row in html.split("<tr").skip(1) {
                push(
                    pick(row, re!(r#"name="input_check" value="(\d+)""#)),
                    pick(row, re!(r#"class="fc_gray" title="([^"]*)""#)),
                    pick(row, re!(r#"goArtistDetail[^"]*" title="([^"]*?) - 페이지 이동""#)),
                    pick(row, re!(r#"goAlbumDetail[^"]*" title="([^"]*?) - 페이지 이동""#)),
                );
            }
        }
        _ => {}
    }
    out
}

/// 사이트 검색 화면 주소 (자동으로 못 읽을 때 브라우저로 열기)
pub fn search_url(site: &str, q: &str) -> String {
    match site {
        "bugs" => format!("https://music.bugs.co.kr/search/track?q={}", enc(q)),
        "genie" => format!("https://www.genie.co.kr/search/searchSong?query={}", enc(q)),
        _ => format!("https://www.melon.com/search/song/index.htm?q={}", enc(q)),
    }
}

/// 사이트 하나의 검색 결과 → (곡 목록, 갈래 ok|none|blocked|changed|net, 오류 글)
fn site_outcome(site: &str, r: Result<String, Fail>) -> (Vec<Value>, &'static str, String) {
    match r {
        Ok(html) => {
            let list = parse_search(site, &html);
            if list.is_empty() { (list, why_empty(&html), String::new()) } else { (list, "ok", String::new()) }
        }
        Err(f) => (vec![], f.kind, f.msg),
    }
}

/// 갈래별 알림 글 (none · ok 는 빈 글)
fn status_text(name: &str, kind: &str, msg: &str) -> String {
    match kind {
        "blocked" => crate::i18n::tr_args("{1}: 자동 검색을 잠시 막았습니다 ({2}). 조금 뒤 다시 찾거나 사이트에서 열어 보세요.", &[&name, &if msg.is_empty() { "봇 확인" } else { msg }]),
        "changed" => crate::i18n::tr_args("{1}: 결과 화면을 읽지 못했습니다 (사이트 화면이 바뀐 듯). 사이트에서 열어 가사를 복사해 붙여 넣으세요.", &[&name]),
        "net" => crate::i18n::tr_args("{1}: 접속하지 못했습니다 ({2})", &[&name, &msg]),
        _ => String::new(),
    }
}

/// 검색 → { results: [{site, siteName, id, title, artist, album}], errors, status: [{site, siteName, kind, count, url}], summary }
/// (사이트마다 앞 15곡, 반주 MR·Inst. 곡은 빼고). kind: ok · none(정말 없음) · blocked(자동 검색 막힘) · changed(화면을 못 읽음) · net(접속 실패)
pub fn search(query: &str) -> Value {
    let q: String = query.trim().chars().take(100).collect();
    if q.is_empty() {
        return json!({ "results": [], "errors": [], "status": [], "summary": "" });
    }
    let handles: Vec<_> = SITES
        .iter()
        .map(|(s, _)| {
            let (s, u) = (s.to_string(), search_url(s, &q));
            std::thread::spawn(move || {
                let r = get(&u);
                (s, r)
            })
        })
        .collect();
    let outs: Vec<(String, Result<String, Fail>)> = handles.into_iter().filter_map(|h| h.join().ok()).collect();
    collect(&q, outs.into_iter().map(|(s, r)| {
        let o = site_outcome(&s, r);
        (s, o)
    }).collect())
}

/// 사이트별 결과를 모아 응답 모양으로 (네트워크와 떼어 시험할 수 있게)
fn collect(q: &str, outs: Vec<(String, (Vec<Value>, &'static str, String))>) -> Value {
    let mr = re!(r"(?i)[(\[]\s*(mr|inst\.?|instrumental|반주)\s*[)\]]");
    let (mut results, mut errors, mut status) = (vec![], vec![], vec![]);
    for (site, (list, kind, msg)) in outs {
        let name = SITES.iter().find(|x| x.0 == site).map(|x| x.1).unwrap_or("");
        let n0 = results.len();
        results.extend(list.into_iter().filter(|x| !mr.is_match(x["title"].as_str().unwrap_or(""))).take(15).map(|mut x| {
            x["siteName"] = json!(name);
            x
        }));
        let t = status_text(name, kind, &msg);
        if !t.is_empty() {
            errors.push(t);
        }
        status.push(json!({ "site": site, "siteName": name, "kind": kind, "count": results.len() - n0, "url": search_url(&site, q) }));
    }
    let summary = summary_of(&status, results.is_empty());
    json!({ "results": results, "errors": errors, "status": status, "summary": summary })
}

/// 결과가 비었을 때 한 줄 까닭
fn summary_of(status: &[Value], empty: bool) -> String {
    if !empty {
        return String::new();
    }
    let kinds: Vec<&str> = status.iter().filter_map(|s| s["kind"].as_str()).collect();
    if !kinds.is_empty() && kinds.iter().all(|k| *k == "none") {
        crate::i18n::tr("세 사이트 모두 찾은 곡이 없습니다. 제목을 줄이거나 띄어쓰기를 바꿔 보세요.")
    } else if !kinds.is_empty() && kinds.iter().all(|k| *k == "net") {
        crate::i18n::tr("가사 사이트에 접속하지 못했습니다. 인터넷 연결을 확인하세요.")
    } else {
        crate::i18n::tr("찾은 곡이 없습니다. 막히거나 못 읽은 사이트가 있어 결과가 빠졌을 수 있습니다.")
    }
}

// ---------------- 찾은 결과 보관 (사이트가 막혀도 전에 찾은 곡은 보이게) ----------------
static CACHE_LOCK: Lazy<parking_lot::Mutex<()>> = Lazy::new(|| parking_lot::Mutex::new(()));
const CACHE_FILE: &str = "lyricsweb-cache.json";

fn cache_load(dir: &std::path::Path) -> Value {
    std::fs::read_to_string(dir.join(CACHE_FILE)).ok().and_then(|t| serde_json::from_str(&t).ok()).filter(|v: &Value| v.is_object()).unwrap_or_else(|| json!({}))
}

fn cache_save(dir: &std::path::Path, v: &Value) {
    let p = dir.join(CACHE_FILE);
    let tmp = dir.join(format!("{CACHE_FILE}.tmp"));
    if std::fs::write(&tmp, v.to_string()).is_ok() {
        let _ = std::fs::rename(&tmp, &p);
    }
}

/// 지역 날짜·시각 "2026-11-25 23:10"
fn local_stamp() -> String {
    let ms = crate::util::now_ms() + crate::localtime::local_offset_ms();
    let (_, hm, _) = crate::localtime::split(ms);
    // 날짜 (Howard Hinnant civil_from_days)
    let z = ms.div_euclid(86_400_000) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02} {hm}")
}

/// 너무 커지지 않게: 오래된 것부터 버려 n 개만
fn trim_map(m: &mut serde_json::Map<String, Value>, n: usize) {
    while m.len() > n {
        let Some(old) = m.iter().min_by(|a, b| a.1["at"].as_str().unwrap_or("").cmp(b.1["at"].as_str().unwrap_or(""))).map(|x| x.0.clone()) else { break };
        m.remove(&old);
    }
}

/// 찾은 결과를 보관하고, 못 찾은 사이트(막힘·못 읽음·접속 실패)는 지난번에 찾아 둔 그 사이트의 결과를 덧붙인다.
/// 덧붙인 결과는 "cached": "날짜 시각", 응답에는 "cachedAt".
pub fn with_cache(dir: &std::path::Path, query: &str, mut r: Value) -> Value {
    let key = crate::textnorm::squash(query);
    if key.is_empty() {
        return r;
    }
    let _g = CACHE_LOCK.lock();
    let mut all = cache_load(dir);
    let failed: Vec<String> = r["status"].as_array().into_iter().flatten().filter(|s| matches!(s["kind"].as_str(), Some("blocked" | "changed" | "net"))).filter_map(|s| s["site"].as_str().map(String::from)).collect();
    let old = all["search"][&key].clone();
    let old_at = old["at"].as_str().unwrap_or("").to_string();
    let mut fresh: Vec<Value> = r["results"].as_array().cloned().unwrap_or_default();
    // 못 찾은 사이트의 지난 결과 (찾은 시각은 곡마다 그대로)
    let keep: Vec<Value> = old["results"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|x| x["site"].as_str().is_some_and(|s| failed.iter().any(|f| f == s)))
        .map(|x| {
            let mut x = x.clone();
            if x["at"].as_str().is_none_or(|s| s.is_empty()) {
                x["at"] = json!(old_at.clone());
            }
            x
        })
        .collect();
    if !fresh.is_empty() {
        let now = local_stamp();
        let mut save: Vec<Value> = fresh
            .iter()
            .map(|x| {
                let mut x = x.clone();
                x["at"] = json!(now.clone());
                x
            })
            .collect();
        save.extend(keep.iter().cloned());
        if !all["search"].is_object() {
            all["search"] = json!({});
        }
        all["search"][&key] = json!({ "at": now, "results": save });
        if let Some(m) = all["search"].as_object_mut() {
            trim_map(m, 300);
        }
        cache_save(dir, &all);
    }
    if !keep.is_empty() {
        let n = keep.len();
        let latest = keep.iter().filter_map(|x| x["at"].as_str()).max().unwrap_or("").to_string();
        for mut x in keep {
            x["cached"] = x["at"].clone();
            fresh.push(x);
        }
        r["results"] = json!(fresh);
        r["cachedAt"] = json!(latest);
        r["summary"] = json!(crate::i18n::tr_args("막히거나 못 읽은 사이트는 {1}에 찾아 둔 결과 {2}곡을 함께 보여 줍니다.", &[&latest, &n]));
    }
    r
}

/// 찾기 + 보관 (dir 이 없으면 보관하지 않음)
pub fn search_cached(query: &str, dir: Option<&std::path::Path>) -> Value {
    let r = search(query);
    match dir {
        Some(d) => with_cache(d, query, r),
        None => r,
    }
}

/// 고른 곡의 가사: 한 번 읽은 곡은 보관해 두었다가 다시 사이트에 묻지 않고 낸다 (가사는 바뀌지 않으므로)
pub fn lyrics_cached(site: &str, id: &str, dir: Option<&std::path::Path>) -> Result<String, String> {
    let key = format!("{site}:{id}");
    if let Some(d) = dir {
        let _g = CACHE_LOCK.lock();
        if let Some(t) = cache_load(d)["lyrics"][&key]["text"].as_str().filter(|t| !t.is_empty()) {
            return Ok(t.to_string());
        }
    }
    let t = lyrics(site, id)?;
    if let (Some(d), false) = (dir, t.is_empty()) {
        let _g = CACHE_LOCK.lock();
        let mut all = cache_load(d);
        if !all["lyrics"].is_object() {
            all["lyrics"] = json!({});
        }
        all["lyrics"][&key] = json!({ "at": local_stamp(), "text": t });
        if let Some(m) = all["lyrics"].as_object_mut() {
            trim_map(m, 500);
        }
        cache_save(d, &all);
    }
    Ok(t)
}

/// 곡 페이지 HTML → 가사 원문
pub fn parse_lyrics(site: &str, html: &str) -> String {
    match site {
        "bugs" => re!(r#"(?s)<div class="lyricsContainer">.*?<xmp>(.*?)</xmp>"#).captures(html).map(|c| c[1].to_string()).unwrap_or_default(),
        "genie" => re!(r#"(?s)<pre id="pLyrics">.*?<p>(.*?)</p>"#).captures(html).map(|c| decode(&c[1])).unwrap_or_default(),
        "melon" => re!(r#"(?s)<div class="lyric" id="d_video_summary">(.*?)</div>"#).captures(html).map(|c| decode(&c[1])).unwrap_or_default(),
        _ => String::new(),
    }
}

/// 가사 줄 정리 ('' 이면 가사 없음)
pub fn clean_lyrics(raw: &str) -> String {
    let sp = re!(r"[ \t\u{a0}]+");
    let t = raw.replace('\r', "").split('\n').map(|l| sp.replace_all(l, " ").trim().to_string()).collect::<Vec<_>>().join("\n");
    let t = re!(r"\n{3,}").replace_all(&t, "\n\n").trim().to_string();
    if t.starts_with("가사 정보가 없습니다") { String::new() } else { t }
}

/// 고른 곡의 가사 (네트워크)
pub fn lyrics(site: &str, id: &str) -> Result<String, String> {
    if !SITES.iter().any(|x| x.0 == site) || id.is_empty() || id.len() > 12 || !id.bytes().all(|b| b.is_ascii_digit()) {
        return Err(crate::i18n::tr("잘못된 곡입니다.").into());
    }
    let url = match site {
        "bugs" => format!("https://music.bugs.co.kr/track/{id}"),
        "genie" => format!("https://www.genie.co.kr/detail/songInfo?xgnm={id}"),
        _ => format!("https://www.melon.com/song/detail.htm?songId={id}"),
    };
    let html = get(&url).map_err(|f| if f.kind == "blocked" { crate::i18n::tr_args("사이트가 자동 접속을 잠시 막았습니다 ({1}). 조금 뒤 다시 고르거나 다른 사이트 결과를 고르세요", &[&f.msg]) } else { f.msg })?;
    Ok(clean_lyrics(&parse_lyrics(site, &html)))
}

// ---------------- 구간·슬라이드로 정리 ----------------
fn key_of(l: &str) -> String {
    l.chars().filter(|c| !c.is_whitespace() && !".,!?~'\"“”‘’()-".contains(*c)).collect::<String>().to_lowercase()
}

/// 가사 글 → (lyrics '[1절]\n…\n\n[후렴]\n…', arrangement '1절 후렴 2절 후렴' 또는 '')
/// 빈 줄(연)이 있으면 그 경계를 지키고, 없으면 되풀이되는 줄 묶음으로 구간을 나눈다.
/// 가장 많이 되풀이 = 후렴, 늘 후렴 앞 = 프리코러스, 나머지 되풀이 = 후렴2…, 한 번만 = 1절 2절…, 끝의 짧은 한 번 = 엔딩.
pub fn arrange(text: &str, per: usize) -> (String, String) {
    let per = per.max(1);
    let raw: Vec<String> = text.replace('\r', "").split('\n').map(|l| re!(r"\s+").replace_all(l, " ").trim().to_string()).collect();
    if raw.iter().any(|l| re!(r"^\[[^\]]{1,20}\]$").is_match(l)) {
        return (re!(r"\n{3,}").replace_all(&raw.join("\n"), "\n\n").trim().to_string(), String::new());
    }
    let mut lines: Vec<String> = vec![];
    let mut forced: HashSet<usize> = HashSet::new();
    let mut gap = true;
    for l in raw {
        if l.is_empty() {
            gap = true;
            continue;
        }
        if l.starts_with('<') && l.ends_with('>') {
            continue; // '< 곡 제목 >' 같은 머리줄
        }
        if gap {
            forced.insert(lines.len());
        }
        gap = false;
        lines.push(l);
    }
    let n = lines.len();
    if n == 0 {
        return (String::new(), String::new());
    }
    let k: Vec<String> = lines.iter().map(|l| key_of(l)).collect();
    let mut last: HashMap<&str, i64> = HashMap::new();
    let p0: Vec<i64> = k
        .iter()
        .enumerate()
        .map(|(i, x)| {
            let j = *last.get(x.as_str()).unwrap_or(&-1);
            last.insert(x, i as i64);
            j
        })
        .collect();
    // 2줄 이상 이어서 되풀이될 때만 '되풀이'
    let p: Vec<i64> = (0..n)
        .map(|i| {
            let j = p0[i];
            if j >= 0 && ((i + 1 < n && p0[i + 1] == j + 1) || (i > 0 && p0[i - 1] == j - 1)) { j } else { -1 }
        })
        .collect();
    let mut cut = forced.clone();
    cut.insert(0);
    let blocks_of = |cut: &HashSet<usize>| -> Vec<(usize, usize)> {
        let starts: Vec<usize> = (0..n).filter(|&i| cut.contains(&i) || i == 0 || ((p[i] < 0) != (p[i - 1] < 0)) || (p[i] >= 0 && p[i] != p[i - 1] + 1)).collect();
        starts.iter().enumerate().map(|(x, &s)| (s, starts.get(x + 1).copied().unwrap_or(n) - 1)).collect()
    };
    for _ in 0..12 {
        let before = cut.len();
        for (s, e) in blocks_of(&cut) {
            cut.insert(s);
            if e + 1 < n {
                cut.insert(e + 1);
            }
        }
        for i in 0..n {
            if p[i] < 0 {
                continue;
            }
            let pi = p[i] as usize;
            if cut.contains(&i) {
                cut.insert(pi);
            }
            if cut.contains(&pi) {
                cut.insert(i);
            }
            if (i + 1 == n || cut.contains(&(i + 1))) && pi + 1 < n {
                cut.insert(pi + 1);
            }
            if i + 1 < n && pi + 1 < n && cut.contains(&(pi + 1)) {
                cut.insert(i + 1);
            }
        }
        if cut.len() == before {
            break;
        }
    }
    // 한 연 안에서 두 줄 이상 묶음이 바로 되풀이되면 (A A) 나눠 둔다
    let mut blocks: Vec<Vec<String>> = vec![];
    for (s, e) in blocks_of(&cut) {
        let b: Vec<String> = lines[s..=e].to_vec();
        let mut split = None;
        let mut len = 2;
        while len * 2 <= b.len() {
            let unit = b[..len].iter().map(|l| key_of(l)).collect::<Vec<_>>().join("|");
            if b.len() % len == 0 && b.iter().enumerate().all(|(i, l)| key_of(l) == key_of(&b[i % len])) && !unit.is_empty() {
                split = Some(len);
                break;
            }
            len += 1;
        }
        match split {
            Some(len) => {
                for _ in 0..b.len() / len {
                    blocks.push(b[..len].to_vec())
                }
            }
            None => blocks.push(b),
        }
    }
    // 한 번만 나오는 묶음이 이어지면 한 절로 합친다 (묶음은 슬라이드 경계로 남긴다)
    let bkey = |b: &Vec<String>| b.iter().map(|l| key_of(l)).collect::<Vec<_>>().join("|");
    let mut bcount: HashMap<String, usize> = HashMap::new();
    for b in &blocks {
        *bcount.entry(bkey(b)).or_default() += 1;
    }
    let mut groups: Vec<(bool, Vec<Vec<String>>)> = vec![];
    for b in blocks {
        let once = bcount[&bkey(&b)] == 1;
        match groups.last_mut() {
            Some(g) if once && g.0 => g.1.push(b),
            _ => groups.push((once, vec![b])),
        }
    }
    let seq: Vec<String> = groups.iter().map(|g| g.1.iter().map(bkey).collect::<Vec<_>>().join("||")).collect();
    let mut uniq: Vec<String> = vec![];
    for s in &seq {
        if !uniq.contains(s) {
            uniq.push(s.clone());
        }
    }
    let count = |k: &str| seq.iter().filter(|x| x.as_str() == k).count();
    let mut text_of: HashMap<String, Vec<Vec<String>>> = HashMap::new();
    for (i, s) in seq.iter().enumerate() {
        text_of.insert(s.clone(), groups[i].1.clone());
    }
    let first = |k: &str| seq.iter().position(|x| x == k).unwrap_or(0);
    let mut label: HashMap<String, String> = HashMap::new();
    let mut rep: Vec<String> = uniq.iter().filter(|k| count(k) > 1).cloned().collect();
    rep.sort_by(|a, b| count(b).cmp(&count(a)).then(first(a).cmp(&first(b))));
    if let Some(chorus) = rep.first().cloned() {
        label.insert(chorus.clone(), "후렴".into());
        let mut extra = 1;
        for kk in rep.iter().skip(1) {
            let idx: Vec<usize> = seq.iter().enumerate().filter(|(_, x)| *x == kk).map(|(i, _)| i).collect();
            let next_other = |i: usize| {
                let mut j = i + 1;
                while j < seq.len() && &seq[j] == kk {
                    j += 1;
                }
                seq.get(j)
            };
            let pre = idx.iter().all(|&i| next_other(i) == Some(&chorus)) && !label.values().any(|v| v == "프리코러스");
            let lb = if pre {
                "프리코러스".to_string()
            } else {
                extra += 1;
                format!("후렴{extra}")
            };
            label.insert(kk.clone(), lb);
        }
        let last_k = seq.last().unwrap().clone();
        let last_ch = seq.iter().rposition(|x| *x == chorus).unwrap_or(0);
        if count(&last_k) == 1 && last_ch < seq.len() - 1 && text_of[&last_k].iter().map(|b| b.len()).sum::<usize>() <= 2 {
            label.insert(last_k, "엔딩".into());
        }
    }
    let mut v = 0;
    for kk in &uniq {
        if !label.contains_key(kk) {
            v += 1;
            label.insert(kk.clone(), format!("{v}절"));
        }
    }
    // 슬라이드: per 줄씩 고르게, 한 줄만 남으면 앞 슬라이드에 붙인다 (2줄씩: 3줄 → 3 · 5줄 → 3·2)
    let slides = |ls: &Vec<String>| -> Vec<String> {
        let len = ls.len();
        let m = (if len % per == 1 && len > per { len / per } else { len.div_ceil(per) }).max(1);
        let mut out = vec![];
        let mut at = 0;
        for x in 0..m {
            let take = (len - at).div_ceil(m - x);
            out.push(ls[at..at + take].join("\n"));
            at += take;
        }
        out
    };
    let lyrics_text = uniq
        .iter()
        .map(|kk| format!("[{}]\n{}", label[kk], text_of[kk].iter().flat_map(slides).collect::<Vec<_>>().join("\n\n")))
        .collect::<Vec<_>>()
        .join("\n\n");
    let order: Vec<&str> = seq.iter().map(|kk| label[kk].as_str()).collect();
    let plain = order.join(" ") == uniq.iter().map(|kk| label[kk].as_str()).collect::<Vec<_>>().join(" ");
    (lyrics_text, if plain { String::new() } else { order.join(" ") })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn arrange_chorus() {
        let t = "주 하나님 지으신 모든 세계\n내 마음 속에 그리어 볼 때\n\n주님의 높고 위대하심을\n내 영혼이 찬양하네\n\n숲 속이나 험한 산골짝에서\n지저귀는 저 새 소리들과\n\n주님의 높고 위대하심을\n내 영혼이 찬양하네";
        let (l, o) = arrange(t, 2);
        assert!(l.starts_with("[1절]\n주 하나님"), "{l}");
        assert!(l.contains("[후렴]\n주님의 높고"), "{l}");
        assert!(l.contains("[2절]\n숲 속이나"), "{l}");
        assert_eq!(o, "1절 후렴 2절 후렴");
    }
    #[test]
    fn arrange_labeled_and_slides() {
        assert_eq!(arrange("[1절]\n가\n\n\n\n나", 2).0, "[1절]\n가\n\n나");
        let (l, o) = arrange("가\n나\n다\n라\n마", 2);
        assert_eq!(l, "[1절]\n가\n나\n다\n\n라\n마");
        assert_eq!(o, "");
    }
    #[test]
    fn parse_pages() {
        let bugs = r#"<table><tr rowType="track" trackId="123"><p class="title"><a title="은혜">은혜</a></p><p class="artist"><a title="손경민">손경민</a></p><a class="album" title="은혜 앨범">x</a></tr>"#;
        let r = parse_search("bugs", bugs);
        assert_eq!(r[0]["id"], "123");
        assert_eq!(r[0]["title"], "은혜");
        assert_eq!(r[0]["artist"], "손경민");
        let g = r##"<tr class="list" songid="777"><a class="title ellipsis" title="은혜">x</a><a class="artist ellipsis" href="#">손경민</a></tr>"##;
        assert_eq!(parse_search("genie", g)[0]["id"], "777");
        assert_eq!(parse_lyrics("genie", r#"<pre id="pLyrics"><p>내가 누려왔던<br>모든 것들이 &amp; </p></pre>"#), "내가 누려왔던\n모든 것들이 & ");
        assert_eq!(clean_lyrics("  가 \u{a0} 나\r\n\n\n\n다"), "가 나\n\n다");
        assert_eq!(clean_lyrics("가사 정보가 없습니다."), "");
    }

    #[test]
    fn empty_reasons() {
        // 2026-11-25 실제 결과 없음 화면에서 가져온 글
        assert_eq!(why_empty("<!-- 검색 결과 없음 --><p>검색 결과가 없습니다.</p>"), "none");
        assert_eq!(why_empty("<li>검색어의 철자나 맞춤법 등을 확인하여 주세요.</li>"), "none");
        assert_eq!(why_empty("<form>reCAPTCHA</form>"), "blocked");
        assert_eq!(why_empty("<div>비정상적인 접근이 감지되었습니다</div>"), "blocked");
        assert_eq!(why_empty("<html><table class='new-layout'></table></html>"), "changed");
    }

    fn fake() -> Value {
        let row = |site: &str, id: &str| json!({ "site": site, "id": id, "title": "은혜", "artist": "손경민", "album": "" });
        collect("은혜", vec![
            ("bugs".into(), (vec![], "blocked", "HTTP 429".into())),
            ("genie".into(), (vec![row("genie", "1"), json!({ "site": "genie", "id": "2", "title": "은혜 (MR)", "artist": "", "album": "" })], "ok", String::new())),
            ("melon".into(), (vec![], "none", String::new())),
        ])
    }

    #[test]
    fn collect_reports_each_site() {
        let r = fake();
        assert_eq!(r["results"].as_array().unwrap().len(), 1, "MR 공은 빼고");
        assert_eq!(r["status"][0]["kind"], "blocked");
        assert!(r["status"][0]["url"].as_str().unwrap().starts_with("https://music.bugs.co.kr/search/track?q=%EC%9D%80"));
        assert_eq!(r["status"][1]["count"], 1);
        assert_eq!(r["errors"].as_array().unwrap().len(), 1, "결과 없음(none)은 오류가 아님");
        assert!(r["errors"][0].as_str().unwrap().starts_with("벅스: 자동 검색을 잠시 막았습니다 (HTTP 429)"));
        assert_eq!(r["summary"], "");
        let none = collect("x", vec![("bugs".into(), (vec![], "none", String::new())), ("melon".into(), (vec![], "none", String::new()))]);
        assert!(none["summary"].as_str().unwrap().starts_with("세 사이트 모두"));
        let net = collect("x", vec![("bugs".into(), (vec![], "net", "x".into()))]);
        assert!(net["summary"].as_str().unwrap().contains("인터넷 연결"));
    }

    #[test]
    fn cache_fills_blocked_sites_and_lyrics() {
        let dir = std::env::temp_dir().join(format!("wp-lyr-{}", crate::util::short_id("t")));
        std::fs::create_dir_all(&dir).unwrap();
        // 1) 벅스가 되던 때: 벅스·지니 결과 보관
        let ok = collect("은혜", vec![
            ("bugs".into(), (vec![json!({ "site": "bugs", "id": "9", "title": "은혜", "artist": "손경민", "album": "" })], "ok", String::new())),
            ("genie".into(), (vec![json!({ "site": "genie", "id": "1", "title": "은혜", "artist": "", "album": "" })], "ok", String::new())),
        ]);
        let r1 = with_cache(&dir, "은혜", ok);
        assert!(r1.get("cachedAt").is_none());
        // 2) 벅스가 막혔을 때: 지니는 새 결과, 벅스는 보관한 결과를 덧붙임 (띄어쓰기가 달라도 같은 검색어)
        let r2 = with_cache(&dir, "은 혜", fake());
        let res = r2["results"].as_array().unwrap();
        assert_eq!(res.len(), 2);
        assert_eq!(res[1]["site"], "bugs");
        assert!(res[1]["cached"].as_str().unwrap().len() >= 16, "찾은 날짜 시각");
        assert!(r2["summary"].as_str().unwrap().contains("찾아 둔 결과 1곡"));
        // 3) 또 막혀도 벅스 결과는 계속 남는다 (새로 찾은 지니와 함께 다시 저장됐으므로)
        let r3 = with_cache(&dir, "은혜", fake());
        assert_eq!(r3["results"].as_array().unwrap().len(), 2);
        // 가사 보관: 한 번 읽은 곡은 네트워크 없이
        {
            let mut all = cache_load(&dir);
            all["lyrics"] = json!({ "bugs:9": { "at": "2026-11-25 10:00", "text": "내가 누려왔던" } });
            cache_save(&dir, &all);
        }
        assert_eq!(lyrics_cached("bugs", "9", Some(&dir)).unwrap(), "내가 누려왔던");
        assert!(lyrics_cached("bugs", "x", Some(&dir)).is_err(), "잘못된 곡 번호는 네트워크 전에 거름");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn stamp_shape() {
        let s = local_stamp();
        assert!(re!(r"^20\d\d-\d\d-\d\d \d\d:\d\d$").is_match(&s), "{s}");
    }
}

#[cfg(test)]
mod net_tests {
    /// 실제 사이트 (네트워크): cargo test -p wp-core lyricsweb::net_tests -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live() {
        let r = super::search("은혜 손경민");
        println!("errors={} n={}", r["errors"], r["results"].as_array().map(|a| a.len()).unwrap_or(0));
        for s in r["status"].as_array().unwrap() { println!("status {} {} {}", s["siteName"], s["kind"], s["count"]); }
        let none = super::search("쀍쀍없는곡제목zzqx");
        for s in none["status"].as_array().unwrap() { println!("없는 곡: {} {}", s["siteName"], s["kind"]); }
        println!("없는 곡 summary: {}", none["summary"]);
        for x in r["results"].as_array().unwrap().iter().take(3) { println!("{} {} {} {}", x["siteName"], x["id"], x["title"], x["artist"]); }
        let first = r["results"].as_array().unwrap().iter().find(|x| x["site"] == "bugs").or(r["results"].get(0)).cloned().unwrap();
        let l = super::lyrics(first["site"].as_str().unwrap(), first["id"].as_str().unwrap()).unwrap();
        println!("lyrics {}자: {}", l.chars().count(), l.chars().take(60).collect::<String>().replace('\n', " / "));
        let (a, o) = super::arrange(&l, 2);
        println!("arranged: {} … order={o}", a.chars().take(40).collect::<String>().replace('\n', " / "));
    }
}
