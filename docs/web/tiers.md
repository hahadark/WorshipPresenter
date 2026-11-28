# 웹판 7단계 일부 — 계정 등급(데모 · 무료 · 유료)과 등급별 저장소 (계약 v1, 구현함)

사관님 말씀(2026-11-28): "성경 · 폰트 · 슬라이드를 온라인에 저장 … 무료 · 유료 · 데모 계정 … 무료는 구글 드라이브, 유료는 자체 클라우드플레어".
다른 교회도 가입해 쓰는 쪽(로드맵 7단계)의 일부다. 설계 요약은 `architecture.md` 12절, 사관님이 하실 구글 준비는 [`google-setup.md`](google-setup.md).

코드: `cloud/src/tiers/` (한도표 `limits.mjs` · 등급 확인 `index.mjs` · 데모 `demo.mjs` `demo-seed.mjs` · 옮기기 `move.mjs` · 관리자 길 `admin.mjs`) ·
`cloud/src/drive/` (구글 OAuth · Drive 부르기 `google.mjs` · 형식 · 오류 코드 `format.mjs` · 가입 · 로그인 · 다시 연결 `oauth.mjs` · 문서 · 동기화 `docs.mjs` · 성경 `bible.mjs` · 파일 저장통 `bucket.mjs` · 폴더 `folders.mjs`) ·
`cloud/src/data/bibleimport.mjs` (성경 넣기 길, 두 저장소) · `cloud/migrations/0005_tiers.sql` · 화면 `cloud/client/demo.*` · 로그인 · 주인 화면에 몇 줄.
시험: `cloud/test/tiers.test.mjs`(단위 9) · `cloud/test/tiers-e2e/tiers.test.mjs`(끝에서 끝 12, wrangler dev + 가짜 구글 `cloud/test/fake-google/server.mjs`). 2026-11-28 web 2459f65 를 합친 뒤 전체 단위 168 · 끝에서 끝 58(1단계 · 2A · 2C · 4B · AI · 등급) 모두 통과. 도구: `scripts/tier-move.mjs` · `scripts/bible-upload.mjs`.

## 1. 큰 틀 — 길은 하나, 저장소만 교회마다

```
 설치판 · 웹 조작 화면 · 출력 · 리모컨        Worker (같은 길)                          저장소
 /api/v1/documents · sync · bible · assets ─▶ 로그인 → 권한(can) → 등급(tiers.prepare) ─┬▶ 유료 paid : D1 documents · bible_* · R2 (2A · 2C 그대로)
                                                                                     ├▶ 무료 free : D1 에는 색인만 + 그 교회의 구글 드라이브 (drive/)
                                                                                     └▶ 데모 demo : D1 의 견본 교회 'demo' (하루마다 처음 모습으로)
```

- 다른 작업자(설치판 동기화 `web-sync` · 웹 조작 화면 `web-egui` · 미디어 `web-media`)의 코드는 **바뀌지 않는다**. 답 모양 · 충돌 규칙(If-Match → 409 + 최신) · 변경 순번(seq) · mutationId 중복 방지가 등급마다 같다 — 시험이 같은 값을 비교한다(4절).
- 저장소에 따라 생기는 새 오류는 **오류 코드**(5절)로만 알린다. 모르는 코드는 상태 코드대로 다루면 된다.
- 등급은 D1 `churches.tier` (`demo` · `free` · `paid`). 지금까지의 교회(견본 `sample` 포함)는 **유료 그대로**(열 기본값).
- 등급은 Worker 인스턴스마다 30초 기억한다(`TIER_CACHE_MS`). 그래서 등급을 옮길 때는 30초를 기다린 뒤 복사한다(6절).

## 2. 등급별 한도표 (안 — 값은 사관님이 정하실 것)

코드 한 곳: `cloud/src/tiers/limits.mjs` 의 `TIER_LIMITS`. 바꾸면 그 표만 고친다. (시험 · 운영에서 잠깐 바꿀 때만 Worker 변수 `TIER_OVERRIDES` JSON 으로 칸을 덮는다.)

