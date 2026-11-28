// 구글 OAuth · Drive v3 를 Worker 가 대신 부른다 (무료 등급 저장소). 계약: docs/web/tiers.md · 사관님 준비: docs/web/google-setup.md
// - 갱신 토큰(refresh token)은 봉인해(AES-GCM, 교회마다 다른 열쇠) D1 drive_accounts 에만 둔다. 브라우저 · 로그 · 응답에 내보내지 않는다.
// - 접근 토큰(1시간)은 Worker 메모리 + 봉인해 D1 에 기억한다 (Worker 인스턴스마다 다시 갱신하지 않게).
// - 범위는 drive.file (이 앱이 만든 파일만) + openid email. 구글의 '제한된 범위' 보안 심사를 피한다.
// - 시험은 가짜 구글(GOOGLE_FAKE=http://127.0.0.1:포트, 로컬 주소만 받음)로만 — test/fake-google/server.mjs.
import { sealSecret } from '../ai/seal.mjs';
import { DriveError, driveErrorOf } from './format.mjs';

export { DriveError };

const enc = new TextEncoder();
const b64u = b => btoa(String.fromCharCode(...new Uint8Array(b))).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
const unb64u = s => Uint8Array.from(atob(s.replace(/-/g, '+').replace(/_/g, '/')), c => c.charCodeAt(0));

export const SCOPES = {
  /** 가입 · 다시 연결: 드라이브(이 앱이 만든 파일만) + 누구인지 */
  drive: 'openid email https://www.googleapis.com/auth/drive.file',
  /** 로그인만: 누구인지 (드라이브 토큰은 바꾸지 않는다) */
  login: 'openid email',
};
export const DRIVE_FILE_SCOPE = 'https://www.googleapis.com/auth/drive.file';

/** 구글 주소들. GOOGLE_FAKE 는 로컬 주소(127.0.0.1 · localhost)일 때만 쓴다 — 배포에 잘못 들어가도 토큰이 다른 곳으로 가지 않게 */
export function endpoints(env) {
  const fake = typeof env.GOOGLE_FAKE === 'string' && /^http:\/\/(127\.0\.0\.1|localhost)(:\d+)?$/.test(env.GOOGLE_FAKE) ? env.GOOGLE_FAKE : '';
  if (fake) {
    return { auth: `${fake}/o/oauth2/v2/auth`, token: `${fake}/token`, revoke: `${fake}/revoke`, api: `${fake}/drive/v3`, upload: `${fake}/upload/drive/v3`, fake: true };
  }
  return {
    auth: 'https://accounts.google.com/o/oauth2/v2/auth', token: 'https://oauth2.googleapis.com/token', revoke: 'https://oauth2.googleapis.com/revoke',
    api: 'https://www.googleapis.com/drive/v3', upload: 'https://www.googleapis.com/upload/drive/v3', fake: false,
  };
}

/** 구글 연결이 설정되어 있나 (클라이언트 id · 비밀값 · 봉인 비밀값) */
export function googleConfigured(env) {
  return typeof env.GOOGLE_CLIENT_ID === 'string' && env.GOOGLE_CLIENT_ID.length > 4 && typeof env.GOOGLE_CLIENT_SECRET === 'string' && env.GOOGLE_CLIENT_SECRET.length > 4 && !!sealSecret(env);
}

// ---------- 봉인 (ai/seal.mjs 와 같은 방식, 목적 글 'wp-drive-token-v1') ----------
async function keyFor(env, church) {
  const base = await crypto.subtle.importKey('raw', enc.encode(sealSecret(env)), 'HKDF', false, ['deriveKey']);
  return crypto.subtle.deriveKey({ name: 'HKDF', hash: 'SHA-256', salt: enc.encode('wp-drive-token-v1'), info: enc.encode(`church:${church}`) }, base, { name: 'AES-GCM', length: 256 }, false, ['encrypt', 'decrypt']);
}
export async function sealToken(env, church, plain) {
  if (!sealSecret(env)) throw new Error('봉인 비밀값이 없습니다');
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const ct = await crypto.subtle.encrypt({ name: 'AES-GCM', iv, additionalData: enc.encode(church) }, await keyFor(env, church), enc.encode(plain));
  return `v1.${b64u(iv)}.${b64u(ct)}`;
}
export async function unsealToken(env, church, sealed) {
  if (typeof sealed !== 'string' || !sealed || !sealSecret(env)) return null;
  const [v, iv, ct, extra] = sealed.split('.');
  if (v !== 'v1' || !iv || !ct || extra !== undefined) return null;
  try {
    return new TextDecoder().decode(await crypto.subtle.decrypt({ name: 'AES-GCM', iv: unb64u(iv), additionalData: enc.encode(church) }, await keyFor(env, church), unb64u(ct)));
  } catch { return null; }
}

