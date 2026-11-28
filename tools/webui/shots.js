// 웹 화면(리모컨 · 휴대폰 카메라 · 웹 편집기) 캡처: 가짜 서버(mock.js) + 머리 없는 엣지, 휴대폰 크기.
//   node tools/webui/shots.js --out tmp/craft/after [--web web] [--only remote-,cam-] [--sizes all]
// 같은 자료 · 같은 상태 · 같은 크기로 예전 화면도 찍을 수 있다 (--web <예전 web 폴더> --out tmp/craft/before) → compare.py 로 쌍.
// 앱 · 실제 자료 · PC 이름과 무관 (가상 자료만). 이 스크립트가 띄운 서버 · 브라우저만 끈다.
'use strict';
const fs = require('fs');
const os = require('os');
const path = require('path');
const mock = require('./mock.js');
const { open, sleep } = require('./browser.js');
const arg = (k, d) => { const i = process.argv.indexOf(k); return i > 0 ? process.argv[i + 1] : d; };
const out = path.resolve(arg('--out', 'tmp/craft/after'));
const only = (arg('--only', '') || '').split(',').filter(Boolean);
fs.mkdirSync(out, { recursive: true });

// 휴대폰 크기 (CSS px) — 작은 안드로이드 · 아이폰 · 큰 아이폰 · 눕힘 · 태블릿
const PHONES = { s360: { w: 360, h: 740 }, s390: { w: 390, h: 844 }, s430: { w: 430, h: 932 }, land: { w: 844, h: 390 }, tab: { w: 820, h: 1180 } };
const DESK = { w: 1280, h: 800, dpr: 1, mobile: false, touch: false };
const NOTCH = '&safe=47,0,34,0'; // 시험용: 안전 영역(노치 · 홈 막대) 흉내 — wpweb.js 가 ?safe= 를 읽는다

const S = [];
const add = (id, kind, dev, o = {}) => S.push({ id, kind, dev, ...o });
const login = pin => `(() => { const p = document.getElementById('pin'); if (!p || (document.getElementById('login') || {}).hidden) return 0; p.value = '${pin}'; p.dispatchEvent(new Event('input', { bubbles: true })); const g = document.getElementById('go'); if (g) g.click(); return 1; })()`;
const click = sel => `(() => { const e = document.querySelector(${JSON.stringify(sel)}); if (e) e.click(); return !!e; })()`;

// ---------- 리모컨 ----------
for (const [k, d] of Object.entries(PHONES)) {
  add(`remote-main-${k}`, 'remote', d, { prep: m => m.send(1), steps: [login('123456')] });
}
add('remote-pin', 'remote', PHONES.s390, {});
add('remote-pin-wrong', 'remote', PHONES.s390, { steps: [login('000000')], wait: 900 });
add('remote-pin-locked', 'remote', PHONES.s390, { prep: m => m.set({ net: { lock: 99 } }), steps: [login('123456')], wait: 900 });
add('remote-nolive', 'remote', PHONES.s390, { steps: [login('123456')] });
add('remote-hidden', 'remote', PHONES.s390, { prep: m => { m.send(1); m.act({ action: 'toggle' }); }, steps: [login('123456')] });
add('remote-bible', 'remote', PHONES.s390, { prep: m => m.send(2), steps: [login('123456')] });
add('remote-score', 'remote', PHONES.s390, { prep: m => m.send(5), steps: [login('123456')] });
add('remote-last-slide', 'remote', PHONES.s390, { prep: m => { m.send(1); m.act({ action: 'goto', index: 3 }); }, steps: [login('123456')] });
add('remote-down', 'remote', PHONES.s390, { prep: m => m.send(1), steps: [login('123456')], after: async (p, m) => { m.set({ net: { down: true } }); await sleep(4200); } });
add('remote-slow', 'remote', PHONES.s390, { prep: m => { m.send(1); m.set({ net: { delay: 1500 } }); }, steps: [login('123456')], wait: 2600 });
add('remote-select', 'remote', PHONES.s390, { prep: m => m.send(1), steps: [login('123456'), click('#cue [data-cue="3"]')] });
add('remote-off-armed', 'remote', PHONES.s390, { prep: m => m.send(1), steps: [login('123456'), click('#xOff')], wait: 300 });
add('remote-stage', 'remote', PHONES.s390, { prep: m => { m.send(3); m.act({ action: 'message', text: '5분 남았습니다' }); m.act({ action: 'timer', op: 'start', minutes: 23 }); }, steps: [login('123456'), click('[data-tab="stage"]')] });
add('remote-layouts', 'remote', PHONES.s390, { prep: m => { m.send(1); m.act({ action: 'scene', output: 'overlay', id: 'sc-lyrics-cam' }); }, steps: [login('123456'), click('[data-tab="layout"]')] });
add('remote-names', 'remote', PHONES.s390, { prep: m => { m.send(3); m.act({ action: 'name', index: 0 }); }, steps: [login('123456'), click('[data-tab="names"]')] });
add('remote-menu', 'remote', PHONES.s390, { prep: m => m.send(1), steps: [login('123456'), click('#menuBtn')] });
add('remote-notch', 'remote', PHONES.s390, { prep: m => m.send(1), steps: [login('123456')], query: NOTCH });
add('remote-en', 'remote', PHONES.s390, { prep: m => { m.set({ lang: 'en' }); m.send(1); }, steps: [login('123456')] });
add('remote-en-stage', 'remote', PHONES.s390, { prep: m => { m.set({ lang: 'en' }); m.send(3); }, steps: [login('123456'), click('[data-tab="stage"]')] });
add('remote-long', 'remote', PHONES.s360, { prep: m => { const it = m.state.cue.items; for (let i = 0; i < 14; i++) it.push({ id: 'x' + i, type: ['song', 'text', 'bible'][i % 3], title: ['감사 찬양 메들리 — 주 은혜임을 · 은혜 아니면', '교회 소식과 다음 주 행사 안내', '시편 23편 교독'][i % 3], note: i % 4 ? '' : '인도자 신호 뒤' }); m.send(1); }, steps: [login('123456')] });
add('remote-full', 'remote', PHONES.s390, { prep: m => m.send(1), steps: [login('123456')], full: true });
add('dock-main', 'dock', { w: 340, h: 520, dpr: 2, mobile: false, touch: false }, { prep: m => m.send(1) });
add('dock-score', 'dock', { w: 340, h: 520, dpr: 2, mobile: false, touch: false }, { prep: m => m.send(5) });
add('dock-wide', 'dock', { w: 620, h: 300, dpr: 2, mobile: false, touch: false }, { prep: m => m.send(1) });

