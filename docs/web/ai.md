# 웹판 5단계 — AI (클라우드 쪽, 계약 v1, 구현함)

`architecture.md` 9절 `web-ai` 가운데 **클라우드 쪽**. 웹 조작 화면 안의 창(동의 창 · 키 넣는 칸 · 결과 보기)은 `web-egui` 몫이고, 그쪽은 이 문서를 계약으로 읽는다.
코드: `cloud/src/ai/` (`routes.mjs` 길 · 한도 · 동의 글 · 주소 검사 · WAV, `seal.mjs` 키 봉인, `index.mjs` D1 · 부르기) · `cloud/migrations/0003_ai.sql`.
시험: `cloud/test/ai.test.mjs`(단위) · `cloud/test/ai-e2e/ai.test.mjs`(wrangler dev + 가짜 공급자). **진짜 AI 공급자 · Workers AI 는 시험에서 부르지 않는다.**

## 1. 무엇을 하나

설치판 AI 도우미(`docs/ai-features.md`, `aihelp.rs`)는 OpenAI 호환 `chat/completions` 주소 하나 + 교회 키 + 모델을 PC 에서 부른다.
웹에서는 같은 요청을 **브라우저가 만들고(같은 Rust 코드 — `cuecheck` · `sermonslides` · `lyricsclean` · `recap` · `saycmd` · `voicefollow` 번역 · `bulletin`), Worker 가 교회 키를 붙여 대신 보낸다.** 키는 D1 에 봉인해 두고 브라우저 · 로그 · 응답에 내보내지 않는다.

- 공급자: 설치판과 같은 셋(오픈라우터 · DeepSeek · OpenAI) + 교회가 넣는 https 주소. 기본은 설치판처럼 오픈라우터.
- **저절로 부르는 길이 없다.** 타이머 · 알람 · 동기화 어디에서도 AI 를 부르지 않는다. 부르는 요청은 `X-WP-Action: user` 머리글이 있어야 하고(웹 조작 화면은 단추 처리 안에서만 붙인다), 기능마다 주인의 동의가 있어야 하고, 교회 하루 한도 · 1분 한도를 넘으면 부르기 **전에** 막는다. 모델은 교회 설정의 것을 쓰고 브라우저가 바꾸지 못한다(비싼 모델로 바꿔 부르지 못하게).
- 보낸 글 · 받은 답 · 소리는 **저장하지 않는다.** 기록에는 언제 · 누가 · 기능 · 어디로(호스트) · 모델 · 결과 코드 · 걸린 시간 · 글자 수 · 토큰 수 · 소리 초만 (90일).

## 2. 길 (권한 `ai` = 주인 · 사람 편집자, `manage` = 주인)

| 길 | 권한 | 내용 |
|---|---|---|
| `GET /api/v1/ai/settings` | ai | `{endpoint, model, endpointUsed, modelUsed, host, hasKey, keyHint:'…abcd', dailyLimit, sttMinutes, today:{calls, tokens, sttSeconds}, providers[], features:[{id, name, what, consented, by, at}], stt:{available, model, accountNeuronsToday, accountNeuronsCap}}` — 키 원문 없음 |
| `PUT /api/v1/ai/settings` | manage | `{endpoint?, model?, key?('' = 지움), dailyLimit?(0~500, 기본 30), sttMinutes?(0~180, 기본 60)}`. 주소가 바뀌면 글 기능 동의가 모두 풀린다(글이 가는 곳이 바뀌므로) |
| `GET /api/v1/ai/consent?feature=` | ai | 동의 창 `{feature, name, what, host, text, consented, by, at}` |
| `POST /api/v1/ai/consent` | manage | `{feature, agree:true|false}` |
| `POST /api/v1/ai/chat` | ai + `X-WP-Action: user` | 아래 |
| `POST /api/v1/ai/transcribe?feature=stt-cloud` | ai + `X-WP-Action: user` | 본문 = WAV(PCM) 30초 · 2 MB 까지 → `{ok, text, seconds, neurons}` |
| `GET /api/v1/ai/log?limit=50` | manage | `{calls:[{at, by, feature, kind, host, model, status, ms, chars, tokens, seconds}], days:[{day, calls, tokens, sttSeconds}]}` |

쿠키로 오는 쓰기(PUT · POST)는 1단계와 같이 같은 Origin + `X-WorshipCue: 1`. 설치판 Bearer 열쇠로는 열리지 않는다(설치판은 자기 키로 직접 부른다).

**기능 id** (동의 열쇠): `cuecheck` 예배 전 점검 · `sermon` 설교 원고 → 슬라이드 · `lyrics` 가사 정리 · `translate` 실시간 자막 번역 · `recap` 예배 후 정리 · `say` 말로 시키기 · `bulletin` 주보 → 큐시트 (앞의 여섯은 `aihelp.rs FEATURES` 그대로) · `stt-cloud` 클라우드 음성 인식.

