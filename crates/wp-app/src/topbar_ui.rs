//! 머리 줄 · 아래 송출 줄 · 상태 (Electron index.html `header.top` · `footer.bottom` · control.js renderStatus).
//! app.rs 의 자식 모듈이라 `Snap` 의 private 필드를 그대로 읽는다.
//!
//! - 머리 줄: 로고 + 워십프리젠터 · 넓은 검색칸(돋보기, 둥근 테두리, 포커스 때 accent 테두리 + 번짐) ·
//!   검색 방식 | 번역본 | 병행 번역 묶음(.hgroup) · '미리보기 켜짐 / 미리보기 끔 · 바로 송출'(.studio) ·
//!   작업공간 고르기(.ws-pick, 배치 아이콘) · 설정 톱니(.icon-btn)
//! - 송출 줄: 이전 F5 · 다음 F6 · 송출 F9 · [글자 숨김 F10 · 지우기 ▾ · 모두 지우기] · 즉석 자막 Ctrl+T ·
//!   예배당 화면: 꺼짐 · 켜기 / 켜짐 · 끄기 Ctrl+Shift+F | 오른쪽 출력 연결 상태 점
//! - 알림(.msg): 오른쪽 아래(송출 줄 위)에 뜨는 상자
use super::{Act, App, Snap};
use crate::icons;
use crate::menubar;
use crate::theme::{self, Weight, alpha, pal};
use crate::widgets::{self, Btn, Kind, Size};
use eframe::egui::{self, Align, Align2, Color32, Id, Key, Layout, Pos2, Rect, Response, RichText, Sense, Shape, Stroke, StrokeKind, Ui, Vec2};
use serde_json::json;
use wp_core::i18n::tr;
use wp_core::workspace::Workspace;

/// index.html .brand-mark (64×64 SVG 그대로)
const BRAND_SVG: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64"><defs><linearGradient id="bg1" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#5a93ff"/><stop offset=".55" stop-color="#3566e0"/><stop offset="1" stop-color="#1e3f9e"/></linearGradient><linearGradient id="bg2" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#ffd25a"/><stop offset="1" stop-color="#f0a21a"/></linearGradient></defs><rect x="2" y="2" width="60" height="60" rx="14" fill="url(#bg1)"/><rect x="12.5" y="16.4" width="25" height="5.2" rx="2.6" fill="#fff"/><rect x="12.5" y="29.4" width="25" height="5.2" rx="2.6" fill="#fff" opacity=".62"/><rect x="12.5" y="42.4" width="25" height="5.2" rx="2.6" fill="#fff" opacity=".36"/><path d="M42 26.5 Q42 24.5 43.8 25.5 L52 30.6 Q53.6 31.6 52 32.6 L43.8 37.7 Q42 38.7 42 36.7 Z" fill="url(#bg2)"/></svg>"##;

/// 머리 줄 높이 (control.css .top: 안쪽 8px 12px, 검색칸 40px)
pub(crate) const TOP_MARGIN: egui::Margin = egui::Margin { left: 12, right: 12, top: 8, bottom: 8 };
/// 송출 줄 안쪽 (.bottom: 8px 12px)
pub(crate) const BOTTOM_MARGIN: egui::Margin = egui::Margin { left: 12, right: 12, top: 8, bottom: 8 };

fn text_w(ui: &Ui, s: &str, size: f32, w: Weight) -> f32 {
    ui.painter().layout_no_wrap(s.to_string(), theme::font(size, w), Color32::WHITE).size().x
}

/// select 모양 (hgroup 안: 테두리 없음, 글 14px, 오른쪽 작은 ▾ — control.css select 배경 그라데이션 화살표)
fn select_at(ui: &mut Ui, rect: Rect, id: Id, text: &str, tip: &str) -> Response {
    let p = pal();
    let resp = ui.interact(rect, id, Sense::click()).on_hover_text(tip);
    let pt = ui.painter_at(rect.shrink2(Vec2::new(0.0, 1.0)));
    pt.text(Pos2::new(rect.left() + 8.0, rect.center().y), Align2::LEFT_CENTER, text, theme::font(14.0, Weight::Regular), p.text);
    // 4px 45° 두 삼각형이 만드는 작은 ▾ (오른쪽에서 13px~5px)
    let (cx, cy) = (rect.right() - 9.0, rect.center().y + 1.0);
    pt.add(Shape::convex_polygon(vec![Pos2::new(cx - 4.0, cy - 2.0), Pos2::new(cx + 4.0, cy - 2.0), Pos2::new(cx, cy + 2.0)], p.muted, Stroke::NONE));
    if resp.has_focus() {
        pt.rect_stroke(rect.shrink(2.0), crate::theme::cr(crate::theme::Rad::Sm), Stroke::new(2.0, p.accent), StrokeKind::Inside);
    }
    resp
}

/// select 의 펼친 목록 (메뉴 줄과 같은 .mb-drop 모양)
fn select_popup(resp: &Response, min_w: f32, content: impl FnOnce(&mut Ui)) {
    egui::Popup::menu(resp).frame(menubar::drop_frame()).gap(2.0).show(|ui| {
        ui.set_min_width(min_w);
        ui.spacing_mut().item_spacing.y = 0.0;
        content(ui);
    });
}

