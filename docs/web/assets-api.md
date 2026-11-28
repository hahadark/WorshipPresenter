# 웹판 2단계 C — 그림 · 영상 · 글꼴 (R2) API (계약 v1, 구현함)

`architecture.md` 6절(자산 길 예약) · `data-api.md` 8절을 구현한 것. 올리기 길 모양(1~5번)은 조율자가 2B(설치판 동기화)와 맞춰 정한 것 그대로다.
코드: `cloud/src/assets/` (`routes.mjs` 길 · 종류 · Range · 안전장치 판단 · 글꼴 CSS, `fonts.mjs` 글꼴 파일 읽기, `usage.mjs` 사용량, `index.mjs` 처리) · `cloud/migrations/0004_assets.sql`.
시험: `cloud/test/assets.test.mjs`(단위) · `cloud/test/assets-e2e/assets.test.mjs`(wrangler dev + 로컬 R2 · D1) · `cloud/scripts/r2-remote-check.mjs`(실제 R2).
저장통: **`worshippresenter-web-assets`** (비공개, APAC, Standard). r2.dev 주소 · 사용자 도메인은 켜지 않았다. Worker 묶음 이름은 `env.FILES`.

## 1. 누가 무엇을

| 로그인 | 받기 `GET /api/v1/assets/{id}` · `fonts.css` | 목록 · 있나 묻기 · 사용량 | 올리기 · 고치기 · 지우기 · 되살리기 | 정리(실제 R2 지우기) |
|---|---|---|---|---|
| 주인 (`owner`) | O | O (+ 계정 전체 무료 범위) | O | O |
| 사람 편집자 (`member-editor`) | O | O | O | X |
| 설치판 열쇠 Bearer (`bridge`) | O | O | O | X |
| 사람 보기만 (`member-viewer`) | O | O | X | X |
| 출력 · OBS (`output`) · 리모컨 PIN (`remote`) · 편집기 PIN (`editor`) | **O** (자기 교회 파일을 id 로) | X | X | X |

- 권한은 `protocol.mjs` 의 `can()` 그대로 — 목록 `read` · 올리기 `write` · 정리 `manage`. 받기 · 글꼴 CSS 는 길의 cap 을 비워 두어 **그 교회의 로그인이면 누구나** (출력 주소 바꾸기가 이것에 기댄다).
- 교회는 로그인(쿠키 서명 · 열쇠 앞머리)에서만 정한다. 모든 D1 질의는 `WHERE church_id = ?1` 로 시작한다. **예외 둘**: 무료 범위 안전장치가 계정 전체 저장량 · 이 달 작업 수의 합(SUM)을 본다 (`usage.mjs guardRow`, 합만 쓰고 다른 교회 줄은 꺼내지 않는다).
- 설치판(2B)은 `Authorization: Bearer 교회.열쇠`(설치판 연결 열쇠)로. 쿠키 · `X-WorshipCue` 가 필요 없다. 브라우저(쿠키)로 쓰는 요청은 1단계처럼 `X-WorshipCue: 1` + 같은 Origin.
- 오류: `{ok:false, error}` + 400 머리글 · 401 로그인 · 403 권한 · 404 없음(남의 교회 것도 404) · 409 조각 빠짐 · 410 정리됨 · 411 Content-Length 없음 · 413 큼 · 415 종류 · 416 Range · 429 나눠 올리기 너무 많음 · 502 R2 오류 · 503 저장통 없음 · **507 quota**.

## 2. 받는 파일

| 묶음 (`kind`) | 종류 (Content-Type) | 확장자 | 한 파일 |
|---|---|---|---|
| image | png · jpeg · gif · webp · avif · bmp · svg+xml(10 MiB) | png jpg jpeg jfif gif webp avif bmp svg | 50 MiB |
| video | mp4 · webm · quicktime · ogg | mp4 m4v webm mov ogv | 2 GiB |
| audio | mpeg · mp4 · aac · wav · ogg · flac | mp3 m4a aac wav ogg oga opus flac | 500 MiB |
| font | ttf · otf · collection · woff · woff2 | ttf otf ttc woff woff2 | 50 MiB |
| pdf | application/pdf | pdf | 200 MiB |

