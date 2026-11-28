// 동기화 API (2B 설치판 동기화 · 6 오프라인 웹 조작 화면이 쓴다). 예시: docs/web/data-api.md '동기화'.
//   GET  /api/v1/sync?after=seq&limit=   → 그 뒤 변경 (문서의 지금 본문과 함께), 쪽 나누기
//   POST /api/v1/sync {mutations:[…]}    → 고친 것 여럿. mutationId 가 같으면 두 번 쓰지 않고 앞의 결과를 돌려준다
import { fail, json } from '../protocol.mjs';
import { DATA_LIMITS, validDocId, validKind, validMutationId } from './routes.mjs';
import { actorOf, currentDocs, docJson, findMutations, preparePayload, raw, writeStatements } from './store.mjs';
import { readBody } from './documents.mjs';

export async function pull(request, env, url, s) {
  const q = url.searchParams;
  const afterText = q.get('after') || '0';
  if (!/^\d{1,15}$/.test(afterText)) return fail(400, 'after 는 0 이상의 숫자입니다.');
  const after = Number(afterText);
  const limit = Math.min(Math.max(Number(q.get('limit')) || DATA_LIMITS.syncDefault, 1), DATA_LIMITS.syncPage);
  const [rows, head] = await env.DB.batch([
    env.DB.prepare(
      `SELECT c.seq, c.kind, c.id, c.revision, c.op, c.at, c.by, c.mutation_id AS mutationId,
              d.revision AS cur, d.deleted_at AS deletedAt, d.payload AS payload
       FROM changes c LEFT JOIN documents d ON d.church_id = c.church_id AND d.kind = c.kind AND d.id = c.id
       WHERE c.church_id = ?1 AND c.seq > ?2 ORDER BY c.seq LIMIT ?3`,
    ).bind(s.church, after, limit + 1),
    env.DB.prepare('SELECT COALESCE(MAX(seq), 0) AS head FROM changes WHERE church_id = ?1').bind(s.church),
  ]);
  let list = rows.results;
  let more = list.length > limit;
  list = list.slice(0, limit);
  const out = [];
  let bytes = 0;
  for (const c of list) {
    // 그 뒤에 또 고쳐진 변경은 본문 없이 '지나감' 표시만 (뒤의 변경이 같은 쪽 또는 다음 쪽에 반드시 온다)
    const latest = c.cur === c.revision;
    const withPayload = latest && c.op === 'put' && !c.deletedAt;
    const add = withPayload ? c.payload.length : 0;
    if (out.length && bytes + add > DATA_LIMITS.syncBytes) { more = true; break; }
    bytes += add;
    const head = { seq: c.seq, kind: c.kind, id: c.id, revision: c.revision, op: c.op, at: c.at, by: c.by, mutationId: c.mutationId };
    if (!latest) head.superseded = true;
    const h = JSON.stringify(head);
    out.push(withPayload ? `${h.slice(0, -1)},"payload":${c.payload}}` : h);
  }
  const lastSeq = out.length ? list[out.length - 1].seq : after;
  const headSeq = head.results[0]?.head || 0;
  // reset: 변경 기록을 줄여 after 뒤를 다 줄 수 없을 때 true (지금은 줄이지 않으므로 늘 false). true 면 목록부터 다시 받는다
  return raw(`{"ok":true,"changes":[${out.join(',')}],"next":${lastSeq},"more":${more},"head":${headSeq},"reset":false}`);
}

/**
 * 본문: { mutations: [{ mutationId, kind, id, op:'put'|'delete', baseRevision, payload? }] } (최대 20개, 차례대로)
 * 답:   { ok, results: [{ mutationId, status:'applied'|'duplicate'|'conflict'|'invalid'|'missing', … }], head }
 * 하나가 충돌해도 나머지는 들어간다 (각자 판 확인). 충돌은 최신(current)을 싣는다 — 사본을 만들지 사람이 고르게.
 */
