# 화면 동작 대조 (원본 Electron 판 ↔ Rust 판)

갈래 C 점검 기록. 원본 `F:\aside\WorshipCue\src\web\control` 의 이벤트 처리기를 Rust 조작 화면과 하나씩 대조한다.
**상태: 진행 중.** 아래 표는 두 번 클릭(dblclick) 처리기 13곳만 담았고, 소스 읽기로 대조한 것이다.
'합성 입력 시험' 칸이 비어 있으면 아직 실제 두 번 클릭 시험으로 확인하지 않았다는 뜻이다.
onclick·oncontextmenu·keydown·drag·drop·wheel·paste 전체 목록과 e2e·core.test.js 대조는 아직 하지 않았다.

## 1. 두 번 클릭

| # | 원본 (파일:줄) | 원본 동작 | Rust 자리 | Rust 동작 | 판정 | 합성 입력 시험 |
|---|---|---|---|---|---|---|
| 1 | control.js:611 | 미리보기 아래 슬라이드 번호 두 번 = 그 장으로 옮기고 송출 | app.rs:1139-1140 | clicked → PreviewIndex, double_clicked → SendPreview (LIVE 쪽 칩은 제외) | 같음 (소스) | 없음 |
| 2 | control.js:917 | 검색 결과 줄 두 번 = 클릭 미리보기가 끝난 뒤 미리보기하고 송출 | center_ui.rs:1422 (장절), 1465 (낱말 검색) | activate + SendPreview / SendRef | 같음 (소스) | 없음 |
| 3 | cue.js:945 | 순서의 슬라이드 줄(.srow) 두 번 = 그 장 송출 | center_ui.rs:1566, 1610 | PreviewIndex + SendPreview (Shift·Ctrl 은 제외) | 같음 (소스) | 없음 |
| 4 | cue.js:1266 | 큐시트 순서 줄 두 번 = **미리보기만** (송출 안 함, 편집 단추 위 제외) | app.rs:631-638 | 클릭 = PreviewItem (바로 송출 모드면 송출), 두 번 = **SendItem (송출)** | **다름 — 아래 2절** | 없음 |
| 5 | cue.js:553 | PPT 나누기 창 썸네일 두 번 = 그 자리에서 나눔 토글 | hymns_ui.rs:286~ | 썸네일 격자 자체가 없음 (범위 글칸·자동·AI 나누기만) | **빠짐** | 없음 |
| 6 | cue.js:1580 | 찬송가 고르기 창 목록 두 번 = 고르기 | hymns_ui.rs:208 근처 | 한 번 클릭으로 넣고 미리보기 (두 번 클릭 처리기 없음) | 확인 필요 (한 번 클릭 동작이 두 번 실행되는지) | 없음 |
| 7 | cuemanager.js:173 | 큐시트 관리 창 줄 두 번 = 그 큐시트 열기 (휴지통 보기 제외) | trackb_ui.rs:791 cues_window | 큐시트 목록 줄이 없음 (지금 큐시트 정보·진행 시계·휴지통만) | **빠짐** | 없음 |
| 8 | grid.js:50 | 격자 칸 두 번 = 그 장 송출 (편집·작업 단추 위 제외) | center_ui.rs:980-985 | Hit::Pick(k, true) | 같음 (소스) | 없음 |
| 9 | grid.js:151 | 전체 격자 칸 두 번 = previewAt 뒤 송출 | center_ui.rs:1899-1904 | Hit::Pick(k, true) | 같음 (소스) | 없음 |
| 10 | library.js:84 | 라이브러리 줄·칸 두 번 = 큐시트에 넣기 | trackb_ui.rs:217 (찬양), thumbs_ui.rs:158 (그림·영상) | 찬양·그림·영상은 두 번 = 넣기. 찬송가·음악·카메라 탭은 확인 못 함 | 일부 같음 | 없음 |
| 11 | slideedit.js:1012 | 편집기 상자 두 번 = 글 바로 고치기 (잠긴 상자 제외) | slideedit_ui.rs, paint_edit.rs | double_clicked 처리기 없음 | **빠진 것으로 보임** | 없음 |
| 12 | ctxmenu.js:141 | 오른쪽 메뉴 '▶ 바로 송출 Ctrl+Enter' | center_ui.rs:871, parity2_ui.rs:207 | 항목 있음 (parity2_ui.rs:207 문구는 tr 을 거치지 않음) | 있음 (동작 미확인) | 없음 |
| 13 | ctxmenu.js:205 | 오른쪽 메뉴 '큐시트에 넣기 (더블클릭)' | thumbs_ui.rs:150, trackb_ui.rs:1142 | 마우스 올림 단추로 있음. 오른쪽 메뉴 항목은 확인 못 함 | 확인 필요 | 없음 |