### `POST /api/v1/ai/chat`

요청 = 설치판 `aihelp::request_body` 와 같은 모양 + `feature`:
```json
{ "feature": "lyrics",
  "messages": [{ "role": "system", "content": "…" }, { "role": "user", "content": "…" }],
  "temperature": 0.2, "max_tokens": 1500, "response_format": { "type": "json_object" } }
```
- `model` 은 보내도 무시(교회 설정). 메시지 20개 · 글 합 60,000자 · `max_tokens` 4,000 까지. `response_format` 은 `json_object` 만 넘긴다.
- 답 = 설치판 `aihelp::response_text` · `usage_text` 가 그대로 읽는 모양: `{ok:true, model, choices:[{message:{role, content}}], usage:{prompt_tokens, completion_tokens, total_tokens}}`
- 오류: 428 머리글 없음 · 403 `{needConsent:true}` 동의 없음 · 409 `{needKey:true}` 키 없음 · 413 너무 김 · 429 `{limit:true}` 하루 한도 / '너무 자주' 1분 한도 · 502 공급자 오류 `{providerStatus}`(키는 글에서 지움) · 503 봉인을 못 풂.

**웹 조작 화면(wasm)에서**: `web-egui` 는 wasm 에서 `ureq` 를 '부르면 오류' 로 두었다 → `aihelp::chat` 의 wasm 쪽은 이 길로 바꿔 보내면 된다(본문 그대로 + `feature`, 단추 처리 안에서 `X-WP-Action: user`). 403 `needConsent` 면 `GET /consent` 글로 동의 창(주인만 `POST`), 409 `needKey` 면 '주인이 AI 설정에서 키를 넣으세요'.

## 3. 키 봉인

AES-GCM 256. 열쇠 = HKDF-SHA256(Worker 비밀값 `AI_SECRET`, 없으면 `AUTH_SECRET`; salt `wp-ai-key-v1`; info `church:<교회>`), 덧붙임 자료(AAD) = 교회 ID → D1 이 새어도 키가 안 보이고, 한 교회 줄을 다른 교회로 옮겨도 풀리지 않는다. 저장 꼴 `v1.<iv>.<암호문>`, 화면에는 끝 4자리만.
`AUTH_SECRET` 을 바꾸면(또는 `AI_SECRET` 을 새로 두면) 봉인한 키를 못 푼다 → 주인이 키를 다시 넣는다(503 안내).

## 4. 음성 인식 — 웹에서 쓸 길 비교 (2026-11-28)

설치판은 PC 안 whisper.cpp(small q5_1 190MB · base 60MB, 실측 표는 `docs/ai-features.md`). 소리는 PC 밖으로 안 나간다.

| 길 | 비용 | 속도 | 한국어 | 개인정보 | 판단 |
|---|---|---|---|---|---|
| **설치판에 맡기기** (같은 교회 PC 의 whisper.cpp, 결과 글만 클라우드로) | 0 | 설치판 실측 그대로 (small 조각당 ~2초) | 설치판 실측 85~95% | 소리가 PC 밖으로 안 나감 | 예배 PC 에 설치판이 있으면 **이것이 첫째**. 결과 글을 웹 조작 화면에 넘기는 메시지는 3단계 송출 주체 정리 때(`hello.source`) 함께 정한다 |
| **브라우저 안 whisper (wasm)** — whisper.cpp wasm · transformers.js | 0 (모델 40~190MB 를 처음 한 번 받음) | 기기 CPU 에 달림. 노트북 base 는 실시간 가능, small 은 느릴 수 있음 · 휴대폰은 어려움 | 같은 모델이면 설치판과 비슷 | 소리가 기기 밖으로 안 나감 | 설치판이 없을 때의 둘째 길. **웹 조작 화면 안의 일(`web-egui`)**, 클라우드는 모델 파일만 내보내면 됨(2C R2 · 공개 GPL/MIT 모델 주소) |
| **Cloudflare Workers AI whisper** (`@cf/openai/whisper-large-v3-turbo`) | 계정 무료 할당 **하루 10,000 neurons**, 이 모델은 **46.63 neurons/분**(≈ 하루 214분). 무료 플랜은 넘으면 실패(과금 없음), 유료 플랜은 넘은 만큼 $0.011/1,000 neurons | 서버 large-v3-turbo, 조각 하나 보통 1~3초(문서 값 아님 — 실제로 재 보지 않음) | large-v3 계열이라 small 보다 나을 것으로 봄(시험 안 함, `language:'ko'` 지원) | 소리가 Cloudflare 로 감(저장하지 않는다고 명시 · 동의 받음) | **구현함(무료 할당 안에서만)**. 기본 꺼짐 — 사관님이 켜기로 정할 때만 |
| 브라우저 Web Speech API (크롬) | 0 | 빠름 | 좋음 | **소리가 구글로 감**, 끄고 켤 수 없음 · 크롬만 | 쓰지 않음 (동의와 통제가 안 됨) |

