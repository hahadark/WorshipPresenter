// 화면 구성(로드맵 2단계) 점검: 실제 웹 출력(output.js)을 머리 없는 엣지로 찍는다.
//   node tools/scenes/e2e.js --exe target\debug\wp-app.exe --old-exe tmp\oldrun\wp-app.exe --data tmp\data --out tmp\scene-e2e
// ① 구성을 하나도 안 고른 자료: 새 output.js 와 2.0.8 output.js(--old-exe 옆 web\) 의 /outputs/main · /outputs/overlay 가 화소까지 같은가
// ② 시험 기준 화면: 방송(OBS) = 카메라 전체 + 아래 성경 자막, 예배당 = 성경 본문 전체 — 구성을 방송에만 골라 두고 두 출력을 찍는다
//    (카메라는 이 PC 의 기본 카메라 — 가상 카메라. 브라우저 허락 창은 --use-fake-ui-for-media-stream 으로 건너뜀)
// ③ 좌우 나눔 구성을 예배당에: 칸 자리 · 칸 크기 글자 맞춤 확인
// 앱은 자료 폴더를 복사해서 띄우고, 끌 때는 이 스크립트가 띄운 PID 만 끈다.
'use strict';
const fs = require('fs');
const path = require('path');
const { spawn } = require('child_process');
const { launch, sleep } = require('../flow-compare/cdp.js');
const png = require('../flow-compare/png.js');
const arg = (k, d) => { const i = process.argv.indexOf(k); return i > 0 ? process.argv[i + 1] : d; };
const exe = path.resolve(arg('--exe', 'target/debug/wp-app.exe'));
const oldExe = arg('--old-exe', '');
const data0 = path.resolve(arg('--data', 'tmp/data'));
const out = path.resolve(arg('--out', 'tmp/scene-e2e'));
const ok = (c, m) => { console.log((c ? '통과: ' : '실패: ') + m); if (!c) process.exitCode = 1; };
fs.mkdirSync(out, { recursive: true });

function copyData(name, patch) {
  const d = path.join(out, 'data-' + name);
  fs.rmSync(d, { recursive: true, force: true });
  fs.mkdirSync(d, { recursive: true });
  for (const f of ['bible.db', 'settings.json']) fs.copyFileSync(path.join(data0, f), path.join(d, f));
  for (const dir of ['media']) if (fs.existsSync(path.join(data0, dir))) fs.cpSync(path.join(data0, dir), path.join(d, dir), { recursive: true });
  const sp = path.join(d, 'settings.json');
  const s = JSON.parse(fs.readFileSync(sp, 'utf8'));
  // 점검 격리: 업데이트 · 통계 · 전역 단축키 · 리모컨 · 내부망 · 동기화 끔
  Object.assign(s, { autoUpdate: false, telemetry: false, globalShortcuts: false, syncDir: '' });
  if (s.remote) s.remote.enabled = false;
  if (s.lanOutput) s.lanOutput.enabled = false;
  (s.sources || []).forEach(x => { x.on = false; });
  delete s.scenes; delete s.outputScenes;
  if (patch) patch(s);
  fs.writeFileSync(sp, JSON.stringify(s, null, 2));
  return d;
}

async function runApp(exePath, data) {
  const legacy = path.join(out, 'empty-legacy');
  fs.mkdirSync(legacy, { recursive: true });
  const p = spawn(exePath, ['--data', data, '--win', '1280x720', '--search', '요3:16', '--send'], { env: { ...process.env, WP_LEGACY_DATA: legacy }, stdio: ['ignore', 'pipe', 'pipe'] });
  let log = '';
  p.stdout.on('data', b => { log += b; });
  p.stderr.on('data', b => { log += b; });
  for (let i = 0; i < 120; i++) {
    const m = log.match(/로컬 서버: http:\/\/127\.0\.0\.1:(\d+)/);
    if (m) { await sleep(2500); return { proc: p, port: m[1] }; }
    await sleep(250);
  }
  p.kill();
  throw new Error('앱 서버 주소를 찾지 못함:\n' + log.slice(-2000));
}

async function shoot(browser, port, which, file, waitCam) {
  const page = await browser.open(`http://127.0.0.1:${port}/outputs/${which}`, 1920, 1080);
  await page.waitFor(`!!document.querySelector('.bc-stage .bc-area')`, 15000);
  if (waitCam) await page.waitFor(`[...document.querySelectorAll('.wp-cam video')].some(v => v.readyState >= 2 && v.videoWidth > 0)`, 15000);
  await page.settle(8000);
  await sleep(600);
  const info = await page.eval(`(() => { const v = document.querySelector('.wp-cam video'); return JSON.stringify({ layers: [...document.querySelectorAll('.wp-layer')].map(e => e.dataset.source), cam: v ? [v.videoWidth, v.videoHeight, v.readyState] : null, fontPx: (document.querySelector('.bc-text') || {}).style ? document.querySelector('.bc-text').style.fontSize : '' }); })()`);
  const buf = await page.shot(file);
  await page.close();
  return { img: png.decode(buf || fs.readFileSync(file)), info: JSON.parse(info) };
}

