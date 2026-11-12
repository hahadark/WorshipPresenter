//! 여러 PC 사이 동기화 · 자동 백업 (Electron sync.js 를 옮김, 파일 형식 같음) [트랙 B]
//!
//! 동기화: 설정의 syncDir(원드라이브 같은 동기화 폴더)에 WorshipCue-sync.json 을 쓰고 읽는다.
//!   - 찬양 라이브러리 · 큐시트 · 오버레이. 같은 id 는 updated_at 이 최신인 쪽이 이긴다 (지우기는 휴지통 표시만 전달).
//!   - 시작할 때 읽고, 바뀐 것이 있으면 1분 안에 쓰고, 끝날 때 쓴다.
//! 백업: 하루 한 번 bible.db 와 settings.json 을 data/backups 에 복사. 10일치 보관.
//!   되돌리기는 bible.restore.db 로 적어 두고 다음 시작 때 바꿔 끼운다 (열린 DB 는 바꿀 수 없으므로).

use crate::core::{Core, Ev, Shared};
use crate::util::iso_now;
use anyhow::{bail, Context, Result};
use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub const SYNC_FILE: &str = "WorshipCue-sync.json";
const KEEP_BACKUPS: usize = 10;

#[derive(Default, Clone)]
pub struct SyncStatus {
    pub last_import: String,
    pub last_export: String,
    pub remote_at: String,
    pub remote_host: String,
    pub error: String,
    pub last_sig: String,
}

pub fn hostname() -> String {
    if let Some(h) = std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).ok().filter(|x| !x.is_empty()) {
        return h;
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::SystemInformation::{ComputerNameDnsHostname, GetComputerNameExW};
        let mut buf = [0u16; 256];
        let mut n = buf.len() as u32;
        // SAFETY: 버퍼 길이를 알려 주고, 돌려받은 길이만큼만 읽는다
        if unsafe { GetComputerNameExW(ComputerNameDnsHostname, buf.as_mut_ptr(), &mut n) } != 0 {
            return String::from_utf16_lossy(&buf[..n as usize]);
        }
    }
    String::new()
}

fn hash(s: &str) -> i32 {
    // JS: h = (h * 31 + charCode) | 0  (UTF-16 단위)
    let mut h: i32 = 0;
    for c in s.encode_utf16() {
        h = h.wrapping_mul(31).wrapping_add(c as i32);
    }
    h
}

/// '최신' 비교: 원격 값이 있고 (로컬이 없거나) 글자 순서로 더 크면
fn newer(remote: &str, local: Option<&str>) -> bool {
    match local {
        None => true,
        Some(l) => remote > l,
    }
}

fn s(v: &Value, k: &str) -> String {
    match &v[k] {
        Value::String(x) => x.clone(),
        Value::Null => String::new(),
        x => x.to_string(),
    }
}

fn cut(x: &str, n: usize) -> String {
    x.chars().take(n).collect()
}

/// 시작 전에(DB 를 열기 전) 예약된 되돌리기를 적용
pub fn apply_pending_restore(data_dir: &Path) -> bool {
    let r = data_dir.join("bible.restore.db");
    if !r.exists() {
        return false;
    }
    let cur = data_dir.join("bible.db");
    for ext in ["", "-wal", "-shm"] {
        let p = PathBuf::from(format!("{}{}", cur.display(), ext));
        if p.exists() {
            let _ = std::fs::rename(&p, data_dir.join(format!("bible.pre-restore.db{ext}")));
        }
    }
    if std::fs::rename(&r, &cur).is_err() {
        return false;
    }
    let sr = data_dir.join("settings.restore.json");
    if sr.exists() {
        let st = data_dir.join("settings.json");
        if st.exists() {
            let _ = std::fs::copy(&st, data_dir.join("settings.pre-restore.json"));
        }
        let _ = std::fs::rename(&sr, &st);
    }
    log::info!("백업에서 되돌림: {}", cur.display());
    true
}

