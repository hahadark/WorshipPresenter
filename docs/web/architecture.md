# 웹판 설계와 계약 (v1)

워십프리젠터 웹판: **설치판은 그대로 두고** GitHub(코드 · 시험 · 배포 작업 흐름) + Cloudflare(Worker · Durable Object · D1 · R2)로 웹을 더한다.
처음에는 우리 교회만 쓰지만 자료는 처음부터 **교회 단위**로 나눈다. 기준 코드 `origin/rust` 951f548 (2026-11-28).

뒤에 오는 작업자는 이 문서 + `cloud/src/protocol.mjs`(길 · 역할 · 메시지 · 한도)를 계약으로 읽는다. 둘을 같이 고친다.
기능별 웹 판단은 `docs/web/feature-matrix.md`.

## 1. 구성

```
 교회 PC (설치판 wp-app)                         Cloudflare (worshippresenter-web)
 ┌──────────────────────────┐   WSS /bridge   ┌──────────────────────────────────────────┐
 │ Core ─ Ev::Live/Draw/Ui ─┼───────────────▶ │ Worker (src/index.mjs)                    │
 │ cloud.rs (나가는 연결 하나)│ ◀── req/cmd ──── │  · 로그인 · 길 나누기 · 페이지에 어댑터     │
 │ 127.0.0.1:37528 서버 그대로│                 │  · 정적 파일 public/ (web/ 에서 빌드 때)    │
 └──────────────────────────┘                 │ ChurchRoom (Durable Object, 교회마다 하나)  │
                                               │  · 송출 주체 1 ↔ 보는 쪽 여럿, 마지막 화면 │
 OBS · 예배당 PC · 휴대폰 · 다른 PC 브라우저      │  · 리모컨/편집기/미디어 요청을 설치판으로   │
   /outputs/main · overlay · stage · out-…  ◀──┤ D1: 교회 · 기기 열쇠 · 틀린 횟수 (1단계)   │
   /remote · /editor · /owner                  │ R2: (2단계) 그림 · 영상 · 글꼴              │
                                               └──────────────────────────────────────────┘
```

- **설치판은 밖으로 나가는 WebSocket 하나만** 연다(포트 열기 · 터널 · 방화벽 설정 없음). 끄면(기본) 지금과 똑같다.
- **페이지는 복사하지 않는다.** `cloud/src/pages.mjs` 가 빌드 때 `web/output/index.html` · `stage.html` · `remote/index.html` · `editor/index.html` 을 글로 묶어 넣고, `<head>` 바로 뒤에 `window.WP_CLOUD={…}` 와 `/cloud/adapter.js` 만 끼운다. 정적 파일(`/shared/*` · `/outputs/output.js` · `/outputs/stage.js` · `/remote.js` · `/groups.js` · `/i18n*.js`)은 `npm run assets` 가 `web/` 에서 `public/` 으로 가져온다(Git 에 넣지 않음). 다른 작업자가 `web/` 을 고치면 다음 배포에 그대로 따라온다.
- **어댑터** (`cloud/client/adapter.js`, 클라우드에서만 끼워짐): ① `EventSource('/events?client=…')` → 같은 주소 WebSocket, 이벤트 이름 · 자료 모양(`render` · `draw` · `hello`)은 설치판 SSE 와 같음 ② `/api/ack` · 영상 알림 → 그 WebSocket 으로 ③ 리모컨의 1초 `fetch('/r/state')` → 실시간으로 받아 둔 상태로 답함(요청 수 0) ④ `BroadcastChannel('wp-live')` 로 같은 PC 의 웹 조작 화면이 보내는 화면도 받음(3 · 6단계) ⑤ 쓰기 요청에 `X-WorshipCue: 1` ⑥ 닫는 코드 4003 이면 로그인 화면으로.
- 설치판 쪽: 새 모듈 `crates/wp-core/src/cloud.rs`(연결 · 상태 보내기 · 요청 받기)와 `crates/wp-app/src/cloud_ui.rs`(연결 → 클라우드 연결 창). 기존 파일에는 모듈 등록 · 메뉴 한 줄 · 창 부르기 한 줄만 더한다.

## 2. 로그인과 교회 단위 분리

**역할** (`protocol.mjs ROLES`): `owner` 주인 · `bridge` 설치판 연결 · `output` 출력 화면 · OBS · `remote` 휴대폰 리모컨 · `editor` 웹 편집기.
**할 수 있는 일** (`can`): owner = 모두(view · remote · editor · manage · source), bridge = source, output = view, remote = remote, editor = editor.

