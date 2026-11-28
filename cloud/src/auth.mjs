// 로그인: 교회마다 '열쇠'(기기 자격)를 D1 에 두고, 맞으면 서명한 쿠키를 준다.
// - 열쇠 원문은 저장하지 않는다. HMAC-SHA256(AUTH_SECRET, 교회:역할:열쇠) 만 둔다 → DB 만 새어 나가도 PIN 을 되찾을 수 없다.
// - 쿠키는 { 교회, 역할, 열쇠 id, 만료 } 를 서명한 것. 열쇠를 지우면 그 열쇠로 만든 쿠키가 모두 죽는다 (요청마다 확인, 30초 기억).
// - 틀린 횟수는 D1 에 센다 (기기 주소별 5번마다 30초~, PIN 은 교회 전체 30번이면 15분 — 긴 열쇠는 교회 전체를 잠그지 않는다).
import { ROLES, SESSION_TTL, validChurch } from './protocol.mjs';
import { memberAlive } from './identity/members.mjs'; // 2단계 사람 계정 ('m:' 열쇠)
import { demoAlive } from './tiers/demo.mjs'; // 7단계 둘러보기 ('d:' 열쇠)

const enc = new TextEncoder();
export const COOKIE = 'wp_session';
export const CHURCH_COOKIE = 'wp_church';

const b64u = bytes => btoa(String.fromCharCode(...new Uint8Array(bytes))).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
const unb64u = s => Uint8Array.from(atob(s.replace(/-/g, '+').replace(/_/g, '/')), c => c.charCodeAt(0));
const hex = bytes => [...new Uint8Array(bytes)].map(x => x.toString(16).padStart(2, '0')).join('');

/** 비밀값이 없거나 짧으면 로그인 전체를 닫는다 (열린 채로 배포되지 않게) */
export function configured(env) {
  return typeof env.AUTH_SECRET === 'string' && env.AUTH_SECRET.length >= 32 && !!env.DB;
}
async function hmac(env, text) {
  const key = await crypto.subtle.importKey('raw', enc.encode(env.AUTH_SECRET), { name: 'HMAC', hash: 'SHA-256' }, false, ['sign']);
  return crypto.subtle.sign('HMAC', key, enc.encode(text));
}
export function safeEqual(a, b) {
  if (typeof a !== 'string' || typeof b !== 'string' || a.length !== b.length) return false;
  let d = 0;
  for (let i = 0; i < a.length; i++) d |= a.charCodeAt(i) ^ b.charCodeAt(i);
  return d === 0;
}
export async function credHash(env, church, role, secret) {
  return hex(await hmac(env, `cred:${church}:${role}:${secret}`));
}

// ---------- 열쇠 만들기 ----------
const ALPHA = 'ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789'; // 헷갈리는 0 O 1 l I 를 뺌
function randomText(n) {
  const out = [];
  while (out.length < n) {
    for (const b of crypto.getRandomValues(new Uint8Array(n * 2))) {
      if (b < 228 && out.length < n) out.push(ALPHA[b % 57]); // 228 = 57 × 4 (치우침 없이)
    }
  }
  return out.join('');
}
export function newId() {
  return randomText(10);
}
/** 리모컨 · 편집기는 사람이 치는 PIN 6자리, 나머지는 긴 열쇠 (약 185비트) */
export function newSecret(role) {
  if (role === 'remote' || role === 'editor') {
    let n;
    do n = crypto.getRandomValues(new Uint32Array(1))[0]; while (n >= 4_294_000_000);
    return String(100000 + (n % 900000));
  }
  return randomText(32);
}

