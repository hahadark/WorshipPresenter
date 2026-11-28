// 6단계 끊김 시험: 실제 브라우저(Edge, 머리 없이)의 웹 조작 화면(/control/)을 인터넷이 끊겼다 이어지는 동안 돌려 본다.
//   node scripts/offline-e2e.mjs [--browser edge|chrome] [--out ../tmp/offline-e2e] [--only paid|free] [--keep]
// 준비: bash tools/web/build.sh (dist-web) — 이 시험이 assets.mjs 를 돌려 public/control 로 넣는다.
//
// 어떻게 끊나: 브라우저는 '끊을 수 있는 중계(TCP)' 를 거쳐 로컬 클라우드(wrangler dev)에 붙는다. 중계를 닫으면 브라우저만 끊기고(연결 거절),
//   그사이 '다른 곳'(이 스크립트)은 클라우드에 바로 붙어 같은 큐시트를 고칠 수 있다. 무료(가짜 구글 드라이브) · 유료(D1) 교회 둘 다.
// 시험마다 새 D1 · DO 자리(--persist-to 임시 폴더)와 빈 포트를 쓴다 — 다른 작업자의 wrangler dev · 자료와 겹치지 않는다. 127.0.0.1 밖으로 나가지 않는다.
//
// 확인하는 것 (docs/web/offline.md 6절)
//  1. 서비스 워커가 앱 껍데기(/control/* · 출력 페이지 · /cloud/*)를 판 이름으로 담음
//  2. 끊긴 동안: 고친 것이 올릴 줄(IndexedDB)에 · '연결 끊김 · 올릴 것 N개' · 같은 브라우저 출력 창은 계속 받음 · 다음(F6)은 줄에 안 들어감
//  3. 끊긴 채 새로 고침: 앱이 뜨고 · 고친 것이 남아 있고 · 마지막 자료로 송출 · 출력 창도 끊긴 채 새로 고쳐 다시 받음
//  4. 다시 이으면: 줄이 올라가고 · 그사이 다른 곳에서 고친 같은 큐시트는 '충돌 사본' 으로 둘 다 남고 · 상태 칸이 사라짐 ·
//     클라우드 출력은 쌓아 둔 조작이 아니라 지금 화면 하나를 받음
//  5. (무료) 구글 드라이브 연결이 끊기면 '저장 멈춤' — '연결 끊김' 과 섞이지 않음
//  6. (유료) 새 판이 나오면 '새 버전 있음' 만 뜨고 그대로 — 사람이 누르면 그때 바뀜
import { spawn, execFileSync } from 'node:child_process';
import { createHmac, randomBytes } from 'node:crypto';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import net from 'node:net';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import WebSocket from 'ws';
import { CLIENT_ID, CLIENT_SECRET, startFakeGoogle } from '../test/fake-google/server.mjs';

const cloud = join(dirname(fileURLToPath(import.meta.url)), '..');
const repo = join(cloud, '..');
const wrangler = join(cloud, 'node_modules', 'wrangler', 'bin', 'wrangler.js');
process.env.WRANGLER_SEND_METRICS = 'false';
const args = process.argv.slice(2);
const opt = (n, d) => { const i = args.indexOf('--' + n); return i >= 0 ? args[i + 1] : d; };
const which = opt('browser', 'edge');
const only = opt('only', '');
const out = resolve(opt('out', join(repo, 'tmp', 'offline-e2e')));
const EXE = { edge: 'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe', chrome: 'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe' }[which];
const sleep = ms => new Promise(r => setTimeout(r, ms));
const results = [];
let tierName = '';
const check = (name, ok, detail) => {
  results.push({ tier: tierName, name, ok: !!ok, detail });
  console.error(`${ok ? 'OK  ' : 'FAIL'} [${tierName}] ${name}${detail !== undefined ? ' → ' + JSON.stringify(detail).slice(0, 300) : ''}`);
};
const freePort = () => new Promise(res => { const s = net.createServer(); s.listen(0, '127.0.0.1', () => { const p = s.address().port; s.close(() => res(p)); }); });
const until = async (f, ms = 15000, step = 250) => { const t0 = Date.now(); for (;;) { let v; try { v = await f(); } catch { v = null; } if (v) return v; if (Date.now() - t0 > ms) return null; await sleep(step); } };

