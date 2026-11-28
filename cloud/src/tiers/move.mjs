// 등급 옮기기 (무료 ↔ 유료): 드라이브의 것을 D1 · R2 로 복사하거나 거꾸로. 주인용 관리 길(ADMIN_TOKEN) — 결제는 없다, 관리자가 손으로 바꾼다.
// 차례: 시작(쓰기 423) → 기다림(Worker 등급 기억 30초가 다 지나게) → 단계마다 조금씩 복사(문서 → 성경 → 파일) → 등급 바꿈 → (따로) 정리.
// 한 번 부를 때 조금씩만 한다 — Workers 무료 플랜 하위 요청 50개 · CPU 10 ms 안에서. scripts/tier-move.mjs 가 끝날 때까지 되풀이해 부른다.
// 판 번호 · 변경 순번(changes)은 두 저장소가 같이 쓰므로 그대로 이어진다 → 설치판 · 웹 화면은 동기화를 다시 받을 필요가 없다.
import { fail, json } from '../protocol.mjs';
import { forgetTier } from './index.mjs';
import { readEnvelope, recover } from '../drive/docs.mjs';
import { drive, accountOf } from '../drive/google.mjs';
import { docFileName, envelopeText, multipartBody, bookChapter, bookHeadings } from '../drive/format.mjs';
import { folderFor } from '../drive/folders.mjs';
import { driveBucket } from '../drive/bucket.mjs';
import * as driveBible from '../drive/bible.mjs';
import { bookD1, versionD1 } from '../data/bibleimport.mjs';

const cacheMs = env => { const n = Number(env.TIER_CACHE_MS); return Number.isFinite(n) && n >= 0 ? n : 30_000; };
const DOCS_PER_STEP = 15, ASSETS_PER_STEP = 3;

const churchRow = (env, id) => env.DB.prepare('SELECT id, name, tier, move_to AS moveTo, move_since AS moveSince, move_state AS moveState FROM churches WHERE id = ?1').bind(id).first();
const saveState = (env, id, st) => env.DB.prepare('UPDATE churches SET move_state = ?2 WHERE id = ?1').bind(id, JSON.stringify(st)).run();

export async function info(env, id) {
  const c = await churchRow(env, id);
  if (!c) return fail(404, '그런 교회가 없습니다.');
  const n = async sql => (await env.DB.prepare(sql).bind(id).first())?.n || 0;
  return json({
    ok: true, church: c.id, name: c.name, tier: c.tier, moveTo: c.moveTo, moveSince: c.moveSince, moveState: c.moveState ? JSON.parse(c.moveState) : null,
    counts: {
      d1Documents: await n('SELECT COUNT(*) AS n FROM documents WHERE church_id = ?1'), driveDocuments: await n('SELECT COUNT(*) AS n FROM drive_docs WHERE church_id = ?1 AND revision > 0'),
      d1Bible: await n('SELECT COUNT(*) AS n FROM bible_versions WHERE church_id = ?1'), driveBible: await n('SELECT COUNT(*) AS n FROM drive_bible WHERE church_id = ?1'),
      assets: await n('SELECT COUNT(*) AS n FROM asset_blobs WHERE church_id = ?1 AND purged_at IS NULL'), driveFiles: await n('SELECT COUNT(*) AS n FROM drive_files WHERE church_id = ?1'),
    },
    drive: await accountOf(env, id).then(a => (a ? { connected: !a.disconnected_at && !!a.refresh_sealed } : null)),
  });
}

