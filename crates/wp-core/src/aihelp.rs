//! AI 도우미 공통 (예배 전 점검 · 설교 원고 → 슬라이드 · 가사 정리 · 자막 번역 · 예배 후 정리 · 말로 시키기)
//!
//! 주보 → 큐시트(bulletin.rs)·PPT 나누기(pptsplit.rs)와 같은 길을 쓴다: OpenAI 호환 `chat/completions` 주소 하나 + 키 + 모델.
//! 기본은 오픈라우터지만 설정의 `ai.endpoint` 에 DeepSeek(https://api.deepseek.com/chat/completions) 같은 다른 OpenAI 호환 주소를 넣을 수 있다.
//! 키는 trackd.rs `ai_key()` (Windows DPAPI · 맥 키체인). 키가 없으면 이 모듈의 함수는 '키가 필요합니다' 오류를 돌려주고, 부르는 쪽은 규칙만으로 동작한다.
//!
//! 밖으로 글을 보내는 기능은 처음 쓸 때 무엇이 어디로 가는지 알리고 동의를 받는다 (설정 `aiConsent.<기능>` = true).
//! 음성은 PC 밖으로 보내지 않는다 (음성 인식은 PC 안의 whisper.cpp, 보내는 것은 글뿐).

use crate::bulletin::{parse_json, DEFAULT_ENDPOINT, DEFAULT_MODEL};
use crate::core::Core;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde_json::{json, Value};

/// AI 연결 설정 (키는 화면·로그에 내지 않는다)
#[derive(Clone, Debug, Default)]
pub struct AiCfg {
    pub key: String,
    pub model: String,
    pub endpoint: String,
}

impl AiCfg {
    pub fn has_key(&self) -> bool {
        !self.key.trim().is_empty()
    }
    pub fn endpoint(&self) -> &str {
        if self.endpoint.trim().is_empty() { DEFAULT_ENDPOINT } else { self.endpoint.trim() }
    }
    pub fn model(&self) -> &str {
        if self.model.trim().is_empty() { DEFAULT_MODEL } else { self.model.trim() }
    }
    /// 글이 가는 곳 (동의 창에 보여 줄 호스트 이름)
    pub fn host(&self) -> String {
        url::Url::parse(self.endpoint()).ok().and_then(|u| u.host_str().map(String::from)).unwrap_or_else(|| self.endpoint().to_string())
    }
}

/// 이름 붙은 OpenAI 호환 주소 (설정 화면의 '제공자' 단추)
pub const PROVIDERS: [(&str, &str, &str); 3] = [
    ("오픈라우터", "https://openrouter.ai/api/v1/chat/completions", "anthropic/claude-haiku-5.5"),
    ("DeepSeek", "https://api.deepseek.com/chat/completions", "deepseek-chat"),
    ("OpenAI", "https://api.openai.com/v1/chat/completions", "gpt-6-luna"),
];

/// 기능 이름 (동의 키) → 화면 이름 · 무엇이 가는지
pub const FEATURES: [(&str, &str, &str); 6] = [
    ("cuecheck", "예배 전 큐시트 점검 (AI)", "큐시트의 찬양 가사 · 공지 글"),
    ("sermon", "설교 원고 → 슬라이드 (AI)", "설교 원고 글"),
    ("lyrics", "가사 자동 정리 (AI)", "붙여 넣은 가사 글"),
    ("translate", "실시간 자막 번역", "음성 인식으로 받아 적은 글 (음성은 보내지 않음)"),
    ("recap", "예배 후 정리 (AI)", "순서 목록 · 송출 시각 · 받아 적은 설교 글 (음성은 보내지 않음)"),
    ("say", "말로 시키기 (AI)", "입력한 말 · 큐시트 이름과 순서 제목"),
];

pub fn feature_name(f: &str) -> &'static str {
    FEATURES.iter().find(|x| x.0 == f).map(|x| x.1).unwrap_or("AI 기능")
}
pub fn feature_what(f: &str) -> &'static str {
    FEATURES.iter().find(|x| x.0 == f).map(|x| x.2).unwrap_or("입력한 글")
}

pub const NEED_KEY: &str = "AI 키가 필요합니다. AI → AI 설정에서 키를 넣으세요 (규칙으로 하는 기능은 키 없이도 됩니다).";

impl Core {
    pub fn ai_cfg(&self) -> AiCfg {
        let a = &self.settings.get()["ai"];
        AiCfg { key: self.ai_key(), model: a["model"].as_str().unwrap_or("").to_string(), endpoint: a["endpoint"].as_str().unwrap_or("").to_string() }
    }
    /// 이 기능으로 글을 밖에 보내도 된다고 동의했나
    pub fn ai_consented(&self, feature: &str) -> bool {
        self.settings.get()["aiConsent"][feature].as_bool() == Some(true)
    }
    pub fn set_ai_consent(&mut self, feature: &str, on: bool) {
        let f = feature.to_string();
        self.settings.update(|d| {
            if !d["aiConsent"].is_object() {
                d["aiConsent"] = json!({});
            }
            d["aiConsent"][f] = json!(on);
        });
    }
}

