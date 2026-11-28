// 끝에서 끝 (7단계 일부): 등급 셋 — 데모 · 무료(가짜 구글 드라이브) · 유료(D1 · R2) — 과 옮기기 · 교회 분리.
// scripts/e2e.mjs 가 wrangler dev(로컬 D1 · R2 · DO) + 가짜 구글(test/fake-google/server.mjs)을 띄운 뒤 부른다. 진짜 구글은 부르지 않는다.
import test, { before } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { BASE, CHURCH, cookieOf, fakeBridge, login, newIp, req, socket, sleep } from '../e2e/helpers.mjs';
import { SHAPE, verseText } from '../data-e2e/fixture.mjs';

const cloud = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const G = process.env.WP_FAKE_GOOGLE;
const ADMIN = JSON.parse(readFileSync(join(cloud, '.secrets', 'auth.json'), 'utf8')).ADMIN_TOKEN;
const W = { 'x-worshipcue': '1' };
const run = Date.now().toString(36);
const sha = b => createHash('sha256').update(b).digest('hex');
const enc = encodeURIComponent;

const fake = (path, body) => fetch(G + path, body ? { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(body) } : {}).then(r => r.json());
const admin = (path, body, method = 'POST') => req(path, { method, headers: { authorization: `Bearer ${ADMIN}` }, body });
const tree = async email => (await fake(`/_fake/tree?email=${enc(email)}`)).files;

/** 구글로 시작하기 / 로그인 / 다시 연결 — 가짜 구글 동의 화면을 지나 콜백까지 */
async function google(mode, { email, name = '', cookie = '', extra = '' } = {}) {
  const ip = newIp();
  const r1 = await req(`/auth/google/start?mode=${mode}${name ? `&name=${enc(name)}` : ''}&hint=${enc(email)}${extra}`, { cookie, ip });
  assert.equal(r1.status, 302, r1.text);
  const loc = new URL(r1.headers.get('location'));
  if (loc.origin === new URL(BASE).origin) return { status: 302, location: loc.pathname + loc.search, cookie: '' };
  const oauth = cookieOf(r1.headers);
  const r2 = await fetch(loc, { redirect: 'manual' });
  assert.equal(r2.status, 302);
  const cb = new URL(r2.headers.get('location'));
  const r3 = await req(cb.pathname + cb.search, { cookie: [cookie, oauth].filter(Boolean).join('; '), ip });
  return { status: r3.status, location: r3.headers.get('location'), cookie: cookieOf(r3.headers) };
}

/** 이진 본문 (helpers.req 는 본문을 JSON 으로 바꾼다) */
async function bin(path, { method = 'GET', cookie, headers = {}, body } = {}) {
  const r = await fetch(BASE + path, { method, headers: { ...headers, ...(cookie ? { cookie } : {}) }, body });
  const text = await r.text();
  let json = null;
  try { json = JSON.parse(text); } catch { /* 글 */ }
  return { status: r.status, text, json, headers: r.headers };
}

const put = (cookie, kind, id, body, rev, extra = {}) => req(`/api/v1/documents/${kind}/${enc(id)}`, { method: 'PUT', cookie, body, headers: { ...W, ...(rev !== undefined ? { 'if-match': String(rev) } : {}), ...extra } });
const get = (cookie, kind, id) => req(`/api/v1/documents/${kind}/${enc(id)}`, { cookie });
const del = (cookie, kind, id, rev) => req(`/api/v1/documents/${kind}/${enc(id)}`, { method: 'DELETE', cookie, headers: { ...W, 'if-match': String(rev) } });

const alice = { email: `alice-${run}@example.com`, name: `앨리스 교회 ${run}` };
const bob = { email: `bob-${run}@example.com`, name: `밥 교회 ${run}` };
let ownerA = '', ownerB = '', ownerPaid = '';

before(async () => {
  assert.ok(G, 'WP_FAKE_GOOGLE 가 없습니다 (npm run test:e2e 로 돌리세요)');
  ownerPaid = await login('owner');
}, { timeout: 60_000 });

test('무료: 구글로 시작하기 → 교회 · 주인 계정 · 드라이브 폴더 (토큰은 브라우저에 오지 않음)', async () => {
  const r = await google('signup', alice);
  assert.equal(r.status, 302);
  assert.match(r.location, /\/owner\?google=welcome$/);
  ownerA = r.cookie;
  const me = await req('/api/v1/me', { cookie: ownerA });
  assert.equal(me.json.role, 'owner');
  alice.church = me.json.church;
  assert.match(alice.church, /^c-[a-z0-9]{8}$/);
  const t = await req('/api/v1/tier', { cookie: ownerA });
  assert.equal(t.json.tier, 'free');
  assert.equal(t.json.store, 'drive');
  assert.equal(t.json.drive.connected, true);
  assert.equal(t.json.limits.multipart, false);
  const d = await req('/api/v1/drive', { cookie: ownerA });
  assert.equal(d.json.connected, true);
  assert.equal(d.json.email, alice.email);
  assert.ok(!/ya29|1\/\/fake|refresh/i.test(JSON.stringify(d.json) + JSON.stringify(t.json)), '토큰이 응답에 없어야 함');
  const files = await tree(alice.email);
  assert.ok(files.some(f => f.path === `워십프리젠터/${alice.name}`), JSON.stringify(files));
  // 같은 구글 계정으로 또 가입하면 새 교회를 만들지 않고 그 교회로
  const again = await google('signup', { email: alice.email, name: '다른 이름' });
  assert.match(again.location, /google=exists/);
  assert.equal((await req('/api/v1/me', { cookie: again.cookie })).json.church, alice.church);
  // 구글로 로그인
  const li = await google('login', { email: alice.email });
  assert.equal((await req('/api/v1/me', { cookie: li.cookie })).json.church, alice.church);
  // 동의 화면에서 드라이브 칸을 끄면 가입하지 않는다
  await fake('/_fake/consent', { email: `nodrive-${run}@example.com`, drive: false });
  const nd = await google('signup', { email: `nodrive-${run}@example.com`, name: '드라이브 없음' });
  assert.match(nd.location, /\/login\?google=scope$/);
});

