// 머리 없는(headless) 엣지/크롬을 띄워 CDP 로 다루는 작은 도구 (외부 모듈 없이, Node 22+ 의 WebSocket·fetch).
// open(url, w, h) → { eval(식), shot(파일), close() }
'use strict';
const { spawn } = require('child_process');
const fs = require('fs');
const path = require('path');

const CANDIDATES = [
  process.env.FLOW_BROWSER,
  'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe',
  'C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe',
  'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe',
].filter(Boolean);

const sleep = ms => new Promise(r => setTimeout(r, ms));

async function launch(profileDir, port = 9400 + Math.floor(Math.random() * 400), extraArgs = []) {
  const exe = CANDIDATES.find(p => fs.existsSync(p));
  if (!exe) throw new Error('엣지/크롬을 찾지 못했습니다 (FLOW_BROWSER 로 지정)');
  fs.mkdirSync(profileDir, { recursive: true });
  const proc = spawn(exe, [`--remote-debugging-port=${port}`, `--user-data-dir=${path.resolve(profileDir)}`, '--headless=new', '--no-first-run',
    '--disable-gpu', '--hide-scrollbars', '--force-device-scale-factor=1', '--autoplay-policy=no-user-gesture-required', ...extraArgs, 'about:blank'], { stdio: 'ignore' });
  const base = `http://127.0.0.1:${port}`;
  for (let i = 0; i < 100; i++) {
    try { const r = await fetch(base + '/json/version'); if (r.ok) break; } catch {}
    await sleep(150);
  }
  // 브라우저 전체 연결 (창 만들기)
  const bws = new WebSocket((await (await fetch(base + '/json/version')).json()).webSocketDebuggerUrl);
  await new Promise((res, rej) => { bws.onopen = res; bws.onerror = rej; });
  let bid = 0; const bwait = new Map();
  bws.onmessage = ev => { const m = JSON.parse(ev.data); if (m.id && bwait.has(m.id)) { const { res, rej } = bwait.get(m.id); bwait.delete(m.id); m.error ? rej(new Error(m.error.message)) : res(m.result); } };
  const bsend = (method, params = {}) => new Promise((res, rej) => { const i = ++bid; bwait.set(i, { res, rej }); bws.send(JSON.stringify({ id: i, method, params })); });
  const browser = {
    proc,
    async open(url, w = 1920, h = 1080) {
      // 탭마다 따로 창을 연다: 한 창의 뒤 탭은 '안 보임' 이라 영상이 멈추고 타이머가 늦어져, 영상 끝남·자동 넘김이 한쪽만 늦게 일어난다
      const t = { id: (await bsend('Target.createTarget', { url: 'about:blank', newWindow: true })).targetId };
      const ws = new WebSocket(`ws://127.0.0.1:${port}/devtools/page/${t.id}`);
      await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
      let id = 0; const wait = new Map(); const logs = [];
      ws.onmessage = ev => {
        const m = JSON.parse(ev.data);
        if (m.id && wait.has(m.id)) { const { res, rej } = wait.get(m.id); wait.delete(m.id); m.error ? rej(new Error(m.error.message)) : res(m.result); }
        else if (m.method === 'Runtime.consoleAPICalled') logs.push(m.params.args.map(a => a.value).join(' '));
      };
      const send = (method, params = {}) => new Promise((res, rej) => { const i = ++id; wait.set(i, { res, rej }); ws.send(JSON.stringify({ id: i, method, params })); });
      await send('Page.enable'); await send('Runtime.enable');
      await send('Emulation.setDeviceMetricsOverride', { width: w, height: h, deviceScaleFactor: 1, mobile: false });
      await send('Page.navigate', { url });
      const page = {
        id: t.id, logs,
        async eval(expr, timeout = 30000) {
          const r = await Promise.race([
            send('Runtime.evaluate', { expression: expr, awaitPromise: true, returnByValue: true }),
            sleep(timeout).then(() => { throw new Error('eval 시간 초과: ' + expr.slice(0, 80)); }),
          ]);
          if (r.exceptionDetails) throw new Error('페이지 오류: ' + (r.exceptionDetails.exception?.description || r.exceptionDetails.text));
          return r.result.value;
        },
        async waitFor(expr, timeout = 15000) {
          const t0 = Date.now();
          while (Date.now() - t0 < timeout) { try { if (await page.eval(expr)) return true; } catch {} await sleep(200); }
          return false;
        },
        // 화면이 다 그려질 때까지: 진행 중인 전환(페이드)이 없고, 그림이 다 읽혔고, 겹쳐 있던 옛 슬라이드가 치워지고,
        // 화면 내용이 연달아 세 번 같을 때. (글이 뜨기 전에 찍어 '둘 다 검은 화면 = 같음' 으로 잘못 판정하던 것 막기)
        async settle(maxMs = 6000) {
          const probe = `(() => { const run = document.getAnimations().filter(a => a.playState === 'running' && !(a.effect && a.effect.getComputedTiming && a.effect.getComputedTiming().iterations === Infinity)).length;
            const imgs = [...document.images].filter(i => !i.complete).length + [...document.querySelectorAll('video')].filter(v => v.getAttribute('src') && v.offsetParent !== null && !v.error && (v.readyState < 3 || (!v.paused && v.currentTime < 0.35))).length; // 영상은 첫 그림이 그려진 뒤에
            const areas = document.querySelectorAll('.bc-stage:not(.bc-measure .bc-stage) > .bc-area').length;
            const fading = [...document.querySelectorAll('.bc-area,.bc-bg,.bc-vlayer,.bc-logoscreen,.bc-medialayer')].filter(e => { const o = parseFloat(getComputedStyle(e).opacity); return o > 0.001 && o < 0.999; }).length;
            return JSON.stringify([run, imgs, areas > 1 ? areas : 0, fading, document.body.innerHTML.length, (document.body.innerText || '').replace(/\\d/g, '#').length]); })()`;
          const t0 = Date.now(); let last = '', same = 0;
          await sleep(250);
          while (Date.now() - t0 < maxMs) {
            let cur = '';
            try { cur = await page.eval(probe, 5000); } catch { cur = ''; }
            const [run, imgs, areas, fading] = cur ? JSON.parse(cur) : [1, 1, 1, 1];
            if (cur && cur === last && !run && !imgs && !areas && !fading) { if (++same >= 2) return true; } else same = 0;
            last = cur;
            await sleep(120);
          }
          return false;
        },
        async shot(file) {
          // 뒤에 있는 탭은 그리지 않아 캡처가 끝나지 않는다 → 앞으로 가져온 뒤 찍고, 그래도 안 끝나면 오류
          await send('Page.bringToFront');
          const r = await Promise.race([send('Page.captureScreenshot', { format: 'png' }), sleep(15000).then(() => { throw new Error('스크린샷 시간 초과'); })]);
          const buf = Buffer.from(r.data, 'base64');
          if (file) fs.writeFileSync(file, buf);
          return buf;
        },
        async close() { try { await fetch(`${base}/json/close/${t.id}`); } catch {} try { ws.close(); } catch {} },
      };
      await sleep(300);
      return page;
    },
    close() { try { bws.close(); } catch {} try { proc.kill(); } catch {} },
  };
  return browser;
}

module.exports = { launch, sleep };
