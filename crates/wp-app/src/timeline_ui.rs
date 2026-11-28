//! (나) 아래 예배 시간표 — 보기 전용 (시안 docs/design/pro-main.html `.tl`).
//!
//! 실제 있는 자료만 그린다: 큐시트 시작 시각(`start_time`) + 순서마다 길이(`duration`, 분) → 순서 막대,
//! 지금 시각 세로선(재생 헤드), 무대 타이머(지금 돌고 있는 것 + 순서의 '송출할 때 자동으로: 타이머 N분').
//! 시안의 '음악 · OBS 장면' 줄은 예정 자료가 없어 넣지 않았다. 끌어서 길이 바꾸기 같은 조작도 없다.
//! 좁은 창(높이 760 미만)에서는 머리 띠만 보이고, 머리의 단추로 펼친다.
use crate::app::{App, Snap};
use crate::theme::{self, alpha, mix, pal};
use eframe::egui::{self, Align2, Color32, Id, Pos2, Rect, Sense, Stroke, Vec2};
use serde_json::Value;
use wp_core::i18n::{tr, tr_args};

const HEAD_H: f32 = 28.0;
const RULER_H: f32 = 20.0;
const LANE_H: f32 = 46.0;
const LANE2_H: f32 = 32.0;
/// 화면 구성 줄 (로드맵 4단계): 사용자 구성이 있을 때만
const LANE3_H: f32 = 26.0;
const LABEL_W: f32 = 96.0;

/// 순서 하나의 자리 (시작 분, 길이 분)
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Clip {
    pub index: usize,
    pub start: f64,
    pub len: f64,
}

/// 시작 시각 "11:00" + 순서 길이 → 순서마다 (시작 분, 길이 분). 시작 시각이 없으면 None
pub(crate) fn plan(start: &str, items: &[Value]) -> Option<(f64, Vec<Clip>)> {
    let t0 = wp_core::localtime::parse_hm(start.trim())? as f64;
    let mut t = t0;
    let mut out = vec![];
    for (i, it) in items.iter().enumerate() {
        let d = match &it["duration"] {
            Value::String(x) => x.trim().parse::<f64>().unwrap_or(0.0),
            v => v.as_f64().unwrap_or(0.0),
        };
        let d = if d.is_finite() { d.max(0.0) } else { 0.0 };
        out.push(Clip { index: i, start: t, len: d });
        t += d;
    }
    Some((t0, out))
}

/// 'HH:MM' (하루 안 분)
fn hm(min: f64) -> String {
    let m = (min.round() as i64).rem_euclid(1440);
    format!("{:02}:{:02}", m / 60, m % 60)
}

/// 눈금 간격(분): 10분에 50px 이상이면 5분 눈금 · 10분 글자, 아니면 더 성기게
fn tick_step(px_per_min: f32) -> (f64, f64) {
    if px_per_min * 10.0 >= 50.0 { (5.0, 10.0) } else if px_per_min * 20.0 >= 50.0 { (10.0, 20.0) } else { (15.0, 30.0) }
}

/// 막대 안 글: 넘치면 글자 중간이 아니라 끝에 말줄임표 (자리가 아주 좁으면 그리지 않음). 그린 폭
fn bar_text(ui: &egui::Ui, pt: &egui::Painter, pos: Pos2, align: Align2, text: &str, font: egui::FontId, color: Color32, max_w: f32) -> f32 {
    if max_w < 10.0 {
        return 0.0;
    }
    let (g, _) = crate::widgets::elide(ui, text, font, color, max_w);
    let r = align.anchor_size(pos, g.size());
    let w = g.size().x;
    pt.galley(r.min, g, color);
    w
}