test('무료: 큐시트 쓰기 · 읽기 · 충돌 409 · 지우기 · 되살리기 · mutationId (드라이브에 JSON 파일)', async () => {
  const id = `cue-${run}`;
  const v1 = { id, name: '주일 예배', serviceDate: '2026-11-29', items: [{ type: 'title', text: '환영' }] };
  let r = await put(ownerA, 'cue', id, v1);
  assert.equal(r.status, 201, r.text);
  assert.equal(r.json.revision, 1);
  let files = await tree(alice.email);
  const f = files.find(x => x.path === `워십프리젠터/${alice.name}/큐시트/주일 예배 (${id}).json`);
  assert.ok(f, JSON.stringify(files.map(x => x.path)));
  const content = JSON.parse(Buffer.from(await (await fetch(`${G}/_fake/file?id=${f.id}`)).arrayBuffer()).toString());
  assert.equal(content.worshipPresenter, 1);
  assert.equal(content.revision, 1);
  assert.deepEqual(content.payload, v1);
  r = await get(ownerA, 'cue', id);
  assert.equal(r.status, 200);
  assert.equal(r.headers.get('etag'), '"1"');
  assert.deepEqual(r.json.payload, v1);
  assert.equal(r.json.title, '주일 예배');
  // 고치기 · 충돌
  r = await put(ownerA, 'cue', id, { ...v1, name: '주일 예배 (고침)' }, 1);
  assert.equal(r.json.revision, 2);
  r = await put(ownerA, 'cue', id, { ...v1, name: '늦은 고침' }, 1);
  assert.equal(r.status, 409);
  assert.equal(r.json.conflict, true);
  assert.equal(r.json.current.revision, 2);
  assert.equal(r.json.current.payload.name, '주일 예배 (고침)');
  r = await put(ownerA, 'cue', id, v1);
  assert.equal(r.status, 409, '이미 있는데 새로 만들기');
  // 이름이 바뀌면 드라이브 파일 이름도
  files = await tree(alice.email);
  assert.ok(files.some(x => x.path.endsWith(`큐시트/주일 예배 (고침) (${id}).json`)));
  // mutationId: 같은 값으로 다시 와도 한 번만
  r = await put(ownerA, 'cue', id, { ...v1, name: '세 번째' }, 2, { 'x-mutation-id': `m-${run}-a` });
  assert.equal(r.json.revision, 3);
  r = await put(ownerA, 'cue', id, { ...v1, name: '세 번째' }, 2, { 'x-mutation-id': `m-${run}-a` });
  assert.equal(r.json.duplicate, true);
  assert.equal(r.json.revision, 3);
  // 지우기 (드라이브 휴지통) · 되살리기
  r = await del(ownerA, 'cue', id, 3);
  assert.equal(r.json.deleted, true);
  assert.equal(r.json.revision, 4);
  assert.equal((await tree(alice.email)).find(x => x.id === f.id).trashed, true);
  r = await get(ownerA, 'cue', id);
  assert.equal(r.status, 404);
  assert.equal(r.json.deleted, true);
  r = await del(ownerA, 'cue', id, 4);
  assert.equal(r.status, 404);
  r = await put(ownerA, 'cue', id, { ...v1, name: '되살림' }, 4);
  assert.equal(r.json.revision, 5);
  assert.equal((await tree(alice.email)).find(x => x.id === f.id).trashed, false);
  // 목록
  r = await req('/api/v1/documents?kind=cue', { cookie: ownerA });
  assert.ok(r.json.documents.some(d => d.id === id && d.revision === 5 && d.title === '되살림'));
  r = await req('/api/v1/documents?kind=cue&include=payload', { cookie: ownerA });
  assert.equal(r.json.documents.find(d => d.id === id).payload.name, '되살림');
  // 사람이 드라이브에서 아주 지우면(휴지통 비움) 410, 다음 쓰기는 새 파일로
  await fake('/_fake/purge-file', { id: f.id });
  r = await get(ownerA, 'cue', id);
  assert.equal(r.status, 410);
  assert.equal(r.json.code, 'drive_missing');
  r = await put(ownerA, 'cue', id, { ...v1, name: '다시 만듦' }, 5);
  assert.equal(r.json.revision, 6);
  assert.equal((await get(ownerA, 'cue', id)).json.payload.name, '다시 만듦');
});

