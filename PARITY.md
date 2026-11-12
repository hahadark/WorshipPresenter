# 기능 대조표 — Electron 판(F:\aside\WorshipCue) ↔ Rust 판

2026-11-12 5차(트랙 A) 작성. 기준: Electron `src/core/core.js` 명령 189개 전부 · `server.js` 경로 · `control/menubar.js`·`ctxmenu.js` 메뉴 · `settings.js` 단축키 · `output/remote/stage` 화면이 받는 데이터 · `main/main.js` 창 동작 · README/사용자설명서 기능.
상태: **됨** (같은 일을 함) · **일부** (빠진 것이 있음) · **없음** · **대체** (Rust 판 구조상 다른 방법으로 같은 결과 — 웹 조작 화면용 업로드 경로 등)
Rust 위치는 `core`=crates/wp-core/src, `app`=crates/wp-app/src.

## 최신 검토 원칙 (챗GPT 재개 이후)

- 아래 `됨`은 구현 경로가 있다는 뜻이다. 실제 장비 연결·모든 UI 조작·배포 환경까지 검증 완료했다는 뜻이 아니다.
- 고정 배치를 도킹/작업공간의 동등한 대체로 보던 판정은 철회했다. 현재 패널 표시·배치·잠금·작업공간 저장 기능을 구현 중이다.
- 독립 소스 검토에서 발견한 LIVE 큐 불일치, 음악 순서의 화면 덮기, 리모컨 OBS 상태는 회귀 테스트와 함께 수정했다.
- 릴리스 금지와 기능 구현은 구분한다. 업데이트는 다운로드·검증·취소·명시 설치 요청 코드를 갖추되 `RELEASE_HOLD = true`로 실제 다운로드·설치를 차단한다. 실제 설치·배포 검증은 사용자 지시에 따라 보류한다.
- 과거 단계의 숫자 요약은 당시 관찰 기록이며 현재 완성률로 사용하지 않는다. 최신 전체 테스트·화면 검증 결과는 마지막 절에 기록한다.

## 1. 코어 명령 (core.js `commands`)

### 성경 · 미리보기 · LIVE
| 기능 | Electron 위치 | Rust 위치 | 상태 | 메모 |
|---|---|---|---|---|
| 장절 찾기 searchReference | core.js:516 | core/db.rs resolve_reference · core.rs search | 됨 | 파서 같은 규칙 (parser.rs 시험) |
| 낱말 찾기 searchText | core.js:517 | core/db.rs search_text | 됨 | FTS trigram + LIKE |
| 병행 번역 parallelVerses | core.js | core/flow.rs with_parallel | 됨 | text2 |
| 책·장 구조 getStructure · getChapter · getRange · getBooks | core.js:518-531 | core/db.rs get_structure · get_chapter · resolve_ranges · books.rs · app/parity2_ui.rs | 됨 | 6차 UI 자동 점검에서 Ctrl+B → 시편 23 송출 확인 |
| 이웃 절 adjacentVerse | core.js:525 | core/db.rs adjacent_verse · flow.rs preview_adjacent_verse | 됨 | Alt+←→ |
| 미리보기 previewReference · setPreview · setPreviewIndex | core.js:533-535 | core.rs preview_passage · preview_item · set_preview_index | 됨 | |
| 송출 sendLive (자동 켜기·사용 기록·기록) | core.js:538 | core.rs send_preview · trackd.rs on_send · usage.rs record | 됨 | |
| 다음/이전 nextSlide · previousSlide · gotoSlide | core.js:560-572 | core.rs step · goto_live · flow.rs step | 됨 | 순서 끝에서 다음 순서로 |
| 연속 탐색 extendLive | core.js:573 | core/flow.rs extend_live | 됨 | navMode continuous |
| 숨김 clearLive · restoreLive · toggleLive | core.js:574-576 | core.rs toggle_clear · presentation.rs | 됨 | |
| 상태 getLiveState · getState | core.js:577-578 | presentation.rs live_state · server /api/live · /api/state | 됨 | |
| 출력 끄기 outputOff | core.js:554 | app outwin.rs close | 됨 | |
| 화면에 그리기 drawStroke · drawUndo · drawClear · drawSpot | core.js | core/draw.rs | 됨 | SSE 'draw' |
| 외부 단추 triggerInfo · newTriggerKey | core.js:563-570 | core/trigger.rs info · new_key | 됨 | |