// ---------- 쿠키 ----------
export async function issueToken(env, { church, role, cred }, now = Date.now()) {
  const exp = now + (SESSION_TTL[role] || 3600) * 1000;
  const body = b64u(enc.encode(JSON.stringify({ c: church, r: role, k: cred, e: exp })));
  return { token: `v1.${body}.${b64u(await hmac(env, `session:${body}`))}`, exp };
}
export async function readToken(env, token, now = Date.now()) {
  if (typeof token !== 'string' || token.length > 600) return null;
  const [v, body, sig, extra] = token.split('.');
  if (v !== 'v1' || !body || !sig || extra !== undefined) return null;
  if (!safeEqual(sig, b64u(await hmac(env, `session:${body}`)))) return null;
  try {
    const p = JSON.parse(new TextDecoder().decode(unb64u(body)));
    if (!validChurch(p.c) || !ROLES.includes(p.r) || p.r === 'bridge' || typeof p.k !== 'string' || !(p.e > now)) return null;
    return { church: p.c, role: p.r, cred: p.k, exp: p.e };
  } catch {
    return null;
  }
}
export function cookies(request) {
  const out = {};
  for (const part of (request.headers.get('cookie') || '').split(';')) {
    const i = part.indexOf('=');
    if (i > 0) out[part.slice(0, i).trim()] = part.slice(i + 1).trim();
  }
  return out;
}
export function setCookie(name, value, request, maxAge, httpOnly = true) {
  const secure = new URL(request.url).protocol === 'https:' ? '; Secure' : '';
  return `${name}=${value}; Path=/; Max-Age=${maxAge}; SameSite=Lax${httpOnly ? '; HttpOnly' : ''}${secure}`;
}

// ---------- 열쇠 확인 ----------
const live = new Map(); // `${church}/${cred}` → { ok, at } (isolate 마다 30초)
const LIVE_MS = 30_000;
export function forget(church, cred) {
  live.delete(`${church}/${cred}`);
}
/** 쿠키의 열쇠가 아직 살아 있는가 (지웠거나 만료되면 false) */
export async function credentialAlive(env, church, cred, now = Date.now(), role = '') {
  if (String(cred).startsWith('m:')) return memberAlive(env, church, cred, role, now);
  if (String(cred).startsWith('d:')) return role === 'demo' && demoAlive(env, church, now);
  const key = `${church}/${cred}`;
  const hit = live.get(key);
  if (hit && now - hit.at < LIVE_MS) return hit.ok;
  const row = await env.DB.prepare(
    'SELECT c.revoked_at AS revoked, c.expires_at AS expires, h.disabled_at AS disabled FROM credentials c JOIN churches h ON h.id = c.church_id WHERE c.church_id = ?1 AND c.id = ?2',
  ).bind(church, cred).first();
  const ok = !!row && !row.revoked && !row.disabled && (!row.expires || row.expires > now);
  if (live.size > 5000) live.clear();
  live.set(key, { ok, at: now });
  return ok;
}

/** 요청의 쿠키 → 로그인 정보 (없거나 죽었으면 null) */
export async function sessionOf(request, env) {
  if (!configured(env)) return null;
  const s = await readToken(env, cookies(request)[COOKIE]);
  if (!s || !(await credentialAlive(env, s.church, s.cred, Date.now(), s.role))) return null;
  return s;
}

// ---------- 틀린 횟수 ----------
const IP_STEP = 5, IP_LOCK = 30_000, ALL_LIMIT = 30, ALL_LOCK = 15 * 60_000, WINDOW = 15 * 60_000;
async function lockedUntil(env, church, scope, now) {
  const r = await env.DB.prepare('SELECT locked_until AS u FROM auth_failures WHERE church_id = ?1 AND scope = ?2').bind(church, scope).first();
  return r && r.u > now ? r.u : 0;
}
async function noteFailure(env, church, scope, now, step, lock) {
  const r = await env.DB.prepare('SELECT count AS n, updated_at AS at FROM auth_failures WHERE church_id = ?1 AND scope = ?2').bind(church, scope).first();
  const n = r && now - r.at < WINDOW ? r.n + 1 : 1;
  // 걸릴 때마다 두 배 (30초 → 1분 → 2분 … 최대 15분)
  const until = n % step === 0 ? now + Math.min(lock * 2 ** (n / step - 1), ALL_LOCK) : 0;
  await env.DB.prepare(
    'INSERT INTO auth_failures (church_id, scope, count, locked_until, updated_at) VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT (church_id, scope) DO UPDATE SET count = ?3, locked_until = ?4, updated_at = ?5',
  ).bind(church, scope, n, until, now).run();
}