/// 묶음 틀 (.hgroup: sunken 바탕 · line 테두리(hover line2) · 둥글기 6 · 높이 34)
fn hgroup_frame(ui: &Ui, rect: Rect, idx: egui::layers::ShapeIdx) {
    let p = pal();
    let hov = ui.rect_contains_pointer(rect);
    ui.painter().set(
        idx,
        Shape::Vec(vec![
            Shape::rect_filled(rect, crate::theme::cr(crate::theme::Rad::Md), p.sunken),
            Shape::rect_stroke(rect, crate::theme::cr(crate::theme::Rad::Md), Stroke::new(p.border_w, if hov { p.line2 } else { p.line }), StrokeKind::Inside),
        ]),
    );
}

impl App {
    /// 머리 줄 (header.top)
    pub(super) fn top_bar(&mut self, ui: &mut Ui, s: &Snap, acts: &mut Vec<Act>) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0; // .top { gap: 10px }
            ui.set_min_height(40.0);
            brand(ui);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                // 설정 톱니 (.icon-btn 34×34)
                if ui.add(Btn::icon_only("settings").tooltip(tr("설정 (Ctrl+,)"))).clicked() {
                    self.tc.settings.open = true;
                }
                self.workspace_picker(ui);
                // 클릭하면: [미리보기] [바로 송출] — 상태를 이름으로 쓴 토글 대신 고르는 분할 단추 (평가 7번).
                // 원본 .studio 색: 미리보기 = pv, 바로 송출 = warn
                if let Some(on) = studio_split(ui, s.preview_mode) {
                    if on != s.preview_mode {
                        acts.push(Act::TogglePreviewMode);
                    }
                }
                self.search_group(ui, s, acts);
                self.search_bar(ui, s, acts);
            });
        });
    }

    /// 검색 방식 | 번역본 | 병행 번역 (.hgroup, 오른쪽→왼쪽 배치 안에서 한 덩어리로)
    fn search_group(&mut self, ui: &mut Ui, s: &Snap, acts: &mut Vec<Act>) {
        let (mode, parallel) = {
            let c = self.core.lock();
            (c.settings.str(&["searchMode"]), c.settings.str(&["parallelVersionId"]))
        };
        let modes = [("auto", tr("자동")), ("reference", tr("장절")), ("text", tr("본문"))];
        let mode_label = modes.iter().find(|m| m.0 == mode).map(|m| m.1.clone()).unwrap_or_else(|| modes[0].1.clone());
        let vname = self.versions.iter().find(|v| v.id == s.version).map(|v| v.name.clone()).unwrap_or_default();
        let none = tr("병행: 없음");
        let pname = self.versions.iter().find(|v| v.id == parallel).map(|v| format!("+ {}", v.name)).unwrap_or_else(|| none.clone());
        // select 너비 = 가장 긴 선택지 + 왼쪽 8 + 오른쪽 24 (css select padding-right 24)
        let pad = 8.0 + 24.0;
        let w_mode = modes.iter().map(|m| text_w(ui, &m.1, 14.0, Weight::Regular)).fold(0.0, f32::max) + pad;
        let w_ver = self.versions.iter().map(|v| text_w(ui, &v.name, 14.0, Weight::Regular)).fold(text_w(ui, &vname, 14.0, Weight::Regular), f32::max).min(160.0) + pad;
        let w_par = self.versions.iter().map(|v| text_w(ui, &format!("+ {}", v.name), 14.0, Weight::Regular)).fold(text_w(ui, &none, 14.0, Weight::Regular), f32::max).min(170.0) + pad;
        let total = w_mode + w_ver + w_par + 2.0;
        let (rect, _) = ui.allocate_exact_size(Vec2::new(total, theme::CTL_H), Sense::hover());
        let bg = ui.painter().add(Shape::Noop);
        hgroup_frame(ui, rect, bg);
        let p = pal();
        let r_mode = Rect::from_min_size(rect.min, Vec2::new(w_mode, rect.height()));
        let r_ver = Rect::from_min_size(Pos2::new(r_mode.right() + 1.0, rect.top()), Vec2::new(w_ver, rect.height()));
        let r_par = Rect::from_min_max(Pos2::new(r_ver.right() + 1.0, rect.top()), rect.max);
        // select + select { border-left: 1px line }
        for x in [r_mode.right() + 0.5, r_ver.right() + 0.5] {
            ui.painter().vline(x, rect.y_range().shrink(1.0), Stroke::new(1.0, p.line));
        }
        let rm = select_at(ui, r_mode, Id::new("hdr-mode"), &mode_label, &tr("기본 검색 방식 (요3:16 → 장절, 사랑 → 본문)"));
        select_popup(&rm, w_mode, |ui| {
            for (k, l) in &modes {
                if menubar::item_row(ui, None, l, "", Some(*k == mode)).clicked() {
                    self.core.lock().settings.update(|d| d["searchMode"] = json!(k));
                    if !self.search.trim().is_empty() {
                        acts.push(Act::SearchList(self.search.clone()));
                    }
                    ui.close();
                }
            }
        });
        let rv = select_at(ui, r_ver, Id::new("hdr-version"), &vname, &tr("성경 번역본"));
        select_popup(&rv, w_ver, |ui| {
            for v in &self.versions {
                if menubar::item_row(ui, None, &v.name, "", Some(v.id == s.version)).clicked() {
                    acts.push(Act::SetVersion(v.id.clone()));
                    ui.close();
                }
            }
        });
        let rp = select_at(ui, r_par, Id::new("hdr-parallel"), &pname, &tr("병행 번역본: 두 번역을 함께 송출 (예: 개역개정 + NIV)"));
        let mut pick: Option<String> = None;
        select_popup(&rp, w_par, |ui| {
            if menubar::item_row(ui, None, &none, "", Some(parallel.is_empty())).clicked() {
                pick = Some(String::new());
                ui.close();
            }
            for v in &self.versions {
                if menubar::item_row(ui, None, &format!("+ {}", v.name), "", Some(v.id == parallel)).clicked() {
                    pick = Some(v.id.clone());
                    ui.close();
                }
            }
        });
        if let Some(id) = pick {
            self.core.lock().settings.update(|d| d["parallelVersionId"] = json!(id));
            if !self.search.trim().is_empty() {
                acts.push(Act::Search(self.search.clone()));
            }
        }
    }

    /// 넓은 검색칸 (#q: 높이 40 · 17px · 왼쪽 38px 돋보기 · 둥글기 8 · 포커스 때 accent 테두리 + 3px 번짐)
    pub(crate) fn search_bar(&mut self, ui: &mut Ui, s: &Snap, acts: &mut Vec<Act>) {
        let w = ui.available_width().max(160.0);
        self.search_box(ui, s, acts, w, 40.0, 17.0, &tr("검색 — 요3:16 · 사랑 · 31장 · 찬양 제목 · 이미지 이름   (Ctrl+K)"));
    }

    /// 찾기칸 (크기를 받아서). (나) 컨트롤 막대는 높이 26 · 글자 12.5
    pub(crate) fn search_box(&mut self, ui: &mut Ui, s: &Snap, acts: &mut Vec<Act>, w: f32, h: f32, fs: f32, hint_text: &str) {
        let p = pal();
        let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
        let bg = ui.painter().add(Shape::Noop);
        let id = Id::new("search-box");
        let small = h < 32.0;
        let icon_x = if small { 8.0 } else { 12.0 };
        let icon_s = if small { 13.0 } else { 18.0 };
        let text_rect = Rect::from_min_max(Pos2::new(rect.left() + icon_x + icon_s + 7.0, rect.top() + 1.0), Pos2::new(rect.right() - if small { 6.0 } else { 12.0 }, rect.bottom() - 1.0));
        let hint = RichText::new(hint_text).color(p.muted2).font(theme::font(fs, Weight::Regular));
        let r = ui.put(
            text_rect,
            egui::TextEdit::singleline(&mut self.search)
                .id(id)
                .frame(egui::Frame::NONE)
                .margin(egui::Margin::ZERO)
                .hint_text(hint)
                .font(theme::font(fs, Weight::Regular))
                .text_color(p.text)
                .vertical_align(Align::Center)
                // Esc 를 egui 의 '초점 놓기' 로 넘기지 않고 이 칸이 받는다 (아래: 내용 모두 고르기, control.js:1077)
                .event_filter(egui::EventFilter { horizontal_arrows: true, vertical_arrows: true, tab: false, escape: true })
                .desired_width(text_rect.width()),
        );
        // 검색칸 Esc = 내용 모두 고르기 (control.js #q keydown: `else $('#q').select()`).
        // 글 쓰는 중이라 전역 Esc(보조 화면 끄기)는 shortcuts 에서 이미 빠졌고, 여기서 눌림을 먹어 뒤의 창·목록도 받지 않는다
        if r.has_focus() && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) {
            if let Some(mut st) = egui::TextEdit::load_state(ui.ctx(), id) {
                let n = self.search.chars().count();
                st.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(n))));
                st.store(ui.ctx(), id);
            }
        }
        let focused = r.has_focus();
        let hov = ui.rect_contains_pointer(rect);
        let mut shapes = vec![];
        let rad = if small { crate::theme::cr(crate::theme::Rad::Md) } else { crate::theme::cr(crate::theme::Rad::Lg) };
        if focused {
            // box-shadow: 0 0 0 3px accent 25%
            shapes.push(Shape::rect_stroke(rect, rad, Stroke::new(3.0, alpha(p.accent, 0.25)), StrokeKind::Outside));
        }
        shapes.push(Shape::rect_filled(rect, rad, p.sunken));
        if p.bevel && !focused {
            // 입력칸: 안쪽 위 1px 어두운 선 (box-shadow: inset 0 1px 1px #000)
            shapes.push(Shape::hline(rect.x_range().shrink(3.0), rect.top() + 0.5, Stroke::new(1.0, Color32::from_black_alpha(200))));
            if hov {
                shapes.push(Shape::rect_stroke(rect, rad, Stroke::new(1.0, p.line2), StrokeKind::Inside));
            }
        } else {
            shapes.push(Shape::rect_stroke(rect, rad, Stroke::new(p.border_w, if focused { p.accent } else if hov { p.line2 } else { p.line }), StrokeKind::Inside));
        }
        ui.painter().set(bg, Shape::Vec(shapes));
        icons::paint(ui, Rect::from_center_size(Pos2::new(rect.left() + icon_x + icon_s / 2.0, rect.center().y), Vec2::splat(icon_s)), "search", p.muted);
        if ui.rect_contains_pointer(rect) && !r.hovered() && ui.input(|i| i.pointer.primary_clicked()) {
            r.request_focus(); // 돋보기·여백을 눌러도 칸으로
        }
        // 예전 검색 줄의 '송출 · 큐시트에 넣기' 단추 → 원본처럼 오른쪽 클릭 메뉴로 (ctxmenu.js '이 본문을 큐시트에 넣기')
        let is_ref = self.searched_bible_item().is_some();
        r.context_menu(|ui| {
            let b = ui.add_enabled(is_ref, egui::Button::new(tr("이 본문을 큐시트에 넣기 (고른 순서 다음)"))).on_disabled_hover_text(tr("장절(예: 요3:16)을 먼저 검색하세요"));
            if b.clicked() {
                self.add_searched_bible(acts);
                ui.close();
            }
            if ui.button(tr("송출 (Ctrl+Enter)")).clicked() {
                acts.push(Act::SendNow(self.search.clone()));
                ui.close();
            }
        });
        if self.focus_search {
            r.request_focus();
            // 패널 첫 크기 재기(sizing pass)에서는 포커스가 안 잡히므로 실제로 그린 뒤에 끈다
            if !ui.is_sizing_pass() && ui.is_visible() {
                self.focus_search = false;
            }
        }
        // 입력 중에는 목록만 (control.js #q input → 후보 목록). 미리보기·LIVE 는 Enter 때만 바뀐다 —
        // 바로 송출 모드에서 '요3:1' 처럼 치는 중인 글이 예배당에 나가면 안 된다
        if r.changed() {
            acts.push(Act::SearchList(self.search.clone()));
        }
        // 검색칸에서 ↓: 키보드를 본문 목록으로 (control.js: #results.focus(), 고른 줄이 없으면 첫 줄)
        if r.has_focus() && self.result["ok"] == json!(true) && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::ArrowDown)) {
            r.surrender_focus();
            crate::center_ui::focus_results(ui.ctx());
        }
        // Enter = 검색 (control.js doSearch: 장절이면 미리보기, 바로 송출 모드면 송출). Ctrl+Enter 는 전역 sendNow 가 먼저 받는다
        if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter) && !i.modifiers.ctrl) && !self.search.trim().is_empty() {
            acts.push(Act::Search(self.search.clone()));
        }
        let _ = s;
    }

    /// 작업공간 고르기 (.ws-pick.hgroup: 배치 아이콘 + select). 펼치면 작업공간 목록 · 저장·관리·패널 · 배치 잠금
    fn workspace_picker(&mut self, ui: &mut Ui) {
        let p = pal();
        let st = &self.workspace.state;
        let current = st
            .saved
            .iter()
            .find(|w| w.id == st.active)
            .map(|w| w.name.clone())
            .or_else(|| Workspace::BUILTINS.iter().find(|(id, _)| *id == st.active).map(|(_, n)| tr(n)))
            .unwrap_or_else(|| tr("기본"));
        let sel_w = 145.0; // .ws-pick select { max-width: 150px }
        let (rect, _) = ui.allocate_exact_size(Vec2::new(8.0 + 16.0 + 2.0 + sel_w, theme::CTL_H), Sense::hover());
        let bg = ui.painter().add(Shape::Noop);
        hgroup_frame(ui, rect, bg);
        icons::paint(ui, Rect::from_center_size(Pos2::new(rect.left() + 8.0 + 8.0, rect.center().y), Vec2::splat(16.0)), "layout-panel-left", p.muted);
        let srect = Rect::from_min_max(Pos2::new(rect.right() - sel_w, rect.top()), rect.max);
        let resp = select_at(ui, srect, Id::new("hdr-workspace"), &current, &tr("작업공간: 상황별 패널 배치 (Ctrl+Alt+1~9)"));
        let mut pick = None;
        let mut manage = false;
        let mut lock = None;
        select_popup(&resp, 220.0, |ui| {
            let st = &self.workspace.state;
            for (id, name) in Workspace::BUILTINS {
                if menubar::item_row(ui, None, &tr(name), "", Some(st.active == id)).clicked() {
                    pick = Some(id.to_string());
                    ui.close();
                }
            }
            for w in &st.saved {
                if menubar::item_row(ui, None, &w.name, "", Some(st.active == w.id)).clicked() {
                    pick = Some(w.id.clone());
                    ui.close();
                }
            }
            menubar::sep(ui);
            if menubar::item_row(ui, Some("columns-2"), &tr("저장 · 관리 · 패널"), "", None).clicked() {
                manage = true;
                ui.close();
            }
            let locked = st.locked;
            if menubar::item_row(ui, Some("lock"), &tr("배치 잠금"), "", if locked { Some(true) } else { None }).clicked() {
                lock = Some(!locked);
            }
        });
        if let Some(id) = pick {
            self.workspace.state.select(&id)
        }
        if manage {
            self.workspace.open = true
        }
        if let Some(l) = lock {
            self.workspace.state.locked = l
        }
    }

    /// 아래 송출 줄 (footer.bottom) — 창 맨 아래 전체 폭 고정
    pub(super) fn bottom_bar(&mut self, ui: &mut Ui, s: &Snap, acts: &mut Vec<Act>) {
        let shortcut_settings = serde_json::json!({"shortcuts": self.core.lock().settings.get()["shortcuts"].clone()});
        self.video_bar(ui); // [트랙 B] 영상·음악 조작 줄 (재생 중일 때만 내용이 생김)
        // [갈래 C] 아래 줄 빈 자리 오른쪽 클릭 = 화면 지우기 메뉴 (ctxmenu.js footer.bottom) — 단추보다 먼저 놓는다
        let bar = ui.interact(ui.available_rect_before_wrap(), ui.id().with("footer-menu"), egui::Sense::click());
        crate::ctx_ui::footer_menu(self, &bar, acts);
        ui.spacing_mut().interact_size.y = 48.0; // 가로 줄 처음 높이 → 단추들이 묶음 높이 가운데에 놓인다
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0; // .bottom { gap: 10px }
            ui.spacing_mut().interact_size.y = theme::CTL_H; // 줄 높이(48)는 이미 잡혔고, 안쪽 묶음은 원래 값으로
            self.workspace_controls(ui, s, acts);
            // 즉석 자막 (.quick-btn: 34px, 열려 있으면 accent 테두리·글자)
            let qon = self.tb.quick;
            let mut q = Btn::new(tr("즉석 자막")).icon("text").kbd(crate::keys::label(&shortcut_settings, "quickText", "Ctrl+T")).tooltip(tr("즉석 자막: 글을 써서 바로 송출"));
            if qon {
                q = q.text_color(pal().accent).icon_color(pal().accent).selected(true);
            }
            if egui::Frame::new().inner_margin(egui::Margin::symmetric(0, 7)).show(ui, |ui| ui.add(q)).inner.clicked() {
                self.tb.quick = !qon;
            }
            // 예배당 화면: 꺼짐/켜짐 + 누르면 할 일 (.big.output-toggle, 켜짐 = pv 바탕·테두리)
            // 원본은 '보조 화면 꺼짐' 이지만 상태 줄('예배당 화면 꺼짐')과 이름을 하나로 맞춘다.
            let running = self.out.running();
            let b = Btn::new(if running { tr("예배당 화면: 켜짐 · 끄기") } else { tr("예배당 화면: 꺼짐 · 켜기") })
                .kbd(crate::keys::label(&shortcut_settings, "toggleOutputWindow", "Ctrl+Shift+F"))
                .size(Size::Big)
                .kind(if running { Kind::Pv } else { Kind::Default })
                .tooltip(tr("예배당 모니터의 전체 화면 출력을 켜고 끕니다"));
            if egui::Frame::new().inner_margin(egui::Margin::symmetric(0, 4)).show(ui, |ui| ui.add(b)).inner.clicked() {
                acts.push(Act::OutputToggle);
            }
            widgets::vsep(ui, 24.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 14.0; // .status { gap: 14px }
                // 중요한 순서(오류 > 예배당 화면 > OBS 전체 > OBS 자막 > 출력 창 > OBS 원격)로 자리를 주고,
                // 모자라면 글은 빼고 점만(툴팁에 전체 문구), 그래도 모자라면 뒤 항목부터 뺀다.
                // 오른쪽→왼쪽 배치라 거꾸로 그려서 화면에는 중요한 것이 왼쪽(송출 줄 쪽)에 온다.
                let items = self.status_items(s);
                let widths: Vec<f32> = items.iter().map(|x| text_w(ui, &x.1, 12.0, Weight::Regular)).collect();
                let fit = widgets::fit_status(&widths, ui.available_width() - 4.0, 14.0);
                let mut open = None;
                for ((col, text, key), f) in items.iter().zip(&fit).rev() {
                    let Some(full) = f else { continue };
                    let tip = match *key {
                        "obs-remote" => tr("누르면 OBS 원격 조작 창"),
                        _ => tr("누르면 출력 설정 (예배당 화면 · OBS 주소)"),
                    };
                    let r = widgets::status_btn(ui, *col, text, !*full, &tip);
                    widgets::mark(&format!("status-{key}"), r.rect);
                    if r.clicked() {
                        open = Some(*key);
                    }
                }
                match open {
                    Some("obs-remote") => crate::trackd_ui::open(crate::trackd_ui::Win::Obs),
                    Some(_) => crate::trackd_ui::open(crate::trackd_ui::Win::Output),
                    None => {}
                }
            });
        });
    }

    /// 출력 연결 상태 (control.js renderStatus 와 같은 문구·색)
    /// (점 색, 글, 항목 이름) — 중요한 것부터
    pub(crate) fn status_items(&mut self, s: &Snap) -> Vec<(Color32, String, &'static str)> {
        let p = pal();
        let mut v = vec![];
        let running = self.out.running();
        if !self.out.error.is_empty() {
            // Rust 판 전용(예배당 창 실행 오류) — 길면 줄여서 송출 단추를 덮지 않게
            let e: String = self.out.error.chars().take(28).collect();
            v.push((p.live, if e.len() < self.out.error.len() { format!("{e}…") } else { e }, "error"));
        }
        // 예배당 화면 (꺼짐 = muted2 · 켜짐 = pv · 모니터 없음 = live)
        let mon = self.out.monitors.iter().find(|m| m.index == self.monitor).map(|m| format!("{} {}×{}", m.name, m.size.0, m.size.1));
        let (c, t) = match (running, mon) {
            (false, _) => (p.muted2, tr("예배당 화면 꺼짐")),
            (true, Some(m)) => (p.pv, wp_core::i18n::tr_args("예배당 화면: {1}", &[&m])),
            (true, None) => (p.live, tr("예배당 모니터 연결 안 됨")),
        };
        v.push((c, t, "output"));
        // OBS 전체 · OBS 자막 (브라우저 소스, 창 제외) — ack 버전이 송출 버전과 같으면 동기화됨
        let (version, acks): (u64, Vec<(String, String, Option<u64>)>) = {
            let c = self.core.lock();
            (c.pres.version, c.outputs.values().map(|o| (o.client.clone(), o.kind.clone(), o.ack.as_ref().and_then(|a| a["version"].as_u64()))).collect())
        };
        let _ = &s.outputs;
        for (out, name) in [("main", "OBS 전체"), ("overlay", "OBS 자막")] {
            let cs: Vec<_> = acks.iter().filter(|c| c.0 == out && c.1 != "window").collect();
            let name = tr(name);
            if cs.is_empty() {
                v.push((p.muted2, wp_core::i18n::tr_args("{1} 연결 없음", &[&name]), out));
                continue;
            }
            let synced = cs.iter().all(|c| c.2 == Some(version));
            v.push((if synced { p.pv } else { p.warn }, wp_core::i18n::tr_args(if synced { "{1} {2}개 동기화됨" } else { "{1} {2}개 갱신 대기" }, &[&name, &cs.len()]), out));
        }
        let win: Vec<_> = acks.iter().filter(|c| c.1 == "window" && c.0 == "main").collect();
        if !win.is_empty() && running {
            let synced = win.iter().all(|c| c.2 == Some(version));
            v.push((if synced { p.pv } else { p.warn }, if synced { tr("예배당 화면 반영됨") } else { tr("예배당 화면 반영 대기") }, "window"));
        }
        // [트랙 D] OBS 원격 조작 상태 (설정에서 켰을 때만)
        if let Some((t, warn)) = crate::trackd_ui::obs_status_line(self.core.lock().settings.get()) {
            v.push((if warn { p.live } else { p.pv }, t, "obs-remote"));
        }
        v
    }

    /// 알림 상자 (.msg: 오른쪽 16px · 아래 72px, 600, 둥글기 8, 종류별 바탕).
    /// 누를 수 있다: 글을 누르면 닫히고, 단추(되돌리기 · 예배당 화면 켜기)가 붙으면 그 동작을 돌려준다
    pub(super) fn toast(&self, ctx: &egui::Context, s: &Snap) -> Option<Act> {
        let (msg, kind, at) = s.toast.as_ref()?;
        // 단추가 있는 알림은 누를 시간을 더 준다
        let life = if s.toast_act.is_some() { 12000 } else { 8000 };
        if wp_core::util::now_ms() - at >= life {
            return None;
        }
        let mut out = None;
        let p = pal();
        let fill = match *kind {
            "error" => p.live_bg,
            "warn" => p.warn_bg,
            "ok" => p.pv_bg,
            _ => p.accent_bg,
        };
        egui::Area::new(Id::new("wp-toast"))
            .order(egui::Order::Foreground)
            // (나) 아래 시간표·상태 줄 위에 뜨게 (prolayout 이 지난 프레임에 잰 아래쪽 높이)
            .anchor(Align2::RIGHT_BOTTOM, Vec2::new(-16.0, -(ctx.data(|d| d.get_temp::<f32>(Id::new("pro-bottom-h"))).unwrap_or(60.0) + 12.0)))
            .interactable(true)
            .show(ctx, |ui| {
                let fr = egui::Frame::new()
                    .fill(fill)
                    .stroke(Stroke::new(1.0, alpha(Color32::WHITE, 0.12)))
                    .corner_radius(crate::theme::cr(crate::theme::Rad::Lg))
                    .inner_margin(egui::Margin::symmetric(16, 11))
                    .shadow(egui::Shadow { offset: [0, 10], blur: 30, spread: 0, color: alpha(Color32::BLACK, 0.55) })
                    .show(ui, |ui| {
                        ui.set_max_width(440.0);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 12.0;
                            let lr = ui.add(egui::Label::new(theme::rich(wp_core::i18n::tr_dyn(msg), 14.0, Weight::SemiBold, p.text)).wrap().selectable(false).sense(Sense::click())).on_hover_text(tr("눌러서 닫기"));
                            if lr.clicked() {
                                out = Some(Act::ToastDismiss);
                            }
                            if let Some(a) = s.toast_act {
                                let (label, icon) = toast_button(a);
                                let br = ui.add(Btn::new(label).icon(icon).size(Size::Sm).height(28.0));
                                widgets::mark("toast-act", br.rect);
                                if br.clicked() {
                                    out = Some(Act::ToastAct(a));
                                }
                            }
                        });
                    });
                let _ = fr;
            });
        out
    }
}

