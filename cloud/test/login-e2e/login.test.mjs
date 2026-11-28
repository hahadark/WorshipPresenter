// 끝에서 끝 (쉬운 로그인 — docs/web/login.md): 구글로 로그인(교회 ID 없이) · 이메일 초대 · 초대 링크 · 교회 고르기 ·
// 이름 · 비밀번호(교회 ID 없이) · 이 기기 기억 · 로그인한 기기 · 로그아웃시키기 · 로그아웃. 가짜 구글(test/fake-google)로만.
import test, { before } from 'node:test';
import assert from 'node:assert/strict';
import { BASE, LOCAL, CHURCH, req, login, cookieOf, newIp } from '../e2e/helpers.mjs';

const W = { 'x-worshipcue': '1', origin: BASE };
const enc = encodeURIComponent;
const run = Date.now().toString(36);
const skip = !LOCAL || !process.env.WP_FAKE_GOOGLE ? '로컬 wrangler dev + 가짜 구글에서만' : false;
let owner = '';

/** 가짜 구글로 한 바퀴: start → (가짜) 동의 → callback. 돌려줌 { status, location, cookie, setCookie[] } */
async function google(mode, { email, name = '', cookie = '', extra = '' } = {}) {
  const ip = newIp();
  const r1 = await req(`/auth/google/start?mode=${mode}${name ? `&name=${enc(name)}` : ''}&hint=${enc(email)}${extra}`, { cookie, ip });
  assert.equal(r1.status, 302, r1.text);
  const loc = new URL(r1.headers.get('location'));
  if (loc.origin === new URL(BASE).origin) return { status: 302, location: loc.pathname + loc.search, cookie: '', setCookie: [] };
  const oauth = cookieOf(r1.headers);
  const r2 = await fetch(loc, { redirect: 'manual' });
  assert.equal(r2.status, 302);
  const cb = new URL(r2.headers.get('location'));
  const r3 = await req(cb.pathname + cb.search, { cookie: [cookie, oauth].filter(Boolean).join('; '), ip });
  return { status: r3.status, location: new URL(r3.headers.get('location'), BASE).pathname + new URL(r3.headers.get('location'), BASE).search, cookie: cookieOf(r3.headers), setCookie: r3.headers.getSetCookie() };
}
const sessionCookieLine = list => list.find(c => c.startsWith('wp_session=') && !c.startsWith('wp_session=;'));
const me = cookie => req('/api/v1/me', { cookie });

before(async () => { if (!skip) owner = await login('owner'); });

test('구글로 로그인: 이메일로 초대받은 사람은 교회 ID 없이 바로 그 역할로 · 이 기기 기억이면 90일 쿠키', { skip }, async () => {
  const carol = `carol-${run}@example.com`;
  // 초대 전에는 들어갈 교회가 없다
  const none = await google('login', { email: `nobody-${run}@example.com` });
  assert.equal(none.location, '/login?google=no-account');
  const iv = await req('/api/v1/invites', { method: 'POST', cookie: owner, headers: W, body: { email: carol, role: 'editor' } });
  assert.equal(iv.status, 201, iv.text);
  assert.equal(iv.json.url, undefined, '이메일 초대는 링크를 주지 않는다');
  assert.equal((await req('/api/v1/invites', { method: 'POST', cookie: owner, headers: W, body: { email: 'bad-email', role: 'editor' } })).status, 400);
  const g = await google('login', { email: carol });
  assert.equal(g.status, 302);
  assert.equal(g.location, '/editor', '편집자의 첫 화면');
  const line = sessionCookieLine(g.setCookie);
  assert.match(line, /Max-Age=7776000/, '이 기기 기억 = 90일');
  assert.match(line, /HttpOnly/);
  assert.match(line, /SameSite=Lax/);
  const m = await me(g.cookie);
  assert.deepEqual([m.json.church, m.json.role], [CHURCH, 'member-editor']);
  const list = await req('/api/v1/invites', { cookie: owner });
  assert.ok(list.json.invites.find(x => x.email === carol).usedAt, '초대는 받음으로');
  const people = await req('/api/v1/members', { cookie: owner });
  assert.ok(people.json.members.some(x => x.google && x.email === carol), '사람 목록에 구글 이메일이 보임');
  // 공용 PC: 이 기기 기억 끔 → 브라우저를 닫으면 끝나는 쿠키
  const g2 = await google('login', { email: carol, extra: '&remember=0&next=%2Fcontrol%2F' });
  assert.equal(g2.location, '/control/', '원래 가려던 주소로');
  assert.doesNotMatch(sessionCookieLine(g2.setCookie), /Max-Age/, '기억하지 않으면 Max-Age 없음');
  // 로그인한 기기 → 주인이 하나를 로그아웃시킴 → 그 쿠키만 죽는다
  const ss = await req('/api/v1/sessions', { cookie: owner });
  const mine = ss.json.sessions.filter(x => x.who.startsWith('carol-'));
  assert.equal(mine.length, 2);
  assert.ok(mine.every(x => x.via === 'google' && x.device));
  const short = mine.find(x => !x.remember);
  assert.equal((await req(`/api/v1/sessions/${short.id}`, { method: 'DELETE', cookie: owner, headers: W })).status, 200);
  assert.equal((await me(g2.cookie)).status, 401, '로그아웃시킨 기기');
  assert.equal((await me(g.cookie)).status, 200, '다른 기기는 그대로');
  // 이 기기에서 로그아웃 → 그 세션도 서버에서 끝
  await req('/auth/logout', { method: 'POST', cookie: g.cookie, headers: W });
  assert.equal((await me(g.cookie)).status, 401, '로그아웃한 쿠키를 다시 써도 안 됨');
  // 편집자는 기기 목록 · 초대를 못 본다
  const g3 = await google('login', { email: carol });
  assert.equal((await req('/api/v1/sessions', { cookie: g3.cookie })).status, 403);
  assert.equal((await req('/api/v1/invites', { method: 'POST', cookie: g3.cookie, headers: W, body: { email: 'x@example.com', role: 'owner' } })).status, 403);
});

