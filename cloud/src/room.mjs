// 교회마다 하나 있는 중계실 (Durable Object, WebSocket Hibernation).
// - 송출 주체(설치판, 나중에는 웹 조작 화면) 하나와 보는 쪽(출력 화면 · 리모컨) 여럿을 잇는다.
// - 예배 자료(본문 · 가사)는 디스크에 쓰지 않는다. 메모리에만 두고, 잠들었다 깨면 송출 주체에게 다시 달라고 한다(sync).
// - 리모컨 · 편집기 · 미디어 요청은 번호를 붙여 송출 주체에게 넘기고 답을 기다린다 (설치판은 밖으로 나가는 연결 하나만 연다).
// 이 클래스는 Worker(index.mjs)만 부른다. 누구인지(x-wp-*)는 Worker 가 쿠키 · 열쇠를 확인해 붙인 값이다.
import { DurableObject } from 'cloudflare:workers';
import * as signal from './media/signal.mjs'; // 4단계 B 휴대폰 카메라 신호 (docs/web/media.md)
import { freshen } from './media/routes.mjs';
import { CLOSE, FROM_SOURCE, FROM_VIEWER, LIMITS, PROTOCOL, VIEW_COMMANDS, clientName, fail, json, mediaPath, unpackChunk } from './protocol.mjs';

const OFFLINE = '설치판(워십프리젠터)이 클라우드에 연결되어 있지 않습니다.';
const send = (ws, obj) => { try { ws.send(typeof obj === 'string' ? obj : JSON.stringify(obj)); return true; } catch { return false; } };

export class ChurchRoom extends DurableObject {
  constructor(ctx, env) {
    super(ctx, env);
    this.renders = new Map(); // 화면 이름 → 출력 자료(JSON 글)
    this.renderAt = new Map(); // 화면 이름 → 받은 때 (늦게 붙은 화면에 줄 때 시계를 맞춘다 — media/routes.mjs freshen)
    this.draw = null; // 화면에 그리기 (JSON 글)
    this.remote = null; // 리모컨 상태 (JSON 글)
    this.pending = new Map(); // 요청 번호 → 기다리는 답
    this.inflight = new Map(); // 같은 파일을 여러 화면이 한꺼번에 청할 때 한 번만 넘긴다
    this.cache = new Map(); // 작은 파일 잠깐 기억 (메모리, 잠들면 사라짐)
    this.cacheBytes = 0;
    this.seq = 1;
    this.lastSync = 0;
    this.presenceTimer = null;
    // 살아 있는지 묻는 글은 방을 깨우지 않고 답한다 (요금 · 잠자기)
    ctx.setWebSocketAutoResponse(new WebSocketRequestResponsePair('ping', 'pong'));
  }

  /** 지금 송출 주체 (대기 중인 연결은 빼고) */
  source() { return this.ctx.getWebSockets('source').find(ws => !this.att(ws).standby) || null; }
  standbys() { return this.ctx.getWebSockets('source').filter(ws => this.att(ws).standby); }
  viewers(client) { return this.ctx.getWebSockets(client ? `c:${client}` : 'viewer'); }
  att(ws) { try { return ws.deserializeAttachment() || {}; } catch { return {}; } }

  async fetch(request) {
    const url = new URL(request.url);
    const path = url.pathname;
    const who = { role: request.headers.get('x-wp-role') || '', cred: request.headers.get('x-wp-cred') || '', exp: Number(request.headers.get('x-wp-exp')) || 0 };
    if (path === '/bridge') return this.acceptSource(request, who);
    if (path === '/events') return this.acceptViewer(request, url, who);
    if (path === '/internal/revoke') return this.revoke(url.searchParams.get('cred') || '');
    if (path.startsWith('/rtc/')) return signal.roomFetch(this, request, url, who);
    if (path === '/r/state') return this.remoteState(request);
    if (path === '/api/ack') return json({ ok: true });
    if (path === '/api/command') return this.viewCommand(await request.json().catch(() => null));
    if (path === '/api/v1/room') return json({ ok: true, ...this.status() });
    if (mediaPath(path)) return this.media(request, url);
    return this.relayJson(request, url);
  }

