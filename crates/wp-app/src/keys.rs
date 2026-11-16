//! [트랙 C] 설정의 단축키 글("Ctrl+Shift+F", "F5", "Alt+ArrowLeft")을 egui 키로 바꾸고, 눌렸는지 본다

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};
use serde_json::Value;

/// "Ctrl+Shift+F" → 단축키. 모르는 키면 None
pub fn parse(s: &str) -> Option<KeyboardShortcut> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let mut m = Modifiers::NONE;
    let parts: Vec<&str> = s.split('+').map(|x| x.trim()).collect();
    let (key, mods) = if s.ends_with("++") { ("+", &parts[..parts.len().saturating_sub(2)]) } else { (*parts.last()?, &parts[..parts.len() - 1]) };
    for p in mods {
        match p.to_lowercase().as_str() {
            "ctrl" | "control" | "cmd" | "commandorcontrol" => m = m | Modifiers::COMMAND,
            "shift" => m = m | Modifiers::SHIFT,
            "alt" | "option" => m = m | Modifiers::ALT,
            _ => return None,
        }
    }
    Some(KeyboardShortcut::new(m, Key::from_name(key)?))
}

/// 현재 쓰는 단축키를 화면에도 같은 값으로 보인다. 빈 값은 꺼진 단축키다.
pub fn label(settings: &Value, id: &str, default: &str) -> String {
    settings["shortcuts"][id].as_str().unwrap_or(default).trim().to_string()
}

/// 설정 shortcuts[id] (없으면 기본값)이 이번 프레임에 눌렸으면 true (눌림은 먹는다)
pub fn pressed(ctx: &egui::Context, settings: &Value, id: &str, default: &str) -> bool {
    let txt = settings["shortcuts"][id].as_str().unwrap_or(default);
    let Some(sc) = parse(txt) else { return false };
    ctx.input_mut(|i| i.consume_shortcut(&sc))
}

/// 글자 입력 중에도 써도 되는 단축키인가 (꾸밈 키가 있거나 F 키)
pub fn safe_while_typing(settings: &Value, id: &str, default: &str) -> bool {
    let txt = settings["shortcuts"][id].as_str().unwrap_or(default);
    match parse(txt) {
        Some(sc) => !sc.modifiers.is_none() || matches!(sc.logical_key, Key::F1 | Key::F2 | Key::F3 | Key::F4 | Key::F5 | Key::F6 | Key::F7 | Key::F8 | Key::F9 | Key::F10 | Key::F11 | Key::F12),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_keys() {
        assert_eq!(parse("F5"), Some(KeyboardShortcut::new(Modifiers::NONE, Key::F5)));
        assert_eq!(parse("Ctrl+Shift+F"), Some(KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::F)));
        assert_eq!(parse("Alt+ArrowLeft"), Some(KeyboardShortcut::new(Modifiers::ALT, Key::ArrowLeft)));
        assert_eq!(parse("Ctrl+Home"), Some(KeyboardShortcut::new(Modifiers::COMMAND, Key::Home)));
        assert_eq!(parse("Escape"), Some(KeyboardShortcut::new(Modifiers::NONE, Key::Escape)));
        assert!(parse("Hyper+Q").is_none());
        assert!(parse("").is_none());
    }
}
