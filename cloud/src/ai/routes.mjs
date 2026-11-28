// 5단계 AI 길 · 한도 · 동의 글 · 주소 검사 · WAV 길이 (Cloudflare 전용 API 를 쓰지 않는다 — node 시험과 같이 씀). 계약: docs/web/ai.md
// 설치판 aihelp.rs 와 같은 길: OpenAI 호환 chat/completions 주소 하나 + 교회 키 + 모델. 웹은 키를 브라우저에 두지 않고 Worker 가 대신 부른다.

/** 이름 붙은 OpenAI 호환 주소 (설치판 aihelp.rs PROVIDERS 와 같게) */
export const PROVIDERS = [
  { name: '오픈라우터', endpoint: 'https://openrouter.ai/api/v1/chat/completions', model: 'anthropic/claude-haiku-5.5' },
  { name: 'DeepSeek', endpoint: 'https://api.deepseek.com/chat/completions', model: 'deepseek-chat' },
  { name: 'OpenAI', endpoint: 'https://api.openai.com/v1/chat/completions', model: 'gpt-6-luna' },
];
export const DEFAULT_ENDPOINT = PROVIDERS[0].endpoint;
export const DEFAULT_MODEL = PROVIDERS[0].model;

/** 기능 (동의 열쇠) → 이름 · 무엇이 가는지. 앞의 여섯은 설치판 aihelp.rs FEATURES 와 같고, bulletin 은 주보 → 큐시트(bulletin.rs) */
export const FEATURES = {
  cuecheck: { name: '예배 전 큐시트 점검 (AI)', what: '큐시트의 찬양 가사 · 공지 글' },
  sermon: { name: '설교 원고 → 슬라이드 (AI)', what: '설교 원고 글' },
  lyrics: { name: '가사 자동 정리 (AI)', what: '붙여 넣은 가사 글' },
  translate: { name: '실시간 자막 번역', what: '음성 인식으로 받아 적은 글 (소리는 보내지 않음)' },
  recap: { name: '예배 후 정리 (AI)', what: '순서 목록 · 송출 시각 · 받아 적은 설교 글 (소리는 보내지 않음)' },
  say: { name: '말로 시키기 (AI)', what: '입력한 말 · 큐시트 이름과 순서 제목' },
  bulletin: { name: '주보로 큐시트 만들기 (AI)', what: '주보 글 (사진 · PDF 는 그 그림)' },
};
/** 음성 인식 (클라우드, Cloudflare Workers AI whisper) — 교회 키가 아니라 계정의 무료 할당을 쓴다 */
export const STT_FEATURE = 'stt-cloud';
export const STT = {
  name: '음성 인식 (클라우드)',
  what: '마이크 소리 조각 (한 번에 30초까지)',
  model: '@cf/openai/whisper-large-v3-turbo',
  /** 문서 값(46.63 neurons/분, 2026-11-28 pricing 쪽)보다 넉넉히 잡는다 — 모델 쪽 값($0.000513/분)과 어긋남이 있어서 */
  neuronsPerMinute: 52,
  /** 계정 전체 무료 할당은 하루 10,000 neurons. 같은 계정의 다른 앱 몫을 남기고 이 앱은 여기까지만 */
  accountNeuronsPerDay: 8000,
  maxSeconds: 30,
  maxBytes: 2 * 1024 * 1024,
};
export const ALL_FEATURES = [...Object.keys(FEATURES), STT_FEATURE];
export const validFeature = f => typeof f === 'string' && ALL_FEATURES.includes(f);
export const featureInfo = f => (f === STT_FEATURE ? { name: STT.name, what: STT.what } : FEATURES[f] || null);

export const AI_LIMITS = {
  /** 교회 하루 AI(글) 부름 기본 · 최대 (주인이 바꿈) */
  dailyDefault: 30,
  dailyMax: 500,
  /** 1분에 (연타 · 자동 반복 막기) */
  perMinute: 6,
  /** 보내는 글 합 · 메시지 수 · 답 길이 */
  chars: 60_000,
  messages: 20,
  maxTokens: 4000,
  maxTokensDefault: 1500,
  /** 음성 인식: 교회 하루 분 · 1분에 조각 수 */
  sttMinutesDefault: 60,
  sttMinutesMax: 180,
  sttPerMinute: 20,
  /** 공급자 기다림 */
  timeoutMs: 90_000,
};

/** 오늘 (UTC 날짜 — Cloudflare 무료 할당이 UTC 0시에 새로 찬다) */
export const dayOf = (ms = Date.now()) => new Date(ms).toISOString().slice(0, 10);

/**
 * 5단계 길 → { kind:'ai', cap, op }. 모르는 길은 null.
 * cap: ai 부르기(주인 · 사람 편집자) · manage 키 · 동의 · 기록(주인)
 */
