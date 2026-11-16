# 1.x 대비 빠진 기능 점검과 복원 (2026-11-16)

## 복원 결과 (master, 2026-11-16 14:30)

| 항목 | 결과 | 확인 |
|---|---|---|
| 추가 창 바깥(조작 화면)을 누르면 닫기 | 됨. 슬라이드 편집기는 예외(큐시트를 함께 누르며 쓰는 창). 창을 연 그 누름에는 닫지 않음 | 합성 시험 2개 |
| AI 로 큐시트 만들기 화면 | 됨. 파일 메뉴 항목, 세 갈래(순서 붙여 넣기·API 키·채팅 AI), 키 발급 링크, 모델 직접 입력, 실행 중 중복 방지·경과 시간·모델·비용, 결과 확인 카드(열기는 단추로), 결과 파일 불러오기, 이름 칸·예시·초안 저장 | 단위 시험. 유료 호출·결과 카드 화면은 실제로 안 봄 |
| 파일·폴더 끌어다 놓기 | 됨. 큐시트·라이브러리·찬송가 폴더·찬양·순서 배경·슬라이드쇼·슬라이드 편집기, 끄는 중 안내 | 합성 입력 시험. 실제 탐색기 끌기는 아직 손으로 안 해 봄 |
| 탐색기에서 복사한 파일 Ctrl+V | 됨(Windows). 맥은 아직 | 시험 |
| 슬라이드 편집기 캡처 그림 Ctrl+V | 됨 | 시험 |
| 큐시트 인쇄 옵션(가사·공지, 성경 본문, 메모, 코드, 큰 글씨, 두 단) | 됨. 맥은 open 으로 열기 | 시험·화면 |
| 찬송가·PPT 검색 폴더별 거르기 | 됨 | 시험 |
| 온라인 찬양 찾기 주소·폴더 설정 | 됨(찬송가 화면·설정 센터) | 시험 |
| 슬라이드쇼 그림 순서(왼쪽 이동) | 됨 | 시험 |
| 슬라이드 편집기 PageUp/PageDown·Home/End | 됨 | 시험 |
| 큰 PPT 넣은 뒤 나누기 제안 | 됨(20장 이상) | 시험 |
| 편집기 영역 선택 중 가장자리 자동 스크롤 | 됨 | 시험 |
| ffprobe 없을 때 영상 길이·남은 시간 | 됨(예배당 화면이 알려 준 길이 사용) | 시험 |
| 맥: AI 키·OBS 비밀번호 키체인 보관 | 됨(맥 빌드 서버에서 키체인 저장·읽기 확인) | 맥 CI |
| 맥: 클립보드 그림·글 붙여 넣기 | 됨(osascript·pbpaste) | 맥 컴파일만 |
| PPTX 를 PowerPoint 없이 원본처럼 (SmartArt·일부 차트) | 아직 | 범위가 커서 보류 |
| Electron 같은 전체 화면 배경막 모달 | 하지 않음 | 바깥 누름 닫기로 대신 |

전체 시험: wp-app 223, wp-core 271 통과(병렬 3회 반복). 드물게 auditd_tests::reimport_menu_and_ask_dialog_by_synthetic_clicks 가 다른 시험과 전역 확인창 상태로 부딪쳐 실패한 적이 있음(재현 안 됨).

---
# 워십프리젠터 1.x → Rust 2.x 누락·축소 기능 감사

조사일: 2026-11-16, 13:18~13:27 KST. 읽기 전용 소스 조사이며 앱 실행·장비 연결·유료 AI 호출 시험은 하지 않았다.

## 1. 범위와 판정 원칙

