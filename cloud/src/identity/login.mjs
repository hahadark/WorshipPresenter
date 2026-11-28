// 쉬운 로그인 (계약: docs/web/login.md) — 교회 ID 를 치지 않고 사람 계정으로.
//   POST /auth/password {login, password, remember, next}   이름(또는 이메일 모양 이름) · 비밀번호 → 교회가 하나면 바로, 여럿이면 고르기
//   GET  /auth/pick · POST /auth/pick {church}                고르기 (10분 서명 쿠키 wp_pick)
//   GET  /auth/invite/<열쇠>                                   초대 링크 → /login?invite=<열쇠>
//   GET  /auth/invite?t=<열쇠>                                 초대 내용 (교회 이름 · 역할 · 끝나는 때)
//   구글: drive/oauth.mjs 의 mode=login 이 googleLogin() 을 부른다 (구글 번호로 된 계정 + 그 이메일로 온 초대 + 받은 초대 링크)
//   주인: GET /api/v1/sessions · DELETE /api/v1/sessions/{id} · GET/POST /api/v1/invites · DELETE /api/v1/invites/{id}
// 세션: 사람마다 · 기기마다 member_sessions 한 줄. 쿠키 열쇠 칸 'm:<사람>:<세대>:<세션>'.
//   이 기기 기억(기본): 90일, 하루 넘게 쓰면 90일로 다시 늘림 · 끄면: 브라우저를 닫으면 끝나는 쿠키 + 서버에서도 12시간.
import * as auth from '../auth.mjs';
import { HOME, PROTOCOL, fail, json } from '../protocol.mjs';
import { readBody } from '../data/documents.mjs';
import { actorOf } from '../data/store.mjs';
import { checkPassword } from './password.mjs';
import { MAX_MEMBERS, MEMBER_ROLES, SESSION_ROLE, createMember, forgetSession, listMembers, parseCred, sessionCredOf, validEmail, validLogin } from './members.mjs';

export const REMEMBER_MS = 90 * 86400_000;
export const SHORT_MS = 12 * 3600_000;
export const RENEW_AFTER_MS = 86400_000;
export const INVITE_LINK_MS = 7 * 86400_000;
export const INVITE_EMAIL_MS = 30 * 86400_000;
const PICK_COOKIE = 'wp_pick';
const BAD = '이름이나 비밀번호가 맞지 않습니다.';
const enc = new TextEncoder();
const b64u = b => btoa(String.fromCharCode(...new Uint8Array(b))).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
const unb64u = s => Uint8Array.from(atob(s.replace(/-/g, '+').replace(/_/g, '/')), c => c.charCodeAt(0));
const ipOf = request => request.headers.get('cf-connecting-ip') || 'local';
const sameOrigin = (request, url) => { const o = request.headers.get('origin'); return !o || o === url.origin; };
export const safeNext = n => (typeof n === 'string' && n.startsWith('/') && !n.startsWith('//') && !n.includes('\\') && n.length < 300 ? n : '');
const hex = b => [...new Uint8Array(b)].map(x => x.toString(16).padStart(2, '0')).join('');
export const tokenHash = async t => hex(await crypto.subtle.digest('SHA-256', enc.encode(`invite:${t}`)));
const rand = n => Array.from(crypto.getRandomValues(new Uint8Array(n)), b => 'ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789'[b % 57]).join('');
export const validInviteToken = t => typeof t === 'string' && /^[A-Za-z0-9]{24,64}$/.test(t);

/** 사용자 에이전트 → '크롬 · 안드로이드' 처럼 짧게 (원문은 저장하지 않는다) */
export function uaName(ua) {
  const u = String(ua || '');
  const b = /Edg\//.test(u) ? '엣지' : /OBS\//.test(u) ? 'OBS' : /SamsungBrowser/.test(u) ? '삼성 인터넷' : /Chrome\//.test(u) ? '크롬' : /Firefox\//.test(u) ? '파이어폭스' : /Safari\//.test(u) ? '사파리' : '브라우저';
  const o = /iPhone|iPad/.test(u) ? '아이폰 · 아이패드' : /Android/.test(u) ? '안드로이드' : /Windows/.test(u) ? '윈도우' : /Mac OS X/.test(u) ? '맥' : /Linux/.test(u) ? '리눅스' : '';
  return o ? `${b} · ${o}` : b;
}

