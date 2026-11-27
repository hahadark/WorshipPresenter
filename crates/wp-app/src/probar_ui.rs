//! (나) 어두운 전문가용 — 위 컨트롤 막대 (시안 docs/design/pro-main.html `.cb`).
//!
//! 왼쪽: 사이드바 · 시간표 접기/펴기 / 가운데: [이전 | 송출 F9 | 다음] · 상태 표시창(예배 시각 | 송출 중 | 다음) ·
//! 글자 숨김 · 지우기 ▾ · 즉석 자막 · 예배당 화면 / 오른쪽: 찾기칸 · 찾기 옵션 · 설정 · 인스펙터 접기/펴기.
//! 좁은 창(1000px)에서는 도구 이름을 빼고 아이콘만, 상태 표시창은 '예배 시각' 칸을 뺀다.
//!
//! 예전 머리 줄(topbar_ui::top_bar)과 아래 송출 줄(bottom_bar)의 단추는 모두 여기 또는 아래 상태 줄로 옮겼다.
use crate::app::{Act, App, Snap};
use crate::icons;
use crate::menubar;
use crate::theme::{self, Weight, alpha, mix, pal};
use crate::widgets;
use eframe::egui::{self, Align, Align2, Color32, Id, Layout, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, UiBuilder, Vec2};
use serde_json::json;
use wp_core::i18n::{tr, tr_args};

/// 지금 지역 시각 (밀리초, 하루 안). 시간대는 1분에 한 번만 다시 구한다 (맥은 SQLite 를 열어 구함)
pub(crate) fn local_ms() -> i64 {
    static OFF: parking_lot::Mutex<(i64, i64)> = parking_lot::Mutex::new((0, 0));
    let now = wp_core::util::now_ms();
    let mut o = OFF.lock();
    if o.0 == 0 || now.saturating_sub(o.0) > 60_000 {
        *o = (now, wp_core::localtime::local_offset_ms());
    }
    now + o.1
}

