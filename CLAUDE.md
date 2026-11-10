# 워십프리젠터 (WorshipPresenter, 옛 이름 WorshipCue) — 이어서 작업하는 에이전트를 위한 핸드오프

> 2026-10-31 이름 변경: 화면·설치 파일·문서의 이름만 '워십프리젠터'(영어 Worship Presenter, 파일 WorshipPresenter-…)로 바꿨다. **저장소** 도 같은 날 hahadark/WorshipCue → **hahadark/WorshipPresenter** 로 바꿨다 (옛 주소는 GitHub 이 301 로 넘겨줌, package.json publish · updater.js REPO 도 새 이름). **바꾸지 않은 것**: appId org.worshipcue.app · 데이터 폴더 %APPDATA%\WorshipCue (main.js 에서 userData 도 여기로 고정) · 문서\WorshipCue 찬양 PPT · 동기화 파일 WorshipCue-sync.json · X-WorshipCue 머리글 · 코드 이름(WorshipCueCore, wc- 저장소 키, 패키지 name worshipcue).

교회 예배 순서(큐시트) 통합 송출 프로그램. Electron 44 + node:sqlite, 조작 화면은 바닐라 JS. 저장소 `hahadark/WorshipPresenter`.
**라이선스: GPL-3.0-or-later** (2026-11-09 사용자 결정 — 수익화 계획 없음, 소스 공개 괜찮음. 그 전에는 UNLICENSED). 그래서 FreeShow(GPL-3.0) 코드도 가져다 쓸 수 있다. 가져온 공개 코드·글꼴은 `THIRD_PARTY_NOTICES.md` 와 설정 → 정보의 목록에 적는다 (Apache-2.0 은 NOTICE 내용도). 설치판에는 resources/LICENSE.txt · THIRD_PARTY_NOTICES.md 로 들어간다.
사용자(hahadark)는 한국어로 대화하고, 설명·커밋 메시지·주석도 한국어로 쓴다. 데모 데이터는 홍길동·○○교회처럼 가상으로.

## 지키는 규칙 (꼭)

- **커밋 작성자**: `git -c user.name=hahadark -c user.email=29118505+hahadark@users.noreply.github.com commit …`
  커밋 메시지 끝에 `Co-Authored-By: Claude <모델명> <noreply@anthropic.com>` 한 줄. PR 본문 끝에는 `🤖 Generated with [Claude Code](https://claude.com/claude-code)`.
