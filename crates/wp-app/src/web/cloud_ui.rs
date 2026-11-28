//! [웹판] 설치판 '클라우드 연결' 창(cloud_ui.rs)의 자리. 브라우저의 조작 화면은 설치판을 클라우드에 잇는 창이 필요 없다.
//! 3단계 다음 일: 이 자리에 '클라우드 송출 주체로 붙기(/bridge, hello.source = web) · 넘겨받기(claim)' 상태를 보인다.
use wp_core::core::Shared;
use eframe::egui;

pub fn open() {}

pub fn entry_button(_ui: &mut egui::Ui) {}

pub fn show(_ctx: &egui::Context, _core: &Shared) {}
