// 휴대폰 카메라 신호 주고받기 — 교회 중계실(room.mjs ChurchRoom) 안에서 돈다. 영상은 지나가지 않는다 (WebRTC 로 휴대폰 → 출력 화면 직접).
// 설치판 phonecam.rs 의 '/cam/poll · /cam/answer · /phonecam/offer · answer · bye' 를 HTTP 폴링 대신 실시간 연결 메시지로:
//   휴대폰 → 중계실: {t:'hello', id, name, size, viewers} (5초마다 · 바뀔 때) · {t:'answer', viewer, sdp, to}
//   중계실 → 휴대폰: {t:'ready', v} · {t:'offer', viewer, sdp, to} · {t:'bye', viewer}
//   보는 화면 → 중계실 (/events 연결로): {t:'rtc', op:'offer', phone, label, viewer, sdp} · {t:'rtc', op:'bye', phone, viewer}
//   중계실 → 보는 화면: {t:'rtc', op:'answer', viewer, sdp} · {t:'rtc', op:'offline', viewer}
// 휴대폰 연결은 잠자기(hibernation) 소켓이라 이름 · 상태는 attachment 에 둔다.
import { fail, json } from '../protocol.mjs';
import { PHONE, normLabel, okId } from './routes.mjs';

const send = (ws, obj) => { try { ws.send(JSON.stringify(obj)); return true; } catch { return false; } };
const att = ws => { try { return ws.deserializeAttachment() || {}; } catch { return {}; } };

export function phones(room) {
  return room.ctx.getWebSockets('phone').map(ws => ({ ws, a: att(ws) })).filter(x => x.a.id);
}
export function phoneList(room) {
  return phones(room).map(({ a }) => ({ id: a.id, name: a.name || '', size: a.size || '', viewers: a.viewers || 0, since: a.since || 0 }));
}

/** 중계실 fetch 의 '/rtc/…' */
export function roomFetch(room, request, url, who) {
  if (url.pathname === '/rtc/phones') return json({ ok: true, phones: phoneList(room), ice: true });
  if (url.pathname === '/rtc/phone') {
    if (request.headers.get('upgrade')?.toLowerCase() !== 'websocket') return fail(426, 'WebSocket 으로 연결하세요.');
    if (who.role !== 'phone') return fail(403, '휴대폰 링크가 필요합니다.');
    if (room.ctx.getWebSockets('phone').length >= PHONE.maxPhones) return fail(429, `휴대폰은 ${PHONE.maxPhones}대까지 붙일 수 있습니다.`);
    const [client, server] = Object.values(new WebSocketPair());
    room.ctx.acceptWebSocket(server, ['phone']);
    server.serializeAttachment({ role: 'phone', id: '', name: who.name || '', since: Date.now(), exp: who.exp || 0 });
    send(server, { t: 'ready', v: 1, helloMs: PHONE.helloMs });
    return new Response(null, { status: 101, webSocket: client });
  }
  return fail(404, '없는 주소입니다.');
}

/** 휴대폰이 보낸 글 */
export function fromPhone(room, ws, a, message) {
  if (typeof message !== 'string' || message.length > PHONE.maxSdp + 2000) return ws.close(4008, 'too large');
  if (a.exp && a.exp < Date.now()) return ws.close(4003, 'expired');
  let m;
  try { m = JSON.parse(message); } catch { return ws.close(4008, 'json'); }
  if (m.t === 'hello') {
    if (!okId(m.id)) return;
    // 같은 휴대폰(id)이 다시 붙으면 앞의 연결을 닫는다
    for (const p of phones(room)) if (p.ws !== ws && p.a.id === m.id) try { p.ws.close(4001, 'replaced'); } catch { /* 이미 닫힘 */ }
    const name = String(m.name || '').replace(/[\u0000-\u001f]/g, '').slice(0, 40);
    const next = { ...a, id: m.id, name: name || a.name || '', size: String(m.size || '').slice(0, 20), viewers: Number(m.viewers) || 0 };
    if (JSON.stringify(next) !== JSON.stringify(a)) { ws.serializeAttachment(next); room.presenceSoon?.(); }
  } else if (m.t === 'answer') {
    if (!okId(m.viewer) || typeof m.sdp !== 'string' || !m.sdp || m.sdp.length > PHONE.maxSdp) return;
    const to = Number(m.to);
    for (const v of room.viewers()) if (att(v).id === to) send(v, { t: 'rtc', op: 'answer', viewer: m.viewer, sdp: m.sdp });
  }
}

/** 보는 화면(/events)이 보낸 {t:'rtc'} */
export function fromViewer(room, ws, a, m) {
  if (!okId(m.viewer)) return;
  const want = String(m.phone || '');
  const list = phones(room);
  // id 로 먼저, 없으면 이름으로 (설치판에 등록한 휴대폰은 클라우드에서 id 가 다르다 — 주소마다 저장소가 따로)
  const hit = list.find(p => p.a.id === want) || (m.label ? list.find(p => normLabel(p.a.name) === normLabel(m.label)) : null);
  if (m.op === 'offer') {
    if (typeof m.sdp !== 'string' || !m.sdp || m.sdp.length > PHONE.maxSdp) return;
    if (!hit) return send(ws, { t: 'rtc', op: 'offline', viewer: m.viewer });
    send(hit.ws, { t: 'offer', viewer: m.viewer, sdp: m.sdp, to: a.id });
  } else if (m.op === 'bye' && hit) {
    send(hit.ws, { t: 'bye', viewer: m.viewer });
  }
}

/** 보는 화면이 보내는 rtc 글은 SDP 를 담아 다른 글보다 크다 (room.mjs 의 4KB 한도 대신) */
export const viewerLimit = message => (typeof message === 'string' && message.startsWith('{"t":"rtc"') ? PHONE.maxSdp + 2000 : 4096);
