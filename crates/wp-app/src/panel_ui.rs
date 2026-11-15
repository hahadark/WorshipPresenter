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
    ("images", "이미지·영상"),
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

/// 라이브러리 탭 아이콘 (library.js TABS)
fn tab_icon(id: &str) -> &'static str {
    match id {
        "songs" => "music",
        "hymns" => "music-2",
        "images" => "image",
        "cameras" => "camera",
        "audio" => "volume-2",
        "overlays" => "layers",
        _ => "palette",
    }
}

/// .lib-tabs: 아이콘 탭 줄 (높이 34 · 12.5px · 종류 색 아이콘 · 고른 탭 굵게 + accent 밑줄) · 오른쪽 ⌃ 검색·단추 줄 접기
fn lib_tab_row(ui: &mut egui::Ui, tab: &mut String) {
    use crate::theme::{font, pal, type_ink, Weight};
    use egui::{Pos2, Rect, Sense, Stroke, Vec2};
    let p = pal();
    let (row, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::hover());
    ui.painter().hline(row.x_range(), row.bottom() - 0.5, Stroke::new(1.0, p.line));
    let mut x = row.left() + 6.0;
    for (id, label) in TABS {
        let on = *tab == id;
        let g = ui.painter().layout_no_wrap(tr(label), font(12.5, if on { Weight::SemiBold } else { Weight::Regular }), p.text);
        let w = 9.0 + 15.0 + 5.0 + g.size().x + 9.0;
        let r = Rect::from_min_size(Pos2::new(x, row.top()), Vec2::new(w, 34.0));
        x += w;
        let resp = ui.interact(r, egui::Id::new(("lib-tab", id)), Sense::click());
        if resp.clicked() {
            *tab = id.into();
        }
        let pt = ui.painter();
        if resp.hovered() && !on {
            pt.rect_filled(r, 0.0, p.hover);
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let fg = if on || resp.hovered() { p.text } else { p.muted };
        let ink = type_ink(id);
        crate::icons::paint(ui, Rect::from_min_size(Pos2::new(r.left() + 9.0, r.center().y - 7.5), Vec2::splat(15.0)), tab_icon(id), if on { ink } else { ink.gamma_multiply(0.8) });
        ui.painter().galley(Pos2::new(r.left() + 9.0 + 15.0 + 5.0, r.center().y - g.size().y / 2.0), g, fg);
        if on {
            ui.painter().rect_filled(Rect::from_min_max(Pos2::new(r.left(), r.bottom() - 2.0), r.right_bottom()), 0.0, p.accent);
        }
    }
    // .lib-fold: 검색·단추 줄 접기/펴기 (28×28, muted)
    let key = egui::Id::new("lib-bar-off");
    let off = ui.data(|d| d.get_temp::<bool>(key)).unwrap_or(false);
    let fr = Rect::from_center_size(Pos2::new(row.right() - 6.0 - 14.0, row.center().y), Vec2::splat(28.0));
    let resp = ui.interact(fr, egui::Id::new("lib-fold"), Sense::click()).on_hover_text(tr("검색·단추 줄 접기/펴기"));
    if resp.hovered() {
        ui.painter().rect_filled(fr, egui::CornerRadius::same(crate::theme::RADIUS), p.hover);
    }
    crate::icons::paint(ui, fr.shrink(6.5), if off { "chevron-down" } else { "chevron-up" }, if resp.hovered() { p.text } else { p.muted });
    if resp.clicked() {
        ui.data_mut(|d| d.insert_temp(key, !off));
    }
}

impl App {
    pub(crate) fn quick_panel(&mut self, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
        let mut slot = selection(ui, "quick-target", "main");
        let (list, mut tpl, settings) = {
            let c = self.core.lock();
            (c.settings.templates(), c.template_for_deck(&slot, None), c.settings.get().clone())
        };
        // 막대를 끄는 중에는 저장하지 않고 이 사본을 보인다 (tplcontrols final=false)
        let draft_id = egui::Id::new(("quick-draft", slot.clone()));
        if let Some(d) = ui.data(|d| d.get_temp::<Value>(draft_id)) {
            if d["id"] == tpl["id"] {
                tpl = d;
            }
        }
        let old_id = tpl["id"].as_str().unwrap_or("").to_string();
        let mut selected = old_id.clone();
        let mut split = if settings["split"]["mode"] == "auto" { "auto".to_string() } else { settings["split"]["count"].as_u64().unwrap_or(1).to_string() };
        let old_split = split.clone();
        let mut nav = settings["navMode"].as_str().unwrap_or("range").to_string();
        let old_nav = nav.clone();
        let collapsed_id = egui::Id::new("quick-collapsed");
        let mut collapsed = ui.data(|d| d.get_temp::<bool>(collapsed_id)).unwrap_or(false);
        // .quick-head: 대상 분할 단추 · 템플릿 · 테마 · 분할 · 이동 · (빈 칸) · 자세히 ▸ · 접기 ▾
        egui::Frame::new().inner_margin(egui::Margin::symmetric(12, 7)).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
                // '빠른 설정' 이름표는 패널 탭 이름과 겹쳐 뺐다 (원본은 탭이 없어 이름표가 있었음)
                let labels: Vec<(String, String)> = TARGETS.iter().map(|(id, l)| (id.to_string(), tr(l))).collect();
                let items: Vec<(String, &str)> = labels.iter().map(|(id, l)| (id.clone(), l.as_str())).collect();
                crate::widgets::segmented_with(ui, &mut slot, &items, crate::widgets::SegStyle { font: 13.0, pad: egui::vec2(9.0, 4.0), fill: false, radius: 6 });
                let tpl_opts: Vec<(String, String)> = list.iter().map(|t| (t["id"].as_str().unwrap_or("").to_string(), wp_core::i18n::tr_name(t["name"].as_str().unwrap_or("")))).collect();
                crate::quick_ui::select(ui, "quick-template", &mut selected, &tpl_opts, 170.0, "이 출력에 쓸 템플릿");
                if ui.add(crate::widgets::Btn::new(tr("테마")).tooltip(tr("구세군 테마 고르기"))).clicked() {
                    self.tc.themes.open = true;
                }
                let split_opts: Vec<(String, String)> = [("auto", "분할: 자동"), ("1", "분할: 한 절씩"), ("2", "분할: 2절씩"), ("3", "분할: 3절씩"), ("4", "분할: 4절씩")].iter().map(|(v, l)| (v.to_string(), tr(l))).collect();
                crate::quick_ui::select(ui, "quick-split", &mut split, &split_opts, 90.0, "슬라이드 나누기");
                let nav_opts = vec![("range".to_string(), tr("이동: 범위 안")), ("continuous".to_string(), tr("이동: 다음 절로 계속"))];
                crate::quick_ui::select(ui, "quick-nav", &mut nav, &nav_opts, 120.0, "F5/F6 으로 범위 끝에 닿았을 때");
                if !self.tb.note.is_empty() {
                    ui.label(crate::theme::rich(&self.tb.note, 12.0, crate::theme::Weight::Regular, crate::theme::pal().muted));
                }
                if ui.add(crate::widgets::Btn::new(format!("{} ▸", tr("자세히"))).tooltip(tr("굵기·줄 간격·여백·장절 색 등 모든 디자인 항목"))).clicked() {
                    self.tc.templates.open_with(&old_id); // 지금 고른 템플릿으로 편집기 열기
                }
                if ui.add(crate::widgets::Btn::new(format!("{} {}", tr(if collapsed { "펴기" } else { "접기" }), if collapsed { "▸" } else { "▾" })).tooltip(tr("빠른 설정 접기/펴기"))).clicked() {
                    collapsed = !collapsed;
                }
            });
        });
        crate::quick_ui::rule(ui);
        ui.data_mut(|d| d.insert_temp(collapsed_id, collapsed));
        remember(ui, "quick-target", slot.clone());
        if selected != old_id {
            self.core.lock().set_template(&slot, &selected);
            if let Some(t) = list.iter().find(|t| t["id"] == selected) {
                tpl = t.clone();
            }
            self.panel_rebuild();
        }
        if split != old_split || nav != old_nav {
            self.core.lock().settings.update(|v| {
                v["split"]["mode"] = json!(if split == "auto" { "auto" } else { "count" });
                if let Ok(n) = split.parse::<u64>() {
                    v["split"]["count"] = json!(n);
                }
                v["navMode"] = json!(nav);
            });
            self.panel_rebuild();
        }
        let mut out = crate::quick_ui::Out::default();
        if !collapsed {
            // .quick-body: 안쪽 8px 12px 10px, 넘치면 굴림
            egui::ScrollArea::vertical().id_salt("quick-body").auto_shrink([false, false]).show(ui, |ui| {
                egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 12, top: 8, bottom: 10 }).show(ui, |ui| {
                    out = crate::quick_ui::basic_controls(ui, &mut tpl);
                    // 순서 되돌리기·다시 하기(↶↷)는 큐시트 머리(cue_ui::undo_buttons)·큐시트 메뉴·Ctrl+Z/Y 에 있어 여기서 뺐다.
                });
            });
        }
        if out.more {
            self.tc.templates.open_with(&old_id);
        }
        if out.changed && !out.fin {
            ui.data_mut(|d| d.insert_temp(draft_id, tpl.clone()));
        }
        if out.changed && out.fin {
            ui.data_mut(|d| d.remove::<Value>(draft_id));
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
        // 점검용 --lib-tab cameras (--left-tab 3 과 함께 쓰면 라이브러리 카메라 탭 스크린샷)
        let first = std::env::args().position(|x| x == "--lib-tab").and_then(|i| std::env::args().nth(i + 1)).unwrap_or_else(|| "songs".into());
        let mut tab = selection(ui, "library-tab", &first);
        lib_tab_row(ui, &mut tab);
        remember(ui, "library-tab", tab.clone());
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
                .selectable_label(selected == id, wp_core::i18n::tr_name(tpl["name"].as_str().unwrap_or("")))
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