// ---------- 휴대폰 카메라 ----------
for (const k of ['s360', 's390', 's430', 'land', 'tab']) add(`cam-ready-${k}`, 'cam', PHONES[k], { wait: 2500 });
add('cam-onair', 'cam', PHONES.land, { prep: m => m.set({ cam: { onAir: ['main', 'overlay'] } }), wait: 2500 });
add('cam-onair-portrait', 'cam', PHONES.s390, { prep: m => m.set({ cam: { onAir: ['main'] } }), wait: 2500 });
add('cam-down', 'cam', PHONES.land, { wait: 2000, after: async (p, m) => { m.set({ net: { down: true } }); await sleep(6000); } });
add('cam-nokey', 'cam', PHONES.s390, { url: m => `http://127.0.0.1:${m.ports.cam}/cam?k=${m.state.camKey}`, prep: m => {}, after: async (p, m) => { m.set({ camKey: 'changed0000000000' }); await sleep(2500); } });
add('cam-denied', 'cam', PHONES.s390, { deny: true, wait: 2500 });
add('cam-menu', 'cam', PHONES.s390, { wait: 2500, steps: [click('#menuBtn')] });
add('cam-notch', 'cam', PHONES.land, { wait: 2500, query: '&safe=0,47,21,47' });
add('cam-en', 'cam', PHONES.land, { prep: m => m.set({ lang: 'en' }), wait: 2500 });

// ---------- 웹 편집기 ----------
const openCue = click('#list [data-id="cue-demo"]');
add('edit-pin', 'edit', DESK, {});
add('edit-list', 'edit', DESK, { steps: [login('4321')] });
add('edit-open-1280', 'edit', DESK, { steps: [login('4321'), openCue] });
add('edit-open-tab', 'edit', { ...PHONES.tab, dpr: 1.5 }, { steps: [login('4321'), openCue] });
add('edit-open-phone', 'edit', PHONES.s390, { steps: [login('4321'), openCue] });
add('edit-open-1600', 'edit', { ...DESK, w: 1600, h: 960 }, { steps: [login('4321'), openCue] });
add('edit-item-song', 'edit', DESK, { steps: [login('4321'), openCue, click('.item[data-k="1"], [data-row="1"]')] });
add('edit-item-phone', 'edit', PHONES.s390, { steps: [login('4321'), openCue, click('[data-row="1"]')] });
add('edit-dirty', 'edit', DESK, { steps: [login('4321'), openCue, `(() => { const i = document.querySelector('.item input[data-f=title]'); if (!i) return 0; i.value += ' (고침)'; i.dispatchEvent(new Event('input', { bubbles: true })); return 1; })()`] });
add('edit-saved', 'edit', DESK, { steps: [login('4321'), openCue, `(() => { const i = document.querySelector('.item input[data-f=title]'); if (!i) return 0; i.value += ' (고침)'; i.dispatchEvent(new Event('input', { bubbles: true })); return 1; })()`, click('#save')], wait: 700 });
add('edit-conflict', 'edit', DESK, { steps: [login('4321'), openCue, `(() => { const i = document.querySelector('.item input[data-f=title]'); if (!i) return 0; i.value += ' (고침)'; i.dispatchEvent(new Event('input', { bubbles: true })); return 1; })()`],
  after: async (p, m) => { m.state.updated['cue-demo'] = '2026-11-27 21:00:00.000'; m.state.cues[0].items[3].title = '설교 (조작 화면에서 고침)'; await p.eval(click('#save')); await sleep(900); } });
