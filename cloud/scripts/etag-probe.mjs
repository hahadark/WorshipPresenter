// 점검: node scripts/etag-probe.mjs <cloud 폴더> — wrangler dev 가 정적 파일을 바꿔 넣었을 때 ETag · 304 를 어떻게 주는지 본다 (6단계 새 판 시험의 '업데이트 안 됨' 원인 확인)
import { spawn, execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync, mkdtempSync } from 'node:fs';
import net from 'node:net';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
const cloud = process.argv[2];
const wrangler = join(cloud, 'node_modules', 'wrangler', 'bin', 'wrangler.js');
const freePort = () => new Promise(res => { const s = net.createServer(); s.listen(0, '127.0.0.1', () => { const p = s.address().port; s.close(() => res(p)); }); });
const sleep = ms => new Promise(r => setTimeout(r, ms));
const port = await freePort(), insp = await freePort();
const persist = mkdtempSync(join(tmpdir(), 'wp-etag-'));
const dev = spawn(process.execPath, [wrangler, 'dev', '--ip', '127.0.0.1', '--port', String(port), '--inspector-port', String(insp), '--local', '--persist-to', persist, '--show-interactive-dev-session=false'], { cwd: cloud, stdio: 'ignore' });
const base = `http://127.0.0.1:${port}`;
for (let i = 0; i < 200; i++) { try { if ((await fetch(base + '/health')).ok) break; } catch { } await sleep(300); }
const file = join(cloud, 'public', 'sw.js');
const orig = readFileSync(file, 'utf8');
try {
  const r1 = await fetch(base + '/sw.js', { cache: 'no-store' });
  const t1 = await r1.text();
  const h1 = Object.fromEntries([...r1.headers].filter(([k]) => /etag|last-modified|cache-control/.test(k)));
  writeFileSync(file, orig.replace(/const VERSION = "[^"]+"/, 'const VERSION = "probe-new"'));
  let seen = null;
  for (let i = 0; i < 60; i++) { const t = await (await fetch(base + '/sw.js', { cache: 'no-store' })).text(); if (t.includes('probe-new')) { seen = i; break; } await sleep(500); }
  const r2 = await fetch(base + '/sw.js', { cache: 'no-store' });
  const h2 = Object.fromEntries([...r2.headers].filter(([k]) => /etag|last-modified|cache-control/.test(k)));
  const cond = await fetch(base + '/sw.js', { headers: { 'if-none-match': h1.etag || '"x"', ...(h1['last-modified'] ? { 'if-modified-since': h1['last-modified'] } : {}) } });
  console.log(JSON.stringify({ before: h1, newContentAfterPolls: seen, after: h2, conditionalWithOldValidators: cond.status, oldHasVersion: /const VERSION/.test(t1) }, null, 1));
} finally {
  writeFileSync(file, orig);
  try { execFileSync('taskkill', ['/pid', String(dev.pid), '/t', '/f'], { stdio: 'ignore' }); } catch { }
}
