# 화면 동작 대조 (원본 Electron 판 ↔ Rust 판)

갈래 C 점검 기록 (2026-11-15 ~ 16). 원본 `F:\aside\WorshipCue\src\web\control` 의 이벤트 처리기를 Rust 조작 화면과 하나씩 대조한다.
'합성 시험' 은 화면을 창 없이 그리며 실제 입력 사건(마우스 누름·뗌, 글쇠)을 넣는 시험이다 (`crates/wp-app/src/dbl_tests.rs`, `auditc_tests.rs`).
실제 창을 사람이 눌러 본 것은 아니다. '소스 근거만' 은 Rust 코드에 그 기능이 있다는 것만 찾은 것이고 동작을 시험하지 않았다.

## 1. 두 번 클릭 13곳

| # | 원본 (파일:줄) | 원본 동작 | 고치기 전 Rust | 지금 | 합성 시험 |
|---|---|---|---|---|---|
| 1 | control.js:611 | 미리보기 아래 번호 두 번 = 그 장으로 옮기고 송출 | egui `double_clicked` (첫 클릭으로 배치가 바뀌면 놓칠 수 있음) | 항목 키 판정(click2)으로 송출 | `dbl_preview_chip_moves_and_sends` |
| 2 | control.js:917 | 본문(검색 결과) 줄 두 번 = 미리보기하고 송출 | 같음 (같은 위험) | click2 로 송출 | `dbl_search_result_row_previews_and_sends` |
| 3 | cue.js:945 | 순서의 슬라이드 줄 두 번 = 그 장 송출 | 같음 (같은 위험) | click2 로 송출 | `dbl_slide_row_sends_that_slide` |
| 4 | cue.js:1266 | 큐시트 순서 줄 두 번 = **미리보기만** | 두 번 = 송출 (편집칸이 열려 줄이 밀리면 안 잡힘) | **의도적 차이 유지**: 두 번 = 송출, 줄이 밀려도 잡힘 | `dbl_cue_row_sends_in_preview_mode` · `dbl_cue_row_in_direct_mode_sends_once_and_stays_on_that_item` · `dbl_cue_row_with_editor_open_still_sends` |
| 5 | cue.js:553 | PPT 나누기 창 썸네일 두 번 = 그 자리에서 나눔 토글 | 썸네일 격자가 없었음 | 나누기 창을 새로 만듦 (`pptsplit_ui.rs`) | `dbl_ppt_split_thumbnail_toggles_split_there_and_again_merges` 외 2개 |
| 6 | cue.js:1580 | 찬송가 고르기 목록 두 번 = 고르기 | 한 번 클릭에 바로 넣음 (두 번 누르면 넣기가 두 번 불림) | 한 번 = 고르기, 두 번·Enter = 넣기 (한 번만) | `dbl_library_hymn_row_adds_once_and_single_click_only_selects` · `hymn_search_arrow_and_enter_add_selected_row` |
| 7 | cuemanager.js:173 | 큐시트 관리 줄 두 번 = 그 큐시트 열기 (휴지통 보기 제외) | 큐시트 목록이 없었음 | 관리 창을 새로 만듦 (`cuemgr_ui.rs`) | `dbl_cue_manager_row_opens_that_cue_and_single_click_only_selects` · `cue_manager_trash_restore_and_no_open_from_trash_view` |
| 8 | grid.js:50 | 격자 칸 두 번 = 그 장 송출 | egui `double_clicked` | click2 | `dbl_grid_cell_sends_that_slide` |
| 9 | grid.js:151 | 전체 격자 칸 두 번 = 그 순서 그 장 송출 | egui `double_clicked` (첫 클릭 뒤 스크롤 따라가기로 칸이 달아남) | click2 + 첫 클릭 때 스크롤하지 않음 | `dbl_all_grid_cell_of_another_item_sends_it` · `dbl_all_list_line_sends_it` · `first_click_in_all_view_does_not_scroll_the_clicked_cell_away` |
| 10 | library.js:84 | 라이브러리 줄·칸 두 번 = 큐시트에 넣기 | 찬양·그림·영상만. 찬송가·음악은 한 번에 넣음, 카메라 없음 | 모든 탭: 한 번 = 고르기, 두 번 = 넣기 (카메라는 두 번 = 송출 배경) | `dbl_library_song_adds_it_once` · `…hymn…` · `dbl_library_music_row_adds_audio_item_once` · `dbl_library_camera_toggles_live_background` · `dbl_library_image_cell_adds_image_item_once` |
| 11 | slideedit.js:1012 | 편집기 상자 두 번 = 글 바로 고치기 (잠긴 상자 제외) | 없었음 | 본문·덧붙인 글상자를 그 자리에서 고침 | `dbl_editor_main_text_edits_in_place_and_ctrl_enter_saves_without_sending` 외 2개 |
| 12 | ctxmenu.js:141 | 오른쪽 메뉴 '▶ 바로 송출 Ctrl+Enter' | 일부 자리에만, 문구가 번역을 거치지 않음 | 본문 줄·슬라이드 줄·격자 칸 메뉴에 있음 | `verse_menu_send_now_goes_live_like_double_click` · `slide_row_and_grid_cell_menu_send_now` |
| 13 | ctxmenu.js:205 | 오른쪽 메뉴 '큐시트에 넣기 (더블클릭)' | 마우스 올림 단추만 | 찬양·찬송가·음악·그림 줄 메뉴에 있음 | `library_menu_add_to_cue_for_song_hymn_music_image` |