- 종류가 비었거나 `application/octet-stream` 이면 확장자로 정한다. 종류와 확장자의 묶음이 다르면(그림이라면서 `.mp4`) 415. HTML · 스크립트 같은 것은 받지 않는다.
- 이름 (`X-Asset-Name`, `encodeURIComponent`): **경로를 떼고** 파일 이름만 남긴다(`C:\Users\김택근\…\부활절.png` → `부활절.png`). R2 열쇠는 `churches/{교회}/assets/{sha256}/{안전한 이름}` — PC 경로 · 사용자 이름은 열쇠에도 목록에도 남지 않는다.

## 3. 올리기

### 3-1. 한 번에 (90 MiB 이하) — `POST /api/v1/assets`

```http
POST /api/v1/assets
Authorization: Bearer sample.xxxxxxxx           (또는 쿠키 + X-WorshipCue: 1)
Content-Type: image/png
Content-Length: 200000
X-Asset-Sha256: 9f86d0…(16진수 64자)
X-Asset-Name: %EB%B6%80%ED%99%9C%EC%A0%88.png
(글꼴만, 선택) X-Asset-Font-Family: %EA%B5%AC%EC%84%B8%EA%B5%B0%EC%B2%B4 · X-Asset-Font-Weight: 700 · X-Asset-Font-Style: normal

<파일 바이트>
```
```json
201 { "ok": true, "existed": false, "id": "NWrCckw6VCzhscEB", "sha256": "9f86d0…", "size": 200000, "type": "image/png",
      "asset": { "id": "…", "sha256": "…", "name": "부활절.png", "type": "image/png", "kind": "image", "size": 200000, "createdAt": 1795…, "createdBy": "Xy12ab34cd",
                 "updatedAt": 1795…, "deleted": false, "deletedAt": null, "purged": false, "verified": true, "font": null, "url": "/api/v1/assets/NWrCckw6VCzhscEB" } }
200 { "ok": true, "existed": true, "id": "<먼저 있던 것>", … }          ← 같은 교회에 같은 sha256 이 있으면 본문을 버린다
```
- **sha256 확인**: Worker 는 본문을 읽지 않고 R2 로 흘려보내며 `put(…, {sha256})` 을 준다 → R2 가 받은 바이트로 다시 계산해 다르면 거절 → **400 `{reason:'sha256'}`**, 아무것도 남지 않는다 (Worker CPU 10 ms 한도에 걸리지 않는다). 실제 R2 에서도 확인함(7절).
- 지운 표시가 있는 같은 파일을 다시 올리면 되살려서 `existed:true` (정리된 것이면 새로 올린다).
- 90 MiB 를 넘으면 413 `{multipart:true}` → 3-3.

### 3-2. 있나 묻기 — `GET /api/v1/assets?sha256=<hex>`

`{ok, asset:{…}|null}` — 지운 표시가 없는 것만. 큰 파일은 먼저 묻고 없을 때만 올린다.

### 3-3. 나눠 올리기 (90 MiB 넘는 영상 · 끊겨도 이어 올리기) — R2 multipart

