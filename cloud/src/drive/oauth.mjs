// '구글로 시작하기' (무료 가입) · 구글로 로그인 · 드라이브 다시 연결 · 연결 끊기. 계약: docs/web/tiers.md 3절 · 준비: docs/web/google-setup.md
//   GET /auth/google/start?mode=signup&name=<교회 이름>[&church=<원하는 ID>][&hint=<전자우편>]   → 구글 동의 화면으로 302
//   GET /auth/google/start?mode=login[&church=]                                                    → 로그인만 (드라이브 토큰은 건드리지 않음)
//   GET /auth/google/start?mode=connect                                                            → (주인 로그인) 드라이브 다시 연결
//   GET /auth/google/callback?code=&state=                                                         → 쿠키 + 302
// - PKCE(S256) + state. 둘은 서명한 쿠키(wp_oauth, 10분, HttpOnly)에 — 서버에 따로 저장하지 않는다.
// - 토큰 · 전자우편은 로그에 남기지 않는다. 실패는 /login?google=<까닭> 으로 돌려보낸다.
import * as auth from '../auth.mjs';
import { HOME, SESSION_TTL, fail, json, validChurch } from '../protocol.mjs';
import { credOf, SESSION_ROLE } from '../identity/members.mjs';
import { dayOf, serviceLimits } from '../tiers/limits.mjs';
import { forgetTier, RESERVED_CHURCHES } from '../tiers/index.mjs';
import { DRIVE_FILE_SCOPE, SCOPES, endpoints, forgetToken, googleConfigured, readIdToken, sealToken, tokenRequest, accountOf, markDisconnected, unsealToken } from './google.mjs';
import { churchFolder, forgetFolders } from './folders.mjs';
import { googleLogin, validInviteToken } from '../identity/login.mjs';

const enc = new TextEncoder();
const b64u = b => btoa(String.fromCharCode(...new Uint8Array(b))).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
const unb64u = s => Uint8Array.from(atob(s.replace(/-/g, '+').replace(/_/g, '/')), c => c.charCodeAt(0));
const OAUTH_COOKIE = 'wp_oauth';
const ALPHA = 'abcdefghijkmnopqrstuvwxyz23456789';
const rand = (n, a = 'ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789') => Array.from(crypto.getRandomValues(new Uint8Array(n)), b => a[b % a.length]).join('');
const ipOf = request => request.headers.get('cf-connecting-ip') || 'local';

async function hmac(env, text) {
  const key = await crypto.subtle.importKey('raw', enc.encode(env.AUTH_SECRET), { name: 'HMAC', hash: 'SHA-256' }, false, ['sign']);
  return b64u(await crypto.subtle.sign('HMAC', key, enc.encode(text)));
}
export async function packState(env, obj) {
  const body = b64u(enc.encode(JSON.stringify(obj)));
  return `v1.${body}.${await hmac(env, `oauth:${body}`)}`;
}
export async function unpackState(env, text, now = Date.now()) {
  if (typeof text !== 'string' || text.length > 2000) return null;
  const [v, body, sig, extra] = text.split('.');
  if (v !== 'v1' || !body || !sig || extra !== undefined || !auth.safeEqual(sig, await hmac(env, `oauth:${body}`))) return null;
  try { const o = JSON.parse(new TextDecoder().decode(unb64u(body))); return o.e > now ? o : null; } catch { return null; }
}
async function challenge(verifier) {
  return b64u(await crypto.subtle.digest('SHA-256', enc.encode(verifier)));
}
const back = (url, why) => Response.redirect(`${url.origin}/login?google=${encodeURIComponent(why)}`, 302);
const redirectUri = (env, url) => (typeof env.GOOGLE_REDIRECT_URI === 'string' && env.GOOGLE_REDIRECT_URI ? env.GOOGLE_REDIRECT_URI : `${url.origin}/auth/google/callback`);
const safeNext = n => (typeof n === 'string' && n.startsWith('/') && !n.startsWith('//') && !n.includes('\\') ? n.slice(0, 300) : '');

// 가입 한도 (주소마다 한 시간) — Worker 메모리 어림 + 하루 전체는 D1 로
const signupIps = new Map();