- Electron 기준: `F:\aside\WorshipCue`, 조사 중 HEAD `a68c714eddc5014c229fca252f91513f14d98135`.
- Rust 기준: `F:\aside\WorshipPresenter-rs`의 **현재 작업 트리**. 조사 시작 HEAD는 `3e7d272453a3a8caad3e371427c07c022d7252ae`, 13:24 재확인 HEAD는 `f86d2a63af6be321e43d4e809e1ab96fcafa5878`이었다. 조사 중 다른 작업의 수정·커밋이 진행되었으므로 설치된 2.0.2 바이너리와 같다고 단정하지 않는다. 이 조사에서는 코드 수정·commit·push를 하지 않았다.
- 이하 `E/`는 Electron 저장소, `R/`는 Rust 저장소 기준 상대 경로다. `app/`는 `R/crates/wp-app/src/`, `core/`는 `R/crates/wp-core/src/`다.
- GitHub에서 `gh release list -R hahadark/WorshipPresenter -L 60`을 실행하고 다음 **1.x 릴리스 18개 전부**의 본문을 각각 `gh release view <태그> ... --json body -q .body`로 읽었다: v1.1.0, v1.2.0, v1.3.0, v1.4.0, v1.4.1, v1.5.0, v1.6.0, v1.7.0, v1.7.1, v1.8.0, v1.9.0, v1.10.0, v1.11.0, v1.12.0, v1.12.1, v1.13.0, v1.14.0, v1.14.1.
- 문서 기준: `R/tmp/docs-main/docs/v1/README.md`, `사용자설명서.md`, `OBS설정가이드.md`. 기존 `PARITY.md`, `docs/behavior-parity.md`, `docs/integration-audit-2026-11-16.md`는 후보 발견에 이용했으나 판정은 실제 처리 코드로 재확인했다.
- **없음**: 특정 사용자 기능의 현재 UI·입력·처리 경로를 찾지 못했고, 관련 구현 전체를 읽어 축소된 경로를 확인한 경우. **반쪽**: 핵심 기능은 있지만 원본의 일부 입력·출력·선택권·플랫폼 동작이 빠진 경우. 단순히 실행 시험이 없다는 이유로 누락으로 판정하지 않았다.
- 영향은 사용자 작업에 미치는 정도, 난이도는 소스 구조를 보고 산정한 **추정**이다. 모든 난이도는 작업량 보증이 아니다.
- 이 목록은 아래 근거 범위의 소스 감사 결과다. 모든 형식·장비·운영체제에서의 실행 동등성까지 검증한 완전한 인증은 아니다. 미확인 사항은 6절에 별도 남겼다.

## 2. 확인된 누락·축소 기능 표

