// 실제 R2 저장통 확인 (2단계 C): Worker 는 로컬(wrangler dev)에서 돌리고 R2 만 원격 저장통에 붙인다 — Worker 를 배포하지 않는다.
//   node scripts/r2-remote-check.mjs
// 로컬 흉내(miniflare)와 실제 R2 가 다를 수 있는 것만 본다: sha256 거절 · Range · 나눠 올리기(조각 둘) · 정리 명령이 실제로 지우는지.
// 견본 파일(지어낸 바이트, 합계 약 34 MB)만 올리고, 끝에 정리 명령으로 지운 뒤 R2 에 남지 않았는지 열쇠마다 확인한다.
// 교회 'r2-check' 를 로컬 D1 에 만들어 쓴다 (다른 견본 교회 줄과 섞이지 않게).
import { spawn, execFileSync } from 'node:child_process';
import { createHash, randomBytes } from 'node:crypto';
import { existsSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:net';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const cloud = join(dirname(fileURLToPath(import.meta.url)), '..');
const wrangler = join(cloud, 'node_modules', 'wrangler', 'bin', 'wrangler.js');
const env = { ...process.env, WRANGLER_SEND_METRICS: 'false' };
const CH = 'r2-check';
const MiB = 1024 * 1024;
const sha = b => createHash('sha256').update(b).digest('hex');
const freePort = () => new Promise(res => { const s = createServer(); s.listen(0, '127.0.0.1', () => { const p = s.address().port; s.close(() => res(p)); }); });
const ok = (c, m) => { if (!c) throw new Error(`실패: ${m}`); console.log(`  ✔ ${m}`); };

// R2 만 원격으로 (remote: true) 붙인 임시 설정
const conf = join(cloud, '.wrangler-r2remote.jsonc');
writeFileSync(conf, readFileSync(join(cloud, 'wrangler.jsonc'), 'utf8').replace('"bucket_name": "worshippresenter-web-assets" }', '"bucket_name": "worshippresenter-web-assets", "remote": true }'));
if (!existsSync(join(cloud, '.secrets', `${CH}.json`))) execFileSync(process.execPath, [join(cloud, 'scripts', 'seed.mjs'), '--church', CH, '--name', 'R2 확인'], { cwd: cloud, stdio: 'ignore', env });
execFileSync(process.execPath, [wrangler, 'd1', 'migrations', 'apply', 'worshippresenter-web', '--local'], { cwd: cloud, stdio: 'ignore', env });
const secret = JSON.parse(readFileSync(join(cloud, '.secrets', `${CH}.json`), 'utf8')).credentials.owner.secret;

const port = await freePort(), insp = await freePort();
const base = `http://127.0.0.1:${port}`;
const dev = spawn(process.execPath, [wrangler, 'dev', '-c', conf, '--ip', '127.0.0.1', '--port', String(port), '--inspector-port', String(insp), '--show-interactive-dev-session=false'], { cwd: cloud, stdio: ['ignore', 'pipe', 'pipe'], env });
let log = '';
dev.stdout.on('data', d => { log += d; });
dev.stderr.on('data', d => { log += d; });
const keys = [];
try {
  const t0 = Date.now();
  for (;;) {
    try { if ((await fetch(base + '/health')).ok) break; } catch { /* 아직 */ }
    if (dev.exitCode !== null || Date.now() - t0 > 120_000) { console.error(log); throw new Error('wrangler dev 가 뜨지 않았습니다'); }
    await new Promise(r => setTimeout(r, 500));
  }
  console.log(`로컬 Worker ${base} (R2 = 원격 worshippresenter-web-assets)`);
  const H = { 'x-worshipcue': '1' };
  const lr = await fetch(base + '/auth/login', { method: 'POST', headers: { ...H, 'content-type': 'application/json', 'cf-connecting-ip': '198.18.250.1' }, body: JSON.stringify({ church: CH, secret }) });
  const cookie = lr.headers.getSetCookie().map(c => c.split(';')[0]).join('; ');
  const call = async (path, init = {}) => {
    const r = await fetch(base + path, { ...init, headers: { ...H, cookie, ...(init.headers || {}) } });
    const buf = Buffer.from(await r.arrayBuffer());
    let json = null;
    try { json = JSON.parse(buf.toString()); } catch { /* 파일 */ }
    return { status: r.status, headers: r.headers, buf, json };
  };
  const up = (body, name, type, hash = sha(body)) => call('/api/v1/assets', { method: 'POST', body, headers: { 'content-type': type, 'x-asset-sha256': hash, 'x-asset-name': encodeURIComponent(name) } });

  console.log('한 번에 올리기');
  const png = randomBytes(300_000);
  const a = await up(png, '확인 그림.png', 'image/png');
  ok(a.status === 201 && a.json.asset.verified, `올림 201 (${a.json?.id})`);
  ok((await up(png, '확인 그림.png', 'image/png')).json.existed === true, '같은 파일 두 번 → existed');
  const bad = await up(randomBytes(1000), 'x.png', 'image/png', sha(Buffer.from('다름')));
  ok(bad.status === 400 && bad.json.reason === 'sha256', `실제 R2 가 sha256 다름을 거절 → 400 (${bad.json?.error})`);
  let r = await call(`/api/v1/assets/${a.json.id}`);
  ok(r.status === 200 && r.buf.equals(png), '받기 전체 같음');
  r = await call(`/api/v1/assets/${a.json.id}`, { headers: { range: 'bytes=1000-1999' } });
  ok(r.status === 206 && r.buf.equals(png.subarray(1000, 2000)) && r.headers.get('content-range') === `bytes 1000-1999/${png.length}`, 'Range 206');
  r = await call(`/api/v1/assets/${a.json.id}`, { headers: { range: 'bytes=-100' } });
  ok(r.buf.equals(png.subarray(png.length - 100)), 'Range 끝에서 100');

  console.log('나눠 올리기 (조각 둘)');
  const size = 32 * MiB + 4321;
  const big = randomBytes(size);
  const s = await call('/api/v1/assets/uploads', { method: 'POST', headers: { 'content-type': 'video/mp4', 'x-asset-sha256': sha(big), 'x-asset-name': encodeURIComponent('확인 영상.mp4'), 'x-asset-size': String(size) } });
  ok(s.status === 201 && s.json.parts === 2, `시작 (${s.json?.uploadId})`);
  const p1 = await call(`/api/v1/assets/uploads/${s.json.uploadId}/parts/1`, { method: 'PUT', body: big.subarray(0, 32 * MiB) });
  const p2 = await call(`/api/v1/assets/uploads/${s.json.uploadId}/parts/2`, { method: 'PUT', body: big.subarray(32 * MiB) });
  ok(p1.status === 200 && p2.status === 200, '조각 둘');
  const done = await call(`/api/v1/assets/uploads/${s.json.uploadId}/complete`, { method: 'POST' });
  ok(done.status === 201 && done.json.size === size, '합치기 201');
  r = await call(`/api/v1/assets/${done.json.id}`, { headers: { range: `bytes=${32 * MiB - 5}-${32 * MiB + 4}` } });
  ok(r.status === 206 && r.buf.equals(big.subarray(32 * MiB - 5, 32 * MiB + 5)), '조각 경계를 넘는 Range');
  const all = await call(`/api/v1/assets/${done.json.id}`);
  ok(sha(all.buf) === sha(big), '받은 전체 sha256 같음');

  const u = await call('/api/v1/assets/usage');
  ok(u.json.church.bytes === png.length + size, `사용량 ${u.json.church.bytes} 바이트`);

  console.log('지우기 → 정리 (R2 에서 실제로)');
  const list = (await call('/api/v1/assets?limit=200')).json.assets;
  for (const x of list) await call(`/api/v1/assets/${x.id}`, { method: 'DELETE' });
  const rows = execFileSync(process.execPath, [wrangler, 'd1', 'execute', 'worshippresenter-web', '--local', '--json', '--command', `SELECT r2_key FROM asset_blobs WHERE church_id = '${CH}' AND purged_at IS NULL`], { cwd: cloud, env }).toString();
  keys.push(...JSON.parse(rows)[0].results.map(x => x.r2_key));
  // 정리 전: 정말 원격 저장통에 있는지 (로컬 흉내에 올라간 것이 아님을 확인)
  for (const k of keys) {
    const got = execFileSync(process.execPath, [wrangler, 'r2', 'object', 'get', `worshippresenter-web-assets/${k}`, '--remote', '--pipe'], { cwd: cloud, env, stdio: ['ignore', 'pipe', 'ignore'], maxBuffer: 64 * MiB });
    ok(got.length === (k.endsWith('.png') ? 300_000 : 32 * MiB + 4321), `원격에 있음: ${k.split('/').slice(-1)[0]} (${got.length} 바이트)`);
  }
  const pr = await call('/api/v1/assets/purge', { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify({ olderThanDays: 0, confirm: 'now', dryRun: false }) });
  ok(pr.status === 200 && pr.json.purged.length === keys.length, `정리 ${pr.json?.purged.length}개 · ${pr.json?.freedBytes} 바이트`);
  ok((await call(`/api/v1/assets/${a.json.id}`)).status === 410, '정리 뒤 410');
} finally {
  if (process.platform === 'win32') { try { execFileSync('taskkill', ['/pid', String(dev.pid), '/t', '/f'], { stdio: 'ignore' }); } catch { /* 꺼짐 */ } } else dev.kill('SIGTERM');
  rmSync(conf, { force: true });
}
// 원격에 남지 않았는지 열쇠마다 (wrangler r2 object get --remote → 없으면 오류)
for (const k of keys) {
  let left = true;
  try { execFileSync(process.execPath, [wrangler, 'r2', 'object', 'get', `worshippresenter-web-assets/${k}`, '--remote', '--pipe'], { cwd: cloud, env, stdio: ['ignore', 'ignore', 'ignore'] }); } catch { left = false; }
  ok(!left, `원격에 남지 않음: ${k.split('/').slice(-1)[0]}`);
}
console.log('끝.');
