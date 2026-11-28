// 끝에서 끝: 로그인 · 실시간 중계 · 리모컨/편집기 되돌림 · 끊김과 다시 이음 · 여러 화면 · 권한 · 열쇠 지우기
// scripts/e2e.mjs 가 wrangler dev 를 띄운 뒤 부른다. 가짜 설치판(fakeBridge)이 설치판 자리에 붙는다.
import test from 'node:test';
import assert from 'node:assert/strict';
import { BASE, CHURCH, cred, req, login, socket, fakeBridge, newIp, sleep, cookieOf } from './helpers.mjs';

const render = (v, text = `본문 ${v}`) => ({ output: 'main', version: v, hasLive: true, visible: true, slide: { kind: 'text', parts: [{ text }] }, template: {} });

test('공개된 것은 상태 확인 · 정적 코드뿐, 나머지는 로그인', async () => {
  const h = await req('/health');
  assert.equal(h.status, 200);
  assert.deepEqual(Object.keys(h.json).sort(), ['configured', 'ok', 'protocol']);
  assert.equal((await req('/shared/render.js')).status, 200, '공개 GPL 코드는 로그인 없이');
  assert.equal((await req('/cloud/adapter.js')).status, 200);
  const p = await req('/outputs/main');
  assert.equal(p.status, 302);
  assert.match(p.headers.get('location'), /\/login\?next=%2Foutputs%2Fmain$/);
  for (const path of ['/r/state', '/e/state', '/e/cue?id=x', '/lib/file?p=x', '/media/a.png', '/api/v1/me', '/api/v1/room', '/api/v1/credentials']) {
    assert.equal((await req(path)).status, 401, path);
  }
  assert.equal((await req('/r/do', { method: 'POST', body: { action: 'next' } })).status, 401);
  const ws = socket('/events?client=main');
  await assert.rejects(ws.opened, e => e.status === 401);
  const br = socket('/bridge', { origin: null });
  await assert.rejects(br.opened, e => e.status === 401, '열쇠 없는 설치판 연결');
  const bad = socket('/bridge', { headers: { authorization: `Bearer ${CHURCH}.wrongwrongwrongwrongwrong` }, origin: null });
  await assert.rejects(bad.opened, e => e.status === 401);
  assert.equal((await req('/nothing')).status, 404);
});

test('출력 열쇠 주소로 열면 쿠키로 바꾸고 주소에서 열쇠를 지운다 · 페이지는 원래 web/ 에 어댑터만', async () => {
  const r = await req(`/outputs/overlay?key=${encodeURIComponent(cred('output').key)}`, { ip: newIp() });
  assert.equal(r.status, 302);
  assert.equal(r.headers.get('location'), '/outputs/overlay');
  const cookie = cookieOf(r.headers);
  assert.match(cookie, /wp_session=v1\./);
  assert.match(r.headers.getSetCookie().join('\n'), /HttpOnly/);
  const page = await req('/outputs/overlay', { cookie });
  assert.equal(page.status, 200);
  assert.match(page.text, /<script src="\/cloud\/adapter\.js"><\/script>/);
  assert.match(page.text, /\/outputs\/output\.js/, '설치판과 같은 출력 페이지');
  assert.match(page.text, /"role":"output"/);
  assert.equal(page.headers.get('cache-control'), 'no-store');
  // 무대 화면 · 추가 출력도 같은 길
  assert.match((await req('/outputs/stage', { cookie })).text, /\/outputs\/stage\.js/);
  assert.equal((await req('/outputs/out-abc', { cookie })).status, 200);
  // 출력 열쇠로는 리모컨 · 편집기 · 주인 화면을 못 연다
  assert.equal((await req('/r/state', { cookie })).status, 403);
  assert.equal((await req('/e/state', { cookie })).status, 403);
  assert.equal((await req('/owner', { cookie })).status, 403);
  assert.equal((await req('/api/v1/credentials', { cookie })).status, 403);
  // 잘못된 열쇠는 그대로 401
  assert.equal((await req(`/outputs/main?key=${CHURCH}.wrongwrongwrongwrong`, { ip: newIp() })).status, 401);
});

