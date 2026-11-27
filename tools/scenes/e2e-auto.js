// 전환 · 자동화(로드맵 4단계) 점검: 실제 웹 출력(output.js)을 머리 없는 엣지로 본다.
//   node tools/scenes/e2e-auto.js --exe target\debug\wp-app.exe --data tmp\data --out tmp\auto-e2e --fake-cam
// 시험 기준:
//   ① 성경봉독(방송 = 카메라 전체 + 아래 자막) → 찬양(방송 = 가사 + 구석 작은 카메라) 으로 넘길 때 방송 출력의 카메라 칸이
//      같은 video 요소 그대로(다시 열지 않음 · 멈추지 않음) 전체 → 구석으로 옮겨 간다 (transform 애니메이션, 기본 0.4초).
//   ② /api/trigger/scene?output=overlay&id=… (지금 한 번) 로 구성이 바뀌고, 다음 순서로 넘기면 되돌아온다.
//   ③ hold=1 이면 출력 기본이 바뀌어 다음 순서에서도 남는다. 순서 동작 '구성 바꾸기'(actions.scene) 도 남는다.
//   ④ 리모컨 상태(/r/state 와 같은 값)에 구성 단추가 보인다.
// 앱은 자료 폴더를 복사해서 띄우고, 이 스크립트가 띄운 PID 만 끈다.
'use strict';
const fs = require('fs');
const path = require('path');
const { spawn } = require('child_process');
const { launch, sleep } = require('../flow-compare/cdp.js');
const arg = (k, d) => { const i = process.argv.indexOf(k); return i > 0 ? process.argv[i + 1] : d; };
const exe = path.resolve(arg('--exe', 'target/debug/wp-app.exe'));
const data0 = path.resolve(arg('--data', 'tmp/data'));
const out = path.resolve(arg('--out', 'tmp/auto-e2e'));
let fails = 0;
const ok = (c, m) => { console.log((c ? '통과: ' : '실패: ') + m); if (!c) { process.exitCode = 1; fails++; } };
fs.mkdirSync(out, { recursive: true });

const KEY = 'e2e-auto';
const scenes = [
  { id: 'sc-text', name: '본문 전체', layers: [{ id: 'l-bg', source: 'background' }, { id: 'l-live', source: 'live' }] },
  { id: 'sc-camsub', name: '카메라 전체 + 아래 성경 자막', layers: [{ id: 'l-cam', source: 'camera:' }, { id: 'l-live', source: 'live', tpl: 'overlay' }] },
  { id: 'sc-lyrics', name: '가사 전체', layers: [{ id: 'l-bg', source: 'background' }, { id: 'l-live', source: 'live' }] },
  { id: 'sc-lyrics-cam', name: '가사 전체 + 구석 작은 카메라', layers: [
    { id: 'l-bg', source: 'background', tpl: 'main' },
    { id: 'l-live', source: 'live', tpl: 'main' },
    { id: 'l-cam2', source: 'camera:', rect: [0.7, 0.68, 0.27, 0.27], radius: 16, border: { width: 4, color: '#ffffff' } }] },
];
const cue = { name: '전환 · 자동화 시험 (○○교회)', items: [
  { id: 'i-bible', type: 'bible', title: '성경봉독', ref: '요3:16-17', note: '', duration: 5, scenes: { main: 'sc-text', overlay: 'sc-camsub' } },
  { id: 'i-song', type: 'song', title: '찬양', lyrics: '[1절]\n새 아침 밝아 오네\n주님을 찬양해\n\n[후렴]\n온 땅이 노래하네\n할렐루야 할렐루야', duration: 5 },
  { id: 'i-n1', type: 'notice', title: '광고 하나', text: '다음 주 홍길동 집사 생일 축하', duration: 3, actions: { scene: { main: 'sc-lyrics' } } },
  { id: 'i-n2', type: 'notice', title: '광고 둘', text: '○○교회 성가대 연습은 오후 2시', duration: 3 },
  { id: 'i-n3', type: 'notice', title: '광고 셋', text: '주차 안내', duration: 3 },
] };