| 요청 | 답 |
|---|---|
| `POST /api/v1/assets/uploads` 머리글은 3-1 과 같고 `X-Asset-Size: <바이트>` 를 더함, 본문 없음 | 201 `{ok, existed:false, uploadId, partSize, parts, size}` · 이미 있으면 200 `{ok, existed:true, id, …}` |
| `PUT /api/v1/assets/uploads/{uploadId}/parts/{n}` 본문 = n 번째 조각 (n 은 1부터, 마지막 말고는 `partSize` 그대로) | `{ok, n, etag}` · 크기가 다르면 400 `{expected}`. 같은 조각을 다시 올리면 덮어씀 |
| `GET /api/v1/assets/uploads/{uploadId}` | `{ok, uploadId, sha256, name, size, partSize, parts, done:[1,2,…], createdAt}` — 끊긴 뒤 어디까지 했나 |
| `POST /api/v1/assets/uploads/{uploadId}/complete` | 3-1 과 같은 답 (201). 빠진 조각이 있으면 409 `{missing:[…]}` |
| `DELETE /api/v1/assets/uploads/{uploadId}` | `{ok, aborted:true}` |

- `partSize` = 32 MiB (서버가 정함 · R2 는 마지막 말고 모두 같은 크기, Workers 요청 본문 100 MB 아래). 조각 수 = ⌈size / partSize⌉.
- 한 교회가 동시에 열어 둘 수 있는 나눠 올리기는 8개 (넘으면 429). 하루 넘게 끝나지 않은 것은 정리 명령이 그만둔다 (조각도 저장 용량을 쓴다).
- **나눠 올린 것은 R2 가 전체 sha256 을 확인하지 못한다** (multipart 에 checksum 이 없다) → `verified:false`. 같은 내용을 나중에 한 번에 올리면(90 MiB 이하일 때) R2 가 확인한 것으로 바뀐다. 교회 안의 편집 권한자를 믿는 정도의 약속이고, 다른 교회에는 번지지 않는다(열쇠가 교회마다).
- 끝내는 사이 같은 내용이 다른 기기에서 다 올라왔으면 조각을 버리고 그것을 돌려준다.

## 4. 받기 — `GET /api/v1/assets/{id}` (HEAD 도)

- `Range: bytes=a-b` · `a-` · `-n` 하나 → 206 + `Content-Range`. 범위 밖 416 + `Content-Range: bytes */size`. 여러 범위는 전체(200). `If-Range` 가 ETag 와 다르면 전체.
- 머리글: `ETag: "<sha256>"` · `Cache-Control: private, max-age=31536000, immutable` (id 의 내용은 바뀌지 않는다, 다른 교회 · 공용 캐시와 나누지 않음) · `Accept-Ranges: bytes` · `X-Content-Type-Options: nosniff` · `Content-Security-Policy: … sandbox` (주소를 바로 열어도 SVG 안 스크립트가 돌지 않게) · `Cross-Origin-Resource-Policy: same-origin` · `Content-Disposition: inline; filename*=UTF-8''…`.
- `If-None-Match` 가 맞으면 **304 (R2 를 부르지 않음)**. HEAD 도 R2 를 부르지 않는다(D1 의 줄로 답함).
- 지운 표시만 한 파일은 **정리 전까지 그대로 받는다** (`X-WP-Asset-Deleted: 1`) — 예배 중 실수로 지워도 화면이 비지 않게. 정리된 것 410, 없는 것 · 남의 교회 것 404.
- **`?fallback=<1단계 주소>`** (선택): 404 · 410 · R2 오류일 때 그 주소로 302. `mediaPath()` 길(`/media/…` · `/lib/file?p=…` · `/hymn/…` · `/hymnal/score/…`)만 받고 다른 곳(밖 주소 · `/api/…`)이면 무시한다. → 5절.
- Worker 는 줄을 30초 기억한다(같은 그림을 출력 여럿이 받을 때 D1 읽기 줄임). 지운 · 고친 것이 다른 Worker 인스턴스에 닿는 데 최대 30초.

## 5. 설치판 중계(1단계)와 R2 — 고르는 규칙

출력 자료 안의 파일 주소는 **설치판(2B)이 정한다**. Worker 는 고르지 않고, 받은 주소대로 R2 또는 중계실로 보낸다.