| 칸 | 데모 | 무료 (구글 드라이브) | 유료 (D1 · R2) | 뜻 · 왜 |
|---|---|---|---|---|
| 저장소 | D1 견본 교회 | 교회의 구글 드라이브 | 우리 D1 · R2 | |
| 문서 수 `documents` | 300 | 2,000 | 20,000 | 살아 있는 큐시트 · 찬양 · 템플릿 · 설정 … (만들 때만 셈, 5분 기억) |
| 성경 번역본 `bibleVersions` | 0 (넣기 없음, 견본 1개) | 3 | 10 | D1 하나 500 MB — 번역본 ≈ 5 MB |
| 파일 전체 `fileBytes` | 0 | 제한 없음(사용자 드라이브 용량) | 20 GB/교회 | 우리 R2 무료는 계정 전체 10 GB |
| 파일 하나 `fileMax` · 나눠 올리기 | 올리기 없음 | 90 MiB · 없음 | 제한 없음(2C: 2 GiB 영상까지) · 있음 | 무료는 Workers 요청 본문 100 MB 아래 한 번에 |
| 동시 화면 `outputs` | 3 | 4 | 20 | 출력 · 무대 · OBS (중계실이 셈) · 출력 열쇠 수 |
| 리모컨 `remotes` | 0 | 2 | 10 | 리모컨 PIN 수 · 동시 리모컨 |
| 기기 열쇠 `credentials` · 사람 `members` | 0 · 0 | 20 · 10 | 50 · 50 | |
| AI `ai` | 없음 | 교회 키로 대신 부르기만 | + 클라우드 음성 인식 | 음성 인식은 우리 계정의 Workers AI 무료 할당을 쓴다 |
| 하루 요청 `requestsPerDay` | 20,000 (데모 전체) | 20,000 | 200,000 | Workers 무료는 계정 전체 10만/일 — 어림(4절 끝) |
| 동기화 한 번 `mutations` · 받기 본문 `pullPayloads` | 20 · 500 | 20 · 40 | 20 · 500 | 무료는 본문 하나 = 구글 부르기 하나 (하위 요청 50개) |
| 로그인 · 되돌림 | 2시간 · 하루 | — | — | |
| 데모 주소별 1분 요청 · 한 시간 새로 들어오기 | 120 · 20 | — | — | 데모가 무료 한도를 다 먹지 않게 |

서비스 전체(`SERVICE_LIMITS`): 하루 새 무료 교회 50 · 한 주소에서 한 시간 가입 3 · 한 구글 계정에 무료 교회 1.

넘으면: 429 `{code:'tier_limit', limit:'<칸 이름>'}` (번역본 · 문서 · 요청 · 열쇠) · 413 `{code:'tier_limit', limit:'fileMax'}` · 막힌 기능 403 `{code:'tier_blocked'}` · 화면 수는 연결이 429 로 거절.

## 3. 무료 — 구글 드라이브

### 3-1. 가입 · 로그인 · 다시 연결 (`drive/oauth.mjs`)

| 길 | 하는 일 |
|---|---|
| `GET /auth/google/start?mode=signup&name=<교회 이름>[&church=<ID>]` | 구글 동의 화면으로. 범위 `openid email drive.file`, `access_type=offline` · `prompt=consent` (갱신 토큰), PKCE S256 + state |
| `GET /auth/google/start?mode=login[&church=]` | 로그인만 (`openid email`, 드라이브 토큰은 건드리지 않음) |
| `GET /auth/google/start?mode=connect` | 주인 로그인 상태에서 드라이브 다시 연결 (토큰 만료 · 사용자가 끊음) |
| `GET /auth/google/callback` | 토큰 받기 → 교회 · 주인 계정(`members.subject = 'google:<번호>'`) · `drive_accounts` → 쿠키 → `/owner?google=welcome` |
| `GET /api/v1/drive` (read) · `POST /api/v1/drive/disconnect` (manage) | 연결 상태(전자우편은 주인에게만 전체) · 끊기(구글에 토큰 거두기 요청, 드라이브 파일은 그대로) |
| `GET /api/v1/tier` (로그인) | `{tier, name, store, moving, limits, usage, drive?:{connected, reconnect}, demo?}` — 웹 조작 화면이 띠 · 안내에 쓴다 |

- 실패하면 `/login?google=<까닭>` (`scope` 드라이브 칸을 끔 · `other-account` 다른 구글 계정으로 다시 연결 · `no-account` · `too-many` · `full` …). 로그인 화면이 한국어로 보여 준다.
- state · PKCE 검증값은 서명한 쿠키 `wp_oauth`(Path=/auth/google, 10분, HttpOnly, SameSite=Lax)에만 — 서버에 저장하지 않는다.
- id_token 은 토큰 주소에서 https 로 바로 받은 것만 읽는다(서명 확인 생략 — 구글 문서가 허락하는 경우, 아래 문서 표). `aud` = 우리 client id, `iss` = accounts.google.com, 만료를 본다.
- 한 구글 계정이 또 가입하면 새 교회를 만들지 않고 그 교회로 들어간다(`google=exists`). 교회 ID 는 `c-` + 8자(고를 수도 있음, `demo` 같은 이름은 막음).
- 다른 구글 계정으로 다시 연결하는 것은 막는다 — `drive.file` 은 '이 앱이 그 계정에 만든 파일' 만 보이므로 앞의 파일이 안 보인다.