// ---------- 끊을 수 있는 중계 (브라우저 ↔ 클라우드) ----------
function cutter(listenPort, targetPort) {
  const socks = new Set();
  let server = null;
  const up = () => new Promise(res => {
    server = net.createServer(c => {
      const u = net.connect(targetPort, '127.0.0.1');
      socks.add(c); socks.add(u);
      c.pipe(u); u.pipe(c);
      const end = () => { c.destroy(); u.destroy(); socks.delete(c); socks.delete(u); };
      c.on('error', end); u.on('error', end); c.on('close', end); u.on('close', end);
    });
    server.listen(listenPort, '127.0.0.1', res);
  });
  const down = () => new Promise(res => { for (const s of socks) s.destroy(); socks.clear(); if (!server) return res(); server.close(() => res()); server = null; });
  return { up, down };
}

// ---------- 브라우저 (CDP, 탭마다 세션) ----------
async function browser() {
  const port = await freePort();
  const prof = join(out, `profile-${which}-${tierName}-${Date.now()}`);
  const proc = spawn(EXE, [`--remote-debugging-port=${port}`, `--user-data-dir=${prof}`, '--headless=new', '--no-first-run', '--force-device-scale-factor=1', '--window-size=1600,960', 'about:blank'], { stdio: 'ignore' });
  const info = await until(async () => (await fetch(`http://127.0.0.1:${port}/json/version`)).json(), 20000, 150);
  const ws = new WebSocket(info.webSocketDebuggerUrl, { perMessageDeflate: false, maxPayload: 256 * 1024 * 1024 });
  await new Promise(r => ws.once('open', r));
  let id = 0;
  const wait = new Map();
  const handlers = new Map();
  ws.on('message', d => {
    const m = JSON.parse(d.toString());
    if (m.id && wait.has(m.id)) { const { res, rej } = wait.get(m.id); wait.delete(m.id); m.error ? rej(new Error(m.error.message)) : res(m.result); return; }
    const h = handlers.get(m.sessionId || '');
    if (h) h(m);
  });
  const send = (method, params = {}, sessionId) => new Promise((res, rej) => { const i = ++id; wait.set(i, { res, rej }); ws.send(JSON.stringify({ id: i, method, params, ...(sessionId ? { sessionId } : {}) })); });
  // 탭마다 따로 창 — 머리 없는 브라우저는 뒤에 숨은 탭의 그리기(requestAnimationFrame)를 멈춰 egui 가 그리지도 · 누름을 받지도 않는다
  async function tab(name) {
    const { targetId } = await send('Target.createTarget', { url: 'about:blank', newWindow: true });
    const { sessionId } = await send('Target.attachToTarget', { targetId, flatten: true });
    const logs = [];
    handlers.set(sessionId, m => {
      if (m.method === 'Page.javascriptDialogOpening') send('Page.handleJavaScriptDialog', { accept: true }, sessionId).catch(() => {});
      else if (m.method === 'Runtime.consoleAPICalled') logs.push(m.params.args.map(a => a.value ?? a.description).join(' '));
      else if (m.method === 'Runtime.exceptionThrown') logs.push('[exception] ' + JSON.stringify(m.params.exceptionDetails).slice(0, 300));
    });
    const s = (m, p) => send(m, p, sessionId);
    await s('Page.enable'); await s('Runtime.enable'); await s('Network.enable');
    await s('Emulation.setDeviceMetricsOverride', { width: 1600, height: 960, deviceScaleFactor: 1, mobile: false });
    const ev = async e => { const x = await s('Runtime.evaluate', { expression: e, awaitPromise: true, returnByValue: true }); return x.result.value; };
    return {
      name, logs, send: s, ev,
      go: url => s('Page.navigate', { url }),
      reload: () => s('Page.reload', { ignoreCache: false }),
      front: () => s('Page.bringToFront'),
      shot: async n => { const x = await s('Page.captureScreenshot', { format: 'png' }); const f = join(out, `${tierName}-${n}.png`); writeFileSync(f, Buffer.from(x.data, 'base64')); return f; },
      click: async (x, y) => { for (const type of ['mousePressed', 'mouseReleased']) await s('Input.dispatchMouseEvent', { type, x, y, button: 'left', clickCount: 1 }); },
      key: async (key, code, vk, modifiers = 0) => { await s('Input.dispatchKeyEvent', { type: 'rawKeyDown', key, code, windowsVirtualKeyCode: vk, modifiers }); await s('Input.dispatchKeyEvent', { type: 'keyUp', key, code, windowsVirtualKeyCode: vk, modifiers }); },
      text: t => s('Input.insertText', { text: t }),
      dbg: async () => { try { return JSON.parse(await ev('window.wpDebug ? window.wpDebug() : "null"')); } catch { return null; } },
      setCookies: async (cookies, host) => { for (const c of cookies) { const [n, ...v] = c.split('='); await s('Network.setCookie', { name: n, value: v.join('='), domain: host, path: '/', httpOnly: true }); } },
    };
  }
  const close = () => { try { ws.close(); } catch { /* 이미 */ } try { execFileSync('taskkill', ['/pid', String(proc.pid), '/t', '/f'], { stdio: 'ignore' }); } catch { /* 이미 */ } };
  return { tab, close };
}

