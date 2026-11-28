// 끝에서 끝 (2단계 C): R2 파일 올리기 · 받기 · Range · 같은 파일 · 나눠 올리기 · 지우기 · 정리 · 글꼴 · 안전장치 · 교회 분리.
// scripts/e2e.mjs 가 wrangler dev(로컬 D1 · 로컬 R2 흉내)를 띄운 뒤 부른다. 원격 저장통은 쓰지 않는다.
// 두 교회: 'sample'(1단계 견본) · 'assets-other'(여기서 만듦). 파일은 시험 안에서 지어낸 바이트다.
import test, { before } from 'node:test';
import assert from 'node:assert/strict';
import { createHash, randomBytes } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { BASE, CHURCH, cred, req, login, cookieOf, newIp } from '../e2e/helpers.mjs';

const cloud = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const OTHER = 'assets-other';
const W = { 'x-worshipcue': '1' };
const MiB = 1024 * 1024;
let other, ownerA, ownerB, output, remote;

const sha = b => createHash('sha256').update(b).digest('hex');
/** 실행마다 다른 내용 (로컬 D1 · R2 는 실행 사이에 남는다) */
const bytes = (n, tag = '') => { const b = randomBytes(n); b.write(`${tag}${Date.now()}`, 0); return b; };

/** 이진 요청 (helpers.req 는 답을 글로 읽는다) */
async function bin(path, { method = 'GET', cookie, headers = {}, body, redirect = 'manual' } = {}) {
  const h = { ...headers };
  if (cookie) h.cookie = cookie;
  const r = await fetch(BASE + path, { method, headers: h, body, redirect });
  const buf = Buffer.from(await r.arrayBuffer());
  let json = null;
  try { json = JSON.parse(buf.toString('utf8')); } catch { /* 파일 */ }
  return { status: r.status, headers: r.headers, buf, json };
}
const upload = (who, body, { name = 'x.png', type = 'image/png', hash = sha(body), extra = {} } = {}) => bin('/api/v1/assets', {
  method: 'POST', body,
  ...(who.bearer ? { headers: { authorization: `Bearer ${who.bearer}`, 'content-type': type, 'x-asset-sha256': hash, 'x-asset-name': encodeURIComponent(name), ...extra } }
    : { cookie: who, headers: { ...W, 'content-type': type, 'x-asset-sha256': hash, 'x-asset-name': encodeURIComponent(name), ...extra } }),
});
const wr = (...a) => execFileSync(process.execPath, [join(cloud, 'node_modules', 'wrangler', 'bin', 'wrangler.js'), ...a], { cwd: cloud, stdio: ['ignore', 'pipe', 'inherit'], env: { ...process.env, WRANGLER_SEND_METRICS: 'false' } });
const sql = command => { for (let i = 0; ; i++) { try { return wr('d1', 'execute', 'worshippresenter-web', '--local', '--command', command); } catch (e) { if (i >= 2) throw e; } } };

async function loginAs(church, secret) {
  const r = await req('/auth/login', { method: 'POST', body: { church, secret }, headers: W, ip: newIp() });
  assert.equal(r.status, 200, r.text);
  return cookieOf(r.headers);
}

before(async () => {
  if (!existsSync(join(cloud, '.secrets', `${OTHER}.json`))) execFileSync(process.execPath, [join('scripts', 'seed.mjs'), '--church', OTHER, '--name', '파일 시험 교회'], { cwd: cloud, stdio: 'ignore' });
  other = JSON.parse(readFileSync(join(cloud, '.secrets', `${OTHER}.json`), 'utf8'));
  ownerA = await login('owner');
  ownerB = await loginAs(OTHER, other.credentials.owner.secret);
  output = await login('output');
  // 리모컨 PIN 쿠키 (1단계 길로 로그인)
  const r = await req(`/r/state?pin=${cred('remote').secret}&c=${CHURCH}`, { ip: newIp() });
  remote = cookieOf(r.headers);
  assert.ok(remote.includes('wp_session'), r.text);
}, { timeout: 120_000 });

