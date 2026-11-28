# 웹판 2단계 A — 교회 자료(D1) API · 사람 계정 (계약 v1, 구현함)

`architecture.md` 6절(예약)을 구현한 것. 2B(설치판 동기화) · 2C(R2 파일) · 3(웹 조작 화면) · 6(오프라인)이 이 문서를 계약으로 읽는다.
코드: `cloud/src/data/` (문서 · 동기화 · 성경 · 장절 파서) · `cloud/src/identity/` (사람 계정) · `cloud/migrations/0002_data.sql` · 도구 `cloud/scripts/bible-import.mjs`.
시험: `cloud/test/data.test.mjs` · `data-parser.test.mjs`(설치판 파서 대조) · `data-e2e/data.test.mjs`(wrangler dev + 로컬 D1).

## 1. 누가 무엇을 (역할 · 권한)

| 로그인 | 쿠키 역할 | read 읽기 | write 고치기 | manage 사람 · 열쇠 관리 | 그 밖 |
|---|---|---|---|---|---|
| 주인 기기 열쇠 · 사람 owner | `owner` | O | O | O | 1단계 모두 |
| 사람 editor (편집자) | `member-editor` | O | O | X | 출력 보기 · 리모컨 · 웹 편집기 |
| 사람 viewer (보기만) | `member-viewer` | O | X | X | 출력 보기 |
| 설치판 연결 열쇠 (Bearer) | `bridge` | O | O | X | 송출 주체 |
| 출력 · 리모컨 PIN · 편집기 PIN | `output` · `remote` · `editor` | X | X | X | 1단계 그대로 |

- 교회는 **로그인에서만** 정한다(쿠키 서명 · 열쇠 앞머리). 주소 · 본문의 교회 값은 쓰지 않는다. 모든 질의가 `WHERE church_id = ?1` 로 시작한다.
- 쿠키로 오는 쓰기(PUT · DELETE · POST · PATCH)는 1단계와 같이 `X-WorshipCue: 1` + 같은 Origin 이어야 한다.
- 설치판(2B)은 쿠키 없이 `Authorization: Bearer 교회.열쇠`(설치판 연결 열쇠, `/bridge` 와 같은 열쇠)로 `/api/v1/documents*` · `/api/v1/sync` · `/api/v1/bible/*` 를 부른다. 열쇠 확인은 30초 기억하므로 지운 열쇠는 늦어도 30초 안에 막힌다. 사람 관리 길은 Bearer 로 열리지 않는다.
- 오류는 1단계와 같이 `{ok:false, error}` + 상태 코드 (401 로그인 · 403 권한/다른 곳 · 404 없음 · 409 충돌 · 413 큼 · 422 장절 · 428 If-Match 필요 · 429 잠금).

## 2. 문서 — `/api/v1/documents`

종류(kind): `cue` 큐시트 · `song` 찬양 · `template` · `theme` · `overlay` · `macro` · `scene` 화면 구성 · `source` · `settings`(교회 공통) · `usage` 사용 기록.
id: `[A-Za-z0-9_][A-Za-z0-9_.:~@-]{0,127}` (설치판 short_id · uuid · `tpl-…` 다 들어감, 주소에서는 `encodeURIComponent`). 본문(payload)은 **설치판 JSON 모양 그대로**, JSON 객체 하나, 1 MiB 까지.

| 요청 | 답 |
|---|---|
| `GET /api/v1/documents/{kind}/{id}` | 200 `{ok, kind, id, revision, updatedAt, updatedBy, deleted:false, title, payload}` + `ETag: "rev"` · 없으면 404 · 지웠으면 404 `{deleted:true, revision}` |
| `PUT …/{kind}/{id}` 본문 = payload, `If-Match` 없음 | **새로 만들기만**: 201 `{ok, revision:1, seq}` · 이미 있으면 409 |
| `PUT …` + `If-Match: 3` (`"3"` · `W/"3"` 도 됨) | 판이 3 이면 200 `{ok, revision:4, seq, updatedAt}` · 다르면 **409 + 최신** (아래) |
| `DELETE …` + `If-Match: 4` | 200 `{ok, revision:5, deleted:true, seq}` · If-Match 없으면 428 · 판이 다르면 409 · 없거나 이미 지움 404 |
| 지운 문서 되살리기 | `PUT` + `If-Match: <묘비의 revision>` |
| `X-Mutation-Id: <8~80자>` (PUT · DELETE, 선택) | 같은 값이 다시 오면 쓰지 않고 200 `{ok, duplicate:true, revision, seq}` (응답을 못 받은 재시도용) |