impl App {
    /// LIVE 패널 머리 아래 주황 띠: 예배당 화면이 꺼진 채 송출 중이면 "회중에게 보이지 않습니다 [지금 켜기]".
    /// (원본은 상태 줄의 작은 점뿐이라 예배 시작 직후 놓치기 쉬움 — 평가 1번)
    pub(super) fn output_off_band(&mut self, ui: &mut Ui, has_live: bool, acts: &mut Vec<Act>) {
        if self.out.running() || !has_live {
            return;
        }
        let p = pal();
        ui.add_space(6.0);
        egui::Frame::new().fill(p.warn_bg).stroke(Stroke::new(1.0, p.warn)).corner_radius(crate::theme::cr(crate::theme::Rad::Md)).inner_margin(egui::Margin::symmetric(10, 4)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                icons::show(ui, "monitor-off", 16.0, p.warn);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let b = ui.add(Btn::new(tr("지금 켜기")).icon("monitor").kind(Kind::Warn).height(28.0).kbd(crate::keys::label(self.core.lock().settings.get(), "toggleOutputWindow", "Ctrl+Shift+F")).tooltip(tr("예배당 모니터에 전체 화면 출력을 켭니다")));
                    widgets::mark("live-output-on", b.rect);
                    if b.clicked() {
                        acts.push(Act::OutputToggle);
                    }
                    ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                        ui.add(egui::Label::new(theme::rich(tr("예배당 화면이 꺼져 있어 회중에게 보이지 않습니다"), 13.0, Weight::SemiBold, p.text)).truncate().selectable(false));
                    });
                });
            });
        });
    }

    /// (나) 송출 중 뷰어 화면 아래쪽에 겹치는 띠: "예배당 화면 꺼짐 — 회중에게 안 보임 [지금 켜기]"
    pub(super) fn output_off_overlay(&mut self, ui: &mut Ui, screen: Rect, has_live: bool, acts: &mut Vec<Act>) {
        if self.out.running() || !has_live {
            return;
        }
        let p = pal();
        let h = 30.0_f32.min(screen.height() * 0.4);
        let band = Rect::from_min_max(Pos2::new(screen.left() + 3.0, screen.bottom() - 3.0 - h), Pos2::new(screen.right() - 3.0, screen.bottom() - 3.0));
        ui.painter().rect_filled(band, theme::cr(theme::Rad::Sm), alpha(theme::mix(p.warn, 35.0, Color32::BLACK), 0.92));
        let key = crate::keys::label(self.core.lock().settings.get(), "toggleOutputWindow", "Ctrl+Shift+F");
        let bw = 82.0_f32.min(band.width() * 0.45);
        let br = Rect::from_min_size(Pos2::new(band.right() - bw - 4.0, band.top() + 4.0), Vec2::new(bw, h - 8.0));
        let r = ui.interact(br, Id::new("live-output-on"), Sense::click()).on_hover_text(format!("{} ({key})", tr("예배당 모니터에 전체 화면 출력을 켭니다")));
        ui.painter().rect_filled(br, theme::cr(theme::Rad::Sm), if r.hovered() { Color32::WHITE } else { p.send_bg });
        ui.painter().with_clip_rect(br).text(br.center(), Align2::CENTER_CENTER, tr("지금 켜기"), theme::font(11.5, Weight::Bold), p.send_fg);
        widgets::mark("live-output-on", br);
        if r.clicked() {
            acts.push(Act::OutputToggle);
        }
        let tr_ = Rect::from_min_max(Pos2::new(band.left() + 8.0, band.top()), Pos2::new(br.left() - 6.0, band.bottom()));
        icons::paint(ui, Rect::from_center_size(Pos2::new(tr_.left() + 7.0, band.center().y), Vec2::splat(14.0)), "monitor-off", Color32::WHITE);
        ui.painter().with_clip_rect(tr_).text(Pos2::new(tr_.left() + 20.0, band.center().y), Align2::LEFT_CENTER, tr("예배당 화면 꺼짐 — 회중에게 안 보임"), theme::font(11.5, Weight::SemiBold), Color32::WHITE);
    }

    /// 송출했는데 예배당 화면이 꺼져 있고 자동으로 켜지도 않을 때, 꺼진 동안 한 번만 알림 + [예배당 화면 켜기]
    pub(super) fn warn_output_off_after_send(&mut self) {
        // 화면 스레드 하나에서만 쓴다 (시험은 시험마다 스레드가 달라 서로 영향을 주지 않는다)
        thread_local!(static WARNED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) });
        if self.out.running() {
            WARNED.with(|w| w.set(false));
            return;
        }
        let mut c = self.core.lock();
        let auto = c.settings.get()["output"]["autoOnSend"].as_bool().unwrap_or(true);
        if auto || c.pres.live.is_none() || WARNED.with(|w| w.replace(true)) {
            return;
        }
        c.notify_act(tr("송출했지만 예배당 화면이 꺼져 있어 회중에게 보이지 않습니다"), "warn", "output-on");
    }
}

