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
use crate::theme::{self, Weight, alpha, pal};
use crate::widgets::{self, Btn, Kind, Size};
use eframe::egui::{self, Align, Align2, Color32, Id, Key, Layout, Pos2, Rect, RichText, Sense, Shape, Stroke, StrokeKind, Ui, Vec2};
use serde_json::json;
use wp_core::i18n::tr;



fn text_w(ui: &Ui, s: &str, size: f32, w: Weight) -> f32 {
    ui.painter().layout_no_wrap(s.to_string(), theme::font(size, w), Color32::WHITE).size().x
}




impl App {


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
            // '>' 로 시작하면 말로 시키기 (예: > 지난주 큐시트 복사해서 설교만 바꿔)
            if let Some(cmd) = self.search.trim().strip_prefix('>') {
                crate::say_ui::open_with(cmd.trim());
                self.search.clear();
            } else {
                acts.push(Act::Search(self.search.clone()));
            }
        }
        let _ = s;
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
            // 송출 중인데 꺼져 있으면 주황 (뷰어 안 띠와 같은 뜻 — 아래 알림 띠 대신 이 칸 하나)
            (false, _) if p.bevel && s.live.is_some() => (p.warn, tr("예배당 화면 꺼짐 — 회중에게 안 보임")),
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
        if p.bevel {
            // (나): 알림은 아래 상태 줄 안에 한 줄로 (probar_ui::status_strip → toast_inline). 떠 있는 창·편집칸을 가리지 않는다
            return None;
        }
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
        // 좁은 뷰어에서는 긴 글이 잘리지 않게 짧은 글로 (2.1.0 다듬기)
        let long = tr("예배당 화면 꺼짐 — 회중에게 안 보임");
        let fits = ui.painter().layout_no_wrap(long.clone(), theme::font(11.5, Weight::SemiBold), Color32::WHITE).size().x <= tr_.width() - 22.0;
        let text = if fits { long } else { tr("예배당 화면 꺼짐") };
        ui.painter().with_clip_rect(tr_).text(Pos2::new(tr_.left() + 20.0, band.center().y), Align2::LEFT_CENTER, text, theme::font(11.5, Weight::SemiBold), Color32::WHITE);
    }

    /// 송출했는데 예배당 화면이 꺼져 있고 자동으로 켜지도 않을 때, 꺼진 동안 한 번만 알림 + [예배당 화면 켜기]
    pub(super) fn warn_output_off_after_send(&mut self) {
        // 화면 스레드 하나에서만 쓴다 (시험은 시험마다 스레드가 달라 서로 영향을 주지 않는다)
        thread_local!(static WARNED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) });
        if self.out.running() {
            WARNED.with(|w| w.set(false));
            return;
        }
        // (나) 전문가 테마: 송출 중 뷰어 안 띠 [지금 켜기] + 상태 줄 '예배당 화면 꺼짐' 칸(주황)이 알린다 — 아래 알림 띠를 또 띄우지 않는다
        if pal().bevel {
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


impl App {
    /// (나) 상태 줄 안의 알림: 종류 색 점 · 글(누르면 닫힘) · [동작 단추] · ✕. 떠 있으면 Some(바탕색)을 먼저 알려 준다
    pub(crate) fn toast_inline(&self, ui: &mut Ui, s: &Snap, acts: &mut Vec<Act>) -> bool {
        let Some((msg, kind, at)) = s.toast.as_ref() else { return false };
        let life = if s.toast_act.is_some() { 12000 } else { 8000 };
        if wp_core::util::now_ms() - at >= life {
            return false;
        }
        let p = pal();
        let c = match *kind {
            "error" => p.live,
            "warn" => p.warn,
            "ok" => p.ok_text,
            _ => p.text2,
        };
        let (dot, _) = ui.allocate_exact_size(Vec2::new(10.0, 18.0), Sense::hover());
        ui.painter().circle_filled(dot.center(), 4.0, c);
        let room = (ui.available_width() * 0.6).max(120.0);
        let lr = ui.scope(|ui| {
            ui.set_max_width(room);
            ui.add(egui::Label::new(theme::rich(wp_core::i18n::tr_dyn(msg), 12.0, Weight::SemiBold, p.text)).truncate().selectable(false).sense(Sense::click()))
        }).inner.on_hover_text(format!("{}\n({})", wp_core::i18n::tr_dyn(msg), tr("눌러서 닫기")));
        widgets::mark("toast-msg", lr.rect);
        if lr.clicked() {
            acts.push(Act::ToastDismiss);
        }
        if let Some(a) = s.toast_act {
            let (label, icon) = toast_button(a);
            let br = ui.add(Btn::new(label).icon(icon).size(Size::Sm).height(18.0));
            widgets::mark("toast-act", br.rect);
            if br.clicked() {
                acts.push(Act::ToastAct(a));
            }
        }
        if ui.add(Btn::icon_only("x").kind(Kind::Ghost).size(Size::Sm).height(18.0).tooltip(tr("닫기"))).clicked() {
            acts.push(Act::ToastDismiss);
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
        true
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

