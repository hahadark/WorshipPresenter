//! 워십프리젠터 Rust 판 코어
pub mod books;
pub mod groups; // 슬라이드 이름표 색·글 정리 (Electron shared/groups.js)
pub mod core;
pub mod lockcheck; // 코어 잠금 재진입(멈춤) 감지
pub mod firstrun; // [갈래 D] 첫 실행 자료 이전 · 시작 안내 창 · 두 번 실행
pub mod db;
pub mod hymnal; // [트랙 A]
pub mod hymns; // [트랙 A]
pub mod pps; // [트랙 A]
pub mod pptx; // [트랙 A]
pub mod pptsplit; // [트랙 A]
pub mod splitedit; // [갈래 C] PPT 나누기 창: 손으로 고치기 (cue.js openSplit)
pub mod pptsheets; // [트랙 A] 모아보기 그림
pub mod sermonpoints; // [트랙 A] 설교 포인트
pub mod aihelp; // AI 도우미 공통 (OpenAI 호환 주소 · 동의 · 시험용 가짜 응답)
pub mod refscan; // 글·음성 인식 글 속 성경 장절 찾기
pub mod stt; // PC 안 음성 인식 (whisper.cpp 서버 · 받기·SHA-256 대조 · WAV)
pub mod voicefollow; // 설교 중 성경 구절 따라가기 · 방송 자막
pub mod lyricsclean; // 가사 자동 정리 (절 · 후렴 · 브리지 · 줄 바꿈 · 띄어쓰기)
pub mod recap; // 예배 후 정리 (시간 표시 · 유튜브 설명 · 설교 요약 재료 · 쇼츠 후보)
pub mod saycmd; // 말로 시키기 (정해진 단계만 · 확인 뒤 실행)
pub mod cuecheck; // 예배 전 큐시트 점검 (규칙 + AI 선택)
pub mod sermonslides; // 설교 원고 → 슬라이드 묶음 (제목 · 성경 · 설교 포인트 · 인용)
pub mod preservice; // [트랙 A] 예배 전 화면
pub mod homefeed; // [트랙 A] 홈페이지 주보
pub mod parity_a; // [트랙 A] 기능 대조에서 빠진 작은 것들
pub mod clippaste; // [트랙 A]
pub mod dropfiles; // 끌어다 놓기·파일 붙여 넣기 분류
pub mod songinbox; // [트랙 A]
pub mod measure;
pub mod parser;
pub mod presentation;
pub mod server;
#[cfg(target_arch = "wasm32")]
pub mod web; // [웹판] 브라우저에서만: 플랫폼 층 (docs/web/egui-web.md)
pub mod bg; // 백그라운드 일 한 겹 (설치판 = 스레드, 웹판 = 알림)
pub mod settings;
pub mod slides;
pub mod textnorm;
pub mod util;
pub mod cues; // [트랙 B]
pub mod cuemgr; // [갈래 C] 큐시트 관리 창 (목록·찾기·정렬·새로·복제·휴지통·내보내기)
pub mod contijson; // 다른 콘티 프로그램 .json 가져오기
pub mod contistudio; // 찬양 콘티 스튜디오 콘티(.worship) 가져오기
pub mod pdf; // PDF 보여 주기 (운영체제의 PDF 그리기로 쪽마다 그림)
pub mod youtube; // 유튜브 주소 재생 (영상 순서, 임베드)
pub mod webedit; // 웹 편집기 (다른 컴퓨터의 브라우저에서 큐시트 고치기, PIN)
#[cfg(not(target_arch = "wasm32"))] // 소켓 · TLS 를 쓰는 설치판 쪽 연결 (브라우저 조작 화면은 /bridge 에 직접 붙는다)
pub mod cloud; // 클라우드 연결 (웹판 1단계, docs/web/architecture.md) — 기본 꺼짐
#[cfg(not(target_arch = "wasm32"))] // [웹판] 설치판 쪽 동기화 (브라우저 조작 화면은 web::store 가 같은 API 를 직접 부른다)
pub mod cloudsync; // 클라우드 자료 동기화 (웹판 2단계 B, docs/web/sync.md) — 기본 꺼짐
pub mod phonecam; // 휴대폰을 카메라로 (https · WebRTC 이어 주기)
pub mod firewall; // Windows 방화벽: 휴대폰·내부망 연결 확인 · 열기
pub mod pptxout; // 큐시트 → PPT 파일 하나 (예배 순서대로)
pub mod library; // [트랙 B]
pub mod localtime; // [트랙 B]
pub mod movedata; // 다른 컴퓨터로 옮기기 (자료 한 파일)
pub mod schedule; // [트랙 B]
pub mod lyricsweb; // [트랙 B]
pub mod songimport; // [트랙 B]
pub mod sync; // [트랙 B]
pub mod draw; // [트랙 B]
pub mod logger; // [트랙 B]
pub mod diagnose; // [트랙 B]
pub mod cameras; // [트랙 B]
pub mod camhelper; // 카메라 찾기·미리보기 도우미 페이지
pub mod thumbs; // [트랙 B]
pub mod overlays; // [트랙 B]
pub mod sources; // 소스 · 화면 구성 (로드맵 1단계, docs/roadmap-sources.md)
pub mod scenes; // 화면 구성 편집 (로드맵 2단계)
pub mod outputs; // 출력 목록 · 출력마다 다른 구성 (로드맵 3단계)
pub mod scenectl; // 화면 구성 전환 · 자동화 (로드맵 4단계)
pub mod camtest; // 카메라 동시 열기 시험 (로드맵 3단계)
pub mod songs; // [트랙 B]
pub mod trackb; // [트랙 B]
pub mod flow; // [트랙 C]
pub mod slideedit; // [트랙 C]
pub mod themes; // [트랙 C]
pub mod importer; // [트랙 C]
pub mod bethlehem; // [트랙 C]
pub mod commentary; // [트랙 C]
pub mod templates; // [트랙 C]
pub mod usage; // [트랙 C]
// [트랙 D] 외부 연동·리모컨·내부망
pub mod companion;
pub mod devices;
pub mod dpapi;
pub mod lan;
pub mod remote;
pub mod trigger;
pub mod extcmd; // [점검 A] 외부 연동 명령 (/api/command)
pub mod bulletin; // [트랙 D]
pub mod trackd; // [트랙 D]
pub mod obs; // [트랙 D]
pub mod winkeys; // [트랙 D]
pub mod updates; // [트랙 D]
pub mod i18n; // [트랙 D]
pub mod actions; // 순서의 '⚡ 송출할 때 자동으로' 정규화·요약·매크로 저장

pub mod workspace;
pub mod pick; // 라이브러리에서 고른 것 넣기 (중복·연타 막기) · 바로 송출

#[cfg(test)]
mod ui_fix_tests; // 조작 화면 결함 수정(검토 1113) 회귀 테스트