// ---------- 틀린 횟수 (auth_failures, 교회를 모르니 church_id '*') ----------
const LOCK_MAX = 15 * 60_000, WINDOW = 15 * 60_000;
async function locked(env, scope, now) {
  const r = await env.DB.prepare("SELECT locked_until AS u FROM auth_failures WHERE church_id = '*' AND scope = ?1").bind(scope).first();
  return !!r && r.u > now;
}
async function noteFailure(env, scope, now, step, lock) {
  const r = await env.DB.prepare("SELECT count AS n, updated_at AS at FROM auth_failures WHERE church_id = '*' AND scope = ?1").bind(scope).first();
  const n = r && now - r.at < WINDOW ? r.n + 1 : 1;
  const until = n % step === 0 ? now + Math.min(lock * 2 ** (n / step - 1), LOCK_MAX) : 0;
  await env.DB.prepare(
    "INSERT INTO auth_failures (church_id, scope, count, locked_until, updated_at) VALUES ('*', ?1, ?2, ?3, ?4) ON CONFLICT (church_id, scope) DO UPDATE SET count = ?2, locked_until = ?3, updated_at = ?4",
  ).bind(scope, n, until, now).run();
}

// ---------- 세션 ----------
function sessionCookie(request, token, remember) {
  const secure = new URL(request.url).protocol === 'https:' ? '; Secure' : '';
  // 기억하지 않으면 Max-Age 없이 (브라우저를 닫으면 사라지는 쿠키)
  return `${auth.COOKIE}=${token}; Path=/${remember ? `; Max-Age=${Math.floor(REMEMBER_MS / 1000)}` : ''}; SameSite=Lax; HttpOnly${secure}`;
}

/** 사람 한 명 · 기기 하나 → 세션 줄 + 쿠키. 돌려줌 { headers:[…], role, home } */
export async function startSession(env, request, church, m, { remember = true, via = '' } = {}, now = Date.now()) {
  const sid = auth.newId();
  const ttl = remember ? REMEMBER_MS : SHORT_MS;
  await env.DB.batch([
    env.DB.prepare('INSERT INTO member_sessions (church_id, id, member_id, remember, via, ua, created_at, last_seen_at, expires_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7, ?8)')
      .bind(church, sid, m.id, remember ? 1 : 0, via, uaName(request.headers.get('user-agent')), now, now + ttl),
    env.DB.prepare('UPDATE members SET last_login_at = ?3 WHERE church_id = ?1 AND id = ?2').bind(church, m.id, now),
    // 끝난 세션 줄은 정리 (그 사람 것만, 30일 지난 것)
    env.DB.prepare('DELETE FROM member_sessions WHERE church_id = ?1 AND member_id = ?2 AND (expires_at < ?3 OR revoked_at < ?3)').bind(church, m.id, now - 30 * 86400_000),
  ]);
  const role = SESSION_ROLE[m.role];
  const { token } = await auth.issueToken(env, { church, role, cred: sessionCredOf(m, sid) }, now, ttl);
  return { role, home: HOME[role], sid, headers: [sessionCookie(request, token, remember), auth.setCookie(auth.CHURCH_COOKIE, church, request, 400 * 86400, false)] };
}

/** 이 기기 기억: 하루 넘게 쓴 세션은 90일로 다시 늘린다 (index.mjs 가 응답에 쿠키를 붙인다). 아니면 null */
export async function maybeRenew(env, request, s, now = Date.now()) {
  const p = parseCred(s.cred);
  if (!p || !p.sid || !(s.exp - now < REMEMBER_MS - RENEW_AFTER_MS)) return null;
  const row = await env.DB.prepare('SELECT remember FROM member_sessions WHERE church_id = ?1 AND id = ?2 AND revoked_at IS NULL').bind(s.church, p.sid).first();
  if (!row || !row.remember) return null;
  await env.DB.prepare('UPDATE member_sessions SET expires_at = ?3, last_seen_at = ?4 WHERE church_id = ?1 AND id = ?2').bind(s.church, p.sid, now + REMEMBER_MS, now).run();
  const { token } = await auth.issueToken(env, { church: s.church, role: s.role, cred: s.cred }, now, REMEMBER_MS);
  return [sessionCookie(request, token, true)];
}

