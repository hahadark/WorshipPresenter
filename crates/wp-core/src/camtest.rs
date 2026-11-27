//! 카메라 동시 열기 시험 (로드맵 3단계 ⑤, docs/roadmap-sources.md '카메라 동시 사용'):
//! 고른 카메라를 예배당 출력 창(wp-output = WebView2)과 브라우저(방송 주소를 여는 쪽 — OBS 브라우저 소스와 같은 Chromium)
//! 두 곳에서 **동시에** 열어 보고, 되는지 · 장치 이름 · 해상도 · 프레임이 들어오는지를 조작 화면에 글로 보인다.
//! 실제 웹캠 · 캡처보드(UVC)는 한 프로세스만 잡게 하는 경우가 많아 교회 PC 에서 직접 확인해야 한다 (가상 카메라로는 알 수 없음).
//!
//! - 페이지 `/helper/camtest?token=…&who=window|browser|obs&id=<deviceId>&label=<이름>` (조작 PC 전용 길, camhelper::routes)
//! - 페이지가 1초마다 `/api/command camTestReport { token, who, ok, label, width, height, fps, frames, error, ua }` 로 알린다.
//! - deviceId 는 브라우저 프로필마다 다르다 → 창(같은 WebView2 프로필)은 id 로, 브라우저는 이름(label)이 같은 장치로 연다.
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde_json::{json, Value};

#[derive(Default)]
struct St {
    token: String,
    label: String,
    started: i64,
    /// who → 마지막 알림
    reports: Vec<(String, Value)>,
}

static ST: Lazy<Mutex<St>> = Lazy::new(|| Mutex::new(St::default()));

/// 새 시험: 일회용 토큰 (전 결과는 지움)
pub fn start(label: &str) -> String {
    let mut s = ST.lock();
    s.token = crate::util::short_id("ct");
    s.label = label.chars().take(200).collect();
    s.started = crate::util::now_ms();
    s.reports.clear();
    s.token.clone()
}

pub fn stop() {
    let mut s = ST.lock();
    s.token.clear();
}

/// 시험 중인가 (토큰이 있고 3분 안)
pub fn active() -> bool {
    let s = ST.lock();
    !s.token.is_empty() && crate::util::now_ms() - s.started < 180_000
}

fn clip(v: &Value, n: usize) -> String {
    v.as_str().unwrap_or("").chars().filter(|c| !c.is_control()).take(n).collect()
}

/// 페이지가 보낸 알림 (토큰이 맞을 때만, 칸은 길이 · 모양을 줄여 담는다)
pub fn report(args: &Value) -> Result<(), String> {
    let mut s = ST.lock();
    if s.token.is_empty() || args["token"].as_str() != Some(s.token.as_str()) {
        return Err("token".into());
    }
    let who = match args["who"].as_str() {
        Some(w @ ("window" | "browser" | "obs")) => w.to_string(),
        _ => return Err("who".into()),
    };
    let num = |k: &str| args[k].as_f64().filter(|x| x.is_finite() && *x >= 0.0 && *x < 1e9).map(|x| (x * 10.0).round() / 10.0);
    let r = json!({
        "ok": args["ok"].as_bool().unwrap_or(false),
        "state": clip(&args["state"], 20),
        "label": clip(&args["label"], 200),
        "width": num("width"), "height": num("height"), "fps": num("fps"), "frames": num("frames"),
        "error": clip(&args["error"], 300),
        "ua": clip(&args["ua"], 200),
        "at": crate::util::now_ms(),
    });
    match s.reports.iter_mut().find(|(w, _)| *w == who) {
        Some(x) => x.1 = r,
        None => s.reports.push((who, r)),
    }
    Ok(())
}

/// 지금까지 결과 (who 차례: window → browser → obs)
pub fn results() -> Vec<(String, Value)> {
    let s = ST.lock();
    let mut v = s.reports.clone();
    let rank = |w: &str| match w {
        "window" => 0,
        "browser" => 1,
        _ => 2,
    };
    v.sort_by_key(|(w, _)| rank(w));
    v
}