### 두 번 클릭을 어떻게 판정하나 (`widgets::click2`)

egui 의 `double_clicked()` 는 두 번의 클릭이 같은 위젯(같은 Id·같은 자리)에 떨어져야 한다. 첫 클릭으로 화면이 바뀌면
(편집칸이 열려 목록이 밀림 · 미리보기가 생김 · 바로 송출로 다시 배치 · 스크롤 따라가기) 두 번째 클릭이 다른 줄에 떨어져 "두 번 눌러도 안 켜진다".
`click2` 는 첫 클릭 때 **항목 키**(순서 id 등)를 기억하고, 0.5초 안에 거의 같은 화면 좌표를 다시 누르면 그 항목이 어디로 밀렸든 그 항목의 두 번 클릭으로 친다.
그때 그 자리에 온 다른 줄은 클릭을 받지 않는다. 두 번째 클릭에서는 한 번 클릭 동작을 다시 하지 않으므로 넣기가 두 번 되지 않는다.
시험: `click2_second_click_counts_even_when_first_click_shifts_the_list` · `click2_slow_second_click_within_half_second_is_double_and_later_is_single` · `click2_two_different_rows_are_two_single_clicks` · `first_click_keeps_rows_in_place_everywhere`.

### 두 번 클릭으로 송출하면 예배당 화면이 켜지나

`output.autoOnSend` (기본 켬) 이면 송출할 때 꺼져 있는 예배당 화면을 켜 달라고 요청한다. 방금 보낸 것을 다시 눌러도 꺼져 있으면 켠다.
시험(요청 횟수로 확인, 실제 창은 띄우지 않음): `double_click_send_asks_to_turn_output_on_on_every_path` · `direct_mode_first_click_asks_to_turn_output_on_and_second_click_too` · `auto_on_off_setting_is_respected`.

## 2. 의도적 차이: 큐시트 순서 줄

원본 (cue.js:951 `quietPreview`, control.js:473): 큐시트 목록에서 순서를 고르면 `holdDirect` 를 올린 채 미리보기한다. 그래서 '바로 송출' 모드여도 순서 줄을 클릭·두 번 클릭해서는 송출되지 않고, Enter(F9)나 본문 쪽 슬라이드를 눌러야 송출된다.

Rust 판: 바로 송출 모드에서 순서 줄 클릭 = 송출, 두 번 클릭 = 모드와 상관없이 송출.

**결정 (2026-11-15, 사용자 지적 "더블클릭하면 바로 켜지게 해야지" 에 따라): Rust 동작을 유지한다.** 안내 문구도 그 동작에 맞춰져 있다 (cue_ui.rs '클릭 = 바로 송출 (회중 화면에 바로 나감)').

그 밖의 의도적 차이:
- 이름표가 하나도 없는 송출(성경 등)에서 숫자·C·B·E·P 를 눌러도 "맞는 부분이 없습니다" 알림을 띄우지 않는다 (원본은 띄움).
- 검색 후보는 드롭다운이 아니라 본문 목록에 보인다. 창은 여러 개 떠 있는 창이라 바깥을 눌러도 닫히지 않는다. 파일 고르기는 윈도우 파일 창을 쓴다.

## 3. 이벤트 처리기 전체 대조

원본 11개 파일(control · cue · library · grid · slideedit · ctxmenu · cuemanager · dropfiles · paste · quicktext · menubar)에서
`.onclick =` 꼴과 `addEventListener(…)` 를 모두 뽑은 것이 282개다 (`tools/behavior-parity/handlers.js`).
한 처리기가 여러 단추를 맡는 경우(편집칸 단추 20종 등)는 한 줄로 세고 설명에 적었다. 표와 수치는 `tools/behavior-parity/table.js` 가 만든다 (빠지거나 겹치면 오류).

