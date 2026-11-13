# 큐시트 순서 우클릭 메뉴 검증

기준: master b483634. 작업 브랜치: gpt-cue-context.
Electron 대조: src/web/control/ctxmenu.js cueItemMenu 및 cue.js itemAction(duration/pptsplit).

## 변경점
- 진행 시계(cueTimes=true)일 때 우클릭 메뉴의 '예정 시간…' 하위 메뉴에서 편집한다.
- 큐시트 관리창과 공통 duration_editor를 사용한다. 분 단위 JSON duration, 정수 0~600(Electron 범위), 실행 취소/저장 경로를 공유한다. 기존 문자열 숫자도 표시한다.
- '여러 순서로 나누기…'는 file이 있는 hymn에서만 활성화한다. 베들레헴 찬송가·빈 파일·그 외 종류는 비활성화한다.
- PPT 메뉴는 선택 순서의 기존 hymns_ui::ppt_box를 펼치고 그 위치로 이동할 뿐, 자동 나누기나 API를 실행하지 않는다. app.rs의 기존 ppt_box 호출을 그대로 사용하므로 새 창/분할 구현이 없다.
- 예정 시간 수정은 duration만 바꾼다. startTime(HH:MM), 예약 schedule, auto 메타데이터를 변환하지 않는다.

## CLI 컴파일 및 단위 테스트

PowerShell에서 해당 작업 트리만 사용:

```powershell
Set-Location F:\aside\wp-rs-i
$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
cargo check --offline -p wp-app
cargo test --offline -p wp-app cue_duration_tests
cargo test --offline -p wp-app cue_ppt_menu_tests
```

## UI 검증용 독립 데이터 생성 및 실행

아래 빈 DB 파일을 반드시 먼저 만든다. Core::prepare_data_dir는 bible.db가 없으면 사용자 Electron 자료를 자동 복사하므로, 빈 파일을 먼저 두어 원본 읽기도 피한다. SQLite가 실행 중 빈 DB에 스키마를 생성한다. 기존 데이터 경로를 재사용하지 않는다.

```powershell
Set-Location F:\aside\wp-rs-i
$testData = Join-Path $PWD ("target\cue-context-data-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $testData | Out-Null
New-Item -ItemType File -Path (Join-Path $testData 'bible.db') | Out-Null
'{"port":37748,"autoUpdate":false,"globalShortcuts":false,"cueTimes":true,"syncDir":"","remote":{"enabled":false},"lan":{"enabled":false}}' | Set-Content -Encoding utf8 (Join-Path $testData 'settings.json')
cargo run --offline -p wp-app -- --data $testData
```

기본 서버는 지정 포트부터 10개만 시도하므로 37698에는 접근하지 않는다. 릴리스 빌드/푸시/외부 API 사용은 하지 않는다. 검증이 끝나면 이 명령으로 띄운 창만 정상 종료한다. 다른 wp-app 프로세스를 이름으로 일괄 종료하지 않는다.

### 예정 시간
1. 빈 큐시트에 공지·제목·음악 등 여러 종류를 넣는다. 큐시트 관리에서 시작 시각을 11:00으로 둔다.
2. 각 순서를 우클릭하고 '예정 시간…'을 연다. 각각 5, 12분을 넣고 관리창에서도 같은 값을 확인한다. 다음 순서의 예정 시각은 11:05, 11:17이어야 한다.
3. 0·600 및 범위 밖 값을 넣어 0~600 정수 범위를 확인한다. Ctrl+Z/다시 실행으로 복원되는지 확인한다.
4. 우클릭에서 시간을 바꾼 뒤 동일 순서의 본문을 편집한다. 관리창/다시 열기에서 duration이 이전 값으로 돌아가지 않아야 한다.
5. 설정에서 진행 시계를 끄면 우클릭 예정 시간 메뉴는 사라진다. 관리창 기존 편집 기능은 유지한다.
6. 대상 항목에 auto 및 기타 메타데이터를 가진 독립 JSON 큐 fixture를 가져온 경우, 내보내기에서 duration 외 필드가 유지되는지 확인한다. 예약 매크로의 요일·HH:MM과 시작 시각도 변하지 않아야 한다.

### PPT 나누기
1. 저작권 없는 시험 PPT를 찬송가·PPT 순서로 넣고 다른 순서를 선택한다.
2. PPT 순서를 우클릭해 '여러 순서로 나누기…'를 누른다. 해당 순서가 선택되고 기존 PPT 도구가 펼쳐져 '자동으로 나누기', '채팅 AI 프롬프트 복사', 'AI 답' 입력이 보여야 한다. 메뉴를 누르는 것만으로 순서 수는 바뀌지 않아야 한다.
3. 규칙으로 구분 가능한 시험 PPT에서 기존 '자동으로 나누기'를 눌러 기존 split_items 경로로 range가 다른 순서들이 생기는지 확인한다. 외부 API 단추는 누르지 않는다.
4. 제목·공지·음악·베들레헴 찬송가 및 file 없는 hymn은 나누기 메뉴가 비활성화되어야 한다.
5. 다른 PPT를 우클릭하여 대상이 바뀌는지 확인한다. 메뉴 진입만으로 auto/예약/슬라이드 편집 메타데이터가 바뀌지 않아야 한다.

단위 테스트는 시간 정수화·범위·메타데이터 보존, 메뉴 항목 종류 조건, 기존 PPT 도구 열기 요청의 대상 ID를 검사한다. 실제 네이티브 화면 클릭 검증과 PPT 렌더링은 위 절차로 별도 확인한다.

## 이번 작업에서 실제 수행한 검증
- cargo check --offline -p wp-app: 성공 (경고 17개).
- cargo check --offline -p wp-app --tests: 성공 (추가 회귀 테스트 3개 포함 테스트 코드 컴파일).
- git diff --check: 성공.
- 단위 테스트 실행과 실제 UI 클릭 검증은 미수행. 실행 명령과 판정 기준은 위에 명시했다.
- 앱 실행 자체를 하지 않았으며 독립 작업 트리 target에서만 검증했다. 사용자 원본/공유 데이터/메인 프로세스/37698 포트는 사용하지 않았다.
