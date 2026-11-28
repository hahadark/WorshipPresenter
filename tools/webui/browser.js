// 머리 없는 엣지/크롬을 휴대폰처럼 다룬다 (tools/flow-compare/cdp.js 위에 얹음, 외부 모듈 없음).
// cdp.js 의 page 는 CDP 명령을 직접 보낼 길이 없어, 같은 탭에 둘째 연결을 열어 화면 크기 · 터치 · 안전 영역 · 끊김을 흉내 낸다.
'use strict';
const { launch, sleep } = require('../flow-compare/cdp.js');

async function open(profile, extraArgs = []) {
  const port = 9800 + Math.floor(Math.random() * 150);
  const b = await launch(profile, port, extraArgs);
  return {
    proc: b.proc,
    // dev: { w, h, dpr, mobile, touch }
    async page(url, dev = {}) {
      const { w = 390, h = 844, dpr = 2, mobile = true, touch = true } = dev;
      const p = await b.open('about:blank', w, h);
      const ws = new WebSocket(`ws://127.0.0.1:${port}/devtools/page/${p.id}`);
      await new Promise((res, rej) => { ws.onopen = res; ws.onerror = rej; });
      let id = 0; const wait = new Map();
      ws.onmessage = ev => { const m = JSON.parse(ev.data); if (m.id && wait.has(m.id)) { const { res, rej } = wait.get(m.id); wait.delete(m.id); m.error ? rej(new Error(m.error.message)) : res(m.result); } };
      const send = (method, params = {}) => new Promise((res, rej) => { const i = ++id; wait.set(i, { res, rej }); ws.send(JSON.stringify({ id: i, method, params })); });
      p.send = send;
      p.size = async (w2, h2, d2 = dpr) => { await send('Emulation.setDeviceMetricsOverride', { width: w2, height: h2, deviceScaleFactor: d2, mobile, screenOrientation: w2 > h2 ? { type: 'landscapePrimary', angle: 90 } : { type: 'portraitPrimary', angle: 0 } }); };
      await p.size(w, h);
      if (touch) await send('Emulation.setTouchEmulationEnabled', { enabled: true, maxTouchPoints: 5 });
      // 손가락으로 누르기 (화면 좌표) — 눌림 표시 · 길게 누르기 · 두 번 누르기 시험
      p.tap = async (x, y, holdMs = 30) => {
        await send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ x, y }] });
        await sleep(holdMs);
        await send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
      };
      // 요소 가운데를 누른다 (CSS 고르개)
      p.tapOn = async (sel, holdMs) => {
        const r = await p.eval(`(() => { const e = document.querySelector(${JSON.stringify(sel)}); if (!e) return null; e.scrollIntoView({ block: 'nearest' }); const r = e.getBoundingClientRect(); return [r.left + r.width / 2, r.top + r.height / 2]; })()`);
        if (!r) throw new Error('누를 요소가 없습니다: ' + sel);
        await p.tap(r[0], r[1], holdMs);
      };
      p.key = async (key, code, mods = 0) => { for (const type of ['keyDown', 'keyUp']) await send('Input.dispatchKeyEvent', { type, key, code: code || key, modifiers: mods, windowsVirtualKeyCode: { ArrowDown: 40, ArrowUp: 38, ArrowLeft: 37, ArrowRight: 39, Enter: 13, Escape: 27, Delete: 46, s: 83, z: 90, PageDown: 34, PageUp: 33, ' ': 32 }[key] || 0 }); };
      p.type = async text => { await send('Input.insertText', { text }); };
      p.offline = async on => { await send('Network.enable'); await send('Network.emulateNetworkConditions', { offline: !!on, latency: 0, downloadThroughput: -1, uploadThroughput: -1 }); };
      p.goto = async u => { await send('Page.navigate', { url: u }); await sleep(350); };
      await p.goto(url);
      return p;
    },
    close() { b.close(); },
  };
}

module.exports = { open, sleep };
