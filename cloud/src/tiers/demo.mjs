// 둘러보기(데모): 로그인 없이 견본 교회 'demo' 하나를 여럿이 같이 본다. 문서는 유료와 같은 D1 길을 쓰되
//   - 하루(resetHours)마다 처음 모습으로 되돌린다: Cron 없이 '들어올 때 오래됐으면' 되돌림. 되돌림도 보통 쓰기(판 +1 · 변경 기록)라 동기화가 그대로 따라온다.
//   - 실시간 중계실은 방문자마다 따로(demo~d:<번호>) — 한 사람의 송출이 다른 방문자 화면에 가지 않는다.
//   - 파일 올리기 · AI · 사람 초대 · 기기 열쇠(오래 쓰는 송출 주소)는 막고(limits.mjs gate · 역할 권한), 로그인은 2시간.
//   - 우리 무료 한도를 다 먹지 않게: 주소마다 1분 요청 · 한 시간 들어오기 · 데모 전체 하루 요청 (limits.mjs).
import * as auth from '../auth.mjs';
import { fail, json } from '../protocol.mjs';
import { writeStatements } from '../data/store.mjs';
import { limitsOf } from './limits.mjs';
import { forgetTier } from './index.mjs';
import { DEMO_BIBLE, DEMO_DOCS, DEMO_SHAPE, demoVerse } from './demo-seed.mjs';

export const DEMO = 'demo';
/** 둘러보기를 켤지: Worker 변수 DEMO_ENABLED = '1' 일 때만 (기본 꺼짐 — 공개는 사관님이 정함). 끄면 /demo 404 · 이미 받은 데모 쿠키도 죽음 */
export const demoEnabled = env => env.DEMO_ENABLED === '1';
const rand = n => Array.from(crypto.getRandomValues(new Uint8Array(n)), b => 'abcdefghijkmnopqrstuvwxyz23456789'[b % 33]).join('');
const ipOf = request => request.headers.get('cf-connecting-ip') || 'local';

// ---------- 주소마다 요청 수 (Worker 메모리 — 어림) ----------
const perMin = new Map();
const entries = new Map();
function bump(map, key, limit) {
  const n = (map.get(key) || 0) + 1;
  if (map.size > 20000) map.clear();
  map.set(key, n);
  return n > limit;
}
/** 데모 로그인의 요청마다 (index.mjs) */
export function demoRate(request, env, now = Date.now()) {
  const L = limitsOf('demo', env);
  if (bump(perMin, `${ipOf(request)}/${Math.floor(now / 60_000)}`, L.perIpPerMinute)) {
    return fail(429, '둘러보기 요청이 너무 잦습니다. 잠시 뒤 다시 해 보세요.', { code: 'tier_limit', limit: 'perIpPerMinute', tier: 'demo' });
  }
  return null;
}

/** 데모 쿠키의 열쇠('d:…')가 살아 있나 — 데모 교회가 있고 막히지 않았으면 (30초 기억) */
const alive = { at: 0, ok: false };
export async function demoAlive(env, church, now = Date.now()) {
  if (church !== DEMO || !demoEnabled(env)) return false;
  if (now - alive.at < 30_000) return alive.ok;
  const r = await env.DB.prepare("SELECT 1 AS x FROM churches WHERE id = ?1 AND tier = 'demo' AND disabled_at IS NULL").bind(DEMO).first();
  alive.at = now; alive.ok = !!r;
  return alive.ok;
}

// ---------- 처음 모습 ----------
const q = t => `'${String(t).replace(/'/g, "''")}'`;
async function createDemo(env, now) {
  const verses = DEMO_SHAPE.flatMap(([b, c, n]) => Array.from({ length: n }, (_, i) => `(?1,?2,${b},${c},${i + 1},${q(demoVerse(b, c, i + 1))})`));
  const structure = {};
  for (const [b, c, n] of DEMO_SHAPE) { structure[b] = structure[b] || []; while (structure[b].length < c) structure[b].push(0); structure[b][c - 1] = n; }
  // 장 수를 맞추려고 빈 장(0절)이 생길 수 있다 → 그 장을 부르면 '본문이 없습니다'
  await env.DB.batch([
    env.DB.prepare("INSERT INTO churches (id, name, created_at, tier, tier_changed_at) VALUES (?1, '견본 교회 (둘러보기)', ?2, 'demo', ?2) ON CONFLICT (id) DO NOTHING").bind(DEMO, now),
    env.DB.prepare(`INSERT INTO bible_versions (church_id, id, name, license_note, verses, language, structure, imported_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) ON CONFLICT (church_id, id) DO NOTHING`)
      .bind(DEMO, DEMO_BIBLE.id, DEMO_BIBLE.name, DEMO_BIBLE.licenseNote, verses.length, DEMO_BIBLE.language, JSON.stringify(structure), now),
    env.DB.prepare(`INSERT OR IGNORE INTO bible_verses (church_id, version, book, chapter, verse, text) VALUES ${verses.join(',')}`).bind(DEMO, DEMO_BIBLE.id),
    env.DB.prepare("INSERT OR IGNORE INTO bible_headings (church_id, version, book, chapter, verse, ord, text) VALUES (?1, ?2, 43, 3, 16, 0, '(견본 소제목) 사랑')").bind(DEMO, DEMO_BIBLE.id),
  ]);
  forgetTier(DEMO);
}

