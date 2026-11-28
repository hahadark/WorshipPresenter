// 사람 계정 로그인 · 관리 (2단계 A). 기기 열쇠(credentials) · 리모컨/편집기 PIN 은 1단계 그대로 둔다.
//   POST /auth/member {church, login, password}        → 비밀번호 로그인 (쿠키)
//   GET  /auth/access?church=&next=                    → Cloudflare Access 로그인 (켜 두었을 때만)
//   GET/POST /api/v1/members · PATCH/DELETE /api/v1/members/{id}   → 주인(manage)만
//   POST /api/v1/me/password {current, next}           → 사람 계정 본인 비밀번호 바꾸기
import * as auth from '../auth.mjs';
import { HOME, PROTOCOL, SESSION_TTL, fail, json, validChurch } from '../protocol.mjs';
import { readBody } from '../data/documents.mjs';
import { accessConfigured, verifyAccessJwt } from './access.mjs';
import { checkPassword, hashPassword, newPassword, weakPassword } from './password.mjs';
import {
  MAX_MEMBERS, MEMBER_ROLES, SESSION_ROLE, createMember, credOf, forgetMember, listMembers, memberById, parseCred, validEmail, validLogin, view,
} from './members.mjs';

const ipOf = request => request.headers.get('cf-connecting-ip') || 'local';
const sameOrigin = (request, url) => { const o = request.headers.get('origin'); return !o || o === url.origin; };
const BAD = '교회 이름이나 로그인 이름 · 비밀번호가 맞지 않습니다.';

// ---------- 틀린 횟수 (1단계 auth_failures 표를 같이 쓴다, scope 앞머리로 나눔) ----------
const LOCK_MAX = 15 * 60_000, WINDOW = 15 * 60_000;
async function locked(env, church, scope, now) {
  const r = await env.DB.prepare('SELECT locked_until AS u FROM auth_failures WHERE church_id = ?1 AND scope = ?2').bind(church, scope).first();
  return !!r && r.u > now;
}
async function noteFailure(env, church, scope, now, step, lock) {
  const r = await env.DB.prepare('SELECT count AS n, updated_at AS at FROM auth_failures WHERE church_id = ?1 AND scope = ?2').bind(church, scope).first();
  const n = r && now - r.at < WINDOW ? r.n + 1 : 1;
  const until = n % step === 0 ? now + Math.min(lock * 2 ** (n / step - 1), LOCK_MAX) : 0;
  await env.DB.prepare(
    'INSERT INTO auth_failures (church_id, scope, count, locked_until, updated_at) VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT (church_id, scope) DO UPDATE SET count = ?3, locked_until = ?4, updated_at = ?5',
  ).bind(church, scope, n, until, now).run();
}

async function sessionCookies(env, request, church, m) {
  const role = SESSION_ROLE[m.role];
  const { token, exp } = await auth.issueToken(env, { church, role, cred: credOf(m) });
  return { role, exp, headers: [auth.setCookie(auth.COOKIE, token, request, SESSION_TTL[role]), auth.setCookie(auth.CHURCH_COOKIE, church, request, 400 * 86400, false)] };
}
const room = (env, church) => env.ROOMS.get(env.ROOMS.idFromName(church));
/** 그 사람의 옛 세대 쿠키로 붙은 실시간 연결을 끊는다 */
async function closeSockets(env, church, m) {
  forgetMember(church, m.id);
  await room(env, church).fetch(new Request(`https://room/internal/revoke?cred=${encodeURIComponent(credOf(m))}`)).catch(() => null);
}

/** 로그인 전에 오는 길. 해당 없으면 null */
export async function publicRoute(request, env, url) {
  const path = url.pathname;
  if (path === '/auth/member' && request.method === 'POST') return passwordLogin(request, env, url);
  if (path === '/auth/access' && request.method === 'GET') return accessLogin(request, env, url);
  return null;
}