  // ---------- 붙기 ----------
  acceptSource(request, who) {
    if (request.headers.get('upgrade')?.toLowerCase() !== 'websocket') return fail(426, 'WebSocket 으로 연결하세요.');
    // 송출 주체 겹침 (3단계): 설치판 열쇠(Bearer) = desktop, 주인 쿠키 = web (웹 조작 화면)
    const kind = who.role === 'bridge' ? 'desktop' : 'web';
    const cur = this.source();
    const ca = cur ? this.att(cur) : null;
    // 대기로 붙는 경우: 웹은 주체가 이미 있으면 늘 대기. 설치판은 웹이 넘겨받아(claim) 쥐고 있을 때만 대기
    const standby = !!cur && (kind === 'web' || (ca.kind === 'web' && ca.claimed));
    const [client, server] = Object.values(new WebSocketPair());
    this.ctx.acceptWebSocket(server, ['source']);
    server.serializeAttachment({ role: 'source', kind, owner: who.role === 'owner', cred: who.cred, since: Date.now(), app: '', lang: 'ko', outputs: [], caps: {}, standby, claimed: false });
    if (standby) {
      send(server, { t: 'ready', v: PROTOCOL, viewers: this.viewers().length, standby: true, active: this.who(cur) });
      return new Response(null, { status: 101, webSocket: client });
    }
    // 주체가 된다: 같은 종류의 옛 연결은 끊고(끊긴 줄 모르고 남은 옛 연결이 막지 않게), 웹 주체는 설치판에게 대기로 내려간다
    for (const old of this.ctx.getWebSockets('source')) {
      if (old === server) continue;
      const a = this.att(old);
      if (a.standby) continue;
      if (a.kind === 'web' && kind === 'desktop') { this.demote(old, server); continue; }
      try { old.serializeAttachment({ ...a, replaced: true }); old.close(CLOSE.replaced, 'replaced'); } catch { /* 이미 닫힘 */ }
    }
    this.takeOver();
    send(server, { t: 'ready', v: PROTOCOL, viewers: this.viewers().length, standby: false });
    return new Response(null, { status: 101, webSocket: client });
  }

  /** 송출 주체가 바뀔 때: 앞 주체의 기억을 비운다 (새 주체가 전부 다시 보낸다) */
  takeOver() {
    this.failPending(503, '설치판 연결이 바뀌었습니다. 다시 해 보세요.');
    this.renders.clear(); this.draw = null; this.remote = null; this.clearCache();
  }
  who(ws) { const a = this.att(ws); return { source: a.kind || 'desktop', app: a.app || '', since: a.since || 0, claimed: !!a.claimed }; }
  /** 주체 → 대기 (끊지 않는다. 화면에 '보기 전용' 을 알리게) */
  demote(ws, by) {
    const a = this.att(ws);
    ws.serializeAttachment({ ...a, standby: true, claimed: false });
    send(ws, { t: 'standby', active: by ? this.who(by) : null });
  }
  /** 대기 → 주체. 새 주체에게 전부 다시 달라고 하고, 화면에 '연결됨' 을 알린다 */
  promote(ws, claimed) {
    const a = this.att(ws);
    ws.serializeAttachment({ ...a, standby: false, claimed: !!claimed });
    this.takeOver();
    send(ws, { t: 'active', claimed: !!claimed });
    this.lastSync = 0;
    send(ws, { t: 'sync' });
    for (const v of this.viewers()) send(v, { t: 'status', online: true });
    this.presenceSoon();
  }
  /** 주인이 넘겨받기를 눌렀다 (웹 조작 화면). 설치판은 끊고(다시 붙으면 대기), 다른 웹 주체는 대기로 */
  claim(ws, a) {
    if (a.kind !== 'web' || !a.owner) return send(ws, { t: 'denied', error: '넘겨받기는 주인만 할 수 있습니다.' });
    const cur = this.source();
    if (cur && cur !== ws) {
      const ca = this.att(cur);
      if (ca.kind === 'desktop') { try { cur.serializeAttachment({ ...ca, replaced: true }); cur.close(CLOSE.replaced, 'claimed'); } catch { /* 이미 닫힘 */ } }
      else this.demote(cur, ws);
    }
    this.promote(ws, true);
  }
  /** 넘겨받은 웹이 돌려준다: 대기 중인 설치판이 있으면 그것을 주체로, 없으면 넘겨받음 표시만 지운다 */
  release(ws) {
    const a = this.att(ws);
    const desk = this.standbys().find(x => this.att(x).kind === 'desktop');
    if (!desk) { ws.serializeAttachment({ ...a, claimed: false }); send(ws, { t: 'active', claimed: false }); return; }
    this.demote(ws, desk);
    this.promote(desk, false);
  }

