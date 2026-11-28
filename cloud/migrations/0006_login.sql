-- 쉬운 로그인: 기기마다 세션(이 기기 기억 · 로그아웃시키기) · 사람 초대(이메일 · 링크) · 구글로 들어온 사람의 이메일.
-- 계약: docs/web/login.md. 모든 표의 첫 열쇠는 church_id (교회 단위 분리). 비밀값 원문은 두지 않는다(초대 링크는 해시만).

-- 구글로 들어온 사람의 확인된 이메일 (주인 화면에 보이고 · 이메일 초대를 맞추는 데 씀). 비밀번호 · Access 계정은 NULL
ALTER TABLE members ADD COLUMN email TEXT;

-- 로그인한 기기 (사람 계정만 — 기기 열쇠 · PIN 은 credentials 그대로)
CREATE TABLE member_sessions (
  church_id TEXT NOT NULL,
  id TEXT NOT NULL,                  -- 쿠키 열쇠 칸 'm:<사람>:<세대>:<id>' 의 끝
  member_id TEXT NOT NULL,
  remember INTEGER NOT NULL DEFAULT 1, -- 1 이 기기 기억(90일, 쓰면 늘어남) · 0 브라우저를 닫으면 끝(12시간)
  via TEXT NOT NULL DEFAULT '',      -- google · password · access · pick
  ua TEXT NOT NULL DEFAULT '',       -- '크롬 · 안드로이드' 같은 짧은 이름만 (원문 저장 안 함)
  created_at INTEGER NOT NULL,
  last_seen_at INTEGER NOT NULL,     -- 하루에 한 번만 고친다 (쓰기 줄이기)
  expires_at INTEGER NOT NULL,
  revoked_at INTEGER,
  PRIMARY KEY (church_id, id)
);
CREATE INDEX member_sessions_by_member ON member_sessions (church_id, member_id);

-- 사람 초대: 이메일(그 구글 계정으로 들어오면 자동으로) · 링크(받은 사람이 구글로 받음). 한 번 쓰면 끝
CREATE TABLE invites (
  church_id TEXT NOT NULL,
  id TEXT NOT NULL,
  email TEXT,                        -- 소문자. 링크 초대는 NULL 일 수 있음
  role TEXT NOT NULL CHECK (role IN ('owner', 'editor', 'viewer')),
  token_hash TEXT,                   -- 링크 초대: SHA-256(링크 열쇠). 이메일만 초대는 NULL
  label TEXT NOT NULL DEFAULT '',
  created_by TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  expires_at INTEGER,                -- NULL = 끝 없음 (관리자가 넣은 것만)
  used_at INTEGER,
  used_by TEXT,                      -- 'm:<사람 id>'
  revoked_at INTEGER,
  PRIMARY KEY (church_id, id)
);
CREATE INDEX invites_by_email ON invites (email);
CREATE UNIQUE INDEX invites_by_token ON invites (token_hash);