/** 로그아웃: 이 기기 세션 줄을 끊는다 (쿠키는 index.mjs 가 지움) */
export async function logout(request, env) {
  const s = await auth.readToken(env, auth.cookies(request)[auth.COOKIE]).catch(() => null);
  const p = s && parseCred(s.cred);
  if (!p || !p.sid) return;
  await env.DB.prepare('UPDATE member_sessions SET revoked_at = ?3 WHERE church_id = ?1 AND id = ?2 AND revoked_at IS NULL').bind(s.church, p.sid, Date.now()).run();
  forgetSession(s.church, p.sid);
}

// ---------- 고르기 (교회가 여럿일 때) ----------
async function hmac(env, text) {
  const key = await crypto.subtle.importKey('raw', enc.encode(env.AUTH_SECRET), { name: 'HMAC', hash: 'SHA-256' }, false, ['sign']);
  return b64u(await crypto.subtle.sign('HMAC', key, enc.encode(text)));
}
async function packPick(env, obj) {
  const body = b64u(enc.encode(JSON.stringify(obj)));
  return `v1.${body}.${await hmac(env, `pick:${body}`)}`;
}
async function unpackPick(env, text, now = Date.now()) {
  if (typeof text !== 'string' || text.length > 4000) return null;
  const [v, body, sig, extra] = text.split('.');
  if (v !== 'v1' || !body || !sig || extra !== undefined || !auth.safeEqual(sig, await hmac(env, `pick:${body}`))) return null;
  try { const o = JSON.parse(new TextDecoder().decode(unb64u(body))); return o.e > now ? o : null; } catch { return null; }
}
const pickCookie = (request, value, maxAge) => `${PICK_COOKIE}=${value}; Path=/auth/pick; Max-Age=${maxAge}; SameSite=Lax; HttpOnly${new URL(request.url).protocol === 'https:' ? '; Secure' : ''}`;

/**
 * 누구인지 확인된 뒤: 들어갈 교회(사람 줄)가 하나면 세션, 여럿이면 고르기 쿠키.
 * list = [{ church, m }]. 돌려줌 { session:{…} } | { pick:true, headers } | { none:true }
 */
export async function finish(env, request, list, { remember = true, via = '', next = '' } = {}) {
  if (!list.length) return { none: true };
  if (list.length === 1) return { session: await startSession(env, request, list[0].church, list[0].m, { remember, via }) };
  const pick = await packPick(env, { l: list.slice(0, 20).map(x => ({ c: x.church, i: x.m.id, g: x.m.gen })), r: remember ? 1 : 0, v: via, n: safeNext(next), e: Date.now() + 600_000 });
  return { pick: true, headers: [pickCookie(request, pick, 600)] };
}

