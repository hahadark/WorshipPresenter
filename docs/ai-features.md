# AI 도우미 일곱 가지 (2026-11-27, 가지 `ai-features`, 아직 릴리스 전)

모두 **도구** 메뉴 아래쪽에 있다. 키가 없어도 규칙으로 하는 부분은 다 되고, AI 단추는 'AI 키가 필요합니다' 로 안내한다.
AI 는 주보 → 큐시트 · PPT 나누기와 같은 길(OpenAI 호환 `chat/completions`, `aihelp.rs`)을 쓴다. **AI → AI 설정** 에서 키 · 제공자(오픈라우터 · DeepSeek · OpenAI 단추, 주소 직접) · 모델 · 기능별 동의를 고친다.
글을 밖으로 보내는 기능은 처음 쓸 때 '무엇이 · 어디로' 를 보여 주고 동의를 받는다(설정 `aiConsent.<기능>`). 음성은 PC 밖으로 보내지 않는다.

| 기능 | 쓰는 법 | 코어 · 화면 | 그림 |
|---|---|---|---|
| 1 예배 전 큐시트 점검 | AI → 예배 전 큐시트 점검. 목록을 누르면 그 순서로. '주보와 비교' 에 순서를 붙여 넣으면 빠진 순서 · 차례 다름 (주보로 큐시트 만들기의 초안 글이 있으면 그것으로). 'AI 로 오타 · 줄 나눔 보기' | `cuecheck.rs` · `ai_ui.rs` | images/ai/check.jpg |
| 2 설교 원고 → 슬라이드 | 도구 · 순서 넣기 → 설교 원고 → 슬라이드. txt · md · docx · hwp · hwpx. 규칙 또는 AI → 후보(제목 · 본문 성경 · 설교 포인트 · 원고 속 구절 · 인용) 골라 넣기(Ctrl+Z 한 번). 설교 포인트 창과 서로 오감 | `sermonslides.rs` · `refscan.rs` · `sermon_ui.rs` | images/ai/sermon.jpg |
| 3 설교 구절 따라가기 | AI → 음성 인식. 처음엔 엔진 · 모델 받기(동의). '듣기 시작' → 들은 장절 목록. '찾으면 바로 다음 칸에'(기본 꺼짐)를 켜도 **'다음' 까지만**, 바로 송출 모드에서도 송출 안 함. 듣는 동안 오른쪽 아래 '듣는 중' | `stt.rs` · `voicefollow.rs` · `listen_ui.rs` · `Core::preview_passage_quiet` | images/ai/listen2.jpg |
| 4 가사 자동 정리 | AI → 가사 자동 정리 · 찬양 편집칸 '가사 자동 정리…'. 두 칸 비교 → 받아들이기 · 새 찬양 순서로 | `lyricsclean.rs` · `lyricsfix_ui.rs` | images/ai/lyrics.jpg |
| 5 방송 실시간 자막 · 번역 | 음성 인식 창 '방송 실시간 자막'. 기본 꺼짐 · 기본은 방송 출력(`/outputs/overlay`)에만, '예배당 화면에도' 는 따로. 8초 조용하면 내림. 영어 번역은 키 + 동의 | `voicefollow.rs caption_overlay` (`trackb.rs overlays_for` 한 줄) | — |
| 6 예배 후 정리 | AI → 예배 후 정리. 날짜 · 영상 시작 시각 → 순서별 시간 표시(유튜브 챕터) · 유튜브 설명 · 설교 요약 재료 · 쇼츠 후보(시간 구간). 녹음 파일 → PC 안에서 글 → 글만 AI 요약 | `recap.rs` · `recap_ui.rs` | images/ai/recap.jpg |
| 7 말로 시키기 | AI → 말로 시키기 · 찾기칸(Ctrl+K)에 `>` 로 시작 + Enter. 할 일 목록을 보고 [실행]. 단계는 큐시트 열기·복제·이름, 순서 고치기·넣기·옮기기, 장절을 '다음' 에, 창 열기, 찾기뿐 — 송출 · 삭제 · 지우기 · 예배당 화면 · 설정은 없음 | `saycmd.rs` · `say_ui.rs` | images/ai/say.jpg |

## 음성 인식 (whisper.cpp)

