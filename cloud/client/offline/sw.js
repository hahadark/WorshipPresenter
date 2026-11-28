/* 워십프리젠터 웹판 서비스 워커 (6단계 오프라인) — docs/web/offline.md
 * 이 파일은 틀이다. `node scripts/assets.mjs` 가 판 이름과 담을 파일 목록을 아래 두 자리에 채워 public/sw.js 로 만든다.
 *
 * 무엇을 담나
 *  - 앱 껍데기 (판마다 따로 'wp-shell-<판>'): /control/* (wasm · js · 글꼴) · 출력 페이지의 js (/outputs/*.js · /shared/*) ·
 *    /cloud/* (어댑터 · 미디어 부품 · pdf.js 약 5 MB). 먼저 담아 둔 것에서 바로 준다 (cache-first).
 *  - 페이지 HTML (/control/ · /outputs/main · overlay · stage · out-…): 늘 인터넷 먼저, 닿지 않으면 마지막으로 받은 것 (network-first).
 *    로그인 · 교회 설정이 페이지 머리에 들어 있어 판으로 묶지 않는다.
 *  - 교회 파일 /api/v1/assets/{id} 의 그림 · 글꼴 (id 는 내용이 바뀌지 않는다 → 담아 둔 것 먼저). 영상(Range 요청)은 담지 않는다.
 * 그 밖의 요청(자료 API · 로그인 · 실시간 연결)은 손대지 않는다 — 웹 조작 화면이 IndexedDB 로 따로 다룬다.
 *
 * 새 판: 브라우저가 이 파일이 바뀐 것을 알면 새 판을 뒤에서 담고 '기다림' 으로 둔다. 예배 중에 저절로 바뀌지 않는다.
 * 조작 화면의 '새 버전 있음 → 바꾸기' 를 누르면(offline.js applyUpdate) 그때 바꾸고 페이지를 다시 연다.
 * 모든 창을 닫았다 다시 열 때도 브라우저가 새 판으로 바꾼다 (그때는 예배 중이 아니다).
 */
'use strict';
const VERSION = '__WP_VERSION__';
const PRECACHE = __WP_PRECACHE__;
const SHELL = 'wp-shell-' + VERSION;
const PAGES = 'wp-pages-v1';
const FILES = 'wp-files-v1';
const FILES_MAX = 400; // 교회 파일 담는 개수 (넘으면 오래된 것부터)
const FILE_BYTES = 15 * 1024 * 1024; // 하나 15 MB 넘는 것(큰 영상 등)은 담지 않는다
const NAV_WAIT = 4000; // 페이지를 인터넷에서 기다리는 시간 (담아 둔 것이 있을 때만 — 그 뒤 담아 둔 것)
const PAGE_RE = /^\/(control\/|outputs\/(main|overlay|stage|out-[A-Za-z0-9_-]{1,40}))$/;
const SHELL_SET = new Set(PRECACHE);
// 처음 담을 때 같이 담는 페이지 (지금 로그인으로 — 나중에 오프라인으로 새로 고쳐도 뜨게)
const FIRST_PAGES = ['/control/', '/outputs/main', '/outputs/overlay', '/outputs/stage'];

// 파일 하나 받기: 멈춘 채 끝나지 않는 받기(느린 망 · 반쯤 끊김)를 기다리지 않게 시간을 두고, 세 번까지 다시
const FILE_WAIT = 90000;
async function getOne(cache, p) {
  for (let n = 1; ; n++) {
    const ac = new AbortController();
    const t = setTimeout(() => ac.abort(), FILE_WAIT);
    try {
      // 담는 내용이 판 이름과 맞게 HTTP 캐시를 거치지 않고 받는다. 본문까지 다 받은 뒤 담는다(받다 멈추면 시간 초과)
      const res = await fetch(p, { cache: 'no-cache', credentials: 'same-origin', signal: ac.signal });
      if (!res.ok) throw new Error(p + ' ' + res.status);
      const body = await res.blob();
      await cache.put(p, new Response(body, { status: res.status, statusText: res.statusText, headers: res.headers }));
      return;
    } catch (e) {
      PROGRESS.retries++;
      console.warn('[sw] 담기 실패', p, n, String(e));
      if (n >= 3) throw e;
    } finally { clearTimeout(t); }
  }
}

