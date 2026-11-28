// [웹판] 브라우저에 조작 화면을 띄워 여는 시간 · 콘솔 · 화면 그림을 남긴다 (외부 모듈 없이 CDP).
// node tools/web/probe.js [--browser edge|chrome] [--url http://127.0.0.1:37610/] [--out tmp/web-probe] [--headful] [--keep]
//   --steps <파일.js> : 열린 뒤 할 일 (module.exports = async (page, ctx) => {...}), ctx = { out, shot(name), sleep, key, type, ime }
'use strict';
const { spawn } = require('child_process');
const fs = require('fs');
const path = require('path');

const args = process.argv.slice(2);
const opt = (n, d) => { const i = args.indexOf('--' + n); return i >= 0 ? args[i + 1] : d; };
const flag = (n) => args.includes('--' + n);
const BROWSERS = {
  edge: ['C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe', 'C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe'],
  chrome: ['C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe', path.join(process.env.LOCALAPPDATA || '', 'Google\\Chrome\\Application\\chrome.exe')],
};
const which = opt('browser', 'edge');
const exe = BROWSERS[which].find((p) => fs.existsSync(p));
const url = opt('url', 'http://127.0.0.1:37610/?debug=1');
const out = path.resolve(opt('out', 'tmp/web-probe'));
const W = +opt('w', 1600), H = +opt('h', 960);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