test('로그인 없이는 아무것도 못 한다 (받기 · 목록 · 올리기 · 글꼴 CSS)', async () => {
  const body = bytes(100);
  const up = await upload(ownerA, body);
  assert.equal(up.status, 201, up.buf.toString());
  for (const p of [`/api/v1/assets/${up.json.id}`, '/api/v1/assets', '/api/v1/assets/fonts.css', '/api/v1/assets/usage', `/api/v1/assets?sha256=${sha(body)}`]) {
    assert.equal((await bin(p)).status, 401, p);
  }
  assert.equal((await bin('/api/v1/assets', { method: 'POST', body, headers: { 'content-type': 'image/png', 'x-asset-sha256': sha(body) } })).status, 401);
  assert.equal((await bin(`/api/v1/assets/${up.json.id}`, { cookie: 'wp_session=v1.x.y' })).status, 401);
  // 쿠키로 오는 쓰기는 X-WorshipCue · 같은 Origin
  assert.equal((await bin('/api/v1/assets', { method: 'POST', cookie: ownerA, body, headers: { 'content-type': 'image/png', 'x-asset-sha256': sha(body) } })).status, 403);
  assert.equal((await bin('/api/v1/assets', { method: 'POST', cookie: ownerA, body, headers: { ...W, origin: 'https://evil.example', 'content-type': 'image/png', 'x-asset-sha256': sha(body) } })).status, 403);
});

test('한 번에 올리기 · 같은 파일 두 번 · sha256 이 틀리면 400 · 받는 종류만', async () => {
  const body = bytes(200_000, 'png');
  const a = await upload(ownerA, body, { name: 'C:\\Users\\김택근\\Pictures\\부활절 배경.png' });
  assert.equal(a.status, 201, a.buf.toString());
  assert.equal(a.json.existed, false);
  assert.equal(a.json.sha256, sha(body));
  assert.equal(a.json.size, body.length);
  assert.equal(a.json.type, 'image/png');
  assert.equal(a.json.asset.name, '부활절 배경.png', 'PC 경로 · 사용자 이름은 남지 않는다');
  assert.equal(a.json.asset.verified, true);
  assert.match(a.json.id, /^[A-Za-z0-9]{16}$/);
  // 같은 파일 → 같은 id, 본문을 버린다
  const b = await upload(ownerA, body, { name: '다른 이름.png' });
  assert.equal(b.status, 200);
  assert.equal(b.json.existed, true);
  assert.equal(b.json.id, a.json.id);
  // 있나 묻기
  const look = await req(`/api/v1/assets?sha256=${sha(body)}`, { cookie: ownerA });
  assert.equal(look.json.asset.id, a.json.id);
  assert.equal((await req(`/api/v1/assets?sha256=${'0'.repeat(64)}`, { cookie: ownerA })).json.asset, null);
  // sha256 이 다르면 R2 가 거절 → 400, 아무것도 남지 않음
  const wrong = bytes(1000);
  const bad = await upload(ownerA, wrong, { hash: sha(Buffer.from('다른 내용')) });
  assert.equal(bad.status, 400, bad.buf.toString());
  assert.equal(bad.json.reason, 'sha256');
  assert.equal((await req(`/api/v1/assets?sha256=${sha(Buffer.from('다른 내용'))}`, { cookie: ownerA })).json.asset, null);
  // 머리글 · 종류
  assert.equal((await upload(ownerA, wrong, { hash: 'abc' })).status, 400);
  assert.equal((await upload(ownerA, wrong, { name: 'x.html', type: 'text/html' })).status, 415);
  assert.equal((await upload(ownerA, wrong, { name: 'x.mp4', type: 'image/png' })).status, 415);
  assert.equal((await upload(ownerA, wrong, { name: 'x.png', extra: { 'x-asset-font-family': 'x' } })).status, 400);
  // 확장자로 종류 정하기
  const oc = bytes(500);
  const o = await upload(ownerA, oc, { name: '그림.jpg', type: 'application/octet-stream' });
  assert.equal(o.json.type, 'image/jpeg');
});

