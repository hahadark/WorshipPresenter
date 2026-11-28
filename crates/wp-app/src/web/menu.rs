//! [웹판] 메뉴 안 A 에서 브라우저로는 할 수 없는 항목을 뺀다 (설치판 메뉴는 그대로).
//! 까닭: 이 PC 의 파일 · 폴더 · 프로그램 실행 · 소켓(장비 · OBS · 내부망 서버) · 설치판 자체(업데이트 · 1.x 자료)를 쓰는 것.
//! 웹에서 따로 하는 것: 리모컨 · 웹 편집기 PIN · 사람 · AI 키 = 주인 화면(/owner, 띠의 [교회 관리]),
//! 성경 넣기 = cloud/scripts/bible-import.mjs · PUT /api/v1/bible/versions, 파일 = 4B 미디어 · 2C 자산 (웹 조작 화면에 붙이는 것은 다음 일).
//! 빼는 목록은 docs/web/egui-web.md '웹에서 뺀 메뉴' 와 같게 고친다.
use crate::menubar::{Menu, Sub};

pub const HIDDEN: &[&str] = &[
    // 파일: 이 PC 파일 · 폴더 · 설치판 자체
    "c:import-bible", "b:conti", "d:reimport", "b:pptx", "b:sync", "c:commentary", "d:bulletin",
    // 화면 · 연결: 소켓 · 이 PC 서버 · 장치
    "d:obs", "x:obs-url", "d:lan", "d:remote", "d:phonecam", "d:webedit", "d:cloud", "d:trigger", "d:devices", "b:cameras", "d:camtest", "b:lib",
    // 도움말: 설치판 업데이트 · 로그 폴더 · 이 PC 연결 점검
    "d:update", "b:diagnose", "b:logbundle", "b:logs",
];

pub fn filter(menus: &mut Vec<Menu>) {
    for m in menus.iter_mut() {
        m.items.retain(|i| !HIDDEN.contains(&i.id));
        for i in m.items.iter_mut() {
            if let Sub::Items(v) = &mut i.sub {
                v.retain(|x| !HIDDEN.contains(&x.id));
            }
        }
        // 빼고 남은 구분선 정리 (처음 · 끝 · 겹침)
        let mut out = Vec::with_capacity(m.items.len());
        for i in m.items.drain(..) {
            let line = i.id == "-";
            let last_line = out.last().is_none_or(|x: &crate::menubar::Item| x.id == "-");
            if line && last_line {
                continue;
            }
            out.push(i);
        }
        while out.last().is_some_and(|x| x.id == "-") {
            out.pop();
        }
        m.items = out;
    }
    menus.retain(|m| !m.items.is_empty());
}

/// 도움말 링크 등 밖 주소: 새 탭
pub fn open_link(url: &str) {
    if let Some(w) = web_sys::window() {
        let _ = w.open_with_url_and_target(url, "_blank");
    }
}
