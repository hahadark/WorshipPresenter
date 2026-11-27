// 웹 편집기 점검: 앱을 웹 편집기 켜고(PIN) 띄워 둔 상태에서
//   node tools/webedit/e2e.js --port 37645 --pin 4321 --out shot.png
// ① PIN 없이 → 401 ② 틀린 PIN → 401 ③ X-WorshipCue 없는 쓰기 → 403 ④ 브라우저로 PIN 넣고 첫 큐시트를 열어 제목을 고치고 저장
// ⑤ 예전 updatedAt 으로 저장 → 409(충돌) ⑥ 그림 저장
'use strict';
const path = require('path');
const { launch, sleep } = require('../flow-compare/cdp.js');
const arg = (k, d) => { const i = process.argv.indexOf(k); return i > 0 ? process.argv[i + 1] : d; };
const port = arg('--port', '37645'), pin = arg('--pin', ''), out = arg('--out', 'webedit.png');
const base = `http://127.0.0.1:${port}`;
const ok = (c, m) => { if (!c) { console.error('실패: ' + m); process.exitCode = 1; } else console.log('통과: ' + m); };
(async () => {
  ok((await fetch(base + '/e/state')).status === 401, 'PIN 없이는 401');
  ok((await fetch(base + '/e/state', { headers: { 'X-PIN': '0000' } })).status === 401, '틀린 PIN 은 401');
  const st = await (await fetch(base + '/e/state', { headers: { 'X-PIN': pin } })).json();
  ok(st.ok && st.cues.length > 0, `맞는 PIN 으로 목록 (${st.cues.length}개)`);
  const id = st.cues[0].id;
  ok((await fetch(base + '/e/save', { method: 'POST', headers: { 'X-PIN': pin, 'Content-Type': 'application/json' }, body: '{}' })).status === 403, 'X-WorshipCue 없는 쓰기는 403');
  const before = (await (await fetch(base + '/e/cue?id=' + id, { headers: { 'X-PIN': pin } })).json()).cue;
  const b = await launch(path.join(__dirname, '..', '..', 'tmp', 'webedit-profile'));
  const p = await b.open(base + '/', 1280, 860);
  await p.waitFor(`!document.getElementById('login').hidden`);
  await p.eval(`document.getElementById('pin').value='${pin}'; document.getElementById('go').click(); 1`);
  await p.waitFor(`!document.getElementById('app').hidden && document.querySelector('#list button')`);
  await p.eval(`document.querySelector('#list button[data-id="${id}"]').click(); 1`);
  await p.waitFor(`document.querySelector('.item input[data-f=title]')`);
  await p.eval(`(() => { const i = document.querySelector('.item input[data-f=title]'); i.value = i.value + ' (웹에서 고침)'; i.dispatchEvent(new Event('input', { bubbles: true })); document.querySelector('[data-add=bible]').click(); return 1; })()`);
  await sleep(200);
  await p.eval(`(() => { const all = document.querySelectorAll('.item'); const last = all[all.length - 1]; const r = last.querySelector('input[data-f=ref]'); r.value = '요 3:16'; r.dispatchEvent(new Event('input', { bubbles: true })); document.getElementById('save').click(); return 1; })()`);
  await p.waitFor(`document.getElementById('msg').textContent.includes('저장')`);
  const after = (await (await fetch(base + '/e/cue?id=' + id, { headers: { 'X-PIN': pin } })).json()).cue;
  ok(after.items[0].title.endsWith('(웹에서 고침)') && after.items.length === before.items.length + 1 && after.items.at(-1).ref === '요 3:16', '브라우저에서 고친 제목 · 더한 성경 순서가 저장됨');
  const r = await fetch(base + '/e/save', { method: 'POST', headers: { 'X-PIN': pin, 'X-WorshipCue': '1', 'Content-Type': 'application/json' }, body: JSON.stringify({ id, base: before.updatedAt, items: before.items }) });
  ok(r.status === 409, '예전 판으로 저장하면 409 (덮어쓰지 않음)');
  await sleep(300);
  await p.shot(out);
  console.log('그림: ' + out);
  await p.close(); b.proc.kill();
})().catch((e) => { console.error(e); process.exit(1); });
