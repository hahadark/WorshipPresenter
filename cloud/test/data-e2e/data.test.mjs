// 끝에서 끝 (2단계 A): 문서 · 동기화 · 성경 · 사람 계정 · 교회 단위 분리. scripts/e2e.mjs 가 wrangler dev(로컬 D1) 를 띄운 뒤 부른다.
// 두 교회: 'sample'(1단계 견본) · 'data-other'(여기서 만듦). 성경은 지어낸 견본 본문(fixture.mjs)을 넣기 도구로 로컬 D1 에 넣는다.
import test, { before } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { CHURCH, cred, req, login, cookieOf, newIp, socket, fakeBridge } from '../e2e/helpers.mjs';
import { makeBibleDb, verseText } from './fixture.mjs';

const cloud = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const OTHER = 'data-other';
const W = { 'x-worshipcue': '1' };
const run = Date.now().toString(36); // 로컬 D1 은 실행 사이에 남으므로 문서 이름에 붙인다
let other, ownerA, ownerB;

async function loginAs(church, secret) {
  const r = await req('/auth/login', { method: 'POST', body: { church, secret }, headers: W, ip: newIp() });
  assert.equal(r.status, 200, r.text);
  return cookieOf(r.headers);
}
const put = (cookie, kind, id, body, rev, extra = {}) => req(`/api/v1/documents/${kind}/${encodeURIComponent(id)}`, {
  method: 'PUT', cookie, body, headers: { ...W, ...(rev !== undefined ? { 'if-match': String(rev) } : {}), ...extra },
});
const del = (cookie, kind, id, rev) => req(`/api/v1/documents/${kind}/${encodeURIComponent(id)}`, { method: 'DELETE', cookie, headers: { ...W, 'if-match': String(rev) } });
const get = (cookie, kind, id) => req(`/api/v1/documents/${kind}/${encodeURIComponent(id)}`, { cookie });
// 로컬 D1 파일을 wrangler dev 와 같이 쓰므로 드물게 잠겨 있을 수 있다 → 세 번까지
function node(...a) {
  for (let i = 0; ; i++) {
    try { return execFileSync(process.execPath, a, { cwd: cloud, stdio: ['ignore', 'pipe', 'inherit'], env: { ...process.env, WRANGLER_SEND_METRICS: 'false' } }).toString(); } catch (e) { if (i >= 2) throw e; }
  }
}

before(async () => {
  if (!existsSync(join(cloud, '.secrets', `${OTHER}.json`))) node(join('scripts', 'seed.mjs'), '--church', OTHER, '--name', '다른 교회 (시험)');
  other = JSON.parse(readFileSync(join(cloud, '.secrets', `${OTHER}.json`), 'utf8'));
  // 견본 성경: sample 에 한국어 · 영어 두 판, data-other 에 하나
  const dir = mkdtempSync(join(tmpdir(), 'wp-bible-fixture-'));
  try {
    const file = join(dir, 'bible.db');
    makeBibleDb(file, [
      { id: 'SAMPLE', name: '견본 번역', language: 'ko', tag: 'A' },
      { id: 'SAMPLE-EN', name: 'Sample English', language: 'en', tag: 'E' },
      { id: 'OTHERV', name: '다른 교회 번역', language: 'ko', tag: 'B' },
    ]);
    // 넣기 도구로 SQL 만 만들고 (wrangler 를 세 번 띄우지 않게) 한 번에 로컬 D1 에
    const parts = [['SAMPLE', CHURCH], ['SAMPLE-EN', CHURCH, '--only', '43:3'], ['OTHERV', OTHER]].map(([v, c, ...more], i) => {
      const out = join(dir, `${i}.sql`);
      node(join('scripts', 'bible-import.mjs'), '--db', file, '--version', v, '--church', c, '--out', out, ...more);
      return readFileSync(out, 'utf8');
    });
    writeFileSync(join(dir, 'all.sql'), parts.join(String.fromCharCode(10)));
    node(join('node_modules', 'wrangler', 'bin', 'wrangler.js'), 'd1', 'execute', 'worshippresenter-web', '--local', '--file', join(dir, 'all.sql'), '-y');
  } finally { rmSync(dir, { recursive: true, force: true }); }
  ownerA = await login('owner');
  ownerB = await loginAs(OTHER, other.credentials.owner.secret);
}, { timeout: 180_000 });