export async function start(env, id, to, now = Date.now()) {
  const c = await churchRow(env, id);
  if (!c) return fail(404, '그런 교회가 없습니다.');
  if (!['free', 'paid'].includes(to)) return fail(400, "to 는 'free' 또는 'paid' 입니다.");
  if (c.tier === 'demo') return fail(409, '둘러보기 교회는 옮기지 않습니다.');
  if (c.tier === to) return fail(409, `이미 ${to} 입니다.`);
  if (c.moveTo) return fail(409, '이미 옮기는 중입니다.', { moveTo: c.moveTo });
  if (to === 'free') {
    const a = await accountOf(env, id);
    if (!a || a.disconnected_at || !a.refresh_sealed) return fail(409, '먼저 주인이 구글 드라이브를 연결해야 합니다 (/auth/google/start?mode=connect).', { code: 'drive_disconnected' });
  }
  if (to === 'paid' && !env.FILES && (await env.DB.prepare('SELECT 1 FROM asset_blobs WHERE church_id = ?1 LIMIT 1').bind(id).first())) return fail(503, '파일 저장통(R2)이 연결되어 있지 않습니다.');
  const st = { from: c.tier, phase: 'docs', cursor: null };
  await env.DB.prepare('UPDATE churches SET move_to = ?2, move_since = ?3, move_state = ?4 WHERE id = ?1 AND move_to IS NULL').bind(id, to, now, JSON.stringify(st)).run();
  forgetTier(id);
  return json({ ok: true, church: id, from: c.tier, to, readyAt: now + cacheMs(env) + 1000, note: '그동안 이 교회의 쓰기는 423 (tier_moving). 송출 · 보기는 그대로.' });
}

export async function cancel(env, id) {
  await env.DB.prepare('UPDATE churches SET move_to = NULL, move_since = NULL, move_state = NULL WHERE id = ?1').bind(id).run();
  forgetTier(id);
  return json({ ok: true, cancelled: true });
}

/** 한 단계. → { done:false, phase, … } | { done:true, tier } */
export async function step(env, id, now = Date.now()) {
  const c = await churchRow(env, id);
  if (!c) return fail(404, '그런 교회가 없습니다.');
  if (!c.moveTo) return fail(409, '옮기는 중이 아닙니다.');
  const wait = c.moveSince + cacheMs(env) + 1000 - now;
  if (wait > 0) return json({ ok: false, error: '쓰기가 다 멈출 때까지 기다리는 중입니다.', retryAfter: Math.ceil(wait / 1000) }, 425);
  const st = JSON.parse(c.moveState || '{}');
  const s = { church: id, role: 'owner', cred: 'tier-move' };
  const toFree = c.moveTo === 'free';
  let res;
  if (st.phase === 'docs') res = toFree ? await docsToDrive(env, s, st, now) : await docsToD1(env, s, st);
  else if (st.phase === 'bible') res = toFree ? await bibleToDrive(env, s, st) : await bibleToD1(env, s, st);
  else if (st.phase === 'assets') res = toFree ? await assetsToDrive(env, s, st) : await assetsToR2(env, s, st);
  else res = { next: 'done' };
  if (res.next === 'done' || st.phase === 'done') {
    await env.DB.prepare('UPDATE churches SET tier = move_to, tier_changed_at = ?2, move_to = NULL, move_since = NULL, move_state = ?3 WHERE id = ?1')
      .bind(id, now, JSON.stringify({ phase: 'done', from: st.from, to: c.moveTo, at: now })).run();
    forgetTier(id);
    return json({ ok: true, done: true, tier: c.moveTo, from: st.from });
  }
  const next = res.next ? { from: st.from, phase: res.next, cursor: null } : { ...st, ...res.state };
  await saveState(env, id, next);
  return json({ ok: true, done: false, phase: next.phase, copied: res.copied || 0 });
}