function copyData() {
  const d = path.join(out, 'data');
  fs.rmSync(d, { recursive: true, force: true });
  fs.mkdirSync(d, { recursive: true });
  for (const f of ['bible.db', 'bible.db-wal', 'settings.json']) if (fs.existsSync(path.join(data0, f))) fs.copyFileSync(path.join(data0, f), path.join(d, f));
  if (fs.existsSync(path.join(data0, 'media'))) fs.cpSync(path.join(data0, 'media'), path.join(d, 'media'), { recursive: true });
  const sp = path.join(d, 'settings.json');
  const s = JSON.parse(fs.readFileSync(sp, 'utf8'));
  Object.assign(s, { autoUpdate: false, telemetry: false, globalShortcuts: false, syncDir: '', triggerKey: KEY, previewMode: true });
  s.output = { ...(s.output || {}), autoOnSend: false };
  if (s.remote) s.remote.enabled = false;
  if (s.lanOutput) s.lanOutput.enabled = false;
  (s.sources || []).forEach(x => { x.on = false; });
  (s.overlays || []).forEach(x => { x.on = false; });
  s.scenes = scenes;
  s.outputScenes = {};
  s.typeScenes = { song: { main: 'sc-lyrics', overlay: 'sc-lyrics-cam' } };
  delete s.sceneTransition; // 기본 (옮겨 가기 0.4초)
  delete s.outputTransitions;
  s.extraOutputs = [];
  fs.writeFileSync(sp, JSON.stringify(s, null, 2));
  const cf = path.join(out, 'cue.json');
  fs.writeFileSync(cf, JSON.stringify({ format: 'worshipcue-cues', version: 1, cues: [cue] }, null, 2));
  return { d, cf, sp };
}

async function runApp(data, cueFile) {
  const legacy = path.join(out, 'empty-legacy');
  fs.mkdirSync(legacy, { recursive: true });
  const p = spawn(exe, ['--data', data, '--win', '1600x960', '--search', '요3:16', '--import-cue', cueFile, '--item', '0', '--send'], { env: { ...process.env, WP_LEGACY_DATA: legacy }, stdio: ['ignore', 'pipe', 'pipe'] });
  let log = '';
  p.stdout.on('data', b => { log += b; });
  p.stderr.on('data', b => { log += b; });
  // 출시 빌드는 창 프로그램이라 표준 출력이 없다 → 자료 폴더의 로그에서도 찾는다
  const logFile = () => { try { const d = path.join(data, 'logs'); return fs.readdirSync(d).map(f => fs.readFileSync(path.join(d, f), 'utf8')).join('\n'); } catch { return ''; } };
  for (let i = 0; i < 160; i++) {
    const m = (log + logFile()).match(/로컬 서버: http:\/\/127\.0\.0\.1:(\d+)/);
    if (m) { await sleep(4000); return { proc: p, port: m[1], log: () => log }; }
    await sleep(250);
  }
  p.kill();
  throw new Error('앱 서버 주소를 찾지 못함:\n' + log.slice(-2000));
}

const INFO = `(() => { const v = document.querySelector('.wp-layer:not([data-ghost]) .wp-cam video'); const t = document.querySelector('.bc-area .bc-text');
  const box = v && v.closest('.wp-layer'); const r = box && box.getBoundingClientRect();
  return JSON.stringify({ scene: document.body.dataset.scene || '', layers: [...document.querySelectorAll('.wp-layer:not([data-ghost])')].filter(e => !e._leaving).map(e => e.dataset.source),
    same: !!(v && window.__cam === v), mark: v ? v.dataset.e2e || '' : '', paused: v ? v.paused : null, cam: v ? [v.videoWidth, v.videoHeight, v.readyState] : null,
    box: r ? [Math.round(r.left), Math.round(r.top), Math.round(r.width), Math.round(r.height)] : null,
    text: t ? t.innerText.replace(/\\s+/g, ' ').slice(0, 60) : '', loads: window.__loads || 0 }); })()`;
// 카메라 칸의 움직임을 프레임마다 적어 둔다 (옮겨 가는 동안 transform 애니메이션 · 상자 크기)
const REC = `(() => { window.__rec = []; const t0 = performance.now(); const tick = () => {
  const v = document.querySelector('.wp-layer:not([data-ghost]) .wp-cam video'); const box = v && v.closest('.wp-layer');
  if (box) { const r = box.getBoundingClientRect(); const a = box.getAnimations ? box.getAnimations().filter(x => x.playState === 'running') : [];
    window.__rec.push({ t: Math.round(performance.now() - t0), w: Math.round(r.width), anim: a.length, same: window.__cam === v, paused: v.paused, ct: v.currentTime }); }
  if (performance.now() - t0 < 2500) requestAnimationFrame(tick); }; requestAnimationFrame(tick); return true; })()`;

async function snap(page, file, wantCam) {
  if (wantCam) await page.waitFor(`[...document.querySelectorAll('.wp-cam video')].some(v => v.readyState >= 2 && v.videoWidth > 0)`, 15000);
  await page.settle(8000);
  await sleep(700);
  const info = JSON.parse(await page.eval(INFO));
  if (file) await page.shot(file);
  return info;
}
const trig = async (base, q) => (await (await fetch(`${base}/api/trigger/${q}${q.includes('?') ? '&' : '?'}key=${KEY}`)).json());

