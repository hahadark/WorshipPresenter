// 끝에서 끝 시험: 로컬 클라우드(wrangler dev, 로컬 D1 · Durable Object)를 띄우고 test/e2e/*.test.mjs 를 돌린다.
//   npm run test:e2e
// - 빈 포트를 골라 띄우므로 다른 작업자의 wrangler dev 와 겹치지 않는다. 끝나면 띄운 것만 끈다.
// - 견본 교회 열쇠가 없으면 scripts/seed.mjs 로 만든다 (.secrets/, Git 제외).
// - WP_BASE 를 주면 이미 떠 있는 클라우드에 대고 돌린다 (그때는 띄우지 않는다).
import { spawn, execFileSync } from 'node:child_process';
import { existsSync, readdirSync } from 'node:fs';
import { createServer } from 'node:net';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const cloud = join(dirname(fileURLToPath(import.meta.url)), '..');
const wrangler = join(cloud, 'node_modules', 'wrangler', 'bin', 'wrangler.js');
process.env.WRANGLER_SEND_METRICS = 'false';

const freePort = () => new Promise(res => { const s = createServer(); s.listen(0, '127.0.0.1', () => { const p = s.address().port; s.close(() => res(p)); }); });

async function main() {
  if (!existsSync(join(cloud, '.secrets', 'sample.json')) || !existsSync(join(cloud, '.dev.vars'))) {
    execFileSync(process.execPath, [join(cloud, 'scripts', 'seed.mjs')], { cwd: cloud, stdio: 'inherit' });
  }
  execFileSync(process.execPath, [join(cloud, 'scripts', 'assets.mjs')], { cwd: cloud, stdio: 'inherit' });
  // 앞 실행 뒤에 새 마이그레이션이 생겼어도 로컬 D1 을 맞춘다 (2단계 0002_* ~)
  if (!process.env.WP_BASE) execFileSync(process.execPath, [wrangler, 'd1', 'migrations', 'apply', 'worshippresenter-web', '--local'], { cwd: cloud, stdio: 'ignore' });
  // 앞 실행의 틀린 횟수가 남아 잠금이 걸리지 않게 (로컬 D1 만)
  if (!process.env.WP_BASE) execFileSync(process.execPath, [wrangler, 'd1', 'execute', 'worshippresenter-web', '--local', '--command', 'DELETE FROM auth_failures'], { cwd: cloud, stdio: 'ignore' });
  let dev = null;
  let base = process.env.WP_BASE;
  if (!base) {
    const port = await freePort();
    const inspector = await freePort();
    base = `http://127.0.0.1:${port}`;
    dev = spawn(process.execPath, [wrangler, 'dev', '--ip', '127.0.0.1', '--port', String(port), '--inspector-port', String(inspector), '--local', '--show-interactive-dev-session=false'], { cwd: cloud, stdio: ['ignore', 'pipe', 'pipe'] });
    let log = '';
    dev.stdout.on('data', d => { log += d; });
    dev.stderr.on('data', d => { log += d; });
    const t0 = Date.now();
    for (;;) {
      try { const r = await fetch(base + '/health'); if (r.ok) break; } catch { /* 아직 */ }
      if (dev.exitCode !== null || Date.now() - t0 > 90_000) { console.error(log); throw new Error('wrangler dev 가 뜨지 않았습니다'); }
      await new Promise(r => setTimeout(r, 300));
    }
    console.log(`로컬 클라우드: ${base}`);
  }
  // 기본: test/e2e/*.test.mjs (1단계) + test/data-e2e/*.test.mjs (2단계 A) + test/assets-e2e/*.test.mjs (2단계 C)
  const all = ['e2e', 'data-e2e', 'assets-e2e'].flatMap(d => existsSync(join(cloud, 'test', d)) ? readdirSync(join(cloud, 'test', d)).filter(f => f.endsWith('.test.mjs')).sort().map(f => join('test', d, f)) : []);
  const files = process.argv.slice(2).length ? process.argv.slice(2) : all;
  const t = spawn(process.execPath, ['--test', '--test-concurrency=1', '--test-timeout=30000', '--test-force-exit', ...files], { cwd: cloud, stdio: 'inherit', env: { ...process.env, WP_BASE: base } });
  const code = await new Promise(r => t.on('exit', r));
  if (dev) {
    if (process.platform === 'win32') { try { execFileSync('taskkill', ['/pid', String(dev.pid), '/t', '/f'], { stdio: 'ignore' }); } catch { /* 이미 꺼짐 */ } } else dev.kill('SIGTERM');
  }
  process.exit(code ?? 1);
}
main().catch(e => { console.error(e); process.exit(1); });