1. 설치판이 그 파일의 sha256 으로 클라우드에 있음을 확인했으면(3-2) 또는 올려서 id 를 받았으면 → `/api/v1/assets/{id}?fallback=<원래 1단계 주소>`.
2. 그렇지 않으면(아직 안 올림 · 올리는 중 · **507 quota** · 저장통 오류) → 지금처럼 1단계 주소(`/media/…` · `/lib/file?p=…`) — 중계실이 설치판에 물어 조각으로 보낸다.
3. R2 쪽이 없어지면(정리 410 · 404 · R2 오류) `fallback` 으로 302 → 설치판이 켜져 있으면 중계로 이어진다.
4. 큰 영상은 다 올라갈 때까지 2 (중계), 끝나면 1 로 바꿔 보낸다. 바뀌는 것은 다음 `render` 부터.
5. 찬송가 악보 · 이미지(`/hymn/…` · `/hymnal/score/…`)는 저작권 자료일 수 있어 **올리지 않고 늘 중계** (사관님이 정하실 것 — 9절).
6. 설치판이 꺼져 있으면 R2 에 있는 것만 보인다 → 예배 전에 그 주 큐시트의 파일을 올려 두는 것을 권한다. 3단계 웹 조작 화면(설치판 없음)은 R2 길만 쓴다.

## 6. 목록 · 고치기 · 지우기 · 정리

| 요청 | 권한 | 내용 |
|---|---|---|
| `GET /api/v1/assets?kind=font&limit=100&cursor=…&deleted=1` | read | `{ok, assets:[…], next}` (새것 먼저, `next` 를 그대로 `cursor=` 에. `deleted=1` 지운 표시 것도) |
| `PATCH /api/v1/assets/{id}` `{name?, family?, weight?, style?}` | write | `{ok, asset, warnings?}` — 글꼴 정보는 글꼴에만 |
| `DELETE /api/v1/assets/{id}` | write | `{ok, id, deleted:true}` — **표시만**. 목록에서 숨고, 받기는 정리 전까지 됨 |
| `POST /api/v1/assets/{id}/restore` | write | 지운 표시 지움. 정리된 것은 410 |
| `POST /api/v1/assets/purge` `{olderThanDays:30, dryRun:true, confirm?}` | **manage (주인)** | 정리 명령. 기본은 **보기만**. `dryRun:false` 로 R2 에서 실제로 지움 |
| `GET /api/v1/assets/usage` | read | 7절 |

정리 명령: **살아 있는 파일(지운 표시가 없거나 `olderThanDays` 안에 지운 것)이 하나도 가리키지 않는 내용만** R2 에서 지우고(R2 지우기 · 나눠 올리기 그만두기는 무료 작업), 줄은 남겨 410 이 되게 한다. 하루 넘은 나눠 올리기도 그만둔다. `olderThanDays:0` 은 `confirm:"now"` 가 있어야 한다. 한 번에 500개까지.

```sh
node scripts/assets-purge.mjs                          # 로컬 sample: 지울 것 보기만
node scripts/assets-purge.mjs --usage                  # 사용량 · 무료 범위만
node scripts/assets-purge.mjs --base https://<배포 주소> --church <교회> --days 30 --yes   # 주인 열쇠: WP_OWNER_SECRET 또는 .secrets/<교회>.remote.json
```

## 7. 글꼴 — `GET /api/v1/assets/fonts.css`

교회가 올린 글꼴을 출력 · 조작 화면이 `@font-face` 로 받는다. **로그인한 페이지(출력 · 무대 · 리모컨 · 편집기)는 머리에 `<link rel="stylesheet" href="/api/v1/assets/fonts.css">` 가 저절로 붙는다** (`pages.mjs inject`). 3단계 웹 조작 화면도 같은 줄을 넣으면 된다.

