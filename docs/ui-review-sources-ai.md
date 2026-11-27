# 새로 생긴 창 · 구획 전체 목록 — 소스 로드맵 1~4단계 + AI 도우미 7개 (2026-11-27, sources 가지)

다음 작업(전체 인터페이스 다듬기)을 위한 목록. 모두 **아직 릴리스 전**(2.0.8 다음). 설계 · 쓰는 법은 `docs/roadmap-sources.md`, AI 는 `docs/ai-features.md`.
캡처는 `docs/images/review/` (1600×960 창 `--win 1600x960`, 2400×1440 으로 찍힘 → jpg). 점검 자료는 가상(○○교회 · 홍길동).

점검 실행 공통 (격리): `WP_LEGACY_DATA=<빈 폴더(Windows 경로, cygpath -w)>` + `target\debug\wp-app.exe --data <자료 복사본> --win 1600x960 … --shot <파일>`.
자료는 `tools/scenes/e2e-auto.js` 가 만든 `tmp\auto-e2e\data` 를 복사해 구성 4개 · 추가 출력 '로비 화면' · 전환 지정을 넣어 썼다 (`tmp\ui4\data`, 큐시트 `tmp\ui4\cue.json` — `--import-cue`).

## 소스 · 화면 구성 (로드맵 1~4단계)

| # | 창 · 구획 | 여는 방법 | 점검 인자 | 캡처 | 코드 |
|---|---|---|---|---|---|
| S1 | 인스펙터 **소스** 탭 (묶음: 화면 내용 · 자막 · 오버레이(실시간 자막 포함) · 카메라 · 웹 페이지, 어디에 나가는지 빨강/흐림, 웹 페이지 더하기) | 오른쪽 인스펙터 [글자 \| 무대 \| 미디어 \| **소스**] | `--ins-tab sources` | `review/s1-sources-tab.jpg` | `sources_ui.rs` (1단계) |
| S2 | 소스 탭 맨 위 **화면 구성** 구획 (고른 순서 · 종류 기본 · 출력 기본, 출력마다 한 줄 · '화면 구성 편집…' · '출력 설정…') | 큐시트에서 순서 고르고 소스 탭 | `--item 0 --ins-tab sources` | `review/s1-sources-tab.jpg` | `sources_ui.rs scene_section` (3단계) |
| S3 | **화면 구성 편집기** (별도 창: 왼쪽 구성 목록 · 가운데 이름 · 쓰는 곳 · 틀 5개 · 미리 보기 출력 · **이 구성으로 바뀔 때**(4단계) · 캔버스 · 오른쪽 칸 목록 · 소스 · 자리·크기 · 자르기 · 모양) | 보기 → 화면 구성 편집… / 소스 탭 → 화면 구성 편집… | `--scene-open [full\|split\|pip\|lower\|three]` | `review/s3-scene-editor.jpg` | `scene_ui.rs` (2~4단계) |
| S4 | **출력 설정** 창의 '출력' 구획 (영상 · 유튜브 소리 · **화면 구성이 바뀔 때** + '출력마다 다르게'(4단계) · 추가 출력 카드: 이름 · 어느 출력처럼 · 템플릿 · 기본 구성 · 모니터 · 띄우기 · **예배당 화면과 함께 켜고 끄기**(4단계) · 주소 복사 · 지우기 · 출력 더하기) | 보기 → 출력 설정 / 상태 줄 연결 점 / 설정 찾기 '출력 추가' | `--open-d output` | `review/s4-output-settings.jpg` | `outputs_ui.rs` · `trackd_ui.rs output_ui` (3~4단계) |
| S5 | 추가 출력 **모니터 창** (예배당 창과 같은 wp-output, Esc = 닫기) | S4 에서 모니터 고르고 '모니터에 띄우기' | (`--shot` 에서는 안 띄움) | — | `outputs_ui.rs frame` |
| S6 | 뷰어 머리 **출력 고르기** [예배당 \| OBS \| 로비 화면 …] (다음 · 송출 중 각각) | 가운데 '보기' | `--item 1 --send` | `review/s6-main-timeline.jpg` | `app.rs` (3단계) |
| S7 | 큐시트 줄 · 시간표 막대의 **작은 격자 아이콘** (출력 기본과 다른 구성, 툴팁) | 순서에 구성 지정이 있을 때 | 같음 | `review/s6-main-timeline.jpg` | `cue_ui.rs` · `timeline_ui.rs` (3단계) |
| S8 | 예배 시간표 **구성 줄** (기본과 다른 구성 막대 · 기본 바꾸는 순서의 세로 금, 보기 전용) | 구성이 하나라도 있고 큐시트에 시작 시각 · 길이 | 같음 | `review/s6-main-timeline.jpg` | `timeline_ui.rs scene_lane` (4단계) |
| S9 | 편집칸 '송출할 때 자동으로' 의 **화면 구성** 줄 (출력마다 [그대로 / 전체 / 구성]) | 순서 편집칸 → 송출할 때 자동으로 | `--edit-actions` | `review/s9-auto-actions.jpg` | `actions_ui.rs` (4단계) |
| S10 | 도구 → **카메라 동시 열기 시험** (카메라 고르기 · 시험 시작 · 곳마다 결과 · 결과 복사) | 도구 메뉴 | `--open-d camtest` | `review/s10-camtest.jpg` | `camtest_ui.rs` (3단계) |
| S11 | 휴대폰 리모컨 · OBS 도킹의 **화면 구성** 카드 (출력마다 단추 · '계속 쓰기' · 되돌리기) | 휴대폰 리모컨(PIN) / OBS 도킹 `http://127.0.0.1:<포트>/dock/` | 브라우저 | `review/s11-remote-scenes.png` | `web/remote` · `remote.rs` (4단계) |
| S12 | 웹 출력 화면 구성 · 전환 (옮겨 가는 중) | `/outputs/overlay` 등 | `tools/scenes/e2e-auto.js` | `review/s12-moving.jpg` · `review/s12-after.jpg` | `web/output/output.js` (2~4단계) |

