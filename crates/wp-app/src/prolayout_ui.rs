//! (나) 어두운 전문가용 — 고정 배치 (시안 docs/design/pro-main.html `.mid` · `.tl`).
//!
//! ```text
//! ┌ 컨트롤 막대 (probar_ui) ────────────────────────────────────────────┐
//! │ 큐시트 판      │ 보기: 다음 | 송출 중 (뷰어 둘)          │ 인스펙터       │
//! │ [순서|성경|    │──────────────── 끌어서 높이 ─────────────│ [글자|무대|    │
//! │  라이브러리]   │ 본문 · 편집 (슬라이드 줄 · 검색 결과 ·   │  미디어]       │
//! │                │  편집칸)                                  │                │
//! ├ 예배 시간표 (timeline_ui) ──────────────────────────────────────────┤
//! └ 상태 줄 ─────────────────────────────────────────────────────────────┘
//! ```
//! 예전 독 작업공간(끌어서 패널 배치, 설정 `rsWorkspace`)은 고정 칸으로 바뀌었다. 저장된 옛 배치는 읽어서 두기만 하고
//! (지우지 않음, 잘못된 값이면 기본값) 쓰지 않는다. 여덟 패널은 모두 여기 어딘가에 있다:
//! 큐시트·성경 책·라이브러리 = 왼쪽 판 탭, 미리보기·LIVE = 뷰어 둘, 본문·편집 = 가운데 아래, 빠른 설정·무대·미디어 = 인스펙터 탭.
use crate::app::{Act, App, Snap};
use crate::theme::{self, Weight, pal};
use crate::widgets;
use eframe::egui::{self, Align, Id, Layout, Margin, Pos2, Rect, Sense, Stroke, UiBuilder, Vec2};
use serde_json::json;
use wp_core::i18n::tr;

/// 지난 프레임의 창 폭 (접기 기본값을 정할 때)
pub(crate) static LAST_W: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// 인스펙터 탭
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum InsTab {
    Text,
    Stage,
    Media,
}

/// 판 머리 띠 (.ph: 높이 30 · head 바탕 · 아래 edge 선 · 굵은 제목 + 흐린 부제). 오른쪽 칸은 `right` 가 채운다
pub(crate) fn pane_head(ui: &mut egui::Ui, title: &str, sub: &str, right: impl FnOnce(&mut egui::Ui)) {
    let p = pal();
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), theme::HEAD_H), Sense::hover());
    ui.painter().rect_filled(r, 0.0, p.head);
    ui.painter().hline(r.x_range(), r.bottom() - 0.5, Stroke::new(1.0, p.edge));
    let mut x = r.left() + 10.0;
    let g = ui.painter().layout_no_wrap(title.to_string(), theme::font(12.0, Weight::SemiBold), p.text);
    let gw = g.size().x;
    ui.painter().galley(Pos2::new(x, r.center().y - g.size().y / 2.0), g, p.text);
    x += gw + 8.0;
    let mut sub_right = x;
    if !sub.is_empty() {
        let g = ui.painter().layout_no_wrap(sub.to_string(), theme::font(11.5, Weight::Regular), p.muted);
        sub_right = x + g.size().x;
        ui.painter().with_clip_rect(r).galley(Pos2::new(x, r.center().y - g.size().y / 2.0), g, p.muted);
    }
    let rr = Rect::from_min_max(Pos2::new(sub_right.min(r.right()), r.top()), Pos2::new(r.right() - 8.0, r.bottom()));
    ui.scope_builder(UiBuilder::new().max_rect(rr).layout(Layout::right_to_left(Align::Center)), |ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        right(ui);
    });
}

