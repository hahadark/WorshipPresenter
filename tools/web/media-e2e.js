// [웹판] 4단계 끝에서 끝: 웹 조작 화면(/control/)에 미디어 붙이기 · 화면 구성 · AI 기다리는 동안 안 멈춤 — 로컬 클라우드(wrangler dev)에서.
//   node tools/web/media-e2e.js --base http://127.0.0.1:8791 [--out tmp/web-media]
// 실제 카메라는 쓰지 않는다 (이 PC 의 OBS Virtual Camera 는 화면 내용을 내보낼 수 있음) — 브라우저 가짜 카메라(--use-fake-device-for-media-stream).
// 시험 파일은 ffmpeg · 손으로 만든 PDF (실제 교회 자료 없음).
'use strict';
const fs = require('fs');
const path = require('path');
const { spawn, execFileSync } = require('child_process');
const args = process.argv.slice(2);
const opt = (n, d) => { const i = args.indexOf('--' + n); return i >= 0 ? args[i + 1] : d; };
const BASE = opt('base', 'http://127.0.0.1:8791');
const out = path.resolve(opt('out', 'tmp/web-media'));
const exe = 'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe';
const cloudDir = path.join(__dirname, '..', '..', 'cloud');
const SAMPLE = JSON.parse(fs.readFileSync(path.join(cloudDir, '.secrets', 'sample.json'), 'utf8'));
const WebSocket = require(path.join(cloudDir, 'node_modules', 'ws'));
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const results = [];
const check = (name, ok, detail) => { results.push({ name, ok: !!ok, detail }); console.error((ok ? 'OK  ' : 'FAIL') + ' ' + name + (detail !== undefined ? ' → ' + JSON.stringify(detail).slice(0, 260) : '')); };

/** 두 쪽짜리 아주 작은 PDF (글자 'WP 1' · 'WP 2') */
function tinyPdf() {
  const objs = [];
  const pages = [];
  objs.push('<< /Type /Catalog /Pages 2 0 R >>');
  objs.push(null); // Pages (뒤에 채움)
  objs.push('<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>');
  for (let i = 1; i <= 2; i++) {
    const s = `BT /F1 96 Tf 120 300 Td (WP ${i}) Tj ET`;
    objs.push(`<< /Length ${s.length} >>\nstream\n${s}\nendstream`);
    const contentNo = objs.length;
    objs.push(`<< /Type /Page /Parent 2 0 R /MediaBox [0 0 800 600] /Resources << /Font << /F1 3 0 R >> >> /Contents ${contentNo} 0 R >>`);
    pages.push(objs.length);
  }
  objs[1] = `<< /Type /Pages /Kids [${pages.map((p) => p + ' 0 R').join(' ')}] /Count ${pages.length} >>`;
  let s = '%PDF-1.4\n';
  const off = [];
  objs.forEach((o, i) => { off.push(s.length); s += `${i + 1} 0 obj\n${o}\nendobj\n`; });
  const x = s.length;
  s += `xref\n0 ${objs.length + 1}\n0000000000 65535 f \n` + off.map((o) => String(o).padStart(10, '0') + ' 00000 n \n').join('');
  s += `trailer\n<< /Size ${objs.length + 1} /Root 1 0 R >>\nstartxref\n${x}\n%%EOF\n`;
  return Buffer.from(s, 'latin1');
}

async function login(role) {
  const r = await fetch(BASE + '/auth/login', { method: 'POST', headers: { 'content-type': 'application/json', 'x-worshipcue': '1', 'cf-connecting-ip': '198.19.10.' + (1 + Math.floor(Math.random() * 200)) }, body: JSON.stringify({ church: SAMPLE.church, secret: SAMPLE.credentials[role].secret }) });
  if (r.status !== 200) throw new Error('로그인 실패 ' + r.status);
  return r.headers.getSetCookie().map((c) => c.split(';')[0]);
}
async function api(cookie, method, p, body) {
  const r = await fetch(BASE + p, { method, headers: { cookie: cookie.join('; '), 'x-worshipcue': '1', ...(body ? { 'content-type': 'application/json' } : {}) }, body: body ? JSON.stringify(body) : undefined });
  let j = null; try { j = await r.json(); } catch {}
  return { status: r.status, json: j };
}
function wsock(p, headers) {
  const s = new WebSocket(BASE.replace(/^http/, 'ws') + p, { headers });
  const box = { s, q: [] };
  s.on('message', (d, bin) => { if (!bin) { try { box.q.push(JSON.parse(d.toString())); } catch {} } });
  box.open = new Promise((res, rej) => { s.once('open', res); s.once('error', rej); });
  box.wait = async (pred, ms = 8000) => { const t0 = Date.now(); for (;;) { const i = box.q.findIndex(pred); if (i >= 0) return box.q.splice(i, 1)[0]; if (Date.now() - t0 > ms) return null; await sleep(50); } };
  return box;
}

