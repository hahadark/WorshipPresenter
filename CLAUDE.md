# 워십프리젠터 Rust 판 — 이어서 작업하는 에이전트를 위한 핸드오프

Electron 판 워십프리젠터(`F:\aside\WorshipCue`, 저장소 `hahadark/WorshipPresenter`)를 Rust 로 다시 만드는 저장소.
2026-11-12 시작. 사용자(hahadark, 김택근 사관)가 **조작 화면까지 Rust(egui)** 를 골랐다 (Tauri 아님).
설명·커밋 메시지·주석은 한국어. 라이선스 GPL-3.0-or-later (Electron 판과 같음).

## 지키는 규칙

- **릴리스 (2026-11-15 사용자 지시로 정지 해제): 기존 저장소 hahadark/WorshipPresenter 의 정식판(v2.0.0~)으로 낸다.** 소스는 그 저장소의 `rust` 가지, Electron 판은 `main` 가지에 그대로. 설치 파일은 Electron 판 자리(%LOCALAPPDATA%\Programs\WorshipCue)·바로가기·제거 항목을 이어받고, `latest.yml` 로 Electron 판 자동 업데이트도 이어진다.

- 커밋: `git -c user.name=hahadark -c user.email=29118505+hahadark@users.noreply.github.com commit …`. 공동 작성자 표기는 실제 작업 모델과 일치시킨다. 챗GPT가 만든 새 작업을 클로드 작업으로 표기하지 않는다.
- **모델 (2026-11-15 사용자 지시): 클로드 페이블로 진행한다. 최근 확인한 작업자 모델은 claude-code/claude-fable-5-2.** 2026-11-12~13 일부 작업은 챗GPT 구독(gpt-6.1-sol)으로 했다. 별도 유료 API로 자동 전환하지 않는다. 클로드 사용량을 다 쓰면 멈추고, 회복 뒤 사용자가 지정한 예약으로 이어서 한다. 2026-11-16 01시·06시 예약은 끝났으므로 새 예약이 있는지 확인한다.
- 릴리스·푸시는 사용자가 시킬 때만. 커밋은 작업 단위마다.
- 릴리스 전 필수: `tools\parity\run.ps1` 로 조작 화면 그림과 실제 웹 출력을 나란히 비교해 차이가 없을 것 (2.0.0 에서 미리보기·LIVE 가 실제 출력과 달랐던 사고, 2026-11-15). 테스트 통과만으로 릴리스하지 않는다.
- 릴리스 순서: Cargo.toml 4곳 버전 → 커밋 → `tools\release.ps1 -Version X.Y.Z` (dist\ 에 Setup.exe·Portable.zip·latest.yml) → 설치 파일을 `/S /TEST /D=<임시>` 로 점검 → `git push origin master:rust` → `gh release create vX.Y.Z --target rust --draft` 로 세 파일 올리기 → 확인 뒤 `gh release edit vX.Y.Z --draft=false --latest`. 맥 빌드는 아직 없다.
- Electron 판 저장소와 사용자의 실제 자료(`%APPDATA%\WorshipCue\data`)는 건드리지 않는다. Rust 판은 처음 실행할 때 그 자료를 **복사**(SQLite backup API·임시 파일 검증 후 교체)해서 `%APPDATA%\WorshipPresenter-rs\data` 에서 쓴다.
- 점검은 `--data <임시 폴더>` 로. 사용자가 띄워 둔 프로그램을 끄지 않는다.
- 저작권 있는 번역본·찬송가 자료(`data-personal/`)는 저장소에 넣지 않는다.

## 2026-11-16 통합 점검 현재 상태