// ---------- 문서 ----------
async function docsToD1(env, s, st) {
  const cur = st.cursor || ['', ''];
  const rows = (await env.DB.prepare(
    `SELECT kind, id, revision, updated_at AS updatedAt, updated_by AS updatedBy, deleted_at AS deletedAt, title, size, file_id AS fileId, pend_token AS pendToken, pend_at AS pendAt, pend_op AS pendOp, pend_mut AS pendMut, pend_title AS pendTitle, pend_size AS pendSize
     FROM drive_docs WHERE church_id = ?1 AND (kind, id) > (?2, ?3) ORDER BY kind, id LIMIT ?4`,
  ).bind(s.church, cur[0], cur[1], DOCS_PER_STEP).all()).results;
  if (!rows.length) return { next: 'bible' };
  const stmts = [];
  for (let x of rows) {
    if (x.pendToken) { await recover(env, s.church, x); x = await env.DB.prepare('SELECT kind, id, revision, updated_at AS updatedAt, updated_by AS updatedBy, deleted_at AS deletedAt, title, size, file_id AS fileId FROM drive_docs WHERE church_id = ?1 AND kind = ?2 AND id = ?3').bind(s.church, x.kind, x.id).first(); }
    if (!x || x.revision === 0) continue;
    let payload = '';
    if (!x.deletedAt && x.fileId) payload = (await readEnvelope(env, s.church, x.fileId)).payloadText || '';
    stmts.push(env.DB.prepare(
      `INSERT INTO documents (church_id, kind, id, payload, revision, updated_at, updated_by, deleted_at, title, size) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
       ON CONFLICT (church_id, kind, id) DO UPDATE SET payload = excluded.payload, revision = excluded.revision, updated_at = excluded.updated_at, updated_by = excluded.updated_by,
         deleted_at = excluded.deleted_at, title = excluded.title, size = excluded.size`,
    ).bind(s.church, x.kind, x.id, payload, x.revision, x.updatedAt, x.updatedBy, x.deletedAt, x.title, x.size));
  }
  if (stmts.length) await env.DB.batch(stmts);
  const last = rows[rows.length - 1];
  return { state: { cursor: [last.kind, last.id] }, copied: stmts.length };
}

async function docsToDrive(env, s, st, now) {
  const cur = st.cursor || ['', ''];
  const rows = (await env.DB.prepare(
    `SELECT kind, id, revision, updated_at AS updatedAt, updated_by AS updatedBy, deleted_at AS deletedAt, title, size, payload FROM documents
     WHERE church_id = ?1 AND (kind, id) > (?2, ?3) ORDER BY kind, id LIMIT ?4`,
  ).bind(s.church, cur[0], cur[1], DOCS_PER_STEP).all()).results;
  if (!rows.length) return { next: 'bible' };
  const old = new Map((await env.DB.prepare(`SELECT kind, id, file_id AS fileId FROM drive_docs WHERE church_id = ?1 AND kind || '/' || id IN (${rows.map((_, i) => `?${i + 2}`).join(',')})`)
    .bind(s.church, ...rows.map(x => `${x.kind}/${x.id}`)).all()).results.map(x => [`${x.kind}/${x.id}`, x.fileId]));
  const stmts = [];
  for (const x of rows) {
    let fileId = old.get(`${x.kind}/${x.id}`) || null;
    if (!x.deletedAt) {
      const text = envelopeText({ kind: x.kind, id: x.id, revision: x.revision, updatedAt: x.updatedAt, updatedBy: x.updatedBy, payloadText: x.payload });
      const appProperties = { wpKind: x.kind, wpId: x.id.slice(0, 100), wpRev: String(x.revision) };
      const name = docFileName(x.kind, x.id, x.title);
      if (fileId) {
        const m = multipartBody({ name, trashed: false, appProperties }, text);
        fileId = (await drive(env, s.church, `upload:/files/${encodeURIComponent(fileId)}?uploadType=multipart&fields=id`, { method: 'PATCH', headers: { 'content-type': m.contentType }, body: m.body }).catch(() => null))?.id || null;
      }
      if (!fileId) {
        const m = multipartBody({ name, parents: [await folderFor(env, s.church, `kind:${x.kind}`)], mimeType: 'application/json', appProperties }, text);
        fileId = (await drive(env, s.church, 'upload:/files?uploadType=multipart&fields=id', { method: 'POST', headers: { 'content-type': m.contentType }, body: m.body })).id;
      }
    }
    stmts.push(env.DB.prepare(
      `INSERT INTO drive_docs (church_id, kind, id, revision, updated_at, updated_by, deleted_at, title, size, file_id) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
       ON CONFLICT (church_id, kind, id) DO UPDATE SET revision = excluded.revision, updated_at = excluded.updated_at, updated_by = excluded.updated_by, deleted_at = excluded.deleted_at,
         title = excluded.title, size = excluded.size, file_id = COALESCE(excluded.file_id, drive_docs.file_id), pend_token = NULL`,
    ).bind(s.church, x.kind, x.id, x.revision, x.updatedAt, x.updatedBy, x.deletedAt, x.title, x.size, fileId));
  }
  await env.DB.batch(stmts);
  const last = rows[rows.length - 1];
  return { state: { cursor: [last.kind, last.id] }, copied: stmts.length };
}

