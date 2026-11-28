// 2단계 B 끝에서 끝: 설치판 동기화(crates/wp-core/src/cloudsync.rs) ↔ 로컬 클라우드(wrangler dev + 로컬 D1).
//   node test/sync-e2e/run.mjs            견본 자료 (지어낸 큐시트 · 찬양 · 그림)
//   node test/sync-e2e/run.mjs --real     이 PC 설치판 자료의 복사본 (%APPDATA%\WorshipPresenter-rs\data 를 SQLite backup 으로 임시 폴더에)
//   --keep                                 끝나도 임시 폴더를 지우지 않음 (원인 찾기)
//   --fake-assets                          그림·영상 길을 2C 대신 이 스크립트의 가짜(메모리)로 (2C 가 없는 가지에서)
// - 로컬 D1 · DO 는 이 실행만의 임시 폴더(--persist-to)에 둔다 → 다른 작업자의 wrangler dev · 앞 실행 자료와 섞이지 않고, 끝나면 지운다.
// - 그림·영상 길(/api/v1/assets*)은 기본으로 2C 의 실제 길(로컬 R2, 같은 --persist-to). --fake-assets 면 가짜 중계가 계약대로 메모리에서 받고
//   나머지 길은 wrangler dev 로 넘긴다. 아무것도 인터넷 너머로 보내지 않는다 (127.0.0.1 만).
// - 원본 자료는 읽기만 한다. 복사본 · 로컬 D1 · 가짜 저장소는 끝나면 지운다.
import { spawn, execFileSync } from 'node:child_process';
import { createHash, createHmac, randomBytes, randomInt } from 'node:crypto';
import { cpSync, existsSync, mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createServer as httpServer } from 'node:http';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { DatabaseSync, backup } from 'node:sqlite';

const cloud = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const repo = join(cloud, '..');
const wrangler = join(cloud, 'node_modules', 'wrangler', 'bin', 'wrangler.js');
const args = process.argv.slice(2);
const REAL = args.includes('--real');
const KEEP = args.includes('--keep');
const FAKE = args.includes('--fake-assets');
process.env.WRANGLER_SEND_METRICS = 'false';

const freePort = () => new Promise(res => { const s = createServer(); s.listen(0, '127.0.0.1', () => { const p = s.address().port; s.close(() => res(p)); }); });
const wr = (...a) => execFileSync(process.execPath, [wrangler, ...a], { cwd: cloud, stdio: ['ignore', 'pipe', 'pipe'] });
const q = s => `'${String(s).replace(/'/g, "''")}'`;
const work = mkdtempSync(join(tmpdir(), 'wp-sync-e2e-'));
const persist = join(work, 'd1');
const data = join(work, 'data');
mkdirSync(data, { recursive: true });