test('교회가 여럿이면 고르기 화면 → 고른 교회로', { skip }, async () => {
  const dave = `dave-${run}@example.com`;
  const sign = await google('signup', { email: dave, name: `데이브 교회 ${run}` });
  assert.match(sign.location, /^\/owner/, '자기 무료 교회의 주인');
  await req('/api/v1/invites', { method: 'POST', cookie: owner, headers: W, body: { email: dave, role: 'viewer' } });
  const g = await google('login', { email: dave });
  assert.equal(g.location, '/login?pick=1');
  const pickCookie = g.setCookie.find(c => c.startsWith('wp_pick='));
  assert.match(pickCookie, /Path=\/auth\/pick/);
  assert.match(pickCookie, /HttpOnly/);
  const pc = pickCookie.split(';')[0];
  const list = await req('/auth/pick', { cookie: pc });
  assert.equal(list.json.churches.length, 2);
  assert.ok(list.json.churches.some(c => c.church === CHURCH && c.role === 'viewer'));
  assert.equal((await req('/auth/pick', { method: 'POST', cookie: pc, headers: { ...W, origin: 'https://evil.example' }, body: { church: CHURCH } })).status, 403, '다른 곳에서 고르기 막음');
  assert.equal((await req('/auth/pick', { method: 'POST', cookie: pc, headers: W, body: { church: 'not-mine' } })).status, 400);
  const p = await req('/auth/pick', { method: 'POST', cookie: pc, headers: W, body: { church: CHURCH } });
  assert.equal(p.status, 200, p.text);
  assert.equal(p.json.home, '/outputs/main');
  assert.deepEqual([(await me(cookieOf(p.headers))).json.church, (await me(cookieOf(p.headers))).json.role], [CHURCH, 'member-viewer']);
  assert.equal((await req('/auth/pick')).status, 401, '고르기 쿠키 없이는 안 됨');
});

test('초대 링크: 받은 사람이 구글로 받음 · 한 번만 · 링크 원문은 서버에 없음', { skip }, async () => {
  const mk = await req('/api/v1/invites', { method: 'POST', cookie: owner, headers: W, body: { role: 'viewer', link: true } });
  assert.equal(mk.status, 201);
  const url = new URL(mk.json.url);
  const token = url.pathname.split('/').pop();
  const to = await req(url.pathname);
  assert.equal(to.status, 302);
  assert.equal(new URL(to.headers.get('location')).pathname + new URL(to.headers.get('location')).search, `/login?invite=${token}`);
  const info = await req(`/auth/invite?t=${token}`);
  assert.equal(info.json.role, 'viewer');
  assert.ok(info.json.church);
  assert.ok(!JSON.stringify((await req('/api/v1/invites', { cookie: owner })).json).includes(token), '목록에 링크 원문이 없다');
  const erin = await google('login', { email: `erin-${run}@example.com`, extra: `&invite=${token}` });
  assert.equal(erin.location, '/outputs/main', '보기만 → 출력 화면');
  assert.equal((await me(erin.cookie)).json.role, 'member-viewer');
  const frank = await google('login', { email: `frank-${run}@example.com`, extra: `&invite=${token}` });
  assert.equal(frank.location, '/login?google=no-account', '한 번 쓴 링크는 끝');
  assert.equal((await req(`/auth/invite?t=${token}`)).status, 404);
  // 지운 초대는 못 씀
  const mk2 = await req('/api/v1/invites', { method: 'POST', cookie: owner, headers: W, body: { role: 'editor', link: true } });
  const t2 = new URL(mk2.json.url).pathname.split('/').pop();
  assert.equal((await req(`/api/v1/invites/${mk2.json.invite.id}`, { method: 'DELETE', cookie: owner, headers: W })).status, 200);
  const gina = await google('login', { email: `gina-${run}@example.com`, extra: `&invite=${t2}` });
  assert.equal(gina.location, '/login?google=no-account');
});