test('자료 길은 로그인 · 권한 뒤에만 (출력 · 리모컨 · 편집기 PIN 기기는 못 읽음)', async () => {
  for (const p of ['/api/v1/documents', '/api/v1/documents/cue/x', '/api/v1/sync', '/api/v1/bible/versions', '/api/v1/bible/passage?version=SAMPLE&ref=요3:16', '/api/v1/members']) {
    assert.equal((await req(p)).status, 401, p);
  }
  assert.equal((await req('/api/v1/sync', { method: 'POST', body: { mutations: [] } })).status, 401);
  const out = await login('output');
  const remote = await login('remote');
  const editor = await login('editor');
  for (const c of [out, remote, editor]) {
    assert.equal((await req('/api/v1/documents', { cookie: c })).status, 403);
    assert.equal((await req('/api/v1/bible/versions', { cookie: c })).status, 403);
    assert.equal((await put(c, 'cue', `x-${run}`, { name: 'x' })).status, 403);
  }
  // 쿠키로 오는 쓰기는 X-WorshipCue 가 있어야 (다른 사이트가 몰래 못 보내게)
  const r = await req(`/api/v1/documents/cue/csrf-${run}`, { method: 'PUT', cookie: ownerA, body: { name: 'x' } });
  assert.equal(r.status, 403);
  assert.equal((await req(`/api/v1/documents/cue/csrf-${run}`, { method: 'PUT', cookie: ownerA, body: { name: 'x' }, headers: { ...W, origin: 'https://evil.example' } })).status, 403);
  // 잘못된 종류 · 이름
  assert.equal((await put(ownerA, 'nope', 'a', {})).status, 404);
  assert.equal((await put(ownerA, 'cue', 'a/b', {})).status, 400);
  assert.equal((await put(ownerA, 'cue', '-bad', {})).status, 400);
  assert.equal((await put(ownerA, 'cue', `arr-${run}`, [1, 2])).status, 400, 'JSON 객체만');
});

test('문서: 만들기 · If-Match 판 확인 · 409 + 최신 · 지움 묘비 · 되살리기', async () => {
  const id = `cue-${run}`;
  const c1 = await put(ownerA, 'cue', id, { id, name: '주일 예배', serviceDate: '2026-11-29', items: [{ kind: 'bible', ref: '요3:16' }] });
  assert.equal(c1.status, 201, c1.text);
  assert.equal(c1.json.revision, 1);
  assert.equal(c1.headers.get('etag'), '"1"');
  // If-Match 없이 다시 만들면 덮어쓰지 않는다
  const again = await put(ownerA, 'cue', id, { id, name: '덮어쓰기' });
  assert.equal(again.status, 409);
  assert.equal(again.json.conflict, true);
  assert.equal(again.json.current.revision, 1);
  assert.equal(again.json.current.payload.name, '주일 예배');
  const c2 = await put(ownerA, 'cue', id, { id, name: '주일 예배 (고침)', items: [] }, 1);
  assert.equal(c2.status, 200);
  assert.equal(c2.json.revision, 2);
  assert.ok(c2.json.seq > c1.json.seq);
  // 옛 판으로 고치면 409 + 최신 (자동으로 덮어쓰지 않음)
  const stale = await put(ownerA, 'cue', id, { id, name: '옛 판에서 고침' }, '"1"');
  assert.equal(stale.status, 409);
  assert.equal(stale.json.current.revision, 2);
  assert.equal(stale.json.current.payload.name, '주일 예배 (고침)');
  const g = await get(ownerA, 'cue', id);
  assert.equal(g.status, 200);
  assert.equal(g.json.payload.name, '주일 예배 (고침)');
  assert.equal(g.json.title, '주일 예배 (고침)');
  assert.equal(g.json.updatedBy, cred('owner').id);
  assert.equal(g.headers.get('etag'), '"2"');
  // 지움: If-Match 가 있어야, 맞아야
  assert.equal((await req(`/api/v1/documents/cue/${id}`, { method: 'DELETE', cookie: ownerA, headers: W })).status, 428);
  assert.equal((await del(ownerA, 'cue', id, 1)).status, 409);
  const d = await del(ownerA, 'cue', id, 2);
  assert.equal(d.status, 200);
  assert.equal(d.json.revision, 3);
  assert.equal(d.json.deleted, true);
  const gone = await get(ownerA, 'cue', id);
  assert.equal(gone.status, 404);
  assert.equal(gone.json.deleted, true);
  assert.equal((await del(ownerA, 'cue', id, 3)).status, 404, '이미 지움');
  assert.equal((await put(ownerA, 'cue', id, { id, name: 'x' })).status, 409, '묘비 위에 새로 만들기는 판이 있어야');
  const back = await put(ownerA, 'cue', id, { id, name: '되살림' }, 3);
  assert.equal(back.status, 200);
  assert.equal(back.json.revision, 4);
  assert.equal((await get(ownerA, 'cue', 'never-' + run)).status, 404);
});

