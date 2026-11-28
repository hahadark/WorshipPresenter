// 워십프리젠터 웹판 계약 v1 — 길 · 역할 · 실시간 메시지 (docs/web/architecture.md 와 같이 고친다)
// 이 파일은 Worker · 중계실(Durable Object) · 시험이 함께 쓴다. Cloudflare 전용 API 를 쓰지 않는다.
import { dataRoute } from './data/routes.mjs'; // 2단계 A 길 (docs/web/data-api.md)
import { aiRoute } from './ai/routes.mjs'; // 5단계 AI 길 (docs/web/ai.md)
import { mediaRoute } from './media/routes.mjs'; // 4단계 B 미디어 길 (docs/web/media.md)

export const PROTOCOL = 1;

/** 역할: owner 주인 · bridge 설치판 연결 · output 출력 화면(OBS 포함) · remote 리모컨 · editor 웹 편집기 */
export const ROLES = ['owner', 'bridge', 'output', 'remote', 'editor', 'member-editor', 'member-viewer']; // 뒤의 둘: 2단계 사람 계정 (identity/)

/** 할 수 있는 일: view 출력 보기 · remote 리모컨 조작 · editor 큐시트 고치기 · manage 기기 열쇠 관리 · source 송출 주체로 붙기 */
// 2단계: read 교회 자료 읽기 · write 고치기 (/api/v1/documents · sync · bible)
const CAPS = {
  owner: ['view', 'remote', 'editor', 'manage', 'source', 'read', 'write', 'ai'], // ai: 5단계 AI 부르기 (docs/web/ai.md)
  bridge: ['source', 'read', 'write'],
  'member-editor': ['view', 'remote', 'editor', 'read', 'write', 'ai'],
  'member-viewer': ['view', 'read'],
  output: ['view'],
  remote: ['remote'],
  editor: ['editor'],
};
export function can(role, cap) {
  return !!CAPS[role] && CAPS[role].includes(cap);
}

/** 로그인 유지 시간(초). 출력 화면은 OBS 에 넣어 두므로 길게, 사람 손에 있는 기기는 짧게. 열쇠를 지우면 그 즉시 끊긴다. */
/** 역할마다 첫 화면 ('/' · 로그인 답의 home). 사람 계정(2단계)도 같은 표를 쓴다 */
export const HOME = { owner: '/owner', remote: '/remote', editor: '/editor', output: '/outputs/main', 'member-editor': '/editor', 'member-viewer': '/outputs/main' };
export const homeOf = role => HOME[role] || '/outputs/main';

export const SESSION_TTL = { owner: 30 * 86400, output: 400 * 86400, remote: 12 * 3600, editor: 12 * 3600, 'member-editor': 30 * 86400, 'member-viewer': 30 * 86400 };

/** 교회 ID: 영문 소문자 · 숫자 · '-' (2~40자). 주소 · 쿠키 · 열쇠 앞머리에 그대로 쓴다. */
const CHURCH_RE = /^[a-z0-9][a-z0-9-]{0,38}[a-z0-9]$/;
export function validChurch(id) {
  return typeof id === 'string' && CHURCH_RE.test(id);
}

/** 실시간 연결이 고르는 화면 이름. 설치판 /events?client= 와 같고, 리모컨 상태를 받는 remote 만 덧붙였다. */
const CLIENT_RE = /^(?:main|overlay|stage|remote|out-[A-Za-z0-9_-]{1,36})$/;
export function clientName(s) {
  return typeof s === 'string' && CLIENT_RE.test(s) ? s : null;
}

/** 출력 페이지 주소 (/outputs/main · overlay · stage · out-…) → 화면 이름. 파일(/outputs/output.js)은 null */
export function outputPage(path) {
  const m = /^\/outputs\/([A-Za-z0-9_-]+)\/?$/.exec(path);
  if (!m) return null;
  return m[1] === 'main' || m[1] === 'overlay' || m[1] === 'stage' || /^out-[A-Za-z0-9_-]{1,36}$/.test(m[1]) ? m[1] : null;
}

/** 설치판이 중계해 주는 미디어 길 (설치판 내부망 출력 lan.rs allowed 와 같은 범위) */
export function mediaPath(path) {
  return path.startsWith('/media/') || path === '/lib/file' || path.startsWith('/hymn/') || path.startsWith('/hymnal/score/');
}

/**
 * 길 → { kind, cap }. 모르는 길은 null (404).
 * kind: page(HTML) · events(실시간, 보는 쪽) · bridge(실시간, 송출 주체) · relay(설치판으로 넘김) · media(설치판 파일) · room(중계실이 답함) · api(Worker 가 답함)
 */
