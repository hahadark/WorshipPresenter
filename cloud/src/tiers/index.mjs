// 계정 등급: 교회마다 demo · free · paid 를 D1 churches.tier 에 두고, 요청마다 '이 등급에서 되나 · 한도 안인가 · 어느 저장소인가' 를 본다.
// 길은 하나다 — /api/v1/documents · sync · bible · assets 는 등급과 상관없이 같은 모양이고, 뒤의 저장소만 다르다 (docs/web/tiers.md).
import { can, fail, json } from '../protocol.mjs';
import { STORE, TIER_LIMITS, TIER_NAMES, dayOf, gate, limitsOf } from './limits.mjs';

/** 사람이 고를 수 없는 교회 ID (데모 · 서비스 길) */
export const RESERVED_CHURCHES = ['demo', 'admin', 'api', 'www', 'login', 'cloud', 'google'];

const memo = new Map(); // 교회 → { at, v }
export function forgetTier(church) { memo.delete(church); }
const cacheMs = env => { const n = Number(env.TIER_CACHE_MS); return Number.isFinite(n) && n >= 0 ? n : 30_000; };

/** 교회 → { tier, store, moving, moveTo, moveSince } (Worker 인스턴스마다 30초 기억 — 등급을 바꾼 뒤 옮기기는 그만큼 기다린다) */
export async function tierOf(env, church, now = Date.now()) {
  const hit = memo.get(church);
  if (hit && now - hit.at < cacheMs(env)) return hit.v;
  const r = await env.DB.prepare('SELECT tier, move_to AS moveTo, move_since AS moveSince FROM churches WHERE id = ?1').bind(church).first();
  const tier = r && TIER_LIMITS[r.tier] ? r.tier : 'paid';
  const v = { tier, store: STORE[tier], moving: !!r?.moveTo, moveTo: r?.moveTo || null, moveSince: r?.moveSince || null };
  if (memo.size > 5000) memo.clear();
  memo.set(church, { at: now, v });
  return v;
}

/** 쓰기인가 (등급을 옮기는 동안 막는 것) */
export function isWrite(r) {
  if (r.kind === 'data') return ['put', 'delete', 'push', 'importVersion', 'importBook', 'deleteVersion'].includes(r.op);
  if (r.kind === 'assets') return !['list', 'usage', 'fonts', 'get'].includes(r.op);
  return false;
}

// ---------- 하루 요청 수 (Worker 메모리에 모았다가 FLUSH 마다 D1 에 더함 — 그래서 어림) ----------
const counters = new Map(); // `${church}/${day}` → { base, pending, at }
const FLUSH_N = 50, FLUSH_MS = 60_000;
export async function countRequest(env, church, tier, now = Date.now()) {
  const L = limitsOf(tier, env);
  if (!L.requestsPerDay) return null;
  const day = dayOf(now);
  const k = `${church}/${day}`;
  let c = counters.get(k);
  if (!c) {
    const row = await env.DB.prepare('SELECT requests FROM tier_usage WHERE church_id = ?1 AND day = ?2').bind(church, day).first();
    c = { base: row?.requests || 0, pending: 0, at: now };
    if (counters.size > 5000) counters.clear();
    counters.set(k, c);
  }
  c.pending++;
  if (c.pending >= FLUSH_N || now - c.at > FLUSH_MS) {
    const n = c.pending;
    c.pending = 0; c.at = now;
    const r = await env.DB.prepare(
      'INSERT INTO tier_usage (church_id, day, requests) VALUES (?1, ?2, ?3) ON CONFLICT (church_id, day) DO UPDATE SET requests = requests + ?3 RETURNING requests',
    ).bind(church, day, n).first().catch(() => null);
    c.base = r ? r.requests : c.base + n;
  }
  if (c.base + c.pending > L.requestsPerDay) {
    return fail(429, '오늘 이 교회의 요청 한도를 다 썼습니다. 내일(UTC 0시) 다시 쓸 수 있습니다.', { code: 'tier_limit', limit: 'requestsPerDay', tier });
  }
  return null;
}
export function requestsToday(church, now = Date.now()) {
  const c = counters.get(`${church}/${dayOf(now)}`);
  return c ? c.base + c.pending : null;
}