- **빌드·릴리스·푸시는 사용자가 시킬 때만.** 커밋은 작업 단위마다 해도 된다.
- **GitHub 에 절대 올리지 않는 것**: `data-personal/`(저작권 있는 번역본·찬송가 `.hdb`/`.cmp`), 개인용 빌드, `telemetry/.stats-key`. 모두 `.gitignore` 에 있다. 새찬송가 악보(`.cmp`, 100MB 넘음)도 저작권 때문에 저장소·공개 릴리스에 넣지 않는다.
- **개인용 빌드는 빌드할 때마다** `D:\OneDrive - The Salvation Army Korea Territory\첨부 파일\` 에 복사한다 (`WorshipPresenter-<버전>-개인용-Setup.exe` · `…-Portable.exe`). 옛 버전은 지우지 않는다. 이 폴더는 작업 폴더 밖이라 앱의 reveal_path 로 열리지 않는다.
- 사용자가 띄워 둔 워십프리젠터(WorshipCue) 를 죽이지 않는다 (점검은 다른 포트·임시 데이터 폴더로).
- 사용자에게 보내는 파일(SendUserFile)은 30MB 미만.
- 줄 끝: 저장소 파일은 LF(일부 CRLF). 편집은 Edit 도구나 `\r\n` 을 보존하는 스크립트로. `sed` 로 `\n` 이스케이프가 든 JS 를 고치면 망가진다.

## ▶ 이어서 할 일 (2026-11-10 넘김): 1.14.0 의 PowerPoint 연동을 실제 PowerPoint 로 확인 — 아직 하나도 못 함

사용자가 맡긴 확인이다. 맡은 세션이 PowerPoint 없는 리눅스 클라우드 컨테이너여서 **확인은 하나도 못 했고**, PC 에서 돌릴 도구 `tools/ppt-check.js` 만 만들었다 (브랜치 `claude/wizardly-franklin-b51ueh` = main + 이 도구). 끝내면 이 절은 지운다.

- **할 곳**: PowerPoint 가 깔린 윈도우 PC (`reg query HKCR\PowerPoint.Application\CurVer`). 없으면 그 사실만 알리고 끝낸다.
- **지킬 것**: 사용자가 열어 둔 프레젠테이션은 닫거나 저장하지 않는다(확인용으로 연 것만 닫음) · 띄워 둔 워십프리젠터를 끄지 않는다(다른 포트·임시 데이터 폴더) · 시작 전에 "클립보드 내용이 바뀐다"고 한 줄 알린다 · 코드를 고치면 main 에서 새 브랜치에 커밋만, 푸시·빌드·릴리스는 시킬 때만.
- **결과는** 항목마다 '확인됨 / 다름(어떻게) / 확인 못 함(왜)' 으로 짧게.

| 알아낼 것 | 도구의 어느 줄 |
|---|---|
| ①-1 PowerPoint 에서 슬라이드를 복사하면 클립보드에 올라오는 형식 이름 (코드가 기대: `PowerPoint 12.0 Internal Slides` — `clippaste.js` SLIDE_FORMATS) | `result.txt` 의 `①-1` 목록 |
| ①-2 그 형식의 내용이 .pptx 와 같은 zip 인가 (`looksLikePptx`) · `PS_CLIP_DUMP` 가 `OK` / `HAVE` / `NONE` 중 무엇을 내나 · 저장된 파일이 PowerPoint 에서 열리나 | `①-2` 의 출력 · looksLikePptx · `PowerPoint 로 열기: OPEN …` |
| ①-3 Electron `clipboard.readBuffer(형식 이름)` 으로 같은 내용이 읽히나 (윈도우의 `clipboard.has` 는 없는 형식에도 true 를 준다 — 그래서 `probeSlides` 가 slides true 여야 Ctrl+V 가 바로 슬라이드로 넣는다) | `①-3` 의 has · readBuffer · sha1 견주기 · `probeSlides → …` |
| ①-4 `PS_CLIP_PASTE` 가 `OK <장수> keep` 을 내나 · 저장된 파일의 테마·모양이 원본과 같은가 · 4:3 원본을 붙였을 때는 | `①-4` 의 출력 · 테마 줄 · `견주기` 의 다른 픽셀 % · 4:3 쪽의 슬라이드 비율 · `compare.html` |
| ①-5 실제 프로그램에서 PowerPoint 에서 복사한 슬라이드를 Ctrl+V → 찬송가·PPT 순서로 들어가나, 그림 한 장으로 들어가나 · Ctrl+Shift+V → 'PowerPoint 슬라이드' 는 | 눈으로 + `result-log.txt` 의 `슬라이드 붙여 넣기: package / keep / plain → …` 줄 · pasted 폴더 |
| ②-1 XML 의 `cNvPr id` 가 COM 의 `Shape.Id` 와 같은가 (그룹 안 도형 포함 · 그룹 안 도형에 `Visible = 0` 이 되는가) | `②-1` 의 같음/다름 · `그룹 안 도형: …(숨기기 ok)` |
| ②-2 `글·그림 따로 고치기…` 를 눌렀을 때 꺼낸 요소가 배경 그림(`slide-NNN-bg.png`)에서 빠져 있나 · 글상자·그림 위치가 원래 슬라이드와 겹쳐 보이지 않나 · `원래대로` 가 되나 (`elements.pptx` · `chart-table.pptx`) | `②-2` (요소 자리 밖에서 달라진 픽셀은 0 근처, 자리 안은 0 보다 커야 함) · 눈으로 + `result-log.txt` 의 `bgFrom powerpoint` |

차례: `node tools/ppt-check.js` (①1~4 · ② 자동, 2~3분) → `node tools/ppt-check.js app` (임시 데이터 폴더 · 포트 37620 으로 프로그램) → PowerPoint 에서 `tests\fixtures\elements.pptx` 의 1·2장을 손으로 복사 → `node tools/ppt-check.js clip` → 뜬 프로그램의 큐시트에 Ctrl+V · Ctrl+Shift+V → 두 픽스처를 큐시트에 넣고 `글·그림 따로 고치기…` · `원래대로` → `node tools/ppt-check.js log`. 결과는 `%TEMP%\wc-ppt-check\out\` 의 `result.txt` · `result-clip.txt` · `result-log.txt` · `compare.html`.

- 도구는 코드의 `PS_CLIP_DUMP` · `PS_CLIP_PASTE` · `PS_BG` 를 고치지 않고 그대로 돌린다. 도구가 따로 쓰는 PowerShell 조각(`PS_HOLD` 원본 열어 복사해 두기 · `PS_FORMATS` · `PS_OPEN` · `PS_SHAPES`)은 **실제 윈도우에서 돌려 보지 못했다** — 리눅스에서 가짜 `powershell.exe` 로 흐름만 봤다. 오류가 나면 도구부터 고친다. 윈도우에서 직접 PowerShell 을 돌릴 수 있는 에이전트라면 도구 없이 위 표를 하나씩 해도 된다.
- 볼 만한 곳 (추측, 확인 안 됨): PowerPoint 가 떠 있지 않을 때 COM 으로 새로 뜬 PowerPoint 는 창이 안 보여서 `PS_CLIP_PASTE` 의 `Activate` · `ExecuteMso('PasteSourceFormatting')` 가 안 되고 `plain` 으로 떨어질 수 있다 · `-EncodedCommand` 로 돌린 PowerShell 의 오류는 `#< CLIXML` 로 시작해 `runPs` 가 기록에 남기는 첫 줄이 쓸모없을 수 있다.
- **끝낸 뒤**: 다른 점은 고치고 `npm test` 를 맞춘다 → 이 문서의 '실제 PowerPoint 로 확인하지 못했다' 메모(아래 표의 'PPT 요소 따로 고치기' · '클립보드 붙여 넣기' 줄, 표 아래 문단, '최근 흐름' v1.14.0 줄)와 `src/core/clippaste.js` 머리말의 같은 주석을 확인한 사실로 바꾼다 → 이 절을 지운다.