| 기능 | Electron 근거(파일:줄 또는 릴리스 태그) | Rust 상태(없음/반쪽/있음) | Rust 근거 | 사용자 영향(높음/중간/낮음) | 구현 난이도(쉬움/보통/어려움) |
|---|---|---|---|---|---|
| 외부 파일·폴더 끌어놓기: 큐시트 넣기, 라이브러리 폴더 등록, 편집기 요소·순서 배경으로 놓기 및 대상 안내 | `E/src/web/control/dropfiles.js:1-7,18-49`; v1.2.0, v1.14.0 | 없음 | `git grep`로 `crates` 전체의 `dropped_files`, `hovered_files`, `drop_file`, `FileDrop` 입력 소비 경로를 찾지 못함. `core/trackb.rs:114`의 파일 등록 함수는 있음. 앱 호출은 `app/trackb_ui.rs:296-298`, `app/hymns_ui.rs:602-606`의 **파일 고르기 단추**뿐. 함수 이름만으로 DnD 구현으로 판정할 수 없음 | 높음 | 보통 |
| 탐색기에서 복사한 파일을 Ctrl+V로 순서에 넣기 | `E/src/web/control/paste.js:100-115`; v1.14.0 | 없음 | `core/clippaste.rs:202-242`는 SLIDES·IMAGE·TEXT만 검사. `app/hymns_ui.rs:527-555`도 슬라이드/이미지만 처리. `GetFileDropList`, `CF_HDROP`, `FileDrop` 소비가 `crates` 전체에 없음 | 중간 | 보통 |
| 슬라이드 편집기에 캡처 이미지를 Ctrl+V로 요소로 붙여넣기 | `E/src/web/control/paste.js:112-116`; v1.14.0 | 반쪽 | `app/slideedit_canvas.rs:38-50`는 `Event::Paste(String)`만 수집. `app/slideedit_ui.rs:675-694`는 내부 요소 또는 글상자로 변환. 큐시트에 이미지 순서를 넣는 별도 경로는 있으나 편집 중 이미지 바이트→요소 경로는 없음 | 중간 | 보통 |
| 큐시트 인쇄의 가사·공지 본문·성경 본문·코드·메모 선택, 큰 글씨·두 단, 앱 안 미리보기·옵션 기억 | `E/src/web/control/cue.js:782-793`; v1.12.0; `사용자설명서.md:243-245` | 반쪽 | `core/cues.rs:119-136`은 번호·예정 시각·제목/장절·메모·시간 표만 생성. 본문·가사 조회 및 옵션이 없음. `core/trackb.rs:423-427`→`app/trackb_ui.rs:80-85`로 HTML 바로 열기만 함 | 높음 | 보통 |
| AI 모델 이름 직접 입력 | `E/src/web/control/ai.js:31,47-50` | 반쪽 | `core/trackd.rs:30-46`은 임의 모델 저장 가능하지만 `app/trackd_ui.rs:298-305`는 `RECOMMENDED` 4개만 제공. 직접 입력 UI 없음 | 중간 | 쉬움 |
| AI API 키 발급 페이지 바로 열기 | `E/src/web/control/ai.js:29,46`: `https://openrouter.ai/settings/keys` | 없음 | `app/trackd_ui.rs:284-305`에는 키 저장·삭제와 모델 선택만 있음. 채팅 AI 사이트 링크와 달리 키 발급 링크 없음 | 낮음 | 쉬움 |
| 채팅 AI 결과 파일(.json/.txt/.md) 불러오기 및 결과 파일 끌어놓기 | `E/src/web/control/ai.js:122-124,148-161,205-215` | 반쪽 | `app/trackd_ui.rs:260-282`는 프롬프트·샘플 저장·사이트 열기·답 글 붙여넣기는 있으나 결과 **파일 읽기** 단추/처리 없음. 주보 파일 고르기는 `bulletin::read_bulletin`으로 읽어 주보 텍스트 칸에 넣는 다른 동작이므로 대체가 아님 | 중간 | 쉬움 |
| AI 호출 중 중복 실행 차단·경과 시간·완료 모델/비용 표시 | `E/src/web/control/ai.js:173-192`는 단추 비활성화, 0.5초 간격 경과 표시, `usage.cost`·실제 모델 표시 | 반쪽 | `app/trackd_ui.rs:244-253`는 키 존재만으로 활성화. `core/trackd.rs:130-148`는 클릭마다 스레드 생성, 진행 중 알림 1회. `_usage`, `_m`을 버리며 AI용 busy 상태 없음. OBS용 `obs_busy`와 다름 | 중간 | 보통 |
| AI API 생성 후 순서 목록·개별 경고 검토와 명시적 ‘이 큐시트 열기’ 단계 | `E/src/web/control/ai.js:195-202`; `E/src/core/core.js:261-263` | 반쪽 | `core/trackd.rs:98-101`은 저장 직후 자동으로 큐시트를 열고, API 경로 `:141-144`는 경고 **개수**만 알림. 상세 경고·생성 순서 목록을 보여 주는 결과 카드가 없음. 단, 규칙/채팅 경로는 `app/trackd_ui.rs:202-207`에 개별 경고 글이 있어 ‘모든 경고 없음’은 아님 | 높음 | 보통 |
| AI 없이 주보 붙여넣기: 생성 전에 큐시트 이름 별도 지정·예시 채우기·재시작 후 초안 복구 | `E/src/web/control/ai.js:81-103`의 `qkName`, `qkEx`, `wc-quick-order` | 반쪽 | `app/trackd_ui.rs:21-33,235-242`는 본문과 규칙 실행 단추만, `core/trackd.rs:105-110`도 text만 받음. 이름은 본문 파서가 정하며 별도 지정 UI 없음. 텍스트는 프로세스 내 `St`에만 보유 | 낮음 | 쉬움 |
| 큰 PPT 추가 직후 자동 분할 제안 | v1.13.0; `E/src/web/control/cue.js:440-453` | 없음 | `docs/behavior-parity.md`도 누락으로 기록. `app/hymns_ui.rs`·`app/pptsplit_ui.rs` 및 `core`에서 추가 후 장수 기반 offer 경로를 찾지 못함. 수동으로 PPT 나누기 창을 여는 기능은 있음 | 낮음 | 쉬움 |
| PowerPoint 없이 PPTX를 원본에 가깝게 렌더링하는 범위: SmartArt·미지원/복합 차트 등 | v1.14.0; `E/src/core/pptxengine.js:1-14`, `pptxbuild.js:184-200`의 IeumOffice 엔진 사용 | 반쪽 | Rust는 자체 파서. `core/pptx/mod.rs:1251-1252`는 diagram 종류만 지정, 도식 내용을 읽지 않음. `core/pptx/chart.rs:68-81`은 특정 차트 종류만 받고 나머지는 None, scatter도 line으로 변환. `core/pptx/svg.rs:415-423`은 읽힌 차트/글만 그리는 경로. 모든 PPT가 깨진다는 뜻은 아니며 **확인된 지원 범위 축소**다 | 높음 | 어려움 |
| 찬송가·PPT 검색에서 등록 폴더별 필터 | `E/src/web/control/cue.js:1467,1496,1503,1571` | 없음 | `app/hymns_ui.rs:125-167`는 검색어와 PPT/베들레헴 원천만 선택, `hymns::search(&c,&q)` 전체 검색. `:173-202`의 폴더 목록은 렌더링 방식·삭제용이고 검색 필터는 아님 | 중간 | 쉬움 |
| 온라인 찬양 PPT 찾기 주소·다운로드 폴더·받은 PPT 폴더를 화면에서 설정 | `E/src/web/control/cue.js:1548-1552` | 반쪽 | `core/songinbox.rs:29-42,66-73`는 `songSearchUrl`, `songDownloadDir`, `songInboxDir` 사용. 그러나 `app` 전체 검색에서 읽는 곳은 `hymns_ui.rs:95`의 검색 주소 사용뿐이며 세 값 편집 UI 없음. 기본 경로의 온라인 찾기·감시는 있음 | 중간 | 쉬움 |
| 슬라이드쇼 그림 순서 왼쪽 이동 | `E/src/web/control/cue.js:1106-1114`의 `data-ssleft`와 배열 교환 | 반쪽 | `app/parity_ui.rs:259-279`는 그림 나열·빼기·더하기만. 해당 그림 목록에서 순서 변경 단추/끌기 없음. 슬라이드 전체 차례 바꾸기와 슬라이드쇼 내부 그림 배열 편집은 구분 | 중간 | 쉬움 |
| 슬라이드 편집기 PageUp/PageDown·Home/End 이동 | `E/src/web/control/slideedit.js:1027-1032` | 반쪽 | `app/slideedit_canvas.rs:60-70`은 방향키·Enter·F2·Delete 등만 처리. `app` 전체에서 해당 이동 키 처리 없음(`keys.rs`의 키 문자열 파싱 시험은 동작 구현이 아님). 방향키 기본 이동은 있음 | 낮음 | 쉬움 |
| 슬라이드 편집기 목록 영역 선택 중 가장자리 자동 스크롤 | `E/src/web/control/slideedit.js:993-1007`, 특히 `:1001-1002` | 반쪽 | `app/slideedit_ui.rs:318-385`는 Shift+끌기 영역 선택과 순서 끌기만 구현, 영역 선택 중 스크롤 이동 호출 없음. `app/center_ui.rs:183-226`의 별도 중앙 슬라이드 순서 끌기에는 자동 스크롤이 있으므로 전역 미구현으로 묶으면 안 됨 | 낮음 | 보통 |
| 추가 창의 모달성: 배경 차단·하나의 모달 흐름 | `E/src/web/control/control.css:190-193` 전체 화면 배경막, `control.js:1375-1389` 공통 모달 | 반쪽 | `app/settings_ui.rs:232-260`, `trackd_ui.rs:148-167` 등은 일반 `egui::Window`, 여러 창을 함께 유지. 바깥 클릭 닫기는 현재 구현되었지만 Electron처럼 전체 화면 배경을 가린 모달은 아님. 기능 부재가 아니라 **상호작용 방식 차이**로 분리 | 중간 | 보통 |
| [맥] AI 키·OBS 암호의 운영체제 보안 저장 | `E/src/main/main.js:384-386`은 플랫폼별 Electron safeStorage 콜백 사용; `E/src/core/core.js:1019-1024` | 반쪽 | `core/dpapi.rs:114-119`의 비Windows protect/unprotect는 바이트를 그대로 반환, `:123-133`은 base64 포장. 맥 Keychain 구현 없음. `app/trackd_ui.rs:286`의 ‘Windows 보호 기능으로 암호화’ 문구도 운영체제 구분 없음 | 높음 | 보통 |
| [맥] 큐시트 인쇄 HTML 열기 | v1.4.0 맥 지원 + v1.12.0 인쇄; 공통 `E/src/web/control/cue.js:780-793` | 반쪽 | `app/trackb_ui.rs:80-85`가 플랫폼 분기 없이 `Command::new("explorer")` 실행. `core/trackd.rs:168-173`에 이미 있는 macOS `open` 분기 함수를 인쇄 경로에서는 사용하지 않음. 실패 반환도 무시 | 중간 | 쉬움 |
| [맥] 클립보드 이미지 붙여넣기 및 메뉴에서 클립보드 다시 읽기 | `E/src/web/control/paste.js:100-116`은 Chromium의 이미지·텍스트 paste 이벤트 사용. PowerPoint 슬라이드 추출은 원래 Windows 전용이므로 여기서 제외 | 반쪽 | `core/clippaste.rs:192-217`의 이미지·텍스트 클립보드 읽기는 Windows Forms + `powershell.exe`. `app/parity2_ui.rs:47-49,100-102`도 같은 경로. 일반 텍스트 Ctrl+V는 egui 이벤트로 가능하나 이미지/메뉴 읽기는 맥 대체 경로 없음 | 중간 | 보통 |
| ffmpeg/ffprobe 미설치 환경의 영상 길이·남은 시간·길이 기반 탐색 | v1.7.0; `E/src/web/control/media.js:75-84,115-127`은 HTML video 메타데이터만 사용 | 반쪽 | `app/media_ui.rs:166-199`는 ffmpeg 탐색 후 옆의 ffprobe 실행으로 길이 획득. 없으면 None 반환. 재생/일시정지/정지/반복 자체가 없다는 뜻은 아님 | 중간 | 보통 |