test('받기: 출력 · 리모컨 PIN 도 id 로 받는다 · ETag 304 · HEAD · 캐시 머리말', async () => {
  const body = bytes(50_000);
  const { json: up } = await upload(ownerA, body, { name: '광고.webp', type: 'image/webp' });
  for (const c of [ownerA, output, remote]) {
    const r = await bin(`/api/v1/assets/${up.id}`, { cookie: c });
    assert.equal(r.status, 200);
    assert.ok(r.buf.equals(body));
    assert.equal(r.headers.get('content-type'), 'image/webp');
    assert.equal(r.headers.get('content-length'), String(body.length));
    assert.equal(r.headers.get('accept-ranges'), 'bytes');
    assert.equal(r.headers.get('etag'), `"${up.sha256}"`);
    assert.equal(r.headers.get('cache-control'), 'private, max-age=31536000, immutable');
    assert.equal(r.headers.get('x-content-type-options'), 'nosniff');
    assert.match(r.headers.get('content-security-policy'), /sandbox/);
    assert.match(r.headers.get('content-disposition'), /^inline; .*filename\*=UTF-8''%EA%B4%91%EA%B3%A0\.webp$/);
  }
  // 출력 · 리모컨은 목록 · 올리기 · 지우기 못 함
  for (const c of [output, remote]) {
    assert.equal((await bin('/api/v1/assets', { cookie: c })).status, 403);
    assert.equal((await upload(c, bytes(10))).status, 403);
    assert.equal((await bin(`/api/v1/assets/${up.id}`, { method: 'DELETE', cookie: c, headers: W })).status, 403);
    assert.equal((await bin('/api/v1/assets/fonts.css', { cookie: c })).status, 200);
  }
  const nm = await bin(`/api/v1/assets/${up.id}`, { cookie: output, headers: { 'if-none-match': `"${up.sha256}"` } });
  assert.equal(nm.status, 304);
  assert.equal(nm.buf.length, 0);
  const head = await bin(`/api/v1/assets/${up.id}`, { method: 'HEAD', cookie: output });
  assert.equal(head.status, 200);
  assert.equal(head.headers.get('content-length'), String(body.length));
  assert.equal((await bin('/api/v1/assets/AAAAAAAAAAAAAAAA', { cookie: ownerA })).status, 404);
});

test('Range: 영상 앞뒤로 넘기기 · 끝에서 n · 범위 밖 416 · If-Range 가 다르면 전체', async () => {
  const body = bytes(300_000);
  const { json: up } = await upload(ownerA, body, { name: '광고.mp4', type: 'video/mp4' });
  const p = `/api/v1/assets/${up.id}`;
  let r = await bin(p, { cookie: output, headers: { range: 'bytes=1000-1999' } });
  assert.equal(r.status, 206);
  assert.equal(r.headers.get('content-range'), `bytes 1000-1999/${body.length}`);
  assert.equal(r.headers.get('content-length'), '1000');
  assert.ok(r.buf.equals(body.subarray(1000, 2000)));
  r = await bin(p, { cookie: output, headers: { range: 'bytes=299000-' } });
  assert.equal(r.status, 206);
  assert.ok(r.buf.equals(body.subarray(299000)));
  r = await bin(p, { cookie: output, headers: { range: 'bytes=-10' } });
  assert.ok(r.buf.equals(body.subarray(body.length - 10)));
  r = await bin(p, { cookie: output, headers: { range: `bytes=${body.length}-` } });
  assert.equal(r.status, 416);
  assert.equal(r.headers.get('content-range'), `bytes */${body.length}`);
  r = await bin(p, { cookie: output, headers: { range: 'bytes=0-9', 'if-range': '"other"' } });
  assert.equal(r.status, 200);
  assert.equal(r.buf.length, body.length);
  r = await bin(p, { cookie: output, headers: { range: 'bytes=0-9', 'if-range': `"${up.sha256}"` } });
  assert.equal(r.status, 206);
});

