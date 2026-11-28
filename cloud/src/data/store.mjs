// 교회 문서 저장 (D1 documents · changes). 문서 API 와 동기화 API 가 함께 쓴다.
// - 모든 질의는 church_id = ?1 로 시작한다. 교회 값은 확인한 로그인(s.church)에서만 온다.
// - 쓰기는 '고친 문서 줄 + 변경 기록 줄' 을 한 묶음(D1 batch = 한 트랜잭션)으로. 판(revision)이 다르면 아무것도 쓰지 않는다 → 409 + 최신.
// - 변경 기록 번호(seq)는 교회마다 1, 2, 3 … (MAX+1, 묶음 안에서 차례대로 매겨짐).
import { titleOf } from './routes.mjs';

const token = () => [...crypto.getRandomValues(new Uint8Array(12))].map(b => b.toString(16).padStart(2, '0')).join('');

/** 누가 고쳤나: 기기 열쇠 id 또는 'm:<사람 id>' (쿠키 안의 세대 번호는 뺀다) */
export function actorOf(s) {
  const c = String(s.cred || '');
  return c.startsWith('m:') ? c.split(':').slice(0, 2).join(':') : c;
}

/**
 * 쓰기 하나 → D1 문 두 개 (문서 · 변경 기록). w = { kind, id, op:'put'|'delete', base, text?, title?, mutationId? }
 * base 0 = 없을 때만 만들기. 지운 문서(묘비)를 되살릴 때는 묘비의 revision 을 base 로.
 */
export function writeStatements(env, church, actor, w, now) {
  const t = token();
  let doc;
  if (w.op === 'put' && w.base === 0) {
    doc = env.DB.prepare(
      `INSERT INTO documents (church_id, kind, id, payload, revision, updated_at, updated_by, deleted_at, title, size, write_token)
       VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6, NULL, ?7, ?8, ?9) ON CONFLICT (church_id, kind, id) DO NOTHING`,
    ).bind(church, w.kind, w.id, w.text, now, actor, w.title, w.size ?? w.text.length, t);
  } else if (w.op === 'put') {
    doc = env.DB.prepare(
      `UPDATE documents SET payload = ?4, revision = revision + 1, updated_at = ?5, updated_by = ?6, deleted_at = NULL, title = ?7, size = ?8, write_token = ?9
       WHERE church_id = ?1 AND kind = ?2 AND id = ?3 AND revision = ?10`,
    ).bind(church, w.kind, w.id, w.text, now, actor, w.title, w.size ?? w.text.length, t, w.base);
  } else {
    doc = env.DB.prepare(
      `UPDATE documents SET payload = '', revision = revision + 1, updated_at = ?4, updated_by = ?5, deleted_at = ?4, title = '', size = 0, write_token = ?6
       WHERE church_id = ?1 AND kind = ?2 AND id = ?3 AND revision = ?7 AND deleted_at IS NULL`,
    ).bind(church, w.kind, w.id, now, actor, t, w.base);
  }
  const change = env.DB.prepare(
    `INSERT INTO changes (church_id, seq, kind, id, revision, op, at, by, mutation_id)
     SELECT ?1, (SELECT COALESCE(MAX(seq), 0) + 1 FROM changes WHERE church_id = ?1), kind, id, revision, ?4, ?5, ?6, ?7
     FROM documents WHERE church_id = ?1 AND kind = ?2 AND id = ?3 AND write_token = ?8
     RETURNING seq, revision`,
  ).bind(church, w.kind, w.id, w.op, now, actor, w.mutationId || null, t);
  return [doc, change];
}

/** 앞서 들어간 mutationId 들 → Map(mutationId → {seq, kind, id, revision, op}) */
export async function findMutations(env, church, ids) {
  const out = new Map();
  if (!ids.length) return out;
  const marks = ids.map((_, i) => `?${i + 2}`).join(',');
  const r = await env.DB.prepare(`SELECT mutation_id AS m, seq, kind, id, revision, op FROM changes WHERE church_id = ?1 AND mutation_id IN (${marks})`).bind(church, ...ids).all();
  for (const x of r.results) out.set(x.m, { seq: x.seq, kind: x.kind, id: x.id, revision: x.revision, op: x.op });
  return out;
}

/** 문서 여럿의 지금 모습 (충돌 답에 싣는다). keys = [[kind, id], …] */
export async function currentDocs(env, church, keys) {
  const out = new Map();
  if (!keys.length) return out;
  const cond = keys.map((_, i) => `(kind = ?${i * 2 + 2} AND id = ?${i * 2 + 3})`).join(' OR ');
  const r = await env.DB.prepare(
    `SELECT kind, id, revision, updated_at AS updatedAt, updated_by AS updatedBy, deleted_at AS deletedAt, payload FROM documents WHERE church_id = ?1 AND (${cond})`,
  ).bind(church, ...keys.flat()).all();
  for (const x of r.results) out.set(`${x.kind}/${x.id}`, x);
  return out;
}

/** 문서 줄 → 답에 싣는 모양. payload 는 저장된 JSON 글을 다시 읽지 않고 그대로 붙인다 (CPU 를 아끼려고) */
export function docJson(row, extra = {}) {
  const head = {
    kind: row.kind, id: row.id, revision: row.revision, updatedAt: row.updatedAt, updatedBy: row.updatedBy,
    deleted: !!row.deletedAt, ...extra,
  };
  const h = JSON.stringify(head);
  return row.deletedAt || row.payload === undefined || row.payload === '' ? h : `${h.slice(0, -1)},"payload":${row.payload}}`;
}

/** 본문(JSON) 확인 → { text, title } | { error, status } */
export function preparePayload(value, limit) {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) return { status: 400, error: '문서는 JSON 객체여야 합니다.' };
  const text = JSON.stringify(value);
  const size = new TextEncoder().encode(text).length;
  if (size > limit) return { status: 413, error: `문서가 너무 큽니다 (${Math.floor(limit / 1024)} KiB 까지).` };
  return { text, size, title: titleOf(value) };
}

export const raw = (body, status = 200, headers = {}) => new Response(body, {
  status,
  headers: { 'content-type': 'application/json; charset=utf-8', 'cache-control': 'no-store', 'x-content-type-options': 'nosniff', ...headers },
});
