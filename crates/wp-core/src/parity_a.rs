//! [트랙 A] 기능 대조(PARITY.md)에서 빠진 작은 코어 기능들
//!  - deleteStagePreset (무대 고정 프리셋 지우기)
//!  - cueText (큐시트를 글로: 순서표 복사·카톡 공유)
//!  - 화면 꺼짐 막기 (Electron powerSaveBlocker 'prevent-display-sleep')
//!  - 두 번 실행 막기 (Electron requestSingleInstanceLock)

use crate::core::{Core, Ev};
use serde_json::{json, Value};

impl Core {
    /// 무대 프리셋 지우기 (고정 중이던 것이면 고정도 푼다)
    pub fn delete_stage_preset(&mut self, id: &str) {
        let id = id.to_string();
        self.settings.update(|d| {
            if let Some(a) = d["stagePresets"].as_array_mut() {
                a.retain(|x| x["id"].as_str() != Some(id.as_str()));
            }
        });
        if self.b.stage.pin.as_ref().is_some_and(|p| p["id"].as_str() == Some(id.as_str())) {
            self.b.stage.pin = None;
            self.emit(Ev::Live);
        }
        self.emit(Ev::Ui);
    }
}

/// 큐시트 → 글 (Electron cueText 와 같은 모양)
pub fn cue_text(name: &str, service_date: &str, start_time: &str, items: &[Value], memo: &str) -> String {
    let when = if service_date.is_empty() { String::new() } else { format!(" ({}{})", service_date, if start_time.is_empty() { String::new() } else { format!(" {start_time}") }) };
    let mut lines = vec![format!("■ {name}{when}")];
    for (i, it) in items.iter().enumerate() {
        let ty = it["type"].as_str().unwrap_or("");
        let sub = match ty {
            "bible" => it["ref"].as_str().unwrap_or(""),
            "hymn" => "",
            _ => it["subtitle"].as_str().unwrap_or(""),
        };
        let note = it["note"].as_str().unwrap_or("");
        lines.push(format!(
            "{}. {}{}{}",
            i + 1,
            it["title"].as_str().unwrap_or(""),
            if sub.is_empty() { String::new() } else { format!(" — {sub}") },
            if note.is_empty() { String::new() } else { format!("  ({note})") }
        ));
    }
    if !memo.is_empty() {
        lines.push(String::new());
        lines.push(format!("※ {memo}"));
    }
    lines.join("\n")
}

impl Core {
    /// 열어 둔 큐시트를 글로
    pub fn active_cue_text(&self) -> Option<String> {
        let c = self.cue.as_ref()?;
        Some(cue_text(&c.name, &c.service_date, &c.start_time, &c.items, &c.memo))
    }
}

/// 순서 복제 (새 id, 제목 그대로) — 큐시트에서 그 다음 자리에
pub fn duplicate_item(it: &Value) -> Value {
    let mut x = it.clone();
    x["id"] = json!(crate::util::short_id("i"));
    x
}

/// 송출하는 동안 화면이 꺼지거나 절전으로 가지 않게 (끝날 때까지). 윈도우만
pub fn keep_display_awake() {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::Power::{SetThreadExecutionState, ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED};
        // 이 값은 부른 스레드에 붙는다 → 프로그램이 끝날 때까지 사는 스레드에서
        std::thread::Builder::new()
            .name("keep-awake".into())
            .spawn(|| {
                SetThreadExecutionState(ES_CONTINUOUS | ES_DISPLAY_REQUIRED | ES_SYSTEM_REQUIRED);
                loop {
                    std::thread::park();
                }
            })
            .ok();
    }
}