test('교회 분리: 남의 열쇠로 남의 파일을 받지도 · 찾지도 · 지우지도 못한다', async () => {
  const body = bytes(10_000);
  const { json: a } = await upload(ownerA, body, { name: 'sample 것.png' });
  const outB = await loginAs(OTHER, other.credentials.output.secret);
  for (const c of [ownerB, outB]) {
    assert.equal((await bin(`/api/v1/assets/${a.id}`, { cookie: c })).status, 404);
    assert.equal((await bin(`/api/v1/assets/${a.id}`, { cookie: c, headers: { range: 'bytes=0-1' } })).status, 404);
  }
  assert.equal((await req(`/api/v1/assets?sha256=${a.sha256}`, { cookie: ownerB })).json.asset, null);
  assert.ok(!(await req('/api/v1/assets?limit=200', { cookie: ownerB })).json.assets.some(x => x.id === a.id));
  assert.equal((await req(`/api/v1/assets/${a.id}`, { method: 'DELETE', cookie: ownerB, headers: W })).status, 404);
  assert.equal((await req(`/api/v1/assets/${a.id}`, { method: 'PATCH', cookie: ownerB, headers: W, body: { name: 'x.png' } })).status, 404);
  // 같은 내용을 다른 교회가 올리면 자기 교회에 따로 (남의 것을 가리키지 않음)
  const b = await upload(ownerB, body, { name: '다른 교회 것.png' });
  assert.equal(b.status, 201);
  assert.equal(b.json.existed, false);
  assert.notEqual(b.json.id, a.id);
  assert.equal((await bin(`/api/v1/assets/${b.json.id}`, { cookie: ownerA })).status, 404);
  // 열쇠 앞머리를 남의 교회로 바꿔 끼우면 401
  const forged = `${OTHER}.${cred('bridge').secret}`;
  assert.equal((await bin(`/api/v1/assets/${b.json.id}`, { headers: { authorization: `Bearer ${forged}` } })).status, 401);
  // 설치판 열쇠(Bearer)는 자기 교회 것만
  const br = { bearer: cred('bridge').key };
  assert.equal((await bin(`/api/v1/assets/${a.id}`, { headers: { authorization: `Bearer ${br.bearer}` } })).status, 200);
  assert.equal((await bin(`/api/v1/assets/${b.json.id}`, { headers: { authorization: `Bearer ${br.bearer}` } })).status, 404);
});

test('설치판 열쇠(Bearer)로 올리기 · 있나 묻기 · 사용량 (정리는 못 함)', async () => {
  const br = { bearer: cred('bridge').key };
  const body = bytes(20_000);
  const h = { authorization: `Bearer ${br.bearer}` };
  assert.equal((await bin(`/api/v1/assets?sha256=${sha(body)}`, { headers: h })).json.asset, null);
  const r = await upload(br, body, { name: '설치판.png' });
  assert.equal(r.status, 201, r.buf.toString());
  assert.equal((await bin(`/api/v1/assets?sha256=${sha(body)}`, { headers: h })).json.asset.id, r.json.id);
  const u = await bin('/api/v1/assets/usage', { headers: h });
  assert.equal(u.status, 200);
  assert.ok(u.json.church.bytes > 0);
  assert.equal(u.json.free, undefined, '계정 전체 범위는 주인에게만');
  assert.equal((await bin('/api/v1/assets/purge', { method: 'POST', headers: { ...h, 'content-type': 'application/json' }, body: '{}' })).status, 403);
});

