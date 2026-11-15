# 키보드 이동 대조 (navigation)

2026-11-16. `docs/behavior-parity.md` 에서 '남음·확인 못 함' 으로 남았던 키보드 이동 네 가지를
원본 Electron 판(`F:\aside\WorshipCue\src\web\control`)과 줄 단위로 대조하고, 빠진 것을 구현했다.
총표(`behavior-parity.md`)는 이 문서를 보고 나중에 갱신한다.

범위: `cue_ui.rs` · `topbar_ui.rs` · `history_ui.rs` · `app.rs`(키 분배만) · 새 시험 `nav_tests.rs`.
슬라이드 편집기, 설치·자료 이전 코드는 건드리지 않았다.

## 결과 요약

| # | 원본 | 동작 | 고치기 전 Rust | 지금 | 시험 |
|---|---|---|---|---|---|
| 1 | control.js:1077 | 검색칸 Esc = 내용 모두 고르기 | 없음 (Esc = 초점 놓기, egui 기본) | 구현 | `nav_search_esc_*` 2개 |
| 2 | cue.js:1271 | 순서 목록 ↑↓ · Alt+↑↓ · Enter · Delete | 없음 (Ctrl+↑↓ · F2 만) | 구현 | `nav_cue_*` 6개 |
| 3 | control.js:390 | 장 격자 방향키 | 없음 | 구현 | `nav_chapter_grid_*` 2개 |
| 4 | control.js:1364 | 송출 기록 ↑↓ · Enter | 없음 (마우스만) | 구현 + 다시 띄우는 대상 고침 | `nav_history_*` 4개 |

네 가지 모두 Rust 판에 없던 것이라 겹쳐 만든 것은 없다. 본문(절) 목록의 ↑↓·Enter 는 이미 있던
`center_ui.rs list_keys` 를 그대로 두었다.

## 키보드를 가진 목록 (공통 틀)

원본은 DOM 초점으로 어느 목록이 키를 받을지 가른다. egui 에서는 누른 줄이 초점을 갖지 않으므로
`cue_ui.rs` 에 `Nav { Cue, Chapters, History }` 를 두고 한 번에 하나만 키보드를 갖게 했다.

- 갖기: 그 목록(줄·빈 자리·장 단추·기록 창)을 누름, 또는 기록 창을 엶.
- 놓기: 밖을 누름 · 글 쓰는 칸에 초점이 감 · 창이 닫힘 · 목록이 두 패스 넘게 그려지지 않음(탭 바꿈·패널 접기).
- 글 쓰는 중(`egui_wants_keyboard_input`)이면 주인은 언제나 없음으로 본다.
- 메뉴(오른쪽 클릭 메뉴 등 egui Popup)가 열려 있으면 목록은 키를 받지 않는다.
- 같은 프레임에 두 번 처리되지 않게: 목록은 눌림을 먹고(`consume_key`), 전역 단축키(`app.rs shortcuts`)는
  목록이 키보드를 가진 동안 Enter(= 미리보기 송출)를, 장 격자·기록 창이면 ←→(= 미리보기 앞뒤)도 건너뛴다.
  원본 control.js:1654·1660 의 `#chapterGrid` · `#cueList` · 모달 제외와 같은 규칙이다
  (원본도 `#cueList` 는 Enter 만 빼고 ←→ 는 빼지 않는다).

## 1. 검색칸 Esc

- 원본 `#q keydown`: 후보 목록이 열려 있으면 닫고, 아니면 `$('#q').select()`.
- Rust: 검색칸이 Esc 를 직접 받게 하고(`event_filter.escape`) 눌리면 내용을 모두 고른다. 초점은 남는다.
  Rust 판에는 후보 목록 상자가 따로 없어(결과가 본문 목록에 바로 보임) '후보 닫기' 단계는 없다.
- 전역 Esc(보조 화면 끄기)와의 관계: 글 쓰는 중에는 전역 Esc 가 원래도 불리지 않았고(`safe_while_typing`),
  이제 Esc 가 초점을 놓지 않으므로 검색칸 안에서는 몇 번을 눌러도 보조 화면이 꺼지지 않는다.
  보조 화면을 Esc 로 끄려면 검색칸 밖(목록·빈 곳)을 누른 뒤 누른다.