/// 인스펙터 접는 구획 머리 (.sh: 높이 26 · ▾/▸ · 굵게 · 오른쪽 흐린 글). 펼쳐져 있으면 true
pub(crate) fn section(ui: &mut egui::Ui, key: &str, title: &str, default_open: bool) -> bool {
    let p = pal();
    let id = Id::new(("pro-sec", key));
    let mut open = ui.data(|d| d.get_temp::<bool>(id)).unwrap_or(default_open);
    let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), theme::SECTION_H), Sense::click());
    // 띠는 판 끝까지 (안쪽 여백을 넘어) 그린다
    let r = Rect::from_min_max(Pos2::new(ui.clip_rect().left().max(r.left() - 12.0), r.top()), Pos2::new(ui.clip_rect().right(), r.bottom()));
    let pt = ui.painter().with_clip_rect(ui.clip_rect());
    pt.rect_filled(r, 0.0, if resp.hovered() { p.hover } else { theme::mix(p.head, 70.0, p.panel) });
    pt.hline(r.x_range(), r.top() + 0.5, Stroke::new(1.0, p.line));
    pt.hline(r.x_range(), r.bottom() - 0.5, Stroke::new(1.0, p.edge));
    // ▾ / ▸ 작은 삼각형
    let c = Pos2::new(r.left() + 14.0, r.center().y);
    let tri = if open {
        vec![Pos2::new(c.x - 3.5, c.y - 2.0), Pos2::new(c.x + 3.5, c.y - 2.0), Pos2::new(c.x, c.y + 2.5)]
    } else {
        vec![Pos2::new(c.x - 2.0, c.y - 3.5), Pos2::new(c.x + 2.5, c.y), Pos2::new(c.x - 2.0, c.y + 3.5)]
    };
    pt.add(egui::Shape::convex_polygon(tri, p.text2, Stroke::NONE));
    pt.text(Pos2::new(r.left() + 24.0, r.center().y), egui::Align2::LEFT_CENTER, title, theme::font(11.5, Weight::SemiBold), p.text);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if resp.clicked() {
        open = !open;
        ui.data_mut(|d| d.insert_temp(id, open));
    }
    open
}

/// 판 바탕 틀 (판 색, 여백 없음)
fn pane_frame() -> egui::Frame {
    egui::Frame::new().fill(pal().panel).inner_margin(Margin::ZERO)
}

impl App {
    /// 배치 전체 (ui_body 가 위 막대 · 상태 줄 다음에 부른다)
    pub(crate) fn pro_layout(&mut self, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
        let p = pal();
        let full = ui.available_rect_before_wrap();
        let w = full.width();
        LAST_W.store(ui.ctx().content_rect().width() as u32, std::sync::atomic::Ordering::Relaxed);
        // ---- 아래: 예배 시간표 (보기 전용)
        if self.pro_timeline_visible() {
            egui::Panel::bottom("pro-timeline").resizable(false).frame(egui::Frame::new().fill(p.panel).inner_margin(Margin::ZERO)).show(ui, |ui| {
                ui.painter().hline(ui.max_rect().x_range(), ui.max_rect().top() - 0.5, Stroke::new(1.0, p.edge));
                self.timeline(ui, s);
            });
        }
        // ---- 왼쪽: 큐시트 판
        if self.pro_side_visible() {
            let def = (w * 0.22).clamp(250.0, 350.0);
            egui::Panel::left("pro-side")
                .resizable(true)
                .default_size(def)
                .size_range(230.0..=(w * 0.4).max(260.0))
                .frame(pane_frame())
                .show(ui, |ui| {
                    ui.painter().vline(ui.max_rect().right() + 0.5, ui.max_rect().y_range(), Stroke::new(1.0, p.edge));
                    self.pro_side(ui, s, acts);
                });
        }
        // ---- 오른쪽: 인스펙터
        if self.pro_inspector_visible() {
            let def = (w * 0.185).clamp(250.0, 330.0);
            egui::Panel::right("pro-inspector")
                .resizable(true)
                .default_size(def)
                .size_range(240.0..=(w * 0.4).max(260.0))
                .frame(pane_frame())
                .show(ui, |ui| {
                    ui.painter().vline(ui.max_rect().left() - 0.5, ui.max_rect().y_range(), Stroke::new(1.0, p.edge));
                    self.pro_inspector(ui, s, acts);
                });
        }
        // 알림 상자가 시간표를 덮지 않게: 창 아래에서 가운데 판 아래끝까지 높이
        let bottom_h = ui.ctx().content_rect().bottom() - ui.available_rect_before_wrap().bottom();
        ui.ctx().data_mut(|d| d.insert_temp(Id::new("pro-bottom-h"), bottom_h));
        // ---- 가운데: 뷰어 둘 + 본문·편집
        egui::CentralPanel::default().frame(egui::Frame::new().fill(p.panel).inner_margin(Margin::ZERO)).show(ui, |ui| {
            self.pro_center(ui, s, acts);
        });
    }