## 3. AI 큐시트 상세 대조

### 3.1 결론

**‘Rust에 AI 큐시트가 아예 없다’는 판정은 틀리다.** 실제 주보 창 단추에서 OpenRouter 호출, JSON 파싱, 찬송가·성경 연결, 새 큐시트 저장·열기까지 연결되어 있다. 반면 화면 완성도와 일부 입출력 선택권이 1.x보다 줄었다. 기존 `PARITY.md`의 aiMakeCue/chatAiImport ‘됨’은 코어 존재를 설명할 뿐 세부 UI 동등성을 보장하지 않는다.

또한 문서의 `ai-chat.png`, `ai.png`에 해당하는 ‘AI 채팅’은 앱 안에서 AI와 계속 대화하는 채팅창이 아니다. 원본 `E/src/web/control/ai.js:107-170`의 실체는 **외부 채팅 AI를 열고 프롬프트·샘플·주보를 사람이 전달한 뒤 JSON 답을 가져오는 마법사**다. 근거 없이 내장 대화 기능 누락으로 추가하지 않았다.

### 3.2 Electron의 화면 흐름

1. 메뉴에서 주보로 큐시트 만들기를 연다. `aiSettings`로 키 유무·모델 목록을 읽고, 키가 없으면 기본 ‘순서 붙여넣기(AI 없이)’, 키가 있으면 API 탭을 고른다. 탭 선택은 `wc-ai-mode`에 보관한다(`ai.js:19-27,63-68`).
2. **규칙 경로**: 예배 순서를 줄마다 붙여넣고, 원하면 별도 큐시트 이름을 입력한다. 예시 넣기와 `wc-quick-order` 초안 보관이 있다. `quickMakeCue`→`parseOrderText`→`planToSavedCue`로 저장한다(`ai.js:81-103`, `core.js:1030-1037`).
3. **API 경로**: 마스킹한 OpenRouter 키 입력칸에서 저장·삭제 또는 키 발급 페이지 열기. 추천 모델 4종 또는 직접 모델 ID를 입력한다(`ai.js:28-51`).
4. 파일 고르기 또는 주보 글 붙여넣기, 추가 요청을 입력한다. 지원 파일은 hwp·hwpx·docx·pdf·txt·png·jpg·jpeg·webp이며, 코어는 md도 읽는다. HWP5/ZIP XML 문서는 글자를 꺼내고 PDF·그림은 원본 내용을 AI에 보낸다(`ai.js:53-59`, `bulletin.js:20-87`). 파일 끌어놓기도 받는다(`ai.js:205-215`, `dropfiles.js:46-47`).
5. 실행 중 단추를 막고 경과 초를 표시한다. 파일이면 로컬 서버 `/api/bulletin`에 본문을 올리고, 글이면 `aiMakeCue` 명령을 보낸다(`ai.js:173-189`). 이것은 앱 내부 업로드 경로이고 실제 AI API 주소와 구분해야 한다.
6. `askAI`가 JSON 응답을 받고 `planToCue`로 항목을 만든다. 찬송가는 등록 폴더의 번호·찬송가 책을 찾아 PPT에 연결, 없으면 베들레헴 가사로 대체, 그것도 없으면 제목 카드와 경고를 만든다. 성경 장절은 DB로 검사하고, 교독문·라이브러리 찬양도 연결한다. 메모·예정 시간·시작 시각을 보존한다(`core.js:240-263`, `bulletin.js:287-340`).
7. 새 큐시트로 **저장**한 뒤, 모델·경과 시간·사용량에 cost가 있으면 비용을 표시한다. 결과 카드에 항목별 제목·부제·장절과 경고 전문을 보여 준다. 사용자가 ‘이 큐시트 열기’를 눌러 화면을 전환한다(`ai.js:190-202`). 이는 미리보기 후 저장하는 단계가 아니라 **저장 후 검토·열기 단계**다.
8. **채팅 AI 경로**: 추가 요청→프롬프트 복사·샘플 JSON 저장→챗GPT/클로드/제미나이 열기→주보·샘플을 외부 채팅에 첨부→받은 답 전체를 붙여넣거나 결과 파일을 불러오기/끌어놓기→큐시트 생성. 샘플 그대로인 응답은 거절한다(`ai.js:107-170`, `bulletin.js:146-179`, `core.js:1042-1051`).

