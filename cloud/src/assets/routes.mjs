// 2단계 C 파일(R2) 길 · 종류 · 한도 · 이름 · Range (Cloudflare 전용 API 를 쓰지 않는다 — protocol.mjs route() 와 시험이 함께 쓴다).
// 계약: docs/web/assets-api.md

const MiB = 1024 * 1024;

export const ASSET_LIMITS = {
  /** 한 번에 올리기 (Workers 무료 플랜 요청 본문 100 MB 아래로) — 넘으면 나눠 올리기 */
  single: 90 * MiB,
  /** 나눠 올리기 조각 (R2: 마지막 말고는 모두 같은 크기, 5 MiB 이상) */
  part: 32 * MiB,
  partMin: 5 * MiB,
  /** R2 multipart 조각 수 한도 */
  parts: 10000,
  /** 끝내지 않은 나눠 올리기는 하루 뒤 정리 명령이 그만둔다 (조각도 저장 용량을 쓴다) */
  uploadTtlMs: 24 * 3600 * 1000,
  /** 한 교회가 동시에 열어 둘 수 있는 나눠 올리기 */
  uploadsOpen: 8,
  /** 목록 한 쪽 */
  listPage: 200,
  listDefault: 100,
  /** 지운 파일을 R2 에서 실제로 지우기 전 기본 기다림 (정리 명령) */
  purgeDays: 30,
};

/** 받는 파일 종류 → 묶음 · 확장자 · 최대 크기. 여기 없는 종류(HTML · 스크립트 …)는 받지 않는다 */
const T = (kind, max, ...exts) => ({ kind, max, exts });
export const TYPES = {
  'image/png': T('image', 50 * MiB, 'png'),
  'image/jpeg': T('image', 50 * MiB, 'jpg', 'jpeg', 'jfif'),
  'image/gif': T('image', 50 * MiB, 'gif'),
  'image/webp': T('image', 50 * MiB, 'webp'),
  'image/avif': T('image', 50 * MiB, 'avif'),
  'image/bmp': T('image', 50 * MiB, 'bmp'),
  'image/svg+xml': T('image', 10 * MiB, 'svg'),
  'video/mp4': T('video', 2048 * MiB, 'mp4', 'm4v'),
  'video/webm': T('video', 2048 * MiB, 'webm'),
  'video/quicktime': T('video', 2048 * MiB, 'mov'),
  'video/ogg': T('video', 2048 * MiB, 'ogv'),
  'audio/mpeg': T('audio', 500 * MiB, 'mp3'),
  'audio/mp4': T('audio', 500 * MiB, 'm4a'),
  'audio/aac': T('audio', 500 * MiB, 'aac'),
  'audio/wav': T('audio', 500 * MiB, 'wav'),
  'audio/ogg': T('audio', 500 * MiB, 'ogg', 'oga', 'opus'),
  'audio/flac': T('audio', 500 * MiB, 'flac'),
  'font/ttf': T('font', 50 * MiB, 'ttf'),
  'font/otf': T('font', 50 * MiB, 'otf'),
  'font/collection': T('font', 50 * MiB, 'ttc'),
  'font/woff': T('font', 50 * MiB, 'woff'),
  'font/woff2': T('font', 50 * MiB, 'woff2'),
  'application/pdf': T('pdf', 200 * MiB, 'pdf'),
  // 슬라이드 (PowerPoint, 사관님 2026-11-28 '슬라이드를 온라인에 저장'). 브라우저가 열지 않게 늘 attachment 로 내보낸다 (disposition)
  'application/vnd.openxmlformats-officedocument.presentationml.presentation': T('slides', 500 * MiB, 'pptx'),
  'application/vnd.openxmlformats-officedocument.presentationml.slideshow': T('slides', 500 * MiB, 'ppsx'),
  'application/vnd.ms-powerpoint': T('slides', 500 * MiB, 'ppt', 'pps'),
};
/** 다른 이름으로 오는 같은 종류 */
const TYPE_ALIASES = {
  'image/jpg': 'image/jpeg', 'image/x-ms-bmp': 'image/bmp', 'audio/x-wav': 'audio/wav', 'audio/wave': 'audio/wav', 'audio/mp3': 'audio/mpeg',
  'audio/x-m4a': 'audio/mp4', 'audio/x-flac': 'audio/flac', 'video/x-m4v': 'video/mp4', 'application/font-woff': 'font/woff',
  'application/x-font-woff': 'font/woff', 'application/font-woff2': 'font/woff2', 'application/x-font-ttf': 'font/ttf',
  'application/x-font-truetype': 'font/ttf', 'application/font-sfnt': 'font/ttf', 'application/x-font-otf': 'font/otf',
  'application/x-font-opentype': 'font/otf', 'application/vnd.ms-opentype': 'font/otf',
  'application/mspowerpoint': 'application/vnd.ms-powerpoint', 'application/powerpoint': 'application/vnd.ms-powerpoint', 'application/x-mspowerpoint': 'application/vnd.ms-powerpoint',
};
const BY_EXT = Object.fromEntries(Object.entries(TYPES).flatMap(([t, v]) => v.exts.map(e => [e, t])));
const GENERIC = ['', 'application/octet-stream', 'binary/octet-stream', 'application/unknown'];