- **기기 열쇠** (D1 `credentials`): 주인이 `/owner` 에서 기기마다 만든다. 출력 · 설치판 · 주인은 긴 무작위 열쇠(32자, 약 185비트), 리모컨 · 편집기는 사람이 치는 PIN 6자리. **원문은 저장하지 않고** `HMAC-SHA256(AUTH_SECRET, "cred:교회:역할:열쇠")` 만 둔다. 만들 때 한 번만 보여 준다. 지우면 그 열쇠로 붙은 연결을 바로 끊는다(닫는 코드 4003).
- **쿠키** `wp_session` = `v1.<{c 교회, r 역할, k 열쇠 id, e 만료}>.<HMAC>`, HttpOnly · SameSite=Lax · https 면 Secure. 요청마다 열쇠가 살아 있는지 D1 로 본다(Worker 인스턴스마다 30초 기억). 유지 시간: owner 30일 · output 400일(OBS 에 넣어 둠) · remote/editor 12시간. `wp_church` 쿠키는 교회 ID 만(PIN 칸이 어느 교회인지 알게).
- **로그인 길**: `/login`(교회 ID + 열쇠/PIN) · 출력 주소 `?key=교회.열쇠`(쿠키로 바꾸고 302 로 열쇠를 주소에서 지움 — OBS 가 쿠키를 잃어도 같은 주소로 다시 로그인) · 리모컨 페이지의 PIN 칸(`/r/state?pin=…&c=교회`) · 편집기 PIN 칸(`X-PIN`). 설치판은 `Authorization: Bearer 교회.열쇠` 로 `/bridge` 에만.
- **틀린 횟수**: 기기 주소(`cf-connecting-ip`)별 5번마다 잠금(30초부터 두 배씩, 최대 15분), 교회 전체 30번이면 15분. D1 `auth_failures`.
- **쓰기 보호**: 쿠키로 오는 쓰기 · 실시간 연결은 `Origin` 이 같고 `X-WorshipCue: 1`(또는 `Sec-Fetch-Site: same-origin`)일 때만. 새로 만든 페이지(로그인 · 주인)는 엄격한 CSP, 리모컨 · 편집기는 `X-Frame-Options: DENY`.
- **교회 단위 분리**: 교회는 쿠키 · 열쇠 앞머리에서만 정한다. 본문 · 쿼리의 교회 값을 믿지 않는다(PIN 로그인의 `c=` 는 '어느 교회의 PIN 인지' 만 정하고, 그 교회의 해시로만 맞춘다). 중계실은 `ROOMS.idFromName(교회)` — 교회마다 따로. D1 의 모든 표는 `church_id` 가 첫 열쇠이고 모든 질의는 `WHERE church_id = ?1` 로 시작한다. Worker 가 중계실에 넘길 때 `x-wp-*` 머리글을 지우고 확인한 값으로 다시 쓴다.
- **2단계 A 에서 한 것**: 사람 계정 `members`(owner · editor · viewer, 비밀번호 또는 Cloudflare Access) — 쿠키 역할 `owner` · `member-editor` · `member-viewer`, 자료 권한 `read` · `write` 를 더함. 기기 열쇠 · PIN 은 그대로 ([`data-api.md`](data-api.md) 1 · 5절).
- (처음 적은 것) **2단계에서 바꿀 것**: 주인 로그인을 사람 계정으로(선택지 — Cloudflare Access 무료 50명 · GitHub/Google OAuth). 기기 열쇠는 그대로 둔다(OBS · 휴대폰에는 계정이 없다). 교회 만들기는 지금 `POST /api/v1/admin/churches`(Worker 비밀값 `ADMIN_TOKEN` 이 있을 때만 열림) 또는 `scripts/seed.mjs`.

## 3. HTTP 계약 (1단계, 구현함)

응답은 모두 `Cache-Control: no-store`(정적 파일 빼고). 오류는 `{ok:false, error}` + 상태 코드. HTML 요청에 로그인이 없으면 `/login?next=…` 로 302, API 는 401.

| 길 | 방법 · 권한 | 내용 |
|---|---|---|
| `/health` | GET · 공개 | `{ok, protocol:1, configured}` — 자료 없음 |
| `/login` · `/auth/login` · `/auth/logout` | GET · POST | 로그인 화면 · `{church, secret}` → `{ok, protocol, role, home}` + 쿠키 · 나가기 |
| `/` | GET | 역할의 첫 화면으로 302 (owner `/owner`, output `/outputs/main`, remote `/remote`, editor `/editor`) |
| `/outputs/main` · `overlay` · `stage` · `out-…` | GET · view | 설치판과 **같은 페이지** + 어댑터. `?key=` 로그인 가능 |
| `/remote` · `/editor` | GET · remote/editor | 같은 페이지. 로그인 전에도 `?c=교회` 면 PIN 칸만 있는 껍데기(자료 없음) |
| `/owner` | GET · manage | 주인 화면 (연결 상태 · 기기 열쇠 · 3단계: 사람 · 교회 AI) |
| `/control/` | GET · read | 3단계 웹 조작 화면 (주인 · 편집자 · 보기만 · 둘러보기). 페이지만 Worker 가 보내고 wasm · js · 글꼴은 정적 파일 `/control/*` (파일 하나 25 MiB 아래) |
| `/api/v1/control/bible/book?version=&book=` | GET · read | 3단계: 번역본의 한 권 전체 `{verses:[[장,절,글]…], headings}` — 웹 조작 화면이 메모리 SQLite 에 넣는다 |
| `/events?client=main\|overlay\|stage\|out-…\|remote` | WebSocket · view (remote 는 remote) | 아래 '실시간' — 보는 쪽 |
| `/bridge` | WebSocket · Bearer 설치판 열쇠 (또는 owner 쿠키) | 아래 '실시간' — 송출 주체 |
| `/r/state` | GET · remote | 설치판 `remote::state_for` + `ok:true`. 중계실이 받아 둔 것으로 바로 답함 |
| `/r/do` | POST · remote | 설치판 `remote::act` 로 넘김. 본문 · 답 그대로 (`pin` 칸은 무시) |
| `/e/state` · `/e/cue?id=` · `/e/songs?q=` | GET · editor | 설치판 `webedit` 그대로 (`cues` · `cue` · `songs`) |
| `/e/save` · `/e/new` · `/e/song` | POST · editor | 그대로. 충돌이면 409 `{conflict:true}` 를 그대로 |
| `/media/{f}` · `/lib/file?p=` · `/hymn/{key}/{file}` · `/hymnal/score/{book}/{file}` | GET · view | 설치판이 자기 로컬 서버에서 읽어 이진 조각으로 보냄(Range 포함). 설치판 내부망 출력(`lan.rs allowed`)과 같은 범위 · 같은 폴더 경계 |
| `/api/ack` · `/api/command` | POST · view | 수신 확인 · `videoMeta`/`videoEnded` 만 (나머지 명령 403) |
| `/api/v1/me` | GET · 로그인 | `{church, churchName, role, exp, protocol}` |
| `/api/v1/room` | GET · manage | 중계실 상태 `{online, app, lang, outputs, caps, since, viewers[]}` |
| `/api/v1/credentials` · `/api/v1/credentials/{id}` | GET · POST · DELETE · manage | 기기 열쇠 목록 · 만들기(`{role,label}` → 원문 한 번) · 지우기 |

**클라우드에 열지 않는 것**: 설치판 로컬 서버의 `/api/state` · `/api/live` · 외부 연동 `/api/command` 전체 · `/api/trigger/*` · `/dock` · `/helper/*` · 휴대폰 카메라 길 · 파일 시스템 · 장비 포트. 설치판도 클라우드 요청 중 위 표의 길만 받는다(`cloud.rs` 가 한 번 더 거른다).

