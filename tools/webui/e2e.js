// 웹 세 화면의 핵심 흐름을 머리 없는 엣지로 끝까지 눌러 본다 (외부 모듈 없음).
//   node tools/webui/e2e.js                 가짜 서버(mock.js)로 — 끊김 · 409 · 열쇠 바뀜 · 송출 중 같은 상태까지
//   node tools/webui/e2e.js --remote http://127.0.0.1:37540/ --rpin 123456 --edit http://127.0.0.1:37545/ --epin 4321 [--cam https://…/cam?k=…]
//                                            진짜 앱으로 (가짜 서버만 되는 경우는 건너뜀)
//   [--only remote,edit,cam] [--shots 폴더]
// 리모컨: PIN 틀림 → 맞음 → 순서 '다음'에 올림 → 송출 → 다음 → 글자 숨김/보이기 → 모두 끄기(짧게=안내, 길게=끄기) → 끊김 띠 → 다시 붙음 → 뒤로 가기 막힘
// 편집기: PIN 틀림 → 맞음 → 열기 → 고침 표시 → Ctrl+S 저장 → 다른 곳이 먼저 저장 → 409 띠(덮어쓰지 않음) → 다시 불러오기 → 순서 더하기 · 지우기 · 되돌리기 · 키보드 옮기기
// 카메라: 가짜 카메라로 허락 → PC 연결(ready) → 송출 중 빨강 → 열쇠 바뀜 안내 · 허락 거부 안내
'use strict';
const os = require('os');
const path = require('path');
const fs = require('fs');
const { open, sleep } = require('./browser.js');
const mock = require('./mock.js');
const arg = (k, d) => { const i = process.argv.indexOf(k); return i > 0 ? process.argv[i + 1] : d; };
const only = (arg('--only', 'remote,edit,cam') || '').split(',');
const shots = arg('--shots');
if (shots) fs.mkdirSync(shots, { recursive: true });

let pass = 0, fail = 0;
const ok = (c, m) => { if (c) { pass++; console.log('통과  ' + m); } else { fail++; console.log('실패  ' + m); } };
const wait = async (p, expr, ms = 8000) => { try { await p.waitFor(expr, ms); return true; } catch { return false; } };
const shot = async (p, name) => { if (shots) await p.shot(path.join(shots, name + '.png')); };