/**
 * 교회 + 열쇠(또는 PIN) → 맞는 자격. roles 안에서만 찾는다.
 * 돌려주는 값: { ok:true, role, cred } · { ok:false, status, error }
 */
export async function verify(env, church, secret, roles, ip, now = Date.now()) {
  const bad = { ok: false, status: 401, error: '교회 이름이나 열쇠(PIN)가 맞지 않습니다.' };
  if (!configured(env)) return { ok: false, status: 503, error: '로그인이 아직 설정되지 않았습니다.' };
  if (!validChurch(church) || typeof secret !== 'string' || secret.length < 4 || secret.length > 128) return bad;
  // 교회 전체 잠금은 PIN(6자리)에만: 긴 열쇠(설치판 · 출력 · 주인)는 맞히기가 불가능하므로 주소별로만 늦춘다.
  // 그렇지 않으면 누구든 틀린 PIN 을 30번 보내 설치판 연결 · OBS 로그인까지 막을 수 있다.
  const pinLike = secret.length <= 12;
  const scopes = pinLike ? [`ip:${ip || '-'}`, 'pins'] : [`ip:${ip || '-'}`];
  for (const s of scopes) {
    if (await lockedUntil(env, church, s, now)) return { ok: false, status: 429, error: '여러 번 틀렸습니다. 잠시 뒤에 다시 해 보세요.' };
  }
  const hashes = await Promise.all(roles.map(r => credHash(env, church, r, secret)));
  const marks = hashes.map((_, i) => `?${i + 2}`).join(',');
  const row = await env.DB.prepare(
    `SELECT c.id AS id, c.role AS role, c.secret_hash AS hash FROM credentials c JOIN churches h ON h.id = c.church_id
     WHERE c.church_id = ?1 AND c.secret_hash IN (${marks}) AND c.revoked_at IS NULL AND h.disabled_at IS NULL AND (c.expires_at IS NULL OR c.expires_at > ${Number(now)}) LIMIT 1`,
  ).bind(church, ...hashes).first();
  // 해시가 같아도 그 역할의 해시여야 한다 (역할을 섞어 쓰지 못하게)
  if (row && roles.includes(row.role) && safeEqual(row.hash, hashes[roles.indexOf(row.role)])) {
    await env.DB.prepare('UPDATE credentials SET last_used_at = ?3 WHERE church_id = ?1 AND id = ?2').bind(church, row.id, now).run();
    return { ok: true, role: row.role, cred: row.id };
  }
  await noteFailure(env, church, scopes[0], now, IP_STEP, IP_LOCK);
  if (pinLike) await noteFailure(env, church, 'pins', now, ALL_LIMIT, ALL_LOCK);
  return bad;
}

/** `교회.열쇠` 꼴 (출력 주소의 ?key= · 설치판 연결 열쇠) 을 나눈다 */
export function splitKey(text) {
  if (typeof text !== 'string') return null;
  const i = text.indexOf('.');
  if (i < 1) return null;
  const church = text.slice(0, i), secret = text.slice(i + 1);
  return validChurch(church) && secret ? { church, secret } : null;
}

// ---------- 열쇠 관리 (주인) ----------
export async function createCredential(env, church, role, label = '', now = Date.now()) {
  if (!ROLES.includes(role)) throw new Error('role');
  const id = newId(), secret = newSecret(role);
  await env.DB.prepare('INSERT INTO credentials (church_id, id, role, label, secret_hash, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)')
    .bind(church, id, role, String(label).slice(0, 60), await credHash(env, church, role, secret), now).run();
  return { id, role, label, secret };
}
export async function listCredentials(env, church) {
  const r = await env.DB.prepare('SELECT id, role, label, created_at AS createdAt, last_used_at AS lastUsedAt FROM credentials WHERE church_id = ?1 AND revoked_at IS NULL ORDER BY created_at').bind(church).all();
  return r.results;
}
export async function revokeCredential(env, church, id, now = Date.now()) {
  const r = await env.DB.prepare('UPDATE credentials SET revoked_at = ?3 WHERE church_id = ?1 AND id = ?2 AND revoked_at IS NULL').bind(church, id, now).run();
  forget(church, id);
  return r.meta.changes > 0;
}