async function pickGet(request, env) {
  const p = await unpackPick(env, auth.cookies(request)[PICK_COOKIE]);
  if (!p) return fail(401, '고를 시간이 지났습니다. 다시 로그인해 주세요.');
  const out = [];
  for (const x of p.l) {
    const r = await env.DB.prepare('SELECT m.role AS role, m.label AS label, h.name AS name FROM members m JOIN churches h ON h.id = m.church_id WHERE m.church_id = ?1 AND m.id = ?2 AND m.gen = ?3 AND m.disabled_at IS NULL AND h.disabled_at IS NULL').bind(x.c, x.i, x.g).first();
    if (r) out.push({ church: x.c, name: r.name, role: r.role });
  }
  return json({ ok: true, churches: out, next: p.n || '' });
}
async function pickPost(request, env, url) {
  if (!sameOrigin(request, url)) return fail(403, '다른 곳에서 온 요청입니다.');
  const p = await unpackPick(env, auth.cookies(request)[PICK_COOKIE]);
  if (!p) return fail(401, '고를 시간이 지났습니다. 다시 로그인해 주세요.');
  const b = await readBody(request, 500);
  if (b.error) return b.error;
  const x = p.l.find(y => y.c === String(b.value?.church || ''));
  if (!x) return fail(400, '고를 수 있는 교회가 아닙니다.');
  const m = await env.DB.prepare('SELECT m.* FROM members m JOIN churches h ON h.id = m.church_id WHERE m.church_id = ?1 AND m.id = ?2 AND m.gen = ?3 AND m.disabled_at IS NULL AND h.disabled_at IS NULL').bind(x.c, x.i, x.g).first();
  if (!m) return fail(401, '그 교회 계정이 바뀌었습니다. 다시 로그인해 주세요.');
  const sess = await startSession(env, request, x.c, m, { remember: !!p.r, via: p.v || 'pick' });
  const res = json({ ok: true, protocol: PROTOCOL, church: x.c, role: sess.role, home: p.n || sess.home });
  for (const h of [...sess.headers, pickCookie(request, '', 0)]) res.headers.append('set-cookie', h);
  return res;
}

// ---------- 이름 · 비밀번호 (교회 ID 없이) ----------
async function passwordLogin(request, env, url) {
  if (!sameOrigin(request, url)) return fail(403, '다른 곳에서 온 요청입니다.');
  if (!auth.configured(env)) return fail(503, '로그인이 아직 설정되지 않았습니다.');
  const b = await readBody(request, 2000);
  if (b.error) return b.error;
  const a = b.value || {};
  const login = String(a.login || '').trim().toLowerCase();
  const password = typeof a.password === 'string' ? a.password : '';
  if (!validLogin(login) || !password || password.length > 200) return fail(401, BAD);
  const now = Date.now();
  const ipScope = `pip:${ipOf(request)}`, userScope = `plogin:${login}`;
  if ((await locked(env, ipScope, now)) || (await locked(env, userScope, now))) return fail(429, '여러 번 틀렸습니다. 잠시 뒤에 다시 해 보세요.');
  const subject = `pw:${login}`;
  const rows = (await env.DB.prepare(
    'SELECT m.* FROM members m JOIN churches h ON h.id = m.church_id WHERE m.subject = ?1 AND m.disabled_at IS NULL AND h.disabled_at IS NULL ORDER BY m.created_at LIMIT 5',
  ).bind(subject).all()).results;
  const list = [];
  // 같은 이름이 여러 교회에 있으면 비밀번호가 맞는 교회만 (교회마다 따로 만든 계정). 없는 이름이어도 한 번은 계산한다 (있는지 시간으로 알아내지 못하게)
  for (const m of rows.length ? rows : [{ church_id: '-', secret_hash: 'p1$20000$AAAAAAAAAAAAAAAAAAAAAA$x' }]) {
    if (await checkPassword(env.AUTH_SECRET, m.church_id, subject, password, m.secret_hash || 'p1$20000$AAAAAAAAAAAAAAAAAAAAAA$x') && m.id) list.push({ church: m.church_id, m });
  }
  if (!list.length) {
    await noteFailure(env, ipScope, now, 5, 30_000);
    await noteFailure(env, userScope, now, 10, 60_000);
    return fail(401, BAD);
  }
  const r = await finish(env, request, list, { remember: a.remember !== false, via: 'password', next: a.next });
  if (r.pick) {
    const res = json({ ok: true, pick: true });
    for (const h of r.headers) res.headers.append('set-cookie', h);
    return res;
  }
  const res = json({ ok: true, protocol: PROTOCOL, church: list[0].church, role: r.session.role, home: safeNext(a.next) || r.session.home });
  for (const h of r.session.headers) res.headers.append('set-cookie', h);
  return res;
}

