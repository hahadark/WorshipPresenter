// 무료 등급 성경: 번역본마다 드라이브 '성경/<번역본>/' 폴더, 책마다 파일 하나 (format.mjs bookFileText — 장마다 한 줄).
// 장절 확인은 D1 drive_bible.structure 로(유료와 같은 data/bible.mjs planPassage), 본문은 책 파일을 읽어 부른 장만 꺼낸다.
// 빠르게: 책 파일을 Worker 메모리에 기억(32 MB · 10분) + Cloudflare 캐시(사용자 도메인에서만 동작, workers.dev 에서는 없을 수 있음).
// 다시 넣으면 새 파일을 만들므로(파일 id 가 바뀜) 기억이 저절로 새것이 된다.
import { fail, json } from '../protocol.mjs';
import { VERSION_RE, passageReply, planPassage } from '../data/bible.mjs';
import { bookNames, getBook } from '../data/parser.mjs';
import { drive } from './google.mjs';
import { bookChapter, bookFileText, bookHeadings, cleanName, multipartBody, splitBook } from './format.mjs';
import { folderFor, trash } from './folders.mjs';

const books = new Map(); // `${church}/${fileId}` → { split, bytes }
let cachedBytes = 0;
const CACHE_LIMIT = 32 * 1024 * 1024;
const CACHE_MS = 10 * 60_000;

function remember(k, split, bytes) {
  if (bytes > CACHE_LIMIT / 4) return;
  while (cachedBytes + bytes > CACHE_LIMIT && books.size) {
    const [old, v] = books.entries().next().value;
    books.delete(old); cachedBytes -= v.bytes;
  }
  books.set(k, { split, bytes, at: Date.now() });
  cachedBytes += bytes;
}

/** 책 파일 → 나눈 것. 메모리 → Cloudflare 캐시 → 드라이브 차례 */
async function bookSplit(env, church, fileId) {
  const k = `${church}/${fileId}`;
  const hit = books.get(k);
  if (hit && Date.now() - hit.at < CACHE_MS) { books.delete(k); books.set(k, hit); return hit.split; }
  const cacheKey = `https://wp-drive-cache.invalid/bible/${encodeURIComponent(church)}/${encodeURIComponent(fileId)}`;
  const cache = typeof caches !== 'undefined' && caches.default ? caches.default : null;
  let text = null;
  if (cache) { try { const r = await cache.match(cacheKey); if (r) text = await r.text(); } catch { /* 캐시 없음 */ } }
  if (text === null) {
    const r = await drive(env, church, `/files/${encodeURIComponent(fileId)}?alt=media`, {}, { raw: true });
    text = await r.text();
    if (cache) { try { await cache.put(cacheKey, new Response(text, { headers: { 'cache-control': 'private, max-age=86400', 'content-type': 'application/json' } })); } catch { /* 캐시 없음 */ } }
  }
  const split = splitBook(text);
  if (split) remember(k, split, text.length * 2);
  return split;
}

const verRow = (env, church, v) => env.DB.prepare('SELECT version AS id, name, language, license_note AS licenseNote, verses, structure, folder_id AS folderId, meta_file_id AS metaFileId, books, imported_at AS importedAt FROM drive_bible WHERE church_id = ?1 AND version = ?2').bind(church, v).first();

export async function versions(request, env, url, s) {
  const r = await env.DB.prepare('SELECT version AS id, name, language, license_note AS licenseNote, verses, imported_at AS importedAt FROM drive_bible WHERE church_id = ?1 ORDER BY name').bind(s.church).all();
  return json({ ok: true, versions: r.results });
}

export async function passage(request, env, url, s) {
  const v = url.searchParams.get('version') || '';
  const ref = url.searchParams.get('ref') || '';
  if (!VERSION_RE.test(v)) return fail(400, '번역본(version)을 골라 주세요.');
  if (ref.length > 200) return fail(400, '장절이 너무 깁니다.');
  const ver = await verRow(env, s.church, v);
  if (!ver) return fail(404, '선택한 번역본 데이터가 없습니다.');
  const plan = planPassage(ver, ref);
  if (plan.error) return plan.error;
  let map = {};
  try { map = JSON.parse(ver.books || '{}'); } catch { /* 빈 것 */ }
  const fileId = map[plan.bookId]?.f;
  if (!fileId) return plan.bad('해당 구절의 본문이 없습니다.');
  const split = await bookSplit(env, s.church, fileId);
  if (!split) return fail(502, '드라이브의 성경 파일을 읽을 수 없습니다 (드라이브에서 고쳤을 수 있습니다).', { code: 'drive_corrupt' });
  const lists = plan.norm.map(([sc, sv, ec, ev]) => {
    const out = [];
    for (let c = sc; c <= ec; c++) {
      for (const [verse, text] of bookChapter(split, c)) {
        if ((c > sc || verse >= sv) && (c < ec || verse <= ev)) out.push({ book: plan.bookId, chapter: c, verse, text });
      }
    }
    return out;
  });
  const first = plan.norm[0][0], last = plan.norm[plan.norm.length - 1][2];
  const headings = bookHeadings(split).filter(h => h[0] >= first && h[0] <= last).map(([chapter, verse, text]) => ({ chapter, verse, text }));
  return passageReply(ver, plan, lists, headings);
}