async function remoteFlow(b, url, pin, m) {
  console.log('— 리모컨 ' + url);
  const p = await b.page(url, { w: 390, h: 844 });
  ok(await wait(p, `!document.getElementById('login').hidden`), 'PIN 화면이 먼저 나온다');
  ok(await p.eval(`document.getElementById('main').hidden && getComputedStyle(document.getElementById('transport')).display !== 'none' ? !document.getElementById('transport').offsetParent : true`), '로그인 전에는 아래 조작 막대가 안 보인다 (W3)');
  await p.eval(`(() => { const i = document.getElementById('pin'); i.focus(); i.value = '${pin === '000000' ? '111111' : '000000'}'; i.dispatchEvent(new Event('input', { bubbles: true })); return 1; })()`);
  ok(await wait(p, `document.getElementById('loginErr').textContent.length > 3`), '틀린 PIN → 안내 (' + (await p.eval(`document.getElementById('loginErr').innerText`).catch(() => '')).replace(/\n/g, ' / ').slice(0, 60) + ')');
  await sleep(300);
  await p.eval(`(() => { const i = document.getElementById('pin'); i.value = '${pin}'; i.dispatchEvent(new Event('input', { bubbles: true })); return 1; })()`);
  ok(await wait(p, `!document.getElementById('main').hidden && document.getElementById('connText').textContent.length > 0 && document.getElementById('connDot').className.includes('ok')`), '맞는 PIN 6자리 → 저절로 연결');
  ok(await wait(p, `document.querySelectorAll('#cue .crow').length > 1`), '큐시트 순서가 보인다');
  const n0 = m ? m.state.log.length : 0;
  await p.tapOn('#cue .crow[data-cue="1"]');
  await sleep(300);
  ok(await p.eval(`document.querySelector('#cue .crow[data-cue="1"]').classList.contains('sel') && !!document.querySelector('#cue [data-send="1"]')`), "순서를 누르면 '다음'에 올라가고 [송출] 단추가 나온다 (W2)");
  if (m) ok(m.state.log.length === n0, '한 번 누른 것만으로는 송출하지 않는다');
  await shot(p, 'e2e-remote-armed');
  await p.tapOn('#cue [data-send="1"]');
  ok(await wait(p, `document.getElementById('readout').classList.contains('onair') && document.querySelector('#cue .crow[data-cue="1"]').classList.contains('live')`), '[송출] → 송출 중(빨강) · 그 줄에 송출 중 표시');
  const c0 = await p.eval(`document.getElementById('count').textContent`);
  await p.tapOn('#bNext');
  ok(await wait(p, `document.getElementById('count').textContent !== ${JSON.stringify(c0)}`), `다음 → 장 번호가 바뀜 (${c0} → ${await p.eval(`document.getElementById('count').textContent`)})`);
  // 빠르게 두 번 (0.3초 안) — 둘 다 들어가야 한다
  const c1 = await p.eval(`document.getElementById('count').textContent`);
  await p.tapOn('#bPrev'); await sleep(120); await p.tapOn('#bNext');
  await sleep(1500);
  ok(await p.eval(`document.getElementById('count').textContent`) === c1, `빠르게 이전 · 다음 (0.12초 사이) → 둘 다 들어감 (${c1})`);
  await p.tapOn('#xHide');
  ok(await wait(p, `document.getElementById('readout').classList.contains('hidden-text') && document.getElementById('xHide').classList.contains('warn')`), '글자 숨김 → 표시창 숨김 중 · 단추 주황');
  await shot(p, 'e2e-remote-hidden');
  await p.tapOn('#xHide');
  ok(await wait(p, `document.getElementById('readout').classList.contains('onair')`), '다시 누르면 글자 보이기');
  if (m) {
    const before = m.state.log.filter(x => x === 'off').length;
    await p.tapOn('#xOff', 60);
    await sleep(500);
    ok(m.state.log.filter(x => x === 'off').length === before && await p.eval(`!!document.querySelector('.toast')`), '모두 끄기를 짧게 누르면 끄지 않고 안내만 (W1)');
    await p.tapOn('#xOff', 1200);
    await sleep(600);
    ok(m.state.log.filter(x => x === 'off').length === before + 1, '길게(1초) 누르면 모두 끄기');
    m.send(1);
    m.set({ net: { down: true } });
    ok(await wait(p, `!document.getElementById('lost').hidden && document.getElementById('readout').classList.contains('stale')`, 9000), '끊기면 주황 띠 · 표시창은 마지막 상태로 흐리게');
    await shot(p, 'e2e-remote-lost');
    m.set({ net: { down: false } });
    ok(await wait(p, `document.getElementById('lost').hidden && document.getElementById('connDot').className.includes('ok')`, 9000), '다시 붙으면 띠가 사라진다 (저절로)');
  }
  await p.eval(`history.back(); 1`);
  await sleep(700);
  ok(await p.eval(`!document.getElementById('main').hidden && location.pathname === '/'`), '뒤로 가기를 눌러도 리모컨이 닫히지 않는다');
  await p.close();
}

