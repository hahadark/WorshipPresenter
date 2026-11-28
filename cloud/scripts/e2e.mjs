// 끝에서 끝 시험: 로컬 클라우드(wrangler dev, 로컬 D1 · Durable Object)를 띄우고 test/e2e/*.test.mjs 를 돌린다.
//   npm run test:e2e
// - 빈 포트를 골라 띄우므로 다른 작업자의 wrangler dev 와 겹치지 않는다. 끝나면 띄운 것만 끈다.
// - 견본 교회 열쇠가 없으면 scripts/seed.mjs 로 만든다 (.secrets/, Git 제외).
// - WP_BASE 를 주면 이미 떠 있는 클라우드에 대고 돌린다 (그때는 띄우지 않는다).
import { spawn, execFileSync } from 'node:child_process';
import { existsSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:net';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { CLIENT_ID, CLIENT_SECRET, startFakeGoogle } from '../test/fake-google/server.mjs'; // 7단계 무료 등급 시험 (진짜 구글은 부르지 않음)

const cloud = join(dirname(fileURLToPath(import.meta.url)), '..');
const wrangler = join(cloud, 'node_modules', 'wrangler', 'bin', 'wrangler.js');
process.env.WRANGLER_SEND_METRICS = 'false';

const freePort = () => new Promise(res => { const s = createServer(); s.listen(0, '127.0.0.1', () => { const p = s.address().port; s.close(() => res(p)); }); });

async function main() {
  if (!existsSync(join(cloud, '.secrets', 'sample.json')) || !existsSync(join(cloud, '.dev.vars'))) {
    execFileSync(process.execPath, [join(cloud, 'scripts', 'seed.mjs')], { cwd: cloud, stdio: 'inherit' });
  }
  execFileSync(process.execPath, [join(cloud, 'scripts', 'assets.mjs')], { cwd: cloud, stdio: 'inherit' });
  // 로컬에서만 가짜 AI 공급자를 켠다 (AI_MOCK=1, 배포에는 넣지 않음 — docs/web/ai.md)
  { const dv = join(cloud, '.dev.vars'); const t = readFileSync(dv, 'utf8'); if (!/^AI_MOCK=/m.test(t)) writeFileSync(dv, t.trimEnd() + '\nAI_MOCK=1\n'); }
  // 앞 실행 뒤에 새 마이그레이션이 생겼어도 로컬 D1 을 맞춘다 (2단계 0002_* ~)
  if (!process.env.WP_BASE) execFileSync(process.execPath, [wrangler, 'd1', 'migrations', 'apply', 'worshippresenter-web', '--local'], { cwd: cloud, stdio: 'ignore' });
  // 앞 실행의 틀린 횟수가 남아 잠금이 걸리지 않게 (로컬 D1 만)
  if (!process.env.WP_BASE) execFileSync(process.execPath, [wrangler, 'd1', 'execute', 'worshippresenter-web', '--local', '--command', 'DELETE FROM auth_failures; DELETE FROM members' /* 로컬 시험 D1 에 사람 계정이 실행마다 쌓여 50개 한도에 걸리지 않게 */], { cwd: cloud, stdio: 'ignore' });
  let dev = null;
  let base = process.env.WP_BASE;
  // 가짜 구글 (OAuth + Drive v3). 로컬 클라우드에만 붙인다 — GOOGLE_FAKE 는 127.0.0.1 주소만 받는다 (drive/google.mjs)
  const fake = !base ? await startFakeGoogle() : null;
  if (!base) {
    const port = await freePort();
    const inspector = await freePort();
    base = `http://127.0.0.1:${port}`;
    dev = spawn(process.execPath, [wrangler, 'dev', '--ip', '127.0.0.1', '--port', String(port), '--inspector-port', String(inspector), '--local', '--show-interactive-dev-session=false',
      '--var', `GOOGLE_FAKE:${fake.url}`, '--var', `GOOGLE_CLIENT_ID:${CLIENT_ID}`, '--var', `GOOGLE_CLIENT_SECRET:${CLIENT_SECRET}`,
      // 등급 기억을 끄고(옮기기 시험이 30초를 기다리지 않게) 데모 주소별 한도를 시험하기 쉽게 줄인다
      '--var', 'TIER_CACHE_MS:0', '--var', 'SERVICE_OVERRIDES:{"freeSignupsPerDay":100000,"signupsPerIpPerHour":1000}', '--var', 'TIER_OVERRIDES:{"demo":{"perIpPerMinute":40,"entriesPerIpPerHour":200}}'], { cwd: cloud, stdio: ['ignore', 'pipe', 'pipe'] });
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
  // 기본: test/e2e/*.test.mjs (1단계) + test/data-e2e/*.test.mjs (2단계 A) + test/assets-e2e/*.test.mjs (2단계 C) + test/ai-e2e/*.test.mjs (5단계) + test/media-e2e/*.test.mjs (4단계 B) + test/tiers-e2e/*.test.mjs (7단계 등급)
  const all = ['e2e', 'data-e2e', 'assets-e2e', 'ai-e2e', 'media-e2e', 'tiers-e2e'].flatMap(d => existsSync(join(cloud, 'test', d)) ? readdirSync(join(cloud, 'test', d)).filter(f => f.endsWith('.test.mjs')).sort().map(f => join('test', d, f)) : []);
  const files = process.argv.slice(2).length ? process.argv.slice(2) : all;
  const t = spawn(process.execPath, ['--test', '--test-concurrency=1', '--test-timeout=30000', '--test-force-exit', ...files], { cwd: cloud, stdio: 'inherit', env: { ...process.env, WP_BASE: base, ...(fake ? { WP_FAKE_GOOGLE: fake.url } : {}) } });
  const code = await new Promise(r => t.on('exit', r));
  if (dev) {
    if (process.platform === 'win32') { try { execFileSync('taskkill', ['/pid', String(dev.pid), '/t', '/f'], { stdio: 'ignore' }); } catch { /* 이미 꺼짐 */ } } else dev.kill('SIGTERM');
  }
  if (fake) await fake.close();
  process.exit(code ?? 1);
}
main().catch(e => { console.error(e); process.exit(1); });