export function aiRoute(method, path) {
  if (!path.startsWith('/api/v1/ai/')) return null;
  const get = method === 'GET' || method === 'HEAD';
  const p = path.slice('/api/v1/ai/'.length);
  if (p === 'settings' && get) return { kind: 'ai', cap: 'ai', op: 'settings' };
  if (p === 'settings' && method === 'PUT') return { kind: 'ai', cap: 'manage', op: 'save' };
  if (p === 'consent' && get) return { kind: 'ai', cap: 'ai', op: 'consentText' };
  if (p === 'consent' && method === 'POST') return { kind: 'ai', cap: 'manage', op: 'consent' };
  if (p === 'chat' && method === 'POST') return { kind: 'ai', cap: 'ai', op: 'chat' };
  if (p === 'transcribe' && method === 'POST') return { kind: 'ai', cap: 'ai', op: 'transcribe' };
  if (p === 'log' && get) return { kind: 'ai', cap: 'manage', op: 'log' };
  return null;
}

/** 주소의 호스트 (동의 글에 보여 줄 '어디로') */
export function hostOf(endpoint) {
  try { return new URL(endpoint).host; } catch { return String(endpoint || ''); }
}

/**
 * 교회가 넣는 AI 주소: https 만, 이름 있는 호스트만 (IP · localhost · 내부 이름 금지 — Worker 가 남의 내부망을 부르지 않게).
 * 가짜 공급자 'mock://…' 는 시험 환경(AI_MOCK=1)에서만.
 */
export function checkEndpoint(endpoint, env = {}) {
  const e = String(endpoint || '').trim();
  if (!e) return { ok: true, endpoint: '' };
  if (e.startsWith('mock://')) return env.AI_MOCK === '1' ? { ok: true, endpoint: e } : { ok: false, error: '시험용 주소는 쓸 수 없습니다.' };
  let u;
  try { u = new URL(e); } catch { return { ok: false, error: 'AI 주소가 올바르지 않습니다.' }; }
  const h = u.hostname.toLowerCase();
  if (u.protocol !== 'https:') return { ok: false, error: 'AI 주소는 https:// 로 시작해야 합니다.' };
  if (u.username || u.password) return { ok: false, error: '주소에 계정 정보를 넣지 마세요 (키는 따로 넣습니다).' };
  if (!h.includes('.') || h === 'localhost' || h.endsWith('.localhost') || h.endsWith('.local') || h.endsWith('.internal') || /^[\d.]+$/.test(h) || h.includes(':') || h.startsWith('[')) {
    return { ok: false, error: 'AI 주소는 인터넷 이름(예: api.deepseek.com)이어야 합니다.' };
  }
  if (e.length > 300) return { ok: false, error: 'AI 주소가 너무 깁니다.' };
  return { ok: true, endpoint: u.href };
}

/** 모델 이름 (공급자마다 'vendor/model:tag' 꼴) */
export const validModel = m => typeof m === 'string' && /^[A-Za-z0-9][A-Za-z0-9._:/@+-]{0,119}$/.test(m);

/**
 * 브라우저가 보낸 요청 → 공급자에게 보낼 본문 (설치판 aihelp.rs request_body 와 같은 모양).
 * 모델은 교회 설정의 것을 쓴다(브라우저가 비싼 모델로 바꾸지 못하게). 돌려줌: { ok, body, chars } · { ok:false, status, error }
 */
export function buildChat(a, model) {
  if (!a || typeof a !== 'object') return { ok: false, status: 400, error: '잘못된 요청' };
  const msgs = a.messages;
  if (!Array.isArray(msgs) || !msgs.length || msgs.length > AI_LIMITS.messages) return { ok: false, status: 400, error: '메시지가 없거나 너무 많습니다.' };
  let chars = 0;
  const messages = [];
  for (const m of msgs) {
    if (!m || !['system', 'user', 'assistant'].includes(m.role) || typeof m.content !== 'string') return { ok: false, status: 400, error: '메시지 모양이 올바르지 않습니다 (role · content 글).' };
    chars += m.content.length;
    messages.push({ role: m.role, content: m.content });
  }
  if (!messages.some(m => m.role === 'user' && m.content.trim())) return { ok: false, status: 400, error: '보낼 글이 비었습니다.' };
  if (chars > AI_LIMITS.chars) return { ok: false, status: 413, error: `보낼 글이 너무 깁니다 (${AI_LIMITS.chars.toLocaleString()}자까지).` };
  const maxTokens = Number.isInteger(a.max_tokens) && a.max_tokens > 0 ? Math.min(a.max_tokens, AI_LIMITS.maxTokens) : AI_LIMITS.maxTokensDefault;
  const temperature = typeof a.temperature === 'number' && a.temperature >= 0 && a.temperature <= 1 ? a.temperature : 0.2;
  const body = { model, messages, temperature, max_tokens: maxTokens };
  if (a.response_format && a.response_format.type === 'json_object') body.response_format = { type: 'json_object' };
  return { ok: true, body, chars };
}

