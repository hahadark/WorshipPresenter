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
        ("출력 설정 (예배당 화면 · 모니터 · OBS 숨김)", "Output settings (sanctuary · monitor · OBS hide)"),
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

// 시험에서는 언어를 그 시험 스레드에만 적용한다: 전역으로 바꾸면 한국어 문구를 확인하는 다른 시험이
// 같은 때에 돌다가 가끔 실패했다 (diagnose·obs 시험이 번갈아 실패 — 갈래 D 점검에서 확인)
#[cfg(test)]
thread_local! {
    static EN_TEST: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

pub fn set_lang(lang: &str) {
    #[cfg(test)]
    {
        EN_TEST.with(|c| c.set(Some(lang == "en")));
    }
    #[cfg(not(test))]
    EN.store(lang == "en", Ordering::Relaxed);
}

pub fn is_en() -> bool {
    #[cfg(test)]
    if let Some(v) = EN_TEST.with(|c| c.get()) {
        return v;
    }
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

/// 기본 템플릿·테마 이름처럼 자료에 든 이름: 사전에 있는 것만 영어로 (사용자가 지은 이름은 그대로).
/// 테마를 적용해 생긴 "레드 쉴드 · 예배당" · "… · OBS 자막" 꼴도 앞·뒤를 따로 옮긴다.
pub fn tr_name(s: &str) -> String {
    if !is_en() {
        return s.to_string();
    }
    let t = tr(s);
    if t != s {
        return t;
    }
    for suf in [" · 예배당", " · OBS 자막"] {
        if let Some(base) = s.strip_suffix(suf) {
            let b = tr(base);
            if b != base {
                return format!("{b}{}", tr(suf));
            }
        }
    }
    t
}

/// &'static str 그대로 쓰는 자리(if/else 가지 등)용: 번역한 글을 한 번만 만들어 두고 같은 것을 돌려준다
pub fn tr_s(s: &'static str) -> &'static str {
    if !is_en() {
        return s;
    }
    static CACHE: Lazy<parking_lot::Mutex<HashMap<&'static str, &'static str>>> = Lazy::new(|| parking_lot::Mutex::new(HashMap::new()));
    let mut c = CACHE.lock();
    if let Some(v) = c.get(s) {
        return v;
    }
    let t = tr(s);
    let v: &'static str = if t == s { s } else { Box::leak(t.into_boxed_str()) };
    c.insert(s, v);
    v
}

/// 코어가 만든 알림처럼 이미 자리가 채워진 글(예: "3장 없음")을 번역한다 (Electron i18n.js 의 {1} 패턴 맞추기).
/// 사전에 글 그대로 있으면 그것, 없으면 {N} 자리가 있는 문구를 정규식으로 맞춰 자리를 옮겨 채운다.
/// 맞는 것이 없으면 그대로. 알림은 자주 같은 글이 다시 그려지므로 마지막 몇 개를 기억해 둔다.
pub fn tr_dyn(s: &str) -> String {
    dyn_inner(s, 0, false)
}

/// tr_dyn 과 같지만 채워진 자리 안의 글도 다시 옮긴다 ("모니터 2대 · 출력 꺼짐 — …" 처럼 문구 안에 문구가 든 것).
/// 프로그램이 만든 글(연결 점검 결과)에만 쓴다 — 사용자 글이 사전 문구와 같으면 그것도 옮겨지므로.
pub fn tr_dyn_deep(s: &str) -> String {
    dyn_inner(s, 3, false)
}

/// nested = 다른 문구의 자리 안에 든 글. 번역본 이름·순서 제목 같은 사용자 글일 수 있으므로
/// 뚜렷한 문구(글 그대로면 4자 이상, 패턴이면 자리 밖 글이 12바이트 이상)만 옮긴다.
fn dyn_inner(s: &str, deep: u8, nested: bool) -> String {
    if !is_en() || s.is_empty() {
        return s.to_string();
    }
    static PATS: Lazy<Vec<(regex::Regex, Vec<usize>, String, usize)>> = Lazy::new(|| {
        let mut out = vec![];
        for (k, v) in DICT.iter() {
            if !k.contains('{') {
                continue;
            }
            // "{1}장 · {2}" → ^(.+?)장 · (.+?)$ , 자리 번호 순서도 기억
            let mut re = String::from("^");
            let mut order = vec![];
            let mut rest = k.as_str();
            while let Some(a) = rest.find('{') {
                let after = &rest[a + 1..];
                let num = after.find('}').map(|e| &after[..e]).filter(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
                match num.and_then(|n| n.parse::<usize>().ok()) {
                    Some(i) => {
                        re.push_str(&regex::escape(&rest[..a]));
                        re.push_str("(.+?)");
                        order.push(i);
                        rest = &after[after.find('}').unwrap_or(0) + 1..];
                    }
                    None => {
                        re.push_str(&regex::escape(&rest[..a + 1]));
                        rest = after;
                    }
                }
            }
            re.push_str(&regex::escape(rest));
            re.push('$');
            // 글자가 거의 없는 문구("{1}" · "{1} {2}")는 아무 글에나 맞으므로 뺀다
            if order.is_empty() || k.len() < order.len() * 3 + 4 {
                continue;
            }
            if let Ok(r) = regex::Regex::new(&re) {
                let literal = k.len().saturating_sub(order.len() * 3);
                out.push((r, order, v.clone(), literal));
            }
        }
        // 긴 문구(더 구체적인 것)부터
        out.sort_by_key(|p| std::cmp::Reverse(p.0.as_str().len()));
        out
    });
    static RECENT: Lazy<parking_lot::Mutex<Vec<(String, String)>>> = Lazy::new(|| parking_lot::Mutex::new(vec![]));
    // 깊은 번역은 결과가 다를 수 있어 기억 칸의 열쇠를 따로 쓴다
    let key = if deep > 0 { format!("\u{1}{s}") } else { s.to_string() };
    if let Some(v) = RECENT.lock().iter().find(|x| x.0 == key) {
        return v.1.clone();
    }
    let t = tr(s);
    let out = if t != s && (!nested || s.chars().count() >= 4) {
        t
    } else {
        PATS.iter()
            .filter(|p| !nested || p.3 >= 12)
            .find_map(|(re, order, en, _)| {
                let c = re.captures(s)?;
                let mut args: Vec<String> = vec![String::new(); order.iter().copied().max().unwrap_or(0)];
                for (k, &i) in order.iter().enumerate() {
                    if let (Some(slot), Some(m)) = (i.checked_sub(1).and_then(|i| args.get_mut(i)), c.get(k + 1)) {
                        *slot = if deep > 1 && m.as_str().chars().any(|ch| ('가'..='힣').contains(&ch)) { dyn_inner(m.as_str(), deep - 1, true) } else { m.as_str().to_string() };
                    }
                }
                let refs: Vec<&dyn std::fmt::Display> = args.iter().map(|a| a as &dyn std::fmt::Display).collect();
                Some(fill_args(en, &refs))
            })
            .unwrap_or_else(|| s.to_string())
    };
    let mut r = RECENT.lock();
    if r.len() >= 64 {
        r.remove(0);
    }
    r.push((key, out.clone()));
    out
}

/// 자리가 있는 문구: tr_args("{1}장 · {2}", &[&a, &b]) — 사전은 Electron 처럼 {1} {2} 자리로 찾고, 자리를 채운다
pub fn tr_args(template: &str, args: &[&dyn std::fmt::Display]) -> String {
    fill_args(&tr(template), args)
}

/// 원래 문구의 자리만 치환한다. 인수에 들어 있는 {2} 같은 사용자 글은 다시 치환하지 않는다.
fn fill_args(template: &str, args: &[&dyn std::fmt::Display]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let candidate = &rest[start + 1..];
        if let Some(end) = candidate.find('}') {
            let number = &candidate[..end];
            if !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit()) {
                if let Ok(index) = number.parse::<usize>() {
                    if let Some(arg) = index.checked_sub(1).and_then(|i| args.get(i)) {
                        out.push_str(&arg.to_string());
                        rest = &candidate[end + 1..];
                        continue;
                    }
                }
            }
        }
        out.push('{');
        rest = candidate;
    }
    out.push_str(rest);
    out
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
    fn arguments_are_not_reinterpreted() {
        assert_eq!(fill_args("{1} / {2} / {1}", &[&"사용자 {2}", &"끝"]), "사용자 {2} / 끝 / 사용자 {2}");
        assert_eq!(fill_args("{0} {3} {x} {1} {", &[&"정상"]), "{0} {3} {x} 정상 {");
        assert_eq!(fill_args("{{1}}", &[&"글"]), "{글}");
    }

    #[test]
    fn lang() {
        let set_lang = |l: &str| EN_TEST.with(|c| c.set(Some(l == "en"))); // 이 스레드에서만
        set_lang("ko");
        assert_eq!(tr("파일"), "파일");
        assert_eq!(with_lang(b"<html lang=\"ko\"><head></head>".to_vec()), b"<html lang=\"ko\"><head></head>".to_vec());
        set_lang("en");
        // [감사 E] 기본 템플릿·테마 이름은 영어로, 사용자가 지은 이름은 그대로
        assert_eq!(tr_name("일반 예배"), "Standard service");
        assert_eq!(tr_name("레드 쉴드 · 예배당"), "Red Shield · Sanctuary");
        assert_eq!(tr_name("우리 교회 템플릿"), "우리 교회 템플릿");
        // [감사 E] 문구 안에 든 문구도 옮긴다 (연결 점검 결과). 얕은 번역은 자리를 그대로 둔다
        assert!(!tr_dyn_deep("모니터 2대 · 출력 꺼짐 — '예배당 화면 켜기' 로 켭니다").chars().any(|c| ('가'..='힣').contains(&c)), "{}", tr_dyn_deep("모니터 2대 · 출력 꺼짐 — '예배당 화면 켜기' 로 켭니다"));
        assert_eq!(tr_dyn("저장 실패: 디스크 가득"), "Save failed: 디스크 가득");
        assert_eq!(tr_dyn("'찬양' 순서 삭제"), "Delete item '찬양'");
        // 자리 안의 번역본 이름·순서 제목은 건드리지 않는다
        assert!(tr_dyn_deep("7개 — NIV (2011) · 개역개정 · 개역한글 …").contains("개역개정 · 개역한글"), "{}", tr_dyn_deep("7개 — NIV (2011) · 개역개정 · 개역한글 …"));
        assert!(tr_dyn_deep("2번 '찬양': 찬양 가사가 비어 있습니다").contains("'찬양'"), "{}", tr_dyn_deep("2번 '찬양': 찬양 가사가 비어 있습니다"));
        assert_eq!(tr("파일"), "File");
        assert_eq!(tr("새 버전"), "New version");
        assert_eq!(tr("홍길동"), "홍길동");
        assert_eq!(tr_args("{1}장 없음", &[&3]), "{1}장 없음".replace("{1}", "3"));
        // 이미 채워진 알림도 사전 패턴으로 번역
        let (k, v) = DICT.iter().find(|(k, _)| k.starts_with("{1}") && k.matches('{').count() == 1 && k.len() > 6).map(|(k, v)| (k.clone(), v.clone())).unwrap();
        assert_eq!(tr_dyn(&k.replace("{1}", "가나 7")), v.replace("{1}", "가나 7"));
        assert_eq!(tr_dyn("홍길동 사관"), "홍길동 사관");
        let h = String::from_utf8(with_lang(b"<html lang=\"ko\"><head></head>".to_vec())).unwrap();
        assert!(h.contains("data-lang=\"en\"") && h.contains("i18n-en.js"));
        set_lang("ko");
    }
}