// ---------- 성경 (한 단계에 책 하나) ----------
async function bibleToD1(env, s, st) {
  let todo = st.todo;
  if (!todo) {
    const vs = (await env.DB.prepare('SELECT version, name, language, license_note AS licenseNote, books FROM drive_bible WHERE church_id = ?1 ORDER BY version').bind(s.church).all()).results;
    todo = vs.map(v => ({ v: v.version, meta: { name: v.name, language: v.language, licenseNote: v.licenseNote }, books: Object.entries(JSON.parse(v.books || '{}')).map(([b, x]) => [Number(b), x.f]), made: false }));
  }
  const t = todo[0];
  if (!t) return { next: 'assets' };
  if (!t.made) { await versionD1(env, s, t.v, t.meta); t.made = true; return { state: { todo }, copied: 0 }; }
  const b = t.books.shift();
  if (!b) { todo.shift(); return { state: { todo } }; }
  const split = await driveBible.bookSplit(env, s.church, b[1]);
  if (split) {
    const chapters = Array.from({ length: split.head.chapterCount }, (_, i) => bookChapter(split, i + 1));
    const counts = chapters.map(c => c.reduce((m, x) => Math.max(m, x[0]), 0));
    await bookD1(env, s, t.v, b[0], { chapters, headings: bookHeadings(split), verses: chapters.reduce((n, c) => n + c.length, 0), counts });
  }
  return { state: { todo }, copied: 1 };
}

async function bibleToDrive(env, s, st) {
  let todo = st.todo;
  if (!todo) {
    const vs = (await env.DB.prepare('SELECT id, name, language, license_note AS licenseNote, structure FROM bible_versions WHERE church_id = ?1 ORDER BY id').bind(s.church).all()).results;
    todo = vs.map(v => ({ v: v.id, meta: { name: v.name, language: v.language, licenseNote: v.licenseNote }, books: Object.keys(JSON.parse(v.structure || '{}')).map(Number).sort((a, b) => a - b), made: false }));
  }
  const t = todo[0];
  if (!t) return { next: 'assets' };
  if (!t.made) { await driveBible.importVersion(env, s, t.v, t.meta); t.made = true; return { state: { todo }, copied: 0 }; }
  const book = t.books.shift();
  if (book === undefined) { todo.shift(); return { state: { todo } }; }
  const [vs, hs] = await env.DB.batch([
    env.DB.prepare('SELECT chapter, verse, text FROM bible_verses WHERE church_id = ?1 AND version = ?2 AND book = ?3 ORDER BY chapter, verse').bind(s.church, t.v, book),
    env.DB.prepare('SELECT chapter, verse, text FROM bible_headings WHERE church_id = ?1 AND version = ?2 AND book = ?3 ORDER BY chapter, verse, ord').bind(s.church, t.v, book),
  ]);
  const chapters = [];
  for (const x of vs.results) { while (chapters.length < x.chapter) chapters.push([]); chapters[x.chapter - 1].push([x.verse, x.text]); }
  if (chapters.length) {
    const counts = chapters.map(c => c.reduce((m, x) => Math.max(m, x[0]), 0));
    await driveBible.importBook(env, s, t.v, book, { chapters, headings: hs.results.map(h => [h.chapter, h.verse, h.text]), verses: vs.results.length, counts });
  }
  return { state: { todo }, copied: 1 };
}

