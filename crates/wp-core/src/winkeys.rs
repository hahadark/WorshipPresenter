//! [트랙 D] 윈도우 전용: 전역 단축키 (창이 안 보일 때도 F5/F6/F9/F10) · 한글 입력 자동 전환
//! 전역 단축키는 Electron 판과 같은 네 동작만 (prevSlide · nextSlide · sendPreview · toggleClear), 키는 설정 shortcuts 에서.

use crate::core::Shared;
use serde_json::{json, Value};

/// Electron GLOBAL_ACTIONS
pub const GLOBAL_ACTIONS: [&str; 4] = ["prevSlide", "nextSlide", "sendPreview", "toggleClear"];

/// "Ctrl+Shift+F" · "F5" · "PageDown" · "ArrowLeft" → (MOD_* 비트, 가상 키). 모르는 키면 None
pub fn parse_combo(combo: &str) -> Option<(u32, u32)> {
    let mut mods = 0u32;
    let mut vk = None;
    for k in combo.split('+').map(|x| x.trim()).filter(|x| !x.is_empty()) {
        match k.to_lowercase().as_str() {
            "ctrl" | "control" | "commandorcontrol" => mods |= 0x2,
            "alt" => mods |= 0x1,
            "shift" => mods |= 0x4,
            "meta" | "win" | "super" => mods |= 0x8,
            _ => vk = Some(vk_of(k)?),
        }
    }
    vk.map(|v| (mods, v))
}

fn vk_of(k: &str) -> Option<u32> {
    let l = k.to_lowercase();
    if let Some(n) = l.strip_prefix('f').and_then(|x| x.parse::<u32>().ok()) {
        return (1..=24).contains(&n).then_some(0x6f + n);
    }
    if k.chars().count() == 1 {
        let c = k.chars().next()?.to_ascii_uppercase();
        if c.is_ascii_alphanumeric() {
            return Some(c as u32);
        }
    }
    Some(match l.as_str() {
        "space" => 0x20,
        "enter" => 0x0d,
        "escape" | "esc" => 0x1b,
        "tab" => 0x09,
        "backspace" => 0x08,
        "pageup" => 0x21,
        "pagedown" => 0x22,
        "end" => 0x23,
        "home" => 0x24,
        "arrowleft" | "left" => 0x25,
        "arrowup" | "up" => 0x26,
        "arrowright" | "right" => 0x27,
        "arrowdown" | "down" => 0x28,
        "insert" => 0x2d,
        "delete" => 0x2e,
        _ => return None,
    })
}

/// 지금 설정으로 등록할 것: [(동작, 조합)]. 끈 상태면 빈 목록
pub fn wanted(settings: &Value) -> Vec<(String, String)> {
    if !settings["globalShortcuts"].as_bool().unwrap_or(false) {
        return vec![];
    }
    GLOBAL_ACTIONS.iter().filter_map(|a| settings["shortcuts"][*a].as_str().filter(|c| !c.is_empty()).map(|c| (a.to_string(), c.to_string()))).collect()
}

/// 등록 상태 (설정 화면 표시용): { available, enabled, registered: [], failed: [] }
static STATUS: once_cell::sync::Lazy<parking_lot::Mutex<Value>> = once_cell::sync::Lazy::new(|| parking_lot::Mutex::new(json!({ "available": cfg!(windows), "enabled": false, "registered": [], "failed": [] })));

pub fn status() -> Value {
    STATUS.lock().clone()
}

pub fn run_action(core: &Shared, action: &str) {
    let mut c = core.lock();
    match action {
        "prevSlide" => c.step(-1),
        "nextSlide" => c.step(1),
        "sendPreview" => {
            if c.pres.preview.is_some() {
                c.send_preview();
            }
        }
        "toggleClear" => c.toggle_clear(),
        _ => {}
    }
}