### 클릭과 두 번 클릭이 함께 발생하는 문제

egui 는 두 번째 클릭에서 `clicked()` 와 `double_clicked()` 가 같이 참이 된다.
- 찬양 목록(trackb_ui.rs:212-221)은 `else if` 로 묶여 있어 넣기가 한 번만 된다.
- 그림·영상(thumbs_ui.rs:158)은 넣기 뒤 `wp_core::pick` 의 연타 무시·같은 것 고르기에 기대고 있다.
- 한 번 클릭으로 넣는 찬송가 목록(6번)은 두 번 누르면 넣기가 두 번 불린다. `pick` 이 막아 주는지 시험으로 확인해야 한다.

## 2. 큐시트 순서 줄: 원본과 다른 점 (결정 필요, 바꾸지 않음)

원본 규칙 (cue.js:951 `quietPreview`, control.js:473):
- 큐시트 목록에서 순서를 고르면 `app.holdDirect` 를 올린 채 미리보기한다.
- `directSend` 는 `directMode() && !app.holdDirect && 최근 2.5초 안의 사용자 조작` 일 때만 송출한다.
- 그래서 원본은 **'미리보기 끄기(바로 송출)' 모드여도 큐시트 순서 줄을 클릭·두 번 클릭해서는 송출되지 않는다.** 송출은 Enter(F9)나 본문 쪽 슬라이드를 눌러야 된다 (cue.js:1283).

Rust 판 지금 동작:
- 바로 송출 모드에서 순서 줄 클릭 = 송출 (사용자가 '클릭해도 바로 송출 안 된다'고 해서 넣은 것).
- 순서 줄 두 번 클릭 = 모드와 상관없이 송출 (app.rs:636).
- 안내 문구도 그렇게 적혀 있다: cue_ui.rs:520 '클릭 = 바로 송출 (회중 화면에 바로 나감)'.

판단: 사용자의 최근 지적("더블클릭하면 바로 켜지게 해야지")과 지금 Rust 동작이 맞고, 원본 규칙으로 되돌리면 그 지적과 어긋난다. 원본과 다르지만 그대로 두는 쪽을 권한다. 사용자 확인이 필요하다.

## 3. 앞 작업자 중간 작업 (커밋 2d83601)

- 검색칸: 치는 중에는 목록만 (`Act::SearchList`), Enter 때 미리보기·송출 (`Act::Search`), Ctrl+Enter = `Act::SendNow`.
- 즉석 자막: Enter 송출 · Shift+Enter 줄 바꿈 · Esc 닫기 · 최근 목록 12개 · 긴 글 나눔 · 큐시트에 넣기.
- Ctrl+M·Ctrl+T·Ctrl+L·Ctrl+P 가 설정의 단축키를 따름.
- `auditc_tests.rs` 8개 시험 통과 확인.

## 4. 남은 일

- 위 표 13곳을 합성 두 번 클릭 시험으로 확인.
- 5·7·11번 구현 (PPT 나누기 썸네일, 큐시트 관리 목록, 편집기 글 바로 고치기).
- 예배당 화면이 꺼져 있을 때 송출하면 자동으로 켜지는지 (output.autoOnSend) 확인.
- 2순위 전체 이벤트 처리기 목록과 e2e·core.test.js 대조.