// ---------- 문서 수 (5분 기억, 만들 때만 본다) ----------
const docCounts = new Map();
export async function documentCount(env, church, store) {
  const hit = docCounts.get(church);
  if (hit && Date.now() - hit.at < 5 * 60_000) return hit.n;
  const table = store === 'drive' ? 'drive_docs' : 'documents';
  const extra = store === 'drive' ? ' AND revision > 0' : '';
  const r = await env.DB.prepare(`SELECT COUNT(*) AS n FROM ${table} WHERE church_id = ?1 AND deleted_at IS NULL${extra}`).bind(church).first();
  if (docCounts.size > 5000) docCounts.clear();
  docCounts.set(church, { n: r?.n || 0, at: Date.now() });
  return r?.n || 0;
}
export const bumpDocuments = (church, d) => { const h = docCounts.get(church); if (h) h.n += d; };

/**
 * 요청 하나의 등급 확인. index.mjs 가 로그인 · 권한(can) 을 본 뒤, 자료 · 파일 · AI · 사람 길에서 부른다.
 * → Response(막음) | tier 정보 (s.tier 로 붙여 각 모듈이 저장소를 고른다)
 */
export async function prepare(request, env, url, s, r) {
  const t = await tierOf(env, s.church);
  const g = gate(t.tier, r, env);
  if (g) return fail(g.status, g.error, { code: g.code, limit: g.limit, tier: g.tier });
  if (t.moving && isWrite(r)) return fail(423, '저장소를 옮기는 중이라 잠시 고칠 수 없습니다. 송출 · 보기는 그대로 됩니다.', { code: 'tier_moving', tier: t.tier, moveTo: t.moveTo });
  const lim = await countRequest(env, s.church, t.tier);
  if (lim) return lim;
  // 새 문서 만들기 (If-Match 없는 PUT · 동기화 올리기의 baseRevision 0) — 한도에 닿았을 때만 본문을 미리 본다
  const L = limitsOf(t.tier, env);
  const creating = r.kind === 'data' && ((r.op === 'put' && !request.headers.get('if-match')) || r.op === 'push');
  if (creating && L.documents !== null) {
    const n = await documentCount(env, s.church, t.store);
    if (n >= L.documents) {
      let makes = r.op === 'put';
      if (r.op === 'push') {
        const b = await request.clone().json().catch(() => null);
        makes = !!b?.mutations?.some?.(m => m && m.op === 'put' && !m.baseRevision);
      }
      if (makes) return fail(429, `이 등급은 문서를 ${L.documents}개까지 둡니다. 안 쓰는 것을 지워 주세요.`, { code: 'tier_limit', limit: 'documents', tier: t.tier });
    }
  }
  return t;
}

/** 한 화면(출력) · 리모컨 수 한도를 중계실에 알린다 (index.mjs toRoom 이 머리글로) */
export function roomLimits(tier, env) {
  const L = limitsOf(tier, env);
  return { outputs: L.outputs, remotes: L.remotes };
}

// ---------- /api/v1/tier · /api/v1/drive ----------
export function tierRoute(method, path) {
  const get = method === 'GET' || method === 'HEAD';
  if (path === '/api/v1/tier' && get) return { kind: 'tiers', cap: null, op: 'tier' };
  if (path === '/api/v1/drive' && get) return { kind: 'tiers', cap: 'read', op: 'drive' };
  if (path === '/api/v1/drive/disconnect' && method === 'POST') return { kind: 'tiers', cap: 'manage', op: 'disconnect' };
  return null;
}

export async function handle(request, env, url, s, r) {
  const t = await tierOf(env, s.church);
  if (r.op === 'tier') {
    const L = limitsOf(t.tier, env);
    const out = { ok: true, tier: t.tier, name: TIER_NAMES[t.tier], store: t.store, moving: t.moving, moveTo: t.moveTo, limits: L, usage: { requestsToday: requestsToday(s.church) } };
    if (t.store === 'drive') {
      const { accountOf } = await import('../drive/google.mjs');
      const a = await accountOf(env, s.church);
      out.drive = { connected: !!a && !a.disconnected_at && !!a.refresh_sealed, reason: a?.disconnect_reason ?? null, reconnect: can(s.role, 'manage') ? '/auth/google/start?mode=connect' : null };
    }
    if (t.tier === 'demo') out.demo = { resetHours: L.resetHours, note: '둘러보기는 누구나 같이 보는 견본 교회입니다. 개인 정보를 넣지 마세요. 고친 것은 하루 안에 처음 모습으로 돌아갑니다.' };
    return json(out);
  }
  const oauth = await import('../drive/oauth.mjs');
  if (r.op === 'drive') return oauth.status(request, env, url, s, can(s.role, 'manage'));
  if (r.op === 'disconnect') return oauth.disconnect(request, env, url, s);
  return fail(404, '없는 주소입니다.');
}