### 3-2. 토큰 보관

- 갱신 토큰 · 접근 토큰은 AES-GCM 256 으로 **봉인해 D1 `drive_accounts`** 에. 열쇠 = HKDF(Worker 비밀값 `AI_SECRET` 또는 `AUTH_SECRET`, salt `wp-drive-token-v1`, info `church:<교회>`), 덧붙임 자료 = 교회 ID (`ai/seal.mjs` 와 같은 방식, 목적 글만 다름). 교회 줄을 다른 교회로 옮겨도 풀리지 않는다.
- 브라우저 · 응답 · 로그에 토큰 · 전자우편을 내보내지 않는다(시험이 응답에 토큰 꼴이 없는지 본다). `console.error` 에는 오류 이름만.
- 접근 토큰은 Worker 메모리 + D1(봉인)에 기억하고 2분 앞서 갱신한다. 갱신이 `invalid_grant` 면 토큰을 지우고 `disconnected_at` 을 적는다 → 503 `drive_disconnected`.

### 3-3. 드라이브 안 (사람이 봐도 알게)

```
내 드라이브/워십프리젠터/<교회 이름>/
  큐시트/주일 예배 (cue1ab).json        ← 문서 봉투 (아래) — 이름이 바뀌면 파일 이름도 바뀜
  찬양/ · 템플릿/ · 테마/ · 자막/ · 매크로/ · 화면 구성/ · 소스/ · 설정/ · 사용 기록/
  성경/KRV/번역본.json · 43 요한복음.json …    ← 책마다 하나
  파일/그림/환영.png · 파일/영상/ · 파일/소리/ · 파일/글꼴/구세군체.woff2 · 파일/PDF/   ← 원래 형식 그대로
```
- 문서 봉투: `{"worshipPresenter":1,"kind":"cue","id":"…","revision":3,"updatedAt":…,"updatedBy":"…","deleted":false,"payload":{설치판 JSON 그대로}}`. 앱 속성(appProperties) `wpKind · wpId · wpRev` 를 같이 단다.
- 지우기 = 봉투를 `deleted:true` 로 쓰고 **휴지통으로**(사람이 드라이브에서 되살릴 수 있게). 되살리면 휴지통에서 꺼낸다.
- 폴더 id 는 D1 `drive_accounts.folders` (JSON). 처음 쓸 때 만들고, 둘이 동시에 만들면 하나만 남긴다.

### 3-4. 판 번호 — 드라이브 표시와 우리 revision

드라이브에는 '조건부 쓰기(If-Match)' 가 없다(files.update 문서에 없음). 그래서 **판 번호는 D1 `drive_docs.revision` 이 정한다**:

1. 잡기: 그 줄의 판이 `If-Match` 이고 아무도 쓰는 중이 아니면 `pend_token` 을 건다 (한 SQL 문 — 원자적). 아니면 409 + 최신(본문은 드라이브에서).
2. 드라이브에 봉투(판 = base+1) 쓰기 (multipart 한 번). 실패하면 잡은 것을 풀고 오류 코드.
3. 마무리: 판 +1 · 변경 기록 한 줄(`changes`, 유료와 같은 표 · 같은 순번) — D1 한 묶음.
4. 2 와 3 사이에 Worker 가 죽으면, 30초 넘은 잡기를 다음 요청이 본다: 드라이브 봉투가 base+1 이면 **마무리해 주고**(그 mutationId 그대로), 아니면 **풀어 준다**. 만드는 중(파일 id 모름)이면 앱 속성으로 드라이브에서 찾는다. 시험함(끝에서 끝 '쓰다가 죽은 경우').
5. 30초 안의 쓰는 중에 또 쓰면 409 `{conflict:true, code:'drive_busy', current}` — 충돌과 같게 다루면 된다.

드라이브의 `version`(파일마다 오르는 수)은 마지막으로 쓴 값을 `drive_docs.drive_version` 에 적어 둔다(드라이브에서 사람이 고쳤는지 볼 때 — 지금은 쓰지 않음). 사람이 드라이브에서 봉투를 고치면 다음 읽기부터 그 내용이 보이지만 판 번호는 그대로라 동기화는 알지 못한다 → **드라이브의 파일은 '보기 · 백업용', 고치기는 앱에서**. 고쳐서 JSON 이 깨지면 502 `drive_corrupt`, 휴지통을 비워 아주 지우면 410 `drive_missing` (다음 쓰기는 새 파일로).

### 3-5. 동기화 (유료와 같은 모양)

