// 무료 등급 문서 · 동기화: 본문은 그 교회의 구글 드라이브 파일, 판 번호 · 묘비 · 변경 기록은 D1 (작은 것만).
// 길 · 답 모양은 유료(data/documents.mjs · sync.mjs)와 같다 — 설치판 · 웹 조작 화면 코드는 저장소를 모른다. 계약: docs/web/tiers.md 4절
//
// 쓰기 차례 (판 확인은 D1 이 한다 — 드라이브에는 '조건부 쓰기(If-Match)' 가 없다):
//   1. 잡기: drive_docs 줄의 판이 base 이고 아무도 쓰는 중이 아니면 pend_token 을 건다 (아니면 409 + 최신)
//   2. 드라이브에 봉투(판 = base+1) 쓰기 — 실패하면 잡은 것을 풀고 오류(drive_*)
//   3. 마무리: 판 +1 · 변경 기록 한 줄 (D1 한 묶음). 2 와 3 사이에 Worker 가 죽으면 30초 뒤 다음 요청이
//      드라이브 봉투를 보고 '썼으면 마무리 · 안 썼으면 풀기' 를 대신 한다 (recover).
import { fail, json } from '../protocol.mjs';
import { DATA_LIMITS, decodeCursor, encodeCursor, parseIfMatch, validDocId, validKind, validMutationId } from '../data/routes.mjs';
import { actorOf, findMutations, preparePayload, raw } from '../data/store.mjs';
import { conflictBody, readBody } from '../data/documents.mjs';
import { readMutations } from '../data/sync.mjs';
import { limitsOf } from '../tiers/limits.mjs';
import { drive } from './google.mjs';
import { DriveError, docFileName, envelopeText, multipartBody, parseEnvelope } from './format.mjs';
import { folderFor } from './folders.mjs';

const STALE_MS = 30_000;
const tok = () => [...crypto.getRandomValues(new Uint8Array(12))].map(b => b.toString(16).padStart(2, '0')).join('');
const COLS = 'kind, id, revision, updated_at AS updatedAt, updated_by AS updatedBy, deleted_at AS deletedAt, title, size, file_id AS fileId, pend_token AS pendToken, pend_at AS pendAt, pend_op AS pendOp, pend_mut AS pendMut, pend_title AS pendTitle, pend_size AS pendSize';

const rowOf = (env, church, kind, id) => env.DB.prepare(`SELECT ${COLS} FROM drive_docs WHERE church_id = ?1 AND kind = ?2 AND id = ?3`).bind(church, kind, id).first();

/** 드라이브 오류 → 답 (주인에게 다시 연결할 곳을 같이 알린다) */
export function driveFail(e) {
  if (!(e instanceof DriveError)) throw e;
  const extra = { code: e.code, ...e.extra };
  delete extra.grant;
  if (e.code === 'drive_disconnected') extra.reconnect = '/auth/google/start?mode=connect';
  return fail(e.status, e.message, extra);
}

// ---------- 드라이브의 봉투 ----------
/** 파일 id → 봉투 (큰 문서는 앞머리만 읽고 본문 글은 그대로 잘라 쓴다 — Worker CPU 10 ms) */
export async function readEnvelope(env, church, fileId) {
  const r = await drive(env, church, `/files/${encodeURIComponent(fileId)}?alt=media`, {}, { raw: true });
  const text = await r.text();
  if (text.length > 256 * 1024 && text.startsWith('{"worshipPresenter":1,')) {
    const i = text.indexOf(',"payload":');
    const e = text.lastIndexOf('}');
    if (i > 0 && e === text.length - 1) {
      let head = null;
      try { head = JSON.parse(text.slice(0, i) + '}'); } catch { /* 아래에서 전체를 읽음 */ }
      const p = text.slice(i + 11, -1);
      if (head && Number.isInteger(head.revision) && p.startsWith('{') && p.endsWith('}')) return { ...head, deleted: !!head.deleted, payloadText: p };
    }
  }
  const v = parseEnvelope(text);
  if (!v) throw new DriveError('drive_corrupt', '드라이브의 문서 파일을 읽을 수 없습니다 (드라이브에서 고쳤을 수 있습니다).');
  return v;
}

