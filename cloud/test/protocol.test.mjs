// 계약(src/protocol.mjs) · 로그인 쿠키(src/auth.mjs) 단위 시험 — Cloudflare 없이 node 로
import test from 'node:test';
import assert from 'node:assert/strict';
import { PROTOCOL, can, clientName, outputPage, mediaPath, route, validChurch, packChunk, unpackChunk, VIEW_COMMANDS } from '../src/protocol.mjs';
import { issueToken, readToken, splitKey, newSecret, safeEqual, credHash, configured } from '../src/auth.mjs';

const q = s => new URLSearchParams(s);

test('계약 버전 1', () => assert.equal(PROTOCOL, 1));

test('역할별 권한: 출력은 보기만, 설치판은 송출만, 주인은 모두', () => {
  assert.ok(can('owner', 'manage') && can('owner', 'view') && can('owner', 'remote') && can('owner', 'editor') && can('owner', 'source'));
  assert.ok(can('output', 'view') && !can('output', 'remote') && !can('output', 'editor') && !can('output', 'source'));
  assert.ok(can('remote', 'remote') && !can('remote', 'view') && !can('remote', 'editor'));
  assert.ok(can('editor', 'editor') && !can('editor', 'remote'));
  assert.ok(can('bridge', 'source') && !can('bridge', 'view'));
  assert.ok(!can('nobody', 'view'));
});

test('교회 ID · 화면 이름 · 출력 주소 모양 (설치판 /events?client= 와 같음)', () => {
  for (const ok of ['sample', 'daegu-sarang', 'a1']) assert.ok(validChurch(ok), ok);
  for (const bad of ['', 'A', 'a', '-a', 'a-', 'a_b', 'a.b', '한글', 'x'.repeat(41), null]) assert.ok(!validChurch(bad), String(bad));
  for (const c of ['main', 'overlay', 'stage', 'remote', 'out-1', 'out-abc_DEF-9']) assert.equal(clientName(c), c);
  for (const c of ['', 'MAIN', 'out-', 'out-a/b', 'out-' + 'x'.repeat(37), 'x']) assert.equal(clientName(c), null, c);
  assert.equal(outputPage('/outputs/main'), 'main');
  assert.equal(outputPage('/outputs/stage/'), 'stage');
  assert.equal(outputPage('/outputs/out-xy'), 'out-xy');
  assert.equal(outputPage('/outputs/output.js'), null);
  assert.equal(outputPage('/outputs/remote'), null);
});

test('미디어 길은 설치판 내부망 출력(lan.rs allowed)과 같은 범위', () => {
  for (const p of ['/media/a.png', '/lib/file', '/hymn/k/1.svg', '/hymnal/score/b/1.png']) assert.ok(mediaPath(p), p);
  for (const p of ['/api/state', '/api/live', '/lib', '/hymnal/x', '/medias/a']) assert.ok(!mediaPath(p), p);
});

test('길 나누기: 모르는 길 · 설치판 전용 길은 열지 않는다', () => {
  assert.deepEqual(route('GET', '/outputs/main', q('')), { kind: 'page', cap: 'view', page: 'output' });
  assert.deepEqual(route('GET', '/outputs/stage', q('')), { kind: 'page', cap: 'view', page: 'stage' });
  assert.equal(route('GET', '/remote', q('')).cap, 'remote');
  assert.equal(route('GET', '/editor', q('')).cap, 'editor');
  assert.equal(route('GET', '/owner', q('')).cap, 'manage');
  assert.deepEqual(route('GET', '/events', q('client=remote')), { kind: 'events', cap: 'remote', client: 'remote' });
  assert.equal(route('GET', '/events', q('client=stage')).cap, 'view');
  assert.equal(route('GET', '/events', q('')).client, 'main');
  assert.equal(route('GET', '/events', q('client=../x')), null);
  assert.equal(route('POST', '/r/do', q('')).kind, 'relay');
  assert.equal(route('GET', '/r/do', q('')), null);
  assert.equal(route('POST', '/e/save', q('')).cap, 'editor');
  assert.equal(route('GET', '/e/save', q('')), null);
  assert.equal(route('GET', '/bridge', q('')).cap, 'source');
  // 설치판 로컬 서버에만 있는 길: 클라우드에서는 열지 않는다
  for (const [m, p] of [['GET', '/api/state'], ['GET', '/api/live'], ['GET', '/dock'], ['POST', '/api/trigger/next'], ['GET', '/helper/cameras'], ['GET', '/phonecam'], ['PUT', '/r/do'], ['GET', '/api/v1/admin/churches']]) {
    assert.equal(route(m, p, q('')), null, `${m} ${p}`);
  }
  assert.deepEqual(VIEW_COMMANDS, ['videoMeta', 'videoEnded']);
});

