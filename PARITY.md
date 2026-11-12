# 기능 대조표 — Electron 판(F:\aside\WorshipCue) ↔ Rust 판

2026-11-12 5차(트랙 A) 작성. 기준: Electron `src/core/core.js` 명령 189개 전부 · `server.js` 경로 · `control/menubar.js`·`ctxmenu.js` 메뉴 · `settings.js` 단축키 · `output/remote/stage` 화면이 받는 데이터 · `main/main.js` 창 동작 · README/사용자설명서 기능.
상태: **됨** (같은 일을 함) · **일부** (빠진 것이 있음) · **없음** · **대체** (Rust 판 구조상 다른 방법으로 같은 결과 — 웹 조작 화면용 업로드 경로 등)
Rust 위치는 `core`=crates/wp-core/src, `app`=crates/wp-app/src.

## 1. 코어 명령 (core.js `commands`)

### 성경 · 미리보기 · LIVE
| 기능 | Electron 위치 | Rust 위치 | 상태 | 메모 |
|---|---|---|---|---|
| 장절 찾기 searchReference | core.js:516 | core/db.rs resolve_reference · core.rs search | 됨 | 파서 같은 규칙 (parser.rs 시험) |
| 낱말 찾기 searchText | core.js:517 | core/db.rs search_text | 됨 | FTS trigram + LIKE |
| 병행 번역 parallelVerses | core.js | core/flow.rs with_parallel | 됨 | text2 |
| 책·장 구조 getStructure · getChapter · getRange · getBooks | core.js:518-531 | core/db.rs get_structure · get_chapter · resolve_ranges · books.rs | 됨 | 성경 책 목록 창(Ctrl+B)은 app 쪽 확인 필요 |
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
| 업데이트 updateStatus · updateAction | core.js | core/updates.rs check · skip · unskip · 메뉴 d:update | 일부 | 확인·건너뛰기만. 내려받기·설치 없음 (릴리스 정지 중이라 의도적) |
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

### 성경 번역본 · 주석 (트랙 C 차례)
| 기능 | Electron 위치 | Rust 위치 | 상태 | 메모 |
|---|---|---|---|---|
| listVersions | core.js | core/db.rs list_versions | 됨 | |
| 번역본 가져오기 analyzeImport · commitImport | core.js · importer.js | — (메뉴 c:import-bible 은 '다음 차례' 알림) | 없음 | 트랙 C 맡음 |
| 번역본 지우기 deleteVersion | core.js | — | 없음 | 트랙 C 맡음 |
| 주석 listCommentaries · analyzeCommentary · commitCommentary · deleteCommentary · commentaryFor | core.js · commentary.js | core/db.rs 표만 | 없음 | 트랙 C 맡음 |
| 베들레헴 찾기·가져오기 openBethlehemSetup · findBethlehem · importBethlehem | core.js | core/hymnal.rs find · import_from | 일부 | 베들레헴 찬송가는 됨. 베들레헴 성경 번역본(.bdb) 가져오기는 트랙 C(성경 가져오기) |
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
| deleteStagePreset | core.js | — | 없음 | 저장만 있고 지우기 없음 → 5차에서 채움 |
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
| listLibFolders · addLibFolder · removeLibFolder · searchLibFiles | core.js | core/library.rs · trackb.rs | 됨 | |