test('나눠 올리기: 조각 크기 확인 · 빠진 조각 409 · 끝내기 · 조각 경계를 넘는 Range', { timeout: 120_000 }, async () => {
  const size = 32 * MiB + 1234;
  const body = bytes(size, 'big');
  const hash = sha(body);
  const hdr = { ...W, 'content-type': 'video/mp4', 'x-asset-sha256': hash, 'x-asset-name': encodeURIComponent('설교 영상.mp4'), 'x-asset-size': String(size) };
  const s = await req('/api/v1/assets/uploads', { method: 'POST', cookie: ownerA, headers: hdr });
  assert.equal(s.status, 201, s.text);
  assert.equal(s.json.existed, false);
  assert.equal(s.json.partSize, 32 * MiB);
  assert.equal(s.json.parts, 2);
  const u = s.json.uploadId;
  const put = (n, b) => bin(`/api/v1/assets/uploads/${u}/parts/${n}`, { method: 'PUT', cookie: ownerA, headers: W, body: b });
  assert.equal((await put(1, body.subarray(0, 1000))).status, 400, '마지막 말고는 partSize 그대로');
  assert.equal((await put(3, body.subarray(0, 10))).status, 400);
  const p2 = await put(2, body.subarray(32 * MiB));
  assert.equal(p2.status, 200, p2.buf.toString());
  assert.equal(p2.json.n, 2);
  let st = await req(`/api/v1/assets/uploads/${u}`, { cookie: ownerA });
  assert.deepEqual(st.json.done, [2]);
  const miss = await req(`/api/v1/assets/uploads/${u}/complete`, { method: 'POST', cookie: ownerA, headers: W });
  assert.equal(miss.status, 409);
  assert.deepEqual(miss.json.missing, [1]);
  assert.equal((await put(1, body.subarray(0, 32 * MiB))).status, 200);
  // 다른 교회는 이 나눠 올리기를 못 봄
  assert.equal((await req(`/api/v1/assets/uploads/${u}`, { cookie: ownerB })).status, 404);
  const done = await req(`/api/v1/assets/uploads/${u}/complete`, { method: 'POST', cookie: ownerA, headers: W });
  assert.equal(done.status, 201, done.text);
  assert.equal(done.json.size, size);
  assert.equal(done.json.asset.verified, false, '나눠 올린 것은 올린 쪽이 알린 sha256');
  assert.equal((await req(`/api/v1/assets/uploads/${u}`, { cookie: ownerA })).status, 404, '끝나면 사라짐');
  const r = await bin(`/api/v1/assets/${done.json.id}`, { cookie: output, headers: { range: `bytes=${32 * MiB - 10}-${32 * MiB + 9}` } });
  assert.equal(r.status, 206);
  assert.ok(r.buf.equals(body.subarray(32 * MiB - 10, 32 * MiB + 10)));
  const all = await bin(`/api/v1/assets/${done.json.id}`, { cookie: output });
  assert.equal(sha(all.buf), hash);
  // 같은 것을 다시 나눠 올리려 하면 바로 existed
  const again = await req('/api/v1/assets/uploads', { method: 'POST', cookie: ownerA, headers: hdr });
  assert.equal(again.json.existed, true);
  assert.equal(again.json.id, done.json.id);
  // 한 번에 올리기 한도를 넘으면 413 + multipart
  const tooBig = await bin('/api/v1/assets', { method: 'POST', cookie: ownerA, headers: { ...W, 'content-type': 'video/mp4', 'x-asset-sha256': '1'.repeat(64), 'x-asset-name': 'a.mp4' }, body: Buffer.alloc(91 * MiB) });
  assert.equal(tooBig.status, 413);
  assert.equal(tooBig.json.multipart, true);
  // 그만두기
  const s2 = await req('/api/v1/assets/uploads', { method: 'POST', cookie: ownerA, headers: { ...hdr, 'x-asset-sha256': '2'.repeat(64), 'x-asset-size': '100' } });
  assert.equal(s2.json.parts, 1);
  assert.equal((await req(`/api/v1/assets/uploads/${s2.json.uploadId}`, { method: 'DELETE', cookie: ownerA, headers: W })).json.aborted, true);
  assert.equal((await req(`/api/v1/assets/uploads/${s2.json.uploadId}`, { cookie: ownerA })).status, 404);
});