/** 봉투를 드라이브에 쓴다 → { fileId, version } */
async function writeEnvelope(env, church, row, w, revision, now, actor) {
  const text = envelopeText({ kind: w.kind, id: w.id, revision, updatedAt: now, updatedBy: actor, deleted: w.op === 'delete', payloadText: w.op === 'put' ? w.text : null });
  const name = docFileName(w.kind, w.id, w.op === 'put' ? w.title : row?.title);
  const appProperties = { wpKind: w.kind, wpId: w.id.slice(0, 100), wpRev: String(revision) };
  if (row?.fileId) {
    const m = multipartBody({ name, trashed: w.op === 'delete', appProperties }, text);
    try {
      return await drive(env, church, `upload:/files/${encodeURIComponent(row.fileId)}?uploadType=multipart&fields=id,version`, { method: 'PATCH', headers: { 'content-type': m.contentType }, body: m.body })
        .then(f => ({ fileId: f.id, version: String(f.version ?? '') }));
    } catch (e) {
      // 사람이 드라이브에서 아주 지웠으면(휴지통 비움) 새로 만든다
      if (!(e instanceof DriveError) || e.code !== 'drive_missing') throw e;
    }
  }
  const parent = await folderFor(env, church, `kind:${w.kind}`);
  const m = multipartBody({ name, parents: [parent], mimeType: 'application/json', trashed: w.op === 'delete', appProperties }, text);
  const f = await drive(env, church, 'upload:/files?uploadType=multipart&fields=id,version', { method: 'POST', headers: { 'content-type': m.contentType }, body: m.body });
  return { fileId: f.id, version: String(f.version ?? '') };
}

// ---------- 잡기 · 마무리 · 풀기 ----------
function claimStmt(env, church, w, t, now) {
  const title = w.op === 'put' ? w.title : '';
  const size = w.op === 'put' ? w.size : 0;
  if (w.op === 'put' && w.base === 0) {
    return env.DB.prepare(
      `INSERT INTO drive_docs (church_id, kind, id, revision, updated_at, updated_by, title, size, pend_token, pend_at, pend_op, pend_mut, pend_title, pend_size)
       VALUES (?1, ?2, ?3, 0, ?4, '', '', 0, ?5, ?4, 'put', ?6, ?7, ?8)
       ON CONFLICT (church_id, kind, id) DO UPDATE SET pend_token = excluded.pend_token, pend_at = excluded.pend_at, pend_op = 'put', pend_mut = excluded.pend_mut,
         pend_title = excluded.pend_title, pend_size = excluded.pend_size
       WHERE drive_docs.revision = 0 AND drive_docs.pend_token IS NULL
       RETURNING revision, file_id AS fileId, title`,
    ).bind(church, w.kind, w.id, now, t, w.mutationId || null, title, size);
  }
  return env.DB.prepare(
    `UPDATE drive_docs SET pend_token = ?4, pend_at = ?5, pend_op = ?6, pend_mut = ?7, pend_title = ?8, pend_size = ?9
     WHERE church_id = ?1 AND kind = ?2 AND id = ?3 AND revision = ?10 AND revision > 0 AND pend_token IS NULL AND (?6 = 'put' OR deleted_at IS NULL)
     RETURNING revision, file_id AS fileId, title`,
  ).bind(church, w.kind, w.id, t, now, w.op, w.mutationId || null, title, size, w.base);
}