/// 화면 구성 줄 (보기 전용): 기본과 다른 구성을 쓰는 순서는 막대, 기본을 바꾸는 순서는 앞에 작은 표시
fn scene_lane(ui: &egui::Ui, pt: &egui::Painter, lane: Rect, clips: &[Clip], xof: &dyn Fn(f64) -> f32, s: &Snap) {
    let p = pal();
    let hover = ui.ctx().pointer_hover_pos();
    let mut tip: Option<String> = None;
    let mut drew = false;
    for c in clips.iter().filter(|c| c.len > 0.0) {
        let x0 = xof(c.start) + 1.0;
        let x1 = (xof(c.start + c.len) - 1.0).max(x0 + 2.0);
        let r = Rect::from_min_max(Pos2::new(x0, lane.top() + 5.0), Pos2::new(x1, lane.bottom() - 5.0));
        let mark = s.scene_marks.get(c.index).cloned().unwrap_or_default();
        let act = s.scene_acts.get(c.index).cloned().unwrap_or_default();
        if !mark.is_empty() {
            pt.rect_filled(r, theme::cr(theme::Rad::Sm), p.raised);
            pt.rect_stroke(r, theme::cr(theme::Rad::Sm), Stroke::new(1.0, p.line2), egui::StrokeKind::Inside);
            // 막대에는 구성 이름 하나만 (출력이 여럿이면 첫 것 + 개수) — 나머지는 툴팁
            let names: Vec<&str> = mark.split(" · ").map(|x| x.split(": ").nth(1).unwrap_or(x)).collect();
            let short = if names.len() > 1 { format!("{} +{}", names[0], names.len() - 1) } else { names.first().copied().unwrap_or("").to_string() };
            let icon_room = r.width() >= 30.0;
            if icon_room {
                crate::icons::paint(ui, Rect::from_center_size(Pos2::new(r.left() + 9.0, r.center().y), Vec2::splat(12.0)), "layout-grid", p.text2);
            }
            let tx = r.left() + if icon_room { 18.0 } else { 4.0 };
            bar_text(ui, pt, Pos2::new(tx, r.center().y), Align2::LEFT_CENTER, &short, theme::Type::Micro.font(), p.text, r.right() - 4.0 - tx);
            drew = true;
        }
        if !act.is_empty() {
            // 기본을 바꾸는 곳: 막대 왼쪽 끝 세로 금
            pt.vline(x0, lane.top() + 3.0..=lane.bottom() - 3.0, Stroke::new(2.0, p.text2));
            drew = true;
        }
        if (!mark.is_empty() || !act.is_empty()) && hover.is_some_and(|h| r.expand2(Vec2::new(3.0, 0.0)).contains(h)) {
            let mut t = vec![];
            if !mark.is_empty() {
                t.push(tr_args("이 순서 동안: {1}", &[&mark]));
            }
            if !act.is_empty() {
                t.push(tr_args("송출할 때 기본을 바꿈: {1}", &[&act]));
            }
            tip = Some(t.join("\n"));
        }
    }
    if !drew {
        pt.text(Pos2::new(lane.left() + LABEL_W + 12.0, lane.center().y), Align2::LEFT_CENTER, tr("모든 순서가 출력 기본 구성을 씁니다"), theme::Type::Micro.font(), p.muted2);
    }
    if let Some(t) = tip {
        ui.interact(lane, Id::new("pro-tl-scene-lane"), Sense::hover()).on_hover_text(t);
    }
}