// 담는 중 얼마나 했나 (점검 · 조작 화면이 물을 수 있게 — message {t:'version'} 답에 같이)
const PROGRESS = { stage: 'script', done: 0, total: PRECACHE.length, now: [], retries: 0 };
async function putAll(cache, list) {
  let i = 0;
  const worker = async () => {
    while (i < list.length) {
      const p = list[i++];
      PROGRESS.now.push(p);
      await getOne(cache, p);
      PROGRESS.done++;
      PROGRESS.now = PROGRESS.now.filter(x => x !== p);
    }
  };
  await Promise.all(Array.from({ length: 4 }, worker));
}

async function cachePage(path) {
  try {
    const res = await fetch(path, { credentials: 'same-origin', redirect: 'manual', cache: 'no-store' });
    if (okPage(res)) await (await caches.open(PAGES)).put(path, res);
  } catch (e) { /* 인터넷 없음 · 권한 없음 — 다음에 열 때 담는다 */ }
}

function okPage(res) {
  return res && res.status === 200 && res.type === 'basic' && !res.redirected && /text\/html/.test(res.headers.get('content-type') || '');
}

self.addEventListener('install', (e) => {
  PROGRESS.stage = 'install';
  PROGRESS.at = Date.now();
  e.waitUntil((async () => {
    // 새 캐시 열기가 끝나지 않으면(저장소가 바쁨) 45초에 이 설치를 버린다 → 지금 판은 그대로 돌고, 페이지(offline.js)가 15초 뒤 다시 새 판을 받는다
    const cache = await Promise.race([caches.open(SHELL), new Promise((_, rej) => setTimeout(() => { PROGRESS.stage = 'open-timeout'; rej(new Error('캐시 열기 45초 초과')); }, 45000))]);
    PROGRESS.stage = 'files';
    await putAll(cache, PRECACHE);
    PROGRESS.stage = 'pages';
    for (const p of FIRST_PAGES) await cachePage(p);
    PROGRESS.stage = 'done';
    // 처음 깔 때는 기다릴 것이 없다 (지금 쓰는 판이 없음). 새 판은 사람이 누를 때까지 기다린다
    if (!self.registration.active) await self.skipWaiting();
  })());
});

self.addEventListener('activate', (e) => {
  e.waitUntil((async () => {
    for (const k of await caches.keys()) if (k.startsWith('wp-shell-') && k !== SHELL) await caches.delete(k);
    await self.clients.claim();
  })());
});

self.addEventListener('message', (e) => {
  const m = e.data || {};
  if (m.t === 'skipWaiting') self.skipWaiting();
  else if (m.t === 'version' && e.ports && e.ports[0]) e.ports[0].postMessage({ version: VERSION, files: PRECACHE.length, progress: PROGRESS });
  else if (m.t === 'forget') e.waitUntil(Promise.all([caches.delete(PAGES), caches.delete(FILES)]));
});

self.addEventListener('fetch', (e) => {
  const req = e.request;
  const url = new URL(req.url);
  if (url.origin !== self.location.origin) return;
  // 나가기: 이 브라우저에 담아 둔 페이지 · 교회 파일을 지운다 (다음 사람이 오프라인에서 못 보게). 요청은 그대로 보낸다
  if (req.method === 'POST' && url.pathname === '/auth/logout') { e.waitUntil(Promise.all([caches.delete(PAGES), caches.delete(FILES)])); return; }
  if (req.method !== 'GET') return;
  const path = url.pathname;
  if (req.mode === 'navigate' && PAGE_RE.test(path)) { e.respondWith(page(e, path)); return; }
  if (SHELL_SET.has(path)) { e.respondWith(shell(req, path)); return; }
  if (path.startsWith('/api/v1/assets/') && !req.headers.has('range')) {
    if (path === '/api/v1/assets/fonts.css') e.respondWith(fresh(req));
    else e.respondWith(file(req));
  }
});

