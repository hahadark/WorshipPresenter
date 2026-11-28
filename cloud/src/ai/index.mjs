// 5단계 AI (클라우드 쪽). index.mjs 가 로그인 · 권한(ai/manage) · 쓰기 보호(같은 곳 + X-WorshipCue)를 본 뒤 handle() 을 부른다.
//   GET  /api/v1/ai/settings            주소 · 모델 · 키 있음/힌트 · 한도 · 오늘 사용량 · 기능별 동의       (ai)
//   PUT  /api/v1/ai/settings            {endpoint?, model?, key?(''=지움), dailyLimit?, sttMinutes?}        (manage)
//   GET  /api/v1/ai/consent?feature=    동의 화면 글                                                         (ai)
//   POST /api/v1/ai/consent             {feature, agree}                                                     (manage)
//   POST /api/v1/ai/chat                OpenAI 호환 본문 + feature → {choices, usage} (교회 키로 대신 부름)   (ai, X-WP-Action: user)
//   POST /api/v1/ai/transcribe?feature=stt-cloud   WAV 본문 → {text} (Workers AI whisper, 무료 할당 안에서만) (ai, X-WP-Action: user)
//   GET  /api/v1/ai/log?limit=          부름 기록 (내용 없음)                                                (manage)
// 저절로 부르는 길은 없다: 타이머 · 알람 · 동기화 어디에서도 이 모듈을 부르지 않는다. 시험은 가짜 공급자(AI_MOCK=1 일 때 mock://)로만.
import { fail, json } from '../protocol.mjs';
import { readBody } from '../data/documents.mjs';
import { actorOf } from '../data/store.mjs';
import { seal, unseal, keyHint, sealSecret } from './seal.mjs';
import {
  AI_LIMITS, DEFAULT_ENDPOINT, DEFAULT_MODEL, FEATURES, PROVIDERS, STT, STT_FEATURE, ALL_FEATURES,
  buildChat, checkEndpoint, consentText, dayOf, featureInfo, hostOf, neuronsFor, pickAnswer, validFeature, validModel, wavInfo,
} from './routes.mjs';

const rid = () => [...crypto.getRandomValues(new Uint8Array(8))].map(b => b.toString(16).padStart(2, '0')).join('');

export async function handle(request, env, url, s, r) {
  switch (r.op) {
    case 'settings': return settings(env, s);
    case 'save': return save(request, env, s);
    case 'consentText': return consentGet(env, url, s);
    case 'consent': return consentSet(request, env, s);
    case 'chat': return chat(request, env, s);
    case 'transcribe': return transcribe(request, env, url, s);
    case 'log': return log(env, url, s);
    default: return fail(404, '없는 주소입니다.');
  }
}

async function conf(env, church) {
  const row = await env.DB.prepare('SELECT endpoint, model, key_sealed AS sealed, key_hint AS hint, daily_limit AS dailyLimit, stt_minutes AS sttMinutes FROM ai_settings WHERE church_id = ?1').bind(church).first();
  const c = row || { endpoint: '', model: '', sealed: '', hint: '', dailyLimit: AI_LIMITS.dailyDefault, sttMinutes: AI_LIMITS.sttMinutesDefault };
  return { ...c, endpointUsed: c.endpoint || DEFAULT_ENDPOINT, modelUsed: c.model || (PROVIDERS.find(p => p.endpoint === (c.endpoint || DEFAULT_ENDPOINT))?.model || DEFAULT_MODEL) };
}
async function consents(env, church) {
  const r = await env.DB.prepare('SELECT feature, consented_by AS by, consented_at AS at FROM ai_consent WHERE church_id = ?1').bind(church).all();
  return Object.fromEntries(r.results.map(x => [x.feature, x]));
}
async function usage(env, church, day = dayOf()) {
  return (await env.DB.prepare('SELECT calls, tokens, stt_seconds AS sttSeconds FROM ai_usage WHERE church_id = ?1 AND day = ?2').bind(church, day).first()) || { calls: 0, tokens: 0, sttSeconds: 0 };
}