async function passwordLogin(request, env, url) {
  if (!sameOrigin(request, url)) return fail(403, '다른 곳에서 온 요청입니다.');
  if (!auth.configured(env)) return fail(503, '로그인이 아직 설정되지 않았습니다.');
  const text = await request.text();
  if (text.length > 2000) return fail(413, '너무 큽니다.');
  let a;
  try { a = JSON.parse(text); } catch { return fail(400, '잘못된 요청'); }
  const church = String(a.church || '').trim().toLowerCase();
  const login = String(a.login || '').trim().toLowerCase();
  const password = typeof a.password === 'string' ? a.password : '';
  if (!validChurch(church) || !validLogin(login) || !password || password.length > 200) return fail(401, BAD);
  const now = Date.now();
  const ipScope = `mip:${ipOf(request)}`, userScope = `mlogin:${login}`;
  if ((await locked(env, church, ipScope, now)) || (await locked(env, church, userScope, now))) return fail(429, '여러 번 틀렸습니다. 잠시 뒤에 다시 해 보세요.');
  const subject = `pw:${login}`;
  const m = await env.DB.prepare(
    'SELECT m.* FROM members m JOIN churches h ON h.id = m.church_id WHERE m.church_id = ?1 AND m.subject = ?2 AND m.disabled_at IS NULL AND h.disabled_at IS NULL',
  ).bind(church, subject).first();
  // 없는 이름이어도 같은 만큼 계산한다 (이름이 있는지 시간으로 알아내지 못하게)
  const ok = await checkPassword(env.AUTH_SECRET, church, subject, password, m?.secret_hash || 'p1$20000$AAAAAAAAAAAAAAAAAAAAAA$x');
  if (!m || !ok) {
    await noteFailure(env, church, ipScope, now, 5, 30_000);
    await noteFailure(env, church, userScope, now, 10, 60_000);
    return fail(401, BAD);
  }
  await env.DB.prepare('UPDATE members SET last_login_at = ?3 WHERE church_id = ?1 AND id = ?2').bind(church, m.id, now).run();
  const c = await sessionCookies(env, request, church, m);
  const res = json({ ok: true, protocol: PROTOCOL, role: c.role, memberRole: m.role, home: HOME[c.role] });
  for (const h of c.headers) res.headers.append('set-cookie', h);
  return res;
}

async function accessLogin(request, env, url) {
  if (!auth.configured(env) || !accessConfigured(env)) return fail(404, '없는 주소입니다.');
  const church = url.searchParams.get('church') || auth.cookies(request)[auth.CHURCH_COOKIE] || '';
  if (!validChurch(church)) return fail(400, '어느 교회인지 알 수 없습니다 (…?church=교회).');
  const who = await verifyAccessJwt(request.headers.get('cf-access-jwt-assertion') || '', { team: env.ACCESS_TEAM_DOMAIN, aud: env.ACCESS_AUD }).catch(() => null);
  if (!who) return fail(401, 'Cloudflare Access 로그인을 확인하지 못했습니다.');
  const m = await env.DB.prepare(
    'SELECT m.* FROM members m JOIN churches h ON h.id = m.church_id WHERE m.church_id = ?1 AND m.subject = ?2 AND m.disabled_at IS NULL AND h.disabled_at IS NULL',
  ).bind(church, `access:${who.email}`).first();
  if (!m) return fail(403, '이 교회에 등록된 사람이 아닙니다. 주인에게 등록을 부탁해 주세요.');
  await env.DB.prepare('UPDATE members SET last_login_at = ?3 WHERE church_id = ?1 AND id = ?2').bind(church, m.id, Date.now()).run();
  const c = await sessionCookies(env, request, church, m);
  const next = url.searchParams.get('next') || '';
  const to = next.startsWith('/') && !next.startsWith('//') ? next : HOME[c.role];
  const res = new Response(null, { status: 302, headers: { location: to, 'cache-control': 'no-store' } });
  for (const h of c.headers) res.headers.append('set-cookie', h);
  return res;
}

// ---------- 로그인 뒤 (index.mjs 가 권한 · 쓰기 보호를 본 뒤 부른다) ----------
export async function handle(request, env, url, s, r) {
  switch (r.op) {
    case 'list': return json({ ok: true, members: await listMembers(env, s.church) });
    case 'create': return create(request, env, s);
    case 'update': return update(request, env, url, s);
    case 'remove': return remove(env, url, s);
    case 'password': return changePassword(request, env, s);
    default: return fail(404, '없는 주소입니다.');
  }
}

async function create(request, env, s) {
  const b = await readBody(request, 4000);
  if (b.error) return b.error;
  const a = b.value || {};
  if (!MEMBER_ROLES.includes(a.role)) return fail(400, '역할은 owner · editor · viewer 중 하나입니다.');
  const label = String(a.label || '').slice(0, 60);
  let subject, secret = null, secretHash = null;
  if (a.login !== undefined) {
    const login = String(a.login).trim().toLowerCase();
    if (!validLogin(login)) return fail(400, '로그인 이름은 영문 소문자 · 숫자 · . _ @ + - 로 3~64자입니다.');
    subject = `pw:${login}`;
    secret = newPassword();
    secretHash = await hashPassword(env.AUTH_SECRET, s.church, subject, secret);
  } else if (a.email !== undefined) {
    const email = String(a.email).trim().toLowerCase();
    if (!validEmail(email)) return fail(400, '전자우편 주소가 올바르지 않습니다.');
    subject = `access:${email}`;
  } else return fail(400, 'login(비밀번호 계정) 또는 email(Cloudflare Access 계정)을 주세요.');
  if ((await listMembers(env, s.church)).length >= MAX_MEMBERS) return fail(409, `사람 계정은 ${MAX_MEMBERS}개까지입니다.`);
  const m = await createMember(env, s.church, { subject, role: a.role, label, secretHash });
  if (!m) return fail(409, '이미 있는 계정입니다.');
  // 처음 비밀번호는 지금 한 번만 보여 준다 (원문은 저장하지 않는다)
  return json({ ok: true, member: view(m), password: secret || undefined }, 201);
}