// ---------- 토큰 ----------
/** 구글 토큰 주소 부르기 (form) → JSON. 실패는 DriveError */
export async function tokenRequest(env, params) {
  const e = endpoints(env);
  let r;
  try {
    r = await fetch(e.token, { method: 'POST', headers: { 'content-type': 'application/x-www-form-urlencoded' }, body: new URLSearchParams(params).toString() });
  } catch { throw new DriveError('drive_unavailable', '구글에 연결하지 못했습니다.'); }
  const j = await r.json().catch(() => ({}));
  if (!r.ok) {
    if (j.error === 'invalid_grant') throw new DriveError('drive_disconnected', '구글 드라이브 연결이 끊어졌습니다. 주인이 다시 연결해 주세요.', { grant: true });
    throw new DriveError('drive_unavailable', '구글이 토큰을 주지 않았습니다.', { providerStatus: r.status });
  }
  return j;
}

/** id_token 안을 읽는다. 토큰 주소에서 https 로 바로 받은 것이므로 서명은 보지 않아도 된다 (구글 OpenID Connect 문서) — 다른 곳에서 받은 id_token 에는 쓰지 말 것 */
export function readIdToken(env, idToken, now = Date.now()) {
  if (typeof idToken !== 'string') return null;
  const parts = idToken.split('.');
  if (parts.length !== 3) return null;
  let p;
  try { p = JSON.parse(new TextDecoder().decode(unb64u(parts[1]))); } catch { return null; }
  const iss = endpoints(env).fake ? ['https://accounts.google.com', 'accounts.google.com', 'fake-google'] : ['https://accounts.google.com', 'accounts.google.com'];
  if (!iss.includes(p.iss) || p.aud !== env.GOOGLE_CLIENT_ID || typeof p.sub !== 'string' || !p.sub || !(p.exp * 1000 > now - 60_000)) return null;
  return { sub: p.sub, email: typeof p.email === 'string' ? p.email.toLowerCase() : '', emailVerified: p.email_verified === true || p.email_verified === 'true' };
}

const memo = new Map(); // 교회 → { token, exp, gen }
const SKEW = 120_000;
export function forgetToken(church) { memo.delete(church); }

export async function accountOf(env, church) {
  return env.DB.prepare('SELECT * FROM drive_accounts WHERE church_id = ?1').bind(church).first();
}

/** 연결이 끊긴 것으로 적는다 (토큰을 지우고 이유를 남김). 출력 화면은 중계실 메모리의 마지막 화면 그대로 */
export async function markDisconnected(env, church, reason, now = Date.now()) {
  forgetToken(church);
  await env.DB.prepare(`UPDATE drive_accounts SET refresh_sealed = '', access_sealed = '', access_exp = 0, disconnected_at = ?2, disconnect_reason = ?3, updated_at = ?2 WHERE church_id = ?1`)
    .bind(church, now, reason).run();
}

