//! 첫 실행·자료 이전 보조 (갈래 D 점검, 2026-11-15)
//! - 이전 표시 파일(migrated-from-electron.json): 언제 어디서 무엇을 가져왔는지
//! - 다시 가져오기 판단: 가져온 뒤 Electron 판(1.x) 자료가 또 바뀌었는데 이 판에서는 아직 쓰지 않았으면 저절로 다시 가져오고,
//!   양쪽 다 바뀌었거나(표시 파일 없이) 예전 2.x 자료가 1.x 자료보다 낡았으면 첫 화면에서 묻는다. 지금 자료는 pre-reimport\ 에 보관.
//! - DB 복사는 SQLite 온라인 백업(쪽 단위 복사)으로: VACUUM INTO 는 실제 자료(163MB)에서 11초 걸렸다. 진행률을 시작 안내 창에 보인다
//! - 시작 창: 켜자마자 작은 시작 창을 띄워 더블클릭 뒤 아무것도 안 보이는 일이 없게 (조작 화면 첫 프레임이 그려지면 닫힘)
//! - 두 번 실행: 먼저 켜진 프로세스 번호(instance.pid)의 창을 앞으로 (같은 제목의 Electron 판 창을 잘못 올리지 않게)
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

pub const MARKER: &str = "migrated-from-electron.json";
pub const REQUEST: &str = "reimport.request";
pub const ASIDE_DIR: &str = "pre-reimport";
/// DB 복사 진행률 (0..=1000). 0 이면 아직 시작 안 함
pub static PROGRESS: AtomicU32 = AtomicU32::new(0);
static SPLASH_DONE: AtomicBool = AtomicBool::new(false);
static ASK: once_cell::sync::Lazy<parking_lot::Mutex<Option<Value>>> = once_cell::sync::Lazy::new(|| parking_lot::Mutex::new(None));

fn same_dir(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().trim_end_matches(['\\', '/']).eq_ignore_ascii_case(b.to_string_lossy().trim_end_matches(['\\', '/']))
}

fn mtime_ms(p: &Path) -> i64 {
    std::fs::metadata(p).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// 자료 폴더에서 가장 늦게 바뀐 시각 (DB·WAL·설정)
fn latest_mtime(dir: &Path) -> i64 {
    ["bible.db", "bible.db-wal", "settings.json"].iter().map(|f| mtime_ms(&dir.join(f))).max().unwrap_or(0)
}

/// 사용자가 만든 내용의 지문: 큐시트·찬양의 개수와 마지막 만든/고친/지운 시각. 읽기만 한다
pub fn content_stamp(db: &Path) -> String {
    let Ok(c) = rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY) else { return String::new() };
    let mut out = vec![];
    for t in ["Cuesheets", "Songs"] {
        let cols: Vec<String> = c
            .prepare(&format!("PRAGMA table_info({t})"))
            .and_then(|mut s| s.query_map([], |r| r.get::<_, String>(1))?.collect())
            .unwrap_or_default();
        if cols.is_empty() {
            out.push(format!("{t}:-"));
            continue;
        }
        let mut parts = vec!["count(*)".to_string()];
        for k in ["updated_at", "created_at", "deleted_at"] {
            if cols.iter().any(|x| x == k) {
                parts.push(format!("ifnull(max({k}),'')"));
            }
        }
        let sql = format!("SELECT {} FROM {t}", parts.join(" || '|' || "));
        let v: String = c.query_row(&sql, [], |r| r.get(0)).unwrap_or_default();
        out.push(format!("{t}:{v}"));
    }
    out.join(";")
}

/// 쓰는 동안 저절로 바뀌는 설정 칸 (포트·검색 기록·마지막에 연 것·통계·화면 배치). '사용자가 설정을 고쳤나' 를 볼 때는 뺀다.
/// 여기 없는 칸이 저절로 바뀌면 묻는 쪽으로 기운다 (말없이 바꾸는 것보다 안전한 쪽)
const VOLATILE_SETTINGS: [&str; 12] =
    ["port", "history", "activeCueId", "versionId", "parallelVersionId", "searchMode", "skipVersion", "telemetryDay", "telemetryId", "cueMgr", "thumbsFold", "rsEditorFrac"];

/// 키 순서와 상관없이 같은 내용이면 같은 글이 되게
fn canon(v: &Value, out: &mut String) {
    match v {
        Value::Object(o) => {
            let mut keys: Vec<&String> = o.keys().collect();
            keys.sort();
            out.push('{');
            for k in keys {
                out.push_str(&serde_json::to_string(k).unwrap_or_default());
                out.push(':');
                canon(&o[k.as_str()], out);
                out.push(',');
            }
            out.push('}');
        }
        Value::Array(a) => {
            out.push('[');
            for x in a {
                canon(x, out);
                out.push(',');
            }
            out.push(']');
        }
        x => out.push_str(&x.to_string()),
    }
}

/// 설정 지문: 템플릿·단축키·출력 설정처럼 사용자가 고친 내용만 본다. 읽기만 한다.
/// 1.x 가 쓴 파일과 이 판이 다시 쓴 파일(기본값 채움·템플릿 다듬기)이 같은 지문이 되도록 settings.rs 의 읽기와 같은 다듬기를 거친다.
/// 파일이 없으면 "-", 깨졌으면 "?"
pub fn settings_stamp(dir: &Path) -> String {
    use crate::settings::{deep_merge_defaults, merge_into, normalize_template, DEFAULTS};
    use sha2::{Digest, Sha256};
    let Ok(txt) = std::fs::read_to_string(dir.join("settings.json")) else { return "-".into() };
    let Some(raw) = serde_json::from_str::<Value>(txt.trim_start_matches('\u{feff}')).ok().filter(|v| v.is_object()) else { return "?".into() };
    let mut d = deep_merge_defaults(&DEFAULTS, &raw);
    let mut sc = DEFAULTS["shortcuts"].clone();
    merge_into(&mut sc, &raw["shortcuts"]);
    d["shortcuts"] = sc;
    let mut tpls: Vec<Value> = match raw["templates"].as_array() {
        Some(a) if !a.is_empty() => a.iter().map(normalize_template).collect(),
        _ => DEFAULTS["templates"].as_array().cloned().unwrap_or_default(),
    };
    if !tpls.iter().any(|t| t["id"] == "overlay-lyrics") {
        if let Some(p) = DEFAULTS["templates"].as_array().and_then(|a| a.iter().find(|t| t["id"] == "overlay-lyrics")) {
            tpls.push(normalize_template(p));
        }
    }
    d["templates"] = json!(tpls);
    if let Some(o) = d.as_object_mut() {
        for k in VOLATILE_SETTINGS {
            o.remove(k);
        }
    }
    // 오버레이가 지금 켜져 있는지는 쓰는 동안 바뀌는 상태 ('모두 지우기' 가 끈다)
    for o in d["overlays"].as_array_mut().into_iter().flatten() {
        if let Some(o) = o.as_object_mut() {
            o.remove("on");
        }
    }
    let mut text = String::new();
    canon(&d, &mut text);
    let mut h = Sha256::new();
    h.update(text.as_bytes());
    h.finalize().iter().take(12).map(|b| format!("{b:02x}")).collect()
}

/// 성경 번역본·주석 지문 (개수 | 마지막 가져온 시각 | 절·항목 수). 표가 없으면 빈 것과 같게. 읽기만 한다
fn library_stamp(db: &Path) -> String {
    let c = rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).ok();
    [("BibleVersions", "verse_count"), ("Commentaries", "entry_count")]
        .iter()
        .map(|(t, n)| {
            let sql = format!("SELECT count(*) || '|' || ifnull(max(imported_at),'') || '|' || ifnull(sum({n}),0) FROM {t}");
            let v = c.as_ref().and_then(|c| c.query_row(&sql, [], |r| r.get::<_, String>(0)).ok()).unwrap_or_else(|| "0||0".into());
            format!("{t}:{v}")
        })
        .collect::<Vec<_>>()
        .join(";")
}

/// 큐시트·찬양 말고 사용자가 바꿀 수 있는 것(설정·템플릿·성경 번역본·주석)의 지문
pub fn extra_stamp(dir: &Path) -> String {
    format!("settings:{};{}", settings_stamp(dir), library_stamp(&dir.join("bible.db")))
}

fn count_of(stamp: &str, table: &str) -> i64 {
    stamp.split(';').find_map(|p| p.strip_prefix(&format!("{table}:"))).and_then(|v| v.split('|').next()).and_then(|n| n.parse().ok()).unwrap_or(0)
}

/// 지문 안의 가장 늦은 만든/고친/지운 시각 (ISO 문자열이라 글자 순서로 견줄 수 있다)
fn last_edit(stamp: &str) -> String {
    stamp.split(';').flat_map(|p| p.split_once(':').map(|x| x.1).unwrap_or("").split('|').skip(1)).max().unwrap_or("").to_string()
}

fn info_of(reason: &str, old: &Path, data_dir: &Path, e_now: &str, r_now: &str) -> Value {
    json!({
        "reason": reason, "from": old.to_string_lossy(), "dataDir": data_dir.to_string_lossy(),
        "electronCues": count_of(e_now, "Cuesheets"), "electronSongs": count_of(e_now, "Songs"),
        "rustCues": count_of(r_now, "Cuesheets"), "rustSongs": count_of(r_now, "Songs"),
        "electronChanged": latest_mtime(old), "rustChanged": latest_mtime(data_dir),
    })
}

/// 밀리초 시각 → 이 PC 시간대의 "2026-11-15 18:59" (날짜 크레이트 없이 SQLite 로)
pub fn local_time(ms: i64) -> String {
    if ms <= 0 {
        return String::new();
    }
    rusqlite::Connection::open_in_memory()
        .and_then(|c| c.query_row("SELECT strftime('%Y-%m-%d %H:%M', ?1, 'unixepoch', 'localtime')", [ms / 1000], |r| r.get::<_, String>(0)))
        .unwrap_or_default()
}