- 다섯 갈래의 밤사이 수정과 추가 안전성·입력 보완을 master에 합쳤다. 전체 자동 시험 430개 통과·19개 제외. 세부는 docs/integration-audit-2026-11-16.md.
- 공개판은 2.0.6(2026-11-26, 휴대폰을 카메라로 · 이름 자막 · 방화벽 확인·열기, Windows 만, 설치판으로 휴대폰 카메라 1920x1080 받음 확인, parity 43/44 같은 결과). 그 전 2.0.5(2026-11-26, 콘티 가져오기 · 초성 검색 · PPT 내보내기 · 곡 정보 · 가사 찾기 보강 · 인터넷 자료 안내, Windows 만 · 맥 dmg 는 2.0.3 그대로, parity 43/44 같은 결과). 그 전 2.0.4(2026-11-18): 예배당 화면이 켜지자마자 꺼지던 문제(교회 PC) — wp-output 의 WebView2 자료 폴더를 %LOCALAPPDATA%\WorshipPresenter-rs\WebView2 로 고정(설치 폴더에 쓸 수 없으면 0x80070005 로 종료), 스스로 꺼질 때 stderr 이유를 알림·로그에. 맥 dmg 는 2.0.3 그대로. parity 43/44(obs-pip-score 72/56 은 2.0.3 부터 있던 차이, 미해결).
- 이전 공개판 2.0.3(2026-11-16): 추가 창 바깥 누름 닫기·AI 큐시트 화면·끌어다 놓기·인쇄 옵션 등 1.x 누락 복원(docs/missing-features-2026-11-16.md), 맥 키체인. 이전 2.0.2: 설정 센터·테마 관리·템플릿 복제·예배당 화면 한 번에 켜기 포함. 맥(arm64) dmg 도 같은 릴리스에 있음(rust b542549, GitHub Actions mac.yml: mac-build 가지 push 또는 수동 실행 → 산출물 dmg 를 받아 릴리스에 올림). 맥은 비밀 글 암호화(키체인)·전역 단축키·PowerPoint 기능이 없음.
- 원본 처리기282개 중 미대조22개, 소스근거만158개다. docs/behavior-parity.md를 읽고 '전부 동일'로 보고하지 말 것.
- 편집기 입력 소유권은 App::ui_body 전체 합성시험으로 보완했다. 원본에는 잠금 단추가 없고 공통요소 제한이 그 동작이다. 원본에 없는 기능을 누락이라고 세지 말 것.
- 실제 OBS/카메라/Companion/여러 모니터 장비, 일부 첫 실행 지연·4K 프레임 튐은 추가 검증이 필요하다.

## 2026-11-25 찬양 콘티 스튜디오에서 가져온 기능 → 2.0.5 공개 (2026-11-26)

- 콘티(.worship) 가져오기 · 초성 검색 · 웹 가사 찾기 까닭 구분·보관 · 큰 폴더 목록 기억 · 곡 정보(Key·박자·BPM) · 큐시트 → PPT 내보내기 · 인터넷 자료 사용 안내. 세부는 docs/contistudio-and-choseong.md.
- 찬양 콘티 스튜디오(윤명중, AGPL-3.0) 코드는 옮기지 않았다. 파일 형식과 동작만 참고.

## 2026-11-26 쉬운방송에서 가져온 아이디어 → 2.0.6 공개 (2026-11-26)

- 이름 자막 · 방화벽 확인·열기 · 휴대폰을 카메라로(https 37560 · WebRTC) · 리모컨에 무대 메시지 표시. 세부는 docs/easylive-ideas.md. 휴대폰 카메라 점검 tools/phonecam/e2e.js.

## 2026-11-27 조작 화면 새 디자인 (나) 어두운 전문가용 + 예배송출 2차 기능 7개 → 2.0.8

