// 3단계: 송출 주체 겹침 — 웹 조작 화면(주인 쿠키)이 예배 중인 설치판을 몰래 밀어내지 않는다.
// 설치판이 주체면 웹은 '대기'(보기 전용)로 붙고, 주인이 넘겨받기(claim)를 눌러야만 바뀐다. (architecture.md 4 · 5절)
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { fakeBridge, login, req, socket } from './helpers.mjs';

const render = (box, tag) => box.send({ t: 'render', client: 'main', data: { version: tag, tag } });
const gotRender = (viewer, tag, ms = 3000) => viewer.next(m => m && m.t === 'render' && m.data?.tag === tag, ms);
async function noRender(viewer, tag, ms = 800) {
  await assert.rejects(gotRender(viewer, tag, ms), /오지 않음/, `대기 중인 웹의 화면(${tag})이 출력에 가면 안 됨`);
}
async function webSource(cookie) {
  const w = socket('/bridge', { cookie });
  await w.opened;
  const ready = await w.next('ready');
  w.send({ t: 'hello', v: 1, app: '웹 조작 화면', lang: 'ko', outputs: ['main'], caps: {}, source: 'web' });
  return { w, ready };
}

test('설치판이 송출 중이면 웹은 대기로 붙고, 넘겨받기 전에는 출력에 닿지 않는다', async () => {
  const owner = await login('owner');
  const output = await login('output');
  const desk = await fakeBridge();
  assert.equal(desk.ready.standby, false);
  const viewer = socket('/events?client=main', { cookie: output });
  await viewer.opened;
  await viewer.next('hello');
  render(desk, 'd1');
  await gotRender(viewer, 'd1');

  // 웹이 붙는다 → 대기
  const { w, ready } = await webSource(owner);
  assert.equal(ready.standby, true);
  assert.equal(ready.active.source, 'desktop');
  render(w, 'w1');
  await noRender(viewer, 'w1');
  assert.equal(desk.closed, null, '설치판이 끊기면 안 됨');
  render(desk, 'd2');
  await gotRender(viewer, 'd2');

  // 방 상태: 주체는 설치판, 대기에 웹
  let st = (await req('/api/v1/room', { cookie: owner })).json;
  assert.equal(st.source, 'desktop');
  assert.equal(st.standby.length, 1);
  assert.equal(st.standby[0].source, 'web');

  // 주인이 넘겨받기 → 설치판 4001, 웹이 주체 · 전부 다시 달라는 sync
  w.send({ t: 'claim' });
  const act = await w.next('active');
  assert.equal(act.claimed, true);
  await w.next('sync');
  const c = await desk.waitClose();
  assert.equal(c.code, 4001);
  render(w, 'w2');
  await gotRender(viewer, 'w2');

  // 설치판이 다시 붙어도(자동 재연결) 넘겨받은 동안은 대기 — 웹을 밀어내지 않는다
  const desk2 = await fakeBridge();
  assert.equal(desk2.ready.standby, true);
  render(desk2, 'd3');
  await noRender(viewer, 'd3');
  render(w, 'w3');
  await gotRender(viewer, 'w3');
  st = (await req('/api/v1/room', { cookie: owner })).json;
  assert.equal(st.source, 'web');
  assert.equal(st.claimed, true);

  // 돌려주기 → 설치판이 주체, 웹은 대기
  w.send({ t: 'release' });
  await w.next('standby');
  await desk2.next('active');
  await desk2.next('sync');
  render(desk2, 'd4');
  await gotRender(viewer, 'd4');
  render(w, 'w4');
  await noRender(viewer, 'w4');

  // 다시 넘겨받았다가 웹 창을 닫으면 대기 중인 설치판이 저절로 주체
  w.send({ t: 'claim' });
  await w.next('active');
  const c2 = await desk2.waitClose();
  assert.equal(c2.code, 4001);
  const desk3 = await fakeBridge();
  assert.equal(desk3.ready.standby, true);
  await w.close();
  await desk3.next('active', 5000);
  render(desk3, 'd5');
  await gotRender(viewer, 'd5');

  await Promise.all([viewer.close(), desk3.close()]);
});

test('설치판이 없으면 웹이 바로 주체, 설치판이 붙으면 웹은 대기로 내려간다 (끊지 않음)', async () => {
  const owner = await login('owner');
  const output = await login('output');
  // 앞 시험의 연결이 남아 있지 않게 잠깐
  const { w, ready } = await webSource(owner);
  if (ready.standby) { w.send({ t: 'claim' }); await w.next('active'); }
  const viewer = socket('/events?client=main', { cookie: output });
  await viewer.opened;
  render(w, 'w10');
  await gotRender(viewer, 'w10');
  const desk = await fakeBridge();
  // 넘겨받은 상태(claim)였으면 설치판이 대기, 아니면 웹이 대기 — 어느 쪽도 끊기지 않는다
  if (desk.ready.standby) {
    render(desk, 'd10');
    await noRender(viewer, 'd10');
    w.send({ t: 'release' });
    await w.next('standby');
  } else {
    await w.next('standby');
  }
  assert.equal(w.closed, null, '웹 조작 화면이 끊기면 안 됨');
  render(desk, 'd11');
  await gotRender(viewer, 'd11');
  await Promise.all([viewer.close(), desk.close(), w.close()]);
});

test('주인이 아닌 로그인은 송출 주체로 붙지 못한다 (넘겨받기도 못 함)', async () => {
  const output = await login('output');
  const s = socket('/bridge', { cookie: output });
  await assert.rejects(s.opened, e => e.status === 401 || e.status === 403);
});