// ---------- 구글 (drive/oauth.mjs 가 토큰 · id_token 확인을 마친 뒤 부른다) ----------
/** 구글 계정 → 들어갈 교회들. 받은 초대 링크 · 이 이메일로 온 초대를 먼저 사람 계정으로 바꾼다 */
export async function googleMemberships(env, who, inviteToken = '', now = Date.now()) {
  const subject = `google:${who.sub}`;
  const claim = async inv => {
    const old = await env.DB.prepare('SELECT * FROM members WHERE church_id = ?1 AND subject = ?2').bind(inv.church_id, subject).first();
    if (!old || old.disabled_at) {
      if ((await listMembers(env, inv.church_id)).length >= MAX_MEMBERS) return;
      await createMember(env, inv.church_id, { subject, role: inv.role, label: (who.email || '').split('@')[0].slice(0, 60) || (inv.label || ''), secretHash: null }, now);
    }
    const m = await env.DB.prepare('SELECT id FROM members WHERE church_id = ?1 AND subject = ?2').bind(inv.church_id, subject).first();
    await env.DB.prepare('UPDATE invites SET used_at = ?3, used_by = ?4 WHERE church_id = ?1 AND id = ?2 AND used_at IS NULL').bind(inv.church_id, inv.id, now, m ? `m:${m.id}` : '').run();
  };
  if (validInviteToken(inviteToken)) {
    const inv = await env.DB.prepare('SELECT * FROM invites WHERE token_hash = ?1 AND used_at IS NULL AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at > ?2)').bind(await tokenHash(inviteToken), now).first();
    // 링크에 이메일이 적혀 있으면 그 이메일의 구글 계정만 받는다
    if (inv && (!inv.email || (who.emailVerified && inv.email === who.email))) await claim(inv);
  }
  if (who.emailVerified && who.email) {
    const invs = (await env.DB.prepare('SELECT * FROM invites WHERE email = ?1 AND used_at IS NULL AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at > ?2) LIMIT 20').bind(who.email, now).all()).results;
    for (const inv of invs) await claim(inv);
  }
  const rows = (await env.DB.prepare(
    'SELECT m.church_id AS church, m.* FROM members m JOIN churches h ON h.id = m.church_id WHERE m.subject = ?1 AND m.disabled_at IS NULL AND h.disabled_at IS NULL ORDER BY m.created_at LIMIT 20',
  ).bind(subject).all()).results;
  // 확인된 이메일을 사람 줄에 적어 둔다 (주인 화면에 보이게 · 다음 초대 맞추기)
  if (who.emailVerified && who.email) for (const r of rows) if (r.email !== who.email) await env.DB.prepare('UPDATE members SET email = ?3 WHERE church_id = ?1 AND id = ?2').bind(r.church, r.id, who.email).run();
  return rows.map(m => ({ church: m.church, m }));
}

/** 구글 로그인 마무리 → 302 응답 (oauth.mjs 가 부름) */
export async function googleLogin(env, request, url, st, who, extraCookies = []) {
  const list = await googleMemberships(env, who, st.i || '');
  const r = await finish(env, request, list, { remember: st.r !== 0, via: 'google', next: st.n });
  const h = new Headers({ 'cache-control': 'no-store' });
  for (const c of extraCookies) h.append('set-cookie', c);
  if (r.none) { h.set('location', `${url.origin}/login?google=no-account`); return new Response(null, { status: 302, headers: h }); }
  if (r.pick) {
    for (const c of r.headers) h.append('set-cookie', c);
    h.set('location', `${url.origin}/login?pick=1`);
    return new Response(null, { status: 302, headers: h });
  }
  for (const c of r.session.headers) h.append('set-cookie', c);
  h.set('location', `${url.origin}${safeNext(st.n) || r.session.home}`);
  return new Response(null, { status: 302, headers: h });
}

// ---------- 초대 (공개 쪽) ----------
async function inviteInfo(env, url) {
  const t = url.searchParams.get('t') || '';
  if (!validInviteToken(t)) return fail(404, '초대가 없거나 끝났습니다.');
  const r = await env.DB.prepare('SELECT i.role AS role, i.email AS email, i.expires_at AS exp, h.name AS name FROM invites i JOIN churches h ON h.id = i.church_id WHERE i.token_hash = ?1 AND i.used_at IS NULL AND i.revoked_at IS NULL AND (i.expires_at IS NULL OR i.expires_at > ?2) AND h.disabled_at IS NULL')
    .bind(await tokenHash(t), Date.now()).first();
  if (!r) return fail(404, '초대가 없거나 끝났습니다. 보낸 분께 새 초대를 부탁해 주세요.');
  const mask = e => (e ? e.replace(/^(.{2}).*(@.*)$/, '$1…$2') : '');
  return json({ ok: true, church: r.name, role: r.role, email: mask(r.email), expiresAt: r.exp });
}