- 주인이 2차 시안 (나) 를 고름(시안 원본 docs/design/). 적용 기록·자리 옮긴 조작·시안과 다른 곳: **docs/design-pro-apply.md**.
- 배치: 독 작업공간 → 고정 칸 (`prolayout_ui.rs`: 큐시트 판 · 뷰어 둘 · 본문·편집 · 인스펙터, 설정 `rsProLayout` {side, inspector, timeline, viewers}). 옛 `rsWorkspace` 는 읽기만 하고 지우지 않음. `workspace_ui.rs` 의 독 코드(workspace_content)는 이제 안 부름(보기 → '배치' 창만 씀).
- 위 컨트롤 막대 · 상태 표시창 · 아래 상태 줄(알림 포함) = `probar_ui.rs`, 예배 시간표(보기 전용) = `timeline_ui.rs`. 옛 머리 줄·송출 줄(top_bar · bottom_bar)은 지움.
- 테마: 새 기본 `pro`('전문가 (기본)'). 옛 '어둡게' 기본 사용자는 처음 한 번만 pro 로(`ui.proDesign`). 둥글기는 숫자 대신 `theme::cr(Rad::…)`, 높이 `theme::ctl_h()`, 자리 색 `pal().head/bar/edge/lcd/next/live_dk/send_bg`, `pal().bevel` = 전문가 테마(단추 위·아래 선). 빨강 = 송출 중, 노랑 = 다음(미리보기)만 — 기본 단추(Primary)에 노랑을 쓰지 말 것. 종류 아이콘 색 없음.
- **2.0.8 공개 (2026-11-27)**: Windows 설치판·포터블·latest.yml + 맥 dmg(mac-build CI, 시험 wp-app 232 · wp-core 314) 같은 릴리스, 내려받아 SHA-256 대조 같음. 설치판 /S /TEST 로 켜서 새 화면 확인. 시험 wp-app 233 · wp-core 315. parity 43/44 같은 글자 크기(obs-pip-image 는 2.0.7 부터의 차이), 슬라이드 그리는 코드는 안 바꿈. main 의 README 대표 그림 · 설명서 주 화면·막대 그림도 교체(나머지 그림 목록은 docs/design-pro-apply.md).
- 화면 문구에 색 이모지를 쓰지 말 것(egui 글꼴에 없음). 점검 캡처: `--win 1600x960 --item 6 --send --shot`.

## 2026-11-27 예배송출(worship-caster)에서 가져온 아이디어 → 2.0.7 공개 (2026-11-27)

- 공개: Windows 설치판·포터블·latest.yml + 맥 dmg(맥 CI 시험 통과) 모두 같은 릴리스, 내려받아 SHA-256 대조 같음. 시험 wp-core 302 · wp-app 227 (firstrun::copy_db_with_progress 는 전역 PROGRESS 를 함께 써서 병렬로 돌면 가끔 실패 — 혼자 돌리면 통과, 원래 있던 흔들림). parity 44/44 같은 글자 크기(obs-pip-score 차이 없어짐). 설치판 /S /TEST 로 켜서 '몇 시까지' 와 /api/trigger/timer?at= 확인.

- 순서지 글 복사에 곡 정보 [Key · 박자 · BPM] · 무대 타이머 '몇 시까지'(조작 화면 · 리모컨 · /api/trigger/timer?at=) · 예배당 화면 4:3(템플릿 → 위치와 여백 → 화면 비율, paint_slide 레터박스) · 다른 컴퓨터로 옮기기(.wpmove, movedata.rs). 세부·대조표·제안은 docs/worshipcaster-ideas.md.
- 예배송출 저장소에는 라이선스가 없다 → 코드는 옮기지 않고 동작만 참고. 맥: localtime.rs local_offset_ms 가 맥에서 0 이던 것을 SQLite localtime 으로 고침.
- 점검용 인자 추가: `--tpl-open <묶음>`(템플릿 편집 창 열기). 웹 출력 캡처는 앱을 `--search x --item N --send`(--shot 없이, 15초에 꺼지지 않게)로 띄우고 tools/flow-compare/cdp.js launch → open(url, w, h) → shot.

## 2026-11-27 밤: 예배송출 제안 일곱 가지 넣음 (아직 릴리스 전 — 주인이 "릴리즈"라고 하면 2.0.8)

- 예배 틀 묶음(큐시트 관리 '예배 틀에서…') · 다른 콘티 .json 가져오기(contijson.rs) · 손상 자동 복구(sync.rs recover_from_backup, 원본은 damaged\ 에) · 웹 편집기 PIN(webedit.rs · web/editor, 기본 꺼짐, PIN 없으면 127.0.0.1 만) · 유튜브 주소 재생(youtube.rs · render.js 임베드 · output.js syncYouTube) · 움직이는 배경(템플릿 background.motion, render.js motionLayer · wp-app motion.rs) · PDF(pdf.rs, Windows.Data.Pdf · 맥 PDFKit). 쓰는 법 · 한계 · 확인 못 한 것은 docs/worshipcaster-ideas.md '2차'.
- 시험 wp-core 315 (19 제외) · wp-app 229 통과. parity 43/44 (obs-pip-image 72/46 은 2.0.7 코드로도 같음). 점검 도구 tools/webedit/e2e.js · tools/youtube/e2e.js. 점검 인자 `--frame <열쇠>` · `--pdf <파일>` · `--tool webedit`.
- 점검 격리 주의: Git Bash 에서 `WP_LEGACY_DATA` 를 `/c/...` 꼴로 주면 무시되고 실제 1.x 자료를 본다 → `cygpath -w` 로 Windows 경로를 줄 것.
- 새 화면 문구는 en.json 맨 앞에 넣었다. 웹 편집기 페이지(web/editor)는 한국어만 (영어 사전을 붙이지 않음).