function commitStmts(env, church, k, t, { op, at, by, mutationId, title, size, fileId, version }) {
  const wt = tok();
  return [
    env.DB.prepare(
      `UPDATE drive_docs SET revision = revision + 1, updated_at = ?5, updated_by = ?6, deleted_at = CASE WHEN ?7 = 'delete' THEN ?5 ELSE NULL END,
         title = ?8, size = ?9, file_id = ?10, drive_version = ?11, write_token = ?12,
         pend_token = NULL, pend_at = NULL, pend_op = NULL, pend_mut = NULL, pend_title = NULL, pend_size = NULL
       WHERE church_id = ?1 AND kind = ?2 AND id = ?3 AND pend_token = ?4`,
    ).bind(church, k.kind, k.id, t, at, by, op, op === 'put' ? title : '', op === 'put' ? size : 0, fileId, version || null, wt),
    env.DB.prepare(
      `INSERT INTO changes (church_id, seq, kind, id, revision, op, at, by, mutation_id)
       SELECT ?1, (SELECT COALESCE(MAX(seq), 0) + 1 FROM changes WHERE church_id = ?1), kind, id, revision, ?4, ?5, ?6, ?7
       FROM drive_docs WHERE church_id = ?1 AND kind = ?2 AND id = ?3 AND write_token = ?8
       RETURNING seq, revision`,
    ).bind(church, k.kind, k.id, op, at, by, mutationId || null, wt),
  ];
}

async function release(env, church, k, t) {
  await env.DB.batch([
    env.DB.prepare('DELETE FROM drive_docs WHERE church_id = ?1 AND kind = ?2 AND id = ?3 AND pend_token = ?4 AND revision = 0 AND file_id IS NULL').bind(church, k.kind, k.id, t),
    env.DB.prepare('UPDATE drive_docs SET pend_token = NULL, pend_at = NULL, pend_op = NULL, pend_mut = NULL, pend_title = NULL, pend_size = NULL WHERE church_id = ?1 AND kind = ?2 AND id = ?3 AND pend_token = ?4').bind(church, k.kind, k.id, t),
  ]);
}

/**
 * 30초 넘게 '쓰는 중' 으로 남은 줄을 정리한다: 드라이브에 다음 판이 써졌으면 마무리, 아니면 풀기.
 * 만드는 중이던 것(file_id 없음)은 드라이브에서 앱 속성(wpKind · wpId)으로 찾아 본다.
 */
export async function recover(env, church, row) {
  const k = { kind: row.kind, id: row.id };
  let fileId = row.fileId, envl = null;
  try {
    if (!fileId) {
      const q = `appProperties has { key='wpKind' and value='${row.kind}' } and appProperties has { key='wpId' and value='${row.id.slice(0, 100).replace(/'/g, "\\'")}' }`;
      const list = await drive(env, church, `/files?q=${encodeURIComponent(q)}&fields=files(id,version)&pageSize=5`);
      fileId = list.files?.[0]?.id || null;
    }
    if (fileId) envl = await readEnvelope(env, church, fileId);
  } catch (e) {
    if (!(e instanceof DriveError) || !['drive_missing', 'drive_corrupt'].includes(e.code)) throw e;
  }
  if (envl && envl.revision === row.revision + 1) {
    await env.DB.batch(commitStmts(env, church, k, row.pendToken, {
      op: row.pendOp, at: envl.updatedAt || Date.now(), by: envl.updatedBy || 'recover', mutationId: row.pendMut, title: row.pendTitle, size: row.pendSize, fileId, version: null,
    }));
  } else {
    await release(env, church, k, row.pendToken);
  }
}

/** 문서 줄 → 충돌 답의 current (본문은 드라이브에서) */
async function currentOf(env, church, row) {
  if (!row || row.revision === 0) return null;
  const head = { kind: row.kind, id: row.id, revision: row.revision, updatedAt: row.updatedAt, updatedBy: row.updatedBy, deletedAt: row.deletedAt };
  if (row.deletedAt || !row.fileId) return head;
  const e = await readEnvelope(env, church, row.fileId);
  return { ...head, payload: e.payloadText ?? '' };
}

/**
 * 쓰기 하나. w = { kind, id, op, base, text?, title?, size?, mutationId? }
 * → { status:'applied', revision, seq, updatedAt } | { status:'duplicate', … } | { status:'conflict'|'missing', current } (DriveError 는 던짐)
 */
