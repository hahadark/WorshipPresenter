//! 찬양 콘티 스튜디오(Worship Conti Studio, 윤명중 · AGPL-3.0)의 콘티 파일(.worship) 가져오기.
//! 파일 형식만 읽는다(코드는 옮기지 않음). 콘티 → 큐시트: 곡 순서 · 송폼 → 부를 순서 · Key·템포·메모 → 진행 메모 ·
//! 곡에 연결한 PPT · 곡과 무관한 PPT 자유 항목(광고·성경 봉독 등).

use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::{json, Value};
use std::path::Path;

/// 콘티 스튜디오 ContiItem 의 송폼 기본값 — 손대지 않은 곡에도 들어 있으므로 '정한 송폼'으로 보지 않는다
pub const DEFAULT_FORM: &str = "Intro - V1 - C - Outro";
/// ppt_extras 의 after 값: 맨 끝
const PPT_END: &str = "__end__";

/// 이 JSON 이 콘티 스튜디오 파일인가 (service_info 또는 item_type 이 있는 items)
pub fn is_conti(v: &Value) -> bool {
    let items = v["items"].as_array();
    items.is_some() && (v["service_info"].is_object() || items.into_iter().flatten().any(|it| it.get("item_type").is_some() || it.get("song_form").is_some()))
}

/// 송폼 한 조각 → 부를 순서 낱말 (반주만 있는 조각은 None)
fn form_token(seg: &str) -> Option<String> {
    static NUM: Lazy<Regex> = Lazy::new(|| Regex::new(r"(\d+)").unwrap());
    let t: String = seg.chars().filter(|c| !c.is_whitespace() && !".:'\"".contains(*c)).collect::<String>().to_lowercase();
    if t.is_empty() {
        return None;
    }
    let num = || NUM.captures(&t).map(|m| m[1].to_string());
    // 반주만: 인트로·아웃트로·간주·전조·반주·연주·필인
    const INST: [&str; 14] = ["intro", "outro", "interlude", "inter", "inst", "solo", "fill", "인트로", "아웃트로", "간주", "전조", "반주", "연주", "필인"];
    if INST.iter().any(|k| t.starts_with(k)) || t == "i" || t == "o" {
        return None;
    }
    if t.starts_with("coda") || t.starts_with("코다") {
        return Some("엔딩".into());
    }
    if t.starts_with("prechorus") || t.starts_with("pre") || t.starts_with("pc") || t.starts_with("프리코러스") {
        return Some("pre".into());
    }
    if t.starts_with("chorus") || t.starts_with("ch") || t.starts_with("후렴") || t.starts_with("코러스") || t.starts_with('c') {
        return Some("후렴".into());
    }
    if t.starts_with("bridge") || t.starts_with("br") || t.starts_with("브릿지") || t.starts_with("브리지") || t == "b" || (t.starts_with('b') && t[1..].chars().all(|c| c.is_ascii_digit())) {
        return Some("브릿지".into());
    }
    if t.starts_with("ending") || t.starts_with("end") || t.starts_with("엔딩") || t.starts_with("마무리") {
        return Some("엔딩".into());
    }
    if t.starts_with("tag") || t.starts_with("태그") {
        return Some("태그".into());
    }
    if t.starts_with("verse") || t.starts_with('v') || t.ends_with('절') || t.chars().all(|c| c.is_ascii_digit()) {
        return Some(format!("{}절", num().unwrap_or_else(|| "1".into())));
    }
    None
}

