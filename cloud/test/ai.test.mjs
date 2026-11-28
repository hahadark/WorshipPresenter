// 5단계 AI 단위 시험 (Cloudflare 없이 node): 길 · 권한 · 주소 검사 · 본문 만들기 · 키 봉인 · 동의 글 · WAV 길이 · neurons
import test from 'node:test';
import assert from 'node:assert/strict';
import { can, route } from '../src/protocol.mjs';
import { AI_LIMITS, PROVIDERS, STT, STT_FEATURE, aiRoute, buildChat, checkEndpoint, consentText, neuronsFor, pickAnswer, validFeature, validModel, wavInfo } from '../src/ai/routes.mjs';
import { keyHint, seal, unseal } from '../src/ai/seal.mjs';

const q = s => new URLSearchParams(s);

test('AI 길 · 권한: 부르기는 주인 · 사람 편집자만, 키 · 동의 · 기록은 주인만', () => {
  assert.deepEqual(route('POST', '/api/v1/ai/chat', q('')), { kind: 'ai', cap: 'ai', op: 'chat' });
  assert.deepEqual(route('POST', '/api/v1/ai/transcribe', q('')), { kind: 'ai', cap: 'ai', op: 'transcribe' });
  assert.equal(route('GET', '/api/v1/ai/settings', q('')).cap, 'ai');
  assert.equal(route('PUT', '/api/v1/ai/settings', q('')).cap, 'manage');
  assert.equal(route('POST', '/api/v1/ai/consent', q('')).cap, 'manage');
  assert.equal(route('GET', '/api/v1/ai/log', q('')).cap, 'manage');
  for (const [m, p] of [['GET', '/api/v1/ai/chat'], ['PUT', '/api/v1/ai/chat'], ['DELETE', '/api/v1/ai/settings'], ['GET', '/api/v1/ai/x']]) assert.equal(aiRoute(m, p), null, `${m} ${p}`);
  assert.ok(can('owner', 'ai') && can('member-editor', 'ai'));
  for (const r of ['member-viewer', 'output', 'remote', 'editor', 'bridge']) assert.ok(!can(r, 'ai'), `${r} 는 AI 를 못 부른다`);
  assert.ok(validFeature('cuecheck') && validFeature(STT_FEATURE) && !validFeature('auto') && !validFeature(''));
});

test('AI 주소: https 인터넷 이름만 (내부망 · IP · 계정 정보 · 시험 주소 막음)', () => {
  for (const p of PROVIDERS) assert.ok(checkEndpoint(p.endpoint).ok, p.endpoint);
  assert.deepEqual(checkEndpoint(''), { ok: true, endpoint: '' });
  for (const bad of ['http://api.deepseek.com/x', 'https://127.0.0.1/v1', 'https://localhost/v1', 'https://10.0.0.5/v1', 'https://[::1]/v1', 'https://intranet/v1', 'https://a.local/v1', 'https://u:p@api.x.com/v1', 'ftp://a.b', 'nonsense']) {
    assert.equal(checkEndpoint(bad).ok, false, bad);
  }
  assert.equal(checkEndpoint('mock://echo').ok, false, '시험 주소는 AI_MOCK 없이는 안 됨');
  assert.equal(checkEndpoint('mock://echo', { AI_MOCK: '1' }).ok, true);
  assert.ok(validModel('anthropic/claude-haiku-5.5') && validModel('deepseek-chat') && !validModel('a b') && !validModel(''));
});

test('본문: 설치판 aihelp.rs request_body 모양, 모델은 교회 설정 것, 한도', () => {
  const r = buildChat({ feature: 'lyrics', model: 'expensive/model', messages: [{ role: 'system', content: 's' }, { role: 'user', content: '가사' }], response_format: { type: 'json_object' }, max_tokens: 99999, temperature: 5 }, 'deepseek-chat');
  assert.ok(r.ok);
  assert.deepEqual(r.body, { model: 'deepseek-chat', messages: [{ role: 'system', content: 's' }, { role: 'user', content: '가사' }], temperature: 0.2, max_tokens: AI_LIMITS.maxTokens, response_format: { type: 'json_object' } });
  assert.equal(buildChat({ messages: [] }, 'm').status, 400);
  assert.equal(buildChat({ messages: [{ role: 'system', content: 's' }] }, 'm').status, 400, '사용자 글 없음');
  assert.equal(buildChat({ messages: [{ role: 'tool', content: 'x' }] }, 'm').status, 400);
  assert.equal(buildChat({ messages: [{ role: 'user', content: 'x'.repeat(AI_LIMITS.chars + 1) }] }, 'm').status, 413);
  assert.equal(buildChat({ messages: Array(AI_LIMITS.messages + 1).fill({ role: 'user', content: 'x' }) }, 'm').status, 400);
  assert.deepEqual(pickAnswer({ choices: [{ message: { content: [{ text: 'a' }, { text: 'b' }] } }], usage: { prompt_tokens: 3, completion_tokens: 4 } }), { text: 'ab', usage: { prompt_tokens: 3, completion_tokens: 4, total_tokens: 7 } });
  assert.equal(pickAnswer(null).text, '');
});