  acceptViewer(request, url, who) {
    if (request.headers.get('upgrade')?.toLowerCase() !== 'websocket') return fail(426, 'WebSocket 으로 연결하세요.');
    const name = clientName(url.searchParams.get('client') || 'main');
    if (!name) return fail(400, '모르는 화면 이름입니다.');
    if (this.viewers().length >= LIMITS.viewers) return fail(429, '이 교회에 붙은 화면이 너무 많습니다.');
    // 7단계 등급 한도 (Worker 가 x-wp-max-* 로 알림 — tiers/limits.mjs outputs · remotes)
    const cap = (h, n) => { const v = request.headers.get(h); return v !== null && v !== '' && Number.isFinite(Number(v)) && n >= Number(v); };
    if (name === 'remote' ? cap('x-wp-max-remotes', this.viewers('remote').length) : cap('x-wp-max-outputs', this.viewers().length - this.viewers('remote').length)) {
      return fail(429, name === 'remote' ? '이 등급에서 동시에 붙는 리모컨 수를 넘었습니다.' : '이 등급에서 동시에 붙는 화면 수를 넘었습니다.', { code: 'tier_limit', limit: name === 'remote' ? 'remotes' : 'outputs' });
    }
    const ua = (request.headers.get('user-agent') || '').toLowerCase();
    const kind = url.searchParams.has('window') ? 'window' : ua.includes('obs') ? 'obs' : 'browser';
    const id = 1 + (crypto.getRandomValues(new Uint32Array(1))[0] % 2_000_000_000);
    const [client, server] = Object.values(new WebSocketPair());
    this.ctx.acceptWebSocket(server, ['viewer', `c:${name}`]);
    server.serializeAttachment({ role: 'viewer', client: name, id, kind, cred: who.cred, exp: who.exp, since: Date.now(), ack: null });
    const src = this.source();
    send(server, { t: 'hello', v: PROTOCOL, id, online: !!src });
    this.sendState(server, name);
    this.presenceSoon();
    return new Response(null, { status: 101, webSocket: client });
  }

  /** 새로 붙은 화면에 지금 것을 보낸다. 방이 잠들었다 깨어 기억이 없으면 송출 주체에게 다시 달라고 한다 */
  sendState(ws, name) {
    let missing = false;
    if (name === 'remote') {
      if (this.remote) send(ws, `{"t":"remote","data":${freshen(this.remote, Date.now() - (this.remoteAt || Date.now()))}}`); else missing = true;
    } else {
      const r = this.renders.get(name);
      if (r) send(ws, `{"t":"render","data":${freshen(r, Date.now() - (this.renderAt.get(name) || Date.now()))}}`); else missing = true;
      if (name !== 'stage' && this.draw) send(ws, `{"t":"draw","data":${this.draw}}`);
    }
    if (missing) this.askSync();
  }
  askSync() {
    const src = this.source();
    if (!src || Date.now() - this.lastSync < 1000) return;
    this.lastSync = Date.now();
    send(src, { t: 'sync' });
  }