async function editFlow(b, url, pin, m) {
  console.log('— 웹 편집기 ' + url);
  const p = await b.page(url, { w: 1280, h: 800, dpr: 1, mobile: false, touch: false });
  ok(await wait(p, `!document.getElementById('login').hidden`), 'PIN 화면');
  await p.eval(`document.getElementById('pin').value = 'x0x0'; document.getElementById('go').click(); 1`);
  ok(await wait(p, `document.getElementById('lerr').textContent.length > 3`), '틀린 PIN → 안내');
  await p.eval(`document.getElementById('pin').value = '${pin}'; document.getElementById('go').click(); 1`);
  ok(await wait(p, `!document.getElementById('app').hidden && document.querySelector('#list button[data-id]')`), '맞는 PIN → 큐시트 목록');
  const H = { 'X-PIN': pin };
  const st = await (await fetch(url + 'e/state', { headers: H })).json();
  const id = (st.cues.find(c => !c.template && c.count > 1) || st.cues[0]).id;
  await p.eval(`document.querySelector('#list button[data-id="${id}"]').click(); 1`);
  ok(await wait(p, `document.querySelector('#list button[data-id="${id}"]').classList.contains('on') && document.querySelector('.item input[data-f=title]')`), '큐시트를 열면 순서 표');
  await p.eval(`(() => { const i = document.querySelector('.item input[data-f=title]'); i.focus(); i.value += ' (웹)'; i.dispatchEvent(new Event('input', { bubbles: true })); return 1; })()`);
  ok(await p.eval(`document.getElementById('state').classList.contains('dirty') && !document.getElementById('save').disabled && document.querySelector('.item').classList.contains('changed')`), '고치면 고친 것 있음 · 저장 단추 켜짐 · 바뀐 줄 표시');
  await p.key('s', 'KeyS', 2); // Ctrl+S
  ok(await wait(p, `document.getElementById('msg').textContent.includes('저장') || document.getElementById('msg').textContent.includes('Saved')`), 'Ctrl+S → 저장됨');
  const after = (await (await fetch(url + 'e/cue?id=' + id, { headers: H })).json()).cue;
  ok(after.items[0].title.endsWith(' (웹)'), 'PC 쪽에 들어감');
  // 409: 다른 곳이 먼저 저장
  await p.eval(`(() => { const i = document.querySelector('.item input[data-f=title]'); i.value = i.value.replace(' (웹)', ''); i.dispatchEvent(new Event('input', { bubbles: true })); return 1; })()`);
  const other = JSON.parse(JSON.stringify(after.items)); other[0].title = after.items[0].title.replace(' (웹)', ' (조작 화면)');
  const r = await fetch(url + 'e/save', { method: 'POST', headers: { ...H, 'X-WorshipCue': '1', 'Content-Type': 'application/json' }, body: JSON.stringify({ id, base: after.updatedAt, items: other }) });
  ok(r.ok, '(다른 곳에서 먼저 저장)');
  await p.eval(`document.getElementById('save').click(); 1`);
  ok(await wait(p, `!document.getElementById('banner').hidden && document.getElementById('state').classList.contains('err')`), '409 → 충돌 띠 · 저장 안 됨 표시');
  const mid = (await (await fetch(url + 'e/cue?id=' + id, { headers: H })).json()).cue;
  ok(mid.items[0].title.endsWith(' (조작 화면)'), '다른 곳의 고침을 덮어쓰지 않음');
  await shot(p, 'e2e-edit-conflict');
  await p.eval(`document.querySelector('#banner .btn.send').click(); 1`);
  ok(await wait(p, `document.getElementById('banner').hidden && document.querySelector('.item input[data-f=title]').value.endsWith(' (조작 화면)')`), '[최신으로 다시 불러오기] → 그쪽 고침이 보임');
  // 더하기 · 지우기 · 되돌리기 · 키보드
  const n = await p.eval(`document.querySelectorAll('.item').length`);
  await p.eval(`document.querySelector('[data-add=bible]').click(); 1`);
  ok(await wait(p, `document.querySelectorAll('.item').length === ${n + 1} && document.querySelector('.item.open input[data-f=ref]') === document.activeElement`), '성경 더하기 → 맨 끝에 펼쳐지고 장절 칸에 커서');
  await p.type('요 3:16');
  await p.eval(`document.activeElement.blur(); 1`);
  await p.eval(`document.querySelector('.item[data-k="0"] [data-del]').click(); 1`);
  ok(await wait(p, `!!document.querySelector('.item .confirm')`), '지우기 → 한 번 더 묻기');
  await p.eval(`document.querySelector('.item .confirm [data-delyes]').click(); 1`);
  ok(await wait(p, `document.querySelectorAll('.item').length === ${n}`), '지움');
  await p.eval(`document.querySelector('.toast .act').click(); 1`);
  ok(await wait(p, `document.querySelectorAll('.item').length === ${n + 1}`), '알림의 [되돌리기] → 돌아옴');
  await p.eval(`document.querySelector('.item[data-k="0"] .head .n').click(); 1`); // 펼침 = 고름
  await p.eval(`document.activeElement && document.activeElement.blur(); 1`);
  const t0 = await p.eval(`document.querySelector('.item[data-k="0"] input[data-f=title]').value`);
  await p.key('ArrowDown', 'ArrowDown', 1); // Alt+↓
  ok(await wait(p, `document.querySelector('.item[data-k="1"] input[data-f=title]').value === ${JSON.stringify(t0)}`), 'Alt+↓ → 고른 순서가 아래로');
  await p.eval(`document.getElementById('save').click(); 1`);
  ok(await wait(p, `document.getElementById('state').classList.contains('saved')`), '저장');
  const fin = (await (await fetch(url + 'e/cue?id=' + id, { headers: H })).json()).cue;
  ok(fin.items.length === n + 1 && fin.items.at(-1).ref === '요 3:16' && fin.items[1].title === t0, `PC 쪽: 성경 순서 더해짐 · 차례 바뀜 (${fin.items.length}개)`);
  // 휴대폰 폭: 목록 → 표
  await p.size(390, 844);
  await sleep(400);
  ok(await p.eval(`getComputedStyle(document.querySelector('.list-pane')).display === 'none' && getComputedStyle(document.getElementById('back')).display !== 'none'`), '휴대폰 폭: 표만 보이고 [‹ 목록] 단추');
  await shot(p, 'e2e-edit-phone');
  await p.eval(`document.getElementById('back').click(); 1`);
  ok(await wait(p, `getComputedStyle(document.querySelector('.list-pane')).display !== 'none'`), '[‹] → 목록');
  await p.close();
}

