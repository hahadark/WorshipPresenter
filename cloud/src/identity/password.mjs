// 사람 계정 비밀번호: PBKDF2-SHA256(HMAC(AUTH_SECRET, 교회:subject:비밀번호), 소금 16바이트, 반복 N).
// - AUTH_SECRET 을 섞으므로(pepper) D1 만 새어 나가서는 비밀번호를 맞혀 볼 수 없다.
// - 반복 수는 저장 글에 같이 적는다('p1$반복$소금$해시') → 나중에 올려도 옛 해시를 읽는다.
// - Workers 무료 플랜은 요청당 CPU 10 ms 라 반복을 크게 잡지 못한다. 20,000 번 ≈ 몇 ms (로그인 · 비밀번호 바꾸기에만 쓴다).
//   workerd 의 PBKDF2 는 100,000 번까지만 받는다.
const enc = new TextEncoder();
export const ITERATIONS = 20_000;
export const MIN_LENGTH = 10;

const b64u = bytes => btoa(String.fromCharCode(...new Uint8Array(bytes))).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
const unb64u = s => Uint8Array.from(atob(s.replace(/-/g, '+').replace(/_/g, '/')), c => c.charCodeAt(0));

async function peppered(secret, church, subject, password) {
  const key = await crypto.subtle.importKey('raw', enc.encode(secret), { name: 'HMAC', hash: 'SHA-256' }, false, ['sign']);
  return crypto.subtle.sign('HMAC', key, enc.encode(`member:${church}:${subject}:${password}`));
}
async function derive(material, salt, iterations) {
  const key = await crypto.subtle.importKey('raw', material, 'PBKDF2', false, ['deriveBits']);
  return crypto.subtle.deriveBits({ name: 'PBKDF2', hash: 'SHA-256', salt, iterations }, key, 256);
}

export async function hashPassword(secret, church, subject, password, iterations = ITERATIONS) {
  const salt = crypto.getRandomValues(new Uint8Array(16));
  const bits = await derive(await peppered(secret, church, subject, password), salt, iterations);
  return `p1$${iterations}$${b64u(salt)}$${b64u(bits)}`;
}

export async function checkPassword(secret, church, subject, password, stored) {
  const [v, it, salt, hash] = String(stored || '').split('$');
  const iterations = Number(it);
  if (v !== 'p1' || !(iterations >= 1000 && iterations <= 100_000) || !salt || !hash) return false;
  const bits = b64u(await derive(await peppered(secret, church, subject, password), unb64u(salt), iterations));
  if (bits.length !== hash.length) return false;
  let d = 0;
  for (let i = 0; i < bits.length; i++) d |= bits.charCodeAt(i) ^ hash.charCodeAt(i);
  return d === 0;
}

/** 너무 쉬운 비밀번호를 막는다 (길이 · 같은 글자 반복 · 로그인 이름 그대로) */
export function weakPassword(password, login) {
  if (typeof password !== 'string' || password.length < MIN_LENGTH) return `비밀번호는 ${MIN_LENGTH}자 이상이어야 합니다.`;
  if (password.length > 200) return '비밀번호가 너무 깁니다.';
  if (new Set(password).size < 4) return '비밀번호에 여러 글자를 섞어 주세요.';
  if (login && password.toLowerCase().includes(String(login).toLowerCase())) return '비밀번호에 로그인 이름을 넣지 마세요.';
  return '';
}

/** 처음 비밀번호 (주인이 만들 때 한 번 보여 준다): 헷갈리는 글자를 뺀 16자 */
export function newPassword() {
  const A = 'ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789';
  const out = [];
  while (out.length < 16) for (const b of crypto.getRandomValues(new Uint8Array(32))) if (b < 228 && out.length < 16) out.push(A[b % 57]);
  return out.join('');
}
