// 성경 넣기 (어느 등급이든 같은 길): 설치판 bible.db 사본 → PUT /api/v1/bible/versions/{id} · …/books/{책}
//   node scripts/bible-upload.mjs --db <bible.db> --list
//   node scripts/bible-upload.mjs --base https://<주소> --church <교회> --owner-secret <주인 열쇠> --db <bible.db> --version KRV [--only 43,45] --license-ok
// - 무료(구글 드라이브) 교회는 이 길로만 넣을 수 있다 (D1 에 바로 넣는 scripts/bible-import.mjs 는 유료 교회 것).
// - 실제 자료를 건드리지 않게 db 를 임시 폴더로 복사해 사본만 읽는다. 저작권 허락이 있는 번역본만 (--license-ok).
import { DatabaseSync } from 'node:sqlite';
import { copyFileSync, existsSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, join } from 'node:path';

const args = process.argv.slice(2);
const opt = (k, d) => { const i = args.indexOf(k); return i >= 0 ? args[i + 1] : d; };
const has = k => args.includes(k);
const die = m => { console.error(m); process.exit(2); };
const dbFile = opt('--db');
if (!dbFile || !existsSync(dbFile)) die('--db <bible.db 경로> 가 필요합니다.');
const work = mkdtempSync(join(tmpdir(), 'wp-bible-up-'));
const copy = join(work, basename(dbFile));
copyFileSync(dbFile, copy);
for (const x of ['-wal', '-shm']) if (existsSync(dbFile + x)) copyFileSync(dbFile + x, copy + x);
const db = new DatabaseSync(copy);
try {
  if (has('--list')) { for (const v of db.prepare('SELECT id, name, language, verse_count AS n FROM BibleVersions ORDER BY id').all()) console.log(`${v.id}\t${v.name}\t${v.language || ''}\t${v.n}`); process.exit(0); }
  if (!has('--license-ok')) die('--license-ok 를 같이 주세요 (그 번역본을 교회가 쓸 허락이 있을 때만).');
  const base = opt('--base', 'http://127.0.0.1:8787'), church = opt('--church'), secret = opt('--owner-secret') || process.env.WP_OWNER_SECRET, version = opt('--version');
  if (!church || !secret || !version) die('--church · --owner-secret(또는 WP_OWNER_SECRET) · --version 이 필요합니다.');
  const ver = db.prepare('SELECT id, name, language FROM BibleVersions WHERE id = ?').get(version);
  if (!ver) die(`번역본 ${version} 이 db 에 없습니다.`);
  const lr = await fetch(`${base}/auth/login`, { method: 'POST', headers: { 'content-type': 'application/json', 'x-worshipcue': '1' }, body: JSON.stringify({ church, secret }) });
  if (!lr.ok) die(`로그인 실패 ${lr.status}`);
  const cookie = lr.headers.getSetCookie().map(c => c.split(';')[0]).join('; ');
  const call = (path, body) => fetch(`${base}${path}`, { method: 'PUT', headers: { cookie, 'content-type': 'application/json', 'x-worshipcue': '1' }, body: JSON.stringify(body) }).then(async r => ({ status: r.status, json: await r.json().catch(() => ({})) }));
  let r = await call(`/api/v1/bible/versions/${encodeURIComponent(version)}`, { name: ver.name, language: ver.language || 'ko', licenseNote: opt('--license-note', '') });
  if (r.status !== 201) die(`번역본 만들기 실패 ${r.status} ${JSON.stringify(r.json)}`);
  const only = opt('--only') ? opt('--only').split(',').map(Number) : null;
  const books = db.prepare('SELECT DISTINCT book_id AS b FROM BibleVerses WHERE version_id = ? ORDER BY book_id').all(version).map(x => x.b).filter(b => !only || only.includes(b));
  for (const b of books) {
    const rows = db.prepare('SELECT chapter, verse, text FROM BibleVerses WHERE version_id = ? AND book_id = ? ORDER BY chapter, verse').all(version, b);
    const chapters = [];
    for (const x of rows) { while (chapters.length < x.chapter) chapters.push([]); chapters[x.chapter - 1].push([x.verse, x.text]); }
    const headings = db.prepare('SELECT chapter, verse, text FROM BibleHeadings WHERE version_id = ? AND book_id = ? ORDER BY chapter, verse').all(version, b).map(h => [h.chapter, h.verse, h.text]);
    r = await call(`/api/v1/bible/versions/${encodeURIComponent(version)}/books/${b}`, { chapters, headings });
    if (r.status !== 200) die(`${b}권 실패 ${r.status} ${JSON.stringify(r.json)}`);
    console.log(`${b}권 ${r.json.verses}절`);
  }
} finally {
  try { db.close(); } catch { /* 이미 */ }
  rmSync(work, { recursive: true, force: true });
}