/// 메뉴 '1.x 자료 다시 가져오기…': 같은 묻는 창을 연다. 1.x 자료가 없으면 reason 이 "none"
pub fn menu_ask() {
    let Some(data_dir) = CUR_DIR.lock().clone() else { return };
    let old = crate::core::legacy_data_dir().filter(|o| o.join("bible.db").is_file() && !same_dir(o, &data_dir));
    let v = match old {
        Some(old) => info_of("menu", &old, &data_dir, &content_stamp(&old.join("bible.db")), &content_stamp(&data_dir.join("bible.db"))),
        None => json!({ "reason": "none", "dataDir": data_dir.to_string_lossy() }),
    };
    set_ask(Some(v));
}

pub fn read_marker(data_dir: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(data_dir.join(MARKER)).ok()?).ok()
}

/// 지금 두 자료의 상태를 표시 파일에 적는다 (가져온 직후, 또는 '지금 자료 그대로 쓰기' 를 골랐을 때)
pub fn write_marker(data_dir: &Path, old: &Path, how: &str) {
    let m = json!({
        "from": old.to_string_lossy(),
        "at": crate::util::iso_now(),
        "how": how,
        "electron": content_stamp(&old.join("bible.db")),
        "electronSettingsMtime": mtime_ms(&old.join("settings.json")),
        "rust": content_stamp(&data_dir.join("bible.db")),
        // 2.x 에서 설정·성경 번역본만 고친 것도 '이 판에서 고침'으로 보려고 (없는 표시 파일은 예전 판이 쓴 것)
        "electronExtra": extra_stamp(old),
        "rustExtra": extra_stamp(data_dir),
        "app": env!("CARGO_PKG_VERSION"),
    });
    let tmp = data_dir.join(format!("{MARKER}.tmp"));
    if std::fs::write(&tmp, serde_json::to_string_pretty(&m).unwrap_or_default()).is_ok() {
        let _ = std::fs::rename(&tmp, data_dir.join(MARKER));
    }
}

#[derive(Debug, PartialEq)]
pub enum Recheck {
    Nothing,
    /// 지금 자료를 보관하고 다시 가져온다 (이유)
    Reimport(String),
    /// 첫 화면에서 묻는다
    Ask(Value),
}

/// 이미 이 판 자료(bible.db)가 있을 때: 1.x 자료를 다시 가져와야 하나
pub fn recheck(data_dir: &Path, old: Option<&Path>) -> Recheck {
    if data_dir.join(REQUEST).exists() {
        let _ = std::fs::remove_file(data_dir.join(REQUEST));
        return Recheck::Reimport("다시 가져오기를 골랐습니다".into());
    }
    let Some(old) = old.filter(|o| o.join("bible.db").is_file() && !same_dir(o, data_dir)) else { return Recheck::Nothing };
    let e_now = content_stamp(&old.join("bible.db"));
    if e_now.is_empty() {
        return Recheck::Nothing;
    }
    let r_now = content_stamp(&data_dir.join("bible.db"));
    let info = |reason: &str| info_of(reason, old, data_dir, &e_now, &r_now);
    match read_marker(data_dir) {
        Some(m) => {
            let (e_extra, r_extra) = (extra_stamp(old), extra_stamp(data_dir));
            // 예전 판이 쓴 표시 파일에는 설정·성경 지문이 없다
            let has_extra = m["electronExtra"].is_string() && m["rustExtra"].is_string();
            let e_db_changed = m["electron"].as_str() != Some(e_now.as_str());
            let e_changed = e_db_changed
                || if has_extra {
                    // 1.x 를 켜기만 해도 설정 파일 시각은 바뀐다 → 내용(저절로 바뀌는 칸 제외)으로 본다
                    m["electronExtra"].as_str() != Some(e_extra.as_str())
                } else {
                    mtime_ms(&old.join("settings.json")) > m["electronSettingsMtime"].as_i64().unwrap_or(0) + 2000
                };
            if !e_changed {
                return Recheck::Nothing;
            }
            if !has_extra && !e_db_changed && m["rust"].as_str() == Some(r_now.as_str()) && e_extra == r_extra {
                // 예전 표시 파일 + 1.x 설정 파일 시각만 바뀜 + 두 쪽 설정·성경이 지금 같다 → 가져올 것도 잃을 것도 없다. 표시만 새로 적는다
                write_marker(data_dir, old, m["how"].as_str().unwrap_or("existing"));
                return Recheck::Nothing;
            }
            // 묻지 않고 바꾸는 것은 '1.x 에서 그대로 복사해 온 뒤 이 판에서 아무것도 고치지 않았다'가 확실할 때만:
            // 큐시트·찬양뿐 아니라 설정·템플릿·성경 번역본도 그대로여야 하고(예전 표시 파일은 알 수 없으니 묻는다),
            // '지금 자료 그대로 쓰기'를 골랐거나(kept) 원래 있던 2.x 자료(existing)는 1.x 와 다른 자료이므로 늘 묻는다
            let r_untouched = m["rust"].as_str() == Some(r_now.as_str()) && has_extra && m["rustExtra"].as_str() == Some(r_extra.as_str()) && m["how"] == "migrated";
            if r_untouched {
                Recheck::Reimport("가져온 뒤 1.x 판 자료가 바뀌었고 이 판에서는 아직 고친 것이 없습니다".into())
            } else {
                Recheck::Ask(info("both"))
            }
        }
        None => {
            // 표시 파일이 없는 예전 2.x 자료 (2.0.1 전 개발·시험 실행이 남긴 것 등)
            // 파일 시각만 보면 안 된다: 낡은 2.x 자료로 한 번 켜기만 해도 설정·WAL 시각이 새로워진다.
            // 내용(큐시트·찬양의 마지막 고친 시각·개수)으로도 견준다
            let newer = latest_mtime(old) > latest_mtime(data_dir) || last_edit(&e_now) > last_edit(&r_now) || count_of(&e_now, "Cuesheets") > count_of(&r_now, "Cuesheets");
            if e_now == r_now || !newer {
                write_marker(data_dir, old, "existing");
                Recheck::Nothing
            } else {
                Recheck::Ask(info("nomarker"))
            }
        }
    }
}

/// 지금 이 판 자료를 data\pre-reimport\<시각>\ 으로 옮겨 보관한다 (지우지 않음)
pub fn set_aside(data_dir: &Path) -> std::io::Result<PathBuf> {
    let ts: String = crate::util::iso_now().chars().take(19).map(|c| if c == ':' || c == 'T' { '-' } else { c }).collect();
    let mut dir = data_dir.join(ASIDE_DIR).join(&ts);
    let mut n = 1;
    while dir.exists() {
        n += 1;
        dir = data_dir.join(ASIDE_DIR).join(format!("{ts}-{n}"));
    }
    std::fs::create_dir_all(&dir)?;
    for f in ["bible.db", "bible.db-wal", "bible.db-shm", "settings.json", "settings.backup.json", MARKER] {
        let p = data_dir.join(f);
        if p.exists() {
            std::fs::rename(&p, dir.join(f))?;
        }
    }
    Ok(dir)
}

/// SQLite 온라인 백업으로 DB 를 한 시점 그대로 복사 (다른 프로그램이 열어 써도 안전, WAL 내용 포함). 진행률은 PROGRESS
/// (켜는 중 화면이 읽는 전역 값 — 프로그램에서는 한 번에 하나만 복사한다)
pub fn copy_db(src: &Path, dst: &Path) -> rusqlite::Result<()> {
    copy_db_with(src, dst, &|p| PROGRESS.store(p, Ordering::Relaxed))
}