  // ---------- 실시간 메시지 ----------
  async webSocketMessage(ws, message) {
    const a = this.att(ws);
    if (a.role === 'source' && a.standby) return typeof message === 'string' ? this.fromStandby(ws, a, message) : undefined;
    if (a.role === 'source') return typeof message === 'string' ? this.fromSource(ws, a, message) : this.chunk(message);
    if (a.role === 'phone') return signal.fromPhone(this, ws, a, message);
    if (typeof message !== 'string' || message.length > signal.viewerLimit(message)) return ws.close(CLOSE.protocol, 'too large');
    if (a.exp && a.exp < Date.now()) return ws.close(CLOSE.revoked, 'expired');
    let m;
    try { m = JSON.parse(message); } catch { return ws.close(CLOSE.protocol, 'json'); }
    if (!m || !FROM_VIEWER.includes(m.t)) return; // 모르는 종류는 버린다 (앞으로 늘어날 수 있다)
    if (m.t === 'ack') {
      a.ack = { version: Number(m.version) || 0, width: Number(m.width) || 0, height: Number(m.height) || 0, visible: !!m.visible, at: Date.now() };
      ws.serializeAttachment(a);
      this.presenceSoon();
    } else if (m.t === 'rtc') {
      signal.fromViewer(this, ws, a, m);
    } else if (m.t === 'cmd' && a.client !== 'remote') {
      this.viewCommand(m);
    }
  }

  /** 대기 중인 송출 주체: 화면 자료는 버리고 hello · claim 만 받는다 */
  fromStandby(ws, a, text) {
    if (text.length > LIMITS.message) return ws.close(CLOSE.protocol, 'too large');
    let m;
    try { m = JSON.parse(text); } catch { return ws.close(CLOSE.protocol, 'json'); }
    if (m?.t === 'hello') {
      if (m.v !== PROTOCOL) return ws.close(CLOSE.protocol, 'version');
      ws.serializeAttachment({ ...a, app: String(m.app || '').slice(0, 40), lang: m.lang === 'en' ? 'en' : 'ko' });
    } else if (m?.t === 'claim') this.claim(ws, a);
  }

  fromSource(ws, a, text) {
    if (text.length > LIMITS.message) return ws.close(CLOSE.protocol, 'too large');
    let m;
    try { m = JSON.parse(text); } catch { return ws.close(CLOSE.protocol, 'json'); }
    if (!m || !FROM_SOURCE.includes(m.t)) return;
    switch (m.t) {
      case 'hello': {
        if (m.v !== PROTOCOL) return ws.close(CLOSE.protocol, 'version');
        Object.assign(a, { app: String(m.app || '').slice(0, 40), lang: m.lang === 'en' ? 'en' : 'ko', outputs: (Array.isArray(m.outputs) ? m.outputs : []).filter(clientName).slice(0, 40), caps: { remote: !!m.caps?.remote, editor: !!m.caps?.editor, media: !!m.caps?.media } });
        ws.serializeAttachment(a);
        for (const v of this.viewers()) send(v, { t: 'status', online: true });
        this.presenceSoon();
        break;
      }
      case 'render': {
        const name = clientName(m.client);
        if (!name || name === 'remote' || !m.data || typeof m.data !== 'object') return;
        const s = JSON.stringify(m.data);
        this.renders.set(name, s);
        this.renderAt.set(name, Date.now());
        const frame = `{"t":"render","data":${s}}`;
        for (const v of this.viewers(name)) send(v, frame);
        break;
      }
      case 'draw': {
        this.draw = JSON.stringify(m.data ?? { strokes: [], spot: null });
        const frame = `{"t":"draw","data":${this.draw}}`;
        for (const v of this.viewers()) { const c = this.att(v).client; if (c !== 'stage' && c !== 'remote') send(v, frame); }
        break;
      }
      case 'remote': {
        if (!m.data || typeof m.data !== 'object') return;
        this.remote = JSON.stringify(m.data);
        this.remoteAt = Date.now();
        const frame = `{"t":"remote","data":${this.remote}}`;
        for (const v of this.viewers('remote')) send(v, frame);
        break;
      }
      case 'res':
        this.answer(m);
        break;
      case 'claim': // 이미 주체 — 넘겨받은 것으로 표시만 (그 뒤 다시 붙는 설치판이 대기로 들어오게)
        if (a.kind === 'web' && a.owner) { ws.serializeAttachment({ ...a, claimed: true }); send(ws, { t: 'active', claimed: true }); }
        else send(ws, { t: 'denied', error: '넘겨받기는 주인만 할 수 있습니다.' });
        break;
      case 'release':
        if (a.kind === 'web') this.release(ws);
        break;
    }
  }