// ---------- 설정 ----------
async function settings(env, s) {
  const c = await conf(env, s.church);
  const ok = await consents(env, s.church);
  const acct = (await env.DB.prepare('SELECT neurons FROM ai_account WHERE day = ?1').bind(dayOf()).first())?.neurons || 0;
  return json({
    ok: true,
    endpoint: c.endpoint, model: c.model, endpointUsed: c.endpointUsed, modelUsed: c.modelUsed, host: hostOf(c.endpointUsed),
    hasKey: !!c.sealed, keyHint: c.hint, dailyLimit: c.dailyLimit, sttMinutes: c.sttMinutes,
    today: await usage(env, s.church),
    providers: PROVIDERS,
    features: ALL_FEATURES.map(f => ({ id: f, ...featureInfo(f), consented: !!ok[f], by: ok[f]?.by || '', at: ok[f]?.at || 0 })),
    stt: { available: !!env.AI || env.AI_MOCK === '1', model: STT.model, accountNeuronsToday: acct, accountNeuronsCap: STT.accountNeuronsPerDay },
  });
}

async function save(request, env, s) {
  const b = await readBody(request, 4000);
  if (b.error) return b.error;
  const a = b.value || {};
  const c = await conf(env, s.church);
  let { endpoint, model, sealed, hint, dailyLimit, sttMinutes } = c;
  if (a.endpoint !== undefined) {
    const e = checkEndpoint(a.endpoint, env);
    if (!e.ok) return fail(400, e.error);
    endpoint = e.endpoint;
  }
  if (a.model !== undefined) {
    const m = String(a.model || '').trim();
    if (m && !validModel(m)) return fail(400, '모델 이름이 올바르지 않습니다.');
    model = m;
  }
  if (a.key !== undefined) {
    const k = String(a.key || '').trim();
    if (k && (k.length < 8 || k.length > 400 || /\s/.test(k))) return fail(400, 'AI 키 모양이 올바르지 않습니다.');
    if (k && !sealSecret(env)) return fail(503, '키를 봉인할 비밀값이 서버에 없습니다.');
    sealed = k ? await seal(env, s.church, k) : '';
    hint = k ? keyHint(k) : '';
  }
  if (a.dailyLimit !== undefined) {
    if (!Number.isInteger(a.dailyLimit) || a.dailyLimit < 0 || a.dailyLimit > AI_LIMITS.dailyMax) return fail(400, `하루 한도는 0~${AI_LIMITS.dailyMax} 번입니다.`);
    dailyLimit = a.dailyLimit;
  }
  if (a.sttMinutes !== undefined) {
    if (!Number.isInteger(a.sttMinutes) || a.sttMinutes < 0 || a.sttMinutes > AI_LIMITS.sttMinutesMax) return fail(400, `음성 인식 하루 한도는 0~${AI_LIMITS.sttMinutesMax} 분입니다.`);
    sttMinutes = a.sttMinutes;
  }
  await env.DB.prepare(
    `INSERT INTO ai_settings (church_id, endpoint, model, key_sealed, key_hint, daily_limit, stt_minutes, updated_at, updated_by) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
     ON CONFLICT (church_id) DO UPDATE SET endpoint = ?2, model = ?3, key_sealed = ?4, key_hint = ?5, daily_limit = ?6, stt_minutes = ?7, updated_at = ?8, updated_by = ?9`,
  ).bind(s.church, endpoint, model, sealed, hint, dailyLimit, sttMinutes, Date.now(), actorOf(s)).run();
  // 주소가 바뀌면 글이 가는 곳이 바뀐다 → 동의를 다시 받는다 (설치판 동의 창은 '어디로' 를 보여 주고 받은 것)
  if (a.endpoint !== undefined && (endpoint || DEFAULT_ENDPOINT) !== c.endpointUsed) {
    await env.DB.prepare(`DELETE FROM ai_consent WHERE church_id = ?1 AND feature != ?2`).bind(s.church, STT_FEATURE).run();
  }
  return settings(env, s);
}

