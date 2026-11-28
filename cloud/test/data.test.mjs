// 2단계 A 단위 시험 (Cloudflare 없이 node 로): 길 · 권한 · If-Match · 쪽 표시 · 비밀번호 · Cloudflare Access JWT · 사람 계정 열쇠
import test from 'node:test';
import assert from 'node:assert/strict';
import { can, route, ROLES, SESSION_TTL } from '../src/protocol.mjs';
import { dataRoute, parseIfMatch, encodeCursor, decodeCursor, validDocId, validKind, validMutationId, titleOf, KINDS } from '../src/data/routes.mjs';
import { docJson, preparePayload, actorOf } from '../src/data/store.mjs';
import { hashPassword, checkPassword, weakPassword, newPassword } from '../src/identity/password.mjs';
import { verifyAccessJwt } from '../src/identity/access.mjs';
import { parseCred, credOf, SESSION_ROLE, validLogin, validEmail, view } from '../src/identity/members.mjs';
import { issueToken, readToken } from '../src/auth.mjs';

const q = s => new URLSearchParams(s);
const SECRET = 'x'.repeat(40);

test('2단계 길: 문서 · 동기화 · 성경 · 사람 계정 (1단계 route() 에 이어 붙음)', () => {
  assert.deepEqual(route('GET', '/api/v1/documents', q('')), { kind: 'data', cap: 'read', op: 'list' });
  assert.deepEqual(route('GET', '/api/v1/documents/cue/abc', q('')), { kind: 'data', cap: 'read', op: 'get' });
  assert.deepEqual(route('PUT', '/api/v1/documents/cue/abc', q('')), { kind: 'data', cap: 'write', op: 'put' });
  assert.deepEqual(route('DELETE', '/api/v1/documents/cue/abc', q('')), { kind: 'data', cap: 'write', op: 'delete' });
  assert.equal(route('POST', '/api/v1/documents/cue/abc', q('')), null);
  assert.equal(route('GET', '/api/v1/documents/cue/a/b', q('')), null);
  assert.deepEqual(route('GET', '/api/v1/sync', q('after=3')), { kind: 'data', cap: 'read', op: 'pull' });
  assert.deepEqual(route('POST', '/api/v1/sync', q('')), { kind: 'data', cap: 'write', op: 'push' });
  assert.equal(route('PUT', '/api/v1/sync', q('')), null);
  assert.equal(route('GET', '/api/v1/bible/versions', q('')).cap, 'read');
  assert.equal(route('GET', '/api/v1/bible/passage', q('')).op, 'passage');
  assert.equal(route('GET', '/api/v1/members', q('')).cap, 'manage');
  assert.equal(route('PATCH', '/api/v1/members/abcdefghij', q('')).op, 'update');
  assert.equal(route('DELETE', '/api/v1/members/abcdefghij', q('')).op, 'remove');
  assert.equal(route('DELETE', '/api/v1/members/a', q('')), null);
  assert.deepEqual(route('POST', '/api/v1/me/password', q('')), { kind: 'identity', cap: null, op: 'password' });
  // 1단계 길은 그대로
  assert.deepEqual(route('GET', '/api/v1/me', q('')), { kind: 'api', cap: null });
  assert.equal(route('DELETE', '/api/v1/credentials/abcdefghij', q('')).kind, 'api');
  assert.equal(dataRoute('GET', '/api/v1/room'), null);
});

test('권한: 자료 읽기 · 쓰기는 주인 · 설치판 · 사람 편집자, 읽기만은 보기 계정. 출력 · 리모컨 · PIN 편집기는 못 함', () => {
  for (const r of ['owner', 'bridge', 'member-editor']) assert.ok(can(r, 'read') && can(r, 'write'), r);
  assert.ok(can('member-viewer', 'read') && !can('member-viewer', 'write'));
  for (const r of ['output', 'remote', 'editor']) assert.ok(!can(r, 'read') && !can(r, 'write'), r);
  assert.ok(!can('member-editor', 'manage') && !can('member-editor', 'source'));
  assert.ok(can('member-editor', 'view') && can('member-editor', 'editor') && can('member-editor', 'remote'));
  assert.ok(can('member-viewer', 'view') && !can('member-viewer', 'remote') && !can('member-viewer', 'editor'));
  assert.ok(!can('bridge', 'manage') && !can('bridge', 'view'));
  for (const r of Object.values(SESSION_ROLE)) { assert.ok(ROLES.includes(r)); assert.ok(SESSION_TTL[r] > 0); }
});

