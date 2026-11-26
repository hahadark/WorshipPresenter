//! [갈래 C] 큐시트 관리 창의 코어 (Electron core.js listCues · newCue · duplicateCue · trashCues · restoreCues ·
//! purgeCues · exportCues · importCues · updateCueMeta · cueText, db.js listCues · purgeTrash).
//! 조작 화면 쪽은 wp-app cuemgr_ui.rs (Electron control/cuemanager.js).

use crate::core::{Core, Ev};
use crate::cues;
use crate::db::{BibleStore, Cue};
use crate::util::{iso_now, short_id};
use anyhow::{bail, Result};
use serde_json::{json, Value};

/// 목록에 보이는 순서 미리보기 수 (db.js: items.slice(0, 40))
const PREVIEW_MAX: usize = 40;
const DAYS: [&str; 7] = ["일", "월", "화", "수", "목", "금", "토"];

/// 1970-01-01 부터의 날 수 → (년, 월, 일)
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// "2026-11-15" → 1970-01-01 부터의 날 수 (모양이 다르면 None)
fn parse_date(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let n = |r: std::ops::Range<usize>| s.get(r).and_then(|x| x.parse::<i64>().ok());
    let (y, m, d) = (n(0..4)?, n(5..7)?, n(8..10)?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some(crate::localtime::days_from_civil(y, m, d))
}

fn fmt_date(days: i64) -> String {
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

/// 오늘 (이 PC 시간대)
pub fn today() -> String {
    fmt_date((crate::util::now_ms() + crate::localtime::local_offset_ms()).div_euclid(86_400_000))
}

/// 날짜의 요일 ("일"…"토"). 날짜 모양이 아니면 빈 글
pub fn day_of(date: &str) -> &'static str {
    match parse_date(date) {
        Some(d) => DAYS[(d + 4).rem_euclid(7) as usize], // 1970-01-01 은 목요일
        None => "",
    }
}

/// 날짜에 n 일 더하기 (모양이 다르면 오늘 기준)
pub fn add_days(date: &str, n: i64) -> String {
    let base = parse_date(date).unwrap_or_else(|| parse_date(&today()).unwrap_or(0));
    fmt_date(base + n)
}

/// '몇 분 전' (cuemanager.js ago). now_ms 기준
pub fn ago(iso: &str, now_ms: i64) -> String {
    match ago_parts(iso, now_ms) {
        Some((t, Some(n))) => crate::i18n::tr_args(t, &[&n]),
        Some((t, None)) => crate::i18n::tr(t),
        None => iso.chars().take(10).collect(),
    }
}

/// (문구 틀, 숫자). 30일이 넘었거나 시각을 못 읽으면 None (날짜를 그대로 보인다)
fn ago_parts(iso: &str, now_ms: i64) -> Option<(&'static str, Option<i64>)> {
    let m = ((now_ms - parse_iso_ms(iso)?) as f64 / 60000.0).round() as i64;
    if m < 1 {
        Some(("방금", None))
    } else if m < 60 {
        Some(("{1}분 전", Some(m)))
    } else if m < 1440 {
        Some(("{1}시간 전", Some((m as f64 / 60.0).round() as i64)))
    } else if m < 43200 {
        Some(("{1}일 전", Some((m as f64 / 1440.0).round() as i64)))
    } else {
        None
    }
}

/// "2026-11-15T03:04:05.678Z" → UTC ms
fn parse_iso_ms(iso: &str) -> Option<i64> {
    let days = parse_date(iso.get(0..10)?)?;
    let n = |r: std::ops::Range<usize>| iso.get(r).and_then(|x| x.parse::<i64>().ok()).unwrap_or(0);
    Some(days * 86_400_000 + n(11..13) * 3_600_000 + n(14..16) * 60_000 + n(17..19) * 1000)
}

/// 큐시트 목록 (trash 면 휴지통만). 고정한 것이 먼저, 그다음 예배 날짜 최신순 — Electron db.js listCues 와 같은 모양
pub fn list(store: &BibleStore, trash: bool) -> Vec<Value> {
    let sql = "SELECT id, name, COALESCE(service_date,''), COALESCE(start_time,''), items, COALESCE(updated_at,''), COALESCE(created_at,''), COALESCE(memo,''), pinned, template, COALESCE(deleted_at,'') \
               FROM Cuesheets WHERE (COALESCE(deleted_at,'') = '') = ? ORDER BY pinned DESC, COALESCE(service_date, '') DESC, updated_at DESC";
    let Ok(mut st) = store.db.prepare(sql) else { return vec![] };
    st.query_map([if trash { 0 } else { 1 }], |r| {
        let items: Vec<Value> = serde_json::from_str(&r.get::<_, String>(4)?).unwrap_or_default();
        let preview: Vec<Value> = items
            .iter()
            .take(PREVIEW_MAX)
            .map(|x| {
                let s = |k: &str| x[k].as_str().filter(|v| !v.is_empty()).map(String::from);
                let sub = s("ref").or_else(|| s("subtitle")).or_else(|| x["number"].as_i64().map(|n| format!("{n}장"))).unwrap_or_default();
                json!({ "type": x["type"], "title": x["title"].as_str().unwrap_or(""), "sub": sub })
            })
            .collect();
        Ok(json!({
            "id": r.get::<_, String>(0)?, "name": r.get::<_, String>(1)?, "serviceDate": r.get::<_, String>(2)?, "startTime": r.get::<_, String>(3)?,
            "updatedAt": r.get::<_, String>(5)?, "createdAt": r.get::<_, String>(6)?, "count": items.len(), "memo": r.get::<_, String>(7)?,
            "pinned": r.get::<_, i64>(8)? != 0, "template": r.get::<_, i64>(9)? != 0, "deletedAt": r.get::<_, String>(10)?, "preview": preview,
        }))
    })
    .map(|it| it.filter_map(|x| x.ok()).collect())
    .unwrap_or_default()
}

/// 찾기·보기·정렬 (cuemanager.js rows). view: all·pinned·template·trash · sort: date·updated·name
pub fn rows(all: &[Value], trash: &[Value], view: &str, sort: &str, q: &str) -> Vec<Value> {
    let mut l: Vec<Value> = if view == "trash" { trash.to_vec() } else { all.to_vec() };
    if view == "pinned" {
        l.retain(|c| c["pinned"] == true);
    }
    if view == "template" {
        l.retain(|c| c["template"] == true);
    }
    let squash = |s: &str| s.split_whitespace().collect::<String>().to_lowercase();
    let q = squash(q);
    if !q.is_empty() {
        l.retain(|c| {
            let mut hay = format!("{}{}{}", c["name"].as_str().unwrap_or(""), c["serviceDate"].as_str().unwrap_or(""), c["memo"].as_str().unwrap_or(""));
            for p in c["preview"].as_array().into_iter().flatten() {
                hay.push_str(p["title"].as_str().unwrap_or(""));
                hay.push_str(p["sub"].as_str().unwrap_or(""));
            }
            squash(&hay).contains(&q)
        });
    }
    let s = |c: &Value, k: &str| c[k].as_str().unwrap_or("").to_string();
    l.sort_by(|x, y| {
        let pin = if view != "trash" { (y["pinned"] == true).cmp(&(x["pinned"] == true)) } else { std::cmp::Ordering::Equal };
        pin.then_with(|| match sort {
            "name" => s(x, "name").cmp(&s(y, "name")),
            "updated" => s(y, "updatedAt").cmp(&s(x, "updatedAt")),
            _ => s(y, "serviceDate").cmp(&s(x, "serviceDate")).then_with(|| s(y, "updatedAt").cmp(&s(x, "updatedAt"))),
        })
    });
    l
}

/// 새 큐시트 기본 순서 (Electron cues.js sampleItems: 구세군 성결예배 예시)
pub fn sample_items(en: bool) -> Vec<Value> {
    let list: [(&str, &str, &str, &str); 8] = if en {
        [("title", "Call to Worship", "subtitle", ""), ("song", "Praise", "lyrics", ""), ("title", "Prayer", "subtitle", ""), ("bible", "Scripture Reading", "ref", "Ps 23"), ("title", "Sermon", "subtitle", ""), ("text", "Announcements", "body", ""), ("title", "Benediction", "subtitle", ""), ("blank", "Blank", "mode", "background")]
    } else {
        [("title", "예배로 부름", "subtitle", ""), ("song", "찬양", "lyrics", ""), ("title", "대표기도", "subtitle", ""), ("bible", "성경 봉독", "ref", "시 23"), ("title", "설교", "subtitle", ""), ("text", "광고", "body", ""), ("title", "축도", "subtitle", ""), ("blank", "빈 화면", "mode", "background")]
    };
    list.iter()
        .map(|(ty, title, k, v)| {
            let mut it = crate::core::new_item(ty, title);
            it[*k] = json!(v);
            it
        })
        .collect()
}

fn fresh(items: &[Value]) -> Vec<Value> {
    items
        .iter()
        .map(|it| {
            let mut x = it.clone();
            x["id"] = json!(short_id("i"));
            x
        })
        .collect()
}

impl Core {
    pub fn cue_list(&self, trash: bool) -> Vec<Value> {
        list(&self.store, trash)
    }

    fn cues_changed(&mut self) {
        self.cues = self.store.list_cues();
        self.emit(Ev::Ui);
    }

    /// 새 큐시트를 만들어 연다 (Electron newCue: 이름 '오늘 예배', 기본 순서 — empty 면 빈 큐시트) → id
    pub fn new_cue_default(&mut self, empty: bool) -> Result<String> {
        let en = self.settings.get()["uiLang"] == "en";
        let today = today();
        let cue = Cue {
            id: short_id("c"),
            name: format!("{today} {}", if en { "Service" } else { "예배" }),
            service_date: today,
            start_time: String::new(),
            items: if empty { vec![] } else { sample_items(en) },
            memo: String::new(),
            pinned: false,
            template: false,
        };
        self.store.save_cue(&cue)?;
        self.cues = self.store.list_cues();
        self.open_cue(&cue.id);
        Ok(cue.id)
    }

    /// 복제 (Electron duplicateCue). next_week 면 예배 날짜를 7일 뒤로 (이름 속 날짜도). 메모는 옮기지 않는다. 열지는 않는다 → 새 큐시트
    pub fn duplicate_cue_as(&mut self, id: &str, name: Option<&str>, service_date: Option<&str>, next_week: bool) -> Result<Cue> {
        let Some(src) = self.store.get_cue(id) else { bail!("큐시트를 찾을 수 없습니다.") };
        let mut date = service_date.map(String::from).unwrap_or_else(|| src.service_date.clone());
        let mut name = name.unwrap_or("").to_string();
        if next_week {
            date = add_days(&src.service_date, 7);
            if name.is_empty() {
                name = if !src.service_date.is_empty() && src.name.contains(&src.service_date) { src.name.replacen(&src.service_date, &date, 1) } else { format!("{} ({date})", src.name) };
            }
        }
        if name.is_empty() {
            name = format!("{} {}", src.name, crate::i18n::tr("복사본"));
        }
        let n = Cue { id: short_id("c"), name, service_date: date, start_time: src.start_time.clone(), items: fresh(&src.items), memo: String::new(), pinned: false, template: false };
        self.store.save_cue(&n)?;
        self.cues_changed();
        Ok(n)
    }

    /// 서식에서 새 큐시트 (cuemanager.js fromTpl): '오늘 + 서식 이름(앞 날짜는 뺌)', 예배 날짜 오늘 → 만들어 연다
    pub fn new_cue_from_template(&mut self, tpl_id: &str) -> Result<String> {
        let today = today();
        let base = self.store.get_cue(tpl_id).map(|t| t.name).unwrap_or_default();
        // 이름 앞의 'YYYY-MM-DD ' 는 뗀다
        let rest = match base.get(0..10).filter(|d| parse_date(d).is_some()) {
            Some(_) => base[10..].trim_start().to_string(),
            None => base,
        };
        let name = format!("{today} {}", if rest.is_empty() { crate::i18n::tr("예배") } else { rest });
        let c = self.duplicate_cue_as(tpl_id, Some(&name), Some(&today), false)?;
        self.open_cue(&c.id);
        Ok(c.id)
    }

    /// 이름·날짜·메모·고정·서식 (Electron updateCueMeta 의 검증 그대로). 열린 큐시트면 다시 읽는다
    pub fn update_cue_meta_of(&mut self, id: &str, m: &Value) -> Result<()> {
        if self.store.get_cue(id).is_none() {
            bail!("큐시트를 찾을 수 없습니다.");
        }
        let mut out = json!({});
        if let Some(n) = m["name"].as_str().map(str::trim).filter(|n| !n.is_empty()) {
            out["name"] = json!(n.chars().take(200).collect::<String>());
        }
        if let Some(d) = m["serviceDate"].as_str().filter(|d| d.is_empty() || parse_date(d).is_some()) {
            out["serviceDate"] = json!(d);
        }
        if let Some(t) = m["memo"].as_str() {
            out["memo"] = json!(t.chars().take(1000).collect::<String>());
        }
        for k in ["pinned", "template"] {
            if m[k].is_boolean() {
                out[k] = m[k].clone();
            }
        }
        cues::update_meta(&self.store, id, &out)?;
        if self.cue.as_ref().is_some_and(|c| c.id == id) {
            // 순서는 메모리의 것을 지킨다 (아직 저장 안 된 편집이 있을 수 있다)
            if let (Some(cur), Some(saved)) = (self.cue.as_mut(), self.store.get_cue(id)) {
                cur.name = saved.name;
                cur.service_date = saved.service_date;
                cur.memo = saved.memo;
                cur.pinned = saved.pinned;
                cur.template = saved.template;
            }
        }
        self.cues_changed();
        Ok(())
    }

    /// 휴지통으로 (여러 개, 고정도 푼다). 열려 있던 큐시트를 지웠으면 남은 것 중 첫 큐시트를 연다 (cuemanager.js afterRemove)
    pub fn trash_cues(&mut self, ids: &[String]) -> Result<usize> {
        let at = iso_now();
        for id in ids {
            cues::update_meta(&self.store, id, &json!({ "deletedAt": at, "pinned": false }))?;
        }
        self.cues = self.store.list_cues();
        if self.cue.as_ref().is_some_and(|c| ids.contains(&c.id)) {
            match self.cues.first().map(|c| c.id.clone()) {
                Some(next) => self.open_cue(&next),
                None => {
                    self.cue = None;
                    self.settings.update(|d| d["activeCueId"] = json!(""));
                }
            }
        }
        self.emit(Ev::Ui);
        Ok(ids.len())
    }

    pub fn restore_cues(&mut self, ids: &[String]) -> Result<usize> {
        for id in ids {
            cues::restore(&self.store, id)?;
        }
        self.cues_changed();
        Ok(ids.len())
    }

    /// 휴지통에서 아주 지우기 (휴지통에 있는 것만)
    pub fn purge_cue_ids(&mut self, ids: &[String]) -> Result<usize> {
        let mut n = 0;
        for id in ids {
            n += cues::purge(&self.store, Some(id))?;
        }
        self.cues_changed();
        Ok(n)
    }

    /// 휴지통에 30일 넘게 있던 큐시트 비우기 (Electron db.js purgeTrash, 켤 때 한 번)
    pub fn purge_old_trash(&mut self, days: i64) -> usize {
        let before = fmt_iso(crate::util::now_ms() - days * 86_400_000);
        let n = self.store.db.execute("DELETE FROM Cuesheets WHERE COALESCE(deleted_at,'') != '' AND deleted_at < ?", [&before]).unwrap_or(0);
        if n > 0 {
            log::info!("휴지통: {days}일 지난 큐시트 {n}개를 비웠습니다");
            self.cues = self.store.list_cues();
        }
        n
    }

    /// 여러 큐시트를 파일 하나로 (Electron exportCues 와 같은 모양: { worshipcue: 2, exportedAt, cues: [{ name, serviceDate, startTime, memo, items }] })
    pub fn export_cues_to(&self, ids: &[String], path: &std::path::Path) -> Result<usize> {
        let cues: Vec<Value> = ids
            .iter()
            .filter_map(|i| self.store.get_cue(i))
            .map(|c| json!({ "name": c.name, "serviceDate": c.service_date, "startTime": c.start_time, "memo": c.memo, "items": c.items }))
            .collect();
        if cues.is_empty() {
            bail!("내보낼 큐시트가 없습니다.");
        }
        std::fs::write(path, serde_json::to_string_pretty(&json!({ "worshipcue": 2, "exportedAt": iso_now(), "cues": cues }))?)?;
        Ok(cues.len())
    }

    /// 파일들에서 가져오기 → (가져온 수, 첫 큐시트 id, 못 읽은 파일 글)
    pub fn import_cue_files(&mut self, paths: &[std::path::PathBuf]) -> (usize, String, Vec<String>) {
        let (n, first, errs, _) = self.import_cue_files_ex(paths);
        (n, first, errs)
    }

    /// 워십프리젠터 큐시트 파일(.json)과 찬양 콘티 스튜디오 콘티(.worship)를 함께 받는다
    /// → (가져온 수, 첫 큐시트 id, 못 읽은 파일 글, 가져왔지만 확인할 것)
    pub fn import_cue_files_ex(&mut self, paths: &[std::path::PathBuf]) -> (usize, String, Vec<String>, Vec<String>) {
        let before: std::collections::HashSet<String> = self.store.list_cues().into_iter().map(|c| c.id).collect();
        let (mut n, mut errs, mut warns) = (0, vec![], vec![]);
        for p in paths {
            let name = p.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
            let stem = p.file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
            let r = std::fs::read_to_string(p).map_err(anyhow::Error::from).and_then(|t| Ok(serde_json::from_str::<Value>(t.trim_start_matches('\u{feff}'))?)).and_then(|v| {
                if crate::contistudio::is_conti(&v) {
                    let (k, w) = self.import_conti(&v, &stem)?;
                    warns.extend(w.into_iter().map(|x| format!("{name}: {x}")));
                    Ok(k)
                } else {
                    cues::import(&self.store, &v)
                }
            });
            match r {
                Ok(k) => n += k,
                Err(e) => errs.push(format!("{name}: {e}")),
            }
        }
        self.cues_changed();
        let first = self.cues.iter().find(|c| !before.contains(&c.id)).map(|c| c.id.clone()).unwrap_or_default();
        (n, first, errs, warns)
    }

    /// 찬양 콘티 스튜디오 콘티 한 개 → 새 큐시트 (가사가 없는 곡은 찬양 라이브러리에서 찾는다)
    fn import_conti(&mut self, v: &Value, fallback: &str) -> Result<(usize, Vec<String>)> {
        let db = &self.store.db;
        let find = |t: &str| crate::songs::find_by_title(db, t);
        let (cue, warnings) = crate::contistudio::to_cue(v, fallback, &find).map_err(anyhow::Error::msg)?;
        let s = |k: &str| cue[k].as_str().unwrap_or("").to_string();
        let c = Cue { id: short_id("c"), name: s("name"), service_date: s("serviceDate"), start_time: s("startTime"), items: cue["items"].as_array().cloned().unwrap_or_default(), memo: s("memo"), pinned: false, template: false };
        self.store.save_cue(&c)?;
        Ok((1, warnings))
    }

    /// 순서지 (글) — 단톡방·문서에 붙이기 (Electron cueText)
    pub fn cue_text_of(&self, id: &str) -> Option<String> {
        let c = if self.cue.as_ref().is_some_and(|c| c.id == id) { self.cue.clone() } else { self.store.get_cue(id) }?;
        Some(crate::parity_a::cue_text_with(&c.name, &c.service_date, &c.start_time, &c.items, &c.memo, &|it| self.song_meta_of(it)))
    }
}

/// UTC ms → ISO 8601 (iso_now 와 같은 모양)
fn fmt_iso(ms: i64) -> String {
    let secs = ms.div_euclid(1000);
    let (days, rem) = (secs.div_euclid(86400), secs.rem_euclid(86400));
    format!("{}T{:02}:{:02}:{:02}.{:03}Z", fmt_date(days), rem / 3600, (rem % 3600) / 60, rem % 60, ms.rem_euclid(1000))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn core() -> (Core, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("wp-cuemgr-{}-{}", std::process::id(), short_id("t")));
        std::fs::create_dir_all(&dir).unwrap();
        drop(BibleStore::open(&dir.join("bible.db")).unwrap());
        (Core::open(dir.clone(), dir.clone(), None).unwrap(), dir)
    }

    #[test]
    fn dates() {
        assert_eq!(day_of("2026-11-15"), "일");
        assert_eq!(day_of("2026-11-12"), "목");
        assert_eq!(day_of("날짜 아님"), "");
        assert_eq!(add_days("2026-11-29", 7), "2026-12-06");
        assert_eq!(add_days("2026-12-27", 7), "2027-01-03");
        assert_eq!(fmt_iso(parse_iso_ms("2026-11-15T03:04:05.000Z").unwrap()), "2026-11-15T03:04:05.000Z");
        let now = parse_iso_ms("2026-11-15T12:00:00.000Z").unwrap();
        assert_eq!(ago_parts("2026-11-15T11:59:50.000Z", now), Some(("방금", None)));
        assert_eq!(ago_parts("2026-11-15T11:30:00.000Z", now), Some(("{1}분 전", Some(30))));
        assert_eq!(ago_parts("2026-11-15T09:00:00.000Z", now), Some(("{1}시간 전", Some(3))));
        assert_eq!(ago_parts("2026-11-12T12:00:00.000Z", now), Some(("{1}일 전", Some(3))));
        assert_eq!(ago("2026-01-01T00:00:00.000Z", now), "2026-01-01", "30일이 넘으면 날짜");
    }

    /// 순서지 글: 찬양 순서에 라이브러리 곡 정보(Key · 박자 · BPM)가 붙는다 — 찬양팀 단톡방 콘티
    #[test]
    fn cue_text_has_song_meta_from_library() {
        let (mut c, dir) = core();
        crate::songs::ensure_schema(&c.store.db).ok();
        let s = crate::songs::save_song(&c.store.db, &json!({ "title": "주 은혜임을", "lyrics": "가사", "key": "G", "meter": "4/4", "bpm": "72" })).unwrap();
        c.new_cue("찬양 콘티").unwrap();
        let id = c.cue.as_ref().unwrap().id.clone();
        let mut it = crate::core::new_item("song", "주 은혜임을");
        it["songId"] = s["id"].clone();
        c.add_item(it, None);
        let mut own = crate::core::new_item("song", "직접 적은 곡");
        own["key"] = json!("A");
        c.add_item(own, None);
        c.add_item(crate::core::new_item("title", "설교"), None);
        let t = c.cue_text_of(&id).unwrap();
        assert!(t.contains("1. 주 은혜임을 [Key G · 4/4 · BPM 72]"), "{t}");
        assert!(t.contains("2. 직접 적은 곡 [Key A]"), "순서에 적은 Key 가 먼저: {t}");
        assert!(t.contains("3. 설교") && !t.contains("설교 ["), "찬양이 아니면 붙이지 않음: {t}");
        assert_eq!(c.active_cue_text().unwrap(), t);
        drop(c);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn new_default_has_sample_order_and_empty_is_empty() {
        let (mut c, dir) = core();
        let id = c.new_cue_default(false).unwrap();
        let cue = c.cue.clone().unwrap();
        assert_eq!(cue.id, id);
        assert_eq!(cue.items.len(), 8, "Electron sampleItems 와 같은 기본 순서");
        assert_eq!(cue.items[3]["ref"], "시 23");
        assert!(cue.name.ends_with(" 예배") && cue.name.starts_with(&today()));
        c.new_cue_default(true).unwrap();
        assert!(c.cue.as_ref().unwrap().items.is_empty());
        drop(c);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn duplicate_next_week_moves_date_in_name_and_drops_memo() {
        let (mut c, dir) = core();
        c.new_cue("2026-11-15 주일 예배").unwrap();
        let id = c.cue.as_ref().unwrap().id.clone();
        c.add_item(crate::core::new_item("title", "설교"), None);
        c.update_cue_meta_of(&id, &json!({ "serviceDate": "2026-11-15", "memo": "성찬 주일", "pinned": true })).unwrap();
        let n = c.duplicate_cue_as(&id, None, None, true).unwrap();
        assert_eq!(n.name, "2026-11-22 주일 예배");
        assert_eq!(n.service_date, "2026-11-22");
        assert_eq!(n.memo, "", "복제는 메모를 옮기지 않는다");
        assert!(!n.pinned);
        assert_eq!(n.items.len(), 1);
        assert_ne!(n.items[0]["id"], c.cue.as_ref().unwrap().items[0]["id"], "순서 id 는 새로");
        assert_eq!(c.cue.as_ref().unwrap().id, id, "복제해도 열린 큐시트는 그대로");
        let plain = c.duplicate_cue_as(&id, None, None, false).unwrap();
        assert!(plain.name.starts_with("2026-11-15 주일 예배 ") && plain.name.len() > "2026-11-15 주일 예배 ".len(), "{}", plain.name);
        assert_eq!(plain.service_date, "2026-11-15");
        // 이름에 날짜가 없으면 뒤에 붙인다
        c.update_cue_meta_of(&id, &json!({ "name": "주일 낮 예배" })).unwrap();
        assert_eq!(c.duplicate_cue_as(&id, None, None, true).unwrap().name, "주일 낮 예배 (2026-11-22)");
        drop(c);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn template_makes_today_cue_and_opens_it() {
        let (mut c, dir) = core();
        c.new_cue("2026-01-04 주일 낮 예배").unwrap();
        let tpl = c.cue.as_ref().unwrap().id.clone();
        c.update_cue_meta_of(&tpl, &json!({ "template": true })).unwrap();
        let id = c.new_cue_from_template(&tpl).unwrap();
        let cue = c.cue.clone().unwrap();
        assert_eq!(cue.id, id);
        assert_eq!(cue.name, format!("{} 주일 낮 예배", today()));
        assert_eq!(cue.service_date, today());
        assert!(!cue.template, "서식에서 만든 것은 서식이 아니다");
        drop(c);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn list_rows_filter_sort_and_trash_flow() {
        let (mut c, dir) = core();
        for (n, d) in [("가 예배", "2026-11-01"), ("나 수요 예배", "2026-11-04"), ("다 새벽", "2026-11-08")] {
            c.new_cue(n).unwrap();
            let id = c.cue.as_ref().unwrap().id.clone();
            c.update_cue_meta_of(&id, &json!({ "serviceDate": d })).unwrap();
        }
        let b = c.cues.iter().find(|x| x.name == "나 수요 예배").unwrap().id.clone();
        c.open_cue(&b);
        c.add_item(json!({ "id": "x", "type": "bible", "title": "성경 봉독", "ref": "요 3:16" }), None);
        c.update_cue_meta_of(&b, &json!({ "pinned": true, "memo": "성찬" })).unwrap();
        let all = c.cue_list(false);
        assert_eq!(all.len(), 3);
        assert_eq!(all[0]["name"], "나 수요 예배", "고정이 먼저");
        assert_eq!(all[0]["preview"][0]["sub"], "요 3:16");
        let names = |v: Vec<Value>| v.iter().map(|x| x["name"].as_str().unwrap().to_string()).collect::<Vec<_>>();
        assert_eq!(names(rows(&all, &[], "all", "date", "")), ["나 수요 예배", "다 새벽", "가 예배"]);
        assert_eq!(names(rows(&all, &[], "all", "name", "")), ["나 수요 예배", "가 예배", "다 새벽"]);
        assert_eq!(names(rows(&all, &[], "pinned", "date", "")), ["나 수요 예배"]);
        assert_eq!(names(rows(&all, &[], "all", "date", "요3:16")), ["나 수요 예배"], "순서 제목·장절로도 찾는다 (빈칸 무시)");
        assert_eq!(names(rows(&all, &[], "all", "date", "성 찬")), ["나 수요 예배"], "메모로도");
        // 열린 큐시트를 휴지통으로 → 남은 첫 큐시트가 열린다
        assert_eq!(c.trash_cues(&[b.clone()]).unwrap(), 1);
        assert_ne!(c.cue.as_ref().map(|x| x.id.clone()), Some(b.clone()));
        assert!(c.cue.is_some());
        let trash = c.cue_list(true);
        assert_eq!(names(trash.clone()), ["나 수요 예배"]);
        assert_eq!(trash[0]["pinned"], false, "휴지통으로 가면 고정이 풀린다");
        assert_eq!(c.cue_list(false).len(), 2);
        // 휴지통에 없는 것은 아주 지우지 못한다
        let live_id = c.cue.as_ref().unwrap().id.clone();
        assert_eq!(c.purge_cue_ids(&[live_id]).unwrap(), 0);
        assert_eq!(c.restore_cues(&[b.clone()]).unwrap(), 1);
        assert_eq!(c.cue_list(false).len(), 3);
        c.trash_cues(&[b.clone()]).unwrap();
        assert_eq!(c.purge_old_trash(30), 0, "방금 지운 것은 남는다");
        c.store.db.execute("UPDATE Cuesheets SET deleted_at = '2020-01-01T00:00:00.000Z' WHERE id = ?", [&b]).unwrap();
        assert_eq!(c.purge_old_trash(30), 1);
        assert!(c.cue_list(true).is_empty());
        drop(c);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn export_import_round_trip_in_electron_shape() {
        let (mut c, dir) = core();
        c.new_cue("내보낼 예배").unwrap();
        let id = c.cue.as_ref().unwrap().id.clone();
        c.add_item(crate::core::new_item("title", "설교"), None);
        let f = dir.join("out.json");
        assert_eq!(c.export_cues_to(&[id.clone()], &f).unwrap(), 1);
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
        assert_eq!(v["worshipcue"], 2);
        assert_eq!(v["cues"][0]["name"], "내보낼 예배");
        assert!(v["cues"][0].get("id").is_none(), "Electron 과 같은 모양 (id 없음)");
        let (n, first, errs) = c.import_cue_files(&[f, dir.join("없는파일.json")]);
        assert_eq!(n, 1);
        assert_eq!(errs.len(), 1);
        assert!(!first.is_empty() && first != id);
        assert_eq!(c.cue_text_of(&first).unwrap().lines().next().unwrap().starts_with("■ 내보낼 예배"), true);
        drop(c);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn import_conti_studio_worship_file() {
        let (mut c, dir) = core();
        // 라이브러리에 있는 곡은 가사를 끌어온다
        c.save_song(&json!({ "title": "꽃들도", "lyrics": "[1절]\n꽃들도\n\n[후렴]\n주를" })).unwrap();
        let f = dir.join("주일 콘티.worship");
        std::fs::write(&f, "\u{feff}".to_string() + &json!({
            "version": "1.0",
            "service_info": { "date": "2026-11-29", "service_name": "주일 찬양예배", "worship_leader": "김택근" },
            "items": [
                { "order": 1, "item_type": "SONG", "title": "꽃 들도", "key": "A", "song_form": "V1 - C - V1 - C x2", "lyrics": "" },
                { "order": 2, "item_type": "SONG", "title": "없는 곡", "key": "G" }
            ]
        }).to_string()).unwrap();
        let (n, first, errs, warns) = c.import_cue_files_ex(&[f]);
        assert_eq!((n, errs.len()), (1, 0), "{errs:?}");
        assert_eq!(warns.len(), 1, "가사 없는 곡 하나: {warns:?}");
        let cue = c.store.get_cue(&first).unwrap();
        assert_eq!(cue.name, "주일 찬양예배");
        assert_eq!(cue.service_date, "2026-11-29");
        assert!(cue.memo.contains("인도: 김택근"));
        assert_eq!(cue.items[0]["type"], "song");
        assert_eq!(cue.items[0]["order"], "1절 후렴 1절 후렴 후렴");
        assert!(cue.items[0]["lyrics"].as_str().unwrap().contains("꽃들도"));
        assert!(!cue.items[0]["songId"].as_str().unwrap().is_empty());
        assert_eq!(cue.items[1]["type"], "title");
        // 슬라이드까지: 부를 순서대로 5장
        let slides = crate::slides::arrange(crate::slides::lyrics_chunks(cue.items[0]["lyrics"].as_str().unwrap()), cue.items[0]["order"].as_str().unwrap());
        assert_eq!(slides.len(), 5);
        drop(c);
        let _ = std::fs::remove_dir_all(dir);
    }
}