test('지우기는 표시만 → 되살리기 → 정리 명령 뒤에야 R2 에서 사라짐 (410 · 설치판 중계로 돌려보내기)', async () => {
  const body = bytes(5000);
  const { json: up } = await upload(ownerB, body, { name: '지울 것.png' });
  const p = `/api/v1/assets/${up.id}`;
  const d = await req(p, { method: 'DELETE', cookie: ownerB, headers: W });
  assert.equal(d.status, 200);
  assert.equal(d.json.deleted, true);
  assert.ok(!(await req('/api/v1/assets', { cookie: ownerB })).json.assets.some(x => x.id === up.id), '목록에서 숨김');
  assert.ok((await req('/api/v1/assets?deleted=1', { cookie: ownerB })).json.assets.find(x => x.id === up.id).deleted);
  const still = await bin(p, { cookie: ownerB });
  assert.equal(still.status, 200, '예배 중 실수로 지워도 화면은 그대로');
  assert.equal(still.headers.get('x-wp-asset-deleted'), '1');
  assert.equal((await req(`${p}/restore`, { method: 'POST', cookie: ownerB, headers: W })).json.deleted, false);
  assert.ok((await req('/api/v1/assets', { cookie: ownerB })).json.assets.some(x => x.id === up.id));
  await req(p, { method: 'DELETE', cookie: ownerB, headers: W });
  // 정리: 기본은 보기만 · 기다림(30일) 안이면 안 지움
  let pr = await req('/api/v1/assets/purge', { method: 'POST', cookie: ownerB, headers: W, body: {} });
  assert.equal(pr.json.dryRun, true);
  assert.ok(!pr.json.purged.some(x => x.sha256 === up.sha256));
  assert.equal((await req('/api/v1/assets/purge', { method: 'POST', cookie: ownerB, headers: W, body: { olderThanDays: 0, dryRun: false } })).status, 400, '0일은 confirm 필요');
  pr = await req('/api/v1/assets/purge', { method: 'POST', cookie: ownerB, headers: W, body: { olderThanDays: 0, confirm: 'now' } });
  assert.ok(pr.json.purged.some(x => x.sha256 === up.sha256), '보기만');
  assert.equal((await bin(p, { cookie: ownerB })).status, 200);
  pr = await req('/api/v1/assets/purge', { method: 'POST', cookie: ownerB, headers: W, body: { olderThanDays: 0, confirm: 'now', dryRun: false } });
  assert.equal(pr.status, 200, pr.text);
  assert.ok(pr.json.purged.some(x => x.sha256 === up.sha256));
  assert.ok(pr.json.freedBytes >= body.length);
  assert.equal((await bin(p, { cookie: ownerB })).status, 410);
  const fb = await bin(`${p}?fallback=${encodeURIComponent('/lib/file?p=C:\\a.png')}`, { cookie: ownerB });
  assert.equal(fb.status, 302);
  assert.equal(fb.headers.get('location'), '/lib/file?p=C:\\a.png');
  assert.equal((await bin(`${p}?fallback=${encodeURIComponent('https://evil.example/x')}`, { cookie: ownerB })).status, 410, '돌려보내기는 설치판 중계 길만');
  assert.equal((await bin(`${p}?fallback=${encodeURIComponent('/api/v1/me')}`, { cookie: ownerB })).status, 410);
  assert.equal((await req(`${p}/restore`, { method: 'POST', cookie: ownerB, headers: W })).status, 410);
  // 다시 올리면 새로
  const again = await upload(ownerB, body, { name: '지울 것.png' });
  assert.equal(again.status, 201);
  assert.notEqual(again.json.id, up.id);
  assert.ok((await bin(`/api/v1/assets/${again.json.id}`, { cookie: ownerB })).buf.equals(body));
  // 다른 교회(sample) 것은 정리에 안 걸림
  assert.ok(!pr.json.purged.some(x => x.names.includes('sample 것')));
});

// ---------- 글꼴 ----------
function sfnt(names, weight) {
  const enc = s => { const b = []; for (const ch of s) { const c = ch.charCodeAt(0); b.push(c >> 8, c & 255); } return b; };
  const recs = [], strs = [];
  let off = 0;
  for (const [id, lang, text] of names) { const b = enc(text); recs.push([3, 1, lang, id, b.length, off]); strs.push(...b); off += b.length; }
  const name = [0, 0, 0, recs.length, 0, 6 + recs.length * 12];
  for (const r of recs) for (const v of r) name.push(v >> 8, v & 255);
  name.push(...strs);
  const os2 = new Array(96).fill(0); os2[4] = weight >> 8; os2[5] = weight & 255;
  const u32 = v => [(v >>> 24) & 255, (v >>> 16) & 255, (v >>> 8) & 255, v & 255];
  const head = 12 + 2 * 16;
  const out = [...u32(0x00010000), 0, 2, 0, 0, 0, 0, 0, 0];
  out.push(...'OS/2'.split('').map(c => c.charCodeAt(0)), 0, 0, 0, 0, ...u32(head), ...u32(os2.length));
  out.push(...'name'.split('').map(c => c.charCodeAt(0)), 0, 0, 0, 0, ...u32(head + os2.length), ...u32(name.length));
  return Buffer.from([...out, ...os2, ...name, ...randomBytes(64)]);
}

