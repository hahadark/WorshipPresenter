/* 워십프리젠터 웹판 4단계 B — 출력 화면(예배당 · OBS · 무대)용 미디어 부품. 클라우드에서 연 페이지에만 끼워진다 (src/pages.mjs).
 * web/output · web/shared 는 고치지 않고 그 아래에서 브라우저 기능만 바꿔 끼운다. 계약: docs/web/media.md
 *  1. 카메라를 '이름으로': 설치판이 정한 장치 id(camera:<id>)는 PC · 브라우저 · 사이트마다 달라 다른 PC 에서는 맞지 않는다.
 *     출력 자료의 cameras[id].label(설치판이 붙인 이름)로 이 PC 의 같은 이름 장치를 찾는다. 사람이 /cameras 에서 고른 짝을 먼저 쓴다.
 *  2. 휴대폰 카메라: render.js 가 부르는 /phonecam/offer · answer · bye · list 를 실시간 연결(교회 중계실)로 바꾼다 (HTTP 폴링 없음).
 *  3. WebRTC 에 공개 무료 STUN 을 넣는다 (render.js · cam.html 은 같은 내부망만 보고 iceServers 를 비워 둠). TURN 은 쓰지 않는다.
 *  4. 같은 PC 의 출력 탭끼리 영상 재생 위치를 맞춘다 (BroadcastChannel). 코어 시계에 맞추는 render.js syncVideo 는 그대로.
 */