- **원본과 다른 점 (알고 둔 것)**: 원본은 전역 처리기가 먼저(capture) 돌아서, 기본 설정(outputOff = Escape)이면
  검색칸 안의 Esc 도 보조 화면을 끄고 `select()` 까지 가지 못한다. 즉 원본의 '모두 고르기' 는 outputOff 단축키를
  다른 키로 바꿨을 때만 보인다. 이번 지시("글 입력 중 우선")에 따라 Rust 판은 글 쓰는 칸이 먼저 받는다.
- 시험: `nav_search_esc_selects_all_and_keeps_focus` (Esc 뒤 초점 유지, 다음 글자가 전체를 바꿈),
  `nav_search_esc_does_not_fire_global_escape_shortcut_while_typing` (Esc 에 묶은 전역 단축키가 칸 안에서는 안 불리고, 칸 밖에서는 한 번 불림).

## 2. 큐시트 순서 목록

원본 `list keydown` (Ctrl·Meta 가 눌렸으면 아무것도 안 함):

| 키 | 원본 | Rust |
|---|---|---|
| ↑ ↓ | `st.sel` 만 ±1 (끝에서 멈춤), 편집칸 다시 그림. 미리보기·송출 없음 | 같음. 고른 줄이 보이게 스크롤 |
| Alt+↑ Alt+↓ | 고른 순서를 한 칸 옮김, 끝이면 아무 일 없음 | 같음 (`Act::MoveItem`, 되돌리기 가능) |
| Enter | 고른 순서가 이미 미리보기면 `sendPreview`, 아니면 `quietPreview` | 같음. 바로 송출 모드에서도 첫 Enter 는 송출하지 않음 |
| Delete | `confirm()` 으로 묻고 지움, 고른 자리는 `min(i, len-1)` | 묻지 않고 지우고 알림에 [되돌리기] (아래) |

- 마우스 동작은 그대로다: 바로 송출 모드에서 줄을 누르면 송출, 두 번 누르면 송출 (원본은 `quietPreview` — 의도적 차이 유지).
- **원본과 다른 점**: Delete 는 `confirm()` 창 대신 Rust 판의 다른 지우기(오른쪽 클릭 메뉴·편집칸 휴지통)와 같은 길을 쓴다 —
  바로 지우고 "'○○' 순서를 삭제했습니다 [되돌리기]" 알림, Ctrl+Z 로도 되살아난다. 지운 뒤 고른 자리는 원본처럼 같은 번호(끝이면 새 끝).
- 바로 송출 모드의 조용한 미리보기는 `core.item_deck` 으로 미리보기만 바꾼다. 찬송가 PPT 를 아직 만드는 중이면
  알림만 뜨고 '다 만들면 띄우기' 예약은 하지 않는다(미리보기 모드에서는 기존 `preview_item` 길이라 예약됨).
- 시험: `nav_cue_arrows_only_select_and_enter_previews_then_sends_once`,
  `nav_cue_enter_is_quiet_in_direct_mode_but_click_still_sends`,
  `nav_cue_alt_arrows_move_one_step_and_undo_restores`,
  `nav_cue_delete_removes_one_keeps_position_and_is_undoable`,
  `nav_cue_keys_do_nothing_while_typing_or_after_clicking_elsewhere`,
  `nav_cue_keys_are_blocked_while_row_menu_is_open`.

## 3. 성경 장 격자

- 원본: 초점 있는 장 단추에서 → +1 · ← −1 · ↓ +한 줄 · ↑ −한 줄, 범위를 넘으면 첫·끝 장에 멈춤. 초점만 옮긴다.
  Enter 는 단추의 기본 동작(누름)이라 그 장을 연다. 장을 열면 검색칸에 '시23' 을 넣고 절 목록만 띄우며
  (미리보기는 절을 눌렀을 때), 초점은 절 목록으로 가지 않고 장 단추에 남는다.
- Rust: 장 단추를 누르면 그 장이 초점을 갖고(강조 테두리), 방향키·Enter 가 위와 같이 동작한다. 한 줄의 칸 수는 실제로 그린 격자의 열 수.
  장을 연 뒤에도 키보드는 격자에 남아 방향키로 옆 장을 이어서 열 수 있다. 미리보기가 올라가 있어도 ←→·Enter 가 미리보기 이동·송출로 새지 않는다.
