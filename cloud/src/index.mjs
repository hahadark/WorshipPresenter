// 워십프리젠터 웹판 Worker — 로그인 · 길 나누기. 실시간과 설치판 중계는 교회마다 하나인 중계실(room.mjs)이 한다.
// 정적 파일(/shared · /outputs/*.js · /cloud/* …)은 Cloudflare 가 Worker 를 거치지 않고 바로 보낸다 (공개된 GPL 코드, 요청 수에 안 들어감).
// 여기로 오는 것은 페이지(HTML) · 실시간 · API · 설치판 파일뿐이고, 모두 로그인을 본다.
import * as auth from './auth.mjs';
import { PROTOCOL, SESSION_TTL, can, fail, homeOf, json, route, validChurch } from './protocol.mjs';
import { page } from './pages.mjs';
import * as data from './data/index.mjs';
import * as identity from './identity/index.mjs';
import * as assets from './assets/index.mjs';
import * as ai from './ai/index.mjs'; // 5단계 AI (docs/web/ai.md)
import * as media from './media/index.mjs'; // 4단계 B 미디어 (docs/web/media.md)
import { isPhonePath } from './media/routes.mjs';
import * as tiers from './tiers/index.mjs'; // 7단계 일부: 등급(데모 · 무료 · 유료) · 등급별 저장소 (docs/web/tiers.md)
import { limitsOf } from './tiers/limits.mjs';
import * as demo from './tiers/demo.mjs';
import * as admin from './tiers/admin.mjs';
import * as google from './drive/oauth.mjs';

export { ChurchRoom } from './room.mjs';

const room = (env, church) => env.ROOMS.get(env.ROOMS.idFromName(church));
const roomName = s => (s.role === 'demo' ? demo.demoRoom(s) : s.church);
const ipOf = request => request.headers.get('cf-connecting-ip') || 'local';
const sameOrigin = (request, url) => { const o = request.headers.get('origin'); return !o || o === url.origin; };
/** 페이지가 자리만 채워 보내는 값 (어댑터가 넣어 둔 'cloud', 도킹의 'dock') 은 열쇠로 치지 않는다 */
const realPin = s => (typeof s === 'string' && s.trim() && !['cloud', 'dock'].includes(s.trim()) ? s.trim() : '');

/** 중계실로 넘긴다. 누구인지는 여기서 확인한 값으로 덮어쓴다 (밖에서 온 x-wp-* 는 버려진다) */
function toRoom(env, request, s) {
  const headers = new Headers(request.headers);
  for (const k of [...headers.keys()]) if (k.startsWith('x-wp-')) headers.delete(k);
  headers.set('x-wp-role', s.role);
  headers.set('x-wp-cred', s.cred);
  headers.set('x-wp-exp', String(s.exp || 0));
  if (s.limits) { headers.set('x-wp-max-outputs', String(s.limits.outputs ?? '')); headers.set('x-wp-max-remotes', String(s.limits.remotes ?? '')); }
  return room(env, roomName(s)).fetch(new Request(request, { headers }));
}

async function loginCookies(env, request, church, v) {
  const { token, exp } = await auth.issueToken(env, { church, role: v.role, cred: v.cred });
  return { exp, headers: [auth.setCookie(auth.COOKIE, token, request, SESSION_TTL[v.role]), auth.setCookie(auth.CHURCH_COOKIE, church, request, 400 * 86400, false)] };
}
function withCookies(response, list) {
  if (!list?.length || response.status === 101) return response;
  const r = new Response(response.body, response);
  for (const c of list) r.headers.append('set-cookie', c);
  return r;
}

export default {
  async fetch(request, env) {
    try {
      return await handle(request, env);
    } catch (e) {
      console.error('요청 처리 실패', e?.stack || e);
      return fail(500, '클라우드가 요청을 처리하지 못했습니다.');
    }
  },
};

