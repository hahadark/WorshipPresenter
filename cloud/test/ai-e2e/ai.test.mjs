// 끝에서 끝 (5단계 AI): 키 봉인 · 동의 · 사람이 누를 때만 · 하루/분 한도 · 기록 · 권한 · 교회 분리 · 음성 인식 무료 할당 멈춤.
// 진짜 AI 공급자 · Workers AI 는 부르지 않는다: 로컬 wrangler dev 의 AI_MOCK=1 + 가짜 주소 mock:// 로만 (scripts/e2e.mjs · seed.mjs).
import test, { before } from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { BASE, LOCAL, CHURCH, req, login, cookieOf, newIp } from '../e2e/helpers.mjs';

const cloud = join(dirname(fileURLToPath(import.meta.url)), '..', '..');
const W = { 'x-worshipcue': '1', origin: BASE };
const U = { ...W, 'x-wp-action': 'user' };
const KEY = 'sk-mock-ABCDEFGH12345678';
let owner;

// 로컬 D1 파일을 wrangler dev 와 같이 쓰므로 드물게 잠겨 있거나 느릴 수 있다 → 세 번까지
function d1(sql) {
  for (let i = 0; ; i++) {
    try { return d1once(sql); } catch (e) { if (i >= 2) throw e; }
  }
}
function d1once(sql) {
  execFileSync(process.execPath, [join(cloud, 'node_modules', 'wrangler', 'bin', 'wrangler.js'), 'd1', 'execute', 'worshippresenter-web', '--local', '--command', sql], { cwd: cloud, stdio: 'ignore', env: { ...process.env, WRANGLER_SEND_METRICS: 'false' } });
}
const chat = (cookie, body, headers = U) => req('/api/v1/ai/chat', { method: 'POST', cookie, headers, body: { feature: 'lyrics', messages: [{ role: 'system', content: '가사를 정리' }, { role: 'user', content: '주님만 바라봅니다' }], ...body } });
const put = (cookie, body) => req('/api/v1/ai/settings', { method: 'PUT', cookie, headers: W, body });
const consent = (cookie, feature, agree = true) => req('/api/v1/ai/consent', { method: 'POST', cookie, headers: W, body: { feature, agree } });

function wav(seconds, rate = 16000) {
  const n = Math.round(seconds * rate), b = Buffer.alloc(44 + n * 2);
  b.write('RIFF', 0); b.writeUInt32LE(36 + n * 2, 4); b.write('WAVE', 8); b.write('fmt ', 12); b.writeUInt32LE(16, 16);
  b.writeUInt16LE(1, 20); b.writeUInt16LE(1, 22); b.writeUInt32LE(rate, 24); b.writeUInt32LE(rate * 2, 28); b.writeUInt16LE(2, 32); b.writeUInt16LE(16, 34);
  b.write('data', 36); b.writeUInt32LE(n * 2, 40);
  return b;
}
async function stt(cookie, body, headers = U) {
  const r = await fetch(`${BASE}/api/v1/ai/transcribe?feature=stt-cloud`, { method: 'POST', headers: { ...headers, cookie, 'content-type': 'audio/wav' }, body });
  return { status: r.status, json: await r.json().catch(() => null) };
}

before(async () => {
  if (!LOCAL) return; // 배포한 곳에는 가짜 공급자가 없다 → 아래 시험은 모두 건너뜀
  // 앞 실행의 사용량 · 기록 · 동의 · 설정을 비운다 (로컬 D1)
  d1(`DELETE FROM ai_calls; DELETE FROM ai_usage; DELETE FROM ai_account; DELETE FROM ai_consent; DELETE FROM ai_settings;`);
  owner = await login('owner');
});

test('처음: 키 없음 · 동의 없음 · 기능 목록과 공급자 (설치판과 같은 셋)', { skip: !LOCAL && '로컬 wrangler dev(AI_MOCK=1)에서만' }, async () => {
  const s = await req('/api/v1/ai/settings', { cookie: owner });
  assert.equal(s.status, 200, s.text);
  assert.equal(s.json.hasKey, false);
  assert.equal(s.json.host, 'openrouter.ai', '기본은 설치판처럼 오픈라우터');
  assert.deepEqual(s.json.providers.map(p => p.name), ['오픈라우터', 'DeepSeek', 'OpenAI']);
  assert.ok(s.json.features.every(f => !f.consented));
  assert.equal(s.json.stt.available, true, '로컬은 가짜 음성 인식');
});

