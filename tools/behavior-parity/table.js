// 원본(Electron) 조작 화면의 이벤트 처리기를 Rust 판과 대조한 표를 만든다 (docs/behavior-parity.md 3절).
// 쓰는 법 (저장소 뿌리에서):
//   node tools/behavior-parity/handlers.js <Electron 저장소>/src/web/control tmp/handlers.tsv   ← 처리기 줄 뽑기
//   node tools/behavior-parity/table.js tmp/handlers.tsv tmp/parity_table.md                    ← 표와 수치
// 아래 목록에 없는 처리기 줄이 생기거나(원본이 바뀜) 같은 줄을 두 번 넣으면 오류로 멈춘다.
// 상태: T=같음(합성 시험) S=같음(소스 근거만) F=고침·새로 만듦(합성 시험) G=고침(코어 시험·소스)
//       D=다름(의도) L=다름(화면 설계) R=남음·확인 못 함 N=해당 없음
const fs = require('fs');
const [, , inFile = 'tmp/handlers.tsv', outFile = 'tmp/parity_table.md'] = process.argv;
const H = fs.readFileSync(inFile, 'utf8').split('\n').filter(Boolean).map(l => { const [loc, ev] = l.split('\t'); const [f, n] = loc.split(':'); return { f, n: +n, ev }; });
const G = [];
const g = (f, lines, what, st, rust) => G.push({ f, lines, what, st, rust });

