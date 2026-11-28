# 창 · 대화상자 다듬기 (craft-windows, 2026-11-28)

F2 작업 기록. 주 화면 밖의 창 전부를 같은 짜임 · 같은 부품 · 같은 말로 맞춘다.
디자인 체계(F1, `docs/design-system.md` · `theme.rs` · `widgets.rs` · `kit.rs`) 위에, 창에만 필요한 짜임은 `crates/wp-app/src/widgets_win.rs` 에 둔다.

캡처: `node tools/craft/windows-shots.js --exe target\debug\wp-app.exe --seed tmp\craft\seed --cue tmp\craft\cue.json --out tmp\craft\after [--sets base,sizes,en,themes,states] [--only id,…]`
(격리 실행: 경우마다 seed 를 새로 복사, `WP_LEGACY_DATA` = 빈 폴더, `COMPUTERNAME=TEST-PC`, 주 템플릿은 로고 없는 것. 자기가 띄운 PID 만 끈다.)
훑어보기: `python tools/craft/grid.py <폴더> <접두> 이름…` (6장씩 한 장), `python tools/craft/sheet.py <폴더> <출력> 이름…`.

## 창 규칙 (widgets_win)

| 무엇 | 규칙 | 부품 |
|---|---|---|
| 처음 크기 | 내용 높이에 맞춰 연다. 한도는 창이 놓인 자리에서 화면 아래 끝까지(넘으면 창 안에서 스크롤). 칸을 창 높이로 나누는 편집기(`vscroll(false)`)만 정한 높이 | `win::window` |
| 처음 자리 | 가운데 위쪽(왼쪽 큐시트는 비킴), 이미 열린 창 수만큼 비껴 놓기. 화면 폭 55% 넘는 창은 가운데. 메뉴 막대를 덮지 않음 | `win::window` |
| 진행 중에 여는 창 | 즉석 자막 · 무대 화면 · 이름 자막 · 회중 메시지 · 오버레이 · 매크로 · 그리기는 뷰어(다음 · 송출 중) 밖에 뜬다: 뷰어 아래 → 왼쪽 → 오른쪽 | `.live(ctx)` + `note_viewers` |
| Esc | 맨 위 창 하나를 닫는다. 닫을 창이 없을 때만 전역 단축키(예배당 화면 끄기)로 간다. 편집기처럼 바깥을 눌러도 안 닫히는 창(`keep_on_outside`)은 Esc 로도 닫지 않음 | `win::esc_first` |
| Enter | 창이 열려 있는 동안 주 화면의 'Enter = 미리보기 송출' 을 막는다. 글칸의 Enter 는 그 칸이 받음(즉석 자막 송출 · 무대 메시지 보내기 · 사용 기록 등) | `win::modal_open` |
| 아래 단추 줄 | 위에 얇은 선, 주 단추(Primary) 하나가 맨 오른쪽, 취소 · 닫기는 그 왼쪽. 위험한 것 · 보조는 왼쪽 | `win::footer` · `footer_split` |
| 위험한 동작 | 채우지 않은 단추(`Kind::Danger`) + 한 번 더 묻기(`widgets::confirm_*`) | `win::danger` |
| 글 | 맨 위 한 줄 설명 `lead`, 조절 칸 아래 `hint`, 구획 머리 `section`(= `widgets::group_head`), 양식 줄 `field`(= `widgets::form_row`) | |
| 상태 | 결과 글은 아이콘 + 색(성공 = 회색 체크, 오류 = 주황, 확인 = 깃발), ✓ · ✗ 기호를 화면에 내지 않음. 빈 상태 `empty`(디자인 체계 `widgets::empty`), 하는 중 `busy` | `win::status` · `message` · `empty` |
| 고른 것 | 전문가 테마에서 노랑을 쓰지 않는다: 목록 고른 줄 = `sel` 바탕 + 회색 막대, 색 견본 · 캔버스 고른 칸 · 편집 안내선 = `theme::pick()`(밝은 무채색) | `win::list_row` · `theme::pick` |

## 창 목록

상태: **다듬음** = 이번에 짜임 · 크기 · 글 · 상태를 손보고 화면에서 확인 / **공통만** = 공통 규칙(크기 · 자리 · Esc · 고른 것 색)만 받고 안쪽은 그대로 / **코드만** = 점검 인자가 없어 화면 캡처 없이 코드로만 봄.