/// 콘티 스튜디오 송폼 → 워십프리젠터 부를 순서.
/// "Intro - V1 - C - V2 - C - Bridge - C x2 - Outro" → "1절 후렴 2절 후렴 브릿지 후렴 후렴"
pub fn form_to_order(form: &str) -> String {
    static REP: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\s*(?:[x×*]\s*(\d)|(\d)\s*(?:번|회|times))\s*$").unwrap());
    static SPLIT: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s*(?:-|–|—|→|>|,|/|\||·|\n)\s*").unwrap());
    let mut out: Vec<String> = vec![];
    for seg in SPLIT.split(form.trim()) {
        let seg = seg.trim();
        if seg.is_empty() {
            continue;
        }
        let (body, times) = match REP.captures(seg) {
            Some(m) => {
                let n: usize = m.get(1).or(m.get(2)).and_then(|x| x.as_str().parse().ok()).unwrap_or(1);
                (seg[..m.get(0).unwrap().start()].trim(), n.clamp(1, 9))
            }
            None => (seg, 1),
        };
        // "Verse 1" · "V 2" 처럼 숫자만 띄어 쓴 것은 한 조각, "I V1 C O" 처럼 여럿을 띄어 쓴 것은 다시 나눈다
        let words: Vec<&str> = body.split_whitespace().collect();
        let parts: Vec<&str> = if words.len() > 1 && !words.iter().any(|w| w.chars().all(|c| c.is_ascii_digit())) { words } else { vec![body] };
        for p in parts {
            if let Some(tok) = form_token(p) {
                for _ in 0..times {
                    out.push(tok.clone());
                }
            }
        }
    }
    out.join(" ")
}

/// 곡을 찾는 함수: (제목들) → 라이브러리 찬양 { id, title, lyrics, credit, arrangement }
pub type SongFinder<'a> = &'a dyn Fn(&str) -> Option<Value>;

fn s(v: &Value, k: &str) -> String {
    v[k].as_str().map(|x| x.trim().to_string()).or_else(|| v[k].as_i64().map(|n| n.to_string())).unwrap_or_default()
}

/// "Grace (손경민)" → "Grace"
fn strip_paren(t: &str) -> String {
    static P: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s*[\(\[（].*?[\)\]）]\s*").unwrap());
    P.replace_all(t, " ").trim().to_string()
}

/// 진행 메모: "Key A · 4/4 · BPM 70 · 송폼 Intro - V1 - C / 메모"
fn note_of(it: &Value, show_form: bool) -> String {
    let mut a: Vec<String> = vec![];
    let key = s(it, "key");
    if !key.is_empty() {
        a.push(format!("Key {key}"));
    }
    let ts = s(it, "time_signature");
    if !ts.is_empty() {
        a.push(ts);
    }
    let tempo = s(it, "tempo");
    if !tempo.is_empty() {
        a.push(format!("BPM {tempo}")); // ♩ 는 맑은 고딕에서 J 처럼 보인다
    }
    let form = s(it, "song_form");
    if show_form && !form.is_empty() {
        a.push(format!("송폼 {form}"));
    }
    let mut note = a.join(" · ");
    let memo = s(it, "memo");
    if !memo.is_empty() {
        if !note.is_empty() {
            note.push_str(" / ");
        }
        note.push_str(&memo);
    }
    note
}

fn file_stem(p: &str) -> String {
    Path::new(p).file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_default()
}

/// 이 PC 에 있는 PPT 파일인가
fn ppt_ok(p: &str) -> bool {
    let low = p.to_lowercase();
    !p.is_empty() && (low.ends_with(".pptx") || low.ends_with(".ppt") || low.ends_with(".pps") || low.ends_with(".ppsx")) && Path::new(p).is_file()
}

fn ppt_item(path: &str, title: &str) -> Value {
    let (number, ftitle, name) = crate::hymns::parse_name(Path::new(path));
    let mut it = crate::hymns::new_hymn_item(&crate::hymns::HymnFile { file: path.into(), number, title: if ftitle.is_empty() { name.clone() } else { ftitle }, name, folder: String::new() });
    if !title.is_empty() {
        it["title"] = json!(title);
    }
    it
}