// ---------- 로컬 클라우드 ----------
const W = { 'x-worshipcue': '1' };
let DIRECT = ''; // 클라우드에 바로 ('다른 곳')
let VIA = ''; // 브라우저가 쓰는 주소 (끊을 수 있는 중계)
async function api(cookie, method, p, body, headers = {}) {
  const r = await fetch(DIRECT + p, { method, headers: { ...(cookie ? { cookie } : {}), ...W, ...(body ? { 'content-type': 'application/json' } : {}), ...headers }, body: body ? JSON.stringify(body) : undefined, redirect: 'manual' });
  let json = null; try { json = await r.json(); } catch { /* 글 */ }
  return { status: r.status, json, headers: r.headers };
}
const cookieOf = h => h.getSetCookie().map(c => c.split(';')[0]).filter(c => !c.endsWith('='));
const ip = () => `198.18.${1 + Math.floor(Math.random() * 250)}.${1 + Math.floor(Math.random() * 250)}`;

function viewer(cookie) {
  const s = new WebSocket(DIRECT.replace(/^http/, 'ws') + '/events?client=main', { headers: { cookie, origin: DIRECT } });
  const box = { s, q: [] };
  s.on('message', (d, bin) => { if (!bin) { try { const m = JSON.parse(d.toString()); m.at = Date.now(); box.q.push(m); } catch { /* pong */ } } });
  box.open = new Promise((res, rej) => { s.once('open', res); s.once('error', rej); });
  return box;
}