```css
@font-face{font-family:"구세군체";font-weight:400;font-style:normal;font-display:block;src:url("/api/v1/assets/R…") format("truetype");}
@font-face{font-family:"구세군체";font-weight:700;font-style:normal;font-display:block;src:url("/api/v1/assets/B…") format("truetype");}
@font-face{font-family:"구세군체 Bold";font-weight:1 1000;font-style:normal;font-display:block;src:url("/api/v1/assets/S…") format("woff2");}
```

- **이름 · 굵기 · 모양을 파일과 따로 적는다** (`font_family` · `font_weight` · `font_style`). 브라우저는 `@font-face` 에 적힌 값을 믿고 파일 안 OS/2 굵기 값은 보지 않는다 → '구세군체 Bold' 파일 안 굵기가 400 이어도 Regular 와 겹쳐 숨는 일(render.js `localFontAlias` 설명, 2026-11-10)이 없다.
- 처음 값: 올릴 때 머리글로 적은 값 → 없으면 **파일 안에서 읽은 제안** (`fonts.mjs`: TTF · OTF · TTC 첫 글꼴 · WOFF 의 name 1번(GDI 가족 이름, 한국어 먼저) · 2/17번 · OS/2 굵기 · 기울임). 이름은 'Bold' 인데 파일 굵기가 400 처럼 200 이상 어긋나면 **이름 쪽(700)으로 적고 `warnings` 로 알린다**. WOFF2 는 안을 읽지 않으니 이름 · 굵기를 적어 보낸다. 나눠 올린 글꼴도 안을 읽지 않는다.
- `font-family` 는 **설치판 템플릿에 적힌 글꼴 이름과 같아야** 한다 (`render.js` 가 `font-family:'구세군체 Bold', …` 처럼 템플릿 이름을 그대로 쓴다). 이름 1번을 제안하는 것도 그래서다. 다르면 `PATCH` 로 고친다.
- 한 이름에 굵기가 하나뿐이면 `font-weight:1 1000` 으로 내보내 템플릿이 굵게(700)를 불러도 가짜 굵기를 덧입히지 않는다. 같은 이름 · 굵기 · 모양이 둘이면 **나중에 올린 것**만 쓰고, 올리기 · 고치기 답에 `warnings` 로 알린다.
- `font-display:block` (예배 화면에 잠깐 다른 글꼴이 보였다 바뀌지 않게, 최대 3초 기다림). CSS 는 `ETag` + `private, max-age=60`.
- 같은 파일을 두 이름으로 등록하는 것(별칭)은 아직 없다 — 같은 sha256 은 하나로 합쳐지므로. 필요하면 덧붙인다.

## 8. R2 무료 범위 · 안전장치 · 사용량

R2 에는 지출 상한 설정이 없다 → 올리기 전에 Worker 가 막는다 (`usage.mjs guard`, `routes.mjs quotaDecision`).

| 무료 범위 (계정 전체, 2026-11-28 확인 — architecture.md 8절) | 세는 법 | 80% 에 닿으면 |
|---|---|---|
| 저장 10 GB-월 | D1 `asset_blobs` 크기 합 + 나눠 올리는 중인 크기 (지금 양 ≤ 8 GB 이면 달 평균도 ≤ 8 GB) | **올리기 · 나눠 올리기 시작 507 `{error:'quota', reason:'storage'}`** |
| A 작업 100만/월 (put · multipart 시작 · 조각 · 끝내기) | 작업마다 D1 `asset_usage.class_a` 에 바로 더함 | **507 `reason:'classA'`** (나눠 올리기는 시작할 때 조각 수 + 2 만큼 미리 봄) |
| B 작업 1,000만/월 (get) | Worker 안에 모았다가 1분마다 한 번 더함 (D1 쓰기 한도를 아끼려고) | 막지 않는다 (예배 중 화면이 멈추면 안 됨) — `usage.free.classB.warn:true` |
| (교회 한도, 선택) | D1 `asset_limits.max_bytes` | 507 `reason:'church'` |

