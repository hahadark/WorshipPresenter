// 견본 교회와 기기 열쇠 만들기 (로컬 또는 미리보기 배포용).
//   node scripts/seed.mjs                       → 로컬 D1 (wrangler dev 가 쓰는 .wrangler/) 에 'sample' 교회
//   node scripts/seed.mjs --church my-church --name "우리 교회"
//   node scripts/seed.mjs --remote --config wrangler.preview.jsonc   → 배포한 D1 에 (AUTH_SECRET 은 .secrets/auth.json 의 값)
// - AUTH_SECRET 이 없으면 무작위로 만들어 .dev.vars(로컬) · .secrets/auth.json 에 둔다. 둘 다 Git 에 넣지 않는다.
// - 만든 열쇠 원문은 .secrets/<교회>.json 에만 저장한다 (D1 에는 HMAC 해시만).
import { createHmac, randomBytes, randomInt } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, writeFileSync, rmSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';

const cloud = join(dirname(fileURLToPath(import.meta.url)), '..');
const args = process.argv.slice(2);
const opt = (k, d) => { const i = args.indexOf(k); return i >= 0 ? args[i + 1] : d; };
const remote = args.includes('--remote');
const church = opt('--church', 'sample');
const name = opt('--name', church === 'sample' ? '견본 교회' : church);
const config = opt('--config', 'wrangler.jsonc');
if (!/^[a-z0-9][a-z0-9-]{0,38}[a-z0-9]$/.test(church)) throw new Error('교회 ID 는 영문 소문자 · 숫자 · - 로 2~40자');

const secretsDir = join(cloud, '.secrets');
mkdirSync(secretsDir, { recursive: true });
const authFile = join(secretsDir, remote ? 'auth.remote.json' : 'auth.json');
let auth = existsSync(authFile) ? JSON.parse(readFileSync(authFile, 'utf8')) : null;
if (!auth) {
  auth = { AUTH_SECRET: randomBytes(32).toString('base64url'), ADMIN_TOKEN: randomBytes(32).toString('base64url') };
  writeFileSync(authFile, JSON.stringify(auth, null, 2));
}
if (!remote) {
  // wrangler dev 가 읽는 비밀값 파일
  // AI_MOCK=1: 가짜 AI 공급자(mock://)를 로컬에서만 허락 (docs/web/ai.md). 배포 비밀값에는 넣지 않는다 (deploy.mjs 는 AUTH_SECRET 만)
  writeFileSync(join(cloud, '.dev.vars'), `AUTH_SECRET=${auth.AUTH_SECRET}\nADMIN_TOKEN=${auth.ADMIN_TOKEN}\nAI_MOCK=1\n`);
}

const ALPHA = 'ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789';
const text = n => Array.from({ length: n }, () => ALPHA[randomInt(ALPHA.length)]).join('');
const secretFor = role => (role === 'remote' || role === 'editor' ? String(randomInt(100000, 1000000)) : text(32));
const hash = (role, secret) => createHmac('sha256', auth.AUTH_SECRET).update(`cred:${church}:${role}:${secret}`).digest('hex');
const q = s => `'${String(s).replace(/'/g, "''")}'`;

const now = Date.now();
const made = {};
const sql = [`INSERT INTO churches (id, name, created_at) VALUES (${q(church)}, ${q(name)}, ${now}) ON CONFLICT (id) DO NOTHING;`];
for (const role of ['owner', 'bridge', 'output', 'remote', 'editor']) {
  const id = text(10), secret = secretFor(role);
  made[role] = { id, secret, key: role === 'output' || role === 'bridge' ? `${church}.${secret}` : undefined };
  sql.push(`INSERT INTO credentials (church_id, id, role, label, secret_hash, created_at) VALUES (${q(church)}, ${q(id)}, ${q(role)}, ${q('견본으로 만든 ' + role)}, ${q(hash(role, secret))}, ${now});`);
}
const tmp = join(secretsDir, `seed-${now}.sql`);
writeFileSync(tmp, sql.join('\n'));
const wr = (...a) => execFileSync(process.execPath, [join(cloud, 'node_modules', 'wrangler', 'bin', 'wrangler.js'), ...a], { cwd: cloud, stdio: ['ignore', 'inherit', 'inherit'] });
try {
  wr('d1', 'migrations', 'apply', 'worshippresenter-web', remote ? '--remote' : '--local', '-c', config);
  wr('d1', 'execute', 'worshippresenter-web', remote ? '--remote' : '--local', '-c', config, '--file', tmp, '-y');
} finally { rmSync(tmp, { force: true }); }
const out = join(secretsDir, `${church}${remote ? '.remote' : ''}.json`);
writeFileSync(out, JSON.stringify({ church, name, createdAt: new Date(now).toISOString(), credentials: made }, null, 2));
console.log(`교회 '${church}' 열쇠를 만들었습니다 → ${out} (Git 에 넣지 마세요)`);
