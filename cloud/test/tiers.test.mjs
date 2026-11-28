// 단위 (7단계 등급 · 드라이브 형식): 한도표 · 등급 문 · 드라이브 오류 변환 · 문서 봉투 · 성경 책 파일 · OAuth state · 가짜 구글 multipart
import test from 'node:test';
import assert from 'node:assert/strict';
import { TIER_LIMITS, TIERS, STORE, gate, limitsOf, serviceLimits } from '../src/tiers/limits.mjs';
import { ERROR_CODES, DriveError, driveErrorOf, envelopeText, parseEnvelope, bookFileText, splitBook, bookChapter, bookHeadings, checkBook, docFileName, multipartBody, cleanName } from '../src/drive/format.mjs';
import { endpoints, readIdToken } from '../src/drive/google.mjs';
import { packState, unpackState } from '../src/drive/oauth.mjs';
import { route, can } from '../src/protocol.mjs';
import { isWrite } from '../src/tiers/index.mjs';

test('한도표: 등급 셋 · 같은 칸 · 저장소', () => {
  assert.deepEqual(TIERS, ['demo', 'free', 'paid']);
  assert.deepEqual(STORE, { demo: 'demo', free: 'drive', paid: 'd1' });
  const keys = Object.keys(TIER_LIMITS.paid).sort();
  for (const t of TIERS) assert.deepEqual(Object.keys(TIER_LIMITS[t]).sort(), keys, t);
  assert.equal(TIER_LIMITS.free.multipart, false);
  assert.equal(TIER_LIMITS.demo.fileMax, 0);
  assert.ok(TIER_LIMITS.free.pullPayloads <= 45, '드라이브는 한 요청 하위 요청 50개 안에서');
  // 덮어쓰기는 env 로만, 칸 단위
  assert.equal(limitsOf('demo', { TIER_OVERRIDES: '{"demo":{"perIpPerMinute":5}}' }).perIpPerMinute, 5);
  assert.equal(limitsOf('demo', { TIER_OVERRIDES: '{"demo":{"perIpPerMinute":5}}' }).documents, TIER_LIMITS.demo.documents);
  assert.equal(limitsOf('demo', { TIER_OVERRIDES: 'not json' }).perIpPerMinute, TIER_LIMITS.demo.perIpPerMinute);
  assert.equal(limitsOf('nope'), TIER_LIMITS.paid);
  assert.equal(serviceLimits({ SERVICE_OVERRIDES: '{"freeSignupsPerDay":3}' }).freeSignupsPerDay, 3);
});

test('등급 문: 데모는 파일 · AI · 초대 막음, 무료는 나눠 올리기 · 클라우드 음성 인식 막음, 유료는 다 됨', () => {
  const A = (op, kind = 'assets') => ({ kind, op });
  assert.equal(gate('demo', A('upload')).code, 'tier_blocked');
  assert.equal(gate('demo', A('get')), null, '데모도 받기는 됨');
  assert.equal(gate('demo', A('list')), null);
  assert.equal(gate('demo', A('chat', 'ai')).limit, 'ai');
  assert.equal(gate('demo', A('create', 'identity')).limit, 'members');
  assert.equal(gate('free', A('upload')), null);
  assert.equal(gate('free', A('start')).limit, 'multipart');
  assert.equal(gate('free', A('transcribe', 'ai')).limit, 'ai');
  assert.equal(gate('free', A('chat', 'ai')), null, '무료는 교회 키로 대신 부르기만');
  for (const op of ['upload', 'start', 'part', 'complete', 'purge']) assert.equal(gate('paid', A(op)), null, op);
  assert.equal(gate('paid', A('transcribe', 'ai')), null);
});

