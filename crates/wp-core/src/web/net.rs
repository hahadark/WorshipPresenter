//! [웹판] 같은 곳(클라우드 Worker)으로 보내는 요청 — 기다렸다 답을 받는 꼴 (동기 XMLHttpRequest).
//! 설치판 코드의 `aihelp::chat` 처럼 '답이 올 때까지 기다리는' 함수를 고치지 않고 웹에서 돌리려는 것.
//! 기다리는 동안 조작 화면이 멈추므로(예배 중 쓰는 기능에는 쓰지 않는다) 부르기 전에 '하는 중…' 을 한 번 그리게 `defer` 뒤에서 부른다.
use serde_json::Value;

/// POST JSON → (상태 코드, 답 JSON). 쿠키는 브라우저가 붙인다. 머리글 X-WorshipCue: 1 (쓰기 보호) 은 늘 붙인다
pub fn post_json_sync(path: &str, body: &Value, headers: &[(&str, &str)]) -> Result<(u16, Value), String> {
    let x = web_sys::XmlHttpRequest::new().map_err(|e| format!("{e:?}"))?;
    x.open_with_async("POST", path, false).map_err(|e| format!("{e:?}"))?;
    let _ = x.set_request_header("content-type", "application/json");
    let _ = x.set_request_header("x-worshipcue", "1");
    for (k, v) in headers {
        let _ = x.set_request_header(k, v);
    }
    x.send_with_opt_str(Some(&body.to_string())).map_err(|_| crate::i18n::tr("클라우드에 닿지 못했습니다. 인터넷 연결을 확인해 주세요.").to_string())?;
    let status = x.status().unwrap_or(0);
    let text = x.response_text().ok().flatten().unwrap_or_default();
    Ok((status, serde_json::from_str(&text).unwrap_or(Value::Null)))
}