- 엔진: whisper.cpp b5907 `whisper-server.exe`(MIT, Windows x64 CPU 판 zip 9.2MB 에서 서버 · DLL 만). 모델: small q5_1 190MB(논리 코어 8개 이상 기본) · base q5_1 60MB. 주소 · SHA-256 은 `stt.rs` 에 박음, 다르면 지움. 받는 곳 `<자료>\stt\`.
- 맥: 미리 빌드된 엔진이 없어 받지 않는다. Homebrew `whisper-cpp` 의 `whisper-server` 가 있으면 씀(창에 안내). 마이크는 rodio 의 cpal(맥도 됨). **맥에서 실제로 돌려 보지 못함.**
- 서버 인자 `-t (코어/2, 최대 4) -nf -ac 768`. 조각은 쉼(0.5초)에서, 최대 6초.
- 실측(이 PC: 논리 코어 12, CPU 판): 합성 한국어 설교 음성(Windows 'Heami' 음성, 성경 구절 80문장) — `cargo test -p wp-core --release -- --ignored stt_bench --nocapture` (WP_STT_BENCH · WP_STT_DIR · WP_STT_MODEL)

| 모델 | 고유어로 읽은 수 ('열다섯 장') | 한자어로 읽은 수 ('십오 장', 설교자 말투) | 인식 늦음(조각당) | 엔진 CPU (PC 전체) | 메모리 |
|---|---|---|---|---|---|
| small | 맞춘 묶음 17/20 · 처음 보는 묶음 17/20 (85%) | 19/20 · 19/20 (95%) | 평균 1.9~2.0초 · 최대 2.3초 | 12~13% (코어 하나의 150%) | 389MB |
| base | 15/20 · 13/20 (65~75%) | 19/20 · 17/20 (85~95%) | 평균 0.6초 | 4~5% | 200MB |

  '맞춘 묶음' 은 규칙을 고칠 때 본 20문장, '처음 보는 묶음' 은 그 뒤 따로 만든 20문장. 다른 무거운 작업과 겹친 구간에서는 늦음이 10~50초까지 튄 적이 있다(같은 때 엔진 CPU 비율은 오히려 낮음 — 다른 프로그램이 CPU 를 씀).
- 앱 전체 흐름: `WP_LISTEN_WAV=<16kHz wav>` + `--listen-start` 로 마이크 대신 파일. 바로 송출 모드에서 4구절 모두 '다음' 에만, 송출 중은 비어 있음(images/ai/listen2.jpg).

## 시험 · 점검

- wp-core 349 통과(21 제외 — 새 `stt_bench` · `ai_live` 포함), wp-app 234 통과(1 제외). 새 시험: aihelp 2 · cuecheck 5 · refscan 6 · sermonslides 3 · stt 5 · voicefollow 2 · lyricsclean 4 · recap 3 · saycmd 4 · ai_ui 1.
- 실제 AI: `WP_AI_LIVE_KEY_ENV=DEEPSEEK_API_KEY WP_AI_LIVE_ENDPOINT=https://api.deepseek.com/chat/completions WP_AI_LIVE_MODEL=deepseek-chat cargo test -p wp-core -- --ignored ai_live --nocapture` — 6기능 한 번씩 2,898 토큰(1센트 아래).
- 점검 인자: `--open-ai check,settings,sermon,lyrics,recap,say,listen` · `--sermon-file` · `--lyrics-file` · `--say "<말>"` · `--recap-day YYYY-MM-DD --recap-start HH:MM:SS` · `--listen-start` · 환경 `WP_SHOT_AT=<초>` (`--shot` 시각) · `WP_AI_MOCK=<파일>`(AI 답 대신).

## 확인 못 한 것

- 실제 사람 설교 음성 · 실제 교회 마이크 · 회중석 소음 · 맥. 엔진 GPU(CUDA) 판은 쓰지 않음(받을 크기가 수백 MB).
- 방송 자막의 OBS 화면 모양(오버레이 배너 길을 그대로 씀) · 번역 늦음(한 줄에 AI 한 번).
- `sources` 가지(출력 구조 바꾸는 중)와 겹칠 곳: `trackb.rs overlays_for` 의 자막 한 줄 · `core.rs` 새 함수 2개(preview_passage_quiet · overlays_changed). web/output · presentation 은 고치지 않음.
- 2.0.8 의 전문가 테마에서 종류 아이콘이 색이 있던 것을 단색으로 고침(`theme.rs type_ink`).