impl Core {
    fn sync_dir(&self) -> String {
        self.settings.str(&["syncDir"]).trim().to_string()
    }
    fn sync_file(&self) -> Option<PathBuf> {
        let d = self.sync_dir();
        (!d.is_empty()).then(|| PathBuf::from(d).join(SYNC_FILE))
    }

    /// DB 가 바뀌었는지 싸게 보는 서명
    pub fn sync_signature(&self) -> String {
        let db = &self.store.db;
        let a: (i64, Option<String>) = db.query_row("SELECT COUNT(*), MAX(updated_at) FROM Songs", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap_or((0, None));
        let b: (i64, Option<String>, Option<String>) = db.query_row("SELECT COUNT(*), MAX(updated_at), MAX(deleted_at) FROM Cuesheets", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap_or((0, None, None));
        let ov = self.settings.get()["overlays"].to_string();
        format!("{}|{}|{}|{}|{}|{}|{}", a.0, a.1.unwrap_or_default(), b.0, b.1.unwrap_or_default(), b.2.unwrap_or_default(), ov.len(), hash(&ov))
    }

    pub fn sync_export_if_changed(&mut self) -> Result<bool> {
        if self.sync_dir().is_empty() {
            return Ok(false);
        }
        let sig = self.sync_signature();
        if sig == self.b.sync.last_sig {
            return Ok(false);
        }
        self.sync_export()?;
        Ok(true)
    }

    /// 지금 상태를 동기화 파일로
    pub fn sync_export(&mut self) -> Result<Value> {
        let f = self.sync_file().context("동기화 폴더를 설정하세요.")?;
        if let Some(d) = f.parent() {
            std::fs::create_dir_all(d)?;
        }
        let db = &self.store.db;
        let songs: Vec<Value> = {
            let mut st = db.prepare("SELECT id, title, lyrics, credit, tags, arrangement, arrangements, updated_at FROM Songs")?;
            let rows = st.query_map([], |r| {
                Ok(json!({ "id": r.get::<_, String>(0)?, "title": r.get::<_, String>(1)?, "lyrics": r.get::<_, Option<String>>(2)?, "credit": r.get::<_, Option<String>>(3)?, "tags": r.get::<_, Option<String>>(4)?, "arrangement": r.get::<_, Option<String>>(5)?, "arrangements": r.get::<_, Option<String>>(6)?, "updated_at": r.get::<_, Option<String>>(7)? }))
            })?;
            rows.filter_map(|x| x.ok()).collect()
        };
        let cues: Vec<Value> = {
            let mut st = db.prepare("SELECT id, name, service_date, start_time, items, updated_at, memo, pinned, template, deleted_at, created_at FROM Cuesheets")?;
            let rows = st.query_map([], |r| {
                Ok(json!({ "id": r.get::<_, String>(0)?, "name": r.get::<_, String>(1)?, "service_date": r.get::<_, Option<String>>(2)?, "start_time": r.get::<_, Option<String>>(3)?, "items": r.get::<_, String>(4)?, "updated_at": r.get::<_, Option<String>>(5)?, "memo": r.get::<_, Option<String>>(6)?, "pinned": r.get::<_, i64>(7)?, "template": r.get::<_, i64>(8)?, "deleted_at": r.get::<_, Option<String>>(9)?, "created_at": r.get::<_, Option<String>>(10)? }))
            })?;
            rows.filter_map(|x| x.ok()).collect()
        };
        let at = iso_now();
        let data = json!({ "v": 1, "app": "WorshipCue", "at": at, "host": hostname(), "songs": songs, "cues": cues, "overlays": self.settings.get()["overlays"].as_array().cloned().unwrap_or_default() });
        let tmp = f.with_extension("json.tmp");
        std::fs::write(&tmp, data.to_string())?;
        std::fs::rename(&tmp, &f)?;
        self.b.sync.last_export = at.clone();
        self.b.sync.error.clear();
        self.b.sync.last_sig = self.sync_signature();
        Ok(json!({ "at": at, "songs": songs.len(), "cues": cues.len() }))
    }

    /// 동기화 파일을 읽어 최신 것을 받아들인다 → { songs, cues, overlays } 받은 개수
    pub fn sync_import(&mut self) -> Result<Value> {
        let Some(f) = self.sync_file().filter(|f| f.exists()) else { return Ok(json!({ "songs": 0, "cues": 0, "overlays": 0, "none": true })) };
        let txt = std::fs::read_to_string(&f)?;
        let data: Value = serde_json::from_str(txt.trim_start_matches('\u{feff}')).context("동기화 파일을 읽지 못했습니다.")?;
        if data["app"] != "WorshipCue" || !data["songs"].is_array() {
            bail!("동기화 파일 형식이 다릅니다.");
        }
        self.b.sync.remote_at = s(&data, "at");
        self.b.sync.remote_host = s(&data, "host");
        let (mut gs, mut gc, mut go) = (0, 0, 0);
        {
            let db = &self.store.db;
            let tx = db.unchecked_transaction()?;
            for x in data["songs"].as_array().into_iter().flatten() {
                let id = s(x, "id");
                if !(4..=40).contains(&id.len()) || !id.chars().all(|c| c.is_ascii_alphanumeric()) || s(x, "title").trim().is_empty() {
                    continue;
                }
                let local: Option<Option<String>> = tx.query_row("SELECT updated_at FROM Songs WHERE id = ?", [&id], |r| r.get(0)).optional()?;
                if local.is_some() && !newer(&s(x, "updated_at"), local.as_ref().map(|l| l.as_deref().unwrap_or(""))) {
                    continue;
                }
                let arr = if x["arrangements"].is_string() { cut(&s(x, "arrangements"), 5000) } else { "[]".into() };
                let up = if s(x, "updated_at").is_empty() { iso_now() } else { s(x, "updated_at") };
                tx.execute(
                    "INSERT OR REPLACE INTO Songs (id, title, lyrics, credit, tags, arrangement, arrangements, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                    params![id, cut(&s(x, "title"), 200), cut(&s(x, "lyrics"), 50000), cut(&s(x, "credit"), 300), cut(&s(x, "tags"), 300), cut(&s(x, "arrangement"), 300), arr, up],
                )?;
                gs += 1;
            }
            for c in data["cues"].as_array().into_iter().flatten() {
                let id = s(c, "id");
                if id.is_empty() || s(c, "name").trim().is_empty() {
                    continue;
                }
                let local: Option<(Option<String>, Option<String>)> = tx.query_row("SELECT updated_at, deleted_at FROM Cuesheets WHERE id = ?", [&id], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
                let (lu, ld) = match &local {
                    Some((u, d)) => (Some(u.clone().unwrap_or_default()), Some(d.clone().unwrap_or_default())),
                    None => (None, None),
                };
                if !newer(&s(c, "updated_at"), lu.as_deref()) && !newer(&s(c, "deleted_at"), ld.as_deref()) {
                    continue;
                }
                let items = match serde_json::from_str::<Value>(if s(c, "items").is_empty() { "[]" } else { c["items"].as_str().unwrap_or("[]") }) {
                    Ok(v) => v.to_string(),
                    Err(_) => continue,
                };
                let flag = |k: &str| c[k].as_bool().map(|b| b as i64).or(c[k].as_i64()).unwrap_or(0).min(1);
                tx.execute(
                    "INSERT OR REPLACE INTO Cuesheets (id, name, service_date, start_time, items, updated_at, memo, pinned, template, deleted_at, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                    params![id, cut(&s(c, "name"), 200), s(c, "service_date"), s(c, "start_time"), items, s(c, "updated_at"), s(c, "memo"), flag("pinned"), flag("template"), s(c, "deleted_at"), s(c, "created_at")],
                )?;
                gc += 1;
            }
            tx.commit()?;
        }
        // 오버레이: id 별로 updatedAt 비교 (없으면 원격 것을 넣는다). 켜짐 여부는 이 PC 것
        if let Some(remote) = data["overlays"].as_array().filter(|a| !a.is_empty()).cloned() {
            self.settings.update(|d| {
                let mut mine: Vec<Value> = d["overlays"].as_array().cloned().unwrap_or_default();
                for o in &remote {
                    let Some(id) = o["id"].as_str() else { continue };
                    let pos = mine.iter().position(|x| x["id"] == id);
                    if let Some(i) = pos {
                        if !newer(&s(o, "updatedAt"), Some(&s(&mine[i], "updatedAt"))) {
                            continue;
                        }
                    }
                    let mut n = o.clone();
                    n["on"] = json!(pos.map(|i| mine[i]["on"].as_bool().unwrap_or(false)).unwrap_or(false));
                    match pos {
                        Some(i) => mine[i] = n,
                        None => mine.push(n),
                    }
                    go += 1;
                }
                d["overlays"] = json!(mine);
            });
        }
        self.b.sync.last_import = iso_now();
        self.b.sync.error.clear();
        if gs + gc > 0 {
            self.cues = self.store.list_cues();
            if let Some(id) = self.cue.as_ref().map(|c| c.id.clone()) {
                self.cue = self.store.get_cue(&id);
            }
        }
        if go > 0 {
            self.emit(Ev::Live);
        }
        self.emit(Ev::Ui);
        log::info!("동기화 읽음: 찬양 {gs} · 큐시트 {gc} · 오버레이 {go} ({} {})", self.b.sync.remote_host, self.b.sync.remote_at);
        Ok(json!({ "songs": gs, "cues": gc, "overlays": go }))
    }

    /// 지금 동기화: 읽고 → 쓰기
    pub fn sync_now(&mut self) -> Result<Value> {
        if self.sync_dir().is_empty() {
            bail!("동기화 폴더를 먼저 고르세요.");
        }
        let got = self.sync_import();
        if let Err(e) = &got {
            self.b.sync.error = e.to_string();
        }
        let put = self.sync_export()?;
        Ok(json!({ "imported": got.unwrap_or(Value::Null), "exported": put }))
    }

    pub fn set_sync_dir(&mut self, dir: &str) -> Result<Value> {
        let d = dir.trim().to_string();
        if !d.is_empty() && !Path::new(&d).is_dir() {
            bail!("폴더를 찾을 수 없습니다: {d}");
        }
        let d2 = d.clone();
        self.settings.update(|s| s["syncDir"] = json!(d2));
        self.b.sync.last_sig.clear();
        if d.is_empty() {
            return Ok(self.sync_info());
        }
        self.sync_now()?;
        Ok(self.sync_info())
    }

    pub fn sync_info(&self) -> Value {
        let f = self.sync_file();
        let file_at = f.as_ref().and_then(|f| std::fs::metadata(f).ok()).and_then(|m| m.modified().ok()).map(|t| {
            let ms = t.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
            ms.to_string()
        });
        let st = &self.b.sync;
        json!({
            "dir": self.sync_dir(), "file": f.map(|p| p.to_string_lossy().to_string()).unwrap_or_default(), "exists": file_at.is_some(), "fileAtMs": file_at.unwrap_or_default(),
            "host": hostname(), "lastImport": st.last_import, "lastExport": st.last_export, "remoteAt": st.remote_at, "remoteHost": st.remote_host, "error": st.error,
            "restorePending": self.restore_pending(),
        })
    }

    // ---------------- 백업 ----------------
    pub fn backup_dir(&self) -> PathBuf {
        self.data_dir.join("backups")
    }

    /// 최신순 [{ day, db, settings, size }]
    pub fn backups(&self) -> Vec<Value> {
        backups_in(&self.data_dir)
    }

    /// 지금 백업 (코어 잠금을 쥔 채로 부르면 큰 DB 에서 몇 초 멈춘다 → 화면은 backup_files 를 따로 스레드에서)
    pub fn backup_now(&mut self) -> Result<Value> {
        let _ = self.store.db.execute_batch("PRAGMA wal_checkpoint(PASSIVE)");
        backup_files(&self.data_dir)
    }

    /// 오늘 백업이 없으면 만든다
    pub fn backup_daily(&mut self) -> Result<Option<Value>> {
        if has_today(&self.data_dir) {
            return Ok(None);
        }
        self.backup_now().map(Some)
    }
    pub fn restore_backup(&mut self, day: &str) -> Result<Value> {
        restore_files(&self.data_dir, day)
    }

    pub fn cancel_restore(&mut self) {
        for f in ["bible.restore.db", "settings.restore.json"] {
            let _ = std::fs::remove_file(self.data_dir.join(f));
        }
    }

    pub fn restore_pending(&self) -> bool {
        self.data_dir.join("bible.restore.db").exists()
    }
}

/// 시작 뒤 백그라운드: 곧바로 읽기 · 20초 뒤 하루 1회 백업 · 1분마다 바뀌었으면 쓰기
pub fn install(core: &Shared) {
    let core = core.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(2));
        {
            let mut c = core.lock();
            if let Err(e) = c.sync_import() {
                c.b.sync.error = e.to_string();
                log::warn!("동기화 읽기 실패: {e}");
            }
            c.b.sync.last_sig = c.sync_signature();
        }
        let mut n = 0u64;
        loop {
            std::thread::sleep(std::time::Duration::from_secs(10));
            n += 1;
            if n == 2 {
                // 백업은 잠금 없이 (큰 DB 를 복사하는 동안 화면이 멈추지 않게)
                let dir = core.lock().data_dir.clone();
                if !has_today(&dir) {
                    if let Err(e) = backup_files(&dir) {
                        log::warn!("자동 백업 실패: {e}");
                    }
                }
            }
            let mut c = core.lock();
            if n % 6 == 0 {
                if let Err(e) = c.sync_export_if_changed() {
                    c.b.sync.error = e.to_string();
                }
            }
        }
    });
}