/// copy_db 와 같되 진행률(1..=1000)을 받는 쪽을 따로 준다. 전역 PROGRESS 를 건드리지 않으므로
/// 여러 복사가 동시에 돌아도(병렬 시험) 서로의 진행률을 덮지 않는다
pub fn copy_db_with(src: &Path, dst: &Path, progress: &dyn Fn(u32)) -> rusqlite::Result<()> {
    let t0 = std::time::Instant::now();
    let from = rusqlite::Connection::open_with_flags(src, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut to = rusqlite::Connection::open(dst)?;
    // 받는 쪽은 임시 이름(bible.db.migrating)이고 다 된 뒤에야 bible.db 로 바꾸므로 쪽마다 디스크에 밀어 넣을 필요가 없다
    // (끝에 한 번만 밀어 넣는다). 기본값(FULL)으로는 디스크가 바쁠 때 수십 초가 걸렸다
    let _ = to.execute_batch("PRAGMA synchronous=OFF;");
    let mut busy = 0u32;
    {
        let b = rusqlite::backup::Backup::new(&from, &mut to)?;
        loop {
            let r = b.step(4096)?;
            let p = b.progress();
            if p.pagecount > 0 {
                progress((((p.pagecount - p.remaining) as i64 * 1000) / p.pagecount as i64).clamp(1, 1000) as u32);
            }
            match r {
                rusqlite::backup::StepResult::Done => break,
                rusqlite::backup::StepResult::More => {}
                _ => {
                    busy += 1;
                    std::thread::sleep(std::time::Duration::from_millis(50))
                }
            }
        }
    }
    // 열어 보기 점검 (깨진 파일이면 여기서 실패)
    let t1 = t0.elapsed();
    let ok: String = to.query_row("PRAGMA quick_check(1)", [], |r| r.get(0))?;
    drop(to);
    if ok != "ok" {
        return Err(rusqlite::Error::InvalidQuery);
    }
    let t2 = t0.elapsed();
    // 한 번에 디스크로 (이름을 바꾸기 전에)
    if let Ok(f) = std::fs::OpenOptions::new().write(true).open(dst) {
        let _ = f.sync_all();
    }
    log::info!(
        "자료 복사 {:.0}MB: 복사 {}ms · 점검 {}ms · 디스크에 쓰기 {}ms (기다림 {busy}회)",
        std::fs::metadata(dst).map(|m| m.len()).unwrap_or(0) as f64 / 1_048_576.0,
        t1.as_millis(),
        (t2 - t1).as_millis(),
        (t0.elapsed() - t2).as_millis()
    );
    Ok(())
}

// ---------------- 옛 설치 파일 정리 ----------------
/// 받아 둔 설치 파일을 지운다 (설치판에서만 부른다).
/// - update_cache: 이 판이 받아 둔 설치 파일(자료 폴더\update-cache)의 설치 파일·받다 만 파일을 모두 지운다.
///   지금 받는 중이거나 설치하려고 받아 둔 파일이 있을 수 있으면 cleanup_installers_guarded 를 쓴다
/// - electron_cache: Electron 판 자동 업데이트가 남긴 폴더(%LOCALAPPDATA%\worshipcue-updater, 옛 설치 파일 100MB 이상).
///   그 안에 설치 파일·블록맵·pending 말고 다른 것이 있으면 손대지 않는다
pub fn cleanup_installers(update_cache: &Path, electron_cache: Option<&Path>) -> u64 {
    cleanup_impl(update_cache, electron_cache, false, None)
}

/// 켠 뒤에 부르는 정리: 이번 실행의 업데이트 받기와 부딪치지 않게 한다.
/// - in_use(받는 중·검증 중·설치 준비됨)이면 update_cache 는 아예 건드리지 않는다
/// - 그렇지 않아도 started(이 프로그램이 켜진 시각) 뒤에 만들어졌거나 바뀐 파일은 남긴다
///   (상태를 본 직후에 받기가 시작돼도 그 파일은 지우지 않게). 옛 실행이 남긴 것만 지운다
pub fn cleanup_installers_guarded(update_cache: &Path, electron_cache: Option<&Path>, in_use: bool, started: std::time::SystemTime) -> u64 {
    cleanup_impl(update_cache, electron_cache, in_use, Some(started))
}

fn cleanup_impl(update_cache: &Path, electron_cache: Option<&Path>, in_use: bool, started: Option<std::time::SystemTime>) -> u64 {
    let mut freed = 0u64;
    if in_use {
        log::info!("업데이트 파일을 받는 중이거나 설치 준비가 되어 있어 받아 둔 설치 파일 정리를 건너뜁니다");
    } else if let Ok(rd) = std::fs::read_dir(update_cache) {
        for e in rd.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            // 시각을 읽지 못하면 이번 실행의 것일 수 있으니 남긴다
            let from_this_run = started.is_some_and(|s| {
                e.metadata().map(|m| [m.created(), m.modified()].into_iter().any(|t| t.map(|t| t >= s).unwrap_or(true))).unwrap_or(true)
            });
            if p.is_file() && !from_this_run && (name.ends_with("-Setup.exe") || name.ends_with(".part")) {
                let len = e.metadata().map(|m| m.len()).unwrap_or(0);
                if std::fs::remove_file(&p).is_ok() {
                    freed += len;
                }
            }
        }
    }
    if let Some(dir) = electron_cache.filter(|d| d.file_name().is_some_and(|n| n == "worshipcue-updater") && d.is_dir()) {
        let known = |n: &str| n == "pending" || n.ends_with(".exe") || n.ends_with(".blockmap") || n.ends_with(".json") || n.starts_with("temp-");
        let entries: Vec<_> = std::fs::read_dir(dir).into_iter().flatten().flatten().collect();
        if entries.iter().all(|e| known(&e.file_name().to_string_lossy())) {
            fn size(p: &Path) -> u64 {
                if p.is_dir() {
                    std::fs::read_dir(p).into_iter().flatten().flatten().map(|e| size(&e.path())).sum()
                } else {
                    std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
                }
            }
            let before = size(dir);
            let _ = std::fs::remove_dir_all(dir); // 지금 실행 중인 설치 파일은 잠겨 있어 남는다 → 다음에 켤 때 다시
            freed += before.saturating_sub(if dir.exists() { size(dir) } else { 0 });
        }
    }
    if freed > 0 {
        log::info!("받아 둔 옛 설치 파일을 지웠습니다 ({:.0}MB)", freed as f64 / 1_048_576.0);
    }
    freed
}

/// 설치판이면 켠 뒤 잠시 있다가(설치 파일이 아직 끝나는 중일 수 있다) 옛 설치 파일을 지운다
pub fn start_installer_cleanup(data_dir: &Path) {
    if !crate::updates::installed_copy() {
        return;
    }
    let cache = data_dir.join("update-cache");
    // 이 프로그램이 켜진 시각 (여유 5초): 그 뒤에 생긴 파일은 이번 실행이 받은 것
    let started = std::time::SystemTime::now() - std::time::Duration::from_millis(since_process_start_ms() + 5000);
    std::thread::Builder::new()
        .name("cleanup".into())
        .spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(20));
            cleanup_installers_guarded(&cache, dirs::data_local_dir().map(|d| d.join("worshipcue-updater")).as_deref(), crate::updates::cache_in_use(), started);
        })
        .ok();
}

// ---------------- 1.x 폴더에만 있는 파일 챙기기 ----------------
// 붙여 넣은 PPT(pasted)·베들레헴 찬송가(hymnal)는 처음에 복사하지 않고 1.x 폴더를 그대로 읽는다(첫 실행을 빠르게).
// 그래서 사용자가 %APPDATA%\WorshipCue 를 지우면 그 PPT 순서가 '파일 없음'이 되고 다시 만들 수도 없다.
// → 켠 뒤 한가할 때 이 판 자료 폴더로 복사해 두고, 1.x 쪽 파일이 없어졌으면 큐시트가 복사본을 가리키게 고친다.
// 1.x 폴더는 읽기만 한다. PPT 그림 캐시(hymn-cache)는 다시 만들 수 있어 복사하지 않는다.

/// 1.x 자료 폴더에서 챙겨 둘 폴더
pub const KEEP_DIRS: [&str; 2] = ["pasted", "hymnal"];

/// 다 쓴 임시 파일(part)을 진짜 이름(to)으로 놓는다. 그 이름의 파일이 이미 있으면 덮지 않고 false.
/// 윈도우의 이름 바꾸기(rename)는 있던 파일을 말없이 덮으므로 쓰지 않는다: 하드 링크는 대상이 있으면 실패한다(확인과 놓기가 한 동작).
/// 하드 링크를 못 만드는 디스크(FAT·일부 네트워크 폴더)에서는 '새로 만들기 전용'으로 열어 옮겨 쓴다.
/// 어느 쪽이든 임시 파일은 지운다
fn place_no_clobber(part: &Path, to: &Path) -> std::io::Result<bool> {
    use std::io::ErrorKind::AlreadyExists;
    let r = match std::fs::hard_link(part, to) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == AlreadyExists || to.exists() => Ok(false),
        Err(_) => match std::fs::OpenOptions::new().write(true).create_new(true).open(to) {
            Err(e) if e.kind() == AlreadyExists => Ok(false),
            Err(e) => Err(e),
            Ok(mut out) => {
                let r = std::fs::File::open(part).and_then(|mut i| std::io::copy(&mut i, &mut out)).and_then(|_| out.sync_all());
                drop(out);
                if r.is_err() {
                    let _ = std::fs::remove_file(to); // 방금 우리가 새로 만든 반쪽 파일만 (create_new 가 성공했으므로 남의 파일이 아니다)
                }
                r.map(|_| true)
            }
        },
    };
    let _ = std::fs::remove_file(part);
    r
}

fn copy_missing(src: &Path, dst: &Path, done: &mut (usize, usize, u64)) {
    copy_missing_with(src, dst, done, &mut |_| {})
}

/// before_place: 임시 파일을 다 쓰고 진짜 이름으로 놓기 직전에 부른다 (시험에서 '그 사이 같은 이름 파일이 생김'을 만든다)
fn copy_missing_with(src: &Path, dst: &Path, done: &mut (usize, usize, u64), before_place: &mut dyn FnMut(&Path)) {
    let Ok(rd) = std::fs::read_dir(src) else { return };
    for e in rd.flatten() {
        let (from, to) = (e.path(), dst.join(e.file_name()));
        match e.file_type() {
            Ok(t) if t.is_dir() => copy_missing_with(&from, &to, done, before_place),
            // 이미 있으면 손대지 않는다 (이 판에서 새로 가져온 찬송가를 옛것으로 덮지 않게)
            Ok(t) if t.is_file() && !to.exists() => {
                // 임시 이름으로 다 쓴 뒤 이름을 바꾼다: 복사 도중 꺼져도 반쪽 파일이 진짜 이름으로 남지 않게
                let mut part = to.clone().into_os_string();
                part.push(".part");
                let part = PathBuf::from(part);
                // 진짜 이름으로 놓을 때는 덮지 않는다: 위에서 '없음'을 확인한 뒤 복사하는 사이(큰 파일은 몇 초)에
                // 이 판에서 같은 이름의 찬송가를 새로 가져왔을 수 있다
                let r = std::fs::create_dir_all(dst).and_then(|_| std::fs::copy(&from, &part)).and_then(|n| {
                    before_place(&to);
                    place_no_clobber(&part, &to).map(|placed| placed.then_some(n))
                });
                match r {
                    Ok(Some(n)) => {
                        done.0 += 1;
                        done.2 += n;
                    }
                    Ok(None) => log::info!("1.x 파일을 챙기는 사이 같은 이름의 새 파일이 생겨 그대로 둠: {}", to.display()),
                    Err(e) => {
                        let _ = std::fs::remove_file(&part);
                        done.1 += 1;
                        log::warn!("1.x 파일을 챙기지 못함: {} ({e})", from.display());
                    }
                }
            }
            _ => {}
        }
    }
}

/// 1.x 폴더의 pasted·hymnal 가운데 이 판 자료 폴더에 없는 파일을 복사한다. → (복사한 수, 실패한 수, 바이트)
pub fn keep_legacy_files(data_dir: &Path, old: &Path) -> (usize, usize, u64) {
    let mut done = (0, 0, 0);
    if same_dir(old, data_dir) {
        return done;
    }
    for d in KEEP_DIRS {
        let from = old.join(d);
        if from.is_dir() {
            copy_missing(&from, &data_dir.join(d), &mut done);
        }
    }
    done
}

/// 켠 뒤 잠시 있다가(켜는 시간·첫 조작을 느리게 하지 않게) 백그라운드로 챙긴다
pub fn start_keep_legacy_files(data_dir: &Path) {
    let Some(old) = crate::core::legacy_data_dir().filter(|o| o.is_dir()) else { return };
    let data_dir = data_dir.to_path_buf();
    std::thread::Builder::new()
        .name("keep-legacy".into())
        .spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(20));
            let t0 = std::time::Instant::now();
            let (ok, failed, bytes) = keep_legacy_files(&data_dir, &old);
            if ok + failed > 0 {
                log::info!("1.x 폴더에만 있던 파일 챙김: {ok}개 {}MB, 실패 {failed}개, {}ms ({} → {})", bytes / 1_048_576, t0.elapsed().as_millis(), old.display(), data_dir.display());
            }
        })
        .ok();
}

