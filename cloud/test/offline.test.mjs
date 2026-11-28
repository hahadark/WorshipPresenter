// 6단계 오프라인: 서비스 워커(client/offline/sw.js)의 담는 규칙과 조작 화면 페이지에 끼우는 것 (docs/web/offline.md)
// 브라우저 없이: 가짜 self · caches · fetch 위에서 sw.js 를 그대로 돌린다.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import vm from 'node:vm';

const here = dirname(fileURLToPath(import.meta.url));
const SRC = readFileSync(join(here, '..', 'client', 'offline', 'sw.js'), 'utf8');
const ORIGIN = 'https://wp.example';

class FakeCache {
  constructor() { this.m = new Map(); }
  key(r, o = {}) { const u = new URL(typeof r === 'string' ? r : r.url, ORIGIN); return o.ignoreSearch ? u.origin + u.pathname : u.href; }
  async match(r, o) { if (o && o.ignoreSearch) { for (const [k, v] of this.m) if (this.key(k, o) === this.key(r, o)) return v.clone(); return undefined; } const v = this.m.get(this.key(r)); return v ? v.clone() : undefined; }
  async put(r, res) { this.m.set(this.key(r), res); }
  async delete(r) { return this.m.delete(this.key(r)); }
  async keys() { return [...this.m.keys()].map(u => ({ url: u })); }
}

function load({ version = 'v1', precache = ['/control/wp-app.js', '/outputs/output.js'], active = false, net } = {}) {
  const store = new Map();
  const caches = {
    async open(n) { if (!store.has(n)) store.set(n, new FakeCache()); return store.get(n); },
    async keys() { return [...store.keys()]; },
    async delete(n) { return store.delete(n); },
  };
  const listeners = {};
  const calls = [];
  const self = {
    location: new URL(ORIGIN + '/sw.js'),
    registration: { active: active ? {} : null },
    clients: { claim: async () => { calls.push('claim'); } },
    skipWaiting: async () => { calls.push('skipWaiting'); },
    addEventListener: (t, f) => { listeners[t] = f; },
  };
  const fetch = async (req, init) => {
    const url = new URL(typeof req === 'string' ? req : req.url, ORIGIN);
    calls.push('fetch ' + url.pathname + url.search);
    return net(url, req, init);
  };
  const code = SRC.replace("'__WP_VERSION__'", JSON.stringify(version)).replace('= __WP_PRECACHE__;', '= ' + JSON.stringify(precache) + ';');
  vm.runInNewContext(code, { self, caches, fetch, Response, Request, Headers, URL, AbortController, Blob, Set, Map, Promise, setTimeout, clearTimeout, console, Array });
  const fire = async (type, ev = {}) => {
    let waited = null, responded = null;
    const e = { ...ev, waitUntil: p => { waited = p; }, respondWith: p => { responded = p; } };
    listeners[type](e);
    if (waited) await waited;
    return responded ? await responded : undefined;
  };
  const request = (path, { mode = 'no-cors', method = 'GET', headers = {} } = {}) => ({ url: ORIGIN + path, method, mode, headers: new Headers(headers) });
  return { store, caches, calls, fire, request };
}

const html = (body = 'page') => new Response(`<html>${body}</html>`, { status: 200, headers: { 'content-type': 'text/html; charset=utf-8' } });
const ok = (body = 'x', type = 'application/javascript') => new Response(body, { status: 200, headers: { 'content-type': type } });
// Node 의 Response 는 type 이 'default' — 브라우저의 같은 곳 응답('basic')처럼
const basic = r => { Object.defineProperty(r, 'type', { value: 'basic' }); return r; };
const down = () => { throw new TypeError('Failed to fetch'); };

test('처음 깔기: 앱 껍데기를 판 이름으로 담고 페이지도 담음 · 기다리지 않음', async () => {
  const sw = load({ net: (u) => basic(u.pathname.endsWith('/') || u.pathname.startsWith('/outputs/main') ? html(u.pathname) : ok(u.pathname)) });
  await sw.fire('install');
  const shell = sw.store.get('wp-shell-v1');
  assert.ok(shell);
  assert.equal((await shell.keys()).length, 2);
  assert.ok(await (await sw.caches.open('wp-pages-v1')).match('/control/'));
  assert.ok(sw.calls.includes('skipWaiting'), '처음에는 바로');
});

test('새 판: 기다림 (사람이 누를 때 skipWaiting) · 바뀌면 옛 판 지움', async () => {
  const sw = load({ version: 'v2', active: true, net: () => basic(ok()) });
  sw.store.set('wp-shell-v1', new FakeCache());
  await sw.fire('install');
  assert.ok(!sw.calls.includes('skipWaiting'), '저절로 바꾸지 않는다');
  assert.ok(sw.store.has('wp-shell-v1'), '깔기만 해서는 옛 판을 지우지 않는다 (지금 화면이 쓰는 중)');
  await sw.fire('message', { data: { t: 'skipWaiting' } });
  assert.ok(sw.calls.includes('skipWaiting'));
  await sw.fire('activate');
  assert.ok(!sw.store.has('wp-shell-v1') && sw.store.has('wp-shell-v2'));
});