/// 끝날 때: 바뀐 것이 있으면 쓰기 (조용히)
pub fn on_exit(core: &mut Core) {
    let _ = core.sync_export_if_changed();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::new_item;

    fn core(dir: &Path) -> Core {
        std::fs::create_dir_all(dir).unwrap();
        if !dir.join("bible.db").exists() {
            std::fs::write(dir.join("bible.db"), b"").unwrap();
        }
        Core::open(dir.to_path_buf(), dir.to_path_buf(), None).unwrap()
    }

    #[test]
    fn hash_like_js() {
        // JS: let h=0; for (c of 'abc') h=(h*31+c.charCodeAt(0))|0 → 96354
        assert_eq!(hash("abc"), 96354);
        assert!(newer("2026-11-12", Some("2026-11-11")));
        assert!(!newer("", Some("")));
    }

    #[test]
    fn two_pcs_and_backup() {
        let root = std::env::temp_dir().join(format!("wp-sync-{}-{}", crate::util::now_ms(), rand::random::<u32>()));
        let share = root.join("onedrive");
        std::fs::create_dir_all(&share).unwrap();
        // PC 1: 큐시트 하나 → 동기화 폴더로
        let mut a = core(&root.join("pc1"));
        a.new_cue("주일 예배").unwrap();
        a.add_item(new_item("title", "예배로 부름"), None);
        a.set_sync_dir(&share.to_string_lossy()).unwrap();
        assert!(share.join(SYNC_FILE).exists());
        let raw: Value = serde_json::from_str(&std::fs::read_to_string(share.join(SYNC_FILE)).unwrap()).unwrap();
        assert_eq!(raw["app"], "WorshipCue");
        // PC 2: 같은 폴더를 고르면 받아 온다
        let mut b = core(&root.join("pc2"));
        let n0 = b.store.list_cues().len();
        b.set_sync_dir(&share.to_string_lossy()).unwrap();
        assert_eq!(b.store.list_cues().len(), n0 + 1);
        // 백업 · 되돌리기 예약 · 취소
        let bk = b.backup_now().unwrap();
        assert_eq!(b.backups().len(), 1);
        assert!(b.backup_daily().unwrap().is_none());
        b.restore_backup(bk["day"].as_str().unwrap()).unwrap();
        assert!(b.restore_pending());
        b.cancel_restore();
        assert!(!b.restore_pending());
        // 되돌리기 적용 (시작 전)
        b.restore_backup(bk["day"].as_str().unwrap()).unwrap();
        drop(b);
        assert!(apply_pending_restore(&root.join("pc2")));
        assert!(root.join("pc2").join("bible.pre-restore.db").exists());
    }
}

