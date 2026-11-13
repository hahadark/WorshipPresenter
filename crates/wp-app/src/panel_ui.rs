//! 빠른 설정과 라이브러리의 임베드 렌더. 기존 코어의 설정·템플릿 계약을 유지한다.
use crate::app::{Act, App, Snap};
use crate::form::{check, color, num, pick, text};
use eframe::egui;
use serde_json::{json, Value};
use wp_core::{core::Ev, i18n::tr, templates};

const TARGETS: [(&str, &str); 3] = [
    ("main", "예배당 화면"),
    ("overlay", "OBS 성경"),
    ("overlayLyrics", "OBS 찬송가"),
];
const TABS: [(&str, &str); 7] = [
    ("songs", "찬양"),
    ("hymns", "찬송가·PPT"),
    ("images", "그림·영상"),
    ("cameras", "카메라"),
    ("audio", "음악"),
    ("overlays", "오버레이"),
    ("templates", "템플릿"),
];
fn selection(ui: &egui::Ui, key: &str, default: &str) -> String {
    ui.data(|d| d.get_temp::<String>(egui::Id::new(key)))
        .unwrap_or_else(|| default.into())
}
fn remember(ui: &mut egui::Ui, key: &str, value: String) {
    ui.data_mut(|d| d.insert_temp(egui::Id::new(key), value));
}

