// 진짜 앱이 띄운 웹 화면을 찍는다 (점검 앱을 격리 자료로 띄워 둔 뒤).
//   node tools/webui/real-shots.js --remote http://127.0.0.1:37641/ --rpin 123456 --dock http://127.0.0.1:37529/dock --edit http://127.0.0.1:37646/ --epin 4321 --cam https://<내부망>:37661/cam?k=… --out tmp/craft/real
'use strict';
const os = require('os');
const fs = require('fs');
const path = require('path');
const { open, sleep } = require('./browser.js');
const arg = (k, d) => { const i = process.argv.indexOf(k); return i > 0 ? process.argv[i + 1] : d; };
const out = path.resolve(arg('--out', 'tmp/craft/real'));
fs.mkdirSync(out, { recursive: true });
const login = pin => `(() => { const p = document.getElementById('pin'); if (!p || document.getElementById('login').hidden) return 0; p.value = '${pin}'; p.dispatchEvent(new Event('input', { bubbles: true })); const g = document.getElementById('go'); if (g && !g.disabled) g.click(); return 1; })()`;

(async () => {
  const b = await open(path.join(os.tmpdir(), 'wp-webui-real-' + process.pid), ['--use-fake-device-for-media-stream', '--use-fake-ui-for-media-stream', '--ignore-certificate-errors']);
  const S = [];
  if (arg('--remote')) {
    S.push(['real-remote-390', arg('--remote'), { w: 390, h: 844 }, [login(arg('--rpin'))]]);
    S.push(['real-remote-360', arg('--remote'), { w: 360, h: 740 }, [login(arg('--rpin'))]]);
    S.push(['real-remote-land', arg('--remote'), { w: 844, h: 390 }, [login(arg('--rpin'))]]);
    S.push(['real-remote-tab', arg('--remote'), { w: 820, h: 1180 }, [login(arg('--rpin'))]]);
    S.push(['real-remote-layouts', arg('--remote'), { w: 390, h: 844 }, [login(arg('--rpin')), `document.querySelector('[data-tab="layout"]').click()`]]);
  }
  if (arg('--dock')) S.push(['real-dock', arg('--dock'), { w: 340, h: 520, dpr: 1, mobile: false, touch: false }, []]);
  if (arg('--edit')) {
    S.push(['real-edit-1280', arg('--edit'), { w: 1280, h: 800, dpr: 1, mobile: false, touch: false }, [login(arg('--epin'))]]);
    S.push(['real-edit-1600', arg('--edit'), { w: 1600, h: 960, dpr: 1, mobile: false, touch: false }, [login(arg('--epin'))]]);
    S.push(['real-edit-390', arg('--edit'), { w: 390, h: 844 }, [login(arg('--epin'))]]);
  }
  if (arg('--cam')) {
    S.push(['real-cam-390', arg('--cam'), { w: 390, h: 844 }, []]);
    S.push(['real-cam-land', arg('--cam'), { w: 844, h: 390 }, []]);
  }
  for (const [id, url, dev, steps] of S) {
    const p = await b.page(url, dev);
    await sleep(900);
    for (const s of steps) { try { await p.eval(s); } catch { /* 없음 */ } await sleep(900); }
    await sleep(1500);
    await p.shot(path.join(out, id + '.png'));
    console.log('ok  ' + id);
    await p.close();
  }
  b.close();
  try { b.proc.kill(); } catch { /* 끝남 */ }
  process.exit(0);
})().catch(e => { console.error(e); process.exit(1); });