export async function publicRoute(request, env, url) {
  if (url.pathname === '/auth/google/start' && request.method === 'GET') return start(request, env, url);
  if (url.pathname === '/auth/google/callback' && request.method === 'GET') return callback(request, env, url);
  return null;
}

async function start(request, env, url) {
  if (!auth.configured(env) || !googleConfigured(env)) return fail(404, '구글 연결이 아직 설정되지 않았습니다 (docs/web/google-setup.md).');
  const q = url.searchParams;
  const mode = q.get('mode') || 'login';
  if (!['signup', 'login', 'connect'].includes(mode)) return fail(400, 'mode 는 signup · login · connect 중 하나입니다.');
  const st = { m: mode, s: rand(24), v: rand(64), e: Date.now() + 600_000, n: safeNext(q.get('next')) };
  if (mode === 'signup') {
    const name = String(q.get('name') || '').normalize('NFC').replace(/[\u0000-\u001f\u007f<>]/g, '').trim();
    if (name.length < 2 || name.length > 60) return back(url, 'name');
    const want = String(q.get('church') || '').trim().toLowerCase();
    if (want && (!validChurch(want) || RESERVED_CHURCHES.includes(want))) return back(url, 'church-id');
    st.name = name; st.c = want;
  }
  if (mode === 'connect') {
    const s = await auth.sessionOf(request, env);
    if (!s || s.role !== 'owner') return back(url, 'owner');
    st.c = s.church;
  }
  if (mode === 'login') {
    const c = q.get('church'); if (validChurch(c)) st.c = c;
    // 쉬운 로그인: 이 기기 기억(기본 켬) · 받은 초대 링크 (identity/login.mjs googleLogin)
    st.r = q.get('remember') === '0' ? 0 : 1;
    const inv = q.get('invite'); if (validInviteToken(inv)) st.i = inv;
  }
  const e = endpoints(env);
  const p = new URLSearchParams({
    client_id: env.GOOGLE_CLIENT_ID, redirect_uri: redirectUri(env, url), response_type: 'code', state: st.s,
    scope: mode === 'login' ? SCOPES.login : SCOPES.drive, code_challenge: await challenge(st.v), code_challenge_method: 'S256',
    include_granted_scopes: 'false',
  });
  if (mode === 'login') p.set('prompt', 'select_account');
  else { p.set('access_type', 'offline'); p.set('prompt', 'consent'); } // 갱신 토큰을 늘 새로 받는다
  const hint = q.get('hint');
  if (hint && hint.length < 200) p.set('login_hint', hint);
  const cookie = `${OAUTH_COOKIE}=${await packState(env, st)}; Path=/auth/google; Max-Age=600; SameSite=Lax; HttpOnly${url.protocol === 'https:' ? '; Secure' : ''}`;
  return new Response(null, { status: 302, headers: { location: `${e.auth}?${p}`, 'set-cookie': cookie, 'cache-control': 'no-store' } });
}

async function ownerCookies(env, request, church, m) {
  const role = SESSION_ROLE[m.role];
  const { token } = await auth.issueToken(env, { church, role, cred: credOf(m) });
  return [auth.setCookie(auth.COOKIE, token, request, SESSION_TTL[role]), auth.setCookie(auth.CHURCH_COOKIE, church, request, 400 * 86400, false)];
}
function done(url, to, cookies) {
  const h = new Headers({ location: `${url.origin}${to}`, 'cache-control': 'no-store' });
  for (const c of cookies) h.append('set-cookie', c);
  h.append('set-cookie', `${OAUTH_COOKIE}=; Path=/auth/google; Max-Age=0; SameSite=Lax; HttpOnly${url.protocol === 'https:' ? '; Secure' : ''}`);
  return new Response(null, { status: 302, headers: h });
}

