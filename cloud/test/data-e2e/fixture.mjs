// 시험용 견본 성경 db (설치판 bible.db 와 같은 표 모양). 본문은 지어낸 글이다 — 실제 번역본을 쓰지 않는다 (저작권).
// 장 · 절 수만 실제 성경과 같게 몇 장 분량: 창 1-3 · 시 23 · 요 1 · 요 3 · 롬 8 · 요일 1 · 유 1
import { DatabaseSync } from 'node:sqlite';

const SCHEMA = `
CREATE TABLE IF NOT EXISTS BibleVersions (id TEXT PRIMARY KEY, name TEXT NOT NULL, language TEXT, copyright TEXT, license TEXT, source TEXT, imported_at TEXT, verse_count INTEGER DEFAULT 0);
CREATE TABLE IF NOT EXISTS BibleVerses (rowid INTEGER PRIMARY KEY, version_id TEXT NOT NULL, book_id INTEGER NOT NULL, chapter INTEGER NOT NULL, verse INTEGER NOT NULL, text TEXT NOT NULL, search_text TEXT NOT NULL, search_compact TEXT NOT NULL, UNIQUE (version_id, book_id, chapter, verse));
CREATE TABLE IF NOT EXISTS BibleHeadings (version_id TEXT NOT NULL, book_id INTEGER NOT NULL, chapter INTEGER NOT NULL, verse INTEGER NOT NULL, text TEXT NOT NULL);
`;

/** [책, 장, 절 수] */
export const SHAPE = [[1, 1, 31], [1, 2, 25], [1, 3, 24], [19, 23, 6], [43, 1, 51], [43, 3, 36], [45, 8, 39], [62, 1, 10], [65, 1, 25]];

export const verseText = (tag, b, c, v) => `[${tag}] ${b}권 ${c}장 ${v}절 견본 본문 '따옴표'`;

/** versions = [{id, name, language, tag}] */
export function makeBibleDb(file, versions) {
  const db = new DatabaseSync(file);
  db.exec(SCHEMA);
  const iv = db.prepare('INSERT OR REPLACE INTO BibleVersions (id, name, language, copyright, verse_count) VALUES (?, ?, ?, ?, ?)');
  const ix = db.prepare('INSERT OR REPLACE INTO BibleVerses (version_id, book_id, chapter, verse, text, search_text, search_compact) VALUES (?, ?, ?, ?, ?, ?, ?)');
  const ih = db.prepare('INSERT INTO BibleHeadings (version_id, book_id, chapter, verse, text) VALUES (?, ?, ?, ?, ?)');
  db.exec('BEGIN');
  for (const v of versions) {
    let n = 0;
    for (const [b, c, count] of SHAPE) {
      for (let x = 1; x <= count; x++) { const t = verseText(v.tag, b, c, x); ix.run(v.id, b, c, x, t, t, t); n++; }
    }
    ih.run(v.id, 43, 3, 1, `[${v.tag}] 니고데모 (견본 소제목)`);
    ih.run(v.id, 43, 3, 16, `[${v.tag}] 하나님의 사랑 (견본 소제목)`);
    iv.run(v.id, v.name, v.language, '견본 — 지어낸 본문', n);
  }
  db.exec('COMMIT');
  db.close();
}