(async () => {
  fs.mkdirSync(out, { recursive: true });
  // 시험 파일
  const files = {};
  const png = path.join(out, 'test.png'), webm = path.join(out, 'test.webm'), mp3 = path.join(out, 'test.mp3');
  execFileSync('ffmpeg', ['-y', '-loglevel', 'error', '-f', 'lavfi', '-i', 'testsrc=size=640x360:rate=1:duration=1', '-frames:v', '1', png]);
  execFileSync('ffmpeg', ['-y', '-loglevel', 'error', '-f', 'lavfi', '-i', 'testsrc=size=640x360:rate=30:duration=4', '-c:v', 'libvpx', '-b:v', '300k', webm]);
  execFileSync('ffmpeg', ['-y', '-loglevel', 'error', '-f', 'lavfi', '-i', 'sine=frequency=440:duration=2', '-c:a', 'libmp3lame', '-q:a', '6', mp3]);
  files['시험 그림.png'] = fs.readFileSync(png);
  files['시험 영상.webm'] = fs.readFileSync(webm);
  files['시험 소리.mp3'] = fs.readFileSync(mp3);
  files['시험 주보.pdf'] = tinyPdf();

  const owner = await login('owner');
  // 견본 큐시트 (cloud-e2e.js 와 같은 것)
  const seed = JSON.parse(fs.readFileSync(path.join(__dirname, '..', '..', 'web', 'app', 'sample', 'seed.json'), 'utf8'));
  const cueId = 'media-cue-' + Date.now().toString(36);
  await api(owner, 'PUT', '/api/v1/documents/cue/' + cueId, { id: cueId, name: '미디어 시험 예배', serviceDate: '2026-11-29', startTime: '11:00', items: seed.cues[0].items.slice(0, 3), memo: '', pinned: false, template: false });

  const port = 9300 + Math.floor(Math.random() * 300);
  const prof = path.join(out, 'profile-edge-' + Date.now());
  const proc = spawn(exe, [`--remote-debugging-port=${port}`, `--user-data-dir=${prof}`, '--headless=new', '--no-first-run', '--force-device-scale-factor=1', '--window-size=1600,960',
    '--use-fake-device-for-media-stream', '--use-fake-ui-for-media-stream', '--autoplay-policy=no-user-gesture-required', 'about:blank'], { stdio: 'ignore' });
  const cdpBase = `http://127.0.0.1:${port}`;
  for (let i = 0; i < 100; i++) { try { if ((await fetch(cdpBase + '/json/version')).ok) break; } catch {} await sleep(150); }
  const connect = async (t) => {
    const sock = new WebSocket(t.webSocketDebuggerUrl);
    await new Promise((res) => sock.once('open', res));
    let id = 0; const wait = new Map(); const logs = [];
    sock.on('message', (d) => { const m = JSON.parse(d.toString()); if (m.id && wait.has(m.id)) { const { res, rej } = wait.get(m.id); wait.delete(m.id); m.error ? rej(new Error(m.error.message)) : res(m.result); } else if (m.method === 'Runtime.consoleAPICalled') logs.push(m.params.args.map((a) => a.value ?? a.description).join(' ')); else if (m.method === 'Page.javascriptDialogOpening') sock.send(JSON.stringify({ id: 900000 + ++id, method: 'Page.handleJavaScriptDialog', params: { accept: true } })); });
    const send = (method, params = {}) => new Promise((res, rej) => { const i = ++id; wait.set(i, { res, rej }); sock.send(JSON.stringify({ id: i, method, params })); });
    const ev = async (e) => { const x = await send('Runtime.evaluate', { expression: e, awaitPromise: true, returnByValue: true }); if (x.exceptionDetails) throw new Error(JSON.stringify(x.exceptionDetails).slice(0, 300)); return x.result.value; };
    const shot = async (n) => { const x = await send('Page.captureScreenshot', { format: 'png' }); const f = path.join(out, `edge-${n}.png`); fs.writeFileSync(f, Buffer.from(x.data, 'base64')); return f; };
    await send('Page.enable'); await send('Runtime.enable');
    return { sock, send, ev, shot, logs };
  };
  const tab = (await (await fetch(cdpBase + '/json/list')).json()).find((t) => t.type === 'page');
  const P = await connect(tab);
  await P.send('Emulation.setDeviceMetricsOverride', { width: 1600, height: 960, deviceScaleFactor: 1, mobile: false });
  const u = new URL(BASE);
  for (const c of owner) { const [name, value] = c.split('='); await P.send('Network.setCookie', { name, value, domain: u.hostname, path: '/', httpOnly: true }); }
  // 이 브라우저의 설정(열린 큐시트)을 시험 큐시트로
  await P.send('Page.navigate', { url: BASE + '/control/?debug=1' });
  const click = async (x, y) => { for (const type of ['mousePressed', 'mouseReleased']) await P.send('Input.dispatchMouseEvent', { type, x, y, button: 'left', clickCount: 1 }); };
  const dbg = async () => { try { return JSON.parse(await P.ev('window.wpDebug ? window.wpDebug() : "null"')); } catch { return null; } };
  let d = null;
  for (let i = 0; i < 100; i++) { await sleep(250); d = await dbg(); if (d && d.cue && /Active/.test(d.cloud)) break; }
  check('조작 화면 · 송출 주체', d && d.cue && /Active/.test(d.cloud), d && d.cloud);
  // 시험 큐시트로 (큐시트 고르기와 같은 길: Core::open_cue) — 클라우드 문서 목록에 들어올 때까지 기다림
  for (let i = 0; i < 100; i++) { await P.ev(`window.wpOpenCue('${cueId}'); 1`); await sleep(300); d = await dbg(); if (d && d.cue && d.cue.id === cueId) break; }
  check('시험 큐시트를 엶', d && d.cue && d.cue.id === cueId, d && d.cue && d.cue.id);
  const n0 = d.cue.items.length;
  const b = d.cloudButtons || {};
  check('띠에 [파일 넣기…] · [휴대폰 카메라…]', b.files && b.phone, b);

  // 1. 파일 넣기: 그림 · 영상 · 소리 · PDF → 교회 저장소 → 큐시트 끝
  const hs = [];
  for (const [name, buf] of Object.entries(files)) {
    const h = await P.ev(`window.wpMedia.take(${JSON.stringify(name)}, Uint8Array.from(atob(${JSON.stringify(buf.toString('base64'))}), c => c.charCodeAt(0)), '')`);
    hs.push(h);
  }
  await P.ev(`window.wpPutFiles(${JSON.stringify(JSON.stringify(hs))}); 1`);
  let sawBusy = false;
  for (let i = 0; i < 120; i++) { await sleep(250); d = await dbg(); if (d.mediaBusy > 0) sawBusy = true; if (d.cue.items.length >= n0 + 4 && d.mediaBusy === 0) break; }
  const added = d.cue.items.slice(n0);
  check('파일 넣기 → 순서 4개 (그림 · 영상 · 소리 · PDF)', added.length === 4, added.map((x) => x.type + ':' + x.title));
  const img = added.find((x) => x.type === 'image'), vid = added.find((x) => x.type === 'video'), pdf = added.find((x) => x.type === 'slideshow'), snd = added.find((x) => x.type === 'blank');
  check('그림 순서는 교회 저장소 주소', img && /^\/api\/v1\/assets\//.test(img.image), img && img.image);
  check('영상 순서는 교회 저장소 주소', vid && /^\/api\/v1\/assets\//.test(vid.file), vid && vid.file);
  check('PDF → 쪽 그림 슬라이드쇼 (2쪽)', pdf && pdf.pdf === true && pdf.images.length === 2, pdf && pdf.images);
  check('소리 → 빈 화면 + 배경 음악', snd && /^\/api\/v1\/assets\//.test(snd.music), snd && snd.music);
  const assetOk = await fetch(BASE + (pdf ? pdf.images[0] : '/x'), { headers: { cookie: owner.join('; ') } });
  check('PDF 쪽 그림을 받을 수 있음 (PNG)', assetOk.status === 200 && /image\/png/.test(assetOk.headers.get('content-type') || ''), assetOk.status);
  await P.shot('files-added');

  // 2. 출력 탭 (클라우드 출력, 같은 브라우저 — 가짜 카메라도 같음)
  const ot = await (await fetch(cdpBase + '/json/new?' + BASE + '/outputs/main', { method: 'PUT' })).json();
  const O = await connect(ot);
  await O.send('Emulation.setDeviceMetricsOverride', { width: 1280, height: 720, deviceScaleFactor: 1, mobile: false });
  await sleep(1500);
  await P.send('Page.bringToFront');
  const viewer = wsock('/events?client=main', { cookie: owner.join('; '), origin: BASE });
  await viewer.open;
  const rowY = (i) => 203 + i * 30; // 큐시트 줄 (0부터)
  const sendRow = async (i) => { await click(150, rowY(i)); await sleep(500); await click(310, 60); await sleep(1500); };
  // 2-1. 영상
  let vr = null;
  for (let k = 0; k < 2 && !vr; k++) { await sendRow(n0 + 1); vr = await viewer.wait((m) => m.t === 'render' && m.data && m.data.video && m.data.video.key, 6000); }
  check('영상 송출 → 클라우드 출력 자료에 영상', vr, vr && vr.data.video && vr.data.video.key);
  await O.send('Page.bringToFront'); await sleep(2500);
  const vplay = await O.ev(`(()=>{ const v=[...document.querySelectorAll('video')].find(x=>x.src && x.src.includes('/api/v1/assets/')); return v ? {ready:v.readyState, t:v.currentTime, paused:v.paused} : null })()`);
  check('출력 탭에서 영상이 재생됨', vplay && vplay.ready >= 2 && vplay.t > 0.2, vplay);
  await O.shot('out-video');
  await P.send('Page.bringToFront');
  // 2-2. PDF 슬라이드쇼
  let pimg = null;
  for (let k = 0; k < 2 && !(pimg && pimg.w > 0); k++) {
    await P.send('Page.bringToFront');
    await sendRow(n0 + 3);
    await O.send('Page.bringToFront');
    for (let i = 0; i < 15 && !(pimg && pimg.w > 0); i++) { await sleep(300); pimg = await O.ev(`(()=>{ const i=[...document.querySelectorAll('img')].find(x=>x.src.includes('/api/v1/assets/')); return i ? {w:i.naturalWidth, ok:i.complete} : null })()`); }
  }
  check('출력 탭에 PDF 쪽 그림', pimg && (pimg.w > 0 || /assets/.test(String(pimg))), pimg);
  await O.shot('out-pdf');
  await P.send('Page.bringToFront');

  // 3. 카메라: 카메라 설정 창 · 이 페이지가 찾기(가짜 카메라) → 순서 → 출력 탭에서 이름으로 열림
  await P.ev("window.wpMenu('b:cameras'); 1");
  await P.ev('window.wpProbeCameras(); 1');
  for (let i = 0; i < 60; i++) { await sleep(250); d = await dbg(); if (!d.camProbing && (d.cameras || []).some((c) => /fake/i.test(c[1]))) break; }
  const cam = (d.cameras || []).find((c) => /fake/i.test(c[1]));
  check('카메라 찾기 (웹: 이 페이지가 직접) → 가짜 카메라 이름', cam && !d.camProbing, { cameras: d.cameras, probing: d.camProbing });
  await P.shot('cameras');
  await P.ev("window.wpMenu('b:cameras'); 1"); // 창 닫기 (다시 누르면 닫힘 — 아니면 그대로)
  if (cam) {
    await P.ev(`window.wpAddItem(${JSON.stringify(JSON.stringify({ id: 'icam' + Date.now().toString(36), type: 'camera', title: cam[1], note: '', deviceId: cam[0], label: cam[1], fit: 'cover', duration: 0, tplMain: '', tplOverlay: '' }))}); 1`);
    await sleep(800);
    d = await dbg();
    await sendRow(d.cue.items.length - 1);
    await O.send('Page.bringToFront');
    let cv = null;
    for (let i = 0; i < 30 && !(cv && cv.w > 0); i++) { await sleep(300); cv = await O.ev(`(()=>{ const v=[...document.querySelectorAll('video')].find(x=>x.srcObject); return v ? {ready:v.readyState, w:v.videoWidth, live:v.srcObject.getVideoTracks().map(t=>t.label)} : null })()`); }
    check('출력 탭에서 카메라가 열림 (같은 이름)', cv && cv.w > 0, cv);
    await O.shot('out-camera');
    await P.send('Page.bringToFront');
  }

  // 4. 휴대폰 카메라 창: 링크 · QR
  await click(b.phone[0], b.phone[1]); await sleep(500);
  await P.shot('phone-window');
  const pl = await api(owner, 'POST', '/api/v1/media/phone-link', { name: '시험' });
  check('휴대폰 카메라 링크 (POST /api/v1/media/phone-link)', pl.status === 200 && /phonecam\?k=/.test(pl.json && pl.json.url), pl.status);

  // 5. 화면 구성: 좌우 나눔을 만들어 '지금 한 번' → 클라우드 출력 자료 · 출력 탭의 data-scene
  const sid = await P.ev("window.wpSceneMake('split')");
  check('화면 구성 만들기 (좌우 나눔)', sid && !/오류/.test(sid), sid);
  viewer.q.length = 0;
  const tr = await P.ev(`window.wpScene('main', ${JSON.stringify(sid)}, false)`);
  const sr = await viewer.wait((m) => m.t === 'render' && m.data && m.data.scene && m.data.scene.id === sid, 6000);
  check('구성 바꾸기 → 클라우드 출력 자료의 scene', sr, { tr, scene: sr && sr.data.scene.id, transition: sr && sr.data.sceneTransition });
  await O.send('Page.bringToFront'); await sleep(1500);
  const ds = await O.ev('document.body.dataset.scene || ""');
  check('출력 탭이 그 구성으로 (body data-scene)', ds === sid, ds);
  await O.shot('out-scene');
  await P.send('Page.bringToFront');
  await P.ev("window.wpMenu('c:scenes'); 1"); await sleep(800);
  await P.shot('scene-editor');
  await P.ev(`window.wpScene('main', 'full', false)`);
  // 웹 페이지 소스 (인스펙터 소스 탭과 같은 길) → 클라우드 출력 탭에 iframe
  const wsrc = await P.ev(`window.wpSaveSource(JSON.stringify({ kind: 'web', name: '시험 웹', url: 'https://example.com/', on: true, outputs: ['main'] }))`);
  await O.send('Page.bringToFront'); let ifr = null;
  for (let i = 0; i < 20 && !ifr; i++) { await sleep(300); ifr = await O.ev(`(()=>{ const f=[...document.querySelectorAll('iframe')].find(x=>(x.src||'').startsWith('https://example.com')); return f ? f.src : null })()`); }
  check('웹 페이지 소스 → 클라우드 출력 탭에 iframe', ifr, { wsrc, ifr });
  await P.send('Page.bringToFront');

  // 6. AI: 기다리는 동안 화면이 멈추지 않음 (답을 비동기로 받고 일을 다시 돌림)
  await api(owner, 'PUT', '/api/v1/ai/settings', { endpoint: 'mock://e2e', model: 'mock-model', key: 'sk-e2e-test' });
  await api(owner, 'POST', '/api/v1/ai/consent', { feature: 'cuecheck', agree: true });
  // 답이 1.5초 늦게 오게 (네트워크 지연 흉내) — 그 동안 화면 그리기(requestAnimationFrame)가 계속되면 멈추지 않은 것
  await P.send('Network.enable');
  await P.send('Network.emulateNetworkConditions', { offline: false, latency: 1500, downloadThroughput: -1, uploadThroughput: -1 });
  const t0 = await P.ev(`(async () => { window.__frames = 0; (function f(){ window.__frames++; if (window.__frames < 100000) requestAnimationFrame(f); })(); window.wpAiResult = null; window.wpAiTest('cuecheck'); const t = performance.now(); let waiting = false; while (!window.wpAiResult && performance.now() - t < 8000) { const d = JSON.parse(window.wpDebug() || '{}'); if (d.aiWaiting > 0 || window.wpAiWaiting()) waiting = true; await new Promise(r => setTimeout(r, 10)); } return { ms: performance.now() - t, waiting, frames: window.__frames, result: window.wpAiResult }; })()`);
  await P.send('Network.emulateNetworkConditions', { offline: false, latency: 0, downloadThroughput: -1, uploadThroughput: -1 });
  const ar = t0.result ? JSON.parse(t0.result) : null;
  // 1.5초 넘게 기다리는 동안 화면을 60번 넘게 그렸으면 멈추지 않은 것 (동기 요청이면 그동안 0번)
  check('AI 답을 기다리는 동안 화면이 멈추지 않음 (비동기 · 기다리는 표시)', ar && ar.ok && t0.waiting && t0.ms > 1400 && t0.frames > 60, { ms: Math.round(t0.ms), waiting: t0.waiting, frames: t0.frames, ok: ar && ar.ok });
  await P.shot('ai-waiting');

  fs.writeFileSync(path.join(out, 'result.json'), JSON.stringify({ results, logs: P.logs.slice(-40), outLogs: O.logs.slice(-20) }, null, 1));
  console.error(P.logs.filter((l) => /error|오류|panic/i.test(l)).slice(-8).join('\n'));
  const fails = results.filter((x) => !x.ok).length;
  console.log(`${results.length - fails}/${results.length} 통과`);
  try { viewer.s.close(); P.sock.close(); O.sock.close(); } catch {}
  proc.kill();
  process.exit(fails ? 1 : 0);
})().catch((e) => { console.error(e); process.exit(2); });