async function main() {
  mkdirSync(out, { recursive: true });
  if (!existsSync(join(cloud, '.dev.vars')) || !existsSync(join(cloud, '.secrets', 'auth.json'))) execFileSync(process.execPath, [join(cloud, 'scripts', 'seed.mjs')], { cwd: cloud, stdio: 'inherit' });
  execFileSync(process.execPath, [join(cloud, 'scripts', 'assets.mjs')], { cwd: cloud, stdio: 'inherit' });
  if (!existsSync(join(cloud, 'public', 'control', 'wp-app_bg.wasm'))) throw new Error('public/control 에 wasm 이 없습니다 — 먼저 bash tools/web/build.sh');
  const AUTH = readFileSync(join(cloud, '.dev.vars'), 'utf8').match(/^AUTH_SECRET=(.+)$/m)[1].trim();
  const persist = mkdtempSync(join(tmpdir(), 'wp-offline-e2e-'));
  const wr = (...a) => execFileSync(process.execPath, [wrangler, ...a], { cwd: cloud, stdio: 'ignore' });
  wr('d1', 'migrations', 'apply', 'worshippresenter-web', '--local', '--persist-to', persist);

  // 유료 교회 (D1): 주인 · 설치판 열쇠 + 견본 성경(개역한글 6장 — 공공 영역, web/app/sample/seed.json)
  const church = 'off-paid-' + Date.now().toString(36).slice(-5);
  const sec = { owner: randomBytes(24).toString('base64url'), bridge: randomBytes(24).toString('base64url') };
  const hash = (role, s) => createHmac('sha256', AUTH).update(`cred:${church}:${role}:${s}`).digest('hex');
  const q = s => `'${String(s).replace(/'/g, "''")}'`;
  const seed = JSON.parse(readFileSync(join(repo, 'web', 'app', 'sample', 'seed.json'), 'utf8'));
  const krv = seed.versions[0];
  const structure = {};
  for (const [b, c, v] of krv.verses) { const s = (structure[b] ||= []); s[c - 1] = Math.max(s[c - 1] || 0, v); }
  const now = Date.now();
  const sql = [`INSERT INTO churches (id, name, created_at) VALUES (${q(church)}, '오프라인 시험 교회', ${now});`];
  for (const role of ['owner', 'bridge']) sql.push(`INSERT INTO credentials (church_id, id, role, label, secret_hash, created_at) VALUES (${q(church)}, ${q(role + 'k' + now.toString(36))}, ${q(role)}, '시험', ${q(hash(role, sec[role]))}, ${now});`);
  sql.push(`INSERT INTO bible_versions (church_id, id, name, license_note, verses, language, structure, imported_at) VALUES (${q(church)}, 'krv', ${q(krv.meta.name)}, ${q(krv.meta.license)}, ${krv.verses.length}, 'ko', ${q(JSON.stringify(structure))}, ${now});`);
  for (let i = 0; i < krv.verses.length; i += 40) sql.push('INSERT INTO bible_verses (church_id, version, book, chapter, verse, text) VALUES ' + krv.verses.slice(i, i + 40).map(([b, c, v, t]) => `(${q(church)}, 'krv', ${b}, ${c}, ${v}, ${q(t)})`).join(', ') + ';');
  const sqlFile = join(persist, 'seed.sql');
  writeFileSync(sqlFile, sql.join('\n'));
  wr('d1', 'execute', 'worshippresenter-web', '--local', '--persist-to', persist, '--file', sqlFile, '-y');

  const fake = await startFakeGoogle();
  const port = await freePort(), insp = await freePort(), vport = await freePort();
  DIRECT = `http://127.0.0.1:${port}`;
  VIA = `http://127.0.0.1:${vport}`;
  const dev = spawn(process.execPath, [wrangler, 'dev', '--ip', '127.0.0.1', '--port', String(port), '--inspector-port', String(insp), '--local', '--persist-to', persist, '--show-interactive-dev-session=false',
    '--var', `GOOGLE_FAKE:${fake.url}`, '--var', `GOOGLE_CLIENT_ID:${CLIENT_ID}`, '--var', `GOOGLE_CLIENT_SECRET:${CLIENT_SECRET}`, '--var', 'TIER_CACHE_MS:0',
    '--var', 'SERVICE_OVERRIDES:{"freeSignupsPerDay":100000,"signupsPerIpPerHour":1000}'], { cwd: cloud, stdio: ['ignore', 'pipe', 'pipe'] });
  let devLog = '';
  dev.stdout.on('data', d => { devLog += d; }); dev.stderr.on('data', d => { devLog += d; });
  const ok = await until(async () => (await fetch(DIRECT + '/health')).ok, 90000, 300);
  if (!ok) { console.error(devLog.slice(-3000)); throw new Error('wrangler dev 가 뜨지 않았습니다'); }
  const link = cutter(vport, port);
  await link.up();
  console.error(`로컬 클라우드 ${DIRECT} · 브라우저는 중계 ${VIA} · 자리 ${persist}`);
  // 중계를 거쳐도 같은 곳(Origin)으로 보이나 — 쿠키 쓰기 보호
  const probe = await fetch(VIA + '/health').then(r => r.ok).catch(() => false);
  if (!probe) throw new Error('중계가 닿지 않습니다');

  const finish = async code => {
    writeFileSync(join(out, 'result.json'), JSON.stringify({ results, at: new Date().toISOString() }, null, 1));
    const fails = results.filter(x => !x.ok).length;
    console.log(`${results.length - fails}/${results.length} 통과 (그림 · 결과: ${out})`);
    await link.down();
    try { execFileSync('taskkill', ['/pid', String(dev.pid), '/t', '/f'], { stdio: 'ignore' }); } catch { /* 이미 */ }
    await fake.close();
    if (!args.includes('--keep')) { await sleep(1500); try { rmSync(persist, { recursive: true, force: true, maxRetries: 5, retryDelay: 500 }); } catch (e) { console.error('임시 자리를 지우지 못함 (나중에 지워도 됨):', persist); } }
    process.exit(code ?? (fails ? 1 : 0));
  };

  try {
    // 유료: 주인 열쇠로 로그인
    if (!only || only === 'paid') {
      tierName = 'paid';
      const r = await fetch(DIRECT + '/auth/login', { method: 'POST', headers: { 'content-type': 'application/json', ...W, 'cf-connecting-ip': ip() }, body: JSON.stringify({ church, secret: sec.owner }) });
      if (r.status !== 200) throw new Error('유료 로그인 실패 ' + r.status);
      await scenario({ cookie: cookieOf(r.headers), church, link, bible: true, update: true });
    }
    // 무료: 가짜 구글로 시작하기 → 주인 쿠키
    if (!only || only === 'free') {
      tierName = 'free';
      const email = `offline-${Date.now().toString(36)}@example.com`;
      const g = await googleSignup(fake.url, email, '오프라인 무료 교회');
      const me = await api(g.join('; '), 'GET', '/api/v1/tier');
      check('무료 교회 (구글 드라이브 저장소)', me.json && me.json.tier === 'free' && me.json.store === 'drive', me.json && { tier: me.json.tier, store: me.json.store });
      await scenario({ cookie: g, church: (await api(g.join('; '), 'GET', '/api/v1/me')).json.church, link, bible: false, update: false, fake: fake.url, email });
    }
  } catch (e) {
    console.error(e);
    check('시험이 끝까지 돎', false, String(e && e.message || e));
  }
  await finish();
}