/// 한 곳의 결과를 사람이 읽는 한 줄로 (한국어 원문 — 조작 화면이 tr 로 바꾼 이름표를 앞에 붙임)
pub fn line(r: &Value, now: i64) -> String {
    let stale = now - r["at"].as_i64().unwrap_or(0) > 4000;
    if !r["ok"].as_bool().unwrap_or(false) {
        let e = r["error"].as_str().unwrap_or("");
        return if e.is_empty() { crate::i18n::tr("여는 중…") } else { crate::i18n::tr_args("열 수 없음 — {1}", &[&e.to_string()]) };
    }
    let wh = match (r["width"].as_f64(), r["height"].as_f64()) {
        (Some(w), Some(h)) => format!("{w}×{h}"),
        _ => "?".into(),
    };
    let fps = r["fps"].as_f64().map(|f| format!("{f:.0}fps")).unwrap_or_else(|| "?fps".into());
    let frames = r["frames"].as_f64().unwrap_or(0.0);
    let flow = if stale {
        crate::i18n::tr("알림 끊김 (창이나 탭이 닫힘?)")
    } else if frames > 0.0 {
        crate::i18n::tr("화면 들어옴")
    } else {
        crate::i18n::tr("화면이 안 들어옴")
    };
    format!("{} · {} · {} · {} · {flow}", crate::i18n::tr("열림"), r["label"].as_str().unwrap_or(""), wh, fps)
}

/// 복사용 글 (교회 PC 결과를 알려 줄 때)
pub fn summary(label: &str, rows: &[(String, String)]) -> String {
    let mut s = format!("{}: {label}\n", crate::i18n::tr("카메라 동시 열기 시험"));
    for (who, l) in rows {
        s.push_str(&format!("- {who}: {l}\n"));
    }
    s
}