| 창 | 여는 법 | 점검 인자 (캡처 이름) | 상태 |
|---|---|---|---|
| 설정 (모든 구획 · 찾기) | 파일 → 설정 · 위 막대 톱니 | `--open-menu c:settings` · `--settings-cat <구획>` · `--settings-q <말>` (settings · set-*) | 공통만 + 찾은 줄 초점색 회색 |
| 출력 템플릿 편집 | 화면 → 출력 템플릿 · 인스펙터 '자세히' | `--tpl-open main\|overlay` (tpl · tpl-overlay) | 다듬음: 고른 요소 테두리 유지(Codex), 색 견본 · 정렬 칸 고름 노랑 → 무채색, 목록에서 고친 템플릿이 보이게 스크롤, 좁은 창은 탭 |
| 템플릿 목록(관리) | 화면 → 템플릿 | `--open-menu c:templates` (templates) | 공통만 |
| 출력 테마 관리 | 화면 → 테마 관리 | `--open-menu c:themes` (themes) | 공통만 |
| 큐시트 관리 | 큐시트 → 큐시트 관리 | `--item 1 --open-menu b:cues` (cues) | 다듬음: 첫 줄 가림 고침(Codex: `ui.put` → `win::place`), 상세 칸이 옆으로 늘어서 창을 밀어내던 것 고침, 좁은 창 탭 |
| 슬라이드 편집 | 편집 → 슬라이드 편집 (Ctrl+E) | `--item 1 --open-menu c:slideedit` (slideedit) | 공통만 + 고른 슬라이드 · 손잡이 · 안내선 노랑 → 무채색 |
| 찬양 라이브러리 | 큐시트 → 찬양 라이브러리 | `--open-menu b:songs` (songs) | 공통만 |
| 그림 · 영상 · 음악 라이브러리 | 큐시트 → 라이브러리 | `--open-menu b:lib` (lib) | 다듬음: 빈 상태를 목록 자리 위에, '배경 미디어 끄기' 를 아래 줄로(배경이 없으면 꺼짐) |
| 라이브러리 폴더 | 라이브러리 '폴더…' | — | 코드만 |
| 찬송가 · PPT 넣기 / PPT 도구 | 순서 넣기 펼침 | — | 코드만 (공통 규칙은 받음) |
| PPT 여러 순서로 나누기 · 큰 PPT 묻기 | PPT 순서 · 가져올 때 | — | 코드만 |
| 성경 번역본 | 파일 → 성경 번역본 | `--open-menu c:import-bible` (import-bible) | 공통만 |
| 주석 | 보기 → 주석 | `--open-menu c:commentary` (commentary) | 공통만 |
| 화면 구성 편집기 | 화면 → 화면 구성 편집… | `--scene-open [pip]` (scene · scene-empty) | 다듬음: 칸 목록의 고른 줄 아이콘 밝게 · 잠금 아이콘 구분, 미리보기 카메라 · 웹 칸의 둥근 모서리를 바탕까지(전에는 테두리만 둥글었음), 이름 · 틀 · 미리 보기 줄 줄바꿈, '이 구성으로 바뀔 때' 를 양식 줄로, 좁은 창 탭 (Codex) |
| 출력 설정 (예배당 · OBS · 추가 출력) | 화면 → 출력 설정 | `--open-menu d:output` (output) | 다듬음: 모니터 이름 '모니터 2 · 오른쪽 3840×2160'(장치 이름은 마우스 올림), 모니터 없을 때 안내, 넓은 창 가운데 |
| 카메라 설정 | 연결 → 카메라 | `--open-menu b:cameras` (cameras) | 공통만 |
| 카메라 동시 열기 시험 | 연결 → 카메라 동시 열기 시험 | `--open-menu d:camtest` (camtest) | 공통만 |
| 휴대폰 리모컨 · 스트림덱 · 장비 · 내부망 · 휴대폰 카메라 · 웹 편집기 | 연결 메뉴 | `--tool remote\|trigger\|devices\|lan\|phonecam\|webedit` (tool-*) | 공통만 |
| OBS 원격 조작 | 연결 → OBS | `--open-menu d:obs` (obs) | 공통만 |
| 무대 화면 | 송출 → 무대 화면 | `--open-menu b:stage` (stage) | 다듬음: 본문은 F1 의 인스펙터 무대 탭 양식(같은 함수), 여기에 지금 고정한 것 표시 · 고정 없으면 '풀기' 꺼짐 · 메시지 Enter 보내기 · 오류 글, 창은 뷰어 밖 |
| 즉석 자막 | 송출 → 즉석 자막 (Ctrl+T) | `--open-menu b:quick` (quick) | 다듬음: '송출 화면(예배당 · 방송)에 곧바로' 한 줄, [송출] · [내리기](앞에 송출하던 화면으로, 기록에 다시 안 넣음), 지금 송출 중 표시, 최근 글 줄(고치기 · 지우기 아이콘), 뷰어 밖 |
| 회중 메시지 · 이름 자막 · 오버레이 · 매크로 · 그리기 | 송출 메뉴 | `--open-menu b:message\|b:names\|b:overlays\|b:macros\|b:draw` | 공통만 + 뷰어 밖 |
| 여러 PC 동기화 · 백업 · 다른 컴퓨터로 옮기기 | 파일 메뉴 | `--open-menu b:sync` (sync) | 공통만 |
| 송출 기록 | 송출 → 송출 기록 (Ctrl+H) | `--open-menu c:history` (history) | 공통만 (내용 높이) |
| 사용 기록 (저작권 보고) | 파일 → 사용 기록 | `--open-menu c:usage` (usage) | 다듬음: 한 줄 설명, 찾기 줄(디자인 체계 글칸), 빈 상태 둘(기록 없음 · 찾는 것 없음), 아래 줄 [모두 지우기](위험 · 확인) … [CSV 저장…](주) |
| 연결 점검 (예배 전 점검) | 도움말 → 연결 점검 | `--open-menu b:diagnose` (diagnose) | 다듬음: 요약 한 줄(문제 주황 · 확인 깃발 · 괜찮음 회색), 항목마다 제목 · 설명 · 고칠 것, 아래 줄 [로그 묶음 저장…] [로그 폴더] … [다시 점검](주) |
| 큐시트 인쇄 | 큐시트 → 인쇄 (Ctrl+P) | `--open-menu b:print` (print) | 공통만 (본보기: 취소 · 주 단추) |
| 단축키 | 도움말 → 단축키 | `--open-menu a:help-keys` (help-keys) | 다듬음: 송출 · 미리보기 · 큐시트 · 찾기와 창으로 묶음, 키 모양 칩(Escape → Esc, ArrowDown → ↓), 묶음에 없는 새 단축키도 '그 밖' 에 |
| 새 버전 | 도움말 → 업데이트 | `--open-menu d:update` · `--open-x update-new\|update-dl\|update-err` | 다듬음(Codex): 상태별 한 줄 · 다음에 할 일, 릴리스 설명 정리 |
| 정보 | 도움말 → 정보 | `--open-menu c:about` (about) | 공통만 |
| 홈페이지 주보 · 설교 포인트 · 예배 전 화면 | 큐시트 · AI 메뉴 | `--open-menu a:home\|a:points\|a:preservice` | 공통만 |
| 종류를 골라 붙여 넣기 | 편집 → 붙여 넣기 (Ctrl+Shift+V) | `--open-a2 paste` (paste) | 공통만 |
| 주보로 큐시트 만들기 | AI → 주보 | `--open-menu d:bulletin` (bulletin) | 공통만 |
| 배치 | 보기 → 배치 | `--open-menu w:workspaces` (workspaces) | 공통만 |
| AI 설정 · 예배 전 큐시트 점검 · 설교 원고 → 슬라이드 · 음성 인식 · 가사 자동 정리 · 예배 후 정리 · 말로 시키기 | AI 메뉴 | `--open-menu ai:<이름>` (ai-*) | 공통만 |
| AI 로 글 보내기 안내 (처음 쓸 때) | AI 기능 첫 사용 | `--open-x aiconsent` (x-aiconsent) | 공통만 |
| 웹에서 가사 찾기 | 큐시트 → 가사 찾기 (Ctrl+L) | `--open lyrics` (x-lyrics) | 공통만 |
| 인터넷 자료 사용 안내 | 가사 찾기 첫 사용 | — | 코드만 |
| 첫 실행 · 자료 옮김 안내 | 처음 켤 때 | — | 코드만 (첫 실행 격리 시험은 자료를 건드릴 위험이 있어 찍지 않음) |
| 확인 창 · 오류 창 | 곳곳 | — | 공통만 (줄 안 확인 `widgets::confirm_*`) |