test('사람이 누를 때만 · 동의 먼저 · 키 먼저', { skip: !LOCAL && '로컬 wrangler dev(AI_MOCK=1)에서만' }, async () => {
  assert.equal((await chat(owner, {}, W)).status, 428, 'X-WP-Action 없이는 안 부름');
  const nc = await chat(owner, {});
  assert.equal(nc.status, 403);
  assert.equal(nc.json.needConsent, true);
  const ct = await req('/api/v1/ai/consent?feature=lyrics', { cookie: owner });
  assert.match(ct.json.text, /붙여 넣은 가사 글.*openrouter\.ai/);
  assert.equal((await consent(owner, 'lyrics')).json.consented, true);
  const nk = await chat(owner, {});
  assert.equal(nk.status, 409);
  assert.equal(nk.json.needKey, true);
});

test('키 봉인: 넣은 키는 다시 보이지 않고, 가짜 공급자로 설치판과 같은 답 모양', { skip: !LOCAL && '로컬 wrangler dev(AI_MOCK=1)에서만' }, async () => {
  assert.equal((await put(owner, { endpoint: 'http://api.deepseek.com/x' })).status, 400);
  assert.equal((await put(owner, { endpoint: 'https://127.0.0.1/v1' })).status, 400);
  const s = await put(owner, { endpoint: 'mock://echo', model: 'mock-model', key: KEY, dailyLimit: 100 });
  assert.equal(s.status, 200, s.text);
  assert.equal(s.json.hasKey, true);
  assert.equal(s.json.keyHint, '…5678');
  assert.ok(!s.text.includes(KEY) && !s.text.includes('ABCDEFGH'), '키 원문이 답에 없다');
  assert.equal(s.json.features.find(f => f.id === 'lyrics').consented, false, '주소가 바뀌면 동의를 다시 받는다');
  await consent(owner, 'lyrics');
  const r = await chat(owner, { model: 'very-expensive/model' });
  assert.equal(r.status, 200, r.text);
  assert.equal(r.json.model, 'mock-model', '브라우저가 모델을 바꾸지 못한다');
  assert.equal(r.json.choices[0].message.content, '[가짜 AI] 주님만 바라봅니다');
  assert.ok(r.json.usage.total_tokens > 0);
  const j = await chat(owner, { response_format: { type: 'json_object' }, max_tokens: 50 });
  assert.equal(JSON.parse(j.json.choices[0].message.content).maxTokens, 50);
});

test('공급자 오류는 502 로, 키는 오류 글에도 안 나온다', { skip: !LOCAL && '로컬 wrangler dev(AI_MOCK=1)에서만' }, async () => {
  const bad = 'sk-bad-ZZZZYYYYXXXX9999';
  await put(owner, { key: bad });
  const r = await chat(owner, {});
  assert.equal(r.status, 502);
  assert.equal(r.json.providerStatus, 401);
  assert.ok(!r.text.includes(bad) && !r.text.includes('ZZZZYYYY'), r.text);
  await put(owner, { key: KEY });
});

test('하루 한도 · 1분 한도 (부르기 전에 막는다)', { skip: !LOCAL && '로컬 wrangler dev(AI_MOCK=1)에서만' }, async () => {
  const s = await req('/api/v1/ai/settings', { cookie: owner });
  await put(owner, { dailyLimit: s.json.today.calls });
  const lim = await chat(owner, {});
  assert.equal(lim.status, 429);
  assert.equal(lim.json.limit, true);
  await put(owner, { dailyLimit: 100 });
  // 지금까지 1분 안에 부른 것: 성공 2 · 오류 1 = 3 → 3번 더 부르면 6, 그다음은 막힘
  for (let i = 0; i < 3; i++) assert.equal((await chat(owner, {})).status, 200);
  const fast = await chat(owner, {});
  assert.equal(fast.status, 429);
  assert.match(fast.json.error, /자주/);
});

test('기록: 내용 없이 언제 · 누가 · 무엇 · 얼마나 (주인만)', { skip: !LOCAL && '로컬 wrangler dev(AI_MOCK=1)에서만' }, async () => {
  const r = await req('/api/v1/ai/log', { cookie: owner });
  assert.equal(r.status, 200);
  assert.ok(r.json.calls.length >= 6);
  const c = r.json.calls[0];
  assert.deepEqual(Object.keys(c).sort(), ['at', 'by', 'chars', 'feature', 'host', 'kind', 'model', 'ms', 'seconds', 'status', 'tokens']);
  assert.ok(!r.text.includes('주님만 바라봅니다'), '보낸 글은 기록하지 않는다');
  assert.ok(r.json.days[0].calls >= 6);
});

