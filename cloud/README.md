# cloud/ — 워십프리젠터 웹판 (Cloudflare)

설계 · 계약: [`docs/web/architecture.md`](../docs/web/architecture.md) · 기능 대조표: [`docs/web/feature-matrix.md`](../docs/web/feature-matrix.md)

| 파일 | 하는 일 |
|---|---|
| `src/index.mjs` | Worker: 로그인 · 길 나누기 · 페이지 · 주인 API |
| `src/room.mjs` | `ChurchRoom` Durable Object: 교회마다 하나, 설치판 ↔ 화면 중계 |
| `src/auth.mjs` | 기기 열쇠(HMAC 해시) · 쿠키 · 틀린 횟수 |
| `src/protocol.mjs` | 계약: 길 · 역할 · 메시지 · 한도 |
| `src/pages.mjs` | `web/` 의 HTML 을 그대로 가져와 어댑터만 끼움 |
| `client/adapter.js` | 클라우드에서 연 페이지의 SSE → WebSocket 바꾸기 |
| `client/{login,owner,denied}.*` | 웹판이 새로 만든 화면 |
| `src/data/` · `src/identity/` | 2단계 A: 문서 · 동기화 · 성경 API · 장절 파서(설치판과 대조) · 사람 계정 ([`docs/web/data-api.md`](../docs/web/data-api.md)) |
| `src/ai/` | 5단계 AI: 교회 키 봉인 · 동의 · 한도 · 기록 · 대신 부르기 · 클라우드 음성 인식(무료 할당 안에서만) ([`docs/web/ai.md`](../docs/web/ai.md)) |
| `src/media/` · `client/media/` | 4단계 B: 카메라 이름 맞추기 · 휴대폰 카메라 신호(중계실) · PDF(pdf.js 고정판) · 탭끼리 영상 맞추기 ([`docs/web/media.md`](../docs/web/media.md)) — 브라우저 시험 `scripts/media-browser-e2e.mjs` |
| `migrations/` | D1 (`0002_data.sql` = 2단계 A · `0003_ai.sql` = 5단계 AI · `0004_assets.sql` = 2단계 C 파일) |
| `scripts/` | `assets`(web/ → public/) · `seed`(견본 교회) · `e2e` · `deploy` · `inventory`(대조표 빠짐 검사) · `desktop-e2e`(설치판과 함께) · `bible-import`(bible.db 사본 → D1) |

```sh
npm ci
npm run seed        # 로컬 D1 에 견본 교회 'sample' + 열쇠 (.secrets/sample.json, Git 제외)
npm run dev         # http://127.0.0.1:8787  → /login 에 sample + .secrets 의 owner 열쇠
npm test            # 단위
npm run test:e2e    # 빈 포트에 wrangler dev 를 띄워 끝에서 끝
node scripts/assets-purge.mjs --usage   # 2C: R2 사용량 · 지운 파일 정리 (docs/web/assets-api.md)
```

실제 자료(큐시트 · 성경 · 미디어)를 개발 · 미리보기 클라우드에 올리지 않는다. 비밀값(`.dev.vars` · `.secrets/`)은 커밋하지 않는다.
