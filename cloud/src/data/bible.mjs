// 성경 API — 설치판 db.rs list_versions · resolve_reference 와 같은 답 모양. 그 교회가 넣은 번역본만 (church_id = ?1).
//   GET /api/v1/bible/versions
//   GET /api/v1/bible/passage?version=&ref=요3:16
// 장 · 절 수는 번역본 줄의 structure(넣을 때 계산)로 확인해서, 절 줄은 부른 범위만 읽는다 (D1 행 읽기 아끼기).
import { fail, json } from '../protocol.mjs';
import { DATA_LIMITS } from './routes.mjs';
import { bookNames, formatTitle, getBook, parseReference } from './parser.mjs';

export const VERSION_RE = /^[A-Za-z0-9_.-]{1,40}$/;

export async function versions(request, env, url, s) {
  const r = await env.DB.prepare(
    'SELECT id, name, language, license_note AS licenseNote, verses, imported_at AS importedAt FROM bible_versions WHERE church_id = ?1 ORDER BY name',
  ).bind(s.church).all();
  return json({ ok: true, versions: r.results });
}

function bookOf(lang, id) {
  const b = getBook(id);
  if (!b) return { id, name: `${id}권`, short: String(id) };
  return { id: b.id, ...bookNames(b, lang) };
}

/**
 * 장절 확인 (유료 D1 · 무료 드라이브가 같이 쓴다). ver = { id, name, language, structure(글) }
 * → { error: Response } | { book, bookId, norm:[[sc, sv, ec, ev], …] }
 */
export function planPassage(ver, ref) {
  let structure;
  try { structure = JSON.parse(ver.structure || '{}'); } catch { structure = {}; }
  const lang = ver.language || 'ko';
  const chapters = id => (structure[id] || []).length;
  const versesIn = (id, c) => (structure[id] || [])[c - 1] || 0;
  const withChapters = b => ({ ...bookOf(lang, b.id), chapters: chapters(b.id) });

  const p = parseReference(ref);
  if (!p.ok) {
    return { error: json({
      ok: false, error: p.error, incomplete: p.incomplete, ambiguous: p.ambiguous, rest: p.rest,
      book: p.book ? withChapters(p.book) : null, candidates: p.candidates.map(b => bookOf(lang, b.id)),
    }, 422) };
  }
  const book = bookOf(lang, p.book.id);
  const bad = error => ({ error: json({ ok: false, error, book: withChapters(p.book) }, 422) });
  const nCh = chapters(p.book.id);
  if (!nCh) return bad(`이 번역본에는 ${book.name} 데이터가 없습니다.`);
  if (p.ranges.length > DATA_LIMITS.ranges) return bad(`한 번에 ${DATA_LIMITS.ranges}묶음까지 부를 수 있습니다.`);
  const norm = [];
  for (const r of p.ranges) {
    if (r.sc > nCh || r.ec > nCh) return bad(`${book.name}은(는) ${nCh}장까지 있습니다.`);
    const sv = r.sv ?? 1;
    const ev = r.ev ?? versesIn(p.book.id, r.ec);
    const maxS = versesIn(p.book.id, r.sc), maxE = versesIn(p.book.id, r.ec);
    if (sv > maxS) return bad(`${book.name} ${r.sc}장은 ${maxS}절까지 있습니다.`);
    if (ev > maxE) return bad(`${book.name} ${r.ec}장은 ${maxE}절까지 있습니다.`);
    norm.push([r.sc, sv, r.ec, ev]);
  }
  return { book, bookId: p.book.id, norm, bad: msg => bad(msg).error };
}

/** 찾은 절들 → 답 (설치판 db.rs resolve_reference 모양). lists = 묶음마다 [{book, chapter, verse, text}] */
export function passageReply(ver, plan, lists, headings) {
  const verses = [];
  const seen = new Set();
  for (const r of lists) {
    for (const x of r) {
      const k = x.chapter * 1000 + x.verse;
      if (!seen.has(k)) { seen.add(k); verses.push(x); }
    }
  }
  if (!verses.length) return plan.bad('해당 구절의 본문이 없습니다.');
  const { norm, book } = plan;
  return json({
    ok: true,
    passage: {
      versionId: ver.id, versionName: ver.name, book,
      ranges: norm.map(([sc, sv, ec, ev]) => ({ sc, sv, ec, ev })),
      title: formatTitle(book.name, norm),
      verses, headings,
    },
  });
}

export async function passage(request, env, url, s) {
  const v = url.searchParams.get('version') || '';
  const ref = url.searchParams.get('ref') || '';
  if (!VERSION_RE.test(v)) return fail(400, '번역본(version)을 골라 주세요.');
  if (ref.length > 200) return fail(400, '장절이 너무 깁니다.');
  const ver = await env.DB.prepare('SELECT id, name, language, structure FROM bible_versions WHERE church_id = ?1 AND id = ?2').bind(s.church, v).first();
  if (!ver) return fail(404, '선택한 번역본 데이터가 없습니다.');
  const plan = planPassage(ver, ref);
  if (plan.error) return plan.error;
  const { norm, bookId } = plan;
  const verseQ = env.DB.prepare(
    `SELECT book, chapter, verse, text FROM bible_verses
     WHERE church_id = ?1 AND version = ?2 AND book = ?3 AND (chapter, verse) >= (?4, ?5) AND (chapter, verse) <= (?6, ?7) ORDER BY chapter, verse`,
  );
  const first = norm[0], last = norm[norm.length - 1];
  const res = await env.DB.batch([
    ...norm.map(([sc, sv, ec, ev]) => verseQ.bind(s.church, v, bookId, sc, sv, ec, ev)),
    env.DB.prepare('SELECT chapter, verse, text FROM bible_headings WHERE church_id = ?1 AND version = ?2 AND book = ?3 AND chapter BETWEEN ?4 AND ?5 ORDER BY chapter, verse, ord')
      .bind(s.church, v, bookId, first[0], last[2]),
  ]);
  return passageReply(ver, plan, res.slice(0, -1).map(r => r.results), res[res.length - 1].results);
}
