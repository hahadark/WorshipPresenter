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

fn get(url: &str) -> Result<String, String> {
    let resp = ureq::get(url)
        .header("User-Agent", UA)
        .header("Accept-Language", "ko-KR,ko;q=0.9")
        .config()
        .timeout_global(Some(std::time::Duration::from_secs(12)))
        .build()
        .call()
        .map_err(|e| e.to_string())?;
    resp.into_body().read_to_string().map_err(|e| e.to_string())
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

fn search_url(site: &str, q: &str) -> String {
    match site {
        "bugs" => format!("https://music.bugs.co.kr/search/track?q={}", enc(q)),
        "genie" => format!("https://www.genie.co.kr/search/searchSong?query={}", enc(q)),
        _ => format!("https://www.melon.com/search/song/index.htm?q={}", enc(q)),
    }
}

/// 검색 → { results: [{site, siteName, id, title, artist, album}], errors } (사이트마다 앞 15곡, 반주(MR·Inst.) 곡은 뺌)
pub fn search(query: &str) -> Value {
    let q: String = query.trim().chars().take(100).collect();
    if q.is_empty() {
        return json!({ "results": [], "errors": [] });
    }
    let handles: Vec<_> = SITES
        .iter()
        .map(|(s, _)| {
            let (s, u) = (s.to_string(), search_url(s, &q));
            std::thread::spawn(move || {
                let r = get(&u).map(|h| parse_search(&s, &h));
                (s, r)
            })
        })
        .collect();
    let mr = re!(r"(?i)[(\[]\s*(mr|inst\.?|instrumental|반주)\s*[)\]]");
    let (mut results, mut errors) = (vec![], vec![]);
    for h in handles {
        let Ok((site, r)) = h.join() else { continue };
        let name = SITES.iter().find(|x| x.0 == site).map(|x| x.1).unwrap_or("");
        match r {
            Ok(list) => results.extend(list.into_iter().filter(|x| !mr.is_match(x["title"].as_str().unwrap_or(""))).take(15).map(|mut x| {
                x["siteName"] = json!(name);
                x
            })),
            Err(e) => errors.push(format!("{name}: {e}")),
        }
    }
    json!({ "results": results, "errors": errors })
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
    Ok(clean_lyrics(&parse_lyrics(site, &get(&url)?)))
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
}

#[cfg(test)]
mod net_tests {
    /// 실제 사이트 (네트워크): cargo test -p wp-core lyricsweb::net_tests -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live() {
        let r = super::search("은혜 손경민");
        println!("errors={} n={}", r["errors"], r["results"].as_array().map(|a| a.len()).unwrap_or(0));
        for x in r["results"].as_array().unwrap().iter().take(3) { println!("{} {} {} {}", x["siteName"], x["id"], x["title"], x["artist"]); }
        let first = r["results"].as_array().unwrap().iter().find(|x| x["site"] == "bugs").or(r["results"].get(0)).cloned().unwrap();
        let l = super::lyrics(first["site"].as_str().unwrap(), first["id"].as_str().unwrap()).unwrap();
        println!("lyrics {}자: {}", l.chars().count(), l.chars().take(60).collect::<String>().replace('\n', " / "));
        let (a, o) = super::arrange(&l, 2);
        println!("arranged: {} … order={o}", a.chars().take(40).collect::<String>().replace('\n', " / "));
    }
}
