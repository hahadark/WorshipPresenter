# 구글 드라이브 연결 준비 — 사관님이 하실 일 (무료 등급)

무료 등급(교회 자료를 그 교회의 구글 드라이브에 저장)을 진짜 구글에 붙이려면 **구글 클라우드 콘솔에 OAuth 클라이언트**가 있어야 합니다.
코드는 다 되어 있고 가짜 구글로 시험했습니다(`docs/web/tiers.md`). 아래를 하시면 켜집니다. 돈은 들지 않습니다(구글 클라우드 무료 · Drive API 무료).
작업자는 이 단계를 하지 않았습니다(콘솔 · 계정 설정은 사관님 몫).

> 이 문서의 메뉴 이름 · 규칙은 2026-11-28 구글 문서로 확인했습니다. 구글 화면은 자주 바뀌므로 이름이 조금 다를 수 있습니다.

## 0. 정해 둘 것

| 무엇 | 지금 값 (미리보기) | 메모 |
|---|---|---|
| 웹판 주소 | `https://worshippresenter-web.hahadark-media.workers.dev` | 나중에 교회 도메인(예: `web.example.org`)으로 바꾸면 3 · 5단계를 다시 |
| 리디렉션 주소 (콜백) | `https://worshippresenter-web.hahadark-media.workers.dev/auth/google/callback` | 글자 하나까지 같아야 함 |
| 앱 이름 | 예: '워십프리젠터 웹' | 동의 화면에 보임(브랜드 확인 전에는 도메인만 보일 수 있음) |
| 지원 이메일 · 개발자 연락처 | 사관님 이메일 | 동의 화면 · 구글 연락용 |
| 개인정보처리방침 · 서비스 약관 주소 | (아직 없음) | 프로덕션 게시 · 브랜드 확인에 필요 — 웹판에 페이지를 만들어 드릴 수 있음 |

## 1. 프로젝트 만들기 · Drive API 켜기

1. https://console.cloud.google.com/ 에 사관님 구글 계정으로 들어가 **새 프로젝트**(예: `worshippresenter-web`).
2. **메뉴 → APIs & Services → Library → Google Workspace → Google Drive API → Enable**.
   - 문서: "Before using Google APIs, you need to turn them on in a Google Cloud project." — https://developers.google.com/workspace/guides/enable-apis

## 2. Google Auth Platform — 동의 화면 (Branding · Audience · Data Access)

