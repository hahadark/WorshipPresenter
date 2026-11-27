//! 카메라 도우미 웹 페이지 (서버에 내장, Electron 사본인 web/ 은 고치지 않는다)
//!  - /helper/cameras?token=…   wp-output 이 숨은 창으로 연다: getUserMedia(이름 권한) → enumerateDevices →
//!                              /api/command 'cameraDevices' 로 보내고 창을 닫는다.
//!  - /helper/camera?id=…&label=…  카메라 미리보기 (wp-output --windowed). 출력 화면과 같은 render.js 로 열어서
//!                              카메라 설정(해상도·밝기·좌우 반전)이 예배당 화면과 똑같이 적용된다.
//! 예배당 창(wp-output)과 같은 WebView2 프로필·같은 주소(http://127.0.0.1:포트)에서 열어야 deviceId 가 맞는다.

use crate::core::Shared;
use axum::http::header;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;

pub const PROBE_HTML: &str = r#"<!doctype html>
<html lang="ko"><head><meta charset="utf-8"><title>카메라 찾기</title></head>
<body style="background:#111;color:#ccc;font:14px sans-serif;padding:16px">카메라를 찾는 중…
<script>
(async () => {
  const token = new URLSearchParams(location.search).get('token') || '';
  let devices = [], error = '';
  try {
    // 이름(label)을 보려면 한 번 열어야 한다 (Electron library.js 와 같음). 바로 닫는다.
    const s = await navigator.mediaDevices.getUserMedia({ video: true, audio: false });
    s.getTracks().forEach(t => t.stop());
  } catch (e) { error = e ? ((e.name || '') + ': ' + (e.message || '')) : 'getUserMedia'; }
  try {
    devices = (await navigator.mediaDevices.enumerateDevices())
      .filter(d => d.kind === 'videoinput').slice(0, 32)
      .map(d => ({ deviceId: d.deviceId, label: d.label, kind: d.kind }));
  } catch (e) { error = error || String(e); }
  try {
    await fetch('/api/command', { method: 'POST', headers: { 'content-type': 'application/json', 'x-worshipcue': '1' },
      body: JSON.stringify({ cmd: 'cameraDevices', args: { token, devices, error } }) });
  } catch (e) { /* 조작 화면이 꺼졌으면 그냥 닫는다 */ }
  try { window.ipc && window.ipc.postMessage('close'); } catch (e) {}
})();
</script></body></html>"#;

pub const PREVIEW_HTML: &str = r#"<!doctype html>
<html lang="ko"><head><meta charset="utf-8"><title>카메라 미리보기</title>
<link rel="stylesheet" href="/shared/render.css">
<style>
  html, body { margin: 0; height: 100%; background: #000; overflow: hidden; font: 14px sans-serif; }
  video { position: fixed; inset: 0; width: 100%; height: 100%; object-fit: contain; }
  #info { position: fixed; left: 8px; bottom: 8px; color: #fff; background: rgba(0,0,0,.6); padding: 4px 8px; border-radius: 4px; }
</style></head>
<body><video muted autoplay playsinline id="v"></video><div id="info">…</div>
<script src="/shared/render.js"></script>
<script>
(() => {
  const q = new URLSearchParams(location.search);
  const id = q.get('id') || '';
  const label = q.get('label') || (id ? id.slice(0, 12) : '기본 카메라');
  document.title = '📷 ' + label;
  const v = document.getElementById('v'), info = document.getElementById('info');
  v.dataset.cam = id;
  const R = window.BCRender;
  // 출력 화면과 같은 카메라 설정 (해상도·프레임·밝기·좌우 반전)
  async function prefs() {
    try { const s = await (await fetch('/api/state')).json(); if (s && s.cameras) R.setCameraPrefs(s.cameras); } catch (e) {}
  }
  function quality() {
    R.cameraInfo(id).then(i => { info.textContent = label + ' · ' + (i.width || '?') + '×' + (i.height || '?') + ' · ' + (i.fps ? Math.round(i.fps) : '?') + 'fps'; }).catch(() => {});
  }
  prefs().then(() => R.cameraStream(id)).then(s => { v.srcObject = s; v.play().catch(() => {}); quality(); })
    .catch(e => { info.textContent = label + ' — 열 수 없음 (다른 프로그램이 쓰는 중?) ' + ((e && e.name) || ''); });
  setInterval(() => { prefs().then(quality); }, 3000);
})();
</script></body></html>"#;

fn html(s: &'static str) -> axum::response::Response {
    ([(header::CONTENT_TYPE, "text/html; charset=utf-8"), (header::CACHE_CONTROL, "no-store")], crate::i18n::with_lang(s.as_bytes().to_vec())).into_response()
}

/// 조작 PC 전용 길 (내부망 출력 서버에는 붙이지 않는다)
pub fn routes() -> Router<Shared> {
    Router::new()
        .route("/helper/cameras", get(|| async { html(PROBE_HTML) }))
        .route("/helper/camera", get(|| async { html(PREVIEW_HTML) }))
        .route("/helper/camtest", get(|| async { html(crate::camtest::PAGE_HTML) })) // 카메라 동시 열기 시험 (로드맵 3단계)
}
