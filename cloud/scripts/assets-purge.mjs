// 지운 파일 정리 명령 (2단계 C) — '지우기' 는 표시만 하므로, R2 에서 실제로 지우는 것은 이 명령으로만 한다.
//   node scripts/assets-purge.mjs                         → 로컬(127.0.0.1:8787) 'sample' 교회: 지울 것을 보여 주기만
//   node scripts/assets-purge.mjs --days 30 --yes         → 지운 지 30일 지난 것을 R2 에서 지움 (+ 하루 넘게 끝나지 않은 나눠 올리기를 그만둠)
//   node scripts/assets-purge.mjs --days 0 --now --yes    → 지운 표시가 있는 것을 모두 지금 지움
//   node scripts/assets-purge.mjs --usage                 → 사용량 · 무료 범위만 보기
//   --base https://<배포 주소> --church <교회>  (주인 열쇠: 환경변수 WP_OWNER_SECRET, 없으면 .secrets/<교회>[.remote].json)
// - 살아 있는 파일(지운 표시가 없거나 기다림 안에 지운 것)이 하나라도 가리키는 내용은 지우지 않는다.
// - 열쇠는 화면에 찍지 않는다.
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const cloud = join(dirname(fileURLToPath(import.meta.url)), '..');
const args = process.argv.slice(2);
const opt = (k, d) => { const i = args.indexOf(k); return i >= 0 ? args[i + 1] : d; };
const base = opt('--base', 'http://127.0.0.1:8787').replace(/\/$/, '');
const church = opt('--church', 'sample');
const days = Number(opt('--days', '30'));
const remote = !/^http:\/\/(127\.0\.0\.1|localhost)/.test(base);

let secret = process.env.WP_OWNER_SECRET || '';
if (!secret) {
  const f = join(cloud, '.secrets', `${church}${remote ? '.remote' : ''}.json`);
  if (existsSync(f)) secret = JSON.parse(readFileSync(f, 'utf8')).credentials?.owner?.secret || '';
}
if (!secret) { console.error('주인 열쇠가 없습니다 (WP_OWNER_SECRET 또는 .secrets/<교회>.json).'); process.exit(2); }

const H = { 'content-type': 'application/json', 'x-worshipcue': '1', origin: base };
const login = await fetch(`${base}/auth/login`, { method: 'POST', headers: H, body: JSON.stringify({ church, secret }) });
if (!login.ok) { console.error(`로그인 실패 ${login.status}`); process.exit(1); }
const cookie = login.headers.getSetCookie().map(c => c.split(';')[0]).join('; ');
const call = async (path, init = {}) => {
  const r = await fetch(base + path, { ...init, headers: { ...H, cookie, ...(init.headers || {}) } });
  const j = await r.json().catch(() => ({}));
  if (!r.ok) { console.error(`${path} 실패 ${r.status}: ${j.error || ''}`); process.exit(1); }
  return j;
};
const mb = n => `${(n / 1e6).toFixed(1)} MB`;

const u = await call('/api/v1/assets/usage');
console.log(`교회 ${church}: 파일 ${u.church.assets}개 · 저장 ${mb(u.church.bytes)} (지운 표시 ${u.church.deletedAssets}개 · 정리하면 줄 수 있는 것 ${mb(u.church.deletedBytes)}) · 이 달 A ${u.church.classA} · B ${u.church.classB}`);
if (u.free) {
  const f = u.free;
  console.log(`계정 무료 범위 (${u.month}): 저장 ${mb(f.storage.used)} / ${mb(f.storage.limit)} (멈춤 ${mb(f.storage.stopAt)}) · A ${f.classA.used}/${f.classA.limit} · B ${f.classB.used}/${f.classB.limit}${f.uploadsBlocked ? ` · 올리기 멈춤(${f.reason})` : ''}`);
}
if (!args.includes('--usage')) {
  const yes = args.includes('--yes');
  const body = { olderThanDays: days, dryRun: !yes, ...(args.includes('--now') ? { confirm: 'now' } : {}) };
  const r = await call('/api/v1/assets/purge', { method: 'POST', body: JSON.stringify(body) });
  for (const p of r.purged) console.log(`${yes ? '지움' : '지울 것'}: ${p.names || p.sha256.slice(0, 12)} (${mb(p.size)})`);
  for (const p of r.abortedUploads) console.log(`${yes ? '그만둠' : '그만둘 것'}: 나눠 올리기 ${p.name} (${mb(p.size)})`);
  console.log(`${yes ? '정리했습니다' : '보기만 했습니다 (--yes 로 지움)'}: ${r.purged.length}개 · ${mb(r.freedBytes)} · 나눠 올리기 ${r.abortedUploads.length}개`);
}