async function googleSignup(G, email, name) {
  const enc = encodeURIComponent;
  const a = ip();
  const r1 = await fetch(DIRECT + `/auth/google/start?mode=signup&name=${enc(name)}&hint=${enc(email)}`, { redirect: 'manual', headers: { 'cf-connecting-ip': a } });
  const oauth = cookieOf(r1.headers);
  const r2 = await fetch(r1.headers.get('location'), { redirect: 'manual' });
  const cb = new URL(r2.headers.get('location'));
  const r3 = await fetch(DIRECT + cb.pathname + cb.search, { redirect: 'manual', headers: { cookie: oauth.join('; '), 'cf-connecting-ip': a } });
  if (r3.status !== 302) throw new Error('구글로 시작하기 실패 ' + r3.status);
  return cookieOf(r3.headers);
}

const CUE = 'off-cue';
// 조작 화면 자리 (1600x960, tools/web/cloud-e2e.js 와 같은 배치): 큐시트 n 번째 줄 · 송출 단추 · 내용 칸
const rowY = i => 203 + 30 * i;
const SEND = [310, 60];
async function pick(tab, i) { await tab.click(150, rowY(i)); await sleep(550); await tab.click(150, rowY(i)); await sleep(500); }
async function diag(tab) {
  return tab.ev(`JSON.stringify({ href: location.href, times: window.wpTimes || null, boot: (document.getElementById('boot') || {}).className, msg: (document.getElementById('boot_msg') || {}).textContent, ctl: !!(navigator.serviceWorker && navigator.serviceWorker.controller) })`).catch(e => String(e));
}