test('무료: 동기화 받기 · 올리기 (판 · 순번 · 중복 방지 · superseded) 가 유료와 같은 모양', async () => {
  const head0 = (await req('/api/v1/sync?after=0&limit=1', { cookie: ownerA })).json.head;
  const muts = [
    { mutationId: `s-${run}-1`, kind: 'song', id: `song-${run}`, op: 'put', baseRevision: 0, payload: { title: '찬양 1', lyrics: '가사' } },
    { mutationId: `s-${run}-2`, kind: 'song', id: `song-${run}`, op: 'put', baseRevision: 1, payload: { title: '찬양 1 (고침)', lyrics: '가사' } },
    { mutationId: `s-${run}-3`, kind: 'theme', id: `th-${run}`, op: 'put', baseRevision: 0, payload: { name: '테마' } },
    { mutationId: `s-${run}-4`, kind: 'theme', id: `nope-${run}`, op: 'delete', baseRevision: 3 },
  ];
  let r = await req('/api/v1/sync', { method: 'POST', cookie: ownerA, headers: W, body: { mutations: muts } });
  assert.equal(r.status, 200, r.text);
  assert.deepEqual(r.json.results.map(x => x.status), ['applied', 'applied', 'applied', 'missing']);
  assert.equal(r.json.results[1].revision, 2);
  r = await req('/api/v1/sync', { method: 'POST', cookie: ownerA, headers: W, body: { mutations: muts.slice(0, 3) } });
  assert.deepEqual(r.json.results.map(x => x.status), ['duplicate', 'duplicate', 'duplicate']);
  r = await req('/api/v1/sync', { method: 'POST', cookie: ownerA, headers: W, body: { mutations: [{ mutationId: `s-${run}-5`, kind: 'song', id: `song-${run}`, op: 'put', baseRevision: 1, payload: { title: 'x' } }] } });
  assert.equal(r.json.results[0].status, 'conflict');
  assert.equal(r.json.results[0].current.payload.title, '찬양 1 (고침)');
  // 받기
  let after = head0, got = [];
  for (;;) {
    const p = await req(`/api/v1/sync?after=${after}`, { cookie: ownerA });
    got.push(...p.json.changes);
    after = p.json.next;
    if (!p.json.more) { assert.equal(p.json.next, p.json.head); break; }
  }
  const s1 = got.find(c => c.mutationId === `s-${run}-1`), s2 = got.find(c => c.mutationId === `s-${run}-2`);
  assert.equal(s1.superseded, true);
  assert.equal(s1.payload, undefined);
  assert.deepEqual(s2.payload, { title: '찬양 1 (고침)', lyrics: '가사' });
  assert.ok(s2.seq > s1.seq);
});

function bookBody(tag, b) {
  const shape = SHAPE.filter(x => x[0] === b);
  const n = Math.max(...shape.map(x => x[1]));
  const chapters = Array.from({ length: n }, (_, i) => { const s = shape.find(x => x[1] === i + 1); return s ? Array.from({ length: s[2] }, (_, v) => [v + 1, verseText(tag, b, i + 1, v + 1)]) : []; });
  return { chapters, headings: b === 43 ? [[3, 1, `[${tag}] 니고데모 (견본 소제목)`], [3, 16, `[${tag}] 하나님의 사랑 (견본 소제목)`]] : [] };
}

test('무료: 성경 넣기 (책 단위 드라이브 파일) → 장절은 유료와 같은 답, 두 번째부터 드라이브를 읽지 않음', async () => {
  let r = await req('/api/v1/bible/versions/FREEV', { method: 'PUT', cookie: ownerA, headers: W, body: { name: '무료 견본 번역', language: 'ko', licenseNote: '지어낸 본문' } });
  assert.equal(r.status, 201, r.text);
  for (const b of [1, 19, 43, 45]) {
    r = await req(`/api/v1/bible/versions/FREEV/books/${b}`, { method: 'PUT', cookie: ownerA, headers: W, body: bookBody('A', b) });
    assert.equal(r.status, 200, r.text);
  }
  const files = await tree(alice.email);
  assert.ok(files.some(f => f.path === `워십프리젠터/${alice.name}/성경/FREEV/43 요한복음.json`), JSON.stringify(files.map(f => f.path)));
  r = await req('/api/v1/bible/versions', { cookie: ownerA });
  assert.equal(r.json.versions.find(v => v.id === 'FREEV').verses, SHAPE.filter(x => [1, 19, 43, 45].includes(x[0])).reduce((n, x) => n + x[2], 0));
  const t0 = Date.now();
  r = await req(`/api/v1/bible/passage?version=FREEV&ref=${enc('요3:16-18')}`, { cookie: ownerA });
  const first = Date.now() - t0;
  assert.equal(r.status, 200, r.text);
  assert.deepEqual(r.json.passage.verses.map(v => v.verse), [16, 17, 18]);
  assert.equal(r.json.passage.verses[0].text, verseText('A', 43, 3, 16));
  assert.equal(r.json.passage.title, '요한복음 3:16-18');
  const reads = (await fake('/_fake/stats')).calls['GET /drive/v3/files/:id?media'] || 0;
  const t1 = Date.now();
  const again = await req(`/api/v1/bible/passage?version=FREEV&ref=${enc('요3:16-18')}`, { cookie: ownerA });
  const second = Date.now() - t1;
  assert.deepEqual(again.json, r.json);
  assert.equal((await fake('/_fake/stats')).calls['GET /drive/v3/files/:id?media'] || 0, reads, '두 번째는 Worker 기억에서');
  console.log(`  장절 첫 조회 ${first} ms · 두 번째 ${second} ms (로컬)`);
  assert.ok(second < 300, `두 번째 조회 ${second} ms`);
  // 장절 오류는 유료와 같은 422
  r = await req(`/api/v1/bible/passage?version=FREEV&ref=${enc('요3:99')}`, { cookie: ownerA });
  assert.equal(r.status, 422);
  assert.equal(r.json.error, '요한복음 3장은 36절까지 있습니다.');
  // 유료(sample)에 같은 견본(SAMPLE, 2A 시험이 넣음)이 있으면 답 모양까지 같은지
  const paid = await req(`/api/v1/bible/passage?version=SAMPLE&ref=${enc('요3:16-18')}`, { cookie: ownerPaid });
  if (paid.status === 200) {
    const strip = p => ({ ...p, versionId: '', versionName: '' });
    assert.deepEqual(strip(again.json.passage), strip(paid.json.passage));
  }
  // 등급 한도: 무료는 번역본 3개까지
  for (const v of ['FREEV2', 'FREEV3']) assert.equal((await req(`/api/v1/bible/versions/${v}`, { method: 'PUT', cookie: ownerA, headers: W, body: { name: v } })).status, 201);
  r = await req('/api/v1/bible/versions/FREEV4', { method: 'PUT', cookie: ownerA, headers: W, body: { name: 'x' } });
  assert.equal(r.status, 429);
  assert.equal(r.json.code, 'tier_limit');
  for (const v of ['FREEV2', 'FREEV3']) assert.equal((await req(`/api/v1/bible/versions/${v}`, { method: 'DELETE', cookie: ownerA, headers: W })).status, 200);
});