/// 곡 하나 → 큐시트 순서 (경고는 warnings 에)
fn song_item(it: &Value, find: SongFinder, warnings: &mut Vec<String>) -> Value {
    let title = {
        let t = s(it, "title");
        if t.is_empty() { "찬양".to_string() } else { t }
    };
    let form = s(it, "song_form");
    let form_set = !form.is_empty() && form != DEFAULT_FORM;
    let mut order = if form_set { form_to_order(&form) } else { String::new() };
    let ppt = s(it, "ppt_path");
    let mode = s(it, "ppt_mode");
    let note = note_of(it, form_set);
    // 1) 곡에 연결한 PPT (가사로만 하기로 한 곡은 빼고)
    if !ppt.is_empty() && mode != "lyrics" {
        if ppt_ok(&ppt) {
            let mut x = ppt_item(&ppt, &title);
            x["note"] = json!(note);
            return x;
        }
        warnings.push(format!("'{title}': 연결한 PPT 파일이 이 PC 에 없습니다 ({ppt}) — 가사로 넣었습니다"));
    }
    // 2) 콘티 파일의 가사 → 없으면 찬양 라이브러리
    let mut lyrics = s(it, "lyrics");
    let (mut credit, mut song_id) = (String::new(), String::new());
    let found = find(&title).or_else(|| {
        let o = strip_paren(&s(it, "original_title"));
        if o.is_empty() { None } else { find(&o) }
    });
    if let Some(sg) = &found {
        if lyrics.is_empty() {
            lyrics = s(sg, "lyrics");
            credit = s(sg, "credit");
            song_id = s(sg, "id");
        }
        // 콘티에 송폼을 따로 정하지 않았으면 라이브러리에 저장한 부를 순서
        if order.is_empty() {
            order = s(sg, "arrangement");
        }
    }
    if lyrics.is_empty() {
        let mut x = crate::core::new_item("title", &title);
        x["subtitle"] = json!(s(it, "original_title"));
        x["note"] = json!(note);
        warnings.push(format!("'{title}': 가사가 없습니다 — 찬양 라이브러리에 넣거나 가사 찾기(Ctrl+L)로 넣으세요"));
        return x;
    }
    let mut x = crate::core::new_item("song", &title);
    x["lyrics"] = json!(lyrics);
    x["order"] = json!(order);
    x["credit"] = json!(credit);
    x["songId"] = json!(song_id);
    x["note"] = json!(note);
    x
}