(async () => {
  const camArgs = process.argv.includes('--fake-cam') ? ['--use-fake-ui-for-media-stream', '--use-fake-device-for-media-stream'] : ['--use-fake-ui-for-media-stream'];
  const browser = await launch(path.join(out, 'profile'), undefined, camArgs);
  const report = {};
  const { d, cf, sp } = copyData();
  const app = await runApp(d, cf);
  try {
    const base = `http://127.0.0.1:${app.port}`;
    const obs = await browser.open(`${base}/outputs/overlay`, 1920, 1080);
    const main = await browser.open(`${base}/outputs/main`, 1920, 1080);
    for (const pg of [obs, main]) {
      await pg.eval(`window.__loads = (window.__loads || 0) + 1, true`);
      await pg.waitFor(`!!document.querySelector('.bc-stage .bc-area')`, 20000);
    }
    // ① 성경봉독: 카메라 전체 — 이 video 요소에 표시를 해 둔다
    const a = await snap(obs, path.join(out, '1-bible-obs.png'), true);
    await obs.eval(`(() => { const v = document.querySelector('.wp-cam video'); window.__cam = v; v.dataset.e2e = 'first'; return true; })()`);
    report.bible = a;
    ok(a.scene === 'sc-camsub' && a.layers.slice(0, 2).join() === 'camera:,live' && a.cam && a.cam[0] > 0, '성경봉독 · 방송 = 카메라 전체 + 자막: ' + a.layers.join(' · ') + ' ' + JSON.stringify(a.box));
    await obs.eval(REC);
    const r1 = await trig(base, 'cue-next');
    ok(r1.ok === true, '찬양으로 넘김 (/api/trigger/cue-next)');
    await sleep(150);
    await obs.shot(path.join(out, '2-moving-obs.png')); // 옮겨 가는 중 (찍는 시간에 따라 끝났을 수도)
    await obs.waitFor(`document.body.dataset.scene === 'sc-lyrics-cam'`, 15000);
    await sleep(2600);
    const rec = JSON.parse(await obs.eval('JSON.stringify(window.__rec || [])'));
    const b = await snap(obs, path.join(out, '3-song-obs.png'), true);
    report.song = b;
    report.glide = rec.filter((x, i) => i % 3 === 0);
    const animated = rec.filter(x => x.anim > 0);
    const ws = rec.map(x => x.w);
    ok(b.scene === 'sc-lyrics-cam' && b.layers.slice(0, 3).join() === 'background,live,camera:', '찬양 · 방송 = 가사 + 구석 카메라: ' + b.layers.join(' · ') + ' ' + JSON.stringify(b.box));
    ok(b.same && b.mark === 'first', '카메라 칸이 같은 video 요소 그대로 (다시 열지 않음)');
    ok(rec.length > 5 && rec.every(x => x.same && !x.paused), '옮겨 가는 동안 영상이 멈추지 않음 (' + rec.length + '프레임, 모두 같은 요소 · 재생 중)');
    ok(animated.length >= 3 && animated[animated.length - 1].t - animated[0].t >= 250 && animated[animated.length - 1].t - animated[0].t <= 700, '옮겨 가기 애니메이션 ' + (animated.length ? (animated[animated.length - 1].t - animated[0].t) + 'ms' : '없음') + ' (기본 0.4초)');
    ok(b.box && b.box[2] < 1920 * 0.3 && a.box && a.box[2] >= 1900, '전체(' + (a.box && a.box[2]) + 'px) → 구석(' + (b.box && b.box[2]) + 'px)');
    ok(rec.length && rec[rec.length - 1].ct > rec[0].ct, '영상 시간이 흐름 (' + (rec[0] && rec[0].ct.toFixed(2)) + ' → ' + (rec[rec.length - 1] && rec[rec.length - 1].ct.toFixed(2)) + '초)');
    // ② 지금 한 번: 방송을 카메라 전체로 → 다음 순서에서 되돌아옴
    const r2 = await trig(base, 'scene?output=overlay&id=sc-camsub');
    ok(r2.ok === true, '/api/trigger/scene?output=overlay&id=sc-camsub: ' + JSON.stringify(r2));
    await obs.waitFor(`document.body.dataset.scene === 'sc-camsub'`, 10000);
    const c = await snap(obs, path.join(out, '4-once-obs.png'), true);
    report.once = c;
    ok(c.layers.slice(0, 2).join() === 'camera:,live' && c.same, '지금 한 번: 카메라 전체로 (같은 video 요소): ' + c.layers.join(' · '));
    const r3 = await trig(base, 'scene?output=overlay&id=' + encodeURIComponent('가사 전체 + 구석 작은 카메라'));
    await obs.waitFor(`document.body.dataset.scene === 'sc-lyrics-cam'`, 10000);
    ok(r3.ok === true, '구성 이름으로도 고름 (가사 전체 + 구석 작은 카메라)');
    await trig(base, 'scene?output=overlay&id=sc-camsub');
    await obs.waitFor(`document.body.dataset.scene === 'sc-camsub'`, 10000);
    await trig(base, 'cue-next'); // 광고 하나 (지정 없음 → 출력 기본 = 전체)
    await obs.waitFor(`document.body.dataset.scene === 'full'`, 10000);
    const e = await snap(obs, path.join(out, '5-next-obs.png'));
    report.back = e;
    ok(e.scene === 'full' && !e.layers.some(x => x.startsWith('camera:')), '다음 순서로 넘기면 되돌아옴 (방송 = 기본 전체): ' + e.layers.join(' · '));
    // 순서 동작 '구성 바꾸기' (광고 하나: 예배당 기본 → 가사 전체) — 남는 동작
    await main.waitFor(`document.body.dataset.scene === 'sc-lyrics'`, 10000);
    ok(true, "순서 동작 '구성 바꾸기': 광고 하나에서 예배당 기본 = 가사 전체");
    // ③ hold=1: 출력 기본을 바꿈 → 다음 순서에서도 남음
    const r4 = await trig(base, 'scene?output=overlay&id=sc-camsub&hold=1');
    ok(r4.ok === true, 'hold=1: ' + JSON.stringify(r4));
    await obs.waitFor(`document.body.dataset.scene === 'sc-camsub'`, 10000);
    await trig(base, 'cue-next'); // 광고 둘
    await main.waitFor(`/성가대/.test((document.querySelector('.bc-area .bc-text') || {}).innerText || '')`, 10000);
    await sleep(1200);
    const f = await snap(obs, path.join(out, '6-hold-obs.png'), true);
    const fm = await snap(main, path.join(out, '6-hold-main.png'));
    report.hold = { obs: f, main: fm };
    ok(f.scene === 'sc-camsub', 'hold=1 은 다음 순서에서도 남음 (방송 = ' + f.scene + ')');
    ok(fm.scene === 'sc-lyrics', "순서 동작 '구성 바꾸기' 도 다음 순서에 남음 (예배당 = " + fm.scene + ')');
    const saved = JSON.parse(fs.readFileSync(sp, 'utf8'));
    report.outputScenes = saved.outputScenes;
    ok(saved.outputScenes && saved.outputScenes.overlay === 'sc-camsub' && saved.outputScenes.main === 'sc-lyrics', '설정 outputScenes 에 저장됨: ' + JSON.stringify(saved.outputScenes));
    ok([obs, main].length === 2 && f.loads === 1 && fm.loads === 1, '페이지를 다시 열지 않음 (SSE)');
    // ④ 리모컨 상태 (도킹 /dock/state 는 이 PC 에서 PIN 없이 — 휴대폰 /r/state 와 같은 state_for)
    let rs = null;
    try { rs = await (await fetch(`${base}/r/state?pin=dock`)).json(); } catch { /* 없음 */ }
    const sc = rs && (rs.scenes || (rs.state && rs.state.scenes));
    report.remote = sc;
    ok(!!(sc && sc.outputs && sc.outputs.length >= 2 && sc.list.length === 4), '리모컨 상태에 구성 단추: ' + (sc ? sc.outputs.map(o => o.name + '=' + o.now).join(' · ') : JSON.stringify(rs).slice(0, 200)));
    // 리모컨 페이지 모양 (도킹 화면)
    const rp = await browser.open(`${base}/dock/`, 420, 1100);
    await rp.waitFor(`!document.getElementById('scenesCard').hidden`, 10000).catch(() => {});
    await sleep(800);
    await rp.shot(path.join(out, '7-remote-dock.png'));
    ok(await rp.eval(`document.querySelectorAll('#scenes [data-sc]').length >= 10`), '리모컨 화면에 구성 단추 (출력마다)');
  } finally {
    for (const k of Object.keys(browser.pages || {})) try { browser.pages[k].close(); } catch {}
    app.proc.kill();
    browser.proc.kill();
    fs.writeFileSync(path.join(out, 'report.json'), JSON.stringify(report, null, 2));
    console.log(fails ? `실패 ${fails}개` : '모두 통과');
  }
})().catch(e => { console.error(e); process.exitCode = 1; });