## 4. 실시간 계약 (v1)

글 프레임은 UTF-8 JSON, 종류는 `t`. 미디어 본문만 이진 프레임. 계약 버전은 `hello.v` · `ready.v` = `1` (`PROTOCOL`). 출력 자료 안의 `version` 은 **송출 버전**이라 다른 것이다.
버전 규칙: 모르는 `t` 와 모르는 칸은 버린다(덧붙이기는 버전을 안 올림). 뜻이 바뀌거나 칸을 지우면 `PROTOCOL` 을 올리고, 중계실은 다른 버전의 송출 주체를 4008 로 닫는다.

### 보는 쪽 (`/events`)

| 방향 | 메시지 | 뜻 |
|---|---|---|
| ↓ | `{t:'hello', v, id, online}` | 붙자마자. `id` 는 이 연결 번호(설치판 SSE `hello.id` 자리), `online` = 설치판이 붙어 있나 |
| ↓ | `{t:'render', data}` | 그 화면의 출력 자료. **설치판 `Core::output_payload(client)` / `stage_payload()` 그대로** (template · layers · overlays · scene · sources · video · audio …). 붙자마자 마지막 것 |
| ↓ | `{t:'draw', data}` | `draw_json()` 그대로 (무대 · 리모컨에는 안 감) |
| ↓ | `{t:'remote', data}` | `client=remote` 만: `remote::state_for()` 그대로 |
| ↓ | `{t:'status', online}` | 설치판이 붙었다 · 끊겼다. 화면은 마지막 것을 그대로 둔다 |
| ↑ | `{t:'ack', version, width, height, visible}` | 수신 확인 |
| ↑ | `{t:'cmd', cmd:'videoMeta'\|'videoEnded', args:{key, duration}}` | 영상 알림만 |
| ↕ | 글 `ping` → `pong` | 25초마다. 중계실을 깨우지 않고 답함(auto response). 60초 답이 없으면 끊고 다시 이음 |

### 송출 주체 (`/bridge`, 설치판 · 3단계의 웹 조작 화면)

| 방향 | 메시지 | 뜻 |
|---|---|---|
| ↓ | `{t:'ready', v, viewers, standby?, active?}` | 붙자마자. `standby:true` 면 대기로 붙은 것(아래 '송출 주체 겹침'), `active` = 지금 주체 `{source, app, since, claimed}` |
| ↑ | `{t:'hello', v:1, app, lang:'ko'\|'en', outputs:[…], caps:{remote, editor, media}}` | 첫 글. `caps` 로 리모컨 · 편집기 · 미디어 요청을 받을지 알림 |
| ↑ | `{t:'render', client, data}` | 화면 하나의 출력 자료. **붙으면 모든 화면(main · overlay · stage · 추가 출력)을 한 번씩**, 그 뒤로는 바뀐 화면만(같은 글이면 보내지 않음) |
| ↑ | `{t:'draw', data}` · `{t:'remote', data}` | 그리기 · 리모컨 상태 (바뀔 때만) |
| ↓ | `{t:'sync'}` | 중계실이 잠들었다 깨서 기억이 없을 때 '전부 다시' |
| ↓ | `{t:'viewers', list:[{id, client, kind, since, ack}]}` | 붙은 화면 목록 (모아서 0.3초마다) |
| ↓ | `{t:'cmd', cmd, args}` | 출력 화면의 영상 알림 |
| ↓ | `{t:'req', id, method, path, query, headers?, body}` | 리모컨 · 편집기 · 미디어 요청 |
| ↑ | `{t:'res', id, status, headers, body}` | JSON 답 |
| ↑ | `{t:'res', id, status, headers, stream:true}` + 이진 `[id u32 BE][표시 u8][본문]` … | 파일 답. 표시 1 = 끝, 2 = 중단. 조각은 256 KiB 이하 |
| ↓ | `{t:'cancel', id}` | 기다리던 쪽이 그만둠(시간 초과 · 끊김) |
| ↑ | `{t:'claim'}` | (3단계) 대기 중인 웹 조작 화면이 송출을 넘겨받는다. **주인 쿠키로 붙은 웹만** — 아니면 ↓ `{t:'denied', error}` |
| ↑ | `{t:'release'}` | (3단계) 넘겨받은 웹이 돌려준다 → 대기 중인 설치판이 주체 |
| ↓ | `{t:'standby', active}` · `{t:'active', claimed}` | (3단계) 대기로 내려감 · 주체가 됨(뒤이어 `sync`) |

- 요청 기다림: JSON 12초, 파일 조각 사이 30초. 넘으면 504 — **쓰기를 자동으로 다시 보내지 않는다**(리모컨 '다음' 이 두 번 되지 않게). 기다리는 요청 최대 64개.
- 송출 주체는 교회마다 하나. 새로 붙으면 앞의 것을 4001 로 닫는다(끊긴 줄 모르고 남은 옛 연결이 막지 않게). 화면에는 '끊김' 을 알리지 않는다.
- **송출 주체 겹침 (3단계, 덧붙임 — 구현 `room.mjs`, 시험 `test/e2e/claim.test.mjs` · `tools/web/cloud-e2e.js`)**: 설치판 열쇠(Bearer)로 붙은 것 = `desktop`, 주인 쿠키 = `web`.
  - 설치판이 **기본 주체**. 웹은 주체가 이미 있으면 **대기**(`standby`)로 붙는다 — 대기의 `render` · `draw` · `remote` · `res` 는 버리고(출력에 닿지 않음), `viewers` · `req` 도 보내지 않는다. 대기를 끊지 않는다.
  - 웹이 주체였는데 설치판이 붙으면(예배 전 웹으로 준비 → 설치판을 켬) 설치판이 주체, 웹은 대기로 내려간다(끊지 않음, ↓ `standby`).
  - 주인이 웹에서 **[넘겨받기]** 를 누르면(`claim`) 설치판은 4001 로 닫히고 웹이 주체(`claimed`). 넘겨받은 동안 다시 붙는 설치판(자동 재연결)은 대기 — 웹을 밀어내지 않는다. `release` 또는 넘겨받은 웹이 떠나면 대기 중인 설치판이 저절로 주체. **대기 중인 웹은 저절로 주체가 되지 않는다.**
  - 설치판은 이 메시지를 몰라도 된다(모르는 `t` 는 버림): 대기일 때 보낸 것이 버려질 뿐이다. `/api/v1/room` 에 `source`('desktop'|'web') · `claimed` · `standby[]` 를 덧붙였다.