/// 콘티 → 큐시트 { name, serviceDate, startTime, memo, items } 와 경고
pub fn to_cue(v: &Value, fallback_name: &str, find: SongFinder) -> Result<(Value, Vec<String>), String> {
    if !is_conti(v) {
        return Err(crate::i18n::tr("찬양 콘티 스튜디오 파일(.worship)이 아닙니다.").into());
    }
    let info = &v["service_info"];
    let mut warnings = vec![];
    // 곡 순서: 악보를 잘라 나눈 곡(같은 split_group 이 이어짐)은 한 곡으로. 뺀 조각의 uid 는 남긴 곡의 uid 로 (자유 항목 자리)
    let mut songs: Vec<&Value> = vec![];
    let mut alias: std::collections::HashMap<String, String> = Default::default();
    let mut last_group = String::new();
    let mut list: Vec<&Value> = v["items"].as_array().into_iter().flatten().filter(|x| x.is_object()).collect();
    list.sort_by_key(|x| x["order"].as_i64().unwrap_or(i64::MAX)); // 안정 정렬: order 가 같으면 파일 순서
    for it in list {
        let g = s(it, "split_group");
        if !g.is_empty() && g == last_group {
            if let Some(prev) = songs.last() {
                alias.insert(s(it, "uid"), s(prev, "uid"));
            }
            continue;
        }
        last_group = g;
        songs.push(it);
    }
    // 곡과 무관한 PPT 자유 항목 (after = 앞 곡의 uid · "" = 맨 앞 · __end__ = 맨 끝). 옛 파일은 ppt_files 를 맨 끝에.
    let mut extras: Vec<(String, String, String)> = vec![]; // (after, path, title)
    if let Some(a) = v["ppt_extras"].as_array() {
        for ex in a {
            let p = s(ex, "path");
            if !p.is_empty() {
                extras.push((ex["after"].as_str().unwrap_or(PPT_END).to_string(), p, s(ex, "title")));
            }
        }
    } else {
        let linked: Vec<String> = songs.iter().map(|x| s(x, "ppt_path")).collect();
        for p in v["ppt_files"].as_array().into_iter().flatten().filter_map(|x| x.as_str()) {
            if !p.trim().is_empty() && !linked.iter().any(|l| l == p) {
                extras.push((PPT_END.into(), p.to_string(), String::new()));
            }
        }
    }
    let uids: Vec<String> = songs.iter().map(|x| s(x, "uid")).filter(|u| !u.is_empty()).collect();
    let mut items: Vec<Value> = vec![];
    let put_extras = |anchor: &str, items: &mut Vec<Value>, warnings: &mut Vec<String>| {
        for (after, p, t) in &extras {
            let after = alias.get(after).unwrap_or(after);
            let a = if after.is_empty() || uids.contains(after) { after.as_str() } else { PPT_END };
            if a != anchor {
                continue;
            }
            let title = if t.is_empty() { file_stem(p) } else { t.clone() };
            if ppt_ok(p) {
                items.push(ppt_item(p, &title));
            } else {
                let mut x = crate::core::new_item("title", &title);
                x["note"] = json!(format!("PPT 파일 없음: {p}"));
                items.push(x);
                warnings.push(format!("'{title}': PPT 파일이 이 PC 에 없습니다 ({p})"));
            }
        }
    };
    put_extras("", &mut items, &mut warnings);
    for it in &songs {
        let ty = s(it, "item_type").to_uppercase();
        if ty.is_empty() || ty == "SONG" {
            items.push(song_item(it, find, &mut warnings));
        } else {
            let mut x = crate::core::new_item("title", &s(it, "title"));
            x["note"] = json!(note_of(it, false));
            items.push(x);
        }
        let u = s(it, "uid");
        if !u.is_empty() {
            put_extras(&u, &mut items, &mut warnings);
        }
    }
    put_extras(PPT_END, &mut items, &mut warnings);
    if items.is_empty() {
        return Err(crate::i18n::tr("콘티에 곡이 없습니다.").into());
    }
    let date = s(info, "date");
    let date_ok = Regex::new(r"^\d{4}-\d{2}-\d{2}$").unwrap().is_match(&date);
    let service = s(info, "service_name");
    let name = if !service.is_empty() { service } else if !fallback_name.is_empty() { fallback_name.to_string() } else { "가져온 콘티".to_string() };
    let mut memo: Vec<String> = vec![];
    for (k, label) in [("church_name", "교회"), ("worship_leader", "인도"), ("team_members", "찬양팀"), ("theme_message", "주제"), ("memo", "메모")] {
        let x = s(info, k);
        if !x.is_empty() {
            memo.push(format!("{label}: {x}"));
        }
    }
    memo.push("찬양 콘티 스튜디오에서 가져옴".into());
    Ok((json!({ "name": name, "serviceDate": if date_ok { date } else { String::new() }, "startTime": "", "memo": memo.join("\n"), "items": items }), warnings))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forms() {
        assert_eq!(form_to_order("Intro - V1 - C - V2 - C - Bridge - C - Outro"), "1절 후렴 2절 후렴 브릿지 후렴");
        assert_eq!(form_to_order("Intro - V1 - C - V1 - C - 전조(B) - C x2 - Outro"), "1절 후렴 1절 후렴 후렴 후렴");
        assert_eq!(form_to_order("V1-PC-C-V2-PC-C-B-C-Ending"), "1절 pre 후렴 2절 pre 후렴 브릿지 후렴 엔딩");
        assert_eq!(form_to_order("1절 → 후렴 → 2절 → 후렴 2번 → 간주 → 브릿지"), "1절 후렴 2절 후렴 후렴 브릿지");
        assert_eq!(form_to_order("Verse 1 - Chorus - Verse 2 - Chorus×2 - Tag"), "1절 후렴 2절 후렴 후렴 태그");
        assert_eq!(form_to_order("I V1 C O"), "1절 후렴");
        assert_eq!(form_to_order("V1 - C - Coda"), "1절 후렴 엔딩");
        assert_eq!(form_to_order(""), "");
        assert_eq!(form_to_order("Intro - Interlude - Outro"), "");
    }

    #[test]
    fn order_works_with_slides_arrange() {
        // 바꾼 부를 순서가 실제 슬라이드 늘어놓기에서 그대로 쓰이는지
        let lyrics = "[1절]\n가\n\n[후렴]\n나\n\n[2절]\n다\n\n[브릿지]\n라";
        let order = form_to_order("Intro - V1 - C - V2 - C - Bridge - C - Outro");
        let out = crate::slides::arrange(crate::slides::lyrics_chunks(lyrics), &order);
        let texts: Vec<String> = out.iter().map(|c| c.text.clone()).collect();
        assert_eq!(texts, ["가", "나", "다", "나", "라", "나"]);
    }

    fn sample() -> Value {
        serde_json::from_str(r#"{
          "version": "1.0",
          "service_info": { "date": "2026-08-30", "service_name": "주일 2부 찬양예배", "worship_leader": "윤명중 목사", "team_members": "피아노: 이예은", "theme_message": "은혜", "memo": "" },
          "items": [
            { "order": 2, "item_type": "SONG", "title": "꽃들도", "original_title": "花も (Hanamo)", "key": "A -> B", "time_signature": "4/4", "tempo": "76", "song_form": "Intro - V1 - C - V1 - C - 전조(B) - C x2 - Outro", "lyrics": "", "memo": "전조 후 웅장하게", "uid": "u2" },
            { "order": 1, "item_type": "SONG", "title": "은혜", "original_title": "Grace (손경민)", "key": "A", "time_signature": "4/4", "tempo": "70", "song_form": "Intro - V1 - C - V2 - C - Bridge - C - Outro", "lyrics": "[1절]\n내가 누려왔던\n\n[후렴]\n은혜", "memo": "", "uid": "u1" },
            { "order": 3, "item_type": "SONG", "title": "나뉜 곡", "song_form": "Intro - V1 - C - Outro", "split_group": "g1", "split_part": 1, "uid": "u3" },
            { "order": 4, "item_type": "SONG", "title": "나뉜 곡", "song_form": "Intro - V1 - C - Outro", "split_group": "g1", "split_part": 2, "uid": "u4" },
            { "order": 4, "item_type": "SONG", "title": "나뉜 곡", "split_group": "g1", "split_part": 3, "uid": "u4b" },
            { "order": 5, "item_type": "SONG", "title": "PPT 곡", "ppt_path": "Z:\\없는\\곡.pptx", "lyrics": "[1절]\n가사", "uid": "u5" }
          ],
          "ppt_extras": [ { "id": "e1", "path": "Z:\\없는\\광고.pptx", "title": "광고", "after": "u1" }, { "id": "e2", "path": "Z:\\없는\\봉독.pptx", "title": "", "after": "u4b" } ]
        }"#).unwrap()
    }

    #[test]
    fn conti_to_cue() {
        let v = sample();
        assert!(is_conti(&v));
        assert!(!is_conti(&json!({ "items": [{ "type": "song", "title": "x" }] })), "워십프리젠터 큐시트는 아님");
        let find = |t: &str| -> Option<Value> {
            match t {
                "꽃들도" => Some(json!({ "id": "s1", "title": "꽃들도", "lyrics": "[1절]\n꽃들도\n\n[후렴]\n주를", "credit": "작곡 x", "arrangement": "1절 후렴" })),
                "나뉜곡" | "나뉜 곡" => Some(json!({ "id": "s3", "title": "나뉜 곡", "lyrics": "[1절]\n나\n\n[후렴]\n뉨", "credit": "", "arrangement": "1절 후렴 후렴" })),
                _ => None,
            }
        };
        let (cue, warnings) = to_cue(&v, "파일이름", &find).unwrap();
        assert_eq!(cue["name"], "주일 2부 찬양예배");
        assert_eq!(cue["serviceDate"], "2026-08-30");
        assert!(cue["memo"].as_str().unwrap().contains("인도: 윤명중 목사"));
        let items = cue["items"].as_array().unwrap();
        let titles: Vec<&str> = items.iter().map(|x| x["title"].as_str().unwrap()).collect();
        assert_eq!(titles, ["은혜", "광고", "꽃들도", "나뉜 곡", "봉독", "PPT 곡"], "order 순서 · 자유 항목은 앞 곡 뒤 · 나뉜 곡은 하나 · 이름 없는 자유 항목은 파일 이름");
        // 콘티 가사 그대로 · 송폼 → 부를 순서 · Key 등은 진행 메모
        assert_eq!(items[0]["type"], "song");
        assert_eq!(items[0]["order"], "1절 후렴 2절 후렴 브릿지 후렴");
        assert!(items[0]["note"].as_str().unwrap().starts_with("Key A · 4/4 · BPM 70 · 송폼 Intro"));
        // 없는 PPT → 제목 순서 + 경고
        assert_eq!(items[1]["type"], "title");
        // 가사 없는 곡 → 라이브러리 가사 · 콘티 송폼 우선
        assert_eq!(items[2]["songId"], "s1");
        assert_eq!(items[2]["order"], "1절 후렴 1절 후렴 후렴 후렴");
        assert!(items[2]["note"].as_str().unwrap().ends_with("/ 전조 후 웅장하게"));
        // 송폼이 기본값이면 라이브러리 부를 순서
        assert_eq!(items[3]["order"], "1절 후렴 후렴");
        // 연결 PPT 가 이 PC 에 없으면 가사로
        assert_eq!(items[5]["type"], "song");
        assert_eq!(items[5]["lyrics"], "[1절]\n가사");
        assert_eq!(warnings.len(), 3, "{warnings:?}");
    }

    /// 실제 콘티 파일 점검: WP_CONTI_FILE=<.worship 경로> cargo test -p wp-core real_conti_file -- --nocapture
    #[test]
    fn real_conti_file() {
        let Ok(p) = std::env::var("WP_CONTI_FILE") else { return };
        let t = std::fs::read_to_string(&p).unwrap();
        let v: Value = serde_json::from_str(t.trim_start_matches('\u{feff}')).unwrap();
        let (cue, w) = match to_cue(&v, "파일", &|_: &str| None) {
            Ok(x) => x,
            Err(e) => return println!("가져오지 않음: {e}"),
        };
        println!("{}", serde_json::to_string_pretty(&json!({ "name": cue["name"], "serviceDate": cue["serviceDate"], "memo": cue["memo"],
            "items": cue["items"].as_array().unwrap().iter().map(|x| json!([x["type"], x["title"], x["order"], x["note"]])).collect::<Vec<_>>(), "warnings": w })).unwrap());
    }

    #[test]
    fn missing_lyrics_and_real_ppt() {
        let dir = std::env::temp_dir().join(format!("wp-conti-{}", crate::util::short_id("t")));
        std::fs::create_dir_all(&dir).unwrap();
        let ppt = dir.join("010 전능왕 오셔서.pptx");
        std::fs::write(&ppt, b"x").unwrap();
        let v = json!({ "service_info": {}, "items": [
            { "order": 1, "item_type": "SONG", "title": "모르는 곡", "key": "G" },
            { "order": 2, "item_type": "SONG", "title": "전능왕", "ppt_path": ppt.to_string_lossy(), "key": "F" }
        ]});
        let (cue, w) = to_cue(&v, "파일이름", &|_: &str| None).unwrap();
        let items = cue["items"].as_array().unwrap();
        assert_eq!(cue["name"], "파일이름");
        assert_eq!(items[0]["type"], "title");
        assert_eq!(items[0]["note"], "Key G");
        assert_eq!(w.len(), 1);
        assert_eq!(items[1]["type"], "hymn");
        assert_eq!(items[1]["title"], "전능왕");
        assert_eq!(items[1]["file"], ppt.to_string_lossy().as_ref());
        let _ = std::fs::remove_dir_all(dir);
    }
}