export async function push(request, env, url, s) {
  const b = await readBody(request, DATA_LIMITS.document * 4);
  if (b.error) return b.error;
  const list = b.value?.mutations;
  if (!Array.isArray(list) || !list.length) return fail(400, 'mutations 가 비었습니다.');
  if (list.length > DATA_LIMITS.mutations) return fail(413, `한 번에 ${DATA_LIMITS.mutations}개까지 올립니다.`);
  const results = new Array(list.length);
  const todo = [];
  const seen = new Set();
  let total = 0;
  for (const [i, m] of list.entries()) {
    const bad = error => { results[i] = { mutationId: m?.mutationId ?? null, status: 'invalid', error }; };
    if (!m || typeof m !== 'object') { bad('항목이 객체가 아닙니다.'); continue; }
    if (!validMutationId(m.mutationId)) { bad('mutationId 가 올바르지 않습니다 (8~80자 영문 · 숫자 · _ . : -).'); continue; }
    if (seen.has(m.mutationId)) return fail(400, `같은 mutationId 가 두 번 있습니다: ${m.mutationId}`);
    seen.add(m.mutationId);
    if (!validKind(m.kind)) { bad('없는 문서 종류입니다.'); continue; }
    if (!validDocId(m.id)) { bad('문서 이름이 올바르지 않습니다.'); continue; }
    if (m.op !== 'put' && m.op !== 'delete') { bad("op 는 'put' 또는 'delete' 입니다."); continue; }
    const base = m.baseRevision ?? 0;
    if (!Number.isInteger(base) || base < 0) { bad('baseRevision 은 0 이상의 정수입니다.'); continue; }
    if (m.op === 'delete' && base === 0) { bad('지울 때는 baseRevision 이 있어야 합니다.'); continue; }
    const w = { kind: m.kind, id: m.id, op: m.op, base, mutationId: m.mutationId };
    if (m.op === 'put') {
      const p = preparePayload(m.payload, DATA_LIMITS.document);
      if (p.error) { bad(p.error); continue; }
      total += p.size;
      Object.assign(w, p);
    }
    todo.push([i, w]);
  }
  if (total > DATA_LIMITS.document * 4) return fail(413, '한 번에 올리는 양이 너무 큽니다. 나눠 올려 주세요.');
  // 앞서 들어간 것은 다시 쓰지 않는다 (같은 문서 · 같은 결과를 돌려줌)
  const prev = await findMutations(env, s.church, todo.map(([, w]) => w.mutationId));
  const apply = [];
  for (const [i, w] of todo) {
    const p = prev.get(w.mutationId);
    if (!p) { apply.push([i, w]); continue; }
    results[i] = p.kind === w.kind && p.id === w.id
      ? { mutationId: w.mutationId, status: 'duplicate', kind: w.kind, id: w.id, revision: p.revision, seq: p.seq }
      : { mutationId: w.mutationId, status: 'invalid', error: '같은 mutationId 를 다른 문서에 썼습니다.' };
  }
  const now = Date.now();
  const actor = actorOf(s);
  if (apply.length) {
    const res = await env.DB.batch(apply.flatMap(([, w]) => writeStatements(env, s.church, actor, w, now)));
    const lost = [];
    apply.forEach(([i, w], k) => {
      const made = res[k * 2 + 1].results?.[0];
      if (made) results[i] = { mutationId: w.mutationId, status: 'applied', kind: w.kind, id: w.id, revision: made.revision, seq: made.seq, deleted: w.op === 'delete' };
      else lost.push([i, w]);
    });
    if (lost.length) {
      const cur = await currentDocs(env, s.church, [...new Map(lost.map(([, w]) => [`${w.kind}/${w.id}`, [w.kind, w.id]])).values()]);
      for (const [i, w] of lost) {
        const c = cur.get(`${w.kind}/${w.id}`) || null;
        const missing = w.op === 'delete' && (!c || c.deletedAt);
        results[i] = { mutationId: w.mutationId, status: missing ? 'missing' : 'conflict', kind: w.kind, id: w.id, current: c ? JSON.parse(docJson(c)) : null };
      }
    }
  }
  const head = await env.DB.prepare('SELECT COALESCE(MAX(seq), 0) AS head FROM changes WHERE church_id = ?1').bind(s.church).first();
  return json({ ok: true, results, head: head?.head || 0 });
}
