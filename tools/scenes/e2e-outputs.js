// 출력마다 다른 구성(로드맵 3단계) 점검: 실제 웹 출력(output.js)을 머리 없는 엣지로 찍는다.
//   node tools/scenes/e2e-outputs.js --exe target\debug\wp-app.exe --data tmp\data --out tmp\outputs-e2e --fake-cam
// 시험 기준 (설정만으로): 큐시트 하나에서
//   성경봉독 순서: 예배당 = 본문 전체, 방송 = 카메라 전체 + 아래 성경 자막   ← 순서에 직접 (item.scenes, 큐시트 파일로 가져옴)
//   찬양 순서:     예배당 = 가사 전체, 방송 = 가사 전체 + 구석 작은 카메라   ← 찬양 종류 기본 (settings.typeScenes)
// 성경봉독을 송출해 두 출력(+ 추가 출력 '로비 화면')을 찍고, 외부 단추와 같은 길(/api/trigger/cue-next)로 다음 순서로 넘겨
// 같은 페이지(다시 열지 않음)가 각자 바뀌는지 다시 찍는다. 앱은 자료 폴더를 복사해서 띄우고, 이 스크립트가 띄운 PID 만 끈다.
'use strict';
const fs = require('fs');
const path = require('path');
const { spawn } = require('child_process');
const { launch, sleep } = require('../flow-compare/cdp.js');
const arg = (k, d) => { const i = process.argv.indexOf(k); return i > 0 ? process.argv[i + 1] : d; };
const exe = path.resolve(arg('--exe', 'target/debug/wp-app.exe'));
const data0 = path.resolve(arg('--data', 'tmp/data'));
const out = path.resolve(arg('--out', 'tmp/outputs-e2e'));
const ok = (c, m) => { console.log((c ? '통과: ' : '실패: ') + m); if (!c) process.exitCode = 1; };
fs.mkdirSync(out, { recursive: true });

const KEY = 'e2e-outputs';
const LOBBY = 'out-lobby';
const scenes = [
  { id: 'sc-text', name: '본문 전체', layers: [{ id: 'l-bg', source: 'background' }, { id: 'l-live', source: 'live' }] },
  { id: 'sc-camsub', name: '카메라 전체 + 아래 성경 자막', layers: [{ id: 'l-cam', source: 'camera:' }, { id: 'l-live', source: 'live', tpl: 'overlay' }] },
  { id: 'sc-lyrics', name: '가사 전체', layers: [{ id: 'l-bg', source: 'background' }, { id: 'l-live', source: 'live' }] },
  { id: 'sc-lyrics-cam', name: '가사 전체 + 구석 작은 카메라', layers: [
    { id: 'l-bg', source: 'background', tpl: 'main' },
    { id: 'l-live', source: 'live', tpl: 'main' },
    { id: 'l-cam', source: 'camera:', rect: [0.7, 0.68, 0.27, 0.27], radius: 16, border: { width: 4, color: '#ffffff' } }] },
];
const cue = { name: '출력마다 다른 구성 시험 (○○교회)', items: [
  { id: 'i-bible', type: 'bible', title: '성경봉독', ref: '요3:16-17', note: '', duration: 5, scenes: { main: 'sc-text', overlay: 'sc-camsub' } },
  { id: 'i-song', type: 'song', title: '찬양', lyrics: '[1절]\n새 아침 밝아 오네\n주님을 찬양해\n\n[후렴]\n온 땅이 노래하네\n할렐루야 할렐루야', duration: 5 },
] };

