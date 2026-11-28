// 배포: D1 id 를 채운 wrangler.preview.jsonc 를 만들고, D1 마이그레이션 → Worker 올리기(비밀값을 같은 판에).
//   WP_D1_DATABASE_ID=<id> WP_AUTH_SECRET=<32자 이상> node scripts/deploy.mjs
//   (이 PC 에서는 비밀값이 없으면 .secrets/auth.remote.json — scripts/seed.mjs --remote 가 만든 것 — 을 쓴다)
// - 비밀값은 화면 · 로그에 찍지 않는다. 임시 비밀값 파일은 올린 뒤 지운다.
// - --dry-run 이면 올리지 않고 묶음만 만든다.
import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, writeFileSync, rmSync, mkdirSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const cloud = join(dirname(fileURLToPath(import.meta.url)), '..');
const dry = process.argv.includes('--dry-run');
const wrangler = join(cloud, 'node_modules', 'wrangler', 'bin', 'wrangler.js');
const run = (...a) => execFileSync(process.execPath, [wrangler, ...a], { cwd: cloud, stdio: 'inherit', env: { ...process.env, WRANGLER_SEND_METRICS: 'false' } });

let d1 = process.env.WP_D1_DATABASE_ID || '';
let secret = process.env.WP_AUTH_SECRET || '';
let admin = process.env.WP_ADMIN_TOKEN || '';
const local = join(cloud, '.secrets', 'auth.remote.json');
if ((!secret || !d1) && existsSync(local)) {
  const a = JSON.parse(readFileSync(local, 'utf8'));
  secret ||= a.AUTH_SECRET || '';
  d1 ||= a.D1_DATABASE_ID || '';
}
if (!/^[0-9a-f-]{36}$/.test(d1)) { console.error('WP_D1_DATABASE_ID 가 없습니다 (wrangler d1 create worshippresenter-web 의 id).'); process.exit(2); }
if (secret.length < 32) { console.error('WP_AUTH_SECRET 이 없거나 32자보다 짧습니다.'); process.exit(2); }

const base = readFileSync(join(cloud, 'wrangler.jsonc'), 'utf8');
const conf = base.replace('"00000000-0000-0000-0000-000000000000"', JSON.stringify(d1));
if (conf === base) { console.error('wrangler.jsonc 의 D1 자리 표시를 찾지 못했습니다.'); process.exit(2); }
writeFileSync(join(cloud, 'wrangler.preview.jsonc'), conf);

execFileSync(process.execPath, [join(cloud, 'scripts', 'assets.mjs')], { cwd: cloud, stdio: 'inherit' });
mkdirSync(join(cloud, '.secrets'), { recursive: true });
const sf = join(cloud, '.secrets', `deploy-${Date.now()}.json`);
writeFileSync(sf, JSON.stringify(admin ? { AUTH_SECRET: secret, ADMIN_TOKEN: admin } : { AUTH_SECRET: secret }));
try {
  if (dry) {
    run('deploy', '-c', 'wrangler.preview.jsonc', '--dry-run', '--outdir', '.build');
  } else {
    run('d1', 'migrations', 'apply', 'worshippresenter-web', '--remote', '-c', 'wrangler.preview.jsonc');
    run('deploy', '-c', 'wrangler.preview.jsonc', '--secrets-file', sf);
  }
} finally {
  rmSync(sf, { force: true });
}