## 2026-11-27 AI 도우미 일곱 가지 (가지 ai-features, 아직 릴리스 전)

- 예배 전 큐시트 점검 · 설교 원고 → 슬라이드 · 음성 인식(설교 구절 따라가기 · 방송 실시간 자막·번역, whisper.cpp 를 처음 켤 때 받음) · 가사 자동 정리 · 예배 후 정리 · 말로 시키기 · AI 설정. 모두 도구 메뉴. 세부 · 실측 · 확인 못 한 것은 docs/ai-features.md.
- 지킬 것: 음성 인식 결과는 '다음' 칸까지만(송출 금지) · 자막은 기본 방송 출력에만 · 말로 시키기에 송출·삭제 단계를 더하지 말 것 · 받는 파일은 stt.rs 의 SHA-256 대조.

## 구조

| 경로 | 내용 |
|---|---|
| `crates/wp-core` | 코어 (egui 와 무관). `books`·`parser`(장절)·`textnorm`·`db`(bible.db 그대로)·`settings`(settings.json 그대로, `serde_json::Value`)·`slides`(슬라이드 나누기, Electron `shared/slides.js` + `cue.js buildSlides`)·`measure`(글이 화면에 들어가는지: 맑은 고딕 ab_glyph 로 근사, Electron 은 DOM 측정)·`presentation`(미리보기/LIVE)·`core`(상태·출력 데이터·명령)·`server`(axum, 출력 웹 화면·SSE) |
| `crates/wp-app` | 조작 화면 (eframe/egui 0.37). `app.rs` 화면·단축키, `paint.rs` 미리보기·LIVE 를 egui 로 직접 그림(그라데이션·로고·상자·외곽선 근사), `outwin.rs` 예배당 출력 창 프로세스 |
| `crates/wp-output` | 예배당 모니터 출력 창 (tao + wry = WebView2). `--list` 모니터 목록, `--monitor N`, `--windowed` |
| `web/` | Electron 판 `src/web/{output,shared,remote}` 를 그대로 복사. 예배당 창·OBS 브라우저 소스·무대 화면은 웹으로 남긴다 (OBS 때문에) |
| `crates/wp-core/data/*.json` | Electron 판에서 뽑은 책 목록·기본 설정·템플릿 기본값·단축키 (`node -e` 로 생성) |

- 조작 화면 ↔ 코어: `Arc<parking_lot::Mutex<Core>>`. 화면은 프레임마다 `Snap` 을 떠서 그리고, 누른 것은 `Act` 로 모아 끝에 적용.
- 코어 → 출력: `tokio::sync::broadcast` (`Ev::Live` 면 SSE 로 `render` 를 다시 보냄). 데이터 모양(`outputPayload`·`stagePayload`)은 Electron 판과 같아서 `web/output/*.js` 를 고치지 않고 쓴다.
- 주소: `http://127.0.0.1:37528/outputs/main` · `/outputs/overlay` · `/outputs/stage` (포트가 쓰이고 있으면 다음 포트).

## 명령

