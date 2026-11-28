// 장절 파서 — 설치판 crates/wp-core/src/parser.rs · books.rs 를 줄마다 옮긴 것 (Electron 판 parser.js 와 같은 규칙).
// "요3:16", "요한복음 3장 16절", "행5:27-32", "롬 8", "창1:1-2:3", "롬8:1,28", "요 3 16" …
// 책 목록은 복사하지 않고 설치판의 data/books.json 을 그대로 묶는다.
// 대조 시험: test/data-parser.test.mjs (정답 test/data-golden/golden.json 은 Rust 코드를 그대로 돌려 만든 것).
// 주의: Rust regex 의 \d · \s 는 유니코드 기준이라 여기서는 \p{Nd} · \s(u) 로 맞춘다. 숫자는 i64 로 못 읽으면 '없음' (Rust parse 와 같게).
import BOOKS from '../../../crates/wp-core/data/books.json' with { type: 'json' };

export { BOOKS };

/** 책 이름 비교 열쇠: NFC · 소문자 · 공백과 '.' 빼기 (books.rs key) */
export function key(s) {
  return [...String(s).normalize('NFC').toLowerCase()].filter(c => !/^\s$/u.test(c) && c !== '.').join('');
}

const EXACT = new Map();
for (const [i, b] of BOOKS.entries()) {
  for (const n of [b.name, b.short, ...b.aliases, b.osis, b.en]) {
    const k = key(n);
    if (!EXACT.has(k)) EXACT.set(k, i);
  }
}
const BY_ID = new Map(BOOKS.map(b => [b.id, b]));
export const getBook = id => BY_ID.get(id) || null;

/** 책 이름 해석. 정확히 일치하면 그 책, 아니면 접두어 후보 (여럿이면 고르지 않고 돌려준다) */
export function resolveBook(input) {
  const k = key(input);
  if (!k) return { book: null, candidates: [] };
  if (EXACT.has(k)) return { book: BOOKS[EXACT.get(k)], candidates: [] };
  const cands = BOOKS.filter(b => [b.name, b.short, b.en, b.osis, ...b.aliases].some(n => key(n).startsWith(k)));
  if (cands.length === 1) return { book: cands[0], candidates: [] };
  return { book: null, candidates: cands };
}

/** 번역본 언어에 맞는 책 이름 (영어: 'John' · 'John ', 그 밖: '요한복음' · '요') */
export function bookNames(b, lang) {
  return String(lang || '').toLowerCase().startsWith('en') ? { name: b.en, short: `${b.en} ` } : { name: b.name, short: b.short };
}

// Rust str::trim · char::is_whitespace 와 같은 공백 (White_Space). JS 의 \s 에는 U+FEFF 가 더 들어 있어 따로 둔다.
const WS = '\\t\\n\\v\\f\\r \\u0085\\u00a0\\u1680\\u2000-\\u200a\\u2028\\u2029\\u202f\\u205f\\u3000';
const isWs = c => new RegExp(`^[${WS}]$`, 'u').test(c);
const rtrim = s => s.replace(new RegExp(`^[${WS}]+|[${WS}]+$`, 'gu'), '');

export function normalizeInput(s) {
  const t = [...String(s ?? '').normalize('NFC')].map(c => {
    if (c === '　' || c === ' ') return ' ';
    if ('~～〜–—−'.includes(c)) return '-';
    if (c === '：') return ':';
    return c;
  }).join('');
  return rtrim(t);
}

const D = '\\p{Nd}';
const S = `[${WS}]`;
const JOHN_N = new RegExp(`^(요한)${S}*([123])${S}*서`, 'u');
const SPLIT = new RegExp(`^(${D}?${S}*[^${D}${WS}:.,\\-][^${D}:,\\-]*?)${S}*(${D}[^\\n]*)?$`, 'u');
const DASH_SP = new RegExp(`${S}*-${S}*`, 'gu');
const SEPS = /[:장절.,，、;]/u;
const NUMTOK = new RegExp(`^${D}+(-${D}+)?$`, 'u');
const RANGE_TOK = new RegExp(`^${D}+-${D}+$`, 'u');
const JANG_NUM = new RegExp(`장(${D})`, 'gu');
const DOT = new RegExp(`(${D})\\.(${D})`, 'gu');
const COMMAS = /[,，、;]+/gu;
const SEG = new RegExp(`^(${D}+)(?::(${D}+))?(?:-(${D}+)(?::(${D}+))?)?$`, 'u');

function splitBookAndRest(s0) {
  const s = s0.replace(JOHN_N, (_m, a, n) => `${a}${n === '1' ? '일' : n === '2' ? '이' : '삼'}서`);
  const m = SPLIT.exec(s);
  if (!m) return null;
  return [rtrim(m[1]), m[2] ? rtrim(m[2]) : ''];
}

const splitWs = s => s.split(new RegExp(`${S}+`, 'u')).filter(Boolean);

function spacesToColon(rest) {
  const r = rtrim(rest).replace(DASH_SP, '-');
  if (SEPS.test(r)) return rest;
  const t = splitWs(r);
  if (t.length < 2 || !t.every(x => NUMTOK.test(x))) return rest;
  const dash = x => x.includes('-');
  if (t.length === 2) return `${t[0]}:${t[1]}`;
  if (t.length === 3 && !dash(t[0]) && RANGE_TOK.test(t[1]) && !dash(t[2])) {
    const i = t[1].indexOf('-');
    return `${t[0]}:${t[1].slice(0, i)}-${t[1].slice(i + 1)}:${t[2]}`;
  }
  if (t.length === 3 && !t.some(dash)) return `${t[0]}:${t[1]}-${t[2]}`;
  if (t.length === 4 && !t.some(dash)) return `${t[0]}:${t[1]}-${t[2]}:${t[3]}`;
  return rest;
}

