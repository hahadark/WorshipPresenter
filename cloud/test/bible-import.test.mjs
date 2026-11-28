// 성경 넣기 도구(scripts/bible-import.mjs): 장이 많은 한글 번역본도 SQL 문 하나가 D1 한도(100,000 바이트) 아래로 나뉜다.
// 예전에는 글자 수로 90,000 을 재서 한글(3바이트)이면 문 하나가 ~270 KB → 로컬 · 원격 D1 에서 SQLITE_TOOBIG.
// 견본은 지어낸 본문이다 (실제 번역본 아님).
import test from 'node:test';
import assert from 'node:assert/strict';
import { DatabaseSync } from 'node:sqlite';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const cloud = join(dirname(fileURLToPath(import.meta.url)), '..');

test('장이 많은 한글 번역본: SQL 문마다 90,000 바이트 아래 · 절은 하나도 빠지지 않음', () => {
  const dir = mkdtempSync(join(tmpdir(), 'wp-bible-big-'));
  try {
    const file = join(dir, 'bible.db');
    const db = new DatabaseSync(file);
    db.exec(`CREATE TABLE BibleVersions (id TEXT PRIMARY KEY, name TEXT NOT NULL, language TEXT, copyright TEXT, license TEXT, source TEXT, imported_at TEXT, verse_count INTEGER DEFAULT 0);
      CREATE TABLE BibleVerses (rowid INTEGER PRIMARY KEY, version_id TEXT NOT NULL, book_id INTEGER NOT NULL, chapter INTEGER NOT NULL, verse INTEGER NOT NULL, text TEXT NOT NULL, search_text TEXT NOT NULL, search_compact TEXT NOT NULL);`);
    const ins = db.prepare('INSERT INTO BibleVerses (version_id, book_id, chapter, verse, text, search_text, search_compact) VALUES (?, ?, ?, ?, ?, ?, ?)');
    db.exec('BEGIN');
    let n = 0;
    for (let c = 1; c <= 150; c++) for (let v = 1; v <= 20; v++) { const t = '지어낸 견본 시편 본문입니다 '.repeat(4) + `${c}:${v}`; ins.run('BIG', 19, c, v, t, t, t); n++; }
    db.exec('COMMIT');
    db.prepare('INSERT INTO BibleVersions (id, name, language, copyright, verse_count) VALUES (?, ?, ?, ?, ?)').run('BIG', '견본 큰 번역본', 'ko', '견본', n);
    db.close();
    const out = join(dir, 'out.sql');
    execFileSync(process.execPath, [join(cloud, 'scripts', 'bible-import.mjs'), '--db', file, '--version', 'BIG', '--church', 'sample', '--out', out], { stdio: 'ignore' });
    const sql = readFileSync(out, 'utf8');
    const stmts = sql.split(/;\n(?=DELETE|INSERT)/);
    assert.ok(Buffer.byteLength(sql) > 300_000, '시험 자료가 충분히 큼 (예전 방식이면 문 하나가 한도를 넘는 크기)');
    const big = stmts.map(s => Buffer.byteLength(s, 'utf8')).filter(b => b > 90_000);
    assert.deepEqual(big, [], 'SQL 문 하나도 90,000 바이트를 넘지 않음');
    const verseInserts = stmts.filter(s => s.startsWith('INSERT INTO bible_verses'));
    assert.ok(verseInserts.length >= 4, `여러 문으로 나뉨 (${verseInserts.length})`);
    const rows = verseInserts.reduce((a, s) => a + (s.match(/\('sample','BIG',19,/g) || []).length, 0);
    assert.equal(rows, n, '절이 빠지지 않음');
    assert.match(sql, /"19":\[(20,){149}20\]/, '장마다 절 수(150장)');
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
