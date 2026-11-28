/* 워십프리젠터 웹판 — 같은 브라우저 출력 어댑터 (인터넷 · 서버 없이).
 * dist-web 의 /outputs/main · overlay · stage 페이지 머리에만 끼워진다 (설치판 · 클라우드가 여는 페이지에는 없다).
 * 계약: docs/web/architecture.md 5절. 메시지 모양은 클라우드 어댑터(cloud/client/adapter.js)의 BroadcastChannel 부분과 같다.
 *  - EventSource('/events?client=…') → BroadcastChannel('wp-live') 의 {t:'render', client, data} · {t:'draw', data}
 *  - 열리면 {t:'sync'} 로 지금 화면을 달라고 하고, 2초마다 {t:'out', id, client, kind} 로 '살아 있음' (조작 화면의 연결된 출력 수)
 *  - fetch('/api/ack') · 영상 알림('/api/command' videoMeta · videoEnded) → 같은 채널
 *  - 두 번 누르기 · F11 = 전체 화면 (브라우저는 사용자가 누른 뒤에만 전체 화면을 허락한다)
 */
(function () {
  'use strict';
  var CHURCH = '';
  var nativeFetch = window.fetch.bind(window);
  var bc; try { bc = new BroadcastChannel('wp-live'); } catch (e) { return; }
  var id = Math.floor(Math.random() * 2147483647) + 1;
  var kind = /[?&]window=1/.test(location.search) ? 'window' : 'browser';
  var list = [];
  function post(m) { m.church = CHURCH; try { bc.postMessage(m); } catch (e) { /* 닫힘 */ } }
  function LocalEvents(path) {
    var self = this, target = document.createDocumentFragment();
    self.url = path; self.readyState = 1; self.onerror = null; self.onopen = null; self.onmessage = null;
    self.addEventListener = target.addEventListener.bind(target);
    self.removeEventListener = target.removeEventListener.bind(target);
    self.dispatchEvent = target.dispatchEvent.bind(target);
    self.client = new URL(path, location.href).searchParams.get('client') || 'main';
    self.emit = function (type, data) { self.dispatchEvent(new MessageEvent(type, { data: JSON.stringify(data) })); };
    self.close = function () { self.readyState = 2; list.splice(list.indexOf(self), 1); };
    list.push(self);
    setTimeout(function () {
      self.emit('hello', { id: id });
      if (self.onopen) self.onopen(new Event('open'));
      post({ t: 'sync' });
      post({ t: 'out', id: id, client: self.client, kind: kind });
    }, 0);
  }
  var NativeES = window.EventSource;
  window.EventSource = function (path, options) {
    var u = new URL(path, location.href);
    return u.origin === location.origin && /\/events$/.test(u.pathname) ? new LocalEvents(path) : new NativeES(path, options);
  };
  bc.onmessage = function (e) {
    var m = e.data;
    if (!m || (m.church || '') !== CHURCH) return;
    list.forEach(function (es) {
      if (m.t === 'render' && m.client === es.client) es.emit('render', m.data);
      else if (m.t === 'draw' && es.client !== 'stage') es.emit('draw', m.data);
    });
  };
  setInterval(function () { list.forEach(function (es) { post({ t: 'out', id: id, client: es.client, kind: kind }); }); }, 2000);
  window.addEventListener('pagehide', function () { post({ t: 'bye', id: id }); });
  function ok(body) { return Promise.resolve(new Response(JSON.stringify(body), { status: 200, headers: { 'content-type': 'application/json' } })); }
  window.fetch = function (input, init) {
    init = init || {};
    var u; try { u = new URL(typeof input === 'string' ? input : input.url, location.href); } catch (e) { return nativeFetch(input, init); }
    var method = String(init.method || 'GET').toUpperCase();
    if (u.origin === location.origin && method === 'POST' && typeof init.body === 'string') {
      var b; try { b = JSON.parse(init.body); } catch (e) { b = null; }
      if (b && /\/api\/ack$/.test(u.pathname)) { post({ t: 'out', id: id, client: (list[0] || {}).client || 'main', kind: kind, ack: b }); return ok({ ok: true }); }
      if (b && /\/api\/command$/.test(u.pathname) && (b.cmd === 'videoMeta' || b.cmd === 'videoEnded')) { post({ t: 'cmd', cmd: b.cmd, args: b.args }); return ok({ ok: true, result: null }); }
    }
    return nativeFetch(input, init);
  };
  // 전체 화면: 두 번 누르기 · F11 (출력 창에서 Esc 는 전체 화면 풀기)
  function full() { var d = document.documentElement; if (!document.fullscreenElement && d.requestFullscreen) d.requestFullscreen().catch(function () {}); else if (document.exitFullscreen) document.exitFullscreen().catch(function () {}); }
  window.addEventListener('dblclick', full);
  window.addEventListener('keydown', function (e) { if (e.key === 'F11') { e.preventDefault(); full(); } });
})();