/// 큐시트 안의 `<1.x 자료>\pasted\…` 경로 가운데 그 파일이 없어졌고 이 판 자료 폴더에 복사본이 있는 것을 복사본으로 바꾼다.
/// 1.x 파일이 그대로 있으면 아무것도 바꾸지 않는다(1.x 가 만들어 둔 PPT 그림 캐시를 계속 쓴다). 고친 큐시트 수를 돌려준다.
/// updated_at 은 그대로 둔다 (내용이 아니라 이 PC 의 파일 자리만 바뀐 것이라 '이 판에서 고침'으로 세지 않게).
pub fn repoint_pasted(db: &rusqlite::Connection, data_dir: &Path, old: &Path) -> usize {
    if same_dir(old, data_dir) {
        return 0;
    }
    let norm = |s: &str| s.replace('/', "\\").to_lowercase();
    let prefix = norm(&old.join("pasted").to_string_lossy()).trim_end_matches('\\').to_string() + "\\";
    let n_prefix = prefix.chars().count();
    let rows: Vec<(String, String)> = db
        .prepare("SELECT id, items FROM Cuesheets WHERE items LIKE '%pasted%'")
        .and_then(|mut s| s.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect())
        .unwrap_or_default();
    fn strings<'a>(v: &'a Value, out: &mut Vec<&'a str>) {
        match v {
            Value::String(s) => out.push(s),
            Value::Array(a) => a.iter().for_each(|x| strings(x, out)),
            Value::Object(o) => o.values().for_each(|x| strings(x, out)),
            _ => {}
        }
    }
    let mut fixed = 0;
    for (id, items) in rows {
        let Ok(v) = serde_json::from_str::<Value>(&items) else { continue };
        let mut all = vec![];
        strings(&v, &mut all);
        let mut text = items.clone();
        for s in all {
            if !norm(s).starts_with(&prefix) || Path::new(s).exists() {
                continue;
            }
            let rel: String = s.chars().skip(n_prefix).collect();
            let new = data_dir.join("pasted").join(&rel);
            if !new.is_file() {
                continue;
            }
            // 글 그대로 바꾼다 (JSON 을 다시 쓰면 키 순서가 바뀔 수 있다)
            let esc = |x: &str| serde_json::to_string(x).unwrap_or_default();
            text = text.replace(&esc(s), &esc(&new.to_string_lossy()));
        }
        if text != items && serde_json::from_str::<Value>(&text).is_ok() && db.execute("UPDATE Cuesheets SET items = ?1 WHERE id = ?2", [&text, &id]).is_ok() {
            fixed += 1;
        }
    }
    if fixed > 0 {
        log::info!("1.x 폴더에서 없어진 붙여 넣은 PPT {fixed}개 큐시트를 이 판 복사본으로 연결 ({})", data_dir.join("pasted").display());
    }
    fixed
}

// ---------------- 켜는 시간 재기 ----------------
static MARKS: once_cell::sync::Lazy<parking_lot::Mutex<Vec<(&'static str, u64)>>> = once_cell::sync::Lazy::new(|| parking_lot::Mutex::new(vec![]));
static FRAMES: AtomicU32 = AtomicU32::new(0);

/// 프로세스가 만들어진 뒤 지난 시간(ms). 실행 파일 읽기·백신 검사 시간도 들어간다
pub fn since_process_start_ms() -> u64 {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::Foundation::FILETIME;
        use windows_sys::Win32::System::SystemInformation::GetSystemTimeAsFileTime;
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
        let z = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
        let (mut c, mut e, mut k, mut u, mut now) = (z, z, z, z, z);
        if GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut k, &mut u) != 0 {
            GetSystemTimeAsFileTime(&mut now);
            let v = |f: FILETIME| ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64;
            return v(now).saturating_sub(v(c)) / 10_000;
        }
    }
    0
}

/// 시작 단계 표시 (첫 프레임 뒤 한 줄로 로그에 남긴다)
pub fn mark(name: &'static str) {
    MARKS.lock().push((name, since_process_start_ms()));
}

/// 조작 화면이 프레임을 그릴 때마다 부른다: 첫 프레임 시작·끝 시각을 적고 로그로 남긴다
pub fn frame_tick() {
    let n = FRAMES.fetch_add(1, Ordering::Relaxed);
    if n > 1 {
        return;
    }
    mark(if n == 0 { "첫 프레임 시작" } else { "첫 프레임 끝" });
    if n == 1 {
        splash_done(); // 조작 화면이 실제로 그려졌다 → 시작 창 닫기
    }
    if n == 1 {
        let line = MARKS.lock().iter().map(|(k, v)| format!("{k} {v}")).collect::<Vec<_>>().join(" → ");
        log::info!("켜는 시간(ms, 프로세스 시작부터): {line}");
    }
}

// ---------------- 첫 화면에서 묻기 ----------------
static CUR_DIR: once_cell::sync::Lazy<parking_lot::Mutex<Option<PathBuf>>> = once_cell::sync::Lazy::new(|| parking_lot::Mutex::new(None));
pub fn set_ask(v: Option<Value>) {
    *ASK.lock() = v;
}
pub fn ask() -> Option<Value> {
    ASK.lock().clone()
}

/// 켤 때 생긴 중요한 문제(깨진 설정 파일로 백업·기본 설정을 씀, 1.x 자료를 복사하지 못함): 금방 사라지는 알림 말고
/// 첫 화면에 [확인] 을 누를 때까지 남는 창으로 보인다 (알림은 다른 알림이 뜨면 바로 덮인다)
static START_WARN: once_cell::sync::Lazy<parking_lot::Mutex<Vec<String>>> = once_cell::sync::Lazy::new(|| parking_lot::Mutex::new(vec![]));
pub fn add_start_warning(msg: impl Into<String>) {
    START_WARN.lock().push(msg.into());
}
pub fn start_warnings() -> Vec<String> {
    START_WARN.lock().clone()
}
pub fn clear_start_warnings() {
    START_WARN.lock().clear();
}
/// '지금 자료 그대로 쓰기': 지금 상태를 표시해 두어 1.x 자료가 또 바뀌기 전까지는 묻지 않는다
pub fn keep_current(data_dir: &Path) {
    if let Some(a) = ask().filter(|a| a["reason"] != "menu" && a["reason"] != "none") {
        if let Some(from) = a["from"].as_str() {
            write_marker(data_dir, Path::new(from), "kept");
        }
    }
    set_ask(None);
}

static NO_RESTART: AtomicBool = AtomicBool::new(false);
/// 시험용: 다시 가져오기를 골라도 프로그램을 다시 띄우지 않는다 (요청 파일만 남김)
pub fn set_no_restart_for_test(v: bool) {
    NO_RESTART.store(v, Ordering::SeqCst);
}

/// '1.x 자료 다시 가져오기': 요청 파일을 남기고 이 프로그램을 다시 띄운다 (새 프로세스는 지금 프로세스가 끝나길 기다렸다 가져온다)
pub fn request_reimport_and_restart(data_dir: &Path) -> std::io::Result<()> {
    std::fs::write(data_dir.join(REQUEST), crate::util::iso_now())?;
    let exe = std::env::current_exe()?;
    let mut args: Vec<String> = vec![];
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        if a == "--wait-pid" {
            it.next();
            continue;
        }
        args.push(a);
    }
    args.push("--wait-pid".into());
    args.push(std::process::id().to_string());
    if !NO_RESTART.load(Ordering::SeqCst) {
        std::process::Command::new(exe).args(args).spawn()?;
    }
    set_ask(None);
    Ok(())
}

/// --wait-pid: 앞 프로세스가 끝날 때까지 (최대 30초) 기다린다
pub fn wait_for_pid(pid: u32) {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE};
        let h = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        if !h.is_null() {
            WaitForSingleObject(h, 30_000);
            CloseHandle(h);
        }
    }
    #[cfg(not(windows))]
    let _ = pid;
}

// ---------------- 두 번 실행 ----------------
/// 두 번 실행 검사에 쓴 핸들(뮤텍스)을 닫는다 — 먼저 켜진 것이 끝나길 기다렸다 다시 검사하려고
pub fn close_instance_handle(h: Option<isize>) {
    #[cfg(windows)]
    if let Some(h) = h.filter(|h| *h != 0) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(h as _);
        }
    }
    #[cfg(not(windows))]
    let _ = h;
}
pub fn write_instance_pid(data_dir: &Path) {
    *CUR_DIR.lock() = Some(data_dir.to_path_buf());
    let _ = std::fs::write(data_dir.join("instance.pid"), std::process::id().to_string());
}

/// 먼저 켜진 워십프리젠터(같은 자료 폴더)의 창. 프로세스 번호로 찾으므로 같은 제목의 다른 프로그램(Electron 1.x 등)은 건드리지 않는다.
/// 조작 화면(winit 창)이나 시작 창만 센다 — winit 의 보이지 않는 보조 창('Winit Thread Event Target')은 창이 닫힌 뒤에도
/// 프로세스가 끝날 때까지 남아 있어서, 그것을 세면 '꺼지는 중'인 것을 '켜져 있다'로 잘못 본다
fn instance_window(data_dir: &Path) -> Option<isize> {
    let pid = std::fs::read_to_string(data_dir.join("instance.pid")).ok().and_then(|s| s.trim().parse::<u32>().ok())?;
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::Foundation::{HWND, LPARAM};
        use windows_sys::Win32::UI::WindowsAndMessaging::*;
        struct Find {
            pid: u32,
            main: HWND,
            any: HWND,
        }
        unsafe extern "system" fn cb(h: HWND, l: LPARAM) -> i32 {
            let f = unsafe { &mut *(l as *mut Find) };
            let mut p = 0u32;
            unsafe { GetWindowThreadProcessId(h, &mut p) };
            if p != f.pid || unsafe { IsWindowVisible(h) } == 0 || !unsafe { GetWindow(h, GW_OWNER) }.is_null() {
                return 1;
            }
            let mut buf = [0u16; 64];
            let n = unsafe { GetClassNameW(h, buf.as_mut_ptr(), 64) };
            let class = String::from_utf16_lossy(&buf[..n.max(0) as usize]);
            if class != "Window Class" && class != "WorshipPresenterSplash" {
                return 1;
            }
            let n = unsafe { GetWindowTextW(h, buf.as_mut_ptr(), 64) };
            if String::from_utf16_lossy(&buf[..n.max(0) as usize]) == "워십프리젠터" {
                f.main = h;
                return 0;
            }
            if f.any.is_null() {
                f.any = h;
            }
            1
        }
        let mut f = Find { pid, main: std::ptr::null_mut(), any: std::ptr::null_mut() };
        EnumWindows(Some(cb), &mut f as *mut Find as LPARAM);
        let h = if f.main.is_null() { f.any } else { f.main };
        if h.is_null() { None } else { Some(h as isize) }
    }
    #[cfg(not(windows))]
    {
        let _ = pid;
        None
    }
}