// ---------- 넣기 (주인) ----------
export async function countVersions(env, church) {
  return (await env.DB.prepare('SELECT COUNT(*) AS n FROM drive_bible WHERE church_id = ?1').bind(church).first())?.n || 0;
}

/** 번역본 만들기 · 다시 넣기 (책은 비운 채로). meta = { name, language, licenseNote } */
export async function importVersion(env, s, id, meta) {
  const old = await verRow(env, s.church, id);
  if (old?.folderId) await trash(env, s.church, old.folderId);
  const parent = await folderFor(env, s.church, 'bible');
  const folder = await drive(env, s.church, '/files?fields=id', {
    method: 'POST', headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ name: cleanName(id, 40), mimeType: 'application/vnd.google-apps.folder', parents: [parent], appProperties: { wp: '1', wpBible: id } }),
  });
  const m = multipartBody({ name: '번역본.json', parents: [folder.id], mimeType: 'application/json' },
    JSON.stringify({ worshipPresenter: 1, kind: 'bible-version', id, name: meta.name, language: meta.language, licenseNote: meta.licenseNote }, null, 1));
  const f = await drive(env, s.church, 'upload:/files?uploadType=multipart&fields=id', { method: 'POST', headers: { 'content-type': m.contentType }, body: m.body });
  await env.DB.prepare(
    `INSERT INTO drive_bible (church_id, version, name, language, license_note, verses, structure, folder_id, meta_file_id, books, imported_at)
     VALUES (?1, ?2, ?3, ?4, ?5, 0, '{}', ?6, ?7, '{}', ?8)
     ON CONFLICT (church_id, version) DO UPDATE SET name = excluded.name, language = excluded.language, license_note = excluded.license_note, verses = 0,
       structure = '{}', folder_id = excluded.folder_id, meta_file_id = excluded.meta_file_id, books = '{}', imported_at = excluded.imported_at`,
  ).bind(s.church, id, meta.name, meta.language, meta.licenseNote, folder.id, f.id, Date.now()).run();
  return json({ ok: true, version: id }, 201);
}

/** 책 하나 넣기. c = format.checkBook 결과 */
export async function importBook(env, s, id, book, c) {
  const ver = await verRow(env, s.church, id);
  if (!ver) return fail(404, '먼저 번역본을 만들어 주세요 (PUT /api/v1/bible/versions/{id}).');
  let map = {};
  try { map = JSON.parse(ver.books || '{}'); } catch { /* 빈 것 */ }
  const b = getBook(book);
  const name = b ? bookNames(b, ver.language).name : `${book}권`;
  const text = bookFileText({ version: id, book, name, chapters: c.chapters, headings: c.headings });
  const m = multipartBody({ name: `${String(book).padStart(2, '0')} ${cleanName(name, 40)}.json`, parents: [ver.folderId], mimeType: 'application/json' }, text);
  const f = await drive(env, s.church, 'upload:/files?uploadType=multipart&fields=id', { method: 'POST', headers: { 'content-type': m.contentType }, body: m.body });
  const key = `$."${book}"`;
  await env.DB.prepare(
    `UPDATE drive_bible SET books = json_set(books, ?3, json(?4)), structure = json_set(structure, ?3, json(?5)),
       verses = (SELECT COALESCE(SUM(json_extract(value, '$.n')), 0) FROM json_each(json_set(drive_bible.books, ?3, json(?4)))), imported_at = ?6
     WHERE church_id = ?1 AND version = ?2`,
  ).bind(s.church, id, key, JSON.stringify({ f: f.id, n: c.verses }), JSON.stringify(c.counts), Date.now()).run();
  // 앞서 넣은 같은 책 파일은 휴지통으로 (새 파일 id 라 Worker 기억이 저절로 새것이 됨)
  if (map[book]?.f) await trash(env, s.church, map[book].f);
  return json({ ok: true, version: id, book, verses: c.verses, chapters: c.chapters.length });
}

export async function deleteVersion(env, s, id) {
  const ver = await verRow(env, s.church, id);
  if (!ver) return fail(404, '그런 번역본이 없습니다.');
  if (ver.folderId) await trash(env, s.church, ver.folderId);
  await env.DB.prepare('DELETE FROM drive_bible WHERE church_id = ?1 AND version = ?2').bind(s.church, id).run();
  return json({ ok: true, deleted: true });
}

/** 옮기기(move.mjs)용: 번역본 줄 · 책 파일 글 */
export { verRow, bookSplit };
