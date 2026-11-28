// Cloudflare Access (Zero Trust, 무료 50명) 로 들어온 사람 확인 — 고를 수 있는 두 번째 로그인 길.
// 켜는 법(사관님이 계정에서 정할 일): Access 앱을 /auth/access 에 걸고, Worker 변수 ACCESS_TEAM_DOMAIN(예: myteam.cloudflareaccess.com) ·
// ACCESS_AUD(앱의 Audience 태그)를 넣는다. 둘 중 하나라도 없으면 이 길은 닫혀 있다(404).
// Access 가 붙여 보내는 Cf-Access-Jwt-Assertion(RS256) 을 팀의 공개 열쇠(/cdn-cgi/access/certs)로 확인하고 email 을 꺼낸다.
// 주인이 미리 members 에 'access:<전자우편>' 으로 넣어 둔 사람만 들어온다.
const dec = new TextDecoder();
const unb64u = s => Uint8Array.from(atob(s.replace(/-/g, '+').replace(/_/g, '/')), c => c.charCodeAt(0));
const parse = s => JSON.parse(dec.decode(unb64u(s)));

export function accessConfigured(env) {
  return typeof env.ACCESS_TEAM_DOMAIN === 'string' && /^[a-z0-9-]+\.cloudflareaccess\.com$/.test(env.ACCESS_TEAM_DOMAIN) && typeof env.ACCESS_AUD === 'string' && env.ACCESS_AUD.length >= 16;
}

let certs = { at: 0, team: '', keys: [] };
async function teamKeys(team, fetcher) {
  if (certs.team === team && Date.now() - certs.at < 3600_000 && certs.keys.length) return certs.keys;
  const r = await fetcher(`https://${team}/cdn-cgi/access/certs`);
  if (!r.ok) throw new Error(`Access 공개 열쇠를 받지 못함 ${r.status}`);
  const j = await r.json();
  certs = { at: Date.now(), team, keys: Array.isArray(j.keys) ? j.keys : [] };
  return certs.keys;
}

/**
 * JWT 확인 → { email, sub } | null. opts: { team, aud, now?, keys?(시험용 JWK 목록), fetcher? }
 */
export async function verifyAccessJwt(token, opts) {
  if (typeof token !== 'string' || token.length > 8192) return null;
  const parts = token.split('.');
  if (parts.length !== 3) return null;
  let head, body;
  try { head = parse(parts[0]); body = parse(parts[1]); } catch { return null; }
  if (head.alg !== 'RS256' || typeof head.kid !== 'string') return null;
  const keys = opts.keys || (await teamKeys(opts.team, opts.fetcher || fetch));
  const jwk = keys.find(k => k.kid === head.kid && k.kty === 'RSA');
  if (!jwk) return null;
  const key = await crypto.subtle.importKey('jwk', { kty: 'RSA', n: jwk.n, e: jwk.e, alg: 'RS256', ext: true }, { name: 'RSASSA-PKCS1-v1_5', hash: 'SHA-256' }, false, ['verify']);
  const ok = await crypto.subtle.verify('RSASSA-PKCS1-v1_5', key, unb64u(parts[2]), new TextEncoder().encode(`${parts[0]}.${parts[1]}`));
  if (!ok) return null;
  const now = Math.floor((opts.now ?? Date.now()) / 1000);
  const aud = Array.isArray(body.aud) ? body.aud : [body.aud];
  if (!aud.includes(opts.aud)) return null;
  if (body.iss !== `https://${opts.team}`) return null;
  if (!(body.exp > now) || (body.nbf && body.nbf > now + 60)) return null;
  if (typeof body.email !== 'string' || !body.email.includes('@')) return null;
  return { email: body.email.toLowerCase(), sub: String(body.sub || '') };
}