export function route(method, path, query) {
  const get = method === 'GET' || method === 'HEAD';
  const post = method === 'POST';
  if (get && outputPage(path)) return { kind: 'page', cap: 'view', page: outputPage(path) === 'stage' ? 'stage' : 'output' };
  if (get && (path === '/remote' || path === '/remote/')) return { kind: 'page', cap: 'remote', page: 'remote' };
  if (get && (path === '/editor' || path === '/editor/')) return { kind: 'page', cap: 'editor', page: 'editor' };
  if (get && (path === '/owner' || path === '/owner/')) return { kind: 'page', cap: 'manage', page: 'owner' };
  if (get && path === '/events') {
    const c = clientName(query.get('client') || 'main');
    return c ? { kind: 'events', cap: c === 'remote' ? 'remote' : 'view', client: c } : null;
  }
  if (get && path === '/bridge') return { kind: 'bridge', cap: 'source' };
  if ((get && path === '/r/state') || (post && path === '/r/do')) return { kind: path === '/r/state' ? 'room' : 'relay', cap: 'remote' };
  if (get && ['/e/state', '/e/cue', '/e/songs'].includes(path)) return { kind: 'relay', cap: 'editor' };
  if (post && ['/e/save', '/e/new', '/e/song'].includes(path)) return { kind: 'relay', cap: 'editor' };
  if (get && mediaPath(path)) return { kind: 'media', cap: 'view' };
  if (post && (path === '/api/ack' || path === '/api/command')) return { kind: 'room', cap: 'view' };
  if (get && path === '/api/v1/me') return { kind: 'api', cap: null };
  if (get && path === '/api/v1/room') return { kind: 'room', cap: 'manage' };
  if (path === '/api/v1/credentials' && (get || post)) return { kind: 'api', cap: 'manage' };
  if (method === 'DELETE' && /^\/api\/v1\/credentials\/[A-Za-z0-9]{6,24}$/.test(path)) return { kind: 'api', cap: 'manage' };
  return dataRoute(method, path) || aiRoute(method, path) || mediaRoute(method, path);
}

/** 출력 화면이 /api/command 로 알릴 수 있는 것 (설치판 내부망 출력과 같은 경계). 그 밖의 명령은 클라우드에서 받지 않는다. */
export const VIEW_COMMANDS = ['videoMeta', 'videoEnded'];

// ---------- 실시간 메시지 (글 프레임은 JSON, 미디어 본문만 이진 프레임) ----------
// 송출 주체(설치판 · 웹 조작 화면) → 중계실
export const FROM_SOURCE = ['hello', 'render', 'draw', 'remote', 'res'];
// 보는 쪽(출력 · 리모컨) → 중계실
export const FROM_VIEWER = ['ack', 'cmd', 'rtc']; // rtc: 4단계 B 휴대폰 카메라 신호 (media/signal.mjs)

export const LIMITS = {
  /** 글 프레임 하나 (출력 자료 한 장). Cloudflare 가 받는 한도는 32MiB — 그보다 훨씬 작게 잡는다 */
  message: 4 * 1024 * 1024,
  /** 리모컨 · 편집기가 보내는 본문 */
  body: 2 * 1024 * 1024,
  /** 한 교회 방에 붙는 화면 수 */
  viewers: 200,
  /** 설치판에 넘겨 놓고 답을 기다리는 요청 수 */
  pending: 64,
  /** 이진 프레임 하나 (설치판이 이 크기로 잘라 보낸다) */
  chunk: 256 * 1024,
  /** 방 메모리에 잠깐 두는 작은 파일 하나 · 전체 */
  cacheItem: 2 * 1024 * 1024,
  cacheTotal: 32 * 1024 * 1024,
};

/** 닫는 코드 */
export const CLOSE = { replaced: 4001, revoked: 4003, protocol: 4008 };

/** 이진 프레임 머리 5바이트: 요청 번호(u32 BE) + 표시(1 = 끝, 2 = 중단) */
export function packChunk(id, flags, bytes) {
  const out = new Uint8Array(5 + bytes.length);
  new DataView(out.buffer).setUint32(0, id);
  out[4] = flags;
  out.set(bytes, 5);
  return out;
}
export function unpackChunk(buf) {
  const u = buf instanceof Uint8Array ? buf : new Uint8Array(buf);
  if (u.length < 5) return null;
  return { id: new DataView(u.buffer, u.byteOffset, 5).getUint32(0), flags: u[4], bytes: u.subarray(5) };
}

export function json(body, status = 200, headers = {}) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json; charset=utf-8', 'cache-control': 'no-store', 'x-content-type-options': 'nosniff', ...headers },
  });
}
export const fail = (status, error, extra = {}) => json({ ok: false, error, ...extra }, status);