test('설치판 → 여러 화면 동시 중계 · 새로 붙은 화면은 지금 것을 바로 · 화면별로 자기 것만', async () => {
  const cookie = await login('output');
  const bridge = await fakeBridge();
  bridge.send({ t: 'render', client: 'main', data: render(1) });
  bridge.send({ t: 'render', client: 'stage', data: { version: 1, stage: true, current: '무대' } });
  await sleep(150);
  const views = await Promise.all([0, 1, 2, 3, 4].map(async () => { const v = socket('/events?client=main', { cookie }); await v.opened; return v; }));
  for (const v of views) {
    const hello = await v.next('hello');
    assert.equal(hello.online, true);
    assert.ok(hello.id > 0);
    const first = await v.next('render');
    assert.equal(first.data.version, 1, '붙자마자 마지막 화면');
  }
  const stage = socket('/events?client=stage', { cookie });
  await stage.opened;
  assert.equal((await stage.next('render')).data.current, '무대');
  bridge.send({ t: 'render', client: 'main', data: render(2) });
  for (const v of views) assert.equal((await v.next('render')).data.version, 2);
  await assert.rejects(stage.next('render', 400), /오지 않음/, '무대 화면은 예배당 자료를 받지 않는다');
  // 그리기는 무대를 빼고
  bridge.send({ t: 'draw', data: { strokes: [[1, 2]], spot: null } });
  assert.deepEqual((await views[0].next('draw')).data.strokes, [[1, 2]]);
  await assert.rejects(stage.next('draw', 400), /오지 않음/);
  // 설치판이 붙은 화면 목록을 받는다
  const seen = await bridge.next(m => m.t === 'viewers' && m.list.length >= 6, 3000);
  assert.ok(seen.list.every(x => ['main', 'stage'].includes(x.client)));
  // 출력 화면의 수신 확인 · 영상 알림은 실시간 연결로
  views[0].send({ t: 'ack', version: 2, width: 1920, height: 1080, visible: true });
  views[0].send({ t: 'cmd', cmd: 'videoEnded', args: { key: 'k', duration: 3 } });
  const cmd = await bridge.next('cmd');
  assert.deepEqual([cmd.cmd, cmd.args.key, cmd.args.duration], ['videoEnded', 'k', 3]);
  views[0].send({ t: 'cmd', cmd: 'clearAll', args: {} });
  await assert.rejects(bridge.next('cmd', 400), /오지 않음/, '출력 화면은 영상 알림 말고 명령을 못 보낸다');
  for (const v of [...views, stage]) await v.close();
  await bridge.close();
});

test('설치판이 끊기면 화면은 마지막 것을 두고 끊김을 알림 · 다시 이으면 이어 받음 · 요청은 503', async () => {
  const cookie = await login('output');
  let bridge = await fakeBridge();
  bridge.send({ t: 'render', client: 'main', data: render(10) });
  const v = socket('/events?client=main', { cookie });
  await v.opened;
  await v.next('hello');
  assert.equal((await v.next('render')).data.version, 10);
  await bridge.close();
  assert.equal((await v.next('status')).online, false);
  const rc = await login('remote');
  const off = await req('/r/do', { method: 'POST', cookie: rc, headers: { 'x-worshipcue': '1', origin: BASE }, body: { action: 'next' } });
  assert.equal(off.status, 503);
  assert.match(off.json.error, /연결되어 있지 않습니다/);
  bridge = await fakeBridge();
  assert.equal((await v.next('status')).online, true);
  // 다시 붙은 설치판은 처음부터 전체를 보낸다 (설치판 쪽 규칙) → 화면이 이어 받는다
  bridge.send({ t: 'render', client: 'main', data: render(11) });
  assert.equal((await v.next('render')).data.version, 11);
  // 보는 쪽이 끊겼다 다시 붙으면 지금 것을 바로
  await v.close();
  const v2 = socket('/events?client=main', { cookie });
  await v2.opened;
  await v2.next('hello');
  assert.equal((await v2.next('render')).data.version, 11);
  // 같은 교회에 설치판이 또 붙으면 앞의 것을 끊는다 (4001), 화면에는 끊김을 알리지 않는다
  const second = await fakeBridge();
  assert.equal((await bridge.waitClose()).code, 4001);
  await assert.rejects(v2.next('status', 500).then(m => { if (m.online === false) throw new Error('끊김을 알림'); throw new Error('오지 않음'); }), /오지 않음/);
  await second.close();
  await v2.close();
});