/** 문서를 처음 모습으로: 견본에 있는 것은 그 본문으로(같으면 그대로), 없는 것은 지움. 보통 쓰기라 판 · 변경 기록이 이어진다 */
export async function resetDemo(env, now = Date.now()) {
  const cur = (await env.DB.prepare('SELECT kind, id, revision, deleted_at AS deletedAt, payload FROM documents WHERE church_id = ?1').bind(DEMO).all()).results;
  const byKey = new Map(cur.map(x => [`${x.kind}/${x.id}`, x]));
  const want = new Map(DEMO_DOCS.map(d => [`${d.kind}/${d.id}`, d]));
  const stmts = [];
  const actor = 'demo-reset';
  for (const [k, d] of want) {
    const text = JSON.stringify(d.payload);
    const c = byKey.get(k);
    if (c && !c.deletedAt && c.payload === text) continue;
    stmts.push(...writeStatements(env, DEMO, actor, { kind: d.kind, id: d.id, op: 'put', base: c ? c.revision : 0, text, title: d.payload.name || d.payload.title || '', size: text.length }, now));
  }
  for (const [k, c] of byKey) {
    if (want.has(k) || c.deletedAt) continue;
    stmts.push(...writeStatements(env, DEMO, actor, { kind: c.kind, id: c.id, op: 'delete', base: c.revision }, now));
  }
  for (let i = 0; i < stmts.length; i += 200) await env.DB.batch(stmts.slice(i, i + 200));
  return stmts.length / 2;
}

/** 데모 교회가 있게 · 오래됐으면 되돌리기 (들어올 때마다) → { reset } | Response(오류) */
export async function ensureDemo(env, now = Date.now(), force = false) {
  let row = await env.DB.prepare('SELECT tier, demo_reset_at AS at FROM churches WHERE id = ?1').bind(DEMO).first();
  if (!row) { await createDemo(env, now); row = { tier: 'demo', at: null }; }
  if (row.tier !== 'demo') return fail(503, '둘러보기 교회가 준비되지 않았습니다.');
  const L = limitsOf('demo', env);
  const cutoff = now - L.resetHours * 3600_000;
  if (!force && row.at && row.at > cutoff) return { reset: false };
  // 둘이 동시에 들어와도 한 번만 되돌린다
  const claim = await env.DB.prepare('UPDATE churches SET demo_reset_at = ?2 WHERE id = ?1 AND (demo_reset_at IS NULL OR demo_reset_at <= ?3 OR ?4 = 1)').bind(DEMO, now, cutoff, force ? 1 : 0).run();
  if (!claim.meta.changes) return { reset: false };
  const n = await resetDemo(env, now);
  return { reset: true, changed: n };
}

/** GET /demo — 데모 쿠키를 주고 둘러보기 화면으로. 이미 데모 로그인이면 화면만 */
export async function enter(request, env, url, session, page) {
  if (!demoEnabled(env)) return fail(404, '둘러보기가 꺼져 있습니다.');
  if (!auth.configured(env)) return fail(503, '로그인이 아직 설정되지 않았습니다.');
  const now = Date.now();
  const L = limitsOf('demo', env);
  const e = await ensureDemo(env, now);
  if (e instanceof Response) return e;
  if (session?.role === 'demo' && session.church === DEMO) return page('demo', { church: DEMO, role: 'demo', reset: e.reset });
  if (bump(entries, `${ipOf(request)}/${Math.floor(now / 3600_000)}`, L.entriesPerIpPerHour)) {
    return fail(429, '둘러보기를 너무 여러 번 새로 열었습니다. 한 시간 뒤 다시 해 보세요.', { code: 'tier_limit', limit: 'entriesPerIpPerHour', tier: 'demo' });
  }
  const cred = `d:${rand(10)}`;
  const { token } = await auth.issueToken(env, { church: DEMO, role: 'demo', cred }, now);
  const h = new Headers({ location: '/demo', 'cache-control': 'no-store' });
  h.append('set-cookie', auth.setCookie(auth.COOKIE, token, request, L.sessionHours * 3600));
  h.append('set-cookie', auth.setCookie(auth.CHURCH_COOKIE, DEMO, request, L.sessionHours * 3600, false));
  return new Response(null, { status: 302, headers: h });
}

/** 데모 방문자의 중계실 이름 (방문자마다 따로) */
export const demoRoom = s => `${DEMO}~${String(s.cred).replace(/[^a-z0-9:]/gi, '').slice(0, 20)}`;

/** 관리자: 지금 되돌리기 (시험 · 운영) */
export async function adminReset(env) {
  const r = await ensureDemo(env, Date.now(), true);
  return r instanceof Response ? r : json({ ok: true, ...r });
}