## 예전 자리 → 새 자리

| 조작 | 예전 | 새 자리 |
|---|---|---|
| 진행용 창(즉석 자막 · 무대 · 이름 자막 · 회중 메시지 · 오버레이 · 매크로 · 그리기) 처음 뜨는 곳 | 뷰어 위 가운데 | 뷰어 바로 아래(본문 판 위). '송출 중만' 보기면 뷰어 왼쪽. 한 번 옮기면 그 자리 기억 |
| 즉석 자막 '큐시트에 넣기' | 송출 단추 오른쪽 | 왼쪽 (주 단추 [송출] 은 맨 오른쪽, 그 왼쪽 [내리기]) |
| 무대 화면 주소 | 회색 글 + [복사] | [주소 복사] 단추, 주소는 아래 흐린 글 (F1 과 같음) |
| 라이브러리 '배경 미디어 끄기' | 목록 맨 아래 | 아래 단추 줄 왼쪽 |
| 사용 기록 'CSV 저장…' · '모두 지우기' | 위 줄 | 아래 단추 줄 (CSV 저장 = 주, 모두 지우기 = 왼쪽 위험) |
| 연결 점검 '다시 점검' · '로그 묶음 저장…' · '로그 폴더' | 맨 위 요약 옆 | 아래 단추 줄 (다시 점검 = 주) |

