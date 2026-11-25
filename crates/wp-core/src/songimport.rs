//! 찬양 가져오기 (Electron songimport.js 를 옮김) [트랙 B]
//! 다른 프로그램의 찬양 파일 → 찬양 라이브러리 형식 { title, lyrics, credit, arrangement }
//!   OpenLyrics XML (OpenLP 내보내기 .xml) · ChordPro (.cho .chordpro .chopro .crd) · ProPresenter 6 (.pro6)
//!   PowerPoint (.pptx · .ppsx · .ppt · .pps 의 글자) · 글 (.txt, '# 제목' 줄로 여러 곡)
//! 가사 형식: 빈 줄 = 슬라이드, [1절] [후렴] 이름표 줄, 코드는 [G] 처럼

use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::{json, Value};
use std::path::Path;

macro_rules! re {
    ($s:expr) => {{
        static R: Lazy<Regex> = Lazy::new(|| Regex::new($s).unwrap());
        &*R
    }};
}

pub const SONG_EXT: [&str; 12] = ["xml", "cho", "chordpro", "chopro", "crd", "pro6", "pro", "pptx", "ppsx", "ppt", "pps", "txt"];

fn xml_text(s: &str) -> String {
    let s = re!(r"(?i)<br\s*/?>").replace_all(s, "\n");
    let s = re!(r"<[^>]+>").replace_all(&s, "");
    let s = s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'");
    let s = re!(r"&#(\d+);").replace_all(&s, |c: &regex::Captures| c[1].parse::<u32>().ok().and_then(char::from_u32).map(String::from).unwrap_or_default());
    let s = re!(r"(?i)&#x([0-9a-f]+);").replace_all(&s, |c: &regex::Captures| u32::from_str_radix(&c[1], 16).ok().and_then(char::from_u32).map(String::from).unwrap_or_default());
    s.replace("&amp;", "&")
}

fn base(file: &Path) -> String {
    file.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
}

pub fn clean(t: &str) -> String {
    let t = t.replace('\r', "");
    let t = t.split('\n').map(|l| l.trim_end_matches([' ', '\t'])).collect::<Vec<_>>().join("\n");
    re!(r"\n{3,}").replace_all(&t, "\n\n").trim().to_string()
}

fn song(title: &str, lyrics: &str, credit: &str, arrangement: &str) -> Value {
    json!({ "title": title, "lyrics": lyrics, "credit": credit, "arrangement": arrangement })
}

fn credit_of(parts: &[String]) -> String {
    parts.iter().filter(|x| !x.is_empty()).cloned().collect::<Vec<_>>().join(" · ")
}

/// 이름표: v1 → 1절, c → 후렴, b → 브릿지 … (OpenLyrics·ProPresenter·ChordPro 공통)
pub fn label_of(name: &str) -> String {
    let n = name.trim();
    if let Some(m) = re!(r"(?i)^(?:v|verse|절)\s*(\d+)?[a-z]?$").captures(n) {
        return format!("{}절", m.get(1).map(|x| x.as_str()).unwrap_or("1"));
    }
    if let Some(m) = re!(r"(?i)^verse\s+(\d+)").captures(n) {
        return format!("{}절", &m[1]);
    }
    if let Some(m) = re!(r"^(\d+)\s*절$").captures(n) {
        return format!("{}절", &m[1]);
    }
    if re!(r"(?i)^(c|chorus|refrain|후렴)\s*\d*[a-z]?$").is_match(n) || re!(r"(?i)^chorus").is_match(n) {
        return "후렴".into();
    }
    if re!(r"(?i)^(b|bridge|브릿지|브리지)\s*\d*$").is_match(n) || re!(r"(?i)^bridge").is_match(n) {
        return "브릿지".into();
    }
    if re!(r"(?i)^(p|pre-?chorus|프리코러스)\s*\d*$").is_match(n) {
        return "프리코러스".into();
    }
    if re!(r"(?i)^(i|intro|인트로|전주)\s*\d*$").is_match(n) {
        return "전주".into();
    }
    if re!(r"(?i)^(e|o|ending|outro|tag|엔딩|마무리)\s*\d*$").is_match(n) {
        return "엔딩".into();
    }
    n.chars().take(20).collect()
}

