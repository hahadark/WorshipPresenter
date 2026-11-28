-- 2단계 C: 그림 · 영상 · 글꼴 (본문은 R2 비공개 저장통, 여기는 목록 · 나눠 올리기 · 사용량). 계약: docs/web/assets-api.md
-- 모든 표의 첫 열쇠는 church_id. 교회 단위가 아닌 질의는 무료 범위 안전장치의 합계(SUM) 둘뿐이다 (usage.mjs).

-- 0002 의 assets 표에 덧붙임 (한 줄 = 사람이 보는 파일 하나. 같은 내용(sha256)은 R2 에 한 번만 — asset_blobs)
ALTER TABLE assets ADD COLUMN kind TEXT NOT NULL DEFAULT 'image';   -- image · video · audio · font · pdf
ALTER TABLE assets ADD COLUMN created_by TEXT NOT NULL DEFAULT '';  -- 기기 열쇠 id 또는 'm:<사람 id>'
ALTER TABLE assets ADD COLUMN updated_at INTEGER;
ALTER TABLE assets ADD COLUMN font_family TEXT;                     -- @font-face 이름 (템플릿의 글꼴 이름과 같게)
ALTER TABLE assets ADD COLUMN font_weight INTEGER;                  -- 1~1000 (파일 안 값이 아니라 사람이 정한 값)
ALTER TABLE assets ADD COLUMN font_style TEXT;                      -- normal · italic · oblique
ALTER TABLE assets ADD COLUMN purged_at INTEGER;                    -- 정리 명령이 R2 에서 지운 때 (줄은 남김 → 410)
CREATE INDEX assets_by_kind ON assets (church_id, kind, created_at);

-- R2 물건 하나 = (교회, sha256) 하나
CREATE TABLE asset_blobs (
  church_id TEXT NOT NULL,
  sha256 TEXT NOT NULL,
  r2_key TEXT NOT NULL,              -- churches/{교회}/assets/{sha256}/{안전한 파일 이름}
  size INTEGER NOT NULL,
  type TEXT NOT NULL,
  verified INTEGER NOT NULL,         -- 1 = R2 가 sha256 을 확인함 (한 번에 올리기), 0 = 올린 쪽이 알린 값 (나눠 올리기)
  created_at INTEGER NOT NULL,
  purged_at INTEGER,                 -- R2 에서 지움
  PRIMARY KEY (church_id, sha256)
);

-- 나눠 올리는 중 (R2 multipart). 끝내면 지운다
CREATE TABLE asset_uploads (
  church_id TEXT NOT NULL,
  id TEXT NOT NULL,                  -- 밖에 보이는 uploadId (R2 의 upload_id 는 밖에 내지 않는다)
  r2_key TEXT NOT NULL,
  r2_upload TEXT NOT NULL,
  sha256 TEXT NOT NULL,
  name TEXT NOT NULL,
  type TEXT NOT NULL,
  kind TEXT NOT NULL,
  size INTEGER NOT NULL,
  part_size INTEGER NOT NULL,
  parts INTEGER NOT NULL,
  font_family TEXT,
  font_weight INTEGER,
  font_style TEXT,
  created_at INTEGER NOT NULL,
  created_by TEXT NOT NULL,
  PRIMARY KEY (church_id, id)
);
CREATE TABLE asset_upload_parts (
  church_id TEXT NOT NULL,
  upload TEXT NOT NULL,
  n INTEGER NOT NULL,
  etag TEXT NOT NULL,
  size INTEGER NOT NULL,
  PRIMARY KEY (church_id, upload, n)
) WITHOUT ROWID;

-- 이 달 R2 작업 수 (교회마다). A = 올리기 · 나눠 올리기, B = 받기. B 는 Worker 가 모아서 1분마다 더한다
CREATE TABLE asset_usage (
  church_id TEXT NOT NULL,
  month TEXT NOT NULL,               -- 'YYYY-MM' (UTC)
  class_a INTEGER NOT NULL DEFAULT 0,
  class_b INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (church_id, month)
) WITHOUT ROWID;

-- 교회마다 저장 한도 (선택, 없으면 계정 전체 안전장치만). 7단계에서 다른 교회를 받을 때 쓴다
CREATE TABLE asset_limits (
  church_id TEXT NOT NULL PRIMARY KEY,
  max_bytes INTEGER NOT NULL
);