- 중계실은 예배 자료를 **디스크에 쓰지 않는다**(메모리만). 잠들었다 깨면 `sync` 로 다시 받는다 → DO 저장 쓰기 0.
- 한도(`LIMITS`): 글 프레임 4 MiB, 요청 본문 2 MiB, 화면 200개, 작은 파일 기억 하나 2 MiB · 전체 32 MiB.
- 닫는 코드: 4001 바뀜 · 4003 열쇠 지움/만료 · 4008 계약 위반.

### 설치판이 지킬 것 (`cloud.rs`)

1. 기본 꺼짐. 설정 `cloud {enabled, url, keyEnc, remote, editor}` (열쇠는 DPAPI · 키체인 암호화, 화면 · 로그 · `/api/state` 에 내보내지 않음).
2. 다른 스레드에서 돈다. 끊기면 1 → 2 → … 30초(흔들림) 간격으로 다시 잇고, 조작 화면을 막지 않는다. 끊겨도 로컬 출력 · 리모컨 · 내부망은 그대로.
3. 이으면 `hello` → 전체(모든 화면 `render` + `draw` + `remote`). `Ev::Live` 마다 바뀐 화면만, `Ev::Ui` 마다 리모컨 상태가 바뀌었으면.
4. `req` 는 표 3의 길만. `/r/*` 는 `remote::state_for` · `remote::act`, `/e/*` 는 `webedit` 함수, 미디어는 **자기 로컬 서버(127.0.0.1)** 에서 읽어 그대로(폴더 경계 · Range · SVG 처리를 다시 짜지 않음). 설정의 `cloud.remote` · `cloud.editor` 가 꺼져 있으면 403.

## 5. 3단계(웹 조작 화면)가 이 계약을 쓰는 법 — `web-egui` 작업자용

- 브라우저의 조작 화면 = **또 하나의 송출 주체**. owner 쿠키로 `/bridge` 에 붙고 설치판과 같은 메시지(`hello` · `render` · `draw` · `remote` · `res`)를 보낸다. 출력 · 리모컨 · 편집기 · OBS 쪽은 한 줄도 바뀌지 않는다. (구현함: `crates/wp-app/src/web/cloud.rs`. `hello.source:'web'`, `caps` 는 모두 false — 리모컨 · 편집기 · 미디어 요청은 설치판 몫이라 받지 않고 `req` 에는 501)
- **여는 곳**: `/control/` (로그인 · read 권한, 없으면 `/login?next=/control/`). 송출 주체로는 주인(`source`)만 붙고, 편집자 · 보기만은 자료만. 둘러보기(`/demo` 의 demo 역할)도 같은 화면 — 방문자마다 따로인 중계실. 파일은 `tools/web/build.sh` → `dist-web/` → `cloud/scripts/assets.mjs` 가 `public/control/` 로 (wasm 18.9 MB < 25 MiB · 글꼴은 따로).
- **같은 PC 출력**: 조작 화면이 `BroadcastChannel('wp-live')` 로 `{t:'render', client, data, church}` · `{t:'draw', data, church}` 를 보내면, 같은 브라우저의 클라우드 출력 탭이 인터넷 없이도 받는다(어댑터에 구현함). 새 출력 탭은 `{t:'sync', church}` 를 보낸다 → 조작 화면이 전체를 다시 보냄. 출력 탭은 사용자가 눌러 연다(팝업 막힘).
- **같은 PC 출력 → 조작 화면 (3단계 덧붙임)**: 출력 탭은 2초마다 `{t:'out', id, client, kind, cloud, ack?, church}`(살아 있음 · 수신 확인), 닫힐 때 `{t:'bye', id, church}`, 클라우드가 끊겼을 때 영상 알림 `{t:'cmd', cmd:'videoMeta'|'videoEnded', args, church}` 를 같은 채널로 보낸다 → 인터넷 없이도 '연결된 출력' 수와 영상 끝 자동 넘김이 맞는다. `cloud:true`(클라우드로도 붙은 탭)는 조작 화면이 클라우드 송출 중일 때 세지 않는다(중계실 `viewers` 로 센다). 구현: `cloud/client/adapter.js` · `web/app/local-live.js`(클라우드 없이 띄울 때).
- **자료**: 1단계에서는 설치판이 원본이라 웹 조작 화면은 `/e/*` · `/r/*` 로만 고칠 수 있다(설치판이 켜져 있어야 함). 2단계 `/api/v1/documents` 가 생기면 그것을 쓴다(아래 6). 지금 만들 때는 `Transport` 한 겹(문서 읽기 · 쓰기 · 구독 · 송출 · 명령)을 두고 1단계 길 · 2단계 길을 바꿔 끼운다.
  - (구현함, `web/store.rs` · `web/bible.rs`) 로그인하면 큐시트 · 찬양을 `/api/v1/documents?include=payload` 로 받고, 고친 문서를 `PUT` + `If-Match` + `X-Mutation-Id` 로 올리고(1.5초 모아서), 20초마다 `/api/v1/sync?after=` 로 받는다(`more` 를 끝까지). 409 충돌은 덮어쓰지 않는다 — 최신 것을 들이고 이 화면의 것은 `(내 사본)` 새 문서로. 성경은 `/api/v1/bible/versions` + 책마다 `/api/v1/control/bible/book`, IndexedDB 에 보관(번역본 `importedAt` · 절 수가 바뀔 때만 다시). 로그인 없이 내 PC 에서 띄우면 localStorage + 견본(`web/app/sample/seed.json`).