### 큐시트
| 기능 | Electron 위치 | Rust 위치 | 상태 | 메모 |
|---|---|---|---|---|
| listCues · getCue · saveCue · newCue · setActiveCue | core.js | core/db.rs · core.rs open_cue · new_cue · save_cue | 됨 | |
| updateCueMeta · trashCues · restoreCues · purgeCues · duplicateCue · deleteCue | core.js | core/cues.rs · trackb.rs | 됨 | |
| exportCues · importCues | core.js | core/cues.rs export · import · trackb.rs | 됨 | |
| cueText (큐시트를 글로) | core.js | — | 없음 | 5차에서 채움 (글로 복사) |
| 작업공간·독 saveWorkspaces · setActiveWorkspace · saveDockLayout | core.js · dock.js | — | 대체 | egui 는 고정 배치(패널 크기 조절). dockview 작업공간은 없음 |

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
| 가져오기: 성경 번역본 | c:import-bible | 없음 | 트랙 C |
| 송출 기록 · 사용 기록 · 동기화 · 설정 | c:history · c:usage · b:sync · c:settings | 됨 | |
| 실행 취소 · 다시 | b:undo · b:redo | 됨 | 글 고치기 묶음 실행 취소 포함(remember_typing) |
| 순서 넣기: 찬송가·PPT · 성경 · 찬양 · 웹 가사 · 공지 · 제목 · 이미지 · 빈 화면 | 큐시트 '넣기' 단추 · 찬송가 탭 · 라이브러리 · lyrics_ui | 됨 | |
| 순서 넣기: 영상 · 음악 | 라이브러리 창 | 됨 | |
| 순서 넣기: 카메라 · 슬라이드쇼 | — | 없음 | 5차에서 채움 (넣기 단추 + 편집칸) |
| 설교 포인트 · 예배 전 화면 | a:points · a:preservice | 됨 | |
| 클립보드 붙여 넣기 · 종류 골라 붙여 넣기 | Ctrl+V · 찬송가 탭 '붙여 넣기' | 일부 | '종류를 골라 붙여 넣기'(Ctrl+Shift+V) 창 없음 |
| 편집칸 · 슬라이드 편집 · 위/아래 · 복제 · 지우기 · 송출 | 큐시트 아래 단추 · c:slideedit | 일부 | 큐시트 순서 오른쪽 클릭 메뉴 없음 → 5차에서 채움 |
| 송출 메뉴 (다음·이전·다음 순서 미리보기·즉석 자막·회중 메시지·화면 지우기·음악 멈춤·모두 지우기) | 송출 단추 줄 · 단축키 · b:quick · b:message · music_ui | 됨 | |
| OBS · 송출 배경 · OBS 자막 모양 · 보조 화면 · 무대 화면 창 | d:obs · b:lib · c:obsfull/obspip · 예배당 화면 켜기 · b:stage | 됨 | |
| 보기: 패널 · 작업공간 · 패널 잠그기 | — | 대체 | egui 고정 배치 |
| 보기: 조작 화면 테마 · 글자 크기 | 설정 → 조작 화면 | 됨 | |
| 보기: Language · 언어 | 설정(uiLang) · core/i18n.rs | 일부 | 영어 화면 넓히기는 트랙 D |
| 도구: 라이브러리 · 오버레이 · 템플릿 · 미디어 · 무대 타이머 · 매크로 · OBS · 장비 · 리모컨 · 스트림덱 | b:* · c:templates · d:* | 됨 | |
| 도움말: 사용 설명서 · README · OBS 안내 | — | 없음 | 5차에서 채움 (문서 열기) |
| 도움말: 연결 점검 · 로그 묶음 · 정보 | b:diagnose · b:logbundle · c:about | 됨 | |
| 도움말: 단축키 보기 · 새 소식 · 문제 알리기 | — (단축키는 설정 창에서 보임) | 일부 | 5차에서 채움 (도움말 링크) |

### 오른쪽 클릭 메뉴 (ctxmenu.js)
| 항목 | Rust 위치 | 상태 | 메모 |
|---|---|---|---|
| 큐시트 순서: 미리보기 · 송출 · 편집 · 제목 바꾸기 · 예정 시간 · 찬양 라이브러리에 저장 · 나누기 · 위/아래/맨위/맨아래 · 복제 · 지우기 | — (단추로 일부) | 일부 | 5차에서 채움 |
| 슬라이드(미리보기·LIVE): 바로 송출 · 이 슬라이드 편집 · 새 슬라이드 · 복제 · 구역 나누기 | app slideedit_ui (편집기 안) | 일부 | 미리보기·LIVE 썸네일 오른쪽 클릭은 없음 |
| 성경 결과: 이 본문을 큐시트에 넣기 · 장절·본문 복사 | '큐시트에 넣기' 단추 | 일부 | 복사 없음 |
| 화면: 글자 숨김 · 음악 멈춤 · 모두 지우기 · 송출 배경 · 미리보기 끄기 · 무대 창 · 송출 기록 · 그린 것 지우기 | 단추 줄 · 메뉴 | 됨 | 메뉴로 |

