// 외부 연동 계약 대조: Electron 판과 Rust 판의 HTTP 길·인증·응답 모양을 같은 요청으로 비교한다.
//   조작 서버(127.0.0.1): /api/command(X-WorshipCue) · /api/trigger/<동작>?key= · /api/state · /api/live · /dock · /r/state · /r/do · Host 검사
//   휴대폰 리모컨 서버(PIN): / · /r/state?pin= · /r/do
//   내부망 출력 서버(key): 두 판 모두 켜져 있으면 /outputs/main?key= · 막힌 길
// 준비: node tools\flow-compare\prep.js → launch.js start
// 실행: node tools\flow-compare\contract.js [--el 포트] [--rs 포트] [--rs-data tmp\rs-data] [--el-data tmp\el-data] [--out tmp\contract.json]
'use strict';
const fs = require('fs'); const path = require('path'); const http = require('http');
const A = process.argv.slice(2); const opt = (k, d) => { const i = A.indexOf('--' + k); return i >= 0 ? A[i + 1] : d; };
const ports = (() => { try { return JSON.parse(fs.readFileSync('tmp/flow-ports.json', 'utf8')); } catch { return {}; } })();
const EL = Number(opt('el', ports.el)), RS = Number(opt('rs', ports.rs));
const sOf = d => JSON.parse(fs.readFileSync(path.join(d, 'settings.json'), 'utf8'));
const SE = sOf(opt('el-data', 'tmp/el-data')), SR = sOf(opt('rs-data', 'tmp/rs-data'));

function req(port, method, p, { headers = {}, body, host } = {}) {
  return new Promise(resolve => {
    const data = body === undefined ? null : Buffer.from(typeof body === 'string' ? body : JSON.stringify(body));
    const r = http.request({ host: '127.0.0.1', port, method, path: p, headers: { ...(data ? { 'content-type': 'application/json', 'content-length': data.length } : {}), ...(host ? { host } : {}), ...headers }, timeout: 8000 }, res => {
      const ch = []; res.on('data', c => ch.push(c)); res.on('end', () => { const text = Buffer.concat(ch).toString('utf8'); let json; try { json = JSON.parse(text); } catch {} resolve({ status: res.statusCode, type: (res.headers['content-type'] || '').split(';')[0], json, text }); });
    });
    r.on('timeout', () => { r.destroy(); resolve({ status: 0, error: 'timeout' }); });
    r.on('error', e => resolve({ status: 0, error: e.code || e.message }));
    if (data) r.write(data); r.end();
  });
}
// 값 대신 모양(형)만: 두 판의 내용(시각·버전 숫자)은 달라도 된다
const shape = (v, d = 0) => v === null ? 'null' : Array.isArray(v) ? (v.length ? [shape(v[0], d + 1)] : []) : typeof v === 'object' ? (d > 2 ? '{…}' : Object.fromEntries(Object.keys(v).sort().map(k => [k, shape(v[k], d + 1)]))) : typeof v;
const rows = []; let bad = 0;
// cmp: 'status' 상태 코드만 · 'ok' 상태+ok · 'shape' 상태+응답 모양 · 'keys' 상태+맨 위 키(Electron 키가 Rust 에 다 있는지)
async function both(name, fn, cmp = 'shape', note = '') {
  const [e, r] = [await fn('el'), await fn('rs')];
  const view = x => cmp === 'status' || cmp === 'keys' ? { status: x.status } : cmp === 'ok' ? { status: x.status, ok: x.json ? x.json.ok : undefined } : { status: x.status, body: x.json !== undefined ? shape(x.json) : (x.type || x.error) };
  let same = JSON.stringify(view(e)) === JSON.stringify(view(r));
  let missing = [];
  if (cmp === 'keys' && e.json && r.json) { missing = Object.keys(e.json).filter(k => !(k in r.json)); same = same && !missing.length; }
  if (!same && !note) bad++;
  rows.push({ name, same, note, el: { status: e.status, body: e.json !== undefined ? e.json : (e.text || e.error || '').slice(0, 120) }, rs: { status: r.status, body: r.json !== undefined ? r.json : (r.text || r.error || '').slice(0, 120) }, missing });
  const brief = x => `${x.status} ${JSON.stringify(x.json !== undefined ? x.json : (x.text || x.error || '').slice(0, 60)).slice(0, 170)}`;
  console.log(`${same ? '✓' : note ? '△' : '✗'} ${name}${same ? '' : `${note ? ' — ' + note : ''}\n     E ${brief(e)}\n     R ${brief(r)}${missing.length ? '\n     Rust 에 없는 키: ' + missing.join(' ') : ''}`}`);
}
const P = w => (w === 'el' ? EL : RS), S = w => (w === 'el' ? SE : SR);
const H = { 'x-worshipcue': '1' };
const cmd = (name, args, cmp, note) => both(`명령 ${name}${args && Object.keys(args).length ? ' ' + JSON.stringify(args) : ''}`, w => req(P(w), 'POST', '/api/command', { headers: H, body: { cmd: name, args: args || {} } }), cmp || 'ok', note);

