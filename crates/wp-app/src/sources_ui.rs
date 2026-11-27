//! 인스펙터 '소스' 탭 (로드맵 1단계, docs/roadmap-sources.md):
//! 화면에 나올 수 있는 것(소스)과 지금 어느 출력(예배당 · OBS)에 나가고 있는지. 나가는 중인 것만 빨강.
//! 웹 페이지 소스는 여기서 더하고 켜고 지운다. 화면 캡처는 허락 창 없이 열 방법이 없어 아직 없다.
use crate::app::App;
use crate::theme::{self, pal, Weight};
use crate::widgets::{self, Badge};
use eframe::egui::{self, Id};
use serde_json::{json, Value};
use wp_core::i18n::tr;
use wp_core::sources::SourceRow;

/// 묶음: 제목과 그 묶음에 드는 종류
fn group_of(r: &SourceRow) -> usize {
    match r.kind.as_str() {
        "live" | "background" | "media" | "slide-bg" => 0,
        "camera" | "phone" => 2,
        "web" => 3,
        _ => 1, // 오버레이 · 회중 메시지 · 로고 화면 · 그리기
    }
}

const GROUPS: [&str; 4] = ["화면 내용", "자막 · 오버레이", "카메라", "웹 페이지"];

fn on_air_badges(ui: &mut egui::Ui, r: &SourceRow) {
    // 오른쪽부터: OBS · 예배당 (나가는 중만, 빨강 = 송출 중)
    for (o, label) in [("overlay", "OBS"), ("main", "예배당")] {
        if r.on_air.contains(&o) {
            widgets::badge(ui, &tr(label), Badge::Live);
        }
    }
}

impl App {
    pub(crate) fn sources_panel(&mut self, ui: &mut egui::Ui) {
        let p = pal();
        let rows = self.core.lock().sources_status();
        egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 12, top: 8, bottom: 4 }).show(ui, |ui| {
            ui.add(egui::Label::new(theme::rich(tr("화면에 나올 수 있는 것과 지금 나가고 있는 곳입니다. 빨간 표시가 송출 중입니다."), 11.5, Weight::Regular, p.muted)).wrap());
        });
        for (gi, g) in GROUPS.iter().enumerate() {
            let list: Vec<&SourceRow> = rows.iter().filter(|r| group_of(r) == gi).collect();
            if !crate::prolayout_ui::section(ui, &format!("src-{gi}"), &tr(g), true) {
                continue;
            }
            egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 10, top: 4, bottom: 6 }).show(ui, |ui| {
                if list.is_empty() && gi != 3 {
                    ui.add(egui::Label::new(theme::rich(tr("없음"), 11.5, Weight::Regular, p.muted2)));
                }
                for r in &list {
                    if r.user {
                        self.user_source_row(ui, r);
                    } else {
                        Self::source_row(ui, r);
                    }
                }
                if gi == 3 {
                    self.add_web_source(ui);
                }
            });
        }
    }

    fn source_row(ui: &mut egui::Ui, r: &SourceRow) {
        let p = pal();
        ui.horizontal(|ui| {
            ui.set_min_height(24.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                on_air_badges(ui, r);
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add(egui::Label::new(theme::rich(tr(&r.name), 12.5, Weight::SemiBold, if r.on_air.is_empty() { p.text2 } else { p.text })).selectable(false));
                    if !r.detail.is_empty() {
                        // 사용자 글(제목 · 파일 이름 · 사람 이름)은 번역하지 않는다
                        ui.add(egui::Label::new(theme::rich(&r.detail, 11.5, Weight::Regular, p.muted)).truncate().selectable(false));
                    }
                });
            });
        });
    }

    fn user_source_row(&mut self, ui: &mut egui::Ui, r: &SourceRow) {
        let p = pal();
        let Some(mut src) = self.core.lock().user_sources().into_iter().find(|s| s["id"] == r.id.as_str()) else { return };
        let mut changed = false;
        let mut delete = false;
        ui.horizontal(|ui| {
            ui.set_min_height(24.0);
            let mut on = r.enabled;
            if ui.checkbox(&mut on, "").on_hover_text(tr("켜기 · 끄기")).changed() {
                src["on"] = json!(on);
                changed = true;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button(tr("지우기")).clicked() {
                    delete = true;
                }
                on_air_badges(ui, r);
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    ui.add(egui::Label::new(theme::rich(&r.name, 12.5, Weight::SemiBold, if r.on_air.is_empty() { p.text2 } else { p.text })).selectable(false));
                    ui.add(egui::Label::new(theme::rich(&r.detail, 11.0, Weight::Regular, p.muted)).truncate().selectable(false));
                });
            });
        });
        ui.horizontal(|ui| {
            ui.add_space(26.0);
            let outs: Vec<String> = src["outputs"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
            for (o, label) in [("main", "예배당"), ("overlay", "OBS")] {
                let mut has = outs.iter().any(|x| x == o);
                if ui.checkbox(&mut has, tr(label)).changed() {
                    let mut n: Vec<String> = outs.iter().filter(|x| x.as_str() != o).cloned().collect();
                    if has {
                        n.push(o.to_string());
                    }
                    src["outputs"] = json!(n);
                    changed = true;
                }
            }
            let mut place = src["place"].as_str().unwrap_or("under").to_string();
            let before = place.clone();
            egui::ComboBox::from_id_salt(("src-place", &r.id))
                .selected_text(tr(if place == "over" { "글 위" } else { "글 아래" }))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut place, "under".into(), tr("글 아래"));
                    ui.selectable_value(&mut place, "over".into(), tr("글 위"));
                });
            if place != before {
                src["place"] = json!(place);
                changed = true;
            }
        });
        if delete {
            self.core.lock().delete_source(&r.id);
        } else if changed {
            let res = self.core.lock().save_source(&src);
            if let Err(e) = res {
                self.core.lock().notify(e.to_string(), "error");
            }
        }
    }

    fn add_web_source(&mut self, ui: &mut egui::Ui) {
        let p = pal();
        let id = Id::new("src-add-web");
        let (mut name, mut url) = ui.data(|d| d.get_temp::<(String, String)>(id)).unwrap_or_default();
        ui.add_space(4.0);
        ui.add(egui::TextEdit::singleline(&mut name).hint_text(tr("이름 (예: 교회 홈페이지)")).desired_width(f32::INFINITY));
        let mut go = false;
        ui.horizontal(|ui| {
            let r = ui.add(egui::TextEdit::singleline(&mut url).hint_text(tr("웹 주소 (https://…)")).desired_width(ui.available_width() - 64.0));
            go |= r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            go |= ui.add_enabled(!url.trim().is_empty(), egui::Button::new(tr("더하기"))).clicked();
        });
        ui.add(egui::Label::new(theme::rich(tr("예배당 화면에 웹 페이지를 띄웁니다. 사이트가 다른 화면 안에 넣기를 막으면 빈 칸으로 나옵니다."), 11.0, Weight::Regular, p.muted)).wrap());
        if go {
            let v: Value = json!({ "kind": "web", "name": name.trim(), "url": url.trim(), "on": false, "outputs": ["main"] });
            let res = self.core.lock().save_source(&v);
            match res {
                Ok(_) => {
                    name.clear();
                    url.clear();
                }
                Err(e) => self.core.lock().notify(e.to_string(), "error"),
            }
        }
        ui.data_mut(|d| d.insert_temp(id, (name, url)));
    }
}