test('문서: X-Mutation-Id 로 다시 보내도 두 번 쓰지 않는다', async () => {
  const id = `song-${run}`;
  const mid = `mut-${run}-a`;
  const a = await put(ownerA, 'song', id, { title: '나 같은 죄인', lyrics: '…' }, undefined, { 'x-mutation-id': mid });
  assert.equal(a.status, 201);
  const b = await put(ownerA, 'song', id, { title: '나 같은 죄인', lyrics: '…' }, undefined, { 'x-mutation-id': mid });
  assert.equal(b.status, 200);
  assert.equal(b.json.duplicate, true);
  assert.equal(b.json.seq, a.json.seq);
  assert.equal(b.json.revision, 1);
  assert.equal((await put(ownerA, 'song', `${id}-2`, { title: 'x' }, undefined, { 'x-mutation-id': mid })).status, 409, '같은 값을 다른 문서에');
});

test('목록: 종류별 · 쪽 나누기(cursor) · 지운 것 빼기 · 본문 싣기', async () => {
  const ids = [0, 1, 2, 3, 4].map(i => `tpl-${run}-${i}`);
  for (const id of ids) assert.equal((await put(ownerA, 'template', id, { name: `틀 ${id}` })).status, 201);
  assert.equal((await del(ownerA, 'template', ids[2], 1)).status, 200);
  const seen = [];
  let cursor = '';
  for (let n = 0; n < 50; n++) {
    const r = await req(`/api/v1/documents?kind=template&limit=2${cursor ? `&cursor=${cursor}` : ''}`, { cookie: ownerA });
    assert.equal(r.status, 200, r.text);
    assert.ok(r.json.documents.length <= 2);
    for (const d of r.json.documents) { assert.equal(d.kind, 'template'); assert.equal(d.payload, undefined); seen.push(d.id); }
    if (!r.json.next) break;
    cursor = r.json.next;
  }
  const mine = seen.filter(x => x.startsWith(`tpl-${run}-`));
  assert.deepEqual(mine, [ids[0], ids[1], ids[3], ids[4]], '차례대로, 지운 것 빼고');
  assert.deepEqual([...seen].sort(), seen, 'id 차례');
  const withDel = await req(`/api/v1/documents?kind=template&deleted=1&limit=200`, { cookie: ownerA });
  assert.ok(withDel.json.documents.some(d => d.id === ids[2] && d.deleted));
  const full = await req(`/api/v1/documents?kind=template&include=payload&limit=200`, { cookie: ownerA });
  assert.equal(full.json.documents.find(d => d.id === ids[0]).payload.name, `틀 ${ids[0]}`);
  assert.equal((await req('/api/v1/documents?cursor=@@@', { cookie: ownerA })).status, 400);
  assert.equal((await req('/api/v1/documents?kind=nope', { cookie: ownerA })).status, 400);
});

test('동기화 받기: after 뒤 변경 · 쪽 나누기 · 지나간 판은 본문 없이 · 지움', async () => {
  const head0 = (await req('/api/v1/sync?after=0&limit=1', { cookie: ownerA })).json.head;
  const id = `theme-${run}`;
  await put(ownerA, 'theme', id, { name: '판 1' });
  await put(ownerA, 'theme', id, { name: '판 2' }, 1);
  await put(ownerA, 'overlay', `ov-${run}`, { name: '자막' });
  await del(ownerA, 'overlay', `ov-${run}`, 1);
  const got = [];
  let after = head0;
  for (let n = 0; n < 20; n++) {
    const r = await req(`/api/v1/sync?after=${after}&limit=2`, { cookie: ownerA });
    assert.equal(r.status, 200, r.text);
    got.push(...r.json.changes);
    assert.ok(r.json.next >= after);
    after = r.json.next;
    if (!r.json.more) { assert.equal(r.json.next, r.json.head); break; }
  }
  assert.deepEqual(got.map(c => c.seq), [...got.map(c => c.seq)].sort((a, b) => a - b), 'seq 차례');
  const t = got.filter(c => c.id === id);
  assert.equal(t.length, 2);
  assert.equal(t[0].superseded, true);
  assert.equal(t[0].payload, undefined);
  assert.equal(t[1].payload.name, '판 2');
  assert.equal(t[1].revision, 2);
  const ov = got.filter(c => c.id === `ov-${run}`);
  assert.deepEqual(ov.map(c => c.op), ['put', 'delete']);
  assert.equal(ov[1].payload, undefined);
  assert.equal((await req('/api/v1/sync?after=-1', { cookie: ownerA })).status, 400);
  assert.equal((await req('/api/v1/sync?after=999999999', { cookie: ownerA })).json.changes.length, 0);
});