// ---------- 동의 ----------
async function consentGet(env, url, s) {
  const f = url.searchParams.get('feature') || '';
  if (!validFeature(f)) return fail(400, '모르는 기능입니다.');
  const c = await conf(env, s.church);
  const host = f === STT_FEATURE ? 'Cloudflare Workers AI' : hostOf(c.endpointUsed);
  const ok = (await consents(env, s.church))[f];
  return json({ ok: true, feature: f, ...featureInfo(f), host, text: consentText(f, host, c.dailyLimit, c.sttMinutes), consented: !!ok, by: ok?.by || '', at: ok?.at || 0 });
}
async function consentSet(request, env, s) {
  const b = await readBody(request, 1000);
  if (b.error) return b.error;
  const { feature, agree } = b.value || {};
  if (!validFeature(feature)) return fail(400, '모르는 기능입니다.');
  if (agree === true) {
    await env.DB.prepare('INSERT INTO ai_consent (church_id, feature, consented_by, consented_at) VALUES (?1, ?2, ?3, ?4) ON CONFLICT (church_id, feature) DO UPDATE SET consented_by = ?3, consented_at = ?4')
      .bind(s.church, feature, actorOf(s), Date.now()).run();
  } else {
    await env.DB.prepare('DELETE FROM ai_consent WHERE church_id = ?1 AND feature = ?2').bind(s.church, feature).run();
  }
  return json({ ok: true, feature, consented: agree === true });
}

// ---------- 공통 문지기 ----------
/** 사람이 누른 요청인가 (웹 조작 화면은 단추 처리 안에서만 이 머리글을 붙인다 — 계약) · 기능 · 동의 · 분당 한도 */
async function gate(request, env, s, feature, kind) {
  if (request.headers.get('x-wp-action') !== 'user') return fail(428, 'AI 는 사람이 단추를 눌렀을 때만 부릅니다 (X-WP-Action: user).');
  if (!validFeature(feature) || (kind === 'stt') !== (feature === STT_FEATURE)) return fail(400, '모르는 기능입니다.');
  const ok = await env.DB.prepare('SELECT 1 AS y FROM ai_consent WHERE church_id = ?1 AND feature = ?2').bind(s.church, feature).first();
  if (!ok) return fail(403, `${featureInfo(feature).name}: 먼저 주인이 동의해야 합니다 (무엇이 · 어디로 가는지 확인).`, { needConsent: true, feature });
  const per = kind === 'stt' ? AI_LIMITS.sttPerMinute : AI_LIMITS.perMinute;
  const recent = await env.DB.prepare('SELECT COUNT(*) AS n FROM ai_calls WHERE church_id = ?1 AND kind = ?2 AND at > ?3').bind(s.church, kind, Date.now() - 60_000).first();
  if ((recent?.n || 0) >= per) return fail(429, '너무 자주 부릅니다. 1분 뒤에 다시 해 보세요.');
  return null;
}

async function note(env, s, rec) {
  const now = Date.now();
  await env.DB.batch([
    env.DB.prepare('INSERT INTO ai_calls (church_id, id, at, by, feature, kind, host, model, status, ms, chars, tokens, seconds) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)')
      .bind(s.church, rid(), now, actorOf(s), rec.feature, rec.kind, rec.host || '', rec.model || '', rec.status, rec.ms || 0, rec.chars || 0, rec.tokens || 0, rec.seconds || 0),
    env.DB.prepare('UPDATE ai_usage SET tokens = tokens + ?3 WHERE church_id = ?1 AND day = ?2').bind(s.church, dayOf(now), rec.tokens || 0),
    // 기록은 90일만 둔다
    env.DB.prepare('DELETE FROM ai_calls WHERE church_id = ?1 AND at < ?2').bind(s.church, now - 90 * 86400_000),
  ]);
}

/** 하루 한도: 부르기 전에 한 칸을 잡는다 (조건부 UPDATE — 동시에 와도 한도를 넘지 않음) */
async function reserveCall(env, church, limit) {
  const day = dayOf();
  await env.DB.prepare('INSERT INTO ai_usage (church_id, day) VALUES (?1, ?2) ON CONFLICT (church_id, day) DO NOTHING').bind(church, day).run();
  const r = await env.DB.prepare('UPDATE ai_usage SET calls = calls + 1 WHERE church_id = ?1 AND day = ?2 AND calls < ?3').bind(church, day, limit).run();
  return r.meta.changes > 0;
}