- 받기 `GET /api/v1/sync?after=` : 변경 기록은 D1, 최신 판의 본문은 드라이브에서 읽어 싣는다. 한 쪽에 본문 **40개까지**(넘으면 `more:true` — 받는 쪽은 이미 `more` 를 따라 또 부른다). 동시에 6개씩(Workers 한 요청의 동시 연결 수).
- 올리기 `POST /api/v1/sync` : 20개까지, 같은 문서는 차례대로 · 다른 문서는 동시에. 결과 `applied · duplicate · conflict · missing · invalid` 그대로. 구글 오류(연결 끊김 · 가득 참)면 요청 전체가 그 오류 — **같은 mutationId 로 다시 보내면** 들어간 것은 `duplicate` 로 온다.
- 목록 `?include=payload` 는 한 쪽 40개까지.

### 3-6. 성경 — 책 파일 + 기억

- 넣기 (등급과 상관없이 같은 길, 주인): `PUT /api/v1/bible/versions/{id}` `{name, language, licenseNote}` → `PUT …/books/{1~66}` `{chapters:[[[절, 본문], …], …], headings:[[장, 절, 글], …]}` · `DELETE …/{id}`. 도구 `node scripts/bible-upload.mjs --db <bible.db 사본> --version KRV …` (유료는 기존 `bible-import.mjs` 로 D1 에 바로 넣어도 됨).
- 책 파일은 **장마다 한 줄**인 JSON — 사람이 열어 봐도 되고, Worker 는 부른 장의 줄만 `JSON.parse` 한다(시편도 CPU 몇 ms 안).
- 장절 확인(장 · 절 수)은 D1 `drive_bible.structure` 로(유료와 같은 `planPassage`), 본문만 드라이브. 답은 유료와 **필드까지 같다**(시험이 같은 견본 본문으로 유료 답과 비교).
- 빠르게: 책 파일을 Worker 메모리에 기억(32 MB · 10분, 다시 넣으면 새 파일 id 라 저절로 새것). Cloudflare 캐시(`caches.default`)에도 넣어 보지만 **workers.dev 에서는 동작이 문서에 없다**(사용자 도메인에서만 동작한다고 적혀 있음) → 메모리 기억을 기본으로 삼는다.
- 잰 값 (로컬 wrangler dev + 가짜 구글): 요 3:16-18 첫 조회 43~134 ms, 두 번째 36~69 ms(드라이브를 다시 읽지 않음 — 시험이 가짜 구글의 받기 수로 확인). **진짜 구글 · 배포에서는 재지 않았다**: 첫 조회는 구글 받기 한 번(책 하나 0.1~0.5 MB)만큼 더 걸린다. 목표 '장 하나 300 ms 이내' 는 기억된 뒤에 대한 것이다. Worker 인스턴스가 바뀌면(사람이 적을 때) 첫 조회가 다시 생긴다 → 필요하면 웹 조작 화면이 IndexedDB 에 책을 두는 것을 6단계에서 더한다.

### 3-7. 파일 (그림 · 영상 · 글꼴 · PDF)

- 2C(`assets/index.mjs`)의 코드를 **그대로** 쓴다 — 목록 · 이름 · 종류 · sha256 · 글꼴 정보 · `fonts.css` · Range · 지우기 표시 · 정리. R2 저장통(`env.FILES`) 자리에 드라이브 저장통(`drive/bucket.mjs`, R2 의 put · get · delete 꼴)을 끼울 뿐이다. 파일 목록 줄(`assets` · `asset_blobs`)은 D1 에 있고(작은 것), 바이트는 드라이브 '파일/<묶음>/' 에 원래 이름 · 형식으로. R2 열쇠 → 드라이브 파일 id 는 `drive_files`.
- 올리기: 이어 올리기(resumable) 두 번 부르기 — 본문을 Worker 가 읽지 않고 흘려 보낸다(`FixedLengthStream`, CPU 를 쓰지 않음). 끝나면 드라이브가 계산한 `sha256Checksum` 과 `X-Asset-Sha256` 을 비교해 다르면 그 파일을 지우고 400 `{reason:'sha256'}` (2C 와 같은 답). **드라이브가 sha256 을 주지 않으면 확인하지 못한다**(구글 문서: "if available").
- 받기: 드라이브의 Range 를 그대로 흘려 보낸다(206 · 416 은 2C 가 줄로 먼저 판단). 응답 크기 한도 없음, 하위 요청 1개.
- 지우기 표시 → 정리(`/purge`) = 드라이브 휴지통으로. 우리 R2 무료 범위 안전장치(2C `guard`)는 무료 교회를 세지 않는다(드라이브가 가득 차면 507 `drive_quota`).
- 무료는 **나눠 올리기가 없다**(403 `tier_blocked`) — 90 MiB 넘는 영상은 설치판 중계(1단계 `/media/…`)로 그대로 보이거나, 유료로.