test('리모컨 PIN 로그인 · 상태는 실시간으로 받아 둔 것 · 조작은 설치판으로 돌아간다', async () => {
  const bridge = await fakeBridge({ answer: m => (m.path === '/r/do' ? { body: { ok: true, result: { ok: true }, got: m.body.action } } : { body: { ok: true } }) });
  bridge.send({ t: 'remote', data: { live: { has: true, index: 2, count: 5, title: '요 3:16' }, cue: null, control: true } });
  await sleep(150);
  // 교회만 아는 주소로 열면 PIN 칸이 있는 원래 리모컨 페이지 (자료 없음)
  const shell = await req(`/remote?c=${CHURCH}`);
  assert.equal(shell.status, 200);
  assert.match(shell.text, /"authed":false/);
  assert.match(shell.text, /<script src="\/remote\.js"><\/script>/);
  // 틀린 PIN
  const ip = newIp();
  const wrong = await req(`/r/state?pin=000000&c=${CHURCH}`, { ip });
  assert.equal(wrong.status, 401);
  // 맞는 PIN → 상태 + 쿠키
  const ok = await req(`/r/state?pin=${cred('remote').secret}&c=${CHURCH}`, { ip });
  assert.equal(ok.status, 200);
  assert.equal(ok.json.ok, true);
  assert.equal(ok.json.live.title, '요 3:16');
  const cookie = cookieOf(ok.headers);
  // 그 쿠키로 조작 → 설치판이 받고 답
  const done = await req('/r/do', { method: 'POST', cookie, headers: { 'x-worshipcue': '1', origin: BASE }, body: { pin: 'cloud', action: 'next' } });
  assert.equal(done.status, 200);
  assert.equal(done.json.got, 'next');
  const got = bridge.reqs.find(r => r.path === '/r/do');
  assert.equal(got.method, 'POST');
  assert.equal(got.body.action, 'next');
  // 실시간 리모컨 상태
  const live = socket('/events?client=remote', { cookie });
  await live.opened;
  assert.equal((await live.next('remote')).data.live.index, 2);
  bridge.send({ t: 'remote', data: { live: { has: true, index: 3, count: 5 } } });
  assert.equal((await live.next('remote')).data.live.index, 3);
  // 다른 사이트에서 몰래 보내는 쓰기는 막는다
  assert.equal((await req('/r/do', { method: 'POST', cookie, headers: { 'x-worshipcue': '1', origin: 'https://evil.example' }, body: { action: 'next' } })).status, 403);
  assert.equal((await req('/r/do', { method: 'POST', cookie, body: { action: 'next' } })).status, 403, 'X-WorshipCue 없이');
  // 리모컨 PIN 으로는 출력 · 편집기를 못 연다
  assert.equal((await req('/outputs/main', { cookie })).status, 403);
  assert.equal((await req('/e/state', { cookie })).status, 403);
  // 설치판이 늦으면 504 (자동으로 다시 보내지 않는다)
  await live.close();
  await bridge.close();
});

test('웹 편집기: PIN(X-PIN) 로그인 · 요청을 설치판으로 · 충돌 409 를 그대로', async () => {
  const bridge = await fakeBridge({
    answer: m => {
      if (m.path === '/e/state') return { body: { ok: true, cues: [{ id: 'c1', name: '주일' }], active: 'c1' } };
      if (m.path === '/e/save') return { status: 409, body: { ok: false, conflict: true, error: '그 사이 고쳤습니다' } };
      return { status: 404, body: { ok: false } };
    },
  });
  const ip = newIp();
  const st = await req(`/e/state?c=${CHURCH}`, { headers: { 'x-pin': cred('editor').secret }, ip });
  assert.equal(st.status, 200);
  assert.equal(st.json.cues[0].name, '주일');
  const cookie = cookieOf(st.headers);
  const save = await req('/e/save', { method: 'POST', cookie, headers: { 'x-worshipcue': '1', origin: BASE }, body: { id: 'c1', base: 'old', items: [] } });
  assert.equal(save.status, 409);
  assert.equal(save.json.conflict, true);
  assert.equal((await req('/editor', { cookie })).status, 200);
  assert.equal((await req('/remote', { cookie })).status, 403);
  await bridge.close();
});

test('미디어(그림 · 영상 조각)는 설치판이 이진 조각으로 보내 준다', async () => {
  const png = Buffer.from([0x89, 0x50, 0x4e, 0x47, 1, 2, 3, 4, 5]);
  const bridge = await fakeBridge({ answer: m => (m.path === '/lib/file' ? { bytes: png, type: 'image/png' } : { status: 404, body: { ok: false } }) });
  const cookie = await login('output');
  const r = await fetch(`${BASE}/lib/file?p=${encodeURIComponent('C:\\그림\\a.png')}`, { headers: { cookie } });
  assert.equal(r.status, 200);
  assert.equal(r.headers.get('content-type'), 'image/png');
  assert.deepEqual(Buffer.from(await r.arrayBuffer()), png);
  assert.equal(bridge.reqs[0].query.p, 'C:\\그림\\a.png');
  // 같은 파일은 방이 잠깐 기억한다 (설치판에 다시 묻지 않음)
  await fetch(`${BASE}/lib/file?p=${encodeURIComponent('C:\\그림\\a.png')}`, { headers: { cookie } }).then(x => x.arrayBuffer());
  assert.equal(bridge.reqs.length, 1);
  await bridge.close();
});