/** 로그인 전에 오는 길 (identity/index.mjs publicRoute 가 넘긴다). 해당 없으면 null */
export async function publicRoute(request, env, url) {
  const path = url.pathname;
  if (path === '/auth/password' && request.method === 'POST') return passwordLogin(request, env, url);
  if (path === '/auth/pick' && request.method === 'GET') return pickGet(request, env);
  if (path === '/auth/pick' && request.method === 'POST') return pickPost(request, env, url);
  if (path === '/auth/invite' && request.method === 'GET') return inviteInfo(env, url);
  const m = /^\/auth\/invite\/([A-Za-z0-9]{24,64})$/.exec(path);
  if (m && request.method === 'GET') return new Response(null, { status: 302, headers: { location: `${url.origin}/login?invite=${m[1]}`, 'cache-control': 'no-store', 'referrer-policy': 'no-referrer' } });
  return null;
}

// ---------- 주인: 로그인한 기기 · 초대 ----------
export function loginRoute(method, path) {
  const get = method === 'GET' || method === 'HEAD';
  if (path === '/api/v1/sessions' && get) return { kind: 'identity', cap: 'manage', op: 'sessions' };
  if (/^\/api\/v1\/sessions\/[A-Za-z0-9]{6,24}$/.test(path) && method === 'DELETE') return { kind: 'identity', cap: 'manage', op: 'revokeSession' };
  if (path === '/api/v1/invites' && get) return { kind: 'identity', cap: 'manage', op: 'invites' };
  if (path === '/api/v1/invites' && method === 'POST') return { kind: 'identity', cap: 'manage', op: 'createInvite' };
  if (/^\/api\/v1\/invites\/[A-Za-z0-9]{6,24}$/.test(path) && method === 'DELETE') return { kind: 'identity', cap: 'manage', op: 'revokeInvite' };
  return null;
}