test('쓰기 판단 (옮기는 동안 423) · 새 길 · 데모 역할 권한', () => {
  assert.equal(isWrite({ kind: 'data', op: 'put' }), true);
  assert.equal(isWrite({ kind: 'data', op: 'pull' }), false);
  assert.equal(isWrite({ kind: 'data', op: 'importBook' }), true);
  assert.equal(isWrite({ kind: 'assets', op: 'get' }), false);
  assert.equal(isWrite({ kind: 'assets', op: 'remove' }), true);
  const q = new URLSearchParams();
  assert.deepEqual(route('GET', '/api/v1/tier', q), { kind: 'tiers', cap: null, op: 'tier' });
  assert.deepEqual(route('POST', '/api/v1/drive/disconnect', q), { kind: 'tiers', cap: 'manage', op: 'disconnect' });
  assert.deepEqual(route('PUT', '/api/v1/bible/versions/KRV', q), { kind: 'data', cap: 'manage', op: 'importVersion' });
  assert.deepEqual(route('PUT', '/api/v1/bible/versions/KRV/books/43', q), { kind: 'data', cap: 'manage', op: 'importBook' });
  assert.deepEqual(route('DELETE', '/api/v1/bible/versions/KRV', q), { kind: 'data', cap: 'manage', op: 'deleteVersion' });
  assert.equal(route('POST', '/api/v1/bible/versions/KRV', q), null);
  for (const c of ['view', 'read', 'write', 'source', 'remote', 'editor']) assert.equal(can('demo', c), true, c);
  for (const c of ['manage', 'ai']) assert.equal(can('demo', c), false, c);
  assert.equal(can('bridge', 'manage'), false, '설치판 열쇠로 성경 넣기 못 함');
});

test('드라이브 오류 → 코드 · 상태 (구글 오류 문서의 까닭)', () => {
  const e = (status, reason) => driveErrorOf(status, { error: { errors: [{ reason }] } });
  assert.equal(e(403, 'storageQuotaExceeded').code, 'drive_quota');
  assert.equal(e(403, 'storageQuotaExceeded').status, 507);
  assert.equal(e(403, 'userRateLimitExceeded').code, 'drive_rate');
  assert.equal(e(429, 'rateLimitExceeded').status, 429);
  assert.equal(e(401, 'authError').code, 'drive_disconnected');
  assert.equal(e(401, 'authError').status, 503);
  assert.equal(e(404, 'notFound').code, 'drive_missing');
  assert.equal(e(403, 'insufficientPermissions').code, 'drive_disconnected');
  assert.equal(e(500, 'backendError').code, 'drive_unavailable');
  assert.ok(new DriveError('drive_busy', 'x').status === 409);
  for (const [k, v] of Object.entries(ERROR_CODES)) assert.ok(v >= 400 && v < 600, k);
});

test('문서 봉투: 사람이 읽는 JSON, 판 · 지움, 깨진 것은 null', () => {
  const t = envelopeText({ kind: 'cue', id: 'c1', revision: 3, updatedAt: 5, updatedBy: 'x', payloadText: '{"name":"주일"}' });
  assert.deepEqual(JSON.parse(t), { worshipPresenter: 1, kind: 'cue', id: 'c1', revision: 3, updatedAt: 5, updatedBy: 'x', deleted: false, payload: { name: '주일' } });
  assert.equal(parseEnvelope(t).payloadText, '{"name":"주일"}');
  const d = parseEnvelope(envelopeText({ kind: 'cue', id: 'c1', revision: 4, updatedAt: 6, updatedBy: 'x', deleted: true }));
  assert.equal(d.deleted, true);
  assert.equal(d.payloadText, null);
  assert.equal(parseEnvelope('{"kind":"cue"}'), null);
  assert.equal(parseEnvelope('not json'), null);
  assert.equal(parseEnvelope(JSON.stringify({ worshipPresenter: 1, kind: 'cue', id: 'a', revision: 1, payload: [1] })), null);
  assert.equal(docFileName('cue', 'c1', '주일/예배: 1부'), '주일 예배 1부 (c1).json');
  assert.equal(docFileName('cue', 'c1', ''), 'c1.json');
  assert.equal(cleanName('a\u0000b\\c'), 'a b c');
});

test('성경 책 파일: 장마다 한 줄 · 전체가 JSON · 부른 장만 읽기 · 넣기 확인', () => {
  const chapters = [[[1, '태초에 "따옴표"\n줄바꿈'], [2, '둘']], [], [[1, '셋째 장']]];
  const text = bookFileText({ version: 'V', book: 43, name: '요한복음', chapters, headings: [[3, 1, '소제목']] });
  const all = JSON.parse(text);
  assert.deepEqual(all.chapters, chapters);
  assert.equal(all.chapterCount, 3);
  assert.equal(text.split('\n').length, 5);
  const sp = splitBook(text);
  assert.equal(sp.head.name, '요한복음');
  assert.deepEqual(bookChapter(sp, 1), chapters[0]);
  assert.deepEqual(bookChapter(sp, 2), []);
  assert.deepEqual(bookChapter(sp, 3), chapters[2]);
  assert.deepEqual(bookChapter(sp, 4), []);
  assert.deepEqual(bookHeadings(sp), [[3, 1, '소제목']]);
  assert.equal(splitBook('{}'), null);
  const c = checkBook({ chapters: [[[1, 'a'], [3, 'c']], []], headings: [[1, 1, 'h'], ['x']] });
  assert.equal(c.ok, true);
  assert.deepEqual(c.counts, [3, 0]);
  assert.equal(c.verses, 2);
  assert.deepEqual(c.headings, [[1, 1, 'h']]);
  assert.ok(checkBook({ chapters: [[[0, 'a']]] }).error);
  assert.ok(checkBook({ chapters: [] }).error);
  assert.ok(checkBook({ chapters: [[['1', 'a']]] }).error);
});