/// 먼저 켜진 것의 창이 (아직) 있나
pub fn instance_has_window(data_dir: &Path) -> bool {
    instance_window(data_dir).is_some()
}

/// 먼저 켜진 워십프리젠터의 창을 앞으로 (작게 해 둔 창이면 되살린다). 창이 없으면 false
pub fn focus_instance(data_dir: &Path) -> bool {
    let Some(h) = instance_window(data_dir) else { return false };
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::*;
        let h = h as windows_sys::Win32::Foundation::HWND;
        if IsIconic(h) != 0 {
            ShowWindow(h, SW_RESTORE);
        }
        SetForegroundWindow(h);
        // 윈도우는 '방금 사용자가 누른 프로그램'이 아니면 맨 앞으로 오기를 막는다(작업 표시줄만 깜박임).
        // 막혔으면 지금 맨 앞 창의 입력 흐름에 잠깐 붙어 다시 해 보고, 그래도 안 되면 창을 다른 창들 위로 올려 눈에 띄게 한다
        if GetForegroundWindow() != h {
            use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
            let fg = GetForegroundWindow();
            let (me, other) = (GetCurrentThreadId(), if fg.is_null() { 0 } else { GetWindowThreadProcessId(fg, std::ptr::null_mut()) });
            if other != 0 && other != me && AttachThreadInput(me, other, 1) != 0 {
                BringWindowToTop(h);
                SetForegroundWindow(h);
                AttachThreadInput(me, other, 0);
            }
        }
        if GetForegroundWindow() != h {
            SetWindowPos(h, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            SetWindowPos(h, HWND_NOTOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            let fi = FLASHWINFO { cbSize: std::mem::size_of::<FLASHWINFO>() as u32, hwnd: h, dwFlags: FLASHW_ALL | FLASHW_TIMERNOFG, uCount: 3, dwTimeout: 0 };
            FlashWindowEx(&fi);
        }
    }
    let _ = h;
    true
}
// ---------------- 시작 안내 창 ----------------
/// 더블클릭하자마자(프로세스 시작 뒤 수십 ms) 작은 시작 창을 띄운다. 조작 화면은 그래픽 준비 때문에 0.6~1초 뒤에야 보이므로
/// 그동안 '켜지고 있다'는 것이 보이게. 자료를 옮길 때는 진행률을 함께 보인다. 조작 화면 첫 프레임이 그려지면 닫힌다(splash_done)
pub fn start_splash() {
    SPLASH_DONE.store(false, Ordering::SeqCst);
    // 점검 실행(--shot·--parity 등 화면을 찍는 실행)에는 띄우지 않는다
    if std::env::args().any(|a| a == "--no-splash" || a == "--shot" || a == "--parity") {
        return;
    }
    #[cfg(windows)]
    {
        // winit 이 나중에 하는 것과 같은 DPI 설정을 창을 만들기 전에 먼저 (창을 먼저 만들면 조작 화면이 흐릿하게 뜰 수 있다)
        unsafe {
            windows_sys::Win32::UI::HiDpi::SetProcessDpiAwarenessContext(windows_sys::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }
        std::thread::Builder::new().name("splash".into()).spawn(|| unsafe { splash_thread() }).ok();
    }
}

#[cfg(windows)]
fn w16(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
unsafe fn splash_thread() {
    use windows_sys::Win32::Graphics::Gdi::*;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    unsafe {
        let hinst = GetModuleHandleW(std::ptr::null());
        let class = w16("WorshipPresenterSplash");
        let mut wc: WNDCLASSW = std::mem::zeroed();
        wc.lpfnWndProc = Some(splash_proc);
        wc.hInstance = hinst;
        wc.lpszClassName = class.as_ptr();
        wc.hCursor = LoadCursorW(std::ptr::null_mut(), IDC_APPSTARTING);
        RegisterClassW(&wc);
        let dpi = windows_sys::Win32::UI::HiDpi::GetDpiForSystem().max(96) as i32;
        let (w, h) = (440 * dpi / 96, 150 * dpi / 96);
        let (sx, sy) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class.as_ptr(),
            w16("워십프리젠터 켜는 중").as_ptr(),
            WS_POPUP | WS_VISIBLE,
            (sx - w) / 2,
            (sy - h) / 2,
            w,
            h,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            hinst,
            std::ptr::null(),
        );
        if hwnd.is_null() {
            return;
        }
        mark("시작 창");
        let mut last = u32::MAX;
        let mut msg: MSG = std::mem::zeroed();
        while !SPLASH_DONE.load(Ordering::SeqCst) {
            while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            let p = PROGRESS.load(Ordering::Relaxed);
            if p != last {
                last = p;
                InvalidateRect(hwnd, std::ptr::null(), 0);
            }
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
        DestroyWindow(hwnd);
    }
}

#[cfg(windows)]
unsafe extern "system" fn splash_proc(h: windows_sys::Win32::Foundation::HWND, m: u32, w: windows_sys::Win32::Foundation::WPARAM, l: windows_sys::Win32::Foundation::LPARAM) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::Graphics::Gdi::*;
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::*;
    unsafe {
        match m {
            WM_ERASEBKGND => 1,
            WM_PAINT => {
                let mut ps: PAINTSTRUCT = std::mem::zeroed();
                let dc = BeginPaint(h, &mut ps);
                let mut rc: RECT = std::mem::zeroed();
                GetClientRect(h, &mut rc);
                let s = |v: i32| v * rc.right / 440; // 96dpi 기준 치수 → 지금 크기
                let rgb = |r: u32, g: u32, b: u32| r | (g << 8) | (b << 16);
                let fill = |r: &RECT, c: u32| {
                    let b = CreateSolidBrush(c);
                    FillRect(dc, r, b);
                    DeleteObject(b);
                };
                fill(&rc, rgb(0x2f, 0x34, 0x40)); // 테두리
                fill(&RECT { left: 1, top: 1, right: rc.right - 1, bottom: rc.bottom - 1 }, rgb(0x17, 0x1a, 0x21)); // 바탕 (조작 화면 어두운 테마와 비슷하게)
                // 아이콘 (실행 파일에 넣은 것, 없으면 건너뜀)
                let icon = LoadImageW(GetModuleHandleW(std::ptr::null()), 1 as *const u16, IMAGE_ICON, s(56), s(56), 0);
                let text_left = if icon.is_null() {
                    s(28)
                } else {
                    DrawIconEx(dc, s(28), s(38), icon, s(56), s(56), 0, std::ptr::null_mut(), DI_NORMAL);
                    DestroyIcon(icon);
                    s(104)
                };
                SetBkMode(dc, TRANSPARENT as _);
                let face = w16("맑은 고딕");
                let big = CreateFontW(-s(24), 0, 0, 0, 700, 0, 0, 0, 1, 0, 0, 5, 0, face.as_ptr());
                let small = CreateFontW(-s(14), 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 5, 0, face.as_ptr());
                let old = SelectObject(dc, big);
                SetTextColor(dc, rgb(0xf2, 0xf4, 0xf8));
                let mut r1 = RECT { left: text_left, top: s(36), right: rc.right - s(20), bottom: s(72) };
                DrawTextW(dc, w16("워십프리젠터").as_ptr(), -1, &mut r1, DT_LEFT | DT_SINGLELINE | DT_VCENTER);
                SelectObject(dc, small);
                SetTextColor(dc, rgb(0x9a, 0xa3, 0xb2));
                let p = PROGRESS.load(Ordering::Relaxed);
                let line = if p == 0 { "켜는 중입니다…".to_string() } else { format!("처음 켜는 중: 예전 자료를 옮기고 있습니다… {}%", p / 10) };
                let mut r2 = RECT { left: text_left, top: s(74), right: rc.right - s(20), bottom: s(98) };
                DrawTextW(dc, w16(&line).as_ptr(), -1, &mut r2, DT_LEFT | DT_SINGLELINE | DT_VCENTER);
                if p > 0 {
                    let (l0, r0) = (s(28), rc.right - s(28));
                    fill(&RECT { left: l0, top: s(118), right: r0, bottom: s(122) }, rgb(0x2f, 0x34, 0x40));
                    fill(&RECT { left: l0, top: s(118), right: l0 + (r0 - l0) * p as i32 / 1000, bottom: s(122) }, rgb(0x4f, 0x8c, 0xff));
                }
                SelectObject(dc, old);
                DeleteObject(big);
                DeleteObject(small);
                EndPaint(h, &ps);
                0
            }
            _ => DefWindowProcW(h, m, w, l),
        }
    }
}
/// 켜지 못했을 때 (창이 없어 아무것도 안 보이는 일이 없게) 오류 상자
pub fn error_box(msg: &str) {
    splash_done();
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR};
        let w16 = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
        MessageBoxW(std::ptr::null_mut(), w16(msg).as_ptr(), w16("워십프리젠터").as_ptr(), MB_ICONERROR);
    }
    #[cfg(not(windows))]
    eprintln!("{msg}");
    // 맥: 시스템 알림 창 (osascript). 큰따옴표·역슬래시는 AppleScript 문자열에 맞게
    #[cfg(target_os = "macos")]
    {
        let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
        let script = format!("display alert \"워십프리젠터\" message \"{}\" as critical", esc(msg));
        let _ = std::process::Command::new("osascript").arg("-e").arg(script).status();
    }
}