(async () => {
  console.log(`Electron ${EL} · Rust ${RS}`);
  console.log('--- 조작 서버: 보안 경계');
  await both('POST /api/command 머리글 없음 → 403', w => req(P(w), 'POST', '/api/command', { body: { cmd: 'nextSlide' } }), 'status');
  await both('다른 Host 이름 → 421', w => req(P(w), 'GET', '/api/live', { host: 'evil.example.com' }), 'status');
  await both('GET /api/trigger/next 열쇠 없음 → 403', w => req(P(w), 'GET', '/api/trigger/next'), 'shape');
  await both('GET /api/trigger/next 틀린 열쇠 → 403', w => req(P(w), 'GET', '/api/trigger/next?key=wrongwrongwro'), 'shape');
  await both('POST /r/do(도킹) 머리글 없음 → 403', w => req(P(w), 'POST', '/r/do', { body: { action: 'next' } }), 'status');
  // 두 판에 같은 것을 송출해 둔다 (바깥에서 만든 슬라이드 3장 — Electron sendLive({ passage, slides, index }))
  const slide = (x, i) => ({ kind: 'text', ref: '', title: '', label: String(i + 1), parts: [{ book: 0, chapter: 0, verse: 0, text: x }] });
  const deck = { passage: { title: '계약 대조', itemTitle: '', versionName: '', cueId: '', cueIndex: -1, itemType: 'text' }, slides: ['첫 장', '둘째 장', '셋째 장'].map(slide), index: 0 };
  await both('명령 sendLive (슬라이드 3장을 바깥에서)', w => req(P(w), 'POST', '/api/command', { headers: H, body: { cmd: 'sendLive', args: deck } }), 'ok');
  await both('명령 setPreview (미리보기에 올림)', w => req(P(w), 'POST', '/api/command', { headers: H, body: { cmd: 'setPreview', args: { ...deck, passage: { ...deck.passage, title: '미리보기' } } } }), 'ok');
  console.log('--- 조작 서버: 읽기');
  await both('GET /api/live', w => req(P(w), 'GET', '/api/live'), 'keys');
  await both('GET /api/state (Electron 의 맨 위 키)', w => req(P(w), 'GET', '/api/state'), 'keys');
  await both('GET /dock', w => req(P(w), 'GET', '/dock'), 'status');
  await both('GET /remote.js', w => req(P(w), 'GET', '/remote.js'), 'status');
  await both('GET /groups.js', w => req(P(w), 'GET', '/groups.js'), 'status');
  await both('GET /r/state (도킹, PIN 없음)', w => req(P(w), 'GET', '/r/state'), 'keys');
  await both('GET /control/ (조작 화면 웹)', w => req(P(w), 'GET', '/control/'), 'status', 'Rust 판 조작 화면은 egui 라 웹 조작 화면이 없다 (의도한 차이)');
  console.log('--- 외부 단추 /api/trigger');
  const trig = (a, q = '') => both(`trigger ${a}${q}`, w => req(P(w), 'GET', `/api/trigger/${a}?key=${encodeURIComponent(S(w).triggerKey)}${q}`), 'shape');
  for (const a of ['next', 'prev', 'hide', 'hide', 'logo', 'logo', 'bg', 'bg', 'overlays', 'overlays', 'timer', 'timer-pause', 'timer-pause', 'timer-reset', 'draw-clear', 'clear-all', 'send', 'cue-next', 'cue-prev']) await trig(a, a === 'timer' ? '&min=5' : '');
  await trig('message', '&text=' + encodeURIComponent('외부 단추 메시지')); await trig('message', '&text=');
  await trig('macro', '&name=' + encodeURIComponent('없는 매크로')); await trig('no-such-action');
  console.log('--- /api/command (README: 외부 연동 명령)');
  await cmd('noSuchCommand', {}, 'shape');
  const cueId = SE.activeCueId || '';
  for (const [n, a] of [['listCues'], ['getCue', { id: cueId }], ['getCue', { id: 'no-such-cue' }], ['getLiveState'], ['getState'], ['nextSlide'], ['previousSlide'], ['gotoSlide', { index: 0 }], ['clearLive'], ['restoreLive'], ['toggleLive'], ['toggleLive'],
    ['stageMessage', { text: '명령 메시지' }], ['stageMessage', { text: '' }], ['stageTimer', { action: 'start', minutes: 3 }], ['stageTimer', { action: 'pause' }], ['stageTimer', { action: 'reset' }],
    ['setObsView', { pip: true }], ['setObsView', { full: true }], ['setObsView', { full: false }], ['listOverlays'], ['toggleOverlay', { id: 'ovclock1', on: true }], ['toggleOverlay', { id: 'ovclock1', on: false }], ['toggleOverlay', { id: 'no-such' }],
    ['toggleOverlaysOff'], ['toggleOverlaysOff'], ['toggleBackground'], ['toggleBackground'], ['toggleLogoScreen'], ['toggleLogoScreen'], ['showMessage', { text: '회중 메시지', outputs: ['main'] }], ['clearMessage'],
    ['runMacro', { name: '없는 매크로' }], ['videoControl', { action: 'pause' }], ['stagePinSet', { mode: 'order', name: '순서표' }], ['stagePinSet', {}], ['searchReference', { query: '요 3:16' }], ['searchReference', { query: '없는책 99:99' }], ['triggerInfo'], ['clearAll']]) await cmd(n, a);
  // 응답 모양까지 같은지 (외부 도구가 result 를 읽는 것들)
  for (const [n, a] of [['listCues'], ['getLiveState'], ['setObsView', {}], ['listOverlays'], ['triggerInfo']]) await both(`모양 ${n}`, w => req(P(w), 'POST', '/api/command', { headers: H, body: { cmd: n, args: a || {} } }), 'shape');
  // 큐시트 고쳐 저장 (README 의 saveCue): 메모에 한 글자 붙였다 되돌린다
  for (const w of ['el', 'rs']) {
    const g = await req(P(w), 'POST', '/api/command', { headers: H, body: { cmd: 'getCue', args: { id: cueId } } });
    const cue = g.json && g.json.result;
    if (!cue || !cue.items) { console.log(`✗ ${w} saveCue: getCue 실패 ${JSON.stringify(g.json).slice(0, 100)}`); bad++; rows.push({ name: `${w} saveCue 왕복`, same: false }); continue; }
    const s1 = await req(P(w), 'POST', '/api/command', { headers: H, body: { cmd: 'saveCue', args: { cue: { ...cue, memo: (cue.memo || '') + '!' } } } });
    const g2 = await req(P(w), 'POST', '/api/command', { headers: H, body: { cmd: 'getCue', args: { id: cueId } } });
    const ok = !!(s1.json && s1.json.ok && g2.json && g2.json.result && g2.json.result.memo === (cue.memo || '') + '!' && g2.json.result.items.length === cue.items.length);
    await req(P(w), 'POST', '/api/command', { headers: H, body: { cmd: 'saveCue', args: { cue } } });
    console.log(`${ok ? '✓' : '✗'} ${w} saveCue → getCue 로 바뀐 것 확인 · 되돌림${ok ? '' : ' ' + JSON.stringify(s1.json).slice(0, 120)}`); if (!ok) bad++;
    rows.push({ name: `${w} saveCue 왕복`, same: ok });
  }
  console.log('--- 도킹 /r/do (X-WorshipCue)');
  for (const a of [{ action: 'next' }, { action: 'prev' }, { action: 'toggle' }, { action: 'toggle' }, { action: 'goto', index: 0 }, { action: 'message', text: '도킹' }, { action: 'message', text: '' }, { action: 'timer', op: 'start', minutes: 1 }, { action: 'timer', op: 'reset' }, { action: 'obsPip' }, { action: 'obsPip' }, { action: 'obsFull' }, { action: 'obsFull' }, { action: 'nope' }])
    await both(`도킹 ${JSON.stringify(a)}`, w => req(P(w), 'POST', '/r/do', { headers: H, body: a }), 'ok');
  console.log('--- 휴대폰 리모컨 서버 (PIN)');
  const RP = w => S(w).remote.port, PIN = w => S(w).remote.pin;
  await both('GET /', w => req(RP(w), 'GET', '/'), 'status');
  await both('GET /remote.js', w => req(RP(w), 'GET', '/remote.js'), 'status');
  await both('GET /i18n.js', w => req(RP(w), 'GET', '/i18n.js'), 'status');
  await both('GET /r/state PIN 없음 → 401', w => req(RP(w), 'GET', '/r/state'), 'shape');
  await both('GET /r/state 맞는 PIN', w => req(RP(w), 'GET', `/r/state?pin=${PIN(w)}`), 'keys');
  await both('POST /r/do 틀린 PIN → 401', w => req(RP(w), 'POST', '/r/do', { body: { pin: '000000', action: 'next' } }), 'shape');
  for (const a of [{ action: 'next' }, { action: 'prev' }, { action: 'toggle' }, { action: 'toggle' }, { action: 'cue', index: 0 }, { action: 'message', text: '리모컨' }, { action: 'message', text: '' }])
    await both(`리모컨 ${JSON.stringify(a)}`, w => req(RP(w), 'POST', '/r/do', { body: { ...a, pin: PIN(w) } }), 'ok');
  await both('리모컨 서버에서 /api/command → 404', w => req(RP(w), 'POST', '/api/command', { headers: H, body: { cmd: 'nextSlide' } }), 'status');
  await both('리모컨 서버에서 /api/trigger → 404', w => req(RP(w), 'GET', `/api/trigger/next?key=${S(w).triggerKey}`), 'status');
  await both('리모컨 서버에서 /outputs/main → 404', w => req(RP(w), 'GET', '/outputs/main'), 'status');
  if (SE.lanOutput && SE.lanOutput.enabled && SR.lanOutput && SR.lanOutput.enabled) {
    console.log('--- 내부망 출력 서버 (key)');
    const LP = w => S(w).lanOutput.port, K = w => encodeURIComponent(S(w).lanOutput.key);
    await both('GET /outputs/main 열쇠 없음 → 403', w => req(LP(w), 'GET', '/outputs/main'), 'status');
    await both('GET /outputs/main?key=', w => req(LP(w), 'GET', `/outputs/main?key=${K(w)}`), 'status');
    await both('GET /outputs/stage?key=', w => req(LP(w), 'GET', `/outputs/stage?key=${K(w)}`), 'status');
    await both('GET /shared/render.js?key=', w => req(LP(w), 'GET', `/shared/render.js?key=${K(w)}`), 'status');
    await both('GET /api/state?key= (막힘)', w => req(LP(w), 'GET', `/api/state?key=${K(w)}`), 'status');
    await both('GET /api/live?key=', w => req(LP(w), 'GET', `/api/live?key=${K(w)}`), 'status');
    await both('GET /control/?key= (막힘)', w => req(LP(w), 'GET', `/control/?key=${K(w)}`), 'status');
    await both('GET /dock?key= (막힘)', w => req(LP(w), 'GET', `/dock?key=${K(w)}`), 'status');
    await both('GET /api/trigger/next?key= (막힘)', w => req(LP(w), 'GET', `/api/trigger/next?key=${K(w)}`), 'status');
    await both('POST /api/command nextSlide (막힘)', w => req(LP(w), 'POST', `/api/command?key=${K(w)}`, { headers: H, body: { cmd: 'nextSlide' } }), 'status');
    await both('POST /api/command videoEnded (허용)', w => req(LP(w), 'POST', `/api/command?key=${K(w)}`, { headers: H, body: { cmd: 'videoEnded', args: {} } }), 'status');
  } else console.log('--- 내부망 출력: 두 판 모두 켜져 있지 않아 건너뜀');
  console.log(`== 다른 것 ${bad}개 / ${rows.length}개`);
  fs.writeFileSync(opt('out', 'tmp/contract.json'), JSON.stringify(rows, null, 1));
})();
