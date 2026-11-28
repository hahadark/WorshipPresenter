// 성경 넣기 길 (등급과 상관없이 같은 길 — 7단계, docs/web/tiers.md 4절). 주인(manage)만.
//   PUT    /api/v1/bible/versions/{id}               {name, language?, licenseNote?}    → 번역본 만들기 · 다시 넣기(책은 비움)
//   PUT    /api/v1/bible/versions/{id}/books/{책}    {chapters:[[[절, 본문], …], …], headings?:[[장, 절, 글], …]}
//   DELETE /api/v1/bible/versions/{id}
// 유료 = D1 bible_versions · bible_verses · bible_headings (넣기 도구 scripts/bible-import.mjs 와 같은 표), 무료 = 드라이브 책 파일 (drive/bible.mjs).
// 넣기 도구 scripts/bible-upload.mjs 가 bible.db 사본을 책마다 이 길로 보낸다 (어느 등급이든).
import { fail, json } from '../protocol.mjs';
import { VERSION_RE } from './bible.mjs';
import { readBody } from './documents.mjs';
import { checkBook } from '../drive/format.mjs';
import * as driveBible from '../drive/bible.mjs';
import { limitsOf } from '../tiers/limits.mjs';

const BOOK_MAX = 3 * 1024 * 1024; // 시편 ≈ 0.3~0.5 MB

function parts(url) {
  const p = url.pathname.split('/'); // ['', 'api', 'v1', 'bible', 'versions', id, 'books', n]
  return { id: decodeURIComponent(p[5] || ''), book: p[7] !== undefined ? Number(p[7]) : null };
}

export async function handleImport(request, env, url, s, r, tier) {
  const { id, book } = parts(url);
  if (!VERSION_RE.test(id)) return fail(400, '번역본 id 는 영문 · 숫자 · _ . - 로 40자까지입니다.');
  const drive = tier.store === 'drive';
  if (r.op === 'deleteVersion') return drive ? driveBible.deleteVersion(env, s, id) : deleteD1(env, s, id);
  if (r.op === 'importVersion') {
    const b = await readBody(request, 8000);
    if (b.error) return b.error;
    const a = b.value || {};
    const meta = { name: String(a.name || id).slice(0, 80), language: /^[a-z]{2}(-[A-Za-z]{2})?$/.test(a.language || '') ? a.language : 'ko', licenseNote: String(a.licenseNote || '').slice(0, 400) };
    const L = limitsOf(tier.tier, env);
    const have = drive ? await driveBible.countVersions(env, s.church) : (await env.DB.prepare('SELECT COUNT(*) AS n FROM bible_versions WHERE church_id = ?1').bind(s.church).first())?.n || 0;
    const exists = drive ? !!(await driveBible.verRow(env, s.church, id)) : !!(await env.DB.prepare('SELECT 1 FROM bible_versions WHERE church_id = ?1 AND id = ?2').bind(s.church, id).first());
    if (!exists && L.bibleVersions !== null && have >= L.bibleVersions) return fail(429, `이 등급은 번역본을 ${L.bibleVersions}개까지 넣습니다.`, { code: 'tier_limit', limit: 'bibleVersions', tier: tier.tier });
    return drive ? driveBible.importVersion(env, s, id, meta) : versionD1(env, s, id, meta);
  }
  if (!Number.isInteger(book) || book < 1 || book > 66) return fail(400, '책 번호는 1~66 입니다.');
  const b = await readBody(request, BOOK_MAX);
  if (b.error) return b.error;
  const c = checkBook(b.value);
  if (c.error) return fail(400, c.error);
  return drive ? driveBible.importBook(env, s, id, book, c) : bookD1(env, s, id, book, c);
}

