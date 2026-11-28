# craft 합치기 (2026-11-28)

F1~F4 를 `craft` 가지 하나로 합쳤다. rust 에는 넣지 않았다 (사관님이 띄워 보고 정함).

- 바탕: origin/rust 036c428 (메뉴 안 A 951f548 · 바로 송출 경고 없앰 7c0ae98 · 036c428 포함)
- 합친 차례: craft-windows 23f142b(F2, 안에 F1 craft-main 0948bcb) → craft-web 9af56e1(F3) → solid 55da26c(F4)

## 푼 겹침

| 곳 | 어떻게 |
|---|---|
| `probar_ui.rs` 상태 표시창 (rust 7c0ae98 '바로 송출' 흐린 글 ↔ F1 표시창 새로 씀) | F1 것을 씀 — '· 바로 송출' 은 이미 흐린 글(lcd_label)이라 rust 의 뜻 그대로 |
| `system_ui.rs` 백업 · 되돌리기 (F2 글 다듬기 ↔ F4 `uistate::spawn`) | F2 글 + F4 의 `crate::uistate::spawn` (네 곳) |
| `shortcuts.json` (F2 `toggleOutputWindow` 이름 ↔ F4 `outputOff` 'Esc 는 두 번') | 둘 다: '예배당 화면 켜기 · 끄기' · '예배당 화면 끄기 (Esc 는 두 번)' (en.json 에 둘 다 있음) |
| `theme.rs` (F1 체계 ↔ F4 cfg(test) thread_local) | 저절로 합쳐짐, 시험 통과 |
| `workspace_stage` (F1 기준 위 F2 기능) · `errtext.rs` (F2 · F4 같은 내용) · `prolayout_ui` note_viewers 두 줄 · `josa_fix` | F2 가 이미 F1 위에서 맞췄고 F4 와 같은 내용이라 저절로 합쳐짐, note_viewers 두 줄 살아 있음 |

## 넘길 것 처리

- F2 → F1: float_pos 닫힌 창 세기(F1 5f0167c 에서 고침) · note_viewers 유지 · 견본에 창 부품 → `--specimen windows` (docs/design-system.md)
- F4 → F1: 배율 넘침 셋(꺼짐 띠 · 막대 · 뷰어 머리) → F1 0948bcb
- F4 parity 고침(`fit_before_pip`)으로 parity 48/48
- F3: 리모컨 '구성' 탭이 출력 분절로 바뀌어 `tools/scenes/e2e-auto.js` 의 리모컨 확인을 새 화면에 맞춤

## 합친 뒤 확인

| 무엇 | 결과 |
|---|---|
| cargo test --workspace | wp-app 279 · wp-core 384 (21 제외) · wp-output 1 통과 |
| 흔들림 | wp-app 시험 3번 반복 모두 279 통과 |
| clippy --workspace --all-targets | 오류 0 (근사 상수 둘 TAU · FRAC_1_SQRT_2 고침), 경고는 남음 |
| parity | 48/48 같은 글자 크기, 화소 차 최대 4.3% (글자 가장자리) |
| scenes e2e | e2e-outputs 통과 · e2e-auto 모두 통과 (e2e.js 2.0.8 비교는 옛 exe 없어 안 돌림) |
| tools/webui/e2e.js | 가짜 서버 44 통과 · 진짜 앱(격리 자료, 리모컨 · 편집기 · 휴대폰 카메라) 37 통과 |
| release 빌드 | `target\release\wp-app.exe` · `wp-output.exe` |

## 마무리 두 가지 (2026-11-28, 주인 답)

- **무대 조작은 인스펙터 '무대' 탭 한 곳.** 무대 화면 창(화면 → 무대 화면)은 무대 화면 미리 보기(`trackb_ui.rs stage_preview`, 웹 /outputs/stage 배치를 줄인 근사) · 주소 복사 · "조작은 인스펙터 '무대' 탭에서" 안내와 [무대 탭 열기]만. 창과 탭은 원래 같은 함수(`workspace_stage`)를 그렸으므로 탭으로 옮길 조작은 없었다 — 타이머(빠른 시작 · 길이 · 몇 시까지 · 조절) · 무대 메시지 · 고정 · 프리셋 · 무대 화면 창 열기/닫기 · 주소가 모두 탭에 그대로.
- 창 처음 자리는 본문 판 오른쪽 아래(`widgets_win::live_corner` · `note_body`), 사용자가 옮기기 전까지 판 자리를 따라간다 — 뷰어의 '다음' 슬라이드 줄 · 본문 판 머리를 가리지 않는다 (판이 창보다 낮으면 아래 시간표 위로 내려옴).
- **파일 → 새 큐시트 (Ctrl+N)** 맨 위. 큐시트 판 폴더 단추의 '새 큐시트'와 같은 `Act::NewCue`. Ctrl+N 은 쓰이지 않던 키, 글칸에서 쓸 때는 글칸 몫. 메뉴 시험(plan-a.json · 줄 수 · 단축키 12개 · 겹침 없음) · 단축키 창 '큐시트' 구획에 반영.
- 시험: `viewers_tests` 무대 창(미리 보기 · 주소만, 뷰어 · 판 머리 안 가림, 탭에 조작 그대로) · 파일 메뉴 · Ctrl+N.