test('글꼴: 파일 안 이름 · 굵기를 읽어 제안 · 구세군체 Bold(400) 는 700 으로 · fonts.css · 고치기 · 출력 페이지에 끼움', async () => {
  const fam = `구세군체 시험 ${Date.now().toString(36)}`;
  const reg = sfnt([[1, 0x0412, fam], [2, 0x0409, 'Regular']], 400);
  const bold = sfnt([[1, 0x0412, fam], [2, 0x0409, 'Bold']], 400);
  const r1 = await upload(ownerA, reg, { name: `${fam}.ttf`, type: 'font/ttf' });
  assert.equal(r1.status, 201, r1.buf.toString());
  assert.deepEqual(r1.json.asset.font, { family: fam, weight: 400, style: 'normal' });
  const r2 = await upload(ownerA, bold, { name: `${fam} Bold.ttf`, type: 'font/ttf' });
  assert.deepEqual(r2.json.asset.font, { family: fam, weight: 700, style: 'normal' });
  assert.match(r2.json.warnings.join(' '), /400.*700/);
  assert.equal(r2.json.fontInfo.weightClass, 400);
  let css = (await bin('/api/v1/assets/fonts.css', { cookie: output })).buf.toString();
  assert.ok(css.includes(`font-family:"${fam}";font-weight:400;font-style:normal;font-display:block;src:url("/api/v1/assets/${r1.json.id}") format("truetype")`), css);
  assert.ok(css.includes(`font-family:"${fam}";font-weight:700;font-style:normal;font-display:block;src:url("/api/v1/assets/${r2.json.id}")`), css);
  // 적은 값이 파일 안 값보다 먼저 · 다른 이름으로 따로 (템플릿이 '… Bold' 라는 이름을 쓰는 경우)
  const sep = sfnt([[1, 0x0412, `${fam} 따로`], [2, 0x0409, 'Regular']], 400);
  const r3 = await upload(ownerA, sep, { name: 'sep.ttf', type: 'font/ttf', extra: { 'x-asset-font-family': encodeURIComponent(`${fam} Bold`), 'x-asset-font-weight': '400' } });
  assert.deepEqual(r3.json.asset.font, { family: `${fam} Bold`, weight: 400, style: 'normal' });
  css = (await bin('/api/v1/assets/fonts.css', { cookie: ownerA })).buf.toString();
  assert.ok(css.includes(`font-family:"${fam} Bold";font-weight:1 1000;`), '굵기가 하나뿐인 이름은 모든 굵기에');
  // 고치기 (굵기를 400 으로 되돌리면 겹친다는 알림)
  const p = await req(`/api/v1/assets/${r2.json.id}`, { method: 'PATCH', cookie: ownerA, headers: W, body: { weight: 400 } });
  assert.equal(p.status, 200, p.text);
  assert.equal(p.json.asset.font.weight, 400);
  assert.equal(p.json.warnings.length, 1);
  assert.equal((await req(`/api/v1/assets/${r2.json.id}`, { method: 'PATCH', cookie: ownerA, headers: W, body: { weight: 'bold' } })).status, 400);
  await req(`/api/v1/assets/${r2.json.id}`, { method: 'PATCH', cookie: ownerA, headers: W, body: { weight: 700 } });
  // ETag → 304
  const c1 = await bin('/api/v1/assets/fonts.css', { cookie: ownerA });
  assert.equal(c1.headers.get('content-type'), 'text/css; charset=utf-8');
  assert.equal((await bin('/api/v1/assets/fonts.css', { cookie: ownerA, headers: { 'if-none-match': c1.headers.get('etag') } })).status, 304);
  // 그림에는 글꼴 정보를 못 적는다
  const img = await upload(ownerA, bytes(100), { name: 'i.png' });
  assert.equal((await req(`/api/v1/assets/${img.json.id}`, { method: 'PATCH', cookie: ownerA, headers: W, body: { family: 'x' } })).status, 400);
  // 다른 교회 fonts.css 에는 없다
  assert.ok(!(await bin('/api/v1/assets/fonts.css', { cookie: ownerB })).buf.toString().includes(fam));
  // 출력 페이지 머리에 글꼴 CSS
  const page = await req('/outputs/main', { cookie: output });
  assert.equal(page.status, 200);
  assert.match(page.text, /<link rel="stylesheet" href="\/api\/v1\/assets\/fonts\.css">/);
  // 지운 글꼴은 CSS 에서 빠짐
  await req(`/api/v1/assets/${r3.json.id}`, { method: 'DELETE', cookie: ownerA, headers: W });
  assert.ok(!(await bin('/api/v1/assets/fonts.css', { cookie: ownerA })).buf.toString().includes(`${fam} Bold`));
});

