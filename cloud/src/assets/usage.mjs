// R2 무료 범위 안전장치 · 교회별 사용량 (D1 asset_blobs · asset_uploads · asset_usage · asset_limits).
// R2 에는 지출 상한 설정이 없다 → 올리기 전에 계정 전체 저장량 · 이 달 A 작업 수가 무료 범위의 80% 를 넘지 않는지 본다.
// 교회 단위가 아닌 질의는 여기 guardRow 의 합계(SUM) 둘뿐이다 — 이 계정의 R2 저장통은 하나이고 무료 범위는 계정 전체 합이라서. 합만 쓰고 다른 교회 줄은 꺼내지 않는다.
import { freeLimits, monthOf, quotaDecision } from './routes.mjs';

/** 받기(B 작업)는 요청마다 D1 에 쓰면 D1 쓰기 한도(10만/일)를 먹으므로, Worker 안에 모았다가 1분마다 한 번 더한다 */
const pendingB = new Map(); // church → 수
let lastFlush = Date.now();
const FLUSH_MS = 60_000;

export const addA = (env, church, n = 1, now = Date.now()) => env.DB.prepare(
  `INSERT INTO asset_usage (church_id, month, class_a, class_b) VALUES (?1, ?2, ?3, 0)
   ON CONFLICT (church_id, month) DO UPDATE SET class_a = class_a + ?3`,
).bind(church, monthOf(now), n);

export async function countB(env, church) {
  pendingB.set(church, (pendingB.get(church) || 0) + 1);
  if (Date.now() - lastFlush >= FLUSH_MS) await flushB(env);
}
export async function flushB(env) {
  lastFlush = Date.now();
  if (!pendingB.size) return;
  const month = monthOf();
  const list = [...pendingB.entries()];
  pendingB.clear();
  try {
    await env.DB.batch(list.map(([church, n]) => env.DB.prepare(
      `INSERT INTO asset_usage (church_id, month, class_a, class_b) VALUES (?1, ?2, 0, ?3)
       ON CONFLICT (church_id, month) DO UPDATE SET class_b = class_b + ?3`,
    ).bind(church, month, n)));
  } catch (e) {
    // 세는 것 때문에 받기를 멈추지 않는다. 못 더한 수는 다음에 다시
    for (const [c, n] of list) pendingB.set(c, (pendingB.get(c) || 0) + n);
    console.error('R2 받기 수를 더하지 못함', e?.message || e);
  }
}

/** 안전장치에 쓰는 수 (D1 질의 하나) */
export async function guardRow(env, church, now = Date.now()) {
  return env.DB.prepare(
    `SELECT
       (SELECT COALESCE(SUM(size), 0) FROM asset_blobs WHERE purged_at IS NULL AND church_id NOT IN (SELECT id FROM churches WHERE tier = 'free')) AS storedBytes, -- 7단계: 무료(드라이브) 교회는 R2 를 쓰지 않음
       (SELECT COALESCE(SUM(size), 0) FROM asset_uploads) AS reservedBytes,
       (SELECT COALESCE(SUM(class_a), 0) FROM asset_usage WHERE month = ?2) AS classA,
       (SELECT COALESCE(SUM(class_b), 0) FROM asset_usage WHERE month = ?2) AS classB,
       (SELECT COALESCE(SUM(size), 0) FROM asset_blobs WHERE church_id = ?1 AND purged_at IS NULL)
         + (SELECT COALESCE(SUM(size), 0) FROM asset_uploads WHERE church_id = ?1) AS churchBytes,
       (SELECT max_bytes FROM asset_limits WHERE church_id = ?1) AS churchLimit`,
  ).bind(church, monthOf(now)).first();
}

/** 올려도 되나 → { ok } | { ok:false, reason, row } */
export async function guard(env, church, addBytes, addOps) {
  const row = await guardRow(env, church);
  const d = quotaDecision({ ...row, addBytes, addA: addOps }, freeLimits(env));
  return { ...d, row };
}

const REASON = {
  storage: 'R2 무료 저장 범위(10 GB)의 80% 에 닿아 올리기를 멈췄습니다. 안 쓰는 파일을 지우고 정리해 주세요.',
  classA: '이 달 R2 올리기 작업 수가 무료 범위(100만)의 80% 에 닿아 올리기를 멈췄습니다. 다음 달에 다시 됩니다.',
  church: '이 교회에 정해진 저장 한도에 닿았습니다.',
};
/** 507 { ok:false, error:'quota', reason, message } — 설치판(2B)은 이때 올리기를 멈추고 1단계 중계로 돌아간다 */
export function quotaResponse(json, d) {
  return json({ ok: false, error: 'quota', reason: d.reason, message: REASON[d.reason] }, 507);
}

/** GET /api/v1/assets/usage — 교회 사용량. 주인(manage)에게는 계정 전체 무료 범위도 */
export async function usage(env, s, canManage, json) {
  await flushB(env);
  const month = monthOf();
  const c = await env.DB.prepare(
    `SELECT
       (SELECT COUNT(*) FROM assets WHERE church_id = ?1 AND deleted_at IS NULL AND purged_at IS NULL) AS assets,
       (SELECT COUNT(*) FROM assets WHERE church_id = ?1 AND deleted_at IS NOT NULL AND purged_at IS NULL) AS deletedAssets,
       (SELECT COUNT(*) FROM asset_blobs WHERE church_id = ?1 AND purged_at IS NULL) AS objects,
       (SELECT COALESCE(SUM(size), 0) FROM asset_blobs WHERE church_id = ?1 AND purged_at IS NULL) AS bytes,
       (SELECT COALESCE(SUM(b.size), 0) FROM asset_blobs b WHERE b.church_id = ?1 AND b.purged_at IS NULL
          AND NOT EXISTS (SELECT 1 FROM assets a WHERE a.church_id = ?1 AND a.sha256 = b.sha256 AND a.deleted_at IS NULL AND a.purged_at IS NULL)) AS deletedBytes,
       (SELECT COUNT(*) FROM asset_uploads WHERE church_id = ?1) AS uploads,
       (SELECT COALESCE(SUM(size), 0) FROM asset_uploads WHERE church_id = ?1) AS uploadingBytes,
       (SELECT COALESCE(SUM(class_a), 0) FROM asset_usage WHERE church_id = ?1 AND month = ?2) AS classA,
       (SELECT COALESCE(SUM(class_b), 0) FROM asset_usage WHERE church_id = ?1 AND month = ?2) AS classB,
       (SELECT max_bytes FROM asset_limits WHERE church_id = ?1) AS limitBytes`,
  ).bind(s.church, month).first();
  const out = { ok: true, month, church: { ...c, limitBytes: c.limitBytes ?? null } };
  if (canManage) {
    const L = freeLimits(env);
    const g = await guardRow(env, s.church);
    const d = quotaDecision({ ...g, addBytes: 0, addA: 1 }, L);
    const part = (used, limit) => ({ used, limit, stopAt: Math.floor(limit * L.ratio), ratio: Math.round((used / limit) * 10000) / 10000 });
    out.free = {
      storage: part(g.storedBytes + g.reservedBytes, L.bytes),
      classA: part(g.classA, L.classA),
      // 받기(B)는 넘어도 막지 않는다 (예배 중 화면이 멈추면 안 됨) — 80% 를 넘으면 warn 만
      classB: { ...part(g.classB, L.classB), warn: g.classB >= L.classB * L.ratio },
      uploadsBlocked: !d.ok && d.reason !== 'church',
      reason: d.ok ? null : d.reason,
    };
  }
  return json(out);
}