// ---- control.js
g('control.js', [42, 43], '화면 오류·약속 거부를 로그에', 'N', 'Rust 는 panic hook · logger.rs');
g('control.js', [144], '알림을 누르면 닫힘', 'S', 'Act::ToastDismiss');
g('control.js', [150, 151, 168, 187, 197, 202, 207, 218, 220, 221], '코어 → 화면 SSE 사건 (state·settings·update·message·obs·outputs·remote·songs·cues·draw)', 'N', '같은 프로세스라 Snap 으로 바로 읽음');
g('control.js', [324, 325, 326, 338], '처음 실행 안내 (베들레헴 가져오기 · 나중에)', 'S', 'firstrun · import_ui');
g('control.js', [365, 369, 370], '성경 책 창: 책 누름 · 거르기 · Enter = 첫 책', 'S', 'cue_ui.rs 성경 책 탭');
g('control.js', [377], '장 단추 = 절 목록만 띄움 (미리보기는 절을 누를 때)', 'S', 'cue_ui.rs get_chapter');
g('control.js', [390], '장 격자에서 방향키로 옮기기', 'R', '확인 못 함 (방향키 처리 근거 없음)');
g('control.js', [488], '클릭하면 [미리보기][바로 송출] 모드 단추', 'S', 'Act::TogglePreviewMode');
g('control.js', [569], '미리보기 화면의 글상자 누름 → 다시 그림', 'N', 'egui 는 매 프레임 그림');
g('control.js', [606], '미리보기 아래 번호 칩 누름 = 그 장 미리보기', 'T', 'dbl_preview_chip_moves_and_sends');
g('control.js', [611], '번호 칩 두 번 = 그 장 송출', 'F', 'dbl_preview_chip_moves_and_sends (click2 로 바꿈)');
g('control.js', [615, 616], '미리보기 ◀ ▶', 'T', 'preview_pane_menu_sends_and_live_pane_menu_hides_text (PreviewStep)');
g('control.js', [678, 679], '영상 조작 줄 (재생·멈춤·찾기)', 'S', 'media_ui.rs video_bar');
g('control.js', [685], 'LIVE 번호 칩 = 그 장으로', 'S', 'Act::LiveGoto');
g('control.js', [691, 704, 758], '예배당/OBS 보기 · 본문/주석 탭 · 주석 보기', 'S', 'MonitorViews · commentary_ui');
g('control.js', [904], '본문 줄 누름 = 미리보기 · Shift+누름 = 범위', 'T', 'first_click_keeps_rows_in_place_everywhere 등');
g('control.js', [917], '본문 줄 두 번 = 미리보기하고 송출', 'F', 'dbl_search_result_row_previews_and_sends');
g('control.js', [924], '본문 목록 ↑↓ · Shift+↑↓ 범위 · Enter · 맨 위에서 ↑ = 검색칸', 'S', 'center_ui.rs');
g('control.js', [979], '검색칸: 치는 중에는 후보만 (미리보기·송출 안 바뀜)', 'F', 'typing_reference_in_direct_mode_does_not_go_live_until_enter');
g('control.js', [1059, 1066], '후보 드롭다운 누름 · 칸을 떠나면 닫힘', 'L', '후보를 드롭다운이 아니라 본문 목록에 보임');
g('control.js', [1068], '검색칸 Enter = 검색 · ↓ = 목록으로 · Esc = 모두 고르기', 'R', 'Enter·↓ 는 있음 (typing_keeps_current_cue_preview_and_enter_replaces_it). Esc = 모두 고르기 없음');
g('control.js', [1169, 1170, 1171, 1172], '아래 줄: 이전 · 다음 · 송출 · 글자 숨김', 'S', 'topbar_ui.rs bottom_bar');
g('control.js', [1174, 1175, 1176, 1177, 1178, 1180], '배경 · 오버레이 · 로고 · 음악 멈춤 · 모두 지우기 · 더보기', 'S', '지우기 ▾ 메뉴 + (새) 아래 줄·LIVE 오른쪽 메뉴');
g('control.js', [1228], '지금 화면 칩 (층마다 끄기)', 'S', 'app.rs layer_action');
g('control.js', [1245, 1246, 1247], '설정 단추 · 검색 방식 · 이동 방식', 'S', 'settings_ui · panel_ui');
g('control.js', [1291, 1298, 1308, 1313, 1314], '빠른 설정: 대상 · 템플릿 · 접기 · 자세히 · 테마', 'S', 'quick_ui.rs · panel_ui.rs');
g('control.js', [1316, 1320, 1330], '분할 방식 · 병행 번역 · 번역본 고르기', 'S', 'topbar_ui.rs · panel_ui.rs');
g('control.js', [1363, 1370], '송출 기록: 줄 누름 = 다시 띄움 · 모두 지우기', 'S', 'history_ui.rs');
g('control.js', [1364], '송출 기록 목록 키 (↑↓ Enter)', 'R', '확인 못 함');
g('control.js', [1388, 1389], '창 닫기 단추 · 창 바깥 누르면 닫힘', 'L', '떠 있는 창(여러 개) 방식 — 바깥 눌러 닫기 없음');
g('control.js', [1399], '예배당 화면 켜기·끄기 단추', 'S', 'Act::OutputToggle');
g('control.js', [1404], '글꼴 다 읽으면 다시 그림', 'N', '');
g('control.js', [1422], '번호 칩 줄 접기', 'S', 'settings thumbsFold');
g('control.js', [1472], '글칸에 들어가면 한글 입력으로 (focusin)', 'R', '확인 못 함');
g('control.js', [1516], '상태 줄 누름 = 출력 설정', 'S', 'widgets::status_btn');
g('control.js', [1619], '전역 키: 단축키 · Esc 차례 · Alt+1~9 · 1~9/C/B/E/P 건너뛰기 · ←→ · Enter', 'F', 'live_song_number_and_c_keys_jump_to_that_verse_and_chorus (새로) · ctrl_enter_* · custom_shortcuts_*');

