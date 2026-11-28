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
use wp_core::i18n::{tr, tr_args};

/// 지난 프레임의 창 폭 (접기 기본값을 정할 때)
pub(crate) static LAST_W: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// 인스펙터 탭
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum InsTab {
    Text,
    Stage,
    Media,
    /// 소스 목록 (무엇이 있고 어디에 나가는지)
    Sources,
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

/// 접힌 판 손잡이: 판이 있던 가장자리에 얇은 띠(10px) + 가운데 작은 삼각형. 누르면 true
pub(crate) const HANDLE_W: f32 = 10.0;
fn collapsed_handle(ui: &mut egui::Ui, id: &str, left: bool, tip: &str) -> bool {
    let p = pal();
    let mut clicked = false;
    let panel = if left { egui::Panel::left(id.to_string()) } else { egui::Panel::right(id.to_string()) };
    panel.resizable(false).exact_size(HANDLE_W).frame(egui::Frame::new().fill(p.head).inner_margin(Margin::ZERO)).show(ui, |ui| {
        let r = ui.max_rect();
        let resp = ui.interact(r, Id::new((id, "hit")), Sense::click()).on_hover_text(tip);
        widgets::mark(id, r);
        let pt = ui.painter();
        if resp.hovered() {
            pt.rect_filled(r, 0.0, p.hover);
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let x = if left { r.right() - 0.5 } else { r.left() + 0.5 };
        pt.vline(x, r.y_range(), Stroke::new(1.0, p.edge));
        // 가운데 손잡이 금 + 펴는 방향 삼각형
        let c = r.center();
        pt.vline(c.x, egui::Rangef::new(c.y - 22.0, c.y - 8.0), Stroke::new(2.0, p.muted2));
        pt.vline(c.x, egui::Rangef::new(c.y + 8.0, c.y + 22.0), Stroke::new(2.0, p.muted2));
        let d = if left { 1.0 } else { -1.0 };
        let tri = vec![Pos2::new(c.x - 2.0 * d, c.y - 4.0), Pos2::new(c.x + 2.5 * d, c.y), Pos2::new(c.x - 2.0 * d, c.y + 4.0)];
        pt.add(egui::Shape::convex_polygon(tri, if resp.hovered() { p.text } else { p.muted }, Stroke::NONE));
        clicked = resp.clicked();
    });
    clicked
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
        // ---- 접힌 판 손잡이 (좁은 창 · 배율 150% 에서 처음부터 접혀 있을 때도 '여기 판이 있다' 가 보이게). 누르면 폄
        if !self.pro_side_visible() && collapsed_handle(ui, "pro-side-handle", true, &tr("큐시트 판 펴기")) {
            self.pro_toggle_side();
        }
        if !self.pro_inspector_visible() && collapsed_handle(ui, "pro-ins-handle", false, &tr("인스펙터 펴기")) {
            self.pro_toggle_inspector();
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

    /// 오른쪽 인스펙터: 머리 '인스펙터' + [글자 | 무대 | 미디어 | 소스], 그 아래 지금 다음(미리보기) 순서 이름
    fn pro_inspector(&mut self, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
        let p = pal();
        // 안의 칸이 판을 밀어 넓히지 않게 (egui 옆 판은 내용 폭을 따라 커진다)
        let w = ui.available_width();
        ui.set_max_width(w);
        ui.spacing_mut().item_spacing.y = 0.0;
        let id = Id::new("pro-ins-tab");
        // 점검용 --ins-tab sources|stage|media: 처음 고른 탭
        let first = match std::env::args().position(|x| x == "--ins-tab").and_then(|i| std::env::args().nth(i + 1)).as_deref() {
            Some("sources") => InsTab::Sources,
            Some("stage") => InsTab::Stage,
            Some("media") => InsTab::Media,
            _ => InsTab::Text,
        };
        let mut tab = ui.data(|d| d.get_temp::<InsTab>(id)).unwrap_or(first);
        widgets::mark("pro-ins-head", Rect::from_min_size(ui.cursor().min, Vec2::new(ui.available_width(), theme::HEAD_H)));
        pane_head(ui, &tr("인스펙터"), "", |ui| {
            let (a, b, c, d) = (tr("글자"), tr("무대"), tr("미디어"), tr("소스"));
            let r = widgets::segmented_with(ui, &mut tab, &[(InsTab::Text, a.as_str()), (InsTab::Stage, b.as_str()), (InsTab::Media, c.as_str()), (InsTab::Sources, d.as_str())], widgets::SegStyle::TINY);
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
                // 고른 순서가 없으면 아래 칸은 지금 템플릿 전체를 고친다 — 그것을 머리에 밝힌다
                None => {
                    let tpl = s.templates.iter().find(|x| x.0 == s.active_main).map(|x| x.1.clone()).unwrap_or_default();
                    (tr_args("템플릿 '{1}'", &[&tpl]), tr("고른 순서가 없어 템플릿 전체를 고칩니다 (이 템플릿을 쓰는 모든 순서)"))
                }
            };
            egui::Frame::new().inner_margin(Margin { left: 12, right: 12, top: 9, bottom: 8 }).show(ui, |ui| {
                ui.add(egui::Label::new(theme::ty(title, theme::Type::Title, p.text)).truncate().selectable(false));
                ui.add_space(2.0);
                let r = ui.add(egui::Label::new(theme::ty(sub.clone(), theme::Type::Small, p.muted)).truncate().selectable(false));
                if r.hovered() {
                    r.on_hover_text(sub);
                }
            });
            ui.painter().hline(ui.max_rect().x_range(), ui.cursor().top(), Stroke::new(1.0, p.line));
        }
        ui.spacing_mut().item_spacing.y = 4.0;
        match tab {
            InsTab::Text => self.quick_panel(ui, s, acts),
            InsTab::Stage => {
                egui::ScrollArea::vertical().id_salt("pro-stage").auto_shrink([false, false]).show(ui, |ui| {
                    // 판 안쪽 여백 12 (글자 탭과 같은 세로선)
                    egui::Frame::new().inner_margin(Margin { left: 12, right: 12, top: 8, bottom: 12 }).show(ui, |ui| self.workspace_stage(ui));
                });
            }
            InsTab::Sources => {
                egui::ScrollArea::vertical().id_salt("pro-sources").auto_shrink([false, false]).show(ui, |ui| self.sources_panel(ui));
            }
            InsTab::Media => {
                egui::ScrollArea::vertical().id_salt("pro-media").auto_shrink([false, false]).show(ui, |ui| {
                    self.media_panel(ui);
                });
            }
        }
    }

    /// 뷰어 머리 오른쪽 [둘 다 | 송출 중만] 구획 단추. 바꿨으면 새 값
    fn viewer_mode_buttons(&self, ui: &mut egui::Ui, live_only: bool) -> Option<bool> {
        let mut v = live_only;
        let key = crate::keys::label(self.core.lock().settings.get(), "toggleViewers", VIEWERS_KEY);
        let (a, b) = (tr("둘 다"), tr("송출 중만"));
        let (r, cells) = widgets::segmented_cells(ui, &mut v, &[(false, a.as_str()), (true, b.as_str())], widgets::SegStyle::TINY);
        widgets::mark("pro-viewer-mode", r.rect);
        widgets::mark("pro-viewer-both", cells[0]);
        widgets::mark("pro-viewer-live", cells[1]);
        let tip = if key.is_empty() { tr("뷰어 보기 방식: 다음 · 송출 중 둘 다, 또는 송출 중 하나만 (본문·편집 자리 넓히기)") } else { format!("{} ({key})", tr("뷰어 보기 방식: 다음 · 송출 중 둘 다, 또는 송출 중 하나만 (본문·편집 자리 넓히기)")) };
        let r = r.on_hover_text(tip);
        (r.changed() && v != live_only).then_some(v)
    }

    /// 가운데: 머리 '보기' · 뷰어 둘(다음 | 송출 중) · 끌어서 높이 · 본문·편집
    fn pro_center(&mut self, ui: &mut egui::Ui, s: &Snap, acts: &mut Vec<Act>) {
        let p = pal();
        ui.spacing_mut().item_spacing.y = 0.0;
        let full = ui.available_rect_before_wrap();
        // 뷰어 높이 비율 (설정 rsProLayout.viewers, 0.3~0.75). 편집칸 '크게' 면 본문이 판 전체
        let big = crate::app::editor_big(ui.ctx());
        // 바로 송출에서 '이번만 둘 다' 는 미리보기 모드로 돌아오면 지운다 (원래 고른 방식으로)
        if !self.core.lock().direct_mode() {
            self.viewer_direct_both = false;
        }
        if !big && self.pro_live_only() {
            self.pro_center_live_only(ui, s, full, acts);
            return;
        }
        let ratio = self.core.lock().settings.get()["rsProLayout"]["viewers"].as_f64().unwrap_or(if full.height() < 520.0 { 0.6 } else { 0.56 }).clamp(0.25, 0.8) as f32;
        let vh = if big { 0.0 } else { ((full.height() - 6.0) * ratio).clamp(180.0_f32.min(full.height() * 0.5), (full.height() - 160.0).max(180.0)) };
        if vh > 0.0 {
            let top = Rect::from_min_size(full.min, Vec2::new(full.width(), vh));
            crate::widgets_win::note_viewers(ui.ctx(), top); // 진행용 창이 이 자리를 피해 뜬다 (craft-windows)
            ui.scope_builder(UiBuilder::new().max_rect(top).id_salt("pro-viewers"), |ui| {
                ui.set_clip_rect(top.intersect(ui.clip_rect()));
                let mut pick = None;
                pane_head(ui, &tr("보기"), &tr("예배당 화면"), |ui| pick = self.viewer_mode_buttons(ui, false));
                if let Some(v) = pick {
                    self.pro_set_live_only(v);
                }
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
            ui.painter().rect_filled(div, 0.0, if resp.hovered() || resp.dragged() { theme::alpha(p.text2, 0.45) } else { p.edge });
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

/// 뷰어 보기 방식 단축키 기본값 (설정 shortcuts.toggleViewers)
pub(crate) const VIEWERS_KEY: &str = "Ctrl+Shift+L";

/// '송출 중만' 보기에서 송출 뷰어 칸의 폭 (가운데 판 폭 w 에서). 설정 rsProLayout.liveW 비율(0.25~0.6)을 쓰고, 없으면 36%
pub(crate) fn live_only_width(w: f32, saved: Option<f64>) -> f32 {
    let r = saved.map(|x| x.clamp(0.25, 0.6) as f32).unwrap_or(0.36);
    (w * r).clamp(260.0_f32.min(w * 0.5), 640.0_f32.max(w * 0.25)).min(w * 0.6)
}

impl App {
    /// '송출 중만': 송출 뷰어를 가운데 판 오른쪽 칸(위에서 아래까지)에 작게 두고, 본문·편집이 왼쪽 전체 높이를 쓴다.
    /// (2026-11-27 비교: '뷰어 줄 높이만 줄이기' 보다 1000x640 · 1600x960 · 1920x1080 모두 본문 목록이 더 많이 보였다 — docs/viewer-single.md)
    /// 미리보기 모드면 송출 뷰어 아래에 노랑 '다음' 줄(작은 그림 · ◀ n/m ▶ · 오른쪽 클릭 = 미리보기 메뉴)을 둔다.
    fn pro_center_live_only(&mut self, ui: &mut egui::Ui, s: &Snap, full: Rect, acts: &mut Vec<Act>) {
        let p = pal();
        let saved = self.core.lock().settings.get()["rsProLayout"]["liveW"].as_f64();
        let direct = self.core.lock().direct_mode();
        let vw = live_only_width(full.width(), saved);
        let right = Rect::from_min_max(Pos2::new(full.right() - vw, full.top()), full.max);
        crate::widgets_win::note_viewers(ui.ctx(), right);
        let div = Rect::from_min_max(Pos2::new(right.left() - 6.0, full.top()), Pos2::new(right.left(), full.bottom()));
        let left = Rect::from_min_max(full.min, Pos2::new(div.left(), full.bottom()));
        // ---- 오른쪽 칸: 머리 '보기' [둘 다 | 송출 중만] · 송출 뷰어 · (미리보기 모드) 노랑 '다음' 줄
        ui.scope_builder(UiBuilder::new().max_rect(right).id_salt("pro-viewers"), |ui| {
            ui.set_clip_rect(right.intersect(ui.clip_rect()));
            let mut pick = None;
            pane_head(ui, &tr("보기"), "", |ui| pick = self.viewer_mode_buttons(ui, true));
            if let Some(v) = pick {
                self.pro_set_live_only(v);
            }
            widgets::mark("pro-view-head", Rect::from_min_size(right.min, Vec2::new(right.width(), theme::HEAD_H)));
            let body = Rect::from_min_max(Pos2::new(right.left(), right.top() + theme::HEAD_H), right.max);
            ui.painter().rect_filled(body, 0.0, theme::mix(p.panel, 80.0, p.bg));
            ui.scope_builder(UiBuilder::new().max_rect(body).id_salt(("pro-viewer", true)), |ui| {
                ui.set_clip_rect(body.intersect(ui.clip_rect()));
                egui::ScrollArea::vertical().id_salt(("pro-viewer-scroll", true)).auto_shrink([false, false]).show(ui, |ui| {
                    // 뷰어 높이는 칸 폭이 정한다 (16:9) — 칸 높이 전체를 주면 화면이 아래로 늘지 않게 deck_view 가 폭에 맞춘다
                    self.deck_view(ui, &tr("LIVE"), s.live.as_ref().map(|l| &l.deck), true, s.live.as_ref(), s, acts);
                    if !direct {
                        self.next_strip(ui, s, acts);
                    }
                });
            });
        });
        // ---- 끌어서 폭 바꾸기 (6px 세로 띠, 설정 rsProLayout.liveW)
        let resp = ui.interact(div, Id::new("pro-live-split"), Sense::drag());
        widgets::mark("pro-live-split", div);
        ui.painter().rect_filled(div, 0.0, if resp.hovered() || resp.dragged() { theme::alpha(p.text2, 0.45) } else { p.edge });
        if resp.hovered() || resp.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
        if resp.dragged() {
            if let Some(pos) = resp.interact_pointer_pos() {
                let r = ((full.right() - pos.x) / full.width().max(1.0)).clamp(0.25, 0.6);
                self.core.lock().settings.update(|d| {
                    if !d["rsProLayout"].is_object() {
                        d["rsProLayout"] = json!({});
                    }
                    d["rsProLayout"]["liveW"] = json!((r * 100.0).round() / 100.0);
                });
            }
        }
        // ---- 왼쪽: 본문·편집 (위에서 아래까지)
        ui.scope_builder(UiBuilder::new().max_rect(left).id_salt("pro-main"), |ui| {
            ui.set_clip_rect(left.intersect(ui.clip_rect()));
            ui.spacing_mut().item_spacing.y = 4.0;
            self.main_panel(ui, s, acts);
        });
        ui.allocate_rect(full, Sense::hover());
    }
}