  async webSocketClose(ws, code) {
    const a = this.att(ws);
    try { ws.close(code === 1005 || code === 1006 ? 1000 : code); } catch { /* 이미 닫힘 */ }
    if (a.role === 'source') {
      // 바뀐 것(4001)이면 새 송출 주체가 이미 있다 — 그때는 화면에 '끊김' 을 알리지 않는다. 대기는 화면과 상관없다
      if (a.replaced || a.standby) return;
      // 주체가 떠났는데 대기 중인 설치판이 있으면 그것이 주체가 된다 (설치판이 기본 주체). 대기 중인 웹은 저절로 올라가지 않는다
      const desk = this.standbys().find(x => this.att(x).kind === 'desktop');
      if (desk) { this.promote(desk, false); return; }
      this.failPending(503, OFFLINE);
      for (const v of this.viewers()) send(v, { t: 'status', online: false });
    } else {
      this.presenceSoon();
    }
  }
  async webSocketError(ws) { return this.webSocketClose(ws, 1006); }

  /** 열쇠를 지웠을 때: 그 열쇠로 붙은 연결을 바로 끊는다 */
  revoke(cred) {
    let n = 0;
    for (const ws of this.ctx.getWebSockets()) {
      const c = String(this.att(ws).cred || '');
      if (cred && (c === cred || (cred.startsWith('m:') && c.startsWith(`${cred}:`)))) { n++; try { ws.close(CLOSE.revoked, 'revoked'); } catch { /* 이미 닫힘 */ } }
    }
    return json({ ok: true, closed: n });
  }

  // ---------- 누가 붙어 있나 ----------
  status() {
    const src = this.source();
    const s = src ? this.att(src) : null;
    const viewers = this.viewers().map(v => { const a = this.att(v); return { id: a.id, client: a.client, kind: a.kind, since: a.since, ack: a.ack }; });
    return { online: !!src, app: s?.app || '', lang: s?.lang || 'ko', outputs: s?.outputs || [], caps: s?.caps || {}, since: s?.since || 0, viewers,
      source: s ? s.kind || 'desktop' : null, claimed: !!s?.claimed, standby: this.standbys().map(x => this.who(x)) }; // 3단계 덧붙임
  }
  /** 붙은 화면 목록을 송출 주체에게 (조작 화면이 '클라우드로 보는 화면 N개' 를 보이게). 잦은 변화는 모아서 한 번 */
  presenceSoon() {
    if (this.presenceTimer) return;
    this.presenceTimer = setTimeout(() => {
      this.presenceTimer = null;
      const src = this.source();
      if (src) send(src, { t: 'viewers', list: this.status().viewers });
    }, 300);
  }

  // ---------- 송출 주체에게 넘기는 요청 ----------
  /** 출력 화면이 알리는 '영상 길이 · 영상 끝남' 만 넘긴다 */
  viewCommand(m) {
    if (!m || !VIEW_COMMANDS.includes(m.cmd)) return fail(403, '클라우드에서는 쓸 수 없는 명령입니다.');
    const src = this.source();
    if (src) send(src, { t: 'cmd', cmd: m.cmd, args: { key: String(m.args?.key ?? '').slice(0, 400), duration: Number(m.args?.duration) || null } });
    return json({ ok: true, result: null });
  }