export async function writeDoc(env, s, w) {
  if (w.mutationId) {
    const prev = (await findMutations(env, s.church, [w.mutationId])).get(w.mutationId);
    if (prev) return prev.kind === w.kind && prev.id === w.id ? { status: 'duplicate', ...prev } : { status: 'invalid', error: '같은 mutationId 를 다른 문서에 썼습니다.' };
  }
  const now = Date.now();
  const actor = actorOf(s);
  const k = { kind: w.kind, id: w.id };
  for (let attempt = 0; ; attempt++) {
    const t = tok();
    const claimed = await claimStmt(env, s.church, w, t, now).first();
    if (!claimed) {
      const row = await rowOf(env, s.church, w.kind, w.id);
      if (row?.pendToken && now - row.pendAt > STALE_MS && attempt === 0) { await recover(env, s.church, row); continue; }
      if (w.op === 'delete' && (!row || row.revision === 0 || row.deletedAt)) return { status: 'missing', current: row && row.revision ? await currentOf(env, s.church, row) : null };
      return { status: 'conflict', busy: !!row?.pendToken && row.revision === w.base, current: await currentOf(env, s.church, row) };
    }
    const rev = claimed.revision + 1;
    let f;
    try {
      f = await writeEnvelope(env, s.church, { ...claimed, fileId: claimed.fileId, title: claimed.title }, w, rev, now, actor);
    } catch (e) {
      await release(env, s.church, k, t).catch(() => {});
      throw e;
    }
    const res = await env.DB.batch(commitStmts(env, s.church, k, t, { op: w.op, at: now, by: actor, mutationId: w.mutationId, title: w.title, size: w.size, fileId: f.fileId, version: f.version }));
    const made = res[1].results?.[0];
    if (made) return { status: 'applied', revision: made.revision, seq: made.seq, updatedAt: now, deleted: w.op === 'delete' };
    // 30초 넘게 걸려 다른 요청이 대신 마무리했다 (recover) — 같은 mutationId 면 그 결과
    if (w.mutationId) {
      const prev = (await findMutations(env, s.church, [w.mutationId])).get(w.mutationId);
      if (prev) return { status: 'applied', revision: prev.revision, seq: prev.seq, updatedAt: now, deleted: w.op === 'delete' };
    }
    throw new DriveError('drive_busy', '같은 문서를 다른 곳에서 동시에 고쳤습니다. 최신을 받아 다시 해 주세요.');
  }
}

// ---------- 길 (유료와 같은 답 모양) ----------
function target(url) {
  const [, , , , kind, rawId] = url.pathname.split('/');
  let id;
  try { id = decodeURIComponent(rawId); } catch { return fail(400, '문서 이름이 올바르지 않습니다.'); }
  if (!validKind(kind)) return fail(404, '없는 문서 종류입니다.');
  if (!validDocId(id)) return fail(400, '문서 이름이 올바르지 않습니다.');
  return [kind, id];
}

const headJson = (row, extra = {}) => JSON.stringify({ kind: row.kind, id: row.id, revision: row.revision, updatedAt: row.updatedAt, updatedBy: row.updatedBy, deleted: !!row.deletedAt, ...extra });
const withPayload = (h, p) => (p ? `${h.slice(0, -1)},"payload":${p}}` : h);

export async function getDocument(request, env, url, s) {
  const t = target(url);
  if (t instanceof Response) return t;
  let row = await rowOf(env, s.church, t[0], t[1]);
  if (row?.pendToken && Date.now() - row.pendAt > STALE_MS) { await recover(env, s.church, row); row = await rowOf(env, s.church, t[0], t[1]); }
  if (!row || row.revision === 0) return fail(404, '그런 문서가 없습니다.');
  if (row.deletedAt) return fail(404, '지운 문서입니다.', { deleted: true, revision: row.revision });
  const e = await readEnvelope(env, s.church, row.fileId);
  // 쓰는 중이라 드라이브가 한 판 앞서 있으면 그것이 곧 마무리될 판이다
  const rev = e.revision === row.revision + 1 && row.pendToken ? e.revision : row.revision;
  if (e.deleted) return fail(404, '지운 문서입니다.', { deleted: true, revision: rev });
  return raw(`{"ok":true,${withPayload(headJson({ ...row, revision: rev }, { title: row.title }), e.payloadText).slice(1)}`, 200, { etag: `"${rev}"` });
}

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
  const r = await writeDoc(env, s, w);
  if (r.status === 'duplicate') return json({ ok: true, duplicate: true, kind, id, revision: r.revision, seq: r.seq, deleted: r.op === 'delete' }, 200, { etag: `"${r.revision}"` });
  if (r.status === 'invalid') return fail(409, r.error);
  if (r.status === 'applied') return json({ ok: true, kind, id, revision: r.revision, seq: r.seq, updatedAt: r.updatedAt, deleted: op === 'delete' }, op === 'put' && base === 0 ? 201 : 200, { etag: `"${r.revision}"` });
  if (r.status === 'missing') return fail(404, r.current ? '이미 지운 문서입니다.' : '그런 문서가 없습니다.', r.current ? { deleted: true, revision: r.current.revision } : {});
  return raw(conflictBody(r.current, r.busy ? { code: 'drive_busy' } : {}), 409);
}