### 3-8. 연결이 끊겼을 때

- 사용자가 구글 계정 설정에서 앱 권한을 거두거나, 갱신 토큰이 만료(아래 '구글 문서')되면: 그 교회의 자료 길은 **503 `{code:'drive_disconnected', reconnect:'/auth/google/start?mode=connect'}`**. `/api/v1/tier` · `/api/v1/drive` 의 `connected:false`.
- **송출은 끊기지 않는다**: 출력 화면 · 중계실은 자료 저장소를 보지 않는다(설치판 · 웹 조작 화면이 보낸 화면을 메모리에 둠). 시험: 끊긴 뒤에도 붙어 있는 화면은 그대로, 새로 붙는 화면도 마지막 화면을 받는다.
- 주인 화면(`/owner`) '등급 · 저장소' 칸에 '다시 연결하기' 가 뜬다. 웹 조작 화면은 503 + `code` 를 보고 같은 안내를 띄우면 된다. 다시 연결하면 앞의 폴더 · 파일을 그대로 이어 쓴다.

## 4. 데모 — 둘러보기 (`tiers/demo.mjs`)

- **켜고 끄기: Worker 변수 `DEMO_ENABLED=1` 일 때만 열린다 (기본 꺼짐 — 공개는 사관님이 정하실 때까지).** 꺼져 있으면 `/demo` 404 · 로그인 화면의 '둘러보기' 링크 숨김 · 이미 받은 데모 쿠키도 죽는다. 시험(`npm run test:e2e`)은 `--var DEMO_ENABLED:1` 로 켠다.
- `GET /demo` (로그인 없이): 견본 교회 `demo` 를 만들고(없으면) **2시간짜리 데모 쿠키**(역할 `demo`, 열쇠 `d:<방문자 번호>`)를 주고 `/demo` 화면으로. 화면: 견본 큐시트 · 찬양 목록 · 성경 찾기 · 출력 화면 열기 · '구글로 무료 시작하기'.
- **여럿이 같은 견본 교회**를 본다(방문자마다 사본을 만들면 D1 쓰기가 방문자 수만큼 든다). 그래서 화면에 '누구나 같이 보는 견본, 개인 정보를 넣지 마세요' 를 띄운다.
- **실시간 중계실은 방문자마다 따로** (`demo~d:<번호>`) — 한 방문자의 송출이 다른 방문자 화면에 가지 않는다(시험함).
- **되돌리기**: Cron 없이, `/demo` 에 들어올 때 마지막 되돌림이 하루(`resetHours`)를 넘었으면 처음 모습으로. 되돌림도 보통 쓰기(판 +1 · 변경 기록)라 동기화가 그대로 따라온다(seq 가 다시 1 이 되지 않음). 둘이 동시에 들어와도 한 번만(조건부 UPDATE). 관리자 `POST /api/v1/admin/demo/reset` 으로 지금 되돌릴 수 있다.
- 견본 자료(`demo-seed.mjs`)는 **모두 지어낸 것** — 찬양 가사 · 성경 본문(장 · 절 수만 실제와 같음) · 그림(SVG). 실제 번역본 · 찬송가 · 교회 자료를 쓰지 않는다.
- 막는 것: 파일 올리기(403 `tier_blocked`) · AI · 사람 초대 · 기기 열쇠(=오래 쓰는 송출 주소) · 성경 넣기 · 주인 화면 — 역할 권한(`demo` = view · remote · editor · source · read · write)과 등급 문(`gate`)으로. 받기 · 읽기는 된다.
- 한도: 주소마다 1분 120요청 · 한 시간 새로 들어오기 20 · 데모 전체 하루 2만 · 동시 화면 3 (모두 Worker 메모리 어림 + D1 하루 합).

## 5. 오류 코드 (다른 작업자가 알아야 할 것)

모든 오류는 `{ok:false, error, code?, …}`. `code` 는 새로 생긴 것만 — 없으면 2A · 2C 의 뜻 그대로.