test('사람 계정 쿠키: 역할 · 열쇠 칸(m:id:세대)이 서명 안에 들어가고 바꿔 끼우면 깨진다', async () => {
  const env = { AUTH_SECRET: SECRET, DB: {} };
  const m = { id: 'abcdefghij', gen: 3 };
  assert.equal(credOf(m), 'm:abcdefghij:3');
  assert.deepEqual(parseCred('m:abcdefghij:3'), m);
  assert.equal(parseCred('abcdefghij'), null);
  assert.equal(parseCred('m:x:1'), null);
  const { token } = await issueToken(env, { church: 'sample', role: 'member-editor', cred: credOf(m) });
  assert.deepEqual((await readToken(env, token)).cred, 'm:abcdefghij:3');
  assert.equal((await readToken(env, token)).role, 'member-editor');
  const [v, body, sig] = token.split('.');
  const forged = JSON.parse(Buffer.from(body, 'base64url').toString());
  forged.r = 'owner';
  assert.equal(await readToken(env, `${v}.${Buffer.from(JSON.stringify(forged)).toString('base64url')}.${sig}`), null);
  assert.ok(validLogin('kim') && validLogin('kim.sarang') && validLogin('a@b.kr'));
  for (const bad of ['ab', 'Kim', '-kim', 'kim-', '김', 'a b', 'x'.repeat(65)]) assert.ok(!validLogin(bad), bad);
  assert.ok(validEmail('kim@example.org') && !validEmail('kim@') && !validEmail('a b@c.de'));
  assert.deepEqual(view({ id: 'i1', subject: 'pw:kim', role: 'editor', label: '김', created_at: 1, last_login_at: null, secret_hash: 'p1$…' }),
    { id: 'i1', role: 'editor', label: '김', provider: 'password', login: 'kim', createdAt: 1, lastLoginAt: null });
  assert.equal(view({ id: 'i2', subject: 'access:a@b.kr', role: 'viewer', label: '', created_at: 1 }).email, 'a@b.kr');
  assert.equal(actorOf({ cred: 'm:abcdefghij:3' }), 'm:abcdefghij');
  assert.equal(actorOf({ cred: 'Xy12345678' }), 'Xy12345678');
});

test('If-Match · 쪽 표시 · 문서 이름 · mutationId 모양', () => {
  assert.equal(parseIfMatch(null), 0);
  assert.equal(parseIfMatch(''), 0);
  assert.equal(parseIfMatch('3'), 3);
  assert.equal(parseIfMatch('"12"'), 12);
  assert.equal(parseIfMatch('W/"7"'), 7);
  for (const bad of ['abc', '*', '-1', '1.5', '"1', '1,2']) assert.equal(parseIfMatch(bad), bad === '"1' ? 1 : null, bad);
  const c = encodeCursor(['cue', '한글-id']);
  assert.deepEqual(decodeCursor(c), ['cue', '한글-id']);
  assert.equal(decodeCursor(''), null);
  assert.equal(decodeCursor('@@@'), undefined);
  assert.equal(decodeCursor(encodeCursor([1, 2])), undefined);
  for (const ok of ['abc', 'cue1abcd123', 'tpl-theme-main', '550e8400-e29b-41d4-a716-446655440000', 'a:b', '_x']) assert.ok(validDocId(ok), ok);
  for (const bad of ['', '-a', '.', '..', 'a/b', 'a b', '한글', 'x'.repeat(129)]) assert.ok(!validDocId(bad), bad);
  assert.equal(KINDS.length, 10);
  assert.ok(validKind('settings') && !validKind('Cue'));
  assert.ok(validMutationId('550e8400-e29b-41d4') && !validMutationId('short') && !validMutationId('a b c d e f'));
});

test('문서 본문: JSON 객체만 · 크기 한도 · 목록 이름 · 답 모양 (본문 글을 다시 읽지 않고 이어 붙임)', () => {
  assert.equal(preparePayload([1], 100).status, 400);
  assert.equal(preparePayload('x', 100).status, 400);
  assert.equal(preparePayload(null, 100).status, 400);
  assert.equal(preparePayload({ a: 'x'.repeat(200) }, 100).status, 413);
  assert.equal(preparePayload({ a: '가'.repeat(40) }, 100).status, 413, 'UTF-8 바이트로 셈');
  assert.deepEqual(preparePayload({ name: '주일' }, 100), { text: '{"name":"주일"}', size: 17, title: '주일' });
  assert.equal(titleOf({ title: '찬양' }), '찬양');
  assert.equal(titleOf({ name: 3 }), '');
  const row = { kind: 'cue', id: 'a', revision: 2, updatedAt: 5, updatedBy: 'k', deletedAt: null, payload: '{"x":[1]}' };
  assert.deepEqual(JSON.parse(docJson(row)), { kind: 'cue', id: 'a', revision: 2, updatedAt: 5, updatedBy: 'k', deleted: false, payload: { x: [1] } });
  assert.deepEqual(JSON.parse(docJson({ ...row, deletedAt: 9, payload: '' })).deleted, true);
  assert.equal(JSON.parse(docJson({ ...row, deletedAt: 9, payload: '' })).payload, undefined);
});