test('동기화 올리기: 여러 개 · mutationId 로 두 번 안 들어감 · 충돌은 최신과 함께 · 나머지는 들어감', async () => {
  const base = `macro-${run}`;
  await put(ownerA, 'macro', `${base}-c`, { name: '먼저 있던 것' });
  const body = {
    mutations: [
      { mutationId: `${run}-m1`, kind: 'macro', id: `${base}-a`, op: 'put', baseRevision: 0, payload: { name: 'A' } },
      { mutationId: `${run}-m2`, kind: 'macro', id: `${base}-a`, op: 'put', baseRevision: 1, payload: { name: 'A2' } },
      { mutationId: `${run}-m3`, kind: 'macro', id: `${base}-c`, op: 'put', baseRevision: 0, payload: { name: '충돌' } },
      { mutationId: `${run}-m4`, kind: 'macro', id: `${base}-nope`, op: 'delete', baseRevision: 1 },
      { mutationId: `${run}-m5`, kind: 'bogus', id: 'x', op: 'put', payload: {} },
    ],
  };
  const r = await req('/api/v1/sync', { method: 'POST', cookie: ownerA, body, headers: W });
  assert.equal(r.status, 200, r.text);
  const [m1, m2, m3, m4, m5] = r.json.results;
  assert.equal(m1.status, 'applied'); assert.equal(m1.revision, 1);
  assert.equal(m2.status, 'applied'); assert.equal(m2.revision, 2); assert.equal(m2.seq, m1.seq + 1);
  assert.equal(m3.status, 'conflict'); assert.equal(m3.current.payload.name, '먼저 있던 것');
  assert.equal(m4.status, 'missing');
  assert.equal(m5.status, 'invalid');
  assert.equal(r.json.head, m2.seq);
  // 같은 것을 다시 올려도 (응답을 못 받은 재시도) 두 번 쓰지 않는다
  const again = await req('/api/v1/sync', { method: 'POST', cookie: ownerA, body, headers: W });
  assert.equal(again.json.results[0].status, 'duplicate');
  assert.equal(again.json.results[0].seq, m1.seq);
  assert.equal(again.json.results[1].status, 'duplicate');
  assert.equal(again.json.head, r.json.head, '변경 기록이 늘지 않음');
  assert.equal((await get(ownerA, 'macro', `${base}-a`)).json.payload.name, 'A2');
  // 한 요청 안에 같은 mutationId 두 번 · 너무 많이
  assert.equal((await req('/api/v1/sync', { method: 'POST', cookie: ownerA, headers: W, body: { mutations: [body.mutations[0], body.mutations[0]] } })).status, 400);
  const many = Array.from({ length: 21 }, (_, i) => ({ mutationId: `${run}-many-${i}`, kind: 'macro', id: `${base}-m${i}`, op: 'put', payload: {} }));
  assert.equal((await req('/api/v1/sync', { method: 'POST', cookie: ownerA, headers: W, body: { mutations: many } })).status, 413);
  // 20개는 한 번에 (D1 요청당 질의 50개 안)
  const twenty = many.slice(0, 20);
  const ok20 = await req('/api/v1/sync', { method: 'POST', cookie: ownerA, headers: W, body: { mutations: twenty } });
  assert.equal(ok20.status, 200, ok20.text);
  assert.ok(ok20.json.results.every(x => x.status === 'applied'));
});

test('설치판 열쇠(Bearer)로 자료 동기화 (2B 가 쓰는 길) · 틀린 열쇠는 401', async () => {
  const auth = { authorization: `Bearer ${cred('bridge').key}` };
  const r = await req('/api/v1/sync?after=0&limit=5', { headers: auth });
  assert.equal(r.status, 200, r.text);
  const m = await req('/api/v1/sync', { method: 'POST', headers: auth, body: { mutations: [{ mutationId: `${run}-bridge-1`, kind: 'settings', id: `church-${run}`, op: 'put', payload: { name: '교회 설정' } }] } });
  assert.equal(m.json.results[0].status, 'applied');
  const d = await req(`/api/v1/documents/settings/church-${run}`, { headers: auth });
  assert.equal(d.json.updatedBy, cred('bridge').id);
  assert.equal((await req('/api/v1/sync', { headers: { authorization: `Bearer ${CHURCH}.wrongwrongwrongwrongwrongwrong` }, ip: newIp() })).status, 401);
  assert.equal((await req('/api/v1/sync', { headers: { authorization: `Bearer ${cred('output').key}` }, ip: newIp() })).status, 401, '출력 열쇠는 설치판 열쇠가 아님');
  assert.equal((await req('/api/v1/members', { headers: auth })).status, 401, '사람 계정 관리는 Bearer 로 안 됨');
});