test('키 봉인: 풀면 같은 키, 다른 교회 · 다른 비밀값으로는 안 풀림, 원문이 안 보임', async () => {
  const env = { AUTH_SECRET: 'x'.repeat(40) };
  const s = await seal(env, 'sample', 'sk-or-v1-abcdef123456');
  assert.match(s, /^v1\.[\w-]+\.[\w-]+$/);
  assert.ok(!s.includes('abcdef'));
  assert.equal(await unseal(env, 'sample', s), 'sk-or-v1-abcdef123456');
  assert.equal(await unseal(env, 'other', s), null, '다른 교회 줄로 옮기면 안 풀림');
  assert.equal(await unseal({ AUTH_SECRET: 'y'.repeat(40) }, 'sample', s), null);
  assert.equal(await unseal({ AUTH_SECRET: 'x'.repeat(40), AI_SECRET: 'z'.repeat(40) }, 'sample', s), null, 'AI_SECRET 이 있으면 그것으로');
  assert.notEqual(await seal(env, 'sample', 'k'.repeat(10)), await seal(env, 'sample', 'k'.repeat(10)), '같은 키도 매번 다른 봉인');
  assert.equal(keyHint('sk-1234567890'), '…7890');
  assert.equal(keyHint('short'), '');
});

test('동의 글: 무엇이 · 어디로 · 저장 안 함 · 사람이 누를 때만 · 한도', () => {
  const t = consentText('sermon', 'api.deepseek.com', 30, 60);
  for (const w of ['설교 원고 글', 'api.deepseek.com', '저장하지 않고', '단추를 누를 때만', '30번']) assert.ok(t.includes(w), w);
  const s = consentText(STT_FEATURE, 'Cloudflare Workers AI', 30, 60);
  for (const w of ['마이크 소리', 'Workers AI', '무료 할당', '60분', "'다음' 칸까지만"]) assert.ok(s.includes(w), w);
  assert.equal(consentText('nope', 'x', 1, 1), '');
});

function wav(seconds, rate = 16000) {
  const n = Math.round(seconds * rate), b = Buffer.alloc(44 + n * 2);
  b.write('RIFF', 0); b.writeUInt32LE(36 + n * 2, 4); b.write('WAVE', 8); b.write('fmt ', 12); b.writeUInt32LE(16, 16);
  b.writeUInt16LE(1, 20); b.writeUInt16LE(1, 22); b.writeUInt32LE(rate, 24); b.writeUInt32LE(rate * 2, 28); b.writeUInt16LE(2, 32); b.writeUInt16LE(16, 34);
  b.write('data', 36); b.writeUInt32LE(n * 2, 40);
  return b;
}
test('WAV 길이 · neurons (무료 할당 계산)', () => {
  const w = wavInfo(wav(2.5));
  assert.ok(w.ok);
  assert.equal(Math.round(w.seconds * 10) / 10, 2.5);
  assert.equal(wavInfo(Buffer.from('not a wav file at all, definitely not')).ok, false);
  const f = wav(1); f.writeUInt16LE(3, 20); // float
  assert.equal(wavInfo(f).ok, false);
  assert.equal(neuronsFor(60), STT.neuronsPerMinute);
  assert.equal(neuronsFor(0.2), Math.ceil(STT.neuronsPerMinute / 60 * 100) / 100, '1초로 올림');
  assert.ok(STT.neuronsPerMinute >= 46.63, '문서 값보다 작게 잡지 않는다');
  assert.ok(STT.accountNeuronsPerDay < 10000, '계정 무료 할당(10,000)보다 아래에서 멈춤');
  assert.ok(STT.accountNeuronsPerDay / STT.neuronsPerMinute > 120, '그래도 하루 두 시간 넘게');
});