/** 교회의 접근 토큰 (필요하면 갱신). 끊겼으면 DriveError('drive_disconnected') */
export async function accessToken(env, church, { force = false } = {}) {
  const now = Date.now();
  const hit = memo.get(church);
  if (!force && hit && hit.exp - SKEW > now) return hit.token;
  const acct = await accountOf(env, church);
  if (!acct) throw new DriveError('drive_disconnected', '구글 드라이브가 연결되어 있지 않습니다. 주인이 연결해 주세요.');
  if (acct.disconnected_at || !acct.refresh_sealed) throw new DriveError('drive_disconnected', '구글 드라이브 연결이 끊어졌습니다. 주인이 다시 연결해 주세요.', { reason: acct.disconnect_reason || null });
  if (!force && acct.access_sealed && acct.access_exp - SKEW > now) {
    const t = await unsealToken(env, church, acct.access_sealed);
    if (t) { memo.set(church, { token: t, exp: acct.access_exp }); return t; }
  }
  const refresh = await unsealToken(env, church, acct.refresh_sealed);
  if (!refresh) throw new DriveError('drive_disconnected', '봉인한 구글 토큰을 풀지 못했습니다(비밀값이 바뀜). 주인이 다시 연결해 주세요.', { reason: 'unseal' });
  let j;
  try {
    j = await tokenRequest(env, { grant_type: 'refresh_token', refresh_token: refresh, client_id: env.GOOGLE_CLIENT_ID, client_secret: env.GOOGLE_CLIENT_SECRET });
  } catch (e) {
    if (e.grant) await markDisconnected(env, church, 'invalid_grant');
    throw e;
  }
  const exp = now + Math.max(60, Number(j.expires_in) || 3600) * 1000;
  memo.set(church, { token: j.access_token, exp });
  if (memo.size > 2000) memo.clear();
  // 다른 Worker 인스턴스도 쓰게 봉인해 둔다 (구글이 새 갱신 토큰을 주면 그것으로 바꾼다)
  const sets = ['access_sealed = ?2', 'access_exp = ?3', 'updated_at = ?4'];
  const args = [church, await sealToken(env, church, j.access_token), exp, now];
  if (j.refresh_token) { args.push(await sealToken(env, church, j.refresh_token)); sets.push(`refresh_sealed = ?${args.length}`); }
  await env.DB.prepare(`UPDATE drive_accounts SET ${sets.join(', ')} WHERE church_id = ?1`).bind(...args).run();
  return j.access_token;
}

/**
 * 드라이브 부르기. path = '/files/…'(api) 또는 'upload:/files/…'(upload) 또는 'https://…'(이어 올리기 주소).
 * 401 이면 한 번 토큰을 새로 받아 다시 부른다. 오류는 DriveError(code) — format.mjs driveErrorOf.
 * raw:true 면 Response 를 그대로 (받기 · Range 를 흘려 보낼 때).
 */
export async function drive(env, church, path, init = {}, { raw = false, retry = true } = {}) {
  const e = endpoints(env);
  const url = path.startsWith('upload:') ? e.upload + path.slice(7) : /^https?:\/\//.test(path) ? path : e.api + path;
  if (/^https?:\/\//.test(path) && !path.startsWith(e.upload) && !path.startsWith(e.api)) throw new DriveError('drive_unavailable', '모르는 드라이브 주소입니다.');
  const token = await accessToken(env, church);
  const headers = new Headers(init.headers || {});
  headers.set('authorization', `Bearer ${token}`);
  let r;
  try { r = await fetch(url, { ...init, headers }); } catch { throw new DriveError('drive_unavailable', '구글 드라이브에 연결하지 못했습니다.'); }
  if (r.status === 401 && retry && !(init.body instanceof ReadableStream)) {
    forgetToken(church);
    await accessToken(env, church, { force: true });
    return drive(env, church, path, init, { raw, retry: false });
  }
  if (r.ok || (raw && (r.status === 206 || r.status === 304 || r.status === 416))) return raw ? r : r.status === 204 ? null : r.json().catch(() => ({}));
  const body = await r.json().catch(() => ({}));
  const err = driveErrorOf(r.status, body);
  if (err.code === 'drive_disconnected') await markDisconnected(env, church, 'auth');
  throw err;
}

/** 폴더 하나 만들기 → id */
export async function createFolder(env, church, name, parent) {
  const f = await drive(env, church, '/files?fields=id', {
    method: 'POST', headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ name, mimeType: 'application/vnd.google-apps.folder', ...(parent ? { parents: [parent] } : {}), appProperties: { wp: '1' } }),
  });
  return f.id;
}