impl App {
    pub(crate) fn quick_panel(&mut self, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
        self.workspace_controls(ui, s, acts);
        let mut slot = selection(ui, "quick-target", "main");
        ui.horizontal_wrapped(|ui| {
            for (id, label) in TARGETS {
                ui.selectable_value(&mut slot, id.into(), tr(label));
            }
        });
        remember(ui, "quick-target", slot.clone());
        let (list, mut tpl, settings) = {
            let c = self.core.lock();
            (
                c.settings.templates(),
                c.template_for_deck(&slot, None),
                c.settings.get().clone(),
            )
        };
        let old_id = tpl["id"].as_str().unwrap_or("").to_string();
        let mut selected = old_id.clone();
        egui::ComboBox::from_id_salt("quick-template")
            .selected_text(tpl["name"].as_str().unwrap_or(""))
            .show_ui(ui, |ui| {
                for t in &list {
                    ui.selectable_value(
                        &mut selected,
                        t["id"].as_str().unwrap_or("").into(),
                        t["name"].as_str().unwrap_or(""),
                    );
                }
            });
        if selected != old_id {
            self.core.lock().set_template(&slot, &selected);
            if let Some(t) = list.iter().find(|t| t["id"] == selected) {
                tpl = t.clone();
            }
            self.panel_rebuild();
        }
        let mut split = if settings["split"]["mode"] == "auto" {
            0
        } else {
            settings["split"]["count"].as_u64().unwrap_or(1) as usize
        };
        let old_split = split;
        egui::ComboBox::from_id_salt("quick-split")
            .selected_text(tr(match split {
                1 => "분할: 한 절씩",
                2 => "분할: 2절씩",
                3 => "분할: 3절씩",
                4 => "분할: 4절씩",
                _ => "분할: 자동",
            }))
            .show_ui(ui, |ui| {
                for (n, label) in [
                    (0, "분할: 자동"),
                    (1, "분할: 한 절씩"),
                    (2, "분할: 2절씩"),
                    (3, "분할: 3절씩"),
                    (4, "분할: 4절씩"),
                ] {
                    ui.selectable_value(&mut split, n, tr(label));
                }
            });
        let mut nav = settings["navMode"].as_str().unwrap_or("range").to_string();
        let old_nav = nav.clone();
        egui::ComboBox::from_id_salt("quick-nav")
            .selected_text(tr(if nav == "continuous" {
                "이동: 다음 절로 계속"
            } else {
                "이동: 범위 안"
            }))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut nav, "range".into(), tr("이동: 범위 안"));
                ui.selectable_value(&mut nav, "continuous".into(), tr("이동: 다음 절로 계속"));
            });
        if split != old_split || nav != old_nav {
            self.core.lock().settings.update(|v| {
                v["split"]["mode"] = json!(if split == 0 { "auto" } else { "count" });
                if split > 0 {
                    v["split"]["count"] = json!(split);
                }
                v["navMode"] = json!(nav);
            });
            self.panel_rebuild();
        }
        let mut changed = false;
        egui::CollapsingHeader::new(tr("기본 디자인"))
            .default_open(true)
            .show(ui, |ui| {
                changed |= basic_fields(ui, &mut tpl);
            });
        if changed {
            let result = {
                let mut c = self.core.lock();
                let result = templates::save_template(&mut c.settings, &tpl);
                c.emit(Ev::Live);
                c.emit(Ev::Ui);
                result
            };
            self.tb.note = match result {
                Ok(_) => tr("저장했습니다."),
                Err(e) => e,
            };
            self.panel_rebuild();
        }
        if ui.button(tr("자세히")).clicked() {
            self.tc.templates.open = true;
        }
        if ui.button(tr("테마")).clicked() {
            self.tc.themes.open = true;
        }
        if !self.tb.note.is_empty() {
            ui.label(&self.tb.note);
        }
        self.trackb_toolbar(ui);
    }

    /// 재송출하지 않고 덱만 재계산한다. 열린 큐가 LIVE의 큐와 다르면 순서 번호를 재사용하지 않는다.
    fn panel_rebuild(&mut self) {
        let mut c = self.core.lock();
        let rebuild = |c: &mut wp_core::core::Core, deck: &wp_core::presentation::Deck| {
            let meta = &deck.passage;
            let mut next = if meta["itemType"] == "bible" {
                let version = meta["versionId"].as_str().unwrap_or("");
                let query = meta["query"].as_str().unwrap_or("");
                let r = c.store.resolve_reference(version, query);
                if r["ok"] != true {
                    return None;
                }
                let p = wp_core::flow::with_parallel(c, r["passage"].clone());
                let item = c
                    .cue
                    .as_ref()
                    .filter(|q| meta["cueId"] == q.id)
                    .and_then(|q| q.items.get(meta["cueIndex"].as_u64()? as usize));
                wp_core::presentation::Deck {
                    passage: meta.clone(),
                    slides: wp_core::slides::build_bible(&p, &c.split_opts(item)),
                    index: deck.index,
                }
            } else if matches!(meta["itemType"].as_str(), Some("song" | "text" | "hymn" | "title")) {
                let index = meta["cueIndex"].as_u64()? as usize;
                let cue = c.cue.as_ref().filter(|q| meta["cueId"] == q.id)?;
                if index >= cue.items.len() { return None; }
                let mut rebuilt = c.item_deck(index).ok()?;
                // 덱만 재구성한다. 재송출·사용 기록·자동 동작은 실행하지 않는다.
                rebuilt.passage = meta.clone();
                rebuilt.index = deck.index;
                if let Some(key) = deck.slides.get(deck.index).and_then(|s| s["ekey"].as_str()) {
                    if let Some(k) = rebuilt.slides.iter().position(|s| s["ekey"] == key) { rebuilt.index = k; }
                }
                rebuilt
            } else {
                return None;
            };
            // 현재 절을 유지하되 분할 결과가 짧아졌으면 끝으로 제한한다.
            if let Some(slide) = deck.slides.get(deck.index) {
                if let Some(part) = slide["parts"].as_array().and_then(|p| p.first()) {
                    if let (Some(ch), Some(v)) = (part["chapter"].as_i64(), part["verse"].as_i64())
                    {
                        next.index = wp_core::slides::index_of_verse(&next.slides, ch, v);
                    }
                }
            }
            next.index = next.index.min(next.slides.len().saturating_sub(1));
            Some(next)
        };
        if let Some(d) = c.pres.preview.clone().and_then(|d| rebuild(&mut c, &d)) {
            c.pres.preview = Some(d);
        }
        if let Some(d) = c.pres.live.clone().and_then(|l| rebuild(&mut c, &l.deck)) {
            if let Some(l) = &mut c.pres.live {
                l.deck = d;
            }
        }
        c.emit(Ev::Live);
        c.emit(Ev::Ui);
    }

    pub(crate) fn library_tabs_panel(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Act>) {
        let mut tab = selection(ui, "library-tab", "songs");
        ui.horizontal_wrapped(|ui| {
            for (id, label) in TABS {
                ui.selectable_value(&mut tab, id.into(), tr(label));
            }
        });
        remember(ui, "library-tab", tab.clone());
        ui.separator();
        match tab.as_str() {
            "hymns" => crate::hymns_ui::tab(self, ui, acts),
            "images" => self.library_kind_panel(ui, "image"),
            "audio" => {
                self.library_kind_panel(ui, "audio");
                self.music_bar(ui);
            }
            "cameras" => self.panel_cameras(ui, acts),
            "overlays" => self.overlays_panel(ui),
            "templates" => self.panel_templates(ui),
            _ => self.songs_panel(ui),
        }
    }
    fn panel_cameras(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Act>) {
        // 화면 셋 공용 목록(camscan): 연결된 카메라(도우미가 찾은 것) → 설정 → 큐시트. 출력 웹과 같은 deviceId 계약이며 기본 장치도 고를 수 있다.
        crate::camscan::scan_bar(ui, &self.core);
        let cameras = crate::camscan::choices(&self.core.lock());
        let default = wp_core::cameras::Choice { device_id: String::new(), label: tr("기본 카메라"), connected: false, configured: false };
        for c in std::iter::once(default).chain(cameras) {
            let (id, label) = (c.device_id.clone(), c.label.clone());
            ui.group(|ui| {
                ui.label(if id.is_empty() { format!("📷 {label}") } else { crate::camscan::display_name(&c) });
                ui.horizontal_wrapped(|ui| {
                    if ui.button(tr("미리보기")).on_hover_text(tr("작은 창에 카메라를 엽니다 (출력과 같은 카메라 설정 적용)")).clicked() { crate::camscan::preview(&self.core, &id, &label); }
                    if ui.button(tr("순서로")).clicked() { acts.push(Act::AddItem("camera", Some(json!({"title": label, "deviceId": id, "label": label, "fit": "cover"})))); }
                    if ui.button(tr("배경으로")).clicked() { self.core.lock().set_background_media(Some(json!({"type":"camera", "deviceId":id,"label":label}))); }
                });
            });
        }
        if ui.button(tr("배경 미디어 끄기")).clicked() {
            self.core.lock().set_background_media(None);
        }
    }
    fn panel_templates(&mut self, ui: &mut egui::Ui) {
        let list = self.core.lock().settings.templates();
        let mut selected = selection(ui, "library-template", "");
        for tpl in list {
            let id = tpl["id"].as_str().unwrap_or("");
            if ui
                .selectable_label(selected == id, tpl["name"].as_str().unwrap_or(""))
                .clicked()
            {
                selected = id.into();
            }
            if selected == id {
                ui.horizontal_wrapped(|ui| {
                    for (slot, label) in TARGETS {
                        if ui.button(tr(label)).clicked() {
                            self.core.lock().set_template(slot, id);
                            self.panel_rebuild();
                        }
                    }
                });
                let mut draft = tpl.clone();
                if basic_fields(ui, &mut draft) {
                    let result = templates::save_template(&mut self.core.lock().settings, &draft);
                    self.tb.note = match result {
                        Ok(_) => tr("저장했습니다."),
                        Err(e) => e,
                    };
                    self.panel_rebuild();
                }
                if ui.button(tr("복제")).clicked() {
                    let _ = templates::duplicate_template(&mut self.core.lock().settings, id, None);
                }
                if ui.button(tr("삭제")).clicked() {
                    if let Err(e) = templates::delete_template(&mut self.core.lock().settings, id) {
                        self.tb.note = e;
                    }
                }
            }
        }
        remember(ui, "library-template", selected);
        if ui.button(tr("새 템플릿")).clicked() {
            let mut tpl = wp_core::settings::TEMPLATE_BASE.clone();
            tpl["name"] = json!(tr("새 템플릿"));
            let _ = templates::save_template(&mut self.core.lock().settings, &tpl);
        }
        if ui.button(tr("자세히")).clicked() {
            self.tc.templates.open = true;
        }
        if ui.button(tr("테마")).clicked() {
            self.tc.themes.open = true;
        }
        if !self.tb.note.is_empty() {
            ui.label(&self.tb.note);
        }
    }
}
fn basic_fields(ui: &mut egui::Ui, t: &mut Value) -> bool {
    let mut ch = false;
    egui::Grid::new("panel-basic-design")
        .num_columns(2)
        .show(ui, |ui| {
            ch |= text(ui, t, &["font", "family"], "글꼴", 180.0);
            ui.end_row();
            ch |= num(ui, t, &["font", "size"], "글자 크기", 8.0, 400.0, 1.0);
            ui.end_row();
            ch |= color(ui, t, &["font", "color"], "글자 색");
            ui.end_row();
            ch |= color(ui, t, &["background", "color"], "배경 색");
            ui.end_row();
            ch |= pick(
                ui,
                t,
                &["align"],
                "정렬",
                &[("left", "왼쪽"), ("center", "가운데"), ("right", "오른쪽")],
            );
            ui.end_row();
            ch |= check(ui, t, &["font", "autoFit"], "글자 자동 맞춤");
            ui.end_row();
        });
    ch
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_quick_targets_and_library_tabs_are_preserved() {
        assert_eq!(
            TARGETS.map(|(id, _)| id),
            ["main", "overlay", "overlayLyrics"]
        );
        assert_eq!(
            TABS.map(|(id, _)| id),
            [
                "songs",
                "hymns",
                "images",
                "cameras",
                "audio",
                "overlays",
                "templates"
            ]
        );
    }
}
