// 쉬운 로그인 단위 시험 (Cloudflare 없이 node): 길 · 세션 열쇠 · 기기 이름 · 돌아갈 주소 · 초대 열쇠 · 이 기기 기억 늘리기
import test from 'node:test';
import assert from 'node:assert/strict';
import { route } from '../src/protocol.mjs';
import { issueToken, readToken } from '../src/auth.mjs';
import { parseCred, sessionCredOf } from '../src/identity/members.mjs';
import { REMEMBER_MS, RENEW_AFTER_MS, loginRoute, maybeRenew, safeNext, uaName, validInviteToken } from '../src/identity/login.mjs';

const q = new URLSearchParams();
const env = { AUTH_SECRET: 'x'.repeat(40) };

test('주인 길: 로그인한 기기 · 초대 (manage)', () => {
  assert.deepEqual(route('GET', '/api/v1/sessions', q), { kind: 'identity', cap: 'manage', op: 'sessions' });
  assert.deepEqual(route('DELETE', '/api/v1/sessions/AbCdEf1234', q), { kind: 'identity', cap: 'manage', op: 'revokeSession' });
  assert.deepEqual(route('POST', '/api/v1/invites', q), { kind: 'identity', cap: 'manage', op: 'createInvite' });
  assert.equal(route('DELETE', '/api/v1/invites/x', q), null);
  assert.equal(loginRoute('PUT', '/api/v1/sessions'), null);
});

test('세션 열쇠 m:<사람>:<세대>:<세션> · 예전 열쇠도 그대로 읽힘', async () => {
  const cred = sessionCredOf({ id: 'Abcdef1234', gen: 3 }, 'Sess567890');
  assert.equal(cred, 'm:Abcdef1234:3:Sess567890');
  assert.deepEqual(parseCred(cred), { id: 'Abcdef1234', gen: 3, sid: 'Sess567890' });
  assert.deepEqual(parseCred('m:Abcdef1234:3'), { id: 'Abcdef1234', gen: 3 });
  assert.equal(parseCred('m:Abcdef1234:3:x'), null);
  const t = await issueToken(env, { church: 'sample', role: 'member-editor', cred }, 1000, REMEMBER_MS);
  assert.equal(t.exp, 1000 + REMEMBER_MS, '이 기기 기억 90일');
  assert.equal((await readToken(env, t.token, 2000)).cred, cred);
});

test('기기 이름 · 돌아갈 주소 · 초대 열쇠 모양', () => {
  assert.equal(uaName('Mozilla/5.0 (Linux; Android 14) AppleWebKit Chrome/130 Mobile Safari/537.36'), '크롬 · 안드로이드');
  assert.equal(uaName('Mozilla/5.0 (iPhone; CPU iPhone OS 18_0) AppleWebKit Version/18 Mobile Safari/604.1'), '사파리 · 아이폰 · 아이패드');
  assert.equal(uaName('Mozilla/5.0 (Windows NT 10.0) Chrome/130 Safari/537.36 Edg/130'), '엣지 · 윈도우');
  assert.equal(uaName(''), '브라우저');
  assert.equal(safeNext('/control/'), '/control/');
  for (const bad of ['//evil.example', 'https://evil.example', '/\\evil', 'control', '']) assert.equal(safeNext(bad), '', bad);
  assert.ok(validInviteToken('A'.repeat(32)) && !validInviteToken('short') && !validInviteToken('a-b'.repeat(12)));
});

test('이 기기 기억: 하루 넘게 쓴 세션만 90일로 다시 늘림 · 기억하지 않은 세션은 그대로', async () => {
  const calls = [];
  const db = remember => ({ prepare: sql => ({ bind: (...a) => ({ first: async () => (calls.push(['first', sql, a]), { remember }), run: async () => (calls.push(['run', sql, a]), {}) }) }) });
  const request = new Request('https://x.example/control/');
  const cred = 'm:Abcdef1234:3:Sess567890';
  const now = 10 * 86400_000;
  const fresh = { church: 'sample', role: 'owner', cred, exp: now + REMEMBER_MS - 1000 };
  assert.equal(await maybeRenew({ ...env, DB: db(1) }, request, fresh, now), null, '방금 받은 쿠키는 그대로 (쓰기 없음)');
  assert.equal(calls.length, 0);
  const old = { ...fresh, exp: now + REMEMBER_MS - RENEW_AFTER_MS - 1000 };
  const c = await maybeRenew({ ...env, DB: db(1) }, request, old, now);
  assert.match(c[0], /^wp_session=v1\..*Max-Age=7776000.*HttpOnly; Secure/);
  assert.ok(calls.some(([k, sql, a]) => k === 'run' && /expires_at/.test(sql) && a[2] === now + REMEMBER_MS));
  assert.equal(await maybeRenew({ ...env, DB: db(0) }, request, old, now), null, '기억하지 않은 세션');
  assert.equal(await maybeRenew({ ...env, DB: db(1) }, request, { ...old, cred: 'm:Abcdef1234:3' }, now), null, '세션 없는 예전 쿠키');
});