| 원본 (파일:줄) | 처리기 수 | 무엇을 하면 무엇이 되나 | 상태 | Rust 자리 · 시험 |
|---|---|---|---|---|
| control.js:42·43 | 2 | 화면 오류·약속 거부를 로그에 | 해당 없음 | Rust 는 panic hook · logger.rs |
| control.js:144 | 1 | 알림을 누르면 닫힘 | 같음 (소스 근거만) | Act::ToastDismiss |
| control.js:150·151·168·187·197·202·207·218·220·221 | 10 | 코어 → 화면 SSE 사건 (state·settings·update·message·obs·outputs·remote·songs·cues·draw) | 해당 없음 | 같은 프로세스라 Snap 으로 바로 읽음 |
| control.js:324·325·326·338 | 4 | 처음 실행 안내 (베들레헴 가져오기 · 나중에) | 같음 (소스 근거만) | firstrun · import_ui |
| control.js:365·369·370 | 3 | 성경 책 창: 책 누름 · 거르기 · Enter = 첫 책 | 같음 (소스 근거만) | cue_ui.rs 성경 책 탭 |
| control.js:377 | 1 | 장 단추 = 절 목록만 띄움 (미리보기는 절을 누를 때) | 같음 (소스 근거만) | cue_ui.rs get_chapter |
| control.js:390 | 1 | 장 격자에서 방향키로 옮기기 | 고침·새로 만듦 (합성 시험) | nav_tests: 성경 장 격자 방향키 |
| control.js:488 | 1 | 클릭하면 [미리보기][바로 송출] 모드 단추 | 같음 (소스 근거만) | Act::TogglePreviewMode |
| control.js:569 | 1 | 미리보기 화면의 글상자 누름 → 다시 그림 | 해당 없음 | egui 는 매 프레임 그림 |
| control.js:606 | 1 | 미리보기 아래 번호 칩 누름 = 그 장 미리보기 | 같음 (합성 시험) | dbl_preview_chip_moves_and_sends |
| control.js:611 | 1 | 번호 칩 두 번 = 그 장 송출 | 고침·새로 만듦 (합성 시험) | dbl_preview_chip_moves_and_sends (click2 로 바꿈) |
| control.js:615·616 | 2 | 미리보기 ◀ ▶ | 같음 (합성 시험) | preview_pane_menu_sends_and_live_pane_menu_hides_text (PreviewStep) |
| control.js:678·679 | 2 | 영상 조작 줄 (재생·멈춤·찾기) | 같음 (소스 근거만) | media_ui.rs video_bar |
| control.js:685 | 1 | LIVE 번호 칩 = 그 장으로 | 같음 (소스 근거만) | Act::LiveGoto |
| control.js:691·704·758 | 3 | 예배당/OBS 보기 · 본문/주석 탭 · 주석 보기 | 같음 (소스 근거만) | MonitorViews · commentary_ui |
| control.js:904 | 1 | 본문 줄 누름 = 미리보기 · Shift+누름 = 범위 | 같음 (합성 시험) | first_click_keeps_rows_in_place_everywhere 등 |
| control.js:917 | 1 | 본문 줄 두 번 = 미리보기하고 송출 | 고침·새로 만듦 (합성 시험) | dbl_search_result_row_previews_and_sends |
| control.js:924 | 1 | 본문 목록 ↑↓ · Shift+↑↓ 범위 · Enter · 맨 위에서 ↑ = 검색칸 | 같음 (소스 근거만) | center_ui.rs |
| control.js:979 | 1 | 검색칸: 치는 중에는 후보만 (미리보기·송출 안 바뀜) | 고침·새로 만듦 (합성 시험) | typing_reference_in_direct_mode_does_not_go_live_until_enter |
| control.js:1059·1066 | 2 | 후보 드롭다운 누름 · 칸을 떠나면 닫힘 | 다름 (화면 설계) | 후보를 드롭다운이 아니라 본문 목록에 보임 |
| control.js:1068 | 1 | 검색칸 Enter = 검색 · ↓ = 목록으로 · Esc = 모두 고르기 | 고침·새로 만듦 (합성 시험) | nav_tests: 검색칸 Esc 전체 선택·전역 Esc 충돌 방지 (입력 우선 의도차이) |
| control.js:1169·1170·1171·1172 | 4 | 아래 줄: 이전 · 다음 · 송출 · 글자 숨김 | 같음 (소스 근거만) | topbar_ui.rs bottom_bar |
| control.js:1174·1175·1176·1177·1178·1180 | 6 | 배경 · 오버레이 · 로고 · 음악 멈춤 · 모두 지우기 · 더보기 | 같음 (소스 근거만) | 지우기 ▾ 메뉴 + (새) 아래 줄·LIVE 오른쪽 메뉴 |
| control.js:1228 | 1 | 지금 화면 칩 (층마다 끄기) | 같음 (소스 근거만) | app.rs layer_action |
| control.js:1245·1246·1247 | 3 | 설정 단추 · 검색 방식 · 이동 방식 | 같음 (소스 근거만) | settings_ui · panel_ui |
| control.js:1291·1298·1308·1313·1314 | 5 | 빠른 설정: 대상 · 템플릿 · 접기 · 자세히 · 테마 | 같음 (소스 근거만) | quick_ui.rs · panel_ui.rs |
| control.js:1316·1320·1330 | 3 | 분할 방식 · 병행 번역 · 번역본 고르기 | 같음 (소스 근거만) | topbar_ui.rs · panel_ui.rs |
| control.js:1363·1370 | 2 | 송출 기록: 줄 누름 = 다시 띄움 · 모두 지우기 | 같음 (소스 근거만) | history_ui.rs |
| control.js:1364 | 1 | 송출 기록 목록 키 (↑↓ Enter) | 고침·새로 만듦 (합성 시험) | nav_tests: 기록 ↑↓·Enter 및 올바른 대상 재생 |
| control.js:1388·1389 | 2 | 창 닫기 단추 · 창 바깥 누르면 닫힘 | 다름 (화면 설계) | 떠 있는 창(여러 개) 방식 — 바깥 눌러 닫기 없음 |
| control.js:1399 | 1 | 예배당 화면 켜기·끄기 단추 | 같음 (소스 근거만) | Act::OutputToggle |
| control.js:1404 | 1 | 글꼴 다 읽으면 다시 그림 | 해당 없음 |  |
| control.js:1422 | 1 | 번호 칩 줄 접기 | 같음 (소스 근거만) | settings thumbsFold |
| control.js:1472 | 1 | 글칸에 들어가면 한글 입력으로 (focusin) | 같음 (소스 근거만) | trackd_ui::ime_on_focus · winkeys::ime_korean (실제 IME 전환은 미시험, 검색칸 제외 글칸) |
| control.js:1516 | 1 | 상태 줄 누름 = 출력 설정 | 같음 (소스 근거만) | widgets::status_btn |
| control.js:1619 | 1 | 전역 키: 단축키 · Esc 차례 · Alt+1~9 · 1~9/C/B/E/P 건너뛰기 · ←→ · Enter | 고침·새로 만듦 (합성 시험) | live_song_number_and_c_keys_jump_to_that_verse_and_chorus (새로) · ctrl_enter_* · custom_shortcuts_* |
| cue.js:109 | 1 | Ctrl+Z · Ctrl+Y (글칸·창 밖에서) | 같음 (소스 근거만) | Act::Undo |
| cue.js:227 | 1 | 진행 시계 시작 시각 | 같음 (소스 근거만) | cuemgr_ui.rs 시작 시각 칸 |
| cue.js:452·453 | 2 | 큰 PPT 를 넣은 뒤 "여러 순서로 나눌까요" 묻기 | 남음·확인 못 함 | 없음 (메뉴·단추로는 나누기 창을 열 수 있음) |
| cue.js:546·553·558·571·572·574·580·581 | 8 | PPT 나누기 창: 썸네일 누름·Shift·두 번, 범위 칸, 범위 더하기, 이름 Enter, 합치기·빼기, 이름 고치기, 모두 비우기 | 고침·새로 만듦 (합성 시험) | dbl_ppt_split_thumbnail_toggles_split_there_and_again_merges · ppt_split_click_start_shift_click_end_then_add_range_with_name · ppt_split_rule_button_merge_remove_clear_and_go_replaces_the_item |
| cue.js:621·649 | 2 | PPT 나누기: 규칙으로 · 나누기 | 고침·새로 만듦 (합성 시험) | ppt_split_rule_button_merge_remove_clear_and_go_replaces_the_item |
| cue.js:622·623·631·641 | 4 | PPT 나누기: AI · 채팅 프롬프트 복사 · 채팅 열기 · 답 불러오기 | 고침 (코어 시험·소스) | pptsplit_ui.rs (화면 시험 없음 — API·클립보드 필요) |
| cue.js:632 | 1 | 모아보기 그림 저장 | 같음 (소스 근거만) | hymns_ui.rs PPT 도구 칸 (새 창에는 단추 없음) |
| cue.js:791 | 1 | 인쇄 옵션 (메모·가사·본문) | 남음·확인 못 함 | 확인 못 함 |
| cue.js:792 | 1 | 인쇄 | 같음 (소스 근거만) | menubar b:print |
| cue.js:828 | 1 | 카메라 고르기 창 | 남음·확인 못 함 | 확인 못 함 |
| cue.js:916·917·918 | 3 | 음악 순서: 재생 · 멈춤 · 페이드아웃 | 같음 (소스 근거만) | music_ui.rs |
| cue.js:938 | 1 | 슬라이드 줄 누름 = 그 장 미리보기 (Shift·Ctrl = 여러 장) | 같음 (합성 시험) | first_click_keeps_rows_in_place_everywhere |
| cue.js:945 | 1 | 슬라이드 줄 두 번 = 그 장 송출 | 고침·새로 만듦 (합성 시험) | dbl_slide_row_sends_that_slide |
| cue.js:1102·1103·1104 | 3 | 편집칸 입력 · 고르기 · 테마 | 같음 (소스 근거만) | app.rs 편집칸 |
| cue.js:1105 | 1 | 편집칸 글칸에서 Esc = 칸 나가기 | 같음 (소스 근거만) | egui 기본 (Esc = 초점 놓기) |
| cue.js:1106 | 1 | 슬라이드쇼 그림 지우기 · 왼쪽으로 | 남음·확인 못 함 | 확인 못 함 |
| cue.js:1117·1127·1134 | 3 | 장비 줄 · 보내 보기 · 송출할 때 자동으로 | 같음 (소스 근거만) | actions_ui.rs |
| cue.js:1146·1153 | 2 | 부를 순서 저장·쓰기·지우기 · 이름표 칩 | 같음 (소스 근거만) | app.rs 편집칸 |
| cue.js:1161 | 1 | 편집칸 단추 20종 (닫기·위·아래·복제·지우기·PPT 나누기·요소 꺼내기·음악·OBS·장비·매크로·Companion 눌러 보기 …) | 남음·확인 못 함 | Companion 눌러 보기(actions_ui.rs)·그림 더하기(parity_ui.rs)는 소스로 확인. 나머지 단추 개별 UI 시험 미완료 |
| cue.js:1237·1246 | 2 | 편집칸 보이기 단추 · 그림 파일 고르기 | 같음 (소스 근거만) | toggleEditor · rfd |
| cue.js:1258 | 1 | 순서 줄 누름 = 미리보기 (바로 송출 모드여도 송출 안 함) | 다름 (의도) | 바로 송출 모드에서는 누르면 송출 — dbl_cue_row_in_direct_mode_sends_once_and_stays_on_that_item |
| cue.js:1266 | 1 | 순서 줄 두 번 = 미리보기만 | 다름 (의도) | 두 번 = 송출 (사용자 결정) — dbl_cue_row_sends_in_preview_mode |
| cue.js:1271 | 1 | 순서 목록 키: ↑↓ · Alt+↑↓ 옮기기 · Enter · Delete | 고침·새로 만듦 (합성 시험) | nav_tests: 순서 목록 ↑↓·Alt+↑↓·Delete·Enter, Undo |
| cue.js:1289·1290·1295·1310 | 4 | 순서 끌어 옮기기 · 라이브러리에서 끌어 놓기 | 같음 (소스 근거만) | cue_row_dnd · dnd payload |
| cue.js:1417·1418·1441 | 3 | 큐시트 고르기 · ＋ 넣기 · 왼쪽 탭 | 같음 (소스 근거만) | cue_ui.rs |
| cue.js:1498·1499·1559·1564 | 4 | 찬송가 폴더 빼기 · 읽는 방식 · 폴더 고르기 · 등록 | 같음 (소스 근거만) | hymns_ui.rs |
| cue.js:1511·1547·1597 | 3 | 온라인에서 찾기 · 지켜보기 멈춤 | 같음 (소스 근거만) | hymns_ui.rs · songinbox |
| cue.js:1515 | 1 | 찬송가 창 방식 (PPT / 베들레헴) | 같음 (소스 근거만) | hymns_ui.rs |
| cue.js:1548·1551·1552 | 3 | 찾기 주소 · 다운로드 폴더 · 받은 PPT 폴더 | 남음·확인 못 함 | 확인 못 함 |
| cue.js:1557·1558 | 2 | 찬송가 기본 배경 · 다른 이미지로 | 같음 (소스 근거만) | hymns_ui.rs default_bg_row |
| cue.js:1571 | 1 | 폴더로 거르기 | 남음·확인 못 함 | 확인 못 함 |
| cue.js:1573 | 1 | 찬송가 찾기 (치면 목록) | 같음 (소스 근거만) | hymns_ui.rs |
| cue.js:1574·1579 | 2 | 찬송가 목록 ↑↓·Enter · 한 번 = 고르기 | 고침·새로 만듦 (합성 시험) | hymn_search_arrow_and_enter_add_selected_row · dbl_library_hymn_row_adds_once_and_single_click_only_selects |
| cue.js:1580 | 1 | 찬송가 목록 두 번 = 넣기 | 고침·새로 만듦 (합성 시험) | dbl_library_hymn_row_adds_once_and_single_click_only_selects |
| cue.js:1586 | 1 | 큐시트 관리 창 열기 | 고침·새로 만듦 (합성 시험) | cue_list_empty_area_menu_adds_item_and_opens_cue_manager |
| library.js:30 | 1 | 라이브러리 탭 바꾸기 | 같음 (합성 시험) | dbl_library_* 시험들 |
| library.js:34·54 | 2 | 목록 키 (↑↓ Enter) · 검색칸 키 | 남음·확인 못 함 | 찬송가 탭은 시험 있음. 다른 탭은 확인 못 함 |
| library.js:53 | 1 | 라이브러리 찾기 | 같음 (소스 근거만) |  |
| library.js:62 | 1 | 끌어서 큐시트로 | 같음 (소스 근거만) | dnd_set_drag_payload |
| library.js:83 | 1 | 줄·칸 한 번 = 고르기 | 고침·새로 만듦 (합성 시험) | dbl_library_* (찬송가·음악은 한 번에 넣던 것 고침) |
| library.js:84 | 1 | 줄·칸 두 번 = 큐시트에 넣기 | 고침·새로 만듦 (합성 시험) | dbl_library_song_adds_it_once · …hymn… · …music… · …image… |
| library.js:98·99·100·116 | 4 | 찬양: 새 찬양 · 웹 가사 · 가져오기 · 고치기 | 같음 (소스 근거만) | trackb_ui.rs |
| library.js:123·145·170·171 | 4 | 찬양 가져오기 창 (파일 · PPT 에서 · 모두 고르기 · 가져오기) | 남음·확인 못 함 | 확인 못 함 |
| library.js:205·206·207·208·218·219 | 6 | 가사 찾기 창 | 같음 (소스 근거만) | lyrics_ui.rs |
| library.js:286·287·288 | 3 | 찬양 저장 · 저장하고 넣기 · 지우기 | 같음 (소스 근거만) | trackb_ui.rs |
| library.js:298·316·317·319·321 | 5 | 찬송가 설정 · 폴더 고르기 · 폴더 관리 · 올리기 · 폴더 등록 | 같음 (소스 근거만) | trackb_ui.rs · thumbs_ui.rs |
| library.js:333·334·336·337·342 | 5 | 그림·영상 칸 단추: 템플릿 배경 · 송출 배경 · 순서 배경 · 로고 · 재생 | 같음 (소스 근거만) | thumbs_ui.rs |
| library.js:358·359·378·383·384·385 | 6 | 카메라: 새로 고침 · 끄기 · 설정 · 순서로 · 순서 배경 · 템플릿 배경 | 같음 (소스 근거만) | panel_ui.rs · camscan.rs |
| library.js:382 | 1 | 카메라 송출 배경 (두 번 = 켜기·끄기) | 고침·새로 만듦 (합성 시험) | dbl_library_camera_toggles_live_background |
| library.js:439·448·454·455 | 4 | 카메라 설정 창 | 같음 (소스 근거만) | camera prefs |
| library.js:518·519·520 | 3 | 폴더 관리 창 | 같음 (소스 근거만) | add_lib_folder · remove_lib_folder |
| library.js:542·545·546·547 | 4 | 파일 고르기 창 (라이브러리에서) | 다름 (화면 설계) | 윈도우 파일 고르기 창(rfd)을 씀 |
| library.js:577·581·582 | 4 | 소리 재생기 내부 사건 | 해당 없음 |  |
| library.js:602·603·604·605·606·607 | 6 | 음악 재생 줄: 재생·멈춤·페이드·찾기·소리·이어 재생 | 같음 (소스 근거만) | music_ui.rs |
| library.js:636·645·646·647·673·674·675·676·691 | 9 | 오버레이: 새로 · 켜기 · 고치기 · 지우기 · 위치 · 크기 · 여백 · 그림 · 저장 | 같음 (소스 근거만) | trackb_ui.rs overlays_window |
| library.js:705·706·717·723·728 | 5 | 테마 탭: 테마 창 · 템플릿 편집 · 적용 · 순서에 · 편집 | 같음 (소스 근거만) | theme_ui.rs |
| grid.js:24·25·30 | 3 | 보기 바꾸기 · 썸네일 크기 · Ctrl+휠 | 같음 (소스 근거만) | center_ui.rs |
| grid.js:41·137 | 2 | 격자·전체 격자 칸 누름 = 미리보기 | 같음 (합성 시험) | first_click_in_all_view_does_not_scroll_the_clicked_cell_away |
| grid.js:50·151 | 2 | 격자·전체 격자 칸 두 번 = 송출 | 고침·새로 만듦 (합성 시험) | dbl_grid_cell_sends_that_slide · dbl_all_grid_cell_of_another_item_sends_it · dbl_all_list_line_sends_it |
| grid.js:52·150 | 2 | 격자 키 (방향키 · Enter · 여러 장 고르기 키) | 같음 (소스 근거만) | center_ui.rs msel_keys |
| slideedit.js:490·491 | 2 | 글 바로 고치기: 다른 곳 누름 = 저장 · Esc · Ctrl+Enter | 고침·새로 만듦 (합성 시험) | dbl_editor_main_text_edits_in_place_and_ctrl_enter_saves_without_sending · dbl_editor_text_box_edits_in_place_escape_cancels_click_away_saves |
| slideedit.js:1012 | 1 | 상자 두 번 = 글 바로 고치기 (잠긴 상자 제외) | 고침·새로 만듦 (합성 시험) | dbl_editor_* 3개 |
| slideedit.js:1011 | 1 | 요소 누름·끌기 (옮기기) | 같음 (소스 근거만) | translate_item |
| slideedit.js:549·550 | 2 | 요소 끌어 옮기기·크기 조절 손잡이 | 고침·새로 만듦 (합성 시험) | slideedit_canvas_tests: 손잡이 크기 조절·비율 유지·끌기 붙기 |
| slideedit.js:656·984·1152 | 3 | 여러 장 고르기 줄 · 목록 누름 (Shift·Ctrl) · 본문 목록 고르기 키 | 같음 (소스 근거만) | slideedit_ui.rs · msel_keys |
| slideedit.js:694·699·721·993·999·1010 | 6 | 끌어서 여러 장 고르기 (올가미) | 다름 (의도) | slideedit_canvas_tests: 원본은 목록 그냥 끌기, Rust는 Shift+끌기 (기존 차례 옮기기와 구분) |
| slideedit.js:692·1061 | 2 | 썸네일 끌림 막기 · 창 크기 바뀜 | 해당 없음 |  |
| slideedit.js:912 | 1 | 속성 칸 단추들 (정렬·굵게·겹침 차례·잠금·복제 …) | 같음 (소스 근거만) | 원본 잠금은 공통 요소 제한. 본문 글 앞/뒤는 기존 글 뒤 체크와 같음. slideedit_canvas 공통 요소 제한 시험 |
| slideedit.js:955·965 | 2 | 속성 칸 입력 | 같음 (소스 근거만) | slideedit_ui.rs |
| slideedit.js:1013 | 1 | 편집기 키: Enter·F2 = 글 고치기 · Delete · 방향키 이동 · Ctrl+C/V/D | 고침·새로 만듦 (합성 시험) | slideedit_canvas_tests: Enter·F2·Delete·방향키·Ctrl+D 및 텍스트 붙여넣기 |
| slideedit.js:1052 | 1 | 편집기 창 키: Esc 닫기 · Ctrl+Z | 고침·새로 만듦 (합성 시험) | slideedit_canvas_tests: Esc 단계별 처리·Undo |
| ctxmenu.js:245 | 1 | 오른쪽 클릭 메뉴 9곳 (순서 · 빈 자리 · 슬라이드 · 전체 격자 · 본문 줄 · 미리보기 · LIVE · 라이브러리 · 아래 줄) | 고침·새로 만듦 (합성 시험) | verse_menu_* · slide_row_and_grid_cell_menu_send_now · library_menu_add_to_cue_for_song_hymn_music_image · preview_pane_menu_sends_and_live_pane_menu_hides_text · cue_list_empty_area_menu_adds_item_and_opens_cue_manager · cue_row_menu_note_is_saved_on_the_item |
| ctxmenu.js:31 | 1 | 메뉴 항목 누름 = 실행하고 닫힘 | 같음 (합성 시험) | 위 메뉴 시험들 |
| ctxmenu.js:254 | 1 | 메뉴 바깥 누르면 닫힘 | 같음 (소스 근거만) | egui 기본 |
| ctxmenu.js:258 | 1 | 메뉴 키 (↑↓ Enter Esc) | 남음·확인 못 함 | 확인 못 함 |
| ctxmenu.js:30·38·255·256·257 | 5 | 초점 유지 · 마우스 올림 강조 · 창 흐림/크기/스크롤 때 닫기 | 해당 없음 | egui 메뉴가 맡음 |
| cuemanager.js:162·173·174·175 | 4 | 큐시트 관리: 단추 전부 · 줄 두 번 = 열기 · 찾기 · 이름/날짜/메모/고정/서식/정렬/고르기 | 고침·새로 만듦 (합성 시험) | dbl_cue_manager_row_opens_that_cue_and_single_click_only_selects 외 cue_manager_* 5개 |
| cuemanager.js:130 | 1 | 큐시트 파일 가져오기 (여러 개) | 고침 (코어 시험·소스) | cuemgr.rs import_cue_files — 파일 고르기 창은 시험 못 함 |
| dropfiles.js:123 | 1 | 파일을 끌어 놓으면 종류에 맞게 넣기 | 같음 (소스 근거만) | hymns_ui.rs dropped_files |
| dropfiles.js:120·121·122 | 3 | 끄는 동안 "여기에 놓으세요" 겹침 안내 | 남음·확인 못 함 | 확인 못 함 |
| paste.js:199·201 | 2 | Ctrl+V 붙여 넣기 · Ctrl+Shift+V 종류 골라 | 같음 (소스 근거만) | hymns_ui.rs paste · parity2_ui.rs |
| quicktext.js:79·80·81·82·89 | 5 | 즉석 자막: 닫기 · 송출 · 큐시트에 · Enter/Shift+Enter/Esc · 최근 목록 | 고침·새로 만듦 (합성 시험) | quick_text_* 3개 |
| quicktext.js:132 | 1 | 즉석 자막 단추 | 같음 (소스 근거만) | bottom_bar |
| quicktext.js:130 | 1 | 창 크기 바뀌면 자리 다시 | 해당 없음 |  |
| menubar.js:201·222 | 2 | 메뉴줄 누름 · 바깥 누르면 닫힘 | 같음 (소스 근거만) | menubar.rs |
| menubar.js:211·223 | 2 | 열린 채 옆 메뉴로 넘어가기 · Alt·방향키 | 남음·확인 못 함 | 확인 못 함 |
| menubar.js:221 | 1 | 하위 메뉴 닫힘 늦추기 | 해당 없음 |  |