메뉴 → **Google Auth Platform** (구역: Branding · Audience · Clients · Data Access · Verification Center — https://support.google.com/cloud/answer/15544987).

1. **Branding**: 앱 이름 · 지원 이메일 · (선택) 로고 · 앱 도메인(홈페이지 · 개인정보처리방침 · 약관 주소) · **Authorized domains** 에 `hahadark-media.workers.dev` (또는 교회 도메인).
   - "Add your Authorized Domains before you add your redirect or origin URIs." · "All domains used in your project … must be pre-registered here." — https://support.google.com/cloud/answer/15549049
   - `workers.dev` 하위 주소를 승인 도메인으로 받아 주는지는 **확인하지 못했습니다**(도메인은 공개 접미사 목록을 따라야 하고, 확인이 필요한 앱은 Search Console 로 도메인 소유를 확인해야 함 — https://support.google.com/cloud/answer/13464321). 막히면 교회 도메인을 Cloudflare 에 붙이는 것이 가장 쉽습니다.
2. **Audience**: 사용자 유형 **External**(구글 계정이 있는 누구나 — 다른 교회가 가입하려면 이것).
   - 처음에는 **Testing** 상태입니다: 시험 사용자 **100명까지**, 그리고 **"Authorizations by a test user will expire seven days from the time of consent"** — 우리는 드라이브 범위를 쓰므로 테스트 상태에서는 7일마다 연결이 끊깁니다(다시 연결하면 됨). 시험하실 때는 사관님 계정을 **Test users** 에 넣으세요. — https://support.google.com/cloud/answer/15549945
   - 실제로 쓰려면 **Publish app** → **In production**.
3. **Data Access → Add or Remove Scopes** 에 아래 셋만 (그 밖은 넣지 마세요):
   - `openid` · `.../auth/userinfo.email` (누구인지)
   - `https://www.googleapis.com/auth/drive.file` — **"이 앱이 만든 파일만"**. 구글 분류로 **비민감(Non-sensitive), 권장 범위**입니다. 드라이브 전체(`.../auth/drive`)는 **제한(Restricted) 범위**라 서버에 저장하면 보안 심사가 필요합니다 — https://developers.google.com/workspace/drive/api/guides/api-specific-auth
   - 비민감 범위만 쓰면 "it is not mandatory for your app to complete the app verification process". 다만 동의 화면에 **앱 이름 · 로고를 보이려면 가벼운 브랜드 확인**이 필요하고, 확인 전에는 "only your application domain will be visible" — https://support.google.com/cloud/answer/13463073 · https://support.google.com/cloud/answer/10311615

## 3. Clients — OAuth 클라이언트 만들기

1. Google Auth Platform → **Clients → Create Client → Application type: Web application** → 이름(예: '워십프리젠터 웹').
2. **Authorized redirect URIs** 에 0절의 콜백 주소 하나: `https://worshippresenter-web.hahadark-media.workers.dev/auth/google/callback`
   - 규칙: https 만(localhost 빼고) · IP 주소 안 됨 · 와일드카드 안 됨 · 바꾼 것이 반영되기까지 "5 minutes to a few hours" — https://support.google.com/cloud/answer/15549257
   - **Authorized JavaScript origins 는 비워 둡니다**(브라우저가 구글을 직접 부르지 않고 Worker 가 부릅니다).
3. **Create** → 나오는 **Client ID** 와 **Client secret** 을 적어 둡니다. "Your application's client secret will only be shown after you create the client." (같은 문서)

## 4. Worker 비밀값 넣기 (작업자에게 맡기셔도 됨 — 값은 채팅에 붙이지 말고 직접)

`cloud/` 에서 (Cloudflare 로그인이 된 PC):

```sh
npx wrangler secret put GOOGLE_CLIENT_ID --name worshippresenter-web       # 3단계의 Client ID
npx wrangler secret put GOOGLE_CLIENT_SECRET --name worshippresenter-web   # 3단계의 Client secret
# (선택) 리디렉션 주소를 고정하려면 — 없으면 들어온 주소의 origin + /auth/google/callback
npx wrangler secret put GOOGLE_REDIRECT_URI --name worshippresenter-web
```
- 둘이 없으면 `/auth/google/*` 은 404 이고 로그인 화면의 '구글로 시작하기' 는 눌러도 안내만 나옵니다(지금 미리보기 상태).
- 토큰 봉인에는 이미 있는 `AUTH_SECRET`(또는 따로 둔 `AI_SECRET`)을 씁니다. **이 값을 바꾸면 모든 교회가 드라이브를 다시 연결해야 합니다**(AI 키와 같음).
- `GOOGLE_FAKE` · `TIER_OVERRIDES` · `SERVICE_OVERRIDES` · `TIER_CACHE_MS` 는 **시험용이므로 배포에 넣지 마세요**(`GOOGLE_FAKE` 는 넣어도 로컬 주소가 아니면 무시합니다).
- 새 마이그레이션 `0005_tiers.sql` 을 배포한 D1 에 적용해야 합니다(배포 작업 때 `wrangler d1 migrations apply worshippresenter-web --remote`).

## 5. 확인 (사관님 계정으로)

1. `https://…workers.dev/login` → '처음이세요?' → 교회 이름 → **구글로 무료 시작하기** → 구글 동의 화면에서 드라이브 칸을 **켠 채** 허용.
2. `/owner` 의 '등급 · 저장소' 에 '무료 (구글 드라이브) · 연결됨' 이 보이고, 사관님 구글 드라이브에 `워십프리젠터/<교회 이름>/` 폴더가 생깁니다.
3. 잘 안 되면 주소창의 `/login?google=<까닭>` 을 작업자에게 알려 주세요 (`scope` = 드라이브 칸을 끔, `token` = 클라이언트 값 · 리디렉션 주소가 다름, `state` = 10분이 지남).
4. 연결 끊기 시험: https://myaccount.google.com/permissions 에서 이 앱의 권한을 지우면 웹판이 '다시 연결하기' 를 보입니다(송출 중인 화면은 그대로).

## 6. 알아 두실 것 (구글 쪽 한도)

- 갱신 토큰은 6개월 안 쓰면 · 사용자가 권한을 지우면 · 한 구글 계정 · 한 클라이언트에 100개를 넘으면(가장 오래된 것부터) 죽습니다 — https://developers.google.com/identity/protocols/oauth2 ('Refresh token expiration'). 웹판은 그때 '다시 연결' 을 안내합니다.
- Drive API 는 무료이고 사용자마다 1분 단위 할당이 있습니다(넘으면 잠시 막힘 → 웹판은 429 로 '잠시 뒤') — https://developers.google.com/workspace/drive/api/guides/limits
- 다른 교회가 가입하면 그 교회의 자료는 **그 교회 구글 계정의 드라이브**에 저장됩니다(우리 계정이 아님). 우리 쪽 D1 에는 계정 · 봉인한 토큰 · 폴더 id · 판 번호 같은 작은 것만 남습니다.