async function handle(request, env) {
  const url = new URL(request.url);
  const path = url.pathname;
  const method = request.method;

  if (path === '/health') return json({ ok: true, protocol: PROTOCOL, configured: auth.configured(env) });
  if (path === '/login' && method === 'GET') return page('login', { v: PROTOCOL });
  if (path === '/auth/login' && method === 'POST') return login(request, env, url);
  if (path === '/auth/logout' && method === 'POST') {
    if (!sameOrigin(request, url)) return fail(403, '다른 곳에서 온 요청입니다.');
    return json({ ok: true }, 200, { 'set-cookie': auth.setCookie(auth.COOKIE, '', request, 0) });
  }
  if (path === '/api/v1/admin/churches' && method === 'POST') return adminCreateChurch(request, env);
  if (path.startsWith('/api/v1/admin/')) return admin.handle(request, env, url);
  if (path.startsWith('/auth/google/')) { const x = await google.publicRoute(request, env, url); if (x) return x; }
  if (path.startsWith('/auth/')) { const x = await identity.publicRoute(request, env, url); if (x) return x; }
  if (isPhonePath(path)) return media.publicRoute(request, env, url, toRoom); // 휴대폰 카메라: 로그인 대신 서명 링크

  // 설치판(2B)의 자료 동기화: 쿠키 대신 설치판 연결 열쇠 (data/index.mjs bearer)
  const dr = /^Bearer\s/i.test(request.headers.get('authorization') || '') ? route(method, path, url.searchParams) : null;
  if (dr?.kind === 'data') return data.bearer(request, env, url, dr, tiered(data.handle));
  if (dr?.kind === 'assets') return data.bearer(request, env, url, dr, tiered(assets.handle)); // 2C 파일 (같은 열쇠 확인)

  const session = await auth.sessionOf(request, env);
  // 둘러보기 (로그인 없이 — 데모 쿠키를 준다)
  if (path === '/demo' && method === 'GET') return demo.enter(request, env, url, session, (name, cfg) => page(name, { v: PROTOCOL, ...cfg, authed: true }));
  if (session?.role === 'demo') { const x = demo.demoRate(request, env); if (x) return x; }
  if (path === '/' && (method === 'GET' || method === 'HEAD')) {
    const to = !session ? '/login' : homeOf(session.role);
    return Response.redirect(url.origin + to, 302);
  }

  const r = route(method, path, url.searchParams);
  if (!r) return fail(404, '없는 주소입니다.');

  // 송출 주체: 설치판은 '교회.열쇠' 를 Authorization 으로, 웹 조작 화면은 주인 쿠키로
  if (r.kind === 'bridge') {
    const key = auth.splitKey((request.headers.get('authorization') || '').replace(/^Bearer\s+/i, ''));
    if (key) {
      const v = await auth.verify(env, key.church, key.secret, ['bridge'], ipOf(request));
      if (!v.ok) return fail(v.status, v.error);
      return toRoom(env, request, { church: key.church, role: 'bridge', cred: v.cred });
    }
    if (!session || !can(session.role, 'source')) return fail(401, '설치판 연결 열쇠가 필요합니다.');
    if (!sameOrigin(request, url)) return fail(403, '다른 곳에서 온 요청입니다.');
    return toRoom(env, request, session);
  }

  // 로그인 없이 온 요청에 열쇠가 같이 실려 있으면 (출력 주소의 ?key= · 리모컨 PIN · 편집기 X-PIN) 그 자리에서 로그인시킨다
  // 지금 로그인이 이 길의 권한이 없으면(예: 출력 쿠키가 있는 브라우저에서 리모컨 PIN) 같이 온 열쇠로 다시 로그인할 수 있게 한다
  let s = session && (!r.cap || can(session.role, r.cap)) ? session : null, fresh = null;
  if (!s) {
    const jar = auth.cookies(request);
    const hinted = url.searchParams.get('c') || jar[auth.CHURCH_COOKIE] || '';
    let church = '', secret = '', roles = [];
    if (r.kind === 'page' && r.cap === 'view' && url.searchParams.has('key')) {
      const k = auth.splitKey(url.searchParams.get('key'));
      if (k) ({ church, secret } = k), (roles = ['output']);
    } else if (path === '/r/state' && realPin(url.searchParams.get('pin'))) {
      church = hinted; secret = realPin(url.searchParams.get('pin')); roles = ['remote'];
    } else if (path.startsWith('/e/') && realPin(request.headers.get('x-pin'))) {
      church = hinted; secret = realPin(request.headers.get('x-pin')); roles = ['editor'];
    }
    if (roles.length) {
      if (!validChurch(church)) return fail(401, '어느 교회인지 알 수 없습니다. 받은 주소(…?c=교회)를 그대로 열어 주세요.');
      const v = await auth.verify(env, church, secret, roles, ipOf(request));
      if (!v.ok) return fail(v.status, v.error);
      fresh = await loginCookies(env, request, church, v);
      s = { church, role: v.role, cred: v.cred, exp: fresh.exp };
      if (r.kind === 'page') {
        // 열쇠가 주소에 남지 않게 한 번 돌려보낸다 (그 뒤로는 쿠키)
        url.searchParams.delete('key');
        return withCookies(new Response(null, { status: 302, headers: { location: url.pathname + url.search } }), fresh.headers);
      }
    }
  }

  if (!s && session) s = session; // 열쇠 없이 권한 밖의 길 → 아래에서 '권한 없음'
  if (!s) {
    if (r.kind !== 'page') return fail(401, '로그인이 필요합니다.');
    // 리모컨 · 편집기는 교회만 알면 껍데기를 보여 준다 — 페이지 안의 PIN 칸이 곧 로그인 칸이다 (자료는 PIN 이 맞아야 나온다)
    const c = url.searchParams.get('c');
    const church = validChurch(c) ? c : validChurch(auth.cookies(request)[auth.CHURCH_COOKIE]) ? auth.cookies(request)[auth.CHURCH_COOKIE] : '';
    if (church && (r.page === 'remote' || r.page === 'editor')) {
      const res = page(r.page, { v: PROTOCOL, church, role: '', authed: false, page: r.page });
      return withCookies(res, [auth.setCookie(auth.CHURCH_COOKIE, church, request, 400 * 86400, false)]);
    }
    return Response.redirect(`${url.origin}/login?next=${encodeURIComponent(path + url.search)}`, 302);
  }

  if (r.cap && !can(s.role, r.cap)) {
    if (r.kind === 'page') return page('denied', { v: PROTOCOL, church: s.church, role: s.role }, 403);
    return fail(403, '이 기기에는 그 권한이 없습니다.');
  }
  // 쿠키로 온 쓰기 요청 · 실시간 연결은 같은 곳에서 온 것만 (다른 사이트가 몰래 보내지 못하게)
  if (method !== 'GET' && method !== 'HEAD') {
    if (!sameOrigin(request, url) || (request.headers.get('x-worshipcue') !== '1' && request.headers.get('sec-fetch-site') !== 'same-origin')) return fail(403, '다른 곳에서 온 요청입니다.');
  }
  if (r.kind === 'events' && !sameOrigin(request, url)) return fail(403, '다른 곳에서 온 요청입니다.');

  // 등급: 자료 · 파일 · AI · 사람 길은 등급이 되는지 · 한도 안인지 보고 저장소를 고른다 (tiers/index.mjs prepare)
  if (['data', 'assets', 'ai', 'identity', 'media-api'].includes(r.kind)) {
    const t = await tiers.prepare(request, env, url, s, r);
    if (t instanceof Response) return withCookies(t, fresh?.headers);
    s = { ...s, tier: t };
  } else if (r.kind === 'events' || r.kind === 'bridge' || r.kind === 'page' || r.kind === 'tiers') {
    const t = await tiers.tierOf(env, s.church);
    s = { ...s, tier: t, limits: tiers.roomLimits(t.tier, env) };
  }
  let res;
  if (r.kind === 'page') res = page(r.page, { v: PROTOCOL, church: s.church, role: s.role, authed: true, page: r.page });
  else if (r.kind === 'api') res = await api(request, env, url, s);
  else if (r.kind === 'data') res = await data.handle(request, env, url, s, r);
  else if (r.kind === 'identity') res = await identity.handle(request, env, url, s, r);
  else if (r.kind === 'assets') res = await assets.handle(request, env, url, s, r);
  else if (r.kind === 'ai') res = await ai.handle(request, env, url, s, r);
  else if (r.kind === 'media-api') res = await media.handle(request, env, url, s, r); // ('media' 는 1단계 설치판 파일 중계)
  else if (r.kind === 'tiers') res = await tiers.handle(request, env, url, s, r);
  else res = await toRoom(env, request, s);
  return withCookies(res, fresh?.headers);
}