// ---------- AI (글) ----------
async function chat(request, env, s) {
  const b = await readBody(request, AI_LIMITS.chars * 4);
  if (b.error) return b.error;
  const a = b.value || {};
  const g = await gate(request, env, s, a.feature, 'chat');
  if (g) return g;
  const c = await conf(env, s.church);
  if (!c.sealed) return fail(409, 'AI 키가 필요합니다. 주인이 AI 설정에서 교회 키를 넣으세요 (규칙으로 하는 기능은 키 없이도 됩니다).', { needKey: true });
  const req = buildChat(a, c.modelUsed);
  if (!req.ok) return fail(req.status, req.error);
  const key = await unseal(env, s.church, c.sealed);
  if (!key) return fail(503, '봉인한 AI 키를 풀지 못했습니다. 주인이 키를 다시 넣어 주세요.');
  if (!(await reserveCall(env, s.church, c.dailyLimit))) return fail(429, `오늘 AI 부름 한도(${c.dailyLimit}번)를 다 썼습니다. 내일(UTC 0시) 다시 쓸 수 있고, 주인이 한도를 바꿀 수 있습니다.`, { limit: true });
  const host = hostOf(c.endpointUsed);
  const t0 = Date.now();
  let status = 0, v = null;
  try {
    if (c.endpointUsed.startsWith('mock://')) {
      if (env.AI_MOCK !== '1') throw new Error('시험용 주소는 쓸 수 없습니다.');
      ({ status, v } = mockChat(req.body, key));
    } else {
      const res = await fetch(c.endpointUsed, {
        method: 'POST',
        headers: { authorization: `Bearer ${key}`, 'content-type': 'application/json', 'HTTP-Referer': 'https://github.com/hahadark/WorshipPresenter', 'X-Title': 'Worship Presenter' },
        body: JSON.stringify(req.body),
        signal: AbortSignal.timeout(AI_LIMITS.timeoutMs),
      });
      status = res.status;
      v = await res.json().catch(() => null);
    }
  } catch (e) {
    await note(env, s, { feature: a.feature, kind: 'chat', host, model: c.modelUsed, status: 0, ms: Date.now() - t0, chars: req.chars });
    return fail(502, `AI 연결 실패: ${String(e?.message || e).slice(0, 120)}`);
  }
  const ans = pickAnswer(v);
  await note(env, s, { feature: a.feature, kind: 'chat', host, model: c.modelUsed, status, ms: Date.now() - t0, chars: req.chars, tokens: ans.usage.total_tokens });
  if (status < 200 || status >= 300) {
    const m = String(v?.error?.message || '').replaceAll(key, '***').slice(0, 200);
    return fail(502, `AI 오류 (${status}): ${m}`, { providerStatus: status });
  }
  if (!ans.text.trim()) return fail(502, 'AI 응답이 비었습니다.');
  return json({ ok: true, model: c.modelUsed, choices: [{ message: { role: 'assistant', content: ans.text } }], usage: ans.usage });
}

/** 가짜 공급자 (시험 · 점검 전용, AI_MOCK=1). mock://fail 이면 401 오류 */
function mockChat(body, key) {
  const user = [...body.messages].reverse().find(m => m.role === 'user')?.content || '';
  if (key.includes('bad')) return { status: 401, v: { error: { message: `Invalid key ${key}` } } };
  const content = body.response_format ? JSON.stringify({ mock: true, model: body.model, chars: user.length, maxTokens: body.max_tokens }) : `[가짜 AI] ${user.slice(0, 80)}`;
  return { status: 200, v: { choices: [{ message: { content } }], usage: { prompt_tokens: Math.ceil(user.length / 2), completion_tokens: 10, total_tokens: Math.ceil(user.length / 2) + 10 } } };
}

