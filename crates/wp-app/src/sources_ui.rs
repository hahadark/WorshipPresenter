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

/// 오른쪽부터: OBS · 예배당. 화면이 붙은 출력에 나가는 중이면 빨강(송출 중), 출력 화면이 꺼져 있으면 흐린 글자(나갈 차례만)
fn on_air_badges(ui: &mut egui::Ui, r: &SourceRow, connected: &[&str]) {
    for (o, label) in [("overlay", "OBS"), ("main", "예배당")] {
        if !r.on_air.contains(&o) {
            continue;
        }
        if connected.contains(&o) {
            widgets::badge(ui, &tr(label), Badge::Live);
        } else {
            ui.add(egui::Label::new(theme::rich(tr(label), 11.0, Weight::Regular, pal().muted2)).selectable(false))
                .on_hover_text(tr("출력 화면이 꺼져 있어 지금은 나가지 않습니다"));
        }
    }
}

impl App {
    pub(crate) fn sources_panel(&mut self, ui: &mut egui::Ui) {
        let p = pal();
        let (rows, connected) = {
            let c = self.core.lock();
            (c.sources_status(), c.connected_outputs())
        };
        egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 12, top: 8, bottom: 4 }).show(ui, |ui| {
            ui.add(egui::Label::new(theme::rich(tr("화면에 나올 수 있는 것과 지금 나가고 있는 곳입니다. 빨간 표시가 송출 중입니다."), 11.5, Weight::Regular, p.muted)).wrap());
        });
        // 화면 구성 (로드맵 2단계): 출력마다 쓰는 구성 · 편집 창 열기
        if crate::prolayout_ui::section(ui, "src-scenes", &tr("화면 구성"), true) {
            let (names, used) = {
                let c = self.core.lock();
                let names: Vec<(String, String)> = c.scenes().iter().map(|s| (s["id"].as_str().unwrap_or("").to_string(), s["name"].as_str().unwrap_or("").to_string())).collect();
                (names, [c.output_scene("main"), c.output_scene("overlay")])
            };
            let mut open: Option<String> = None;
            egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 10, top: 4, bottom: 6 }).show(ui, |ui| {
                for (i, (o, label)) in [("main", "예배당"), ("overlay", "방송 (OBS)")].into_iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.set_min_height(24.0);
                        ui.add(egui::Label::new(theme::rich(tr(label), 12.0, Weight::Regular, p.muted)).selectable(false));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let cur = used[i].clone();
                            let shown = names.iter().find(|(k, _)| *k == cur).map(|(_, n)| n.clone()).unwrap_or_else(|| tr("전체 화면 (기본)"));
                            let mut pick = cur.clone();
                            egui::ComboBox::from_id_salt(("src-scene-pick", o)).width(170.0).selected_text(shown).show_ui(ui, |ui| {
                                ui.selectable_value(&mut pick, wp_core::scenes::FULL.to_string(), tr("전체 화면 (기본)"));
                                for (k, n) in &names {
                                    ui.selectable_value(&mut pick, k.clone(), n);
                                }
                            });
                            if pick != cur {
                                let r = self.core.lock().set_output_scene(o, &pick);
                                if let Err(e) = r {
                                    self.core.lock().notify(e.to_string(), "error");
                                }
                            }
                        });
                    });
                }
                if ui.add(widgets::Btn::new(tr("화면 구성 편집…")).icon("layout-grid").size(widgets::Size::Sm)).clicked() {
                    open = Some(used[1].clone());
                }
            });
            if let Some(id) = open {
                self.tc.scenes.open_with(&id);
            }
        }
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
                        self.user_source_row(ui, r, &connected);
                    } else {
                        Self::source_row(ui, r, &connected);
                    }
                }
                if gi == 3 {
                    self.add_web_source(ui);
                }
            });
        }
    }

    fn source_row(ui: &mut egui::Ui, r: &SourceRow, connected: &[&str]) {
        let p = pal();
        ui.horizontal(|ui| {
            ui.set_min_height(24.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                on_air_badges(ui, r, connected);
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

    fn user_source_row(&mut self, ui: &mut egui::Ui, r: &SourceRow, connected: &[&str]) {
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
                on_air_badges(ui, r, connected);
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