/// "HH:MM:SS"
pub(crate) fn clock_text(local: i64) -> String {
    let s = local.div_euclid(1000).rem_euclid(86_400);
    format!("{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
}

/// 상태 표시창에 보일 '송출 중' · '다음' 내용
pub(crate) struct LcdInfo {
    /// 송출 중: (번호·제목, 슬라이드 k/n, 경과 초)
    pub live: Option<(String, String, i64)>,
    /// 다음: (예정 시각, 번호·제목)
    pub next: Option<(String, String)>,
    /// 바로 송출 방식 (누르면 바로 나감) — 표시창 '송출 중' 이름표 옆에 글자로
    pub direct: bool,
}

/// 순서 이름 (번호 + 제목) — 큐시트 순서면 'N 제목', 아니면 제목만
fn deck_title(d: &wp_core::presentation::Deck) -> (Option<usize>, String) {
    let idx = d.passage["cueIndex"].as_i64().filter(|i| *i >= 0).map(|i| i as usize);
    let t = d.passage["itemTitle"].as_str().filter(|x| !x.is_empty()).or_else(|| d.passage["title"].as_str()).unwrap_or("").to_string();
    (idx, match idx { Some(i) => format!("{} {}", i + 1, t), None => t })
}

/// 지금 송출 중인 순서가 언제 시작됐는지 (순서가 바뀔 때마다 다시 잼). 슬라이드를 넘겨도 그대로
fn live_started(ctx: &egui::Context, key: &str) -> i64 {
    let id = Id::new("pro-live-start");
    let now = wp_core::util::now_ms();
    let (k, at) = ctx.data(|d| d.get_temp::<(String, i64)>(id)).unwrap_or_default();
    if k == key {
        return at;
    }
    ctx.data_mut(|d| d.insert_temp(id, (key.to_string(), now)));
    now
}

pub(crate) fn lcd_info(ctx: &egui::Context, s: &Snap, planned: &[String]) -> LcdInfo {
    let live = s.live.as_ref().map(|l| {
        let (_, t) = deck_title(&l.deck);
        let n = l.deck.slides.len();
        let k = if n > 1 { format!("{}/{}", l.deck.index + 1, n) } else { String::new() };
        let key = format!("{}:{}:{}", l.deck.passage["cueId"], l.deck.passage["cueIndex"], l.deck.passage["title"]);
        let at = live_started(ctx, &key);
        (t, k, ((wp_core::util::now_ms() - at) / 1000).max(0))
    });
    // 다음: 미리보기에 다른 것이 올라 있으면 그것, 아니면 송출 중 순서의 다음 순서
    let live_idx = s.live.as_ref().and_then(|l| l.deck.passage["cueIndex"].as_i64()).filter(|i| *i >= 0).map(|i| i as usize);
    let pv = s.preview.as_ref().filter(|d| s.live.as_ref().is_none_or(|l| l.deck.passage != d.passage));
    let next = if let Some(d) = pv {
        let (i, t) = deck_title(d);
        Some((i.and_then(|i| planned.get(i).cloned()).unwrap_or_default(), t))
    } else {
        live_idx.map(|i| i + 1).filter(|i| *i < s.items.len()).map(|i| {
            let it = &s.items[i];
            let t = crate::cue_ui::detail(it);
            let name = it["title"].as_str().filter(|x| !x.is_empty()).map(|x| x.to_string()).unwrap_or(t);
            (planned.get(i).cloned().unwrap_or_default(), format!("{} {}", i + 1, name))
        })
    };
    LcdInfo { live, next, direct: false }
}

/// 막대 위 도드라진 단추 (.tpb) — 아이콘(+글)
fn tp_button(ui: &mut Ui, rect: Rect, id: Id, icon: &str, label: &str, kbd: &str, send: bool, tip: &str) -> Response {
    let p = pal();
    let resp = ui.interact(rect, id, Sense::click()).on_hover_text(tip);
    let hov = resp.hovered();
    let (bg, fg) = if send {
        (if hov { if p.bevel { Color32::WHITE } else { mix(p.send_bg, 85.0, Color32::WHITE) } } else { p.send_bg }, p.send_fg)
    } else {
        (if hov { mix(p.raised, 80.0, Color32::WHITE) } else { p.raised }, p.text)
    };
    let r = if resp.is_pointer_button_down_on() { rect.translate(Vec2::new(0.0, 0.5)) } else { rect };
    let pt = ui.painter();
    pt.rect_filled(r, theme::cr(theme::Rad::Md), bg);
    widgets::paint_bevel(ui, r);
    if !p.bevel {
        pt.rect_stroke(r, theme::cr(theme::Rad::Md), Stroke::new(p.border_w, if send { bg } else { p.line }), StrokeKind::Inside);
    }
    let ig = 14.0;
    let lg = (!label.is_empty()).then(|| pt.layout_no_wrap(label.to_string(), theme::font(13.5, Weight::Bold), fg));
    let kg = (!kbd.is_empty()).then(|| pt.layout_no_wrap(kbd.to_string(), theme::font(10.5, Weight::SemiBold), alpha(fg, 0.55)));
    let w = ig + lg.as_ref().map(|g| 6.0 + g.size().x).unwrap_or(0.0) + kg.as_ref().map(|g| 6.0 + g.size().x).unwrap_or(0.0);
    let mut x = r.center().x - w / 2.0;
    icons::paint(ui, Rect::from_center_size(Pos2::new(x + ig / 2.0, r.center().y), Vec2::splat(ig)), icon, fg);
    x += ig;
    if let Some(g) = lg {
        x += 6.0;
        let gw = g.size().x;
        pt.galley(Pos2::new(x, r.center().y - g.size().y / 2.0), g, fg);
        x += gw;
    }
    if let Some(g) = kg {
        x += 6.0;
        pt.galley(Pos2::new(x, r.center().y - g.size().y / 2.0 + 1.0), g, alpha(fg, 0.55));
    }
    if hov {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// 아이콘 위 · 이름 아래 도구 (.tool). `on` 이면 강조색, `narrow` 면 이름 없이 아이콘만
fn tool(ui: &mut Ui, rect: Rect, id: Id, icon: &str, label: &str, on: Option<Color32>, narrow: bool, tip: &str) -> Response {
    let p = pal();
    let resp = ui.interact(rect, id, Sense::click()).on_hover_text(tip);
    let hov = resp.hovered();
    if hov || on.is_some() {
        ui.painter().rect_filled(rect.shrink2(Vec2::new(1.0, 3.0)), theme::cr(theme::Rad::Md), if on.is_some() { alpha(on.unwrap_or(p.hover), 0.16) } else { p.hover });
    }
    let ic = on.unwrap_or(if hov { p.text } else { p.text2 });
    if narrow {
        icons::paint(ui, Rect::from_center_size(rect.center(), Vec2::splat(18.0)), icon, ic);
    } else {
        icons::paint(ui, Rect::from_center_size(Pos2::new(rect.center().x, rect.top() + 17.0), Vec2::splat(17.0)), icon, ic);
        let g = ui.painter().layout_no_wrap(label.to_string(), theme::font(10.5, Weight::Medium), on.unwrap_or(p.muted));
        let gx = rect.center().x - g.size().x / 2.0;
        ui.painter().galley(Pos2::new(gx, rect.top() + 30.0), g, on.unwrap_or(p.muted));
    }
    if hov {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// 작은 아이콘 단추 (.icb: 30×26), `on` 이면 노랑
pub(crate) fn icb(ui: &mut Ui, rect: Rect, id: Id, icon: &str, on: bool, tip: &str) -> Response {
    let p = pal();
    let resp = ui.interact(rect, id, Sense::click()).on_hover_text(tip);
    if resp.hovered() {
        ui.painter().rect_filled(rect, theme::cr(theme::Rad::Md), p.hover);
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    icons::paint(ui, Rect::from_center_size(rect.center(), Vec2::splat(17.0)), icon, if on { p.accent } else { p.text2 });
    resp
}

/// 상태 표시창 (.lcd)
fn paint_lcd(ui: &mut Ui, rect: Rect, info: &LcdInfo, clock: Option<&str>) {
    let p = pal();
    let pt = ui.painter();
    pt.rect_filled(rect, theme::cr(theme::Rad::Lg), p.lcd);
    // 안으로 들어간 느낌: 위 어두운 선 · 아래 밝은 선
    pt.hline(rect.x_range().shrink(4.0), rect.top() + 0.5, Stroke::new(1.0, Color32::from_black_alpha(160)));
    pt.hline(rect.x_range().shrink(4.0), rect.bottom() + 0.5, Stroke::new(1.0, alpha(Color32::WHITE, 0.08)));
    let label_f = theme::font(9.5, Weight::SemiBold);
    let sep = mix(p.lcd, 70.0, Color32::WHITE);
    let mut cols: Vec<(f32, Box<dyn Fn(&egui::Painter, Rect)>)> = vec![];
    if let Some(c) = clock {
        let c = c.to_string();
        let lf = label_f.clone();
        let (lt, ll) = (p.lcd_text, p.lcd_label);
        cols.push((112.0, Box::new(move |pt: &egui::Painter, r: Rect| {
            pt.text(Pos2::new(r.left() + 12.0, r.top() + 6.0), Align2::LEFT_TOP, tr("예배 시각"), lf.clone(), ll);
            pt.text(Pos2::new(r.left() + 12.0, r.bottom() - 4.0), Align2::LEFT_BOTTOM, &c, theme::font(20.0, Weight::SemiBold), lt);
        })));
    }
    let rest = (rect.width() - cols.iter().map(|c| c.0).sum::<f32>()) / 2.0;
    let (live_c, next_c) = (p.live_text, p.next);
    let lt = p.lcd_text;
    let ll = p.lcd_label;
    let live = info.live.clone();
    let direct = info.direct;
    let warn_c = p.warn;
    let lf = label_f.clone();
    cols.push((rest, Box::new(move |pt: &egui::Painter, r: Rect| {
        let r = r.shrink2(Vec2::new(12.0, 0.0));
        let pt = pt.with_clip_rect(r);
        match &live {
            Some((t, k, sec)) => {
                let lr = pt.text(Pos2::new(r.left(), r.top() + 6.0), Align2::LEFT_TOP, format!("{} · {:02}:{:02}", tr("송출 중"), sec / 60, sec % 60), lf.clone(), live_c);
                if direct {
                    pt.text(Pos2::new(lr.right() + 6.0, r.top() + 6.0), Align2::LEFT_TOP, tr("바로 송출"), lf.clone(), warn_c);
                }
                let g = pt.layout_no_wrap(t.clone(), theme::font(13.0, Weight::Bold), lt);
                let gw = g.size().x;
                pt.galley(Pos2::new(r.left(), r.bottom() - 6.0 - g.size().y), g, lt);
                if !k.is_empty() {
                    pt.text(Pos2::new(r.left() + gw + 6.0, r.bottom() - 6.0), Align2::LEFT_BOTTOM, k, theme::font(12.0, Weight::Regular), lt);
                }
            }
            None => {
                let lr = pt.text(Pos2::new(r.left(), r.top() + 6.0), Align2::LEFT_TOP, tr("송출 중"), lf.clone(), ll);
                if direct {
                    pt.text(Pos2::new(lr.right() + 6.0, r.top() + 6.0), Align2::LEFT_TOP, tr("바로 송출"), lf.clone(), warn_c);
                }
                pt.text(Pos2::new(r.left(), r.bottom() - 6.0), Align2::LEFT_BOTTOM, tr("없음"), theme::font(13.0, Weight::Bold), ll);
            }
        }
    })));
    let next = info.next.clone();
    cols.push((rest, Box::new(move |pt: &egui::Painter, r: Rect| {
        let r = r.shrink2(Vec2::new(12.0, 0.0));
        let pt = pt.with_clip_rect(r);
        match &next {
            Some((at, t)) => {
                let head = if at.is_empty() { tr("다음") } else { format!("{} · {}", tr("다음"), at) };
                pt.text(Pos2::new(r.left(), r.top() + 6.0), Align2::LEFT_TOP, head, label_f.clone(), next_c);
                pt.text(Pos2::new(r.left(), r.bottom() - 6.0), Align2::LEFT_BOTTOM, t, theme::font(13.0, Weight::Bold), lt);
            }
            None => {
                pt.text(Pos2::new(r.left(), r.top() + 6.0), Align2::LEFT_TOP, tr("다음"), label_f.clone(), ll);
                pt.text(Pos2::new(r.left(), r.bottom() - 6.0), Align2::LEFT_BOTTOM, tr("없음"), theme::font(13.0, Weight::Bold), ll);
            }
        }
    })));
    let mut x = rect.left();
    let n = cols.len();
    for (k, (w, f)) in cols.into_iter().enumerate() {
        let r = Rect::from_min_size(Pos2::new(x, rect.top()), Vec2::new(w, rect.height()));
        f(pt, r);
        x += w;
        if k + 1 < n {
            pt.vline(x, rect.y_range().shrink(7.0), Stroke::new(1.0, sep));
        }
    }
}

impl App {
    /// 위 컨트롤 막대 (높이 58). 바깥 Panel 이 bar 색 · 아래 edge 선을 그린다
    pub(crate) fn control_bar(&mut self, ui: &mut Ui, s: &Snap, acts: &mut Vec<Act>) {
        let p = pal();
        let (bar, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), theme::BAR_H), Sense::hover());
        // 위 1px 밝은 선 (box-shadow: inset 0 1px 0 #4a4a4e)
        ui.painter().hline(bar.x_range(), bar.top() + 0.5, Stroke::new(1.0, p.bar_hi));
        let shortcut_settings = json!({"shortcuts": self.core.lock().settings.get()["shortcuts"].clone()});
        let sk = |id: &str, d: &str| crate::keys::label(&shortcut_settings, id, d);
        let w = bar.width();
        let cy = bar.center().y;
        // 폭에 따라: 도구 이름 · 예배 시각 칸
        let narrow = w < 1360.0;
        let tool_w = if narrow { 38.0 } else { 56.0 };
        let send_w = if w < 1200.0 { 118.0 } else { 150.0 };
        let left_w = 12.0 + 30.0 + 2.0 + 30.0;
        let search_w = (w * 0.14).clamp(150.0, 260.0);
        let right_w = search_w + 6.0 + 30.0 + 2.0 + 30.0 + 2.0 + 30.0 + 12.0;
        let tp_w = 44.0 + 2.0 + send_w + 2.0 + 44.0;
        let tools_w = tool_w * 4.0;
        let room = w - left_w - right_w - tp_w - tools_w - 4.0 * 12.0;
        let lcd_w = room.clamp(200.0, 470.0);
        let show_clock = lcd_w >= 380.0;
        let center_w = tp_w + 12.0 + lcd_w + 12.0 + tools_w;
        // 가운데 묶음: 남는 자리 가운데 (모자라면 왼쪽 묶음 바로 뒤)
        let free_l = bar.left() + left_w + 12.0;
        let free_r = bar.right() - right_w - 12.0;
        let cx0 = (free_l + ((free_r - free_l) - center_w).max(0.0) / 2.0).max(free_l);

        // ---- 왼쪽: 사이드바 · 시간표
        let r1 = Rect::from_center_size(Pos2::new(bar.left() + 12.0 + 15.0, cy), Vec2::new(30.0, 26.0));
        let side_on = self.pro_side_visible();
        if icb(ui, r1, Id::new("pro-side"), "panel-left", side_on, &tr("왼쪽 큐시트 판 보이기·숨기기")).clicked() {
            self.pro_toggle_side();
        }
        let r2 = r1.translate(Vec2::new(32.0, 0.0));
        let tl_on = self.pro_timeline_visible();
        if icb(ui, r2, Id::new("pro-timeline"), "panel-bottom", tl_on, &tr("아래 예배 시간표 보이기·숨기기")).clicked() {
            self.pro_toggle_timeline();
        }

        // ---- 가운데: [이전 | 송출 | 다음]
        let mut x = cx0;
        let th = 32.0;
        let rp = Rect::from_min_size(Pos2::new(x, cy - th / 2.0), Vec2::new(44.0, th));
        let r = tp_button(ui, rp, Id::new("pro-prev"), "skip-back", "", "", false, &format!("{} ({})", tr("이전 슬라이드"), sk("prevSlide", "F5")));
        widgets::mark("pro-prev", r.rect);
        if r.clicked() {
            acts.push(Act::Step(-1));
        }
        x += 46.0;
        let rs = Rect::from_min_size(Pos2::new(x, cy - th / 2.0), Vec2::new(send_w, th));
        let r = tp_button(ui, rs, Id::new("pro-send"), "play", &tr("송출"), &sk("sendPreview", "F9"), true, &tr("미리보기를 송출"));
        widgets::mark("pro-send", r.rect);
        if r.clicked() {
            acts.push(Act::SendPreview);
        }
        x += send_w + 2.0;
        let rn = Rect::from_min_size(Pos2::new(x, cy - th / 2.0), Vec2::new(44.0, th));
        let r = tp_button(ui, rn, Id::new("pro-next"), "skip-forward", "", "", false, &format!("{} ({})", tr("다음 슬라이드"), sk("nextSlide", "F6")));
        widgets::mark("pro-next", r.rect);
        if r.clicked() {
            acts.push(Act::Step(1));
        }
        x += 44.0 + 12.0;
        // ---- 상태 표시창
        let planned = self.core.lock().planned_times();
        let mut info = lcd_info(ui.ctx(), s, &planned);
        info.direct = !s.preview_mode;
        let lr = Rect::from_min_size(Pos2::new(x, cy - 21.0), Vec2::new(lcd_w, 42.0));
        let clock = clock_text(local_ms());
        paint_lcd(ui, lr, &info, show_clock.then_some(clock.as_str()));
        let lresp = ui.interact(lr, Id::new("pro-lcd"), Sense::hover());
        if !show_clock {
            lresp.on_hover_text(format!("{} {}", tr("예배 시각"), clock));
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(1000 - (local_ms().rem_euclid(1000)) as u64 + 5));
        x += lcd_w + 12.0;
        // ---- 도구: 글자 숨김 · 지우기 ▾ · 즉석 자막 · 예배당 화면
        let tr_ = |x: f32| Rect::from_min_size(Pos2::new(x, bar.top() + 6.0), Vec2::new(tool_w, bar.height() - 12.0));
        let hidden = s.live.as_ref().is_some_and(|l| !l.visible);
        let r = tool(ui, tr_(x), Id::new("pro-hide"), "eye-off", &if hidden { tr("글자 보이기") } else { tr("글자 숨김") }, hidden.then_some(p.warn), narrow, &format!("{} ({})", tr("글자(슬라이드)만 숨기기·다시 보이기"), sk("toggleClear", "F10")));
        widgets::mark("pro-hide", r.rect);
        if r.clicked() {
            acts.push(Act::ToggleClear);
        }
        x += tool_w;
        let more_on = s.hide_bg || s.logo;
        let r = tool(ui, tr_(x), Id::new("pro-clear"), "eraser", &tr("지우기"), more_on.then_some(p.warn), narrow, &tr("예배당 화면 비우기: 배경 지우기 · 로고 화면 · 모두 지우기 (큐시트 순서는 그대로)"));
        widgets::mark("pro-clear", r.rect);
        egui::Popup::menu(&r).frame(menubar::drop_frame()).gap(2.0).show(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            if menubar::item_row(ui, Some("image"), &tr("배경 지우기"), "", s.hide_bg.then_some(true)).clicked() {
                acts.push(Act::ToggleBg);
                ui.close();
            }
            if menubar::item_row(ui, Some("flag"), &tr("로고 화면"), "", s.logo.then_some(true)).clicked() {
                acts.push(Act::ToggleLogo);
                ui.close();
            }
            menubar::sep(ui);
            if menubar::item_row(ui, Some("x"), &tr("모두 지우기"), "", None).on_hover_text(tr("예배당 화면을 모두 비움: 글자·배경·로고 (큐시트 순서는 지우지 않음)")).clicked() {
                acts.push(Act::ClearAll);
                ui.close();
            }
        });
        x += tool_w;
        let qon = self.tb.quick;
        let r = tool(ui, tr_(x), Id::new("pro-quick"), "text", &tr("즉석 자막"), qon.then_some(p.accent), narrow, &format!("{} ({})", tr("즉석 자막: 글을 써서 바로 송출"), sk("quickText", "Ctrl+T")));
        widgets::mark("pro-quick", r.rect);
        if r.clicked() {
            self.tb.quick = !qon;
        }
        x += tool_w;
        let running = self.out.running();
        let r = tool(
            ui,
            tr_(x),
            Id::new("pro-output"),
            if running { "monitor" } else { "monitor-off" },
            &if running { tr("예배당 켜짐") } else { tr("예배당 꺼짐") },
            running.then_some(p.ok_text),
            narrow,
            &format!("{} ({})", if running { tr("예배당 화면: 켜짐 · 누르면 끄기") } else { tr("예배당 화면: 꺼짐 · 누르면 켜기") }, sk("toggleOutputWindow", "Ctrl+Shift+F")),
        );
        widgets::mark("pro-output", r.rect);
        if r.clicked() {
            acts.push(Act::OutputToggle);
        }

        // ---- 오른쪽: 찾기칸 · 찾기 옵션 · 설정 · 인스펙터
        let mut rx = bar.right() - 12.0 - 30.0;
        let ri = Rect::from_min_size(Pos2::new(rx, cy - 13.0), Vec2::new(30.0, 26.0));
        let ins_on = self.pro_inspector_visible();
        if icb(ui, ri, Id::new("pro-inspector"), "panel-right", ins_on, &tr("오른쪽 인스펙터 보이기·숨기기")).clicked() {
            self.pro_toggle_inspector();
        }
        rx -= 32.0;
        let rg = Rect::from_min_size(Pos2::new(rx, cy - 13.0), Vec2::new(30.0, 26.0));
        if icb(ui, rg, Id::new("pro-settings"), "settings", false, &tr("설정 (Ctrl+,)")).clicked() {
            self.tc.settings.open = true;
        }
        rx -= 32.0;
        let ro = Rect::from_min_size(Pos2::new(rx, cy - 13.0), Vec2::new(30.0, 26.0));
        let or = icb(ui, ro, Id::new("pro-search-opts"), "book-open", false, &tr("찾기 방식 · 성경 번역본 · 병행 번역"));
        widgets::mark("pro-search-opts", or.rect);
        self.search_options_popup(&or, s, acts);
        rx -= 6.0 + search_w;
        let rsr = Rect::from_min_size(Pos2::new(rx, cy - 13.0), Vec2::new(search_w, 26.0));
        ui.scope_builder(UiBuilder::new().max_rect(rsr).layout(Layout::left_to_right(Align::Center)), |ui| {
            self.search_box(ui, s, acts, rsr.width(), 26.0, 12.5, &tr("요3:16 · 찬양 제목 (Ctrl+K)"));
        });
    }

    /// 찾기 방식 · 번역본 · 병행 번역을 한 펼침 메뉴로 (예전 머리 줄 .hgroup 세 칸)
    pub(crate) fn search_options_popup(&mut self, resp: &Response, s: &Snap, acts: &mut Vec<Act>) {
        let (mode, parallel) = {
            let c = self.core.lock();
            (c.settings.str(&["searchMode"]), c.settings.str(&["parallelVersionId"]))
        };
        let modes = [("auto", tr("자동")), ("reference", tr("장절")), ("text", tr("본문"))];
        let mut pick_mode = None;
        let mut pick_par: Option<String> = None;
        egui::Popup::menu(resp).frame(menubar::drop_frame()).gap(2.0).show(|ui| {
            ui.set_min_width(240.0);
            ui.spacing_mut().item_spacing.y = 0.0;
            menubar::head(ui, &tr("찾기 방식"));
            for (k, l) in &modes {
                if menubar::item_row(ui, None, l, "", Some(*k == mode)).clicked() {
                    pick_mode = Some(*k);
                    ui.close();
                }
            }
            menubar::sep(ui);
            menubar::head(ui, &tr("성경 번역본"));
            for v in &self.versions {
                if menubar::item_row(ui, None, &v.name, "", Some(v.id == s.version)).clicked() {
                    acts.push(Act::SetVersion(v.id.clone()));
                    ui.close();
                }
            }
            menubar::sep(ui);
            menubar::head(ui, &tr("병행 번역 (두 번역을 함께 송출)"));
            if menubar::item_row(ui, None, &tr("병행: 없음"), "", Some(parallel.is_empty())).clicked() {
                pick_par = Some(String::new());
                ui.close();
            }
            for v in &self.versions {
                if menubar::item_row(ui, None, &format!("+ {}", v.name), "", Some(v.id == parallel)).clicked() {
                    pick_par = Some(v.id.clone());
                    ui.close();
                }
            }
        });
        if let Some(k) = pick_mode {
            self.core.lock().settings.update(|d| d["searchMode"] = json!(k));
            if !self.search.trim().is_empty() {
                acts.push(Act::SearchList(self.search.clone()));
            }
        }
        if let Some(id) = pick_par {
            self.core.lock().settings.update(|d| d["parallelVersionId"] = json!(id));
            if !self.search.trim().is_empty() {
                acts.push(Act::Search(self.search.clone()));
            }
        }
    }

    /// 아래 상태 줄 (높이 24): 영상·음악 조작(재생 중일 때) · 미리보기/바로 송출 · 출력 연결 상태 점.
    /// 빈 자리 오른쪽 클릭 = 화면 지우기 메뉴 (예전 아래 송출 줄과 같음)
    pub(crate) fn status_strip(&mut self, ui: &mut Ui, s: &Snap, acts: &mut Vec<Act>) {
        self.video_bar(ui);
        let bar = ui.interact(ui.available_rect_before_wrap(), ui.id().with("footer-menu"), Sense::click());
        crate::ctx_ui::footer_menu(self, &bar, acts);
        // 알림이 떠 있으면 상태 줄 바탕을 그 종류 색으로 살짝 (눈에 띄게)
        if let Some((_, kind, at)) = s.toast.as_ref().filter(|_| pal().bevel) {
            let life = if s.toast_act.is_some() { 12000 } else { 8000 };
            if wp_core::util::now_ms() - at < life {
                let p = pal();
                let c = match *kind { "error" => p.live, "warn" => p.warn, "ok" => p.ok_text, _ => p.line2 };
                let r = ui.max_rect().expand2(Vec2::new(10.0, 2.0));
                ui.painter().rect_filled(r, 0.0, theme::mix(c, 22.0, p.head));
            }
        }
        ui.horizontal(|ui| {
            ui.set_min_height(22.0);
            ui.spacing_mut().item_spacing.x = 8.0;
            let p = pal();
            // 알림을 상태 줄 안에 두는 것은 (나) 전문가 테마만 (옛 테마는 예전처럼 오른쪽 아래 상자)
            let toast = p.bevel && self.toast_inline(ui, s, acts);
            if toast {
                widgets::vsep(ui, 14.0);
            }
            // 누르면: [미리보기 | 바로 송출]
            ui.add(egui::Label::new(theme::rich(tr("누르면"), 11.5, Weight::Regular, p.muted)).selectable(false));
            let mut pv = s.preview_mode;
            let (a, b) = (tr("미리보기"), tr("바로 송출"));
            let r = widgets::segmented_with(ui, &mut pv, &[(true, a.as_str()), (false, b.as_str())], widgets::SegStyle::TINY);
            widgets::mark("studio-split", r.rect);
            let (l, rr) = r.rect.split_left_right_at_fraction(0.5);
            widgets::mark("studio-preview", l);
            widgets::mark("studio-direct", rr);
            let r = r.on_hover_text(tr("미리보기: 누른 것이 '다음' 화면에 놓이고 송출 단추로 내보냄 · 바로 송출: 누르는 즉시 회중 화면에"));
            if r.changed() && pv != s.preview_mode {
                acts.push(Act::TogglePreviewMode);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 14.0;
                let items = self.status_items(s);
                let widths: Vec<f32> = items.iter().map(|x| ui.painter().layout_no_wrap(x.1.clone(), theme::font(12.0, Weight::Regular), Color32::WHITE).size().x).collect();
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
        let _ = tr_args;
    }
}

/// (나) 배치의 접기/펴기 (설정 `rsProLayout` {side, inspector, timeline}). 예전 작업공간 배치(`rsWorkspace`)는 읽기만 하고 건드리지 않는다
impl App {
    fn pro_flag(&self, k: &str) -> bool {
        // 사용자가 정하지 않았으면: 인스펙터는 창이 1280 보다 좁을 때 처음에 접어 둔다 (가운데 뷰어 자리)
        let w = crate::prolayout_ui::LAST_W.load(std::sync::atomic::Ordering::Relaxed);
        let def = !(k == "inspector" && w > 0 && w < 1280);
        self.core.lock().settings.get()["rsProLayout"][k].as_bool().unwrap_or(def)
    }
    fn pro_set(&self, k: &str, on: bool) {
        self.core.lock().settings.update(|s| {
            if !s["rsProLayout"].is_object() {
                s["rsProLayout"] = json!({});
            }
            s["rsProLayout"][k] = json!(on);
        });
    }
    pub(crate) fn pro_side_visible(&self) -> bool {
        self.pro_flag("side")
    }
    pub(crate) fn pro_toggle_side(&mut self) {
        let on = self.pro_side_visible();
        self.pro_set("side", !on);
    }
    /// 왼쪽 판을 펴기만 (Ctrl+B 등 '큐시트로 가기')
    pub(crate) fn pro_show_side(&mut self) {
        if !self.pro_side_visible() {
            self.pro_set("side", true);
        }
    }
    pub(crate) fn pro_inspector_visible(&self) -> bool {
        self.pro_flag("inspector")
    }
    pub(crate) fn pro_toggle_inspector(&mut self) {
        let on = self.pro_inspector_visible();
        self.pro_set("inspector", !on);
    }
    pub(crate) fn pro_timeline_visible(&self) -> bool {
        self.pro_flag("timeline")
    }
    pub(crate) fn pro_toggle_timeline(&mut self) {
        let on = self.pro_timeline_visible();
        self.pro_set("timeline", !on);
    }
}