test('목록: 묶음 거르기 · 쪽 넘기기', async () => {
  const first = await req('/api/v1/assets?kind=font&limit=1', { cookie: ownerA });
  assert.equal(first.status, 200);
  assert.equal(first.json.assets.length, 1);
  assert.ok(first.json.assets.every(a => a.kind === 'font'));
  assert.ok(first.json.next);
  const second = await req(`/api/v1/assets?kind=font&limit=1&cursor=${first.json.next}`, { cookie: ownerA });
  assert.notEqual(second.json.assets[0].id, first.json.assets[0].id);
  assert.equal((await req('/api/v1/assets?kind=html', { cookie: ownerA })).status, 400);
  assert.equal((await req('/api/v1/assets?cursor=zzz', { cookie: ownerA })).status, 400);
  const viewer = await req('/api/v1/assets', { cookie: output });
  assert.equal(viewer.status, 403);
});

test('사용량 · 무료 범위 안전장치: 넘으면 507 quota, 받기는 그대로', { timeout: 180_000 }, async () => {
  const u = await req('/api/v1/assets/usage', { cookie: ownerA });
  assert.equal(u.status, 200);
  assert.ok(u.json.church.bytes > 32 * 1024 * 1024);
  assert.ok(u.json.church.assets > 0);
  assert.ok(u.json.church.classA > 0);
  assert.ok(u.json.church.classB > 0, '받기 수 (모아 두었다 더함)');
  assert.equal(u.json.free.storage.limit, 10e9);
  assert.equal(u.json.free.storage.stopAt, 8e9);
  assert.equal(u.json.free.uploadsBlocked, false);
  // 교회 한도를 지금 쓴 만큼으로 줄이면 올리기가 507
  const { json: before } = await req('/api/v1/assets/usage', { cookie: ownerB });
  sql(`INSERT INTO asset_limits (church_id, max_bytes) VALUES ('${OTHER}', ${before.church.bytes + before.church.uploadingBytes + 10}) ON CONFLICT (church_id) DO UPDATE SET max_bytes = excluded.max_bytes`);
  try {
    const body = bytes(1000);
    const r = await upload(ownerB, body, { name: '넘침.png' });
    assert.equal(r.status, 507, r.buf.toString());
    assert.equal(r.json.error, 'quota');
    assert.equal(r.json.reason, 'church');
    const m = await req('/api/v1/assets/uploads', { method: 'POST', cookie: ownerB, headers: { ...W, 'content-type': 'video/mp4', 'x-asset-sha256': sha(body), 'x-asset-name': 'a.mp4', 'x-asset-size': '1000' } });
    assert.equal(m.status, 507);
    // 이미 있는 파일 · 받기는 막지 않는다
    const list = (await req('/api/v1/assets', { cookie: ownerB })).json.assets;
    assert.equal((await bin(`/api/v1/assets/${list[0].id}`, { cookie: ownerB })).status, 200);
    assert.equal((await req('/api/v1/assets/usage', { cookie: ownerB })).json.church.limitBytes, before.church.bytes + before.church.uploadingBytes + 10);
  } finally {
    sql(`DELETE FROM asset_limits WHERE church_id = '${OTHER}'`);
  }
  assert.equal((await upload(ownerB, bytes(1000), { name: '다시.png' })).status, 201);
});