## 자주 쓰는 명령

| 할 일 | 명령 |
|---|---|
| 단위 테스트 (45개) | `npm test` (~1분) |
| 실제 프로그램 점검 (18개, 숨은 창) | `npm run test:e2e` (~3분, `tests/e2e/run.js` → `smoke.js`, 결과 `tests/e2e/out/`) |
| 영어 사전 | 문구 추가는 `i18n/en.json` 맨 앞(`{` 다음)에 넣는 스크립트로, 그 뒤 `node tools/i18n-build.js` → `src/web/shared/i18n-en.js`. 빠진 문구는 `node tools/i18n-extract.js` → `tools/i18n-missing.json` |
| 공개 릴리스 | `npm version X.Y.Z --no-git-tag-version` → 커밋 "X.Y.Z" → `git push` → `gh workflow run release.yml -f tag=vX.Y.Z -f ref=main` → `gh run watch <id>` → 초안 확인 → `gh release edit vX.Y.Z --draft=false --latest --notes-file <노트>` |
| 개인용 빌드 | `npx electron-builder --win --config build-personal.json --publish never` → `dist-personal/` → 원드라이브 첨부 파일 폴더로 복사 |
| 스크린샷 (디자인 점검) | `tests/e2e/run.js` 와 같은 방식: 임시 데이터 폴더로 앱을 띄우고 두 번째 Electron 에서 `offscreen` 창에 `/control/` 을 열어 `capturePage()` |
| 실제 PowerPoint 로 확인 (윈도우 · PowerPoint 가 깔린 PC) | `node tools/ppt-check.js` (클립보드의 슬라이드 ①1~4 · 도형 id · PS_BG 배경 — 코드의 PS_CLIP_DUMP · PS_CLIP_PASTE · PS_BG 를 그대로 돌림) → `… app`(임시 데이터 폴더 · 포트 37620 으로 띄우기) → `… clip`(손으로 복사한 클립보드로 다시) → `… log`(프로그램 기록 모으기). 결과 `%TEMP%\wc-ppt-check\out\result*.txt` · `compare.html`. 클립보드가 바뀐다. 이 도구 자체는 가짜 PowerShell 로 흐름만 돌려 봤다 (2026-11-10) |

릴리스 워크플로(`.github/workflows/release.yml`)는 windows job 에서 `npm test` → `npm run test:e2e` → 한국어·영어 빌드, mac job 에서 dmg 를 만들어 **초안** 릴리스에 올린다. `gh release view` 에는 `isLatest` 가 없으니 `gh release list` 로 확인.

## 구조 한눈에