## AI 도우미 7개 (origin/ai-features 를 합침)

| # | 창 · 구획 | 여는 방법 | 점검 인자 | 캡처 | 코드 |
|---|---|---|---|---|---|
| A0 | **AI 설정** (키 · 제공자 단추 · 모델 · 기능별 동의) · 처음 쓸 때 동의 창 | 도구 → AI 설정 | `--open-ai settings` | `review/a0-ai-settings.jpg` | `ai_ui.rs` |
| A1 | **예배 전 큐시트 점검** (점검 목록 · 주보와 비교 · AI 오타) | 도구 → 예배 전 큐시트 점검 | `--open-ai check` | `review/a1-check.jpg` (그 전 `images/ai/check.jpg`) | `cuecheck.rs` · `ai_ui.rs` |
| A2 | **설교 원고 → 슬라이드** (파일 · 규칙/AI · 후보 고르기) | 도구 / 순서 넣기 펼침 '설교 원고 → 슬라이드…' | `--open-ai sermon [--sermon-file f]` | `review/a2-sermon.jpg` | `sermon_ui.rs` |
| A3 | **음성 인식** 창 (엔진 · 모델 받기 · 듣기 · 들은 장절 · 찾으면 다음 칸에 · 방송 실시간 자막) + 오른쪽 아래 '듣는 중' | 도구 → 음성 인식 | `--open-ai listen` | `review/a3-listen.jpg` | `listen_ui.rs` |
| A4 | **가사 자동 정리** (두 칸 비교 · 받아들이기 · 새 순서로) · 찬양 편집칸 '가사 자동 정리…' 단추 | 도구 → 가사 자동 정리 / 찬양 편집칸 | `--open-ai lyrics [--lyrics-file f]` | `review/a4-lyrics.jpg` | `lyricsfix_ui.rs` |
| A5 | **방송 실시간 자막** — A3 창 안 구획. 출력에서는 이제 **'실시간 자막' 소스**(`caption`): 소스 탭 '자막 · 오버레이' 묶음에 보이고 구성 편집기 칸에 넣을 수 있음(기본은 그대로 방송 출력 아래쪽) | A3 | — | `review/s1-sources-tab.jpg`(소스 줄) | `voicefollow.rs` · `sources.rs` · `scenes.rs` |
| A6 | **예배 후 정리** (날짜 · 영상 시작 · 시간 표시 · 유튜브 설명 · 요약 재료 · 쇼츠 후보 · 녹음 요약) | 도구 → 예배 후 정리 | `--open-ai recap [--recap-day … --recap-start …]` | `review/a6-recap.jpg` | `recap_ui.rs` |
| A7 | **말로 시키기** (할 일 목록 · 실행) · 찾기칸 `>` | 도구 → 말로 시키기 / Ctrl+K 에 `>` | `--open-ai say [--say "<말>"]` | `review/a7-say.jpg` | `say_ui.rs` |

## 다듬을 때 눈여겨볼 것 (작업하며 본 것)

- 소스 탭 · 편집기 · 출력 설정에 같은 '구성 고르기' 가 세 군데 — 이름 · 차례를 같게 맞출 여지.
- 출력 설정 창이 길다(일반 · 모니터 · 출력 · OBS 주소 · 내부망). '출력' 구획을 따로 탭으로 나눌 여지.
- 편집기 '이 구성으로 바뀔 때' 는 오른쪽→왼쪽 배치라 시간 칸이 고르기 왼쪽에 붙는다.
- 리모컨 화면 구성 카드는 구성 수 × 출력 수만큼 단추 — 구성이 많으면 길어짐(고르기 칸으로 바꿀 여지).
- AI 창들은 각자 따로 뜨는 창(egui Window) — 크기 · 단추 등급(Primary 는 노랑 금지) 점검.