| 할 일 | 명령 |
|---|---|
| 단위 테스트 | `cargo test -p wp-core` |
| 개발 실행 | `cargo run -p wp-app -- --data <임시 폴더>` |
| 점검 실행 (스크린샷 저장 뒤 종료) | `target\debug\wp-app.exe --data <폴더> --search "요3:16" --send --shot shot.png` · `--item 6` (큐시트 7번째 순서 미리보기) |
| 출시 빌드 | `cargo build --release` → `target\release\wp-app.exe` + `wp-output.exe` (+ `web\` 은 실행 파일 옆 또는 저장소의 `web\`) |

Rust 는 rustup(winget) 으로 설치: `%USERPROFILE%\.cargo\bin` (새 셸이면 PATH 에 앞에 붙이기).
egui 0.37 은 API 가 바뀌었다: `App::ui(&mut self, ui, frame)`, `egui::Panel::left/top(...).show(ui, …)`, `CentralPanel::default_margins().show(ui, …)`, `ctx.request_screenshot(콜백)`, 그림 주소는 윈도우에서 `file:///C:/…` (슬래시 셋).

## 1단계 (2026-11-12) 된 것

- 성경: 장절 검색(Electron 파서 그대로 — 요3:16, 요 3 16, 롬8:1,28, 요한1서…), 낱말 검색(FTS trigram + LIKE), 번역본 고르기, 절 누르면 그 절부터 미리보기, 두 번 누르면 송출
- 큐시트: 기존 큐시트 열기·새로 만들기, 순서 넣기(성경·찬양·공지·제목·빈 화면·그림)·옮기기·지우기·고치기, 누르면 미리보기·두 번 누르면 송출, 순서 끝에서 F6 → 다음 순서
- 슬라이드: 성경(화면에 맞춰 절 묶기·긴 절 나누기·(1/2) 표시), 찬양 가사(이름표·부를 순서), 공지(--- 나눔·교독 역할), 제목, 빈 화면, 그림, 카메라·영상·슬라이드쇼(출력은 웹이 그림)
- 송출: 미리보기/LIVE, F5·F6·F9·F10·Space·PageUp/Down·Ctrl+↑↓·Ctrl+K·Ctrl+Shift+F, 글자 숨김·배경 끄기·로고·모두 지우기, 템플릿 고르기(예배당·OBS 자막), '미리보기 거쳐 송출' 끄기
- 출력: 예배당 모니터 창(wp-output), OBS 주소 복사, 출력 연결 수 표시, `/api/ack`, 라이브러리 파일(Range)

## 현재 통합 상태 (6차 진행 중)

- 성경 번역본 분석·가져오기·삭제, 베들레헴 폴더 일괄 가져오기, EPUB 주석 가져오기·조회·삭제를 통합했다.
- 찬송가·PPT, 라이브러리·그리기·백업, 템플릿·설정, 리모컨·장비 연동과 영어 화면은 각 트랙에서 구현되어 있으며 통합·회귀 검증 중이다.
- 작업 트리별 미커밋 변경을 버리지 않는다. `git status`와 각 브랜치 로그를 확인하고 재개한다.
- `PARITY.md`는 소스 대조 결과이며, '됨'이 실제 장비나 모든 UI 흐름의 실행 검증을 뜻하지 않는다. 검토에서 드러난 누락·오류를 반영해 갱신한다.
- 현재 우선 검증: LIVE 큐와 열린 큐 불일치, 음악 순서의 화면 유지, 편집기 세부, 백업·썸네일, 업데이트 상태 기계. 실제 배포·설치 시험은 하지 않는다.

## 최초 로드맵 (역사 기록, 현재 미구현 목록이 아님)

1. **찬송가·PPT 순서(hymn)** — 사용자 큐시트에 가장 많다. 먼저 Electron 판 `hymn-cache` 의 SVG/PNG 를 다시 쓰고, 그 뒤 PPTX 를 Rust 로 직접 읽기 (Electron 판은 이음오피스 JS 엔진). 베들레헴 찬송가(hymnal)
2. 찬양 라이브러리(Songs) · 가사 웹 찾기 · 곡 가져오기
3. 템플릿 편집기 · 테마 · 설정 화면 (단축키·출력·글꼴)
4. 슬라이드 편집(edits: 글상자·그림·도형·꾸미기), 슬라이드 차례·구역, 자동 넘김·글자 자동 숨김
5. 오버레이·회중 메시지·무대 화면(타이머·고정)·매크로·예약
6. 휴대폰 리모컨 · 스트림덱 `/api/trigger` · OBS websocket · Companion · 장비(X32·PTZ) · 내부망 출력
7. 여러 PC 동기화 · 백업 · 사용 기록(저작권 보고)
8. 주보 → 큐시트(규칙·AI) · 설교 포인트 · 예배 전 화면 · 홈페이지 주보 · PPT 나누기 · 클립보드 붙여 넣기
9. 설치 파일(NSIS 등) · 자동 업데이트(GitHub 릴리스) · 영어판 · 맥
10. 실행 취소, 끌어서 놓기(파일·순서), 한글 입력 자동 전환

조작 화면 미리보기·LIVE·썸네일·편집기 그림(paint.rs paint_slide)은 웹 출력(render.js) 규칙을 그대로 옮겨 그린다 — 자리·크기 맞춤·줄바꿈이 웹과 같다(docs/slide-parity.md, 점검 --parity). 영상·카메라는 재생하지 않는다. 글자 크기 맞춤은 `measure.rs` 근사라 Electron 과 슬라이드 나눔이 조금 다를 수 있다 — 실제 교회 자료로 비교 필요.

## 점검 실행 격리 주의 (2026-11-16)

`--data <빈 임시 폴더>`만으로는 격리되지 않는다. DB가 없으면 실제 Electron 자료와 설정을 복사해 미리 꺼둔 통계·업데이트·리모컨 설정을 덮을 수 있다. 점검 전에 빈 SQLite DB를 만들거나, `WP_LEGACY_DATA`를 bible.db가 없는 별도 빈 폴더로 지정한다. autoUpdate/telemetry/globalShortcuts/remote/lanOutput/output.autoOnSend와 syncDir를 다시 확인한다. 개발판 통계는 기본 차단되어 있지만 이것을 점검 격리 대신 믿지 말 것.

## 2026-11-27 소스 · 화면 구성 로드맵 1단계 (sources 가지, 아직 릴리스 전)

- 설계 · 확인 결과: docs/roadmap-sources.md (로드맵 1~5단계 · 소스/구성/출력 모델 · 옮김 규칙 · 단계별 위험 · 화면 캡처 · 카메라 동시 사용).
- 코어 `sources.rs`: 내장 소스(background · media · slide-bg · live · overlay:<id> · message · logo-screen · draw · pip · camera:<id>) + 사용자 소스 설정 `sources[]`(지금은 웹 페이지만, 덧붙인 키). `output_payload` 에 `scene`(기본 구성 full) · `sources` 덧붙임.
- web/output/output.js: 무대 아래는 소스마다 `.wp-layer` 하나, 구성 차례대로. 층 상자는 z-index·opacity 를 안 줘서 그림이 2.0.8 과 화소까지 같음 (확인함). 있는 층은 차례가 맞으면 옮기지 않는다 (옮기면 영상·카메라·iframe 이 끊김).
- 조작 화면: 인스펙터 [글자 | 무대 | 미디어 | 소스]. 점검 인자 `--ins-tab sources`.
- 화면 캡처는 WebView2 가 허락 창 없이 할 수 없어 넣지 않음. 카메라 동시 열기는 가상 카메라로만 확인 (실제 UVC 는 교회 PC 에서 시험 필요).

## 2026-11-27 소스 · 화면 구성 로드맵 2단계 — 화면 구성 편집기 (sources 가지, 아직 릴리스 전)

- 쓰는 법 · 저장 형식 · 그리는 규칙 · 확인 · 3단계 전에 정할 것: docs/roadmap-sources.md '2단계 한 것'.
- 코어 `scenes.rs`: 설정 `scenes[]` · `outputScenes` {main, overlay} (덧붙인 키), 기본 틀 5개, `effective_scene`(맨 위 오버레이 · 메시지 · 로고 화면 · 그리기 자동), `box_template`(칸 크기 글자 맞춤). `Core::scene_for` 가 고른 구성 → 없으면 기본 full.
- 조작 화면 `scene_ui.rs` (별도 창, 보기 → 화면 구성 편집…, 점검 인자 `--scene-open [full|split|pip|lower|three]`), 뷰어는 `paint::paint_scene`.
- 웹: render.js `sceneNeedsBox · sceneTemplate · sceneBoxCss · sceneHost`, output.js 칸 상자 · 카메라 칸(`.wp-cam`). 기본 구성은 무대에 바로 그려 2.0.8 과 화소까지 같음.
- 점검: `node tools/scenes/e2e.js --exe targetdebugwp-app.exe --old-exe <2.0.8 web 을 옆에 둔 exe> --data tmpdata --out tmpscene-e2e --fake-cam`. **이 PC 의 OBS Virtual Camera 는 화면 내용을 내보낼 수 있으니 캡처는 --fake-cam 으로.**

## 2026-11-27 소스 · 화면 구성 로드맵 3단계 — 출력마다 다른 구성 (sources 가지, 아직 릴리스 전)

- 쓰는 법 · 저장 형식 · 확인 · 4단계 전에 정할 것: docs/roadmap-sources.md '3단계 한 것'.
- 코어 `outputs.rs`: 추가 출력 `extraOutputs[]`(/outputs/out-…, kind main|overlay 로 같은 종류 내장 출력처럼) · 종류별 기본 `typeScenes` · 순서 `scenes` · 소리 `output.audioOutput`. 구성 고르는 차례 = 순서 → 종류 기본 → 출력 기본 → full (`Core::scene_choice_for_deck`).
- 조작 화면: 인스펙터 소스 탭 '화면 구성'(`sources_ui.rs scene_section`), 출력 설정 창 '출력' 구획 · 추가 출력 창(`outputs_ui.rs`), 도구 → 카메라 동시 열기 시험(`camtest_ui.rs` · 코어 `camtest.rs` · `/helper/camtest`), 구성 편집기 Alt+누름 = 겹친 아래 칸 차례로. 점검 인자 `--open-d camtest`.
- 점검: `node tools/scenes/e2e-outputs.js --exe target\debug\wp-app.exe --data tmp\data --out tmp\outputs-e2e --fake-cam` (시험 기준: 성경봉독 · 찬양 순서마다 예배당 · 방송이 각자 바뀜, cue-next 로 넘김).

## 2026-11-27 소스 · 화면 구성 로드맵 4단계 — 전환 · 자동화 + AI 도우미 7개 합침 (sources 가지, 아직 릴리스 전)

- 쓰는 법 · 저장 형식 · 확인 · 제안: docs/roadmap-sources.md '4단계 한 것'. AI 는 docs/ai-features.md. 새 창 · 구획 전체 목록과 캡처(다음 인터페이스 다듬기용): **docs/ui-review-sources-ai.md** (docs/images/review/).
- 코어 `scenectl.rs`: 전환(`sceneTransition` · `outputTransitions` · 구성 `transition`, 기본 move 0.4초), '지금 한 번'(`b.scene_once`, 다음 순서까지) · `/api/trigger/scene?output=&id=&hold=1`, 순서 동작 `actions.scene`(출력 기본, 남음). 고르는 차례 = 지금 한 번 → 순서 → 종류 → 출력 기본 → full.
- 웹 출력: 카메라 · 웹 칸은 같은 요소 그대로 새 자리로(FLIP, Web Animations), 템플릿이 바뀌어도 옮김. `<body data-scene>` 점검용.
- AI 5번 실시간 자막 = 소스 `caption` (띄운 동안만 기본 구성 칸, 구성에 넣을 수 있음). AI 메뉴 시험은 `ai_ui::dry()` 로 창을 열지 않음(전역 창 상태가 다른 화면 시험을 가렸음).
- 점검: `node tools/scenes/e2e-auto.js --exe target\debug\wp-app.exe --data tmp\data --out tmp\auto-e2e --fake-cam` (20개).

## 2026-11-27 인터페이스 다듬기 (sources 가지, 아직 릴리스 전)

- 목록 · 결정 · 용어집 · 전후 점수 · 제안: **docs/ui-polish.md**. 캡처 `node tools/polish/shots.js` (95장, 격리 · `COMPUTERNAME=TEST-PC`), 외부 판정 `tools/polish/judge.py` · `score.py` · `table.py` · 쌍 비교 `compare.py` (키는 OPENROUTER_API_KEY 환경변수만).
- 창 안 양식은 공용 부품으로: `widgets::form_row` / `form_label`(오른쪽 맞춤 이름표 칸) · `group_head`(구획 머리) · `empty_state` · `status_mark`(✔/✖ 글자 대신 아이콘 — 글꼴에 따라 색 이모지로 나옴). 새 창은 `widgets::float_window` (접기 삼각형 없음 · 화면 안 · 높이 한도).
- 용어: 예배당 화면 = Sanctuary screen, 화면 구성 = Layout(s) (Scene 은 OBS 장면), 내장 출력 이름 'OBS'. 전문가 테마는 오류 글 주황 · 성공 글 회색 (빨강 = 송출 중만).

## 2026-11-27 2.1.0 공개 (소스 · 화면 구성 1~4단계 + AI 도우미 7개 + 인터페이스 다듬기 1차)

- https://github.com/hahadark/WorshipPresenter/releases/tag/v2.1.0 (Latest, 태그 v2.1.0 = rust 27ff6df). sources 를 rust 에 빨리 감기로 합침. 자산 4개(Setup.exe · Portable.zip · latest.yml · mac-arm64.dmg) 올린 것과 내려받은 것 SHA-256 같음, 공개 latest.yml = 2.1.0.
- 맥 dmg: mac-build 가지에 2.1.0 커밋을 push → mac CI(run 45966446797, wp-app 244 · wp-core 371) 산출물. 시험 wp-app 245 · wp-core 372, parity 46/48(원래 차이 둘), scenes e2e 8 · outputs 9 · auto 20.
- 설치판 /S /TEST /D=<임시> 로 2.0.8 · 2.1.0 설치·실행 (실제 설치본 · 레지스트리 그대로 확인). 2.0.8 자료 사본 → 2.1.0 → 다시 2.0.8 로 열어도 설정 키 잃음 없음 · 템플릿 · 큐시트 창 정상. 임시 2.0.8 이 '새 버전 2.1.0' 을 찾음(검증 다운로드 단추까지, 실제 설치는 안 눌러 봄).
- main: README · 사용 설명서에 '화면 구성' · 'AI 도우미 일곱 가지' 절, 대표 그림 교체(readme-main · readme-scenes · readme-ai).
- 2.0.8 의 '새 버전' 창은 릴리스 설명 마크다운을 날것으로 보여 준다 (2.1.0 에서도 같은 코드 — 다음 판에서 다룰 것).
- 다음 판 릴리스는 주인이 말할 때. 순서는 위 '릴리스 순서' 그대로 (rust 가지에서: Cargo.toml 4곳 → 빌드 · 시험 · parity · scenes e2e → tools\release.ps1 → /S /TEST 설치 확인 → push rust · mac-build → gh release create --target rust --draft → 해시 대조 → --draft=false --latest).

## 2026-11-27 인터페이스 다듬기 2차 (rust 가지, 릴리스 전)

- 1차 제안 전부: 전문가 테마 선택색 회색 · 접힌 판 손잡이 · 송출 중 꺼짐 알림 하나(뷰어 띠 + 상태 줄 칸) · 메뉴 괄호 설명 오른쪽 흐린 글 · 새 테마 '전문가 밝게'(pro-light) · 큐시트 관리 · 테마 관리 · 템플릿 편집 · 음성 인식 모델 · 주 화면 크기 맞춤. 세부 · 잰 값 표 · 쌍 비교: docs/ui-polish.md '2차 다듬기'.
- 새 공용: `widgets::segmented_cells`(칸마다 사각형 → 시험 표시), `theme::lift` · `theme::strong_text`(밝은 전문가 테마에서 흰 글 · 흰 hover 를 쓰지 말 것), `menubar::split_desc`.
- 전문가 테마(어둡게 · 밝게) 공통 규칙: `p.bevel` 로 가르고, 밝은 쪽은 `p.dark == false`. 노랑(accent · next)은 '다음' 에만, '켜짐 · 고름' 은 sel · text2 회색.

## 2026-11-27 뷰어 하나만 보기 (rust 가지, 릴리스 전)

- 보기 방식 [둘 다 | 송출 중만]: 뷰어 머리 구획 단추 · 보기 메뉴 · Ctrl+Shift+L(단축키 id `toggleViewers`) · 설정 `rsProLayout.viewerMode`. 바로 송출이면 저절로 송출 중만(`ui.directLiveOnly`, 기본 켜짐).
- '송출 중만' = 송출 뷰어를 가운데 판 오른쪽 칸(`prolayout_ui.rs pro_center_live_only`), 본문·편집은 왼쪽 전체 높이, 미리보기 조작은 노랑 '다음' 줄(`app.rs next_strip`). 비교 · 까닭 · 시험: docs/viewer-single.md, 캡처 `node tools/viewers/shots.js`.