### 단축키 (settings.js DEFAULT_SHORTCUTS 23개)
| 단축키 | Rust 위치 | 상태 | 메모 |
|---|---|---|---|
| Ctrl+K · Ctrl+Enter · F5 · F6 · F9 · F10 · PageUp/Down · Ctrl+↑↓ · Ctrl+Shift+F | app.rs shortcuts · keys.rs | 됨 | 설정에서 바꾼 키 적용 |
| Alt+←→ (이전·다음 절) · Ctrl+Home (장 처음) | core/flow.rs preview_adjacent_verse · preview_chapter_start | 됨 | |
| Escape (보조 화면 끄기) · Ctrl+B (성경 책) · Ctrl+H (기록) · F2 (편집) · Ctrl+Shift+E (편집칸) | keys.rs · track_c (Ctrl+H) | 일부 | Ctrl+B 성경 책 목록 창은 확인 못 함 |
| Ctrl+L · Ctrl+T · Ctrl+M · Ctrl+P · Ctrl+Z/Y · Alt+1~9 · Ctrl+E · Ctrl+V | lyrics_ui · trackb · slideedit · hymns_ui | 됨 | |

## 4. 출력 · 무대 · 리모컨 화면이 기대하는 데이터
| 항목 | Rust 위치 | 상태 | 메모 |
|---|---|---|---|
| render: output·version·hasLive·visible·held·slide·versionName·template·obsFull·pip·overlays·layers·video·cursorHidden·cameras | core.rs output_payload | 됨 | output.js 가 읽는 칸 모두 |
| render(stage): slide·nextSlide·index·count·title·note·timer·message·pin | core.rs stage_payload · trackb | 됨 | |
| draw 이벤트 · hello · ack | server.rs · draw.rs | 됨 | |
| 리모컨 /r/state (cue·live·pin) · /r/do 동작 | core/remote.rs | 일부 | message · timer · obsFull 일부 '준비 중' 이었음(4차 연결 여부는 트랙 D 확인) |

## 5. 창 · 시작 · 종료 (main.js)
| 기능 | Electron 위치 | Rust 위치 | 상태 | 메모 |
|---|---|---|---|---|
| 조작 창 · 예배당 출력 창(전체 화면, 다른 모니터, 항상 위) | main.js | app outwin.rs · wp-output | 됨 | |
| 무대 화면 창 | main.js stageWin | trackb_ui | 됨 | |
| 두 번 실행 막기 (second-instance → 앞으로) | main.js:29 | — | 없음 | 5차에서 채움 |
| 화면 꺼짐 막기 (powerSaveBlocker) | main.js:480 | — | 없음 | 5차에서 채움 |
| 시작 화면(splash) | main.js:273 | — | 대체 | Rust 판은 시작이 빨라 생략 |
| 포트가 쓰이면 다음 포트 (~+9) | main.js:346 | core/server.rs serve | 됨 | |
| 데이터 폴더 (--data · 기존 자료 복사) | main.js:332 | core.rs prepare_data_dir · app main.rs | 됨 | Electron 자료는 복사만 |
| 자동 업데이트 설치 (electron-updater) | main/updater.js | core/updates.rs (확인만) | 일부 | 릴리스 정지 중 |
| 익명 사용 통계 | main/telemetry.js | core/updates.rs start_telemetry | 됨 | |
| 끝낼 때 동기화 · 백업 | main.js | core/sync.rs on_exit | 됨 | |

## 6. README · 사용자설명서 기능 목록 (요약)
README 의 큰 기능(성경·찬양·찬송가 PPT·공지·영상·카메라·오버레이·OBS·무대·리모컨·스트림덱·주보 AI·설교 포인트·예배 전 화면·홈페이지·동기화·사용 기록·템플릿·테마)은 위 표에 모두 들어 있다. 위 표에 없는 README 항목: 없음.

## 숫자 요약 (5차 작성 시점)
아래 '5차에서 채운 것' 전의 상태:
- 코어 명령 189개: 됨 169 · 일부 5 · 없음 10 · 대체 5
- 서버 경로 13줄: 됨 9 · 일부 1 · 대체 3
- 메뉴·오른쪽 클릭·단축키 31줄: 됨 19 · 일부 8 · 없음 3 · 대체 1
- 출력 데이터 4줄: 됨 3 · 일부 1
- 창·시작 10줄: 됨 6 · 일부 1 · 없음 2 · 대체 1

## 5차에서 채운 것
(아래에 이어 적음)