/// 전역 단축키 스레드: 설정이 바뀌면 다시 등록 (1초마다 확인)
#[cfg(windows)]
pub fn start_global_shortcuts(core: Shared) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey, MOD_NOREPEAT};
    use windows_sys::Win32::UI::WindowsAndMessaging::{PeekMessageW, MSG, PM_REMOVE, WM_HOTKEY};
    std::thread::Builder::new()
        .name("global-hotkeys".into())
        .spawn(move || {
            let mut current: Vec<(String, String)> = vec![];
            let mut ids: Vec<(i32, String)> = vec![];
            let mut tick = 0u32;
            loop {
                if tick % 33 == 0 {
                    let want = wanted(core.lock().settings.get());
                    if want != current {
                        for (id, _) in ids.drain(..) {
                            unsafe { UnregisterHotKey(std::ptr::null_mut(), id) };
                        }
                        let (mut ok, mut bad) = (vec![], vec![]);
                        for (i, (action, combo)) in want.iter().enumerate() {
                            let id = 0x5750 + i as i32;
                            let reg = parse_combo(combo).map(|(m, vk)| unsafe { RegisterHotKey(std::ptr::null_mut(), id, m | MOD_NOREPEAT, vk) } != 0).unwrap_or(false);
                            if reg {
                                ids.push((id, action.clone()));
                                ok.push(action.clone());
                            } else {
                                bad.push(action.clone());
                            }
                        }
                        let enabled = !want.is_empty();
                        *STATUS.lock() = json!({ "available": true, "enabled": enabled, "registered": ok, "failed": bad });
                        current = want;
                    }
                }
                tick = tick.wrapping_add(1);
                let mut msg: MSG = unsafe { std::mem::zeroed() };
                while unsafe { PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) } != 0 {
                    if msg.message == WM_HOTKEY {
                        if let Some((_, a)) = ids.iter().find(|(id, _)| *id as usize == msg.wParam) {
                            run_action(&core, a);
                        }
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(30));
            }
        })
        .ok();
}
#[cfg(not(windows))]
pub fn start_global_shortcuts(_core: Shared) {}

/// 지금 앞에 있는 창(= 사용자가 입력하는 우리 창)의 입력기를 한글로. 반환: 바꿨는가
#[cfg(windows)]
pub fn ime_korean() -> bool {
    use windows_sys::Win32::UI::Input::Ime::{ImmGetDefaultIMEWnd, IMC_SETCONVERSIONMODE, IME_CMODE_NATIVE};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, SendMessageW, WM_IME_CONTROL};
    unsafe {
        let w = GetForegroundWindow();
        if w.is_null() {
            return false;
        }
        let ime = ImmGetDefaultIMEWnd(w);
        if ime.is_null() {
            return false;
        }
        // 한글(NATIVE) — 전각 등 다른 비트는 지금 상태 그대로 두지 않고 기본 한글 입력으로
        SendMessageW(ime, WM_IME_CONTROL, IMC_SETCONVERSIONMODE as usize, IME_CMODE_NATIVE as isize);
        true
    }
}
#[cfg(not(windows))]
pub fn ime_korean() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn combos() {
        assert_eq!(parse_combo("F5"), Some((0, 0x74)));
        assert_eq!(parse_combo("F10"), Some((0, 0x79)));
        assert_eq!(parse_combo("Ctrl+Shift+F"), Some((0x6, 'F' as u32)));
        assert_eq!(parse_combo("Alt+ArrowLeft"), Some((0x1, 0x25)));
        assert_eq!(parse_combo("PageDown"), Some((0, 0x22)));
        assert_eq!(parse_combo("Ctrl+"), None);
        assert_eq!(parse_combo("Ctrl+Weird"), None);
    }
    #[test]
    fn wanted_list() {
        let s = json!({ "globalShortcuts": true, "shortcuts": { "prevSlide": "F5", "nextSlide": "F6", "sendPreview": "F9", "toggleClear": "", "focusSearch": "Ctrl+K" } });
        let w = wanted(&s);
        assert_eq!(w.len(), 3);
        assert!(w.iter().all(|(a, _)| a != "focusSearch"));
        assert!(wanted(&json!({ "globalShortcuts": false, "shortcuts": { "prevSlide": "F5" } })).is_empty());
    }
}