| code | 상태 | 뜻 | 받는 쪽이 할 일 |
|---|---|---|---|
| `drive_disconnected` | 503 | 구글 연결 끊김 · 토큰 만료 · 권한 거둠 | 주인에게 `reconnect` 주소 안내. 송출은 그대로. 보낼 줄은 쌓아 두고 나중에 같은 mutationId 로 |
| `drive_quota` | 507 | 사용자의 구글 드라이브가 가득 참 | 드라이브 정리 안내. 2C 의 507 `quota`(R2 안전장치)와 코드로 구별 |
| `drive_rate` | 429 | 구글이 잠시 막음 | 조금 뒤 다시 (지수 대기) |
| `drive_unavailable` | 502 | 구글 오류 · 연결 실패 | 다시 |
| `drive_missing` | 410 | 드라이브에서 사람이 아주 지움 | 그 문서는 다음 쓰기에 새로 만들어진다 |
| `drive_corrupt` | 502 | 드라이브의 파일을 사람이 고쳐 깨짐 | 앱에서 다시 저장 |
| `drive_busy` | 409 (`conflict:true`, `current`) | 같은 문서를 다른 곳에서 쓰는 중 | 충돌과 같게 |
| `tier_blocked` | 403 | 이 등급에 없는 기능 | 숨기거나 '유료에서' 안내 |
| `tier_limit` | 429 · 413 · 연결 429 | 등급 한도 (`limit` = 칸 이름) | 안내 |
| `tier_moving` | 423 | 저장소를 옮기는 중 — 쓰기만 막음 | 잠시 뒤 다시 (읽기 · 송출은 됨) |

- **2B (`web-sync`)**: 503 `drive_disconnected` · 423 `tier_moving` · 429 는 '나중에 다시' 로(보낼 줄 유지). 무료 교회의 받기는 한 쪽이 작게(본문 40개) 오므로 `more` 를 꼭 따라갈 것. 성경을 웹에 넣을 때는 `PUT /api/v1/bible/versions/…` 를 쓰면 등급과 상관없다.
- **3 (`web-egui`)**: 시작할 때 `GET /api/v1/tier` 로 등급 · 한도 · 드라이브 연결을 보고 띠를 띄운다(데모: '같이 보는 견본' · 무료 끊김: '다시 연결'). `tier_blocked` 인 단추는 숨긴다. 데모 쿠키로도 `/bridge` 에 붙을 수 있다(자기만의 중계실).
- **4B (`web-media`)**: 무료 교회의 영상은 `/api/v1/assets/{id}` Range 그대로(드라이브에서 흘려 옴). 90 MiB 넘는 것은 무료에 없다.

## 6. 등급 옮기기 (무료 ↔ 유료) — 주인용 관리 길

결제는 만들지 않았다. 관리자가 Worker 비밀값 `ADMIN_TOKEN` 으로:

```sh
node scripts/tier-move.mjs --church c-abcd1234 --info                 # 등급 · 저장소별 수
node scripts/tier-move.mjs --church c-abcd1234 --to paid               # 무료 → 유료 (드라이브 → D1 · R2)
node scripts/tier-move.mjs --church c-abcd1234 --to free               # 유료 → 무료 (주인이 먼저 구글 연결 — /auth/google/start?mode=connect)
node scripts/tier-move.mjs --church c-abcd1234 --cleanup               # 끝난 뒤 옛 저장소 정리
# 배포에서는 --base https://<주소> 와 WP_ADMIN_TOKEN
```
- 차례: 시작(쓰기 423 `tier_moving`, 읽기 · 송출은 됨) → **30초 기다림**(다른 Worker 인스턴스의 등급 기억이 다 지나게, 그 전에는 425) → 문서(15개씩) → 성경(책 하나씩) → 파일(3개씩, 흘려 보냄) → 등급 바꿈. 한 번 부를 때 조금씩 — 하위 요청 50 · CPU 10 ms 안에서. 도구가 끝날 때까지 되풀이해 부른다. 중간에 멈추면 다시 실행하면 이어서, `--cancel` 로 그만두기.
- **판 번호 · 변경 순번은 그대로 이어진다**: 두 저장소가 `changes` 표를 같이 쓰고, 옮길 때 revision 을 그대로 옮긴다 → 설치판 · 웹 화면은 동기화를 처음부터 다시 받을 필요가 없다(시험: 옮기기 전후 `sync?after=0` 이 같은 seq · revision).
- 정리: 무료 → 유료 뒤에는 D1 의 드라이브 색인만 지운다(**사용자 드라이브의 파일은 그 사람 것이라 지우지 않는다**). 유료 → 무료 뒤에는 D1 본문 · R2 바이트를 지운다.

## 7. 개인정보 · 보안 · 교회 단위 분리

