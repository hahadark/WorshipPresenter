// 웹 화면(휴대폰 리모컨 · 휴대폰 카메라 · 웹 편집기)을 앱 없이 띄워 보는 가짜 서버 (외부 모듈 없음).
// 진짜 서버(crates/wp-core/src/remote.rs · phonecam.rs · webedit.rs)와 같은 주소 · 같은 모양의 답을 준다.
// 쓰는 곳: 화면 캡처(tools/webui/shots.js) · 흐름 시험(tools/webui/e2e.js) — 진짜 앱으로는 만들기 어려운 상태
// (끊김 · 느린 와이파이 · PIN 잠김 · 다른 사람이 고침 409 · 휴대폰 영상이 송출 중)를 마음대로 만든다.
//
//   node tools/webui/mock.js [--web web] [--remote 37741] [--edit 37746] [--cam 37761]
//   → http://127.0.0.1:37741/  (PIN 123456)   리모컨        http://127.0.0.1:37741/dock  OBS 도킹
//     http://127.0.0.1:37746/  (PIN 4321)     웹 편집기
//     http://127.0.0.1:37761/cam?k=k123456789abcdef  휴대폰 카메라 (127.0.0.1 은 http 여도 카메라가 열린다)
//   상태 바꾸기: POST /__mock  { net: { down, delay }, remote: {…}, … }   (본문은 state 에 깊게 합쳐짐)
'use strict';
const http = require('http');
const fs = require('fs');
const path = require('path');

const TYPES = { '.html': 'text/html; charset=utf-8', '.js': 'text/javascript; charset=utf-8', '.css': 'text/css; charset=utf-8', '.woff2': 'font/woff2', '.png': 'image/png', '.svg': 'image/svg+xml' };

// ---------- 본보기 자료 (가상 — 홍길동 · ○○교회) ----------
function demoCue() {
  return {
    id: 'cue-demo', name: '주일 예배 (○○교회)', serviceDate: '2026-11-29', memo: '설교 뒤 헌금 · 광고는 짧게',
    items: [
      { id: 'i1', type: 'title', title: '예배로 부름', subtitle: '시편 100편', note: '' },
      { id: 'i2', type: 'song', title: '새 아침 밝아 오네', lyrics: '[1절]\n새 아침 밝아 오네\n주님을 찬양해\n\n[후렴]\n온 땅이 노래하네\n할렐루야 할렐루야\n\n[2절]\n어둠은 물러가고\n주의 빛 비추네', order: '1절 후렴 2절 후렴', note: 'Key G · 인도자 신호에 후렴 한 번 더', credit: '홍길동 사 · 곡', songId: 's1' },
      { id: 'i3', type: 'bible', title: '성경봉독', ref: '요 3:16-17', note: '봉독: 홍길동 집사' },
      { id: 'i4', type: 'title', title: '설교', subtitle: '하나님이 세상을 이처럼 사랑하사', note: '설교 40분' },
      { id: 'i5', type: 'text', title: '광고', body: '다음 주 구역 모임은 수요일 저녁 7시\n---\n○○교회 성가대 연습은 오후 2시', note: '' },
      { id: 'i6', type: 'hymn', title: '찬송가 305장 나 같은 죄인 살리신', note: '' },
      { id: 'i7', type: 'blank', title: '축도', note: '' },
    ],
  };
}
const SONGS = [
  { id: 's1', title: '새 아침 밝아 오네', key: 'G', credit: '홍길동', lyrics: '[1절]\n새 아침 밝아 오네\n주님을 찬양해', order: '1절 후렴' },
  { id: 's2', title: '주의 은혜라', key: 'D', credit: '', lyrics: '[1절]\n모든 것이 은혜\n은혜였소', order: '' },
  { id: 's3', title: '주님 다시 오실 때까지', key: 'A', credit: '', lyrics: '[1절]\n주님 다시 오실 때까지\n나는 이 길을 가리라', order: '' },
];