test('무료: 웹 조작 화면의 책 받기(/api/v1/control/bible/book)도 드라이브 책 파일에서 (6단계 덧붙임)', async () => {
  let r = await req('/api/v1/control/bible/book?version=FREEV&book=43', { cookie: ownerA });
  assert.equal(r.status, 200, r.text);
  const want = SHAPE.filter(x => x[0] === 43).reduce((n, x) => n + x[2], 0);
  assert.equal(r.json.verses.length, want);
  const v316 = r.json.verses.find(x => x[0] === 3 && x[1] === 16);
  assert.equal(v316[2], verseText('A', 43, 3, 16));
  assert.deepEqual(r.json.headings.map(h => h.slice(0, 2)), [[3, 1], [3, 16]]);
  assert.ok(r.json.importedAt);
  // 넣지 않은 책 · 없는 번역본
  assert.equal((await req('/api/v1/control/bible/book?version=FREEV&book=2', { cookie: ownerA })).status, 404);
  assert.equal((await req('/api/v1/control/bible/book?version=NOPE&book=43', { cookie: ownerA })).status, 404);
});

// 1×1 PNG
const PNG = Buffer.from('89504e470d0a1a0a0000000d49484452000000010000000108060000001f15c4890000000d4944415478da63f8cfc0f01f0005000201a5f3c2d90000000049454e44ae426082', 'hex');

test('무료: 그림 · 글꼴은 드라이브 \'파일/\' 에 원래 형식 그대로 · Range · sha256 확인 · fonts.css · 나눠 올리기 없음', async () => {
  const up = (bytes, type, name, extra = {}) => bin('/api/v1/assets', { method: 'POST', cookie: ownerA, headers: { ...W, 'content-type': type, 'x-asset-sha256': sha(bytes), 'x-asset-name': enc(name), ...extra }, body: bytes });
  const png = Buffer.concat([PNG, Buffer.from(run)]); // 실행마다 다른 내용
  let r = await up(png, 'image/png', '환영.png');
  assert.equal(r.status, 201, r.text);
  const id = r.json.id;
  const files = await tree(alice.email);
  const df = files.find(f => f.path === `워십프리젠터/${alice.name}/파일/그림/환영.png`);
  assert.ok(df, JSON.stringify(files.map(f => f.path)));
  assert.equal(df.size, png.length);
  const raw = Buffer.from(await (await fetch(`${G}/_fake/file?id=${df.id}`)).arrayBuffer());
  assert.equal(sha(raw), sha(png));
  // 받기 · Range
  let g = await fetch(`${BASE}/api/v1/assets/${id}`, { headers: { cookie: ownerA } });
  assert.equal(g.status, 200);
  assert.equal(sha(Buffer.from(await g.arrayBuffer())), sha(png));
  g = await fetch(`${BASE}/api/v1/assets/${id}`, { headers: { cookie: ownerA, range: 'bytes=0-3' } });
  assert.equal(g.status, 206);
  assert.deepEqual(Buffer.from(await g.arrayBuffer()), png.subarray(0, 4));
  // 같은 것 다시 → existed (드라이브에 둘째 파일을 만들지 않음)
  r = await up(png, 'image/png', '환영.png');
  assert.equal(r.json.existed, true);
  assert.equal((await tree(alice.email)).filter(f => f.path.endsWith('파일/그림/환영.png')).length, 1);
  // sha256 이 다르면 400 + 드라이브에 남지 않음
  const bad = Buffer.concat([PNG, Buffer.from('x' + run)]);
  r = await bin('/api/v1/assets', { method: 'POST', cookie: ownerA, headers: { ...W, 'content-type': 'image/png', 'x-asset-sha256': sha(png.subarray(1)), 'x-asset-name': enc('나쁜.png') }, body: bad });
  assert.equal(r.status, 400);
  assert.equal(r.json.reason, 'sha256');
  assert.ok(!(await tree(alice.email)).some(f => f.path.endsWith('나쁜.png')));
  // 글꼴 (WOFF2 는 안을 읽지 않으므로 이름 · 굵기를 적어 보냄)
  const font = Buffer.from(`wOF2-fake-font-${run}`);
  r = await up(font, 'font/woff2', '구세군체.woff2', { 'x-asset-font-family': enc('구세군체'), 'x-asset-font-weight': '700' });
  assert.equal(r.status, 201, r.text);
  assert.ok((await tree(alice.email)).some(f => f.path === `워십프리젠터/${alice.name}/파일/글꼴/구세군체.woff2`));
  const css = await req('/api/v1/assets/fonts.css', { cookie: ownerA });
  assert.match(css.text, /font-family:"구세군체"/);
  // 나눠 올리기는 무료 등급에 없다
  r = await req('/api/v1/assets/uploads', { method: 'POST', cookie: ownerA, headers: { ...W, 'content-type': 'video/mp4', 'x-asset-sha256': sha('v'), 'x-asset-name': 'a.mp4', 'x-asset-size': String(200 * 1024 * 1024) } });
  assert.equal(r.status, 403);
  assert.equal(r.json.code, 'tier_blocked');
  // 지우기 표시 → 정리(실제로 지움) = 드라이브 휴지통
  r = await req(`/api/v1/assets/${id}`, { method: 'DELETE', cookie: ownerA, headers: W });
  assert.equal(r.json.deleted, true);
  r = await req('/api/v1/assets/purge', { method: 'POST', cookie: ownerA, headers: W, body: { olderThanDays: 0, confirm: 'now', dryRun: false } });
  assert.equal(r.status, 200, r.text);
  assert.equal((await tree(alice.email)).find(f => f.id === df.id).trashed, true);
  alice.font = { id: (await req('/api/v1/assets?kind=font', { cookie: ownerA })).json.assets[0].id, bytes: font };
});