/** 본문 여럿을 드라이브에서 (동시에 6개까지 — Workers 한 요청의 동시 연결 수) */
async function payloads(env, church, rows) {
  const out = new Map();
  let i = 0;
  const work = async () => {
    while (i < rows.length) {
      const x = rows[i++];
      const e = await readEnvelope(env, church, x.fileId);
      out.set(`${x.kind}/${x.id}`, e.deleted ? null : e.payloadText);
    }
  };
  await Promise.all(Array.from({ length: Math.min(6, rows.length) }, work));
  return out;
}

export async function listDocuments(request, env, url, s) {
  const q = url.searchParams;
  const kind = q.get('kind') || '';
  if (kind && !validKind(kind)) return fail(400, '없는 문서 종류입니다.');
  const L = limitsOf('free', env);
  const withPayload_ = q.get('include') === 'payload';
  const max = withPayload_ ? Math.min(DATA_LIMITS.listPage, L.pullPayloads) : DATA_LIMITS.listPage;
  const limit = Math.min(Math.max(Number(q.get('limit')) || DATA_LIMITS.listDefault, 1), max);
  const cur = decodeCursor(q.get('cursor'));
  if (cur === undefined || (cur && cur.length !== 2)) return fail(400, 'cursor 가 올바르지 않습니다.');
  const where = ['church_id = ?1', 'revision > 0'];
  const args = [s.church];
  if (kind) { args.push(kind); where.push(`kind = ?${args.length}`); }
  if (cur) { args.push(cur[0], cur[1]); where.push(`(kind, id) > (?${args.length - 1}, ?${args.length})`); }
  if (q.get('deleted') !== '1') where.push('deleted_at IS NULL');
  const r = await env.DB.prepare(`SELECT ${COLS} FROM drive_docs WHERE ${where.join(' AND ')} ORDER BY kind, id LIMIT ${limit + 1}`).bind(...args).all();
  let rows = r.results;
  const more = rows.length > limit;
  rows = rows.slice(0, limit);
  const body = withPayload_ ? await payloads(env, s.church, rows.filter(x => !x.deletedAt && x.fileId)) : null;
  const last = rows[rows.length - 1];
  const next = more && last ? encodeCursor([last.kind, last.id]) : null;
  const items = rows.map(x => withPayload(headJson(x, { title: x.title, size: x.size }), body?.get(`${x.kind}/${x.id}`)));
  return raw(`{"ok":true,"documents":[${items.join(',')}],"next":${JSON.stringify(next)}}`);
}

