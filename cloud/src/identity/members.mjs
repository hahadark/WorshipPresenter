// 사람 계정 (D1 members). 역할 owner 주인 · editor 편집자 · viewer 보기만.
// 쿠키의 역할: owner → 'owner'(기기 주인 열쇠와 같은 권한), editor → 'member-editor', viewer → 'member-viewer' (protocol.mjs CAPS).
// 쿠키의 열쇠 칸(k) = 'm:<id>:<세대>'. 역할 · 비밀번호를 바꾸거나 지우면 세대를 올려 앞서 준 쿠키를 모두 죽인다.
import { newId } from '../auth.mjs';

export const MEMBER_ROLES = ['owner', 'editor', 'viewer'];
export const SESSION_ROLE = { owner: 'owner', editor: 'member-editor', viewer: 'member-viewer' };
const LOGIN_RE = /^[a-z0-9][a-z0-9._@+-]{1,62}[a-z0-9]$/;
const EMAIL_RE = /^[^\s@]{1,64}@[^\s@]{1,190}\.[^\s@]{2,}$/;
export const validLogin = s => typeof s === 'string' && LOGIN_RE.test(s);
export const validEmail = s => typeof s === 'string' && s.length <= 254 && EMAIL_RE.test(s);
export const MAX_MEMBERS = 50;

export const credOf = m => `m:${m.id}:${m.gen}`;
/** 기기 세션이 붙은 열쇠 (쉬운 로그인, login.mjs): 'm:<사람>:<세대>:<세션>' — 세션 하나만 '로그아웃시키기' 할 수 있게 */
export const sessionCredOf = (m, sid) => `m:${m.id}:${m.gen}:${sid}`;
export function parseCred(cred) {
  const m = /^m:([A-Za-z0-9]{6,24}):(\d{1,9})(?::([A-Za-z0-9]{6,24}))?$/.exec(String(cred || ''));
  return m ? { id: m[1], gen: Number(m[2]), ...(m[3] ? { sid: m[3] } : {}) } : null;
}

/** 줄 → 바깥에 보일 모양 (해시는 내보내지 않는다) */
export function view(row) {
  const [provider, ...rest] = row.subject.split(':');
  const name = rest.join(':');
  return {
    id: row.id, role: row.role, label: row.label,
    provider: provider === 'pw' ? 'password' : provider,
    ...(provider === 'pw' ? { login: name } : provider === 'google' ? { google: true, email: row.email || '' } : { email: name }), // google: 구글 번호는 내보내지 않고 확인된 이메일만 (쉬운 로그인)
    createdAt: row.created_at, lastLoginAt: row.last_login_at ?? null,
  };
}

// 쿠키가 살아 있는가 (auth.credentialAlive 가 'm:' 열쇠를 여기로 넘긴다). 30초 기억 — 기기 열쇠와 같은 규칙
const live = new Map();
const LIVE_MS = 30_000;
export function forgetMember(church, id) {
  for (const k of live.keys()) if (k.startsWith(`${church}/m:${id}:`)) live.delete(k);
}
/** 세션 하나를 끊었을 때 (이 Worker 인스턴스의 기억만 — 다른 인스턴스는 30초 안에) */
export function forgetSession(church, sid) {
  for (const k of live.keys()) if (k.startsWith(`${church}/m:`) && k.split('/')[1].endsWith(`:${sid}`)) live.delete(k);
}
export async function memberAlive(env, church, cred, role, now = Date.now()) {
  const p = parseCred(cred);
  if (!p) return false;
  const key = `${church}/${cred}/${role}`;
  const hit = live.get(key);
  if (hit && now - hit.at < LIVE_MS) return hit.ok;
  const row = await env.DB.prepare(
    'SELECT m.role AS role, m.gen AS gen, m.disabled_at AS disabled, h.disabled_at AS churchOff FROM members m JOIN churches h ON h.id = m.church_id WHERE m.church_id = ?1 AND m.id = ?2',
  ).bind(church, p.id).first();
  let ok = !!row && !row.disabled && !row.churchOff && row.gen === p.gen && SESSION_ROLE[row.role] === role;
  // 기기 세션이 붙은 열쇠면 그 세션이 살아 있어야 한다 (주인 화면의 '로그아웃시키기' · 로그아웃)
  if (ok && p.sid) {
    const ss = await env.DB.prepare('SELECT revoked_at AS revoked, expires_at AS exp FROM member_sessions WHERE church_id = ?1 AND id = ?2 AND member_id = ?3').bind(church, p.sid, p.id).first();
    ok = !!ss && !ss.revoked && ss.exp > now;
  }
  if (live.size > 5000) live.clear();
  live.set(key, { ok, at: now });
  return ok;
}

export async function listMembers(env, church) {
  const r = await env.DB.prepare('SELECT * FROM members WHERE church_id = ?1 AND disabled_at IS NULL ORDER BY created_at').bind(church).all();
  return r.results.map(view);
}
export async function memberById(env, church, id) {
  return env.DB.prepare('SELECT * FROM members WHERE church_id = ?1 AND id = ?2').bind(church, id).first();
}
export async function memberBySubject(env, church, subject) {
  return env.DB.prepare('SELECT * FROM members WHERE church_id = ?1 AND subject = ?2').bind(church, subject).first();
}

/** 만들기 (지웠던 같은 이름이 있으면 새 id · 새 세대로 되살린다) → 줄 | null(이미 있음) */
export async function createMember(env, church, { subject, role, label, secretHash }, now = Date.now()) {
  const old = await memberBySubject(env, church, subject);
  if (old && !old.disabled_at) return null;
  const id = newId();
  if (old) {
    await env.DB.prepare('UPDATE members SET id = ?3, role = ?4, label = ?5, secret_hash = ?6, gen = gen + 1, updated_at = ?7, disabled_at = NULL, last_login_at = NULL WHERE church_id = ?1 AND subject = ?2')
      .bind(church, subject, id, role, label, secretHash, now).run();
  } else {
    await env.DB.prepare('INSERT INTO members (church_id, subject, role, created_at, id, label, secret_hash, gen, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?4)')
      .bind(church, subject, role, now, id, label, secretHash).run();
  }
  return memberBySubject(env, church, subject);
}