export async function handle(request, env, url, s, r) {
  const room = env.ROOMS.get(env.ROOMS.idFromName(s.church));
  const now = Date.now();
  switch (r.op) {
    case 'sessions': {
      const rows = (await env.DB.prepare(
        `SELECT s.id, s.member_id AS memberId, s.remember, s.via, s.ua, s.created_at AS createdAt, s.last_seen_at AS lastSeenAt, s.expires_at AS expiresAt, m.label, m.email, m.subject, m.role
         FROM member_sessions s JOIN members m ON m.church_id = s.church_id AND m.id = s.member_id
         WHERE s.church_id = ?1 AND s.revoked_at IS NULL AND s.expires_at > ?2 AND m.disabled_at IS NULL ORDER BY s.last_seen_at DESC LIMIT 200`,
      ).bind(s.church, now).all()).results;
      const me = parseCred(s.cred);
      return json({ ok: true, sessions: rows.map(x => ({ id: x.id, memberId: x.memberId, who: x.label || x.email || x.subject.replace(/^pw:/, ''), role: x.role, device: x.ua, via: x.via, remember: !!x.remember, createdAt: x.createdAt, lastSeenAt: x.lastSeenAt, expiresAt: x.expiresAt, current: !!me && me.sid === x.id })) });
    }
    case 'revokeSession': {
      const id = url.pathname.split('/').pop();
      const x = await env.DB.prepare('SELECT s.member_id AS mid, m.gen AS gen FROM member_sessions s JOIN members m ON m.church_id = s.church_id AND m.id = s.member_id WHERE s.church_id = ?1 AND s.id = ?2 AND s.revoked_at IS NULL').bind(s.church, id).first();
      if (!x) return fail(404, '그런 기기가 없습니다.');
      await env.DB.prepare('UPDATE member_sessions SET revoked_at = ?3 WHERE church_id = ?1 AND id = ?2').bind(s.church, id, now).run();
      forgetSession(s.church, id);
      // 그 기기로 붙은 실시간 연결(출력 · 조작 화면)도 바로 끊는다
      await room.fetch(new Request(`https://room/internal/revoke?cred=${encodeURIComponent(`m:${x.mid}:${x.gen}:${id}`)}`)).catch(() => null);
      return json({ ok: true });
    }
    case 'invites': {
      const rows = (await env.DB.prepare('SELECT id, email, role, label, token_hash AS th, created_at AS createdAt, expires_at AS expiresAt, used_at AS usedAt FROM invites WHERE church_id = ?1 AND revoked_at IS NULL AND (used_at IS NULL OR used_at > ?2) ORDER BY created_at DESC LIMIT 100')
        .bind(s.church, now - 30 * 86400_000).all()).results;
      return json({ ok: true, invites: rows.map(x => ({ id: x.id, email: x.email || '', role: x.role, label: x.label, link: !!x.th, createdAt: x.createdAt, expiresAt: x.expiresAt, usedAt: x.usedAt, expired: !!x.expiresAt && x.expiresAt <= now })) });
    }
    case 'createInvite': {
      const b = await readBody(request, 2000);
      if (b.error) return b.error;
      const a = b.value || {};
      if (!MEMBER_ROLES.includes(a.role)) return fail(400, '역할은 owner · editor · viewer 중 하나입니다.');
      const email = a.email ? String(a.email).trim().toLowerCase() : '';
      if (email && !validEmail(email)) return fail(400, '이메일 주소가 올바르지 않습니다.');
      if (!email && !a.link) return fail(400, '이메일을 적거나 초대 링크를 만드세요.');
      if (email) {
        const dup = await env.DB.prepare('SELECT 1 FROM members WHERE church_id = ?1 AND email = ?2 AND disabled_at IS NULL').bind(s.church, email).first();
        if (dup) return fail(409, '이미 이 교회 사람입니다 (역할은 사람 목록에서 바꾸세요).');
        // 같은 이메일의 끝나지 않은 초대는 새것으로 바꾼다
        await env.DB.prepare('UPDATE invites SET revoked_at = ?3 WHERE church_id = ?1 AND email = ?2 AND used_at IS NULL AND revoked_at IS NULL').bind(s.church, email, now).run();
      }
      const pending = await env.DB.prepare('SELECT COUNT(*) AS n FROM invites WHERE church_id = ?1 AND used_at IS NULL AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at > ?2)').bind(s.church, now).first();
      if ((pending?.n || 0) >= 50) return fail(409, '받지 않은 초대가 너무 많습니다. 안 쓰는 초대를 지워 주세요.');
      const id = auth.newId();
      const token = a.link ? rand(32) : '';
      const exp = now + (a.link ? INVITE_LINK_MS : INVITE_EMAIL_MS);
      await env.DB.prepare('INSERT INTO invites (church_id, id, email, role, token_hash, label, created_by, created_at, expires_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)')
        .bind(s.church, id, email || null, a.role, token ? await tokenHash(token) : null, String(a.label || '').slice(0, 60), actorOf(s), now, exp).run();
      // 링크 원문은 지금 한 번만 보여 준다 (서버에는 해시만)
      return json({ ok: true, invite: { id, email, role: a.role, link: !!token, expiresAt: exp }, url: token ? `${url.origin}/auth/invite/${token}` : undefined }, 201);
    }
    case 'revokeInvite': {
      const id = url.pathname.split('/').pop();
      const r2 = await env.DB.prepare('UPDATE invites SET revoked_at = ?3 WHERE church_id = ?1 AND id = ?2 AND revoked_at IS NULL AND used_at IS NULL').bind(s.church, id, now).run();
      if (!r2.meta.changes) return fail(404, '그런 초대가 없습니다.');
      return json({ ok: true });
    }
    default: return fail(404, '없는 주소입니다.');
  }
}
