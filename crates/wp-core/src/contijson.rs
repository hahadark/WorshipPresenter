//! 다른 콘티 프로그램이 내보낸 .json 가져오기 (형식 자료가 없는 프로그램들을 위한 '너그러운' 읽기).
//! 특정 프로그램의 코드·형식을 옮기지 않았다. JSON 안을 훑어 '제목 + (가사 · Key · BPM · 부른 이)' 를 가진 묶음을 곡으로 본다.
//!  - 곡 목록이 든 배열(songs · items · list · setlist · 곡 …)이 있으면 그 차례대로 콘티(큐시트)로
//!  - 가사는 글 하나이거나, 조각 배열(글 · {이름표, 글|줄[]})이어도 된다 → "[1절]\n…\n\n[후렴]\n…"
//!  - 맨 위의 이름·날짜(title · name · date · serviceDate · 날짜 …)는 큐시트 이름·예배 날짜로
//!
//! 우리 큐시트 파일(.json, items 에 type)과 찬양 콘티 스튜디오(.worship)는 이 모듈 앞에서 먼저 걸러진다.

use serde_json::{json, Value};

const TITLE: [&str; 10] = ["title", "songTitle", "song_title", "name", "songName", "song_name", "제목", "곡명", "곡제목", "찬양명"];
const LYRICS: [&str; 14] = ["lyrics", "lyric", "words", "text", "content", "body", "가사", "lyricsText", "lyrics_text", "sections", "parts", "verses", "slides", "stanzas"];
const KEY: [&str; 6] = ["key", "songKey", "song_key", "originalKey", "키", "조"];
const BPM: [&str; 4] = ["bpm", "tempo", "템포", "빠르기"];
const METER: [&str; 5] = ["meter", "timeSignature", "time_signature", "박자", "beat"];
const ORDER: [&str; 8] = ["order", "form", "songForm", "song_form", "flow", "sequence", "송폼", "부를순서"];
const ARTIST: [&str; 8] = ["artist", "author", "singer", "composer", "writer", "credit", "작곡", "가수"];
const NOTE: [&str; 5] = ["memo", "note", "notes", "comment", "메모"];
/// 이 이름의 배열 안은 곡이 아니라 한 곡의 가사 조각
const SECTIONS: [&str; 8] = ["sections", "parts", "verses", "slides", "lyrics", "lines", "가사", "stanzas"];
const DATE: [&str; 5] = ["date", "serviceDate", "service_date", "worshipDate", "날짜"];

/// 읽은 곡 하나
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ContiSong {
    pub title: String,
    pub lyrics: String,
    pub key: String,
    pub bpm: String,
    pub meter: String,
    pub order: String,
    pub credit: String,
    pub note: String,
}

fn get<'a>(o: &'a serde_json::Map<String, Value>, keys: &[&str]) -> Option<&'a Value> {
    keys.iter().find_map(|k| o.get(*k).filter(|v| !v.is_null())).or_else(|| {
        // 대소문자 · 밑줄 차이는 너그럽게
        let norm = |s: &str| s.to_lowercase().replace(['_', '-', ' '], "");
        let want: Vec<String> = keys.iter().map(|k| norm(k)).collect();
        o.iter().find(|(k, v)| !v.is_null() && want.contains(&norm(k))).map(|(_, v)| v)
    })
}

fn scalar(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.trim().to_string(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

/// 가사 값 → 가사 글 (글 · 글 배열 · 조각 배열)
fn lyrics_of(v: &Value) -> String {
    match v {
        Value::String(s) => crate::songimport::clean(s),
        Value::Array(a) => {
            let mut blocks: Vec<String> = vec![];
            for x in a {
                match x {
                    Value::String(s) => blocks.push(s.trim().to_string()),
                    Value::Object(o) => {
                        let label = scalar(get(o, &["label", "name", "type", "section", "title", "tag", "이름표"]));
                        let body = get(o, &["text", "lyrics", "lines", "content", "words", "가사"]).map(lyrics_of).unwrap_or_default();
                        if body.is_empty() {
                            continue;
                        }
                        blocks.push(if label.is_empty() { body } else { format!("[{}]\n{body}", crate::songimport::label_of(&label)) });
                    }
                    _ => {}
                }
            }
            // 글만 든 배열이 한 줄씩이면(줄 목록) 한 조각으로, 여러 줄 글이면 조각마다 빈 줄
            let one_line = a.iter().all(|x| x.as_str().is_some_and(|s| !s.trim().contains('\n')));
            crate::songimport::clean(&blocks.join(if one_line { "\n" } else { "\n\n" }))
        }
        _ => String::new(),
    }
}

/// 부를 순서 값 → 워십프리젠터 부를 순서 ("V1 - C - V2" · ["v1","c"])
fn order_of(v: Option<&Value>) -> String {
    let form = match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(a)) => a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(" - "),
        _ => return String::new(),
    };
    if form.trim().is_empty() {
        return String::new();
    }
    let sep = if form.contains('-') || form.contains('→') || form.contains(',') { form.replace(['→', ','], "-") } else { form.split_whitespace().collect::<Vec<_>>().join(" - ") };
    crate::contistudio::form_to_order(&sep)
}

