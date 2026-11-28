// 드라이브 저장 형식 · 오류 코드 (Cloudflare 전용 API 를 쓰지 않는다 — Worker · 시험 · 가짜 구글이 함께 쓴다). 계약: docs/web/tiers.md
//
// 드라이브 안 (사람이 봐도 알게):
//   워십프리젠터/<교회 이름>/
//     큐시트/  찬양/  템플릿/  테마/  자막/  매크로/  화면 구성/  소스/  설정/  사용 기록/     ← 문서: '<이름> (<id>).json'
//     성경/<번역본 id>/번역본.json · '43 요한복음.json' …                                      ← 책마다 파일 하나
//     파일/그림/ · 파일/영상/ · 파일/소리/ · 파일/글꼴/ · 파일/PDF/                               ← 원래 형식 그대로

/** 저장소에 따라 생기는 오류 (docs/web/tiers.md 5절). 모든 오류 답은 {ok:false, error, code, …} */
export const ERROR_CODES = {
  drive_disconnected: 503, // 구글 연결 끊김 · 토큰 만료 · 사용자가 권한을 거둠 → 주인이 다시 연결 (reconnect 주소를 같이 줌)
  drive_quota: 507,        // 사용자의 구글 드라이브가 가득 참
  drive_rate: 429,         // 구글이 너무 자주 부른다고 막음 (잠시 뒤 다시)
  drive_unavailable: 502,  // 구글 오류 · 연결 실패
  drive_missing: 410,      // 드라이브에서 사람이 그 파일을 지움 (휴지통 비움)
  drive_corrupt: 502,      // 드라이브의 파일을 사람이 고쳐 읽을 수 없음
  drive_busy: 409,         // 같은 문서를 다른 곳에서 쓰는 중 (current 를 같이 줌 — 충돌과 같게 다루면 됨)
  tier_blocked: 403,       // 이 등급에서 쓰지 않는 기능
  tier_limit: 429,         // 등급 한도 (하루 요청 · 1분 요청 · 문서 수 · 번역본 수 — limit 칸에 무엇인지)
  tier_moving: 423,        // 등급을 옮기는 중 (쓰기만 막음, 읽기 · 송출은 됨)
};

export class DriveError extends Error {
  constructor(code, message, extra = {}) {
    super(message);
    this.code = code;
    this.status = ERROR_CODES[code] || 502;
    this.extra = extra;
    this.grant = !!extra.grant;
  }
}

/** 구글 Drive 오류 답 → DriveError (https://developers.google.com/workspace/drive/api/guides/handle-errors) */
export function driveErrorOf(status, body) {
  const reasons = (body?.error?.errors || []).map(e => e.reason);
  const has = r => reasons.includes(r);
  if (status === 401) return new DriveError('drive_disconnected', '구글 드라이브 연결이 끊어졌습니다. 주인이 다시 연결해 주세요.');
  if (has('storageQuotaExceeded') || has('quotaExceeded') && /storage/i.test(body?.error?.message || '')) return new DriveError('drive_quota', '구글 드라이브 저장 공간이 가득 찼습니다. 드라이브를 정리하거나 용량을 늘려 주세요.');
  if (status === 429 || has('userRateLimitExceeded') || has('rateLimitExceeded')) return new DriveError('drive_rate', '구글 드라이브가 잠시 요청을 막았습니다. 조금 뒤 다시 해 보세요.');
  if (status === 404 || has('notFound')) return new DriveError('drive_missing', '드라이브에서 그 파일을 찾지 못했습니다 (드라이브에서 지웠을 수 있습니다).');
  if (status === 403 && (has('insufficientPermissions') || has('forbidden') || has('appNotAuthorizedToFile'))) return new DriveError('drive_disconnected', '구글 드라이브 권한이 없습니다. 주인이 다시 연결해 주세요.');
  return new DriveError('drive_unavailable', '구글 드라이브가 요청을 처리하지 못했습니다.', { providerStatus: status });
}

/** 문서 종류 → 폴더 이름 */
export const KIND_FOLDERS = {
  cue: '큐시트', song: '찬양', template: '템플릿', theme: '테마', overlay: '자막', macro: '매크로',
  scene: '화면 구성', source: '소스', settings: '설정', usage: '사용 기록',
};
/** 파일 묶음 → '파일/' 아래 폴더 이름 */
export const FILE_FOLDERS = { image: '그림', video: '영상', audio: '소리', font: '글꼴', pdf: 'PDF', slides: 'PPT' };
export const ROOT_FOLDER = '워십프리젠터';

