// 문서 API: GET/PUT/DELETE /api/v1/documents/{kind}/{id} · GET /api/v1/documents?kind=&cursor=&limit=&deleted=1&include=payload
// 고칠 때 If-Match: revision (없으면 '새로 만들기만'). 다르면 409 + 최신 — 자동으로 덮어쓰지 않는다 (웹 편집기 충돌 규칙과 같음).
import { fail, json } from '../protocol.mjs';
import { DATA_LIMITS, decodeCursor, encodeCursor, parseIfMatch, validDocId, validKind, validMutationId } from './routes.mjs';
import { actorOf, currentDocs, docJson, findMutations, preparePayload, raw, writeStatements } from './store.mjs';

/** /api/v1/documents/{kind}/{id} → [kind, id] | Response(오류) */
function target(url) {
  const [, , , , kind, rawId] = url.pathname.split('/');
  let id;
  try { id = decodeURIComponent(rawId); } catch { return fail(400, '문서 이름이 올바르지 않습니다.'); }
  if (!validKind(kind)) return fail(404, '없는 문서 종류입니다.');
  if (!validDocId(id)) return fail(400, '문서 이름이 올바르지 않습니다.');
  return [kind, id];
}

/** 본문을 한도 안에서 읽어 JSON 으로 */
export async function readBody(request, limit) {
  const len = Number(request.headers.get('content-length') || 0);
  if (len > limit + 4096) return { error: fail(413, '너무 큽니다.') };
  const text = await request.text();
  if (text.length > limit + 4096) return { error: fail(413, '너무 큽니다.') };
  try { return { value: JSON.parse(text) }; } catch { return { error: fail(400, '본문이 JSON 이 아닙니다.') }; }
}

export const conflictBody = (cur, extra = {}) => `{"ok":false,"conflict":true,"error":"다른 곳에서 먼저 고쳤습니다. 최신 내용을 확인해 주세요.","current":${cur ? docJson(cur) : 'null'}${Object.entries(extra).map(([k, v]) => `,${JSON.stringify(k)}:${JSON.stringify(v)}`).join('')}}`;

export async function getDocument(request, env, url, s) {
  const t = target(url);
  if (t instanceof Response) return t;
  const row = await env.DB.prepare(
    'SELECT kind, id, revision, updated_at AS updatedAt, updated_by AS updatedBy, deleted_at AS deletedAt, title, payload FROM documents WHERE church_id = ?1 AND kind = ?2 AND id = ?3',
  ).bind(s.church, t[0], t[1]).first();
  if (!row) return fail(404, '그런 문서가 없습니다.');
  if (row.deletedAt) return fail(404, '지운 문서입니다.', { deleted: true, revision: row.revision });
  return raw(`{"ok":true,${docJson(row, { title: row.title }).slice(1)}`, 200, { etag: `"${row.revision}"` });
}

/** PUT · DELETE 하나. X-Mutation-Id 를 주면 같은 값으로 다시 와도 두 번 쓰지 않는다 */
export async function writeDocument(request, env, url, s, op) {
  const t = target(url);
  if (t instanceof Response) return t;
  const [kind, id] = t;
  const base = parseIfMatch(request.headers.get('if-match'));
  if (base === null) return fail(400, 'If-Match 는 revision 숫자여야 합니다.');
  if (op === 'delete' && base === 0) return fail(428, '지울 때는 If-Match: revision 이 있어야 합니다.');
  const mutationId = request.headers.get('x-mutation-id') || '';
  if (mutationId && !validMutationId(mutationId)) return fail(400, 'X-Mutation-Id 가 올바르지 않습니다.');
  let w = { kind, id, op, base, mutationId: mutationId || null };
  if (op === 'put') {
    const b = await readBody(request, DATA_LIMITS.document);
    if (b.error) return b.error;
    const p = preparePayload(b.value, DATA_LIMITS.document);
    if (p.error) return fail(p.status, p.error);
    w = { ...w, ...p };
  }
  if (mutationId) {
    const prev = (await findMutations(env, s.church, [mutationId])).get(mutationId);
    if (prev) {
      if (prev.kind !== kind || prev.id !== id) return fail(409, '같은 X-Mutation-Id 를 다른 문서에 썼습니다.');
      return json({ ok: true, duplicate: true, kind, id, revision: prev.revision, seq: prev.seq, deleted: prev.op === 'delete' }, 200, { etag: `"${prev.revision}"` });
    }
  }
  const now = Date.now();
  const res = await env.DB.batch(writeStatements(env, s.church, actorOf(s), w, now));
  const made = res[1].results?.[0];
  if (made) {
    return json({ ok: true, kind, id, revision: made.revision, seq: made.seq, updatedAt: now, deleted: op === 'delete' }, op === 'put' && base === 0 ? 201 : 200, { etag: `"${made.revision}"` });
  }
  const cur = (await currentDocs(env, s.church, [[kind, id]])).get(`${kind}/${id}`) || null;
  if (op === 'delete' && (!cur || cur.deletedAt)) return fail(404, cur ? '이미 지운 문서입니다.' : '그런 문서가 없습니다.', cur ? { deleted: true, revision: cur.revision } : {});
  return raw(conflictBody(cur), 409);
}

export async function listDocuments(request, env, url, s) {
  const q = url.searchParams;
  const kind = q.get('kind') || '';
  if (kind && !validKind(kind)) return fail(400, '없는 문서 종류입니다.');
  const limit = Math.min(Math.max(Number(q.get('limit')) || DATA_LIMITS.listDefault, 1), DATA_LIMITS.listPage);
  const cur = decodeCursor(q.get('cursor'));
  if (cur === undefined || (cur && cur.length !== 2)) return fail(400, 'cursor 가 올바르지 않습니다.');
  const withDeleted = q.get('deleted') === '1';
  const withPayload = q.get('include') === 'payload';
  const cols = `kind, id, revision, updated_at AS updatedAt, updated_by AS updatedBy, deleted_at AS deletedAt, title, size${withPayload ? ', payload' : ''}`;
  const where = ['church_id = ?1'];
  const args = [s.church];
  if (kind) { args.push(kind); where.push(`kind = ?${args.length}`); }
  if (cur) {
    args.push(cur[0], cur[1]);
    where.push(`(kind, id) > (?${args.length - 1}, ?${args.length})`);
  }
  if (!withDeleted) where.push('deleted_at IS NULL');
  const r = await env.DB.prepare(`SELECT ${cols} FROM documents WHERE ${where.join(' AND ')} ORDER BY kind, id LIMIT ${limit + 1}`).bind(...args).all();
  let rows = r.results;
  let more = rows.length > limit;
  rows = rows.slice(0, limit);
  // 본문까지 실으면 한 쪽이 너무 커지지 않게 자른다 (적어도 하나는 싣는다)
  if (withPayload) {
    let bytes = 0, n = 0;
    for (const x of rows) { bytes += x.payload.length; if (n && bytes > DATA_LIMITS.syncBytes) break; n++; }
    if (n < rows.length) { rows = rows.slice(0, n); more = true; }
  }
  const last = rows[rows.length - 1];
  const next = more && last ? encodeCursor([last.kind, last.id]) : null;
  const items = rows.map(x => docJson(x, { title: x.title, size: x.size }));
  return raw(`{"ok":true,"documents":[${items.join(',')}],"next":${JSON.stringify(next)}}`);
}