fn song_of(o: &serde_json::Map<String, Value>, in_list: bool) -> Option<ContiSong> {
    let title = scalar(get(o, &TITLE));
    if title.is_empty() {
        return None;
    }
    let lyrics = get(o, &LYRICS).map(lyrics_of).unwrap_or_default();
    let s = ContiSong {
        title: title.chars().take(200).collect(),
        lyrics,
        key: scalar(get(o, &KEY)),
        bpm: scalar(get(o, &BPM)),
        meter: scalar(get(o, &METER)),
        order: order_of(get(o, &ORDER)),
        credit: scalar(get(o, &ARTIST)),
        note: scalar(get(o, &NOTE)),
    };
    // 목록 안의 묶음은 제목만 있어도 곡 (콘티는 곡 이름만 적는 일이 많다). 그 밖은 곡다운 칸이 하나는 있어야
    let songish = !s.lyrics.is_empty() || !s.key.is_empty() || !s.bpm.is_empty() || !s.credit.is_empty();
    (in_list || songish).then_some(s)
}

fn walk(v: &Value, parent_key: &str, in_list: bool, out: &mut Vec<ContiSong>, depth: usize) {
    if depth > 12 {
        return;
    }
    let sections = SECTIONS.iter().any(|k| k.eq_ignore_ascii_case(parent_key));
    match v {
        Value::Array(a) if !sections => {
            for x in a {
                walk(x, parent_key, true, out, depth + 1);
            }
        }
        Value::Object(o) => {
            let before = out.len();
            for (k, x) in o {
                if x.is_array() || x.is_object() {
                    walk(x, k, false, out, depth + 1);
                }
            }
            if out.len() == before && !sections {
                if let Some(s) = song_of(o, in_list) {
                    out.push(s);
                }
            }
        }
        _ => {}
    }
}

/// JSON 안의 곡들 (파일 차례대로)
pub fn songs(v: &Value) -> Vec<ContiSong> {
    let mut out = vec![];
    walk(v, "", false, &mut out, 0);
    out
}

/// 맨 위의 이름 · 예배 날짜 (YYYY-MM-DD 로, 2026.11.29 · 2026/11/29 도)
pub fn meta(v: &Value) -> (String, String) {
    let Some(o) = v.as_object() else { return (String::new(), String::new()) };
    let name = scalar(get(o, &["title", "name", "serviceName", "service_name", "conti", "contiName", "예배", "제목"]));
    let d = scalar(get(o, &DATE));
    let digits: Vec<&str> = d.split(|c: char| !c.is_ascii_digit()).filter(|x| !x.is_empty()).collect();
    let date = match digits.as_slice() {
        [y, m, dd, ..] if y.len() == 4 && m.len() <= 2 && dd.len() <= 2 => format!("{y}-{:0>2}-{:0>2}", m, dd),
        _ => String::new(),
    };
    (name, date)
}

/// 곡 → 찬양 라이브러리 형식 { title, lyrics, credit, arrangement, key, bpm, meter }
pub fn to_library(s: &ContiSong) -> Value {
    json!({ "title": s.title, "lyrics": s.lyrics, "credit": s.credit, "arrangement": s.order, "key": s.key, "bpm": s.bpm, "meter": s.meter })
}

