//! [트랙 B] 큐시트 관리: 휴지통·되살리기·비우기 · 복제 · 내보내기/가져오기 · 메모·고정·서식 · 실행 취소
//! 진행 시계(순서별 duration · 시작 시각) · 인쇄용 HTML

use crate::db::{BibleStore, Cue};
use crate::util::{iso_now, short_id};
use anyhow::{bail, Result};
use rusqlite::params;
use serde_json::{json, Value};

pub fn list_trash(store: &BibleStore) -> Vec<Value> {
    let Ok(mut st) = store.db.prepare("SELECT id, name, COALESCE(service_date,''), deleted_at FROM Cuesheets WHERE deleted_at != '' ORDER BY deleted_at DESC") else { return vec![] };
    st.query_map([], |r| Ok(json!({ "id": r.get::<_, String>(0)?, "name": r.get::<_, String>(1)?, "serviceDate": r.get::<_, String>(2)?, "deletedAt": r.get::<_, String>(3)? })))
        .map(|it| it.filter_map(|x| x.ok()).collect())
        .unwrap_or_default()
}

/// 메타만 바꾸기: name · serviceDate · startTime · memo · pinned · template · deletedAt
pub fn update_meta(store: &BibleStore, id: &str, m: &Value) -> Result<()> {
    for (k, col) in [("name", "name"), ("serviceDate", "service_date"), ("startTime", "start_time"), ("memo", "memo"), ("deletedAt", "deleted_at")] {
        if let Some(v) = m[k].as_str() {
            store.db.execute(&format!("UPDATE Cuesheets SET {col} = ? WHERE id = ?"), params![v, id])?;
        }
    }
    for (k, col) in [("pinned", "pinned"), ("template", "template")] {
        if let Some(v) = m[k].as_bool() {
            store.db.execute(&format!("UPDATE Cuesheets SET {col} = ? WHERE id = ?"), params![v as i64, id])?;
        }
    }
    Ok(())
}

pub fn trash(store: &BibleStore, id: &str) -> Result<()> {
    update_meta(store, id, &json!({ "deletedAt": iso_now() }))
}
pub fn restore(store: &BibleStore, id: &str) -> Result<()> {
    update_meta(store, id, &json!({ "deletedAt": "" }))
}
/// 휴지통 비우기 (id 가 있으면 그것만)
pub fn purge(store: &BibleStore, id: Option<&str>) -> Result<usize> {
    Ok(match id {
        Some(i) => store.db.execute("DELETE FROM Cuesheets WHERE id = ? AND deleted_at != ''", [i])?,
        None => store.db.execute("DELETE FROM Cuesheets WHERE deleted_at != ''", [])?,
    })
}

fn fresh_items(items: &[Value]) -> Vec<Value> {
    items
        .iter()
        .map(|it| {
            let mut x = it.clone();
            x["id"] = json!(short_id("i"));
            x
        })
        .collect()
}

/// 복제 (서식이면 오늘 날짜로 새 큐시트)
pub fn duplicate(store: &BibleStore, id: &str, name: Option<&str>) -> Result<Cue> {
    let Some(c) = store.get_cue(id) else { bail!("큐시트를 찾을 수 없습니다.") };
    let n = Cue {
        id: short_id("c"),
        name: name.map(String::from).unwrap_or_else(|| if c.template { format!("{} 예배", &iso_now()[..10]) } else { format!("{} 복사본", c.name) }),
        service_date: if c.template { iso_now()[..10].to_string() } else { c.service_date.clone() },
        start_time: c.start_time.clone(),
        items: fresh_items(&c.items),
        memo: c.memo.clone(),
        pinned: false,
        template: false,
    };
    store.save_cue(&n)?;
    Ok(n)
}

/// 내보내기: { format, version, cues: [...] }
pub fn export(store: &BibleStore, ids: &[String]) -> Value {
    let cues: Vec<Value> = ids.iter().filter_map(|i| store.get_cue(i)).map(|c| serde_json::to_value(&c).unwrap_or(Value::Null)).collect();
    json!({ "format": "worshipcue-cues", "version": 1, "exportedAt": iso_now(), "cues": cues })
}

