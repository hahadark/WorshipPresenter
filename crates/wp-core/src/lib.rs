//! 워십프리젠터 Rust 판 코어
pub mod books;
pub mod groups; // 슬라이드 이름표 색·글 정리 (Electron shared/groups.js)
pub mod core;
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
pub mod preservice; // [트랙 A] 예배 전 화면
pub mod homefeed; // [트랙 A] 홈페이지 주보
pub mod parity_a; // [트랙 A] 기능 대조에서 빠진 작은 것들
pub mod clippaste; // [트랙 A]
pub mod songinbox; // [트랙 A]
pub mod measure;
pub mod parser;
pub mod presentation;
pub mod server;
pub mod settings;
pub mod slides;
pub mod textnorm;
pub mod util;
pub mod cues; // [트랙 B]
pub mod cuemgr; // [갈래 C] 큐시트 관리 창 (목록·찾기·정렬·새로·복제·휴지통·내보내기)
pub mod library; // [트랙 B]
pub mod localtime; // [트랙 B]
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