### 3.3 API·모델·설정 키

- 기본 공급자: **OpenRouter**. OpenAI 호환 chat completions API 형태이며, 챗GPT·클로드·제미나이 소비자 구독으로 자동 호출하는 기능은 아니다.
- API: `https://openrouter.ai/api/v1/chat/completions` (`E/src/core/bulletin.js:261-278`, `core/bulletin.rs:16,517-540`).
- 인증: `Authorization: Bearer <사용자가 넣은 키>`; 추가 식별 헤더는 `HTTP-Referer: https://github.com/hahadark/WorshipPresenter`, `X-Title: Worship Presenter`.
- 기본 모델: `anthropic/claude-haiku-5.5`.
- 원본/현 Rust의 추천 모델 ID: `anthropic/claude-haiku-5.5`, `openai/gpt-6-luna`, `google/gemini-3.8-flash`, `anthropic/claude-sonnet-5.5` (`E/src/core/bulletin.js:12-18`, `core/bulletin.rs:9-16`). 이 이름은 **소스에 적힌 식별자**이며 현재 공급자에서 실제 사용 가능한지·가격이 얼마인지는 이번 조사에서 검증하지 않았다.
- 영속 설정: `ai.model`, `ai.endpoint`, `ai.keyEnc`, `ai.key`. Electron `setAi` 입력 인자는 `apiKey`, `model`, `endpoint`다(`core.js:1014-1024`). Rust `Core::set_ai(key,model,endpoint)`가 같은 설정 키를 쓴다(`core/trackd.rs:30-46`).
- Electron은 safeStorage가 가능하면 `keyEnc`, 불가능하면 평문 `key`를 저장한다. Rust Windows는 DPAPI + `dp1:` 형식이고, **같은 PC의 Electron 키를 해독·승계하는 코드도 있다**(`core/dpapi.rs:1-3,136` 이하). 따라서 ‘무조건 모든 키 재입력 필요’라는 과거 기록도 현재 상태로 일반화하면 안 된다. 맥은 Keychain 대체가 빠져 있다.
- endpoint 재정의는 양쪽 코어에 있으나 원본 AI 화면에도 별도 endpoint 편집칸은 확인되지 않았다. Rust에 해당 칸이 없다는 이유만으로 새 누락 항목을 만들지 않았다.

