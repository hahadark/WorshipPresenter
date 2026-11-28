-- 1단계: 교회 · 기기 열쇠 · 틀린 횟수. 예배 자료(본문 · 가사 · 큐시트)는 1단계 D1 에 두지 않는다.
-- 모든 표의 첫 열쇠는 church_id — 다른 교회 자료를 한 질의로 섞어 읽지 못하게 (docs/web/architecture.md '교회 단위 분리')

CREATE TABLE churches (
  id TEXT PRIMARY KEY,               -- 영문 소문자 · 숫자 · '-' (2~40자)
  name TEXT NOT NULL,
  created_at INTEGER NOT NULL,       -- ms
  disabled_at INTEGER                -- 막으면 그 교회의 모든 로그인 · 연결이 끊긴다
);

-- 기기 열쇠: owner 주인 · bridge 설치판 연결 · output 출력 화면(OBS) · remote 리모컨 PIN · editor 웹 편집기 PIN
-- secret_hash = HMAC-SHA256(AUTH_SECRET, 'cred:' || church || ':' || role || ':' || 열쇠) — 원문은 두지 않는다
CREATE TABLE credentials (
  church_id TEXT NOT NULL REFERENCES churches(id),
  id TEXT NOT NULL,
  role TEXT NOT NULL CHECK (role IN ('owner', 'bridge', 'output', 'remote', 'editor')),
  label TEXT NOT NULL DEFAULT '',
  secret_hash TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  last_used_at INTEGER,
  expires_at INTEGER,
  revoked_at INTEGER,
  PRIMARY KEY (church_id, id)
);
CREATE INDEX credentials_by_hash ON credentials (church_id, secret_hash);

-- 틀린 횟수 (scope = 'ip:<주소>' 또는 'all')
CREATE TABLE auth_failures (
  church_id TEXT NOT NULL,
  scope TEXT NOT NULL,
  count INTEGER NOT NULL,
  locked_until INTEGER NOT NULL DEFAULT 0,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY (church_id, scope)
);