test('무료: 드라이브가 가득 차면 507 drive_quota · 구글이 막으면 429 drive_rate', async () => {
  const u = await fake(`/_fake/user?email=${enc(alice.email)}`);
  await fake('/_fake/quota', { email: alice.email, bytes: u.usage + 10 });
  let r = await put(ownerA, 'song', `big-${run}`, { title: '큰 찬양', lyrics: 'x'.repeat(2000) });
  assert.equal(r.status, 507, r.text);
  assert.equal(r.json.code, 'drive_quota');
  assert.equal((await get(ownerA, 'song', `big-${run}`)).status, 404, '실패한 만들기는 남지 않음');
  await fake('/_fake/quota', { email: alice.email, bytes: 15e9 });
  r = await put(ownerA, 'song', `big-${run}`, { title: '큰 찬양', lyrics: 'x'.repeat(2000) });
  assert.equal(r.status, 201, '다시 하면 됨');
  await fake('/_fake/rate', { email: alice.email, n: 1 });
  r = await get(ownerA, 'song', `big-${run}`);
  assert.equal(r.status, 429);
  assert.equal(r.json.code, 'drive_rate');
  assert.equal((await get(ownerA, 'song', `big-${run}`)).status, 200);
});

test('무료: 구글 연결이 끊기면 503 drive_disconnected + 다시 연결 안내, 송출 중인 화면은 마지막 그대로 · 다시 연결하면 이어서', async () => {
  // 설치판 연결 열쇠 (무료도 1단계 중계는 그대로)
  const c = await req('/api/v1/credentials', { method: 'POST', cookie: ownerA, headers: W, body: { role: 'bridge', label: '시험 설치판' } });
  assert.equal(c.status, 200, c.text);
  const bridge = await fakeBridge({ key: c.json.credential.key });
  const view = socket('/events?client=main', { cookie: ownerA });
  await view.opened;
  await view.next('hello');
  bridge.send({ t: 'render', client: 'main', data: { version: 7, text: '송출 중인 화면' } });
  assert.equal((await view.next('render')).data.text, '송출 중인 화면');
  // 사용자가 구글 계정 설정에서 권한을 거둠 (+ 접근 토큰 만료)
  await fake('/_fake/revoke', { email: alice.email });
  await fake('/_fake/expire', { email: alice.email });
  const r = await get(ownerA, 'song', `big-${run}`);
  assert.equal(r.status, 503, r.text);
  assert.equal(r.json.code, 'drive_disconnected');
  assert.equal(r.json.reconnect, '/auth/google/start?mode=connect');
  assert.equal((await req('/api/v1/tier', { cookie: ownerA })).json.drive.connected, false);
  // 화면은 그대로 · 새로 붙는 화면도 마지막 것을 받는다
  await sleep(300);
  assert.equal(view.closed, null);
  const view2 = socket('/events?client=main', { cookie: ownerA });
  await view2.opened;
  assert.equal((await view2.next('render')).data.text, '송출 중인 화면');
  // 다른 구글 계정으로는 다시 연결하지 못한다 (drive.file 이라 앞의 파일이 안 보임)
  const other = await google('connect', { email: `mallory-${run}@example.com`, cookie: ownerA });
  assert.match(other.location, /\/login\?google=other-account$/);
  // 같은 계정으로 다시 연결
  const again = await google('connect', { email: alice.email, cookie: ownerA });
  assert.match(again.location, /\/owner$/);
  assert.equal((await get(ownerA, 'song', `big-${run}`)).status, 200);
  assert.equal((await req('/api/v1/drive', { cookie: ownerA })).json.connected, true);
  await view.close(); await view2.close(); await bridge.close();
  alice.bridgeKey = c.json.credential.key;
});