add('edit-fail', 'edit', DESK, { steps: [login('4321'), openCue, `(() => { const i = document.querySelector('.item input[data-f=title]'); if (!i) return 0; i.value += ' (고침)'; i.dispatchEvent(new Event('input', { bubbles: true })); return 1; })()`],
  after: async (p, m) => { m.set({ net: { down: true } }); await p.eval(click('#save')); await sleep(1500); } });
add('edit-songs', 'edit', DESK, { steps: [login('4321'), openCue, `(() => { const i = document.getElementById('songq'); if (!i) return 0; i.value = '주'; i.dispatchEvent(new Event('input', { bubbles: true })); return 1; })()`], wait: 900 });
add('edit-en', 'edit', DESK, { prep: m => m.set({ lang: 'en' }), steps: [login('4321'), openCue] });
add('edit-nopin-local', 'edit', DESK, { prep: m => m.set({ editPin: '' }) });

(async () => {
  const list = only.length ? S.filter(s => only.some(o => s.id.startsWith(o))) : S;
  const m = await mock.start({ web: arg('--web') });
  const prof = path.join(os.tmpdir(), 'wp-webui-shots-' + process.pid);
  const camArgs = ['--use-fake-device-for-media-stream', '--use-fake-ui-for-media-stream'];
  const b = await open(prof, camArgs);
  const bDeny = await open(prof + '-deny', ['--use-fake-device-for-media-stream', '--deny-permission-prompts']);
  let n = 0;
  try {
    for (const s of list) {
      m.reset();
      if (s.prep) s.prep(m);
      const base = s.url ? s.url(m) : { remote: m.remoteUrl, dock: m.dockUrl, edit: m.editUrl, cam: m.camUrl }[s.kind];
      const url = base + (s.query ? (base.includes('?') ? s.query : '?' + s.query.slice(1)) : '');
      const p = await (s.deny ? bDeny : b).page('about:blank', s.dev);
      // 이전 경우의 저장값이 남지 않게 (같은 출처)
      await p.goto(url);
      await p.eval(`(() => { try { localStorage.clear(); sessionStorage.clear(); } catch {} return 1; })()`).catch(() => {});
      await p.goto(url);
      await sleep(500);
      for (const st of s.steps || []) { try { await p.eval(st); } catch (e) { /* 예전 화면에는 없는 단추 */ } await sleep(450); }
      await sleep(s.wait || 700);
      if (s.after) { try { await s.after(p, m); } catch (e) { console.log('  (after) ' + String(e.message || e).slice(0, 100)); } }
      await p.eval(`document.fonts && document.fonts.ready.then(() => 1)`).catch(() => {});
      await sleep(150);
      const file = path.join(out, s.id + '.png');
      if (s.full) {
        const hgt = await p.eval(`Math.max(document.documentElement.scrollHeight, document.body.scrollHeight)`);
        const r = await p.send('Page.captureScreenshot', { format: 'png', captureBeyondViewport: true, clip: { x: 0, y: 0, width: s.dev.w, height: Math.min(hgt, 6000), scale: 1 } });
        fs.writeFileSync(file, Buffer.from(r.data, 'base64'));
      } else await p.shot(file);
      console.log('ok  ' + s.id);
      n++;
      await p.close();
    }
  } finally {
    b.close(); bDeny.close(); m.close();
    try { b.proc.kill(); bDeny.proc.kill(); } catch { /* 이미 끝남 */ }
  }
  console.log(`${n}장 → ${out}`);
  process.exit(0);
})().catch(e => { console.error(e); process.exit(1); });
