//! [트랙 D] 영어 화면 (설정 uiLang = "en")
//! - 조작 화면(egui): `tr("한국어 문구")` → 영어 사전(data/en.json, Electron 판 i18n/en.json 그대로)에 있으면 영어
//! - 출력 웹 화면: 서버가 HTML 에 i18n 스크립트를 넣는다 (Electron server.js withLang 과 같음)

use once_cell::sync::Lazy;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

static DICT: Lazy<HashMap<String, String>> = Lazy::new(|| {
    let v: Value = serde_json::from_str(include_str!("../data/en.json")).unwrap_or(Value::Null);
    v.as_object().map(|o| o.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect()).unwrap_or_default()
});
// 사전에 없는 Rust 판 문구
static EXTRA: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    HashMap::from([
        ("새 버전", "New version"),
        ("새 버전 확인", "Check for updates"),
        ("주보 → 큐시트", "Bulletin → cue sheet"),
        ("출력 설정 (예배당 화면 · OBS)", "Output settings (sanctuary · OBS)"),
        ("출력 설정 (예배당 창 · 모니터 · OBS 숨김)", "Output settings (window · monitor · OBS hide)"),
        ("찬양 라이브러리", "Song library"),
        ("그림 · 음악 · 영상 라이브러리", "Image · audio · video library"),
        ("Companion · 장비 (X32 · PTZ)", "Companion · devices (X32 · PTZ)"),
        ("내부망 출력 (다른 PC 의 OBS)", "LAN output (OBS on another PC)"),
        ("스트림덱 · 외부 단추", "Stream Deck · external buttons"),
        ("정보 · 라이선스", "About · licenses"),
        ("출력 템플릿 편집", "Edit output templates"),
        ("성경 번역본 가져오기…", "Import Bible translation…"),
        ("사용 기록 (저작권 보고)", "Usage log (copyright report)"),
        ("큐시트 관리 (휴지통 · 복제 · 내보내기)", "Manage cue sheets (trash · duplicate · export)"),
        ("실행 취소", "Undo"),
        ("다시 실행", "Redo"),
        ("큐시트 인쇄", "Print cue sheet"),
        ("송출 기록", "Send history"),
        ("즉석 자막", "Quick text"),
        ("회중 메시지", "Congregation message"),
        ("오버레이 (시계 · 카운트다운 · 배너)", "Overlays (clock · countdown · banner)"),
        ("무대 화면", "Stage display"),
        ("매크로 · 예약 실행", "Macros · schedule"),
    ])
});

static EN: AtomicBool = AtomicBool::new(false);

pub fn set_lang(lang: &str) {
    EN.store(lang == "en", Ordering::Relaxed);
}

pub fn is_en() -> bool {
    EN.load(Ordering::Relaxed)
}

/// 화면 문구: 영어 화면이면 사전에서 찾고, 없으면 그대로 (사용자 글은 넣지 말 것)
pub fn tr(s: &str) -> String {
    if !is_en() {
        return s.to_string();
    }
    if let Some(e) = EXTRA.get(s) {
        return e.to_string();
    }
    DICT.get(s).cloned().unwrap_or_else(|| s.to_string())
}

/// 출력 HTML 에 영어 번역 스크립트 넣기 (prefix = /shared/)
pub fn with_lang(buf: Vec<u8>) -> Vec<u8> {
    if !is_en() {
        return buf;
    }
    let s = String::from_utf8_lossy(&buf);
    if !s.contains("<html lang=\"ko\">") {
        return buf;
    }
    s.replacen("<html lang=\"ko\">", "<html lang=\"en\" data-lang=\"en\">", 1)
        .replacen("<head>", "<head><script src=\"/shared/i18n-en.js\"></script><script src=\"/shared/i18n.js\"></script>", 1)
        .into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lang() {
        set_lang("ko");
        assert_eq!(tr("파일"), "파일");
        assert_eq!(with_lang(b"<html lang=\"ko\"><head></head>".to_vec()), b"<html lang=\"ko\"><head></head>".to_vec());
        set_lang("en");
        assert_eq!(tr("파일"), "File");
        assert_eq!(tr("새 버전"), "New version");
        assert_eq!(tr("홍길동"), "홍길동");
        let h = String::from_utf8(with_lang(b"<html lang=\"ko\"><head></head>".to_vec())).unwrap();
        assert!(h.contains("data-lang=\"en\"") && h.contains("i18n-en.js"));
        set_lang("ko");
    }
}