- 교회는 로그인(쿠키 서명 · 열쇠 앞머리 · 데모 쿠키)에서만 정한다. 새 표도 모두 `church_id` 가 첫 열쇠, 모든 질의가 `church_id = ?1`. 드라이브는 **그 교회의 토큰으로 그 교회 폴더만** 부른다(토큰이 교회마다 봉인되어 다른 교회 것을 쓸 길이 없다).
- 시험한 것(끝에서 끝): 무료 교회 둘(구글 계정 둘) — 같은 kind/id 를 만들어도 따로 · 남의 문서 GET 404 · 목록 · 동기화에 안 보임 · 남의 성경 404 · 남의 파일 id 404 · 유료 견본 교회와도 서로 안 보임 · **가짜 구글이 '남의 파일을 부른 횟수' 0** · 열쇠 앞머리를 남의 교회로 바꿔 끼우면 401. 데모 쿠키로 무료 · 유료 교회의 문서 · 파일 · 성경 404, 방문자끼리 중계실 따로.
- 토큰 · 전자우편은 로그에 남기지 않는다. 데모는 '같이 보는 견본' 이라 화면에 개인 정보 경고.
- 데모 · 무료 가입이 우리 무료 한도를 먹지 않게 요청 한도(2절). 가입은 구글 계정 하나에 하나 · 주소마다 한 시간 3 · 하루 50.

## 8. 비용 · 무료 한도 (2026-11-28 공식 문서로 확인)

| 항목 | 문서에 적힌 것 | 이 설계에서 |
|---|---|---|
| Workers 무료: 하위 요청 | "Subrequests per invocation 50" (Workers Free), "Subrequests to internal services 1,000" (R2 · KV · D1) — https://developers.cloudflare.com/workers/platform/limits/ | 구글 부르기만 50 안에 세면 된다: 동기화 받기 본문 40 · 올리기 20 · 옮기기 문서 15 · 파일 3 · (+토큰 갱신 1 · 폴더 만들기 몇) |
| Workers 무료: CPU | "CPU time per HTTP request 10 ms", "Waiting on network requests … does not count toward CPU time" — 같은 문서 | 파일은 흘려 보냄(읽지 않음), 성경은 부른 장 줄만 파싱, 큰 문서(256 KiB 넘음)는 봉투 앞머리만 파싱 |
| 동시 연결 | "up to six connections simultaneously waiting for response headers" — 같은 문서 | 드라이브 읽기는 6개씩 |
| 요청 · 응답 본문 | 요청 본문 무료 플랜 "100 MB", "Cloudflare does not enforce response body size limits" — 같은 문서 | 무료 파일 하나 90 MiB · 받기(Range)는 크기 한도 없음 |
| Cache API | "Workers deployed to custom domains have access to functional cache operations" · workers.dev 는 적혀 있지 않음 — https://developers.cloudflare.com/workers/runtime-apis/cache/ | 메모리 기억을 기본, 캐시는 되면 덤 |
| Drive 한도 | 사용자 · 프로젝트마다 1분 단위 할당("325,000 quota units" per minute per user), 넘으면 403 userRateLimitExceeded — https://developers.google.com/workspace/drive/api/guides/limits | 429 `drive_rate` |
| Drive 올리기 | 단순 · multipart "5 MB or less", 큰 것은 resumable — https://developers.google.com/workspace/drive/api/guides/manage-uploads | 문서 · 성경 책은 multipart(1 MiB · 0.5 MB 아래), 파일은 resumable |
| Drive Range | "specify the portion … by using a byte range with the Range header" — https://developers.google.com/workspace/drive/api/guides/manage-downloads | 그대로 흘려 보냄 |
| Drive 오류 | storageQuotaExceeded 403 · userRateLimitExceeded 403 · rateLimitExceeded 403/429 · authError 401 — https://developers.google.com/workspace/drive/api/guides/handle-errors | 5절 코드 |

무료 교회 하나의 어림(주일 하나): 문서 고치기 수백 번 = 구글 쓰기 수백 · D1 쓰기 수천 행 · Worker 요청 수천 — 무료 교회가 수십 곳이면 **Workers 무료 10만/일 · D1 쓰기 10만/일(계정 전체)에 닿을 수 있다**. 그 전에 유료 플랜(월 $5, Workers 1,000만 요청 · 하위 요청 1만)을 사관님이 정해야 한다(아래).

## 9. 구글 쪽 문서에서 확인한 것 (2026-11-28) — 준비는 [`google-setup.md`](google-setup.md)