// 순서 → 슬라이드 (진짜 앱의 나눔과 같을 필요는 없다 — 화면 모양을 보려는 것)
function deckOf(it) {
  if (it.type === 'song') {
    const parts = {};
    let cur = '';
    for (const blk of String(it.lyrics || '').split(/\n\s*\n/)) {
      const m = blk.match(/^\[(.+?)\]\n?([\s\S]*)$/);
      if (m) { cur = m[1]; parts[cur] = m[2].trim(); } else if (cur) parts[cur] += '\n' + blk.trim();
    }
    const order = (it.order || Object.keys(parts).join(' ')).split(/\s+/).filter(k => parts[k]);
    return order.map(k => ({ kind: 'text', label: k, text: parts[k] }));
  }
  if (it.type === 'bible') return [
    { kind: 'text', label: '', text: '하나님이 세상을 이처럼 사랑하사 독생자를 주셨으니 이는 그를 믿는 자마다 멸망하지 않고 영생을 얻게 하려 하심이라' },
    { kind: 'text', label: '', text: '하나님이 그 아들을 세상에 보내신 것은 세상을 심판하려 하심이 아니요 그로 말미암아 세상이 구원을 받게 하려 하심이라' }];
  if (it.type === 'text') return String(it.body || '').split(/\n---\n/).map(t => ({ kind: 'text', label: '', text: t }));
  if (it.type === 'title') return [{ kind: 'title', label: '', text: [it.title, it.subtitle].filter(Boolean).join(' · ') }];
  if (it.type === 'hymn') return [1, 2, 3].map(n => ({ kind: 'score', label: n + '절', text: '악보', image: '/__mock/score.svg' }));
  if (it.type === 'blank') return [{ kind: 'blank', label: '', text: '(빈 화면)' }];
  return [{ kind: it.type, label: '', text: it.title }];
}

function newState() {
  const cue = demoCue();
  return {
    lang: 'ko',
    pin: '123456', editPin: '4321', camKey: 'k123456789abcdef',
    net: { down: false, delay: 0, lock: 0 }, // down: API 가 끊김 · delay: ms · lock: PIN 잠김(429) 남은 횟수
    fails: 0,
    cue, // 조작 화면에 열린 큐시트
    live: { cueIndex: -1, index: 0, visible: true, deck: [], foreign: false }, // foreign: 다른 큐시트의 순서를 송출 중
    stage: { message: '', timer: { running: false, remaining: 0, duration: 0, endAt: 0 } },
    people: [{ name: '홍길동', role: '담임 사관' }, { name: '김하늘', role: '대표 기도' }, { name: '이바다', role: '특송' }],
    nameOn: null,
    scenes: [{ id: 'sc-camsub', name: '카메라 + 아래 자막' }, { id: 'sc-lyrics-cam', name: '가사 + 구석 카메라' }],
    outputs: [{ id: 'main', name: '예배당 화면', now: 'full', once: false, base: 'full' }, { id: 'overlay', name: 'OBS', now: 'sc-camsub', once: false, base: 'sc-camsub' }],
    obs: { pip: false, full: false },
    log: [], // 리모컨이 보낸 조작 (시험에서 확인)
    // 웹 편집기
    cues: [cue, { id: 'cue-wed', name: '수요 예배', serviceDate: '2026-12-02', memo: '', items: [{ id: 'w1', type: 'title', title: '수요 예배', subtitle: '', note: '' }] }, { id: 'cue-tpl', name: '예배 틀', serviceDate: '', memo: '', template: true, items: [] }],
    updated: { 'cue-demo': '2026-11-27 20:00:00.000', 'cue-wed': '2026-11-20 09:00:00.000', 'cue-tpl': '2026-10-01 09:00:00.000' },
    saves: 0,
    // 휴대폰 카메라
    cam: { onAir: [], offers: [], polls: 0, last: null, answers: 0 },
  };
}