function copyData() {
  const d = path.join(out, 'data');
  fs.rmSync(d, { recursive: true, force: true });
  fs.mkdirSync(d, { recursive: true });
  for (const f of ['bible.db', 'bible.db-wal', 'settings.json']) if (fs.existsSync(path.join(data0, f))) fs.copyFileSync(path.join(data0, f), path.join(d, f));
  if (fs.existsSync(path.join(data0, 'media'))) fs.cpSync(path.join(data0, 'media'), path.join(d, 'media'), { recursive: true });
  const sp = path.join(d, 'settings.json');
  const s = JSON.parse(fs.readFileSync(sp, 'utf8'));
  // 점검 격리: 업데이트 · 통계 · 전역 단축키 · 리모컨 · 내부망 · 동기화 끔, 송출해도 예배당 창을 띄우지 않음
  Object.assign(s, { autoUpdate: false, telemetry: false, globalShortcuts: false, syncDir: '', triggerKey: KEY, previewMode: true });
  s.output = { ...(s.output || {}), autoOnSend: false };
  if (s.remote) s.remote.enabled = false;
  if (s.lanOutput) s.lanOutput.enabled = false;
  (s.sources || []).forEach(x => { x.on = false; });
  (s.overlays || []).forEach(x => { x.on = false; });
  // 설정만으로: 구성 넷 · 찬양 종류 기본 · 추가 출력 하나 (예배당처럼, 기본 구성)
  s.scenes = scenes;
  s.outputScenes = {};
  s.typeScenes = { song: { main: 'sc-lyrics', overlay: 'sc-lyrics-cam' } };
  s.extraOutputs = [{ id: LOBBY, name: '로비 화면', kind: 'main', template: '', monitor: -1, window: false }];
  fs.writeFileSync(sp, JSON.stringify(s, null, 2));
  const cf = path.join(out, 'cue.json');
  fs.writeFileSync(cf, JSON.stringify({ format: 'worshipcue-cues', version: 1, cues: [cue] }, null, 2));
  return { d, cf };
}

async function runApp(data, cueFile) {
  const legacy = path.join(out, 'empty-legacy');
  fs.mkdirSync(legacy, { recursive: true });
  const p = spawn(exe, ['--data', data, '--win', '1600x960', '--search', '요3:16', '--import-cue', cueFile, '--item', '0', '--send'], { env: { ...process.env, WP_LEGACY_DATA: legacy }, stdio: ['ignore', 'pipe', 'pipe'] });
  let log = '';
  p.stdout.on('data', b => { log += b; });
  p.stderr.on('data', b => { log += b; });
  for (let i = 0; i < 160; i++) {
    const m = log.match(/로컬 서버: http:\/\/127\.0\.0\.1:(\d+)/);
    if (m) { await sleep(4000); return { proc: p, port: m[1], log: () => log }; }
    await sleep(250);
  }
  p.kill();
  throw new Error('앱 서버 주소를 찾지 못함:\n' + log.slice(-2000));
}

const INFO = `(() => { const v = document.querySelector('.wp-cam video'); const t = document.querySelector('.bc-area .bc-text');
  return JSON.stringify({ layers: [...document.querySelectorAll('.wp-layer')].map(e => e.dataset.source), boxed: [...document.querySelectorAll('.wp-layer')].filter(e => e.dataset.hostKey).map(e => e.dataset.source),
    cam: v ? [v.videoWidth, v.videoHeight, v.readyState] : null, text: t ? t.innerText.replace(/\\s+/g, ' ').slice(0, 60) : '', fontPx: t ? t.style.fontSize : '', loads: window.__loads || 0 }); })()`;

async function snap(page, file, wantCam) {
  if (wantCam) await page.waitFor(`[...document.querySelectorAll('.wp-cam video')].some(v => v.readyState >= 2 && v.videoWidth > 0)`, 15000);
  await page.settle(8000);
  await sleep(700);
  const info = JSON.parse(await page.eval(INFO));
  await page.shot(file);
  return info;
}