test('교회 분리: 무료끼리 · 무료 ↔ 유료 (남의 문서 · 목록 · 동기화 · 성경 · 파일 · 드라이브 파일에 닿지 않음)', async () => {
  const r = await google('signup', bob);
  ownerB = r.cookie;
  bob.church = (await req('/api/v1/me', { cookie: ownerB })).json.church;
  assert.notEqual(bob.church, alice.church);
  const before = (await fake('/_fake/stats')).crossUser;
  // 같은 kind/id 로 만들면 각자 교회에 따로
  assert.equal((await put(ownerB, 'cue', `cue-${run}`, { name: '밥의 큐시트' })).status, 201);
  assert.equal((await get(ownerA, 'cue', `cue-${run}`)).json.payload.name, '다시 만듦');
  assert.equal((await get(ownerB, 'cue', `cue-${run}`)).json.payload.name, '밥의 큐시트');
  assert.equal((await get(ownerB, 'song', `song-${run}`)).status, 404);
  assert.ok(!(await req('/api/v1/documents', { cookie: ownerB })).json.documents.some(d => d.id === `song-${run}`));
  assert.ok(!(await req('/api/v1/sync?after=0&limit=500', { cookie: ownerB })).json.changes.some(c => c.id === `song-${run}`));
  assert.equal((await req(`/api/v1/bible/passage?version=FREEV&ref=${enc('요3:16')}`, { cookie: ownerB })).status, 404);
  assert.equal((await req(`/api/v1/assets/${alice.font.id}`, { cookie: ownerB })).status, 404);
  // 유료 견본 교회와도
  assert.equal((await get(ownerPaid, 'song', `song-${run}`)).status, 404);
  assert.equal((await req(`/api/v1/assets/${alice.font.id}`, { cookie: ownerPaid })).status, 404);
  assert.ok(!(await req('/api/v1/documents?kind=cue', { cookie: ownerA })).json.documents.some(d => d.title === '밥의 큐시트'));
  // 드라이브 파일은 각자 구글 계정에만 (Worker 가 남의 토큰으로 남의 파일을 부른 적 없음)
  assert.ok((await tree(bob.email)).every(f => f.path === '워십프리젠터' || f.path.startsWith('워십프리젠터/')));
  assert.ok(!(await tree(bob.email)).some(f => f.path.includes(alice.name)));
  assert.equal((await fake('/_fake/stats')).crossUser, before);
  // 다른 교회 설치판 열쇠의 앞머리를 바꿔 끼우면 401
  const forged = `${bob.church}.${alice.bridgeKey.split('.')[1]}`;
  assert.equal((await req('/api/v1/documents', { headers: { authorization: `Bearer ${forged}` }, ip: newIp() })).status, 401);
});

test('데모: 둘러보기 · 견본 자료 · 막힌 기능 · 방문자마다 중계실 · 남의 자료 안 보임 · 되돌리기 · 요청 한도', async () => {
  const ipA = newIp();
  let r = await req('/demo', { ip: ipA });
  assert.equal(r.status, 302, r.text);
  const A = cookieOf(r.headers);
  r = await req('/demo', { cookie: A, ip: ipA });
  assert.equal(r.status, 200);
  assert.match(r.text, /둘러보기/);
  const B = cookieOf((await req('/demo', { ip: newIp() })).headers);
  const t = await req('/api/v1/tier', { cookie: A, ip: ipA });
  assert.equal(t.json.tier, 'demo');
  assert.match(t.json.demo.note, /개인 정보/);
  r = await req('/api/v1/documents', { cookie: A, ip: ipA });
  assert.ok(r.json.documents.some(d => d.id === 'demo-cue-1'));
  r = await req(`/api/v1/bible/passage?version=DEMO&ref=${enc('요3:16')}`, { cookie: A, ip: ipA });
  assert.equal(r.status, 200, r.text);
  assert.match(r.json.passage.verses[0].text, /견본 본문/);
  // 고쳐 보기는 된다
  const mine = `demo-try-${run}`;
  assert.equal((await put(A, 'cue', mine, { name: '내가 만든 것' })).status, 201);
  const cue = (await get(A, 'cue', 'demo-cue-1')).json;
  assert.equal((await put(A, 'cue', 'demo-cue-1', { ...cue.payload, name: '고친 견본' }, cue.revision)).status, 200);
  // 막힌 것
  r = await bin('/api/v1/assets', { method: 'POST', cookie: A, headers: { ...W, 'content-type': 'image/png', 'x-asset-sha256': sha(PNG), 'x-asset-name': 'a.png' }, body: PNG });
  assert.equal(r.status, 403); assert.equal(r.json.code, 'tier_blocked');
  assert.equal((await req('/api/v1/ai/settings', { cookie: A })).status, 403);
  assert.equal((await req('/api/v1/credentials', { cookie: A })).status, 403);
  assert.equal((await req('/api/v1/members', { cookie: A })).status, 403);
  assert.equal((await req('/api/v1/bible/versions/X', { method: 'PUT', cookie: A, headers: W, body: { name: 'x' } })).status, 403);
  assert.equal((await req('/owner', { cookie: A })).status, 403);
  // 남의 교회 자료
  assert.equal((await get(A, 'cue', `cue-${run}`)).status, 404);
  assert.equal((await get(A, 'song', `song-${run}`)).status, 404);
  assert.equal((await req(`/api/v1/assets/${alice.font.id}`, { cookie: A })).status, 404);
  assert.equal((await req(`/api/v1/bible/passage?version=FREEV&ref=${enc('요3:16')}`, { cookie: A })).status, 404);
  // 방문자마다 중계실: A 의 송출은 A 의 화면에만
  const srcA = socket('/bridge', { cookie: A });
  await srcA.opened; await srcA.next('ready');
  srcA.send({ t: 'hello', v: 1, app: '웹 조작 화면(데모)', lang: 'ko', outputs: ['main'], caps: {} });
  const vA = socket('/events?client=main', { cookie: A }); await vA.opened;
  const vB = socket('/events?client=main', { cookie: B }); await vB.opened;
  assert.equal((await vB.next('hello')).online, false);
  await vA.next('hello');
  srcA.send({ t: 'render', client: 'main', data: { version: 1, text: 'A 의 화면' } });
  assert.equal((await vA.next('render')).data.text, 'A 의 화면');
  await assert.rejects(vB.next('render', 800));
  // 출력 화면 수 한도 (데모 3)
  const more = [];
  const opened = [];
  for (let i = 0; i < 3; i++) { const v = socket('/events?client=main', { cookie: A }); more.push(v); opened.push(await v.opened.then(() => 'ok', e => e.status)); }
  assert.deepEqual(opened, ['ok', 'ok', 429], '넷째 화면은 막힘 (데모는 출력 3)');
  for (const v of [srcA, vA, vB, ...more]) await v.close().catch(() => {});
  // 되돌리기 (관리자 · 또는 하루가 지나 들어올 때)
  r = await admin('/api/v1/admin/demo/reset');
  assert.equal(r.status, 200, r.text);
  assert.equal(r.json.reset, true);
  assert.equal((await get(A, 'cue', mine)).status, 404);
  assert.equal((await get(A, 'cue', 'demo-cue-1')).json.payload.name, '견본 주일 예배');
  // 되돌림도 변경 기록에 남는다 (동기화가 따라옴)
  const ch = (await req('/api/v1/sync?after=0&limit=500', { cookie: A })).json.changes;
  assert.ok(ch.some(c => c.id === mine && c.op === 'delete'));
  // 주소마다 1분 요청 한도 (시험은 40으로 줄임)
  const ip = newIp();
  let limited = null;
  for (let i = 0; i < 60 && !limited; i++) { const x = await req('/api/v1/tier', { cookie: B, ip }); if (x.status === 429) limited = x; }
  assert.ok(limited, '1분 한도');
  assert.equal(limited.json.code, 'tier_limit');
  assert.equal(limited.json.limit, 'perIpPerMinute');
});

