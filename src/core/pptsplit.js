'use strict';
// 한 PPT 를 여러 순서로 나누기 — 예배·행사 전체를 한 파일에 담은 PPT 를 큐시트 순서 여러 개로
//  - 규칙으로 (AI 없이): 짧은 '순서 제목' 슬라이드(찬송 · 기도 · 성경봉독 · 설교 · 광고 · 축도 …) · 찬송가 장 번호 · 성경 장절
//  - API 키로 (오픈라우터, 주보 AI 와 같은 키·모델): 슬라이드 글자 + (글자가 없는 PPT 면) 모아보기 그림
//  - 채팅 AI 로 (키 없이): 프롬프트 복사 → ChatGPT·Claude·Gemini → 답 붙여 넣기
// 결과: parts = [{ a, b, name, kind }] (슬라이드 번호는 1부터, 빠짐없이 · 겹치지 않게 · 차례대로)

const { parseJson, DEFAULT_ENDPOINT, DEFAULT_MODEL } = require('./bulletin');

const KINDS = ['hymn', 'song', 'bible', 'reading', 'prayer', 'sermon', 'notice', 'offering', 'video', 'title'];
const KIND_NAMES = { hymn: '찬송가', song: '찬양', bible: '성경', reading: '교독문', prayer: '기도', sermon: '설교·강의', notice: '광고·소식', offering: '봉헌', video: '영상', title: '제목·그 밖' };
const MAX_PARTS = 200;
const MAX_NAME = 40;

/** 슬라이드 글자 정리: 배열(글상자들)이든 글이든 → 줄들 */
function slideLines(t) {
  const s = Array.isArray(t) ? t.join('\n') : String(t ?? '');
  return s.replace(/\r\n?|\u000b/g, '\n').replace(/[ \t ]+/g, ' ').split('\n').map(l => l.trim()).filter(Boolean);
}
const oneLine = s => String(s ?? '').replace(/\s+/g, ' ').trim();
const clip = (s, n) => { const t = oneLine(s); return t.length <= n ? t : t.slice(0, n - 1).trimEnd() + '…'; };
const tidyName = s => clip(String(s ?? '').replace(/^[\s\-–—·•*※:：]+|[\s\-–—·•:：]+$/g, ''), MAX_NAME);