export const extOf = name => { const m = /\.([A-Za-z0-9]{1,8})$/.exec(name || ''); return m ? m[1].toLowerCase() : ''; };

/**
 * Content-Type 과 파일 이름 → { type, kind, max } | { status, error }.
 * 종류가 비었거나 octet-stream 이면 확장자로 정한다. 둘 다 있는데 묶음이 다르면(그림이라면서 .mp4) 받지 않는다.
 */
export function resolveType(contentType, name) {
  const raw = String(contentType || '').split(';')[0].trim().toLowerCase();
  const fromExt = BY_EXT[extOf(name)];
  const t = GENERIC.includes(raw) ? fromExt : (TYPE_ALIASES[raw] || raw);
  if (!t || !TYPES[t]) return { status: 415, error: '받지 않는 파일 종류입니다 (그림 · 영상 · 소리 · 글꼴 · PDF · PowerPoint 만).' };
  if (fromExt && TYPES[fromExt].kind !== TYPES[t].kind) return { status: 415, error: '파일 종류와 확장자가 맞지 않습니다.' };
  return { type: t, kind: TYPES[t].kind, max: TYPES[t].max };
}

/** 보일 이름: 경로를 떼고(PC 경로 · 사용자 이름이 들어가지 않게) 글자 정리, 200자까지 */
export function displayName(raw) {
  let s = String(raw ?? '');
  s = s.split(/[\\/]/).pop() || '';
  s = s.normalize('NFC').replace(/[\u0000-\u001f\u007f]/g, '').trim();
  if (s.length > 200) { const e = extOf(s); s = s.slice(0, 200 - (e ? e.length + 1 : 0)) + (e ? `.${e}` : ''); }
  return s || 'file';
}

/** R2 열쇠에 넣는 안전한 이름 (글자 · 숫자 · . _ - 만, 100자) */
export function safeKeyName(name) {
  const s = displayName(name).replace(/[^\p{L}\p{N}._-]+/gu, '_').replace(/_+/g, '_').replace(/^[._]+/, '').slice(-100);
  return s || 'file';
}

/** churches/{교회}/assets/{sha256}/{안전한 파일 이름} (architecture.md 6절) */
export const r2Key = (church, sha256, name) => `churches/${church}/assets/${sha256}/${safeKeyName(name)}`;

export const validSha = s => typeof s === 'string' && /^[0-9a-f]{64}$/.test(s);
export const normSha = s => String(s || '').trim().toLowerCase();
const ID_RE = /^[A-Za-z0-9]{16}$/;
export const validAssetId = s => typeof s === 'string' && ID_RE.test(s);
const UP_RE = /^[A-Za-z0-9]{20}$/;
export const validUploadId = s => typeof s === 'string' && UP_RE.test(s);

/** 머리글 X-Asset-Name 은 encodeURIComponent 한 것 (머리글에 한글을 그대로 넣지 못한다). 풀지 못하면 그대로 */
export function headerText(v) {
  if (v === null || v === undefined) return '';
  try { return decodeURIComponent(String(v)); } catch { return String(v); }
}

