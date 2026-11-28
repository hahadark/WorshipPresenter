/* 워십프리젠터 웹판 4단계 B — 휴대폰 카메라 페이지(web/phone/cam.html 그대로)를 클라우드에 붙이는 부품. 계약: docs/web/media.md
 * cam.html 은 설치판의 https 서버에 0.7초마다 POST /cam/poll · 답을 POST /cam/answer 로 보낸다.
 * 여기서는 그 둘을 이 페이지 안에서 받아, 교회 중계실과의 실시간 연결 하나로 바꾼다 (요청 수 0 · 상태는 5초마다 또는 바뀔 때만).
 * 영상은 지나가지 않는다: 휴대폰 → 출력 화면 직접 (WebRTC). 같은 Wi-Fi 가 아니면 공개 STUN 으로 시도하고, 안 되면 TURN 이 필요하다(쓰지 않음).
 */
(function () {
  'use strict';
  var CFG = window.WP_PHONE || {};
  var ICE = CFG.ice && CFG.ice.length ? CFG.ice : [{ urls: 'stun:stun.cloudflare.com:3478' }];
  var K = new URLSearchParams(location.search).get('k') || '';
  var ws = null, open = false, delay = 1000, queue = [], lastHello = '', lastHelloAt = 0, closedFor = '';
  var toOf = {}; // viewer → 중계실이 붙인 보는 화면 번호 (답을 돌려보낼 곳)

  // WebRTC: iceServers 가 비었으면 공개 STUN
  if (typeof window.RTCPeerConnection === 'function') {
    var Orig = window.RTCPeerConnection;
    var Wrapped = function (cfg) { cfg = Object.assign({}, cfg || {}); if (!cfg.iceServers || !cfg.iceServers.length) cfg.iceServers = ICE; return new Orig(cfg); };
    Wrapped.prototype = Orig.prototype;
    Object.keys(Orig).forEach(function (k) { Wrapped[k] = Orig[k]; });
    window.RTCPeerConnection = Wrapped;
  }

  function connect() {
    var u = new URL('/rtc/phone?k=' + encodeURIComponent(K), location.href);
    u.protocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
    ws = new WebSocket(u.href);
    ws.onopen = function () { open = true; delay = 1000; lastHello = ''; };
    ws.onmessage = function (e) {
      var m; try { m = JSON.parse(e.data); } catch (err) { return; }
      if (m.t === 'offer') { toOf[m.viewer] = m.to; queue.push({ t: 'offer', viewer: m.viewer, sdp: m.sdp }); }
      else if (m.t === 'bye') queue.push({ t: 'bye', viewer: m.viewer });
    };
    ws.onclose = function (e) {
      open = false;
      if (e.code === 4003 || e.code === 4001) { closedFor = e.code === 4003 ? 'key' : 'replaced'; return; } // 링크가 끝남 · 다른 탭이 이 휴대폰으로 붙음
      setTimeout(connect, delay + Math.random() * 500);
      delay = Math.min(delay * 2, 15000);
    };
  }
  connect();

  function reply(o, status) { return Promise.resolve(new Response(JSON.stringify(o), { status: status || 200, headers: { 'content-type': 'application/json' } })); }
  var prev = window.fetch;
  window.fetch = function (input, init) {
    var u;
    try { u = new URL(typeof input === 'string' ? input : input.url, location.href); } catch (e) { return prev(input, init); }
    if (u.origin !== location.origin || (u.pathname !== '/cam/poll' && u.pathname !== '/cam/answer')) return prev(input, init);
    var b = {};
    try { b = JSON.parse((init && init.body) || '{}'); } catch (e) { b = {}; }
    if (closedFor === 'key') return reply({ ok: false, error: 'key' });
    if (!open) return Promise.reject(new Error('연결 중')); // cam.html 이 '연결되지 않음' 을 보인다
    if (u.pathname === '/cam/poll') {
      var hello = JSON.stringify({ t: 'hello', id: b.id, name: b.name, size: b.size, viewers: b.viewers });
      if (hello !== lastHello || Date.now() - lastHelloAt > 5000) { try { ws.send(hello); lastHello = hello; lastHelloAt = Date.now(); } catch (e) { /* 다시 이음 */ } }
      var msgs = queue; queue = [];
      return reply({ ok: true, msgs: msgs });
    }
    try { ws.send(JSON.stringify({ t: 'answer', viewer: b.viewer, sdp: b.sdp, to: toOf[b.viewer] })); } catch (e) { /* 다시 이음 */ }
    return reply({ ok: true });
  };
})();