충돌(409)은 자동으로 덮어쓰지 않는다 — 웹 편집기 `/e/save` 의 `{conflict:true}` 규칙과 같다:

```json
{ "ok": false, "conflict": true, "error": "다른 곳에서 먼저 고쳤습니다. 최신 내용을 확인해 주세요.",
  "current": { "kind": "cue", "id": "cue1ab", "revision": 2, "updatedAt": 1795828365729, "updatedBy": "m:Xy12ab34cd", "deleted": false,
               "payload": { "id": "cue1ab", "name": "주일 예배 (고침)", "items": [] } } }
```
`current` 는 문서가 없으면 `null`, 지웠으면 `deleted:true` (payload 없음). `updatedBy` = 기기 열쇠 id 또는 `m:<사람 id>` (`/api/v1/credentials` · `/api/v1/members` 의 id 와 맞춰 이름을 보인다).

**목록** `GET /api/v1/documents?kind=cue&limit=100&cursor=…` → `{ok, documents:[{kind, id, revision, updatedAt, updatedBy, deleted, title, size}], next}`
- `kind` 를 빼면 모든 종류 (kind, id 차례). `limit` 1~200 (기본 100). `next` 가 `null` 이면 끝, 아니면 그대로 `cursor=` 에.
- `deleted=1` 지운 것(묘비)도 · `include=payload` 본문까지 (한 쪽 4 MiB 를 넘으면 그 앞에서 자르고 `next` 를 줌).

```js
// 예: 모든 큐시트 이름 (3 · 6)
let cursor = '', all = [];
do {
  const r = await fetch(`/api/v1/documents?kind=cue${cursor ? `&cursor=${cursor}` : ''}`).then(r => r.json());
  all.push(...r.documents); cursor = r.next;
} while (cursor);
```

## 3. 동기화 — `/api/v1/sync` (2B 설치판 · 6 오프라인이 쓰는 길)

모든 쓰기(문서 API 로 한 것 포함)는 교회마다 1, 2, 3 … 번호(`seq`)가 붙은 변경 기록에 남는다. 받는 쪽은 마지막으로 받은 `seq` 하나만 기억하면 된다.

### 받기 `GET /api/v1/sync?after=<seq>&limit=200`

```json
{ "ok": true,
  "changes": [
    { "seq": 41, "kind": "theme", "id": "th1", "revision": 1, "op": "put", "at": 1795828365000, "by": "Xy12ab34cd", "mutationId": null, "superseded": true },
    { "seq": 42, "kind": "theme", "id": "th1", "revision": 2, "op": "put", "at": 1795828366000, "by": "Xy12ab34cd", "mutationId": "pc-7f3a-0001",
      "payload": { "name": "판 2" } },
    { "seq": 43, "kind": "overlay", "id": "ov1", "revision": 2, "op": "delete", "at": 1795828367000, "by": "m:Ab12cd34ef", "mutationId": null }
  ],
  "next": 43, "more": false, "head": 43, "reset": false }
```
- `payload` 는 **그 문서의 지금 본문**이고, 변경이 최신 판일 때만 실린다. 뒤에 또 고쳐진 변경은 `superseded:true` 로 본문 없이 온다(뒤의 변경이 같은 쪽이나 다음 쪽에 반드시 온다) → 받는 쪽은 `superseded` 를 건너뛰면 된다.
- `more:true` 면 `after=next` 로 바로 또 부른다. `more:false` 면 `next === head`.
- `limit` 1~500 (기본 200), 본문 합 4 MiB 를 넘으면 그 앞에서 자른다(적어도 하나는 준다).
- `reset:true` 는 예약(지금은 늘 false): 변경 기록을 줄인 뒤 `after` 가 너무 옛것이면 true → 목록(`include=payload`)부터 다시 받는다.