// ---- cue.js
g('cue.js', [109], 'Ctrl+Z · Ctrl+Y (글칸·창 밖에서)', 'S', 'Act::Undo');
g('cue.js', [227], '진행 시계 시작 시각', 'S', 'cuemgr_ui.rs 시작 시각 칸');
g('cue.js', [452, 453], '큰 PPT 를 넣은 뒤 "여러 순서로 나눌까요" 묻기', 'R', '없음 (메뉴·단추로는 나누기 창을 열 수 있음)');
g('cue.js', [546, 553, 558, 571, 572, 574, 580, 581], 'PPT 나누기 창: 썸네일 누름·Shift·두 번, 범위 칸, 범위 더하기, 이름 Enter, 합치기·빼기, 이름 고치기, 모두 비우기', 'F', 'dbl_ppt_split_thumbnail_toggles_split_there_and_again_merges · ppt_split_click_start_shift_click_end_then_add_range_with_name · ppt_split_rule_button_merge_remove_clear_and_go_replaces_the_item');
g('cue.js', [621, 649], 'PPT 나누기: 규칙으로 · 나누기', 'F', 'ppt_split_rule_button_merge_remove_clear_and_go_replaces_the_item');
g('cue.js', [622, 623, 631, 641], 'PPT 나누기: AI · 채팅 프롬프트 복사 · 채팅 열기 · 답 불러오기', 'G', 'pptsplit_ui.rs (화면 시험 없음 — API·클립보드 필요)');
g('cue.js', [632], '모아보기 그림 저장', 'S', 'hymns_ui.rs PPT 도구 칸 (새 창에는 단추 없음)');
g('cue.js', [791], '인쇄 옵션 (메모·가사·본문)', 'R', '확인 못 함');
g('cue.js', [792], '인쇄', 'S', 'menubar b:print');
g('cue.js', [828], '카메라 고르기 창', 'R', '확인 못 함');
g('cue.js', [916, 917, 918], '음악 순서: 재생 · 멈춤 · 페이드아웃', 'S', 'music_ui.rs');
g('cue.js', [938], '슬라이드 줄 누름 = 그 장 미리보기 (Shift·Ctrl = 여러 장)', 'T', 'first_click_keeps_rows_in_place_everywhere');
g('cue.js', [945], '슬라이드 줄 두 번 = 그 장 송출', 'F', 'dbl_slide_row_sends_that_slide');
g('cue.js', [1102, 1103, 1104], '편집칸 입력 · 고르기 · 테마', 'S', 'app.rs 편집칸');
g('cue.js', [1105], '편집칸 글칸에서 Esc = 칸 나가기', 'S', 'egui 기본 (Esc = 초점 놓기)');
g('cue.js', [1106], '슬라이드쇼 그림 지우기 · 왼쪽으로', 'R', '확인 못 함');
g('cue.js', [1117, 1127, 1134], '장비 줄 · 보내 보기 · 송출할 때 자동으로', 'S', 'actions_ui.rs');
g('cue.js', [1146, 1153], '부를 순서 저장·쓰기·지우기 · 이름표 칩', 'S', 'app.rs 편집칸');
g('cue.js', [1161], '편집칸 단추 20종 (닫기·위·아래·복제·지우기·PPT 나누기·요소 꺼내기·음악·OBS·장비·매크로·Companion 눌러 보기 …)', 'R', '대부분 있음. Companion 눌러 보기 · 카메라 고르기 · 슬라이드쇼 그림 더하기는 확인 못 함');
g('cue.js', [1237, 1246], '편집칸 보이기 단추 · 그림 파일 고르기', 'S', 'toggleEditor · rfd');
g('cue.js', [1258], '순서 줄 누름 = 미리보기 (바로 송출 모드여도 송출 안 함)', 'D', '바로 송출 모드에서는 누르면 송출 — dbl_cue_row_in_direct_mode_sends_once_and_stays_on_that_item');
g('cue.js', [1266], '순서 줄 두 번 = 미리보기만', 'D', '두 번 = 송출 (사용자 결정) — dbl_cue_row_sends_in_preview_mode');
g('cue.js', [1271], '순서 목록 키: ↑↓ · Alt+↑↓ 옮기기 · Enter · Delete', 'R', 'Ctrl+↑↓ · F2 는 있음. 목록에 초점을 둔 ↑↓·Alt+↑↓·Delete 는 확인 못 함');
g('cue.js', [1289, 1290, 1295, 1310], '순서 끌어 옮기기 · 라이브러리에서 끌어 놓기', 'S', 'cue_row_dnd · dnd payload');
g('cue.js', [1417, 1418, 1441], '큐시트 고르기 · ＋ 넣기 · 왼쪽 탭', 'S', 'cue_ui.rs');
g('cue.js', [1498, 1499, 1559, 1564], '찬송가 폴더 빼기 · 읽는 방식 · 폴더 고르기 · 등록', 'S', 'hymns_ui.rs');
g('cue.js', [1511, 1547, 1597], '온라인에서 찾기 · 지켜보기 멈춤', 'S', 'hymns_ui.rs · songinbox');
g('cue.js', [1515], '찬송가 창 방식 (PPT / 베들레헴)', 'S', 'hymns_ui.rs');
g('cue.js', [1548, 1551, 1552], '찾기 주소 · 다운로드 폴더 · 받은 PPT 폴더', 'R', '확인 못 함');
g('cue.js', [1557, 1558], '찬송가 기본 배경 · 다른 이미지로', 'S', 'hymns_ui.rs default_bg_row');
g('cue.js', [1571], '폴더로 거르기', 'R', '확인 못 함');
g('cue.js', [1573], '찬송가 찾기 (치면 목록)', 'S', 'hymns_ui.rs');
g('cue.js', [1574, 1579], '찬송가 목록 ↑↓·Enter · 한 번 = 고르기', 'F', 'hymn_search_arrow_and_enter_add_selected_row · dbl_library_hymn_row_adds_once_and_single_click_only_selects');
g('cue.js', [1580], '찬송가 목록 두 번 = 넣기', 'F', 'dbl_library_hymn_row_adds_once_and_single_click_only_selects');
g('cue.js', [1586], '큐시트 관리 창 열기', 'F', 'cue_list_empty_area_menu_adds_item_and_opens_cue_manager');