## F1 에 넘길 것

- `widgets::float_pos` 의 열린 창 세기(`areas().is_visible`)가 `Areas::set_state` 를 부른 닫힌 창도 '보임' 으로 센다 → 창이 화면 가운데까지 내려가 뜰 수 있다. 창 부품(widgets_win)은 처음 자리를 직접 정해 피했다.
- `prolayout_ui.rs` 에 `widgets_win::note_viewers` 두 줄(뷰어 자리 알림)을 넣었다.
- 견본 창(`--specimen`)에 창용 부품을 보이게 하려면: `win::footer_split`(주 단추 · 취소 · 위험), `win::status`(네 가지), `win::empty`, `win::list_row`(고른 줄), `win::section` + `win::field` 한 묶음. 함수는 모두 `widgets_win.rs` 에 있고 자료 없이 그릴 수 있다.

## 확인 (2026-11-28)

- 시험: wp-app 273 · wp-core 372 통과 (`widgets_win` 시험 5개: Esc 가 맨 위 창부터 하나씩 · 편집기 창은 Esc 로 안 닫힘 · 좁은 화면 처음 크기 · 내용 높이에 맞춰 열기 · 긴 창이 메뉴 막대를 덮지 않음).
- parity: 50개 경우 중 정적 48개에서 글자 크기 46개 같음 (다른 둘 obs-pip-image · obs-pip-score 는 2.0.7 부터 있던 차이). 슬라이드 그리는 코드는 바꾸지 않음 (화면 구성 미리보기의 카메라 · 웹 자리 칸 둥근 모서리만).
- 화면 구성 e2e: e2e · e2e-outputs · e2e-auto 모두 통과 (2.0.8 실행 파일과의 화소 비교 둘은 옛 실행 파일이 없어 빠짐). 웹 화면은 건드리지 않음.
- 캡처: `tmp\craft\before`(Codex 가 시작 전에 찍은 것) · `tmp\craft\after` · 쌍 `tmp\craft\compare\compare-<창>.png`, 크기 · 배율 `tmp\craft\sz`(1000x640 · 1600x960 150% · 1000x640 150% · 1920 · 125%).

## 못 한 것 · 까닭

- 찬송가 · PPT 넣기 · 큰 PPT 나누기 · 인터넷 자료 안내 · 첫 실행 자료 옮김 창은 점검 인자가 없어 화면으로 확인하지 못했다(공통 규칙은 받음). 첫 실행은 실제 1.x 자료를 복사할 위험이 있어 일부러 찍지 않았다.
- 모니터 '번호 띄우기 3초'(감사 F-1 제안)는 wp-output 에 새 화면이 필요해 넣지 않았다. 대신 모니터 이름을 '모니터 2 · 오른쪽' 으로.
- 무대 화면 창과 인스펙터 무대 탭의 중복(A13)은 같은 함수를 쓰는 지금 구조를 유지했다(창은 뷰어 밖에 뜨게만). 창을 '미리 보기 + 주소' 로 줄이는 것은 메뉴 항목의 동작이 바뀌어 주인 결정이 필요.
- 즉석 자막 송출 키는 원본(quicktext.js)대로 Enter. F5 가 제안한 Ctrl+Enter 는 단축키를 바꾸지 않는다는 약속과 부딪혀 두었다 — 대신 뷰어를 가리지 않게 하고 '어디로 나가는지' 한 줄과 [내리기] 를 넣었다.
- 아주 작은 화면(1000x640 · 150% = 논리 667x427)에서는 큰 창(큐시트 관리 · 화면 구성 · 템플릿 편집)이 메뉴 막대까지 올라와 덮는다. 잘리지는 않고 창 안에서 스크롤된다.