/// '클릭하면: [미리보기] [바로 송출]' (오른쪽→왼쪽 배치 안에서 부름). 눌린 쪽을 Some(미리보기 켬지)로
fn studio_split(ui: &mut Ui, preview: bool) -> Option<bool> {
    let p = pal();
    let mut out = None;
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let direct = Btn::new(tr("바로 송출"))
            .kind(if preview { Kind::Default } else { Kind::Warn })
            .selected(!preview)
            .tooltip(tr("클릭한 본문·순서·슬라이드가 미리보기를 거치지 않고 바로 회중 화면에 나갑니다 (OBS 스튜디오 모드 끄기와 같음)"));
        let r = ui.add(direct);
        widgets::mark("studio-direct", r.rect);
        if r.clicked() {
            out = Some(false);
        }
        let pv = Btn::new(tr("미리보기"))
            .kind(if preview { Kind::Pv } else { Kind::Default })
            .tooltip(tr("클릭하면 미리보기에만 놓이고, 송출 단추를 눌러야 회중 화면에 나갑니다"));
        let r = ui.add(pv);
        widgets::mark("studio-preview", r.rect);
        if r.clicked() {
            out = Some(true);
        }
    });
    ui.add(egui::Label::new(theme::rich(tr("클릭하면"), 13.0, Weight::Regular, p.muted)).selectable(false));
    out
}