  /** 리모컨 상태: 기억해 둔 것이 있으면 바로, 없으면 송출 주체에게 묻는다 */
  remoteState(request) {
    if (!this.source()) return fail(503, OFFLINE);
    const fresh = this.remote && freshen(this.remote, Date.now() - (this.remoteAt || Date.now()));
    if (fresh && fresh.length > 2) return new Response(`{"ok":true,${fresh.slice(1)}`, { headers: { 'content-type': 'application/json; charset=utf-8', 'cache-control': 'no-store' } });
    return this.relayJson(request, new URL(request.url));
  }

  /** JSON 을 주고받는 요청 (/r/do · /e/*) */
  async relayJson(request, url) {
    let body = null;
    if (request.method === 'POST') {
      const text = await request.text();
      if (text.length > LIMITS.body) return fail(413, '너무 큽니다.');
      try { body = JSON.parse(text); } catch { return fail(400, '잘못된 요청'); }
    }
    const r = await this.ask({ method: request.method, path: url.pathname, query: Object.fromEntries(url.searchParams), body }, 12_000);
    if (r.error) return fail(r.status, r.error);
    if (r.bytes) return new Response(r.bytes, { status: r.status, headers: { 'content-type': r.headers['content-type'] || 'application/json; charset=utf-8', 'cache-control': 'no-store' } });
    return json(r.body ?? { ok: false }, r.status);
  }

  /** 설치판의 파일 (그림 · 악보 · 글꼴 · 영상 조각). 작은 파일은 잠깐 기억하고, 같은 파일을 여럿이 청하면 한 번만 넘긴다 */
  async media(request, url) {
    const key = url.pathname + url.search;
    const range = request.headers.get('range');
    const msg = { method: 'GET', path: url.pathname, query: Object.fromEntries(url.searchParams), headers: range ? { range } : {} };
    const reply = r => {
      if (r.error) return fail(r.status, r.error);
      const h = new Headers({ 'cache-control': 'private, max-age=600', 'x-content-type-options': 'nosniff' });
      for (const k of ['content-type', 'content-range', 'accept-ranges', 'content-length']) if (r.headers[k]) h.set(k, r.headers[k]);
      if (!r.bytes && !r.stream) return new Response(null, { status: r.status, headers: h });
      return new Response(request.method === 'HEAD' ? null : r.bytes || r.stream, { status: r.status, headers: h });
    };
    if (range) return reply(await this.ask(msg, 30_000, true));
    const hit = this.cache.get(key);
    if (hit) { hit.at = Date.now(); return reply(hit.r); }
    let first = false, p = this.inflight.get(key);
    if (!p) {
      first = true;
      p = this.ask(msg, 30_000, true).finally(() => this.inflight.delete(key));
      this.inflight.set(key, p);
    }
    const r = await p;
    if (r.bytes || r.error) {
      if (first && r.bytes && r.status === 200) this.remember(key, r);
      return reply(r);
    }
    // 큰 파일(흐름)은 한 사람만 읽을 수 있다 — 같이 기다리던 화면은 따로 청한다
    return reply(first ? r : await this.ask(msg, 30_000, true));
  }

  /**
   * 송출 주체에게 요청을 넘기고 답을 기다린다.
   * 답: { status, headers, body } (JSON) · { status, headers, bytes } (작은 파일) · { status, headers, stream } (큰 파일) · { status, error }
   */
  ask(msg, timeoutMs, binary = false) {
    const src = this.source();
    if (!src) return Promise.resolve({ status: 503, error: OFFLINE });
    if (this.pending.size >= LIMITS.pending) return Promise.resolve({ status: 429, error: '요청이 밀려 있습니다. 잠시 뒤에 다시 해 보세요.' });
    const id = this.seq = (this.seq % 0x7fffffff) + 1;
    return new Promise(resolve => {
      const p = { resolve, binary, timer: null, head: null, parts: null, size: 0, writer: null };
      p.touch = ms => { clearTimeout(p.timer); p.timer = setTimeout(() => this.drop(id, 504, '설치판이 제때 답하지 않았습니다. 화면을 다시 불러 결과를 확인하세요.'), ms); };
      p.touch(timeoutMs);
      this.pending.set(id, p);
      if (!send(src, { t: 'req', id, ...msg })) this.drop(id, 503, OFFLINE);
    });
  }
  /** 기다리던 요청을 끝낸다 (시간 초과 · 연결 끊김) */
  drop(id, status, error) {
    const p = this.pending.get(id);
    if (!p) return;
    this.pending.delete(id);
    clearTimeout(p.timer);
    if (p.writer) { p.writer.abort(error).catch(() => {}); } else p.resolve({ status, error });
    const src = this.source();
    if (src) send(src, { t: 'cancel', id });
  }
  failPending(status, error) {
    for (const id of [...this.pending.keys()]) this.drop(id, status, error);
  }

