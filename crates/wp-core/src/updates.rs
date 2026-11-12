//! [트랙 D] 새 버전 확인 · 익명 사용 통계 (Electron 판 updater.js · telemetry.js)
//! - 새 버전: GitHub 릴리스 API 의 latest 를 읽어 지금 버전과 비교만 한다. 내려받기·설치는 하지 않는다
//!   (Rust 판은 아직 릴리스가 없어서 저장소 주소는 설정 updateRepo 로, 비어 있으면 확인하지 않음).
//! - 사용 통계: 하루 한 번 { id, v, lang, os, build, kind } 를 보낸다. 설정 telemetry=false 면 안 보냄.
//!   개발 실행(디버그 빌드)에서는 보내지 않는다 (WC_TELEMETRY_DEV 가 있으면 보냄).

use crate::core::{Ev, Shared};
use serde_json::{json, Value};

pub const TELEMETRY_URL: &str = "https://worshipcue-usage.hahadark-media.workers.dev/ping";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// "v1.2.10" > "1.2.9" ?
pub fn newer(a: &str, b: &str) -> bool {
    let p = |s: &str| -> Vec<u64> { s.trim().trim_start_matches('v').split(['.', '-']).take(3).map(|x| x.parse().unwrap_or(0)).collect() };
    let (x, y) = (p(a), p(b));
    for i in 0..3 {
        let (m, n) = (x.get(i).copied().unwrap_or(0), y.get(i).copied().unwrap_or(0));
        if m != n {
            return m > n;
        }
    }
    false
}

/// 릴리스 JSON → 상태 ({ state: available|latest, version, notes, notesUrl, publishedAt, checkedAt })
pub fn state_from_release(r: &Value, current: &str, now: i64) -> Result<Value, String> {
    let tag = r["tag_name"].as_str().ok_or("릴리스 정보에 tag_name 이 없습니다.")?;
    if newer(tag, current) {
        Ok(json!({ "state": "available", "version": tag.trim_start_matches('v'), "checkedAt": now, "notes": r["body"].as_str().unwrap_or("").chars().take(6000).collect::<String>(), "notesUrl": r["html_url"], "publishedAt": r["published_at"] }))
    } else {
        Ok(json!({ "state": "latest", "checkedAt": now }))
    }
}

static UPDATE: once_cell::sync::Lazy<parking_lot::Mutex<Value>> = once_cell::sync::Lazy::new(|| parking_lot::Mutex::new(json!({ "state": "idle", "current": VERSION })));

/// 지금 업데이트 상태 (+ 건너뛰기 표시)
pub fn status(settings: &Value) -> Value {
    let mut u = UPDATE.lock().clone();
    u["current"] = json!(VERSION);
    let skip = settings["skipVersion"].as_str().unwrap_or("");
    u["skipped"] = json!(!skip.is_empty() && u["version"].as_str() == Some(skip));
    u["repo"] = json!(settings["updateRepo"].as_str().unwrap_or(""));
    u
}

/// api 주소: 설정 updateRepo 가 "owner/name" 이면 GitHub, "http…" 면 그 주소 그대로 (시험용 가짜 서버)
pub fn api_url(repo: &str) -> Option<String> {
    let r = repo.trim();
    if r.is_empty() {
        return None;
    }
    if r.starts_with("http://") || r.starts_with("https://") {
        return Some(r.to_string());
    }
    Some(format!("https://api.github.com/repos/{r}/releases/latest"))
}

fn get_json(url: &str) -> Result<Value, String> {
    let mut r = ureq::get(url).header("User-Agent", "WorshipPresenter-rs").header("Accept", "application/vnd.github+json").call().map_err(|e| format!("새 버전 확인 실패: {e}"))?;
    r.body_mut().read_json().map_err(|e| e.to_string())
}

/// 새 버전 확인 (백그라운드). manual 이면 결과를 알림으로
pub fn check(core: &Shared, manual: bool) {
    let repo = core.lock().settings.get()["updateRepo"].as_str().unwrap_or("").to_string();
    let Some(url) = api_url(&repo) else {
        if manual {
            core.lock().notify("새 버전 확인 주소(저장소)가 정해지지 않았습니다. Rust 판은 아직 릴리스 전입니다.", "warn");
        }
        return;
    };
    *UPDATE.lock() = json!({ "state": "checking" });
    let sh = core.clone();
    std::thread::spawn(move || {
        let r = get_json(&url).and_then(|j| state_from_release(&j, VERSION, crate::util::now_ms()));
        let v = r.unwrap_or_else(|e| json!({ "state": "error", "error": e, "checkedAt": crate::util::now_ms() }));
        *UPDATE.lock() = v.clone();
        let mut c = sh.lock();
        let skipped = c.settings.get()["skipVersion"].as_str() == v["version"].as_str();
        if v["state"] == "available" && (!skipped || manual) {
            c.notify(format!("새 버전 {} 이 나왔습니다 (도움말 → 새 버전)", v["version"].as_str().unwrap_or("")), "info");
        } else if manual {
            c.notify(if v["state"] == "latest" { "지금이 최신 버전입니다.".to_string() } else { v["error"].as_str().unwrap_or("확인 실패").to_string() }, if v["state"] == "error" { "warn" } else { "ok" });
        }
        c.emit(Ev::Ui);
    });
}

pub fn skip(core: &Shared) {
    let v = UPDATE.lock()["version"].as_str().unwrap_or("").to_string();
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
    let kind = if cfg!(target_os = "macos") { "mac" } else if std::env::var_os("PORTABLE_EXECUTABLE_DIR").is_some() { "portable" } else if cfg!(windows) { "installer" } else { std::env::consts::OS };
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
    Err(format!("상태 {}", r.status()))
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
        assert_eq!(api_url("hahadark/WorshipPresenter-rs").unwrap(), "https://api.github.com/repos/hahadark/WorshipPresenter-rs/releases/latest");
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