impl App {
    /// 바로 송출 중일 때 머리 줄 아래에 계속 보이는 주황 띠 (평가 7번)
    pub(super) fn direct_band(&mut self, ui: &mut Ui, acts: &mut Vec<Act>) {
        let p = pal();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            icons::show(ui, "radio", 16.0, p.warn);
            ui.add(egui::Label::new(theme::rich(tr("클릭하면 바로 송출됩니다"), 13.0, Weight::Bold, p.text)).selectable(false));
            ui.add(egui::Label::new(theme::rich(tr("큐시트 순서·본문·슬라이드를 누르는 즉시 회중 화면에 나갑니다"), 13.0, Weight::Regular, p.text2)).truncate().selectable(false));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let b = ui.add(Btn::new(tr("미리보기로 돌아가기")).size(Size::Sm).height(24.0).tooltip(tr("클릭하면 미리보기에 놓고 송출 단추로 내보내는 방식으로")));
                widgets::mark("direct-band-back", b.rect);
                if b.clicked() {
                    acts.push(Act::TogglePreviewMode);
                }
            });
        });
    }
}

/// 알림 단추 글·아이콘 (Core::notify_act 의 act 이름)
fn toast_button(act: &str) -> (String, &'static str) {
    match act {
        "undo" => (tr("되돌리기"), "undo-2"),
        "output-on" => (tr("예배당 화면 켜기"), "monitor"),
        "add-dup" => (tr("그래도 하나 더 넣기"), "plus"),
        _ => (tr("확인"), "check"),
    }
}

