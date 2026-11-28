// 끝에서 끝 (4단계 B 미디어): 휴대폰 링크 권한 · 휴대폰 페이지 · 신호 주고받기(가짜 휴대폰 ↔ 가짜 출력 화면) · 이름으로 찾기 · 늦게 붙은 화면의 시계.
// 영상 · 카메라 실물은 scripts/media-browser-e2e.mjs (헤드리스 엣지 · 가짜 카메라)가 본다.
import test from 'node:test';
import assert from 'node:assert/strict';
import { BASE, CHURCH, req, login, socket, fakeBridge, sleep } from '../e2e/helpers.mjs';

const W = { 'x-worshipcue': '1', origin: BASE };
const link = async (cookie, name = '') => req('/api/v1/media/phone-link', { method: 'POST', cookie, headers: W, body: { name } });
const kOf = url => new URL(url).searchParams.get('k');

test('휴대폰 링크: 쓰기 권한만 만든다 · 링크 없이는 휴대폰 페이지 · 연결이 안 열림', async () => {
  const out = await login('output');
  assert.equal((await link(out)).status, 403, '출력 열쇠로는 못 만든다');
  assert.equal((await req('/api/v1/media/phone-link', { method: 'POST', body: {} })).status, 401);
  const owner = await login('owner');
  const r = await link(owner, '강단 휴대폰');
  assert.equal(r.status, 200, r.text);
  assert.match(r.json.url, /\/phonecam\?k=ph1\./);
  assert.ok(r.json.expiresAt > Date.now() + 23 * 3600_000);
  assert.equal((await req('/phonecam')).status, 403);
  assert.equal((await req('/phonecam?k=ph1.bad.bad')).status, 403);
  const page = await req(`/phonecam?k=${kOf(r.json.url)}`);
  assert.equal(page.status, 200);
  assert.match(page.text, /<script src="\/cloud\/media\/phone\.js"><\/script>/);
  assert.match(page.text, /\/cam\/poll/, '설치판 휴대폰 페이지(web/phone/cam.html) 그대로');
  assert.equal(page.headers.get('referrer-policy'), 'no-referrer');
  const bad = socket('/rtc/phone?k=ph1.bad.bad', { origin: null });
  await assert.rejects(bad.opened, e => e.status === 403);
});

test('휴대폰 ↔ 출력 화면 신호: offer → 휴대폰, answer → 그 화면에만 · 없는 휴대폰은 offline · 이름으로도 찾음 · bye', async () => {
  const owner = await login('owner');
  const k = kOf((await link(owner)).json.url);
  const bridge = await fakeBridge();
  const phone = socket(`/rtc/phone?k=${k}`, { origin: null });
  await phone.opened;
  assert.equal((await phone.next('ready')).v, 1);
  phone.send({ t: 'hello', id: 'phoneAAA1', name: '강단 휴대폰', size: '1920x1080', viewers: 0 });
  await sleep(200);
  const list = await req('/rtc/phones', { cookie: owner });
  assert.equal(list.status, 200);
  assert.deepEqual(list.json.phones.map(p => [p.id, p.name, p.size]), [['phoneAAA1', '강단 휴대폰', '1920x1080']]);
  const out = await login('output');
  assert.equal((await req('/rtc/phones', { cookie: out })).status, 200, '출력 화면도 목록은 봄');
  const v1 = socket('/events?client=main', { cookie: out });
  const v2 = socket('/events?client=main', { cookie: out });
  await v1.opened; await v2.opened;
  const id1 = (await v1.next('hello')).id;
  await v2.next('hello');
  const sdp = 'v=0\r\n' + 'a=x\r\n'.repeat(1500); // 4KB 를 넘는 SDP (실제 크기)
  v1.send({ t: 'rtc', op: 'offer', phone: 'phoneAAA1', viewer: 'viewA001', sdp });
  const offer = await phone.next('offer');
  assert.equal(offer.viewer, 'viewA001');
  assert.equal(offer.sdp, sdp);
  assert.equal(offer.to, id1);
  phone.send({ t: 'answer', viewer: 'viewA001', sdp: 'v=0 answer', to: offer.to });
  const ans = await v1.next(m => m.t === 'rtc' && m.op === 'answer');
  assert.equal(ans.sdp, 'v=0 answer');
  await assert.rejects(v2.next(m => m.t === 'rtc', 400), /오지 않음/, '다른 화면에는 안 감');
  // 없는 휴대폰 → offline · 설치판에 등록한 휴대폰(id 다름)은 이름으로
  v2.send({ t: 'rtc', op: 'offer', phone: 'nobody99', viewer: 'viewB001', sdp: 'v=0' });
  assert.equal((await v2.next(m => m.t === 'rtc')).op, 'offline');
  v2.send({ t: 'rtc', op: 'offer', phone: 'desktopId9', label: '강단  휴대폰', viewer: 'viewB002', sdp: 'v=0' });
  assert.equal((await phone.next('offer')).viewer, 'viewB002');
  v1.send({ t: 'rtc', op: 'bye', phone: 'phoneAAA1', viewer: 'viewA001' });
  assert.equal((await phone.next('bye')).viewer, 'viewA001');
  // 같은 휴대폰이 다시 붙으면 앞의 연결은 닫힘
  const again = socket(`/rtc/phone?k=${k}`, { origin: null });
  await again.opened; await again.next('ready');
  again.send({ t: 'hello', id: 'phoneAAA1', name: '강단 휴대폰' });
  assert.equal((await phone.waitClose()).code, 4001);
  for (const s of [v1, v2, again]) await s.close();
  await bridge.close();
});

test('늦게 붙은 화면도 영상 위치 · 무대 타이머가 맞다 (캐시한 출력 자료의 시계를 앞으로)', async () => {
  const bridge = await fakeBridge();
  const t0 = Date.now();
  bridge.send({ t: 'render', client: 'main', data: { output: 'main', version: 1, hasLive: true, visible: true, slide: { kind: 'video', image: '/media/x.webm' }, template: {}, video: { key: 'k', playing: true, offset: 0, at: t0, now: t0 } } });
  bridge.send({ t: 'render', client: 'stage', data: { output: 'stage', version: 1, timer: { running: true, remaining: 60_000, now: t0, duration: 60_000 } } });
  await sleep(1500);
  const out = await login('output');
  const v = socket('/events?client=main', { cookie: out });
  await v.opened;
  const r = await v.next('render');
  assert.ok(r.data.video.now >= t0 + 1200, `now 가 받은 뒤 지난 만큼 앞으로 (${r.data.video.now - t0}ms)`);
  assert.equal(r.data.video.at, t0);
  const s = socket('/events?client=stage', { cookie: out });
  await s.opened;
  assert.ok((await s.next('render')).data.timer.now >= t0 + 1200);
  await v.close(); await s.close();
  await bridge.close();
});