test('이름 · 비밀번호: 교회 ID 없이 · 같은 이름이 두 교회면 비밀번호가 맞는 교회 · 틀리면 잠시 막음', { skip }, async () => {
  const name = `pwuser-${run}`;
  const mk = await req('/api/v1/members', { method: 'POST', cookie: owner, headers: W, body: { login: name, role: 'editor' } });
  assert.equal(mk.status, 201, mk.text);
  const ok = await req('/auth/password', { method: 'POST', headers: W, ip: newIp(), body: { login: name, password: mk.json.password } });
  assert.equal(ok.status, 200, ok.text);
  assert.deepEqual([ok.json.church, ok.json.home], [CHURCH, '/editor']);
  assert.match(sessionCookieLine(ok.headers.getSetCookie()), /Max-Age=7776000/);
  const short = await req('/auth/password', { method: 'POST', headers: W, ip: newIp(), body: { login: name, password: mk.json.password, remember: false, next: '/control/' } });
  assert.doesNotMatch(sessionCookieLine(short.headers.getSetCookie()), /Max-Age/);
  assert.equal(short.json.home, '/control/');
  assert.equal((await req('/auth/password', { method: 'POST', headers: { ...W, origin: 'https://evil.example' }, ip: newIp(), body: { login: name, password: mk.json.password } })).status, 403);
  // 다른 교회(데이브의 무료 교회)에도 같은 이름 → 비밀번호가 다르면 맞는 교회로 바로
  const dave = await google('login', { email: `dave-${run}@example.com` }); // 고르기 쿠키만 나온다 → 고른다
  const pc = dave.setCookie.find(c => c.startsWith('wp_pick=')).split(';')[0];
  const churches = (await req('/auth/pick', { cookie: pc })).json.churches;
  const other = churches.find(c => c.church !== CHURCH).church;
  const daveOwner = cookieOf((await req('/auth/pick', { method: 'POST', cookie: pc, headers: W, body: { church: other } })).headers);
  const mk2 = await req('/api/v1/members', { method: 'POST', cookie: daveOwner, headers: W, body: { login: name, role: 'viewer' } });
  assert.equal(mk2.status, 201, mk2.text);
  const r2 = await req('/auth/password', { method: 'POST', headers: W, ip: newIp(), body: { login: name, password: mk2.json.password } });
  assert.deepEqual([r2.json.church, r2.json.home], [other, '/outputs/main']);
  // 틀린 비밀번호 5번 → 그 주소에서 잠시 막힘
  const ip = newIp();
  for (let i = 0; i < 5; i++) assert.equal((await req('/auth/password', { method: 'POST', headers: W, ip, body: { login: name, password: 'wrong-' + i } })).status, 401);
  assert.equal((await req('/auth/password', { method: 'POST', headers: W, ip, body: { login: name, password: mk.json.password } })).status, 429);
  assert.equal((await req('/auth/password', { method: 'POST', headers: W, ip: newIp(), body: { login: `nobody-${run}`, password: 'x' } })).status, 401, '없는 이름도 같은 답');
  // 예전 길(교회 ID + 이름)도 그대로 — 기기 목록에 보인다
  const old = await req('/auth/member', { method: 'POST', headers: W, ip: newIp(), body: { church: CHURCH, login: name, password: mk.json.password } });
  assert.equal(old.status, 200);
  const ss = await req('/api/v1/sessions', { cookie: owner });
  assert.ok(ss.json.sessions.filter(x => x.who === name).length >= 3);
});

test('로그인 화면: 구글 단추 · 이름 · 비밀번호 · 이 기기 기억 · 고급(교회 ID · 열쇠)', { skip }, async () => {
  const p = await req('/login');
  for (const id of ['id="google"', 'id="pw"', 'id="remember" checked', '<details class="box" id="adv">', 'id="church"', 'id="secret"', 'id="pick"']) assert.ok(p.text.includes(id), id);
  const o = await req('/owner', { cookie: owner });
  assert.match(o.text, /owner-login\.js/);
});
