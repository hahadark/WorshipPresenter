//! GitHub 릴리스 확인과 명시적 다운로드·설치 요청. 릴리스 정지 중에는 확인만 허용한다.
use crate::core::{Ev, Shared};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};
pub const TELEMETRY_URL: &str = "https://worshipcue-usage.hahadark-media.workers.dev/ping";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// 출시 승인(2026-11-15, 사용자: 기존 저장소 정식판). 다시 막을 때만 true 로. 환경변수/설정/빌드 유형으로 바꿀 수 없다.
pub const RELEASE_HOLD: bool = false;
pub const HOLD_REASON: &str =
    "전체 기능 포팅 완료 전 릴리스 정지: 버전 확인만 가능하며 다운로드와 설치는 차단됩니다.";
/// Electron 판과 같은 저장소·같은 설치 파일 이름(WorshipPresenter-X.Y.Z-Setup.exe)을 쓴다
pub const TRUSTED_REPO: &str = "hahadark/WorshipPresenter";
fn hex(bytes: &[u8]) -> String { bytes.iter().map(|b|format!("{b:02x}")).collect() }
const MAX_ASSET: u64 = 512 * 1024 * 1024;
static BUSY: AtomicBool = AtomicBool::new(false);
static CANCEL: AtomicBool = AtomicBool::new(false);
static UPDATE: once_cell::sync::Lazy<parking_lot::Mutex<Value>> =
    once_cell::sync::Lazy::new(|| parking_lot::Mutex::new(json!({"state":"idle"})));
static ASSET: once_cell::sync::Lazy<parking_lot::Mutex<Option<Asset>>> =
    once_cell::sync::Lazy::new(|| parking_lot::Mutex::new(None));
static READY: once_cell::sync::Lazy<parking_lot::Mutex<Option<PathBuf>>> =
    once_cell::sync::Lazy::new(|| parking_lot::Mutex::new(None));