| 상태 | 처리기 수 |
|---|---|
| 같음 (합성 시험) | 9 |
| 같음 (소스 근거만) | 158 |
| 고침·새로 만듦 (합성 시험) | 45 |
| 고침 (코어 시험·소스) | 5 |
| 다름 (의도) | 8 |
| 다름 (화면 설계) | 8 |
| 남음·확인 못 함 | 22 |
| 해당 없음 | 27 |
| **합계** | **282** |

읽는 법:
- 표는 2026-11-16 통합 점검 결과로 갱신했다. '소스 근거만'은 실제 조작이 같은지 보장하지 않는다.
- 미확인 항목은 남겨 두었다. 큰 PPT를 넣은 직후 나눌지 묻기, 인쇄 옵션, 일부 가져오기·메뉴 키·끌기 안내 등은 별도 확인이 필요하다.
- 편집기 공통 요소의 수정 제한은 원본의 '잠금' 동작이다. 원본에 없는 잠금 단추를 미구현이라고 세었던 앞 보고를 정정한다.
- 슬라이드 목록 올가미는 기존 차례 옮기기와 구분해 Shift+끌기다. Ctrl+C/X 요소 복사는 원본 대비 추가 기능이고, 그림 붙여넣기 등은 아직 미구현이다.
- 참고 문서: navigation-parity.md. 편집기 글쇠가 메인 화면의 입력을 가로채지 않는지 slideedit_focus_tests의 App::ui_body 전체 합성 시험 3개로 확인했다.