// ---------- 유료 (D1) ----------
async function versionD1(env, s, id, meta) {
  // 다시 넣으면 앞의 본문을 지운다 (bible-import.mjs 와 같음)
  await env.DB.batch([
    env.DB.prepare('DELETE FROM bible_verses WHERE church_id = ?1 AND version = ?2').bind(s.church, id),
    env.DB.prepare('DELETE FROM bible_headings WHERE church_id = ?1 AND version = ?2').bind(s.church, id),
    env.DB.prepare(
      `INSERT INTO bible_versions (church_id, id, name, license_note, verses, language, structure, imported_at) VALUES (?1, ?2, ?3, ?4, 0, ?5, '{}', ?6)
       ON CONFLICT (church_id, id) DO UPDATE SET name = excluded.name, license_note = excluded.license_note, verses = 0, language = excluded.language, structure = '{}', imported_at = excluded.imported_at`,
    ).bind(s.church, id, meta.name, meta.licenseNote, meta.language, Date.now()),
  ]);
  return json({ ok: true, version: id }, 201);
}

const q = t => `'${String(t).replace(/'/g, "''")}'`;
/** 본문은 글로 묶어 넣는다 (D1 원격은 문 하나 100 KB · 묶을 값 100개 — 교회 · 번역본만 ?1 ?2 로) */
function chunks(rows, head, fmt, max = 90_000) {
  const out = [];
  let cur = [], len = head.length;
  for (const r of rows) {
    const t = fmt(r);
    if (cur.length && len + t.length + 1 > max) { out.push(head + cur.join(',')); cur = []; len = head.length; }
    cur.push(t); len += t.length + 1;
  }
  if (cur.length) out.push(head + cur.join(','));
  return out;
}

async function bookD1(env, s, id, book, c) {
  const ver = await env.DB.prepare('SELECT id FROM bible_versions WHERE church_id = ?1 AND id = ?2').bind(s.church, id).first();
  if (!ver) return fail(404, '먼저 번역본을 만들어 주세요 (PUT /api/v1/bible/versions/{id}).');
  const verses = c.chapters.flatMap((ch, i) => ch.map(([v, t]) => [i + 1, v, t]));
  const stmts = [
    env.DB.prepare(
      `UPDATE bible_versions SET verses = verses - (SELECT COUNT(*) FROM bible_verses WHERE church_id = ?1 AND version = ?2 AND book = ?3) + ?4,
         structure = json_set(structure, ?5, json(?6)), imported_at = ?7 WHERE church_id = ?1 AND id = ?2`,
    ).bind(s.church, id, book, verses.length, `$."${book}"`, JSON.stringify(c.counts), Date.now()),
    env.DB.prepare('DELETE FROM bible_verses WHERE church_id = ?1 AND version = ?2 AND book = ?3').bind(s.church, id, book),
    env.DB.prepare('DELETE FROM bible_headings WHERE church_id = ?1 AND version = ?2 AND book = ?3').bind(s.church, id, book),
    ...chunks(verses, 'INSERT INTO bible_verses (church_id, version, book, chapter, verse, text) VALUES ', ([ch, v, t]) => `(?1,?2,${book},${ch},${v},${q(t)})`)
      .map(sql => env.DB.prepare(sql).bind(s.church, id)),
    ...chunks(c.headings.map((h, i) => [...h, i]), 'INSERT OR IGNORE INTO bible_headings (church_id, version, book, chapter, verse, ord, text) VALUES ', ([ch, v, t, i]) => `(?1,?2,${book},${ch},${v},${i},${q(t)})`)
      .map(sql => env.DB.prepare(sql).bind(s.church, id)),
  ];
  await env.DB.batch(stmts);
  return json({ ok: true, version: id, book, verses: verses.length, chapters: c.chapters.length });
}

async function deleteD1(env, s, id) {
  const r = await env.DB.batch([
    env.DB.prepare('DELETE FROM bible_verses WHERE church_id = ?1 AND version = ?2').bind(s.church, id),
    env.DB.prepare('DELETE FROM bible_headings WHERE church_id = ?1 AND version = ?2').bind(s.church, id),
    env.DB.prepare('DELETE FROM bible_versions WHERE church_id = ?1 AND id = ?2').bind(s.church, id),
  ]);
  if (!r[2].meta.changes) return fail(404, '그런 번역본이 없습니다.');
  return json({ ok: true, deleted: true });
}

export { bookD1, versionD1 };
