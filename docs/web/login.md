# 웹판 쉬운 로그인 (계약 v1, 구현함 — 2026-11-28)

사관님 뜻: "어디서든 주소만 있으면 바로". 교회 ID · 긴 열쇠 없이 **구글** 또는 **이름 · 비밀번호**로 들어간다.
코드: `cloud/src/identity/login.mjs` (새) · `identity/members.mjs` · `identity/index.mjs` · `drive/oauth.mjs`(구글 로그인 마무리만 넘김) · `client/login.*` · `client/owner-login.js` · `migrations/0006_login.sql`.
시험: `cloud/test/login.test.mjs`(단위 4) · `cloud/test/login-e2e/login.test.mjs`(끝에서 끝 5, 가짜 구글).

## 로그인 화면 (`/login`, 휴대폰 한 화면)

1. **구글로 로그인** (큰 단추) → `/auth/google/start?mode=login[&remember=0][&next=][&invite=]` → 구글 → 들어갈 교회가
   - 하나면 바로 그 역할의 첫 화면(또는 `next`),
   - 여럿이면 `/login?pick=1` 에서 고르기,
   - 없으면 `/login?google=no-account` ('주인에게 이 이메일로 초대를 부탁하거나 새 교회를 무료로').
   들어갈 교회 = 그 구글 번호로 된 사람 계정 + **그 구글 계정의 확인된 이메일로 온 초대**(받는 순간 사람 계정으로 바뀜) + 받은 초대 링크.
2. **이름 · 비밀번호** → `POST /auth/password {login, password, remember, next}` — 교회 ID 없이. 같은 이름이 여러 교회에 있으면 비밀번호가 맞는 교회만, 그래도 여럿이면 고르기.
3. **이 기기 기억** (기본 켬): 켜면 90일 쿠키 · 하루 넘게 쓰면 다시 90일(`maybeRenew`). 끄면 브라우저를 닫으면 사라지는 쿠키 + 서버에서도 12시간.
4. **고급: 교회 ID · 기기 열쇠 · PIN** (접혀 있음): 예전 `POST /auth/login` · `POST /auth/member` 그대로 (설치판 · OBS · 시험이 씀).
5. 새 교회 무료 시작(7단계) · 둘러보기 링크는 그대로 (둘러보기는 `DEMO_ENABLED=1` 일 때만).

출력 화면(OBS 열쇠 주소) · 리모컨 PIN · 설치판 연결 열쇠는 바뀌지 않았다 — 사람 로그인과 별개.
`/control` · `/remote` · `/editor` · `/owner` 는 같은 쿠키를 쓰고, 로그인 없이 열면 `/login?next=<그 주소>` → 로그인 뒤 그 주소로.

## 길

| 길 | 누가 | 내용 |
|---|---|---|
| `POST /auth/password` | 누구나 | 위 2. 틀리면 401 같은 글(없는 이름도), 주소별 5번 · 이름별 10번마다 잠시 막음(429) |
| `GET /auth/pick` · `POST /auth/pick {church}` | 고르기 쿠키 | `wp_pick` (서명 · 10분 · `Path=/auth/pick` · HttpOnly). 고르는 순간 그 사람 계정이 아직 같은지(세대) 다시 봄 |
| `GET /auth/invite/<열쇠>` | 누구나 | `/login?invite=<열쇠>` 로 302 |
| `GET /auth/invite?t=<열쇠>` | 누구나 | `{church, role, email(가림), expiresAt}` · 없거나 끝났으면 404 |
| `GET /api/v1/sessions` | 주인 | 로그인한 기기 `{id, who, role, device('크롬 · 안드로이드'), via, remember, createdAt, lastSeenAt, current}` |
| `DELETE /api/v1/sessions/{id}` | 주인 | 그 기기 로그아웃시키기 — 쿠키가 바로 죽고 그 기기로 붙은 실시간 연결도 끊음 |
| `GET/POST /api/v1/invites` · `DELETE /api/v1/invites/{id}` | 주인 | `{email?, role, link?}` → 이메일 초대(30일) · 링크 초대(7일, 원문은 이때 한 번만, 서버엔 SHA-256) |
| `POST /auth/logout` | 로그인 | 쿠키를 지우고 이 기기 세션도 서버에서 끝냄 |

## 세션 · 보안

- 사람 계정 쿠키의 열쇠 칸 = `m:<사람>:<세대>:<세션>` (`member_sessions` 한 줄). 예전 `m:<사람>:<세대>` 쿠키도 그대로 읽힌다.
  역할 · 비밀번호를 바꾸거나 사람을 지우면 세대가 올라 그 사람의 모든 기기가 끝나고(예전 규칙), 기기 하나만 끝낼 때는 세션 줄을 끊는다.
- 쿠키: `HttpOnly` · `SameSite=Lax` · https 면 `Secure`. 쓰기 요청은 같은 Origin + `X-WorshipCue: 1`(예전 규칙). 구글은 state + PKCE(S256) 그대로.
- 구글 이메일은 `email_verified` 인 것만 초대에 맞춘다. 이메일이 적힌 링크 초대는 그 이메일의 구글 계정만 받는다. 초대는 한 번 쓰면 끝.
- 구글 번호는 밖으로 내보내지 않고, 사람 목록에는 확인된 이메일만 (주인만 봄).

## 사관님이 해 보실 순서 (미리보기)

휴대폰에서 미리보기 주소 → **구글로 로그인** → hahadark@gmail.com 고르기 → 주인 화면(견본 교회) → '조작 화면'(`/control`) → 견본 큐시트 '견본 주일 예배'.
견본 교회에는 hahadark@gmail.com 이 주인으로 초대되어 있어(끝 없음) 처음 로그인하는 순간 주인 계정이 된다. 비밀번호는 만들지 않았다.