```js
// 받는 쪽 (2B 설치판 · 6 IndexedDB 공통 꼴)
async function pullAll(store) {
  let after = await store.getCursor();                 // 처음이면 0
  for (;;) {
    const r = await api(`/api/v1/sync?after=${after}`);
    for (const c of r.changes) {
      if (c.superseded) continue;
      if (c.op === 'delete') await store.remove(c.kind, c.id, c.revision);
      else await store.put(c.kind, c.id, c.revision, c.payload);   // 로컬에 고치다 만 것이 있으면 → 충돌 화면(사본)
    }
    after = r.next; await store.setCursor(after);
    if (!r.more) return r.head;
  }
}
```

### 올리기 `POST /api/v1/sync`

```json
{ "mutations": [
  { "mutationId": "pc-7f3a-0002", "kind": "cue", "id": "cue1ab", "op": "put", "baseRevision": 3, "payload": { "id": "cue1ab", "name": "…", "items": [] } },
  { "mutationId": "pc-7f3a-0003", "kind": "song", "id": "s9", "op": "put", "baseRevision": 0, "payload": { "title": "…" } },
  { "mutationId": "pc-7f3a-0004", "kind": "overlay", "id": "ov2", "op": "delete", "baseRevision": 5 } ] }
```
```json
{ "ok": true, "head": 57, "results": [
  { "mutationId": "pc-7f3a-0002", "status": "applied",  "kind": "cue", "id": "cue1ab", "revision": 4, "seq": 56, "deleted": false },
  { "mutationId": "pc-7f3a-0003", "status": "conflict", "kind": "song", "id": "s9", "current": { "revision": 1, "payload": { … }, … } },
  { "mutationId": "pc-7f3a-0004", "status": "applied",  "kind": "overlay", "id": "ov2", "revision": 6, "seq": 57, "deleted": true } ] }
```
- 한 번에 **20개까지**, 차례대로 적용한다(같은 문서를 두 번 고치면 둘째의 `baseRevision` = 첫째의 결과 판). 한 요청 안에서 `mutationId` 가 겹치면 400.
- 각자 판을 확인한다: 하나가 충돌해도 나머지는 들어간다. `baseRevision` 0 = 새로 만들기만.
- `status`: `applied` · `duplicate`(앞서 같은 `mutationId` 로 들어감 — `revision` · `seq` 는 그때 것, **두 번 쓰지 않는다**) · `conflict`(`current` = 최신) · `missing`(지울 것이 없음) · `invalid`(`error`).
- `mutationId` 는 올리는 쪽이 만들어 **보내기 전에 저장**해 두고, 답을 못 받았으면 같은 값으로 다시 보낸다(응답이 사라져도 두 번 들어가지 않음). 교회마다 따로 센다.
- 올린 뒤에는 받기(`after=`)에서 내 변경도 다시 온다 → `mutationId` 로 알아보고 건너뛰면 된다.
- 충돌은 사본을 만들고 사람이 고른다(`architecture.md` 7절). 서버는 합치지 않는다.

### 2B 설치판 작업자에게

- 열쇠: 설치판 연결 열쇠(`cloud.keyEnc`)를 그대로 `Authorization: Bearer 교회.열쇠` 로. 쿠키 · `X-WorshipCue` 는 필요 없다.
- (제안 — 2B 가 정할 것) 설치판 `Cuesheets` 줄 → `kind:'cue'`, payload 는 설치판 `Cue` 모양 그대로 `{id, name, serviceDate, startTime, items[], memo, pinned, template}`. 서버는 payload 안을 보지 않고(목록 이름 `name`·`title` 만 꺼냄) 빠진 칸을 채우지 않는다. 설정은 기기마다 다른 것(모니터 · 장비 포트 · 열쇠)을 빼고 `kind:'settings', id:'church'` 하나로.
- 처음 맞추기: `GET /api/v1/documents?include=payload` 로 다 받거나 `sync?after=0` 부터. 그 뒤로는 `after=<마지막 seq>` 만.
- 묻는 간격: 예배 중이 아니면 몇십 초 ~ 분 단위로 충분하다(행 읽기 어림은 5절). 바뀐 것이 없으면 답의 `changes` 가 비고 행 읽기는 2~3개.