- **송출 주체 겹침**: 4절 '송출 주체 겹침' 에 정한 대로 구현함(대기 · `claim` · `release`). 웹 조작 화면은 위쪽 가운데 띠에 상태(클라우드 송출 · 보기 전용)와 [넘겨받기…](확인 창) · [설치판에 돌려주기] · [교회 관리] 를 보인다.
- 브라우저에서 쓰지 못하는 것: OS 파일 경로 · SQLite 파일 · TCP/UDP(장비) · 전역 단축키. (한글 IME 는 된다 — 실제 Windows 한국어 입력기로 Edge · Chrome 15/15, `docs/web/egui-web.md`) https 페이지에서 `http://127.0.0.1:37528` 을 부르는 것은 섞인 내용 · 로컬 네트워크 접근 허락에 걸려 기본 길로 삼지 않는다.

## 6. 2단계 자료 계약 (2A 구현함 — 쓰는 법 · 예시 · 덧붙인 칸은 [`data-api.md`](data-api.md), 자산 길은 2C)

> 2B 설치판 동기화(구현함): [`sync.md`](sync.md) — 설치판 문서 모양 · 충돌 사본 규칙 · 파일 주소 바꾸기(`/api/v1/assets/{id}?e=` · `wp-local:` · 출력 `?fallback=`).

D1 (교회 단위, 모든 질의 `church_id = ?1`):

```sql
documents (church_id, kind, id, payload TEXT, revision INTEGER, updated_at, updated_by, deleted_at, PRIMARY KEY (church_id, kind, id))
  -- kind: cue · song · template · theme · overlay · macro · scene · source · settings(교회 공통) · usage
changes   (church_id, seq INTEGER, kind, id, revision, op 'put'|'delete', at, by, mutation_id, PRIMARY KEY (church_id, seq))
bible_versions (church_id, id, name, license_note, verses INTEGER, PRIMARY KEY (church_id, id))
bible_verses   (church_id, version, book, chapter, verse, text, PRIMARY KEY (church_id, version, book, chapter, verse))
assets    (church_id, id, sha256, name, type, size, r2_key, created_at, deleted_at, PRIMARY KEY (church_id, id))
members   (church_id, subject, role, created_at, PRIMARY KEY (church_id, subject))   -- 사람 계정 (2단계 로그인)
```

- `payload` 는 설치판 JSON 모양 그대로(큐시트 `{id, name, serviceDate, items[]}` 등). 고칠 때 `If-Match: revision` — 다르면 409 + 최신. 자동으로 덮어쓰지 않는다(웹 편집기 충돌 규칙과 같음).
- 길(예약): `GET/PUT/DELETE /api/v1/documents/{kind}/{id}`, `GET /api/v1/documents?kind=&cursor=`, `GET /api/v1/sync?after=seq` · `POST /api/v1/sync`(mutationId 로 두 번 안 들어가게), `GET /api/v1/bible/versions` · `/api/v1/bible/passage?version=&ref=`(로그인 뒤 · 교회 것만), `POST /api/v1/assets` · `GET /api/v1/assets/{id}`(Range).
- 성경 본문은 교회가 허락을 가진 번역본만. D1 무료 DB 하나는 500 MB 까지 — 번역본을 다 넣지 않는다.
- R2 열쇠: `churches/{교회}/assets/{sha256}/{안전한 파일 이름}`. 버킷은 공개하지 않고 Worker 가 로그인을 본 뒤 내보낸다. PC 경로 · 사용자 이름은 열쇠에 넣지 않는다. 출력 자료 안의 `/lib/file?p=C:\…` 주소는 2단계에서 `/api/v1/assets/{id}` 로 바꿔 보낸다(설치판 쪽에서 주소를 바꾸는 한 겹).
- **2C 구현함** ([`assets-api.md`](assets-api.md)): 비공개 저장통 `worshippresenter-web-assets`(`env.FILES`) · 한 번에 올리기(90 MiB, R2 가 sha256 확인) · 나눠 올리기(32 MiB 조각) · Range 받기(출력 · PIN 기기도 id 로) · 지우기는 표시만 + 주인의 정리 명령 · `fonts.css`(이름 · 굵기를 따로 적음, 로그인한 페이지 머리에 저절로) · 무료 범위 80% 에서 올리기 507 `quota`. 설치판 중계와 고르는 규칙은 그 문서 5절.

## 7. 6단계(오프라인)가 이 계약을 쓰는 법

> **구현함 — 쓰는 법 · 저장 모양 · 시험: [`offline.md`](offline.md)** (서비스 워커 `/sw.js` 판별 저장 · IndexedDB `wp-offline` 올릴 줄 · `POST /api/v1/sync` 20개씩 · 충돌 사본 · '연결 끊김 · 올릴 것 N개' · '새 버전 있음'). 계약은 바꾸지 않았다 — 기존 길만 쓴다.

- 설치판은 이미 오프라인에 강하다: 인터넷이 끊겨도 같은 PC 의 예배당 창 · OBS(127.0.0.1) · 내부망 출력은 그대로 돌고, `cloud.rs` 는 뒤에서 다시 잇기만 한다. 다시 이으면 **전체 상태부터** 보내므로 맞추기가 따로 필요 없다.
- 웹 조작 화면: Service Worker 로 앱 껍데기를 버전별로 저장, IndexedDB 에 그 교회 문서 · 변경 줄 · 그림. 송출은 같은 PC 의 `BroadcastChannel('wp-live')` 로 계속. 다시 이으면 `GET /api/v1/sync?after=` 로 받고 `POST /api/v1/sync` 로 올림. 충돌은 사본을 만들고 사람이 고름.
- 다음 · 이전 같은 **순간 조작은 쌓아 두었다 나중에 보내지 않는다**.
- 클라우드 출력 화면은 끊겨도 마지막 화면을 유지하고, 다시 이으면 지금 것을 받는다(1단계 구현 · 시험함).

## 8. 비용 — Cloudflare 무료 범위 (2026-11-28 공식 문서로 확인)