(async () => {
  if (!exe) throw new Error(which + ' 를 찾지 못했습니다');
  fs.mkdirSync(out, { recursive: true });
  const port = 9600 + Math.floor(Math.random() * 300);
  let prof = path.join(out, 'profile-' + which);
  if (!flag('keep-profile')) { try { fs.rmSync(prof, { recursive: true, force: true }); } catch { prof += '-' + Date.now(); } }
  const bargs = [`--remote-debugging-port=${port}`, `--user-data-dir=${prof}`, '--no-first-run', '--no-default-browser-check', '--force-device-scale-factor=1',
    // 창 크기: 앱 창(--app)은 테두리 16 · 제목 줄 39 를 더해야 페이지가 W×H 가 된다 (headless 와 같은 좌표로 누르려고)
    flag('headful') ? `--window-size=${W + 16},${H + 39}` : `--window-size=${W},${H}`, '--window-position=40,40', ...(flag('headful') ? [] : ['--headless=new']), '--enable-unsafe-swiftshader', '--disable-features=CalculateNativeWinOcclusion', '--disable-backgrounding-occluded-windows', '--disable-renderer-backgrounding', flag('app') ? `--app=${url}` : flag('headful') ? url : 'about:blank'];
  const proc = spawn(exe, bargs, { stdio: 'ignore' });
  const base = `http://127.0.0.1:${port}`;
  for (let i = 0; i < 100; i++) { try { if ((await fetch(base + '/json/version')).ok) break; } catch {} await sleep(150); }
  let tab;
  for (let i = 0; i < 100 && !tab; i++) { tab = (await (await fetch(base + '/json/list')).json()).find((t) => t.type === 'page'); if (!tab) await sleep(150); }
  console.error('탭:', tab && tab.url);
  const ws = new WebSocket(tab.webSocketDebuggerUrl);
  await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
  let id = 0; const wait = new Map(); const logs = [];
  ws.onmessage = (ev) => {
    const m = JSON.parse(ev.data);
    if (m.id && wait.has(m.id)) { const { res, rej } = wait.get(m.id); wait.delete(m.id); m.error ? rej(new Error(m.error.message)) : res(m.result); }
    else if (m.method === 'Runtime.consoleAPICalled') logs.push(`[${m.params.type}] ` + m.params.args.map((a) => a.value ?? a.description).join(' '));
    else if (m.method === 'Runtime.exceptionThrown') logs.push('[exception] ' + (m.params.exceptionDetails.exception?.description || m.params.exceptionDetails.text));
  };
  const send = (method, params = {}) => new Promise((res, rej) => { const i = ++id; wait.set(i, { res, rej }); ws.send(JSON.stringify({ id: i, method, params })); });
  const evalJs = async (expr) => { const r = await send('Runtime.evaluate', { expression: expr, awaitPromise: true, returnByValue: true }); if (r.exceptionDetails) throw new Error(r.exceptionDetails.text); return r.result.value; };
  await send('Page.enable'); await send('Runtime.enable');
  if (!flag('headful')) await send('Emulation.setDeviceMetricsOverride', { width: W, height: H, deviceScaleFactor: 1, mobile: false });
  await send('Network.enable');
  if (flag('nocache')) await send('Network.setCacheDisabled', { cacheDisabled: true });
  // --net 50,20 : 내려받기 50Mbps · 지연 20ms 로 흉내 (교회 인터넷 가정)
  const net = opt('net');
  if (net) { const [mbps, lat] = net.split(',').map(Number); await send('Network.emulateNetworkConditions', { offline: false, latency: lat, downloadThroughput: mbps * 1e6 / 8, uploadThroughput: 10e6 / 8 }); }
  const t0 = Date.now();
  // 창을 띄워 보는 때는 주소를 처음부터 연다 (about:blank 로 열고 옮기면 주소 표시줄이 키보드 초점을 쥐고 있다)
  if (!flag('headful')) await send('Page.navigate', { url });
  let times = null;
  for (let i = 0; i < 600; i++) {
    await sleep(200);
    try { times = await evalJs('JSON.stringify(window.wpTimes||null)'); times = JSON.parse(times); } catch { times = null; }
    if (times && times.painted) break;
    const err = await evalJs("document.getElementById('boot')?.classList.contains('error') ? document.getElementById('boot_msg').textContent : ''").catch(() => '');
    if (err) { logs.push('[boot error] ' + err); break; }
  }
  const gl = await evalJs("(()=>{const c=document.createElement('canvas').getContext('webgl2');if(!c)return 'none';const e=c.getExtension('WEBGL_debug_renderer_info');return e?c.getParameter(e.UNMASKED_RENDERER_WEBGL):'webgl2'})()").catch((e) => String(e));
  const shot = async (name) => {
    const r = await send('Page.captureScreenshot', { format: 'png' });
    const f = path.join(out, `${which}-${name}.png`);
    fs.writeFileSync(f, Buffer.from(r.data, 'base64'));
    return f;
  };
  // 키 · 글 넣기 (CDP 합성: 실제 IME 시험을 대신하지 않는다)
  const key = async (k, code, vk, mods = 0, text) => {
    await send('Input.dispatchKeyEvent', { type: text ? 'keyDown' : 'rawKeyDown', key: k, code, windowsVirtualKeyCode: vk, modifiers: mods, text });
    await send('Input.dispatchKeyEvent', { type: 'keyUp', key: k, code, windowsVirtualKeyCode: vk, modifiers: mods });
  };
  const type = (t) => send('Input.insertText', { text: t });
  const ime = (t, s = t.length, e = t.length) => send('Input.imeSetComposition', { text: t, selectionStart: s, selectionEnd: e });
  const click = async (x, y, n = 1) => {
    for (const type of ['mousePressed', 'mouseReleased']) await send('Input.dispatchMouseEvent', { type, x, y, button: 'left', clickCount: n });
  };
  // 실제 운영체제 키 입력 (tools/web/ime-keys.ps1). 창 배율은 devicePixelRatio
  const dpr = await evalJs('window.devicePixelRatio');
  const ps = (actions) => require('child_process').execFileSync('powershell', ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', path.join(__dirname, 'ime-keys.ps1'), '-BrowserPid', String(proc.pid), '-Scale', String(dpr), '-Title', '워십프리젠터', '-Actions', actions], { encoding: 'utf8' }).trim();
  const dbg = async () => JSON.parse(await evalJs('window.wpDebug ? window.wpDebug() : "null"'));
  console.error('열림:', JSON.stringify(times));
  // 다른 탭(출력 화면 등)을 열어 다룬다: { eval, shot(name), close, logs }
  const tab2 = async (u, w = 1280, h = 720) => {
    const t = await (await fetch(base + '/json/new?' + u, { method: 'PUT' })).json();
    const ws2 = new WebSocket(t.webSocketDebuggerUrl);
    await new Promise((res, rej) => { ws2.onopen = res; ws2.onerror = rej; });
    let i2 = 0; const w2 = new Map(); const lg = [];
    ws2.onmessage = (ev) => { const m = JSON.parse(ev.data); if (m.id && w2.has(m.id)) { const { res, rej } = w2.get(m.id); w2.delete(m.id); m.error ? rej(new Error(m.error.message)) : res(m.result); } else if (m.method === 'Runtime.consoleAPICalled') lg.push(m.params.args.map((a) => a.value ?? a.description).join(' ')); };
    const s2 = (method, params = {}) => new Promise((res, rej) => { const k = ++i2; w2.set(k, { res, rej }); ws2.send(JSON.stringify({ id: k, method, params })); });
    await s2('Page.enable'); await s2('Runtime.enable');
    await s2('Emulation.setDeviceMetricsOverride', { width: w, height: h, deviceScaleFactor: 1, mobile: false });
    return {
      logs: lg, send: s2,
      eval: async (e) => { const r = await s2('Runtime.evaluate', { expression: e, awaitPromise: true, returnByValue: true }); return r.result.value; },
      shot: async (name) => { await s2('Page.bringToFront'); const r = await s2('Page.captureScreenshot', { format: 'png' }); const f = path.join(out, `${which}-${name}.png`); fs.writeFileSync(f, Buffer.from(r.data, 'base64')); await send('Page.bringToFront'); return f; },
      close: async () => { try { await fetch(base + '/json/close/' + t.id); } catch {} },
    };
  };
  const targets = async () => (await (await fetch(base + '/json/list')).json()).map((t) => ({ type: t.type, url: t.url }));
  const result = { browser: which, exe, gl, url, dpr, wallMs: Date.now() - t0, times, logs };
  result.boot = await shot('boot');
  const steps = opt('steps');
  if (steps && times && times.painted) {
    const fn = require(path.resolve(steps));
    result.steps = await fn({ send, eval: evalJs }, { out, shot, sleep, key, type, ime, click, logs, ps, dbg, pid: proc.pid, tab2, targets, base });
  }
  fs.writeFileSync(path.join(out, `${which}-result.json`), JSON.stringify(result, null, 1));
  console.log(JSON.stringify({ ...result, logs: logs.slice(-30) }, null, 1));
  if (!flag('keep')) { try { ws.close(); } catch {} proc.kill(); }
  process.exit(0);
})().catch((e) => { console.error(e); process.exit(1); });