### 3.4 프롬프트와 응답 처리

- 시스템 지시: 한국 교회 방송실 도우미, 주보의 주일 낮/오전 예배 우선, **설명 없이 JSON 하나**.
- 스키마: `name`, `serviceDate`, `startTime`, `items[]`; 각 항목은 `type`, `title`, `subtitle`, `hymnNumber`, `hymnbook`, `ref`, `body`, `note`, `duration`.
- 규칙: 찬송은 hymn, 성경은 bible/ref, 미정 찬양팀은 title, 광고는 text의 `---`로 요약 분할, 설교 제목·설교자는 subtitle, 일어섬은 note, 마지막은 blank ‘예배 후’, 주보에 없는 내용은 만들지 않기.
- 요청값: `temperature: 0.1`, `response_format: {type:"json_object"}`. 글은 60,000자 범위, PDF는 data URL의 `file`, 이미지는 data URL의 `image_url`로 전송한다.
- 채팅 프롬프트는 샘플 값 재사용 금지·읽기 어려운 부분은 ‘확인 필요’·JSON 코드블록 또는 결과 파일 응답을 추가로 안내한다. 응답 파서는 코드블록·앞뒤 설명·최상위 배열·`cue.items` 포장도 처리하고 샘플 동일 내용은 거절한다.
- 이 핵심 프롬프트·파싱·변환은 Rust `core/bulletin.rs:141-253,401-476,478-540`에 실제로 이식되어 있다. 단순 함수명만 확인한 것이 아니다.

### 3.5 현재 Rust에서 되는 경로와 빠진 부분

실제 연결: `app/menubar.rs`의 `d:bulletin`→`app/trackd_ui.rs:99-108,148-167`→`bulletin_ui:212`→API 단추 `:244-253`→`core/trackd.rs:130-148`→`bulletin::ask_ai`→`Core::plan_to_saved_cue:52-101`.

