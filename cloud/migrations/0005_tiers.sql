-- 7단계 일부: 계정 등급(데모 · 무료 · 유료)과 등급별 저장소. 계약: docs/web/architecture.md 12절 · docs/web/tiers.md
-- 모든 표의 첫 열쇠는 church_id (교회 단위 분리). 무료(구글 드라이브) 교회는 여기에 '작은 것' 만 둔다:
--   계정 · 등급 · 봉인한 구글 토큰 · 드라이브 폴더/파일 id · 판 번호 · 변경 순번(changes 표, 유료와 같이 씀) · 파일 목록.
--   문서 본문 · 성경 본문 · 파일 바이트는 그 교회의 구글 드라이브에만 있다.

-- 교회 등급. 지금까지의 교회(견본 'sample' 등)는 유료(D1 · R2) 그대로
ALTER TABLE churches ADD COLUMN tier TEXT NOT NULL DEFAULT 'paid';      -- demo · free · paid
ALTER TABLE churches ADD COLUMN tier_changed_at INTEGER;
ALTER TABLE churches ADD COLUMN move_to TEXT;                            -- 옮기는 중이면 갈 등급 (그동안 쓰기는 423)
ALTER TABLE churches ADD COLUMN move_since INTEGER;
ALTER TABLE churches ADD COLUMN move_state TEXT;                         -- 옮기기 진행 (JSON: 단계 · 다음 자리)
ALTER TABLE churches ADD COLUMN demo_reset_at INTEGER;                   -- 데모: 마지막으로 처음 모습으로 되돌린 때
ALTER TABLE churches ADD COLUMN signup_by TEXT;                          -- 무료 가입: 'google:<sub>' (한 구글 계정에 무료 교회 하나)

-- 구글 드라이브 연결 (교회마다 하나). 토큰은 AES-GCM 봉인 (ai/seal.mjs 와 같은 방식, 목적 글이 다름)
CREATE TABLE drive_accounts (
  church_id TEXT PRIMARY KEY,
  google_sub TEXT NOT NULL,              -- 구글 계정 번호 (다시 연결할 때 같은 계정인지 본다)
  email TEXT NOT NULL DEFAULT '',        -- 주인 화면에 '어느 계정' 인지 보이려고만. 로그에 남기지 않는다
  refresh_sealed TEXT NOT NULL DEFAULT '',
  access_sealed TEXT NOT NULL DEFAULT '',
  access_exp INTEGER NOT NULL DEFAULT 0,
  scope TEXT NOT NULL DEFAULT '',
  root_id TEXT,                          -- '워십프리젠터' 폴더
  church_folder_id TEXT,                 -- '워십프리젠터/<교회 이름>' 폴더
  folders TEXT NOT NULL DEFAULT '{}',    -- {"cue":"<id>", "bible":"<id>", "files:image":"<id>", …}
  connected_at INTEGER NOT NULL,
  disconnected_at INTEGER,               -- 토큰이 죽었거나(사용자가 끊음 · 만료) 주인이 끊음
  disconnect_reason TEXT,
  updated_at INTEGER NOT NULL
);

-- 무료 교회 문서 색인 (본문은 드라이브 파일). 판 번호 · 묘비 · 쓰는 중 표시
CREATE TABLE drive_docs (
  church_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  id TEXT NOT NULL,
  revision INTEGER NOT NULL,             -- 0 = 아직 만들어지지 않음(만드는 중)
  updated_at INTEGER NOT NULL,
  updated_by TEXT NOT NULL,
  deleted_at INTEGER,
  title TEXT NOT NULL DEFAULT '',
  size INTEGER NOT NULL DEFAULT 0,
  file_id TEXT,                          -- 드라이브 파일 id
  drive_version TEXT,                    -- 마지막으로 쓴 뒤 드라이브가 준 version (사람이 드라이브에서 고쳤는지 볼 때)
  write_token TEXT,                      -- 변경 기록 줄을 정확히 하나만 남기려고 (유료 documents 와 같은 쓰임)
  pend_token TEXT,                       -- 쓰는 중: 이 값을 쥔 요청만 마무리한다
  pend_at INTEGER,
  pend_op TEXT,
  pend_mut TEXT,
  pend_title TEXT,
  pend_size INTEGER,
  PRIMARY KEY (church_id, kind, id)
);

-- 무료 교회 성경 번역본 (본문은 드라이브 '성경/<번역본>/' 의 책 파일)
CREATE TABLE drive_bible (
  church_id TEXT NOT NULL,
  version TEXT NOT NULL,
  name TEXT NOT NULL,
  language TEXT NOT NULL DEFAULT 'ko',
  license_note TEXT NOT NULL DEFAULT '',
  verses INTEGER NOT NULL DEFAULT 0,
  structure TEXT NOT NULL DEFAULT '{}',  -- {"책":[장마다 절 수]} (유료 bible_versions 와 같음)
  folder_id TEXT,
  meta_file_id TEXT,
  books TEXT NOT NULL DEFAULT '{}',      -- {"43":"<파일 id>", …}
  imported_at INTEGER,
  PRIMARY KEY (church_id, version)
);

-- 무료 교회 파일: 2C 의 assets · asset_blobs 표를 그대로 쓰고(목록 · 이름 · 글꼴 정보 · sha256), 바이트가 있는 드라이브 파일 id 만 여기에
CREATE TABLE drive_files (
  church_id TEXT NOT NULL,
  r2_key TEXT NOT NULL,                  -- 2C 가 정한 열쇠 그대로 (churches/{교회}/assets/{sha256}/{이름})
  file_id TEXT NOT NULL,
  size INTEGER NOT NULL,
  sha256 TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  PRIMARY KEY (church_id, r2_key)
) WITHOUT ROWID;

-- 등급별 하루 요청 수 (Worker 안에 모았다가 몇십 번마다 한 번 더한다 — D1 쓰기 한도를 아끼려고, 그래서 어림)
CREATE TABLE tier_usage (
  church_id TEXT NOT NULL,
  day TEXT NOT NULL,                     -- 'YYYY-MM-DD' (UTC)
  requests INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (church_id, day)
) WITHOUT ROWID;

-- 구글로 로그인: 구글 계정 번호(members.subject = 'google:<sub>')로 교회를 찾는다 · 한 구글 계정에 무료 교회 하나
CREATE INDEX members_by_subject ON members (subject);
CREATE INDEX churches_by_signup ON churches (signup_by) WHERE signup_by IS NOT NULL;
CREATE INDEX churches_by_tier ON churches (tier, created_at);