// ---------------- 규칙으로 (AI 없이) ----------------
// 줄 머리의 순서 이름 → 종류 (순서 제목 슬라이드에서만 본다: 가사·설교 본문 속 '기도' 같은 낱말에 속지 않게)
const SECTIONS = [
  [/^(?:예배\s*(?:로|에)?의?\s*부름|예배\s*부름|부름|묵도|입례|개회|개회\s*(?:예배|선언|사|기도|찬송)|여는\s*(?:예배|찬양)|환영|인사|소개|오프닝)/, 'title'],
  [/^(?:교독문|교독|성시\s*교독)/, 'reading'],
  [/^(?:새\s*찬송가|구세군\s*찬송가|찬송가|찬송|송영|경배\s*찬송)/, 'hymn'],
  [/^(?:찬양|특송|특별\s*찬양|경배와\s*찬양|헌금\s*송|봉헌\s*송|찬양\s*대|찬양\s*팀|축가)/, 'song'],
  [/^(?:성경\s*봉독|성경\s*말씀|본문\s*말씀|말씀\s*봉독|봉독|성경|본문)/, 'bible'],
  [/^(?:대표\s*기도|합심\s*기도|통성\s*기도|중보\s*기도|주\s*기도문?|기도)/, 'prayer'],
  [/^(?:신앙\s*고백|사도\s*신경|교리\s*(?:고백|낭독)?)/, 'title'],
  [/^(?:설교|말씀|강의|특강|강연|발제|주제\s*강의|간증)/, 'sermon'],
  [/^(?:헌금|봉헌)/, 'offering'],
  [/^(?:광고|교회\s*소식|알림|공지|소식|행사\s*안내|안내)/, 'notice'],
  [/^(?:축도|폐회|파송|축복\s*기도|마침|폐회\s*(?:기도|선언|사)|기념\s*촬영|시상|임명|보고|회의|점심|식사|휴식|만찬)/, 'title'],
];
const PART_NO = /^(?:제\s*)?([1-9]\d?)\s*부(?:\s|$|[.:：)])/; // '1부 예배' · '제2부'
// 순서 이름 뒤에 오는 담당자 ('대표기도 홍길동 사관')
const ROLE = /(?:사관|목사|장로|집사|권사|전도사|교역자|정위|참위|부위|참령|부령|정령|사령관|서기장|교수|박사|형제|자매|청년|위원장?|부장|회장|님|팀|찬양대|성가대|일동|다같이|회중|인도자)$/;
/** 줄 머리가 순서 이름인가: '설교' · '설교: 제목' · '대표기도 (홍길동)' · '찬송 288장' · '대표기도 홍길동 사관' — '말씀 앞에 엎드려' 같은 가사는 아님 */
function sectionHead(h, re) {
  const m = h.match(re);
  if (!m) return false;
  const after = h.slice(m[0].length);
  if (/^[가-힣]/.test(after)) return false; // '찬양하세' · '기도하는' 처럼 낱말이 이어짐
  const rest = after.trim();
  if (!rest || /^[:：·\-–—(（\[|/]/.test(rest) || /^\d/.test(rest)) return true;
  const toks = rest.split(/\s+/);
  if (toks.length === 1 && rest.length <= 12) return true; // '설교 홍길동'
  return toks.length <= 3 && ROLE.test(rest);
}
const HYMN_NO = /(새\s*찬송가|구세군\s*찬송가|구세군|찬송가|찬송)?\s*(?:제\s*)?(\d{1,3})\s*장(?![가-힣])/;
const REF = /(?:^|\s)(창|출|레|민|신|수|삿|룻|삼상|삼하|왕상|왕하|대상|대하|스|느|에|욥|시|잠|전|아|사|렘|애|겔|단|호|욜|암|옵|욘|미|나|합|습|학|슥|말|마|막|눅|요|행|롬|고전|고후|갈|엡|빌|골|살전|살후|딤전|딤후|딛|몬|히|약|벧전|벧후|요일|요이|요삼|유|계|[가-힣]{2,6}(?:복음|기|서|편|록|가))\s*(\d{1,3})\s*[:장]\s*(\d{1,3})/;

/** 슬라이드 하나 → { kind, key, name } (새 순서가 시작될 만한 슬라이드) 또는 null */
function classify(lines) {
  if (!lines.length) return null;
  const head = lines[0];
  const total = lines.join('').replace(/\s/g, '').length;
  const short = total <= 60 && lines.length <= 4; // 순서 제목 슬라이드 (글이 적다)
  const top = lines.slice(0, 2).join(' ');
  // 찬송가 장 번호: 머리 두 줄 안에 '찬송가 288장' · '288장 예수를 나의 구주 삼고' (같은 곡의 다음 장도 머리에 번호가 있으면 같은 순서)
  let m = top.match(HYMN_NO);
  if (m && !/^\s*\d{1,3}\s*절/.test(top.slice(top.indexOf(m[0]) + m[0].length)) && !REF.test(top) && (m[1] || /^\s*(?:제\s*)?\d{1,3}\s*장/.test(head) || short)) { // '요한복음 3장 16절' 은 성경
    const after = top.slice(top.indexOf(m[0]) + m[0].length).replace(/^[\s.:：\-–]+/, '');
    const book = m[1] ? m[1].replace(/\s+/g, ' ').trim() : '';
    return { kind: 'hymn', key: `h${m[2]}`, name: tidyName(`${book && book !== '찬송' ? (book === '구세군' ? '구세군 찬송가' : book) + ' ' : '찬송 '}${m[2]}장${after ? ' ' + after.split(/\s{2,}| \/ /)[0] : ''}`) };
  }
  if (!short) return null;
  if ((m = head.match(PART_NO))) return { kind: 'title', key: `p${m[1]}`, name: tidyName(lines.slice(0, 2).join(' ')) };
  const h = head.replace(/^[\s\d.)\-–·•*※]+/, '');
  for (const [re, kind] of SECTIONS) {
    if (!sectionHead(h, re)) continue;
    const name = tidyName(lines.slice(0, Math.min(2, lines.length)).join(' · '));
    return { kind, key: `${kind}:${h.replace(/\s+/g, '').slice(0, 12)}`, name };
  }
  // 성경 장절만 크게 쓴 슬라이드 ('요한복음 3:16-18')
  if ((m = top.match(REF)) && total <= 40) return { kind: 'bible', key: `b${m[1]}${m[2]}`, name: tidyName(`성경봉독 ${m[0]}`) };
  return null;
}

/**
 * 규칙으로 나누기 (AI 없이). texts[k] = k+1 번째 슬라이드 글자 (배열이나 글)
 * → [{ a, b, name, kind }]
 */
function ruleSplit(texts, { title = '' } = {}) {
  const n = Array.isArray(texts) ? texts.length : 0;
  if (!n) throw new Error('슬라이드가 없습니다.');
  const parts = [];
  let cur = null;
  for (let k = 0; k < n; k++) {
    const c = classify(slideLines(texts[k]));
    // 새 순서: 순서 제목·다른 곡이 나올 때 (성경봉독 뒤의 본문 슬라이드는 같은 순서)
    if (!cur || (c && c.key !== cur.key && !(c.kind === 'bible' && cur.kind === 'bible'))) {
      cur = { a: k + 1, b: k + 1, name: c ? c.name : '', kind: c ? c.kind : '', key: c ? c.key : '' };
      parts.push(cur);
    } else cur.b = k + 1;
  }
  // 이름 없는 묶음: 첫 슬라이드의 첫 줄 (없으면 'PPT 3-5')
  for (const p of parts) {
    if (p.name) continue;
    const first = slideLines(texts[p.a - 1])[0];
    p.name = first ? tidyName(first) : `${title || 'PPT'} ${p.a === p.b ? p.a : `${p.a}-${p.b}`}`;
    if (!p.kind) p.kind = 'title';
  }
  return cleanParts(parts, n, { title });
}

// ---------------- 결과 정리 (규칙 · AI 공통) ----------------
/**
 * AI 답이나 화면에서 고친 묶음 → 빠짐없이 · 겹치지 않게 · 차례대로
 * 빈 틈(어느 묶음에도 없는 슬라이드)은 앞 묶음에 붙이고, 맨 앞 틈은 '처음' 묶음으로
 */
function cleanParts(list, count, { title = '' } = {}) {
  const n = Math.max(0, Math.floor(Number(count) || 0));
  if (!n) throw new Error('슬라이드 수를 알 수 없습니다.');
  const num = v => { const x = Math.round(Number(v)); return Number.isFinite(x) ? x : NaN; };
  let parts = (Array.isArray(list) ? list : []).map(p => {
    if (!p || typeof p !== 'object') return null;
    let a = num(p.a ?? p.from ?? p.start ?? p.first), b = num(p.b ?? p.to ?? p.end ?? p.last);
    const r = typeof p.range === 'string' && p.range.match(/^\s*(\d+)\s*(?:[-~]\s*(\d+))?\s*$/);
    if (r && !Number.isFinite(a)) { a = +r[1]; b = +(r[2] || r[1]); }
    if (Array.isArray(p.slides) && p.slides.length && !Number.isFinite(a)) { const s = p.slides.map(num).filter(Number.isFinite); a = Math.min(...s); b = Math.max(...s); }
    if (!Number.isFinite(b)) b = a;
    if (!Number.isFinite(a)) return null;
    if (a > b) [a, b] = [b, a];
    a = Math.max(1, a); b = Math.min(n, b);
    if (a > n || b < a) return null;
    const kind = KINDS.includes(p.kind) ? p.kind : KINDS.includes(p.type) ? p.type : 'title';
    return { a, b, name: tidyName(p.name ?? p.title ?? p.label ?? ''), kind };
  }).filter(Boolean).sort((x, y) => x.a - y.a || y.b - x.b);
  // 겹침: 뒤 묶음의 시작을 밀어낸다 (다 덮이면 버림)
  const out = [];
  for (const p of parts) {
    const prev = out[out.length - 1];
    if (prev && p.a <= prev.b) p.a = prev.b + 1;
    if (p.a > p.b) continue;
    out.push(p);
  }
  parts = out;
  if (!parts.length) return [{ a: 1, b: n, name: tidyName(title) || '전체', kind: 'title' }];
  // 틈: 앞 묶음에 붙인다 (빈 화면·구분 슬라이드가 보통 순서 끝에 있다)
  for (let i = 1; i < parts.length; i++) if (parts[i].a > parts[i - 1].b + 1) parts[i - 1].b = parts[i].a - 1;
  if (parts[0].a > 1) parts.unshift({ a: 1, b: parts[0].a - 1, name: '처음', kind: 'title' });
  parts[parts.length - 1].b = n;
  // 너무 많으면 뒤쪽을 마지막 묶음에 합친다
  if (parts.length > MAX_PARTS) { const last = parts[MAX_PARTS - 1]; last.b = n; parts.length = MAX_PARTS; }
  parts.forEach(p => { if (!p.name) p.name = `${tidyName(title) || 'PPT'} ${p.a === p.b ? p.a : `${p.a}-${p.b}`}`; });
  return parts.map(({ a, b, name, kind }) => ({ a, b, name, kind }));
}

// ---------------- AI ----------------
const FORMAT = `{
  "parts": [
    { "from": 1, "to": 2, "title": "표지", "kind": "title" },
    { "from": 3, "to": 8, "title": "개회 찬송 (새찬송가 8장)", "kind": "hymn" },
    { "from": 9, "to": 9, "title": "대표기도 · 홍길동 사관", "kind": "prayer" }
  ]
}`;
const SYSTEM = `당신은 한국 교회의 예배 화면(PPT) 담당자입니다. 예배나 행사(수련회 · 사관회 · 세미나 등) 전체를 한 파일에 담은 PowerPoint 를 받아, 예배 송출 프로그램의 큐시트 순서로 나눕니다. 송출 담당자는 순서마다 한 번에 이어서 넘깁니다.
답은 아래 형식의 JSON 하나만 씁니다.

형식:
${FORMAT}

원칙:
- 슬라이드 번호는 1부터입니다. 모든 슬라이드를 빠짐없이, 겹치지 않게, 차례대로 나눕니다 (다음 순서의 from = 앞 순서의 to + 1).
- 한 순서는 진행에서 한 덩어리입니다: 찬송 한 곡, 찬양 한 곡, 기도, 성경봉독, 교독문, 설교(또는 강의) 전체, 광고, 영상 하나, 1부·2부 제목, 축도 등.
- 같은 곡의 여러 절·후렴은 한 순서입니다. 곡이 바뀌면 새 순서입니다. 설교 중의 본문·요점 슬라이드는 설교 순서에 넣습니다.
- 순서 사이의 빈(검은) 슬라이드·구분 슬라이드는 앞 순서 끝에 붙입니다. 맨 앞 표지는 '표지' 순서로 둡니다.
- title 은 큐시트에 보일 짧은 이름(25자 안쪽)입니다. 슬라이드에 있는 순서 이름·곡 제목·장 번호·담당자를 그대로 씁니다. 슬라이드에 없는 이름·곡·장절을 지어내지 않습니다.
- kind 는 ${KINDS.map(k => `${k}(${KIND_NAMES[k]})`).join(' · ')} 중 하나입니다.
- 모아보기 그림이 있으면 칸 왼쪽 위의 번호가 슬라이드 번호입니다. 글자와 그림을 함께 보고 판단합니다.`;

/** 슬라이드 글자 목록 → 프롬프트에 넣을 글 ('[3] 찬송 8장 / 거룩 거룩 거룩 …') */
function slideList(texts, { budget = 60000 } = {}) {
  const n = texts.length;
  const per = Math.max(40, Math.min(240, Math.floor(budget / Math.max(1, n)) - 8));
  return texts.map((t, k) => {
    const lines = slideLines(t);
    return `[${k + 1}] ${lines.length ? clip(lines.join(' / '), per) : '(글자 없음)'}`;
  }).join('\n');
}

function userText(texts, { title = '', extra = '', sheets = 0 } = {}) {
  const empty = texts.filter(t => !slideLines(t).length).length;
  const head = [
    title && `PPT 파일: ${title}`,
    `슬라이드 ${texts.length}장${empty ? ` (그 가운데 ${empty}장은 글자가 없습니다${sheets ? ' — 모아보기 그림에서 보세요' : ''})` : ''}.`,
    sheets && `모아보기 그림 ${sheets}장을 함께 보냅니다 (칸마다 왼쪽 위에 슬라이드 번호).`,
    extra && `추가 요청: ${extra}`,
  ].filter(Boolean).join('\n');
  return `${head}\n\n[슬라이드 글자]\n${slideList(texts)}`;
}

/** 채팅 AI (키 없이) 에 붙여 넣을 프롬프트 */
function chatPrompt(texts, { title = '', extra = '', sheets = 0 } = {}) {
  if (!Array.isArray(texts) || !texts.length) throw new Error('슬라이드가 없습니다.');
  return `${SYSTEM}

답하는 법:
- JSON 하나만 \`\`\`json 코드 블록으로 답합니다. 앞뒤 설명은 쓰지 않습니다.${sheets ? '\n- 함께 올린 모아보기 그림도 봅니다.' : ''}

${userText(texts, { title, extra, sheets })}`;
}

/**
 * 오픈라우터(또는 OpenAI 호환)로 나누기
 * sheets: [{ from, to, url: 'data:image/jpeg;base64,…' }] — 조작 화면이 만든 모아보기 그림 (없어도 됨)
 */
async function askSplit({ apiKey, model, texts, title = '', extra = '', sheets = [], fetchImpl = fetch, endpoint = DEFAULT_ENDPOINT }) {
  if (!apiKey) throw new Error('오픈라우터 API 키가 없습니다. 파일 → 주보로 큐시트 만들기 → API 키로 에서 넣거나, 채팅 AI 를 쓰세요.');
  if (!Array.isArray(texts) || !texts.length) throw new Error('슬라이드가 없습니다.');
  const imgs = (Array.isArray(sheets) ? sheets : []).filter(s => s && /^data:image\/(jpeg|png|webp);base64,/.test(String(s.url || ''))).slice(0, 40);
  const content = [{ type: 'text', text: userText(texts, { title, extra, sheets: imgs.length }) }];
  for (const s of imgs) {
    content.push({ type: 'text', text: `모아보기: 슬라이드 ${s.from}-${s.to}` });
    content.push({ type: 'image_url', image_url: { url: s.url } });
  }
  const r = await fetchImpl(endpoint || DEFAULT_ENDPOINT, {
    method: 'POST',
    headers: { Authorization: `Bearer ${apiKey}`, 'Content-Type': 'application/json', 'HTTP-Referer': 'https://github.com/hahadark/WorshipPresenter', 'X-Title': 'Worship Presenter' },
    body: JSON.stringify({ model: model || DEFAULT_MODEL, messages: [{ role: 'system', content: SYSTEM }, { role: 'user', content: imgs.length ? content : content[0].text }], temperature: 0.1, response_format: { type: 'json_object' } }),
  });
  const body = await r.json().catch(() => ({}));
  if (!r.ok) throw new Error(`오픈라우터 오류 (${r.status}): ${(body.error && body.error.message) || r.statusText}`);
  const msg = body.choices && body.choices[0] && body.choices[0].message;
  if (!msg || !msg.content) throw new Error('AI 응답이 비었습니다.');
  const raw = typeof msg.content === 'string' ? msg.content : msg.content.map(c => c.text || '').join('');
  const parts = readParts(raw, texts.length, { title });
  if (!parts) throw new Error('AI 답에서 순서 나누기(parts)를 찾지 못했습니다. 다시 해 보거나 다른 모델을 고르세요.');
  return { parts, usage: body.usage || null, model: body.model || model };
}

/** 글에서 parts JSON 을 찾아 정리 (못 찾으면 null) */
function readParts(text, count, opts = {}) {
  const t = String(text || '').trim();
  if (!/[{[]/.test(t)) return null;
  const blocks = [...t.matchAll(/```(?:json|JSON)?\s*([\s\S]*?)```/g)].map(m => m[1]);
  for (const b of [...blocks, t]) {
    let o;
    try { o = parseJson(b); } catch {
      const a = b.indexOf('['), z = b.lastIndexOf(']'); // [ … ] 만 답한 경우
      if (a < 0 || z <= a) continue;
      try { o = JSON.parse(b.slice(a, z + 1)); } catch { continue; }
    }
    const list = Array.isArray(o) ? o : o && (o.parts || o.items || o.cues || o.순서);
    if (!Array.isArray(list) || !list.length) continue;
    const parts = cleanParts(list, count, opts);
    if (parts.length) return parts;
  }
  return null;
}

/** 채팅 AI 답 → parts (못 읽으면 오류) */
function parseAnswer(text, count, opts = {}) {
  const p = readParts(text, count, opts);
  if (!p) throw new Error('AI 답에서 순서 나누기 JSON 을 찾지 못했습니다. AI 에게 "JSON 코드 블록 하나로 다시 답해 줘" 라고 해 보세요.');
  return p;
}

module.exports = { slideLines, classify, ruleSplit, cleanParts, slideList, chatPrompt, askSplit, readParts, parseAnswer, SYSTEM, FORMAT, KINDS, KIND_NAMES };