### 6 오프라인 작업자에게

- 위 `pullAll` 꼴 그대로 IndexedDB 에. 고치기는 '보낼 줄'에 `{mutationId, kind, id, op, baseRevision, payload}` 를 먼저 쓰고, 다시 이으면 20개씩 `POST`.
- 다음 · 이전 같은 순간 조작은 여기로 보내지 않는다(문서가 아님).

## 4. 성경 — `/api/v1/bible/*` (read)

- `GET /api/v1/bible/versions` → `{ok, versions:[{id, name, language, licenseNote, verses, importedAt}]}` — **그 교회가 넣은 것만**.
- `GET /api/v1/bible/passage?version=KRV&ref=요3:16-18` → 설치판 `db.rs resolve_reference` 와 같은 모양:

```json
{ "ok": true, "passage": { "versionId": "KRV", "versionName": "…", "book": { "id": 43, "name": "요한복음", "short": "요" },
  "ranges": [{ "sc": 3, "sv": 16, "ec": 3, "ev": 18 }], "title": "요한복음 3:16-18",
  "verses": [{ "book": 43, "chapter": 3, "verse": 16, "text": "…" }, …], "headings": [{ "chapter": 3, "verse": 16, "text": "…" }] } }
```
- 장절을 못 읽거나 범위 밖이면 **422** + 설치판과 같은 글 · 칸: `{ok:false, error, incomplete, ambiguous, rest, book:{id,name,short,chapters}, candidates:[…]}` (예: `요한복음 3장은 36절까지 있습니다.`, `요한` → `ambiguous:true` + 후보). 번역본이 없으면 404. 한 번에 20묶음까지.
- 장절 해석은 `cloud/src/data/parser.mjs` — 설치판 `crates/wp-core/src/parser.rs` · `books.rs` 를 줄마다 옮긴 것이고, 책 목록은 `crates/wp-core/data/books.json` 을 그대로 묶는다. 입력 118개(`cloud/test/data-golden/cases.json`)를 Rust 코드로 돌린 정답과 정규화 · 책 이름 · 장절 · '장절처럼 보이나'까지 모두 같다. 설치판 파서가 바뀌면 `cloud/test/data-golden/Cargo.toml` 머리말대로 정답을 다시 만든다.
- **7단계(등급)에서 더한 넣기 길** (주인 `manage`, 등급과 상관없이 같음 — 유료는 D1, 무료는 구글 드라이브 책 파일): `PUT /api/v1/bible/versions/{id}` `{name, language, licenseNote}` · `PUT …/{id}/books/{1~66}` `{chapters:[[[절, 본문], …], …], headings:[[장, 절, 글], …]}` · `DELETE …/{id}`. 도구 `scripts/bible-upload.mjs`. 번역본 수는 등급 한도(`tier_limit`). 자세히는 [`tiers.md`](tiers.md) 3-6.
- **등급에 따라 생기는 오류 코드**(`drive_*` · `tier_*`)는 [`tiers.md`](tiers.md) 5절 — 무료 교회의 받기(`sync` · `include=payload`)는 한 쪽 본문이 40개까지라 `more` 를 꼭 따라갈 것.
- 낱말 검색(FTS)은 아직 없다(D1 FTS5 · 행 읽기 어림이 필요 — 3단계에서 정할 것).

**넣기 도구** `node scripts/bible-import.mjs` (`cloud/` 에서):