- `src/main/main.js` — Electron 메인 (창·보조 화면·업데이트·텔레메트리·동기화 시작·종료 시 `core.sync.stop()`)
- `src/core/core.js` — 코어. `core.commands` 는 객체 리터럴(`this` = core), 보조 함수는 모듈 수준에. 명령은 `/cmd` 로 조작 화면이 부른다(`window.BC.cmd(name, args)`).
  주요 모듈: `bulletin.js`(주보 → 큐시트 규칙 파서 `parseOrderText` · `planToCue`), `sermonpoints.js`(설교 원고 → 대지 → '제목' 순서, 규칙·오픈라우터·채팅 AI — 화면은 `control/points.js`), `preservice.js`(예배 전 화면 자동 만들기: 말씀카드 폴더 최근 묶음 · 설교쇼츠 '홈페이지 묶음' cardLine · QT 첫날 본문 → 슬라이드쇼, 화면은 `control/preservice.js`. `BCCue.addItem(type, { …, _at: 0 })` 으로 맨 앞에), `homefeed.js`(홈페이지 주보 JSON → 큐시트·광고 슬라이드, 화면은 `control/home.js`. 대구사랑교회 홈페이지 저장소 `sarmydglove/daegu-sarang-church` 의 `site/src/pages/bulletin/feed.json.ts` 가 그 JSON 을 낸다 — https://구세군대구사랑교회.kr/bulletin/feed.json), `usagelog.js`(사용 기록, 저작권 보고), `sync.js`(여러 PC 동기화 `WorshipCue-sync.json` · 하루 1회 백업·되돌리기), `hymnal.js`(베들레헴 찬송가 가사·악보), `presentation.js`, `server.js`(127.0.0.1:37528, SSE)
- DB: `bible.db` 하나에 성경 + 찬양 + 큐시트 + 사용 기록 (WAL). `data/`(공개) · `data-personal/`(개인)
- `src/web/control/` — 조작 화면. `index.html` 기본 틀, `dock.js`(dockview 패널 배치·작업공간), `menubar.js`(상단 메뉴), `ctxmenu.js`(오른쪽 클릭), `cue.js`(큐시트·편집칸·실행 취소), `library.js`, `settings.js`, `usage.js`, `ai.js`(주보 붙여 넣기 · AI), `control.css`
- `src/web/shared/` — `render.js`(슬라이드 그리기·카메라), `icons.js`(**Lucide 아이콘 92개**, `BCIcon('name')` · `BCIcon.type('bible')` · 정적 HTML 은 `<i data-icon="…">`), `i18n.js`(영어 화면 런타임: `[data-noi18n]` 은 번역 안 함, 따옴표 안 자리(“{1}”)는 사용자 글로 두고 번역 안 함)
- `src/web/output/`(예배당·OBS·무대 화면) · `src/web/remote/`(휴대폰 리모컨) · `src/web/splash/`
- `tests/core.test.js` · `tests/e2e/` · `tools/`(가져오기·i18n·아이콘 `make-icon.js`)
- 문서: `README.md`(한국어) · `README.en.md` · `docs/사용자설명서.md` · `docs/OBS설정가이드.md`. 메뉴를 바꾸면 README·설명서의 "메뉴 한눈에" 표도 같이 고친다.

## 디자인 규칙 (1.11.0 에서 정한 것)

- 아이콘은 이모지 대신 Lucide 선 아이콘 한 벌 (`src/web/shared/icons.js`). 새 아이콘이 필요하면 lucide 의 svg 안쪽을 PATHS 에 추가. 메뉴 항목은 `{ ic, label, key, run, sub }`.
- `control.css` 끝의 "조작 화면 디자인 정돈 (1.11)" 블록이 공통 규칙: 토큰(`--radius`, `--shadow`, `--ctl-h: 34px`), 단추 등급(`.primary` 파랑 채움 · `.ghost` · `.icon-btn`), 얇은 스크롤바, `.hgroup`(선택칸 묶음), 메뉴·오른쪽 클릭 메뉴는 `18px 아이콘 | 글 | 단축키` 세 칸.
- 메뉴 6개: 파일(문서·가져오기·기록·동기화·백업·설정) · 큐시트(실행 취소 · 순서 넣기 · 고른 순서) · 송출 · 보기 · 도구(라이브러리·리모컨·스트림덱) · 도움말. 같은 기능은 한 곳에만.
- 라이브러리 머리: 밑줄 탭 한 줄 + 오른쪽 ⌃ 로 검색·단추 줄 접기(`localStorage wc-lib-bar`), 520px 보다 좁으면 아이콘만.
- 화면 테마 6개(만나로·어둡게·고대비·밝게·따뜻하게·푸른 밤)는 :root 토큰으로만 색을 쓴다. 새 색을 직접 쓰지 말 것.
- **만나로 테마(1.12, 골라 쓰는 밝은 테마 — 사용자가 써 보고 눈이 아프다고 해서 기본은 다시 어둡게)**: control.css 맨 끝 블록. 만나로 스튜디오·설교쇼츠 편집기(F:\aside\sermon-shorts-app)와 같은 모양 — 바탕 #f2f3f7 · 흰 카드(독 그룹 16px 둥글게, 사이 10px 는 dock.js dockTheme()) · 남색 머리 줄(.menubar+.top 안에서는 밝은 글자 토큰으로 바꿔 씀, 안의 .suggest·.mb-drop 은 다시 밝은 토큰) · 파란 그라데이션 .primary · 분절 탭 고른 것은 남색. 넓은 요소 규칙(utton, 입력칸)은 :where() 로 우선순위를 낮춰 클래스 규칙을 덮지 않게 했다.
- 종류별 아이콘 색: --tc 를 [data-t](큐시트 .cue-ico) · [data-add](순서 넣기) · [data-tab](라이브러리 탭)에 준다. 큐 아이콘은 연한 색 바탕 칩.
- 글꼴: Pretendard Variable (src/web/shared/fonts/, OFL) — 모든 테마의 조작 화면. 송출 화면 글꼴과는 별개.

## 최근 흐름 (2026-10-29 기준)

- v1.9.0: 주보 순서 붙여 넣기(AI 없이) · 영어 화면에서 사용자 글 보호 · 카메라 안 닫히던 문제 · e2e 점검 추가
- v1.10.0: 앱 아이콘 · 📊 사용 기록(저작권 보고 CSV) · 여러 PC 동기화 · 자동 백업·되돌리기 · 큐 편집 Ctrl+Z/Y · 개인용에 악보(.cmp)
- v1.11.0: 조작 화면 디자인 정돈 (Lucide 아이콘 · 메뉴 재배치 · 상단/아래 줄/메뉴/오른쪽 클릭 모양 통일 · 라이브러리 머리 접기)
- v1.13.0 (2026-11-09): PPT 자동으로 나누기 (AI · 규칙 · 채팅 AI, 20장 넘는 PPT 를 넣으면 물음) · 채팅 AI 바로 가기(ChatGPT·Claude·Gemini 열기) 고침 · 큰 PPT 내보내기 시간 제한 늘림
- v1.14.0 (2026-11-10): PPT 요소 따로 고치기(글·그림·도형) · PowerPoint 없이 PPT 그리기를 이음오피스 엔진으로(차트·표·실제 글꼴 줄바꿈) · 끌어서 슬라이드 차례 바꾸기 · 여기서 새 순서로 나누기(구역) · 클립보드에서 붙여 넣기(Ctrl+V) · Ctrl+휠 썸네일 크기 · 라이선스 GPL-3.0-or-later. 실제 PowerPoint 로 확인하지 못한 것(요소 배경 PS_BG · 클립보드 슬라이드)은 아래 표의 메모대로 — 사용자 PC 피드백을 먼저 반영

### 아직 실제로 확인하지 못한 것 / 다음 후보

- 여러 PC 동기화·백업은 한 PC 안에서 두 데이터 폴더로만 시험했다. 실제 두 PC + 원드라이브에서 검증 필요.
- 카메라는 가짜 카메라(`--use-fake-device-for-media-stream`)로만 점검했다.
- (1.12 에서 편집칸 배경 단추·카메라 표시·더보기 메뉴·무대 화면·리모컨·강조 단추는 아이콘으로 바꿈) 아직 이모지가 남은 곳: 빠른 설정의 배경 단추(`tplcontrols.js`), 오버레이 종류(`library.js OV_TYPES`), 오른쪽 클릭 메뉴의 테마 목록 `🌹`, 설정 화면 일부. 같은 아이콘 규칙으로 바꾸면 더 정돈된다.
- 연결 계획(쇼츠편집기 채팅에서 받은 목록, 사용자가 1번부터 차례로 하라고 함): ① 설교 포인트 슬라이드(완료, 다음 판) ② 광고·주보 연동(완료, 다음 판: 홈페이지 /bulletin/feed.json → 큐시트·광고) ③ 예배 전 대기 화면(완료, 다음 판: 큐시트 → 순서 넣기 → 예배 전 화면 자동 만들기) ④ 실시간 자막 ⑤ 예배 녹음·자동 정리 ⑥ 실시간 통역(낮음).
- 큐시트 패널의 "큐시트 · 성경 책" 독 탭 안에 다시 "큐시트(예배 순서) | 성경 책" 분절 탭이 있어 머리가 두 줄 — 하나로 합칠 여지.
- 사용자가 1.11.0 을 써 보고 눈에 걸리는 곳을 말해 줄 예정 — 그 피드백을 먼저 반영.

## 2026-10-31 더한 것 (큐워십 체험판의 기능 목록을 참고만 — 코드·문구·디자인은 가져오지 않음, 아직 릴리스 전)

| 기능 | 코어 | 화면 | 설정 키 |
|---|---|---|---|
| 즉석 자막 (Ctrl+T) | — | `control/quicktext.js` | localStorage `wc-quick-texts` |
| 교독문 역할 (인도자:·회중:·다같이:) | `cues.js` roleLabels | `shared/slides.js` roleText · textChunks({roles}) | — |
| 글자 자동 숨김 · 처음/끝 한 번 더 숨김 | `core.js` scheduleHide · slideStep · edge | `cue.js` 편집칸 | item hideAfter · edgeHide, `cueEdgeHide` |
| 연결 점검 · 로그 묶음 | `diagnose.js` · `logbundle.js` | `diag.js` · `logs.js` | — |
| 새 버전 창 · 이번 버전 건너뛰기 | `main/updater.js` (notes · skip/unskip) | `updatepanel.js` | `skipVersion` |
| OBS 원격 조작 (obs-websocket 5) | `obs.js` ObsLink (core.obs) | `obspanel.js` | `obs` {enabled,host,port,passwordEnc} |
| 무대 화면 고정 (순서표·고정 글·프리셋) | `core.js` stage.pin · stagePinSet | `stagepanel.js` · `output/stage.js` | `stagePresets` |
| 무대 화면 모니터 (전체 화면 두 번째 창) | `setStageWindow` | `stagepanel.js` | `stageOut` (main.js stageWin) |
| PPT 여러 순서로 (슬라이드 범위) | `cues.js` hymn range | `cue.js` pptRange · openSplit | item range |
| PPT 요소 따로 고치기 (2026-11-09, 1.13 다음 — 사용자가 ① 글자만 대신 ② 요소별을 고름. 같은 날 PowerPoint 없이 되게 다시 만듦) | **2026-11-09 이음오피스(IeumOffice, Apache-2.0) PPTX 엔진으로 (Fable 작업, GPL 전환 뒤)**: `src/core/vendor/ieum-pptx.js`(이음오피스 pptx-engine · pptx-render · font-metrics · zip-gate + jszip · fast-xml-parser · opentype.js · bidi-js 를 `tools/build-ieum-pptx.js --src <이음오피스 소스>` 로 esbuild 묶음 1MB, 커밋 번호는 파일 머리말 · 라이선스 글은 같은 폴더 · 실행 때 빌드 없음) · `pptxengine.js`(엔진 RenderTree → 요소 기록·숨길 id(cNvPr id, 그룹 자식은 nvId)·슬라이드 글 — 글상자 글은 엔진이 나눈 줄 그대로 '\n', fit 으로 글꼴이 달라도 안 꺾이게) · `pptxdraw.js`(RenderTree → SVG, 이음오피스 export-svg.ts 를 Node 용으로 옮김: 글은 <text> + textLength, 그림은 href="media-N.ext" 로 두고 서버 `/hymn/` 가 `hymns.svgToSend` 로 data: 를 넣어 보냄 — 캐시가 그림 수만큼만 큼) · `pptxfonts.js`(윈도우·맥·리눅스 글꼴 폴더 → opentype.js → 엔진 OpentypeMetrics, 없는 글꼴은 글자 종류별 대체 → 추정 메트릭) · 엔진이 파일·슬라이드에서 실패하면 아래 직접 쓴 파서로 (raw.engine 'ieum' | 'own' | 'mixed') · 캐시 키 pptx 모드만 v4 · `pptxbuild.build` 는 async. 테스트 픽스처 `chart-table.pptx`(차트·표·자동 번호·줄바꿈·그라데이션·돌린 글). 큰 PPT 점검: 200장 81MB → 2.8초 · 캐시 81MB · RSS 450MB (엔진이 zip 을 다 메모리에 푼다). **이전 (직접 쓴 파서, 지금은 되돌아갈 때만)**: PowerPoint 없이 .pptx/.ppsx 를 직접 읽는다 (직접 쓴 코드, 외부 라이브러리 없음): `pptxparse.js`(작은 XML 파서 · zip 관계 · 테마 색/글꼴 · clrMap · 마스터/레이아웃/슬라이드 상속 — 개체 틀 짝(type·idx) · lstStyle → txStyles → defaultTextStyle · bodyPr · 그룹 좌표 변환 · prstGeom/custGeom · 채우기/선 · 표 · SmartArt(dsp) · 배경) · `pptxsvg.js`(슬라이드/배경/요소 하나 → SVG. 글은 foreignObject HTML, 그림은 data:, 미리 정의 모양 ~100개, 표는 HTML 표) · `pptxbuild.js`(분류: 글만 → text · 그림 → image(media-N.ext, crop) · 단색 rect/roundRect/ellipse → shape · 그 밖 → slide-NNN-eMM.svg · 표/차트/SmartArt/돌린 것/세로 글/바닥글 → 배경. slide-NNN.svg(full) · slide-NNN-bg.svg · layers.json v3 {bgs, slides, hide, texts}) · `pptrender.js` PS_BG: PowerPoint 가 있으면 hide.json 의 도형 id(cNvPr id = Shape.Id)를 숨기고 Slide.Export → slide-NNN-bg.png (문서화된 API 만, 숨은 Shape.Export 는 버림) · `pptlayers.js` toItems(포인트 → 화면 % · 1080px, id p{k}t{n} 글 · p{k}i{n} 그림 · p{k}s{n} 도형, 색 '#hex'·BGR 둘 다) · `hymns.js` mode 'pptx'(PowerPoint 없음) = pptxbuild full → SVG 슬라이드 + layers:true, mode 'slides' + xml → layers:true · layers()/buildLayers()/readLayers()/layerBgs · 캐시 키 v3 · 명령 pptTextLayers | `cue.js` pptTextOn/Off · pptTextRow(meta.layers 면 보임) · buildSlides 가 edits[k].pptBg 면 배경 그림, 없으면 꺼낸 요소 뺌 · `render.js` 글상자 font · lineHeight · data-fit · 그림 crop(키워서 잘라 냄) | edits[k].pptBg · item font/fit/lineHeight · image crop {l,t,r,b} (요소 60개 · 슬라이드 1200장까지). 테스트: `tests/fixtures/elements.pptx`(python-pptx, `make-ppt-fixtures.py`) 로 단위 2개 · e2e 2-8c. 실제 PowerPoint(PS_BG · Shape.Id 가 cNvPr id 와 같은지)와 실제 교회 PPT 로는 아직 확인 못 함 — `tests/ppt-bg-mock.ps1` 가짜 COM 으로 흐름만 |
| 끌어서 슬라이드 차례 바꾸기 (2026-11-09) | `cues.js` slideOrder (성경 빼고) | `slideedit.js` attachSel(끌기 = 옮기기, 예전 '끌어서 영역 고르기' 대신) · moveSlides · applySeq(원래 차례 slideOrder + 덧붙임 at) · 편집기 ▲▼ 도 원래 슬라이드에 · `cue.js` decorate 의 orderOf · `slides.js` reorderChunks/reorderEdits | item slideOrder |
| PPT 자동으로 나누기 (AI · 규칙 · 채팅 AI, 2026-11-09) | `pptsplit.js` ruleSplit · askSplit · cleanParts, 명령 pptSplitRule/AI/Prompt/Import/Sheets | `cue.js` openSplit · offerSplit(20장 넘으면 물음) · `control/pptsplit.js`(슬라이드 글자 · 모아보기 그림) | — (ai 키·모델은 주보 AI 와 같음) |
| 여기서 새 순서로 나누기 = 구역 (2026-11-09) | — | 슬라이드 오른쪽 클릭(`ctxmenu.js` slideMenu · 전체 격자) → `slideedit.js` splitAt/splitWhy: PPT 는 sources() 차례로 앞·뒤 → 범위 + skip + slideOrder + extra at + edits 번호 다시, 공지·찬양 `slides.js` splitChunks/splitEdits (찬양은 songId·order 끊음), 제목은 장 · `cue.js` BCCue.replaceItem (실행 취소) | — |
| 클립보드 붙여 넣기 → 새 순서 (2026-11-10, Ctrl+V · Ctrl+Shift+V 골라서) | `clippaste.js` planText(글 → 장절이면 성경(줄마다) · 가사 모양이면 찬양 · 그 밖 공지, as 로 종류 지정) · grabSlides(PowerPoint 슬라이드 → `<데이터>/pasted/clip-날짜-시각.pptx`: ① 클립보드의 'PowerPoint 12.0 Internal Slides' 묶음(zip)을 Electron `clipboard.readBuffer` 로 ② 윈도우 .NET 으로 다시(PS_CLIP_DUMP) ③ PowerPoint 에 새 프레젠테이션 → `ExecuteMso('PasteSourceFormatting')` → 안 되면 `Slides.Paste` → SaveAs(PS_CLIP_PASTE). 방법마다 `hymns.load` 로 정말 열리는지 보고 안 되면 다음으로. ③ 은 슬라이드가 있다고 확인됐거나(①의 묶음 · ②의 OK/HAVE) 사용자가 직접 골랐을 때(force)만) · probeSlides: 읽히는 묶음이 있어야 slides true, 형식 이름만 보이면 'maybe'(글이 없는 붙여 넣기에서만 조용히 시도), 있을 수 없는 형식 이름까지 '있다'고 하면 형식 확인을 믿지 않음(formatCheck false) — 1.14.0 첫 빌드의 e2e 가 윈도우 실제 프로그램에서 글 붙여 넣기가 슬라이드 쪽으로 새서 실패한 뒤 넣음 · 명령 clipboardInfo · pasteNow · pasteTextPlan · pasteSlides · `hymns.isAllowed` 가 pasted 폴더의 PPT 를 열어 줌 · 그림은 `/api/media` (머리글 X-Kind: clip → media/clip-*.png) · `main.js` desktop.clipboardHas · clipboardBuffer · pasteIntoControl(webContents.paste) | `control/paste.js` (document 의 paste 이벤트 — 글 입력칸·열린 창은 그대로 두고, 탐색기 파일은 `dropfiles.js` addPaths 로, 글+그림이 같이 오면 묻기, 슬라이드 편집기가 열려 있으면 `slideedit.js` pasteText · dropImage. 메뉴·오른쪽 클릭은 BCPaste.run → 코어 pasteNow 가 화면에 붙여 넣기를 시켜 같은 이벤트로) | — |
| Ctrl+휠 썸네일 크기 (2026-11-10) | — | `grid.js` setSize (막대와 같이, 150~420px) | localStorage `wc-slide-grid-size` |
| 한글 입력 자동 전환 (윈도우) | `imeKorean` | control.js focusin | `imeAuto` (main/ime.js PowerShell 도우미) |
| Companion 단추 누르기 | `companion.js` | 외부 단추 창 · 순서 자동 동작 | `companion` {host,port} |
| 장비 직접 (X32 OSC · PTZ VISCA) | `devices.js` | `devicepanel.js` | `devices` |

순서마다 '⚡ 송출할 때 자동으로'(item.actions)에 obsScene/obsStream/obsRecord · stagePin · companion · dev[] 가 더해졌다 (실행은 `cue.js` runActions, 정리는 `cues.js`).
클립보드의 PowerPoint 슬라이드는 실제 PowerPoint 로 확인하지 못했다 (묶음 형식 이름 · 그 안이 .pptx 인지 · 원본 서식 유지 명령) — 가짜 클립보드 묶음(픽스처)과 `tests/clip-paste-mock.ps1` 가짜 COM 으로 흐름만. 글·그림·탐색기 파일 붙여 넣기와 Ctrl+휠은 브라우저로 확인.
실제 장비(OBS·Companion·X32·PTZ)로는 확인하지 못했다 — 가짜 서버/UDP 로 프로토콜 바이트만 단위 테스트. 테스트: 단위 61개 · e2e 40개.

### 같은 날 ProPresenter 7.15 참고로 더한 것 (기능 아이디어만)

| 기능 | 코어 | 화면 | 설정 키 |
|---|---|---|---|
| 회중 메시지 (Ctrl+M) | `core.js` showMessage · messageOverlay (배너 오버레이 'wpmessage') | `messagepanel.js` | `messageTemplates` |
| 매크로 (Alt+1~9 · /api/trigger/macro · 켤 때) | saveMacro · runMacro → 'run-macro' 이벤트 | `macropanel.js` (실행은 cue.js runActions) | `macros` · item.actions.macro |
| 예약 실행 | checkSchedule (30초) | macropanel.js 아래 | `schedule` |
| 큐시트 인쇄 (Ctrl+P) | — | `cue.js` printHtml · openPrint (iframe srcdoc) | localStorage `wc-print` |
| 부를 순서 여러 개 | `library.js` Songs.arrangements(JSON) · saveSongArrangement | `cue.js` arrRow | — |
| 글 한 줄 묻기 | — | `ask.js` (app.ask) — **Electron 은 prompt() 가 없다. prompt() 쓰지 말 것** | — |

테스트: 단위 65개 · e2e 46개.

## 2026-11-04 사용자 피드백 반영 (아직 릴리스 전)

- **OBS 자막 성경·찬송가 따로**: `activeTemplates.overlayLyrics` (기본 프리셋 `overlay-lyrics` '찬송가·찬양 두 줄', 예전 설정에는 `settings.js load` 가 넣음). 고르는 규칙은 코어 `isLyrics`(core.js) = 조작 화면 `app.isLyrics`(control.js): passage.lyrics · itemType hymn/song · 그림/악보 슬라이드에 caption. 순서에만 고른 tpl.overlay 가 먼저. 템플릿 자리는 `TEMPLATE_SLOTS` (main · overlay · overlayLyrics). 빠른 설정 탭 'OBS 성경 | OBS 찬송가'.
- **가사 줄바꿈 버그**: `render.js partsHtml` 에서 내어쓰기(align 'hanging')면 text 슬라이드도 hangingHtml 로 가 줄바꿈이 없어졌다 → text 는 늘 bc-pre.
- **리모컨 ■ 끄기**: `remote.js act('off')` = clearAll + (보조 화면이 켜져 있으면) outputOff.
- **보조 화면 모니터**: `main.js syncOutputWindow` 가 저장한 displayId 가 없으면 조작 화면과 다른 모니터로 바꿔 저장. 그래도 없으면 조작 화면 `checkOutputSetup` 이 설정 → 출력 을 연다(같은 상태에선 한 번). 사용자 PC 에서 실제로 저장된 displayId(1102995331)가 지금 모니터와 달랐다.
- **패널 접기**: `dock.js` collapse/expand (setConstraints 로 탭 줄 높이에 고정), 머리 단추 ▾/▴ · 탭 누르면 펴짐 · localStorage `wc-dock-folded` [{ids,h}] · 빠른 설정 '접기' 는 `BCDock.toggleFold('quick')`.