| 제품 | 무료 범위 · 주의 | 문서 |
|---|---|---|
| Workers | 요청 10만/일, CPU 10 ms/요청, 메모리 128 MB. **정적 파일 요청은 무료 · 무제한**. WebSocket 은 처음 연결 한 번만 요청으로 셈, Worker 를 지나는 메시지는 요청이 아님. 정적 파일 2만 개 · 파일 하나 25 MiB | https://developers.cloudflare.com/workers/platform/pricing/ · https://developers.cloudflare.com/workers/platform/limits/ |
| Durable Objects | 무료 플랜은 **SQLite 저장 방식만**. 요청 10만/일, 실행 13,000 GB-s/일. 들어오는 WebSocket 메시지 20개 = 요청 1, **나가는 메시지는 무료**, 잠자는(hibernation) 동안 실행 시간 없음. 받는 메시지 최대 32 MiB. 잠자기 API 연결 최대 32,768/객체 | https://developers.cloudflare.com/durable-objects/platform/pricing/ · https://developers.cloudflare.com/durable-objects/platform/limits/ · https://developers.cloudflare.com/durable-objects/api/state/ |
| D1 | 행 읽기 500만/일, 쓰기 10만/일, 전체 5 GB, **DB 하나 500 MB**, 계정에 DB 10개, 요청 하나에 질의 50개. 넘으면 다음 날(UTC 0시)까지 오류 | https://developers.cloudflare.com/d1/platform/pricing/ · https://developers.cloudflare.com/d1/platform/limits/ |
| R2 | 저장 10 GB-월, A 작업 100만/월, B 작업 1,000만/월, 내보내기 무료(Standard 만). **R2 를 켜려면 '구독' 결제 절차가 있다** — 2단계 전에 사관님이 확인 | https://developers.cloudflare.com/r2/pricing/ · https://developers.cloudflare.com/r2/get-started/ |

우리 교회 1단계 어림 (예배 3시간, 출력 화면 4 · 리모컨 2 · 편집기 1):
- Worker 요청: 페이지 · 로그인 · 연결 · 미디어 몇백 번. 출력은 WebSocket 하나라 시간에 비례하지 않는다. 리모컨 1초 묻기는 어댑터가 실시간 상태로 답해 **0**. 주인 화면은 15초마다 묻는다(숨긴 탭은 안 물음).
- DO: 들어오는 메시지(설치판 render · ack · ping)는 20개 = 요청 1. 송출 버전이 하루 수백~수천 번 바뀌어도 요청 수백 번 수준. 미디어 중계는 256 KiB 조각마다 메시지 1 → 100 MB 영상 ≈ 400 메시지 ≈ 요청 20. 같은 작은 파일은 중계실이 기억(최대 32 MiB).
- 실행 시간: 보는 쪽 · 설치판이 계속 붙어 있으면 객체가 깨어 있는 시간이 생긴다(ping 은 auto response 라 깨우지 않음). 128 MB 로 하루 종일 깨어 있어도 ≈ 10,800 GB-s < 13,000. 다른 교회를 받기 시작하면(7단계) 계정 전체 합으로 다시 따진다.
- D1: 로그인 · 열쇠 확인(30초 기억) 정도 — 하루 수천 행.
- **주의**: 무료 한도는 계정 전체 합이다. 이 계정에는 이미 다른 Worker · D1(7개)이 있다. 한도를 넘으면 무료 플랜은 멈추고(과금 아님), 유료 플랜 계정이면 넘은 만큼 과금될 수 있다 → 배포 전 계정 요금제를 확인.

## 9. 2~6단계 작업 묶음 (서로 겹치지 않게)

| 가지 | 맡을 것 | 고칠 폴더 (그 밖은 건드리지 않음) | 기대는 것 |
|---|---|---|---|
| `web-data` (2A) | D1 문서 · 변경 기록 · 성경 번역본 API, 사람 계정 로그인(주인) | `cloud/src/data/` · `cloud/src/identity/` · `cloud/migrations/0002_*`~ · `cloud/test/data*` | 이 계약 |
| `web-sync` (2B) | 설치판 ↔ D1 동기화(큐시트 · 찬양 · 템플릿 · 설정), 출력 자료의 파일 주소 바꾸기 | `crates/wp-core/src/cloudsync.rs`(새) · `cloud/src/sync/` | 2A 의 문서 API |
| `web-assets` (2C) | R2 비공개 저장 · 올리기(해시) · Range 내보내기, 글꼴 | `cloud/src/assets/` | 2A 권한, **R2 켜기(사관님)** |
| `web-egui` (3) | 웹 조작 화면 (지금 진행 중) | 새 WASM 크레이트 · `cloud/src/control/` · `cloud/client/control/` | 1단계 `/bridge`, 뒤에 2A |
| `web-scenes` (4A) | 화면 구성 편집 · 출력별 구성 · 전환 (웹) | 3단계 화면 안의 새 모듈 | 3 |
| `web-media` (4B) | 카메라(이름으로) · 휴대폰 카메라 신호(DO) · PDF(pdf.js) · 영상 · 유튜브 — **구현함: `docs/web/media.md`** | `cloud/src/media/` · `cloud/client/media/` | 2C, TURN 은 켜지 않음(비용 승인) |
| `web-ai` (5) | AI 길(서버 쪽 키 보관 · 동의) · 음성 인식 선택 — **클라우드 쪽 구현함: `docs/web/ai.md`**. 변환(HWP/PPTX)은 wasm 에서 그대로 돌아 따로 만들 것 없음 | `cloud/src/ai/` · `migrations/0003_ai.sql` | 2A · 2C, 유료 모델 자동 호출 금지 |
| `web-offline` (6) | Service Worker · IndexedDB · 충돌 화면 · 끊김 시험 — **구현함: `docs/web/offline.md`** | `cloud/client/offline/` · `cloud/src/sync/` 의 받기 쪽 | 2A · 2B · 3 |
| `web-tiers` (7 일부) | 계정 등급(데모 · 무료 · 유료) · 등급별 저장소(무료 = 교회의 구글 드라이브) · 둘러보기 · 옮기기 — **구현함: `docs/web/tiers.md`** (12절) | `cloud/src/tiers/` · `cloud/src/drive/` · `cloud/src/data/bibleimport.mjs` · `migrations/0005_tiers.sql` | 2A · 2C, 구글 준비(사관님) |