/// 최신순 [{ day, db, settings, size }]
pub fn backups_in(data_dir: &Path) -> Vec<Value> {
    let dir = data_dir.join("backups");
    let mut out = vec![];
    for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let Some(day) = name.strip_prefix("WorshipCue-").and_then(|x| x.strip_suffix(".db")) else { continue };
        if day.len() != 10 || day.as_bytes()[4] != b'-' {
            continue;
        }
        let st = dir.join(format!("settings-{day}.json"));
        out.push(json!({ "day": day, "db": e.path().to_string_lossy(), "settings": if st.exists() { st.to_string_lossy().to_string() } else { String::new() }, "size": e.metadata().map(|m| m.len()).unwrap_or(0) }));
    }
    out.sort_by(|a, b| b["day"].as_str().cmp(&a["day"].as_str()));
    out
}

pub fn has_today(data_dir: &Path) -> bool {
    let today = iso_now()[..10].to_string();
    backups_in(data_dir).iter().any(|b| b["day"] == today.as_str())
}

/// 백업 만들기: 따로 연 연결로 VACUUM INTO (코어 잠금 없이 — 프로그램이 멈추지 않게)
pub fn backup_files(data_dir: &Path) -> Result<Value> {
    let dir = data_dir.join("backups");
    std::fs::create_dir_all(&dir)?;
    let day = iso_now()[..10].to_string();
    let out = dir.join(format!("WorshipCue-{day}.db"));
    let tmp = dir.join(format!("WorshipCue-{day}.db.tmp"));
    let _ = std::fs::remove_file(&tmp);
    let src = data_dir.join("bible.db");
    let ok = rusqlite::Connection::open_with_flags(&src, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .and_then(|c| c.execute("VACUUM INTO ?", [tmp.to_string_lossy().to_string()]))
        .is_ok()
        && std::fs::rename(&tmp, &out).is_ok();
    if !ok {
        std::fs::copy(&src, &out)?;
    }
    let st = data_dir.join("settings.json");
    if st.exists() {
        std::fs::copy(&st, dir.join(format!("settings-{day}.json")))?;
    }
    for b in backups_in(data_dir).iter().skip(KEEP_BACKUPS) {
        for k in ["db", "settings"] {
            if let Some(p) = b[k].as_str().filter(|x| !x.is_empty()) {
                let _ = std::fs::remove_file(p);
            }
        }
    }
    log::info!("백업: {}", out.display());
    Ok(backups_in(data_dir).into_iter().next().unwrap_or(Value::Null))
}

/// 되돌리기 예약 (코어 잠금 없이 — 큰 파일 복사 동안 화면이 멈추지 않게). 임시 이름으로 복사한 뒤 바꿔 끼워, 도중에 끊겨도 반쪽 파일로 되돌리지 않는다
pub fn restore_files(data_dir: &Path, day: &str) -> Result<Value> {
    let b = backups_in(data_dir).into_iter().find(|x| x["day"] == day).context("그 날짜의 백업이 없습니다.")?;
    let tmp = data_dir.join("bible.restore.db.tmp");
    std::fs::copy(b["db"].as_str().unwrap_or(""), &tmp)?;
    std::fs::rename(&tmp, data_dir.join("bible.restore.db"))?;
    if let Some(st) = b["settings"].as_str().filter(|x| !x.is_empty()) {
        std::fs::copy(st, data_dir.join("settings.restore.json"))?;
    }
    log::info!("되돌리기 예약: {day}");
    Ok(json!({ "day": day, "restartNeeded": true }))
}