test('유료: 지금 그대로 (D1 · R2) — 등급 표시만 더해짐', async () => {
  const t = await req('/api/v1/tier', { cookie: ownerPaid });
  assert.equal(t.json.tier, 'paid');
  assert.equal(t.json.store, 'd1');
  const id = `paid-${run}`;
  assert.equal((await put(ownerPaid, 'song', id, { title: '유료 찬양' })).status, 201);
  assert.equal((await get(ownerPaid, 'song', id)).json.payload.title, '유료 찬양');
});

// 로컬 D1 에 바로 (쓰다가 끊긴 상태를 만들려고). wrangler dev 와 같은 파일이라 드물게 잠김 → 세 번까지
function d1(sql) {
  for (let i = 0; ; i++) {
    try { return execFileSync(process.execPath, [join('node_modules', 'wrangler', 'bin', 'wrangler.js'), 'd1', 'execute', 'worshippresenter-web', '--local', '--command', sql], { cwd: cloud, stdio: ['ignore', 'pipe', 'pipe'], env: { ...process.env, WRANGLER_SEND_METRICS: 'false' } }).toString(); } catch (e) { if (i >= 2) throw e; }
  }
}

test('무료: 쓰다가 Worker 가 죽은 경우 — 드라이브에 썼으면 다음 요청이 마무리, 안 썼으면 풀기, 쓰는 중이면 409 drive_busy', { skip: !/127.0.0.1/.test(BASE) }, async () => {
  const q = t => `'${String(t).replace(/'/g, "''")}'`;
  const mk = async id => { assert.equal((await put(ownerA, 'cue', id, { name: `처음 ${id}` })).status, 201); return (await tree(alice.email)).find(f => f.path.endsWith(`(${id}).json`)); };
  // 가: 드라이브에는 판 2 를 썼는데 D1 마무리 전에 죽음
  const a = `rec-a-${run}`, fa = await mk(a);
  await fake('/_fake/edit-file', { id: fa.id, content: JSON.stringify({ worshipPresenter: 1, kind: 'cue', id: a, revision: 2, updatedAt: 1, updatedBy: 'x', deleted: false, payload: { name: '끊긴 쓰기' } }) });
  d1(`UPDATE drive_docs SET pend_token = 'crash-a', pend_at = 1, pend_op = 'put', pend_mut = ${q('rec-' + run + '-m')}, pend_title = '끊긴 쓰기', pend_size = 20 WHERE church_id = ${q(alice.church)} AND kind = 'cue' AND id = ${q(a)}`);
  let r = await get(ownerA, 'cue', a);
  assert.equal(r.status, 200, r.text);
  assert.equal(r.json.revision, 2);
  assert.equal(r.json.payload.name, '끊긴 쓰기');
  const ch = (await req('/api/v1/sync?after=0&limit=500', { cookie: ownerA })).json.changes.filter(c => c.id === a);
  assert.deepEqual(ch.map(c => [c.revision, c.mutationId]), [[1, null], [2, `rec-${run}-m`]], '변경 기록도 정확히 한 줄씩');
  assert.equal((await put(ownerA, 'cue', a, { name: '이어서' }, 2)).json.revision, 3);
  // 나: 잡기만 하고 드라이브에 못 쓰고 죽음 → 풀고 다시 씀
  const b = `rec-b-${run}`;
  await mk(b);
  d1(`UPDATE drive_docs SET pend_token = 'crash-b', pend_at = 1, pend_op = 'put', pend_title = 'x', pend_size = 1 WHERE church_id = ${q(alice.church)} AND kind = 'cue' AND id = ${q(b)}`);
  r = await put(ownerA, 'cue', b, { name: '다시' }, 1);
  assert.equal(r.status, 200, r.text);
  assert.equal(r.json.revision, 2);
  // 다: 지금 쓰는 중 (30초 안) → 409 drive_busy + 최신
  d1(`UPDATE drive_docs SET pend_token = 'busy', pend_at = ${Date.now()}, pend_op = 'put', pend_title = 'x', pend_size = 1 WHERE church_id = ${q(alice.church)} AND kind = 'cue' AND id = ${q(b)}`);
  r = await put(ownerA, 'cue', b, { name: '겹침' }, 2);
  assert.equal(r.status, 409);
  assert.equal(r.json.code, 'drive_busy');
  assert.equal(r.json.current.revision, 2);
  d1(`UPDATE drive_docs SET pend_token = NULL WHERE church_id = ${q(alice.church)} AND kind = 'cue' AND id = ${q(b)}`);
});