test('성경: 번역본 목록 · 장절 (설치판 resolve_reference 와 같은 모양) · 오류', async () => {
  const v = await req('/api/v1/bible/versions', { cookie: ownerA });
  assert.equal(v.status, 200);
  const ids = v.json.versions.map(x => x.id);
  assert.ok(ids.includes('SAMPLE') && ids.includes('SAMPLE-EN'));
  assert.ok(!ids.includes('OTHERV'), '다른 교회 번역본은 안 보임');
  const p = await req(`/api/v1/bible/passage?version=SAMPLE&ref=${encodeURIComponent('요 3 16')}`, { cookie: ownerA });
  assert.equal(p.status, 200, p.text);
  assert.equal(p.json.passage.title, '요한복음 3:16');
  assert.deepEqual(p.json.passage.book, { id: 43, name: '요한복음', short: '요' });
  assert.deepEqual(p.json.passage.verses, [{ book: 43, chapter: 3, verse: 16, text: verseText('A', 43, 3, 16) }]);
  assert.equal(p.json.passage.headings.length, 2);
  const multi = await req(`/api/v1/bible/passage?version=SAMPLE&ref=${encodeURIComponent('롬8:1,28-30')}`, { cookie: ownerA });
  assert.equal(multi.json.passage.title, '로마서 8:1, 28-30');
  assert.deepEqual(multi.json.passage.verses.map(x => x.verse), [1, 28, 29, 30]);
  const cross = await req(`/api/v1/bible/passage?version=SAMPLE&ref=${encodeURIComponent('창1:30-2:2')}`, { cookie: ownerA });
  assert.deepEqual(cross.json.passage.verses.map(x => `${x.chapter}:${x.verse}`), ['1:30', '1:31', '2:1', '2:2']);
  const whole = await req(`/api/v1/bible/passage?version=SAMPLE&ref=${encodeURIComponent('시편 23')}`, { cookie: ownerA });
  assert.equal(whole.json.passage.verses.length, 6);
  assert.deepEqual(whole.json.passage.ranges, [{ sc: 23, sv: 1, ec: 23, ev: 6 }]);
  const jude = await req(`/api/v1/bible/passage?version=SAMPLE&ref=${encodeURIComponent('유 3')}`, { cookie: ownerA });
  assert.equal(jude.json.passage.title, '유다서 1:3');
  const en = await req(`/api/v1/bible/passage?version=SAMPLE-EN&ref=${encodeURIComponent('John 3:16-17')}`, { cookie: ownerA });
  assert.equal(en.json.passage.title, 'John 3:16-17');
  assert.equal(en.json.passage.versionName, 'Sample English');
  // 오류 (설치판과 같은 글)
  const e1 = await req(`/api/v1/bible/passage?version=SAMPLE&ref=${encodeURIComponent('요3:99')}`, { cookie: ownerA });
  assert.equal(e1.status, 422);
  assert.equal(e1.json.error, '요한복음 3장은 36절까지 있습니다.');
  const e2 = await req(`/api/v1/bible/passage?version=SAMPLE&ref=${encodeURIComponent('요한 3:16')}`, { cookie: ownerA });
  assert.equal(e2.json.ambiguous, true);
  assert.ok(e2.json.candidates.length > 1);
  const e3 = await req(`/api/v1/bible/passage?version=SAMPLE&ref=${encodeURIComponent('출 3:14')}`, { cookie: ownerA });
  assert.equal(e3.json.error, '이 번역본에는 출애굽기 데이터가 없습니다.');
  const e4 = await req(`/api/v1/bible/passage?version=SAMPLE-EN&ref=${encodeURIComponent('요 1:1')}`, { cookie: ownerA });
  assert.equal(e4.json.error, '해당 구절의 본문이 없습니다.', '일부만 넣은 번역본');
  const e6 = await req(`/api/v1/bible/passage?version=SAMPLE-EN&ref=${encodeURIComponent('롬 8:1')}`, { cookie: ownerA });
  assert.equal(e6.json.error, '이 번역본에는 Romans 데이터가 없습니다.');
  const e5 = await req(`/api/v1/bible/passage?version=SAMPLE&ref=${encodeURIComponent('요')}`, { cookie: ownerA });
  assert.equal(e5.json.incomplete, true);
  assert.equal(e5.json.book.chapters, 3);
  assert.equal((await req('/api/v1/bible/passage?version=NONE&ref=요3:16', { cookie: ownerA })).status, 404);
});