pub fn splash_done() {
    SPLASH_DONE.store(true, Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("wp-firstrun-{tag}-{}", rand::random::<u64>()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }
    fn db_with(dir: &Path, cues: &[(&str, &str)]) {
        let c = rusqlite::Connection::open(dir.join("bible.db")).unwrap();
        c.execute_batch("CREATE TABLE IF NOT EXISTS Cuesheets(id TEXT PRIMARY KEY, items TEXT, updated_at TEXT, created_at TEXT, deleted_at TEXT); CREATE TABLE IF NOT EXISTS Songs(id TEXT, updated_at TEXT);").unwrap();
        for (id, at) in cues {
            c.execute("INSERT OR REPLACE INTO Cuesheets(id, items, updated_at, created_at) VALUES(?1,'[]',?2,?2)", [id, at]).unwrap();
        }
    }

    #[test]
    fn keeps_legacy_only_files_and_repoints_when_old_is_gone() {
        let (old, new) = (tmp("ko"), tmp("kn"));
        std::fs::create_dir_all(old.join("pasted")).unwrap();
        std::fs::create_dir_all(old.join("hymnal")).unwrap();
        std::fs::create_dir_all(old.join("hymn-cache").join("k")).unwrap();
        std::fs::write(old.join("pasted").join("clip-가.pptx"), b"ppt-a").unwrap();
        std::fs::write(old.join("pasted").join("clip-b.pptx"), b"ppt-b").unwrap();
        std::fs::write(old.join("hymnal").join("hymn.hdb"), b"old-hdb").unwrap();
        std::fs::write(old.join("hymnal").join("hymn.cmp"), b"old-cmp").unwrap();
        std::fs::write(old.join("hymn-cache").join("k").join("meta.json"), b"{}").unwrap();
        // 이 판에서 새로 가져온 찬송가는 덮지 않는다
        std::fs::create_dir_all(new.join("hymnal")).unwrap();
        std::fs::write(new.join("hymnal").join("hymn.hdb"), b"newer").unwrap();
        let a = old.join("pasted").join("clip-가.pptx").to_string_lossy().to_string();
        let b = old.join("pasted").join("clip-b.pptx").to_string_lossy().replace('\\', "/"); // 슬래시로 적힌 경로도
        let gone = old.join("pasted").join("never-copied.pptx").to_string_lossy().to_string();
        let q = |s: &str| serde_json::to_string(s).unwrap();
        // 키 순서가 가나다순이 아닌 글 (Electron 이 쓴 그대로 남는지 보려고)
        let items = format!(r#"[{{"z":1,"type":"hymn","file":{},"a":2}},{{"type":"hymn","file":{}}},{{"type":"hymn","file":{}}},{{"type":"hymn","file":"D:\\찬송가\\pasted\\1.pptx"}}]"#, q(&a), q(&b), q(&gone));
        let c = rusqlite::Connection::open(new.join("bible.db")).unwrap();
        c.execute_batch("CREATE TABLE Cuesheets(id TEXT PRIMARY KEY, items TEXT, updated_at TEXT);").unwrap();
        c.execute("INSERT INTO Cuesheets VALUES('c1', ?1, 't0')", [&items]).unwrap();
        c.execute("INSERT INTO Cuesheets VALUES('c2', '[]', 't0')", []).unwrap();
        let get = |c: &rusqlite::Connection| c.query_row("SELECT items, updated_at FROM Cuesheets WHERE id='c1'", [], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))).unwrap();

        // 복사본이 아직 없으면 아무것도 안 바꾼다
        assert_eq!(repoint_pasted(&c, &new, &old), 0);
        assert_eq!(keep_legacy_files(&new, &old), (3, 0, 5 + 5 + 7));
        assert_eq!(std::fs::read(new.join("pasted").join("clip-가.pptx")).unwrap(), b"ppt-a");
        assert_eq!(std::fs::read(new.join("hymnal").join("hymn.hdb")).unwrap(), b"newer");
        assert_eq!(std::fs::read(new.join("hymnal").join("hymn.cmp")).unwrap(), b"old-cmp");
        assert!(!new.join("hymn-cache").exists());
        assert!(!std::fs::read_dir(new.join("pasted")).unwrap().flatten().any(|e| e.file_name().to_string_lossy().ends_with(".part")));
        assert_eq!(keep_legacy_files(&new, &old), (0, 0, 0)); // 두 번째는 할 일 없음
        // 1.x 파일이 그대로 있으면 큐시트를 안 바꾼다 (1.x 캐시를 계속 쓴다)
        assert_eq!(repoint_pasted(&c, &new, &old), 0);
        assert_eq!(get(&c).0, items);
        // 1.x 폴더는 그대로
        assert_eq!(std::fs::read(old.join("hymnal").join("hymn.hdb")).unwrap(), b"old-hdb");
        assert_eq!(std::fs::read_dir(old.join("pasted")).unwrap().count(), 2);

        // 사용자가 1.x 폴더를 지움 → 복사본으로 연결
        std::fs::remove_dir_all(&old).unwrap();
        assert_eq!(repoint_pasted(&c, &new, &old), 1);
        let (now, at) = get(&c);
        assert_eq!(at, "t0");
        let v: Value = serde_json::from_str(&now).unwrap();
        assert_eq!(v[0]["file"], json!(new.join("pasted").join("clip-가.pptx").to_string_lossy()));
        assert_eq!(v[1]["file"], json!(new.join("pasted").join("clip-b.pptx").to_string_lossy()));
        assert_eq!(v[2]["file"], json!(gone)); // 복사본이 없는 것은 그대로
        assert_eq!(v[3]["file"], "D:\\찬송가\\pasted\\1.pptx"); // 다른 폴더는 그대로
        assert!(now.find("\"z\"").unwrap() < now.find("\"type\"").unwrap(), "키 순서 유지: {now}");
        assert!(Path::new(v[0]["file"].as_str().unwrap()).is_file());
        assert_eq!(repoint_pasted(&c, &new, &old), 0); // 다시 불러도 그대로
    }

    /// '묻기' 상태(ASK)는 프로세스에 하나뿐이라, 그것을 쓰는 시험끼리 겹치지 않게
    static ASK_LOCK: parking_lot::Mutex<()> = parking_lot::Mutex::new(());
    #[test]
    fn stamp_and_marker_flow() {
        let _ask = ASK_LOCK.lock();
        let (old, new) = (tmp("o"), tmp("n"));
        db_with(&old, &[("a", "2026-11-01")]);
        std::fs::write(old.join("settings.json"), "{}").unwrap();
        db_with(&new, &[("a", "2026-11-01")]);
        write_marker(&new, &old, "migrated");
        assert_eq!(recheck(&new, Some(&old)), Recheck::Nothing);
        // 1.x 에서 큐시트를 더 만들었고 이 판은 그대로 → 저절로 다시 가져오기
        db_with(&old, &[("b", "2026-11-14")]);
        assert!(matches!(recheck(&new, Some(&old)), Recheck::Reimport(_)));
        // 이 판에서도 고쳤으면 → 묻기
        db_with(&new, &[("c", "2026-11-15")]);
        let Recheck::Ask(v) = recheck(&new, Some(&old)) else { panic!("묻기여야 함") };
        assert_eq!(v["reason"], "both");
        assert_eq!(v["electronCues"], 2);
        // '그대로 쓰기' 를 고르면 다시 묻지 않음
        set_ask(Some(v));
        keep_current(&new);
        assert_eq!(recheck(&new, Some(&old)), Recheck::Nothing);
        let _ = std::fs::remove_dir_all(old);
        let _ = std::fs::remove_dir_all(new);
    }

    #[test]
    fn stale_rust_data_without_marker_asks() {
        // 이 PC 사례: 2.0.1 전에 만든 2.x 자료(표시 파일 없음)가 남아 있고, 1.x 에서 그 뒤에 더 썼다
        let (old, new) = (tmp("o2"), tmp("n2"));
        db_with(&new, &[("a", "2026-11-12")]);
        std::fs::write(new.join("settings.json"), "{}").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(30));
        db_with(&old, &[("a", "2026-11-12"), ("b", "2026-11-15")]);
        std::fs::write(old.join("settings.json"), "{}").unwrap();
        let Recheck::Ask(v) = recheck(&new, Some(&old)) else { panic!("묻기여야 함") };
        assert_eq!(v["reason"], "nomarker");
        assert_eq!((v["electronCues"].as_i64(), v["rustCues"].as_i64()), (Some(2), Some(1)));
        // 같은 내용이면 묻지 않고 표시만 남김
        let (old3, new3) = (tmp("o3"), tmp("n3"));
        db_with(&old3, &[("a", "x")]);
        db_with(&new3, &[("a", "x")]);
        assert_eq!(recheck(&new3, Some(&old3)), Recheck::Nothing);
        assert!(read_marker(&new3).is_some());
        for d in [old, new, old3, new3] {
            let _ = std::fs::remove_dir_all(d);
        }
    }

    #[test]
    fn stale_rust_data_opened_once_still_asks() {
        // 낡은 2.x 자료로 이미 한 번 켜서 파일 시각은 2.x 쪽이 더 새롭지만, 내용은 1.x 가 더 최근
        let (old, new) = (tmp("o4"), tmp("n4"));
        db_with(&old, &[("a", "2026-11-12"), ("b", "2026-11-15")]);
        std::fs::write(old.join("settings.json"), "{}").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(30));
        db_with(&new, &[("a", "2026-11-12")]);
        std::fs::write(new.join("settings.json"), "{}").unwrap();
        assert!(matches!(recheck(&new, Some(&old)), Recheck::Ask(_)));
        assert_eq!(last_edit("Cuesheets:2|2026-11-15|2026-11-01;Songs:-"), "2026-11-15");
        for d in [old, new] {
            let _ = std::fs::remove_dir_all(d);
        }
    }

    #[test]
    fn cleanup_removes_only_installers() {
        let d = tmp("cl");
        let cache = d.join("update-cache");
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(cache.join("abc-1-WorshipPresenter-2.0.1-Setup.exe"), vec![0u8; 1000]).unwrap();
        std::fs::write(cache.join("123.part"), b"x").unwrap();
        std::fs::write(cache.join("note.txt"), b"keep").unwrap();
        let el = d.join("worshipcue-updater");
        std::fs::create_dir_all(el.join("pending")).unwrap();
        std::fs::write(el.join("installer.exe"), vec![0u8; 5000]).unwrap();
        std::fs::write(el.join("current.blockmap"), b"b").unwrap();
        std::fs::write(el.join("pending").join("WorshipPresenter-2.0.1-Setup.exe"), b"s").unwrap();
        assert!(cleanup_installers(&cache, Some(&el)) >= 6000);
        assert!(cache.join("note.txt").is_file() && !cache.join("123.part").exists());
        assert!(!el.exists());
        // 이름이 다른 폴더·모르는 파일이 든 폴더는 손대지 않는다
        let other = d.join("something-else");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("installer.exe"), b"x").unwrap();
        let el2 = d.join("x").join("worshipcue-updater");
        std::fs::create_dir_all(&el2).unwrap();
        std::fs::write(el2.join("내 문서.hwp"), b"x").unwrap();
        cleanup_installers(&cache, Some(&other));
        cleanup_installers(&cache, Some(&el2));
        assert!(other.join("installer.exe").is_file() && el2.join("내 문서.hwp").is_file());
        let _ = std::fs::remove_dir_all(d);
    }

    // ---- 다시 가져오기 판단: 설정·성경 번역본만 고친 경우 ----
    /// 1.x 자료와 그것을 그대로 복사해 온 2.x 자료 (표시 파일 how=migrated)
    fn migrated_pair(tag: &str) -> (PathBuf, PathBuf) {
        let (old, new) = (tmp(&format!("{tag}o")), tmp(&format!("{tag}n")));
        db_with(&old, &[("a", "2026-11-01")]);
        std::fs::write(old.join("settings.json"), r#"{"port":37528,"output":{"autoOnSend":true},"templates":[{"id":"t1","name":"예배당"}]}"#).unwrap();
        assert!(crate::core::prepare_data_dir_from(&new, Some(&old), None).unwrap().is_some());
        assert_eq!(read_marker(&new).unwrap()["how"], "migrated");
        (old, new)
    }
    /// 이 판이 설정을 읽고 f 로 고쳐 저장한다 (실제와 같이 기본값이 채워지고 템플릿이 다듬어진 모양으로 저장됨)
    fn rust_saves(dir: &Path, f: impl FnOnce(&mut Value)) {
        let mut s = crate::settings::Settings::load(dir);
        s.update(f);
        s.save_now().unwrap();
    }
    fn add_cue(dir: &Path, id: &str, at: &str) {
        db_with(dir, &[(id, at)]);
    }
    fn cleanup(dirs: &[&PathBuf]) {
        for d in dirs {
            let _ = std::fs::remove_dir_all(d);
        }
    }
    #[test]
    fn only_source_changed_reimports_and_keeps_previous_data() {
        let (old, new) = migrated_pair("src");
        // 이 판에서는 쓰기만 했다: 저절로 바뀌는 칸(포트·검색 기록·통계·마지막 큐시트)만 바뀌고 파일은 이 판 모양으로 다시 써짐
        rust_saves(&new, |d| {
            d["port"] = json!(37529);
            d["history"] = json!([{ "at": 1, "title": "요 3:16" }]);
            d["telemetryDay"] = json!("2026-11-16");
            d["telemetryId"] = json!("abc");
            d["activeCueId"] = json!("a");
        });
        assert_eq!(recheck(&new, Some(&old)), Recheck::Nothing, "1.x 가 그대로면 아무 일도 없다");
        // 1.x 를 켜기만 해서 설정 파일 시각·포트만 바뀜 → 다시 가져오지도 묻지도 않는다
        std::fs::write(old.join("settings.json"), r#"{"port":40000,"history":[1],"output":{"autoOnSend":true},"templates":[{"id":"t1","name":"예배당"}]}"#).unwrap();
        assert_eq!(recheck(&new, Some(&old)), Recheck::Nothing);
        // 1.x 에서 큐시트를 더 만듦 → 묻지 않고 다시 가져오되, 전에 쓰던 자료는 pre-reimport 에 남는다
        add_cue(&old, "b", "2026-11-14");
        assert!(matches!(recheck(&new, Some(&old)), Recheck::Reimport(_)));
        let note = crate::core::prepare_data_dir_from(&new, Some(&old), None).unwrap().unwrap();
        assert!(note.contains(ASIDE_DIR), "{note}");
        assert_eq!(count_of(&content_stamp(&new.join("bible.db")), "Cuesheets"), 2);
        let kept: Vec<_> = std::fs::read_dir(new.join(ASIDE_DIR)).unwrap().flatten().map(|e| e.path()).collect();
        assert_eq!(kept.len(), 1);
        assert_eq!(count_of(&content_stamp(&kept[0].join("bible.db")), "Cuesheets"), 1);
        assert!(std::fs::read_to_string(kept[0].join("settings.json")).unwrap().contains("37529"));
        // 1.x 설정을 실제로 고침(템플릿 이름) + 이 판은 그대로 → 다시 가져오기
        assert_eq!(recheck(&new, Some(&old)), Recheck::Nothing);
        std::fs::write(old.join("settings.json"), r#"{"port":40000,"output":{"autoOnSend":true},"templates":[{"id":"t1","name":"본당"}]}"#).unwrap();
        assert!(matches!(recheck(&new, Some(&old)), Recheck::Reimport(_)));
        cleanup(&[&old, &new]);
    }
    #[test]
    fn rust_settings_only_change_asks_and_keeps_data() {
        let (old, new) = migrated_pair("set");
        // 이 판에서 설정만 고침 (큐시트·찬양은 그대로)
        rust_saves(&new, |d| d["output"]["autoOnSend"] = json!(false));
        assert_eq!(read_marker(&new).unwrap()["rust"], json!(content_stamp(&new.join("bible.db"))), "큐시트·찬양 지문은 그대로");
        let before = std::fs::read_to_string(new.join("settings.json")).unwrap();
        add_cue(&old, "b", "2026-11-14");
        // 고치기 전에는 여기서 Reimport 가 나와 방금 고친 설정이 1.x 것으로 되돌아갔다
        let Recheck::Ask(v) = recheck(&new, Some(&old)) else { panic!("설정만 고쳤어도 묻기여야 함") };
        assert_eq!(v["reason"], "both");
        // 켜는 흐름 전체: 묻기만 하고 자료는 그대로
        let _ask = ASK_LOCK.lock();
        assert!(crate::core::prepare_data_dir_from(&new, Some(&old), None).unwrap().is_none());
        assert_eq!(ask().unwrap()["reason"], "both");
        set_ask(None);
        assert_eq!(std::fs::read_to_string(new.join("settings.json")).unwrap(), before);
        assert_eq!(count_of(&content_stamp(&new.join("bible.db")), "Cuesheets"), 1);
        assert!(!new.join(ASIDE_DIR).exists());
        // 템플릿만 고친 경우도 묻는다
        let (old2, new2) = migrated_pair("tpl");
        rust_saves(&new2, |d| d["templates"][0]["name"] = json!("본당 자막"));
        add_cue(&old2, "b", "2026-11-14");
        assert!(matches!(recheck(&new2, Some(&old2)), Recheck::Ask(_)));
        // '지금 자료 그대로 쓰기' 뒤에는 다시 묻지 않고, 1.x 가 또 바뀌어도 말없이 바꾸지 않고 묻는다
        set_ask(Some(info_of("both", &old2, &new2, "", "")));
        keep_current(&new2);
        assert_eq!(recheck(&new2, Some(&old2)), Recheck::Nothing);
        add_cue(&old2, "c", "2026-11-15");
        assert!(matches!(recheck(&new2, Some(&old2)), Recheck::Ask(_)), "그대로 쓰기로 한 자료는 저절로 바꾸지 않는다");
        cleanup(&[&old, &new, &old2, &new2]);
    }
    #[test]
    fn rust_bible_version_change_asks() {
        let (old, new) = migrated_pair("bib");
        let c = rusqlite::Connection::open(new.join("bible.db")).unwrap();
        c.execute_batch("CREATE TABLE IF NOT EXISTS BibleVersions(id TEXT PRIMARY KEY, name TEXT, imported_at TEXT, verse_count INTEGER); CREATE TABLE IF NOT EXISTS Commentaries(id TEXT PRIMARY KEY, name TEXT, imported_at TEXT, entry_count INTEGER);").unwrap();
        drop(c);
        // 빈 표가 생긴 것(이 판이 열면서 만드는 표)은 바뀐 것으로 치지 않는다
        add_cue(&old, "b", "2026-11-14");
        assert!(matches!(recheck(&new, Some(&old)), Recheck::Reimport(_)));
        // 이 판에서 번역본을 가져옴 → 묻기
        let c = rusqlite::Connection::open(new.join("bible.db")).unwrap();
        c.execute("INSERT INTO BibleVersions VALUES('nkrv','개역개정','2026-11-16T10:00:00Z',31102)", []).unwrap();
        drop(c);
        assert!(matches!(recheck(&new, Some(&old)), Recheck::Ask(_)));
        cleanup(&[&old, &new]);
    }
    #[test]
    fn old_marker_without_settings_stamp_is_conservative() {
        // 2.0.x 가 쓴 표시 파일: 설정·성경 지문이 없다
        let old_marker = |old: &Path, new: &Path| {
            let m = json!({ "from": old.to_string_lossy(), "at": "2026-11-15T00:00:00Z", "how": "migrated", "electron": content_stamp(&old.join("bible.db")), "electronSettingsMtime": 1000, "rust": content_stamp(&new.join("bible.db")), "app": "2.0.1" });
            std::fs::write(new.join(MARKER), m.to_string()).unwrap();
        };
        // 1.x 설정 파일 시각만 새롭고 두 쪽 내용이 같다 → 묻지 않고 표시 파일만 새 모양으로
        let (old, new) = migrated_pair("om1");
        rust_saves(&new, |d| d["port"] = json!(37530));
        old_marker(&old, &new);
        assert_eq!(recheck(&new, Some(&old)), Recheck::Nothing);
        let m = read_marker(&new).unwrap();
        assert!(m["rustExtra"].is_string() && m["how"] == "migrated");
        add_cue(&old, "b", "2026-11-14");
        assert!(matches!(recheck(&new, Some(&old)), Recheck::Reimport(_)), "새 표시 파일이 된 뒤에는 전처럼 저절로");
        // 이 판 설정이 1.x 와 다르다(언제 고쳤는지 알 수 없음) → 묻는다
        let (old2, new2) = migrated_pair("om2");
        rust_saves(&new2, |d| d["output"]["autoOnSend"] = json!(false));
        old_marker(&old2, &new2);
        assert!(matches!(recheck(&new2, Some(&old2)), Recheck::Ask(_)));
        // 예전 표시 파일 + 1.x 큐시트가 바뀜: 이 판 설정을 고쳤는지 알 수 없으니 묻는다
        let (old3, new3) = migrated_pair("om3");
        old_marker(&old3, &new3);
        add_cue(&old3, "b", "2026-11-14");
        assert!(matches!(recheck(&new3, Some(&old3)), Recheck::Ask(_)));
        cleanup(&[&old, &new, &old2, &new2, &old3, &new3]);
    }
    #[test]
    fn settings_stamp_ignores_rewrite_and_volatile_keys() {
        let d = tmp("ss");
        assert_eq!(settings_stamp(&d), "-");
        // 1.x 가 쓴 모양 (기본값 빠짐, 키 순서 제각각, BOM)
        std::fs::write(d.join("settings.json"), "\u{feff}{\"templates\":[{\"name\":\"예배당\",\"id\":\"t1\"}],\"overlays\":[{\"id\":\"o1\",\"on\":true}],\"port\":1}").unwrap();
        let a = settings_stamp(&d);
        assert_eq!(a.len(), 24);
        // 이 판이 읽고 저절로 바뀌는 칸만 고쳐 다시 쓴다 → 같은 지문
        rust_saves(&d, |v| {
            for k in VOLATILE_SETTINGS {
                v[k] = json!("바뀜");
            }
            v["overlays"][0]["on"] = json!(false);
        });
        assert_eq!(settings_stamp(&d), a);
        rust_saves(&d, |_| {});
        assert_eq!(settings_stamp(&d), a, "다시 읽고 써도 그대로");
        // 사용자가 고치는 칸은 지문이 바뀐다
        rust_saves(&d, |v| v["shortcuts"]["quickText"] = json!("Ctrl+J"));
        assert_ne!(settings_stamp(&d), a);
        std::fs::write(d.join("settings.json"), "[1,2").unwrap();
        assert_eq!(settings_stamp(&d), "?");
        cleanup(&[&d]);
    }
    // ---- 1.x 파일 챙기기: 확인한 뒤 생긴 파일을 덮지 않기 ----
    #[test]
    fn copy_missing_does_not_clobber_file_created_during_copy() {
        let (old, new) = (tmp("rco"), tmp("rcn"));
        std::fs::create_dir_all(old.join("hymnal")).unwrap();
        std::fs::write(old.join("hymnal").join("hymn.hdb"), b"old-hdb").unwrap();
        std::fs::write(old.join("hymnal").join("hymn.cmp"), b"old-cmp").unwrap();
        // 고치기 전 방식(이름 바꾸기)은 윈도우에서 있던 파일을 말없이 덮는다는 것을 확인
        #[cfg(windows)]
        {
            let (a, b) = (new.join("a.part"), new.join("a"));
            std::fs::write(&a, b"from-old").unwrap();
            std::fs::write(&b, b"newer").unwrap();
            std::fs::rename(&a, &b).unwrap();
            assert_eq!(std::fs::read(&b).unwrap(), b"from-old", "rename 은 덮어쓴다");
            std::fs::remove_file(&b).unwrap();
        }
        // '없음'을 확인하고 복사하는 사이, 이 판에서 같은 이름의 찬송가를 새로 가져옴
        let mut done = (0, 0, 0);
        let mut hook = |to: &Path| {
            if to.file_name().is_some_and(|n| n == "hymn.hdb") {
                std::fs::write(to, b"newer-imported").unwrap();
            }
        };
        copy_missing_with(&old.join("hymnal"), &new.join("hymnal"), &mut done, &mut hook);
        assert_eq!(std::fs::read(new.join("hymnal").join("hymn.hdb")).unwrap(), b"newer-imported", "새로 생긴 파일을 덮지 않는다");
        assert_eq!(std::fs::read(new.join("hymnal").join("hymn.cmp")).unwrap(), b"old-cmp");
        assert_eq!(done, (1, 0, 7), "덮지 않고 둔 것은 복사·실패 어느 쪽으로도 세지 않는다");
        let names: Vec<String> = std::fs::read_dir(new.join("hymnal")).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
        assert!(!names.iter().any(|n| n.ends_with(".part")), "임시 파일이 남지 않는다: {names:?}");
        assert_eq!(std::fs::read(old.join("hymnal").join("hymn.hdb")).unwrap(), b"old-hdb", "1.x 폴더는 그대로");
        // 놓기 함수 자체: 대상이 있으면 false, 없으면 놓고 true, 임시 파일이 없으면 오류(대상은 안 생김)
        let (p, t) = (new.join("x.part"), new.join("x"));
        std::fs::write(&p, b"1").unwrap();
        assert!(place_no_clobber(&p, &t).unwrap());
        assert!(!p.exists() && std::fs::read(&t).unwrap() == b"1");
        std::fs::write(&p, b"2").unwrap();
        assert!(!place_no_clobber(&p, &t).unwrap());
        assert!(!p.exists() && std::fs::read(&t).unwrap() == b"1");
        assert!(place_no_clobber(&new.join("없음.part"), &new.join("y")).is_err());
        assert!(!new.join("y").exists(), "실패하면 반쪽 파일을 남기지 않는다");
        cleanup(&[&old, &new]);
    }
    // ---- 옛 설치 파일 정리와 업데이트 받기 ----
    #[test]
    fn guarded_cleanup_leaves_active_download_and_ready_installer() {
        let d = tmp("gc");
        let cache = d.join("update-cache");
        std::fs::create_dir_all(&cache).unwrap();
        let (part, ready, note) = (cache.join("123.part"), cache.join("abc-1-WorshipPresenter-2.0.2-Setup.exe"), cache.join("note.txt"));
        let fill = || {
            std::fs::write(&part, vec![0u8; 300]).unwrap();
            std::fs::write(&ready, vec![0u8; 700]).unwrap();
            std::fs::write(&note, b"keep").unwrap();
        };
        fill();
        let hour = std::time::Duration::from_secs(3600);
        let now = std::time::SystemTime::now();
        // 고치기 전 동작(지금의 무조건 정리): 받는 중인 파일·검증이 끝난 설치 파일을 모두 지운다
        assert_eq!(cleanup_installers(&cache, None), 1000);
        assert!(!part.exists() && !ready.exists() && note.is_file());
        // 1) 받는 중·설치 준비됨 → 오래된 파일이라도 손대지 않는다
        fill();
        assert_eq!(cleanup_installers_guarded(&cache, None, true, now + hour), 0);
        assert!(part.is_file() && ready.is_file());
        // 2) 상태를 본 직후 받기가 시작된 경우: 이번 실행(켜진 시각 뒤)에 생긴 파일은 남긴다
        assert_eq!(cleanup_installers_guarded(&cache, None, false, now - hour), 0);
        assert!(part.is_file() && ready.is_file());
        // 3) 옛 실행이 남긴 파일(켜지기 전에 생김)만 지운다. 다른 파일은 그대로
        assert_eq!(cleanup_installers_guarded(&cache, None, false, now + hour), 1000);
        assert!(!part.exists() && !ready.exists() && note.is_file());
        // 업데이트를 한 번도 안 받은 처음 상태는 '쓰는 중'이 아니다
        let _ = crate::updates::cache_in_use();
        cleanup(&[&d]);
    }
    #[test]
    fn request_file_forces_reimport_and_set_aside_keeps_files() {
        let d = tmp("r");
        db_with(&d, &[("a", "1")]);
        std::fs::write(d.join("settings.json"), "{\"k\":1}").unwrap();
        std::fs::write(d.join(REQUEST), "x").unwrap();
        assert!(matches!(recheck(&d, None), Recheck::Reimport(_)));
        assert!(!d.join(REQUEST).exists(), "요청은 한 번만");
        let kept = set_aside(&d).unwrap();
        assert!(kept.join("bible.db").is_file() && kept.join("settings.json").is_file());
        assert!(!d.join("bible.db").exists());
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn copy_db_with_progress() {
        let (a, b) = (tmp("ca"), tmp("cb"));
        let c = rusqlite::Connection::open(a.join("bible.db")).unwrap();
        c.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE t(x BLOB);").unwrap();
        for _ in 0..200 {
            c.execute("INSERT INTO t VALUES(zeroblob(8000))", []).unwrap();
        }
        // 진행률은 이 복사만의 값으로 본다 (전역 PROGRESS 는 병렬로 도는 다른 시험의 복사도 쓰므로 여기서 보면 흔들린다)
        let seen = std::sync::Mutex::new(Vec::<u32>::new());
        copy_db_with(&a.join("bible.db"), &b.join("x.db"), &|p| seen.lock().unwrap().push(p)).unwrap();
        let seen = seen.into_inner().unwrap();
        assert_eq!(seen.last(), Some(&1000), "끝나면 100%");
        assert!(seen.windows(2).all(|w| w[0] <= w[1]) && seen.iter().all(|p| (1..=1000).contains(p)), "진행률은 1..=1000 에서 줄지 않는다: {seen:?}");
        let n: i64 = rusqlite::Connection::open(b.join("x.db")).unwrap().query_row("SELECT count(*) FROM t", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 200);
        assert!(copy_db(&a.join("없음.db"), &b.join("y.db")).is_err());
        drop(c);
        let _ = std::fs::remove_dir_all(a);
        let _ = std::fs::remove_dir_all(b);
    }
}