    /// 왼쪽 판: 머리 '큐시트' + [순서 | 성경 | 라이브러리]
    fn pro_side(&mut self, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
        let w = ui.available_width();
        ui.set_max_width(w);
        ui.spacing_mut().item_spacing.y = 0.0;
        let mut tab = crate::cue_ui::side_tab(ui.ctx());
        pane_head(ui, &tr("큐시트"), "", |ui| {
            let (a, b, c) = (tr("순서"), tr("성경 책"), tr("라이브러리"));
            let r = widgets::segmented_with(ui, &mut tab, &[("cue".to_string(), a.as_str()), ("books".to_string(), b.as_str()), ("library".to_string(), c.as_str())], widgets::SegStyle::TINY);
            widgets::mark("pro-side-tabs", r.rect);
        });
        crate::cue_ui::set_side_tab(ui.ctx(), &tab);
        ui.spacing_mut().item_spacing.y = 4.0;
        crate::hymns_ui::global(self, ui, acts); // Ctrl+V 붙여 넣기 받기 (어느 탭이든)
        match tab.as_str() {
            "books" => {
                ui.add_space(6.0);
                crate::cue_ui::books(self, ui);
            }
            "library" => {
                egui::ScrollArea::vertical().id_salt("pro-lib").auto_shrink([false, false]).show(ui, |ui| self.library_tabs_panel(ui, acts));
            }
            _ => {
                ui.add_space(6.0);
                self.cue_panel(ui, s, acts);
            }
        }
    }

    /// 오른쪽 인스펙터: 머리 '인스펙터' + [글자 | 무대 | 미디어], 그 아래 지금 다음(미리보기) 순서 이름
    fn pro_inspector(&mut self, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
        let p = pal();
        // 안의 칸이 판을 밀어 넓히지 않게 (egui 옆 판은 내용 폭을 따라 커진다)
        let w = ui.available_width();
        ui.set_max_width(w);
        ui.spacing_mut().item_spacing.y = 0.0;
        let id = Id::new("pro-ins-tab");
        let mut tab = ui.data(|d| d.get_temp::<InsTab>(id)).unwrap_or(InsTab::Text);
        widgets::mark("pro-ins-head", Rect::from_min_size(ui.cursor().min, Vec2::new(ui.available_width(), theme::HEAD_H)));
        pane_head(ui, &tr("인스펙터"), "", |ui| {
            let (a, b, c) = (tr("글자"), tr("무대"), tr("미디어"));
            let r = widgets::segmented_with(ui, &mut tab, &[(InsTab::Text, a.as_str()), (InsTab::Stage, b.as_str()), (InsTab::Media, c.as_str())], widgets::SegStyle::TINY);
            widgets::mark("pro-ins-tabs", r.rect);
        });
        ui.data_mut(|d| d.insert_temp(id, tab));
        // .ititle: 굵은 이름 + 흐린 '종류 · 템플릿'
        let deck = s.preview.as_ref().or(s.live.as_ref().map(|l| &l.deck));
        if tab == InsTab::Text {
            let (title, sub) = match deck {
                Some(d) => {
                    let idx = d.passage["cueIndex"].as_i64().filter(|i| *i >= 0);
                    let t = d.passage["itemTitle"].as_str().filter(|x| !x.is_empty()).or_else(|| d.passage["title"].as_str()).unwrap_or("");
                    let ty = d.passage["itemType"].as_str().unwrap_or("bible");
                    let tpl = s.templates.iter().find(|x| x.0 == s.active_main).map(|x| x.1.clone()).unwrap_or_default();
                    (match idx { Some(i) => format!("{} {}", i + 1, t), None => t.to_string() }, format!("{} · {} '{}'", tr(crate::center_ui::type_name(ty)), tr("템플릿"), tpl))
                }
                None => (tr("고른 순서 없음"), tr("큐시트 순서나 본문을 누르면 여기에 나옵니다")),
            };
            egui::Frame::new().inner_margin(Margin { left: 12, right: 12, top: 9, bottom: 8 }).show(ui, |ui| {
                ui.add(egui::Label::new(theme::rich(title, 14.0, Weight::Bold, p.text)).truncate().selectable(false));
                ui.add_space(2.0);
                ui.add(egui::Label::new(theme::rich(sub, 11.5, Weight::Regular, p.muted)).truncate().selectable(false));
            });
            ui.painter().hline(ui.max_rect().x_range(), ui.cursor().top(), Stroke::new(1.0, p.line));
        }
        ui.spacing_mut().item_spacing.y = 4.0;
        match tab {
            InsTab::Text => self.quick_panel(ui, s, acts),
            InsTab::Stage => {
                egui::ScrollArea::vertical().id_salt("pro-stage").auto_shrink([false, false]).show(ui, |ui| self.workspace_stage(ui));
            }
            InsTab::Media => {
                egui::ScrollArea::vertical().id_salt("pro-media").auto_shrink([false, false]).show(ui, |ui| {
                    self.media_panel(ui);
                });
            }
        }
    }