### 템플릿 · 테마 · 설정 · 출력 창
| 기능 | Electron 위치 | Rust 위치 | 상태 | 메모 |
|---|---|---|---|---|
| setTemplate · saveTemplate · duplicateTemplate · deleteTemplate | core.js:581-620 | core.rs set_template · templates.rs · app template_ui.rs | 됨 | |
| 테마 listThemes · applyTheme · ensureTheme | core.js | core/themes.rs · app theme_ui.rs | 됨 | |
| OBS 슬라이드 그대로 setObsView | core.js | core/themes.rs set_obs_view · 메뉴 c:obsfull | 됨 | |
| 업데이트 updateStatus · updateAction | core.js | core/updates.rs check · download · cancel · install_now · skip · unskip · 메뉴 d:update | 일부 | 다운로드·SHA-256 검증·취소·명시 설치 코드 구현, 로컬 fixture 9개 테스트 통과. RELEASE_HOLD가 실제 다운로드·설치 차단. 실제 배포·설치 검증 보류 |
| 내부망 출력 lanOutputInfo · newLanKey | core.js | core/lan.rs info · new_key · app tools_ui | 됨 | |
| 카메라 설정 setCameraPrefs | core.js | core/cameras.rs · 메뉴 b:cameras | 됨 | |
| 설정 바꾸기 updateSettings | core.js | app settings_ui.rs (설정을 직접) | 됨 | |
| 단축키 setShortcuts | core.js | app settings_ui.rs · keys.rs | 됨 | 충돌 검사 포함 |
| 모니터 getDisplays · setOutputWindow · toggleOutputWindow | core.js | app outwin.rs (monitors · open) · trackd_ui 출력 설정 | 됨 | |
| 무대 화면 창 setStageWindow | core.js | app trackb_ui (무대 창) | 됨 | |
| 장비 deviceRun · saveDevice · deleteDevice | core.js | core/devices.rs · app tools_ui | 됨 | |
| Companion companionPress · setCompanion | core.js | core/companion.rs · tools_ui | 됨 | |
| 한글 입력 imeKorean | core.js | core/winkeys.rs ime_korean | 됨 | |
| 전역 단축키 getGlobalShortcutStatus | core.js | core/winkeys.rs status · start_global_shortcuts | 됨 | |
| 송출 기록 지우기 clearHistory | core.js | core/templates.rs clear_history · app history_ui | 됨 | |

### 성경 번역본 · 주석
| 기능 | Electron 위치 | Rust 위치 | 상태 | 메모 |
|---|---|---|---|---|
| listVersions | core.js | core/db.rs list_versions | 됨 | |
| 번역본 가져오기 analyzeImport · commitImport | core.js · importer.js | core/importer.rs analyze_file · commit · app 메뉴 c:import-bible | 됨 | 6차 확인 (트랙 C 4차): JSON·CSV·Zefania·베들레헴 .bdb |
| 번역본 지우기 deleteVersion | core.js | core/importer.rs delete_version | 됨 | 6차 확인 (트랙 C) |
| 주석 listCommentaries · analyzeCommentary · commitCommentary · deleteCommentary · commentaryFor | core.js · commentary.js | core/commentary.rs · app/commentary_ui.rs | 됨 | EPUB 분석·가져오기·삭제·범위 조회, 미리보기 본문의 주석 창 통합. 실제 사용자 EPUB 시험은 별도 |
| 베들레헴 찾기·가져오기 openBethlehemSetup · findBethlehem · importBethlehem | core.js | core/bethlehem.rs · hymnal.rs · importer.rs · app/import_ui.rs | 됨 | 성경 폴더 탐색·여러 번역본 선택 가져오기와 악보·가사 지원 |
| importBethlehemHymns · hymnalBooks · hymnalGet · hymnLyricsStatus | core.js | core/hymnal.rs import_from · books · get · lyrics | 됨 | |