## 4. 원본 e2e 시험(tests/e2e/smoke.js 62개 확인) 가운데 이 갈래 범위

| 원본 확인 | Rust 판 | 시험 |
|---|---|---|
| 성경 검색 → 미리보기 · 송출 (F9) · 모두 지우기 | 있음 | `typing_keeps_current_cue_preview_and_enter_replaces_it` · `ctrl_enter_send_now_searches_then_sends` |
| 즉석 자막: Ctrl+T 로 열고 Esc 로 닫기 · Enter 로 바로 송출 · 최근 목록 | 있음 | `quick_text_ctrl_t_opens_enter_sends_clears_and_remembers` · `quick_text_shift_enter_is_newline_and_recent_click_resends` |
| PPT 나누기 창 (슬라이드 · Shift 범위 · 다음 시작 번호) | 새로 만듦 | `ppt_split_click_start_shift_click_end_then_add_range_with_name` |
| PPT 나누기 → 순서 2개 · 범위만 송출 | 새로 만듦 (순서 바뀜까지 시험. 범위만 송출은 기존 코어) | `ppt_split_rule_button_merge_remove_clear_and_go_replaces_the_item` |
| PPT 자동으로 나누기 (규칙) | 있음 | 같은 시험의 '규칙으로' 단추 |
| PPT 나누기: 두 번 눌러 새 순서 · 다시 두 번 눌러 합치기 | 새로 만듦 | `dbl_ppt_split_thumbnail_toggles_split_there_and_again_merges` |
| PPT AI 나누기 → 고친 이름으로 순서 3개 | 화면은 있음 | 시험 없음 (API 키 필요) |
| 끌어서 차례 바꾸기 · Ctrl+Z | 있음 (앞 작업) | `ui_integration_tests.rs` (이 갈래에서 다시 확인하지 않음) |
| 구역 나누기 (오른쪽 클릭 → 새 순서) | 있음 | 메뉴 항목까지만 (`slide_row_and_grid_cell_menu_send_now` 는 송출 항목) |
| 붙여 넣기 · Ctrl+휠 썸네일 크기 · 큐 편집 실행 취소 | 소스 근거만 | 이 갈래에서 시험 안 함 |