test('교회 단위 분리: 다른 교회 열쇠로 남의 문서 · 목록 · 변경 기록 · 성경을 읽지도 고치지도 못한다', async () => {
  const id = `secret-${run}`;
  const a = await put(ownerA, 'cue', id, { name: 'A 교회만의 큐시트' });
  assert.equal(a.status, 201);
  // B 교회 주인
  assert.equal((await get(ownerB, 'cue', id)).status, 404);
  const listB = await req('/api/v1/documents?limit=200&deleted=1&include=payload', { cookie: ownerB });
  assert.ok(!listB.text.includes('A 교회만의'), '목록에 안 보임');
  const syncB = await req('/api/v1/sync?after=0&limit=500', { cookie: ownerB });
  assert.ok(!syncB.text.includes(id) && !syncB.text.includes('A 교회만의'), '변경 기록에 안 보임');
  // 같은 이름으로 만들면 B 교회 것이 따로 생긴다 (A 것을 덮지 않음)
  const b = await put(ownerB, 'cue', id, { name: 'B 교회 것' });
  assert.equal(b.status, 201);
  assert.equal(b.json.revision, 1);
  assert.equal((await get(ownerA, 'cue', id)).json.payload.name, 'A 교회만의 큐시트');
  // 판 번호를 알아도 남의 것을 지우지 못한다 (자기 교회 것만 지워짐)
  assert.equal((await del(ownerB, 'cue', id, 1)).status, 200);
  assert.equal((await get(ownerA, 'cue', id)).status, 200);
  // 동기화로 올려도 자기 교회에만
  const push = await req('/api/v1/sync', { method: 'POST', cookie: ownerB, headers: W, body: { mutations: [{ mutationId: `${run}-iso-1`, kind: 'cue', id, op: 'put', baseRevision: 1, payload: { name: '남의 것 고치기 시도' } }] } });
  assert.equal(push.json.results[0].status, 'conflict', 'B 교회의 묘비(판 2)와 비교됨');
  assert.equal((await get(ownerA, 'cue', id)).json.payload.name, 'A 교회만의 큐시트');
  // mutationId 가 같아도 교회가 다르면 따로 센다
  const pushA = await req('/api/v1/sync', { method: 'POST', cookie: ownerA, headers: W, body: { mutations: [{ mutationId: `${run}-iso-1`, kind: 'cue', id: `${id}-m`, op: 'put', payload: { name: 'A' } }] } });
  assert.equal(pushA.json.results[0].status, 'applied');
  // 성경
  const vB = await req('/api/v1/bible/versions', { cookie: ownerB });
  assert.deepEqual(vB.json.versions.map(x => x.id), ['OTHERV']);
  assert.equal((await req('/api/v1/bible/passage?version=SAMPLE&ref=요3:16', { cookie: ownerB })).status, 404, 'A 교회 번역본');
  const pB = await req(`/api/v1/bible/passage?version=OTHERV&ref=${encodeURIComponent('요3:16')}`, { cookie: ownerB });
  assert.equal(pB.json.passage.verses[0].text, verseText('B', 43, 3, 16));
  assert.equal((await req('/api/v1/bible/passage?version=OTHERV&ref=요3:16', { cookie: ownerA })).status, 404, 'B 교회 번역본');
  // B 교회 설치판 열쇠
  const bb = { authorization: `Bearer ${other.credentials.bridge.key}` };
  assert.equal((await req(`/api/v1/documents/cue/${id}`, { headers: bb })).status, 404);
  assert.ok(!(await req('/api/v1/sync?after=0&limit=500', { headers: bb })).text.includes('A 교회만의'));
  // 열쇠 앞머리를 남의 교회로 바꿔 끼우면 해시가 맞지 않는다
  assert.equal((await req('/api/v1/sync', { headers: { authorization: `Bearer ${CHURCH}.${other.credentials.bridge.secret}` }, ip: newIp() })).status, 401);
  // 사람 계정 목록도 교회마다
  const mk = await req('/api/v1/members', { method: 'POST', cookie: ownerA, headers: W, body: { login: `iso${run}`, role: 'viewer' } });
  assert.equal(mk.status, 201);
  assert.ok(!(await req('/api/v1/members', { cookie: ownerB })).text.includes(`iso${run}`));
  assert.equal((await req(`/api/v1/members/${mk.json.member.id}`, { method: 'DELETE', cookie: ownerB, headers: W })).status, 404, '남의 교회 사람은 못 지움');
  const asB = await req('/auth/member', { method: 'POST', headers: W, ip: newIp(), body: { church: OTHER, login: `iso${run}`, password: mk.json.password } });
  assert.equal(asB.status, 401, '다른 교회 이름으로는 로그인 안 됨');
});