test('페이지: 인터넷 먼저, 끊기면 담아 둔 것 · 주소 뒤 ?debug 는 같은 페이지', async () => {
  let online = true;
  const sw = load({ net: () => (online ? basic(html('new')) : down()) });
  const r1 = await sw.fire('fetch', { request: sw.request('/control/?debug=1', { mode: 'navigate' }) });
  assert.match(await r1.text(), /new/);
  online = false;
  const r2 = await sw.fire('fetch', { request: sw.request('/control/', { mode: 'navigate' }) });
  assert.equal(r2.status, 200);
  assert.match(await r2.text(), /new/);
  // 담아 둔 것이 없으면 안내 페이지
  const r3 = await sw.fire('fetch', { request: sw.request('/outputs/stage', { mode: 'navigate' }) });
  assert.equal(r3.status, 503);
  assert.match(await r3.text(), /연결되어 있지 않/);
});

test('페이지: 로그인이 끝나 302 면 담아 둔 것 대신 그대로 (로그인 화면으로)', async () => {
  let mode = 'ok';
  const sw = load({ net: () => (mode === 'ok' ? basic(html()) : basic(new Response(null, { status: 302, headers: { location: '/login' } }))) });
  await sw.fire('fetch', { request: sw.request('/control/', { mode: 'navigate' }) });
  mode = 'login';
  const r = await sw.fire('fetch', { request: sw.request('/control/', { mode: 'navigate' }) });
  assert.equal(r.status, 302);
});

test('앱 껍데기는 담아 둔 것 먼저 (인터넷을 부르지 않음)', async () => {
  const sw = load({ net: () => basic(ok('net')) });
  await sw.fire('install');
  const n = sw.calls.length;
  const r = await sw.fire('fetch', { request: sw.request('/control/wp-app.js?x=1') });
  assert.equal(await r.text(), 'net');
  assert.equal(sw.calls.length, n, '담아 둔 것에서');
});

test('교회 그림은 담고 · 영상 Range 와 자료 API 는 손대지 않음 · fonts.css 는 인터넷 먼저', async () => {
  let online = true;
  const sw = load({ net: (u) => (!online ? down() : basic(u.pathname.endsWith('.css') ? ok('css', 'text/css') : ok('png', 'image/png'))) });
  const img = await sw.fire('fetch', { request: sw.request('/api/v1/assets/a1?e=.png') });
  assert.equal(await img.text(), 'png');
  await new Promise(r => setTimeout(r, 5));
  online = false;
  const again = await sw.fire('fetch', { request: sw.request('/api/v1/assets/a1?e=.png') });
  assert.equal(await again.text(), 'png', '끊겨도 담아 둔 그림');
  assert.equal(await sw.fire('fetch', { request: sw.request('/api/v1/assets/v1?e=.mp4', { headers: { range: 'bytes=0-' } }) }), undefined, '영상(Range)은 그대로 보냄');
  assert.equal(await sw.fire('fetch', { request: sw.request('/api/v1/sync?after=0') }), undefined, '자료 API 는 웹 조작 화면이 IndexedDB 로');
  assert.equal(await sw.fire('fetch', { request: sw.request('/api/v1/documents/cue/x', { method: 'PUT' }) }), undefined);
  online = true;
  await sw.fire('fetch', { request: sw.request('/api/v1/assets/fonts.css') });
  online = false;
  const css = await sw.fire('fetch', { request: sw.request('/api/v1/assets/fonts.css') });
  assert.equal(await css.text(), 'css');
});

test('나가기(POST /auth/logout): 담아 둔 페이지 · 교회 파일을 지움 (앱 껍데기는 남김)', async () => {
  const sw = load({ net: () => basic(html()) });
  await sw.fire('install');
  await (await sw.caches.open('wp-files-v1')).put('/api/v1/assets/a', ok());
  const r = await sw.fire('fetch', { request: sw.request('/auth/logout', { method: 'POST' }) });
  assert.equal(r, undefined, '요청은 그대로 보냄');
  assert.ok(!sw.store.has('wp-pages-v1') && !sw.store.has('wp-files-v1'));
  assert.ok(sw.store.has('wp-shell-v1'));
});

test('조작 화면 페이지에만 offline.js (둘러보기 · 로그인 전은 아님)', async () => {
  const { offlineTag } = await import('../src/offline.mjs');
  assert.match(offlineTag({ authed: true, role: 'owner' }), /\/cloud\/offline\/offline\.js/);
  assert.match(offlineTag({ authed: true, role: 'member-editor' }), /offline\.js/);
  assert.equal(offlineTag({ authed: true, role: 'demo' }), '');
  assert.equal(offlineTag({ authed: false }), '');
  const pages = readFileSync(join(here, '..', 'src', 'pages.mjs'), 'utf8');
  assert.match(pages, /name === 'control'.*offlineTag\(cfg\)/, '조작 화면 페이지에 끼운다');
});
