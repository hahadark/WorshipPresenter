// 송출 → 예배당 화면이 바뀌기까지 걸리는 시간 (실제 웹 출력 /outputs/main 을 브라우저로 열고, /api/trigger/next 를 부른 때부터
// 화면(무대 DOM)이 바뀐 때까지). 구성 전환(transition) 중 프레임 간격도 잰다 (requestAnimationFrame 간격의 최댓값 · 16.7ms 넘은 수).
// 사용: node tools/solid/latency.js --port <조작 화면 포트> [--key solid-local-tests] [--n 20] [--w 1920 --h 1080] [--out tmp/solid/latency.json]
'use strict';
const fs = require('fs');
const path = require('path');
const http = require('http');
const { launch, sleep } = require('../flow-compare/cdp');
const arg = (n, d) => { const i = process.argv.indexOf('--' + n); return i > 0 ? process.argv[i + 1] : d; };
const port = +arg('port', 0);
const key = arg('key', 'solid-local-tests');
const n = +arg('n', 20);
const w = +arg('w', 1920), h = +arg('h', 1080);
const out = arg('out', 'tmp/solid/latency.json');
if (!port) throw new Error('--port 가 필요합니다');
const get = p => new Promise((ok, no) => http.get({ host: '127.0.0.1', port, path: p }, r => { let b = ''; r.on('data', d => (b += d)); r.on('end', () => ok(b)); }).on('error', no));

const probe = `(() => {
  if (window.__lat) return true;
  window.__lat = { changes: [], gaps: [] };
  const root = document.body;
  new MutationObserver(() => { window.__lat.changes.push(Date.now()); }).observe(root, { subtree: true, childList: true, characterData: true, attributes: true });
  let last = performance.now();
  const tick = t => { window.__lat.gaps.push(t - last); last = t; requestAnimationFrame(tick); };
  requestAnimationFrame(tick);
  return true;
})()`;

async function main() {
  const browser = await launch(path.resolve('tmp/solid/latency-browser'));
  const rows = [];
  try {
    const page = await browser.open(`http://127.0.0.1:${port}/outputs/main`, w, h);
    await page.waitFor('!!document.querySelector("body")');
    await sleep(2000);
    await page.eval(probe);
    for (let i = 0; i < n; i++) {
      await page.eval('window.__lat.changes = []; window.__lat.gaps = []; true');
      const t0 = Date.now();
      await get(`/api/trigger/${i % 2 ? 'prev' : 'next'}?key=${encodeURIComponent(key)}`);
      let first = 0;
      for (let k = 0; k < 100 && !first; k++) {
        await sleep(10);
        const c = await page.eval('window.__lat.changes[0] || 0');
        if (c) first = c;
      }
      await sleep(900); // 바뀐 뒤 전환이 끝날 때까지의 프레임 간격
      const gaps = await page.eval('window.__lat.gaps.slice(1)');
      rows.push({ i, ms: first ? first - t0 : null, maxGap: Math.max(0, ...gaps), slowFrames: gaps.filter(g => g > 1000 / 60 * 1.5).length, frames: gaps.length });
      console.log(JSON.stringify(rows[rows.length - 1]));
    }
    await page.close();
  } finally { browser.close(); }
  const ok = rows.filter(r => r.ms != null).map(r => r.ms).sort((a, b) => a - b);
  const pct = q => ok[Math.min(ok.length - 1, Math.floor(q * ok.length))];
  const summary = { n: rows.length, changed: ok.length, p50: pct(0.5), p90: pct(0.9), max: ok[ok.length - 1], maxFrameGap: Math.max(...rows.map(r => r.maxGap)), slowFrames: rows.reduce((a, r) => a + r.slowFrames, 0), size: `${w}x${h}` };
  fs.mkdirSync(path.dirname(out), { recursive: true });
  fs.writeFileSync(out, JSON.stringify({ summary, rows }, null, 2));
  console.log(JSON.stringify(summary));
}
main().catch(e => { console.error(e.stack); process.exitCode = 1; });