// ---------------- 시험용 가짜 응답 ----------------
type Mock = Box<dyn Fn(&Value) -> Result<Value, String> + Send>;
static MOCK: Lazy<Mutex<Option<Mock>>> = Lazy::new(|| Mutex::new(None));

/// 시험·점검: 네트워크 대신 이 함수가 응답 본문(choices…)을 만든다. None 이면 원래대로
pub fn set_mock(f: Option<Mock>) {
    *MOCK.lock() = f;
}

/// 점검 실행에서 `WP_AI_MOCK=<파일>` 이면 그 파일 내용을 AI 답으로 쓴다 (실제 호출 없이 화면 흐름 확인)
fn env_mock() -> Option<String> {
    let p = std::env::var("WP_AI_MOCK").ok()?;
    std::fs::read_to_string(p).ok()
}

/// 요청 본문
pub fn request_body(model: &str, system: &str, user: &str, json_mode: bool, max_tokens: u32) -> Value {
    let mut b = json!({ "model": model, "messages": [{ "role": "system", "content": system }, { "role": "user", "content": user }], "temperature": 0.2, "max_tokens": max_tokens });
    if json_mode {
        b["response_format"] = json!({ "type": "json_object" });
    }
    b
}

/// 응답 본문 → 글
pub fn response_text(body: &Value) -> Result<String, String> {
    let msg = &body["choices"][0]["message"]["content"];
    let text = match msg {
        Value::String(s) => s.clone(),
        Value::Array(a) => a.iter().filter_map(|c| c["text"].as_str()).collect::<Vec<_>>().join(""),
        _ => String::new(),
    };
    if text.trim().is_empty() {
        return Err(crate::i18n::tr("AI 응답이 비었습니다.").into());
    }
    Ok(text)
}

/// AI 에 묻고 답 글을 돌려준다 (오래 걸리니 백그라운드에서). 반환: (답 글, 사용량)
pub fn chat(cfg: &AiCfg, system: &str, user: &str, json_mode: bool, max_tokens: u32) -> Result<(String, Value), String> {
    let body = request_body(cfg.model(), system, user, json_mode, max_tokens);
    if let Some(m) = MOCK.lock().as_ref() {
        let v = m(&body)?;
        return Ok((response_text(&v)?, v["usage"].clone()));
    }
    if let Some(t) = env_mock() {
        return Ok((t, Value::Null));
    }
    if !cfg.has_key() {
        return Err(crate::i18n::tr(NEED_KEY).into());
    }
    let mut resp = ureq::post(cfg.endpoint())
        .header("Authorization", &format!("Bearer {}", cfg.key.trim()))
        .header("HTTP-Referer", "https://github.com/hahadark/WorshipPresenter")
        .header("X-Title", "Worship Presenter")
        .config()
        .http_status_as_error(false)
        .timeout_global(Some(std::time::Duration::from_secs(120)))
        .build()
        .send_json(&body)
        .map_err(|e| crate::i18n::tr_args("AI 연결 실패: {1}", &[&e]))?;
    let status = resp.status().as_u16();
    let v: Value = resp.body_mut().read_json().unwrap_or(Value::Null);
    if !(200..300).contains(&status) {
        let m = v["error"]["message"].as_str().unwrap_or("").chars().take(200).collect::<String>();
        return Err(crate::i18n::tr_args("AI 오류 ({1}): {2}", &[&status, &m]));
    }
    Ok((response_text(&v)?, v["usage"].clone()))
}

/// JSON 으로 답을 받는다 (```json 코드 블록도 읽음)
pub fn chat_json(cfg: &AiCfg, system: &str, user: &str, max_tokens: u32) -> Result<(Value, Value), String> {
    let (t, u) = chat(cfg, system, user, true, max_tokens)?;
    Ok((parse_json(&t)?, u))
}

/// 사용량 → 짧은 글 (예: '토큰 1,234')
pub fn usage_text(u: &Value) -> String {
    let n = u["total_tokens"].as_i64().unwrap_or(0);
    if n == 0 { String::new() } else { format!("토큰 {n}") }
}