// ---- library.js
g('library.js', [30], '라이브러리 탭 바꾸기', 'T', 'dbl_library_* 시험들');
g('library.js', [34, 54], '목록 키 (↑↓ Enter) · 검색칸 키', 'R', '찬송가 탭은 시험 있음. 다른 탭은 확인 못 함');
g('library.js', [53], '라이브러리 찾기', 'S', '');
g('library.js', [62], '끌어서 큐시트로', 'S', 'dnd_set_drag_payload');
g('library.js', [83], '줄·칸 한 번 = 고르기', 'F', 'dbl_library_* (찬송가·음악은 한 번에 넣던 것 고침)');
g('library.js', [84], '줄·칸 두 번 = 큐시트에 넣기', 'F', 'dbl_library_song_adds_it_once · …hymn… · …music… · …image…');
g('library.js', [98, 99, 100, 116], '찬양: 새 찬양 · 웹 가사 · 가져오기 · 고치기', 'S', 'trackb_ui.rs');
g('library.js', [123, 145, 170, 171], '찬양 가져오기 창 (파일 · PPT 에서 · 모두 고르기 · 가져오기)', 'R', '확인 못 함');
g('library.js', [205, 206, 207, 208, 218, 219], '가사 찾기 창', 'S', 'lyrics_ui.rs');
g('library.js', [286, 287, 288], '찬양 저장 · 저장하고 넣기 · 지우기', 'S', 'trackb_ui.rs');
g('library.js', [298, 316, 317, 319, 321], '찬송가 설정 · 폴더 고르기 · 폴더 관리 · 올리기 · 폴더 등록', 'S', 'trackb_ui.rs · thumbs_ui.rs');
g('library.js', [333, 334, 336, 337, 342], '그림·영상 칸 단추: 템플릿 배경 · 송출 배경 · 순서 배경 · 로고 · 재생', 'S', 'thumbs_ui.rs');
g('library.js', [358, 359, 378, 383, 384, 385], '카메라: 새로 고침 · 끄기 · 설정 · 순서로 · 순서 배경 · 템플릿 배경', 'S', 'panel_ui.rs · camscan.rs');
g('library.js', [382], '카메라 송출 배경 (두 번 = 켜기·끄기)', 'F', 'dbl_library_camera_toggles_live_background');
g('library.js', [439, 448, 454, 455], '카메라 설정 창', 'S', 'camera prefs');
g('library.js', [518, 519, 520], '폴더 관리 창', 'S', 'add_lib_folder · remove_lib_folder');
g('library.js', [542, 545, 546, 547], '파일 고르기 창 (라이브러리에서)', 'L', '윈도우 파일 고르기 창(rfd)을 씀');
g('library.js', [577, 581, 582], '소리 재생기 내부 사건', 'N', '');
g('library.js', [602, 603, 604, 605, 606, 607], '음악 재생 줄: 재생·멈춤·페이드·찾기·소리·이어 재생', 'S', 'music_ui.rs');
g('library.js', [636, 645, 646, 647, 673, 674, 675, 676, 691], '오버레이: 새로 · 켜기 · 고치기 · 지우기 · 위치 · 크기 · 여백 · 그림 · 저장', 'S', 'trackb_ui.rs overlays_window');
g('library.js', [705, 706, 717, 723, 728], '테마 탭: 테마 창 · 템플릿 편집 · 적용 · 순서에 · 편집', 'S', 'theme_ui.rs');