// ---------- 견본 / 복사본 자료 ----------
async function prepareData() {
  if (REAL) {
    const src = process.env.WP_SYNC_E2E_SOURCE || join(process.env.APPDATA || '', 'WorshipPresenter-rs', 'data');
    if (!existsSync(join(src, 'bible.db'))) throw new Error(`설치판 자료가 없습니다: ${src}`);
    // 쓰는 중인 DB 도 안전하게: 읽기 전용으로 열어 SQLite backup (원본은 건드리지 않음)
    const db = new DatabaseSync(join(src, 'bible.db'), { readOnly: true });
    await backup(db, join(data, 'bible.db'));
    db.close();
    if (existsSync(join(src, 'settings.json'))) cpSync(join(src, 'settings.json'), join(data, 'settings.json'));
    console.log(`설치판 자료 복사본: ${data}`);
    return;
  }
  // 견본: 사용자 폴더처럼 보이는 곳의 그림 (경로가 새지 않는지 보려고)
  const pics = join(work, 'Users', '견본사용자', 'Pictures');
  mkdirSync(pics, { recursive: true });
  const png = Buffer.from('89504e470d0a1a0a0000000d4948445200000001000000010806000000', 'hex');
  writeFileSync(join(pics, '배경 (1).png'), png);
  writeFileSync(join(pics, 'intro.mp4'), randomBytes(40_000));
  const enc = p => '/lib/file?p=' + [...Buffer.from(p)].map(b => /[A-Za-z0-9\-_.!~*'()]/.test(String.fromCharCode(b)) ? String.fromCharCode(b) : '%' + b.toString(16).toUpperCase().padStart(2, '0')).join('');
  const db = new DatabaseSync(join(data, 'bible.db'));
  db.exec(`CREATE TABLE IF NOT EXISTS Cuesheets (id TEXT PRIMARY KEY, name TEXT NOT NULL, service_date TEXT, items TEXT NOT NULL, updated_at TEXT);
           CREATE TABLE IF NOT EXISTS Songs (id TEXT PRIMARY KEY, title TEXT NOT NULL, lyrics TEXT NOT NULL DEFAULT '', credit TEXT NOT NULL DEFAULT '', tags TEXT NOT NULL DEFAULT '', updated_at TEXT);`);
  const ic = db.prepare('INSERT INTO Cuesheets (id, name, service_date, items, updated_at) VALUES (?, ?, ?, ?, ?)');
  const now = new Date().toISOString();
  ic.run('cse2e1', '견본 주일 예배', '2026-11-29', JSON.stringify([
    { id: 'a', type: 'title', title: '예배로 부름' },
    { id: 'b', type: 'image', image: enc(join(pics, '배경 (1).png')) },
    { id: 'c', type: 'video', image: enc(join(pics, 'intro.mp4')) },
    { id: 'd', type: 'image', image: join(pics, '배경 (1).png') },
  ]), now);
  ic.run('cse2e2', '견본 수요 예배', '2026-12-02', JSON.stringify([{ id: 'a', type: 'text', body: '광고' }]), now);
  ic.run('cse2e3', '견본 금요 기도회', '2026-12-04', '[]', now);
  const is = db.prepare('INSERT INTO Songs (id, title, lyrics, updated_at) VALUES (?, ?, ?, ?)');
  for (let i = 1; i <= 25; i++) is.run(`se2e${i}x`, `견본 찬양 ${i}`, `[1절]\n지어낸 가사 ${i}`, now);
  db.close();
  console.log(`견본 자료: 큐시트 3 · 찬양 25 · 그림 2 (${data})`);
}

// ---------- 로컬 클라우드 ----------
function authSecret() {
  const f = join(cloud, '.secrets', 'auth.json');
  if (!existsSync(f) || !existsSync(join(cloud, '.dev.vars'))) execFileSync(process.execPath, [join(cloud, 'scripts', 'seed.mjs')], { cwd: cloud, stdio: 'inherit' });
  return JSON.parse(readFileSync(f, 'utf8')).AUTH_SECRET;
}

function seedChurch(secret) {
  const church = 'sync-e2e';
  const ALPHA = 'ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789';
  const key = Array.from({ length: 32 }, () => ALPHA[randomInt(ALPHA.length)]).join('');
  const hash = createHmac('sha256', secret).update(`cred:${church}:bridge:${key}`).digest('hex');
  const now = Date.now();
  const sqlFile = join(work, 'seed.sql');
  writeFileSync(sqlFile, [
    `INSERT INTO churches (id, name, created_at) VALUES (${q(church)}, ${q('동기화 시험 교회')}, ${now});`,
    `INSERT INTO credentials (church_id, id, role, label, secret_hash, created_at) VALUES (${q(church)}, ${q('e2ebridge1')}, 'bridge', ${q('동기화 시험 설치판')}, ${q(hash)}, ${now});`,
  ].join('\n'));
  wr('d1', 'migrations', 'apply', 'worshippresenter-web', '--local', '--persist-to', persist);
  wr('d1', 'execute', 'worshippresenter-web', '--local', '--persist-to', persist, '--file', sqlFile, '-y');
  rmSync(sqlFile);
  return `${church}.${key}`;
}

async function startCloud() {
  execFileSync(process.execPath, [join(cloud, 'scripts', 'assets.mjs')], { cwd: cloud, stdio: 'ignore' });
  const port = await freePort();
  const inspector = await freePort();
  const base = `http://127.0.0.1:${port}`;
  const dev = spawn(process.execPath, [wrangler, 'dev', '--ip', '127.0.0.1', '--port', String(port), '--inspector-port', String(inspector), '--local', '--persist-to', persist, '--show-interactive-dev-session=false'], { cwd: cloud, stdio: ['ignore', 'pipe', 'pipe'] });
  let log = '';
  dev.stdout.on('data', d => { log += d; });
  dev.stderr.on('data', d => { log += d; });
  const t0 = Date.now();
  for (;;) {
    try { const r = await fetch(base + '/health'); if (r.ok) break; } catch { /* 아직 */ }
    if (dev.exitCode !== null || Date.now() - t0 > 90_000) { console.error(log); throw new Error('wrangler dev 가 뜨지 않았습니다'); }
    await new Promise(r => setTimeout(r, 300));
  }
  return { dev, base };
}

// ---------- 그림·영상 길 (계약대로, 메모리) + 나머지는 wrangler 로 ----------
function startProxy(upstream, key) {
  const assets = new Map(); // id → {sha256, name, type, bytes}
  const bySha = new Map();
  const stats = { uploads: 0, existed: 0, reads: 0, bytes: 0 };
  const MAX = 200 * 1000 * 1000; // 이보다 크면 507 (한도 흉내)
  const send = (res, code, obj, headers = {}) => { res.writeHead(code, { 'content-type': 'application/json; charset=utf-8', ...headers }); res.end(JSON.stringify(obj)); };
  const server = httpServer(async (req, res) => {
    try {
      const u = new URL(req.url, 'http://x');
      const chunks = [];
      for await (const c of req) chunks.push(c);
      const body = Buffer.concat(chunks);
      if (u.pathname === '/api/v1/assets' || u.pathname.startsWith('/api/v1/assets/')) {
        if (req.headers.authorization !== `Bearer ${key}`) return send(res, 401, { ok: false, error: '로그인' });
        if (req.method === 'GET' && u.pathname === '/api/v1/assets') {
          const id = bySha.get(u.searchParams.get('sha256') || '');
          const a = id && assets.get(id);
          return send(res, 200, { ok: true, asset: a ? { id, sha256: a.sha256, size: a.bytes.length, type: a.type, name: a.name } : null });
        }
        if (req.method === 'POST' && u.pathname === '/api/v1/assets') {
          const sha = String(req.headers['x-asset-sha256'] || '');
          const name = decodeURIComponent(String(req.headers['x-asset-name'] || ''));
          if (/[\\/]/.test(name)) return send(res, 400, { ok: false, error: '이름에 경로' });
          if (bySha.has(sha)) { stats.existed++; const id = bySha.get(sha); const a = assets.get(id); return send(res, 200, { ok: true, id, sha256: sha, size: a.bytes.length, type: a.type, existed: true }); }
          if (body.length > MAX) return send(res, 507, { ok: false, error: 'quota' });
          if (createHash('sha256').update(body).digest('hex') !== sha) return send(res, 400, { ok: false, error: 'sha256 다름' });
          const id = 'as' + sha.slice(0, 20);
          assets.set(id, { sha256: sha, name, type: String(req.headers['content-type'] || 'application/octet-stream'), bytes: body });
          bySha.set(sha, id);
          stats.uploads++; stats.bytes += body.length;
          return send(res, 201, { ok: true, id, sha256: sha, size: body.length, type: assets.get(id).type, existed: false });
        }
        const m = u.pathname.match(/^\/api\/v1\/assets\/([A-Za-z0-9_-]+)$/);
        if (req.method === 'GET' && m) {
          const a = assets.get(m[1]);
          if (!a) return send(res, 404, { ok: false, error: '없음' });
          stats.reads++;
          res.writeHead(200, { 'content-type': a.type, 'content-length': a.bytes.length, 'accept-ranges': 'bytes' });
          return res.end(a.bytes);
        }
        return send(res, 404, { ok: false, error: '없는 주소' }); // 나눠 올리기는 이 시험에서 쓰지 않음
      }
      const h = { ...req.headers };
      delete h.host; delete h['content-length'];
      const r = await fetch(upstream + req.url, { method: req.method, headers: h, body: ['GET', 'HEAD'].includes(req.method) ? undefined : body, redirect: 'manual' });
      const out = Buffer.from(await r.arrayBuffer());
      const rh = {};
      r.headers.forEach((v, k) => { if (k !== 'content-encoding' && k !== 'content-length' && k !== 'transfer-encoding') rh[k] = v; });
      res.writeHead(r.status, rh);
      res.end(out);
    } catch (e) { send(res, 502, { ok: false, error: String(e) }); }
  });
  return new Promise(resolve => server.listen(0, '127.0.0.1', () => resolve({ server, stats, url: `http://127.0.0.1:${server.address().port}` })));
}

async function main() {
  let dev = null;
  let proxy = null;
  let code = 1;
  try {
    await prepareData();
    const key = seedChurch(authSecret());
    const c = await startCloud();
    dev = c.dev;
    if (FAKE) proxy = await startProxy(c.base, key);
    const url = proxy ? proxy.url : c.base;
    console.log(`로컬 클라우드 ${c.base} (D1 · R2 ${persist})${proxy ? ` · 그림·영상 길 가짜 중계 ${proxy.url}` : ' · 그림·영상 길 = 2C (로컬 R2)'}`);
    const env = { ...process.env, WP_SYNC_E2E_URL: url, WP_SYNC_E2E_KEY: key, WP_SYNC_E2E_DATA: data, WP_LEGACY_DATA: join(work, 'no-legacy') };
    const t = spawn('cargo', ['test', '-p', 'wp-core', '--lib', 'cloudsync::tests::e2e_local_cloud', '--', '--ignored', '--nocapture', '--exact'], { cwd: repo, stdio: 'inherit', env });
    code = await new Promise(r => t.on('exit', r));
    if (proxy) console.log(`그림·영상 길(가짜): 새로 올림 ${proxy.stats.uploads} (${proxy.stats.bytes} 바이트) · 이미 있음 ${proxy.stats.existed} · 읽기 ${proxy.stats.reads}`);
    else {
      const r = await fetch(`${c.base}/api/v1/assets?limit=200`, { headers: { authorization: `Bearer ${key}` } }).then(r => r.json());
      const list = r.assets || [];
      const leak = list.filter(a => /[\/:]/.test(a.name || '') || (process.env.USERNAME && JSON.stringify(a).includes(process.env.USERNAME)));
      console.log(`그림·영상 길(2C): 저장된 파일 ${list.length}개 · ${list.reduce((n, a) => n + (a.size || 0), 0)} 바이트 · 이름에 경로/사용자 이름 ${leak.length}개`);
      if (leak.length) code = code || 3;
    }
    console.log(code === 0 ? '동기화 끝에서 끝: 통과' : `동기화 끝에서 끝: 실패 (${code})`);
  } finally {
    proxy?.server.close();
    if (dev) {
      if (process.platform === 'win32') { try { execFileSync('taskkill', ['/pid', String(dev.pid), '/t', '/f'], { stdio: 'ignore' }); } catch { /* 이미 꺼짐 */ } } else dev.kill('SIGTERM');
    }
    if (!KEEP) { await new Promise(r => setTimeout(r, 800)); try { rmSync(work, { recursive: true, force: true }); } catch (e) { console.warn(`임시 폴더를 다 지우지 못함: ${work} (${e.message})`); } }
  }
  process.exit(code ?? 1);
}
main().catch(e => { console.error(e); process.exit(1); });