// ---------------- OpenLyrics (OpenLP) ----------------
pub fn parse_open_lyrics(xml: &str, file: &Path) -> Vec<Value> {
    let tag = |re: &Regex| re.captures(xml).map(|m| xml_text(&m[1]).trim().to_string()).unwrap_or_default();
    let mut title = tag(re!(r"(?is)<title[^>]*>(.*?)</title>"));
    if title.is_empty() {
        title = base(file);
    }
    let authors: Vec<String> = re!(r"(?is)<author[^>]*>(.*?)</author>").captures_iter(xml).map(|m| xml_text(&m[1]).trim().to_string()).filter(|x| !x.is_empty()).collect();
    let copyright = tag(re!(r"(?is)<copyright>(.*?)</copyright>"));
    let ccli = tag(re!(r"(?is)<ccliNo>(.*?)</ccliNo>"));
    let mut names: Vec<(String, String)> = vec![];
    let mut blocks: Vec<String> = vec![];
    for m in re!(r"(?is)<verse\b([^>]*)>(.*?)</verse>").captures_iter(xml) {
        let nm = re!(r#"(?i)name="([^"]*)""#).captures(&m[1]).map(|x| x[1].to_string()).unwrap_or_default();
        let label = label_of(&nm);
        names.push((nm.to_lowercase(), label.clone()));
        let slides: Vec<String> = re!(r"(?is)<lines\b[^>]*>(.*?)</lines>")
            .captures_iter(&m[2])
            .map(|x| clean(&xml_text(&re!(r#"(?i)<chord\s+name="([^"]+)"\s*/>"#).replace_all(&x[1], "[$1]"))))
            .filter(|s| !s.is_empty())
            .collect();
        if slides.is_empty() {
            continue;
        }
        blocks.push(format!("[{}]\n{}", label, slides.join("\n\n")));
    }
    // 같은 이름표가 두 번 이상이면 (v1a·v1b 처럼 나뉜 것) 이름표는 처음만
    let mut seen: Vec<String> = vec![];
    let lyrics = blocks
        .iter()
        .map(|b| {
            let first = b.split('\n').next().unwrap_or("").to_string();
            if seen.contains(&first) {
                b.split('\n').skip(1).collect::<Vec<_>>().join("\n")
            } else {
                seen.push(first);
                b.clone()
            }
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let order: Vec<String> = tag(re!(r"(?is)<verseOrder>(.*?)</verseOrder>"))
        .split_whitespace()
        .map(|t| names.iter().find(|x| x.0 == t.to_lowercase()).map(|x| x.1.clone()).unwrap_or_else(|| label_of(t)))
        .collect();
    let arrangement = order.iter().enumerate().filter(|(k, t)| !(*k > 0 && **t == order[k - 1] && t.ends_with('절'))).map(|(_, t)| t.as_str()).collect::<Vec<_>>().join(" ");
    let credit = credit_of(&[authors.join(", "), copyright, if ccli.is_empty() { String::new() } else { format!("CCLI {ccli}") }]);
    let mut sg = song(&title, &clean(&lyrics), &credit, &arrangement);
    // 곡 정보: <key> · <tempo type="bpm"> (글로 쓴 빠르기는 버림)
    let key = tag(re!(r"(?is)<key>(.*?)</key>"));
    if !key.is_empty() {
        sg["key"] = json!(key);
    }
    if let Some(m) = re!(r#"(?is)<tempo[^>]*type="bpm"[^>]*>\s*(\d{2,3})\s*</tempo>"#).captures(xml) {
        sg["bpm"] = json!(m[1].to_string());
    }
    vec![sg]
}

// ---------------- ChordPro ----------------
pub fn parse_chord_pro(text: &str, file: &Path) -> Vec<Value> {
    let (mut title, mut artist, mut copyright, mut ccli) = (String::new(), String::new(), String::new(), String::new());
    let (mut key, mut meter, mut bpm) = (String::new(), String::new(), String::new());
    let mut out: Vec<String> = vec![];
    let mut verse_no = 0;
    for raw in text.replace('\r', "").split('\n') {
        let line = raw.trim_end();
        if let Some(d) = re!(r"(?i)^\{\s*([a-z_]+)\s*(?::\s*(.*?))?\s*\}$").captures(line.trim()) {
            let k = d[1].to_lowercase();
            let v = d.get(2).map(|x| x.as_str().trim().to_string()).unwrap_or_default();
            match k.as_str() {
                "title" | "t" => title = v,
                "artist" | "composer" | "lyricist" | "subtitle" | "st" => artist = if artist.is_empty() { v } else { format!("{artist}, {v}") },
                "copyright" => copyright = v,
                "ccli" => ccli = v,
                "key" => key = v,
                "time" => meter = v,
                "tempo" => bpm = v,
                "start_of_chorus" | "soc" => out.extend([String::new(), format!("[{}]", if v.is_empty() { "후렴".into() } else { label_of(&v) })]),
                "start_of_verse" | "sov" => {
                    let l = if v.is_empty() {
                        verse_no += 1;
                        format!("{verse_no}절")
                    } else {
                        label_of(&v)
                    };
                    out.extend([String::new(), format!("[{l}]")]);
                }
                "start_of_bridge" | "sob" => out.extend([String::new(), "[브릿지]".into()]),
                "comment" | "c" | "comment_italic" | "ci" | "comment_box" | "cb" if !v.is_empty() => {
                    let l = label_of(&v);
                    if re!(r"절$|후렴|브릿지|전주|엔딩|프리코러스").is_match(&l) {
                        if l.ends_with('절') {
                            verse_no = l.trim_end_matches('절').parse().unwrap_or(verse_no);
                        }
                        out.extend([String::new(), format!("[{l}]")]);
                    }
                }
                "chorus" => out.extend([String::new(), "[후렴]".into()]),
                _ => {} // 그 밖의 지시어(카포·끝 표시)는 버림
            }
            continue;
        }
        if line.trim().starts_with('#') {
            continue; // 주석
        }
        out.push(line.to_string());
    }
    let credit = credit_of(&[artist, copyright, if ccli.is_empty() { String::new() } else { format!("CCLI {ccli}") }]);
    let title = if title.is_empty() { base(file) } else { title };
    let mut sg = song(&title, &clean(&out.join("\n")), &credit, "");
    for (k, v) in [("key", key), ("meter", meter), ("bpm", bpm)] {
        if !v.is_empty() {
            sg[k] = json!(v);
        }
    }
    vec![sg]
}

// ---------------- ProPresenter 6 (.pro6, XML + RTF/글) ----------------
pub fn rtf_to_text(rtf: &str) -> String {
    let s = re!(r"\\par[d]?\b ?").replace_all(rtf, "\n");
    let s = re!(r"\\line\b ?").replace_all(&s, "\n");
    let s = re!(r"\\tab\b ?").replace_all(&s, " ");
    let s = re!(r"\\u(-?\d+)\??").replace_all(&s, |c: &regex::Captures| {
        let mut n: i64 = c[1].parse().unwrap_or(0);
        if n < 0 {
            n += 65536;
        }
        char::from_u32(n as u32).map(String::from).unwrap_or_default()
    });
    let s = re!(r"(?i)\\'([0-9a-f]{2})").replace_all(&s, |c: &regex::Captures| u8::from_str_radix(&c[1], 16).map(|b| (b as char).to_string()).unwrap_or_default());
    // 글꼴·색 표 같은 머리 그룹 지우기
    let s = re!(r"\{\\(?:fonttbl|colortbl|stylesheet|\*)[^{}]*(?:\{[^{}]*\}[^{}]*)*\}").replace_all(&s, "");
    let s = re!(r"(?i)\\[a-z]+-?\d* ?").replace_all(&s, "");
    let s = s.replace(['{', '}'], "");
    re!(r"\n{2,}").replace_all(&s, "\n").trim().to_string()
}

pub fn parse_pro6(xml: &str, file: &Path) -> Result<Vec<Value>, String> {
    let attr = |re: &Regex| re.captures(xml).map(|m| xml_text(&m[1]).trim().to_string()).unwrap_or_default();
    let mut title = attr(re!(r#"CCLISongTitle="([^"]*)""#));
    if title.is_empty() {
        title = base(file);
    }
    let num = attr(re!(r#"CCLISongNumber="([^"]*)""#));
    let credit = credit_of(&[attr(re!(r#"CCLIAuthor="([^"]*)""#)), attr(re!(r#"CCLIPublisher="([^"]*)""#)), if num.is_empty() { String::new() } else { format!("CCLI {num}") }]);
    let mut blocks = vec![];
    for g in re!(r"(?s)<RVSlideGrouping\b([^>]*)>(.*?)</RVSlideGrouping>").captures_iter(xml) {
        let label = label_of(&xml_text(&re!(r#"name="([^"]*)""#).captures(&g[1]).map(|x| x[1].to_string()).unwrap_or_default()));
        let mut slides = vec![];
        for sl in re!(r"(?s)<RVDisplaySlide\b.*?</RVDisplaySlide>").find_iter(&g[2]) {
            let mut texts = vec![];
            for te in re!(r"(?s)<RVTextElement\b([^>]*)>(.*?)</RVTextElement>").captures_iter(sl.as_str()) {
                let plain = re!(r#"PlainText="([^"]*)""#).captures(&te[1]).or_else(|| re!(r#"<NSString[^>]*rvXMLIvarName="PlainText"[^>]*>([^<]*)<"#).captures(&te[2])).map(|x| x[1].to_string());
                let rtf = re!(r#"RTFData="([^"]*)""#).captures(&te[1]).or_else(|| re!(r#"<NSString[^>]*rvXMLIvarName="RTFData"[^>]*>([^<]*)<"#).captures(&te[2])).map(|x| x[1].to_string());
                let t = match (plain.filter(|x| !x.is_empty()), rtf.filter(|x| !x.is_empty())) {
                    (Some(p), _) => crate::dpapi::unb64(&p).map(|b| String::from_utf8_lossy(&b).to_string()).unwrap_or_default(),
                    (None, Some(r)) => crate::dpapi::unb64(&r).map(|b| rtf_to_text(&b.iter().map(|&c| c as char).collect::<String>())).unwrap_or_default(),
                    _ => String::new(),
                };
                if !t.trim().is_empty() {
                    texts.push(t.trim().to_string());
                }
            }
            if !texts.is_empty() {
                slides.push(clean(&texts.join("\n")));
            }
        }
        if !slides.is_empty() {
            blocks.push(format!("[{}]\n{}", label, slides.join("\n\n")));
        }
    }
    if blocks.is_empty() {
        return Err(crate::i18n::tr("ProPresenter 파일에서 가사를 찾지 못했습니다 (ProPresenter 7 의 .pro 는 아직 못 읽습니다 — 텍스트·ChordPro 로 내보내 주세요).").into());
    }
    Ok(vec![song(&title, &clean(&blocks.join("\n\n")), &credit, "")])
}

// ---------------- PowerPoint (글자만) ----------------
pub fn power_point_texts(file: &Path) -> Result<Vec<String>, String> {
    let ext = file.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    if ext == "pptx" || ext == "ppsx" {
        let bytes = std::fs::read(file).map_err(|e| e.to_string())?;
        let mut doc = crate::pptx::Doc::open(&bytes)?;
        let n = doc.slides.len();
        let mut out = vec![];
        for k in 0..n {
            if let Ok(s) = doc.slide(k) {
                out.push(crate::pptx::slide_text(&s));
            }
        }
        Ok(out)
    } else {
        let (_, _, texts) = crate::pps::extract(file)?;
        Ok(texts.into_iter().map(|t| t.join("\n")).collect())
    }
}

pub fn songs_from_slides(texts: &[String], file: &Path) -> Result<Vec<Value>, String> {
    let mut slides: Vec<String> = texts.iter().map(|t| clean(t)).filter(|t| !t.is_empty()).collect();
    if slides.is_empty() {
        return Err(crate::i18n::tr("PPT 안에 글자가 없습니다 (악보 그림만 있는 PPT 는 찬송가·PPT 폴더로 쓰세요).").into());
    }
    // 첫 장이 제목 한 줄이면 제목으로
    let b = base(file);
    let mut title = re!(r"^\d+\s*[._-]?\s*").replace(&b, "").to_string();
    if title.is_empty() {
        title = b;
    }
    if slides.len() > 1 && !slides[0].contains('\n') && slides[0].chars().count() <= 40 {
        title = slides.remove(0);
    }
    Ok(vec![song(&title, &slides.join("\n\n"), "", "")])
}

// ---------------- 붙여 넣은 글 ----------------
/// '# 제목' 줄마다 새 곡 (없으면 한 곡: 첫 줄이 짧고 다음 줄이 비면 제목). 이름표가 없으면 되풀이를 찾아 구간·슬라이드로 (lines 줄씩)
pub fn parse_song_text(text: &str, fallback: &str, lines: usize) -> Vec<Value> {
    let src = text.replace('\r', "");
    let mut parts: Vec<(String, String)> = vec![];
    let head = re!(r"^#\s*\S");
    if src.lines().any(|l| head.is_match(l)) {
        let mut cur: Option<(String, String)> = None;
        for l in src.lines() {
            if head.is_match(l) {
                if let Some(c) = cur.take() {
                    parts.push(c);
                }
                cur = Some((l.trim_start_matches('#').trim().to_string(), String::new()));
            } else if let Some(c) = &mut cur {
                c.1.push_str(l);
                c.1.push('\n');
            }
        }
        if let Some(c) = cur {
            parts.push(c);
        }
    } else {
        let mut ls: Vec<&str> = src.trim().split('\n').collect();
        let mut title = fallback.to_string();
        if title.is_empty() && ls.len() > 2 && ls[0].trim().chars().count() <= 40 && ls[1].trim().is_empty() {
            title = ls.remove(0).trim().to_string();
        }
        if title.is_empty() {
            title = ls.iter().find(|l| !l.trim().is_empty()).map(|l| l.trim().chars().take(40).collect()).unwrap_or_default();
        }
        parts.push((title, ls.join("\n")));
    }
    parts
        .into_iter()
        .map(|(t, b)| {
            let (l, a) = crate::lyricsweb::arrange(&b, lines);
            song(&t.chars().take(200).collect::<String>(), &l, "", &a)
        })
        .filter(|x| !x["title"].as_str().unwrap_or("").is_empty() && !x["lyrics"].as_str().unwrap_or("").trim().is_empty())
        .collect()
}

/// 바이트 → 글 (UTF-8, 깨지면 EUC-KR/CP949)
pub fn decode_text(buf: &[u8]) -> String {
    let s = String::from_utf8_lossy(buf);
    let s = if s.contains('\u{fffd}') { encoding_rs::EUC_KR.decode(buf).0.to_string() } else { s.to_string() };
    s.trim_start_matches('\u{feff}').to_string()
}

/// 파일 하나 → 찬양 목록 (보통 한 곡)
pub fn parse_song_file(file: &Path) -> Result<Vec<Value>, String> {
    let ext = file.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    if ["pptx", "ppsx", "ppt", "pps"].contains(&ext.as_str()) {
        return songs_from_slides(&power_point_texts(file)?, file);
    }
    let buf = std::fs::read(file).map_err(|e| e.to_string())?;
    if buf.len() > 5 * 1024 * 1024 {
        return Err(crate::i18n::tr("파일이 너무 큽니다.").into());
    }
    let text = decode_text(&buf);
    if ext == "pro6" || text.contains("<RVPresentationDocument") {
        return parse_pro6(&text, file);
    }
    if ext == "pro" {
        return Err(crate::i18n::tr("ProPresenter 7 (.pro) 파일은 아직 못 읽습니다. ProPresenter 에서 텍스트나 ChordPro 로 내보내 주세요.").into());
    }
    if ext == "xml" || re!(r"(?i)<song\b[^>]*openlyrics").is_match(&text) {
        if !re!(r"(?i)<song\b").is_match(&text) {
            return Err(crate::i18n::tr("OpenLyrics(OpenLP) 형식의 XML 이 아닙니다.").into());
        }
        return Ok(parse_open_lyrics(&text, file));
    }
    if ["cho", "chordpro", "chopro", "crd"].contains(&ext.as_str()) || re!(r"(?im)^\{\s*(title|t|soc|start_of_chorus)\b").is_match(&text) {
        return Ok(parse_chord_pro(&text, file));
    }
    if ext == "txt" {
        return Ok(if re!(r"(?m)^#\s*\S").is_match(&text) { parse_song_text(&text, "", 2) } else { vec![song(&base(file), &clean(&text), "", "")] });
    }
    Err(crate::i18n::tr("읽을 수 없는 형식입니다 (OpenLP .xml · ChordPro .cho · ProPresenter 6 .pro6 · PPT · .txt).").into())
}

/// 여러 파일 → (곡 목록, 못 읽은 파일 '이름: 까닭')
pub fn parse_song_files(files: &[std::path::PathBuf]) -> (Vec<Value>, Vec<String>) {
    let (mut songs, mut errors) = (vec![], vec![]);
    for f in files {
        match parse_song_file(f) {
            Ok(s) => songs.extend(s),
            Err(e) => errors.push(format!("{}: {e}", f.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default())),
        }
    }
    (songs, errors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    #[test]
    fn labels() {
        assert_eq!(label_of("v1"), "1절");
        assert_eq!(label_of("Verse 2"), "2절");
        assert_eq!(label_of("c"), "후렴");
        assert_eq!(label_of("Chorus 2"), "후렴");
        assert_eq!(label_of("b1"), "브릿지");
        assert_eq!(label_of("pre-chorus"), "프리코러스");
        assert_eq!(label_of("Ending"), "엔딩");
        assert_eq!(label_of("intro"), "전주");
    }
    #[test]
    fn openlyrics() {
        let x = r#"<?xml version="1.0"?><song xmlns="http://openlyrics.info/namespace/2009/song"><properties><titles><title>주 하나님 지으신 모든 세계</title></titles><authors><author>Carl Boberg</author></authors><copyright>Public Domain</copyright><ccliNo>14181</ccliNo><verseOrder>v1 c v2 c</verseOrder></properties><lyrics><verse name="v1"><lines>주 하나님 지으신<br/>모든 세계</lines></verse><verse name="c"><lines><chord name="G"/>주님의 높고<br/>위대하심을</lines></verse><verse name="v2"><lines>숲 속이나 &amp; 험한 산골</lines></verse></lyrics></song>"#;
        let s = &parse_open_lyrics(x, &PathBuf::from("a.xml"))[0];
        assert_eq!(s["title"], "주 하나님 지으신 모든 세계");
        assert_eq!(s["lyrics"], "[1절]\n주 하나님 지으신\n모든 세계\n\n[후렴]\n[G]주님의 높고\n위대하심을\n\n[2절]\n숲 속이나 & 험한 산골");
        assert_eq!(s["arrangement"], "1절 후렴 2절 후렴");
        assert_eq!(s["credit"], "Carl Boberg · Public Domain · CCLI 14181");
    }
    #[test]
    fn chordpro() {
        let t = "{title: 은혜}\n{artist: 손경민}\n# 주석\n{key: G}\n{start_of_verse}\n[G]내가 누려왔던\n{end_of_verse}\n{soc}\n그 모든 것이 은혜\n{eoc}\n{c: Verse 2}\n두 번째";
        let s = &parse_chord_pro(t, &PathBuf::from("x.cho"))[0];
        assert_eq!(s["title"], "은혜");
        assert_eq!(s["credit"], "손경민");
        assert_eq!(s["lyrics"], "[1절]\n[G]내가 누려왔던\n\n[후렴]\n그 모든 것이 은혜\n\n[2절]\n두 번째");
        assert_eq!(s["key"], "G");
        let s2 = &parse_chord_pro("{title: x}\n{time: 6/8}\n{tempo: 84}\n가사", &PathBuf::from("x.cho"))[0];
        assert_eq!((s2["meter"].as_str(), s2["bpm"].as_str(), s2.get("key")), (Some("6/8"), Some("84"), None));
        let ol = r#"<song><properties><titles><title>t</title></titles><key>D</key><tempo type="bpm">90</tempo></properties><lyrics><verse name="v1"><lines>가</lines></verse></lyrics></song>"#;
        let s3 = &parse_open_lyrics(ol, &PathBuf::from("t.xml"))[0];
        assert_eq!((s3["key"].as_str(), s3["bpm"].as_str()), (Some("D"), Some("90")));
    }
    #[test]
    fn pro6_and_rtf() {
        assert_eq!(rtf_to_text(r"{\rtf1\ansi{\fonttbl{\f0 Arial;}}\f0 \u51452?\u54812?\par two}"), "주혜\ntwo");
        let plain = crate::dpapi::b64("은혜로다\n주님".as_bytes());
        let x = format!(r#"<RVPresentationDocument CCLISongTitle="은혜" CCLIAuthor="홍길동"><RVSlideGrouping name="Verse 1"><RVDisplaySlide><RVTextElement PlainText="{plain}"></RVTextElement></RVDisplaySlide></RVSlideGrouping></RVPresentationDocument>"#);
        let s = &parse_pro6(&x, &PathBuf::from("a.pro6")).unwrap()[0];
        assert_eq!(s["title"], "은혜");
        assert_eq!(s["lyrics"], "[1절]\n은혜로다\n주님");
        assert!(parse_pro6("<RVPresentationDocument/>", &PathBuf::from("a.pro6")).is_err());
    }
    #[test]
    fn text_and_slides() {
        let v = parse_song_text("# 첫 곡\n가\n나\n\n# 둘째 곡\n다\n라", "", 2);
        assert_eq!(v.len(), 2);
        assert_eq!(v[1]["title"], "둘째 곡");
        let s = songs_from_slides(&["은혜".into(), "내가 누려왔던\n모든 것들이".into()], &PathBuf::from("010 은혜.pptx")).unwrap();
        assert_eq!(s[0]["title"], "은혜");
        assert_eq!(s[0]["lyrics"], "내가 누려왔던\n모든 것들이");
        assert_eq!(decode_text(&encoding_rs::EUC_KR.encode("가사").0), "가사");
    }
    #[test]
    fn files() {
        let d = std::env::temp_dir().join(format!("wp-songimp-{}", crate::util::now_ms()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("곡.cho"), "{t: 곡}\n가사").unwrap();
        std::fs::write(d.join("곡.txt"), "가\n나").unwrap();
        std::fs::write(d.join("곡.pro"), "x").unwrap();
        let (s, e) = parse_song_files(&[d.join("곡.cho"), d.join("곡.txt"), d.join("곡.pro")]);
        assert_eq!(s.len(), 2);
        assert_eq!(e.len(), 1);
    }
}