(async () => {
  const camArgs = process.argv.includes('--fake-cam') ? ['--use-fake-ui-for-media-stream', '--use-fake-device-for-media-stream'] : ['--use-fake-ui-for-media-stream'];
  const browser = await launch(path.join(out, 'profile'), undefined, camArgs);
  const report = {};
  const { d, cf } = copyData();
  const app = await runApp(d, cf);
  try {
    const base = `http://127.0.0.1:${app.port}`;
    const pages = {};
    for (const w of ['main', 'overlay', LOBBY]) {
      pages[w] = await browser.open(`${base}/outputs/${w}`, 1920, 1080);
      await pages[w].eval(`window.__loads = (window.__loads || 0) + 1, true`);
      await pages[w].waitFor(`!!document.querySelector('.bc-stage .bc-area')`, 20000);
    }
    // ① 성경봉독 (순서에 직접 지정)
    const s1 = {};
    s1.main = await snap(pages.main, path.join(out, '1-bible-main.png'));
    s1.overlay = await snap(pages.overlay, path.join(out, '1-bible-obs.png'), true);
    s1.lobby = await snap(pages[LOBBY], path.join(out, '1-bible-lobby.png'));
    report.bible = s1;
    ok(s1.main.layers.slice(0, 2).join() === 'background,live' && !s1.main.layers.some(x => x.startsWith('camera:')), '성경봉독 · 예배당 = 본문 전체: ' + s1.main.layers.join(' · '));
    ok(s1.overlay.layers.slice(0, 2).join() === 'camera:,live' && s1.overlay.cam && s1.overlay.cam[0] > 0, '성경봉독 · 방송 = 카메라 전체 + 아래 자막: ' + s1.overlay.layers.join(' · ') + ' 카메라 ' + JSON.stringify(s1.overlay.cam));
    ok(/하나님이|세상을/.test(s1.main.text) && /하나님이|세상을/.test(s1.overlay.text), '두 출력 모두 성경 본문: ' + s1.main.text.slice(0, 24) + ' / ' + s1.overlay.text.slice(0, 24));
    ok(s1.lobby.layers[0] === 'background' && s1.lobby.layers.includes('media'), '추가 출력(로비 화면) = 예배당처럼 기본 구성: ' + s1.lobby.layers.join(' · '));
    // ② 다음 순서로 (외부 단추와 같은 길)
    const r = await (await fetch(`${base}/api/trigger/cue-next?key=${KEY}`)).json();
    ok(r.ok === true, '다음 순서로 넘김 (/api/trigger/cue-next): ' + JSON.stringify(r));
    await pages.main.waitFor(`/새 아침|찬양/.test((document.querySelector('.bc-area .bc-text') || {}).innerText || '')`, 15000);
    const s2 = {};
    s2.main = await snap(pages.main, path.join(out, '2-song-main.png'));
    s2.overlay = await snap(pages.overlay, path.join(out, '2-song-obs.png'), true);
    s2.lobby = await snap(pages[LOBBY], path.join(out, '2-song-lobby.png'));
    report.song = s2;
    ok(s2.main.layers.slice(0, 2).join() === 'background,live' && !s2.main.layers.some(x => x.startsWith('camera:')), '찬양 · 예배당 = 가사 전체: ' + s2.main.layers.join(' · '));
    ok(s2.overlay.layers.slice(0, 3).join() === 'background,live,camera:' && s2.overlay.boxed.includes('camera:') && s2.overlay.cam && s2.overlay.cam[0] > 0, '찬양 · 방송 = 가사 전체 + 구석 작은 카메라: ' + s2.overlay.layers.join(' · ') + ' (상자: ' + s2.overlay.boxed.join(' · ') + ')');
    ok(/새 아침|노래/.test(s2.main.text) && /새 아침|노래/.test(s2.overlay.text), '두 출력 모두 가사: ' + s2.main.text.slice(0, 20) + ' / ' + s2.overlay.text.slice(0, 20));
    ok(['main', 'overlay', LOBBY].every(w => (s2[w === LOBBY ? 'lobby' : w].loads) === 1), '페이지를 다시 열지 않고 각자 바뀜 (SSE)');
    // 소리 내는 출력: 예배당 하나 (payload.audio)
    const st = await (await fetch(`${base}/api/state`)).json();
    report.audio = st.audio;
  } finally {
    for (const k of Object.keys(browser.pages || {})) try { browser.pages[k].close(); } catch {}
    app.proc.kill();
    browser.proc.kill();
    fs.writeFileSync(path.join(out, 'report.json'), JSON.stringify(report, null, 2));
  }
})().catch(e => { console.error(e); process.exitCode = 1; });