struct Busy;
impl Busy {
    fn acquire() -> Result<Self, String> {
        BUSY.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map(|_| Self)
            .map_err(|_| crate::i18n::tr("다른 업데이트 작업이 진행 중입니다.").into())
    }
}
impl Drop for Busy {
    fn drop(&mut self) {
        BUSY.store(false, Ordering::SeqCst);
    }
}
#[derive(Clone, Debug)]
struct Asset {
    url: String,
    name: String,
    size: u64,
    hash: String,
}
fn gate() -> Result<(), String> {
    if RELEASE_HOLD {
        Err(crate::i18n::tr(HOLD_REASON))
    } else {
        Ok(())
    }
}
fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .max_redirects(0)
        .timeout_global(Some(std::time::Duration::from_secs(30)))
        .build()
        .new_agent()
}
pub fn newer(a: &str, b: &str) -> bool {
    let p = |s: &str| -> Vec<u64> {
        s.trim()
            .trim_start_matches('v')
            .split(['.', '-'])
            .take(3)
            .map(|x| x.parse().unwrap_or(0))
            .collect()
    };
    let (x, y) = (p(a), p(b));
    for i in 0..3 {
        let (m, n) = (
            x.get(i).copied().unwrap_or(0),
            y.get(i).copied().unwrap_or(0),
        );
        if m != n {
            return m > n;
        }
    }
    false
}
pub fn state_from_release(r: &Value, current: &str, now: i64) -> Result<Value, String> {
    let tag = r["tag_name"]
        .as_str()
        .ok_or(crate::i18n::tr("릴리스 정보에 tag_name이 없습니다."))?;
    if r["draft"] == true || r["prerelease"] == true {
        return Err(crate::i18n::tr("정식 릴리스가 아닙니다.").into());
    }
    Ok(if newer(tag, current) {
        json!({"state":"available","version":tag.trim_start_matches('v'),"checkedAt":now,"notes":r["body"].as_str().unwrap_or("").chars().take(6000).collect::<String>(),"notesUrl":r["html_url"],"publishedAt":r["published_at"]})
    } else {
        json!({"state":"latest","checkedAt":now})
    })
}
fn safe_name(n: &str) -> bool {
    !n.is_empty()
        && n.len() < 160
        && n.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        && !n.contains("..")
        && !n.ends_with('.')
        && n.starts_with("WorshipPresenter-")
        && n.ends_with("-Setup.exe")
        && !n.contains("-English-")
        && !n.contains("Portable")
}
fn asset_from_release(r: &Value, repo: &str) -> Result<Asset, String> {
    if repo != TRUSTED_REPO {
        return Err(crate::i18n::tr("승인된 Rust 릴리스 저장소가 아닙니다.").into());
    }
    let tag = r["tag_name"]
        .as_str()
        .filter(|t| {
            !t.is_empty()
                && t.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b".-_".contains(&b))
        })
        .ok_or(crate::i18n::tr("잘못된 릴리스 태그"))?;
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        _ => return Err(crate::i18n::tr("지원하지 않는 설치 아키텍처").into()),
    };
    let list = r["assets"].as_array().ok_or(crate::i18n::tr("설치 자산 없음"))?;
    let mut found = None;
    for a in list {
        let n = a["name"].as_str().unwrap_or("");
        // 지금은 x64 설치 파일 하나만 낸다 (arm64 는 아직 없음)
        if !safe_name(n) || arch != "x64" {
            continue;
        }
        let u = a["browser_download_url"]
            .as_str()
            .ok_or(crate::i18n::tr("다운로드 주소 없음"))?;
        let parsed = url::Url::parse(u).map_err(|e| e.to_string())?;
        if parsed.scheme() != "https"
            || parsed.host_str() != Some("github.com")
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.port().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
            || parsed.path() != format!("/{repo}/releases/download/{tag}/{n}")
        {
            return Err(crate::i18n::tr("신뢰할 수 없는 릴리스 자산 주소").into());
        }
        let hash=a["digest"].as_str().and_then(|s|s.strip_prefix("sha256:")).filter(|s|s.len()==64 && s.bytes().all(|b|b.is_ascii_hexdigit())).ok_or(crate::i18n::tr("신뢰 가능한 SHA-256 해시가 없습니다. GitHub 릴리스에 digest를 제공한 뒤 다시 확인하세요."))?;
        let size = a["size"]
            .as_u64()
            .filter(|s| *s > 0 && *s <= MAX_ASSET)
            .ok_or(crate::i18n::tr("설치 파일 크기 제한 초과 또는 잘못된 크기"))?;
        if found.is_some() {
            return Err(crate::i18n::tr("설치 자산이 여러 개여서 안전하게 선택할 수 없습니다.").into());
        }
        found = Some(Asset {
            url: u.into(),
            name: n.into(),
            size,
            hash: hash.to_ascii_lowercase(),
        });
    }
    found.ok_or(crate::i18n::tr("이 아키텍처용 Windows 설치 자산이 없습니다.").into())
}
/// 설치판인가: 실행 파일 옆에 제거 프로그램(설치 파일이 씀)이 있으면 설치판.
/// 포터블 zip·개발 실행에서 설치 파일을 돌리면 포터블 폴더는 그대로 두고 %LOCALAPPDATA% 에 따로 설치·바로가기를 만들어 버린다 → 받기·설치를 막는다
pub fn installed_copy() -> bool {
    std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("Uninstall WorshipPresenter.exe").is_file())).unwrap_or(false)
}
pub const PORTABLE_NOTE: &str = "설치 없이 쓰는 판(포터블)은 스스로 바꾸지 않습니다. 릴리스 페이지에서 새 Portable.zip 을 받거나 설치 파일로 설치하세요.";
pub fn status(settings: &Value) -> Value {
    let mut u = UPDATE.lock().clone();
    u["current"] = json!(VERSION);
    u["releaseHold"] = json!(RELEASE_HOLD);
    u["holdReason"] = json!(crate::i18n::tr(HOLD_REASON));
    let installed = installed_copy();
    u["portable"] = json!(!installed);
    u["canDownload"] = json!(
        !RELEASE_HOLD && cfg!(windows) && installed && ASSET.lock().is_some() && !BUSY.load(Ordering::SeqCst)
    );
    u["canInstall"] = json!(
        !RELEASE_HOLD && cfg!(windows) && installed && READY.lock().is_some() && !BUSY.load(Ordering::SeqCst)
    );
    u["skipped"] = json!(
        settings["skipVersion"].as_str().filter(|s| !s.is_empty()) == u["version"].as_str()
            && u["version"].is_string()
    );
    u["repo"] = settings["updateRepo"].clone();
    u
}
pub fn api_url(repo: &str) -> Option<String> {
    let r = repo.trim();
    let parts: Vec<_> = r.split('/').collect();
    if parts.len() != 2
        || parts.iter().any(|p| {
            p.is_empty()
                || *p == "."
                || *p == ".."
                || !p
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        })
    {
        return None;
    }
    Some(format!("https://api.github.com/repos/{r}/releases/latest"))
}
fn get_json(url: &str) -> Result<Value, String> {
    let mut r = agent()
        .get(url)
        .header("User-Agent", "WorshipPresenter-rs")
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| crate::i18n::tr_args("새 버전 확인 실패: {1}", &[&e]))?;
    let mut bytes = Vec::new();
    r.body_mut()
        .as_reader()
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 1024 * 1024 {
        return Err(crate::i18n::tr("릴리스 응답이 너무 큽니다.").into());
    }
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}
pub fn check(core: &Shared, manual: bool) {
    let repo = core.lock().settings.get()["updateRepo"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(TRUSTED_REPO)
        .to_string();
    let Some(url) = api_url(&repo) else {
        if manual {
            core.lock().notify(
                crate::i18n::tr("올바른 GitHub owner/repo를 입력하세요."),
                "warn",
            );
        }
        return;
    };
    let Ok(guard) = Busy::acquire() else {
        return;
    };
    *UPDATE.lock() = json!({"state":"checking"});
    *ASSET.lock() = None;
    *READY.lock() = None;
    let sh = core.clone();
    std::thread::spawn(move || {
        let _guard = guard;
        let result = get_json(&url).and_then(|r| {
            let mut s = state_from_release(&r, VERSION, crate::util::now_ms())?;
            if s["state"] == "available" {
                match asset_from_release(&r, &repo) {
                    Ok(a) => *ASSET.lock() = Some(a),
                    Err(e) => s["downloadError"] = json!(e),
                }
            }
            Ok(s)
        });
        let s = result.unwrap_or_else(|e| json!({"state":"error","error":e}));
        *UPDATE.lock() = s.clone();
        let mut c = sh.lock();
        let skipped = c.settings.get()["skipVersion"].as_str() == s["version"].as_str();
        if s["state"] == "available" && (!skipped || manual) {
            c.notify(
                crate::i18n::tr_args(
                    "새 버전 {1}이 나왔습니다 (도움말 → 새 버전)",
                    &[&s["version"].as_str().unwrap_or("")]
                ),
                "info",
            );
        } else if manual {
            c.notify(
                if s["state"] == "latest" {
                    crate::i18n::tr("지금이 최신 버전입니다.").to_string()
                } else {
                    s["error"].as_str().map(str::to_string).unwrap_or_else(|| crate::i18n::tr("확인 실패"))
                },
                "info",
            );
        }
        c.emit(Ev::Ui);
    });
}
/// 리다이렉트는 HTTPS GitHub 자산 CDN으로만, 최대 3회. 자동 리다이렉트는 비활성화.
fn response(url: &str) -> Result<ureq::http::Response<ureq::Body>, String> {
    let mut u = url.to_string();
    for _ in 0..4 {
        let r = agent()
            .get(&u)
            .header("User-Agent", "WorshipPresenter-rs")
            .header("Accept-Encoding", "identity")
            .call()
            .map_err(|e| e.to_string())?;
        if r.status().is_redirection() {
            let next = r
                .headers()
                .get("location")
                .and_then(|h| h.to_str().ok())
                .ok_or(crate::i18n::tr("리다이렉트 주소 없음"))?;
            let v = url::Url::parse(next).map_err(|e| e.to_string())?;
            if v.scheme() != "https"
                || !matches!(
                    v.host_str(),
                    Some("release-assets.githubusercontent.com" | "objects.githubusercontent.com")
                )
                || !v.username().is_empty()
                || v.password().is_some()
                || v.port().is_some()
            {
                return Err(crate::i18n::tr("신뢰할 수 없는 자산 리다이렉트").into());
            }
            u = next.into();
            continue;
        }
        return Ok(r);
    }
    Err(crate::i18n::tr("리다이렉트 횟수 초과").into())
}
/// 임시 파일은 create_new, 성공 시에만 같은 디렉터리로 원자적 rename. 실패·취소 때 정리.
fn cache_stream(
    reader: &mut dyn Read,
    a: &Asset,
    dir: &Path,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64, bool),
) -> Result<PathBuf, String> {
    if !safe_name(&a.name) || a.size == 0 || a.size > MAX_ASSET {
        return Err(crate::i18n::tr("잘못된 파일명 또는 크기").into());
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    if std::fs::symlink_metadata(dir)
        .map_err(|e| e.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err(crate::i18n::tr("캐시 디렉터리는 링크일 수 없습니다.").into());
    }
    let part = dir.join(format!("{}.part", rand::random::<u64>()));
    let result = (|| {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&part)
            .map_err(|e| e.to_string())?;
        let mut hash = Sha256::new();
        let mut total = 0u64;
        let mut buf = [0u8; 64 * 1024];
        loop {
            if cancel.load(Ordering::SeqCst) {
                return Err(crate::i18n::tr("다운로드 취소됨").into());
            }
            let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            total += n as u64;
            if total > a.size || total > MAX_ASSET {
                return Err(crate::i18n::tr("설치 파일 크기 제한 초과").into());
            }
            f.write_all(&buf[..n]).map_err(|e| e.to_string())?;
            hash.update(&buf[..n]);
            progress(total, false);
        }
        progress(total, true);
        if total != a.size {
            return Err(crate::i18n::tr("설치 파일이 불완전합니다.").into());
        }
        if hex(&hash.finalize()) != a.hash {
            return Err(crate::i18n::tr("SHA-256 검증 실패: 설치 파일을 폐기했습니다.").into());
        }
        if cancel.load(Ordering::SeqCst) {
            return Err(crate::i18n::tr("다운로드 취소됨").into());
        }
        f.sync_all().map_err(|e| e.to_string())?;
        drop(f);
        let target = dir.join(format!(
            "{}-{}-{}",
            a.hash.get(..16).unwrap_or(&a.hash), // latest.yml 의 해시가 짧거나 깨져 있어도 panic 하지 않게
            rand::random::<u64>(),
            a.name
        ));
        std::fs::rename(&part, &target).map_err(|e| e.to_string())?;
        Ok(target)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(part);
    }
    result
}
pub fn download(core: &Shared) -> Result<(), String> {
    gate()?;
    if !cfg!(windows) {
        return Err(crate::i18n::tr("Windows 설치판에서만 지원합니다.").into());
    }
    if !installed_copy() {
        return Err(crate::i18n::tr(PORTABLE_NOTE).into());
    }
    let guard = Busy::acquire()?;
    let a = ASSET
        .lock()
        .clone()
        .ok_or(crate::i18n::tr("검증 가능한 새 버전을 먼저 확인하세요."))?;
    if core.lock().settings.get()["skipVersion"].as_str() == UPDATE.lock()["version"].as_str() {
        return Err(crate::i18n::tr("건너뛴 버전입니다.").into());
    }
    let dir = core.lock().data_dir.join("update-cache");
    CANCEL.store(false, Ordering::SeqCst);
    *READY.lock() = None;
    UPDATE.lock()["state"] = json!("downloading");
    let sh = core.clone();
    std::thread::spawn(move || {
        let _guard = guard;
        let result = response(&a.url).and_then(|mut r| {
            cache_stream(
                &mut r.body_mut().as_reader(),
                &a,
                &dir,
                &CANCEL,
                |n, verify| {
                    let mut s = UPDATE.lock();
                    s["state"] = json!(if verify { "verifying" } else { "downloading" });
                    s["received"] = json!(n);
                    s["total"] = json!(a.size);
                    s["percent"] = json!(n * 100 / a.size);
                },
            )
        });
        let mut s = UPDATE.lock();
        match result {
            Ok(p) => {
                *READY.lock() = Some(p);
                s["state"] = json!("ready");
                s["error"] = Value::Null;
            }
            Err(e) => {
                s["state"] = json!(if CANCEL.load(Ordering::SeqCst) {
                    "cancelled"
                } else {
                    "error"
                });
                s["error"] = json!(e);
            }
        }
        drop(s);
        sh.lock().emit(Ev::Ui);
    });
    Ok(())
}
pub fn cancel() {
    CANCEL.store(true, Ordering::SeqCst);
}
/// 자동 종료 설치는 하지 않는다. UI의 명시 요청 때만, 디스크 파일을 재검증한 후 실행.
pub fn install_now(core: &Shared) -> Result<(), String> {
    gate()?;
    let _guard = Busy::acquire()?;
    if !installed_copy() {
        return Err(crate::i18n::tr(PORTABLE_NOTE).into());
    }
    if !cfg!(windows) || UPDATE.lock()["state"] != "ready" {
        return Err(crate::i18n::tr("설치 준비 상태가 아닙니다.").into());
    }
    if core.lock().settings.get()["skipVersion"].as_str() == UPDATE.lock()["version"].as_str() {
        return Err(crate::i18n::tr("건너뛴 버전은 설치하지 않습니다.").into());
    }
    let a = ASSET.lock().clone().ok_or(crate::i18n::tr("설치 메타데이터 없음"))?;
    let p = READY.lock().clone().ok_or(crate::i18n::tr("준비된 설치 파일 없음"))?;
    let m = std::fs::symlink_metadata(&p).map_err(|e| e.to_string())?;
    if !m.is_file() || m.len() != a.size {
        return Err(crate::i18n::tr("설치 파일 크기 또는 종류가 바뀌었습니다.").into());
    }
    let mut f = std::fs::File::open(&p).map_err(|e| e.to_string())?;
    let mut h = Sha256::new();
    let mut buf = [0u8; 65536];
    let mut count = 0u64;
    loop {
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        count += n as u64;
        if count > a.size {
            return Err(crate::i18n::tr("설치 파일 크기가 바뀌었습니다.").into());
        }
        h.update(&buf[..n]);
    }
    if hex(&h.finalize()) != a.hash {
        return Err(crate::i18n::tr("설치 직전 SHA-256 재검증 실패").into());
    }
    // 쉘 없이 인자 분리. 실제 릴리스 승인 전 gate가 이 경로 진입을 차단한다.
    #[cfg(windows)]
    std::process::Command::new(&p)
        .args(["/S", "--updated", "--force-run"])
        .spawn()
        .map_err(|e| crate::i18n::tr_args("설치 실행 실패: {1}", &[&e]))?;
    UPDATE.lock()["state"] = json!("installing");
    core.lock().emit(Ev::Ui);
    Ok(())
}
pub fn skip(core: &Shared) {
    let v = UPDATE.lock()["version"].as_str().unwrap_or("").to_string();
    cancel();
    let mut c = core.lock();
    c.settings.update(|d| d["skipVersion"] = json!(v));
    c.emit(Ev::Ui);
}
pub fn unskip(core: &Shared) {
    let mut c = core.lock();
    c.settings.update(|d| d["skipVersion"] = json!(""));
    c.emit(Ev::Ui);
}

// ---------------- 익명 사용 통계 ----------------
pub fn os_tag() -> String {
    let p = match std::env::consts::OS { "windows" => "win32", "macos" => "darwin", o => o };
    let a = match std::env::consts::ARCH { "x86_64" => "x64", "aarch64" => "arm64", o => o };
    format!("{p}-{a}")
}

/// 보낼 내용 (Electron 과 같은 칸). 이름·교회·성경·큐시트 내용은 넣지 않는다
pub fn ping_body(settings: &Value, version: &str) -> Value {
    let kind = if cfg!(target_os = "macos") { "mac" } else if std::env::var_os("PORTABLE_EXECUTABLE_DIR").is_some() || (cfg!(windows) && !installed_copy()) { "portable" } else if cfg!(windows) { "installer" } else { std::env::consts::OS };
    let lang = settings["uiLang"].as_str().filter(|x| !x.is_empty()).unwrap_or("ko");
    json!({ "id": settings["telemetryId"], "v": version, "lang": lang, "os": os_tag(), "build": if lang == "en" { "en" } else { "ko" }, "kind": kind })
}

/// 오늘 보내야 하는가
pub fn due(settings: &Value, today: &str) -> bool {
    settings["telemetry"].as_bool() != Some(false) && settings["telemetryDay"].as_str() != Some(today)
}

/// 한 번 보내기. 성공하면 telemetryDay 를 오늘로
pub fn send_once(core: &Shared, url: &str) -> Result<bool, String> {
    let today = crate::util::iso_now()[..10].to_string();
    let body = {
        let mut c = core.lock();
        if c.settings.get()["telemetryId"].as_str().is_none_or(|x| x.is_empty()) {
            let id: String = (0..12).map(|_| format!("{:02x}", rand::random::<u8>())).collect();
            c.settings.update(|d| d["telemetryId"] = json!(id));
        }
        if !due(c.settings.get(), &today) {
            return Ok(false);
        }
        ping_body(c.settings.get(), VERSION)
    };
    let agent = ureq::Agent::config_builder().timeout_global(Some(std::time::Duration::from_secs(15))).build().new_agent();
    let r = agent.post(url).send_json(&body).map_err(|e| e.to_string())?;
    if r.status().is_success() {
        core.lock().settings.update(|d| d["telemetryDay"] = json!(today));
        return Ok(true);
    }
    Err(crate::i18n::tr_args("상태 {1}", &[&(r.status())]))
}

/// 켜고 1분 뒤, 그다음 3시간마다 (날이 바뀌었으면) 보낸다
pub fn start_telemetry(core: Shared) {
    if cfg!(debug_assertions) && std::env::var_os("WC_TELEMETRY_DEV").is_none() {
        return;
    }
    let url = std::env::var("WC_TELEMETRY_URL").unwrap_or_else(|_| TELEMETRY_URL.to_string());
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(60));
        loop {
            let _ = send_once(&core, &url); // 인터넷이 없으면 다음에
            std::thread::sleep(std::time::Duration::from_secs(3 * 60 * 60));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    #[test]
    fn versions() {
        assert!(newer("v1.2.10", "1.2.9"));
        assert!(!newer("1.2.9", "1.2.9"));
        assert!(!newer("v0.1.0", "0.2.0"));
        let s = state_from_release(&json!({ "tag_name": "v9.0.0", "body": "고친 것", "html_url": "u" }), "0.1.0", 1).unwrap();
        assert_eq!(s["state"], "available");
        assert_eq!(s["version"], "9.0.0");
        assert_eq!(state_from_release(&json!({ "tag_name": "v0.0.1" }), "0.1.0", 1).unwrap()["state"], "latest");
        assert_eq!(api_url("hahadark/WorshipPresenter").unwrap(), "https://api.github.com/repos/hahadark/WorshipPresenter/releases/latest");
        assert!(api_url("").is_none());
    }

    #[test]
    fn telemetry_to_fake_server() {
        // 가짜 통계 서버 (실제 주소로는 보내지 않는다)
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let got = std::sync::Arc::new(parking_lot::Mutex::new(String::new()));
        let g = got.clone();
        std::thread::spawn(move || {
            if let Ok((mut s, _)) = l.accept() {
                s.set_read_timeout(Some(std::time::Duration::from_secs(3))).ok();
                let mut all = vec![];
                let mut buf = vec![0u8; 8192];
                while !String::from_utf8_lossy(&all).contains('}') {
                    let n = s.read(&mut buf).unwrap_or(0);
                    if n == 0 { break; }
                    all.extend_from_slice(&buf[..n]);
                }
                *g.lock() = String::from_utf8_lossy(&all).to_string();
                let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
            }
        });
        let dir = std::env::temp_dir().join(format!("wp-tel-{}", crate::util::now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        drop(rusqlite::Connection::open(dir.join("bible.db")).unwrap());
        let core: Shared = std::sync::Arc::new(parking_lot::Mutex::new(crate::core::Core::open(dir.clone(), dir.join("web"), None).unwrap()));
        let url = format!("http://127.0.0.1:{port}/ping");
        assert_eq!(send_once(&core, &url), Ok(true));
        let req = got.lock().clone();
        assert!(req.starts_with("POST /ping"));
        let body: Value = serde_json::from_str(req.split("\r\n\r\n").nth(1).unwrap_or("")).unwrap();
        assert_eq!(body["os"], os_tag());
        assert_eq!(body["v"], VERSION);
        assert_eq!(body["id"].as_str().unwrap().len(), 24);
        assert!(!req.contains("bible"));
        // 같은 날은 다시 안 보냄 · 끄면 안 보냄
        assert_eq!(send_once(&core, &url), Ok(false));
        assert!(!due(&json!({ "telemetry": false }), "2026-11-12"));
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[cfg(test)]
mod updater_security_tests {
    use super::*;
    fn asset(bytes: &[u8]) -> Asset {
        Asset {
            url: String::new(),
            name: "WorshipPresenter-1.0.0-Setup.exe".into(),
            size: bytes.len() as u64,
            hash: hex(&Sha256::digest(bytes)),
        }
    }
    fn dir() -> PathBuf {
        std::env::temp_dir().join(format!("wp-update-test-{}", rand::random::<u64>()))
    }
    #[test]
    fn release_hold_lifted_for_official_release() {
        assert!(!RELEASE_HOLD);
        assert!(gate().is_ok());
        // 영어판·포터블·다른 이름은 고르지 않는다
        assert!(safe_name("WorshipPresenter-2.0.0-Setup.exe"));
        assert!(!safe_name("WorshipPresenter-2.0.0-English-Setup.exe"));
        assert!(!safe_name("WorshipPresenter-2.0.0-Portable.exe"));
    }
    #[test]
    fn untrusted_metadata_rejected() {
        assert!(api_url("http://127.0.0.1:9").is_none());
        assert!(api_url("evil/repo?x").is_none());
        let a = asset(b"fixture");
        let mut r = json!({"tag_name":"v1.0.0","assets":[{"name":a.name,"size":7,"digest":format!("sha256:{}",a.hash),"browser_download_url":format!("https://github.com/{}/releases/download/v1.0.0/{}",TRUSTED_REPO,a.name)}]});
        assert!(asset_from_release(&r, TRUSTED_REPO).is_ok());
        assert!(asset_from_release(&r, "evil/repo").is_err());
        r["assets"][0]["digest"] = Value::Null;
        assert!(
            asset_from_release(&r, TRUSTED_REPO)
                .unwrap_err()
                .contains("SHA-256")
        );
        r["assets"][0]["name"] = json!("../bad.exe");
        assert!(asset_from_release(&r, TRUSTED_REPO).is_err());
    }
    #[test]
    fn atomic_cache_hash_size_cancel_cleanup() {
        let d = dir();
        let bytes = b"local fixture only";
        let a = asset(bytes);
        let cancel = AtomicBool::new(false);
        let mut events = Vec::new();
        let p = cache_stream(&mut &bytes[..], &a, &d, &cancel, |n, v| events.push((n, v))).unwrap();
        assert_eq!(std::fs::read(p).unwrap(), bytes);
        assert!(events.last().unwrap().1);
        let baseline = std::fs::read_dir(&d).unwrap().count();
        let mut bad = a.clone();
        bad.hash = "0".repeat(64);
        assert!(
            cache_stream(&mut &bytes[..], &bad, &d, &cancel, |_, _| {})
                .unwrap_err()
                .contains("SHA-256")
        );
        bad = a.clone();
        bad.size -= 1;
        assert!(cache_stream(&mut &bytes[..], &bad, &d, &cancel, |_, _| {}).is_err());
        assert!(cache_stream(&mut &bytes[..3], &a, &d, &cancel, |_, _| {}).is_err());
        cancel.store(true, Ordering::SeqCst);
        assert!(
            cache_stream(&mut &bytes[..], &a, &d, &cancel, |_, _| {})
                .unwrap_err()
                .contains("취소")
        );
        assert_eq!(std::fs::read_dir(&d).unwrap().count(), baseline);
        std::fs::remove_dir_all(d).unwrap();
    }
    #[test]
    fn concurrent_work_rejected() {
        let _guard = Busy::acquire().unwrap();
        assert!(Busy::acquire().is_err());
    }
    #[test]
    fn fake_server_stream_only() {
        let bytes = b"fake local installer bytes, never executed";
        let a = asset(bytes);
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            s.set_read_timeout(Some(std::time::Duration::from_secs(3)))
                .unwrap();
            let mut req = [0; 2048];
            let _ = s.read(&mut req);
            write!(
                s,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                bytes.len()
            )
            .unwrap();
            s.write_all(bytes).unwrap();
        });
        let mut r = agent()
            .get(format!("http://{addr}/fixture"))
            .call()
            .unwrap();
        let d = dir();
        let p = cache_stream(
            &mut r.body_mut().as_reader(),
            &a,
            &d,
            &AtomicBool::new(false),
            |_, _| {},
        )
        .unwrap();
        assert_eq!(std::fs::read(p).unwrap(), bytes);
        server.join().unwrap();
        std::fs::remove_dir_all(d).unwrap();
    }
}

#[cfg(test)]
mod transfer_edge_tests {
 use super::*;
 #[test] fn cancellation_during_stream_removes_partial(){
  let bytes=vec![42u8;150000];let a=Asset{url:String::new(),name:"WorshipPresenter-9.9.9-Setup.exe".into(),size:bytes.len() as u64,hash:hex(&Sha256::digest(&bytes))};
  let d=std::env::temp_dir().join(format!("wp-cancel-{}",rand::random::<u64>()));let cancel=AtomicBool::new(false);
  assert!(cache_stream(&mut &bytes[..],&a,&d,&cancel,|_,_|cancel.store(true,Ordering::SeqCst)).is_err());
  assert_eq!(std::fs::read_dir(&d).unwrap().count(),0);std::fs::remove_dir_all(d).unwrap();
 }
 #[test] fn unsafe_redirect_never_followed(){
  let l=std::net::TcpListener::bind("127.0.0.1:0").unwrap();let addr=l.local_addr().unwrap();
  let t=std::thread::spawn(move||{let(mut s,_)=l.accept().unwrap();s.set_read_timeout(Some(std::time::Duration::from_secs(3))).unwrap();let mut b=[0;2048];let _=s.read(&mut b);s.write_all(b"HTTP/1.1 302 Found\r\nLocation: http://example.invalid/installer.exe\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();});
  assert!(response(&format!("http://{addr}/fixture")).unwrap_err().contains("신뢰"));t.join().unwrap();
 }
}

#[cfg(test)]
mod real_release_tests {
    use super::*;

    /// 실제 GitHub 릴리스로 끝까지: 최신 릴리스 읽기 → 설치 자산 고르기(이름·주소·digest) → 리다이렉트 따라 받기 → 크기·SHA-256 검증.
    /// 인터넷을 쓰므로 평소에는 돌리지 않는다: cargo test -p wp-core --offline real_release -- --ignored --nocapture
    /// WP_UPDATE_TEST_OUT 이 있으면 받은 설치 파일을 그 폴더에 남긴다 (/TEST 설치 점검용)
    #[test]
    #[ignore]
    fn real_release_download_and_verify() {
        let r = get_json(&api_url(TRUSTED_REPO).unwrap()).expect("릴리스 읽기");
        println!("최신 릴리스: {} 자산 {}개", r["tag_name"], r["assets"].as_array().map(|a| a.len()).unwrap_or(0));
        let st = state_from_release(&r, "1.14.1", crate::util::now_ms()).expect("상태");
        println!("1.14.1 에서 본 상태: {st}");
        let a = asset_from_release(&r, TRUSTED_REPO).expect("설치 자산");
        println!("자산: {} {}바이트 sha256 {}", a.name, a.size, a.hash);
        let keep = std::env::var_os("WP_UPDATE_TEST_OUT").map(PathBuf::from);
        let dir = keep.clone().unwrap_or_else(|| std::env::temp_dir().join(format!("wp-real-{}", rand::random::<u64>())));
        let cancel = AtomicBool::new(false);
        let mut resp = response(&a.url).expect("받기 시작");
        let p = cache_stream(&mut resp.body_mut().as_reader(), &a, &dir, &cancel, |_, _| {}).expect("받기·검증");
        println!("받음: {}", p.display());
        assert_eq!(std::fs::metadata(&p).unwrap().len(), a.size);
        if keep.is_none() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}
