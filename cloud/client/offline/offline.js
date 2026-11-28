/* 워십프리젠터 웹판 오프라인 대비 (6단계) — 웹 조작 화면(/control/) 페이지 머리에만 끼워진다 (src/pages.mjs). docs/web/offline.md
 *  1. 서비스 워커(/sw.js) 등록 · 새 판 알기 · 사람이 눌렀을 때만 바꾸기 (예배 중에 저절로 바뀌지 않게)
 *  2. 조작 화면(wasm, web/store.rs)이 쓰는 보관함: IndexedDB 'wp-offline' / kv — 교회 문서 · 올릴 줄 · 받은 차례
 *  3. 인터넷 연결 알림 (online · offline 사건) · 자주 쓰는 그림 미리 받아 두기
 * window.wpOffline 로 내보낸다. 값은 모두 JSON 글로 주고받는다 (wasm 쪽이 serde_json 으로 읽음).
 */
(function () {
  'use strict';
  var CFG = window.WP_CLOUD || {};
  var S = { log: [], sw: 'none', update: false, version: '', online: navigator.onLine !== false, error: '', wake: 0 };
  var reg = null;
  var retries = 0;

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
      S.log.push(Date.now() + ' found ' + (w && w.state));
      if (w) w.addEventListener('statechange', function () {
        S.log.push(Date.now() + ' ' + w.state);
        if (S.log.length > 20) S.log.shift();
        mark();
        // 새 판 담기가 실패하면(인터넷 · 캐시 저장소) 지금 판은 그대로 — 조금 뒤 다시 받아 본다 (다섯 번까지)
        if (w.state === 'redundant' && !r.waiting && retries < 5) { retries++; S.retries = retries; setTimeout(check, 15000); }
      });
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
    // 브라우저가 저장 공간이 모자랄 때 이 사이트 자료(못 올린 고친 것)를 지우지 않게 부탁한다 (묻는 창은 없음 — 허락 여부는 브라우저가 정함)
    try { if (navigator.storage && navigator.storage.persist) navigator.storage.persist().then(function (ok) { S.persisted = !!ok; }); } catch (e) { /* 없음 */ }
    setInterval(check, 30 * 60 * 1000); // 30분마다 새 판이 있나 (바꾸지는 않는다)
  }

  // 같은 브라우저에서 조작 화면 탭은 하나만 송출 · 저장 (Web Locks). 둘째 탭은 '보기 전용' 으로 기다리다가,
  // 앞 탭이 닫히면 잠금을 받아 다시 열려 이어받는다 (지금 화면 · 올릴 줄을 깨끗이 다시 읽으려고)
  S.tab = 'none';
  if (CFG.authed && CFG.page === 'control' && CFG.role !== 'demo') {
    var lockName = 'wp-control:' + (CFG.church || '');
    var hold = function () { return new Promise(function () { /* 탭이 닫힐 때까지 쥔다 */ }); };
    if (navigator.locks && navigator.locks.request) {
      S.tab = 'pending';
      navigator.locks.request(lockName, { ifAvailable: true }, function (l) {
        if (l) { S.tab = 'primary'; return hold(); }
        S.tab = 'secondary';
        navigator.locks.request(lockName, function () {
          S.tab = 'promoted';
          window.wpIsLive = function () { return false; }; // 보기 전용 탭은 송출하지 않았다 — 떠나기 물음 없이
          setTimeout(function () { location.reload(); }, 300);
          return hold();
        });
        return null;
      }).catch(function () { S.tab = 'primary'; });
    } else S.tab = 'primary'; // 잠금이 없는 브라우저 — 예전처럼
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

  /** 이 브라우저에 보관한 교회 자료를 지운다 — 교회 문서 · 올릴 줄 · 받은 차례(wp-offline) · 성경(wp-web) · 담아 둔 페이지 · 교회 파일 · 출력 마지막 화면.
   *  앱 껍데기(wp-shell-*, 공개 코드)는 남긴다. 다른 탭이 열고 있어도 되게 데이터베이스를 지우지 않고 안을 비운다 */
  function clearStore(name) {
    return new Promise(function (res) {
      var r = indexedDB.open(name, 1);
      r.onupgradeneeded = function () { r.result.createObjectStore('kv'); };
      r.onsuccess = function () {
        var d = r.result;
        try { var t = d.transaction('kv', 'readwrite'); t.objectStore('kv').clear(); t.oncomplete = t.onerror = t.onabort = function () { d.close(); res(); }; } catch (e) { d.close(); res(); }
      };
      r.onerror = function () { res(); };
    });
  }
  function forget() {
    var jobs = [clearStore('wp-offline'), clearStore('wp-web')];
    if (window.caches) jobs.push(caches.delete('wp-pages-v1'), caches.delete('wp-files-v1'));
    try { Object.keys(localStorage).forEach(function (k) { if (k.indexOf('wp-last:') === 0) localStorage.removeItem(k); }); } catch (e) { /* 저장소 없음 */ }
    return Promise.all(jobs.map(function (p) { return Promise.resolve(p).catch(function () {}); }));
  }
  /** 로그아웃 바로 전에 부른다 → true 면 로그아웃해도 된다. 못 올린 고친 것이 있으면 한 번 묻고, 지운다 */
  function beforeLogout() {
    return kv.list('o|').then(function (t) {
      var n = 0;
      try { n = JSON.parse(t).length; } catch (e) { /* 없음 */ }
      if (n > 0 && !window.confirm('올리지 않은 고친 것 ' + n + '개가 있습니다.\n\n로그아웃하면 이 브라우저에 보관한 교회 자료와 함께 지워져 교회 클라우드에 올라가지 않습니다.\n\n그래도 로그아웃할까요?\n(취소하면 그대로 두고, 조작 화면이 인터넷에 이어지면 올립니다.)')) return false;
      return forget().then(function () { return true; });
    });
  }

  window.wpOffline = {
    beforeLogout: beforeLogout,
    forget: forget,
    kv: kv,
    state: function () { return JSON.stringify({ sw: S.sw, update: S.update, version: S.version, online: S.online, error: S.error, wake: S.wake, persisted: !!S.persisted, installRetries: S.retries || 0, log: S.log, tab: S.tab, controlled: !!(navigator.serviceWorker && navigator.serviceWorker.controller) }); },
    applyUpdate: applyUpdate,
    check: check,
    prefetch: prefetch,
  };
})();