- 한도 값은 Worker 변수로 바꿀 수 있다: `ASSETS_FREE_BYTES`(기본 10e9) · `ASSETS_FREE_CLASS_A`(1e6) · `ASSETS_FREE_CLASS_B`(1e7) · `ASSETS_GUARD_RATIO`(0.8). GB 는 10^9 로 셈 (작게 잡음).
- 이미 있는 파일 · 받기 · 지우기 · 정리는 막지 않는다. 507 이면 설치판은 올리기를 멈추고 5절의 2 (중계) 로 돌아간다.
- 이 계정의 R2 저장통은 이것 하나다. 다른 저장통을 만들면 그쪽 사용량은 이 셈에 들어가지 않는다 — 그때는 한도 값을 줄인다.
- 받기 수는 어림이다: Worker 인스턴스가 1분 안에 내려가면 그만큼 덜 센다. 304 · HEAD 는 R2 를 부르지 않아 세지 않는다(실제로도 R2 작업이 아니다).
- 어림 (우리 교회): 배경 · 광고 그림 수백 장 + 영상 몇십 개 ≈ 수 GB, 올리기 A 작업 월 수천, 받기는 출력 화면 4개가 처음 한 번씩(그 뒤 304 · 브라우저 기억) → B 월 수만. 저장만 무료 범위의 수십 % 이고 나머지는 1% 아래.

`GET /api/v1/assets/usage`:
```json
{ "ok": true, "month": "2026-11",
  "church": { "assets": 77, "deletedAssets": 7, "objects": 70, "bytes": 239000000, "deletedBytes": 1200, "uploads": 0, "uploadingBytes": 0, "classA": 119, "classB": 76, "limitBytes": null },
  "free": { "storage": { "used": 239100000, "limit": 10000000000, "stopAt": 8000000000, "ratio": 0.0239 },
            "classA": { "used": 145, "limit": 1000000, "stopAt": 800000, "ratio": 0.0001 },
            "classB": { "used": 101, "limit": 10000000, "stopAt": 8000000, "ratio": 0, "warn": false },
            "uploadsBlocked": false, "reason": null } }
```
`free` 는 주인에게만 (계정 전체 합이라).

## 9. 시험한 것 · 확인 못 한 것

**단위** (`npm test`, 2C 9개): 길 · 권한 표 · 종류/확장자 · 이름(경로 떼기) · R2 열쇠 · Range 11가지 · 안전장치 판단(저장 · A · 교회 · 경계값) · 글꼴 정보 확인 · 글꼴 CSS(겹침 · 한 굵기) · 글꼴 파일 읽기(구세군체 Bold 400 → 700 · 보통 · 기울임 · OTF · WOFF · WOFF2 · 못 읽음 · 굵기 낱말).

**끝에서 끝** (`npm run test:e2e`, 2C 11개, wrangler dev + 로컬 R2 · D1): 로그인 없이 받기 · 목록 · 올리기 · 글꼴 CSS 401 · 쓰기 보호 403 · 한 번에 올리기 · **같은 파일 두 번(existed)** · sha256 다름 400 · 종류 415 · 출력 · 리모컨 PIN 받기 O / 목록 · 올리기 · 지우기 X · ETag 304 · HEAD · 캐시 머리말 · **Range**(206 · 끝에서 n · 416 · If-Range) · **교회 분리**(남의 id 404 · Range 404 · 있나 묻기 null · 목록에 없음 · 지우기 · 고치기 404 · 같은 내용을 따로 저장 · 열쇠 앞머리 바꿔 끼우기 401 · Bearer 도 자기 교회만) · 설치판 Bearer 로 올리기 · 묻기 · 사용량 (정리 403) · **나눠 올리기**(조각 크기 400 · 빠진 조각 409 · 이어 보기 · 끝내기 · 조각 경계를 넘는 Range · 전체 sha256 · 다시 하면 existed · 90 MiB 넘는 한 번에 413 · 그만두기) · 지우기(표시만 · 받기 됨 · 되살리기 · 정리 보기만 · confirm · 실제 정리 → 410 · fallback 302 · 밖 주소 fallback 무시 · 다시 올리면 새로) · 글꼴(제안 · 경고 · CSS · 적은 값 우선 · `1 1000` · 고치기 · 304 · 다른 교회 CSS 에 없음 · **출력 페이지에 link**) · 목록 쪽 넘기기 · **안전장치 507 quota**(교회 한도로, 나눠 올리기 시작도) · 받기는 그대로. 1단계 · 2A 시험과 함께 34개 모두 통과.