/// 이미 떠 있는 워십프리젠터(Rust)가 있는가: 같은 데이터 폴더의 이름 붙은 뮤텍스 (있으면 true)
/// 돌려받은 핸들은 프로그램이 끝날 때까지 쥐고 있어야 한다
pub fn single_instance(data_dir: &std::path::Path) -> (bool, Option<isize>) {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
        use windows_sys::Win32::System::Threading::CreateMutexW;
        // 데이터 폴더마다 하나 (점검용 --data 실행은 따로)
        let key: String = data_dir.to_string_lossy().to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect();
        // 끝 120글자만 (바이트로 자르면 한글 폴더 이름 한가운데가 잘려 켜자마자 꺼진다)
        let tail: String = { let n = key.chars().count(); key.chars().skip(n.saturating_sub(120)).collect() };
        let name: Vec<u16> = format!("Local\\WorshipPresenterRs_{tail}").encode_utf16().chain(std::iter::once(0)).collect();
        let h = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
        let exists = GetLastError() == ERROR_ALREADY_EXISTS;
        (exists, Some(h as isize))
    }
    // 맥 등: 자료 폴더의 잠금 파일 (다른 프로그램이 쥐고 있으면 이미 떠 있는 것). 프로그램이 끝나면 운영체제가 푼다
    #[cfg(not(windows))]
    {
        let _ = std::fs::create_dir_all(data_dir);
        let path = data_dir.join(".wp-instance.lock");
        match std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(&path) {
            Ok(f) => match f.try_lock() {
                Ok(()) => {
                    if let Ok(mut v) = INSTANCE_LOCKS.lock() { v.push(f); }
                    (false, Some(1))
                }
                Err(std::fs::TryLockError::WouldBlock) => (true, None),
                Err(_) => (false, None),
            },
            Err(_) => (false, None),
        }
    }
}

#[cfg(not(windows))]
static INSTANCE_LOCKS: std::sync::Mutex<Vec<std::fs::File>> = std::sync::Mutex::new(Vec::new());

/// `single_instance` 가 돌려준 핸들을 놓는다 (다시 확인하기 전에 — 쥐고 있으면 앞 프로그램이 꺼져도 이름이 남는다)
pub fn release_instance(h: Option<isize>) {
    #[cfg(windows)]
    if let Some(h) = h.filter(|h| *h != 0) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(h as _);
        }
    }
    #[cfg(not(windows))]
    if h.is_some() {
        if let Ok(mut v) = INSTANCE_LOCKS.lock() { v.clear(); }
    }
}

/// 이미 떠 있는 창을 앞으로 (창 제목으로 찾기)
pub fn focus_existing_window(title: &str) -> bool {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, SetForegroundWindow, ShowWindow, SW_RESTORE};
        let t: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
        let h = FindWindowW(std::ptr::null(), t.as_ptr());
        if h.is_null() {
            return false;
        }
        ShowWindow(h, SW_RESTORE);
        SetForegroundWindow(h);
        true
    }
    #[cfg(not(windows))]
    {
        let _ = title;
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn text_and_dup() {
        let items = vec![json!({ "type": "bible", "title": "성경 봉독", "ref": "시 23", "note": "" }), json!({ "type": "hymn", "title": "찬송", "subtitle": "x", "note": "일어서서" }), json!({ "type": "title", "title": "설교", "subtitle": "선한 목자" })];
        let t = cue_text("주일예배", "2026-11-15", "11:00", &items, "성찬");
        assert_eq!(t, "■ 주일예배 (2026-11-15 11:00)\n1. 성경 봉독 — 시 23\n2. 찬송  (일어서서)\n3. 설교 — 선한 목자\n\n※ 성찬");
        let d = duplicate_item(&items[0]);
        assert_ne!(d["id"], items[0]["id"]);
        assert_eq!(d["ref"], "시 23");
        let dir = std::env::temp_dir().join(format!("wp-si-{}", std::process::id()));
        let (a, _h) = single_instance(&dir);
        let (b, _h2) = single_instance(&dir);
        assert!(!a && b);
    }
}

#[cfg(test)]
mod audit_e_boundary_tests {
    /// 긴 한글 폴더 이름: 두 번 실행 막기 이름을 만들다 꺼지지 않는다 (전에는 바이트로 잘라 panic)
    #[test]
    fn single_instance_with_long_hangul_path() {
        let dir = std::path::PathBuf::from(format!("C:\\사용자\\{}\\자료-{}", "가나다라마바사아자차카타파하".repeat(6), std::process::id()));
        let (already, _h) = super::single_instance(&dir);
        assert!(!already);
    }
}
