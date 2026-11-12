//! [트랙 C] 상단 메뉴 줄 (Electron menubar.js 처럼 파일·큐시트·송출·보기·도구·도움말)
//! 다른 트랙도 `menus()` 의 목록에 항목을 더하고, 고른 id 를 자기 처리기에서 받으면 된다.
//! 항목 id 는 "트랙:동작" 처럼 짓는다 (예: "c:settings").

use eframe::egui::{self, RichText};

pub struct Item {
    pub id: &'static str,
    pub label: &'static str,
    pub key: &'static str,
}

pub struct Menu {
    pub label: &'static str,
    pub items: Vec<Item>,
}

const fn it(id: &'static str, label: &'static str, key: &'static str) -> Item {
    Item { id, label, key }
}

/// 메뉴 목록. 빈 label("-") 은 구분선
pub fn menus() -> Vec<Menu> {
    vec![
        Menu { label: "파일", items: vec![
            it("c:import-bible", "성경 번역본 가져오기…", ""),
            it("c:usage", "사용 기록 (저작권 보고)", ""),
            it("b:sync", "여러 PC 동기화 · 백업", ""), // [트랙 B]
            it("-", "-", ""),
            it("c:settings", "설정", "Ctrl+,"),
        ] },
        Menu { label: "큐시트", items: vec![
            it("c:slideedit", "슬라이드 편집 (고른 순서)", "Ctrl+E"),
            it("b:cues", "큐시트 관리 (휴지통 · 복제 · 내보내기)", ""),
            it("b:undo", "실행 취소", "Ctrl+Z"),
            it("b:redo", "다시 실행", "Ctrl+Y"),
            it("-", "-", ""),
            it("b:print", "큐시트 인쇄", "Ctrl+P"),
        ] },
        Menu { label: "송출", items: vec![
            it("c:history", "송출 기록", "Ctrl+H"),
            it("-", "-", ""),
            it("b:quick", "즉석 자막", "Ctrl+T"),
            it("b:message", "회중 메시지", "Ctrl+M"),
            it("b:overlays", "오버레이 (시계 · 카운트다운 · 배너)", ""),
            it("b:stage", "무대 화면", ""),
            it("b:macros", "매크로 · 예약 실행", "Alt+1~9"),
            it("b:draw", "화면에 그리기 · 강조", ""), // [트랙 B]
        ] },
        Menu { label: "보기", items: vec![
            it("w:workspaces", "작업공간 · 패널 배치", ""),
            it("c:commentary", "주석 보기 · 가져오기", ""),
            it("c:templates", "출력 템플릿 편집", ""),
            it("c:themes", "테마 고르기 (구세군 · 방송 스타일)", ""),
            it("-", "-", ""),
            it("c:obsfull", "OBS 자막: 슬라이드 그대로 켜기·끄기", ""),
            it("c:obspip", "OBS 자막: 그림 슬라이드 작게 켜기·끄기", ""),
            it("d:output", "출력 설정 (예배당 창 · 모니터 · OBS 숨김)", ""), // [트랙 D]
        ] },
        Menu { label: "도구", items: vec![
            it("b:songs", "찬양 라이브러리", ""),
            it("b:lib", "그림 · 음악 · 영상 라이브러리", ""),
            it("b:cameras", "카메라 설정", ""), // [트랙 B]
            it("-", "-", ""),
            it("d:remote", "휴대폰 리모컨", ""),
            it("d:trigger", "스트림덱 · 외부 단추", ""),
            it("d:devices", "Companion · 장비 (X32 · PTZ)", ""),
            it("d:lan", "내부망 출력 (다른 PC 의 OBS)", ""),
            it("d:obs", "OBS 원격 조작", ""), // [트랙 D]
            it("d:bulletin", "주보 → 큐시트", ""), // [트랙 D]
            it("a:home", "홈페이지 주보 → 큐시트 · 광고", ""), // [트랙 A]
            it("a:points", "설교 포인트 슬라이드", ""), // [트랙 A]
            it("a:preservice", "예배 전 화면 만들기", ""), // [트랙 A]
        ] },
        Menu { label: "도움말", items: vec![
            it("b:diagnose", "연결 점검 (예배 전 점검)", ""), // [트랙 B]
            it("b:logbundle", "로그 묶음 저장…", ""), // [트랙 B]
            it("b:logs", "로그 폴더 열기", ""), // [트랙 B]
            it("-", "-", ""),
            it("a:help-manual", "사용 설명서", ""), // [트랙 A]
            it("a:help-readme", "그림으로 보는 사용법 (README)", ""), // [트랙 A]
            it("a:help-obs", "OBS 연결 안내", ""), // [트랙 A]
            it("a:help-keys", "단축키 보기", ""), // [트랙 A]
            it("a:help-news", "새 소식 (버전별 바뀐 점)", ""), // [트랙 A]
            it("a:help-issue", "문제 알리기 · 건의", ""), // [트랙 A]
            it("c:about", "정보 · 라이선스", ""),
            it("d:update", "새 버전 확인", ""), // [트랙 D]
        ] },
    ]
}

/// 메뉴 줄을 그리고, 누른 항목 id 를 돌려준다
pub fn show(ui: &mut egui::Ui, menus: &[Menu]) -> Option<&'static str> {
    let mut picked = None;
    ui.horizontal(|ui| {
        for m in menus {
            if m.items.is_empty() {
                continue;
            }
            ui.menu_button(wp_core::i18n::tr(m.label), |ui| { // [트랙 D] 영어 화면
                for i in &m.items {
                    if i.id == "-" {
                        ui.separator();
                        continue;
                    }
                    let text = if i.key.is_empty() { wp_core::i18n::tr(i.label) } else { format!("{}      {}", wp_core::i18n::tr(i.label), i.key) }; // [트랙 D] 영어 화면
                    if ui.button(RichText::new(text)).clicked() {
                        picked = Some(i.id);
                        ui.close();
                    }
                }
            });
        }
    });
    picked
}
