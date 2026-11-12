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
            it("-", "-", ""),
            it("c:settings", "설정", "Ctrl+,"),
        ] },
        Menu { label: "큐시트", items: vec![] },
        Menu { label: "송출", items: vec![
            it("c:history", "송출 기록", "Ctrl+H"),
        ] },
        Menu { label: "보기", items: vec![
            it("c:templates", "출력 템플릿 편집", ""),
        ] },
        Menu { label: "도구", items: vec![] },
        Menu { label: "도움말", items: vec![
            it("c:about", "정보 · 라이선스", ""),
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
            ui.menu_button(m.label, |ui| {
                for i in &m.items {
                    if i.id == "-" {
                        ui.separator();
                        continue;
                    }
                    let text = if i.key.is_empty() { i.label.to_string() } else { format!("{}      {}", i.label, i.key) };
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