test('사람 계정: 주인이 만들기 · 비밀번호 로그인 · 역할별 권한 · 역할 바꾸면 옛 쿠키 끊김 · 지우기', async () => {
  const login = `kim${run}`;
  const mk = await req('/api/v1/members', { method: 'POST', cookie: ownerA, headers: W, body: { login, role: 'editor', label: '김 집사' } });
  assert.equal(mk.status, 201, mk.text);
  assert.equal(mk.json.member.login, login);
  assert.equal(mk.json.member.role, 'editor');
  assert.equal(mk.json.password.length, 16);
  assert.equal((await req('/api/v1/members', { method: 'POST', cookie: ownerA, headers: W, body: { login, role: 'viewer' } })).status, 409);
  assert.ok(!(await req('/api/v1/members', { cookie: ownerA })).text.includes('secret'), '해시를 내보내지 않음');
  // 로그인
  const bad = await req('/auth/member', { method: 'POST', headers: W, ip: newIp(), body: { church: CHURCH, login, password: 'wrong-password' } });
  assert.equal(bad.status, 401);
  const ok = await req('/auth/member', { method: 'POST', headers: W, ip: newIp(), body: { church: CHURCH, login, password: mk.json.password } });
  assert.equal(ok.status, 200, ok.text);
  assert.equal(ok.json.role, 'member-editor');
  assert.match(ok.headers.getSetCookie().join('\n'), /HttpOnly/);
  const kim = cookieOf(ok.headers);
  const me = await req('/api/v1/me', { cookie: kim });
  assert.equal(me.json.role, 'member-editor');
  // 편집자: 자료 읽고 쓰기 · 출력 보기 O, 열쇠 · 사람 관리 X
  const w = await put(kim, 'song', `kim-song-${run}`, { title: '편집자가 넣은 찬양' });
  assert.equal(w.status, 201);
  assert.equal((await get(ownerA, 'song', `kim-song-${run}`)).json.updatedBy, `m:${mk.json.member.id}`);
  assert.equal((await req('/outputs/main', { cookie: kim })).status, 200);
  assert.equal((await req('/api/v1/credentials', { cookie: kim })).status, 403);
  assert.equal((await req('/api/v1/members', { cookie: kim })).status, 403);
  assert.equal((await req('/owner', { cookie: kim })).status, 403);
  const ws = socket('/events?client=main', { cookie: kim });
  await ws.opened;
  await ws.next('hello');
  // 보기만으로 바꾸면 앞의 쿠키 · 연결이 끊긴다
  const ch = await req(`/api/v1/members/${mk.json.member.id}`, { method: 'PATCH', cookie: ownerA, headers: W, body: { role: 'viewer' } });
  assert.equal(ch.status, 200, ch.text);
  assert.equal((await ws.waitClose()).code, 4003);
  assert.equal((await req('/api/v1/me', { cookie: kim })).status, 401, '옛 쿠키');
  const ok2 = await req('/auth/member', { method: 'POST', headers: W, ip: newIp(), body: { church: CHURCH, login, password: mk.json.password } });
  const viewer = cookieOf(ok2.headers);
  assert.equal(ok2.json.role, 'member-viewer');
  assert.equal((await get(viewer, 'song', `kim-song-${run}`)).status, 200, '보기만: 읽기 O');
  assert.equal((await put(viewer, 'song', `viewer-${run}`, { title: 'x' })).status, 403, '보기만: 쓰기 X');
  assert.equal((await req('/api/v1/bible/versions', { cookie: viewer })).status, 200);
  assert.equal((await req('/r/state', { cookie: viewer })).status, 403, '보기만: 리모컨 X');
  // 본인 비밀번호 바꾸기 → 이 기기는 새 쿠키로 계속, 옛 비밀번호는 안 됨
  assert.equal((await req('/api/v1/me/password', { method: 'POST', cookie: viewer, headers: W, body: { current: 'nope-nope-nope', next: 'Abcdefghij12' } })).status, 401);
  assert.equal((await req('/api/v1/me/password', { method: 'POST', cookie: viewer, headers: W, body: { current: mk.json.password, next: 'short' } })).status, 400);
  const pw = await req('/api/v1/me/password', { method: 'POST', cookie: viewer, headers: W, body: { current: mk.json.password, next: 'Sarang-Daegu-2026' } });
  assert.equal(pw.status, 200, pw.text);
  const viewer2 = cookieOf(pw.headers);
  assert.equal((await req('/api/v1/me', { cookie: viewer })).status, 401, '옛 쿠키');
  assert.equal((await req('/api/v1/me', { cookie: viewer2 })).status, 200, '새 쿠키');
  assert.equal((await req('/auth/member', { method: 'POST', headers: W, ip: newIp(), body: { church: CHURCH, login, password: mk.json.password } })).status, 401);
  assert.equal((await req('/auth/member', { method: 'POST', headers: W, ip: newIp(), body: { church: CHURCH, login, password: 'Sarang-Daegu-2026' } })).status, 200);
  // 주인이 비밀번호 새로 만들기
  const reset = await req(`/api/v1/members/${mk.json.member.id}`, { method: 'PATCH', cookie: ownerA, headers: W, body: { resetPassword: true } });
  assert.equal(reset.json.password.length, 16);
  assert.equal((await req('/api/v1/me', { cookie: viewer2 })).status, 401);
  // 사람 주인 계정은 사람 관리를 할 수 있고, 자기 자신은 못 지운다
  const boss = await req('/api/v1/members', { method: 'POST', cookie: ownerA, headers: W, body: { login: `boss${run}`, role: 'owner' } });
  const bl = await req('/auth/member', { method: 'POST', headers: W, ip: newIp(), body: { church: CHURCH, login: `boss${run}`, password: boss.json.password } });
  assert.equal(bl.json.role, 'owner');
  assert.equal(bl.json.home, '/owner');
  const bossC = cookieOf(bl.headers);
  assert.equal((await req('/api/v1/members', { cookie: bossC })).status, 200);
  assert.equal((await req(`/api/v1/members/${boss.json.member.id}`, { method: 'DELETE', cookie: bossC, headers: W })).status, 409);
  assert.equal((await req(`/api/v1/members/${boss.json.member.id}`, { method: 'PATCH', cookie: bossC, headers: W, body: { role: 'viewer' } })).status, 409);
  // 지우기
  assert.equal((await req(`/api/v1/members/${mk.json.member.id}`, { method: 'DELETE', cookie: bossC, headers: W })).status, 200);
  assert.equal((await req('/auth/member', { method: 'POST', headers: W, ip: newIp(), body: { church: CHURCH, login, password: reset.json.password } })).status, 401);
  assert.ok(!(await req('/api/v1/members', { cookie: ownerA })).json.members.some(m => m.login === login));
  // 지운 이름으로 다시 만들면 새 id 로 되살아남
  const again = await req('/api/v1/members', { method: 'POST', cookie: ownerA, headers: W, body: { login, role: 'viewer' } });
  assert.equal(again.status, 201);
  assert.notEqual(again.json.member.id, mk.json.member.id);
  // 같은 기기 주소에서 5번 틀리면 잠깐 잠금
  const ip = newIp();
  for (let i = 0; i < 5; i++) await req('/auth/member', { method: 'POST', headers: W, ip, body: { church: CHURCH, login, password: `wrong-${i}-xxxxx` } });
  assert.equal((await req('/auth/member', { method: 'POST', headers: W, ip, body: { church: CHURCH, login, password: again.json.password } })).status, 429);
  assert.equal((await req('/auth/member', { method: 'POST', headers: W, ip: newIp(), body: { church: CHURCH, login, password: again.json.password } })).status, 200, '다른 주소는 그대로');
  // Cloudflare Access 길은 설정이 없으면 닫혀 있다
  assert.equal((await req(`/auth/access?church=${CHURCH}`)).status, 404);
});

