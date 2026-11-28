/* 워십프리젠터 웹판 오프라인 대비 (6단계) — 웹 조작 화면(/control/) 페이지 머리에만 끼워진다 (src/pages.mjs). docs/web/offline.md
 *  1. 서비스 워커(/sw.js) 등록 · 새 판 알기 · 사람이 눌렀을 때만 바꾸기 (예배 중에 저절로 바뀌지 않게)
 *  2. 조작 화면(wasm, web/store.rs)이 쓰는 보관함: IndexedDB 'wp-offline' / kv — 교회 문서 · 올릴 줄 · 받은 차례
 *  3. 인터넷 연결 알림 (online · offline 사건) · 자주 쓰는 그림 미리 받아 두기
 * window.wpOffline 로 내보낸다. 값은 모두 JSON 글로 주고받는다 (wasm 쪽이 serde_json 으로 읽음).
 */
(function () {
  'use strict';
  var CFG = window.WP_CLOUD || {};
  var S = { sw: 'none', update: false, version: '', online: navigator.onLine !== false, error: '', wake: 0 };
  var reg = null;

  // ---------- 보관함 (IndexedDB) ----------
  var dbp = null;
  function db() {
    return dbp || (dbp = new Promise(function (res, rej) {
      var r = indexedDB.open('wp-offline', 1);
      r.onupgradeneeded = function () { r.result.createObjectStore('kv'); };
      r.onsuccess = function () { res(r.result); };
      r.onerror = function () { dbp = null; rej(r.error); };
    }));
  }
  function tx(mode, f) {
    return db().then(function (d) {
      return new Promise(function (res, rej) {
        var t = d.transaction('kv', mode);
        var out = f(t.objectStore('kv'));
        // 다 쓰였을 때(complete) 끝낸다 — 보내기 전에 저장이 끝났다고 믿을 수 있게
        t.oncomplete = function () { res(out && 'result' in out ? out.result : 'ok'); };
        t.onerror = t.onabort = function () { rej(t.error || new Error('저장 실패')); };
      });
    });
  }
  var range = function (prefix) { return IDBKeyRange.bound(prefix, prefix + '￿'); };
  var kv = {
    get: function (k) { return tx('readonly', function (s) { return s.get(k); }).then(function (v) { return v == null ? null : v; }).catch(function () { return null; }); },
    /** 앞머리가 같은 것 모두 → '[[열쇠, 값], …]' */
    list: function (prefix) {
      return db().then(function (d) {
        return new Promise(function (res) {
          var out = [];
          var c = d.transaction('kv', 'readonly').objectStore('kv').openCursor(range(prefix));
          c.onsuccess = function () { var x = c.result; if (!x) return res(JSON.stringify(out)); out.push([x.key, x.value]); x.continue(); };
          c.onerror = function () { res(JSON.stringify(out)); };
        });
      }).catch(function () { return '[]'; });
    },
    /** '[["put", 열쇠, 값] | ["del", 열쇠], …]' 를 한 번에 (하나라도 실패하면 모두 안 됨) → 'ok' | 'error:…' */
    batch: function (ops) {
      var list = typeof ops === 'string' ? JSON.parse(ops) : ops;
      return tx('readwrite', function (s) {
        for (var i = 0; i < list.length; i++) {
          var o = list[i];
          if (o[0] === 'put') s.put(o[2], o[1]); else if (o[0] === 'del') s.delete(o[1]);
        }
      }).then(function () { return 'ok'; }).catch(function (e) { return 'error:' + (e && e.name || e); });
    },
    clear: function (prefix) { return tx('readwrite', function (s) { s.delete(range(prefix)); }).then(function () { return 'ok'; }).catch(function () { return 'error'; }); },
  };

  // ---------- 서비스 워커 ----------
  function watch(r) {
    reg = r;
    var mark = function () {
      S.update = !!(r.waiting && navigator.serviceWorker.controller);
      S.sw = r.active ? 'ready' : r.installing ? 'installing' : 'none';
    };
    mark();
    r.addEventListener('updatefound', function () {
      var w = r.installing;
      if (w) w.addEventListener('statechange', mark);
      mark();
    });
    if (r.active) ask(r.active);
    navigator.serviceWorker.addEventListener('controllerchange', function () { if (r.active) ask(r.active); mark(); });
    setInterval(mark, 2000);
  }
  function ask(w) {
    try {
      var ch = new MessageChannel();
      ch.port1.onmessage = function (e) { S.version = (e.data && e.data.version) || ''; };
      w.postMessage({ t: 'version' }, [ch.port2]);
    } catch (e) { /* 무시 */ }
  }
  var SW_ON = 'serviceWorker' in navigator && CFG.authed && CFG.role !== 'demo';
  function register() {
    // 끊긴 채 열면 /sw.js 를 받지 못해 등록이 실패한다 → 이미 깔린 것을 쓰고, 다시 이어지면 다시 등록(새 판 확인)
    return navigator.serviceWorker.register('/sw.js', { scope: '/', updateViaCache: 'none' }).then(function (r) { if (r !== reg) watch(r); S.error = ''; }).catch(function (e) {
      S.error = String(e && e.message || e);
      return navigator.serviceWorker.getRegistration('/').then(function (r) { if (r && !reg) watch(r); });
    });
  }
  function check() {
    if (!SW_ON) return;
    if (reg) reg.update().catch(function () { /* 인터넷 없음 */ });
    else register();
  }
  if (SW_ON) {
    register();
    setInterval(check, 30 * 60 * 1000); // 30분마다 새 판이 있나 (바꾸지는 않는다)
  }

  window.addEventListener('online', function () { S.online = true; S.wake++; check(); });
  window.addEventListener('offline', function () { S.online = false; });

  /** 새 판으로 바꾸고 다시 연다 — 사람이 눌렀을 때만 */
  function applyUpdate() {
    if (!reg || !reg.waiting) return false;
    // 사람이 이미 확인했다 → 송출 중 떠나기 물음(index.html beforeunload)을 한 번 건너뛴다
    window.wpIsLive = function () { return false; };
    navigator.serviceWorker.addEventListener('controllerchange', function () { location.reload(); }, { once: true });
    reg.waiting.postMessage({ t: 'skipWaiting' });
    return true;
  }

  /** 자주 쓰는 그림을 미리 받아 둔다 (서비스 워커가 담음). '["/api/v1/assets/…", …]' */
  function prefetch(list) {
    var urls = typeof list === 'string' ? JSON.parse(list) : list;
    if (!navigator.serviceWorker || !navigator.serviceWorker.controller || !urls.length) return;
    var i = 0;
    var next = function () {
      if (i >= urls.length) return;
      var u = urls[i++];
      fetch(u, { credentials: 'same-origin' }).then(function (r) { return r.arrayBuffer(); }).catch(function () {}).then(next);
    };
    for (var k = 0; k < 3; k++) next();
  }

  window.wpOffline = {
    kv: kv,
    state: function () { return JSON.stringify({ sw: S.sw, update: S.update, version: S.version, online: S.online, error: S.error, wake: S.wake, controlled: !!(navigator.serviceWorker && navigator.serviceWorker.controller) }); },
    applyUpdate: applyUpdate,
    check: check,
    prefetch: prefetch,
  };
})();