(function () {
  'use strict';
  var STORE = 'wp-cloud-camera-map'; // { 교회 카메라 이름(정리한 것) : 이 PC 장치 이름 }
  var ICE = [{ urls: 'stun:stun.cloudflare.com:3478' }];

  // ---------- 이름 맞추기 (src/media/routes.mjs normLabel · matchDevice 와 같게 — test/media.test.mjs 가 둘을 대조) ----------
  function normLabel(s) { return String(s || '').toLowerCase().replace(/\s*\([0-9a-f]{4}:[0-9a-f]{4}\)\s*$/i, '').replace(/\s+/g, ' ').trim(); }
  function matchDevice(label, devices) {
    var want = normLabel(label);
    if (!want) return null;
    var vids = devices.filter(function (d) { return d && d.kind === 'videoinput' && d.label; });
    return vids.find(function (d) { return d.label === label; }) || vids.find(function (d) { return normLabel(d.label) === want; })
      || vids.find(function (d) { var n = normLabel(d.label); return n.indexOf(want) >= 0 || want.indexOf(n) >= 0; }) || null;
  }
  function readMap() { try { return JSON.parse(localStorage.getItem(STORE) || '{}') || {}; } catch (e) { return {}; } }

  var cameras = {}; // 출력 자료의 cameras (설치판 settings.cameras: { 장치 id: { label, … } })
  window.addEventListener('wp-cloud-render', function (e) { if (e.detail && e.detail.cameras && typeof e.detail.cameras === 'object') cameras = e.detail.cameras; });
  function labelOf(id) { var c = cameras[id]; return c && c.label ? String(c.label) : ''; }

  // ---------- 1. getUserMedia: 장치 id → 이 PC 의 같은 이름 장치 ----------
  var md = navigator.mediaDevices;
  var unlocked = false;
  var resolved = {}; // 설치판 장치 id → 이 PC 장치 id (이 페이지 동안)
  function devices() { return md.enumerateDevices().catch(function () { return []; }); }
  /** 장치 이름은 카메라 허락이 있어야 보인다 → 이름이 비었으면 한 번 열었다 바로 닫는다 */
  function unlock(orig) {
    if (unlocked) return Promise.resolve();
    unlocked = true;
    return orig({ video: true, audio: false }).then(function (s) { s.getTracks().forEach(function (t) { t.stop(); }); }).catch(function () {});
  }
  function resolveId(id, orig) {
    if (resolved[id]) return Promise.resolve(resolved[id]);
    return devices().then(function (list) {
      if (list.some(function (d) { return d.kind === 'videoinput' && d.deviceId === id; })) return id; // 이 PC 에서 정한 id 그대로
      var needUnlock = !list.some(function (d) { return d.kind === 'videoinput' && d.label; });
      return (needUnlock ? unlock(orig).then(devices) : Promise.resolve(list)).then(function (all) {
        var name = labelOf(id);
        var want = readMap()[normLabel(name)] || name;
        var hit = matchDevice(want, all);
        if (!hit) { console.warn('[워십프리젠터] 이 PC 에서 카메라 \'' + (name || id) + '\' 을(를) 찾지 못했습니다 — /cameras 에서 맞춰 주세요'); return id; }
        resolved[id] = hit.deviceId;
        return hit.deviceId;
      });
    });
  }
  if (md && md.getUserMedia) {
    var origGUM = md.getUserMedia.bind(md);
    md.getUserMedia = function (c) {
      var v = c && c.video;
      var exact = v && v.deviceId && (v.deviceId.exact || v.deviceId);
      if (typeof exact !== 'string' || !exact || exact.indexOf('phone:') === 0) return origGUM(c);
      return resolveId(exact, origGUM).then(function (id) {
        if (id === exact) return origGUM(c);
        var nv = Object.assign({}, v, { deviceId: { exact: id } });
        return origGUM(Object.assign({}, c, { video: nv }));
      });
    };
  }

  // ---------- 3. WebRTC: iceServers 가 비었으면 공개 STUN ----------
  if (typeof window.RTCPeerConnection === 'function') {
    var Orig = window.RTCPeerConnection;
    var Wrapped = function (cfg) {
      cfg = Object.assign({}, cfg || {});
      if (!cfg.iceServers || !cfg.iceServers.length) cfg.iceServers = ICE;
      return new Orig(cfg);
    };
    Wrapped.prototype = Orig.prototype;
    Object.keys(Orig).forEach(function (k) { Wrapped[k] = Orig[k]; });
    window.RTCPeerConnection = Wrapped;
  }

  // ---------- 2. 휴대폰 카메라 신호 (render.js phoneStream 의 fetch 를 실시간 연결로) ----------
  var answers = {}; // viewer → { sdp } | { offline: true }
  function onRtc(m) {
    if (!m || !m.viewer) return;
    if (m.op === 'answer') answers[m.viewer] = { sdp: m.sdp };
    else if (m.op === 'offline') answers[m.viewer] = { offline: true };
  }
  function hook() { if (window.WPCloud) window.WPCloud.on(onRtc); else setTimeout(hook, 50); }
  hook();
  function reply(o) { return Promise.resolve(new Response(JSON.stringify(o), { status: 200, headers: { 'content-type': 'application/json' } })); }
  var prevFetch = window.fetch;
  window.fetch = function (input, init) {
    var u;
    try { u = new URL(typeof input === 'string' ? input : input.url, location.href); } catch (e) { return prevFetch(input, init); }
    if (u.origin !== location.origin || u.pathname.indexOf('/phonecam/') !== 0) return prevFetch(input, init);
    var body = {};
    try { body = init && typeof init.body === 'string' ? JSON.parse(init.body) : {}; } catch (e) { body = {}; }
    var cloud = window.WPCloud;
    switch (u.pathname) {
      case '/phonecam/offer':
        delete answers[body.viewer];
        if (!cloud || !cloud.send({ t: 'rtc', op: 'offer', phone: body.phone, label: labelOf('phone:' + body.phone), viewer: body.viewer, sdp: body.sdp })) return reply({ ok: false, error: 'offline' });
        return reply({ ok: true });
      case '/phonecam/answer': {
        var a = answers[u.searchParams.get('viewer') || ''];
        if (a && a.sdp) { delete answers[u.searchParams.get('viewer')]; return reply({ ok: true, sdp: a.sdp }); }
        return reply({ ok: true, wait: true });
      }
      case '/phonecam/bye':
        if (cloud) cloud.send({ t: 'rtc', op: 'bye', phone: body.phone, viewer: body.viewer });
        return reply({ ok: true });
      case '/phonecam/list':
        return prevFetch('/rtc/phones', init);
      default:
        return prevFetch(input, init);
    }
  };

  // ---------- 4. 같은 PC 의 출력 탭끼리 영상 위치 맞추기 ----------
  // 가장 먼저 연 탭(번호가 가장 작은 탭)이 기준. 다른 탭은 같은 영상이면 기준과의 차이를 재생 속도로 조금씩 줄이고, 1초 넘게 어긋나면 바로 옮긴다.
  // (각 탭은 이미 코어 시계에 0.5초 안으로 맞춰지므로, 여기서는 탭 사이의 몇십 ms 를 맞춘다 — 같은 화면을 두 모니터로 낼 때)
  try {
    var bc = new BroadcastChannel('wp-video-sync');
    var me = Date.now() * 1000 + Math.floor(Math.random() * 1000);
    var peers = {}; // 탭 번호 → 마지막으로 들은 때
    var lead = function () { var now = Date.now(), min = me; Object.keys(peers).forEach(function (k) { if (now - peers[k] < 3000 && +k < min) min = +k; }); return min === me; };
    var playing = function () {
      var vs = document.querySelectorAll('video:not([data-cam])');
      for (var i = 0; i < vs.length; i++) { var v = vs[i]; if (v.isConnected && (v.currentSrc || v.src) && !v.closest('.bc-measure')) return v; }
      return null;
    };
    var last = {};
    bc.onmessage = function (e) {
      var m = e.data || {};
      if (!m.tab || m.tab === me) return;
      peers[m.tab] = Date.now();
      if (m.t !== 'pos' || lead() || +m.tab > me) return;
      var v = playing();
      if (!v || (v.currentSrc || v.src) !== m.src) return;
      var want = m.time + (m.paused ? 0 : (Date.now() - m.at) / 1000);
      var diff = v.currentTime - want; // + 면 이 탭이 앞섬
      last = { diff: diff, at: Date.now() };
      if (m.paused || v.paused) { v.playbackRate = 1; return; }
      if (Math.abs(diff) > 1) { try { v.currentTime = want; } catch (err) { /* 불러오는 중 */ } v.playbackRate = 1; }
      else if (Math.abs(diff) > 0.03) v.playbackRate = Math.max(0.9, Math.min(1.1, 1 - diff * 0.5));
      else v.playbackRate = 1;
    };
    setInterval(function () {
      var v = playing();
      bc.postMessage(v && lead() ? { t: 'pos', tab: me, src: v.currentSrc || v.src, time: v.currentTime, paused: v.paused, at: Date.now() } : { t: 'here', tab: me });
    }, 500);
    /** 시험 · 상태 확인용 */
    window.WPMediaSync = { lead: lead, last: function () { return last; }, tab: me };
  } catch (e) { /* BroadcastChannel 없음 */ }

  window.WPMedia = { normLabel: normLabel, matchDevice: matchDevice, resolved: function () { return resolved; }, labelOf: labelOf };
})();