async function camFlow(b, bDeny, url, m) {
  console.log('— 휴대폰 카메라 ' + url);
  const p = await b.page(url, { w: 390, h: 844 });
  ok(await wait(p, `/ready|on|sending/.test(document.getElementById('dot').className)`, 15000), '가짜 카메라 허락 → PC 연결 (dot ready)');
  ok(await p.eval(`document.getElementById('s1').className === 'done' && document.getElementById('s2').className === 'done'`), '1 · 2 단계 끝 표시');
  ok(await p.eval(`/^p[a-z0-9]+$/.test(localStorage.getItem('wp-cam-id') || '')`), '이 휴대폰 id 를 기억 (wp-cam-id)');
  if (m) {
    m.set({ cam: { onAir: ['main'] } });
    ok(await wait(p, `document.getElementById('app').classList.contains('onair') && !document.getElementById('air').hidden && document.getElementById('dot').className.includes('on')`), 'PC 가 이 영상을 송출하면 빨강 · 송출 중 · 예배당');
    await shot(p, 'e2e-cam-onair');
    m.set({ cam: { onAir: [] }, camKey: 'changed0000000000' });
    ok(await wait(p, `!document.getElementById('hint').hidden && document.getElementById('hintTitle').textContent.length > 3 && document.getElementById('app').classList.contains('lost')`), '열쇠가 바뀌면 QR 을 다시 찍으라는 안내');
    m.set({ camKey: 'k123456789abcdef' });
  }
  await p.close();
  if (bDeny) {
    const q = await bDeny.page(url, { w: 390, h: 844 });
    ok(await wait(q, `!document.getElementById('hint').hidden && document.getElementById('hint').className.includes('err') && !document.getElementById('start').hidden`, 12000), '카메라 허락을 거부하면 허락하는 법 + [다시 시도]');
    await shot(q, 'e2e-cam-denied');
    await q.close();
  }
}

(async () => {
  const real = !!(arg('--remote') || arg('--edit') || arg('--cam'));
  const m = real ? null : await mock.start({ web: arg('--web') });
  const prof = path.join(os.tmpdir(), 'wp-webui-e2e-' + process.pid);
  const b = await open(prof, ['--use-fake-device-for-media-stream', '--use-fake-ui-for-media-stream', '--ignore-certificate-errors']);
  const bDeny = only.includes('cam') ? await open(prof + '-deny', ['--use-fake-device-for-media-stream', '--deny-permission-prompts', '--ignore-certificate-errors']) : null;
  try {
    if (only.includes('remote') && (m || arg('--remote'))) { if (m) m.reset(); await remoteFlow(b, m ? m.remoteUrl : arg('--remote'), m ? m.state.pin : arg('--rpin'), m); }
    if (only.includes('edit') && (m || arg('--edit'))) { if (m) m.reset(); await editFlow(b, m ? m.editUrl : arg('--edit'), m ? m.state.editPin : arg('--epin'), m); }
    if (only.includes('cam') && (m || arg('--cam'))) { if (m) m.reset(); await camFlow(b, bDeny, m ? m.camUrl : arg('--cam'), m); }
  } catch (e) {
    fail++;
    console.log('실패  (예외) ' + (e && e.stack || e));
  } finally {
    b.close(); if (bDeny) bDeny.close();
    try { b.proc.kill(); if (bDeny) bDeny.proc.kill(); } catch { /* 이미 끝남 */ }
    if (m) m.close();
  }
  console.log(`\n${pass} 통과 · ${fail} 실패`);
  process.exit(fail ? 1 : 0);
})();
