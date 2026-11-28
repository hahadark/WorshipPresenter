/* 워십프리젠터 웹판 어댑터 — 클라우드에서 열 때만 페이지 머리에 끼워진다 (설치판이 여는 페이지에는 없다).
 * web/ 의 출력 · 무대 · 리모컨 · 편집기 페이지를 고치지 않고 클라우드에 붙이는 얇은 층:
 *  1. EventSource('/events?client=…') → 같은 주소의 WebSocket (render · draw · hello 이벤트 모양은 설치판 SSE 와 같다)
 *  2. fetch('/api/ack') · 영상 알림 → 그 WebSocket 으로 (요청 수를 쓰지 않는다)
 *  3. 리모컨의 1초마다 fetch('/r/state') → 실시간으로 받아 둔 상태로 바로 답한다 (요청 수를 쓰지 않는다)
 *  4. 같은 PC 의 웹 조작 화면이 BroadcastChannel('wp-live') 로 보내는 화면도 받는다 (인터넷이 끊겨도 같은 PC 에서는 송출)
 *  5. 쓰기 요청에 X-WorshipCue 머리글
 * 계약: docs/web/architecture.md
 */
(function () {
  'use strict';
  var CFG = window.WP_CLOUD || {};
  var NativeES = window.EventSource;
  var nativeFetch = window.fetch.bind(window);
  var root = document.documentElement;
  var setState = function (s) { root.dataset.cloud = s; };
  setState('connecting');

  // 로그인이 되어 있으면 페이지 자체의 PIN 칸을 건너뛰게 자리만 채운다 (진짜 확인은 쿠키로 한다)
  if (CFG.authed) {
    try {
      if (CFG.page === 'remote') sessionStorage.setItem('wc-pin', 'cloud');
      if (CFG.page === 'editor') sessionStorage.setItem('wp-edit-pin', 'cloud');
    } catch (e) { /* 저장 안 됨 */ }
  }

  function wsUrl(path) {
    var u = new URL(path, location.href);
    u.protocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
    return u.href;
  }

  /** 끊기면 스스로 다시 잇는 실시간 연결 (1초 → 2초 → … 최대 15초, 조금씩 어긋나게) */
  function Link(path, onMessage, onState) {
    var self = this;
    self.ws = null; self.closed = false; self.delay = 1000; self.timer = 0; self.beat = 0; self.lastSeen = 0;
    function open() {
      if (self.closed) return;
      var ws = self.ws = new WebSocket(wsUrl(path));
      ws.onopen = function () {
        self.delay = 1000; self.lastSeen = Date.now(); onState('open');
        clearInterval(self.beat);
        // 25초마다 살아 있는지 묻는다. 답이 60초 없으면 끊고 다시 잇는다 (절전에서 깬 노트북 · 바뀐 Wi-Fi)
        self.beat = setInterval(function () {
          if (Date.now() - self.lastSeen > 60000) { try { ws.close(); } catch (e) { /* 무시 */ } return; }
          try { ws.send('ping'); } catch (e) { /* 무시 */ }
        }, 25000);
      };
      ws.onmessage = function (e) {
        self.lastSeen = Date.now();
        if (e.data === 'pong' || typeof e.data !== 'string') return;
        var m; try { m = JSON.parse(e.data); } catch (err) { return; }
        if (m && m.t) onMessage(m);
      };
      ws.onclose = function (ev) {
        clearInterval(self.beat);
        if (self.closed) return;
        // 열쇠를 지웠거나 로그인이 끝났으면(4003) 다시 잇지 않고 로그인 화면으로
        if (ev && ev.code === 4003) { self.closed = true; location.replace('/login?next=' + encodeURIComponent(location.pathname + location.search)); return; }
        onState('closed');
        self.timer = setTimeout(open, self.delay + Math.random() * 500);
        self.delay = Math.min(self.delay * 2, 15000);
      };
      ws.onerror = function () { try { ws.close(); } catch (e) { /* 무시 */ } };
    }
    self.send = function (obj) {
      if (!self.ws || self.ws.readyState !== 1) return false;
      try { self.ws.send(JSON.stringify(obj)); return true; } catch (e) { return false; }
    };
    self.isOpen = function () { return !!self.ws && self.ws.readyState === 1; };
    self.close = function () { self.closed = true; clearTimeout(self.timer); clearInterval(self.beat); try { self.ws && self.ws.close(); } catch (e) { /* 무시 */ } };
    open();
  }

  // ---------- 1. EventSource('/events') ----------
  var links = []; // 열린 출력 연결 (ack · 영상 알림을 보낼 곳)
  function CloudEvents(path) {
    var self = this;
    var target = document.createDocumentFragment(); // EventTarget 대신 (옛 브라우저 소스에서도 되게)
    self.url = path; self.readyState = 0; self.onerror = null; self.onopen = null; self.onmessage = null;
    self.addEventListener = target.addEventListener.bind(target);
    self.removeEventListener = target.removeEventListener.bind(target);
    self.dispatchEvent = target.dispatchEvent.bind(target);
    self.client = new URL(path, location.href).searchParams.get('client') || 'main';
    function emit(type, data) {
      var ev = new MessageEvent(type, { data: JSON.stringify(data) });
      self.dispatchEvent(ev);
    }
    self.emit = emit;
    self.link = new Link(path, function (m) {
      if (m.t === 'hello') { setState(m.online ? 'online' : 'offline'); emit('hello', { id: m.id }); }
      else if (m.t === 'render' || m.t === 'draw') { emit(m.t, m.data); if (m.t === 'render') try { window.dispatchEvent(new CustomEvent('wp-cloud-render', { detail: m.data })); } catch (e) { /* 옛 브라우저 */ } }
      else if (m.t === 'rtc') hooks.forEach(function (f) { try { f(m); } catch (e) { /* 무시 */ } });
      else if (m.t === 'status') setState(m.online ? 'online' : 'offline');
    }, function (state) {
      self.readyState = state === 'open' ? 1 : 0;
      if (state === 'open') { if (self.onopen) self.onopen(new Event('open')); }
      else { setState('connecting'); var ev = new Event('error'); self.dispatchEvent(ev); if (self.onerror) self.onerror(ev); }
    });
    self.close = function () { self.readyState = 2; self.link.close(); links.splice(links.indexOf(self), 1); };
    links.push(self);
  }
  window.EventSource = function (path, options) {
    var u = new URL(path, location.href);
    return u.origin === location.origin && u.pathname === '/events' ? new CloudEvents(path) : new NativeES(path, options);
  };
  window.EventSource.CONNECTING = 0; window.EventSource.OPEN = 1; window.EventSource.CLOSED = 2;

  // 4단계 B 미디어(cloud/client/media/media.js)가 쓰는 길: 출력 화면의 실시간 연결로 휴대폰 카메라 신호를 주고받는다
  var hooks = [];
  window.WPCloud = {
    send: function (obj) { return links.some(function (es) { return es.link.send(obj); }); },
    on: function (f) { hooks.push(f); },
  };

  // ---------- 4. 같은 PC 의 웹 조작 화면 (BroadcastChannel) ----------
  // 메시지 모양은 실시간 연결과 같다: { t:'render', client:'main', data } · { t:'draw', data }. church 가 있으면 같은 교회 것만.
  try {
    var bc = new BroadcastChannel('wp-live');
    bc.onmessage = function (e) {
      var m = e.data;
      if (!m || (m.church && CFG.church && m.church !== CFG.church)) return;
      links.forEach(function (es) {
        if (m.t === 'render' && m.client === es.client) es.emit('render', m.data);
        else if (m.t === 'draw' && es.client !== 'stage') es.emit('draw', m.data);
      });
    };
    // 새로 연 출력 화면이 '지금 화면을 달라' 고 알린다 (조작 화면이 듣고 다시 보낸다)
    window.addEventListener('load', function () { try { bc.postMessage({ t: 'sync', church: CFG.church || '' }); } catch (e) { /* 무시 */ } });
    // (3단계 덧붙임) 같은 PC 의 웹 조작 화면에 '살아 있음' 을 알린다 — 인터넷이 끊겨도 조작 화면이 연결된 출력 수를 안다.
    // cloud:true = 클라우드로도 붙은 화면 (조작 화면은 클라우드 연결 중이면 중계실의 viewers 로 세고 이것은 세지 않는다)
    var outId = 1 + Math.floor(Math.random() * 2147483646);
    setInterval(function () { links.forEach(function (es) { try { bc.postMessage({ t: 'out', id: outId, client: es.client, kind: 'browser', cloud: es.link.isOpen(), church: CFG.church || '' }); } catch (e) { /* 무시 */ } }); }, 2000);
    window.addEventListener('pagehide', function () { try { bc.postMessage({ t: 'bye', id: outId, church: CFG.church || '' }); } catch (e) { /* 무시 */ } });
  } catch (e) { /* BroadcastChannel 없음 */ }

  // ---------- 3. 리모컨 상태를 실시간으로 ----------
  var remote = { link: null, state: null, at: 0 };
  function remoteStart() {
    if (remote.link || CFG.page !== 'remote') return;
    remote.link = new Link('/events?client=remote', function (m) {
      if (m.t === 'remote') { remote.state = m.data; remote.at = Date.now(); }
      else if (m.t === 'hello' || m.t === 'status') { setState(m.online ? 'online' : 'offline'); if (!m.online) remote.state = null; }
    }, function (state) { if (state !== 'open') remote.state = null; });
  }
  if (CFG.authed) remoteStart();
  /** 받아 둔 상태로 답을 만든다. 타이머는 받은 뒤 흐른 만큼 줄여서 (1초마다 새로 받던 것과 같게 보이도록) */
  function remoteAnswer() {
    var s = JSON.parse(JSON.stringify(remote.state));
    var t = s.stage && s.stage.timer;
    if (t && t.running) { var gone = Date.now() - remote.at; t.remaining -= gone; t.now += gone; }
    s.ok = true;
    return new Response(JSON.stringify(s), { status: 200, headers: { 'content-type': 'application/json' } });
  }

  // ---------- 2 · 5. fetch ----------
  function ok(body) { return Promise.resolve(new Response(JSON.stringify(body), { status: 200, headers: { 'content-type': 'application/json' } })); }
  window.fetch = function (input, init) {
    init = init || {};
    var u;
    try { u = new URL(typeof input === 'string' ? input : input.url, location.href); } catch (e) { return nativeFetch(input, init); }
    if (u.origin !== location.origin) return nativeFetch(input, init);
    var method = String(init.method || (typeof input !== 'string' && input.method) || 'GET').toUpperCase();
    if (method === 'POST' && (u.pathname === '/api/ack' || u.pathname === '/api/command') && typeof init.body === 'string' && links.length) {
      var b; try { b = JSON.parse(init.body); } catch (e) { b = null; }
      var es = links[0];
      if (b && u.pathname === '/api/ack' && es.link.send({ t: 'ack', version: b.version, width: b.width, height: b.height, visible: b.visible })) return ok({ ok: true });
      if (b && u.pathname === '/api/command' && (b.cmd === 'videoMeta' || b.cmd === 'videoEnded') && es.link.send({ t: 'cmd', cmd: b.cmd, args: b.args })) return ok({ ok: true, result: null });
      // (3단계 덧붙임) 클라우드가 끊겼으면 영상 알림을 같은 PC 의 웹 조작 화면에 (영상 끝 → 자동 넘김이 인터넷 없이도)
      if (b && u.pathname === '/api/command' && (b.cmd === 'videoMeta' || b.cmd === 'videoEnded') && bc) { try { bc.postMessage({ t: 'cmd', cmd: b.cmd, args: b.args, church: CFG.church || '' }); return ok({ ok: true, result: null }); } catch (e) { /* 무시 */ } }
    }
    if (method === 'GET' && u.pathname === '/r/state') {
      if (remote.link && remote.link.isOpen() && remote.state) return Promise.resolve(remoteAnswer());
      // 아직 실시간 상태가 없으면 서버에 묻는다 (PIN 로그인도 이 길로 된다). 되면 실시간 연결을 연다
      return nativeFetch(input, init).then(function (res) { if (res.ok) remoteStart(); return res; });
    }
    if (method !== 'GET' && method !== 'HEAD') {
      var h = new Headers(init.headers || (typeof input !== 'string' ? input.headers : undefined));
      h.set('X-WorshipCue', '1');
      init = Object.assign({}, init, { headers: h });
    }
    return nativeFetch(input, init);
  };
})();