  /** 답의 머리 (t: res). 본문이 JSON 이면 여기서 끝, 이진이면(stream) 뒤따르는 조각을 기다린다 */
  answer(m) {
    const p = this.pending.get(m.id);
    if (!p) return;
    const status = Number.isInteger(m.status) && m.status >= 200 && m.status <= 599 ? m.status : 502;
    const headers = {};
    for (const [k, v] of Object.entries(m.headers || {})) if (typeof v === 'string' && v.length < 300) headers[k.toLowerCase()] = v;
    if (!m.stream) {
      this.pending.delete(m.id);
      clearTimeout(p.timer);
      return p.resolve({ status, headers, body: m.body });
    }
    if (!p.binary) return this.drop(m.id, 502, '설치판이 잘못 답했습니다.');
    p.head = { status, headers };
    const len = Number(headers['content-length']);
    if (Number.isFinite(len) && len >= 0 && len <= LIMITS.cacheItem) {
      p.parts = []; // 작은 파일: 다 모아서 한 번에
    } else {
      // 큰 파일: 받는 대로 흘려 보낸다. 길이를 알면 Content-Length 를 지킨다 (영상 탐색)
      const ts = Number.isFinite(len) && len > 0 ? new FixedLengthStream(len) : new TransformStream();
      p.writer = ts.writable.getWriter();
      p.resolve({ status, headers, stream: ts.readable });
    }
    p.touch(30_000);
  }

  /** 이진 조각: [요청 번호 4][표시 1][본문] */
  chunk(buf) {
    const c = unpackChunk(buf);
    const p = c && this.pending.get(c.id);
    if (!p || !p.head) return;
    if (c.flags & 2) return this.drop(c.id, 502, '설치판이 파일을 끝까지 보내지 못했습니다.');
    if (p.parts) {
      p.size += c.bytes.length;
      if (p.size > LIMITS.cacheItem) return this.drop(c.id, 502, '설치판이 알린 것보다 큰 파일을 보냈습니다.');
      if (c.bytes.length) p.parts.push(c.bytes.slice());
    } else if (c.bytes.length) {
      p.writer.write(c.bytes.slice()).catch(() => this.drop(c.id, 499, 'closed'));
    }
    if (c.flags & 1) {
      this.pending.delete(c.id);
      clearTimeout(p.timer);
      if (p.parts) {
        const bytes = new Uint8Array(p.size);
        let o = 0;
        for (const part of p.parts) { bytes.set(part, o); o += part.length; }
        p.resolve({ ...p.head, bytes });
      } else p.writer.close().catch(() => {});
    } else p.touch(30_000);
  }

  // ---------- 작은 파일 기억 ----------
  remember(key, r) {
    const size = r.bytes.length;
    if (size > LIMITS.cacheItem) return;
    while (this.cacheBytes + size > LIMITS.cacheTotal && this.cache.size) {
      let oldest = null;
      for (const [k, v] of this.cache) if (!oldest || v.at < oldest[1].at) oldest = [k, v];
      this.cache.delete(oldest[0]);
      this.cacheBytes -= oldest[1].r.bytes.length;
    }
    this.cache.set(key, { r: { status: r.status, headers: r.headers, bytes: r.bytes }, at: Date.now() });
    this.cacheBytes += size;
  }
  clearCache() { this.cache.clear(); this.cacheBytes = 0; }
}