const selfId = s => parseCred(s.cred)?.id || '';

async function update(request, env, url, s) {
  const id = url.pathname.split('/').pop();
  const b = await readBody(request, 4000);
  if (b.error) return b.error;
  const a = b.value || {};
  const m = await memberById(env, s.church, id);
  if (!m || m.disabled_at) return fail(404, '그런 계정이 없습니다.');
  if (a.role !== undefined && !MEMBER_ROLES.includes(a.role)) return fail(400, '역할은 owner · editor · viewer 중 하나입니다.');
  const roleChange = a.role !== undefined && a.role !== m.role;
  if (roleChange && id === selfId(s)) return fail(409, '자기 역할은 바꿀 수 없습니다. 다른 주인이 바꿔 주세요.');
  let secret = null, hash = m.secret_hash;
  if (a.resetPassword) {
    if (!m.subject.startsWith('pw:')) return fail(400, 'Cloudflare Access 계정에는 비밀번호가 없습니다.');
    secret = newPassword();
    hash = await hashPassword(env.AUTH_SECRET, s.church, m.subject, secret);
  }
  const bump = roleChange || !!a.resetPassword;
  await env.DB.prepare('UPDATE members SET role = ?3, label = ?4, secret_hash = ?5, gen = gen + ?6, updated_at = ?7 WHERE church_id = ?1 AND id = ?2')
    .bind(s.church, id, a.role ?? m.role, a.label !== undefined ? String(a.label).slice(0, 60) : m.label, hash, bump ? 1 : 0, Date.now()).run();
  if (bump) await closeSockets(env, s.church, m);
  return json({ ok: true, member: view(await memberById(env, s.church, id)), password: secret || undefined });
}

async function remove(env, url, s) {
  const id = url.pathname.split('/').pop();
  if (id === selfId(s)) return fail(409, '지금 로그인한 계정은 지울 수 없습니다.');
  const m = await memberById(env, s.church, id);
  if (!m || m.disabled_at) return fail(404, '그런 계정이 없습니다.');
  await env.DB.prepare('UPDATE members SET disabled_at = ?3, gen = gen + 1, secret_hash = NULL, updated_at = ?3 WHERE church_id = ?1 AND id = ?2').bind(s.church, id, Date.now()).run();
  await closeSockets(env, s.church, m);
  return json({ ok: true });
}

async function changePassword(request, env, s) {
  const p = parseCred(s.cred);
  if (!p) return fail(403, '사람 계정으로 로그인했을 때만 바꿀 수 있습니다.');
  const b = await readBody(request, 2000);
  if (b.error) return b.error;
  const a = b.value || {};
  const m = await memberById(env, s.church, p.id);
  if (!m || m.disabled_at || !m.subject.startsWith('pw:')) return fail(403, '비밀번호 계정이 아닙니다.');
  const now = Date.now();
  const scope = `mlogin:${m.subject.slice(3)}`;
  if (await locked(env, s.church, scope, now)) return fail(429, '여러 번 틀렸습니다. 잠시 뒤에 다시 해 보세요.');
  if (!(await checkPassword(env.AUTH_SECRET, s.church, m.subject, String(a.current || ''), m.secret_hash))) {
    await noteFailure(env, s.church, scope, now, 10, 60_000);
    return fail(401, '지금 비밀번호가 맞지 않습니다.');
  }
  const weak = weakPassword(a.next, m.subject.slice(3));
  if (weak) return fail(400, weak);
  await env.DB.prepare('UPDATE members SET secret_hash = ?3, gen = gen + 1, updated_at = ?4 WHERE church_id = ?1 AND id = ?2')
    .bind(s.church, m.id, await hashPassword(env.AUTH_SECRET, s.church, m.subject, a.next), now).run();
  await closeSockets(env, s.church, m);
  // 다른 기기의 쿠키는 죽고, 이 기기는 새 쿠키로 계속
  const c = await sessionCookies(env, request, s.church, { ...m, gen: m.gen + 1 });
  const res = json({ ok: true });
  for (const h of c.headers) res.headers.append('set-cookie', h);
  return res;
}

