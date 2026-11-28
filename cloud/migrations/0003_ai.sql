-- 5단계: AI (교회 키 봉인 · 동의 · 하루 한도 · 부름 기록). 계약: docs/web/ai.md
-- 모든 표의 첫 열쇠는 church_id (교회 단위 분리). 보낸 글 · 받은 답 · 소리는 저장하지 않는다.

-- 교회 AI 설정: OpenAI 호환 주소 · 모델 · 봉인한 키 (AES-GCM, 열쇠는 Worker 비밀값에서)
CREATE TABLE ai_settings (
  church_id TEXT PRIMARY KEY,
  endpoint TEXT NOT NULL DEFAULT '',     -- 비우면 오픈라우터 (설치판과 같은 기본)
  model TEXT NOT NULL DEFAULT '',
  key_sealed TEXT NOT NULL DEFAULT '',   -- 'v1.<iv>.<암호문>' · 비우면 키 없음
  key_hint TEXT NOT NULL DEFAULT '',     -- 끝 4자리
  daily_limit INTEGER NOT NULL DEFAULT 30,
  stt_minutes INTEGER NOT NULL DEFAULT 60,
  updated_at INTEGER NOT NULL,
  updated_by TEXT NOT NULL
);

-- 기능별 동의 (교회 단위, 주인이 함)
CREATE TABLE ai_consent (
  church_id TEXT NOT NULL,
  feature TEXT NOT NULL,
  consented_by TEXT NOT NULL,
  consented_at INTEGER NOT NULL,
  PRIMARY KEY (church_id, feature)
);

-- 교회 하루 사용량 (UTC 날짜)
CREATE TABLE ai_usage (
  church_id TEXT NOT NULL,
  day TEXT NOT NULL,
  calls INTEGER NOT NULL DEFAULT 0,
  tokens INTEGER NOT NULL DEFAULT 0,
  stt_seconds INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (church_id, day)
);

-- 계정 전체 Workers AI 무료 할당 (하루 10,000 neurons 는 계정 단위 — 교회를 가리지 않고 센다)
CREATE TABLE ai_account (
  day TEXT PRIMARY KEY,
  neurons REAL NOT NULL DEFAULT 0
);

-- 부름 기록: 내용 없이 언제 · 누가 · 무엇을 · 얼마나
CREATE TABLE ai_calls (
  church_id TEXT NOT NULL,
  id TEXT NOT NULL,
  at INTEGER NOT NULL,
  by TEXT NOT NULL,
  feature TEXT NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN ('chat', 'stt')),
  host TEXT NOT NULL DEFAULT '',
  model TEXT NOT NULL DEFAULT '',
  status INTEGER NOT NULL,           -- 200 · 공급자 오류 코드 · 0(연결 실패)
  ms INTEGER NOT NULL DEFAULT 0,
  chars INTEGER NOT NULL DEFAULT 0,  -- 보낸 글자 수
  tokens INTEGER NOT NULL DEFAULT 0,
  seconds REAL NOT NULL DEFAULT 0,   -- 음성 인식 소리 길이
  PRIMARY KEY (church_id, id)
);
CREATE INDEX ai_calls_by_time ON ai_calls (church_id, at);