pub const PAGE_HTML: &str = r#"<!doctype html>
<html lang="ko"><head><meta charset="utf-8"><title>카메라 동시 열기 시험</title>
<style>
  html, body { margin: 0; height: 100%; background: #111; color: #eee; font: 15px sans-serif; overflow: hidden; }
  video { position: fixed; inset: 0; width: 100%; height: 100%; object-fit: contain; background: #000; }
  #info { position: fixed; left: 10px; right: 10px; bottom: 10px; background: rgba(0,0,0,.7); padding: 8px 10px; border-radius: 6px; white-space: pre-wrap; }
</style></head>
<body><video muted autoplay playsinline id="v"></video><div id="info">카메라를 여는 중…</div>
<script>
(() => {
  const q = new URLSearchParams(location.search);
  const token = q.get('token') || '', who = q.get('who') || 'browser', want = q.get('id') || '', wantLabel = q.get('label') || '';
  const v = document.getElementById('v'), info = document.getElementById('info');
  const whoName = { window: '예배당 출력 창', browser: '브라우저 (방송 주소 쪽)', obs: 'OBS 브라우저 소스' }[who] || who;
  let st = { ok: false, state: 'opening', label: '', width: 0, height: 0, fps: 0, frames: 0, error: '' };
  let frames = 0, track = null;
  function send() {
    fetch('/api/command', { method: 'POST', headers: { 'content-type': 'application/json', 'x-worshipcue': '1' },
      body: JSON.stringify({ cmd: 'camTestReport', args: { token, who, ua: navigator.userAgent, ...st } }) }).catch(() => {});
    info.textContent = whoName + '\n' + (st.ok ? ('열림 · ' + st.label + ' · ' + st.width + '×' + st.height + ' · ' + Math.round(st.fps || 0) + 'fps · 프레임 ' + st.frames) : (st.error ? ('열 수 없음 — ' + st.error) : '여는 중…'));
  }
  const errText = e => e ? ((e.name || 'Error') + ': ' + (e.message || '')) : 'getUserMedia';
  async function pickId() {
    // 같은 프로필(예배당 창)이면 id 그대로. 다른 브라우저는 이름이 같은 장치
    try {
      const list = (await navigator.mediaDevices.enumerateDevices()).filter(d => d.kind === 'videoinput');
      if (want && list.some(d => d.deviceId === want)) return want;
      const byLabel = wantLabel && list.find(d => d.label && d.label === wantLabel);
      if (byLabel) return byLabel.deviceId;
    } catch (e) {}
    return '';
  }
  async function open() {
    const size = { width: { ideal: 1920 }, height: { ideal: 1080 } };
    let id = await pickId();
    if (!id && wantLabel) {
      // 이름을 보려면 한 번 허락이 필요하다 (브라우저가 묻는다) → 다시 찾기
      try { const s0 = await navigator.mediaDevices.getUserMedia({ video: true }); s0.getTracks().forEach(t => t.stop()); } catch (e) { throw e; }
      id = await pickId();
      if (!id) throw { name: 'NotFoundError', message: '이 브라우저에서 같은 이름의 카메라를 찾지 못했습니다 (' + wantLabel + ')' };
    }
    return navigator.mediaDevices.getUserMedia({ video: id ? { deviceId: { exact: id }, ...size } : size, audio: false });
  }
  open().then(s => {
    track = s.getVideoTracks()[0];
    const set = track.getSettings ? track.getSettings() : {};
    st = { ok: true, state: 'open', label: track.label || wantLabel, width: set.width || 0, height: set.height || 0, fps: set.frameRate || 0, frames: 0, error: '' };
    v.srcObject = s; v.play().catch(() => {});
    const tick = () => { frames++; if (v.requestVideoFrameCallback) v.requestVideoFrameCallback(tick); };
    if (v.requestVideoFrameCallback) v.requestVideoFrameCallback(tick); else setInterval(() => { if (v.currentTime > 0) frames++; }, 100);
    track.onended = () => { st.ok = false; st.state = 'ended'; st.error = '장치가 끊겼습니다 (다른 프로그램이 가져감?)'; send(); };
    send();
  }).catch(e => { st = { ...st, ok: false, state: 'error', error: errText(e) }; send(); });
  // 1초마다 지난 1초에 들어온 프레임 수를 알린다 (0 이면 열렸지만 화면이 안 들어옴)
  setInterval(() => {
    if (track) { const set = track.getSettings ? track.getSettings() : {}; st.width = set.width || st.width; st.height = set.height || st.height; st.fps = set.frameRate || st.fps; }
    st.frames = frames; frames = 0; send();
  }, 1000);
  // 3분 뒤 닫기 (카메라를 계속 잡고 있지 않게)
  setTimeout(() => { if (track) track.stop(); st.ok = false; st.state = 'done'; st.error = '시험 시간이 끝나 카메라를 닫았습니다'; send(); try { window.ipc && window.ipc.postMessage('close'); } catch (e) {} }, 170000);
})();
</script></body></html>"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_needs_token_and_known_who() {
        let t = start("USB 카메라");
        assert!(report(&json!({ "token": "x", "who": "window", "ok": true })).is_err());
        assert!(report(&json!({ "token": t, "who": "evil", "ok": true })).is_err());
        report(&json!({ "token": t, "who": "browser", "ok": false, "error": "NotReadableError: Could not start video source" })).unwrap();
        report(&json!({ "token": t, "who": "window", "ok": true, "label": "USB 카메라", "width": 1920, "height": 1080, "fps": 30, "frames": 29 })).unwrap();
        let r = results();
        assert_eq!(r[0].0, "window", "창이 먼저");
        let now = crate::util::now_ms();
        assert!(line(&r[0].1, now).contains("1920×1080"));
        assert!(line(&r[0].1, now).contains("화면 들어옴"));
        assert!(line(&r[1].1, now).contains("NotReadableError"));
        assert!(line(&r[0].1, now + 10_000).contains("끊김"));
        let s = summary("USB 카메라", &[("창".into(), line(&r[0].1, now))]);
        assert!(s.contains("USB 카메라") && s.contains("1920×1080"));
        stop();
        assert!(report(&json!({ "token": t, "who": "window" })).is_err(), "끝난 시험");
    }
}