// ---------- 음성 인식 (Workers AI whisper, 무료 할당 안에서만) ----------
async function transcribe(request, env, url, s) {
  const g = await gate(request, env, s, url.searchParams.get('feature') || STT_FEATURE, 'stt');
  if (g) return g;
  if (!env.AI && env.AI_MOCK !== '1') return fail(503, '클라우드 음성 인식이 켜져 있지 않습니다 (Workers AI 연결 없음). 설치판의 PC 안 음성 인식을 쓰세요.', { unavailable: true });
  const len = Number(request.headers.get('content-length') || 0);
  if (len > STT.maxBytes) return fail(413, `소리 조각이 너무 큽니다 (${STT.maxSeconds}초 · 2MB 까지).`);
  const buf = new Uint8Array(await request.arrayBuffer());
  if (buf.length > STT.maxBytes) return fail(413, `소리 조각이 너무 큽니다 (${STT.maxSeconds}초 · 2MB 까지).`);
  const w = wavInfo(buf);
  if (!w.ok) return fail(415, w.error);
  if (w.seconds > STT.maxSeconds + 0.5) return fail(413, `소리 조각은 ${STT.maxSeconds}초까지입니다.`);
  const c = await conf(env, s.church);
  const secs = Math.max(1, Math.ceil(w.seconds));
  const day = dayOf();
  // 1) 교회 하루 분 한도  2) 계정 전체 무료 할당 — 둘 다 부르기 전에 조건부로 잡는다. 넘으면 부르지 않고 멈춘다
  await env.DB.prepare('INSERT INTO ai_usage (church_id, day) VALUES (?1, ?2) ON CONFLICT (church_id, day) DO NOTHING').bind(s.church, day).run();
  const mine = await env.DB.prepare('UPDATE ai_usage SET stt_seconds = stt_seconds + ?3 WHERE church_id = ?1 AND day = ?2 AND stt_seconds + ?3 <= ?4').bind(s.church, day, secs, c.sttMinutes * 60).run();
  if (!mine.meta.changes) return fail(429, `오늘 클라우드 음성 인식 한도(${c.sttMinutes}분)를 다 썼습니다. 설치판의 PC 안 음성 인식을 쓰세요.`, { limit: true });
  const n = neuronsFor(w.seconds);
  await env.DB.prepare('INSERT INTO ai_account (day) VALUES (?1) ON CONFLICT (day) DO NOTHING').bind(day).run();
  const acct = await env.DB.prepare('UPDATE ai_account SET neurons = neurons + ?2 WHERE day = ?1 AND neurons + ?2 <= ?3').bind(day, n, STT.accountNeuronsPerDay).run();
  if (!acct.meta.changes) {
    await env.DB.prepare('UPDATE ai_usage SET stt_seconds = stt_seconds - ?3 WHERE church_id = ?1 AND day = ?2').bind(s.church, day, secs).run();
    return fail(429, '오늘 무료 음성 인식 할당을 다 써서 멈췄습니다 (요금이 들지 않게). 내일(UTC 0시) 다시 되고, 그동안은 설치판의 PC 안 음성 인식을 쓰세요.', { limit: true, free: true });
  }
  const t0 = Date.now();
  let text = '', status = 200;
  try {
    if (env.AI_MOCK === '1' && !env.AI) {
      text = `[가짜 음성 인식] ${w.seconds.toFixed(1)}초`;
    } else {
      let bin = '';
      for (let i = 0; i < buf.length; i += 0x8000) bin += String.fromCharCode(...buf.subarray(i, i + 0x8000));
      const r = await env.AI.run(STT.model, { audio: btoa(bin), language: 'ko', task: 'transcribe' });
      text = String(r?.text || '').trim();
    }
  } catch (e) {
    status = 0;
    await note(env, s, { feature: STT_FEATURE, kind: 'stt', host: 'workers-ai', model: STT.model, status, ms: Date.now() - t0, seconds: w.seconds });
    return fail(502, `음성 인식 실패: ${String(e?.message || e).slice(0, 120)}`);
  }
  await note(env, s, { feature: STT_FEATURE, kind: 'stt', host: 'workers-ai', model: STT.model, status, ms: Date.now() - t0, seconds: w.seconds });
  // 결과 글만 돌려준다 (저장하지 않음). 웹 조작 화면은 이것을 '다음' 칸까지만 쓴다 (docs/ai-features.md 지킬 것)
  return json({ ok: true, text, seconds: Math.round(w.seconds * 10) / 10, neurons: n });
}

// ---------- 기록 ----------
async function log(env, url, s) {
  const limit = Math.min(Math.max(Number(url.searchParams.get('limit')) || 50, 1), 200);
  const r = await env.DB.prepare('SELECT at, by, feature, kind, host, model, status, ms, chars, tokens, seconds FROM ai_calls WHERE church_id = ?1 ORDER BY at DESC LIMIT ?2').bind(s.church, limit).all();
  const days = await env.DB.prepare('SELECT day, calls, tokens, stt_seconds AS sttSeconds FROM ai_usage WHERE church_id = ?1 ORDER BY day DESC LIMIT 14').bind(s.church).all();
  return json({ ok: true, calls: r.results, days: days.results, features: FEATURES });
}