function remoteState(st) {
  const L = st.live; const d = L.deck; const cur = d[L.index]; const nx = d[L.index + 1];
  const it = L.cueIndex >= 0 ? st.cue.items[L.cueIndex] : null;
  const t = st.stage.timer;
  const nextItem = !L.foreign && L.cueIndex >= 0 ? (st.cue.items.slice(L.cueIndex + 1).find(x => x.type !== 'audio') || {}).title || '' : '';
  return {
    ok: true,
    live: {
      has: d.length > 0, visible: d.length > 0 && L.visible, index: d.length ? L.index : -1, count: d.length,
      title: it ? it.title : '', text: cur ? cur.text : '', next: nx ? nx.text : '', image: cur && cur.image ? cur.image : '',
      labels: d.map(s => s.label || ''), cueIndex: L.cueIndex, kind: cur ? cur.kind : '', nextKind: nx ? nx.kind : '',
      cueId: d.length ? (L.foreign ? 'cue-other' : st.cue.id) : '', nextItem,
    },
    cue: st.cue ? { id: st.cue.id, name: st.cue.name, items: st.cue.items.map(x => ({ title: x.title, type: x.type, note: x.note || '' })) } : null,
    app: { lang: st.lang, version: 'mock' },
    control: true,
    stage: { message: st.stage.message, timer: { running: t.running, remaining: t.running ? t.endAt - Date.now() : t.remaining, duration: t.duration, now: Date.now() } },
    names: { people: st.people, on: st.nameOn },
    obs: st.obs,
    scenes: st.scenes.length ? { outputs: st.outputs, list: st.scenes } : null,
  };
}

function remoteAct(st, a) {
  const L = st.live; const t = st.stage.timer;
  st.log.push(a.action + (a.op ? ':' + a.op : ''));
  switch (a.action) {
    case 'next':
      if (L.index + 1 < L.deck.length) L.index++;
      else if (!L.foreign && L.cueIndex >= 0 && L.cueIndex + 1 < st.cue.items.length) { L.cueIndex++; L.deck = deckOf(st.cue.items[L.cueIndex]); L.index = 0; L.visible = true; }
      break;
    case 'prev':
      if (L.index > 0) L.index--;
      else if (!L.foreign && L.cueIndex > 0) { L.cueIndex--; L.deck = deckOf(st.cue.items[L.cueIndex]); L.index = L.deck.length - 1; L.visible = true; }
      break;
    case 'toggle': if (L.deck.length) L.visible = !L.visible; break;
    case 'off': L.deck = []; L.cueIndex = -1; L.index = 0; st.nameOn = null; break;
    case 'goto': if (a.index >= 0 && a.index < L.deck.length) L.index = a.index; break;
    case 'cue': {
      const it = st.cue.items[a.index];
      if (!it) return '그 순서가 없습니다.';
      L.cueIndex = a.index; L.deck = deckOf(it); L.index = 0; L.visible = true; L.foreign = false;
      break;
    }
    case 'message': st.stage.message = String(a.text || '').slice(0, 300); break;
    case 'name': st.nameOn = a.index >= 0 && st.people[a.index] ? st.people[a.index] : null; break;
    case 'timer': {
      const cur = t.running ? t.endAt - Date.now() : t.remaining;
      const ms = Math.round((+a.minutes || 0) * 60000);
      if (a.op === 'until') {
        const m = String(a.at || '').match(/^(\d{1,2}):(\d{2})$/);
        if (!m) return "시각을 '11:00' 처럼 적어 주세요.";
        Object.assign(t, { running: true, endAt: Date.now() + 25 * 60000, remaining: 25 * 60000, duration: 25 * 60000 });
      } else if (a.op === 'start') { if (ms <= 0) return '시간(분)을 입력하세요.'; Object.assign(t, { running: true, endAt: Date.now() + ms, remaining: ms, duration: ms }); }
      else if (a.op === 'pause') { t.remaining = cur; t.running = false; }
      else if (a.op === 'resume') { t.running = true; t.endAt = Date.now() + cur; }
      else if (a.op === 'reset') Object.assign(t, { running: false, remaining: 0, duration: 0, endAt: 0 });
      else if (a.op === 'add') { if (t.running) t.endAt += ms; else t.remaining = cur + ms; t.duration += ms; }
      else return '알 수 없는 타이머 조작입니다.';
      break;
    }
    case 'scene': {
      const o = st.outputs.find(x => x.id === a.output); if (!o) return '출력을 찾을 수 없습니다.';
      if (a.clear) { o.now = o.base; o.once = false; }
      else if (a.hold) { o.base = o.now = a.id; o.once = false; }
      else { o.now = a.id; o.once = a.id !== o.base; }
      break;
    }
    case 'obsPip': st.obs.pip = !st.obs.pip; break;
    case 'obsFull': st.obs.full = !st.obs.full; break;
    default: return '알 수 없는 조작입니다.';
  }
  return '';
}