async function callback(request, env, url) {
  if (!auth.configured(env) || !googleConfigured(env)) return fail(404, '구글 연결이 아직 설정되지 않았습니다.');
  const st = await unpackState(env, auth.cookies(request)[OAUTH_COOKIE]);
  const q = url.searchParams;
  if (!st || !q.get('state') || !auth.safeEqual(q.get('state'), st.s)) return back(url, 'state');
  if (q.get('error')) return back(url, q.get('error') === 'access_denied' ? 'denied' : 'google');
  let tok;
  try {
    tok = await tokenRequest(env, { grant_type: 'authorization_code', code: q.get('code') || '', client_id: env.GOOGLE_CLIENT_ID, client_secret: env.GOOGLE_CLIENT_SECRET, redirect_uri: redirectUri(env, url), code_verifier: st.v });
  } catch { return back(url, 'token'); }
  const who = readIdToken(env, tok.id_token);
  if (!who) return back(url, 'token');
  const subject = `google:${who.sub}`;
  const now = Date.now();
  if (st.m === 'login') {
    // 쉬운 로그인: 구글 번호로 된 계정 + 이 이메일로 온 초대 + 받은 초대 링크 → 교회가 하나면 바로, 여럿이면 고르기
    return googleLogin(env, request, url, st, who, [`${OAUTH_COOKIE}=; Path=/auth/google; Max-Age=0; SameSite=Lax; HttpOnly${url.protocol === 'https:' ? '; Secure' : ''}`]);
  }
  // 가입 · 다시 연결은 drive.file 이 꼭 있어야 한다 (구글 동의 화면에서 사람이 칸을 끌 수 있다)
  if (!String(tok.scope || '').split(/\s+/).includes(DRIVE_FILE_SCOPE)) return back(url, 'scope');
  if (!tok.refresh_token) return back(url, 'refresh');
  if (st.m === 'connect') return connect(env, request, url, st, tok, who, now);
  return signup(env, request, url, st, tok, who, now, subject);
}

async function saveAccount(env, church, tok, who, now) {
  const refresh = await sealToken(env, church, tok.refresh_token);
  const access = await sealToken(env, church, tok.access_token);
  const exp = now + Math.max(60, Number(tok.expires_in) || 3600) * 1000;
  await env.DB.prepare(
    `INSERT INTO drive_accounts (church_id, google_sub, email, refresh_sealed, access_sealed, access_exp, scope, connected_at, updated_at)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)
     ON CONFLICT (church_id) DO UPDATE SET google_sub = excluded.google_sub, email = excluded.email, refresh_sealed = excluded.refresh_sealed,
       access_sealed = excluded.access_sealed, access_exp = excluded.access_exp, scope = excluded.scope, connected_at = excluded.connected_at,
       disconnected_at = NULL, disconnect_reason = NULL, updated_at = excluded.updated_at`,
  ).bind(church, who.sub, who.email, refresh, access, exp, String(tok.scope || '').slice(0, 400), now).run();
  forgetToken(church); forgetFolders(church);
}

async function connect(env, request, url, st, tok, who, now) {
  const a = await accountOf(env, st.c);
  // 다른 구글 계정의 드라이브로는 앞서 만든 파일이 보이지 않는다 (drive.file) → 막는다
  if (a && a.google_sub !== who.sub) return back(url, 'other-account');
  await saveAccount(env, st.c, tok, who, now);
  return done(url, st.n || '/owner', []);
}