// ---------- 파일 (2C 의 assets · asset_blobs 줄은 그대로, 바이트만 옮김) ----------
async function blobsAfter(env, church, cursor) {
  return (await env.DB.prepare('SELECT sha256, r2_key AS r2Key, size, type FROM asset_blobs WHERE church_id = ?1 AND purged_at IS NULL AND sha256 > ?2 ORDER BY sha256 LIMIT ?3')
    .bind(church, cursor || '', ASSETS_PER_STEP).all()).results;
}
async function assetsToR2(env, s, st) {
  const rows = await blobsAfter(env, s.church, st.cursor);
  if (!rows.length) return { next: 'done' };
  const src = driveBucket(env, s.church);
  for (const b of rows) {
    const obj = await src.get(b.r2Key);
    if (!obj) continue;
    let body = obj.body;
    if (typeof FixedLengthStream === 'function') { const fl = new FixedLengthStream(b.size); obj.body.pipeTo(fl.writable).catch(() => {}); body = fl.readable; }
    await env.FILES.put(b.r2Key, body, { httpMetadata: { contentType: b.type } });
  }
  return { state: { cursor: rows[rows.length - 1].sha256 }, copied: rows.length };
}
async function assetsToDrive(env, s, st) {
  const rows = await blobsAfter(env, s.church, st.cursor);
  if (!rows.length) return { next: 'done' };
  const dst = driveBucket(env, s.church);
  for (const b of rows) {
    if ((await env.DB.prepare('SELECT 1 FROM drive_files WHERE church_id = ?1 AND r2_key = ?2').bind(s.church, b.r2Key).first())) continue;
    const obj = env.FILES ? await env.FILES.get(b.r2Key) : null;
    if (!obj) continue;
    await dst.put(b.r2Key, obj.body, { size: b.size, sha256: b.sha256, httpMetadata: { contentType: b.type } });
  }
  return { state: { cursor: rows[rows.length - 1].sha256 }, copied: rows.length };
}

/** 옮긴 뒤 옛 저장소 정리. 무료 → 유료: D1 의 드라이브 색인만 지운다(사용자 드라이브의 파일은 그 사람 것이라 그대로). 유료 → 무료: D1 본문 · R2 바이트 지움 */
export async function cleanup(env, id) {
  const c = await churchRow(env, id);
  if (!c) return fail(404, '그런 교회가 없습니다.');
  const st = JSON.parse(c.moveState || '{}');
  if (c.moveTo || st.phase !== 'done') return fail(409, '옮기기가 끝난 뒤에만 정리합니다.');
  if (c.tier === 'paid') {
    await env.DB.batch([
      env.DB.prepare('DELETE FROM drive_docs WHERE church_id = ?1').bind(id),
      env.DB.prepare('DELETE FROM drive_bible WHERE church_id = ?1').bind(id),
      env.DB.prepare('DELETE FROM drive_files WHERE church_id = ?1').bind(id),
    ]);
  } else if (c.tier === 'free') {
    const keys = (await env.DB.prepare('SELECT r2_key AS k FROM asset_blobs WHERE church_id = ?1 AND purged_at IS NULL').bind(id).all()).results.map(x => x.k);
    for (let i = 0; i < keys.length && env.FILES; i += 500) await env.FILES.delete(keys.slice(i, i + 500));
    await env.DB.batch([
      env.DB.prepare('DELETE FROM documents WHERE church_id = ?1').bind(id),
      env.DB.prepare('DELETE FROM bible_verses WHERE church_id = ?1').bind(id),
      env.DB.prepare('DELETE FROM bible_headings WHERE church_id = ?1').bind(id),
      env.DB.prepare('DELETE FROM bible_versions WHERE church_id = ?1').bind(id),
    ]);
  }
  await env.DB.prepare('UPDATE churches SET move_state = NULL WHERE id = ?1').bind(id).run();
  return json({ ok: true, cleaned: c.tier === 'paid' ? 'drive-index' : 'd1-r2' });
}