function merge(a, b) {
  for (const [k, v] of Object.entries(b)) {
    if (v && typeof v === 'object' && !Array.isArray(v) && a[k] && typeof a[k] === 'object' && !Array.isArray(a[k])) merge(a[k], v);
    else a[k] = v;
  }
  return a;
}

function start(opts = {}) {
  const web = path.resolve(opts.web || path.join(__dirname, '..', '..', 'web'));
  const st = newState();
  const servers = [];
  const sockets = new Set();

  const send = (res, code, body, type = 'application/json') => { res.writeHead(code, { 'Content-Type': type, 'Cache-Control': 'no-store' }); res.end(typeof body === 'string' || Buffer.isBuffer(body) ? body : JSON.stringify(body)); };
  const file = (res, rel, html) => {
    const f = path.join(web, rel);
    if (!fs.existsSync(f)) return send(res, 404, 'not found', 'text/plain');
    let buf = fs.readFileSync(f);
    if (html) buf = Buffer.from(buf.toString('utf8').replace('<html lang="ko">', `<html lang="ko" data-app-lang="${st.lang}">`));
    send(res, 200, buf, TYPES[path.extname(f)] || 'application/octet-stream');
  };
  const ui = (res, name) => {
    const map = { 'wpweb.css': 'shared/ui/wpweb.css', 'wpweb.js': 'shared/ui/wpweb.js', 'pretendard.woff2': 'shared/fonts/PretendardVariable.woff2' };
    return map[name] ? file(res, map[name]) : send(res, 404, 'not found', 'text/plain');
  };
  const body = req => new Promise(res => { let b = ''; req.on('data', c => { b += c; }); req.on('end', () => { try { res(JSON.parse(b || '{}')); } catch { res(null); } }); });
  // 끊김 · 느림 흉내 (API 요청에만)
  const net = async (req, res) => {
    if (st.net.down) { req.socket.destroy(); return false; }
    if (st.net.delay) await new Promise(r => setTimeout(r, st.net.delay));
    return true;
  };
  const pinCheck = (res, given, want) => {
    if (st.net.lock > 0) { st.net.lock--; send(res, 429, { ok: false, error: '잠시 후 다시 시도하세요 (PIN 을 여러 번 틀렸습니다).' }); return false; }
    if (given === want) return true;
    st.fails++;
    send(res, 401, { ok: false, error: 'PIN 이 맞지 않습니다.' });
    return false;
  };
  const common = async (req, res, u) => {
    if (u.pathname === '/__mock') {
      if (req.method === 'POST') { const b = await body(req); if (b && b.reset) Object.assign(st, newState()); else if (b) merge(st, b); if (b && b.sendCue != null) remoteAct(st, { action: 'cue', index: b.sendCue }); }
      send(res, 200, st); return true;
    }
    if (u.pathname === '/__mock/score.svg') { send(res, 200, '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 320 180"><rect width="320" height="180" fill="#f4f1e8"/><g stroke="#333" stroke-width="1">' + [50, 58, 66, 74, 82, 110, 118, 126, 134, 142].map(y => `<path d="M20 ${y}H300"/>`).join('') + '</g><g fill="#222">' + [40, 70, 100, 130, 160, 190, 220, 250].map((x, i) => `<ellipse cx="${x}" cy="${62 + (i % 4) * 6}" rx="5" ry="4"/>`).join('') + '</g></svg>', 'image/svg+xml'); return true; }
    if (u.pathname.startsWith('/wpui/')) { ui(res, u.pathname.slice(6)); return true; }
    return false;
  };

  // ---------- 리모컨 (+ /dock) ----------
  const remote = http.createServer(async (req, res) => {
    const u = new URL(req.url, 'http://x');
    if (await common(req, res, u)) return;
    const p = u.pathname;
    if (p === '/' || p === '/index.html' || p === '/dock' || p === '/dock/') return file(res, 'remote/index.html', true);
    if (p === '/remote.js' || p === '/remote.css') return file(res, 'remote' + p);
    if (p === '/groups.js') return file(res, 'shared/groups.js');
    if (p === '/i18n.js' || p === '/i18n-en.js') return file(res, 'shared' + p);
    const dock = (req.headers.referer || '').includes('/dock') || u.searchParams.get('pin') === 'dock';
    if (p === '/r/state') {
      if (!(await net(req, res))) return;
      if (!dock && !pinCheck(res, u.searchParams.get('pin') || '', st.pin)) return;
      return send(res, 200, remoteState(st));
    }
    if (p === '/r/do' && req.method === 'POST') {
      if (!(await net(req, res))) return;
      const a = await body(req);
      if (!a) return send(res, 400, { ok: false, error: '잘못된 요청' });
      if (!(a.pin === 'dock' && req.headers['x-worshipcue'] === '1') && !pinCheck(res, a.pin || '', st.pin)) return;
      const e = remoteAct(st, a);
      return send(res, 200, e ? { ok: false, error: e } : { ok: true, result: { ok: true } });
    }
    send(res, 404, 'not found', 'text/plain');
  });

  // ---------- 웹 편집기 ----------
  const sum = c => ({ id: c.id, name: c.name, serviceDate: c.serviceDate || '', count: c.items.length, pinned: false, template: !!c.template, updatedAt: st.updated[c.id] });
  const cueOf = id => { const c = st.cues.find(x => x.id === id); return c ? { id: c.id, name: c.name, serviceDate: c.serviceDate || '', startTime: '', memo: c.memo || '', items: c.items, updatedAt: st.updated[c.id] } : null; };
  const stamp = () => new Date().toISOString().replace('T', ' ').replace('Z', '');
  const edit = http.createServer(async (req, res) => {
    const u = new URL(req.url, 'http://x');
    if (await common(req, res, u)) return;
    const p = u.pathname;
    if (p === '/' || p === '/index.html') return file(res, 'editor/index.html', true);
    if (p === '/editor.js' || p === '/editor.css') return file(res, 'editor' + p);
    if (!p.startsWith('/e/')) return send(res, 404, 'not found', 'text/plain');
    if (!(await net(req, res))) return;
    const pin = req.headers['x-pin'] || u.searchParams.get('pin') || '';
    if (req.method === 'POST' && req.headers['x-worshipcue'] !== '1') return send(res, 403, '', 'text/plain');
    if (st.editPin && !pinCheck(res, pin, st.editPin)) return;
    if (req.method === 'GET') {
      if (p === '/e/state') return send(res, 200, { ok: true, cues: st.cues.map(sum), active: st.cue.id });
      if (p === '/e/cue') { const c = cueOf(u.searchParams.get('id')); return c ? send(res, 200, { ok: true, cue: c }) : send(res, 404, { ok: false, error: '큐시트를 찾을 수 없습니다.' }); }
      if (p === '/e/songs') { const q = (u.searchParams.get('q') || '').trim(); return send(res, 200, { ok: true, songs: SONGS.filter(s => !q || s.title.includes(q)).map(s => ({ id: s.id, title: s.title, credit: s.credit, key: s.key })) }); }
    } else {
      const a = await body(req);
      if (!a) return send(res, 400, { ok: false, error: '잘못된 요청' });
      if (p === '/e/save') {
        const c = st.cues.find(x => x.id === a.id);
        if (!c) return send(res, 200, { ok: false, error: '큐시트를 찾을 수 없습니다.' });
        if (st.net.saveFail) return send(res, 200, { ok: false, error: '큐시트 형식이 올바르지 않습니다.' });
        if (a.base !== st.updated[c.id]) return send(res, 409, { ok: false, conflict: true, error: '그 사이 다른 곳(조작 화면이나 다른 브라우저)에서 이 큐시트를 고쳤습니다. 다시 불러온 뒤 고쳐 주세요.' });
        c.name = a.name != null ? a.name : c.name; c.serviceDate = a.serviceDate != null ? a.serviceDate : c.serviceDate; c.memo = a.memo != null ? a.memo : c.memo;
        c.items = (a.items || []).map((x, i) => Object.assign({ id: x.id || 'n' + Date.now() + i }, x));
        st.updated[c.id] = stamp(); st.saves++;
        return send(res, 200, { ok: true, cue: cueOf(c.id) });
      }
      if (p === '/e/new') { const c = { id: 'cue-' + Date.now(), name: (a.name || '').trim() || '새 큐시트', serviceDate: '', memo: '', items: [] }; st.cues.unshift(c); st.updated[c.id] = stamp(); return send(res, 200, { ok: true, cue: cueOf(c.id) }); }
      if (p === '/e/song') { const s = SONGS.find(x => x.id === a.id); return s ? send(res, 200, { ok: true, item: { type: 'song', title: s.title, lyrics: s.lyrics, order: s.order, note: '', songId: s.id, credit: s.credit } }) : send(res, 200, { ok: false, error: '곡을 찾을 수 없습니다.' }); }
      // 시험용: 다른 사람이 고친 것처럼 (조작 화면에서 순서 제목을 바꿈)
      if (p === '/e/__touch') { const c = st.cues.find(x => x.id === a.id); if (c) { if (a.title && c.items[0]) c.items[0].title = a.title; st.updated[c.id] = stamp(); } return send(res, 200, { ok: true }); }
    }
    send(res, 404, { ok: false, error: '알 수 없는 요청' });
  });

  // ---------- 휴대폰 카메라 ----------
  const cam = http.createServer(async (req, res) => {
    const u = new URL(req.url, 'http://x');
    if (await common(req, res, u)) return;
    const p = u.pathname;
    if (p === '/cam') return u.searchParams.get('k') === st.camKey ? file(res, 'phone/cam.html', true) : send(res, 403, '<p>주소의 열쇠가 맞지 않습니다.</p>', 'text/html; charset=utf-8');
    if (p === '/cam.js' || p === '/cam.css') return file(res, 'phone' + p);
    if (p === '/cam/poll' || p === '/cam/answer') {
      if (!(await net(req, res))) return;
      const a = await body(req);
      if (!a || a.k !== st.camKey) return send(res, 403, { ok: false, error: 'key' });
      if (p === '/cam/answer') { st.cam.answers++; return send(res, 200, { ok: true }); }
      st.cam.polls++; st.cam.last = { id: a.id, name: a.name, viewers: a.viewers, size: a.size };
      const msgs = st.cam.offers.splice(0);
      return send(res, 200, { ok: true, msgs, onAir: st.cam.onAir });
    }
    send(res, 404, 'not found', 'text/plain');
  });

  const listen = (srv, port) => new Promise((res, rej) => { srv.on('connection', s => { sockets.add(s); s.on('close', () => sockets.delete(s)); }); srv.on('error', rej); srv.listen(port || 0, '127.0.0.1', () => res(srv.address().port)); servers.push(srv); });
  return Promise.all([listen(remote, opts.remote), listen(edit, opts.edit), listen(cam, opts.cam)]).then(([r, e, c]) => ({
    state: st, ports: { remote: r, edit: e, cam: c },
    remoteUrl: `http://127.0.0.1:${r}/`, dockUrl: `http://127.0.0.1:${r}/dock`, editUrl: `http://127.0.0.1:${e}/`, camUrl: `http://127.0.0.1:${c}/cam?k=${st.camKey}`,
    set(b) { if (b.reset) Object.assign(st, newState()); else merge(st, b); return st; },
    reset() { Object.assign(st, newState()); return st; },
    send(i) { remoteAct(st, { action: 'cue', index: i }); st.log.length = 0; },
    act(a) { return remoteAct(st, a); },
    close() { for (const s of sockets) s.destroy(); for (const s of servers) s.close(); },
  }));
}

module.exports = { start, demoCue };

if (require.main === module) {
  const arg = (k, d) => { const i = process.argv.indexOf(k); return i > 0 ? process.argv[i + 1] : d; };
  start({ web: arg('--web'), remote: +arg('--remote', 37741), edit: +arg('--edit', 37746), cam: +arg('--cam', 37761) }).then(m => {
    m.send(1);
    console.log('리모컨      ' + m.remoteUrl + '   (PIN ' + m.state.pin + ')\nOBS 도킹    ' + m.dockUrl + '\n웹 편집기   ' + m.editUrl + '   (PIN ' + m.state.editPin + ')\n휴대폰 카메라 ' + m.camUrl + '\n끝내려면 Ctrl+C');
  });
}