```sh
node scripts/bible-import.mjs --db <bible.db 사본> --list                                      # 번역본 목록
node scripts/bible-import.mjs --db <bible.db> --version KRV --church <교회> --out krv.sql       # SQL 만
node scripts/bible-import.mjs --db <bible.db> --version KRV --church <교회> --local            # 로컬 D1
node scripts/bible-import.mjs --db <bible.db> --version KRV --church <교회> --only 43:1-3,45:8 --local   # 몇 장만
node scripts/bible-import.mjs --db <bible.db> --version KRV --church <교회> --remote --license-ok         # 배포한 D1 (허락 있는 번역본만)
```
- 주어진 db 를 임시 폴더로 **복사해서 사본만 읽는다** (`-wal` · `-shm` 포함, 끝나면 지움). 다시 넣으면 그 번역본을 지우고 새로 넣는다.
- 원격 D1 은 SQL 문 하나 100 KB 까지라 INSERT 를 90 KB 아래로 나눈다. 장마다 절 수(`structure`)를 번역본 줄에 같이 넣어, 장절 확인에 절 줄을 다 읽지 않는다.
- 크기 어림: 번역본 하나 ≈ 31,000절 ≈ 행 31,000개 · 4~6 MB (D1 하나 500 MB). **원격에 넣는 것은 하지 않았다 — 어떤 번역본을 올릴지는 사관님이 정한다.**

## 5. 사람 계정 — `members`

기기 열쇠(OBS · 휴대폰 · 설치판)는 그대로 두고, **사람**(사관님 · 방송팀 · 찬양팀)에게 이름과 역할이 있는 계정을 준다. 두 가지 로그인:

1. **비밀번호** (기본, 바로 씀): 주인이 만든다 → 처음 비밀번호를 한 번 보여 줌 → 본인이 바꾼다.
2. **Cloudflare Access** (선택, Zero Trust 무료 50명): 사관님이 Access 앱을 `/auth/access` 에 걸고 Worker 변수 `ACCESS_TEAM_DOMAIN` · `ACCESS_AUD` 를 넣으면 열린다. 둘 다 없으면 404. 주인이 `email` 로 등록한 사람만 들어온다. (계정 설정이 필요해 이 작업에서는 켜지 않았고, JWT 확인은 단위 시험만 함.)

| 요청 | 권한 | 내용 |
|---|---|---|
| `POST /auth/member` `{church, login, password}` | 공개 | 비밀번호 로그인 → 쿠키 + `{ok, role, memberRole, home}`. 틀린 횟수: 기기 주소별 5번마다 30초~, 계정별 10번마다 1분~ (최대 15분) |
| `GET /auth/access?church=&next=` | Access 가 붙여 준 JWT | 쿠키 + 302 |
| `GET /api/v1/members` | manage | `{members:[{id, role, label, provider:'password'\|'access', login\|email, createdAt, lastLoginAt}]}` |
| `POST /api/v1/members` `{login, role, label}` 또는 `{email, role, label}` | manage | 201 `{member, password}` — `password` 는 **이때 한 번만** |
| `PATCH /api/v1/members/{id}` `{role?, label?, resetPassword?:true}` | manage | 역할 바꾸기 · 비밀번호 새로 만들기(`password` 한 번) |
| `DELETE /api/v1/members/{id}` | manage | 지움 (같은 이름으로 다시 만들면 새 id) |
| `POST /api/v1/me/password` `{current, next}` | 사람 계정 본인 | 비밀번호 바꾸기 (10자 이상). 이 기기는 새 쿠키로 계속 |

- 역할 · 비밀번호를 바꾸거나 지우면 그 사람의 **앞선 쿠키가 모두 죽고**(쿠키 안 세대 번호), 붙어 있던 실시간 연결도 4003 으로 끊는다. 자기 역할 바꾸기 · 자기 지우기는 409.
- 비밀번호는 `PBKDF2-SHA256(HMAC(AUTH_SECRET, 교회:계정:비밀번호), 소금, 20,000번)` — D1 만 새어서는 맞혀 볼 수 없다. Workers 무료 플랜 CPU 10 ms 때문에 반복을 크게 못 잡는다(로그인 · 바꾸기에만 씀).
- 로그인 화면(`/login`)에 '로그인 이름' 칸을 더했다(비우면 1단계 기기 열쇠 · PIN). **주인 화면(`/owner`)의 사람 관리 화면은 아직 없다** — 위 API 를 3단계 웹 조작 화면이나 주인 화면에 붙일 것.
- 교회 만들기 · 다른 교회 열기는 7단계(범위 밖). 교회 단위 분리는 시험함(6절).

