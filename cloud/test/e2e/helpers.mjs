// 끝에서 끝 시험 도우미: 로그인 쿠키 · WebSocket 메시지 기다리기 · 가짜 설치판
import WebSocket from 'ws';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

export const BASE = process.env.WP_BASE || 'http://127.0.0.1:8787';
export const LOCAL = /^http:\/\/(127\.0\.0\.1|localhost)[:/]/.test(BASE);
const cloud = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
export const SAMPLE = JSON.parse(readFileSync(join(cloud, '.secrets', process.env.WP_SAMPLE || 'sample.json'), 'utf8'));
export const CHURCH = SAMPLE.church;
export const cred = role => SAMPLE.credentials[role];

/** 시험마다 다른 '접속 주소' 처럼 보이게 (틀린 횟수 잠금이 서로 번지지 않게). 로컬 Worker 는 cf-connecting-ip 를 그대로 믿는다 */
// 로컬 D1 은 실행 사이에 남으므로 실행마다 다른 주소 묶음 (198.18.0.0/15 시험용 대역)
const run = 1 + Math.floor(Math.random() * 250);
let ipSeq = 1;
export const newIp = () => `198.${18 + (ipSeq > 250 ? 1 : 0)}.${run}.${(ipSeq++ % 250) + 1}`;

export async function req(path, { method = 'GET', cookie, headers = {}, body, ip, redirect = 'manual' } = {}) {
  const h = { ...headers };
  if (cookie) h.cookie = cookie;
  // 로컬 Worker 만: 진짜 Cloudflare 는 손님이 보낸 cf-* 머리글을 403 으로 막는다 (주소는 Cloudflare 가 정함)
  if (ip && LOCAL) h['cf-connecting-ip'] = ip;
  if (body !== undefined && typeof body !== 'string') { h['content-type'] = 'application/json'; body = JSON.stringify(body); }
  const r = await fetch(BASE + path, { method, headers: h, body, redirect });
  const text = await r.text();
  let json = null;
  try { json = JSON.parse(text); } catch { /* 글 */ }
  return { status: r.status, headers: r.headers, text, json };
}

/** set-cookie 들 → cookie 머리글 */
export function cookieOf(headers) {
  return headers.getSetCookie().map(c => c.split(';')[0]).filter(c => !c.endsWith('=')).join('; ');
}

export async function login(role, secret = cred(role).secret) {
  const r = await req('/auth/login', { method: 'POST', body: { church: CHURCH, secret }, headers: { 'x-worshipcue': '1' }, ip: newIp() });
  if (r.status !== 200) throw new Error(`로그인 실패 ${role}: ${r.status} ${r.text}`);
  return cookieOf(r.headers);
}

/** 메시지를 줄 세워 두고 조건에 맞는 것을 기다리는 WebSocket */
export function socket(path, { cookie, headers = {}, origin = BASE } = {}) {
  const h = { ...headers };
  if (cookie) h.cookie = cookie;
  if (origin) h.origin = origin;
  const ws = new WebSocket(BASE.replace(/^http/, 'ws') + path, { headers: h });
  const box = { ws, queue: [], waiters: [], closed: null, binary: [] };
  ws.on('message', (data, isBinary) => {
    if (isBinary) { box.binary.push(data); return; }
    const s = data.toString();
    let m = s;
    try { m = JSON.parse(s); } catch { /* 'pong' 같은 글 */ }
    box.queue.push(m);
    box.waiters = box.waiters.filter(w => { const i = box.queue.findIndex(w.pred); if (i < 0) return true; w.resolve(box.queue.splice(i, 1)[0]); return false; });
  });
  ws.on('close', (code, reason) => { box.closed = { code, reason: reason.toString() }; for (const w of box.waiters) w.closed?.(box.closed); });
  box.opened = new Promise((res, rej) => {
    ws.once('open', () => res(box));
    ws.once('unexpected-response', (_req, resp) => rej(Object.assign(new Error(`연결 거절 ${resp.statusCode}`), { status: resp.statusCode })));
    ws.once('error', rej);
  });
  box.next = (pred = () => true, ms = 5000) => {
    const p = typeof pred === 'string' ? m => m && m.t === pred : pred;
    const i = box.queue.findIndex(p);
    if (i >= 0) return Promise.resolve(box.queue.splice(i, 1)[0]);
    return new Promise((resolve, reject) => {
      const w = { pred: p, resolve: v => { clearTimeout(timer); resolve(v); }, closed: c => { clearTimeout(timer); reject(Object.assign(new Error(`닫힘 ${c.code}`), c)); } };
      const timer = setTimeout(() => { box.waiters = box.waiters.filter(x => x !== w); reject(new Error(`기다린 메시지가 오지 않음 (${path})`)); }, ms);
      box.waiters.push(w);
    });
  };
  box.send = obj => ws.send(typeof obj === 'string' ? obj : JSON.stringify(obj));
  box.close = () => new Promise(res => { if (box.closed || ws.readyState === 3) return res(); ws.once('close', () => res()); ws.close(); });
  // 서버가 닫는 글(close frame)을 보낸 때를 닫힌 것으로 본다. 로컬 workerd 는 그 뒤 TCP 를 늦게 끊어서 'close' 사건이 30초 뒤에야 온다
  // (브라우저는 닫는 인사가 끝나면 스스로 끊는다)
  box.waitClose = (ms = 5000) => new Promise((res, rej) => {
    const t0 = Date.now();
    const tick = () => {
      if (box.closed) return res(box.closed);
      if (ws.readyState >= 2 && ws._closeFrameReceived) return res({ code: ws._closeCode });
      if (Date.now() - t0 > ms) return rej(new Error('닫히지 않음'));
      setTimeout(tick, 20);
    };
    tick();
  });
  return box;
}

/** 가짜 설치판: 붙어서 hello 를 보내고, 요청에는 answer(m) 가 돌려준 값으로 답한다 */
export async function fakeBridge({ key = cred('bridge').key, answer } = {}) {
  const b = socket('/bridge', { headers: { authorization: `Bearer ${key}` }, origin: null });
  await b.opened;
  const ready = await b.next('ready');
  b.send({ t: 'hello', v: 1, app: '시험 설치판', lang: 'ko', outputs: ['main', 'overlay', 'stage'], caps: { remote: true, editor: true, media: true } });
  b.reqs = [];
  b.ws.on('message', (data, isBinary) => {
    if (isBinary) return;
    let m; try { m = JSON.parse(data.toString()); } catch { return; }
    if (m.t !== 'req') return;
    b.reqs.push(m);
    const a = answer ? answer(m) : { status: 200, body: { ok: true, echo: m.path } };
    if (a && a.bytes) {
      b.send({ t: 'res', id: m.id, status: a.status || 200, headers: { 'content-type': a.type || 'application/octet-stream', 'content-length': String(a.bytes.length) }, stream: true });
      const head = Buffer.alloc(5); head.writeUInt32BE(m.id, 0); head[4] = 1;
      b.ws.send(Buffer.concat([head, a.bytes]));
    } else if (a) b.send({ t: 'res', id: m.id, status: a.status || 200, body: a.body });
  });
  b.ready = ready;
  return b;
}

export const sleep = ms => new Promise(r => setTimeout(r, ms));