    /// 가운데: 머리 '보기' · 뷰어 둘(다음 | 송출 중) · 끌어서 높이 · 본문·편집
    fn pro_center(&mut self, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
        let p = pal();
        ui.spacing_mut().item_spacing.y = 0.0;
        let full = ui.available_rect_before_wrap();
        // 뷰어 높이 비율 (설정 rsProLayout.viewers, 0.3~0.75). 편집칸 '크게' 면 본문이 판 전체
        let big = crate::app::editor_big(ui.ctx());
        let ratio = self.core.lock().settings.get()["rsProLayout"]["viewers"].as_f64().unwrap_or(if full.height() < 520.0 { 0.6 } else { 0.56 }).clamp(0.25, 0.8) as f32;
        let vh = if big { 0.0 } else { ((full.height() - 6.0) * ratio).clamp(180.0_f32.min(full.height() * 0.5), (full.height() - 160.0).max(180.0)) };
        if vh > 0.0 {
            let top = Rect::from_min_size(full.min, Vec2::new(full.width(), vh));
            ui.scope_builder(UiBuilder::new().max_rect(top).id_salt("pro-viewers"), |ui| {
                ui.set_clip_rect(top.intersect(ui.clip_rect()));
                pane_head(ui, &tr("보기"), &tr("예배당 화면"), |_| {});
                widgets::mark("pro-view-head", Rect::from_min_size(top.min, Vec2::new(top.width(), theme::HEAD_H)));
                let body = Rect::from_min_max(Pos2::new(top.left(), top.top() + theme::HEAD_H), top.max);
                let half = (body.width() - 1.0) / 2.0;
                let left = Rect::from_min_size(body.min, Vec2::new(half, body.height()));
                let right = Rect::from_min_max(Pos2::new(left.right() + 1.0, body.top()), body.max);
                ui.painter().rect_filled(body, 0.0, p.edge);
                let vbg = theme::mix(p.panel, 80.0, p.bg);
                for (k, r) in [(false, left), (true, right)] {
                    ui.painter().rect_filled(r, 0.0, vbg);
                    ui.scope_builder(UiBuilder::new().max_rect(r).id_salt(("pro-viewer", k)), |ui| {
                        ui.set_clip_rect(r.intersect(ui.clip_rect()));
                        egui::ScrollArea::vertical().id_salt(("pro-viewer-scroll", k)).auto_shrink([false, false]).show(ui, |ui| {
                            if k {
                                self.deck_view(ui, &tr("LIVE"), s.live.as_ref().map(|l| &l.deck), true, s.live.as_ref(), s, acts);
                            } else {
                                self.deck_view(ui, &tr("미리보기"), s.preview.as_ref(), false, None, s, acts);
                            }
                        });
                    });
                }
            });
            // 끌어서 높이 바꾸기 (6px)
            let div = Rect::from_min_size(Pos2::new(full.left(), full.top() + vh), Vec2::new(full.width(), 6.0));
            let resp = ui.interact(div, Id::new("pro-center-split"), Sense::drag());
            ui.painter().rect_filled(div, 0.0, if resp.hovered() || resp.dragged() { theme::alpha(p.accent, 0.45) } else { p.edge });
            if resp.hovered() || resp.dragged() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
            }
            if resp.dragged() {
                if let Some(pos) = resp.interact_pointer_pos() {
                    let r = ((pos.y - full.top()) / (full.height() - 6.0).max(1.0)).clamp(0.25, 0.8);
                    self.core.lock().settings.update(|d| {
                        if !d["rsProLayout"].is_object() {
                            d["rsProLayout"] = json!({});
                        }
                        d["rsProLayout"]["viewers"] = json!((r * 100.0).round() / 100.0);
                    });
                }
            }
        }
        let bottom = Rect::from_min_max(Pos2::new(full.left(), full.top() + if vh > 0.0 { vh + 6.0 } else { 0.0 }), full.max);
        ui.scope_builder(UiBuilder::new().max_rect(bottom).id_salt("pro-main"), |ui| {
            ui.set_clip_rect(bottom.intersect(ui.clip_rect()));
            ui.spacing_mut().item_spacing.y = 4.0;
            self.main_panel(ui, s, acts);
        });
        ui.allocate_rect(full, Sense::hover());
    }
}