test('주인: 기기 열쇠 만들기 · 지우면 그 열쇠로 붙은 화면이 바로 끊긴다 · 방 상태', async () => {
  const owner = await login('owner');
  const me = await req('/api/v1/me', { cookie: owner });
  assert.deepEqual([me.json.church, me.json.role], [CHURCH, 'owner']);
  const made = await req('/api/v1/credentials', { method: 'POST', cookie: owner, headers: { 'x-worshipcue': '1', origin: BASE }, body: { role: 'output', label: '시험 OBS' } });
  assert.equal(made.status, 200);
  assert.match(made.json.credential.key, new RegExp(`^${CHURCH}\\.`));
  const list = await req('/api/v1/credentials', { cookie: owner });
  assert.ok(list.json.credentials.some(c => c.id === made.json.credential.id));
  assert.ok(!list.text.includes(made.json.credential.secret), '목록에 원문이 없다');
  const r = await req(`/outputs/main?key=${made.json.credential.key}`, { ip: newIp() });
  const cookie = cookieOf(r.headers);
  const bridge = await fakeBridge();
  const v = socket('/events?client=main', { cookie });
  await v.opened;
  await v.next('hello');
  const room = await req('/api/v1/room', { cookie: owner });
  assert.equal(room.json.online, true);
  assert.ok(room.json.viewers.length >= 1);
  const del = await req(`/api/v1/credentials/${made.json.credential.id}`, { method: 'DELETE', cookie: owner, headers: { 'x-worshipcue': '1', origin: BASE } });
  assert.equal(del.status, 200);
  assert.equal((await v.waitClose()).code, 4003);
  await sleep(50);
  // 지운 열쇠의 쿠키는 더는 안 된다 (다른 Worker 인스턴스에서는 길어도 30초 안에)
  assert.equal((await req('/outputs/main', { cookie })).status, 302);
  await bridge.close();
});

test('다른 역할 쿠키가 있는 브라우저에서도 PIN · 열쇠로 다시 로그인된다', async () => {
  const bridge = await fakeBridge();
  bridge.send({ t: 'remote', data: { live: { has: false } } });
  await sleep(100);
  const out = await login('output');
  assert.equal((await req('/remote', { cookie: out })).status, 403, '열쇠 없이는 권한 없음');
  const r = await req(`/r/state?pin=${cred('remote').secret}&c=${CHURCH}`, { cookie: out, ip: newIp() });
  assert.equal(r.status, 200);
  const rc = cookieOf(r.headers);
  assert.equal((await req('/remote', { cookie: rc })).status, 200);
  await bridge.close();
});

// ---- 잠금 시험은 맨 뒤 (교회 PIN 잠금이 걸리므로) ----
test('여러 번 틀리면 잠시 막는다 (기기 주소별 5번)', async () => {
  const ip = newIp();
  for (let i = 0; i < 5; i++) assert.equal((await req('/auth/login', { method: 'POST', ip, headers: { 'x-worshipcue': '1' }, body: { church: CHURCH, secret: 'wrong-' + i } })).status, 401);
  const locked = await req('/auth/login', { method: 'POST', ip, headers: { 'x-worshipcue': '1' }, body: { church: CHURCH, secret: cred('owner').secret } });
  assert.equal(locked.status, 429, '맞는 열쇠여도 잠긴 동안은 안 됨');
  // 다른 기기는 그대로
  assert.equal((await req('/auth/login', { method: 'POST', ip: newIp(), headers: { 'x-worshipcue': '1' }, body: { church: CHURCH, secret: cred('owner').secret } })).status, 200);
});

test('틀린 PIN 을 많이 보내도 설치판 연결 · 출력 열쇠 로그인은 막히지 않는다', async () => {
  for (let i = 0; i < 32; i++) await req(`/r/state?pin=${String(100000 + i)}&c=${CHURCH}`, { ip: newIp() });
  const pin = await req(`/r/state?pin=${cred('remote').secret}&c=${CHURCH}`, { ip: newIp() });
  assert.equal(pin.status, 429, 'PIN 은 교회 전체로 잠김');
  const bridge = await fakeBridge();
  assert.equal(bridge.ready.v, 1, '설치판은 그대로 붙음');
  const out = await req(`/outputs/main?key=${cred('output').key}`, { ip: newIp() });
  assert.equal(out.status, 302, '출력 열쇠 로그인도 그대로');
  assert.ok(cookieOf(out.headers).includes('wp_session='));
  await bridge.close();
});
