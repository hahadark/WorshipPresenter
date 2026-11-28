// 4단계 B 미디어 — Worker 쪽 (휴대폰 링크 · 휴대폰 페이지 · 휴대폰 실시간 연결 넘기기). 중계실 안의 신호는 signal.mjs.
// 휴대폰은 로그인하지 않는다: 주인(또는 편집 권한)이 만든 서명 링크(24시간)로만 열린다. 휴대폰에 교회 열쇠 · PIN 을 주지 않는다.
import camHtml from '../../../web/phone/cam.html';
import { fail, json } from '../protocol.mjs';
import { makePhoneLink, readPhoneLink, ICE_SERVERS } from './routes.mjs';

const secretOf = env => (typeof env.AUTH_SECRET === 'string' ? env.AUTH_SECRET : '');

/** 로그인 전에 오는 휴대폰 길 (/phonecam · /rtc/phone). toRoom = index.mjs 의 중계실 넘기기 */
export async function publicRoute(request, env, url, toRoom) {
  const link = await readPhoneLink(secretOf(env), url.searchParams.get('k') || '');
  if (!link) {
    if (url.pathname === '/phonecam') return new Response(PAGE_BAD, { status: 403, headers: { 'content-type': 'text/html; charset=utf-8', 'cache-control': 'no-store' } });
    return fail(403, '휴대폰 링크가 맞지 않거나 끝났습니다. 주인 화면에서 새로 만들어 QR 을 다시 찍으세요.');
  }
  if (url.pathname === '/rtc/phone') return toRoom(env, request, { church: link.church, role: 'phone', cred: 'phone-link', exp: link.exp });
  if (request.method !== 'GET' && request.method !== 'HEAD') return fail(405, '없는 주소입니다.');
  const cfg = JSON.stringify({ church: link.church, exp: link.exp, ice: ICE_SERVERS }).replace(/</g, '\\u003c');
  const tag = `<script>window.WP_PHONE=${cfg}</script><script src="/cloud/media/phone.js"></script>`;
  const html = camHtml.replace(/<head([^>]*)>/i, `<head$1>${tag}`);
  return new Response(html, {
    headers: {
      'content-type': 'text/html; charset=utf-8', 'cache-control': 'no-store',
      // 링크가 주소에 있으므로 다른 곳으로 새지 않게
      'referrer-policy': 'no-referrer', 'x-frame-options': 'DENY', 'x-content-type-options': 'nosniff',
      'permissions-policy': 'camera=(self), microphone=()',
    },
  });
}

/** 로그인한 길 (protocol mediaRoute kind 'media-api') */
export async function handle(request, env, url, s, r) {
  if (r.op === 'link') {
    let a = {};
    try { a = JSON.parse((await request.text()) || '{}'); } catch { return fail(400, '잘못된 요청'); }
    const { token, expiresAt } = await makePhoneLink(secretOf(env), s.church, a.name || '');
    return json({ ok: true, url: `${url.origin}/phonecam?k=${token}`, expiresAt });
  }
  return fail(404, '없는 주소입니다.');
}

const PAGE_BAD = `<!doctype html><html lang="ko"><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>휴대폰 카메라</title>
<body style="font:16px system-ui;background:#111;color:#eee;padding:24px"><h1 style="font-size:18px">링크가 맞지 않거나 끝났습니다</h1>
<p>휴대폰 카메라 링크는 하루 동안만 쓸 수 있습니다. 주인 화면(또는 웹 조작 화면)에서 새 링크를 만들어 QR 을 다시 찍어 주세요.</p></body></html>`;