- **있음**: 주보 파일 읽기, PDF·이미지 멀티모달 전송, 규칙 생성, 추가 요청, 추천 모델 선택, 키 저장/삭제, 프롬프트 복사, 샘플 저장, 외부 채팅 사이트 열기, 답 글 파싱, 샘플 거절, 찬송가·본문·찬양 연결, 새 큐시트 저장 및 열기.
- **없거나 축소**: 직접 모델 입력, 키 발급 링크, 채팅 결과 파일 읽기/끌기, AI 실행 중 중복 방지·경과 시간·비용/모델 결과 표시, API 결과 경고 전문·검토 카드·명시적 열기 단계, 규칙 생성의 별도 이름·초안 저장·예시 채우기.
- **사용자 주의**: API 경로는 결과가 도착하면 현재 열린 큐시트를 자동으로 바꾼다. LIVE를 직접 송출하는 코드와는 다르므로 ‘자동으로 방송에 나간다’고 단정해서는 안 된다. 다만 준비 중이던 큐시트의 화면 전환은 1.x와 다른 동작이다.
- **추정**: AI 단추를 반복 누르면 스레드·유료 호출·생성 큐시트가 중복될 수 있다. 클릭마다 스레드를 생성하는 코드로부터의 추정이며 실제 비용을 발생시키는 재현은 하지 않았다.

## 4. 설정 등 추가 창의 닫힘 동작

### Electron

`control.js:1375-1389`는 공통 `#modal`을 열고 닫는다. X 단추와 모달 바탕에 대한 mousedown에서 닫는다. `control.js:1629,1646`에는 Escape로 닫는 처리도 있다. `control.css:190`의 배경막은 화면 전체를 덮는다. 따라서 설정·주보 등 공통 모달을 쓰는 창은 **배경막 바깥 영역 누름/X/Esc 닫기**가 원본 동작이다. 모든 OS 네이티브 파일 선택창까지 같은 규칙이라는 뜻은 아니다.

### 현재 Rust

- **설정창 바깥 클릭 닫기 있음**: `settings_ui.rs:232`의 `widgets::float_window`→`widgets.rs:984-991`→`close_on_outside:1001-1021`.
- **주보·출력·OBS·새 버전도 현재 바깥 클릭 닫기 있음**: `trackd_ui.rs:158`에서 `close_on_outside` 호출을 재확인했다. 조사 초반의 읽기에는 없었으나 다른 작업의 후속 변경으로 추가된 상태를 최종 반영했다.
- 번역본·동기화·카메라·설교 포인트·홈페이지 주보 등도 `float_window`를 쓰며, 도구/작업공간은 같은 공통 닫기 처리를 직접 호출한다.
- 다만 Rust의 닫기는 **기존 창 밖의 메인 배경 레이어**를 누를 때이며, 메뉴·팝업·다른 떠 있는 창은 제외한다(`widgets.rs:998-1021`). Electron 배경막과 완전히 동일한 모달은 아니다. 여러 창을 함께 띄울 수 있다.
- 설정창 전체에 대한 Esc 닫기는 앱 측 코드에서 공통 처리 경로를 찾지 못했다. egui 내부 기본동작 및 실제 포커스 조건은 실행 확인하지 않았으므로, 이를 별도 확정 누락 수치에는 넣지 않았다. 슬라이드 편집기는 `slideedit_canvas.rs:34-35`에 Esc의 선택해제/닫기 경로가 별도로 있다.
- 따라서 기존 `behavior-parity.md`의 ‘떠 있는 창 방식이라 바깥 누르기 없음’ 기록은 **최신 작업 트리에서는 오래된 판정**이다.

## 5. 운영체제 차이와 과거 기록 정정

### Windows 전용을 새 누락으로 세지 않은 것

- PowerPoint COM으로 슬라이드를 원본 그대로 그림으로 만들거나 PPT 붙여넣기를 보완하는 경로는 원래 Windows·PowerPoint 설치 의존이다. macOS에 COM이 없다는 사실만으로 Rust 신규 누락으로 세지 않았다.
- 한글 입력 자동 전환·Windows 전역 단축키도 원본의 Windows 전용 기능이다. Rust `winkeys.rs`에 구현 경로가 있다.
- Windows 외부 파일 끌기와 파일 복사 붙여넣기는 PowerPoint 자체와 무관한 일반 입력 기능이므로 별도 누락으로 셌다.

### 맥에서 주의할 실제 차이

- Keychain 미사용, 인쇄 HTML을 explorer로 여는 경로, Windows PowerShell 기반 클립보드 읽기는 위 표에 따로 표시했다.
- Rust의 맥 앱·보조 화면·카메라 권한·Command 키의 실제 배포 동작은 맥 장비에서 시험하지 않았다. 원본 v1.4.0의 mac 지원을 근거로 Rust 맥 전체가 없다고 주장하지 않았다.