- 책을 바꾸거나 책 찾기 칸에 글을 쓰면 초점은 풀린다.
- 시험: `nav_chapter_grid_arrows_move_focus_and_enter_opens`, `nav_chapter_grid_keys_need_focus`.

## 4. 송출 기록

- 원본 `openHistory`: 열면 첫 줄을 고르고 목록에 초점. ↑↓ 로 고르고 Enter 또는 줄 누름 = `load(i)` —
  창을 닫고 `searchReference({ query: 기록된 제목, versionId: 기록된 번역본 })`, 검색칸에 기록된 검색 글을 넣고 `loadPassage`
  (바로 송출 모드면 송출). 찾지 못하면 오류 알림.
- Rust:
  - 열면 첫 줄 · ↑↓ · Enter = 다시 띄우고 창 닫기. 검색칸 글과 절 목록도 그 본문으로 바뀐다.
  - 다시 띄우는 대상을 원본처럼 **기록된 제목 + 기록된 번역본**으로 찾는다. 전에는 기록의 검색 글(query)을 지금 번역본으로 찾았고,
    query 가 빈 줄(큐시트의 찬양·제목 순서)은 눌러도 아무 반응이 없었다. 이제 찾지 못하면 "○○: …" 알림이 뜬다.
  - 큐시트 순서에서 나간 기록이어도 그때의 순서 번호를 쓰지 않고 본문 자체를 띄운다(cueId 없음). 그 뒤 큐시트 순서가 바뀌거나
    지워져도 엉뚱한 순서가 나가지 않는다.
  - 찾지 못한 줄에서 Enter 를 눌러도 미리보기에 올려 둔 다른 것이 송출되지 않는다(전역 Enter 가 겹치지 않음).
- **원본과 다른 점 (그대로 둔 것)**: 줄을 마우스로 누를 때는 창을 닫지 않는다 — Rust 판은 '두 번 누르면 송출' 이 있어서다.
  Enter 는 원본대로 닫는다. 기록 창은 모달이 아니라 떠 있는 창이라, 창 밖을 누르면 키보드를 놓는다.
- 시험: `nav_history_arrows_and_enter_replay_the_recorded_passage_not_a_cue_index`,
  `nav_history_replay_in_direct_mode_sends_the_passage_itself`,
  `nav_history_enter_on_non_bible_row_sends_nothing_and_keeps_old_preview`,
  `nav_history_click_and_double_click_use_recorded_title`.

## 남은 점

- 검색칸 Esc: 위에 적은 원본과의 차례 차이(원본 기본 설정에서는 Esc 가 보조 화면을 먼저 끔). 사용자가 원본 차례를 원하면
  `shortcuts` 에서 outputOff 를 글 쓰는 중에도 받게 바꾸면 되지만, 그러면 '모두 고르기' 는 원본처럼 사실상 보이지 않는다.
- 순서 목록 Delete 의 확인 창: 원본 `confirm()` 대신 [되돌리기]. 확인 창이 필요하면 줄 안 확인(`widgets::confirm_*`)으로 바꿀 수 있다.
- Tab 으로 목록에 들어가는 것은 없다(누르거나 창을 열어야 키보드를 가짐). 원본은 Tab 으로도 `#cueList`·장 단추에 초점이 간다.
- 기록 창을 검색칸에 글을 쓰는 중에 Ctrl+H 로 열면, 검색칸이 초점을 쥐고 있어 기록 목록이 바로 키를 받지 않는다(창을 한 번 눌러야 함).
- 송출 기록에서 지워진 번역본으로 나갔던 줄은 다시 띄우지 못하고 알림이 뜬다(원본과 같음). 지금 번역본으로 대신 띄우지는 않는다.
- 합성 입력 시험은 화면 함수를 headless 로 그려 본 것이다. 실제 창에서 손으로 눌러 보는 확인과 `tools\parity\run.ps1` 그림 비교는 하지 않았다
  (그림은 장 단추·기록 줄의 초점 테두리만 새로 생김).
- `track_c.rs` 는 고치지 않았다. 기록 창이 다시 띄운 본문은 `HistoryUi.loaded` 에 두고 `app.rs take_history_loaded` 가 검색칸에 반영한다.