async function moveAll(church, to) {
  let r = await admin(`/api/v1/admin/churches/${church}/move`, { to });
  assert.equal(r.status, 200, r.text);
  for (let i = 0; i < 200; i++) {
    r = await admin(`/api/v1/admin/churches/${church}/move/step`);
    if (r.status === 425) { await sleep(200); continue; }
    assert.equal(r.status, 200, r.text);
    if (r.json.done) return r.json;
  }
  throw new Error('옮기기가 끝나지 않음');
}

test('옮기기: 무료 → 유료 (드라이브 → D1 · R2, 판 · 순번 그대로) → 다시 무료', async () => {
  const docsBefore = (await req('/api/v1/documents?include=payload&limit=40', { cookie: ownerA })).json.documents;
  const syncBefore = (await req('/api/v1/sync?after=0&limit=500', { cookie: ownerA })).json;
  const verse = (await req(`/api/v1/bible/passage?version=FREEV&ref=${enc('롬8:28')}`, { cookie: ownerA })).json;
  let r = await admin(`/api/v1/admin/churches/${alice.church}/move`, { to: 'paid' });
  assert.equal(r.status, 200, r.text);
  // 옮기는 동안: 쓰기 423 · 읽기 됨
  r = await put(ownerA, 'song', `during-${run}`, { title: 'x' });
  assert.equal(r.status, 423); assert.equal(r.json.code, 'tier_moving');
  assert.equal((await get(ownerA, 'cue', `cue-${run}`)).status, 200);
  await admin(`/api/v1/admin/churches/${alice.church}/move/cancel`);
  const done = await moveAll(alice.church, 'paid');
  assert.equal(done.tier, 'paid');
  assert.equal((await req('/api/v1/tier', { cookie: ownerA })).json.store, 'd1');
  const docsAfter = (await req('/api/v1/documents?include=payload&limit=40', { cookie: ownerA })).json.documents;
  assert.deepEqual(docsAfter.map(d => [d.kind, d.id, d.revision, d.payload]), docsBefore.map(d => [d.kind, d.id, d.revision, d.payload]));
  const syncAfter = (await req('/api/v1/sync?after=0&limit=500', { cookie: ownerA })).json;
  assert.equal(syncAfter.head, syncBefore.head, '변경 순번이 이어짐');
  assert.deepEqual(syncAfter.changes.map(c => [c.seq, c.id, c.revision, c.superseded || false]), syncBefore.changes.map(c => [c.seq, c.id, c.revision, c.superseded || false]));
  assert.deepEqual((await req(`/api/v1/bible/passage?version=FREEV&ref=${enc('롬8:28')}`, { cookie: ownerA })).json, verse);
  const f = await fetch(`${BASE}/api/v1/assets/${alice.font.id}`, { headers: { cookie: ownerA } });
  assert.equal(f.status, 200);
  assert.equal(sha(Buffer.from(await f.arrayBuffer())), sha(alice.font.bytes));
  // 유료에서 고친 것
  const cue = (await get(ownerA, 'cue', `cue-${run}`)).json;
  assert.equal((await put(ownerA, 'cue', `cue-${run}`, { ...cue.payload, name: '유료에서 고침' }, cue.revision)).json.revision, cue.revision + 1);
  // 다시 무료로 (D1 → 드라이브)
  const back = await moveAll(alice.church, 'free');
  assert.equal(back.tier, 'free');
  const c2 = await get(ownerA, 'cue', `cue-${run}`);
  assert.equal(c2.json.payload.name, '유료에서 고침');
  assert.equal(c2.json.revision, cue.revision + 1);
  assert.ok((await tree(alice.email)).some(x => x.path.endsWith(`큐시트/유료에서 고침 (cue-${run}).json`) && !x.trashed));
  assert.deepEqual((await req(`/api/v1/bible/passage?version=FREEV&ref=${enc('롬8:28')}`, { cookie: ownerA })).json, verse);
  const f2 = await fetch(`${BASE}/api/v1/assets/${alice.font.id}`, { headers: { cookie: ownerA } });
  assert.equal(sha(Buffer.from(await f2.arrayBuffer())), sha(alice.font.bytes));
  // 정리 (유료 쪽 D1 본문 · R2)
  r = await admin(`/api/v1/admin/churches/${alice.church}/move/cleanup`);
  assert.equal(r.status, 200, r.text);
  assert.equal((await get(ownerA, 'cue', `cue-${run}`)).json.payload.name, '유료에서 고침');
  const info = (await admin(`/api/v1/admin/churches/${alice.church}`, undefined, 'GET')).json;
  assert.equal(info.counts.d1Documents, 0);
});
