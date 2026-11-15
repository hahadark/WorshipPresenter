# 예배 흐름 출력 대조 (Electron 판 ↔ Rust 판)

같은 자료를 두 벌 복사해 두 판을 띄우고, 같은 큐시트를 한 순서·한 슬라이드씩 송출하며
웹 출력 3종(`/outputs/main` 예배당 · `/outputs/overlay` OBS · `/outputs/stage` 무대)의 그림(픽셀 차)·글과 `/api/live` 를 비교한다.
머리 없는 엣지/크롬(CDP)으로 1920×1080 을 찍으므로 사용자 화면에 창이 뜨지 않는다 (Electron 조작 창만 뜬다).

## 준비

1. 자료 두 벌: `tmp\rs-data`, `tmp\el-data` (같은 폴더를 복사. `settings.json` 에 `"autoUpdate": false`, `"telemetry": false`, `output.enabled/autoOnSend: false`).
2. Electron 판(소스에서): `F:\aside\WorshipCue\node_modules\electron\dist\electron.exe F:\aside\WorshipCue --data <el-data>` → 포트는 37528 부터 빈 곳 (`/control/` 이 200 인 포트).
3. Rust 판(조작 화면 없이 코어만): `cargo build -p wp-core --example flow_driver --offline` → `target\debug\examples\flow_driver.exe --data <rs-data>`
   (`<rs-data>\flow-port.txt` 출력 포트, `flow-ctl.txt` 조작 주소). Rust 판 `/api/command` 는 출력 화면용 명령만 받으므로 이 점검 서버로 조작한다.

## 실행

| 스크립트 | 하는 일 |
|---|---|
| `node tools\flow-compare\compare.js --el <포트> --rs-data tmp\rs-data --out tmp\flow-out [--cue id] [--items 0,1] [--max-slides 8]` | 모든 큐시트의 모든 순서: 슬라이드 장 수·내용, 장마다 출력 3종 비교. 다르면 나란히 붙인 그림을 남긴다 (`report.json`) |
| `node tools\flow-compare\scenarios.js --el <포트> --rs-data tmp\rs-data --out tmp\flow-sc --cue <id> [--part ops,edge,split] [--bible-item 6] [--edge-item 13] [--parallel niv2011] [--refs "에 8:9\|롬 8:28-39"]` | `ops` 글자 숨김·F10·배경·로고·회중 메시지·무대 메시지/타이머·모두 지우기, `edge` 순서 처음·끝에서 넘기기, `split` 성경 나눔(한 절씩·두 절씩·자동, 병행 번역) |

| `node tools\flow-compare\hymns.js --el <포트> --rs-data tmp\rs-data --el-data tmp\el-data --out tmp\hymn-out --files files.json [--max-slides 6]` | 찬송가·교독문·붙여 넣은 PPT 순서를 두 판 큐시트 끝에 넣고 장 수·내용·웹 출력(main·overlay) 그림을 비교. 배경·OBS 가사·범위·건너뛰기는 `extra` 로 |
| `node tools\flow-compare\hymnal.js --el <포트> --rs-data tmp\rs-data --el-data tmp\el-data --out tmp\hymnal-out [--outside <등록 폴더 밖 PPT>] [--part search,bethlehem,scores,allow]` | 찬송가 폴더 찾기 40가지 · 베들레헴 찬송가(책 목록·찾기·1,685장 꺼내기·악보 그림) · 열 수 있는 파일 경계(등록 폴더·붙여 넣은 폴더·끌어다 놓은 파일)를 값으로 비교. 베들레헴은 두 판이 같은 `.hdb` 를 읽게 맞춘 뒤(자료 폴더 `hymnal\`) 볼 것 |
- 무대 화면의 시계·타이머 숫자는 글 비교에서 뺀다 (`scenarios.js`). `compare.js` 의 무대 '글 다름' 은 대개 시계 초 차이다.
- 찬송가·PPT 순서는 `compare.js` 에서 장 수만 본다 (다른 점검 갈래).
- 끝나면 자기가 띄운 Electron·flow_driver 만 끈다.