나머지 e2e 확인(교독문 역할, 연결 점검, 새 버전 창, OBS 원격, 무대 화면, PPT 요소 꺼내기, 장비·Companion, 회중 메시지, 매크로, 인쇄, 주보, 설교 포인트, 카메라, 영어 화면, 사용 기록, 동기화)과
`tests/core.test.js` 74개는 다른 갈래(A·B·D·E) 범위라 여기서 대조하지 않았다.

## 5. 이 갈래에서 고치거나 새로 만든 것

- `widgets::click2` 항목 키 두 번 클릭 판정 (모든 두 번 클릭 자리).
- 라이브러리 찬송가·음악 탭: 한 번에 넣던 것을 한 번 = 고르기, 두 번 = 넣기로. 카메라 두 번 = 송출 배경.
- 큐시트 관리 창 (`cuemgr_ui.rs` · `wp-core/cuemgr.rs`): 목록 · 찾기 · 보기(전체·고정·서식·휴지통) · 정렬 · 모두 고르기 · 새 큐시트(기본 순서)·빈 큐시트·서식에서 · 이름/날짜/메모/고정/서식 · 다음 주로 복제 · 그대로 복제 · 순서지 복사 · 내보내기 · 휴지통·되살리기·아주 지우기.
- PPT 나누기 창 (`pptsplit_ui.rs` · `wp-core/splitedit.rs`).
- 편집기 글 바로 고치기 (`slideedit_ui.rs` · `wp-core/slideedit.rs` set_main_text).
- 오른쪽 클릭 메뉴: 본문 줄 · 라이브러리 · 격자 칸 · 미리보기 화면 · LIVE 화면 · 큐시트 빈 자리 · 아래 줄 · 순서 메뉴의 슬라이드 편집/메모/테마/이 다음에 넣기.
- 결함: 순서 메뉴 안의 '제목' 글칸을 누르면 메뉴가 닫혀 제목을 바꿀 수 없던 것.
- 송출 중 1~9 · C · B · E · P 로 절·후렴·브릿지·엔딩·전주 건너뛰기 (원본 jumpKey, Rust 에 없었음).
- 검색칸: 치는 중에는 후보만, Enter 때 미리보기·송출. Ctrl+Enter = 검색해 송출. 즉석 자막 글쇠.

## 6. 남은 일

1. 3절 표의 미확인 22개를 확인하고 없는 것은 구현.
2. '같음 (소스 근거만)' 156개를 영역별로 합성 시험으로 옮기기 (큐시트 → 검색 → 라이브러리 → 미디어 → 무대 → 설정 → 끌어 놓기).
3. 편집기 기본 요소 선택·손잡이·이동·복제·텍스트 붙여넣기는 보완했다. 그림/파일 붙여넣기, 일부 이동 글쇠·오른쪽 메뉴, 목록 올가미 자동 스크롤은 남아 있다.
4. 실제 창에서 사람이 눌러 보는 확인은 하지 않았다 (합성 시험과 `--shot` 그림만).
