// 교회 AI 키 봉인: AES-GCM 256. 열쇠는 Worker 비밀값(AI_SECRET, 없으면 AUTH_SECRET)에서 교회마다 HKDF 로 따로 만들고,
// 교회 ID 를 덧붙임 자료(AAD)로 묶는다 → DB 만 새어도 키가 안 보이고, 한 교회 줄을 다른 교회 줄로 옮겨도 풀리지 않는다.
// 키 원문은 브라우저 · 로그 · 응답에 내보내지 않는다 (끝 4자리 힌트만).
const enc = new TextEncoder();
const b64u = b => btoa(String.fromCharCode(...new Uint8Array(b))).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
const unb64u = s => Uint8Array.from(atob(s.replace(/-/g, '+').replace(/_/g, '/')), c => c.charCodeAt(0));

export function sealSecret(env) {
  const s = typeof env.AI_SECRET === 'string' && env.AI_SECRET.length >= 32 ? env.AI_SECRET : env.AUTH_SECRET;
  return typeof s === 'string' && s.length >= 32 ? s : '';
}

async function keyFor(env, church) {
  const base = await crypto.subtle.importKey('raw', enc.encode(sealSecret(env)), 'HKDF', false, ['deriveKey']);
  return crypto.subtle.deriveKey({ name: 'HKDF', hash: 'SHA-256', salt: enc.encode('wp-ai-key-v1'), info: enc.encode(`church:${church}`) }, base, { name: 'AES-GCM', length: 256 }, false, ['encrypt', 'decrypt']);
}

/** 키 원문 → 'v1.<iv>.<암호문>' */
export async function seal(env, church, plain) {
  if (!sealSecret(env)) throw new Error('봉인 비밀값이 없습니다');
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const ct = await crypto.subtle.encrypt({ name: 'AES-GCM', iv, additionalData: enc.encode(church) }, await keyFor(env, church), enc.encode(plain));
  return `v1.${b64u(iv)}.${b64u(ct)}`;
}

/** 'v1.…' → 키 원문 (못 풀면 null) */
export async function unseal(env, church, sealed) {
  if (typeof sealed !== 'string' || !sealSecret(env)) return null;
  const [v, iv, ct, extra] = sealed.split('.');
  if (v !== 'v1' || !iv || !ct || extra !== undefined) return null;
  try {
    const pt = await crypto.subtle.decrypt({ name: 'AES-GCM', iv: unb64u(iv), additionalData: enc.encode(church) }, await keyFor(env, church), unb64u(ct));
    return new TextDecoder().decode(pt);
  } catch {
    return null;
  }
}

/** 화면에 보일 힌트: 끝 4자리만 */
export const keyHint = k => (typeof k === 'string' && k.length >= 8 ? `…${k.slice(-4)}` : '');
