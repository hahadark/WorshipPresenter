# 워십프리젠터 Rust 판 — 이어서 작업하는 에이전트를 위한 핸드오프

Electron 판 워십프리젠터(`F:\aside\WorshipCue`, 저장소 `hahadark/WorshipPresenter`)를 Rust 로 다시 만드는 저장소.
2026-11-12 시작. 사용자(hahadark, 김택근 사관)가 **조작 화면까지 Rust(egui)** 를 골랐다 (Tauri 아님).
설명·커밋 메시지·주석은 한국어. 라이선스 GPL-3.0-or-later (Electron 판과 같음).

## 지키는 규칙

- **릴리스 정지 (2026-11-12 사용자 지시): Electron 판의 모든 기능을 옮길 때까지 릴리스·설치 파일·공개 저장소·GitHub 업로드를 하지 않는다.** 로컬 커밋만.

- 커밋: `git -c user.name=hahadark -c user.email=29118505+hahadark@users.noreply.github.com commit …`. 공동 작성자 표기는 실제 작업 모델과 일치시킨다. 챗GPT가 만든 새 작업을 클로드 작업으로 표기하지 않는다.
- **현재 작업은 사용자 지시로 챗GPT 구독 모델만 사용한다.** 다른 공급자나 별도 유료 API로 자동 전환하지 않는다.
- 개발 빌드·로컬 테스트는 구현 검증의 일부로 진행한다. 출시 빌드·릴리스·설치 파일 생성·푸시·GitHub 저장소 생성은 금지 상태를 유지한다. 커밋은 작업 단위마다.
- Electron 판 저장소와 사용자의 실제 자료(`%APPDATA%\WorshipCue\data`)는 건드리지 않는다. Rust 판은 처음 실행할 때 그 자료를 **복사**(SQLite `VACUUM INTO`)해서 `%APPDATA%\WorshipPresenter-rs\data` 에서 쓴다.
- 점검은 `--data <임시 폴더>` 로. 사용자가 띄워 둔 프로그램을 끄지 않는다.
- 저작권 있는 번역본·찬송가 자료(`data-personal/`)는 저장소에 넣지 않는다.

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

조작 화면 미리보기는 근사 그림이다(실제 출력은 웹). 글자 크기 맞춤은 `measure.rs` 근사라 Electron 과 슬라이드 나눔이 조금 다를 수 있다 — 실제 교회 자료로 비교 필요.