#[cfg(test)]
pub(crate) static TEST_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cfg_defaults_and_host() {
        let c = AiCfg::default();
        assert!(!c.has_key());
        assert_eq!(c.endpoint(), DEFAULT_ENDPOINT);
        assert_eq!(c.host(), "openrouter.ai");
        let d = AiCfg { key: "x".into(), model: "deepseek-chat".into(), endpoint: PROVIDERS[1].1.into() };
        assert_eq!(d.host(), "api.deepseek.com");
        assert_eq!(d.model(), "deepseek-chat");
    }
    #[test]
    fn no_key_is_error_and_mock_answers() {
        let _g = TEST_LOCK.lock();
        set_mock(None);
        let e = chat(&AiCfg::default(), "s", "u", false, 10).unwrap_err();
        assert!(e.contains("AI 키"), "{e}");
        set_mock(Some(Box::new(|b| {
            assert_eq!(b["messages"][1]["content"], "u");
            Ok(json!({ "choices": [{ "message": { "content": "```json\n{\"a\":1}\n```" } }], "usage": { "total_tokens": 7 } }))
        })));
        let (v, u) = chat_json(&AiCfg::default(), "s", "u", 10).unwrap();
        set_mock(None);
        assert_eq!(v["a"], 1);
        assert_eq!(usage_text(&u), "토큰 7");
    }
}

/// 실제 AI 점검 (기본 건너뜀): WP_AI_LIVE_KEY_ENV=<키가 든 환경변수 이름> WP_AI_LIVE_ENDPOINT · WP_AI_LIVE_MODEL
/// 키는 환경변수에서만 읽고 출력하지 않는다. cargo test -p wp-core -- --ignored ai_live --nocapture
#[cfg(test)]
mod live {
    use super::*;
    #[test]
    #[ignore]
    fn ai_live() {
        let Ok(var) = std::env::var("WP_AI_LIVE_KEY_ENV") else { return };
        let cfg = AiCfg { key: std::env::var(var).unwrap_or_default(), endpoint: std::env::var("WP_AI_LIVE_ENDPOINT").unwrap_or_default(), model: std::env::var("WP_AI_LIVE_MODEL").unwrap_or_default() };
        let _g = TEST_LOCK.lock();
        set_mock(None);
        let mut tokens = 0i64;
        let mut add = |u: &Value| tokens += u["total_tokens"].as_i64().unwrap_or(0);
        let items = vec![json!({"type":"song","title":"찬양","lyrics":"주 은헤 임을 고백 합니다\n내 삶의 모든 것 주께 드립니\n다"}), json!({"type":"text","title":"광고","body":"수요 예베는 저녁 7시 30분 입니다"})];
        let (f, u) = crate::cuecheck::ask_ai(&cfg, &items).unwrap();
        add(&u);
        println!("[점검] {} 곳: {:?}", f.len(), f.iter().map(|x| x.msg.clone()).collect::<Vec<_>>());
        let ms = "제목: 포도나무와 가지\n본문: 요 15:1-8\n첫째, 붙어 있어야 삽니다. 로마서 8장 28절을 보면 모든 것이 합력하여 선을 이룹니다.\n둘째, 열매로 압니다. “기도는 영혼의 호흡이다” — 어느 성도\n셋째, 아버지께 영광이 됩니다.";
        let (p, u) = crate::sermonslides::ask_ai(&cfg, ms, &json!({})).unwrap();
        add(&u);
        println!("[설교] {:?}", p.iter().map(|x| x.label.clone()).collect::<Vec<_>>());
        let (c, u) = crate::lyricsclean::ask_ai(&cfg, "Verse 1\n주님만 바라봅니다 주님만 의지합니다\nChorus\n할렐루야 할렐루야\n\nVerse 2\n주의 이름 높이며\nChorus\n할렐루야 할렐루야").unwrap();
        add(&u);
        println!("[가사] {:?} / {}", c.lyrics, c.order);
        let (sm, u) = crate::recap::ask_summary(&cfg, "[11:15:00] 오늘 요한복음 15장 5절 말씀을 봅니다\n[11:20:00] 첫째 붙어 있어야 삽니다 가지는 나무에 붙어 있어야 열매를 맺습니다\n[11:30:00] 둘째 열매로 압니다", "포도나무와 가지").unwrap();
        add(&u);
        println!("[요약] {sm}");
        let ctx = crate::saycmd::Ctx { cues: vec![("c1".into(), "2026-11-22 주일 예배".into(), "2026-11-22".into())], open_id: "c1".into(), items: vec![("title".into(), "설교".into(), "“포도나무” · 홍길동".into()), ("bible".into(), "성경봉독".into(), "요 15:1-8".into())], today: "2026-11-27".into() };
        let (v, u) = crate::saycmd::ask_ai(&cfg, "지난주 큐시트 복사해서 설교 제목만 '은혜' 로 바꾸고 송출해", &ctx).unwrap();
        add(&u);
        let (ok, bad) = crate::saycmd::validate(&v, &ctx);
        println!("[말] 할 일 {:?} / 뺀 것 {bad:?} / {}", ok.iter().map(|x| x.label.clone()).collect::<Vec<_>>(), v["say"]);
        let (t, u) = chat(&cfg, &crate::voicefollow::translate_system(), "오늘 함께 읽을 말씀은 요한복음 15장 5절입니다", false, 200).unwrap();
        add(&u);
        println!("[번역] {t}");
        println!("합계 토큰 {tokens}");
    }
}