## 6. 교회 단위 분리 — 시험한 것 (`data-e2e/data.test.mjs`)

다른 교회(`data-other`)의 주인 쿠키 · 설치판 열쇠로: 남의 문서 GET 404 · 목록 · 변경 기록에 안 보임 · 같은 kind/id 로 만들면 자기 교회에 따로 생김(남의 것 안 덮음) · 판 번호를 알아도 남의 것 못 지움 · 동기화로 올려도 자기 교회 묘비와 비교됨 · 같은 mutationId 도 교회마다 따로 · 남의 번역본 목록 · 본문 404 · 열쇠 앞머리를 남의 교회로 바꿔 끼우면 401 · 남의 교회 사람 계정 목록 · 지우기 · 로그인 안 됨. 출력 · 리모컨 · 편집기 PIN 쿠키는 자료 길 403.

## 7. D1 무료 한도 안에 두는 법

| 한도 (계정 전체) | 이 API 에서 |
|---|---|
| 요청 하나에 질의 50개 | 문서 하나 쓰기 = 2 (한 트랜잭션 묶음) + 충돌일 때 1. 동기화 올리기 20개 = 앞뒤 확인 2 + 40 |
| 행 읽기 500만/일 | 목록 한 쪽 ≤ 200행, 동기화 받기 한 쪽 ≤ 500행 + 문서 줄. 바뀐 것 없는 묻기 ≈ 2~3행. 장절 = 번역본 1행 + 부른 장 범위의 절만 |
| 행 쓰기 10만/일 | 문서 고치기 1번 = 문서 1 + 변경 기록 1 (+ 색인). 설치판 열쇠 확인은 30초 기억 (마지막 사용 시각 쓰기를 줄임) |
| DB 500 MB | 문서 1 MiB 까지. 변경 기록은 한 줄 ≈ 100 B (지금은 줄이지 않음 — 수십만 줄이 되면 오래된 것을 지우고 `reset` 을 켤 것). 성경은 허락 있는 번역본만 |

어림 (우리 교회): 주일 큐시트 · 찬양 고치기 하루 수백 번 → 쓰기 수천 행. 설치판이 1분마다 묻기 → 하루 1,440번 × 3행 ≈ 4,300행 읽기. 웹 화면에서 장절 수백 번 → 수만 행. 모두 무료 한도의 1% 안팎.

## 8. 2C · 3 작업자에게

- **2C (R2)** — 구현함, 계약은 [`assets-api.md`](assets-api.md). (처음 적은 것) `assets` 표(`church_id, id, sha256, name, type, size, r2_key, created_at, deleted_at`, 색인 `(church_id, sha256)`)만 만들어 두었다. 길 `POST /api/v1/assets` · `GET /api/v1/assets/{id}` 는 `cloud/src/assets/` 에서, 권한은 `can(role, 'write')` · `'read'` 를 그대로 쓰면 된다(`protocol.mjs` route 에 한 줄 + `index.mjs` 에 kind 하나). 문서 payload 안의 파일 주소 바꾸기는 2B.
- 첫 화면: `/` 와 로그인 답의 `home` 은 같은 표(`protocol.mjs HOME`) — member-editor → `/editor`, member-viewer → `/outputs/main`.
- **3 (웹 조작 화면)**: `Transport` 의 '문서 읽기 · 쓰기 · 구독' 을 위 2 · 3절로. 성경은 4절 `passage` (설치판과 같은 모양이라 슬라이드 나누기를 그대로 쓸 수 있음). 구독은 지금 `sync?after=` 묻기 — 실시간 알림(중계실 `{t:'doc'}` 같은 것)이 필요하면 덧붙이는 메시지로 정할 것.