function normalizeRest(rest) {
  let r = [...spacesToColon(rest)].filter(c => !isWs(c)).join('');
  r = r.replace(JANG_NUM, ':$1').replaceAll('장', '').replaceAll('절', '');
  r = r.replace(DOT, '$1:$2');
  r = r.replace(COMMAS, ',');
  return r.replace(/^,+|,+$/g, '');
}

const I64_MAX = 9223372036854775807n;
/** Rust `str::parse::<i64>()` 처럼: ASCII 숫자만, 넘치면 없음 */
function num(s) {
  if (s === undefined || !/^[0-9]+$/.test(s)) return null;
  return BigInt(s) > I64_MAX ? null : Number(s);
}

const err = error => ({ ok: false, error, book: null, candidates: [], incomplete: false, ambiguous: false, rest: '' });

/**
 * 장절 문자열 → { ok:true, book, ranges:[{sc, sv, ec, ev}] } (sv · ev 는 null 일 수 있음)
 *            | { ok:false, error, book, candidates, incomplete, ambiguous, rest }
 */
export function parseReference(input) {
  const s = normalizeInput(input);
  if (!s) return err('검색어를 입력하세요.');
  const parts = splitBookAndRest(s);
  if (!parts || !parts[0]) return err('성경 책 이름을 인식할 수 없습니다.');
  const [bookPart, restRaw] = parts;
  const { book, candidates } = resolveBook(bookPart);
  if (!book) {
    if (candidates.length > 1) {
      return { ok: false, error: `'${bookPart}'에 해당하는 책이 여러 권입니다. 후보에서 선택하세요.`, book: null, candidates, incomplete: false, ambiguous: true, rest: restRaw };
    }
    return err(`'${bookPart}'은(는) 알 수 없는 성경 책 이름입니다.`);
  }
  const incomplete = error => ({ ok: false, error, book, candidates: [], incomplete: true, ambiguous: false, rest: '' });
  const withBook = error => ({ ok: false, error, book, candidates: [], incomplete: false, ambiguous: false, rest: '' });
  if (!restRaw) return incomplete(`${book.name}: 장을 입력하세요. (예: ${book.short}1:1)`);
  const rest = normalizeRest(restRaw);
  if (!rest) return incomplete(`${book.name}: 장을 입력하세요.`);
  const ranges = [];
  let ctx = null;
  for (const seg of rest.split(',')) {
    const m = SEG.exec(seg);
    if (!m) return withBook(`'${restRaw}' 장절 형식을 인식할 수 없습니다. (예: 요3:16, 행5:27-32)`);
    const a = num(m[1]) ?? 0;
    const b = num(m[2]), c = num(m[3]), d = num(m[4]);
    let r;
    if (b === null && d === null) {
      if (ctx !== null) r = { sc: ctx, sv: a, ec: ctx, ev: c ?? a };
      else if (book.singleChapter) { ctx = 1; r = { sc: 1, sv: a, ec: 1, ev: c ?? a }; }
      else r = { sc: a, sv: null, ec: c ?? a, ev: null };
    } else if (d === null) {
      ctx = a;
      r = { sc: a, sv: b, ec: a, ev: c ?? b };
    } else if (b !== null) {
      const cc = c ?? a;
      ctx = cc;
      r = { sc: a, sv: b, ec: cc, ev: d };
    } else {
      const cc = c ?? a;
      ctx = cc;
      r = { sc: a, sv: null, ec: cc, ev: d };
    }
    if (r.sc < 1 || r.ec < 1 || (r.sv !== null && r.sv < 1) || (r.ev !== null && r.ev < 1)) return withBook('장과 절 번호는 1 이상이어야 합니다.');
    if (r.ec < r.sc || (r.ec === r.sc && r.sv !== null && r.ev !== null && r.ev < r.sv)) return withBook('구절 범위의 끝이 시작보다 앞에 있습니다.');
    ranges.push(r);
  }
  return { ok: true, book, ranges };
}

/** 입력이 장절 검색처럼 보이는지 (자동 모드 판별용) */
export function looksLikeReference(input) {
  const s = normalizeInput(input);
  if (!/[0-9]/.test(s)) return false;
  const p = parseReference(s);
  return p.ok || p.ambiguous || !!p.book;
}

/** "요한복음 3:16-18, 20" 같은 제목 (db.rs format_title) — ranges 는 [sc, sv, ec, ev] */
export function formatTitle(book, ranges) {
  const parts = ranges.map(([sc, sv, ec, ev], i) => {
    const samePrev = i > 0 && ranges[i - 1][2] === sc && sc === ec;
    if (sc === ec) {
      const s = sv === ev ? `${sc}:${sv}` : `${sc}:${sv}-${ev}`;
      return samePrev ? s.slice(String(sc).length + 1) : s;
    }
    return `${sc}:${sv}-${ec}:${ev}`;
  });
  return `${book} ${parts.join(', ')}`;
}