test('권한: 출력 · 리모컨 · 보기만 사람은 못 부르고, 사람 편집자는 부르되 키 · 동의는 못 바꿈', { skip: !LOCAL && '로컬 wrangler dev(AI_MOCK=1)에서만' }, async () => {
  const out = await login('output');
  assert.equal((await chat(out, {})).status, 403);
  assert.equal((await req('/api/v1/ai/settings', { cookie: out })).status, 403);
  const rem = await login('remote');
  assert.equal((await chat(rem, {})).status, 403);
  const run = Date.now().toString(36);
  const mk = async role => {
    const m = await req('/api/v1/members', { method: 'POST', cookie: owner, headers: W, body: { login: `ai${role[0]}${run}`, role } });
    assert.equal(m.status, 201, m.text);
    const l = await req('/auth/member', { method: 'POST', headers: W, ip: newIp(), body: { church: CHURCH, login: `ai${role[0]}${run}`, password: m.json.password } });
    assert.equal(l.status, 200, l.text);
    return cookieOf(l.headers);
  };
  const viewer = await mk('viewer');
  assert.equal((await chat(viewer, {})).status, 403);
  const editor = await mk('editor');
  const s = await req('/api/v1/ai/settings', { cookie: editor });
  assert.equal(s.status, 200);
  assert.ok(!s.text.includes(KEY));
  assert.equal((await put(editor, { key: 'sk-x-12345678' })).status, 403);
  assert.equal((await consent(editor, 'sermon')).status, 403);
  assert.equal((await req('/api/v1/ai/log', { cookie: editor })).status, 403);
  // 다른 사이트에서 몰래: 403
  assert.equal((await req('/api/v1/ai/chat', { method: 'POST', cookie: owner, headers: { ...U, origin: 'https://evil.example' }, body: {} })).status, 403);
});

test('교회 분리: 다른 교회는 이 교회 키 · 동의 · 기록을 못 본다', { skip: !LOCAL && '로컬 wrangler dev(AI_MOCK=1)에서만' }, async () => {
  const f = join(cloud, '.secrets', 'data-other.json');
  if (!existsSync(f)) return; // data-e2e 가 만든 둘째 교회가 있을 때만
  const o = JSON.parse(readFileSync(f, 'utf8'));
  const l = await req('/auth/login', { method: 'POST', headers: { 'x-worshipcue': '1' }, ip: newIp(), body: { church: o.church, secret: o.credentials.owner.secret } });
  const other = cookieOf(l.headers);
  const s = await req('/api/v1/ai/settings', { cookie: other });
  assert.equal(s.json.hasKey, false);
  assert.ok(s.json.features.every(x => !x.consented));
  assert.equal((await req('/api/v1/ai/log', { cookie: other })).json.calls.length, 0);
});

test('음성 인식(클라우드): 동의 · WAV · 교회 하루 분 한도 · 계정 무료 할당을 넘기 전에 멈춤', { skip: !LOCAL && '로컬 wrangler dev(AI_MOCK=1)에서만', timeout: 120_000 }, async () => {
  assert.equal((await stt(owner, wav(2))).status, 403, '동의 먼저');
  await consent(owner, 'stt-cloud');
  assert.equal((await stt(owner, wav(2), W)).status, 428, '사람이 누른 것만');
  assert.equal((await stt(owner, Buffer.from('this is not audio data at all .............'))).status, 415);
  assert.equal((await stt(owner, wav(31))).status, 413, '30초까지');
  const ok = await stt(owner, wav(2));
  assert.equal(ok.status, 200, JSON.stringify(ok.json));
  assert.match(ok.json.text, /가짜 음성 인식/);
  assert.equal(ok.json.seconds, 2);
  // 교회 하루 분 한도
  await put(owner, { sttMinutes: 0 });
  const lim = await stt(owner, wav(2));
  assert.equal(lim.status, 429);
  await put(owner, { sttMinutes: 60 });
  // 계정 무료 할당: 거의 다 쓴 것처럼 → 부르지 않고 멈춤 (교회 분은 되돌림)
  const before = (await req('/api/v1/ai/settings', { cookie: owner })).json.today.sttSeconds;
  d1(`UPDATE ai_account SET neurons = 7999.9`);
  const free = await stt(owner, wav(2));
  assert.equal(free.status, 429);
  assert.equal(free.json.free, true);
  assert.equal((await req('/api/v1/ai/settings', { cookie: owner })).json.today.sttSeconds, before, '멈춘 조각은 교회 분에서 빼지 않음');
  const log = await req('/api/v1/ai/log', { cookie: owner });
  assert.ok(log.json.calls.some(c => c.kind === 'stt' && c.seconds >= 2));
});