// ---------- 로그인 ----------
async function login(request, env, url) {
  if (!sameOrigin(request, url)) return fail(403, '다른 곳에서 온 요청입니다.');
  const text = await request.text();
  if (text.length > 2000) return fail(413, '너무 큽니다.');
  let a;
  try { a = JSON.parse(text); } catch { return fail(400, '잘못된 요청'); }
  const church = String(a.church || '').trim().toLowerCase();
  const v = await auth.verify(env, church, String(a.secret || '').trim(), ['owner', 'output', 'remote', 'editor'], ipOf(request));
  if (!v.ok) return fail(v.status, v.error);
  const c = await loginCookies(env, request, church, v);
  const home = homeOf(v.role);
  const res = json({ ok: true, protocol: PROTOCOL, role: v.role, home });
  for (const h of c.headers) res.headers.append('set-cookie', h);
  return res;
}

// ---------- API v1 (Worker 가 답하는 것) ----------
/** Bearer(설치판) 길도 등급 확인을 거치게 */
function tiered(next) {
  return async (request, env, url, s, r) => {
    const t = await tiers.prepare(request, env, url, s, r);
    return t instanceof Response ? t : next(request, env, url, { ...s, tier: t }, r);
  };
}

async function api(request, env, url, s) {
  const path = url.pathname;
  if (path === '/api/v1/me') {
    const c = await env.DB.prepare('SELECT name FROM churches WHERE id = ?1').bind(s.church).first();
    return json({ ok: true, protocol: PROTOCOL, church: s.church, churchName: c?.name || s.church, role: s.role, exp: s.exp });
  }
  if (path === '/api/v1/credentials' && request.method === 'GET') return json({ ok: true, credentials: await auth.listCredentials(env, s.church) });
  if (path === '/api/v1/credentials' && request.method === 'POST') {
    const a = await request.json().catch(() => ({}));
    if (!['owner', 'bridge', 'output', 'remote', 'editor'].includes(a.role)) return fail(400, '열쇠 종류가 올바르지 않습니다.');
    if ((await auth.listCredentials(env, s.church)).length >= 50) return fail(409, '열쇠가 너무 많습니다. 안 쓰는 것을 지워 주세요.');
    // 7단계 등급 한도: 기기 열쇠 전체 · 출력 열쇠 · 리모컨 PIN 수 (tiers/limits.mjs)
    { const L = limitsOf((await tiers.tierOf(env, s.church)).tier, env); const have = await auth.listCredentials(env, s.church);
      const over = (n, lim, what) => lim !== null && lim !== undefined && n >= lim ? fail(429, `이 등급은 ${what}을(를) ${lim}개까지 만듭니다.`, { code: 'tier_limit', limit: what === '기기 열쇠' ? 'credentials' : a.role === 'output' ? 'outputs' : 'remotes' }) : null;
      const x = over(have.length, L.credentials, '기기 열쇠') || (a.role === 'output' ? over(have.filter(c => c.role === 'output').length, L.outputs, '출력 열쇠') : null) || (a.role === 'remote' ? over(have.filter(c => c.role === 'remote').length, L.remotes, '리모컨 PIN') : null);
      if (x) return x; }
    const c = await auth.createCredential(env, s.church, a.role, a.label || '');
    // 열쇠 원문은 지금 한 번만 보여 준다. 출력 · 설치판 열쇠는 '교회.열쇠' 꼴로 쓴다
    return json({ ok: true, credential: { id: c.id, role: c.role, label: c.label, secret: c.secret, key: a.role === 'output' || a.role === 'bridge' ? `${s.church}.${c.secret}` : undefined } });
  }
  if (request.method === 'DELETE') {
    const id = path.split('/').pop();
    if (id === s.cred) return fail(409, '지금 로그인한 열쇠는 지울 수 없습니다. 다른 주인 열쇠로 들어와 지워 주세요.');
    if (!(await auth.revokeCredential(env, s.church, id))) return fail(404, '그런 열쇠가 없습니다.');
    // 그 열쇠로 붙어 있던 화면을 바로 끊는다
    const r = await room(env, s.church).fetch(new Request(`https://room/internal/revoke?cred=${encodeURIComponent(id)}`)).then(x => x.json()).catch(() => ({ closed: 0 }));
    return json({ ok: true, closed: r.closed || 0 });
  }
  return fail(404, '없는 주소입니다.');
}