/// 가져오기: 새 id 로 저장 → 가져온 수
/// 워십프리젠터 큐시트 파일인가 (내보낸 큐시트 하나 · 여러 개). 순서마다 type 이 있어야 한다
/// — 다른 프로그램의 { items: [곡…] } 을 큐시트로 잘못 읽지 않게
pub fn is_cue_file(data: &Value) -> bool {
    let typed = |items: &Value| items.as_array().is_some_and(|a| a.iter().all(|it| it["type"].is_string()));
    match data["cues"].as_array() {
        Some(a) => a.iter().all(|c| typed(&c["items"])),
        None => typed(&data["items"]),
    }
}

pub fn import(store: &BibleStore, data: &Value) -> Result<usize> {
    let list: Vec<Value> = if let Some(a) = data["cues"].as_array() {
        a.clone()
    } else if data["items"].is_array() {
        vec![data.clone()]
    } else {
        bail!("큐시트 파일 형식이 아닙니다.")
    };
    let mut n = 0;
    for c in list {
        let items = c["items"].as_array().cloned().unwrap_or_default();
        let s = |k: &str, d: &str| c[k].as_str().unwrap_or(d).to_string();
        let cue = Cue { id: short_id("c"), name: s("name", "가져온 큐시트"), service_date: s("serviceDate", ""), start_time: s("startTime", ""), items: fresh_items(&items), memo: s("memo", ""), pinned: false, template: c["template"].as_bool().unwrap_or(false) };
        store.save_cue(&cue)?;
        n += 1;
    }
    Ok(n)
}

/// 진행 시계: 시작 시각 "HH:MM" + 순서별 duration(분) → 순서마다 예정 시각.
/// Electron cue.js planMinutes · hm 과 같은 셈: 분은 소수 그대로 더하고(2.5분 + 2.5분 = 5분) 보일 때만 반올림, 글자로 적힌 숫자("5")도 센다,
/// 시작 시각은 `시:분` 꼴(시 1~2자리 · 분 2자리)만 받는다.
pub fn planned_times(start: &str, items: &[Value]) -> Vec<String> {
    let parsed = start.split_once(':').filter(|(h, m)| matches!(h.len(), 1 | 2) && m.len() == 2 && h.bytes().chain(m.bytes()).all(|b| b.is_ascii_digit())).and_then(|(h, m)| Some((h.parse::<f64>().ok()?, m.parse::<f64>().ok()?)));
    let Some((h, m)) = parsed else { return vec![String::new(); items.len()] };
    let mut t = h * 60.0 + m;
    items
        .iter()
        .map(|it| {
            // JS Math.round: 반은 올림
            let s = format!("{:02}:{:02}", ((t / 60.0).floor() as i64) % 24, ((t + 0.5).floor() as i64) % 60);
            let d = match &it["duration"] { Value::String(x) => x.trim().parse::<f64>().unwrap_or(0.0), v => v.as_f64().unwrap_or(0.0) };
            t += if d.is_finite() { d } else { 0.0 };
            s
        })
        .collect()
}

/// Electron 인쇄 창과 같은 여섯 옵션. 누락된 설정은 원본 기본값을 따른다.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct PrintOptions {
    pub lyrics: bool,
    pub bible: bool,
    pub notes: bool,
    pub chords: bool,
    pub big: bool,
    pub cols: bool,
}
impl Default for PrintOptions {
    fn default() -> Self { Self { lyrics: true, bible: false, notes: true, chords: false, big: false, cols: false } }
}

/// 기본 옵션으로 인쇄한다. 기존 호출자와의 호환용이다.
pub fn print_html(c: &Cue) -> String {
    print_html_with(c, &PrintOptions::default(), |_| Ok(Vec::new()))
}

