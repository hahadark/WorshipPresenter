//! 성경·큐시트 DB (Electron 판 db.js 와 같은 bible.db 형식을 그대로 읽고 쓴다)

use crate::books::{book_names, get_book, BOOKS};
use crate::parser::{parse_reference, Parsed, Range};
use crate::textnorm::compact;
use anyhow::Result;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::Path;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS BibleVersions (id TEXT PRIMARY KEY, name TEXT NOT NULL, language TEXT, copyright TEXT, license TEXT, source TEXT, imported_at TEXT, verse_count INTEGER DEFAULT 0);
CREATE TABLE IF NOT EXISTS BibleBooks (id INTEGER PRIMARY KEY, name TEXT NOT NULL, short_name TEXT NOT NULL, testament TEXT NOT NULL, book_order INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS BibleVerses (rowid INTEGER PRIMARY KEY, version_id TEXT NOT NULL, book_id INTEGER NOT NULL, chapter INTEGER NOT NULL, verse INTEGER NOT NULL, text TEXT NOT NULL, search_text TEXT NOT NULL, search_compact TEXT NOT NULL, UNIQUE (version_id, book_id, chapter, verse));
CREATE TABLE IF NOT EXISTS BibleHeadings (version_id TEXT NOT NULL, book_id INTEGER NOT NULL, chapter INTEGER NOT NULL, verse INTEGER NOT NULL, text TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS idx_headings ON BibleHeadings(version_id, book_id, chapter, verse);
CREATE TABLE IF NOT EXISTS BibleNotes (version_id TEXT NOT NULL, book_id INTEGER NOT NULL, chapter INTEGER NOT NULL, verse INTEGER NOT NULL, text TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS idx_notes ON BibleNotes(version_id, book_id, chapter, verse);
CREATE TABLE IF NOT EXISTS Commentaries (id TEXT PRIMARY KEY, name TEXT NOT NULL, source TEXT, license TEXT, imported_at TEXT, entry_count INTEGER DEFAULT 0);
CREATE TABLE IF NOT EXISTS CommentaryEntries (id INTEGER PRIMARY KEY, commentary_id TEXT NOT NULL, book_id INTEGER NOT NULL, s INTEGER NOT NULL, e INTEGER NOT NULL, kind TEXT NOT NULL, title TEXT, text TEXT NOT NULL, seq INTEGER NOT NULL);
CREATE INDEX IF NOT EXISTS idx_cm ON CommentaryEntries(commentary_id, book_id, s, e);
CREATE TABLE IF NOT EXISTS Cuesheets (id TEXT PRIMARY KEY, name TEXT NOT NULL, service_date TEXT, items TEXT NOT NULL, updated_at TEXT);
CREATE VIRTUAL TABLE IF NOT EXISTS BibleVersesFts USING fts5(search_compact, content='BibleVerses', content_rowid='rowid', tokenize='trigram');
"#;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Version {
    pub id: String,
    pub name: String,
    pub language: String,
    pub copyright: String,
    pub verse_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Verse {
    pub book: i64,
    pub chapter: i64,
    pub verse: i64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CueSummary {
    pub id: String,
    pub name: String,
    pub service_date: String,
    pub count: usize,
    pub pinned: bool,
    pub template: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Cue {
    pub id: String,
    pub name: String,
    pub service_date: String,
    pub start_time: String,
    pub items: Vec<Value>,
    pub memo: String,
    pub pinned: bool,
    pub template: bool,
}

pub struct BibleStore {
    pub db: Connection,
    /// versionId → bookId → 장마다 절 수
    structure: HashMap<String, HashMap<i64, Vec<i64>>>,
    langs: HashMap<String, String>,
}

impl BibleStore {
    pub fn open(file: &Path) -> Result<Self> {
        let db = Connection::open(file)?;
        db.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = FULL; PRAGMA foreign_keys = ON;")?;
        db.execute_batch(SCHEMA)?;
        crate::firstrun::mark("DB 열기·표");
        let cols: Vec<String> = {
            let mut st = db.prepare("PRAGMA table_info(Cuesheets)")?;
            st.query_map([], |r| r.get::<_, String>(1))?.filter_map(|x| x.ok()).collect()
        };
        for (c, ddl) in [
            ("start_time", "ALTER TABLE Cuesheets ADD COLUMN start_time TEXT NOT NULL DEFAULT ''"),
            ("memo", "ALTER TABLE Cuesheets ADD COLUMN memo TEXT NOT NULL DEFAULT ''"),
            ("pinned", "ALTER TABLE Cuesheets ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0"),
            ("template", "ALTER TABLE Cuesheets ADD COLUMN template INTEGER NOT NULL DEFAULT 0"),
            ("deleted_at", "ALTER TABLE Cuesheets ADD COLUMN deleted_at TEXT NOT NULL DEFAULT ''"),
            ("created_at", "ALTER TABLE Cuesheets ADD COLUMN created_at TEXT NOT NULL DEFAULT ''"),
        ] {
            if !cols.iter().any(|x| x == c) {
                db.execute_batch(ddl)?;
            }
        }
        {
            // 한 묶음으로 쓴다: 한 줄씩 쓰면 줄마다 디스크에 밀어 넣어(66번) 켤 때마다 0.15~0.6초가 걸렸다 (느린 디스크면 더)
            db.execute_batch("BEGIN")?;
            let r = (|| -> Result<()> {
                let mut ins = db.prepare("INSERT OR REPLACE INTO BibleBooks (id, name, short_name, testament, book_order) VALUES (?, ?, ?, ?, ?)")?;
                for b in BOOKS.iter() {
                    ins.execute(params![b.id, b.name, b.short, b.testament, b.id])?;
                }
                Ok(())
            })();
            db.execute_batch(if r.is_ok() { "COMMIT" } else { "ROLLBACK" })?;
            r?;
        }
        crate::firstrun::mark("책 목록");
        let mut s = Self { db, structure: HashMap::new(), langs: HashMap::new() };
        s.reload_structure()?;
        crate::firstrun::mark("성경 구조");
        Ok(s)
    }

    pub fn reload_structure(&mut self) -> Result<()> {
        self.structure.clear();
        self.langs.clear();
        let mut st = self.db.prepare("SELECT version_id, book_id, chapter, MAX(verse) FROM BibleVerses GROUP BY version_id, book_id, chapter")?;
        let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?)))?;
        for row in rows {
            let (v, b, c, m) = row?;
            let ch = self.structure.entry(v).or_default().entry(b).or_default();
            let idx = (c - 1).max(0) as usize;
            if ch.len() <= idx {
                ch.resize(idx + 1, 0);
            }
            ch[idx] = m;
        }
        Ok(())
    }

    pub fn list_versions(&self) -> Vec<Version> {
        let mut st = match self.db.prepare("SELECT id, name, COALESCE(language,''), COALESCE(copyright,''), COALESCE(verse_count,0) FROM BibleVersions ORDER BY name") {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        st.query_map([], |r| Ok(Version { id: r.get(0)?, name: r.get(1)?, language: r.get(2)?, copyright: r.get(3)?, verse_count: r.get(4)? }))
            .map(|it| it.filter_map(|x| x.ok()).collect())
            .unwrap_or_default()
    }

    pub fn has_version(&self, id: &str) -> bool {
        self.structure.contains_key(id)
    }

    fn version_name(&self, id: &str) -> String {
        self.db
            .query_row("SELECT name FROM BibleVersions WHERE id = ?", [id], |r| r.get::<_, String>(0))
            .optional()
            .ok()
            .flatten()
            .unwrap_or_else(|| id.to_string())
    }

    pub fn lang_of(&mut self, id: &str) -> String {
        if let Some(l) = self.langs.get(id) {
            return l.clone();
        }
        let l = self
            .db
            .query_row("SELECT COALESCE(language,'ko') FROM BibleVersions WHERE id = ?", [id], |r| r.get::<_, String>(0))
            .optional()
            .ok()
            .flatten()
            .unwrap_or_else(|| "ko".into());
        self.langs.insert(id.to_string(), l.clone());
        l
    }

    /// 이 번역본에서 보일 책 이름 {id, name, short}
    pub fn book_of(&mut self, version: &str, book_id: i64) -> Value {
        let b = get_book(book_id).expect("book");
        let (name, short) = book_names(b, &self.lang_of(version));
        json!({ "id": b.id, "name": name, "short": short })
    }

    pub fn chapter_count(&self, v: &str, b: i64) -> i64 {
        self.structure.get(v).and_then(|m| m.get(&b)).map(|c| c.len() as i64).unwrap_or(0)
    }
    pub fn verse_count(&self, v: &str, b: i64, c: i64) -> i64 {
        self.structure.get(v).and_then(|m| m.get(&b)).and_then(|x| x.get((c - 1).max(0) as usize)).copied().unwrap_or(0)
    }

    /// 번역본의 책·장 구조 [{id,name,short,testament,chapters:[절 수…]}]
    pub fn get_structure(&mut self, v: &str) -> Vec<Value> {
        let Some(vm) = self.structure.get(v).cloned() else { return vec![] };
        BOOKS
            .iter()
            .filter(|b| vm.contains_key(&b.id))
            .map(|b| {
                let mut o = self.book_of(v, b.id);
                o["testament"] = json!(b.testament);
                o["chapters"] = json!(vm[&b.id]);
                o
            })
            .collect()
    }

    fn get_verses(&self, v: &str, b: i64, sc: i64, sv: i64, ec: i64, ev: i64) -> Result<Vec<Verse>> {
        let mut st = self.db.prepare_cached(
            "SELECT book_id, chapter, verse, text FROM BibleVerses WHERE version_id = ? AND book_id = ?
             AND (chapter > ? OR (chapter = ? AND verse >= ?)) AND (chapter < ? OR (chapter = ? AND verse <= ?)) ORDER BY chapter, verse",
        )?;
        let rows = st.query_map(params![v, b, sc, sc, sv, ec, ec, ev], |r| Ok(Verse { book: r.get(0)?, chapter: r.get(1)?, verse: r.get(2)?, text: r.get(3)? }))?;
        Ok(rows.filter_map(|x| x.ok()).collect())
    }

    fn get_headings(&self, v: &str, b: i64, sc: i64, ec: i64) -> Result<Vec<Value>> {
        let mut st = self.db.prepare_cached("SELECT chapter, verse, text FROM BibleHeadings WHERE version_id = ? AND book_id = ? AND chapter BETWEEN ? AND ? ORDER BY chapter, verse")?;
        let rows = st.query_map(params![v, b, sc, ec], |r| Ok(json!({ "chapter": r.get::<_, i64>(0)?, "verse": r.get::<_, i64>(1)?, "text": r.get::<_, String>(2)? })))?;
        Ok(rows.filter_map(|x| x.ok()).collect())
    }

    /// 장절 문자열 → {ok, passage} | {ok:false, error, …}
    pub fn resolve_reference(&mut self, v: &str, input: &str) -> Value {
        if !self.has_version(v) {
            return json!({ "ok": false, "error": "선택한 번역본 데이터가 없습니다." });
        }
        match parse_reference(input) {
            Parsed::Err { error, book, candidates, incomplete, ambiguous, rest } => {
                let book = book.map(|b| {
                    let mut o = self.book_of(v, b.id);
                    o["chapters"] = json!(self.chapter_count(v, b.id));
                    o
                });
                let cands: Vec<Value> = candidates.iter().map(|b| self.book_of(v, b.id)).collect();
                json!({ "ok": false, "error": error, "incomplete": incomplete, "ambiguous": ambiguous, "rest": rest, "book": book, "candidates": cands })
            }
            Parsed::Ok { book, ranges } => {
                let mut r = self.resolve_ranges(v, book.id, &ranges);
                if r["ok"] == json!(false) {
                    let mut o = self.book_of(v, book.id);
                    o["chapters"] = json!(self.chapter_count(v, book.id));
                    r["book"] = o;
                }
                r
            }
        }
    }

    pub fn resolve_ranges(&mut self, v: &str, book_id: i64, ranges: &[Range]) -> Value {
        let book = self.book_of(v, book_id);
        let bname = book["name"].as_str().unwrap_or("").to_string();
        let n_ch = self.chapter_count(v, book_id);
        if n_ch == 0 {
            return json!({ "ok": false, "error": format!("이 번역본에는 {} 데이터가 없습니다.", bname) });
        }
        let mut verses: Vec<Verse> = vec![];
        let mut seen = HashSet::new();
        let mut norm: Vec<(i64, i64, i64, i64)> = vec![];
        for r in ranges {
            if r.sc > n_ch || r.ec > n_ch {
                return json!({ "ok": false, "error": format!("{}은(는) {}장까지 있습니다.", bname, n_ch) });
            }
            let sv = r.sv.unwrap_or(1);
            let ev = r.ev.unwrap_or_else(|| self.verse_count(v, book_id, r.ec));
            let max_s = self.verse_count(v, book_id, r.sc);
            let max_e = self.verse_count(v, book_id, r.ec);
            if sv > max_s {
                return json!({ "ok": false, "error": format!("{} {}장은 {}절까지 있습니다.", bname, r.sc, max_s) });
            }
            if ev > max_e {
                return json!({ "ok": false, "error": format!("{} {}장은 {}절까지 있습니다.", bname, r.ec, max_e) });
            }
            norm.push((r.sc, sv, r.ec, ev));
            for x in self.get_verses(v, book_id, r.sc, sv, r.ec, ev).unwrap_or_default() {
                if seen.insert(x.chapter * 1000 + x.verse) {
                    verses.push(x);
                }
            }
        }
        if verses.is_empty() {
            return json!({ "ok": false, "error": "해당 구절의 본문이 없습니다." });
        }
        let headings = self.get_headings(v, book_id, norm[0].0, norm[norm.len() - 1].2).unwrap_or_default();
        json!({
            "ok": true,
            "passage": {
                "versionId": v, "versionName": self.version_name(v),
                "book": book,
                "ranges": norm.iter().map(|r| json!({ "sc": r.0, "sv": r.1, "ec": r.2, "ev": r.3 })).collect::<Vec<_>>(),
                "title": format_title(&bname, &norm),
                "verses": verses, "headings": headings,
            }
        })
    }

    pub fn get_chapter(&mut self, v: &str, book_id: i64, chapter: i64) -> Value {
        self.resolve_ranges(v, book_id, &[Range { sc: chapter, sv: None, ec: chapter, ev: None }])
    }

    /// 다음/이전 절 (같은 책 안에서, 장 경계는 넘어감)
    pub fn adjacent_verse(&self, v: &str, b: i64, c: i64, vs: i64, dir: i64) -> Option<(i64, i64)> {
        let n = self.chapter_count(v, b);
        if n == 0 {
            return None;
        }
        if dir > 0 {
            if vs < self.verse_count(v, b, c) {
                return Some((c, vs + 1));
            }
            if c < n {
                return Some((c + 1, 1));
            }
            return None;
        }
        if vs > 1 {
            return Some((c, vs - 1));
        }
        if c > 1 {
            return Some((c - 1, self.verse_count(v, b, c - 1)));
        }
        None
    }

    /// 본문 키워드 검색. 여러 단어 AND, "따옴표"는 구문. 띄어쓰기 차이 무시
    pub fn search_text(&self, v: &str, query: &str, limit: usize) -> Value {
        let re = regex::Regex::new(r#""([^"]+)"|(\S+)"#).unwrap();
        let mut terms: Vec<(String, String)> = vec![];
        for m in re.captures_iter(query) {
            let raw = m.get(1).or(m.get(2)).map(|x| x.as_str().trim().to_string()).unwrap_or_default();
            let c = compact(&raw);
            if !c.is_empty() {
                terms.push((raw, c));
            }
        }
        if terms.is_empty() {
            return json!({ "ok": false, "error": "검색어를 입력하세요.", "results": [], "terms": [] });
        }
        let mut wh = vec!["v.version_id = ?".to_string()];
        let mut args: Vec<String> = vec![v.to_string()];
        let fts: Vec<&(String, String)> = terms.iter().filter(|t| t.1.chars().count() >= 3).collect();
        if !fts.is_empty() {
            wh.push("v.rowid IN (SELECT rowid FROM BibleVersesFts WHERE BibleVersesFts MATCH ?)".into());
            args.push(fts.iter().map(|t| format!("\"{}\"", t.1.replace('"', "\"\""))).collect::<Vec<_>>().join(" AND "));
        }
        for t in &terms {
            if t.1.chars().count() < 3 {
                wh.push("v.search_compact LIKE ? ESCAPE '\\'".into());
                let esc = t.1.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
                args.push(format!("%{}%", esc));
            }
        }
        let limit = limit.clamp(1, 2000);
        let sql = format!(
            "SELECT v.book_id, v.chapter, v.verse, v.text FROM BibleVerses v WHERE {} ORDER BY v.book_id, v.chapter, v.verse LIMIT {}",
            wh.join(" AND "),
            limit + 1
        );
        let mut rows: Vec<Value> = match self.db.prepare(&sql) {
            Ok(mut st) => st
                .query_map(params_from_iter(args.iter()), |r| {
                    let b: i64 = r.get(0)?;
                    let c: i64 = r.get(1)?;
                    let vs: i64 = r.get(2)?;
                    let bk = get_book(b).unwrap();
                    Ok(json!({ "book": b, "chapter": c, "verse": vs, "text": r.get::<_, String>(3)?, "bookName": bk.name, "bookShort": bk.short, "ref": format!("{} {}:{}", bk.name, c, vs) }))
                })
                .map(|it| it.filter_map(|x| x.ok()).collect())
                .unwrap_or_default(),
            Err(e) => return json!({ "ok": false, "error": e.to_string(), "results": [] }),
        };
        let truncated = rows.len() > limit;
        rows.truncate(limit);
        json!({ "ok": true, "results": rows, "truncated": truncated, "terms": terms.iter().map(|t| t.0.clone()).collect::<Vec<_>>() })
    }

    // ---------- 큐시트 ----------
    pub fn list_cues(&self) -> Vec<CueSummary> {
        let mut st = match self.db.prepare("SELECT id, name, COALESCE(service_date,''), items, COALESCE(updated_at,''), pinned, template FROM Cuesheets WHERE deleted_at = '' ORDER BY pinned DESC, COALESCE(service_date, '') DESC, updated_at DESC") {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        st.query_map([], |r| {
            let items: String = r.get(3)?;
            let n = serde_json::from_str::<Vec<Value>>(&items).map(|v| v.len()).unwrap_or(0);
            Ok(CueSummary { id: r.get(0)?, name: r.get(1)?, service_date: r.get(2)?, count: n, updated_at: r.get(4)?, pinned: r.get::<_, i64>(5)? != 0, template: r.get::<_, i64>(6)? != 0 })
        })
        .map(|it| it.filter_map(|x| x.ok()).collect())
        .unwrap_or_default()
    }

    pub fn get_cue(&self, id: &str) -> Option<Cue> {
        self.db
            .query_row("SELECT id, name, COALESCE(service_date,''), COALESCE(start_time,''), items, memo, pinned, template FROM Cuesheets WHERE id = ?", [id], |r| {
                let items: String = r.get(4)?;
                Ok(Cue {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    service_date: r.get(2)?,
                    start_time: r.get(3)?,
                    items: serde_json::from_str(&items).unwrap_or_default(),
                    memo: r.get(5)?,
                    pinned: r.get::<_, i64>(6)? != 0,
                    template: r.get::<_, i64>(7)? != 0,
                })
            })
            .optional()
            .ok()
            .flatten()
    }

    pub fn save_cue(&self, c: &Cue) -> Result<()> {
        let old: Option<(String, String)> = self
            .db
            .query_row("SELECT created_at, deleted_at FROM Cuesheets WHERE id = ?", [&c.id], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?;
        let now = crate::util::iso_now();
        let (created, deleted) = old.unwrap_or((now.clone(), String::new()));
        self.db.execute(
            "INSERT OR REPLACE INTO Cuesheets (id, name, service_date, start_time, items, updated_at, memo, pinned, template, deleted_at, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![c.id, c.name, c.service_date, c.start_time, serde_json::to_string(&c.items)?, now, c.memo, c.pinned as i64, c.template as i64, deleted, if created.is_empty() { now.clone() } else { created }],
        )?;
        Ok(())
    }
}

/// "요한복음 3:16-18, 20" 같은 제목
pub fn format_title(book: &str, ranges: &[(i64, i64, i64, i64)]) -> String {
    let parts: Vec<String> = ranges
        .iter()
        .enumerate()
        .map(|(i, &(sc, sv, ec, ev))| {
            let same_prev = i > 0 && ranges[i - 1].2 == sc && sc == ec;
            if sc == ec {
                let s = if sv == ev { format!("{}:{}", sc, sv) } else { format!("{}:{}-{}", sc, sv, ev) };
                if same_prev { s[sc.to_string().len() + 1..].to_string() } else { s }
            } else {
                format!("{}:{}-{}:{}", sc, sv, ec, ev)
            }
        })
        .collect();
    format!("{} {}", book, parts.join(", "))
}