/** 새 교회 만들기 (서비스 관리자, ADMIN_TOKEN). 주인 열쇠 하나를 만들어 한 번만 돌려준다. ADMIN_TOKEN 이 없으면 이 길은 닫혀 있다 */
async function adminCreateChurch(request, env) {
  const given = (request.headers.get('authorization') || '').replace(/^Bearer\s+/i, '');
  if (!auth.configured(env) || typeof env.ADMIN_TOKEN !== 'string' || env.ADMIN_TOKEN.length < 32) return fail(404, '없는 주소입니다.');
  if (!auth.safeEqual(given, env.ADMIN_TOKEN)) return fail(401, '관리자 열쇠가 맞지 않습니다.');
  const a = await request.json().catch(() => ({}));
  if (!validChurch(a.id)) return fail(400, '교회 ID 는 영문 소문자 · 숫자 · - 로 2~40자입니다.');
  const now = Date.now();
  const made = await env.DB.prepare('INSERT INTO churches (id, name, created_at) VALUES (?1, ?2, ?3) ON CONFLICT (id) DO NOTHING').bind(a.id, String(a.name || a.id).slice(0, 80), now).run();
  if (!made.meta.changes) return fail(409, '이미 있는 교회 ID 입니다.');
  const owner = await auth.createCredential(env, a.id, 'owner', '처음 만든 주인 열쇠', now);
  return json({ ok: true, church: a.id, owner: { id: owner.id, secret: owner.secret } });
}