/// 곡 → 큐시트 찬양 순서. 가사가 없으면 find(제목)으로 찬양 라이브러리에서
pub fn to_item(s: &ContiSong, find: crate::contistudio::SongFinder, warnings: &mut Vec<String>) -> Value {
    let mut x = crate::core::new_item("song", &s.title);
    let mut lyrics = s.lyrics.clone();
    let mut credit = s.credit.clone();
    if let Some(sg) = find(&s.title) {
        if lyrics.is_empty() {
            lyrics = sg["lyrics"].as_str().unwrap_or("").to_string();
            if credit.is_empty() {
                credit = sg["credit"].as_str().unwrap_or("").to_string();
            }
        }
        x["songId"] = sg["id"].clone();
    }
    if lyrics.is_empty() {
        warnings.push(crate::i18n::tr_args("'{1}': 가사가 없어 제목만 넣었습니다 (찬양 라이브러리에도 없음)", &[&s.title]));
    }
    x["lyrics"] = json!(lyrics);
    x["credit"] = json!(credit);
    x["order"] = json!(s.order);
    x["note"] = json!(s.note);
    for (k, v) in [("key", &s.key), ("bpm", &s.bpm), ("meter", &s.meter)] {
        if !v.is_empty() {
            x[k] = json!(v);
        }
    }
    x
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setlist_with_sections_and_meta() {
        let v = json!({
            "title": "주일 2부 찬양", "date": "2026.11.29",
            "songs": [
                { "title": "주 은혜임을", "key": "G", "bpm": 72, "form": "V1 - C - V2 - C",
                  "sections": [ { "label": "V1", "lines": ["첫 줄", "둘째 줄"] }, { "label": "Chorus", "text": "후렴 글" } ] },
                { "name": "제목만 있는 곡" },
                { "제목": "한글 칸 곡", "가사": "가사 한 줄\n\n두 번째 장", "키": "D" }
            ]
        });
        let s = songs(&v);
        assert_eq!(s.len(), 3, "{s:?}");
        assert_eq!(s[0].title, "주 은혜임을");
        assert_eq!(s[0].lyrics, "[1절]\n첫 줄\n둘째 줄\n\n[후렴]\n후렴 글");
        assert_eq!((s[0].key.as_str(), s[0].bpm.as_str()), ("G", "72"));
        assert_eq!(s[0].order, "1절 후렴 2절 후렴");
        assert_eq!(s[1].title, "제목만 있는 곡");
        assert!(s[1].lyrics.is_empty());
        assert_eq!((s[2].key.as_str(), s[2].lyrics.as_str()), ("D", "가사 한 줄\n\n두 번째 장"));
        assert_eq!(meta(&v), ("주일 2부 찬양".into(), "2026-11-29".into()));
    }

    #[test]
    fn nested_and_bare_array_and_non_song() {
        // 겉 묶음 안의 목록 · 대소문자 다른 열쇠
        let v = json!({ "data": { "conti": { "Items": [ { "SongTitle": "A", "Lyrics": ["한 줄", "두 줄"] } ] } } });
        let s = songs(&v);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].lyrics, "한 줄\n두 줄", "한 줄짜리 글 배열은 한 장");
        // 맨 배열
        assert_eq!(songs(&json!([{ "title": "X" }, { "title": "Y" }])).len(), 2);
        // 곡이 아닌 JSON: 제목뿐인 하나짜리 묶음(목록 밖)은 곡이 아니다
        assert!(songs(&json!({ "title": "설정", "theme": "dark" })).is_empty());
        assert!(songs(&json!({ "a": 1 })).is_empty());
    }

    #[test]
    fn item_uses_library_when_no_lyrics() {
        let s = ContiSong { title: "주 은혜임을".into(), key: "A".into(), ..Default::default() };
        let mut w = vec![];
        let found = |t: &str| (t == "주 은혜임을").then(|| json!({ "id": "s1", "lyrics": "라이브러리 가사", "credit": "작곡 홍길동" }));
        let it = to_item(&s, &found, &mut w);
        assert_eq!((it["lyrics"].as_str(), it["songId"].as_str(), it["key"].as_str()), (Some("라이브러리 가사"), Some("s1"), Some("A")));
        assert!(w.is_empty());
        let none = |_: &str| None;
        let it = to_item(&ContiSong { title: "없는 곡".into(), ..Default::default() }, &none, &mut w);
        assert_eq!(it["lyrics"], "");
        assert_eq!(w.len(), 1);
    }
}