async function scenario({ cookie, church, link, bible, update, fake, email }) {
  const ck = cookie.join('; ');
  const cue = seed().cues[0];
  const payload = { id: CUE, name: '오프라인 시험 예배', serviceDate: '2026-11-29', startTime: '11:00', items: cue.items, memo: '', pinned: false, template: false };
  let r = await api(ck, 'PUT', `/api/v1/documents/cue/${CUE}`, payload);
  check('견본 큐시트 문서', r.status === 201 || r.status === 200, r.status);
  for (const s of seed().songs) await api(ck, 'PUT', `/api/v1/documents/song/${s.id}`, s);
  const host = new URL(VIA).hostname;
  const b = await browser();
  const ctl = await b.tab('control');
  await ctl.setCookies(cookie, host);
  await ctl.go(VIA + '/control/?debug=1');
  let d = await until(async () => { const x = await ctl.dbg(); return x && x.store && x.store.ready && x.cue && x.cue.id === CUE ? x : null; }, 60000);
  check('조작 화면이 교회 큐시트를 받아 엶', d, d && { cue: d.cue && d.cue.id, store: d.store });
  const tInst = Date.now();
  d = await until(async () => { const x = await ctl.dbg(); return x && x.offline && x.offline.controlled && x.offline.sw === 'ready' ? x : null; }, 120000, 500);
  check('서비스 워커가 앱 껍데기를 담고 이 페이지를 맡음', d, d && { ...d.offline, seconds: Math.round((Date.now() - tInst) / 1000) });
  const ver = d && d.offline && d.offline.version;
  const caches = await ctl.ev(`(async () => { const k = await caches.keys(); const pages = await (await caches.open('wp-pages-v1')).keys(); const shell = k.find(x => x.startsWith('wp-shell-')); const files = shell ? (await (await caches.open(shell)).keys()).map(r => new URL(r.url).pathname) : []; return JSON.stringify({ k, pages: pages.map(r => new URL(r.url).pathname), n: files.length, wasm: files.includes('/control/wp-app_bg.wasm'), pdf: files.some(f => f.startsWith('/cloud/vendor/pdfjs/')), media: files.includes('/cloud/media/media.js'), out: files.includes('/outputs/output.js') }); })()`).then(JSON.parse).catch(e => ({ error: String(e) }));
  check('판별 저장: wasm · 출력 페이지 · /cloud/media · pdf.js · 페이지(/control/ · /outputs/main)', caches.wasm && caches.pdf && caches.media && caches.out && (caches.pages || []).includes('/control/') && (caches.pages || []).includes('/outputs/main') && (caches.k || []).includes('wp-shell-' + ver), caches);
  if (bible) {
    d = await until(async () => { const x = await ctl.dbg(); return x && (x.versions || []).includes('krv') ? x : null; }, 30000);
    check('교회 성경을 받음 (IndexedDB 에 보관)', d, d && d.versions);
  }
  // 출력 창 (같은 브라우저 — 서비스 워커가 맡음) · 클라우드 출력(다른 곳의 OBS 처럼, 클라우드에 바로)
  const outTab = await b.tab('output');
  await outTab.go(VIA + '/outputs/main');
  await until(async () => (await outTab.ev('document.readyState')) === 'complete', 15000);
  const outCtl = await until(() => outTab.ev('!!(navigator.serviceWorker && navigator.serviceWorker.controller)'), 10000);
  check('출력 창도 서비스 워커가 맡음', outCtl);
  const cv = viewer(ck);
  await cv.open;
  // 송출: 2번째 줄(찬양 '아침 빛') → 송출
  const song = cue.items[1];
  const line1 = (song.lyrics || '').split('\n').map(x => x.trim()).filter(x => x && !x.startsWith('['))[0] || '';
  await pick(ctl, 1);
  await ctl.shot('1a-picked');
  await ctl.click(...SEND); await sleep(800);
  let shown = await until(async () => (await outTab.ev('document.body.innerText')).includes(line1), 8000);
  check('송출 → 같은 브라우저 출력 창', shown, line1);
  const cloud1 = await until(() => cv.q.find(m => m.t === 'render' && m.data && m.data.hasLive), 8000);
  check('송출 → 클라우드 출력', cloud1);
  await ctl.shot('1-online');

  if (!args.includes('--update-only')) {
  // ===== 끊김 =====
  await link.down();
  const mark = '끊긴 동안 고침 ' + Date.now().toString(36);
  // 7번째 줄(공지 '교회 소식') 내용 칸 끝에 글을 더한다
  await pick(ctl, 6);
  await ctl.shot('2a-picked-notice');
  for (const y of [657, 640, 620]) { await ctl.click(1000, y); await sleep(250); const f = (await ctl.dbg()).focus; if (f) break; }
  await ctl.key('End', 'End', 35, 2);
  await ctl.text(' ' + mark); await sleep(200);
  await ctl.click(170, 620);
  d = await until(async () => { const x = await ctl.dbg(); return x && x.offline.net === 'offline' && x.offline.pending >= 1 ? x : null; }, 20000);
  check("끊김을 알아챔 · '연결 끊김 · 올릴 것 N개'", d, d && { net: d.offline.net, pending: d.offline.pending, queue: d.store.queue });
  const inQueue = d && d.store.queue.every(m => m[0] === 'cue' && m[1] === CUE);
  // 다음(F6) 두 번 — 같은 브라우저 출력은 계속, 줄에는 안 들어감
  await pick(ctl, 5); // '감사의 노래' 미리보기
  await ctl.click(...SEND); await sleep(600);
  const song2 = cue.items[5];
  const l2 = (song2.lyrics || '').split('\n').map(x => x.trim()).filter(x => x && !x.startsWith('['));
  shown = await until(async () => (await outTab.ev('document.body.innerText')).includes(l2[0]), 8000);
  check('끊긴 동안에도 같은 브라우저 출력 창은 송출을 받음', shown, l2[0]);
  await ctl.key('F6', 'F6', 117); await sleep(400);
  await ctl.key('F6', 'F6', 117); await sleep(600);
  const dAfter = await ctl.dbg();
  check('다음 · 이전은 올릴 줄에 쌓이지 않음 (줄에는 고친 큐시트만)', inQueue && dAfter.store.queue.every(m => m[0] === 'cue' && m[1] === CUE), dAfter.store.queue);
  const liveVersion = dAfter.live.version;
  await ctl.shot('2-offline-chip');
  // 클라우드 출력 화면(이미 붙어 있던 것)은 마지막 화면을 그대로 — 끊긴 사이 새 화면이 가지 않음
  const before = cv.q.filter(m => m.t === 'render').length;

  // 그사이 다른 곳(설치판 · 다른 웹)에서 같은 큐시트를 고친다
  const cur = await api(ck, 'GET', `/api/v1/documents/cue/${CUE}`);
  const other = { ...cur.json.payload, name: '오프라인 시험 예배 (다른 곳에서 고침)', memo: '다른 곳' };
  r = await api(ck, 'PUT', `/api/v1/documents/cue/${CUE}`, other, { 'if-match': String(cur.json.revision) });
  check('다른 곳에서 같은 큐시트를 고침 (판 오름)', r.status === 200, r.json && r.json.revision);

  // 끊긴 채 새로 고침
  await ctl.reload();
  d = await until(async () => { const t = JSON.parse(await ctl.ev('JSON.stringify(window.wpTimes||null)')); const x = await ctl.dbg(); return t && t.painted && x && x.store && x.store.ready ? x : null; }, 60000);
  check('끊긴 채 새로 고침 → 앱이 뜸 (서비스 워커)', d, d ? d.store : await diag(ctl));
  if (!d) { await ctl.shot('3x-reload-failed'); writeFileSync(join(out, `${tierName}-logs.txt`), ctl.logs.join('\n')); }
  const items = d && d.cue && JSON.stringify(d.cue.items);
  check('끊긴 채 새로 고침 → 고친 것이 남아 있음 (IndexedDB 올릴 줄)', items && items.includes(mark), mark);
  d = await until(async () => { const x = await ctl.dbg(); return x && x.offline.net === 'offline' && x.offline.pending >= 1 ? x : null; }, 20000);
  check('새로 고친 뒤에도 연결 끊김 · 올릴 것 표시', d, d && d.offline);
  if (bible) {
    d = await until(async () => { const x = await ctl.dbg(); return x && (x.versions || []).includes('krv') ? x : null; }, 20000);
    check('끊긴 채 새로 고침 → 성경도 이 브라우저 보관에서', d, d && d.versions);
  }
  // 마지막 자료로 송출 (같은 브라우저 출력)
  await pick(ctl, 1);
  await ctl.click(...SEND); await sleep(800);
  shown = await until(async () => (await outTab.ev('document.body.innerText')).includes(line1), 8000);
  check('끊긴 채 새로 고친 조작 화면 → 출력 창에 송출', shown);
  await outTab.reload();
  shown = await until(async () => (await outTab.ev('document.readyState')) === 'complete' && (await outTab.ev('document.body.innerText')).includes(line1), 15000);
  check('출력 창도 끊긴 채 새로 고침 → 지금 화면을 다시 받음', shown);
  await ctl.shot('3-offline-reloaded');
  await outTab.shot('3-output-offline');
  const leaked = cv.q.filter(m => m.t === 'render').length - before;
  check('끊긴 동안 클라우드 출력에는 아무것도 가지 않음 (마지막 화면 유지)', leaked === 0, leaked);

  // ===== 다시 이음 =====
  const tUp = Date.now();
  await link.up();
  d = await until(async () => { const x = await ctl.dbg(); return x && x.offline.net === 'online' && x.offline.pending === 0 ? x : null; }, 60000, 500);
  check('다시 이으면 올릴 줄이 다 올라가고 상태 칸이 사라짐', d, d && d.offline);
  let docs = [];
  await until(async () => { const list = await api(ck, 'GET', '/api/v1/documents?kind=cue&include=payload'); docs = (list.json && list.json.documents) || []; return docs.some(x => /충돌 사본/.test(x.payload && x.payload.name || '')); }, 15000, 500);
  const orig = docs.find(x => x.id === CUE);
  const copy = docs.find(x => x.id !== CUE && /충돌 사본/.test(x.payload && x.payload.name || ''));
  check('원래 자리 = 다른 곳에서 고친 것 (덮어쓰지 않음)', orig && orig.payload.name === other.name, orig && orig.payload.name);
  check("이 화면에서 고친 것 = '○○ (충돌 사본 HH:MM)' 새 큐시트", copy && JSON.stringify(copy.payload.items).includes(mark), copy ? { id: copy.id, name: copy.payload.name } : { docs: docs.map(x => [x.id, x.revision, x.payload && x.payload.name, x.updatedBy]), queue: (await ctl.dbg()).store.queue, logs: ctl.logs.filter(l => /충돌|사본|저장|오류|error/i.test(l)).slice(-8) });
  const dd = await ctl.dbg();
  check('조작 화면에도 원래 자리 = 클라우드 판', dd && dd.cue && (dd.cue.id !== CUE || !JSON.stringify(dd.cue.items).includes(mark)), dd && dd.cue && dd.cue.id);
  // 클라우드 출력: 송출 주체가 다시 붙으면 지금 화면 하나 (쌓인 조작을 다시 보내지 않음)
  const back = await until(() => cv.q.find(m => m.t === 'render' && m.at >= tUp && m.data && m.data.hasLive), 30000);
  const after = cv.q.filter(m => m.t === 'render' && m.at >= tUp);
  const last = ((await ctl.dbg()) || {}).live || {};
  check('다시 이으면 클라우드 출력은 지금 화면을 받음 (쌓아 둔 다음 · 이전을 다시 보내지 않음)', back && after.length <= 3 && back.data.version === last.version, { renders: after.length, got: back && back.data.version, now: last.version, offlineLive: liveVersion });
  await ctl.shot('4-reconnected');

  }
  // ===== 등급 오류는 '연결 끊김' 과 섞이지 않음 (무료: 구글 드라이브 연결이 끊김) =====
  if (fake) {
    await fetch(fake + '/_fake/revoke', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ email }) });
    await fetch(fake + '/_fake/expire', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ email }) });
    await pick(ctl, 6);
    for (const y of [657, 640, 620]) { await ctl.click(1000, y); await sleep(250); const f = (await ctl.dbg()).focus; if (f) break; }
    await ctl.key('End', 'End', 35, 2);
    await ctl.text(' 드라이브 끊긴 뒤'); await sleep(200);
    await ctl.click(170, 620);
    d = await until(async () => { const x = await ctl.dbg(); return x && /^blocked:drive_/.test(x.offline.net) ? x : null; }, 30000);
    check("구글 드라이브가 끊기면 '저장 멈춤' (연결 끊김이 아님) · 고친 것은 줄에 남음", d && d.offline.pending >= 1, d && d.offline);
    await ctl.shot('5-drive-blocked');
  }

  // ===== 새 판: 저절로 바뀌지 않고 사람이 누를 때 =====
  if (update) {
    const swFile = join(cloud, 'public', 'sw.js');
    const orig = readFileSync(swFile, 'utf8');
    const newVer = 'e2e' + Date.now().toString(36);
    writeFileSync(swFile, orig.replace(/const VERSION = "[^"]+"/, `const VERSION = "${newVer}"`));
    try {
      const served = await until(async () => (await (await fetch(DIRECT + '/sw.js', { cache: 'no-store' })).text()).includes(newVer), 30000, 500);
      check('(시험 준비) 클라우드가 새 판 sw.js 를 내보냄', served);
      await ctl.ev('window.wpOffline.check(), 1');
      const tUpd = Date.now();
      d = await until(async () => { const x = await ctl.dbg(); return x && x.offline.update === true ? x : null; }, 120000, 1000);
      console.error(`새 판 담기: ${Math.round((Date.now() - tUpd) / 1000)}초`);
      const regInfo = await ctl.ev(`navigator.serviceWorker.getRegistration('/').then(r => r ? r.update().then(() => 'ok', e => 'update-error: ' + e).then(async u => JSON.stringify({ u, installing: !!r.installing, waiting: !!r.waiting, active: r.active && r.active.state, state: window.wpOffline.state(), progress: await new Promise(res => { if (!r.installing) return res(null); const ch = new MessageChannel(); ch.port1.onmessage = e => res(e.data); r.installing.postMessage({ t: 'version' }, [ch.port2]); setTimeout(() => res('no-answer'), 3000); }) })) : 'no-reg')`).catch(e => String(e));
      check("새 판이 나오면 '새 버전 있음' 만 (지금 판 그대로)", d && d.offline.version === ver, d ? d.offline : regInfo);
      await sleep(400);
      await ctl.shot('6-update-available');
      const live = (await ctl.dbg()).live;
      const btn = d && d.offline.buttons.update;
      if (btn) {
        await ctl.click(btn[0], btn[1]); await sleep(500);
        const c = (await ctl.dbg()).offline.buttons['update-confirm'];
        check('송출 중이면 바꾸기 전에 한 번 묻는다', !live.visible || c, { visible: live.visible, confirm: !!c });
        if (c) { await ctl.shot('6-update-confirm'); await ctl.click(c[0], c[1]); }
        d = await until(async () => { const x = await ctl.dbg(); return x && x.offline && x.offline.version === newVer && x.store && x.store.ready ? x : null; }, 60000, 500);
        check('누르면 새 판으로 바뀌고 다시 열림', d, d && d.offline);
      } else check("'바꾸기' 단추", false);
    } finally { writeFileSync(swFile, orig); }
  }
  writeFileSync(join(out, `${tierName}-logs.txt`), [...ctl.logs, '--- output ---', ...outTab.logs].join('\n'));
  try { cv.s.close(); } catch { /* 이미 */ }
  b.close();
}

let SEED = null;
const seed = () => SEED || (SEED = JSON.parse(readFileSync(join(repo, 'web', 'app', 'sample', 'seed.json'), 'utf8')));

main().catch(e => { console.error(e); process.exit(2); });