- 순서: 2A → (2B · 2C · 3) 나란히 → (4A · 4B · 5) 나란히 → 6 이 마지막에 모두 합쳐 시험.
- **공통 파일**(`cloud/src/index.mjs` 의 길 등록 · `protocol.mjs` · 이 문서 · `wrangler.jsonc`)은 각자 한두 줄 덧붙이는 것만 하고, 합칠 때 조율자가 한 번에 맞춘다. 길은 `route()` 표에 줄을 더하는 식으로.
- 설치판(`crates/`) 은 다른 작업자가 크게 고치는 중이므로 새 모듈에 두고, 기존 파일은 등록 한 줄씩만.

## 10. 로컬 개발 · 시험 · 배포

| 할 일 | 명령 (`cloud/` 에서) |
|---|---|
| 준비 | `npm ci` |
| 견본 교회 · 열쇠 만들기 (로컬 D1) | `npm run seed` → `.secrets/sample.json` (Git 제외) |
| 로컬 클라우드 | `npm run dev` (127.0.0.1:8787, 로컬 D1 · DO) |
| 단위 시험 | `npm test` |
| 끝에서 끝 시험 (빈 포트에 wrangler dev 를 띄워) | `npm run test:e2e` |
| 설치판 + 로컬 클라우드 화면 그림 | `node scripts/desktop-e2e.mjs --exe ..\target\debug\wp-app.exe` (복사본 자료 · 다른 포트) |
| 배포 점검(올리지 않음) | `npm run check` |
| 지운 파일 정리 · R2 사용량 (2C) | `node scripts/assets-purge.mjs [--usage] [--days 30 --yes]` |
| 실제 R2 확인 (Worker 는 로컬, 배포 없음, 견본 34 MB 를 올렸다 지움) | `node scripts/r2-remote-check.mjs` |
| 배포 | `node scripts/deploy.mjs` (비밀값 · D1 id 가 있을 때만, 아래) |

- GitHub Actions `.github/workflows/web-cloud.yml`: `web` · `web-*` 가지에 푸시하면 `npm ci` · 단위 시험 · 끝에서 끝 시험 · 배포 점검. 배포는 손으로(`workflow_dispatch`) 돌리고, 저장소 비밀값 `CLOUDFLARE_API_TOKEN` · `CLOUDFLARE_ACCOUNT_ID` · `WP_AUTH_SECRET` · `WP_D1_DATABASE_ID` 가 다 있을 때만 올린다(없으면 건너뜀). 비밀값을 출력하지 않는다.
- 비밀값: `AUTH_SECRET`(32자 이상, 없으면 로그인 전체가 닫힘) · `ADMIN_TOKEN`(선택, 없으면 교회 만들기 길이 닫힘). Worker 비밀값으로만 둔다.

## 11. 1단계 구현 상태 (2026-11-28)

**된 것**
- 클라우드: `/outputs/main · overlay · stage · out-…` · `/remote` · `/editor` · `/owner` · 로그인 · 기기 열쇠 · 실시간 중계 · 리모컨 · 편집기 되돌림 · 미디어 중계(이진 조각) · 열쇠 지우면 바로 끊기 · 틀린 횟수 잠금(PIN 만 교회 전체).
- 설치판: `crates/wp-core/src/cloud.rs`(기본 꺼짐, 스레드 하나, wss · ws, 다시 잇기, 바뀐 것만 보내기, 타이머만 도는 것은 다시 보내지 않음) · `crates/wp-app/src/cloud_ui.rs`(클라우드 연결 창). 창은 **메뉴 연결 → 클라우드 연결**(`d:cloud`, 안 A 표 `docs/menu-2026-11/plan-a.json` 과 메뉴 시험의 기대 목록에 한 줄 더함), 휴대폰 리모컨 · 웹 편집기 창의 '클라우드 연결…' 단추, 점검 인자 `--tool cloud` 로 연다.
- 기존 파일에 더한 것: `wp-core/src/lib.rs`(모듈 한 줄) · `server.rs`(`cloud::init` 한 줄) · `Cargo.toml`(`webpki-roots`, 이미 ureq 가 쓰는 판) · `wp-app/src/main.rs`(모듈 한 줄) · `tools_ui.rs`(창 부르기 한 줄 · 단추 두 줄) · `data/en.json`(맨 앞에 38줄).

**확인한 것** (증거는 `cloud/artifacts/`, Git 제외 — 다시 만들려면 아래 명령)
- `npm test` 단위 9 · `npm run test:e2e` 끝에서 끝 11 (가짜 설치판: 여러 화면 동시 · 끊김/다시 이음 · 로그인 없이 · 권한 · 열쇠 지우기 · 잠금 · 다른 역할 쿠키에서 다시 로그인).
- 같은 끝에서 끝 9개를 **배포한 미리보기(진짜 Cloudflare)** 에 대고도 통과 (잠금 시험 2개는 진짜 Cloudflare 에서 주소를 바꿀 수 없어 뺌).
- `node scripts/desktop-e2e.mjs` — 실제 설치판(격리한 자료 · 다른 포트) + 로컬 클라우드 21/21, **+ 배포한 미리보기(wss) 21/21**: 클라우드 편집기로 큐시트 만들기 · 충돌 409 · 설치판 끄기(끊김 알림 · 리모컨 503) · 다시 켜기(다시 이음) · 클라우드 리모컨 PIN 으로 송출 · 다음 · 무대 메시지 → 클라우드 예배당 · OBS 자막 · 무대 화면과 로컬 예배당 화면이 같은 슬라이드(그림 1-*, 2-*), 리모컨 · 주인 화면 · 로그인 화면 · 설치판 창 그림.
- `cargo test -p wp-core` 377 (새 cloud 5) · `cargo test -p wp-app` 264 통과. (wp-app `auditc_tests::quick_text_long_text…` 가 한 번 시험용 임시 DB 열기에서 실패, 혼자 · 다시 돌리면 통과 — 병렬 흔들림.)