/** 드라이브 파일 이름에 넣을 수 없는(넣으면 헷갈리는) 글자를 뺀다 */
export function cleanName(s, max = 80) {
  const t = String(s ?? '').normalize('NFC').replace(/[\u0000-\u001f\u007f/\\:*?"<>|]/g, ' ').replace(/\s+/g, ' ').trim();
  return t.slice(0, max);
}
/** 문서 파일 이름: '<이름> (<id>).json' — 이름이 없으면 id 만 */
export function docFileName(kind, id, title) {
  const t = cleanName(title, 80);
  return t ? `${t} (${id}).json` : `${id}.json`;
}

// ---------- 문서 봉투 ----------
// 드라이브의 문서 파일 = 사람이 열어 봐도 되는 JSON. payload 는 설치판 JSON 모양 그대로.
//   {"worshipPresenter":1,"kind":"cue","id":"…","revision":3,"updatedAt":…,"updatedBy":"…","deleted":false,"payload":{…}}
// 판 번호(revision)는 D1 색인이 정한다. 파일 안의 revision 은 '이 내용이 몇 판인가' — 쓰다가 끊긴 것을 알아보는 데 쓴다.
export function envelopeText({ kind, id, revision, updatedAt, updatedBy, deleted = false, payloadText = null }) {
  const head = JSON.stringify({ worshipPresenter: 1, kind, id, revision, updatedAt, updatedBy, deleted: !!deleted });
  return deleted || payloadText === null ? head : `${head.slice(0, -1)},"payload":${payloadText}}`;
}
/** 봉투 글 → { kind, id, revision, deleted, payloadText } | null(읽을 수 없음) */
export function parseEnvelope(text) {
  let v;
  try { v = JSON.parse(text); } catch { return null; }
  if (!v || typeof v !== 'object' || v.worshipPresenter !== 1 || typeof v.kind !== 'string' || typeof v.id !== 'string' || !Number.isInteger(v.revision)) return null;
  if (!v.deleted && (v.payload === null || typeof v.payload !== 'object' || Array.isArray(v.payload))) return null;
  return { kind: v.kind, id: v.id, revision: v.revision, updatedAt: v.updatedAt ?? null, updatedBy: v.updatedBy ?? '', deleted: !!v.deleted, payloadText: v.deleted ? null : JSON.stringify(v.payload) };
}

// ---------- multipart/related (메타 + 내용을 한 번에, 5 MB 까지 — 구글 문서) ----------
export function multipartBody(meta, content, contentType = 'application/json; charset=UTF-8') {
  const boundary = `wp${crypto.getRandomValues(new Uint32Array(2)).join('')}`;
  const body = `--${boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n${JSON.stringify(meta)}\r\n--${boundary}\r\nContent-Type: ${contentType}\r\n\r\n${content}\r\n--${boundary}--`;
  return { body, contentType: `multipart/related; boundary=${boundary}` };
}

// ---------- 성경 책 파일 ----------
// 사람이 열어 봐도 되는 JSON 이면서, Worker 가 부른 장만 읽도록(CPU 10 ms) 장마다 한 줄:
//   1줄: {"worshipPresenter":1,"kind":"bible-book","version":"KRV","book":43,"name":"요한복음","chapterCount":21,"chapters":[
//   2줄~: [[1,"태초에 …"],[2,"…"]],        ← 장마다 [절, 본문] 묶음 (마지막 장만 쉼표 없음)
//   끝줄: ],"headings":[[3,1,"니고데모"],…]}
export function bookFileText({ version, book, name, chapters, headings = [] }) {
  const head = JSON.stringify({ worshipPresenter: 1, kind: 'bible-book', version, book, name, chapterCount: chapters.length });
  const lines = [`${head.slice(0, -1)},"chapters":[`];
  chapters.forEach((c, i) => lines.push(JSON.stringify(c) + (i < chapters.length - 1 ? ',' : '')));
  lines.push(`],"headings":${JSON.stringify(headings)}}`);
  return lines.join('\n');
}
/** 책 파일 글 → 줄 나눈 것 (장을 부를 때만 그 줄을 읽는다) */
export function splitBook(text) {
  const lines = String(text).split('\n');
  if (lines.length < 2) return null;
  let head;
  try { head = JSON.parse(lines[0].slice(0, lines[0].lastIndexOf(',"chapters":[')) + '}'); } catch { return null; }
  if (head?.kind !== 'bible-book' || !Number.isInteger(head.chapterCount) || lines.length < head.chapterCount + 2) return null;
  return { head, lines };
}
/** 장 하나 → [[절, 본문], …] */
export function bookChapter(split, chapter) {
  if (!split || chapter < 1 || chapter > split.head.chapterCount) return [];
  let line = split.lines[chapter];
  if (line.endsWith(',')) line = line.slice(0, -1);
  try { return JSON.parse(line); } catch { return []; }
}
export function bookHeadings(split) {
  if (!split) return [];
  const last = split.lines[split.lines.length - 1];
  const i = last.indexOf('"headings":');
  if (i < 0) return [];
  try { return JSON.parse(last.slice(i + 11, -1)); } catch { return []; }
}

/**
 * 성경 넣기 본문 확인. 책 하나: { name?, chapters:[[[절, 본문], …], …], headings?:[[장, 절, 글], …] } → { ok, chapters, headings, verses, counts } | { error }
 * counts = 장마다 마지막 절 번호 (유료 bible_versions.structure 와 같은 뜻)
 */
export function checkBook(body) {
  const ch = body?.chapters;
  if (!Array.isArray(ch) || !ch.length || ch.length > 200) return { error: 'chapters 는 장 배열입니다 (1~200장).' };
  let verses = 0;
  const counts = [];
  const clean = [];
  for (const c of ch) {
    if (!Array.isArray(c) || c.length > 400) return { error: '장마다 [절, 본문] 배열이어야 합니다 (400절까지).' };
    const out = [];
    let max = 0;
    for (const x of c) {
      if (!Array.isArray(x) || !Number.isInteger(x[0]) || x[0] < 1 || x[0] > 400 || typeof x[1] !== 'string' || x[1].length > 4000) return { error: '[절 번호, 본문] 이 올바르지 않습니다.' };
      out.push([x[0], x[1].replace(/\u0000/g, '')]);
      if (x[0] > max) max = x[0];
    }
    verses += out.length;
    counts.push(max);
    clean.push(out);
  }
  const hs = [];
  for (const h of Array.isArray(body.headings) ? body.headings : []) {
    if (Array.isArray(h) && Number.isInteger(h[0]) && Number.isInteger(h[1]) && typeof h[2] === 'string') hs.push([h[0], h[1], h[2].slice(0, 400)]);
    if (hs.length > 2000) break;
  }
  return { ok: true, chapters: clean, headings: hs, verses, counts };
}