/** 글꼴 정보: family(템플릿의 글꼴 이름과 같게) · weight(1~1000) · style. 빈 것은 null */
export function fontMeta({ family, weight, style } = {}) {
  const out = {};
  if (family !== undefined && family !== null && family !== '') {
    const f = String(family).normalize('NFC').replace(/[\u0000-\u001f\u007f"'\\;{}<>]/g, '').trim();
    if (!f || f.length > 100) return { error: '글꼴 이름이 올바르지 않습니다 (100자까지, 따옴표 · 괄호 없이).' };
    out.family = f;
  }
  if (weight !== undefined && weight !== null && weight !== '') {
    const w = Number(weight);
    if (!Number.isInteger(w) || w < 1 || w > 1000) return { error: '글꼴 굵기는 1~1000 의 정수입니다 (보통 400, 굵게 700).' };
    out.weight = w;
  }
  if (style !== undefined && style !== null && style !== '') {
    const s = String(style).toLowerCase();
    if (!['normal', 'italic', 'oblique'].includes(s)) return { error: '글꼴 모양은 normal · italic · oblique 중 하나입니다.' };
    out.style = s;
  }
  return out;
}

/**
 * Range: bytes=… → null(없음 · 여러 범위라 전체를 보냄) | { offset, length } | 'invalid'(416).
 * 한 범위만 받는다 (영상 앞뒤로 넘기기 · 끊긴 데서 받기에는 그것으로 충분).
 */
export function parseRange(header, size) {
  if (!header) return null;
  const m = /^\s*bytes\s*=\s*(\d*)\s*-\s*(\d*)\s*$/i.exec(header);
  if (!m) return /^\s*bytes\s*=.*,/.test(header) ? null : 'invalid';
  const [a, b] = [m[1], m[2]];
  if (a === '' && b === '') return 'invalid';
  if (a === '') {
    const n = Number(b);
    if (n === 0) return 'invalid';
    const len = Math.min(n, size);
    return size === 0 ? 'invalid' : { offset: size - len, length: len };
  }
  const start = Number(a);
  if (start >= size) return 'invalid';
  const end = b === '' ? size - 1 : Math.min(Number(b), size - 1);
  if (end < start) return 'invalid';
  return { offset: start, length: end - start + 1 };
}

/** 받기 머리글: 파일 내용이 바뀌지 않으므로(sha256 = 이름) 오래 기억해도 된다. 다른 교회와 나누지 않게 private */
export const CACHE_IMMUTABLE = 'private, max-age=31536000, immutable';
/** 파일 응답에 늘 붙이는 것: 그 주소를 바로 열어도 SVG 안의 스크립트가 돌지 않게 */
export const FILE_CSP = "default-src 'none'; img-src data:; style-src 'unsafe-inline'; media-src 'self'; sandbox";

/** Content-Disposition (inline, 한글 이름은 RFC 5987) */
export function disposition(name, kind) {
  const n = displayName(name);
  const ascii = n.replace(/[^\x20-\x7e]/g, '_').replace(/["\\]/g, '_');
  // 슬라이드(PowerPoint)는 내려받기만 (주소를 바로 열어도 브라우저 · 오피스가 그 자리에서 열지 않게)
  return `${kind === 'other' || kind === 'slides' ? 'attachment' : 'inline'}; filename="${ascii}"; filename*=UTF-8''${encodeURIComponent(n)}`;
}

/**
 * 2C 길 → { kind:'assets', cap, op }. 모르는 길은 null.
 * cap: read 목록 · write 올리기 · 고치기 · 지우기 · manage 정리 · null(그 교회의 로그인이면 누구나 — 출력 · OBS · 리모컨이 id 로 받기)
 */
export function assetRoute(method, path) {
  if (!path.startsWith('/api/v1/assets')) return null;
  const get = method === 'GET' || method === 'HEAD';
  const A = (cap, op) => ({ kind: 'assets', cap, op });
  if (path === '/api/v1/assets') return get ? A('read', 'list') : method === 'POST' ? A('write', 'upload') : null;
  if (path === '/api/v1/assets/usage') return get ? A('read', 'usage') : null;
  if (path === '/api/v1/assets/purge') return method === 'POST' ? A('manage', 'purge') : null;
  if (path === '/api/v1/assets/fonts.css') return get ? A(null, 'fonts') : null;
  if (path === '/api/v1/assets/uploads') return method === 'POST' ? A('write', 'start') : null;
  let m = /^\/api\/v1\/assets\/uploads\/([A-Za-z0-9]{20})$/.exec(path);
  if (m) return get ? A('write', 'status') : method === 'DELETE' ? A('write', 'abort') : null;
  m = /^\/api\/v1\/assets\/uploads\/([A-Za-z0-9]{20})\/parts\/(\d{1,5})$/.exec(path);
  if (m) return method === 'PUT' ? A('write', 'part') : null;
  m = /^\/api\/v1\/assets\/uploads\/([A-Za-z0-9]{20})\/complete$/.exec(path);
  if (m) return method === 'POST' ? A('write', 'complete') : null;
  m = /^\/api\/v1\/assets\/([A-Za-z0-9]{16})$/.exec(path);
  if (m) return get ? A(null, 'get') : method === 'PATCH' ? A('write', 'update') : method === 'DELETE' ? A('write', 'remove') : null;
  m = /^\/api\/v1\/assets\/([A-Za-z0-9]{16})\/restore$/.exec(path);
  if (m) return method === 'POST' ? A('write', 'restore') : null;
  return null;
}

/** 주소의 n 번째 조각 ('/api/v1/assets/uploads/{u}/parts/{n}' → [u, n]) */
export function pathParts(path) {
  return path.split('/').slice(4);
}

/** 이 달 (UTC, R2 무료 범위는 달마다) */
export const monthOf = (now = Date.now()) => new Date(now).toISOString().slice(0, 7);

/**
 * 무료 범위 안전장치 (R2 에는 지출 상한이 없다). 계정 전체 기준 — 이 계정의 R2 저장통은 이것 하나다.
 * u = { storedBytes(저장된 것), reservedBytes(나눠 올리는 중), addBytes, classA(이 달), addA, churchBytes, churchLimit }
 * limits = { bytes, classA, classB, ratio }
 * → { ok:true } | { ok:false, reason:'storage'|'classA'|'church' }
 */
export function quotaDecision(u, limits) {
  const r = limits.ratio;
  if (u.storedBytes + u.reservedBytes + u.addBytes > limits.bytes * r) return { ok: false, reason: 'storage' };
  if (u.classA + (u.addA || 1) > limits.classA * r) return { ok: false, reason: 'classA' };
  if (u.churchLimit && u.churchBytes + u.addBytes > u.churchLimit) return { ok: false, reason: 'church' };
  return { ok: true };
}

/** env 의 한도 (없으면 R2 무료 범위: 저장 10 GB-월 · A 100만/월 · B 1,000만/월, 80% 에서 멈춤) */
export function freeLimits(env = {}) {
  const n = (v, d) => { const x = Number(v); return Number.isFinite(x) && x > 0 ? x : d; };
  return {
    bytes: n(env.ASSETS_FREE_BYTES, 10e9),
    classA: n(env.ASSETS_FREE_CLASS_A, 1e6),
    classB: n(env.ASSETS_FREE_CLASS_B, 1e7),
    ratio: Math.min(n(env.ASSETS_GUARD_RATIO, 0.8), 1),
  };
}

/** 글꼴 CSS. faces = [{id, family, weight, style, type, createdAt}] — 같은 이름 · 굵기 · 모양이 겹치면 나중 것만 */
export function fontCss(faces) {
  const fmt = { 'font/woff2': 'woff2', 'font/woff': 'woff', 'font/ttf': 'truetype', 'font/otf': 'opentype', 'font/collection': 'collection' };
  const key = f => `${f.family.toLowerCase()}|${f.style}`;
  const newest = new Map();
  for (const f of [...faces].sort((a, b) => a.createdAt - b.createdAt)) newest.set(`${key(f)}|${f.weight}`, f);
  const groups = new Map();
  for (const f of newest.values()) { const k = key(f); if (!groups.has(k)) groups.set(k, []); groups.get(k).push(f); }
  const q = s => `"${s.replace(/["\\]/g, '')}"`;
  const out = ['/* 워십프리젠터 교회 글꼴 (자동으로 만든 것) */'];
  for (const list of groups.values()) {
    for (const f of list.sort((a, b) => a.weight - b.weight)) {
      // 한 이름에 굵기가 하나뿐이면 모든 굵기에 그 파일을 쓴다 (템플릿이 굵게(700)를 불러도 가짜 굵기를 덧입히지 않음)
      const w = list.length === 1 ? '1 1000' : String(f.weight);
      out.push(`@font-face{font-family:${q(f.family)};font-weight:${w};font-style:${f.style};font-display:block;src:url("/api/v1/assets/${f.id}") format("${fmt[f.type] || 'truetype'}");}`);
    }
  }
  return out.join('\n') + '\n';
}
