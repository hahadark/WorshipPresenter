# 메뉴 안 A — 이번에 하지 않고 남긴 것 (2026-11-28)

메뉴 막대(파일 · 편집 · 보기 · 큐시트 · 화면 · 연결 · 송출 · AI · 도움말)와 화면 안내 글·문서의 메뉴 경로 대부분은 바꿨다.
아래는 "메뉴만이라도 먼저" 내기 위해 이번에 손대지 않은 것이다. 다음 작업에서 처리한다.

## 안내 글 (코드 주석 · 시험 메시지 — 화면에는 안 나옴)

- `crates/wp-app/src/panel_ui.rs:241` 주석 "큐시트 메뉴" → 실행 취소 · 다시 실행은 이제 편집 메뉴.
- `crates/wp-app/src/settings_items.rs:10` 주석 "도구 메뉴로 흩어진 것" (역사 설명, 필요하면 "예전 도구 메뉴").
- `crates/wp-app/src/settings_ui.rs:676` · `settings_center_tests.rs:363` 주석 "보기 메뉴와 같은 길" → OBS 자막은 이제 화면 메뉴.
- `crates/wp-app/src/viewers_tests.rs:55` 시험 메시지의 옛 이름 "뷰어: 둘 다 · 송출 중만 바꾸기" → 새 이름 "뷰어 바꾸기".
- `crates/wp-app/src/workspace_ui.rs:40` · `probar_ui.rs:576` · `viewers_tests.rs:2` 의 "보기 메뉴" 는 새 안에서도 맞음 (고칠 것 없음, 확인만 함).

## 사전 · 웹

- `web/shared/i18n-en.js` 에 "도구 → 휴대폰 리모컨" · "도구 → OBS 원격 조작" · "도구 → 매크로" 등 옛 경로 열쇠가 남아 있다. Electron 판에서 옮긴 사전이고 Rust 판 웹 화면(web/remote · web/output · web/editor)에는 그 한국어 문구가 없어 지금은 쓰이지 않는다. 정리할지 정할 것.
- `crates/wp-core/data/en.json` 의 OBS 쪽 "OBS → 도구 → WebSocket 서버 설정" 은 OBS 프로그램 자체의 도구 메뉴라 그대로 둔다.

## 문서 옮기기

- `main` 가지의 README · 사용 설명서(메뉴 경로 · 메뉴 그림)를 안 A 로 바꾸기. 바꿀 자리 표는 `docs/menu-2026-11/migration.md` (69가지 동작의 예전 자리 → 새 자리).
- 다음 릴리스 설명에 `migration.md` 의 이동표 요약 넣기.
- `docs/settings-improvements.md:14` · `docs/ui-review-sources-ai.md:22` 의 "도구 메뉴 / 도구 →" 는 그때의 기록이라 두었다 (표의 '새 자리' 칸은 바꿈).
- `docs/design/round2-README.md` · `docs/behavior-parity.md` 의 화살표는 메뉴 경로가 아님 (고칠 것 없음).

## 확인

- 1600x960 한국어 캡처만 찍었다. 1000x640 · 150% 배율 · 영어 화면의 실제 캡처 전부(`node tools/menu/shots.js`)는 다시 돌리지 않았다.
