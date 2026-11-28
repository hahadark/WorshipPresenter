// [웹판] 3단계 끝에서 끝: 로컬 클라우드(wrangler dev)에 웹 조작 화면(/control/)을 실제 브라우저로 열어 본다.
//   node tools/web/cloud-e2e.js --base http://127.0.0.1:8791 [--browser edge] [--out tmp/web-cloud]
// 준비: cloud/ 에서 npm run seed (.secrets/sample.json), node scripts/assets.mjs (dist-web → public/control), wrangler dev
// 확인하는 것
//  1. 로그인 없이 /control/ → 로그인 화면으로 · /demo/ 는 열림
//  2. 주인 쿠키로 /control/ → 교회 문서(큐시트 · 찬양) · 성경을 받음, 송출 주체(/bridge)로 붙음
//  3. 고친 큐시트가 /api/v1/documents 에 올라감 (If-Match 판이 오름)
//  4. 가짜 설치판이 붙으면 웹은 대기(보기 전용)로 내려가고 클라우드 출력에 보내지 않음 → [넘겨받기] → 웹이 송출
'use strict';
const fs = require('fs');
const path = require('path');
const { spawn } = require('child_process');
const args = process.argv.slice(2);
const opt = (n, d) => { const i = args.indexOf('--' + n); return i >= 0 ? args[i + 1] : d; };
const BASE = opt('base', 'http://127.0.0.1:8791');
const out = path.resolve(opt('out', 'tmp/web-cloud'));
const which = opt('browser', 'edge');
const exe = { edge: 'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe', chrome: 'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe' }[which];
const cloudDir = path.join(__dirname, '..', '..', 'cloud');
const SAMPLE = JSON.parse(fs.readFileSync(path.join(cloudDir, '.secrets', 'sample.json'), 'utf8'));
const WebSocket = require(path.join(cloudDir, 'node_modules', 'ws'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const results = [];
const check = (name, ok, detail) => { results.push({ name, ok: !!ok, detail }); console.error((ok ? 'OK  ' : 'FAIL') + ' ' + name + (detail !== undefined ? ' → ' + JSON.stringify(detail).slice(0, 300) : '')); };

async function login(role) {
  const r = await fetch(BASE + '/auth/login', { method: 'POST', headers: { 'content-type': 'application/json', 'x-worshipcue': '1', 'cf-connecting-ip': '198.19.9.' + (1 + Math.floor(Math.random() * 200)) }, body: JSON.stringify({ church: SAMPLE.church, secret: SAMPLE.credentials[role].secret }) });
  if (r.status !== 200) throw new Error('로그인 실패 ' + role + ' ' + r.status);
  return r.headers.getSetCookie().map((c) => c.split(';')[0]);
}
async function api(cookie, method, p, body, headers = {}) {
  const r = await fetch(BASE + p, { method, headers: { cookie: cookie.join('; '), 'x-worshipcue': '1', ...(body ? { 'content-type': 'application/json' } : {}), ...headers }, body: body ? JSON.stringify(body) : undefined });
  let j = null; try { j = await r.json(); } catch {}
  return { status: r.status, json: j };
}
function ws(p, headers) {
  const s = new WebSocket(BASE.replace(/^http/, 'ws') + p, { headers });
  const box = { s, q: [] };
  s.on('message', (d, bin) => { if (!bin) { try { box.q.push(JSON.parse(d.toString())); } catch {} } });
  box.open = new Promise((res, rej) => { s.once('open', res); s.once('error', rej); s.once('unexpected-response', (_q, r) => rej(new Error('거절 ' + r.statusCode))); });
  box.wait = async (pred, ms = 8000) => { const t0 = Date.now(); for (;;) { const i = box.q.findIndex(pred); if (i >= 0) return box.q.splice(i, 1)[0]; if (Date.now() - t0 > ms) return null; await sleep(50); } };
  box.send = (o) => s.send(JSON.stringify(o));
  return box;
}

(async () => {
  fs.mkdirSync(out, { recursive: true });
  const owner = await login('owner');
  // 견본 문서 (큐시트 · 찬양) — 이미 있으면 그대로
  const seed = JSON.parse(fs.readFileSync(path.join(__dirname, '..', '..', 'web', 'app', 'sample', 'seed.json'), 'utf8'));
  const cue = seed.cues[0];
  const cuePayload = { id: 'cloud-cue-1', name: '클라우드 견본 예배', serviceDate: '2026-11-29', startTime: '11:00', items: cue.items, memo: '', pinned: false, template: false };
  let r = await api(owner, 'GET', '/api/v1/documents/cue/cloud-cue-1');
  if (r.status === 404) r = await api(owner, 'PUT', '/api/v1/documents/cue/cloud-cue-1', cuePayload);
  for (const s of seed.songs) { const g = await api(owner, 'GET', '/api/v1/documents/song/' + s.id); if (g.status === 404) await api(owner, 'PUT', '/api/v1/documents/song/' + s.id, s); }
  const before = (await api(owner, 'GET', '/api/v1/documents/cue/cloud-cue-1')).json;
  check('견본 큐시트 문서', before && before.ok, { revision: before && before.revision });

  // 1. 로그인 없이
  const anon = await fetch(BASE + '/control/', { redirect: 'manual' });
  check('로그인 없이 /control/ → 로그인 화면', anon.status === 302 && /\/login/.test(anon.headers.get('location') || ''), anon.headers.get('location'));
  const wasm = await fetch(BASE + '/control/wp-app_bg.wasm', { method: 'HEAD' });
  check('/control/wp-app_bg.wasm 정적 파일', wasm.status === 200, { type: wasm.headers.get('content-type'), len: wasm.headers.get('content-length') });

  // 브라우저
  const port = 9300 + Math.floor(Math.random() * 300);
  const prof = path.join(out, 'profile-' + which + '-' + Date.now());
  const proc = spawn(exe, [`--remote-debugging-port=${port}`, `--user-data-dir=${prof}`, '--headless=new', '--no-first-run', '--force-device-scale-factor=1', '--window-size=1600,960', 'about:blank'], { stdio: 'ignore' });
  const cdpBase = `http://127.0.0.1:${port}`;
  for (let i = 0; i < 100; i++) { try { if ((await fetch(cdpBase + '/json/version')).ok) break; } catch {} await sleep(150); }
  const tab = (await (await fetch(cdpBase + '/json/list')).json()).find((t) => t.type === 'page');
  const sock = new WebSocket(tab.webSocketDebuggerUrl);
  await new Promise((res) => sock.once('open', res));
  let id = 0; const wait = new Map(); const logs = [];
  let dialogs = [];
  sock.on('message', (d) => { const m = JSON.parse(d.toString()); if (m.method === 'Page.javascriptDialogOpening') { dialogs.push(m.params.type); sock.send(JSON.stringify({ id: 99999 + dialogs.length, method: 'Page.handleJavaScriptDialog', params: { accept: true } })); } if (m.id && wait.has(m.id)) { const { res, rej } = wait.get(m.id); wait.delete(m.id); m.error ? rej(new Error(m.error.message)) : res(m.result); } else if (m.method === 'Runtime.consoleAPICalled') logs.push(m.params.args.map((a) => a.value ?? a.description).join(' ')); else if (m.method === 'Runtime.exceptionThrown') logs.push('[exception] ' + JSON.stringify(m.params.exceptionDetails).slice(0, 300)); });
  const send = (method, params = {}) => new Promise((res, rej) => { const i = ++id; wait.set(i, { res, rej }); sock.send(JSON.stringify({ id: i, method, params })); });
  const ev = async (e) => { const x = await send('Runtime.evaluate', { expression: e, awaitPromise: true, returnByValue: true }); return x.result.value; };
  const shot = async (n) => { const x = await send('Page.captureScreenshot', { format: 'png' }); const f = path.join(out, `${which}-${n}.png`); fs.writeFileSync(f, Buffer.from(x.data, 'base64')); return f; };
  const click = async (x, y) => { for (const type of ['mousePressed', 'mouseReleased']) await send('Input.dispatchMouseEvent', { type, x, y, button: 'left', clickCount: 1 }); };
  const dbg = async () => { try { return JSON.parse(await ev('window.wpDebug ? window.wpDebug() : "null"')); } catch { return null; } };
  await send('Page.enable'); await send('Runtime.enable'); await send('Network.enable');
  await send('Emulation.setDeviceMetricsOverride', { width: 1600, height: 960, deviceScaleFactor: 1, mobile: false });
  const u = new URL(BASE);
  for (const c of owner) { const [name, value] = c.split('='); await send('Network.setCookie', { name, value, domain: u.hostname, path: '/', httpOnly: true }); }
  await send('Page.navigate', { url: BASE + '/control/?debug=1' });
  let times = null;
  for (let i = 0; i < 150; i++) { await sleep(200); try { times = JSON.parse(await ev('JSON.stringify(window.wpTimes||null)')); } catch {} if (times && times.painted) break; }
  check('조작 화면이 뜸 (/control/)', times && times.painted, times);
  // 문서 · 성경 · 송출 주체
  let d = null;
  for (let i = 0; i < 60; i++) { d = await dbg(); if (d && d.cue && d.cue.id === 'cloud-cue-1' && (d.versions || []).includes('krv') && /Active/.test(d.cloud)) break; await sleep(300); }
  check('교회 큐시트를 받아 엶', d && d.cue && d.cue.id === 'cloud-cue-1', d && d.cue && d.cue.id);
  check('교회 성경 번역본을 받음', d && (d.versions || []).includes('krv'), d && d.versions);
  check('송출 주체로 붙음 (설치판 없음)', d && /Active/.test(d.cloud), d && d.cloud);
  await shot('control-active');
  // 성경 검색 (요 3:16) — 교회 번역본에서
  await click(1375, 60); await sleep(200);
  await send('Input.insertText', { text: '요3:16' }); await sleep(200);
  await send('Input.dispatchKeyEvent', { type: 'keyDown', key: 'Enter', code: 'Enter', windowsVirtualKeyCode: 13, text: '\r' });
  await send('Input.dispatchKeyEvent', { type: 'keyUp', key: 'Enter', code: 'Enter', windowsVirtualKeyCode: 13 });
  await sleep(800);
  await shot('control-search');
  // 출력 보는 쪽 (클라우드 /events)
  const viewer = ws('/events?client=main', { cookie: owner.join('; '), origin: BASE });
  await viewer.open;
  // 4번 순서(요 3:16-21) 미리보기 → 송출 단추
  await click(150, 293); await sleep(500);
  await click(310, 60); await sleep(1200);
  const live1 = await viewer.wait((m) => m.t === 'render' && m.data && m.data.hasLive);
  check('웹 송출이 클라우드 출력에 닿음', live1, live1 && live1.data && live1.data.slide && (live1.data.slide.ref || live1.data.slide.kind));
  // 3. 고치기 → 올라감
  await click(150, 383); await sleep(500);
  for (const y of [657, 640, 620]) { await click(1000, y); await sleep(250); const f = (await dbg()).focus; if (f) break; }
  await send('Input.dispatchKeyEvent', { type: 'rawKeyDown', key: 'End', code: 'End', windowsVirtualKeyCode: 35, modifiers: 2 });
  await send('Input.dispatchKeyEvent', { type: 'keyUp', key: 'End', code: 'End', windowsVirtualKeyCode: 35, modifiers: 2 });
  const mark = ' 클라우드 저장 ' + Date.now();
  await send('Input.insertText', { text: mark }); await sleep(200);
  await click(170, 620);
  let after = null;
  for (let i = 0; i < 30; i++) { await sleep(500); after = (await api(owner, 'GET', '/api/v1/documents/cue/cloud-cue-1')).json; if (after && JSON.stringify(after.payload).includes(mark.trim())) break; }
  check('고친 큐시트가 클라우드 문서에 올라감 (판 오름)', after && JSON.stringify(after.payload).includes(mark.trim()) && after.revision > before.revision, { before: before.revision, after: after && after.revision });
  // 4. 가짜 설치판 → 웹 대기 → 넘겨받기
  const desk = ws('/bridge', { authorization: 'Bearer ' + SAMPLE.credentials.bridge.key });
  await desk.open;
  const ready = await desk.wait((m) => m.t === 'ready');
  desk.send({ t: 'hello', v: 1, app: '시험 설치판', lang: 'ko', outputs: ['main'], caps: {} });
  check('설치판이 붙으면 설치판이 주체 (웹이 넘겨받지 않았으므로)', ready && ready.standby === false, ready);
  for (let i = 0; i < 30; i++) { d = await dbg(); if (/Standby/.test(d.cloud)) break; await sleep(200); }
  check('웹은 대기(보기 전용)로', /Standby/.test(d.cloud), d.cloud);
  await shot('control-standby');
  desk.send({ t: 'render', client: 'main', data: { version: 9001, tag: 'desk' } });
  const dr = await viewer.wait((m) => m.t === 'render' && m.data && m.data.tag === 'desk');
  check('설치판 송출이 출력에 닿음', dr);
  viewer.q.length = 0;
  await click(310, 60); await sleep(1500); // 웹에서 다시 송출 (다음 장)
  await send('Input.dispatchKeyEvent', { type: 'rawKeyDown', key: 'F6', code: 'F6', windowsVirtualKeyCode: 117 });
  await send('Input.dispatchKeyEvent', { type: 'keyUp', key: 'F6', code: 'F6', windowsVirtualKeyCode: 117 });
  await sleep(1200);
  const leaked = viewer.q.find((m) => m.t === 'render' && m.data && m.data.hasLive !== undefined && m.data.tag !== 'desk');
  check('대기 중인 웹의 송출은 출력에 가지 않음', !leaked, leaked && leaked.data && leaked.data.version);
  check('설치판은 끊기지 않음', desk.s.readyState === 1);
  // 띠의 [넘겨받기…] → 확인 창 [넘겨받기] (자리는 점검 상태 cloudButtons 에서)
  let claimed = false;
  const cb = (await dbg()).cloudButtons || {};
  check('띠에 [넘겨받기…] 단추', cb.claim, cb);
  if (cb.claim) {
    await click(cb.claim[0], cb.claim[1]); await sleep(500);
    const cf = (await dbg()).cloudButtons || {};
    await shot('control-confirm');
    check('넘겨받기 확인 창', cf.confirm, cf);
    if (cf.confirm) { await click(cf.confirm[0], cf.confirm[1]); }
    for (let i = 0; i < 20; i++) { await sleep(250); d = await dbg(); if (/Active \{ claimed: true/.test(d.cloud)) { claimed = true; break; } }
  }
  check('주인이 [넘겨받기] → 웹이 주체 (넘겨받음)', claimed, d.cloud);
  let closed = null;
  desk.s.once('close', (c) => { closed = c; });
  for (let i = 0; i < 20 && closed === null && desk.s.readyState === 1; i++) await sleep(200);
  check('설치판은 4001 로 닫힘', closed === 4001 || desk.s.readyState >= 2, closed);
  await shot('control-claimed');
  const wr = await viewer.wait((m) => m.t === 'render' && m.data && m.data.hasLive !== undefined, 5000);
  check('넘겨받은 뒤 웹 송출이 출력에 닿음 (sync)', wr);
  const room = await api(owner, 'GET', '/api/v1/room');
  check('방 상태: 주체 web · 넘겨받음', room.json && room.json.source === 'web' && room.json.claimed === true, room.json && { source: room.json.source, claimed: room.json.claimed });

  // 4-1. AI: 웹 조작 화면의 aihelp::chat → /api/v1/ai/chat (교회 키 · 기능별 동의). 로컬은 가짜 공급자(mock://, AI_MOCK=1)
  const aiSet = await api(owner, 'PUT', '/api/v1/ai/settings', { endpoint: 'mock://e2e', model: 'mock-model', key: 'sk-e2e-test' });
  await api(owner, 'POST', '/api/v1/ai/consent', { feature: 'cuecheck', agree: true });
  await api(owner, 'POST', '/api/v1/ai/consent', { feature: 'lyrics', agree: false });
  const aiRun = async (f) => { await ev(`window.wpAiResult = null; window.wpAiTest('${f}'); 1`); for (let i = 0; i < 40; i++) { await sleep(250); const v = await ev('window.wpAiResult'); if (v) return JSON.parse(v); } return null; };
  if (aiSet.status === 200) {
    const a1 = await aiRun('cuecheck');
    check('AI (동의한 기능): 웹 → /api/v1/ai/chat → 답', a1 && a1.ok && /mock/.test(a1.text), a1);
    const a2 = await aiRun('lyrics');
    check('AI (동의 안 한 기능): 주인 동의 안내', a2 && !a2.ok && /동의/.test(a2.error), a2 && a2.error);
  } else check('AI 설정 (주인)', false, aiSet);

  // 5. 교회 관리(/owner): 사람 넣기 · 교회 AI 구획 (웹 조작 화면의 [교회 관리] 가 여는 화면)
  await send('Page.navigate', { url: BASE + '/owner' });
  await sleep(800);
  check('송출 중에 떠나려 하면 한 번 묻는다 (beforeunload)', dialogs.includes('beforeunload'), dialogs);
  let ownerOk = false;
  for (let i = 0; i < 40; i++) { await sleep(250); ownerOk = await ev("!!document.querySelector('#pp-add') && !!document.querySelector('#ai-save')").catch(() => false); if (ownerOk) break; }
  check('주인 화면에 사람 · AI 구획', ownerOk);
  const mlogin = 'web-e2e-' + Date.now().toString(36);
  await ev(`document.getElementById('pp-label').value='시험 편집자'; document.getElementById('pp-login').value='${mlogin}'; document.getElementById('pp-role').value='editor'; document.getElementById('pp-add').click(); 1`);
  let listed = false;
  for (let i = 0; i < 20; i++) { await sleep(250); listed = await ev(`[...document.querySelectorAll('#pp-list td')].some(td => td.textContent === '${mlogin}') && !!document.querySelector('#pp-made code')`); if (listed) break; }
  check('사람 넣기 → 목록 · 처음 비밀번호 한 번', listed, mlogin);
  await shot('owner');
  // 그 사람(편집자)으로 로그인하면 조작 화면이 열리고 송출 주체로는 붙지 않는다 (편집자 · 송출은 주인만)
  const pw = await ev("document.querySelector('#pp-made code:nth-of-type(2)')?.textContent || ''");
  const ml = await fetch(BASE + '/auth/member', { method: 'POST', headers: { 'content-type': 'application/json', 'cf-connecting-ip': '198.19.8.8' }, body: JSON.stringify({ church: SAMPLE.church, login: mlogin, password: pw }) });
  check('넣은 사람이 비밀번호로 로그인', ml.status === 200, ml.status);
  const mc = ml.headers.getSetCookie().map((c) => c.split(';')[0]);
  const mcontrol = await fetch(BASE + '/control/', { headers: { cookie: mc.join('; ') }, redirect: 'manual' });
  check('편집자도 /control/ 을 연다', mcontrol.status === 200, mcontrol.status);
  const mbr = ws('/bridge', { cookie: mc.join('; '), origin: BASE });
  const mres = await mbr.open.then(() => 'opened', (e) => e.message);
  check('편집자는 송출 주체(/bridge)로 못 붙음', /거절 40[13]/.test(mres), mres);
  // 6. 둘러보기 (DEMO_ENABLED=1): 데모 쿠키로 /control/ — 견본 교회 · 송출 주체(방문자마다 따로)
  const dr0 = await fetch(BASE + '/demo', { redirect: 'manual', headers: { 'cf-connecting-ip': '198.19.7.' + (1 + Math.floor(Math.random() * 200)) } });
  const dcook = dr0.headers.getSetCookie().map((c) => c.split(';')[0]);
  if (dr0.status === 302 && dcook.length) {
    await send('Network.clearBrowserCookies');
    for (const c of dcook) { const [name, value] = c.split('='); await send('Network.setCookie', { name, value, domain: u.hostname, path: '/', httpOnly: true }); }
    await send('Page.navigate', { url: BASE + '/control/?debug=1' });
    let dd = null;
    for (let i = 0; i < 80; i++) { await sleep(250); dd = await dbg(); if (dd && dd.cue && /Active/.test(dd.cloud)) break; }
    check('둘러보기: /control/ 에 견본 큐시트 · 송출 주체', dd && dd.cue && /Active/.test(dd.cloud), dd && { cue: dd.cue && dd.cue.id, cloud: dd.cloud, versions: dd.versions });
    await shot('demo-control');
  } else check('둘러보기 들어가기', false, dr0.status);

  fs.writeFileSync(path.join(out, which + '-result.json'), JSON.stringify({ results, logs: logs.slice(-40) }, null, 1));
  console.error(logs.filter((l) => /error|오류|panic|exception/i.test(l)).slice(-10).join('\n'));
  const fails = results.filter((x) => !x.ok).length;
  console.log(`${results.length - fails}/${results.length} 통과`);
  try { viewer.s.close(); desk.s.close(); sock.close(); } catch {}
  proc.kill();
  process.exit(fails ? 1 : 0);
})().catch((e) => { console.error(e); process.exit(2); });