test('multipart/related 본문 모양 (구글 Drive 한 번에 올리기)', () => {
  const m = multipartBody({ name: 'a.json' }, '{"x":1}');
  const b = /boundary=(\S+)/.exec(m.contentType)[1];
  assert.ok(m.body.startsWith(`--${b}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{"name":"a.json"}\r\n--${b}\r\n`));
  assert.ok(m.body.endsWith(`{"x":1}\r\n--${b}--`));
});

test('구글 주소: 가짜는 로컬 주소만, id_token 은 우리 client 것만', () => {
  assert.equal(endpoints({}).token, 'https://oauth2.googleapis.com/token');
  assert.equal(endpoints({ GOOGLE_FAKE: 'http://127.0.0.1:9999' }).token, 'http://127.0.0.1:9999/token');
  assert.equal(endpoints({ GOOGLE_FAKE: 'https://evil.example' }).token, 'https://oauth2.googleapis.com/token', '밖 주소는 무시');
  assert.equal(endpoints({ GOOGLE_FAKE: 'http://127.0.0.1.evil.example' }).fake, false);
  const b = o => Buffer.from(JSON.stringify(o)).toString('base64url');
  const env = { GOOGLE_CLIENT_ID: 'cid' };
  const now = Date.now();
  const tok = p => `${b({ alg: 'RS256' })}.${b({ iss: 'https://accounts.google.com', aud: 'cid', sub: '123', email: 'A@x.org', email_verified: true, exp: Math.floor(now / 1000) + 600, ...p })}.sig`;
  assert.deepEqual(readIdToken(env, tok({}), now), { sub: '123', email: 'a@x.org', emailVerified: true });
  assert.equal(readIdToken(env, tok({ aud: 'other' }), now), null);
  assert.equal(readIdToken(env, tok({ iss: 'https://evil' }), now), null);
  assert.equal(readIdToken(env, tok({ exp: Math.floor(now / 1000) - 3600 }), now), null);
  assert.equal(readIdToken(env, 'x.y', now), null);
});

test('OAuth state 쿠키: 서명 · 만료 · 고친 것 거절', async () => {
  const env = { AUTH_SECRET: 'x'.repeat(40) };
  const s = await packState(env, { m: 'signup', s: 'abc', v: 'ver', e: Date.now() + 60_000 });
  assert.equal((await unpackState(env, s)).s, 'abc');
  const [v, body, sig] = s.split('.');
  const forged = Buffer.from(JSON.stringify({ m: 'connect', c: 'other', s: 'abc', v: 'ver', e: Date.now() + 60_000 })).toString('base64url');
  assert.equal(await unpackState(env, `${v}.${forged}.${sig}`), null);
  assert.equal(await unpackState({ AUTH_SECRET: 'y'.repeat(40) }, s), null);
  const old = await packState(env, { s: 'a', e: Date.now() - 1 });
  assert.equal(await unpackState(env, old), null);
  assert.equal(body.length > 0, true);
});

test('둘러보기는 DEMO_ENABLED=1 일 때만 (기본 꺼짐)', async () => {
  const { demoEnabled, demoAlive, enter } = await import('../src/tiers/demo.mjs');
  assert.equal(demoEnabled({}), false);
  assert.equal(demoEnabled({ DEMO_ENABLED: 'true' }), false);
  assert.equal(demoEnabled({ DEMO_ENABLED: '1' }), true);
  assert.equal(await demoAlive({}, 'demo'), false, '꺼지면 받은 데모 쿠키도 죽음');
  const r = await enter(new Request('https://x/demo'), {}, new URL('https://x/demo'), null, () => null);
  assert.equal(r.status, 404);
});
