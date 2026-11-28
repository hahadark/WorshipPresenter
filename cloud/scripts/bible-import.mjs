// 성경 넣기 도구: 설치판 bible.db(복사본) → D1 bible_versions · bible_verses · bible_headings
//   node scripts/bible-import.mjs --db <bible.db> --list                                  번역본 목록만
//   node scripts/bible-import.mjs --db <bible.db> --version KRV --church sample --out krv.sql   SQL 파일만 만듦
//   node scripts/bible-import.mjs --db <bible.db> --version KRV --church sample --local         로컬 D1 (wrangler dev) 에 넣음
//   … --only 43:1-3,45:8   책:장 범위만 (견본 · 시험용)
//   … --remote --license-ok   배포한 D1 에 넣음. 저작권 허락이 있는 번역본만 — 어떤 번역본을 올릴지는 사관님이 정한다.
// - 실제 자료를 건드리지 않게 db 를 먼저 임시 폴더로 복사해서(-wal · -shm 포함) 그 사본만 읽는다.
// - 같은 교회 · 같은 번역본을 다시 넣으면 앞의 것을 지우고 새로 넣는다.
// - D1 은 SQL 문 하나 100 KB(바이트)까지라 INSERT 를 90,000 바이트 아래로 나눈다. 글자 수가 아니라 UTF-8 바이트로 잰다
//   (한글은 한 글자 3바이트 — 글자 수로 재던 때는 장이 많은 번역본에서 SQLITE_TOOBIG 으로 실패했다).
// - 넣을 때는 SQL 을 4 MiB 아래의 파일 여러 개로 나눠 차례로 실행한다 (한 번에 큰 파일을 보내지 않게).
import { DatabaseSync } from 'node:sqlite';
import { copyFileSync, existsSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';

const cloud = join(dirname(fileURLToPath(import.meta.url)), '..');
const args = process.argv.slice(2);
const opt = (k, d) => { const i = args.indexOf(k); return i >= 0 ? args[i + 1] : d; };
const has = k => args.includes(k);
const die = m => { console.error(m); process.exit(2); };

const dbFile = opt('--db');
if (!dbFile || !existsSync(dbFile)) die('--db <bible.db 경로> 가 필요합니다.');
const church = opt('--church', 'sample');
if (!/^[a-z0-9][a-z0-9-]{0,38}[a-z0-9]$/.test(church)) die('교회 ID 는 영문 소문자 · 숫자 · - 로 2~40자');
const config = opt('--config', 'wrangler.jsonc');
if (has('--remote') && !has('--license-ok')) die('--remote 는 --license-ok 와 같이 주세요 (그 번역본을 교회가 쓸 허락이 있을 때만).');

// ---------- 사본 만들기 ----------
const work = mkdtempSync(join(tmpdir(), 'wp-bible-'));
const copy = join(work, basename(dbFile));
copyFileSync(dbFile, copy);
for (const x of ['-wal', '-shm']) if (existsSync(dbFile + x)) copyFileSync(dbFile + x, copy + x);
const db = new DatabaseSync(copy);

function cleanup() {
  try { db.close(); } catch { /* 이미 */ }
  rmSync(work, { recursive: true, force: true });
}
process.on('exit', cleanup); // die() · process.exit 로 끝나도 사본을 지운다

try {
  const versions = db.prepare("SELECT id, name, COALESCE(language,'') AS language, COALESCE(copyright,'') AS copyright, COALESCE(license,'') AS license, COALESCE(verse_count,0) AS verses FROM BibleVersions ORDER BY name").all();
  if (has('--list')) {
    for (const v of versions) console.log(`${v.id}\t${v.name}\t${v.language || 'ko'}\t${v.verses}절\t${v.copyright || v.license}`);
    process.exit(0);
  }
  const versionId = opt('--version');
  const v = versions.find(x => x.id === versionId);
  if (!v) die(`--version 이 없습니다. 있는 것: ${versions.map(x => x.id).join(', ')}`);
  if (!/^[A-Za-z0-9_.-]{1,40}$/.test(v.id)) die(`번역본 id '${v.id}' 는 웹판에서 쓸 수 없는 글자가 있습니다.`);

  // --only 43:1-3,45:8,19 → [[책, 첫 장, 끝 장]]
  const only = opt('--only') ? opt('--only').split(',').map(t => {
    const m = /^(\d+)(?::(\d+)(?:-(\d+))?)?$/.exec(t.trim());
    if (!m) die(`--only 모양이 틀렸습니다: ${t} (예: 43:1-3,45:8,19)`);
    return [Number(m[1]), m[2] ? Number(m[2]) : 1, m[3] ? Number(m[3]) : m[2] ? Number(m[2]) : 999];
  }) : null;
  const inOnly = (b, c) => !only || only.some(([ob, s, e]) => ob === b && c >= s && c <= e);

  const rows = db.prepare('SELECT book_id AS b, chapter AS c, verse AS v, text FROM BibleVerses WHERE version_id = ? ORDER BY book_id, chapter, verse').all(v.id).filter(r => inOnly(r.b, r.c));
  let heads = [];
  try { heads = db.prepare('SELECT book_id AS b, chapter AS c, verse AS v, text FROM BibleHeadings WHERE version_id = ? ORDER BY book_id, chapter, verse, rowid').all(v.id).filter(r => inOnly(r.b, r.c)); } catch { /* 옛 db */ }
  if (!rows.length) die('넣을 절이 없습니다.');

  // 장마다 절 수 (설치판 reload_structure 와 같이: 장 번호 자리에 그 장의 가장 큰 절 번호). 넣는 책만, 장은 그 책 전체
  const books = new Set(rows.map(r => r.b));
  const structure = {};
  for (const r of db.prepare('SELECT book_id AS b, chapter AS c, MAX(verse) AS m FROM BibleVerses WHERE version_id = ? GROUP BY book_id, chapter').all(v.id)) {
    if (!books.has(r.b)) continue;
    const ch = (structure[r.b] ||= []);
    while (ch.length < r.c) ch.push(0);
    ch[r.c - 1] = r.m;
  }

  const q = s => `'${String(s).replace(/'/g, "''")}'`;
  /** D1 SQL 문 하나의 한도는 100,000 바이트 — 넉넉히 아래로 */
  const MAX_STATEMENT = Number(opt('--max-statement', 90_000));
  /** 한 번에 실행하는 SQL 파일 크기 */
  const MAX_FILE = Number(opt('--max-file', 4 * 1024 * 1024));
  const now = Date.now();
  const sql = [
    `DELETE FROM bible_verses WHERE church_id = ${q(church)} AND version = ${q(v.id)};`,
    `DELETE FROM bible_headings WHERE church_id = ${q(church)} AND version = ${q(v.id)};`,
    `INSERT INTO bible_versions (church_id, id, name, license_note, verses, language, structure, imported_at) VALUES (${q(church)}, ${q(v.id)}, ${q(v.name)}, ${q(v.copyright || v.license)}, ${rows.length}, ${q(v.language || 'ko')}, ${q(JSON.stringify(structure))}, ${now})
     ON CONFLICT (church_id, id) DO UPDATE SET name = excluded.name, license_note = excluded.license_note, verses = excluded.verses, language = excluded.language, structure = excluded.structure, imported_at = excluded.imported_at;`,
  ];
  const bytes = t => Buffer.byteLength(t, 'utf8');
  const chunked = (table, cols, list, row) => {
    const head = `INSERT INTO ${table} (${cols}) VALUES\n`;
    let cur = [], size = bytes(head) + 1;
    const flush = () => { if (cur.length) sql.push(`${head}${cur.join(',\n')};`); cur = []; size = bytes(head) + 1; };
    for (const x of list) {
      const t = row(x);
      const n = bytes(t) + 2;
      if (size + n > MAX_STATEMENT && cur.length) flush();
      if (bytes(head) + n > MAX_STATEMENT) die(`절 하나가 SQL 문 한도보다 큽니다 (${n} 바이트).`);
      cur.push(t); size += n;
    }
    flush();
  };
  chunked('bible_verses', 'church_id, version, book, chapter, verse, text', rows, r => `(${q(church)},${q(v.id)},${r.b},${r.c},${r.v},${q(r.text)})`);
  const ord = new Map();
  chunked('bible_headings', 'church_id, version, book, chapter, verse, ord, text', heads, h => {
    const k = `${h.b}/${h.c}/${h.v}`; const o = (ord.get(k) || 0) + 1; ord.set(k, o);
    return `(${q(church)},${q(v.id)},${h.b},${h.c},${h.v},${o},${q(h.text)})`;
  });
  const text = sql.join('\n');
  const out = opt('--out') || join(work, `bible-${church}-${v.id}.sql`);
  writeFileSync(out, text);
  console.log(`${v.name}(${v.id}) → 교회 '${church}': 절 ${rows.length}개 · 소제목 ${heads.length}개 · 책 ${books.size}권 · SQL ${(text.length / 1024).toFixed(0)} KiB`);
  if (has('--local') || has('--remote')) {
    const wrangler = join(cloud, 'node_modules', 'wrangler', 'bin', 'wrangler.js');
    // SQL 문을 4 MiB 아래의 파일 여러 개로 나눠 차례로 (맨 앞 파일에 지우기 · 번역본 줄이 먼저 들어간다)
    const parts = [];
    let cur = [], size = 0;
    for (const st of sql) {
      const n = bytes(st) + 1;
      if (size + n > MAX_FILE && cur.length) { parts.push(cur.join('\n')); cur = []; size = 0; }
      cur.push(st); size += n;
    }
    if (cur.length) parts.push(cur.join('\n'));
    parts.forEach((t, i) => {
      const f = join(work, `part-${String(i + 1).padStart(3, '0')}.sql`);
      writeFileSync(f, t);
      execFileSync(process.execPath, [wrangler, 'd1', 'execute', 'worshippresenter-web', has('--remote') ? '--remote' : '--local', '-c', config, '--file', f, '-y'], {
        cwd: cloud, stdio: ['ignore', has('--quiet') ? 'ignore' : 'inherit', 'inherit'], env: { ...process.env, WRANGLER_SEND_METRICS: 'false' },
      });
      if (parts.length > 1) console.log(`  ${i + 1}/${parts.length} 넣음`);
    });
    console.log(has('--remote') ? '배포한 D1 에 넣었습니다.' : '로컬 D1 에 넣었습니다.');
  } else if (opt('--out')) console.log(`SQL: ${out}`);
} finally {
  cleanup();
}
