-- 2단계 A: 교회 자료(문서 · 변경 기록) · 성경 번역본 · 파일 목록 · 사람 계정. 계약: docs/web/architecture.md 6절 · docs/web/data-api.md
-- 모든 표의 첫 열쇠는 church_id, 모든 질의는 'WHERE church_id = ?1' 로 시작한다 (교회 단위 분리).
-- 6절에 적힌 칸은 그대로 두고, 덧붙인 칸에는 '(덧붙임)' 을 달았다.

-- 문서: 설치판 JSON 모양 그대로 (큐시트 · 찬양 · 템플릿 · 테마 · 오버레이 · 매크로 · 화면 구성 · 소스 · 설정(교회 공통) · 사용 기록)
CREATE TABLE documents (
  church_id TEXT NOT NULL,
  kind TEXT NOT NULL,                -- cue · song · template · theme · overlay · macro · scene · source · settings · usage
  id TEXT NOT NULL,
  payload TEXT NOT NULL,             -- JSON (지우면 '')
  revision INTEGER NOT NULL,         -- 1 부터. 고칠 때마다 1 씩 (지움도 한 번)
  updated_at INTEGER NOT NULL,       -- ms
  updated_by TEXT NOT NULL,          -- 기기 열쇠 id 또는 'm:<사람 id>'
  deleted_at INTEGER,                -- 지운 표시 (동기화가 '지움' 을 알도록 줄은 남긴다)
  title TEXT NOT NULL DEFAULT '',    -- (덧붙임) 목록에 보일 이름 (payload.name · title)
  size INTEGER NOT NULL DEFAULT 0,   -- (덧붙임) payload 바이트
  write_token TEXT,                  -- (덧붙임) 한 번 쓰기에 붙이는 무작위 값 — 같은 묶음 안에서 변경 기록 줄을 정확히 하나만 남기려고
  PRIMARY KEY (church_id, kind, id)
);

-- 변경 기록: 교회마다 1, 2, 3 … (동기화 GET /api/v1/sync?after=seq)
CREATE TABLE changes (
  church_id TEXT NOT NULL,
  seq INTEGER NOT NULL,
  kind TEXT NOT NULL,
  id TEXT NOT NULL,
  revision INTEGER NOT NULL,
  op TEXT NOT NULL CHECK (op IN ('put', 'delete')),
  at INTEGER NOT NULL,
  by TEXT NOT NULL,
  mutation_id TEXT,                  -- 올린 쪽이 정한 값. 같은 값이 다시 오면 두 번 쓰지 않고 앞의 결과를 돌려준다
  PRIMARY KEY (church_id, seq)
);
CREATE UNIQUE INDEX changes_by_mutation ON changes (church_id, mutation_id) WHERE mutation_id IS NOT NULL;

-- 성경 번역본: 교회가 쓸 허락을 가진 것만 (넣기 도구 scripts/bible-import.mjs). D1 하나는 500 MB — 다 넣지 않는다.
CREATE TABLE bible_versions (
  church_id TEXT NOT NULL,
  id TEXT NOT NULL,
  name TEXT NOT NULL,
  license_note TEXT NOT NULL DEFAULT '',
  verses INTEGER NOT NULL DEFAULT 0,
  language TEXT NOT NULL DEFAULT 'ko', -- (덧붙임) 책 이름을 영어로 보일지 (설치판 BibleVersions.language)
  structure TEXT NOT NULL DEFAULT '{}', -- (덧붙임) {"책 번호":[장마다 절 수]} — 장절을 확인할 때 절 줄을 다 읽지 않게
  imported_at INTEGER,               -- (덧붙임) ms
  PRIMARY KEY (church_id, id)
);
CREATE TABLE bible_verses (
  church_id TEXT NOT NULL,
  version TEXT NOT NULL,
  book INTEGER NOT NULL,
  chapter INTEGER NOT NULL,
  verse INTEGER NOT NULL,
  text TEXT NOT NULL,
  PRIMARY KEY (church_id, version, book, chapter, verse)
) WITHOUT ROWID;
-- (덧붙임) 소제목 (설치판 BibleHeadings). 한 절에 여럿일 수 있어 ord 로 나눈다
CREATE TABLE bible_headings (
  church_id TEXT NOT NULL,
  version TEXT NOT NULL,
  book INTEGER NOT NULL,
  chapter INTEGER NOT NULL,
  verse INTEGER NOT NULL,
  ord INTEGER NOT NULL,
  text TEXT NOT NULL,
  PRIMARY KEY (church_id, version, book, chapter, verse, ord)
) WITHOUT ROWID;

-- 파일 목록 (본문은 R2 — 2C 작업자 몫. 여기서는 표만)
CREATE TABLE assets (
  church_id TEXT NOT NULL,
  id TEXT NOT NULL,
  sha256 TEXT NOT NULL,
  name TEXT NOT NULL,
  type TEXT NOT NULL,
  size INTEGER NOT NULL,
  r2_key TEXT NOT NULL,              -- churches/{교회}/assets/{sha256}/{안전한 파일 이름}
  created_at INTEGER NOT NULL,
  deleted_at INTEGER,
  PRIMARY KEY (church_id, id)
);
CREATE INDEX assets_by_hash ON assets (church_id, sha256);

-- 사람 계정 (주인 · 편집자 · 보기만). 기기 열쇠(credentials)와 따로 — OBS · 휴대폰은 계속 기기 열쇠로.
-- subject = 'pw:<로그인 이름>' (비밀번호) · 'access:<전자우편>' (Cloudflare Access)
CREATE TABLE members (
  church_id TEXT NOT NULL REFERENCES churches(id),
  subject TEXT NOT NULL,
  role TEXT NOT NULL CHECK (role IN ('owner', 'editor', 'viewer')),
  created_at INTEGER NOT NULL,
  id TEXT NOT NULL,                  -- (덧붙임) 쿠키 · 관리 길에 쓰는 짧은 이름 (subject 를 쿠키에 넣지 않게)
  label TEXT NOT NULL DEFAULT '',    -- (덧붙임) 보일 이름
  secret_hash TEXT,                  -- (덧붙임) 'p1$반복$소금$해시' (PBKDF2-SHA256 + AUTH_SECRET). Access 계정은 NULL
  gen INTEGER NOT NULL DEFAULT 1,    -- (덧붙임) 역할 · 비밀번호를 바꾸면 1 올림 → 앞서 받은 쿠키가 모두 죽는다
  updated_at INTEGER,                -- (덧붙임)
  last_login_at INTEGER,             -- (덧붙임)
  disabled_at INTEGER,               -- (덧붙임) 지움 (줄은 남겨 누가 고쳤는지 기록이 이어지게)
  PRIMARY KEY (church_id, subject)
);
CREATE UNIQUE INDEX members_by_id ON members (church_id, id);