/// 순서 자료와 성경 조회 결과를 합친다. 조회를 분리해 격리된 DB로도 시험할 수 있다.
pub fn print_html_with(c: &Cue, o: &PrintOptions, mut bible: impl FnMut(&Value) -> Result<Vec<Value>>) -> String {
    use crate::{groups, slides};
    use crate::i18n::{tr, tr_args};
    let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;");
    let clean = |s: &str| {
        let s = groups::strip_fmt(s);
        let s = if o.chords { s } else { groups::strip_chords(&s) };
        s.lines().map(|l| l.trim_start().strip_prefix('>').map(|x| x.strip_prefix(' ').unwrap_or(x)).unwrap_or(l)).collect::<Vec<_>>().join("\n")
    };
    let times = planned_times(&c.start_time, &c.items);
    let mut rows = String::new();
    for (i, it) in c.items.iter().enumerate() {
        let s = |k: &str| it[k].as_str().unwrap_or("");
        let mut body = String::new();
        match s("type") {
            "song" if o.lyrics => {
                let order = s("order");
                if !order.trim().is_empty() { body.push_str(&format!("<div class=\"ord\">{}: {}</div>", esc(&tr("부를 순서")), esc(order))); }
                let mut last = String::new();
                for chunk in slides::arrange(slides::lyrics_chunks(s("lyrics")), order) {
                    if !chunk.label.is_empty() && chunk.label != last {
                        body.push_str(&format!("<div class=\"lab\">{}</div>", esc(&chunk.label)));
                        last = chunk.label;
                    }
                    body.push_str(&format!("<p>{}</p>", esc(&clean(&chunk.text)).replace('\n', "<br>")));
                }
            }
            "text" if o.lyrics => {
                for chunk in slides::text_chunks(s("body"), 1) {
                    body.push_str(&format!("<p>{}</p>", esc(&clean(&chunk.text)).replace('\n', "<br>")));
                }
            }
            "bible" if o.bible => match bible(it) {
                Ok(verses) => {
                    body.push_str("<p class=\"bib\">");
                    for v in verses {
                        body.push_str(&format!("<sup>{}</sup>{} ", v["verse"], esc(v["text"].as_str().unwrap_or(""))));
                    }
                    body.push_str("</p>");
                }
                Err(e) => body.push_str(&format!("<p class=\"note\">{}</p>", esc(&e.to_string()))),
            },
            _ => {}
        }
        let sub = match s("type") { "bible" => s("ref").to_string(), "title" => s("subtitle").to_string(), "hymn" if !s("range").is_empty() => format!("{} {}", tr("슬라이드"), s("range")), _ => String::new() };
        let kind = match s("type") { "song" => "찬양", "text" => "공지", "bible" => "성경", "title" => "제목", "hymn" => "찬송가", _ => "" };
        rows.push_str(&format!("<section class=\"it{}\"><div class=\"hd\"><span class=\"no\">{}</span><span class=\"tm\">{}</span><b>{}</b><span class=\"sub\">{}</span><span class=\"ty\">{}</span></div>", if body.is_empty() { "" } else { " hasbody" }, i+1, times[i], esc(s("title")), esc(&sub), esc(&tr(kind))));
        if o.notes && !s("note").is_empty() { rows.push_str(&format!("<div class=\"note\">{}: {}</div>", esc(&tr("메모")), esc(s("note")))); }
        rows.push_str(&body);
        rows.push_str("</section>");
    }
    let columns = if o.cols { ".it.hasbody p,.it.hasbody .lab{margin-left:0}.hasbody{column-count:2;column-gap:8mm}.hasbody .hd,.hasbody .note,.hasbody .ord{column-span:all}" } else { "" };
    format!(r#"<!doctype html><html lang="ko"><head><meta charset="utf-8"><title>{}</title><style>
@page{{size:A4;margin:14mm 12mm}}body{{font-family:'Malgun Gothic',sans-serif;color:#111;font-size:{}pt;line-height:1.45;margin:0}}@media screen{{body{{padding:10mm 12mm}}}}
h1{{font-size:16pt;margin:0 0 2px}}.meta{{color:#555;margin-bottom:10px;font-size:10pt}}.it{{border-top:1px solid #ccc;padding:6px 0}}.hd{{display:flex;gap:8px;align-items:baseline;break-after:avoid}}.no{{font-weight:800;min-width:1.6em}}.tm{{color:#0a58ca;font-variant-numeric:tabular-nums}}.sub{{color:#444}}.ty{{margin-left:auto;color:#888;font-size:9pt}}.note{{color:#a33;font-size:10pt;margin:2px 0 0 2.2em}}p{{margin:3px 0 6px 2.2em;break-inside:avoid}}.lab{{margin:6px 0 0 2.2em;font-weight:700;color:#555;font-size:9.5pt}}.ord{{margin:2px 0 0 2.2em;color:#555;font-size:9.5pt}}.bib sup{{color:#888;margin-right:2px}}{}
@media print{{.print-button{{display:none}}}}</style></head><body><button class="print-button" onclick="window.print()">{}</button><h1>{}</h1><div class="meta">{} {} · {} · {}</div>{}{}</body></html>"#,
        esc(&c.name), if o.big {13} else {11}, columns, esc(&tr("인쇄…")), esc(&c.name), esc(&c.service_date), esc(&c.start_time), esc(&tr_args("순서 {1}개", &[&c.items.len()])), esc(&tr("워십프리젠터")),
        if o.notes && !c.memo.is_empty() {format!("<p class=\"note\">{}</p>",esc(&c.memo))} else {String::new()}, rows)
}

/// 실행 취소 (순서 목록 사본, 큐시트별)
#[derive(Default)]
pub struct Undo {
    back: Vec<(String, Vec<Value>)>,
    fwd: Vec<(String, Vec<Value>)>,
}
impl Undo {
    pub fn push(&mut self, cue_id: &str, items: &[Value]) {
        self.back.push((cue_id.to_string(), items.to_vec()));
        if self.back.len() > 100 {
            self.back.remove(0);
        }
        self.fwd.clear();
    }
    pub fn undo(&mut self, cue_id: &str, cur: &[Value]) -> Option<Vec<Value>> {
        let i = self.back.iter().rposition(|x| x.0 == cue_id)?;
        let (_, items) = self.back.remove(i);
        self.fwd.push((cue_id.to_string(), cur.to_vec()));
        Some(items)
    }
    /// 되돌리면(redo 면 다시 하면) 돌아갈 모습 (없으면 None)
    pub fn peek(&self, cue_id: &str, redo: bool) -> Option<&[Value]> {
        let v = if redo { &self.fwd } else { &self.back };
        v.iter().rev().find(|x| x.0 == cue_id).map(|x| x.1.as_slice())
    }
    /// 이 큐시트에서 더 되돌리거나(redo) 다시 할 수 있는 횟수
    pub fn depth(&self, cue_id: &str, redo: bool) -> usize {
        let v = if redo { &self.fwd } else { &self.back };
        v.iter().filter(|x| x.0 == cue_id).count()
    }
    pub fn redo(&mut self, cue_id: &str, cur: &[Value]) -> Option<Vec<Value>> {
        let i = self.fwd.iter().rposition(|x| x.0 == cue_id)?;
        let (_, items) = self.fwd.remove(i);
        self.back.push((cue_id.to_string(), cur.to_vec()));
        Some(items)
    }
}

/// 순서 목록이 `before` 에서 `after` 로 바뀐 일을 짧은 말로 (되돌리기 단추 툴팁·알림용).
/// 예: "'말씀 봉독' 순서 삭제" · "'찬양' 순서 넣기" · "'설교' 순서 옮기기" · "'광고' 고치기"
pub fn describe_change(before: &[Value], after: &[Value]) -> String {
    use crate::i18n::{tr, tr_args};
    let id = |x: &Value| x["id"].as_str().unwrap_or("").to_string();
    let name = |x: &Value| x["title"].as_str().map(str::trim).filter(|s| !s.is_empty()).map(String::from).unwrap_or_else(|| tr("제목 없는 순서"));
    let (a, b): (Vec<String>, Vec<String>) = (before.iter().map(id).collect(), after.iter().map(id).collect());
    let one = |list: Vec<&Value>, single: &str, many: &str| match list.as_slice() {
        [x] => tr_args(single, &[&name(x)]),
        xs => tr_args(many, &[&xs.len()]),
    };
    if after.len() < before.len() {
        return one(before.iter().filter(|x| !b.contains(&id(x))).collect(), "'{1}' 순서 삭제", "순서 {1}개 삭제");
    }
    if after.len() > before.len() {
        return one(after.iter().filter(|x| !a.contains(&id(x))).collect(), "'{1}' 순서 넣기", "순서 {1}개 넣기");
    }
    let mut sa = a.clone();
    let mut sb = b.clone();
    sa.sort();
    sb.sort();
    if a != b && sa == sb {
        // 같은 순서들의 자리만 바뀜: 자리가 가장 많이 바뀐 순서를 옮긴 것으로 본다
        let moved = before.iter().enumerate().max_by_key(|(i, x)| b.iter().position(|y| *y == id(x)).map(|j| j.abs_diff(*i)).unwrap_or(0)).map(|(_, x)| x);
        return moved.map(|x| tr_args("'{1}' 순서 옮기기", &[&name(x)])).unwrap_or_else(|| tr("순서 옮기기"));
    }
    match before.iter().zip(after).position(|(x, y)| x != y) {
        Some(i) => tr_args("'{1}' 고치기", &[&name(&after[i])]),
        None => tr("순서 변경"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn describe_change_names_what_changed() {
        let a = json!({ "id": "a", "title": "말씀 봉독" });
        let b = json!({ "id": "b", "title": "찬양" });
        let c = json!({ "id": "c", "title": "설교" });
        let all = vec![a.clone(), b.clone(), c.clone()];
        assert_eq!(describe_change(&all, &[b.clone(), c.clone()]), "'말씀 봉독' 순서 삭제");
        assert_eq!(describe_change(&all, &[c.clone()]), "순서 2개 삭제");
        assert_eq!(describe_change(&[a.clone(), c.clone()], &all), "'찬양' 순서 넣기");
        assert_eq!(describe_change(&all, &[c.clone(), a.clone(), b.clone()]), "'설교' 순서 옮기기");
        let mut b2 = b.clone();
        b2["title"] = json!("찬양 2");
        assert_eq!(describe_change(&all, &[a.clone(), b2, c.clone()]), "'찬양 2' 고치기");
        assert_eq!(describe_change(&all, &all), "순서 변경");
    }
    #[test]
    fn undo_peek_and_depth() {
        let mut u = Undo::default();
        assert!(u.peek("c", false).is_none());
        u.push("c", &[json!({ "id": "a" })]);
        u.push("x", &[]);
        assert_eq!(u.depth("c", false), 1);
        assert_eq!(u.peek("c", false).unwrap().len(), 1);
        u.undo("c", &[]);
        assert_eq!(u.depth("c", false), 0);
        assert_eq!(u.depth("c", true), 1);
        assert!(u.peek("c", true).unwrap().is_empty());
    }
    fn print_fixture() -> Cue {
        Cue { id: "인쇄시험".into(), name: "주일 <예배>".into(), service_date: "2026-11-15".into(), start_time: "11:00".into(),
            items: vec![json!({"type":"song","title":"찬양","lyrics":"[1절]\n[G]첫가사\n\n[후렴]\n[Am7]후렴가사", "order":"1 C 1", "note":"찬양메모", "duration":3}),
                json!({"type":"text","title":"광고","body":"공지본문 <script>위험</script>\n---\n**다음공지**"}),
                json!({"type":"bible","title":"봉독","ref":"요3:16","versionId":"인쇄시험본"})],
            memo:"전체메모".into(), pinned:false, template:false }
    }
    #[test]
    fn print_content_options_and_escaping() {
        let c = print_fixture();
        let mut o = PrintOptions::default();
        let h = print_html_with(&c, &o, |_| panic!("성경 옵션을 끄면 DB를 조회하지 않는다"));
        assert_eq!(h.matches("첫가사").count(), 2);
        assert!(h.contains("후렴가사") && h.contains("공지본문"));
        assert!(h.contains("찬양메모") && h.contains("전체메모"));
        assert!(!h.contains("[G]") && !h.contains("[Am7]"));
        assert!(h.contains("&lt;script&gt;") && !h.contains("<script>"));
        assert!(h.contains("주일 &lt;예배&gt;") && h.contains("2026-11-15") && h.contains("11:03"));
        o.chords = true;
        let h = print_html_with(&c, &o, |_| Ok(vec![]));
        assert!(h.contains("[G]") && h.contains("[Am7]"));
        o.lyrics = false;
        o.notes = false;
        let h = print_html_with(&c, &o, |_| Ok(vec![]));
        for absent in ["첫가사", "후렴가사", "공지본문", "다음공지", "찬양메모", "전체메모"] { assert!(!h.contains(absent), "{absent}"); }
        assert!(h.contains("찬양") && h.contains("요3:16"));
    }
    #[test]
    fn print_layout_and_defaults() {
        let c = print_fixture();
        let mut o: PrintOptions = serde_json::from_value(json!({})).unwrap();
        assert!(o.lyrics && o.notes && !o.bible && !o.chords && !o.big && !o.cols);
        let h = print_html_with(&c, &o, |_| Ok(vec![]));
        assert!(h.contains("font-size:11pt") && !h.contains("column-count:2"));
        o.big = true; o.cols = true;
        let h = print_html_with(&c, &o, |_| Ok(vec![]));
        assert!(h.contains("font-size:13pt") && h.contains("column-count:2"));
        let saved = serde_json::to_value(&o).unwrap();
        let restored: PrintOptions = serde_json::from_value(saved).unwrap();
        assert!(restored.big && restored.cols);
    }
    #[test]
    fn print_bible_reads_isolated_database() {
        let mut store = store();
        let data = crate::importer::Data { verses: vec![crate::importer::Row { book:43, chapter:3, verse:16, text:"인쇄시험 성경본문 <본문>".into() }], ..Default::default() };
        crate::importer::replace_version(&store.db, &json!({"id":"인쇄시험본","name":"시험"}), &data).unwrap();
        store.reload_structure().unwrap();
        let c = print_fixture();
        let mut o = PrintOptions::default(); o.bible = true;
        let h = print_html_with(&c, &o, |it| {
            let r = store.resolve_reference(it["versionId"].as_str().unwrap(), it["ref"].as_str().unwrap());
            assert_eq!(r["ok"], true);
            Ok(r["passage"]["verses"].as_array().unwrap().clone())
        });
        assert!(h.contains("<sup>16</sup>인쇄시험 성경본문 &lt;본문&gt;"));
        o.bible = false;
        assert!(!print_html_with(&c, &o, |_| panic!("조회 금지")).contains("성경본문"));
    }
    fn store() -> BibleStore {
        let p = std::env::temp_dir().join(format!("wp-cues-{}.db", short_id("t")));
        BibleStore::open(&p).unwrap()
    }
    #[test]
    fn trash_dup_export() {
        let s = store();
        let c = Cue { id: "c1".into(), name: "주일".into(), service_date: "2026-11-15".into(), start_time: "11:00".into(), items: vec![json!({ "id": "a", "type": "title", "title": "예배로 부름", "duration": 3 }), json!({ "id": "b", "type": "bible", "title": "봉독", "ref": "시 23", "duration": 5 })], memo: String::new(), pinned: false, template: false };
        s.save_cue(&c).unwrap();
        let d = duplicate(&s, "c1", None).unwrap();
        assert_eq!(d.name, "주일 복사본");
        assert_ne!(d.items[0]["id"], "a");
        trash(&s, "c1").unwrap();
        assert_eq!(list_trash(&s).len(), 1);
        assert_eq!(s.list_cues().len(), 1);
        restore(&s, "c1").unwrap();
        assert_eq!(s.list_cues().len(), 2);
        update_meta(&s, "c1", &json!({ "pinned": true, "memo": "성찬" })).unwrap();
        assert_eq!(s.get_cue("c1").unwrap().memo, "성찬");
        let ex = export(&s, &["c1".into()]);
        assert_eq!(import(&s, &ex).unwrap(), 1);
        assert_eq!(s.list_cues().len(), 3);
        assert_eq!(planned_times("11:00", &c.items), vec!["11:00", "11:03"]);
        // [점검 A] Electron 과 같은 셈: 소수 분은 그대로 더함 · 글자 숫자 · 자정 넘김 · 틀린 시작 시각
        let d = |v: Value| json!({ "type": "title", "duration": v });
        assert_eq!(planned_times("9:05", &[d(json!(2.5)), d(json!(2.5)), d(json!("5")), d(json!(null)), d(json!(0))]), vec!["09:05", "09:08", "09:10", "09:15", "09:15"]);
        assert_eq!(planned_times("23:50", &[d(json!(15)), d(json!(600)), d(json!(1))]), vec!["23:50", "00:05", "10:05"]);
        assert_eq!(planned_times("", &[d(json!(5))]), vec![""]);
        assert_eq!(planned_times("10:5", &[d(json!(5))]), vec![""]);
        assert_eq!(planned_times("오전 10:30", &[d(json!(5))]), vec![""]);
        assert!(print_html(&c).contains("11:03"));
        trash(&s, "c1").unwrap();
        assert_eq!(purge(&s, None).unwrap(), 1);
    }
    #[test]
    fn undo() {
        let mut u = Undo::default();
        u.push("c", &[json!(1)]);
        let back = u.undo("c", &[json!(1), json!(2)]).unwrap();
        assert_eq!(back.len(), 1);
        assert_eq!(u.redo("c", &back).unwrap().len(), 2);
        assert!(u.undo("x", &[]).is_none());
    }
}
