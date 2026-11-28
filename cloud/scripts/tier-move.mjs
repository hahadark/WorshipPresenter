// 등급 옮기기 (서비스 관리자): 무료(구글 드라이브) ↔ 유료(D1 · R2). docs/web/tiers.md 6절
//   node scripts/tier-move.mjs --church c-abcd1234 --to paid                  로컬 (wrangler dev, ADMIN_TOKEN 은 .secrets/auth.json)
//   node scripts/tier-move.mjs --base https://<배포 주소> --church … --to free  배포 (ADMIN_TOKEN 은 WP_ADMIN_TOKEN 환경변수)
//   … --info · --cancel · --cleanup
// 결제는 없다 — 관리자가 손으로 바꾼다. 옮기는 동안 그 교회의 쓰기는 423, 송출 · 보기는 그대로.
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const cloud = join(dirname(fileURLToPath(import.meta.url)), '..');
const args = process.argv.slice(2);
const opt = (k, d) => { const i = args.indexOf(k); return i >= 0 ? args[i + 1] : d; };
const has = k => args.includes(k);
const base = opt('--base', 'http://127.0.0.1:8787');
const church = opt('--church');
if (!church) { console.error('--church <교회 ID> 가 필요합니다.'); process.exit(2); }
const local = existsSync(join(cloud, '.secrets', 'auth.json')) ? JSON.parse(readFileSync(join(cloud, '.secrets', 'auth.json'), 'utf8')).ADMIN_TOKEN : '';
const token = process.env.WP_ADMIN_TOKEN || (base.startsWith('http://127.0.0.1') ? local : '');
if (!token) { console.error('관리자 열쇠가 없습니다 (WP_ADMIN_TOKEN).'); process.exit(2); }
const call = (path, body, method = 'POST') => fetch(`${base}/api/v1/admin/churches/${church}${path}`, { method, headers: { authorization: `Bearer ${token}`, 'content-type': 'application/json' }, body: body ? JSON.stringify(body) : undefined })
  .then(async r => ({ status: r.status, json: await r.json().catch(() => ({})) }));

if (has('--info')) { console.log(JSON.stringify((await call('', null, 'GET')).json, null, 2)); process.exit(0); }
if (has('--cancel')) { console.log((await call('/move/cancel')).json); process.exit(0); }
if (has('--cleanup')) { console.log((await call('/move/cleanup')).json); process.exit(0); }
const to = opt('--to');
let r = await call('/move', { to });
if (r.status !== 200) { console.error('시작하지 못함:', r.status, r.json); process.exit(1); }
console.log(`옮기기 시작: ${r.json.from} → ${r.json.to} (쓰기 잠시 멈춤)`);
for (;;) {
  r = await call('/move/step');
  if (r.status === 425) { await new Promise(x => setTimeout(x, (r.json.retryAfter || 1) * 1000)); continue; }
  if (r.status !== 200) { console.error('단계 실패:', r.status, r.json, '\n다시 실행하면 이어서 합니다 (--cancel 로 그만두기).'); process.exit(1); }
  if (r.json.done) { console.log(`끝: 이제 ${r.json.tier}. 옛 저장소 정리는 확인 뒤 --cleanup`); break; }
  process.stdout.write(`  ${r.json.phase} ${r.json.copied}\n`);
}