### 기존 감사 문서를 그대로 쓰면 생기는 오판

- `PARITY.md`의 `RELEASE_HOLD=true`는 과거 기록이다. 현재 `core/updates.rs:13`은 **false**, 업데이트 처리기는 check/download/cancel/install 경로가 있다. 이번 누락 목록에서 뺐다.
- Electron safeStorage 키를 무조건 재입력해야 한다는 과거 설명도 현재 `dpapi.rs`의 승계 구현과 다르다.
- 카메라 고르기, 슬라이드쇼 그림 삭제, 찬양 가져오기, Companion 시험 단추 등은 ‘이전 미확인’만으로 누락으로 넣지 않았다. 예를 들어 카메라 선택은 `parity_ui.rs:232-256`, 슬라이드쇼 삭제는 `:267-274`에 실제 단추가 있다.
- 큐시트 클릭·더블클릭과 Shift+끌기 방식은 기존 문서에 사용자 결정에 따른 의도적 차이로 기록되어 있다. 기능 부재와 구분하여 결함 수치에 넣지 않았다.

## 6. 미확인·추가 실행 점검이 필요한 범위

다음은 없는 기능이라고 단정하지 않았다. 근거 없는 항목을 확정 표에 섞지 않기 위한 보류 목록이다.

- 메뉴 Alt/방향키/Enter/Esc 및 열린 채 옆 메뉴 이동: `menubar.rs`는 egui Popup/SubMenu를 사용하므로 앱의 명시적 key handler 부재만으로 동작 부재를 결론 낼 수 없다.
- 일반 라이브러리 탭별 키보드 선택, 찬양 가져오기 전체 포맷·모두 선택, 출력 연결 상태·리모컨·OBS·Companion·X32/PTZ·카메라의 실제 장비 상호작용.
- PPT 원본 충실도는 구조적 한계를 확인했지만 폰트·차트·표·SmartArt의 실제 파일별 손실 크기는 비교 렌더링하지 않았다.
- 출력·미디어의 실제 소리·영상 탐색·페이드·다중 모니터 복원, 업데이터의 실제 배포 설치, 맥 네이티브 메뉴와 권한.
- 조사 중 작업 트리가 바뀌었다. 확정 표의 후보를 구현에 옮기기 전 해당 파일을 다시 읽어야 한다. 보고서의 ‘없음’은 과거 릴리스 바이너리가 아닌 위 기준 시간의 로컬 소스를 뜻한다.

## 7. 구현 확인되어 목록에서 제외한 주요 기능

주요 근거 묶음: 성경 `core/core.rs`, `db.rs`, `flow.rs`와 `app/cue_ui.rs`; 주보 `trackd_ui.rs`·`trackd.rs`·`bulletin.rs`; 슬라이드 편집 `slideedit_ui.rs`·`slideedit_canvas.rs`; 출력·OBS `trackd_ui.rs`·`outwin.rs`·`core/obs.rs`; 작업공간 `workspace_ui.rs`·`panel_ui.rs`; 찬송가/PPT `hymns_ui.rs`·`pptsplit_ui.rs`·`core/hymns.rs`; 큐시트 `cuemgr_ui.rs`·`core/cues.rs`; 카메라 `camscan.rs`·`system_ui.rs`·`core/cameras.rs`.

**있음으로 확인한 주요 기능:** AI API·채팅 답 글·규칙 주보 큐시트 생성, 성경 장절/낱말/책·장 탐색·병행 번역, 찬송가·베들레헴 가사, PPT 수동/규칙/API/외부 채팅 분할, PPT 요소 추출·글 편집, 슬라이드 여러 장 선택·이동·구역 분할, 큐시트 관리·실행취소/재실행, 일반 글/PowerPoint 슬라이드/큐시트용 이미지 붙여넣기(Windows), 템플릿·테마·순서별 꾸미기, 예배당/OBS/무대/LAN 출력, 출력 Esc 끄기·OBS 연동, 카메라 목록·설정·미리보기, 미디어 기본 조작, 즉석 자막·회중 메시지·화면 층 지우기, 무대 타이머/고정/프리셋, 장비·Companion·매크로·예약, 웹 가사·찬양 가져오기·저장한 부를 순서, 설교 포인트·홈페이지 주보·예배 전 화면, 사용 기록·동기화·백업·점검, 패널 도킹/독립 창·작업공간·접기·화면 테마, 한국어/영어, 업데이트 처리기, 설정 및 주요 추가 창 바깥 클릭 닫기.