async function signup(env, request, url, st, tok, who, now, subject) {
  const SERVICE_LIMITS = serviceLimits(env);
  // 한 구글 계정에 무료 교회 하나 — 이미 있으면 그 교회로 들어간다
  const mine = await env.DB.prepare("SELECT id FROM churches WHERE signup_by = ?1 AND disabled_at IS NULL LIMIT ?2").bind(subject, SERVICE_LIMITS.freeChurchesPerGoogleAccount).all();
  if (mine.results.length >= SERVICE_LIMITS.freeChurchesPerGoogleAccount) {
    const church = mine.results[0].id;
    const m = await env.DB.prepare('SELECT * FROM members WHERE church_id = ?1 AND subject = ?2 AND disabled_at IS NULL').bind(church, subject).first();
    if (!m) return back(url, 'exists');
    return done(url, '/owner?google=exists', await ownerCookies(env, request, church, m));
  }
  const ip = ipOf(request);
  const hr = Math.floor(now / 3600_000);
  const k = `${ip}/${hr}`;
  if ((signupIps.get(k) || 0) >= SERVICE_LIMITS.signupsPerIpPerHour) return back(url, 'too-many');
  const today = (await env.DB.prepare("SELECT COUNT(*) AS n FROM churches WHERE tier = 'free' AND created_at >= ?1").bind(Date.parse(dayOf(now))).first())?.n || 0;
  if (today >= SERVICE_LIMITS.freeSignupsPerDay) return back(url, 'full');
  let church = st.c;
  for (let i = 0; i < 5; i++) {
    if (!church) church = `c-${rand(8, ALPHA)}`;
    const r = await env.DB.prepare("INSERT INTO churches (id, name, created_at, tier, tier_changed_at, signup_by) VALUES (?1, ?2, ?3, 'free', ?3, ?4) ON CONFLICT (id) DO NOTHING").bind(church, st.name, now, subject).run();
    if (r.meta.changes) break;
    if (st.c) return back(url, 'church-taken');
    church = '';
  }
  if (!church) return back(url, 'google');
  if (signupIps.size > 5000) signupIps.clear();
  signupIps.set(k, (signupIps.get(k) || 0) + 1);
  const id = auth.newId();
  await env.DB.prepare("INSERT INTO members (church_id, subject, role, created_at, id, label, secret_hash, gen, updated_at) VALUES (?1, ?2, 'owner', ?3, ?4, ?5, NULL, 1, ?3)")
    .bind(church, subject, now, id, who.email.split('@')[0].slice(0, 60) || '주인').run();
  await saveAccount(env, church, tok, who, now);
  forgetTier(church);
  // 드라이브 폴더를 바로 만든다 (실패해도 가입은 됨 — 처음 쓸 때 다시 만든다)
  await churchFolder(env, church).catch(() => null);
  const m = await env.DB.prepare('SELECT * FROM members WHERE church_id = ?1 AND id = ?2').bind(church, id).first();
  return done(url, '/owner?google=welcome', await ownerCookies(env, request, church, m));
}

// ---------- 로그인 뒤: 연결 상태 · 끊기 ----------
export async function status(request, env, url, s, canManage) {
  const a = await accountOf(env, s.church);
  if (!a) return json({ ok: true, connected: false, configured: googleConfigured(env), reconnect: canManage ? '/auth/google/start?mode=connect' : null });
  const mask = e => (e ? e.replace(/^(.{2}).*(@.*)$/, '$1…$2') : '');
  const out = {
    ok: true, configured: googleConfigured(env), connected: !a.disconnected_at && !!a.refresh_sealed,
    email: canManage ? a.email : mask(a.email), connectedAt: a.connected_at, disconnectedAt: a.disconnected_at ?? null, reason: a.disconnect_reason ?? null,
    folderUrl: a.church_folder_id ? `https://drive.google.com/drive/folders/${encodeURIComponent(a.church_folder_id)}` : null,
    reconnect: canManage ? '/auth/google/start?mode=connect' : null,
  };
  if (url.searchParams.get('quota') === '1' && out.connected) {
    const { drive } = await import('./google.mjs');
    const ab = await drive(env, s.church, '/about?fields=storageQuota').catch(() => null);
    if (ab?.storageQuota) out.quota = { limit: Number(ab.storageQuota.limit) || null, usage: Number(ab.storageQuota.usage) || 0 };
  }
  return json(out);
}

export async function disconnect(request, env, url, s) {
  const a = await accountOf(env, s.church);
  if (!a) return fail(404, '연결된 구글 드라이브가 없습니다.');
  const t = await unsealToken(env, s.church, a.refresh_sealed);
  if (t) await fetch(`${endpoints(env).revoke}?token=${encodeURIComponent(t)}`, { method: 'POST', headers: { 'content-type': 'application/x-www-form-urlencoded' } }).catch(() => null);
  await markDisconnected(env, s.church, 'owner');
  return json({ ok: true, connected: false, note: '드라이브의 파일은 그대로 있습니다. 다시 연결하면 이어서 씁니다.' });
}