test('이진 조각 머리: 요청 번호 4바이트 + 표시 1바이트', () => {
  const c = unpackChunk(packChunk(0x01020304, 1, new Uint8Array([9, 8, 7])));
  assert.deepEqual([c.id, c.flags, [...c.bytes]], [0x01020304, 1, [9, 8, 7]]);
  assert.equal(unpackChunk(new Uint8Array(4)), null);
});

const env = { AUTH_SECRET: 'x'.repeat(40), DB: {} };

test('로그인 쿠키: 서명 · 만료 · 고친 것 · 설치판 역할은 쿠키가 될 수 없음', async () => {
  const { token, exp } = await issueToken(env, { church: 'sample', role: 'output', cred: 'abc' }, 1000);
  assert.ok(exp > 1000);
  assert.deepEqual(await readToken(env, token, 2000), { church: 'sample', role: 'output', cred: 'abc', exp });
  assert.equal(await readToken(env, token, exp + 1), null, '만료');
  assert.equal(await readToken({ ...env, AUTH_SECRET: 'y'.repeat(40) }, token, 2000), null, '다른 비밀값');
  const [v, body, sig] = token.split('.');
  const forged = Buffer.from(JSON.stringify({ c: 'other', r: 'owner', k: 'abc', e: exp })).toString('base64url');
  assert.equal(await readToken(env, `${v}.${forged}.${sig}`, 2000), null, '내용을 바꾸면');
  assert.equal(await readToken(env, `${v}.${body}`, 2000), null);
  const br = await issueToken(env, { church: 'sample', role: 'bridge', cred: 'abc' }, 1000);
  assert.equal(await readToken(env, br.token, 2000), null);
  assert.equal(await readToken(env, 'x'.repeat(700), 2000), null);
});

test('열쇠: 교회.열쇠 나누기 · PIN 6자리 · 긴 열쇠 · 해시는 역할마다 다름', async () => {
  assert.deepEqual(splitKey('sample.AbC9'), { church: 'sample', secret: 'AbC9' });
  assert.equal(splitKey('Sample.x'), null);
  assert.equal(splitKey('.x'), null);
  assert.equal(splitKey('sample.'), null);
  for (let i = 0; i < 50; i++) assert.match(newSecret('remote'), /^[1-9]\d{5}$/);
  assert.match(newSecret('output'), /^[A-HJ-NP-Za-km-z2-9]{32}$/);
  assert.notEqual(await credHash(env, 'sample', 'remote', '123456'), await credHash(env, 'sample', 'editor', '123456'));
  assert.notEqual(await credHash(env, 'a1', 'remote', '123456'), await credHash(env, 'b1', 'remote', '123456'));
  assert.ok(safeEqual('abc', 'abc') && !safeEqual('abc', 'abd') && !safeEqual('abc', 'ab'));
  assert.equal(configured({ AUTH_SECRET: 'short', DB: {} }), false, '비밀값이 짧으면 로그인 전체를 닫는다');
  assert.equal(configured({ AUTH_SECRET: 'x'.repeat(40) }), false, 'D1 이 없으면 닫는다');
});