**실제 R2** (`node scripts/r2-remote-check.mjs`, Worker 는 로컬 · R2 만 원격, 배포 없음): 견본 그림 300 KB + 영상 32 MiB+4 KB(조각 둘) — 올리기 · existed · **실제 R2 가 sha256 다름을 거절** · Range · 조각 경계 Range · 전체 sha256 · 사용량 · 정리 → 원격에 있던 열쇠가 정리 뒤 없어짐을 열쇠마다 확인. 끝난 뒤 저장통 `object_count: 0`.

**확인 못 한 것**
- 배포한 Worker(workers.dev) 에서 돌린 것은 아니다(배포는 설계 작업자 몫). 무료 플랜 요청 본문 100 MB · CPU 10 ms 안에 드는지는 배포 뒤 확인 필요 — 한 번에 올리기는 본문을 Worker 가 읽지 않고 흘려보내고, 글꼴만 메모리에 읽는다(50 MiB 까지).
- 실제 OBS · 다른 PC 브라우저에서 `fonts.css` 로 구세군체가 그려지는 화면은 그림으로 보지 않았다 (CSS · 페이지 머리 link 까지만).
- 계정의 실제 R2 요금 대시보드 수와 이 셈이 맞는지 (한 달 지나야 비교 가능).
- R2 의 '끝내지 않은 multipart 7일 뒤 그만두기' 기본 규칙에 기대지 않고 정리 명령이 하루 뒤 그만두게 했다 — 정리 명령을 주기적으로 돌리는 것(Cron)은 아직 없다.

## 10. 2B (`web-sync`) · 4B (`web-media`) 작업자에게

- **2B**: 올리기 순서 = 파일 sha256 계산 → `GET ?sha256=` → 없으면 90 MiB 이하는 `POST /api/v1/assets`, 넘으면 나눠 올리기 → 받은 `id` 로 출력 주소를 `/api/v1/assets/{id}?fallback=<원래 주소>` 로 바꿈 (5절). `existed:true` 도 성공이다. **507 이면 그 예배 동안 올리기를 멈추고** 원래 주소를 그대로 둔다. 나눠 올리기는 `uploadId` 를 저장해 두었다가 끊기면 `GET /uploads/{id}` 의 `done` 을 보고 이어 올린다(하루 안에). `X-Asset-Name` 은 파일 이름만 `encodeURIComponent` (경로 금지 — 서버도 떼지만). 글꼴은 템플릿의 글꼴 이름을 `X-Asset-Font-Family` 로, 굵기를 알면 `X-Asset-Font-Weight` 로 같이 보내 주면 제안보다 정확하다.
- **4B**: 영상은 `<video src="/api/v1/assets/{id}">` 그대로 Range 로 앞뒤 넘기기가 된다(같은 Origin 쿠키). PDF(pdf.js)도 같은 주소로 Range 를 쓴다. 휴대폰 카메라 · 실시간 영상은 R2 가 아니다(DO 신호). 녹화 파일을 올려야 하면 나눠 올리기 길을 쓴다.
- 둘 다: 받기는 출력 · PIN 기기도 되지만 **목록은 read 권한**만. 파일 id 는 16자 `[A-Za-z0-9]`, uploadId 는 20자.