// ---- grid.js
g('grid.js', [24, 25, 30], '보기 바꾸기 · 썸네일 크기 · Ctrl+휠', 'S', 'center_ui.rs');
g('grid.js', [41, 137], '격자·전체 격자 칸 누름 = 미리보기', 'T', 'first_click_in_all_view_does_not_scroll_the_clicked_cell_away');
g('grid.js', [50, 151], '격자·전체 격자 칸 두 번 = 송출', 'F', 'dbl_grid_cell_sends_that_slide · dbl_all_grid_cell_of_another_item_sends_it · dbl_all_list_line_sends_it');
g('grid.js', [52, 150], '격자 키 (방향키 · Enter · 여러 장 고르기 키)', 'S', 'center_ui.rs msel_keys');

// ---- slideedit.js
g('slideedit.js', [490, 491], '글 바로 고치기: 다른 곳 누름 = 저장 · Esc · Ctrl+Enter', 'F', 'dbl_editor_main_text_edits_in_place_and_ctrl_enter_saves_without_sending · dbl_editor_text_box_edits_in_place_escape_cancels_click_away_saves');
g('slideedit.js', [1012], '상자 두 번 = 글 바로 고치기 (잠긴 상자 제외)', 'F', 'dbl_editor_* 3개');
g('slideedit.js', [1011], '요소 누름·끌기 (옮기기)', 'S', 'translate_item');
g('slideedit.js', [549, 550], '요소 끌어 옮기기·크기 조절 손잡이', 'R', '옮기기만 있음. 크기 조절 손잡이 없음 (숫자 칸으로)');
g('slideedit.js', [656, 984, 1152], '여러 장 고르기 줄 · 목록 누름 (Shift·Ctrl) · 본문 목록 고르기 키', 'S', 'slideedit_ui.rs · msel_keys');
g('slideedit.js', [694, 699, 721, 993, 999, 1010], '끌어서 여러 장 고르기 (올가미)', 'R', '없음 (Shift·Ctrl 클릭 · Ctrl+A 로는 됨)');
g('slideedit.js', [692, 1061], '썸네일 끌림 막기 · 창 크기 바뀜', 'N', '');
g('slideedit.js', [912], '속성 칸 단추들 (정렬·굵게·겹침 차례·잠금·복제 …)', 'R', '단순한 속성 칸만 있음 (잠금·겹침 차례·맞춤 단추 없음)');
g('slideedit.js', [955, 965], '속성 칸 입력', 'S', 'slideedit_ui.rs');
g('slideedit.js', [1013], '편집기 키: Enter·F2 = 글 고치기 · Delete · 방향키 이동 · Ctrl+C/V/D', 'R', '없음 (Rust 편집기에 고른 요소 개념이 없음)');
g('slideedit.js', [1052], '편집기 창 키: Esc 닫기 · Ctrl+Z', 'R', '확인 못 함');

// ---- ctxmenu.js
g('ctxmenu.js', [245], '오른쪽 클릭 메뉴 9곳 (순서 · 빈 자리 · 슬라이드 · 전체 격자 · 본문 줄 · 미리보기 · LIVE · 라이브러리 · 아래 줄)', 'F', 'verse_menu_* · slide_row_and_grid_cell_menu_send_now · library_menu_add_to_cue_for_song_hymn_music_image · preview_pane_menu_sends_and_live_pane_menu_hides_text · cue_list_empty_area_menu_adds_item_and_opens_cue_manager · cue_row_menu_note_is_saved_on_the_item');
g('ctxmenu.js', [31], '메뉴 항목 누름 = 실행하고 닫힘', 'T', '위 메뉴 시험들');
g('ctxmenu.js', [254], '메뉴 바깥 누르면 닫힘', 'S', 'egui 기본');
g('ctxmenu.js', [258], '메뉴 키 (↑↓ Enter Esc)', 'R', '확인 못 함');
g('ctxmenu.js', [30, 38, 255, 256, 257], '초점 유지 · 마우스 올림 강조 · 창 흐림/크기/스크롤 때 닫기', 'N', 'egui 메뉴가 맡음');