/** 공급자 답 → 브라우저에 돌려줄 것 (설치판 aihelp.rs response_text 가 읽는 choices[0].message.content · usage 만) */
export function pickAnswer(v) {
  const c = v?.choices?.[0]?.message?.content;
  const text = typeof c === 'string' ? c : Array.isArray(c) ? c.map(x => (x && typeof x.text === 'string' ? x.text : '')).join('') : '';
  const u = v?.usage || {};
  const usage = {
    prompt_tokens: Number(u.prompt_tokens) || 0,
    completion_tokens: Number(u.completion_tokens) || 0,
    total_tokens: Number(u.total_tokens) || (Number(u.prompt_tokens) || 0) + (Number(u.completion_tokens) || 0),
  };
  return { text, usage };
}

/** 동의 화면 글 (설치판 ai_ui 의 동의 창과 같은 뜻) */
export function consentText(feature, host, dailyLimit, sttMinutes) {
  const f = featureInfo(feature);
  if (!f) return '';
  if (feature === STT_FEATURE) {
    return `${f.name}을(를) 켜면 ${f.what}이(가) Cloudflare Workers AI(whisper)로 가서 글로 바뀝니다. 소리와 받아 적은 글은 클라우드에 저장하지 않고, 언제 · 누가 · 몇 초를 보냈는지만 기록합니다. `
      + `요금이 들지 않는 무료 할당 안에서만 돌고(교회 하루 ${sttMinutes}분까지, 계정 전체 무료 몫을 넘기 전에) 넘으면 멈춥니다. 받아 적은 결과는 '다음' 칸까지만 올라가고 저절로 송출되지 않습니다. `
      + `소리를 밖으로 보내고 싶지 않으면 끄고, 설치판의 PC 안 음성 인식을 쓰세요.`;
  }
  return `${f.name}을(를) 쓰면 ${f.what}이(가) 교회가 넣은 AI 키로 ${host || 'AI 회사'}에 보내집니다. 워십프리젠터 클라우드는 보낸 글과 받은 답을 저장하지 않고, `
    + `언제 · 누가 · 어느 기능으로 · 몇 글자를 보냈는지와 토큰 수만 기록합니다. AI 요금은 그 회사가 교회 키로 받습니다. `
    + `AI 는 사람이 단추를 누를 때만 부르고(저절로 부르지 않음), 교회 전체로 하루 ${dailyLimit}번까지만 부를 수 있습니다.`;
}

/**
 * WAV(PCM) 머리 → 길이(초). 웹 조작 화면은 마이크 소리를 16kHz 모노 WAV 로 잘라 보낸다(설치판 stt.rs 와 같은 꼴).
 * 돌려줌: { ok, seconds, sampleRate, channels } · { ok:false, error }
 */
export function wavInfo(buf) {
  const u = buf instanceof Uint8Array ? buf : new Uint8Array(buf);
  const dv = new DataView(u.buffer, u.byteOffset, u.byteLength);
  const tag = o => String.fromCharCode(u[o], u[o + 1], u[o + 2], u[o + 3]);
  if (u.length < 44 || tag(0) !== 'RIFF' || tag(8) !== 'WAVE') return { ok: false, error: '소리는 WAV 로 보내 주세요.' };
  let o = 12, fmt = null, data = 0;
  while (o + 8 <= u.length) {
    const id = tag(o), size = dv.getUint32(o + 4, true);
    if (id === 'fmt ' && o + 24 <= u.length) fmt = { format: dv.getUint16(o + 8, true), channels: dv.getUint16(o + 10, true), sampleRate: dv.getUint32(o + 12, true), bits: dv.getUint16(o + 22, true) };
    if (id === 'data') { data = Math.min(size, u.length - o - 8); break; }
    o += 8 + size + (size % 2);
  }
  if (!fmt || fmt.format !== 1 || !fmt.channels || !fmt.sampleRate || !fmt.bits) return { ok: false, error: 'WAV 는 PCM 이어야 합니다.' };
  if (!data) return { ok: false, error: '소리가 비었습니다.' };
  const seconds = data / (fmt.sampleRate * fmt.channels * (fmt.bits / 8));
  return { ok: true, seconds, sampleRate: fmt.sampleRate, channels: fmt.channels };
}

/** 소리 길이 → 잡아 둘 neurons (1초 단위로 올림) */
export const neuronsFor = seconds => Math.ceil(Math.max(1, Math.ceil(seconds)) * STT.neuronsPerMinute / 60 * 100) / 100;
