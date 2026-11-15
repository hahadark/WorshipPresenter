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

| `node tools\flow-compare\prep.js` → `node tools\flow-compare\launch.js start [--exe tmp/bin/flow_driver_rs.exe]` · `launch.js stop` | 두 복사 자료를 맞추고(시험 영상·그림·오버레이), 두 판을 띄운다. PID 는 `tmp\flow-pids.json`, 포트는 `tmp\flow-ports.json` — 다른 스크립트가 이 파일을 읽으므로 `--el` 을 안 줘도 된다. stop 은 적힌 PID 만 끈다 |
| `node tools\flow-compare\features.js --base 16 --out tmp\flow-feat [--part overlays,obs,quick,media,auto,actions,times]` | 기능 대조: 오버레이, OBS 자막(템플릿 × 순서 종류, 그림 우상단·슬라이드 그대로), 즉석 자막, 영상·음악·카메라·그림 순서, 자동 넘김·자동 숨김·끝에서 숨김, 순서의 '송출할 때 자동으로', 예정 시간. `--base N` 은 앞선 실행이 큐시트 끝에 넣은 시험 순서를 걷어 낸다 |
| `node tools\flow-compare\contract.js [--out tmp\contract.json]` | 외부 연동 계약: 조작 서버(`/api/command` · `/api/trigger` · `/api/state` · `/dock` · `/r/do` · Host 검사), 휴대폰 리모컨 서버(PIN), 내부망 출력(켜져 있을 때)의 상태 코드·응답 모양 |
| `node tools\flow-compare\split-batch.js` · `text-batch.js` | 성경 나눔(범위 × 템플릿 변형 × 분할 방식)·가사/공지/교독문 나눔의 장 수·나눔 위치 일괄 대조 |

- `compare.js` 는 장마다 **예배당 화면에 그 슬라이드의 글이 실제로 보일 때까지**(글 영역 불투명도 1) 기다린 뒤 찍는다. 못 기다리면 `내용 안 보임`, 글이 있어야 하는데 두 판 모두 한 가지 색이면 `둘 다 빈 화면(판정 못 함)` 으로 적는다 — 0% 로 넘기지 않는다. 끝에 요약 줄.
- 오래 도는 스크립트는 셸 호출이 끝나도 살아 있게 띄운다 (PowerShell `Start-Job` 은 호출이 끝나면 죽고 머리 없는 브라우저만 남는다).
- 영상 순서의 그림 비교는 머리 없는 브라우저에서 한쪽 영상이 검게 찍히는 때가 있다(실행마다 다름). `/api/live` 와 '끝나면 다음/검은 화면' 시간표는 안정적이다.
- 무대 화면의 시계·타이머 숫자는 글 비교에서 뺀다 (`scenarios.js`). `compare.js` 의 무대 '글 다름' 은 대개 시계 초 차이다.
- 찬송가·PPT 순서는 `compare.js` 에서 장 수만 본다 (다른 점검 갈래).
- 끝나면 자기가 띄운 Electron·flow_driver 만 끈다.
