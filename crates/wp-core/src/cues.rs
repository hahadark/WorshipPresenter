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

/// 진행 시계: 시작 시각 "HH:MM" + 순서별 duration(분) → 순서마다 예정 시각
pub fn planned_times(start: &str, items: &[Value]) -> Vec<String> {
    let parsed = start.split_once(':').and_then(|(h, m)| Some((h.trim().parse::<i64>().ok()?, m.trim().parse::<i64>().ok()?)));
    let Some((h, m)) = parsed else { return vec![String::new(); items.len()] };
    let mut t = h * 60 + m;
    items
        .iter()
        .map(|it| {
            let s = format!("{:02}:{:02}", (t / 60) % 24, t % 60);
            t += it["duration"].as_f64().unwrap_or(0.0).round() as i64;
            s
        })
        .collect()
}

/// 인쇄용 HTML (브라우저로 열어 인쇄·PDF 저장)
pub fn print_html(c: &Cue) -> String {
    let esc = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    let times = planned_times(&c.start_time, &c.items);
    let rows: String = c
        .items
        .iter()
        .enumerate()
        .map(|(i, it)| {
            let sub = it["ref"].as_str().filter(|x| !x.is_empty()).map(String::from).or_else(|| it["number"].as_i64().filter(|n| *n > 0).map(|n| format!("{n}장"))).unwrap_or_default();
            let dur = it["duration"].as_f64().filter(|d| *d > 0.0).map(|d| format!("{d}분")).unwrap_or_default();
            format!("<tr><td>{}</td><td>{}</td><td><b>{}</b> {}</td><td>{}</td><td>{}</td></tr>", i + 1, times[i], esc(it["title"].as_str().unwrap_or("")), esc(&sub), esc(it["note"].as_str().unwrap_or("")), dur)
        })
        .collect();
    format!(
        "<!doctype html><html lang=\"ko\"><meta charset=\"utf-8\"><title>{0}</title><style>body{{font-family:'Malgun Gothic',sans-serif;margin:24px}}table{{border-collapse:collapse;width:100%}}td,th{{border-bottom:1px solid #ccc;padding:6px 8px;text-align:left}}</style><body onload=\"print()\"><h1>{0}</h1><p>{1} {2}</p><p>{3}</p><table><tr><th>#</th><th>시각</th><th>순서</th><th>메모</th><th>시간</th></tr>{4}</table></body></html>",
        esc(&c.name), esc(&c.service_date), esc(&c.start_time), esc(&c.memo), rows
    )
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