export async function pull(request, env, url, s) {
  const q = url.searchParams;
  const afterText = q.get('after') || '0';
  if (!/^\d{1,15}$/.test(afterText)) return fail(400, 'after 는 0 이상의 숫자입니다.');
  const after = Number(afterText);
  const limit = Math.min(Math.max(Number(q.get('limit')) || DATA_LIMITS.syncDefault, 1), DATA_LIMITS.syncPage);
  const L = limitsOf('free', env);
  const [rows, head] = await env.DB.batch([
    env.DB.prepare(
      `SELECT c.seq, c.kind, c.id, c.revision, c.op, c.at, c.by, c.mutation_id AS mutationId, d.revision AS cur, d.deleted_at AS deletedAt, d.file_id AS fileId
       FROM changes c LEFT JOIN drive_docs d ON d.church_id = c.church_id AND d.kind = c.kind AND d.id = c.id
       WHERE c.church_id = ?1 AND c.seq > ?2 ORDER BY c.seq LIMIT ?3`,
    ).bind(s.church, after, limit + 1),
    env.DB.prepare('SELECT COALESCE(MAX(seq), 0) AS head FROM changes WHERE church_id = ?1').bind(s.church),
  ]);
  let list = rows.results;
  let more = list.length > limit;
  list = list.slice(0, limit);
  // 본문을 실을 것: 최신 판의 put. 한 쪽에 L.pullPayloads 개까지 (드라이브를 하나씩 읽으므로)
  const need = [];
  let cut = list.length;
  for (let i = 0; i < list.length; i++) {
    const c = list[i];
    if (c.cur === c.revision && c.op === 'put' && !c.deletedAt && c.fileId) {
      if (need.length >= L.pullPayloads) { cut = i; more = true; break; }
      need.push(c);
    }
  }
  list = list.slice(0, cut);
  const body = await payloads(env, s.church, need);
  const out = [];
  let bytes = 0;
  for (const c of list) {
    const latest = c.cur === c.revision;
    const p = latest && c.op === 'put' ? body.get(`${c.kind}/${c.id}`) : null;
    if (out.length && bytes + (p?.length || 0) > DATA_LIMITS.syncBytes) { more = true; break; }
    bytes += p?.length || 0;
    const h = { seq: c.seq, kind: c.kind, id: c.id, revision: c.revision, op: c.op, at: c.at, by: c.by, mutationId: c.mutationId };
    if (!latest) h.superseded = true;
    out.push(withPayload(JSON.stringify(h), p));
  }
  const lastSeq = out.length ? list[out.length - 1].seq : after;
  return raw(`{"ok":true,"changes":[${out.join(',')}],"next":${lastSeq},"more":${more},"head":${head.results[0]?.head || 0},"reset":false}`);
}

export async function push(request, env, url, s) {
  const L = limitsOf('free', env);
  const m = await readMutations(request, Math.min(DATA_LIMITS.mutations, L.mutations));
  if (m instanceof Response) return m;
  const { results, todo } = m;
  // 같은 문서는 차례대로, 다른 문서끼리는 동시에 (6개까지)
  const groups = new Map();
  for (const [i, w] of todo) { const k = `${w.kind}/${w.id}`; if (!groups.has(k)) groups.set(k, []); groups.get(k).push([i, w]); }
  const queue = [...groups.values()];
  let gi = 0;
  const work = async () => {
    while (gi < queue.length) {
      for (const [i, w] of queue[gi++]) {
        const r = await writeDoc(env, s, w);
        if (r.status === 'applied') results[i] = { mutationId: w.mutationId, status: 'applied', kind: w.kind, id: w.id, revision: r.revision, seq: r.seq, deleted: w.op === 'delete' };
        else if (r.status === 'duplicate') results[i] = { mutationId: w.mutationId, status: 'duplicate', kind: w.kind, id: w.id, revision: r.revision, seq: r.seq };
        else if (r.status === 'invalid') results[i] = { mutationId: w.mutationId, status: 'invalid', error: r.error };
        else {
          const c = r.current;
          results[i] = { mutationId: w.mutationId, status: r.status, kind: w.kind, id: w.id, current: c ? JSON.parse(withPayload(headJson(c), c.payload)) : null };
        }
      }
    }
  };
  await Promise.all(Array.from({ length: Math.min(6, queue.length) }, work));
  const head = await env.DB.prepare('SELECT COALESCE(MAX(seq), 0) AS head FROM changes WHERE church_id = ?1').bind(s.church).first();
  return json({ ok: true, results, head: head?.head || 0 });
}

/** 살아 있는 문서 수 (등급 한도) */
export async function countDocs(env, church) {
  const r = await env.DB.prepare('SELECT COUNT(*) AS n FROM drive_docs WHERE church_id = ?1 AND revision > 0 AND deleted_at IS NULL').bind(church).first();
  return r?.n || 0;
}
