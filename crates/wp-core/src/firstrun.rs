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
            let e_changed = m["electron"].as_str() != Some(e_now.as_str())
                || mtime_ms(&old.join("settings.json")) > m["electronSettingsMtime"].as_i64().unwrap_or(0) + 2000;
            if !e_changed {
                return Recheck::Nothing;
            }
            if m["rust"].as_str() == Some(r_now.as_str()) {
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
pub fn copy_db(src: &Path, dst: &Path) -> rusqlite::Result<()> {
    let from = rusqlite::Connection::open_with_flags(src, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut to = rusqlite::Connection::open(dst)?;
    {
        let b = rusqlite::backup::Backup::new(&from, &mut to)?;
        loop {
            let r = b.step(4096)?;
            let p = b.progress();
            if p.pagecount > 0 {
                PROGRESS.store((((p.pagecount - p.remaining) as i64 * 1000) / p.pagecount as i64).clamp(1, 1000) as u32, Ordering::Relaxed);
            }
            match r {
                rusqlite::backup::StepResult::Done => break,
                rusqlite::backup::StepResult::More => {}
                _ => std::thread::sleep(std::time::Duration::from_millis(50)),
            }
        }
    }
    // 열어 보기 점검 (깨진 파일이면 여기서 실패)
    let ok: String = to.query_row("PRAGMA quick_check(1)", [], |r| r.get(0))?;
    drop(to);
    if ok != "ok" {
        return Err(rusqlite::Error::InvalidQuery);
    }
    Ok(())
}

// ---------------- 옛 설치 파일 정리 ----------------
/// 받아 둔 설치 파일을 지운다 (설치판에서만 부른다).
/// - update_cache: 이 판이 받아 둔 설치 파일(자료 폴더\update-cache). 켤 때는 받는 중인 것이 없으므로 모두 지운다
/// - electron_cache: Electron 판 자동 업데이트가 남긴 폴더(%LOCALAPPDATA%\worshipcue-updater, 옛 설치 파일 100MB 이상).
///   그 안에 설치 파일·블록맵·pending 말고 다른 것이 있으면 손대지 않는다
pub fn cleanup_installers(update_cache: &Path, electron_cache: Option<&Path>) -> u64 {
    let mut freed = 0u64;
    if let Ok(rd) = std::fs::read_dir(update_cache) {
        for e in rd.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if p.is_file() && (name.ends_with("-Setup.exe") || name.ends_with(".part")) {
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
    std::thread::Builder::new()
        .name("cleanup".into())
        .spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(20));
            cleanup_installers(&cache, dirs::data_local_dir().map(|d| d.join("worshipcue-updater")).as_deref());
        })
        .ok();
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

/// '지금 자료 그대로 쓰기': 지금 상태를 표시해 두어 1.x 자료가 또 바뀌기 전까지는 묻지 않는다
pub fn keep_current(data_dir: &Path) {
    if let Some(a) = ask().filter(|a| a["reason"] != "menu" && a["reason"] != "none") {
        if let Some(from) = a["from"].as_str() {
            write_marker(data_dir, Path::new(from), "kept");
        }
    }
    set_ask(None);
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
    std::process::Command::new(exe).args(args).spawn()?;
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
pub fn write_instance_pid(data_dir: &Path) {
    *CUR_DIR.lock() = Some(data_dir.to_path_buf());
    let _ = std::fs::write(data_dir.join("instance.pid"), std::process::id().to_string());
}

/// 먼저 켜진 워십프리젠터(같은 자료 폴더)의 창을 앞으로. 프로세스 번호로 찾으므로 같은 제목의 다른 프로그램(Electron 1.x 등)은 건드리지 않는다
pub fn focus_instance(data_dir: &Path) -> bool {
    let Some(pid) = std::fs::read_to_string(data_dir.join("instance.pid")).ok().and_then(|s| s.trim().parse::<u32>().ok()) else { return false };
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
        if h.is_null() {
            return false;
        }
        if IsIconic(h) != 0 {
            ShowWindow(h, SW_RESTORE);
        }
        SetForegroundWindow(h);
        true
    }
    #[cfg(not(windows))]
    {
        let _ = pid;
        false
    }
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
    fn stamp_and_marker_flow() {
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
        PROGRESS.store(0, Ordering::Relaxed);
        copy_db(&a.join("bible.db"), &b.join("x.db")).unwrap();
        assert_eq!(PROGRESS.load(Ordering::Relaxed), 1000);
        let n: i64 = rusqlite::Connection::open(b.join("x.db")).unwrap().query_row("SELECT count(*) FROM t", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 200);
        assert!(copy_db(&a.join("없음.db"), &b.join("y.db")).is_err());
        drop(c);
        let _ = std::fs::remove_dir_all(a);
        let _ = std::fs::remove_dir_all(b);
    }
}