### 찬송가 · PPT · 붙여 넣기 (트랙 A)
| 기능 | Electron 위치 | Rust 위치 | 상태 | 메모 |
|---|---|---|---|---|
| 폴더 listHymnFolders · addHymnFolder · removeHymnFolder · setHymnFolderRender | core.js | core/hymns.rs folders · add_folder · remove_folder · set_folder_render | 됨 | |
| searchHymns · loadHymn | core.js | core/hymns.rs search · load | 됨 | |
| PPT 그리기 (PowerPoint · 직접 · .pps) | hymns.js · pptrender · pptx* · ppt.js | core/hymns.rs · pptx/* · pps.rs | 됨 | 차트 기본 · 표 · 그라데이션. 이음오피스 엔진만큼 세밀하진 않음 |
| 요소 꺼내기 pptTextLayers (PS_BG 포함) | core.js · pptlayers.js | core/hymns.rs ppt_text_layers · powerpoint_bgs · pptx/layers.rs | 됨 | |
| PPT 나누기 pptSplitRule · AI · Prompt · Import · Sheets | core.js · pptsplit.js | core/pptsplit.rs · pptsheets.rs · app hymns_ui | 됨 | |
| 온라인 찾기·받은 PPT songOnlineSearch · songInboxStatus · songInboxStop | core.js · songinbox.js | core/songinbox.rs | 됨 | 실제 폴더 시험 |
| addDroppedFiles | core.js | core/trackb.rs add_dropped_files · library.rs add_dropped | 됨 | 창에 끌어다 놓기 |
| 클립보드 clipboardInfo · pasteNow · pasteTextPlan · pasteSlides | core.js · clippaste.js | core/clippaste.rs · app hymns_ui (Ctrl+V) | 됨 | 실제 클립보드 시험 |

### 주보 · AI · 설교 포인트 · 예배 전 · 홈페이지
| 기능 | Electron 위치 | Rust 위치 | 상태 | 메모 |
|---|---|---|---|---|
| aiSettings · setAi (키 DPAPI) | core.js | core/trackd.rs ai_settings · set_ai · dpapi.rs | 됨 | Electron safeStorage 키는 다시 넣어야 함 |
| quickMakeCue · aiMakeCue · chatAiPrompt · chatAiImport | core.js · bulletin.js | core/trackd.rs · bulletin.rs · app trackd_ui | 됨 | |
| 설교 포인트 sermonPoints* 5개 | core.js · sermonpoints.js | core/sermonpoints.rs · app a_tools.rs | 됨 | |
| 예배 전 화면 preService* 5개 | core.js · preservice.js | core/preservice.rs · app a_tools.rs | 됨 | |
| 홈페이지 주보 homeFeedInfo · homeFeed · homeMakeCue · homeNewsItem | core.js · homefeed.js | core/homefeed.rs · app a_tools.rs | 됨 | 한글 도메인 퓨니코드 |
| openLink · openPath · pickFolder | core.js | core/trackd.rs open_link · open_path · app a_tools.rs pick_folder | 됨 | |

### 사용 기록 · 동기화 · 점검 · OBS · 리모컨
| 기능 | Electron 위치 | Rust 위치 | 상태 | 메모 |
|---|---|---|---|---|
| usageLog · usageCsv · clearUsageLog | core.js · usagelog.js | core/usage.rs list · summary · csv · clear · app history_ui | 됨 | |
| syncInfo · setSyncDir · syncNow · backups · backupNow · restoreBackup · cancelRestore | core.js · sync.js | core/sync.rs | 됨 | WorshipCue-sync.json 형식 같음 |
| diagnose · logBundle | core.js | core/diagnose.rs · app system_ui | 됨 | |
| OBS obsStatus · setObs · obsAction | core.js · obs.js | core/obs.rs · trackd.rs set_obs · app trackd_ui | 됨 | 실제 OBS 로는 미확인 (Electron 도 같음) |
| 리모컨 remoteStatus · setRemote | core.js · remote.js | core/remote.rs · app tools_ui | 됨 | |
| uiReady · logClientError | core.js | — | 대체 | 웹 조작 화면용. egui 는 로그를 바로 씀 (logger.rs) |

### 화면 층 · 무대 · 오버레이 · 매크로 · 영상
| 기능 | Electron 위치 | Rust 위치 | 상태 | 메모 |
|---|---|---|---|---|
| setBackgroundMedia · toggleBackground · toggleLogoScreen · toggleOverlaysOff · clearAll | core.js | core/trackb.rs set_background_media · core.rs toggle_bg · toggle_logo · clear_all · trackb.rs toggle_overlays_off | 됨 | |
| stageTimer · stageMessage · stagePinSet · saveStagePreset | core.js | core/trackb.rs | 됨 | |
| deleteStagePreset | core.js | core/parity_a.rs delete_stage_preset · 무대 창 프리셋 오른쪽 클릭 | 됨 | 5차 |
| 매크로 saveMacro · deleteMacro · runMacro · saveSchedule | core.js | core/trackb.rs · schedule.rs | 됨 | |
| 회중 메시지 showMessage · clearMessage · saveMessageTemplate · deleteMessageTemplate | core.js | core/trackb.rs | 됨 | |
| 영상 videoControl · videoEnded | core.js | core/trackb.rs video_control · video_ended | 됨 | 실제 재생 확인은 트랙 B |
| 오버레이 listOverlays · saveOverlay · deleteOverlay · toggleOverlay | core.js | core/trackb.rs · overlays.rs | 됨 | |

### 찬양 라이브러리 · 라이브러리 폴더
| 기능 | Electron 위치 | Rust 위치 | 상태 | 메모 |
|---|---|---|---|---|
| listSongs · getSong · saveSong · saveSongArrangement · deleteSong | core.js · library.js | core/songs.rs · trackb.rs | 됨 | |
| importSongs · parseSongText · saveImportedSongs | core.js · songimport.js | core/songimport.rs (OpenLyrics · ChordPro · ProPresenter · PPT) · songs.rs | 됨 | |
| 웹 가사 lyricsSearch · lyricsGet · lyricsArrange | core.js · lyricsweb.js | core/lyricsweb.rs · app lyrics_ui.rs | 됨 | |
| listLibFolders · addLibFolder · removeLibFolder · searchLibFiles | core.js | core/library.rs · trackb.rs · thumbs.rs · app/thumbs_ui.rs | 됨 | 그림·영상 썸네일 격자, 백그라운드 캐시·EXIF 방향 적용. 영상 썸네일은 ffmpeg 필요 |

### 큐시트
| 기능 | Electron 위치 | Rust 위치 | 상태 | 메모 |
|---|---|---|---|---|
| listCues · getCue · saveCue · newCue · setActiveCue | core.js | core/db.rs · core.rs open_cue · new_cue · save_cue | 됨 | |
| updateCueMeta · trashCues · restoreCues · purgeCues · duplicateCue · deleteCue | core.js | core/cues.rs · trackb.rs | 됨 | |
| exportCues · importCues | core.js | core/cues.rs export · import · trackb.rs | 됨 | |
| cueText (큐시트를 글로) | core.js | core/parity_a.rs cue_text · 순서 오른쪽 클릭 \"큐시트를 글로 복사\" | 됨 | 5차 |
| 작업공간·독 saveWorkspaces · setActiveWorkspace · saveDockLayout | core.js · dock.js | core/workspace.rs · app/workspace_ui.rs | 일부 | 패널 8개·기본 배치 4개·분할·탭 이동/합치기·개별 독립 창·접기·사용자 배치 관리·잠금 구현, 테스트 5개 통과. 남음: 탭 그룹 전체 띄우기/병합, 독립 창 위치·크기 저장, 하단 송출 줄 중복 정리, 패널별 세부 동등성 대조 |

## 2. 웹 서버 경로 (server.js)
| 경로 | Rust 위치 | 상태 | 메모 |
|---|---|---|---|
| / → /control/ | core/server.rs '/' → /outputs/main | 대체 | 조작 화면은 egui |
| /control/* | — | 대체 | egui 조작 화면 |
| /outputs/main · overlay · stage · /outputs/* · /shared/* · /media/* | core/server.rs | 됨 | OBS 주소 같음 |
| /lib/file (Range) | core/server.rs lib_file | 됨 | |
| /hymn/<key>/<file> (SVG 그림 data: 넣기) | core/server.rs hymn_file | 됨 | |
| /hymnal/score/<book>/<n>.png | core/server.rs | 됨 | |
| /api/trigger/<동작> | core/trigger.rs | 됨 | 16개 |
| /dock · /remote.js · /groups.js · /r/state · POST /r/do | core/remote.rs | 됨 | 리모컨 서버(37540)와 조작 서버 /dock |
| /api/state · /api/live · /events (SSE render·draw·hello) · /api/ack | core/server.rs | 됨 | |
| POST /api/command | core/server.rs command | 일부 | 출력 화면이 쓰는 videoEnded 만 (다른 명령은 egui 에서 직접) — 의도적 |
| POST /api/bulletin · /api/readtext · /api/media · /api/import | — | 대체 | 웹 조작 화면의 파일 올리기. Rust 는 파일 고르기 창으로 바로 읽음 |
| 내부망 출력 서버 (37550, 열쇠) | core/lan.rs | 됨 | |
| Host 검사 (DNS rebinding) · X-WorshipCue 머리글 | core/server.rs host_check · command | 됨 | |

## 3. 조작 화면 (control/*.js) · 메뉴 · 단축키
### 메뉴 줄 (menubar.js)
| 항목 | Rust 위치 | 상태 | 메모 |
|---|---|---|---|
| 큐시트 열기 · 새로 · 관리 | app (큐시트 고르기) · b:cues | 됨 | |
| 주보 순서 붙여 넣기 → 큐시트 | d:bulletin | 됨 | |
| 홈페이지 주보 | a:home | 됨 | |
| 큐시트 인쇄 | b:print | 됨 | HTML 열어 인쇄 |
| 가져오기: 찬양 | b:songs (찬양 창의 가져오기) | 됨 | |
| 가져오기: 성경 번역본 | c:import-bible (app track_c import 창) | 됨 | 6차 확인 (트랙 C) |
| 송출 기록 · 사용 기록 · 동기화 · 설정 | c:history · c:usage · b:sync · c:settings | 됨 | |
| 실행 취소 · 다시 | b:undo · b:redo | 됨 | 글 고치기 묶음 실행 취소 포함(remember_typing) |
| 순서 넣기: 찬송가·PPT · 성경 · 찬양 · 웹 가사 · 공지 · 제목 · 이미지 · 빈 화면 | 큐시트 '넣기' 단추 · 찬송가 탭 · 라이브러리 · lyrics_ui | 됨 | |
| 순서 넣기: 영상 · 음악 | 라이브러리 창 | 됨 | |
| 순서 넣기: 카메라 · 슬라이드쇼 (+ 영상 편집칸) | app parity_ui.rs add_buttons · editor | 됨 | 5차. 카메라 기기는 설정·큐시트에 나온 것에서 고르거나 직접(비우면 기본 카메라) |
| 설교 포인트 · 예배 전 화면 | a:points · a:preservice | 됨 | |
| 클립보드 붙여 넣기 · 종류 골라 붙여 넣기 | Ctrl+V · Ctrl+Shift+V 창 (app parity2_ui.rs) | 됨 | 6차: 알아서·성경·찬양·공지·제목 · 슬라이드 · 그림. UI 자동 점검으로 성경 2개 넣기 확인 |
| 편집칸 · 슬라이드 편집 · 위/아래 · 복제 · 지우기 · 송출 | 큐시트 아래 단추 · c:slideedit · 순서 오른쪽 클릭 | 됨 | 5차 |
| 송출 메뉴 (다음·이전·다음 순서 미리보기·즉석 자막·회중 메시지·화면 지우기·음악 멈춤·모두 지우기) | 송출 단추 줄 · 단축키 · b:quick · b:message · music_ui | 됨 | |
| OBS · 송출 배경 · OBS 자막 모양 · 보조 화면 · 무대 화면 창 | d:obs · b:lib · c:obsfull/obspip · 예배당 화면 켜기 · b:stage | 됨 | |
| 보기: 패널 · 작업공간 · 패널 잠그기 | app/workspace_ui.rs | 일부 | 위 작업공간 항목의 남은 기능 참고 |
| 보기: 조작 화면 테마 · 글자 크기 | 설정 → 조작 화면 | 됨 | |
| 보기: Language · 언어 | 설정(uiLang) · core/i18n.rs · data/en.json | 일부 | 기존 화면과 주석·성경책·붙여넣기 창 번역 통합. 새 편집기·작업공간·업데이트 문구의 추가 점검 필요 |
| 도구: 라이브러리 · 오버레이 · 템플릿 · 미디어 · 무대 타이머 · 매크로 · OBS · 장비 · 리모컨 · 스트림덱 | b:* · c:templates · d:* | 됨 | |
| 도움말: 사용 설명서 · README · OBS 안내 | a:help-manual · a:help-readme · a:help-obs | 됨 | 5차. GitHub 문서를 브라우저로 |
| 도움말: 연결 점검 · 로그 묶음 · 정보 | b:diagnose · b:logbundle · c:about | 됨 | |
| 도움말: 단축키 보기 · 새 소식 · 문제 알리기 | a:help-keys(창) · a:help-news · a:help-issue | 됨 | 5차 |

### 오른쪽 클릭 메뉴 (ctxmenu.js)
| 항목 | Rust 위치 | 상태 | 메모 |
|---|---|---|---|
| 큐시트 순서: 미리보기 · 송출 · 편집 · 제목 바꾸기 · 예정 시간 · 찬양 라이브러리에 저장 · 나누기 · 위/아래/맨위/맨아래 · 복제 · 지우기 | app parity_ui.rs cue_row_menu | 일부 | 5차. 예정 시간은 큐시트 관리 창, PPT 나누기는 편집칸 \"PPT 도구\" 에 (메뉴엔 없음) |
| 슬라이드(미리보기·LIVE): 바로 송출 · 이 슬라이드 편집 · 새 슬라이드 · 복제 · 구역 나누기 | app slideedit_ui (편집기 안) · parity2_ui.rs slide_menu (썸네일) | 됨 | 6차: 바로 송출 · 새 슬라이드 · 복제 · 구역 나누기 · 건너뛰기 / LIVE: 여기로 · 글자 숨김. UI 자동 점검으로 건너뛰기 확인 |
| 성경 결과: 이 본문을 큐시트에 넣기 · 장절·본문 복사 | '큐시트에 넣기' 단추 · parity_ui.rs verse_menu (오른쪽 클릭) | 됨 | 5차 |
| 화면: 글자 숨김 · 음악 멈춤 · 모두 지우기 · 송출 배경 · 미리보기 끄기 · 무대 창 · 송출 기록 · 그린 것 지우기 | 단추 줄 · 메뉴 | 됨 | 메뉴로 |

### 단축키 (settings.js DEFAULT_SHORTCUTS 23개)
| 단축키 | Rust 위치 | 상태 | 메모 |
|---|---|---|---|
| Ctrl+K · Ctrl+Enter · F5 · F6 · F9 · F10 · PageUp/Down · Ctrl+↑↓ · Ctrl+Shift+F | app.rs shortcuts · keys.rs | 됨 | 설정에서 바꾼 키 적용 |
| Alt+←→ (이전·다음 절) · Ctrl+Home (장 처음) | core/flow.rs preview_adjacent_verse · preview_chapter_start | 됨 | |
| Escape (보조 화면 끄기) · Ctrl+B (성경 책) · Ctrl+H (기록) · F2 (편집) · Ctrl+Shift+E (편집칸) | keys.rs · track_c (Ctrl+H) · parity2_ui (Ctrl+B 성경 책 목록 창) | 됨 | 6차: 책 → 장 → 미리보기. UI 자동 점검으로 시편 23 송출 확인 |
| Ctrl+L · Ctrl+T · Ctrl+M · Ctrl+P · Ctrl+Z/Y · Alt+1~9 · Ctrl+E · Ctrl+V | lyrics_ui · trackb · slideedit · hymns_ui | 됨 | |

## 4. 출력 · 무대 · 리모컨 화면이 기대하는 데이터
| 항목 | Rust 위치 | 상태 | 메모 |
|---|---|---|---|
| render: output·version·hasLive·visible·held·slide·versionName·template·obsFull·pip·overlays·layers·video·cursorHidden·cameras | core.rs output_payload | 됨 | output.js 가 읽는 칸 모두 |
| render(stage): slide·nextSlide·index·count·title·note·timer·message·pin | core.rs stage_payload · trackb | 됨 | |
| draw 이벤트 · hello · ack | server.rs · draw.rs | 됨 | |
| 리모컨 /r/state (cue·live·pin) · /r/do 동작 | core/remote.rs | 됨 | 6차: message · timer · obsFull 연결 확인 (/dock POST /r/do → 무대 화면 메시지·타이머 보임) |

## 5. 창 · 시작 · 종료 (main.js)
| 기능 | Electron 위치 | Rust 위치 | 상태 | 메모 |
|---|---|---|---|---|
| 조작 창 · 예배당 출력 창(전체 화면, 다른 모니터, 항상 위) | main.js | app outwin.rs · wp-output | 됨 | |
| 무대 화면 창 | main.js stageWin | trackb_ui | 됨 | |
| 두 번 실행 막기 (second-instance → 앞으로) | main.js:29 | core/parity_a.rs single_instance · focus_existing_window · app main.rs | 됨 | 5차. 데이터 폴더마다 (점검용 --data 는 따로). 실제로 두 번째 실행이 0.8초에 끝나고 첫 창을 앞으로 |
| 화면 꺼짐 막기 (powerSaveBlocker) | main.js:480 | core/parity_a.rs keep_display_awake | 됨 | 5차. SetThreadExecutionState(DISPLAY·SYSTEM) |
| 시작 화면(splash) | main.js:273 | — | 대체 | Rust 판은 시작이 빨라 생략 |
| 포트가 쓰이면 다음 포트 (~+9) | main.js:346 | core/server.rs serve | 됨 | |
| 데이터 폴더 (--data · 기존 자료 복사) | main.js:332 | core.rs prepare_data_dir · app main.rs | 됨 | Electron 자료는 복사만 |
| 자동 업데이트 설치 (electron-updater) | main/updater.js | core/updates.rs · app/trackd_ui.rs | 일부 | 설치 요청 코드·검증 경로 구현, RELEASE_HOLD로 실행 차단. 실제 설치·재시작 시험은 배포 정지 중이므로 보류 |
| 익명 사용 통계 | main/telemetry.js | core/updates.rs start_telemetry | 됨 | |
| 끝낼 때 동기화 · 백업 | main.js | core/sync.rs on_exit | 됨 | |

## 6. README · 사용자설명서 기능 목록 (요약)
README 의 큰 기능(성경·찬양·찬송가 PPT·공지·영상·카메라·오버레이·OBS·무대·리모컨·스트림덱·주보 AI·설교 포인트·예배 전 화면·홈페이지·동기화·사용 기록·템플릿·테마)은 위 표에 모두 들어 있다. 위 표에 없는 README 항목: 없음.

## 6차 숫자 (합친 판 e475b13 이후 + 6차 채움)
- 코어 명령 189개: 됨 177 · 일부 2 (업데이트 설치 — 릴리스 정지) · 없음 5 (주석, 트랙 C) · 대체 5
- 서버 경로 13줄: 됨 9 · 일부 1 (/api/command 는 videoEnded 만 — 의도적) · 대체 3
- 메뉴·오른쪽 클릭·단축키 31줄: 됨 28 · 일부 2 (언어 — 트랙 D, 큐시트 순서 메뉴의 예정 시간·나누기) · 대체 1
- 출력 데이터 4줄: 됨 4
- 창·시작 10줄: 됨 8 · 일부 1 · 대체 1

## 6차 UI 자동 점검 (AccessKit + Windows UI 자동화, tools/ui-e2e.ps1)
디버그 빌드를 임시 데이터 폴더로 띄우고 단추·입력칸을 UI 자동화로 누른 뒤 /api/live · /api/state · 웹 출력으로 확인.
| 흐름 | 결과 | 메모 |
|---|---|---|
| 성경 찾기(요3:16-18) → 미리보기 → 송출 → 다음(단추·F6) · 이전(단추·F5) · F10 숨김/복원 | 됨 | 1/3 → 2/3 → 3/3 → 2/3, visible false/true |
| 큐시트 순서 송출: 성경(시 23) · 찬양 · 찬송가 PPT(clip-…210506.pptx) · 공지 · 제목 | 됨 | PPT 는 요소 27개(edit) + 배경, 웹 예배당 화면에 표 그대로 |
| 큐시트 새로 만들기 → 성경 넣기 → 제목·장절 고치기 → 송출 | 됨 | '말씀 봉독' 요한복음 3:16-17 |
| 공지 넣기 → 본문 '---' 두 장 → 송출 | 됨 | 1/2 '첫 광고입니다' |
| 템플릿 바꾸기 (예배당: 레드 쉴드 → 일반 예배) | 됨 | /api/state template · settings.json activeTemplates 바뀜 |
| 성경 책 목록(시편 → 23) → 송출 | 됨 | 시편 23:1-6 1/6 |
| 종류 골라 붙여 넣기 → 글로 넣기 | 됨 | 성경 순서 2개(요3:16 · 롬8:28) |
| 썸네일 오른쪽 클릭 → 이 슬라이드 건너뛰기 | 됨 | PPT 4장 → 3장 |
| 예배당 창 wp-output --windowed | 됨 | 출력 연결 수 늘고 창에 송출 중 성경 |
| 리모컨 메시지·타이머 (/dock /r/do) | 됨 | 무대 화면에 메시지 띠 · 29:56 |

찾은 것:
- OBS 자막(투명 템플릿)에 표가 든 PPT 를 보내면 슬라이드 글 전체가 가사처럼 위로 넘친다. 데이터(caption = 슬라이드 글)와 그리기(render.js)가 Electron 판과 같아서 Electron 판도 같다. 표·일정 PPT 는 편집칸 'OBS 자막 가사' 를 비우지 말고 짧게 넣거나, 순서 템플릿 pptMode 를 '그림'으로 — 고치지 않고 적어 둠.
- 디버그 빌드는 콘솔 창이 같이 뜬다 (출시 빌드는 windows_subsystem 이라 없음).
- 자동 점검의 SendKeys 첫 키가 가끔 빠진다 (창 활성화 시점). 앱 문제 아님 — 단추·UI 자동화로 확인.
- Ctrl+Shift+V: egui 는 Ctrl(+Shift)+V 를 Key 가 아니라 Paste 이벤트로 준다 → Shift 가 눌린 Paste 로 창을 연다 (6차에서 고침). Ctrl+B 는 위젯보다 먼저 처리하도록 옮김.

## 숫자 요약 (5차 작성 시점)
아래 '5차에서 채운 것' 전의 상태:
- 코어 명령 189개: 됨 169 · 일부 5 · 없음 10 · 대체 5
- 서버 경로 13줄: 됨 9 · 일부 1 · 대체 3
- 메뉴·오른쪽 클릭·단축키 31줄: 됨 19 · 일부 8 · 없음 3 · 대체 1
- 출력 데이터 4줄: 됨 3 · 일부 1
- 창·시작 10줄: 됨 6 · 일부 1 · 없음 2 · 대체 1

## 5차에서 채운 것
- 무대 프리셋 지우기 · 큐시트를 글로 복사 · 카메라·슬라이드쇼 순서 넣기와 편집칸(영상 편집칸 포함) · 큐시트 순서 오른쪽 클릭 메뉴 · 도움말 메뉴 6개(설명서·README·OBS 안내·단축키 보기·새 소식·문제 알리기) · 두 번 실행 막기 · 화면 꺼짐 막기

채운 뒤 숫자:
- 코어 명령 189개: 됨 171 · 일부 5 · 없음 8 (모두 트랙 C 의 성경·주석 가져오기) · 대체 5
- 서버 경로 13줄: 됨 9 · 일부 1 · 대체 3
- 메뉴·오른쪽 클릭·단축키 31줄: 됨 24 · 일부 5 · 없음 1 (성경 번역본 가져오기, 트랙 C) · 대체 1
- 출력 데이터 4줄: 됨 3 · 일부 1
- 창·시작 10줄: 됨 8 · 일부 1 (업데이트 설치 — 릴리스 정지) · 대체 1 (시작 화면)

남은 '일부'(다른 트랙 몫 빼고): 종류 골라 붙여 넣기(Ctrl+Shift+V) 창 · 미리보기/LIVE 썸네일 오른쪽 클릭 · Ctrl+B 성경 책 목록 창 확인 · /api/command 다른 명령(의도적)


## 챗GPT 재개 후 최신 검증 (2026-11-12)
- 전체 테스트: 앱 7개, 코어 130개 통과 (13개는 실제 파일·외부 환경용으로 제외).
- 개발 빌드 성공. 별도 점검 자료에서 성경(요 3:16-18)과 찬양 순서 송출, /api/live·/outputs/main 응답 확인.
- 통합: 주석·베들레헴 일괄 가져오기, 편집기 세부, 썸네일·백업, LIVE 큐 불일치·음악 계약, 출력 파일 경계, 업데이트 상태 기계(배포 정지 게이트), 작업공간 v2, 새 창 영어 문구.