문서 (2026-11-28 확인):
- 무료 할당 · 값 · 넘을 때: https://developers.cloudflare.com/workers-ai/platform/pricing/ — "10,000 Neurons per day at no charge" (무료 · 유료 플랜 같음), 무료 플랜은 넘으면 "further operations will fail with an error", 유료 $0.011/1,000 neurons. whisper 41.14 · whisper-large-v3-turbo 46.63 neurons/분. (모델 쪽에는 $0.000513/분 · $0.000453/분으로 조금 다르게 적혀 있어 넉넉히 잡음)
- 모델: https://developers.cloudflare.com/workers-ai/models/whisper-large-v3-turbo/ (`audio` · `language` · `task`)
- 한도: https://developers.cloudflare.com/workers-ai/platform/limits/ — 음성 인식 분당 720 요청
- 로컬 개발: https://developers.cloudflare.com/workers/local-development/ — "AI models always run remotely", 로컬 흉내 없음, 원격 사용은 요금 대상.

**구현한 지킴** (`src/ai/index.mjs transcribe`):
1. 기능 동의(`stt-cloud`) · `X-WP-Action: user` · 분당 20조각.
2. 교회 하루 분 한도(기본 60분, 주인이 0~180) — 부르기 전에 조건부 UPDATE 로 잡는다.
3. **계정 전체 무료 할당**: 하루(UTC) 8,000 neurons 까지만(10,000 중 다른 앱 몫 2,000 을 남김). 소리 길이를 WAV 머리로 재서 52 neurons/분(문서 값보다 큼)으로 미리 잡고, 넘으면 **부르지 않고 429 `{free:true}` 로 멈춘다.** 같은 계정의 다른 앱이 Workers AI 를 쓰면 그 몫은 여기서 세지 못한다 → 주의(아래 '사관님').
4. 소리 · 글을 저장하지 않는다. 결과는 웹 조작 화면에서 '다음' 칸까지만(설치판 지킬 것과 같음 — `docs/ai-features.md`).
5. **AI 바인딩을 `wrangler.jsonc` 에 넣지 않았다**: 넣으면 로컬 `wrangler dev` · CI 에서도 진짜 Workers AI 를 부른다. 켤 때만 `WP_ENABLE_WORKERS_AI=1 node scripts/deploy.mjs` 가 배포 설정에 `"ai": {"binding": "AI"}` 를 넣는다. 바인딩이 없으면 503 `{unavailable:true}` '설치판의 PC 안 음성 인식을 쓰세요'.

## 5. .pptx · HWP — 클라우드에 따로 만들 것이 있나

없다. `.pptx` 읽기 · 그리기(`pptx/` → SVG → resvg), `.ppt/.pps` 그림 · 글 꺼내기(`pps.rs`), HWP · HWPX · DOCX 글 꺼내기(`bulletin.rs`)는 순수 Rust 이고, `web-egui` 가 wp-core 를 wasm32 로 빌드하면서 이 모듈들(cfb · zip · roxmltree · resvg)을 공통 의존성에 그대로 두었다 → **브라우저 안에서 돈다.** 클라우드가 할 일은:
- 만든 그림 · 원본 파일 보관 = 2C(R2) 의 `assets`.
- 글꼴: resvg 가 그릴 때 맑은 고딕 · 구세군체가 필요 → 2C 에서 교회 글꼴을 R2 에 두고 wasm 이 읽게.
- 브라우저로 못 하는 것: 옛 `.ppt` 를 PowerPoint 로 '픽셀 그대로' 그리기(COM) → 설치판이 만들어 R2 에 올리는 길(2B/2C).
- 주보 사진 · PDF 는 AI(위 chat, `bulletin` 동의)로.

## 6. 사관님이 정하실 것 · 하실 일

- 클라우드 음성 인식을 켤지(기본 꺼짐). 켜면 `WP_ENABLE_WORKERS_AI=1` 로 배포. **계정 요금제 확인**: 무료 플랜이면 넘어도 과금 없이 실패, 유료 플랜이면 이 앱은 8,000 에서 멈추지만 같은 계정의 다른 앱이 Workers AI 를 쓰면 합이 10,000 을 넘어 과금될 수 있다.
- AI 키를 웹에 넣을지(주인 화면 · 웹 조작 화면의 AI 설정에서). 설치판 키와 따로 둔다.
- 키 봉인 비밀값을 로그인 비밀값과 나누려면 Worker 비밀값 `AI_SECRET`(32자 이상)을 따로 둔다(선택).
