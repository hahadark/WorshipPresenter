// 계정 등급(데모 · 무료 · 유료) 한도표 — 한 곳(이 표)에만 둔다. 값은 '안' 이고 사관님이 정한다 (docs/web/tiers.md 2절).
// Cloudflare 전용 API 를 쓰지 않는다 (Worker · 시험이 함께 쓴다).

const MiB = 1024 * 1024;

export const TIERS = ['demo', 'free', 'paid'];

/** 등급 → 저장소: demo = D1 의 견본 교회(하루마다 되돌림) · free = 그 교회의 구글 드라이브 · paid = 우리 D1 · R2 */
export const STORE = { demo: 'demo', free: 'drive', paid: 'd1' };

/**
 * 한도표. null = 제한 없음(그 기능의 기술 한도만).
 * - documents: 살아 있는 문서 수 (큐시트 · 찬양 · 템플릿 · 설정 …)
 * - bibleVersions: 넣을 수 있는 번역본 수
 * - fileBytes: 파일(그림 · 영상 · 글꼴 · PDF) 전체 바이트 — 무료는 사용자 드라이브 용량이 한도(null)
 * - fileMax: 파일 하나 · multipart: 나눠 올리기(90 MiB 넘는 영상)
 * - outputs: 동시에 붙는 화면(출력 · 무대 · OBS) 수 · remotes: 리모컨 PIN 수 · credentials: 기기 열쇠 전체 · members: 사람 계정
 * - ai: false | 'own-key'(교회 키로 대신 부르기만) | 'all'(+ 클라우드 음성 인식 — 우리 계정 무료 할당을 씀)
 * - requestsPerDay: Worker 요청(페이지 · API · 파일, 정적 파일과 실시간 메시지는 빼고). 어림 — Worker 마다 모았다가 더한다
 * - mutations: 동기화 한 번에 올리는 수 (드라이브는 하나마다 구글을 한 번 부르므로 Workers 무료 50 하위 요청 안에서)
 * - pullPayloads: 동기화 받기 한 쪽에 본문을 싣는 수 (드라이브에서 하나씩 읽음)
 * - sessionHours: 로그인 유지 (데모만 짧게) · resetHours: 데모를 처음 모습으로 되돌리는 간격
 * - perIpPerMinute: 데모 둘러보기의 주소별 1분 요청 · entriesPerIpPerHour: 둘러보기 새로 들어오기
 */
export const TIER_LIMITS = {
  demo: {
    documents: 300, bibleVersions: 0, fileBytes: 0, fileMax: 0, multipart: false,
    outputs: 3, remotes: 0, credentials: 0, members: 0, ai: false,
    requestsPerDay: 20_000, mutations: 20, pullPayloads: 500,
    sessionHours: 2, resetHours: 24, perIpPerMinute: 120, entriesPerIpPerHour: 20,
  },
  free: {
    documents: 2_000, bibleVersions: 3, fileBytes: null, fileMax: 90 * MiB, multipart: false,
    outputs: 4, remotes: 2, credentials: 20, members: 10, ai: 'own-key',
    requestsPerDay: 20_000, mutations: 20, pullPayloads: 40,
    sessionHours: null, resetHours: null, perIpPerMinute: null, entriesPerIpPerHour: null,
  },
  paid: {
    documents: 20_000, bibleVersions: 10, fileBytes: 20_000 * MiB, fileMax: null, multipart: true,
    outputs: 20, remotes: 10, credentials: 50, members: 50, ai: 'all',
    requestsPerDay: 200_000, mutations: 20, pullPayloads: 500,
    sessionHours: null, resetHours: null, perIpPerMinute: null, entriesPerIpPerHour: null,
  },
};

/** 화면에 보일 이름 */
export const TIER_NAMES = { demo: '둘러보기(데모)', free: '무료 (구글 드라이브)', paid: '유료 (워십프리젠터 클라우드)' };

/**
 * 등급의 한도. env.TIER_OVERRIDES(JSON, 예: {"demo":{"perIpPerMinute":30}}) 가 있으면 그 칸만 덮는다 —
 * 시험 · 운영에서 표를 고치지 않고 잠깐 바꿀 때만. 기본값은 위 표 하나다.
 */
const overrideMemo = { text: null, value: {} };
export function limitsOf(tier, env = {}) {
  const base = TIER_LIMITS[tier] || TIER_LIMITS.paid;
  const text = typeof env.TIER_OVERRIDES === 'string' ? env.TIER_OVERRIDES : '';
  if (!text) return base;
  if (overrideMemo.text !== text) {
    try { overrideMemo.value = JSON.parse(text) || {}; } catch { overrideMemo.value = {}; }
    overrideMemo.text = text;
  }
  const o = overrideMemo.value[tier];
  return o && typeof o === 'object' ? { ...base, ...o } : base;
}

/** 무료 가입 · 데모가 우리 무료 한도를 다 먹지 않게 — 서비스 전체 (등급마다가 아님) */
export const SERVICE_LIMITS = {
  /** 하루에 새로 만들 수 있는 무료 교회 수 (서비스 전체) */
  freeSignupsPerDay: 50,
  /** 한 접속 주소에서 한 시간에 무료 가입 */
  signupsPerIpPerHour: 3,
  /** 한 구글 계정이 가질 수 있는 무료 교회 */
  freeChurchesPerGoogleAccount: 1,
};

/** 서비스 전체 한도 (env.SERVICE_OVERRIDES JSON 으로 시험 · 운영에서만 덮음) */
export function serviceLimits(env = {}) {
  if (typeof env.SERVICE_OVERRIDES !== 'string' || !env.SERVICE_OVERRIDES) return SERVICE_LIMITS;
  try { return { ...SERVICE_LIMITS, ...JSON.parse(env.SERVICE_OVERRIDES) }; } catch { return SERVICE_LIMITS; }
}

/**
 * 이 등급에서 이 길을 쓸 수 있나 (권한 표 can() 을 지난 뒤에 본다) → null(됨) | { status, code, error, limit }
 * r = protocol.route() 의 답 { kind, cap, op }
 */
export function gate(tier, r, env = {}) {
  const L = limitsOf(tier, env);
  const no = (error, limit, status = 403) => ({ status, code: 'tier_blocked', error, limit, tier });
  if (r.kind === 'assets') {
    const writes = ['upload', 'start', 'part', 'complete', 'abort', 'status', 'update', 'remove', 'restore', 'purge'];
    if (!L.fileMax && L.fileMax !== null && writes.includes(r.op)) return no('둘러보기에서는 파일을 올리거나 고칠 수 없습니다.', 'fileMax');
    if (!L.multipart && ['start', 'part', 'complete', 'abort', 'status'].includes(r.op)) {
      return no(`이 등급은 나눠 올리기가 없습니다. ${Math.floor((L.fileMax || 0) / MiB)} MiB 까지 한 번에 올려 주세요.`, 'multipart');
    }
  }
  if (r.kind === 'ai') {
    if (!L.ai) return no('둘러보기에서는 AI 를 쓸 수 없습니다.', 'ai');
    if (L.ai === 'own-key' && r.op === 'transcribe') return no('클라우드 음성 인식은 유료 등급에서만 씁니다. 설치판의 PC 안 음성 인식을 쓰세요.', 'ai');
  }
  if (r.kind === 'identity' && ['create'].includes(r.op) && L.members === 0) return no('둘러보기에서는 사람을 초대할 수 없습니다.', 'members');
  return null;
}

/** 오늘 (UTC) */
export const dayOf = (now = Date.now()) => new Date(now).toISOString().slice(0, 10);