| 무엇 | 문서 | 이 설계에서 |
|---|---|---|
| `drive.file` = **Non-sensitive** ("recommended for most use cases"), `drive`(전체) = **Restricted** — 제한 범위를 서버에 저장 · 전송하면 "you must go through a security assessment" | https://developers.google.com/workspace/drive/api/guides/api-specific-auth | `drive.file` 만 쓴다 → 보안 심사 없음 |
| 비민감 범위만 쓰면 "it is not mandatory for your app to complete the app verification process". 동의 화면에 앱 이름 · 로고를 보이려면 가벼운 브랜드 확인 — 확인 전에는 "only your application domain will be visible" | https://support.google.com/cloud/answer/13463073 · https://support.google.com/cloud/answer/10311615 | 처음에는 확인 없이(도메인만 보임) 시작할 수 있다 |
| '테스트' 상태: 테스트 사용자 100명까지, "Authorizations by a test user will expire seven days from the time of consent" (갱신 토큰도) — openid · email · profile 만 쓰는 앱은 예외 | https://support.google.com/cloud/answer/15549945 | 우리는 drive.file 을 쓰므로 **테스트 상태에서는 7일마다 끊긴다** → 실제로 쓰려면 '프로덕션' 으로 게시 |
| 갱신 토큰이 죽는 경우: 6개월 안 씀 · 사용자가 거둠 · 계정마다 클라이언트당 100개를 넘으면 가장 오래된 것이 죽음 · 테스트 상태 7일 | https://developers.google.com/identity/protocols/oauth2 ('Refresh token expiration') | 모두 `drive_disconnected` → 다시 연결. 다시 연결할 때마다 새 갱신 토큰(옛 것은 그대로 두다 100개 넘으면 저절로 죽음) |
| 토큰 주소에서 https 로 바로 받은 id_token 은 "you can be confident that the token you receive really comes from Google" (서명 확인 생략 가능) | https://developers.google.com/identity/openid-connect/openid-connect | 그 경우에만 읽는다 (`readIdToken`) |

## 10. 확인 못 한 것

- 구글 웹 서버 OAuth 문서(https://developers.google.com/identity/protocols/oauth2/web-server)에는 PKCE(`code_challenge`)가 적혀 있지 않다. 우리는 state + PKCE 를 같이 보낸다(가짜 구글은 PKCE 를 요구). 진짜 구글이 웹 클라이언트의 `code_challenge` 를 거절하면 `oauth.mjs start` 에서 두 칸을 빼면 된다(state 와 client secret 이 남음).
- **진짜 구글에는 붙여 보지 않았다**(클라이언트가 없다 — 사관님 몫). 가짜 구글은 문서에 적힌 동작만 흉내 낸다. 특히 진짜 Drive 의 `sha256Checksum` 이 이어 올리기 직후 바로 오는지, multipart PATCH 로 휴지통 꺼내기 + 내용 바꾸기가 한 번에 되는지, 동의 화면에서 드라이브 칸을 끄는 화면 — 실제로 확인해야 한다.
- 배포(workers.dev)에서 CPU 10 ms 안에 드는지 재지 않았다: 가입 콜백(토큰 봉인 둘 · HMAC), 성경 책 넣기(0.5 MB JSON 확인 · 다시 쓰기), 옮기기 단계. 넘으면 무료 플랜에서는 그 요청이 실패한다 → 배포 뒤 `wrangler tail` 로 재 볼 것. 시편 같은 큰 책은 `bible-upload.mjs` 가 나눠 보내게 고칠 수 있다.
- 진짜 구글 · 교회 인터넷에서 영상 Range 가 끊기지 않는지(가짜 구글로는 Range 206 까지만).
- 데모의 Cloudflare 쪽 공격(많은 주소에서 동시에) — 주소별 한도는 Worker 인스턴스 메모리라 어림이다. 실제로 열기 전에 Cloudflare WAF 속도 제한 규칙(무료 1개)을 `/demo` 에 거는 것을 권한다.
- 사람이 드라이브에서 문서 JSON 을 고친 것을 동기화가 알아채는 것(지금은 '보기 · 백업용').
- 무료 교회의 사람 초대는 비밀번호 계정 · 기기 열쇠만(구글 계정 초대는 다음에).

## 11. 사관님이 정하실 것

1. 한도표 값(2절) — 특히 무료 문서 2,000 · 번역본 3 · 화면 4 · 리모컨 2 · 하루 2만 요청 · 하루 새 무료 교회 50.
2. 결제를 붙일지 · 어떻게(지금은 관리자가 손으로 등급을 바꿈).
3. 구글 클라우드 콘솔 준비([`google-setup.md`](google-setup.md)) — 앱 이름 · 지원 이메일 · 개인정보처리방침 주소 · 게시(프로덕션) 여부 · 브랜드 확인.
4. 다른 교회를 받기 시작할 때 Cloudflare 유료 플랜($5/월)으로 바꿀지 — 무료 플랜은 계정 전체 한도(Workers 10만 요청/일 · D1 쓰기 10만 행/일)라 교회가 늘면 모두 함께 멈춘다.
5. 둘러보기를 공개할지 · 견본 자료 내용 · `/demo` 에 Cloudflare 속도 제한 규칙을 걸지.