**확인 못 한 것 · 남은 것**
- 실제 OBS 브라우저 소스가 쿠키를 오래 지키는지(못 지키면 `?key=` 주소가 매번 다시 로그인하므로 화면은 나옴). 큰 영상을 클라우드로 탐색 재생(Range 는 로컬 서버 그대로 중계, 단위로만 확인). 교회 인터넷 업로드 속도에서의 영상.
- 영어 화면: 클라우드 페이지에 i18n 스크립트를 아직 안 끼움(설치판 `hello.lang` 은 옴).
- 클라우드 화면 수: 브라우저를 닫아도 로컬 workerd 는 TCP 를 늦게 끊어 수가 잠시 남는다(진짜 Cloudflare 에서도 몇 초 남을 수 있음).
- 같은 교회에 설치판 둘이 같은 열쇠로 붙으면 나중 것이 이기고 앞의 것은 1분마다 다시 시도한다 → 예배 PC 하나에만 넣을 것(주인 화면에 경고 글 필요).
- parity(`tools\parity\run.ps1`)는 돌리지 않았다: 그리는 코드(`paint.rs` · `web/`)를 고치지 않았고 `tmp\data` 점검 자료가 필요하다. 릴리스 전에는 규칙대로 돌릴 것.

**미리보기 배포** (견본 자료만, 로그인 뒤에서만)
- 주소: https://worshippresenter-web.hahadark-media.workers.dev (Worker `worshippresenter-web` · D1 `worshippresenter-web` 새로 만듦, 다른 Worker · D1 은 건드리지 않음).
- 들어가는 법: `/login` 에 교회 ID `sample` + 주인 열쇠. 열쇠 · 비밀값은 이 PC 의 `cloud/.secrets/sample.remote.json` · `auth.remote.json` 에만 있다(Git 제외). `ADMIN_TOKEN` 은 올리지 않아 교회 만들기 길은 닫혀 있다.
- 지우려면: `npx wrangler delete worshippresenter-web` · `npx wrangler d1 delete worshippresenter-web`.

## 12. 계정 등급과 등급별 저장소 (7단계 일부, 구현함 — 자세히는 [`tiers.md`](tiers.md), 구글 준비는 [`google-setup.md`](google-setup.md))

- **길은 하나, 저장소만 교회마다.** 설치판 · 웹 조작 화면 · 출력 · 리모컨은 지금처럼 `/api/v1/documents` · `sync` · `bible/*` · `assets/*` 만 부른다. Worker 가 `churches.tier` 를 보고 뒤에서 고른다: **유료** = D1 · R2(2A · 2C 그대로) · **무료** = 그 교회의 구글 드라이브(D1 에는 계정 · 등급 · 봉인한 토큰 · 폴더 id · 판 번호 · 변경 순번 · 파일 목록만) · **데모** = D1 의 견본 교회(하루마다 되돌림). 답 모양 · 409 충돌 · seq · mutationId 가 같다.
- 요청마다: 로그인 → 권한(`can`) → **등급 확인**(`tiers.prepare`: 이 등급에 있는 기능인가 · 한도 안인가 · 옮기는 중이면 쓰기 423) → 저장소. 새 오류는 `code` 로만(`drive_disconnected` 503 · `drive_quota` 507 · `drive_rate` 429 · `drive_missing` 410 · `drive_busy` 409 · `tier_blocked` 403 · `tier_limit` 429/413 · `tier_moving` 423 — tiers.md 5절).
- 한도표는 `cloud/src/tiers/limits.mjs` 한 곳(값은 안 — 사관님이 정함). 화면 · 리모컨 수는 중계실이 `x-wp-max-*` 머리글로 받아 센다.
- **무료 가입** = '구글로 시작하기'(`/auth/google/start?mode=signup`, 범위 `drive.file` + `openid email` — 구글의 비민감 범위라 보안 심사 없음). 드라이브 안은 `워십프리젠터/<교회 이름>/큐시트/…json · 성경/<번역본>/43 요한복음.json · 파일/그림/…`. 드라이브에는 조건부 쓰기가 없어 **판 번호는 D1 이 정하고**(잡기 → 드라이브 쓰기 → 마무리, 끊기면 다음 요청이 마무리 · 풀기), 드라이브 파일의 `version` 은 적어만 둔다.
- **연결이 끊기면** 자료 길만 503 + 다시 연결 주소. 중계실 · 출력은 저장소를 보지 않으므로 **송출 중인 화면은 마지막 그대로**.
- **둘러보기** `/demo` (Worker 변수 `DEMO_ENABLED=1` 일 때만, 기본 꺼짐): 로그인 없이 2시간 데모 쿠키(역할 `demo`), 같이 보는 견본 교회, 실시간 중계실은 방문자마다 따로(`demo~d:…`), 파일 · AI · 초대 · 기기 열쇠 막음, 주소별 요청 한도.
- **옮기기**(무료 ↔ 유료)는 관리자 길(`ADMIN_TOKEN`, `scripts/tier-move.mjs`) — 결제는 없다. 두 저장소가 `changes` 표를 같이 쓰고 revision 을 그대로 옮겨 동기화가 끊기지 않는다.
- 시험: 단위 `test/tiers.test.mjs` · 끝에서 끝 `test/tiers-e2e/`(가짜 구글 `test/fake-google/server.mjs` — 진짜 구글은 부르지 않음). `npm run test:e2e` 가 가짜 구글을 띄우고 `--var GOOGLE_FAKE` 등으로 로컬 Worker 에 붙인다.
- 새 Worker 비밀값 · 변수: `GOOGLE_CLIENT_ID` · `GOOGLE_CLIENT_SECRET`(없으면 구글 길 404) · (선택) `GOOGLE_REDIRECT_URI` · 시험용 `GOOGLE_FAKE`(로컬 주소만) · `TIER_CACHE_MS` · `TIER_OVERRIDES` · `SERVICE_OVERRIDES`. 배포 스크립트(`deploy.mjs`)는 아직 구글 비밀값을 넣지 않는다 — 사관님이 클라이언트를 만든 뒤 `wrangler secret put` (google-setup.md).