(async () => {
  // --fake-cam: 브라우저의 시험 카메라(움직이는 무늬) — 실제 가상 카메라가 화면 내용을 내보내고 있으면 캡처에 남지 않게
  const camArgs = process.argv.includes('--fake-cam') ? ['--use-fake-ui-for-media-stream', '--use-fake-device-for-media-stream'] : ['--use-fake-ui-for-media-stream'];
  const browser = await launch(path.join(out, 'profile'), undefined, camArgs);
  const report = {};
  try {
    // ① 기본 구성: 새 output.js 와 2.0.8 output.js
    const runs = [['new', exe]];
    if (oldExe) runs.push(['old', path.resolve(oldExe)]);
    const shots = {};
    for (const [name, ex] of runs) {
      const app = await runApp(ex, copyData('base-' + name));
      try {
        for (const w of ['main', 'overlay']) shots[name + '-' + w] = await shoot(browser, app.port, w, path.join(out, `base-${name}-${w}.png`));
      } finally { app.proc.kill(); await sleep(800); }
    }
    if (oldExe) {
      for (const w of ['main', 'overlay']) {
        const a = shots['new-' + w].img, b = shots['old-' + w].img;
        const d = png.diff(a, b, 0);
        report['base-' + w] = d.ratio;
        ok(a.w === b.w && a.h === b.h && d.ratio === 0, `구성을 안 고른 ${w}: 2.0.8 output.js 와 다른 화소 ${(d.ratio * 100).toFixed(4)}%`);
      }
    }
    // ② 시험 기준: 방송 = 카메라 전체 + 아래 성경 자막 (예배당은 기본 그대로)
    const sc = { id: 'sc-cam-sub', name: '카메라 + 아래 자막', layers: [
      { id: 'l-cam', source: 'camera:', rect: [0, 0, 1, 1] },
      { id: 'l-live', source: 'live', rect: [0, 0, 1, 1], tpl: 'overlay' }] };
    const split = { id: 'sc-split', name: '좌우 나눔', layers: [
      { id: 'l-bg', source: 'background', rect: [0, 0, 1, 1] },
      { id: 'l-cam', source: 'camera:', rect: [0, 0, 0.5, 1], crop: [0.1, 0, 0.1, 0] },
      { id: 'l-live', source: 'live', rect: [0.5, 0, 0.5, 1], radius: 24, border: { width: 6, color: '#ffffff' } }] };
    const app = await runApp(exe, copyData('scene', s => { s.scenes = [sc, split]; s.outputScenes = { overlay: 'sc-cam-sub' }; }));
    try {
      const obs = await shoot(browser, app.port, 'overlay', path.join(out, 'criterion-obs.png'), true);
      const main = await shoot(browser, app.port, 'main', path.join(out, 'criterion-main.png'));
      report.criterion = { obs: obs.info, main: main.info };
      ok(obs.info.layers.slice(0, 2).join() === 'camera:,live', '방송 화면 칸 차례: 카메라 → 순서 슬라이드 (' + obs.info.layers.join(' · ') + ')');
      ok(!!obs.info.cam && obs.info.cam[0] > 0, '방송 화면에 카메라 영상이 들어옴 ' + JSON.stringify(obs.info.cam));
      ok(main.info.layers[0] === 'background' && !main.info.layers.some(x => x.startsWith('camera:')), '예배당은 기본 구성 그대로 (카메라 없음)');
      ok(fs.existsSync(path.join(out, 'base-new-main.png')) && png.diff(main.img, shots['new-main'].img, 0).ratio === 0, '예배당 화면: 구성 고르기 전과 화소까지 같음');
    } finally { app.proc.kill(); await sleep(800); }
    // ③ 좌우 나눔을 예배당에
    const app3 = await runApp(exe, copyData('split', s => { s.scenes = [sc, split]; s.outputScenes = { main: 'sc-split' }; }));
    try {
      const m = await shoot(browser, app3.port, 'main', path.join(out, 'split-main.png'), true);
      report.split = m.info;
      ok(m.info.layers.slice(0, 3).join() === 'background,camera:,live', '좌우 나눔 칸 차례 ' + m.info.layers.join(' · '));
      ok(parseFloat(m.info.fontPx) > 0, '오른쪽 칸 글자 크기(칸 크기에 맞춤): ' + m.info.fontPx);
    } finally { app3.proc.kill(); await sleep(800); }
  } finally {
    browser.proc.kill();
    fs.writeFileSync(path.join(out, 'report.json'), JSON.stringify(report, null, 2));
  }
})().catch(e => { console.error(e); process.exitCode = 1; });
