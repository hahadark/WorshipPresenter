// 유튜브 순서 점검: 앱을 유튜브 영상 순서를 송출한 채로 띄워 두고
//   node tools/youtube/e2e.js --port 37628 --out yt.png
// ① 예배당 화면(/outputs/main)에 유튜브 임베드가 뜨고 재생되는가 ② 코어에 영상 길이가 들어가는가(videoMeta)
// ③ 조작(videoControl pause · seek)이 임베드에 따라가는가 ④ OBS 화면(/outputs/overlay)도 같은 위치인가
'use strict';
const path = require('path');
const { launch, sleep } = require('../flow-compare/cdp.js');
const arg = (k, d) => { const i = process.argv.indexOf(k); return i > 0 ? process.argv[i + 1] : d; };
const port = arg('--port', '37528'), out = arg('--out', 'yt.png');
const base = `http://127.0.0.1:${port}`;
const ok = (c, m) => { console.log((c ? '통과: ' : '실패: ') + m); if (!c) process.exitCode = 1; };
const cmd = async (name, args) => (await fetch(base + '/api/command', { method: 'POST', headers: { 'Content-Type': 'application/json', 'X-WorshipCue': '1' }, body: JSON.stringify({ cmd: name, args }) })).json();
const yt = `(() => { const f = document.querySelector('iframe.bc-yt'); return f ? { src: f.src, ...(f._yt || {}) } : null; })()`;
(async () => {
  const b = await launch(path.join(__dirname, '..', '..', 'tmp', 'yt-profile'));
  const main = await b.open(base + '/outputs/main', 1280, 720);
  const obs = await b.open(base + '/outputs/overlay', 1280, 720);
  if (!(await main.waitFor(`document.querySelector('iframe.bc-yt')`, 20000))) { ok(false, '예배당 화면에 유튜브 임베드가 없음'); process.exit(1); }
  ok(true, '예배당 화면에 유튜브 임베드');
  ok(await main.waitFor(`(() => { const f = document.querySelector('iframe.bc-yt'); return f && f._yt && f._yt.ready && f._yt.state === 1; })()`, 30000), '재생 중 (playerState 1): ' + JSON.stringify(await main.eval(yt)));
  await sleep(3000);
  const s1 = await main.eval(yt);
  ok(s1 && s1.time > 1 && s1.dur > 0, `재생 위치 ${s1 && s1.time.toFixed(1)}초 · 길이 ${s1 && s1.dur}초`);
  await main.shot(out);
  console.log('그림: ' + out);
  const r = await cmd('videoControl', { action: 'pause' });
  ok(r.ok && r.result && r.result.playing === false, '조작: 멈춤');
  await sleep(2500);
  const s2 = await main.eval(yt);
  ok(s2.state === 2, `임베드도 멈춤 (state ${s2.state})`);
  await cmd('videoControl', { action: 'seek', value: 10 });
  await cmd('videoControl', { action: 'play' });
  await sleep(3000);
  const s3 = await main.eval(yt), s4 = await obs.eval(yt);
  ok(s3.state === 1 && s3.time >= 10 && s3.time < 16, `+위치 옮김 뒤 재생 (${s3.time.toFixed(1)}초)`);
  ok(s4 && Math.abs(s4.time - s3.time) < 2.5, `OBS 화면도 같은 위치 (${s4 && s4.time.toFixed(1)}초)`);
  await obs.shot(out.replace(/\.png$/, '-obs.png'));
  await main.close(); await obs.close(); b.proc.kill();
})().catch((e) => { console.error(e); process.exit(1); });