// ---- cuemanager.js
g('cuemanager.js', [162, 173, 174, 175], '큐시트 관리: 단추 전부 · 줄 두 번 = 열기 · 찾기 · 이름/날짜/메모/고정/서식/정렬/고르기', 'F', 'dbl_cue_manager_row_opens_that_cue_and_single_click_only_selects 외 cue_manager_* 5개');
g('cuemanager.js', [130], '큐시트 파일 가져오기 (여러 개)', 'G', 'cuemgr.rs import_cue_files — 파일 고르기 창은 시험 못 함');

// ---- dropfiles.js
g('dropfiles.js', [123], '파일을 끌어 놓으면 종류에 맞게 넣기', 'S', 'hymns_ui.rs dropped_files');
g('dropfiles.js', [120, 121, 122], '끄는 동안 "여기에 놓으세요" 겹침 안내', 'R', '확인 못 함');

// ---- paste.js
g('paste.js', [199, 201], 'Ctrl+V 붙여 넣기 · Ctrl+Shift+V 종류 골라', 'S', 'hymns_ui.rs paste · parity2_ui.rs');

// ---- quicktext.js
g('quicktext.js', [79, 80, 81, 82, 89], '즉석 자막: 닫기 · 송출 · 큐시트에 · Enter/Shift+Enter/Esc · 최근 목록', 'F', 'quick_text_* 3개');
g('quicktext.js', [132], '즉석 자막 단추', 'S', 'bottom_bar');
g('quicktext.js', [130], '창 크기 바뀌면 자리 다시', 'N', '');

// ---- menubar.js
g('menubar.js', [201, 222], '메뉴줄 누름 · 바깥 누르면 닫힘', 'S', 'menubar.rs');
g('menubar.js', [211, 223], '열린 채 옆 메뉴로 넘어가기 · Alt·방향키', 'R', '확인 못 함');
g('menubar.js', [221], '하위 메뉴 닫힘 늦추기', 'N', '');

// ---- 검증: 모든 처리기 줄이 꼭 한 번씩
const key = h => h.f + ':' + h.n;
const used = new Map();
for (const x of G) for (const n of x.lines) {
  const k = x.f + ':' + n;
  if (!H.some(h => key(h) === k)) throw new Error('목록에 없는 줄 ' + k);
  used.set(k, (used.get(k) || 0) + 1);
}
for (const h of H) if (!used.has(key(h))) throw new Error('빠진 처리기 ' + key(h));
for (const [k, v] of used) if (v > 1) throw new Error('두 번 넣음 ' + k);

const NAME = { T: '같음 (합성 시험)', S: '같음 (소스 근거만)', F: '고침·새로 만듦 (합성 시험)', G: '고침 (코어 시험·소스)', D: '다름 (의도)', L: '다름 (화면 설계)', R: '남음·확인 못 함', N: '해당 없음' };
const cnt = {};
let total = 0;
for (const x of G) {
  x.c = x.lines.reduce((a, n) => a + H.filter(h => h.f === x.f && h.n === n).length, 0);
  cnt[x.st] = (cnt[x.st] || 0) + x.c;
  total += x.c;
}
let out = '| 원본 (파일:줄) | 처리기 수 | 무엇을 하면 무엇이 되나 | 상태 | Rust 자리 · 시험 |\n|---|---|---|---|---|\n';
for (const x of G) out += `| ${x.f}:${x.lines.join('·')} | ${x.c} | ${x.what} | ${NAME[x.st]} | ${x.rust || ''} |\n`;
out += '\n| 상태 | 처리기 수 |\n|---|---|\n';
for (const k of 'TSFGDLRN') out += `| ${NAME[k]} | ${cnt[k] || 0} |\n`;
out += `| **합계** | **${total}** |\n`;
fs.writeFileSync(outFile, out);
console.log(JSON.stringify(cnt), '합계', total, '처리기 줄', H.length);
