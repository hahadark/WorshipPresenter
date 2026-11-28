// 4단계 B 미디어 — 길 · 휴대폰 카메라 링크 · 캐시한 출력 자료의 시계 맞추기 · 이름 맞추기 (Cloudflare 전용 API 없음 — node 시험과 같이 씀)
// 계약: docs/web/media.md
//
// 길 (주소 '/media/…' 는 1단계 설치판 파일 중계가 쓰므로 피한다):
//   GET  /phonecam?k=<링크>        휴대폰 카메라 페이지 (web/phone/cam.html 그대로 + 클라우드 연결 부품). 로그인 대신 서명한 링크
//   WS   /rtc/phone?k=<링크>       휴대폰 ↔ 교회 중계실 (신호만, 영상은 휴대폰 → 출력 화면 직접)
//   GET  /rtc/phones               붙어 있는 휴대폰 목록 (view)
//   POST /api/v1/media/phone-link  휴대폰 링크 만들기 {name?} → {url, expiresAt} (write: 주인 · 사람 편집자 · 설치판)
//   GET  /cameras                  이 PC 의 카메라를 교회 카메라 이름에 맞추는 화면 (view)

export const PHONE = {
  /** 휴대폰 링크 유효 시간 (예배 하루) */
  linkTtlMs: 24 * 3600_000,
  /** 한 교회에 붙을 수 있는 휴대폰 */
  maxPhones: 8,
  /** 휴대폰이 상태를 알리는 간격 (클라우드에서는 0.7초 폴링 대신 — 요청 수) */
  helloMs: 5000,
  /** SDP 한 개 */
  maxSdp: 60_000,
};

/** 공개된 무료 STUN (https://developers.cloudflare.com/realtime/turn/faq/ "free and unlimited"). TURN 은 쓰지 않는다 (docs/web/media.md 3절) */
export const ICE_SERVERS = [{ urls: 'stun:stun.cloudflare.com:3478' }];

export function mediaRoute(method, path) {
  const get = method === 'GET' || method === 'HEAD';
  if (get && path === '/rtc/phones') return { kind: 'room', cap: 'view' };
  if (get && (path === '/cameras' || path === '/cameras/')) return { kind: 'page', cap: 'view', page: 'cameras' };
  if (method === 'POST' && path === '/api/v1/media/phone-link') return { kind: 'media-api', cap: 'write', op: 'link' };
  return null;
}

/** 로그인 대신 링크로 여는 길 (index.mjs 가 쿠키를 보기 전에 묻는다) */
export const isPhonePath = path => path === '/phonecam' || path === '/rtc/phone';

const enc = new TextEncoder();
const b64u = b => btoa(String.fromCharCode(...new Uint8Array(b))).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
const unb64u = s => Uint8Array.from(atob(s.replace(/-/g, '+').replace(/_/g, '/')), c => c.charCodeAt(0));
async function mac(secret, text) {
  const k = await crypto.subtle.importKey('raw', enc.encode(secret), { name: 'HMAC', hash: 'SHA-256' }, false, ['sign']);
  return b64u(await crypto.subtle.sign('HMAC', k, enc.encode(`phone-link:${text}`)));
}
function same(a, b) {
  if (typeof a !== 'string' || typeof b !== 'string' || a.length !== b.length) return false;
  let d = 0;
  for (let i = 0; i < a.length; i++) d |= a.charCodeAt(i) ^ b.charCodeAt(i);
  return d === 0;
}

/** 휴대폰 링크: 'ph1.<{c 교회, e 만료, n 무작위, l 이름}>.<HMAC>' — 휴대폰에 로그인 열쇠를 주지 않는다 */
export async function makePhoneLink(secret, church, name = '', now = Date.now()) {
  const n = b64u(crypto.getRandomValues(new Uint8Array(9)));
  const body = b64u(enc.encode(JSON.stringify({ c: church, e: now + PHONE.linkTtlMs, n, l: String(name).slice(0, 40) })));
  return { token: `ph1.${body}.${await mac(secret, body)}`, expiresAt: now + PHONE.linkTtlMs };
}
export async function readPhoneLink(secret, token, now = Date.now()) {
  if (typeof token !== 'string' || token.length > 400 || typeof secret !== 'string' || secret.length < 32) return null;
  const [v, body, sig, extra] = token.split('.');
  if (v !== 'ph1' || !body || !sig || extra !== undefined) return null;
  if (!same(sig, await mac(secret, body))) return null;
  try {
    const p = JSON.parse(new TextDecoder().decode(unb64u(body)));
    if (typeof p.c !== 'string' || !(p.e > now)) return null;
    return { church: p.c, exp: p.e, name: String(p.l || '') };
  } catch {
    return null;
  }
}

/** 휴대폰 · 보는 화면 id (설치판 phonecam.rs ok_id 와 같은 모양) */
export const okId = s => typeof s === 'string' && /^[A-Za-z0-9_-]{4,64}$/.test(s);

/**
 * 중계실이 기억해 둔 출력 자료를 늦게 붙은 화면에 줄 때: 만든 때의 시계(now)를 그만큼 앞으로.
 * render.js syncVideo · 무대 타이머는 'now' 와 받은 때의 차이를 두 PC 의 시계 차이로 보므로, 오래된 자료를 그대로 주면 영상 위치 · 남은 시간이 틀린다.
 * (설치판 로컬 SSE 는 붙을 때마다 새로 만들어 이 문제가 없다)
 */
export function freshen(text, ageMs) {
  if (!(ageMs > 250) || typeof text !== 'string' || !/"now":/.test(text)) return text;
  let d;
  try { d = JSON.parse(text); } catch { return text; }
  let changed = false;
  const bump = o => { if (o && typeof o === 'object' && typeof o.now === 'number') { o.now += ageMs; changed = true; } };
  bump(d.video);
  bump(d.timer);
  bump(d.stage && d.stage.timer);
  return changed ? JSON.stringify(d) : text;
}

/** 카메라 · 휴대폰 이름 맞추기: 소문자 · 공백 정리 · 끝의 '(046d:0825)' 같은 장치 번호 빼기 */
export function normLabel(s) {
  return String(s || '').toLowerCase().replace(/\s*\([0-9a-f]{4}:[0-9a-f]{4}\)\s*$/i, '').replace(/\s+/g, ' ').trim();
}
/** 교회 카메라 이름 → 이 PC 장치 (정확히 → 정리해서 같음 → 한쪽이 다른 쪽을 품음). 없으면 null */
export function matchDevice(label, devices) {
  const want = normLabel(label);
  if (!want) return null;
  const vids = devices.filter(d => d && d.kind === 'videoinput' && d.label);
  return vids.find(d => d.label === label) || vids.find(d => normLabel(d.label) === want)
    || vids.find(d => normLabel(d.label).includes(want) || want.includes(normLabel(d.label))) || null;
}
