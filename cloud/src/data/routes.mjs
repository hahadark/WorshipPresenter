// 2단계 A 길 · 한도 (Cloudflare 전용 API 를 쓰지 않는다 — protocol.mjs route() 와 시험이 함께 쓴다). 계약: docs/web/data-api.md

/** 문서 종류 (architecture.md 6절) */
export const KINDS = ['cue', 'song', 'template', 'theme', 'overlay', 'macro', 'scene', 'source', 'settings', 'usage'];

/** 문서 id: 설치판 short_id(base36) · Electron 판 uuid · 'tpl-…' 같은 모양을 다 받는다. 경로 · 쿠키에 그대로 넣을 수 있는 글자만 */
const ID_RE = /^[A-Za-z0-9_][A-Za-z0-9_.:~@-]{0,127}$/;
export const validDocId = s => typeof s === 'string' && ID_RE.test(s);
export const validKind = s => KINDS.includes(s);

/** 올린 쪽이 정하는 mutationId (uuid · 무작위 글) */
const MUT_RE = /^[A-Za-z0-9_.:-]{8,80}$/;
export const validMutationId = s => typeof s === 'string' && MUT_RE.test(s);

export const DATA_LIMITS = {
  /** 문서 하나 (D1 줄 하나는 2,000,000 바이트까지 — 넉넉히 아래로) */
  document: 1024 * 1024,
  /** 목록 한 쪽 (줄 수) */
  listPage: 200,
  listDefault: 100,
  /** 동기화 받기 한 쪽 (줄 수 · 본문 바이트) — 한 쪽이 D1 행 읽기 몇백 개를 넘지 않게 */
  syncPage: 500,
  syncDefault: 200,
  syncBytes: 4 * 1024 * 1024,
  /** 동기화 올리기 한 번에 (D1 은 요청 하나에 질의 50개 — 고친 것 하나에 질의 2개 + 앞뒤 확인) */
  mutations: 20,
  /** 한 번에 부를 수 있는 장절 묶음 (롬8:1,28,… ) */
  ranges: 20,
};

/**
 * 2단계 길 → { kind:'data'|'identity', cap, op }. 모르는 길은 null.
 * cap: read 자료 읽기 · write 자료 고치기 · manage 사람 계정 관리 · null(로그인만)
 */
export function dataRoute(method, path) {
  const get = method === 'GET' || method === 'HEAD';
  if (path === '/api/v1/documents' && get) return { kind: 'data', cap: 'read', op: 'list' };
  if (/^\/api\/v1\/documents\/[^/]+\/[^/]+$/.test(path)) {
    if (get) return { kind: 'data', cap: 'read', op: 'get' };
    if (method === 'PUT') return { kind: 'data', cap: 'write', op: 'put' };
    if (method === 'DELETE') return { kind: 'data', cap: 'write', op: 'delete' };
    return null;
  }
  if (path === '/api/v1/sync') {
    if (get) return { kind: 'data', cap: 'read', op: 'pull' };
    if (method === 'POST') return { kind: 'data', cap: 'write', op: 'push' };
    return null;
  }
  if (path === '/api/v1/bible/versions' && get) return { kind: 'data', cap: 'read', op: 'versions' };
  if (path === '/api/v1/bible/passage' && get) return { kind: 'data', cap: 'read', op: 'passage' };
  // 사람 계정 (identity/)
  if (path === '/api/v1/members' && get) return { kind: 'identity', cap: 'manage', op: 'list' };
  if (path === '/api/v1/members' && method === 'POST') return { kind: 'identity', cap: 'manage', op: 'create' };
  if (/^\/api\/v1\/members\/[A-Za-z0-9]{6,24}$/.test(path)) {
    if (method === 'PATCH') return { kind: 'identity', cap: 'manage', op: 'update' };
    if (method === 'DELETE') return { kind: 'identity', cap: 'manage', op: 'remove' };
    return null;
  }
  if (path === '/api/v1/me/password' && method === 'POST') return { kind: 'identity', cap: null, op: 'password' };
  return null;
}

/** If-Match: 3 · "3" · W/"3" → 3. 없으면 0 (새로 만들기만), 못 읽으면 null */
export function parseIfMatch(v) {
  if (v === null || v === undefined || v === '') return 0;
  const m = /^\s*(?:W\/)?"?(\d{1,15})"?\s*$/.exec(String(v));
  return m ? Number(m[1]) : null;
}

/** 목록 · 동기화 쪽 표시 (불투명한 글) */
export function encodeCursor(parts) {
  return btoa(unescape(encodeURIComponent(JSON.stringify(parts)))).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}
export function decodeCursor(s) {
  if (!s) return null;
  try {
    const v = JSON.parse(decodeURIComponent(escape(atob(s.replace(/-/g, '+').replace(/_/g, '/')))));
    return Array.isArray(v) && v.every(x => typeof x === 'string') ? v : undefined;
  } catch {
    return undefined;
  }
}

/** 목록에 보일 이름 (큐시트 name · 찬양 title …) */
export function titleOf(payload) {
  if (!payload || typeof payload !== 'object') return '';
  const t = payload.name ?? payload.title ?? payload.label ?? '';
  return typeof t === 'string' ? t.slice(0, 200) : '';
}