/// 로고 + 워십<b>프리젠터</b> (.brand: 26px 그림 · 간격 8 · 700, 프리젠터는 hl 800)
fn brand(ui: &mut Ui) {
    let p = pal();
    let r = ui.add(egui::Image::from_bytes("bytes://wp-brand.svg", BRAND_SVG).fit_to_exact_size(Vec2::splat(26.0)));
    let _ = r.on_hover_text(tr("워십프리젠터"));
    ui.add_space(8.0 - ui.spacing().item_spacing.x);
    let name = tr("워십프리젠터");
    let mut job = egui::text::LayoutJob::default();
    // 영어 화면 등 번역되면 한 덩어리로
    let (a, b) = if name == "워십프리젠터" { ("워십".to_string(), "프리젠터".to_string()) } else { (name.clone(), String::new()) };
    job.append(&a, 0.0, egui::TextFormat { font_id: theme::font(14.0, Weight::Bold), color: p.text, extra_letter_spacing: 0.2, ..Default::default() });
    job.append(&b, 0.0, egui::TextFormat { font_id: theme::font(14.0, Weight::Bold), color: p.hl, extra_letter_spacing: 0.2, ..Default::default() });
    ui.label(job);
    ui.add_space(2.0); // .brand { margin-right: 2px }
}