test("'/' 는 역할마다 로그인 답의 home 과 같은 첫 화면으로 (사람 계정 · 기기 열쇠)", async () => {
  const first = async cookie => new URL((await req('/', { cookie })).headers.get('location')).pathname;
  assert.equal(await first(''), '/login');
  assert.equal(await first(ownerA), '/owner');
  assert.equal(await first(await login('output')), '/outputs/main');
  assert.equal(await first(await login('remote')), '/remote');
  assert.equal(await first(await login('editor')), '/editor');
  for (const role of ['editor', 'viewer', 'owner']) {
    const name = `home${role}${run}`;
    const mk = await req('/api/v1/members', { method: 'POST', cookie: ownerA, headers: W, body: { login: name, role } });
    assert.equal(mk.status, 201, mk.text);
    const r = await req('/auth/member', { method: 'POST', headers: W, ip: newIp(), body: { church: CHURCH, login: name, password: mk.json.password } });
    assert.equal(r.status, 200, r.text);
    assert.equal(await first(cookieOf(r.headers)), r.json.home, role);
    assert.equal(r.json.home, { editor: '/editor', viewer: '/outputs/main', owner: '/owner' }[role]);
  }
});

test('기기 열쇠 · PIN 로그인 · 실시간 중계는 그대로 (1단계와 함께 돈다)', async () => {
  const bridge = await fakeBridge();
  const out = await login('output');
  const v = socket('/events?client=main', { cookie: out });
  await v.opened;
  assert.equal((await v.next('hello')).online, true);
  bridge.send({ t: 'render', client: 'main', data: { version: 77, output: 'main' } });
  assert.equal((await v.next(m => m.t === 'render' && m.data.version === 77)).data.version, 77);
  await v.close();
  await bridge.close();
});