impl App {
    pub(crate) fn timeline(&mut self, ui: &mut egui::Ui, s: &Snap) {
        let p = pal();
        let (start, live_idx) = {
            let c = self.core.lock();
            let st = c.cue.as_ref().map(|c| c.start_time.clone()).unwrap_or_default();
            // 송출 중 순서가 지금 열린 큐시트의 것일 때만 순서 번호로 본다
            let li = s.live.as_ref().filter(|l| l.deck.passage["cueId"].as_str() == Some(s.cue_id.as_str())).and_then(|l| l.deck.passage["cueIndex"].as_i64()).filter(|i| *i >= 0).map(|i| i as usize);
            (st, li)
        };
        let next_idx = s.preview.as_ref().filter(|d| d.passage["cueId"].as_str() == Some(s.cue_id.as_str())).and_then(|d| d.passage["cueIndex"].as_i64()).filter(|i| *i >= 0).map(|i| i as usize).filter(|i| Some(*i) != live_idx).or_else(|| live_idx.map(|i| i + 1).filter(|i| *i < s.items.len()));
        let planned = plan(&start, &s.items);
        let total: f64 = planned.as_ref().map(|(_, c)| c.iter().map(|x| x.len).sum()).unwrap_or(0.0);
        let now_min = (crate::probar_ui::local_ms().rem_euclid(86_400_000)) as f64 / 60_000.0;
        // 늦음/빠름: 송출 중 순서의 예정 시작 ↔ 실제로 송출을 시작한 때
        let delay = match (&planned, live_idx) {
            (Some((_, clips)), Some(i)) if clips.get(i).is_some_and(|c| c.len > 0.0 || total > 0.0) => {
                let at = ui.ctx().data(|d| d.get_temp::<(String, i64)>(Id::new("pro-live-start"))).map(|x| x.1).unwrap_or(0);
                if at > 0 {
                    let started = ((at + (crate::probar_ui::local_ms() - wp_core::util::now_ms())).rem_euclid(86_400_000)) as f64 / 60_000.0;
                    let d = started - clips[i].start;
                    (d.abs() < 180.0).then_some(d)
                } else {
                    None
                }
            }
            _ => None,
        };
        // 높이: 창이 낮으면 머리만 (펼침 단추로 바꿈 — 이 실행 동안만 기억)
        let small = ui.ctx().content_rect().height() < 760.0;
        let open_id = Id::new("pro-tl-open");
        let mut open = ui.data(|d| d.get_temp::<bool>(open_id)).unwrap_or(!small);
        // ---- 머리 띠
        let (hr, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), HEAD_H), Sense::hover());
        ui.painter().rect_filled(hr, 0.0, p.head);
        ui.painter().hline(hr.x_range(), hr.bottom() - 0.5, Stroke::new(1.0, p.edge));
        let mut x = hr.left() + theme::sp::PANE;
        let g = ui.painter().layout_no_wrap(tr("예배 시간표"), theme::Type::BodyStrong.font(), p.text);
        let gw = g.size().x;
        ui.painter().galley(Pos2::new(x, hr.center().y - g.size().y / 2.0), g, p.text);
        x += gw + 10.0;
        let sub = match &planned {
            Some((t0, _)) if total > 0.0 => tr_args("{1} – {2} · 계획대로면 {3} 마침", &[&hm(*t0), &hm(t0 + total), &hm(t0 + total)]),
            Some(_) => tr("순서마다 길이(분)를 넣으면 막대로 보입니다"),
            None if s.items.is_empty() => tr("큐시트가 비어 있습니다"),
            None => tr("큐시트에 시작 시각을 넣으면 예정 시각이 보입니다 (큐시트 관리 · 편집칸)"),
        };
        bar_text(ui, ui.painter(), Pos2::new(x, hr.center().y), Align2::LEFT_CENTER, &sub, theme::Type::Small.font(), p.muted, (hr.right() - 200.0 - x).max(0.0));
        // 오른쪽: 늦음 · 접기 단추
        let fold_r = Rect::from_center_size(Pos2::new(hr.right() - 22.0, hr.center().y), Vec2::new(28.0, 22.0));
        let fr = ui.interact(fold_r, Id::new("pro-tl-fold"), Sense::click()).on_hover_text(if open { tr("시간표 접기") } else { tr("시간표 펼치기") });
        if fr.hovered() {
            ui.painter().rect_filled(fold_r, theme::cr(theme::Rad::Sm), p.hover);
        }
        crate::icons::paint(ui, Rect::from_center_size(fold_r.center(), Vec2::splat(15.0)), if open { "chevron-down" } else { "chevron-up" }, p.text2);
        crate::widgets::mark("pro-tl-fold", fold_r);
        if fr.clicked() {
            open = !open;
            ui.data_mut(|d| d.insert_temp(open_id, open));
        }
        if let Some(d) = delay {
            let m = d.round() as i64;
            let (t, c) = if m > 0 { (tr_args("지금 {1}분 늦음", &[&m]), p.warn) } else if m < 0 { (tr_args("지금 {1}분 빠름", &[&(-m)]), p.ok_text) } else { (tr("계획대로"), p.muted) };
            ui.painter().text(Pos2::new(fold_r.left() - 10.0, hr.center().y), Align2::RIGHT_CENTER, t, theme::Type::SmallStrong.font(), c);
        }
        if !open {
            return;
        }
        // ---- 몸통: 눈금 · 순서 줄 · 무대 줄
        let has_scenes = !s.scene_marks.is_empty();
        let body_h = RULER_H + LANE_H + LANE2_H + if has_scenes { LANE3_H } else { 0.0 };
        let (br, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), body_h), Sense::hover());
        let pt = ui.painter().with_clip_rect(br);
        let track = Rect::from_min_max(Pos2::new(br.left() + LABEL_W, br.top()), br.max);
        let lane1 = Rect::from_min_size(Pos2::new(br.left(), br.top() + RULER_H), Vec2::new(br.width(), LANE_H));
        let lane2 = Rect::from_min_size(Pos2::new(br.left(), lane1.bottom()), Vec2::new(br.width(), LANE2_H));
        let lane3 = Rect::from_min_size(Pos2::new(br.left(), lane2.bottom()), Vec2::new(br.width(), if has_scenes { LANE3_H } else { 0.0 }));
        let mut lanes = vec![(lane1, tr("순서"), tr("예배당")), (lane2, tr("무대"), tr("타이머"))];
        if has_scenes {
            lanes.push((lane3, tr("구성"), String::new()));
        }
        // 이름 칸
        for (r, a, b) in lanes {
            let lr = Rect::from_min_size(r.min, Vec2::new(LABEL_W, r.height()));
            pt.rect_filled(lr, 0.0, mix(p.head, 60.0, p.panel));
            pt.vline(lr.right() - 0.5, lr.y_range(), Stroke::new(1.0, p.edge));
            // 이름표: 판 왼쪽 여백 12 에 맞춤. 높은 줄은 두 줄, 낮은 줄은 한 줄로 가운데
            let x0 = lr.left() + theme::sp::PANE;
            if r.height() > 36.0 {
                pt.text(Pos2::new(x0, r.top() + 7.0), Align2::LEFT_TOP, a, theme::Type::SmallStrong.font(), p.text2);
                pt.text(Pos2::new(x0, r.top() + 23.0), Align2::LEFT_TOP, b, theme::Type::Micro.font(), p.muted);
            } else {
                let w = pt.text(Pos2::new(x0, r.center().y), Align2::LEFT_CENTER, a, theme::Type::SmallStrong.font(), p.text2).width();
                pt.text(Pos2::new(x0 + w + theme::sp::X6, r.center().y), Align2::LEFT_CENTER, b, theme::Type::Micro.font(), p.muted);
            }
            pt.hline(r.x_range(), r.bottom() - 0.5, Stroke::new(1.0, p.line_soft));
        }
        let ruler_lr = Rect::from_min_size(br.min, Vec2::new(LABEL_W, RULER_H));
        pt.rect_filled(ruler_lr, 0.0, mix(p.head, 60.0, p.panel));
        pt.hline(br.x_range(), br.top() + RULER_H - 0.5, Stroke::new(1.0, p.line));
        let Some((t0, clips)) = planned.filter(|_| total > 0.0) else {
            // 자료가 없으면 안내만 (막대를 지어내지 않는다)
            let msg = if s.items.is_empty() { tr("큐시트를 열면 순서가 여기에 시간 막대로 놓입니다.") } else if start.trim().is_empty() { tr("큐시트 시작 시각(예: 11:00)과 순서마다 길이(분)를 넣으면 예배 시간표가 그려집니다.") } else { tr("순서마다 길이(분)를 넣으면 예배 시간표가 그려집니다. (편집칸 '길이')") };
            bar_text(ui, &pt, Pos2::new(track.left() + 12.0, lane1.center().y), Align2::LEFT_CENTER, &msg, theme::Type::Small.font(), p.muted, track.width() - 24.0);
            self.timer_lane(ui, &pt, lane2, track, None, now_min);
            if has_scenes {
                pt.text(Pos2::new(track.left() + 12.0, lane3.center().y), Align2::LEFT_CENTER, tr("순서 길이를 넣으면 기본과 다른 화면 구성이 여기에 보입니다."), theme::Type::Micro.font(), p.muted2);
            }
            return;
        };
        // 범위: 시작 ~ 마침 (지금이 조금 넘으면 지금까지)
        let t1 = (t0 + total).max(t0 + 10.0);
        let span = t1 - t0;
        let ppm = (track.width() - 8.0) / span as f32;
        let xof = |m: f64| track.left() + 2.0 + ((m - t0) as f32) * ppm;
        let (minor, major) = tick_step(ppm);
        let mut m = (t0 / minor).ceil() * minor;
        while m <= t1 + 0.01 {
            let x = xof(m);
            pt.vline(x, (br.top() + RULER_H - 6.0)..=(br.top() + RULER_H), Stroke::new(1.0, p.line2));
            if ((m / major).round() * major - m).abs() < 0.01 {
                crate::widgets::tnum(ui, Pos2::new(x + 3.0, br.top() + 3.0), Align2::LEFT_TOP, &hm(m), theme::Type::Micro.font(), p.muted2);
            }
            m += minor;
        }
        // 순서 막대
        let hover_pos = ui.ctx().pointer_hover_pos();
        let mut tip: Option<String> = None;
        for c in clips.iter().filter(|c| c.len > 0.0) {
            let x0 = xof(c.start) + 1.0;
            let x1 = xof(c.start + c.len) - 1.0;
            let r = Rect::from_min_max(Pos2::new(x0, lane1.top() + 5.0), Pos2::new(x1.max(x0 + 2.0), lane1.bottom() - 5.0));
            let done = live_idx.is_some_and(|l| c.index < l);
            let live = live_idx == Some(c.index);
            let next = next_idx == Some(c.index);
            // 어두운 테마는 흰 글, 밝은 테마는 진한 글 (막대 바탕이 옅어서)
            let strong = if p.dark { Color32::WHITE } else { p.text };
            let (bg, fg, sub_c) = if live {
                (mix(p.live, if p.dark { 55.0 } else { 25.0 }, p.panel), strong, alpha(strong, 0.75))
            } else if next {
                (mix(p.next, 30.0, p.panel), strong, alpha(strong, 0.75))
            } else if done {
                (mix(p.raised, 55.0, p.panel), p.muted2, p.muted2)
            } else {
                (p.raised, p.text, p.muted)
            };
            pt.rect_filled(r, theme::cr(theme::Rad::Sm), bg);
            if live {
                pt.rect_stroke(r, theme::cr(theme::Rad::Sm), Stroke::new(2.0, p.live), egui::StrokeKind::Inside);
            } else if next {
                pt.rect_stroke(r, theme::cr(theme::Rad::Sm), Stroke::new(2.0, p.next), egui::StrokeKind::Inside);
            } else if !done {
                pt.hline(r.x_range().shrink(2.0), r.top() + 0.5, Stroke::new(1.0, alpha(Color32::WHITE, 0.10)));
            }
            let it = &s.items[c.index];
            let name = it["title"].as_str().filter(|x| !x.is_empty()).map(String::from).unwrap_or_else(|| tr(crate::center_ui::type_name(it["type"].as_str().unwrap_or(""))));
            let detail = crate::cue_ui::detail(it);
            // 출력마다 다른 화면 구성 (로드맵 3단계): 막대 오른쪽 위 작은 격자 아이콘 — 글은 그 자리를 빼고 잰다
            let mark = s.scene_marks.get(c.index).filter(|m| !m.is_empty());
            let icon = mark.is_some() && r.width() > 60.0;
            if icon {
                crate::icons::paint(ui, Rect::from_center_size(Pos2::new(r.right() - 10.0, r.top() + 11.0), Vec2::splat(12.0)), "layout-grid", sub_c);
            }
            let room = r.width() - 12.0 - if icon { 16.0 } else { 0.0 };
            // 좁은 막대는 번호만, 번호도 안 들어가면 비움 (글자 중간에서 잘리지 않게)
            let num = format!("{}", c.index + 1);
            let num_w = crate::widgets::Tnum::new(ui, &num, theme::Type::Small.font(), fg).size.x;
            if room >= num_w + 34.0 {
                bar_text(ui, &pt, Pos2::new(r.left() + 6.0, r.top() + 5.0), Align2::LEFT_TOP, &format!("{num} {name}"), theme::Type::Small.font(), fg, room);
                if !detail.is_empty() && room >= 40.0 {
                    bar_text(ui, &pt, Pos2::new(r.left() + 6.0, r.top() + 20.0), Align2::LEFT_TOP, &detail, theme::Type::Micro.font(), sub_c, r.width() - 12.0);
                }
            } else if room >= num_w {
                crate::widgets::tnum(ui, Pos2::new(r.left() + 6.0, r.top() + 5.0), Align2::LEFT_TOP, &num, theme::Type::Small.font(), fg);
            }
            if hover_pos.is_some_and(|hp| r.contains(hp)) {
                let sc = mark.map(|m| format!("\n{}: {m}", tr("화면 구성"))).unwrap_or_default();
                tip = Some(format!("{} {} · {} – {} ({}분){}{sc}", c.index + 1, name, hm(c.start), hm(c.start + c.len), c.len, if detail.is_empty() { String::new() } else { format!("\n{detail}") }));
            }
        }
        if let Some(t) = tip {
            ui.interact(lane1, Id::new("pro-tl-lane"), Sense::hover()).on_hover_text(t);
        }
        self.timer_lane(ui, &pt, lane2, track, Some((&clips, t0, ppm, s)), now_min);
        if has_scenes {
            scene_lane(ui, &pt, lane3, &clips, &xof, s);
        }
        // 지금 시각 세로선 (재생 헤드) — 범위 안일 때만
        if now_min >= t0 && now_min <= t1 {
            let x = xof(now_min);
            let head_c = if p.dark { Color32::WHITE } else { p.text };
            pt.vline(x, (br.top() + RULER_H - 2.0)..=br.bottom(), Stroke::new(1.0, head_c));
            pt.add(egui::Shape::convex_polygon(vec![Pos2::new(x - 5.0, br.top() + RULER_H - 8.0), Pos2::new(x + 5.0, br.top() + RULER_H - 8.0), Pos2::new(x, br.top() + RULER_H - 1.0)], head_c, Stroke::NONE));
            ui.ctx().request_repaint_after(std::time::Duration::from_secs(5));
        }
    }

    /// 무대 타이머 줄: 순서의 '송출할 때 자동으로: 타이머 N분'(예정) + 지금 돌고 있는 타이머
    fn timer_lane(&self, ui: &egui::Ui, pt: &egui::Painter, lane: Rect, track: Rect, plan: Option<(&[Clip], f64, f32, &Snap)>, now_min: f64) {
        let p = pal();
        let timer = self.core.lock().b.stage.timer.clone();
        let mut drew = false;
        if let Some((clips, t0, ppm, s)) = plan {
            let xof = |m: f64| track.left() + 2.0 + ((m - t0) as f32) * ppm;
            for c in clips {
                let mins = s.items[c.index]["actions"]["timer"].as_f64().unwrap_or(0.0);
                if mins <= 0.0 {
                    continue;
                }
                let r = Rect::from_min_max(Pos2::new(xof(c.start) + 1.0, lane.top() + 5.0), Pos2::new((xof(c.start + mins) - 1.0).max(xof(c.start) + 3.0), lane.bottom() - 5.0));
                pt.rect_filled(r, theme::cr(theme::Rad::Sm), p.raised);
                bar_text(ui, pt, Pos2::new(r.left() + 6.0, r.center().y), Align2::LEFT_CENTER, &tr_args("타이머 {1}:00", &[&(mins as i64)]), theme::Type::Micro.font(), p.text, r.width() - 12.0);
                drew = true;
            }
            // 지금 돌고 있는 타이머 (예정과 따로): 밝은 테두리 + 밝은 글 — 노랑은 '다음' 에만, 빨강은 송출 중에만
            let tc = p.text;
            if timer.running || timer.remaining > 0 {
                let left_ms = if timer.running { timer.end_at - wp_core::util::now_ms() } else { timer.remaining };
                let start = now_min - (timer.duration - left_ms).max(0) as f64 / 60_000.0;
                let end = now_min + left_ms.max(0) as f64 / 60_000.0;
                let sec = (left_ms.max(0) / 1000) as i64;
                let label = format!("{} {:02}:{:02}{}", tr("무대 타이머"), sec / 60, sec % 60, if timer.running { "" } else { " (멈춤)" });
                let (x0, x1) = (xof(start).max(track.left()), xof(end).min(track.right()));
                if x1 - x0 >= 40.0 {
                    let r = Rect::from_min_max(Pos2::new(x0, lane.top() + 8.0), Pos2::new(x1, lane.bottom() - 8.0));
                    crate::widgets::mark("timeline-timer", r);
                    pt.rect_stroke(r, theme::cr(theme::Rad::Sm), Stroke::new(1.5, tc), egui::StrokeKind::Inside);
                    bar_text(ui, pt, Pos2::new(r.left() + 6.0, r.center().y), Align2::LEFT_CENTER, &label, theme::Type::Micro.font(), tc, r.width() - 12.0);
                } else {
                    // 지금 시각이 시간표 범위 밖(예배 전 · 뒤)이거나 너무 짧아 칸이 안 보이면: 줄 끝 쪽에 글로 (예전에는 칸이 화면 밖에 그려져 줄이 비어 보였다)
                    let past = xof(now_min) > track.right();
                    let (pos, align) = if past { (Pos2::new(track.right() - 10.0, lane.center().y), Align2::RIGHT_CENTER) } else { (Pos2::new(track.left() + 12.0, lane.center().y), Align2::LEFT_CENTER) };
                    let g = pt.layout_no_wrap(label, theme::Type::Micro.font(), tc);
                    let rr = align.anchor_size(pos, g.size()).expand2(Vec2::new(6.0, 3.0));
                    pt.rect_filled(rr, theme::cr(theme::Rad::Sm), p.panel);
                    pt.rect_stroke(rr, theme::cr(theme::Rad::Sm), Stroke::new(1.5, tc), egui::StrokeKind::Inside);
                    pt.galley(rr.min + Vec2::new(6.0, 3.0), g, tc);
                    crate::widgets::mark("timeline-timer", rr);
                }
                ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
                drew = true;
            }
        } else if timer.running || timer.remaining > 0 {
            let left_ms = if timer.running { timer.end_at - wp_core::util::now_ms() } else { timer.remaining };
            let sec = (left_ms.max(0) / 1000) as i64;
            pt.text(Pos2::new(track.left() + 12.0, lane.center().y), Align2::LEFT_CENTER, format!("{} {:02}:{:02}", tr("무대 타이머"), sec / 60, sec % 60), theme::Type::SmallStrong.font(), p.text);
            ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
            drew = true;
        }
        if !drew {
            pt.text(Pos2::new(track.left() + 12.0, lane.center().y), Align2::LEFT_CENTER, tr("무대 타이머 없음"), theme::Type::Small.font(), p.muted2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn plan_from_start_and_durations() {
        let items = vec![json!({"duration": 10}), json!({"duration": "2"}), json!({}), json!({"duration": 5})];
        let (t0, c) = plan("11:00", &items).unwrap();
        assert_eq!(t0, 660.0);
        assert_eq!(c.iter().map(|x| (x.start, x.len)).collect::<Vec<_>>(), vec![(660.0, 10.0), (670.0, 2.0), (672.0, 0.0), (672.0, 5.0)]);
        assert!(plan("", &items).is_none());
        assert_eq!(hm(1445.0), "00:05");
    }

    #[test]
    fn ticks_follow_width() {
        assert_eq!(tick_step(23.0), (5.0, 10.0));
        assert_eq!(tick_step(3.0), (10.0, 20.0));
        assert_eq!(tick_step(1.0), (15.0, 30.0));
    }
}