// 담을 때는 본문을 먼저 끝까지 받아(Blob) 두 응답을 따로 만든다 — res.clone() 으로 갈라 담으면, 한쪽(페이지)이 읽기를 그만둘 때
// 다른 쪽(cache.put)이 끝나지 않고 멈춰 이 사이트의 캐시 일 전체(새 판 담기의 caches.open 포함)가 막히는 것을 봤다 (2026-11-28, 끊김 시험)
async function settle(res) {
  const body = await res.blob();
  const init = { status: res.status, statusText: res.statusText, headers: res.headers };
  return [new Response(body, init), new Response(body, init)];
}

/** 페이지: 인터넷 먼저 (로그인이 끝났으면 서버의 302 를 그대로), 닿지 않으면 담아 둔 것 */
async function page(e, path) {
  const cache = await caches.open(PAGES);
  const kept = await cache.match(path);
  const net = fetch(e.request).then(async (res) => {
    if (!okPage(res)) return res;
    const [give, keep] = await settle(res);
    e.waitUntil(cache.put(path, keep).catch(() => {}));
    return give;
  });
  net.catch(() => {}); // 늦게 실패해도 (이미 담아 둔 것을 줬으면) 조용히
  try {
    if (!kept) return await net;
    // 담아 둔 것이 있으면 인터넷을 너무 오래 기다리지 않는다 (느린 망 · 반쯤 끊김)
    return await Promise.race([net, new Promise((_, rej) => setTimeout(() => rej(new Error('늦음')), NAV_WAIT))]).then((res) => (res.status >= 502 && res.status <= 504 ? kept : res));
  } catch (err) {
    if (kept) return kept;
    return new Response('<!doctype html><meta charset="utf-8"><title>연결 끊김</title><body style="font:15px sans-serif;background:#17181b;color:#c9cdd3;padding:40px">인터넷에 연결되어 있지 않고, 이 브라우저에 담아 둔 화면도 없습니다. 연결된 뒤 한 번 열어 두면 다음부터는 끊겨도 열립니다.</body>', { status: 503, headers: { 'content-type': 'text/html; charset=utf-8' } });
  }
}

/** 앱 껍데기: 이 판에 담아 둔 것 먼저 */
async function shell(req, path) {
  const cache = await caches.open(SHELL);
  const hit = await cache.match(path, { ignoreSearch: true });
  if (hit) return hit;
  const res = await fetch(req);
  if (!(res.ok && res.type === 'basic')) return res;
  const [give, keep] = await settle(res);
  cache.put(path, keep).catch(() => {});
  return give;
}

/** fonts.css: 교회가 글꼴을 더하면 바뀐다 → 인터넷 먼저 */
async function fresh(req) {
  const cache = await caches.open(FILES);
  try {
    const res = await fetch(req);
    if (!res.ok) return res;
    const [give, keep] = await settle(res);
    cache.put(req.url, keep).catch(() => {});
    return give;
  } catch (err) {
    const hit = await cache.match(req.url);
    if (hit) return hit;
    throw err;
  }
}

/** 교회 파일(그림 · 글꼴): 담아 둔 것 먼저, 없으면 받아서 담는다 (영상 · 큰 파일은 담지 않음) */
async function file(req) {
  const cache = await caches.open(FILES);
  const hit = await cache.match(req.url);
  if (hit) return hit;
  const res = await fetch(req);
  const type = res.headers.get('content-type') || '';
  const len = +res.headers.get('content-length') || 0;
  if (!(res.status === 200 && res.type === 'basic' && /^(image\/|font\/|application\/font|application\/x-font|text\/css)/.test(type) && len <= FILE_BYTES)) return res;
  const [give, keep] = await settle(res);
  // 길이를 미리 몰랐던 것(나눠 보냄)도 다 받은 뒤 크기를 본다
  if ((await keep.clone().blob()).size <= FILE_BYTES) cache.put(req.url, keep).then(() => trim(cache)).catch(() => {});
  return give;
}

async function trim(cache) {
  const keys = await cache.keys();
  for (let i = 0; i < keys.length - FILES_MAX; i++) await cache.delete(keys[i]);
}