test('비밀번호: 소금 · AUTH_SECRET 을 섞은 PBKDF2 · 교회 · 계정이 다르면 맞지 않음 · 쉬운 비밀번호 막기', async () => {
  const h = await hashPassword(SECRET, 'sample', 'pw:kim', 'Sarang-Daegu-2026');
  assert.match(h, /^p1\$20000\$[A-Za-z0-9_-]{22}\$[A-Za-z0-9_-]{43}$/);
  assert.notEqual(h, await hashPassword(SECRET, 'sample', 'pw:kim', 'Sarang-Daegu-2026'), '소금이 매번 다름');
  assert.ok(await checkPassword(SECRET, 'sample', 'pw:kim', 'Sarang-Daegu-2026', h));
  assert.ok(!(await checkPassword(SECRET, 'sample', 'pw:kim', 'Sarang-Daegu-2027', h)));
  assert.ok(!(await checkPassword(SECRET, 'other', 'pw:kim', 'Sarang-Daegu-2026', h)), '다른 교회');
  assert.ok(!(await checkPassword(SECRET, 'sample', 'pw:lee', 'Sarang-Daegu-2026', h)), '다른 계정');
  assert.ok(!(await checkPassword('y'.repeat(40), 'sample', 'pw:kim', 'Sarang-Daegu-2026', h)), 'AUTH_SECRET 이 다르면');
  assert.ok(!(await checkPassword(SECRET, 'sample', 'pw:kim', 'x', 'p1$999999999$a$b')), '반복 수 한도');
  assert.ok(!(await checkPassword(SECRET, 'sample', 'pw:kim', 'x', null)));
  assert.ok(weakPassword('short', 'kim'));
  assert.ok(weakPassword('aaaaaaaaaaaa', 'kim'));
  assert.ok(weakPassword('kim-sarang-123', 'kim'));
  assert.equal(weakPassword('Sarang-Daegu-2026', 'kim'), '');
  const p = newPassword();
  assert.equal(p.length, 16);
  assert.ok(!/[0O1lI]/.test(p));
});

test('Cloudflare Access JWT: 서명 · 대상(aud) · 발급처 · 만료 · 열쇠 id 를 모두 본다', async () => {
  const team = 'myteam.cloudflareaccess.com', aud = 'a'.repeat(32);
  const kp = await crypto.subtle.generateKey({ name: 'RSASSA-PKCS1-v1_5', modulusLength: 2048, publicExponent: new Uint8Array([1, 0, 1]), hash: 'SHA-256' }, true, ['sign', 'verify']);
  const jwk = { ...(await crypto.subtle.exportKey('jwk', kp.publicKey)), kid: 'k1' };
  const b64 = o => Buffer.from(JSON.stringify(o)).toString('base64url');
  const now = Math.floor(Date.now() / 1000);
  async function sign(body, head = { alg: 'RS256', kid: 'k1' }, key = kp.privateKey) {
    const data = `${b64(head)}.${b64(body)}`;
    const sig = await crypto.subtle.sign('RSASSA-PKCS1-v1_5', key, new TextEncoder().encode(data));
    return `${data}.${Buffer.from(sig).toString('base64url')}`;
  }
  const good = { aud: [aud], iss: `https://${team}`, exp: now + 600, iat: now, email: 'Kim@Example.org', sub: 'u1' };
  const opts = { team, aud, keys: [jwk] };
  assert.deepEqual(await verifyAccessJwt(await sign(good), opts), { email: 'kim@example.org', sub: 'u1' });
  assert.equal(await verifyAccessJwt(await sign({ ...good, aud: ['other'] }), opts), null, 'aud');
  assert.equal(await verifyAccessJwt(await sign({ ...good, iss: 'https://evil.cloudflareaccess.com' }), opts), null, 'iss');
  assert.equal(await verifyAccessJwt(await sign({ ...good, exp: now - 1 }), opts), null, '만료');
  assert.equal(await verifyAccessJwt(await sign({ ...good, email: undefined }), opts), null, 'email 없음');
  assert.equal(await verifyAccessJwt(await sign(good, { alg: 'RS256', kid: 'k2' }), opts), null, '모르는 열쇠');
  assert.equal(await verifyAccessJwt(await sign(good, { alg: 'HS256', kid: 'k1' }), opts), null, '다른 알고리즘');
  const other = await crypto.subtle.generateKey({ name: 'RSASSA-PKCS1-v1_5', modulusLength: 2048, publicExponent: new Uint8Array([1, 0, 1]), hash: 'SHA-256' }, true, ['sign']);
  assert.equal(await verifyAccessJwt(await sign(good, undefined, other.privateKey), opts), null, '다른 열쇠로 서명');
  const t = await sign(good);
  const [h, , s] = t.split('.');
  assert.equal(await verifyAccessJwt(`${h}.${b64({ ...good, email: 'boss@example.org' })}.${s}`, opts), null, '본문 바꿔치기');
  assert.equal(await verifyAccessJwt('a.b', opts), null);
  // 공개 열쇠는 팀 주소에서 받아 기억한다
  let calls = 0;
  const fetcher = async url => { calls++; assert.equal(url, `https://${team}/cdn-cgi/access/certs`); return new Response(JSON.stringify({ keys: [jwk] })); };
  assert.ok(await verifyAccessJwt(t, { team, aud, fetcher }));
  assert.ok(await verifyAccessJwt(t, { team, aud, fetcher }));
  assert.equal(calls, 1);
});
