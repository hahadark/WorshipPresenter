//! 화면 구성 편집 창 (로드맵 2단계, docs/roadmap-sources.md) — 템플릿 편집 창처럼 따로 뜨는 창.
//! 왼쪽: 구성 목록 (쓰는 곳 표시 · 새로(기본 틀 5개) · 복제 · 지우기)
//! 가운데: 캔버스 (출력과 같은 그림 · 칸 끌기 · 모서리 끌기 · 격자/가장자리 붙기 · 누르면 고르기) + 틀 바꾸기 · 쓰는 곳
//! 오른쪽: 인스펙터 (칸 목록 · 보이기 · 잠그기 · 겹침 순서 · 소스 · 글 모양 · 자리 · 자르기 · 투명도 · 둥글기 · 테두리)
//! 바꾸면 잠깐 뒤 저절로 저장(코어 save_scene → 출력에 바로). 실행 취소 Ctrl+Z · 다시 Ctrl+Shift+Z / Ctrl+Y.
use crate::paint;
use crate::theme::{self, pal, Weight};
use crate::widgets::{self, Btn, Size};
use eframe::egui::{self, Align2, Color32, CursorIcon, Id, Key, KeyboardShortcut, Modifiers, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use serde_json::{json, Value};
use wp_core::core::{Core, Shared};
use wp_core::i18n::{tr, tr_args};
use wp_core::scenes;

/// 자동 저장까지 기다리는 시간 (초)
const SAVE_DELAY: f64 = 0.35;
/// 가장자리 · 가운데 · 다른 칸에 붙는 거리 (화면 px)
const SNAP_PX: f32 = 8.0;
/// 격자 칸 수 (가로 · 세로 같은 비율)
const GRID: f64 = 24.0;
/// 출력 (설정 outputScenes 열쇠, 이름)
/// 출력 목록 (id, 보일 이름): 예배당 · 방송 (OBS) · 추가 출력 (로드맵 3단계)
fn outs(c: &Core) -> Vec<(String, String)> {
    c.output_list().into_iter().map(|(k, n)| { let n = if k == "main" || k == "overlay" { tr(&n) } else { n }; (k, n) }).collect()
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum DragKind {
    Move,
    /// 움직이는 변: 왼 1 · 오른 2 · 위 4 · 아래 8
    Resize(u8),
}

#[derive(Clone, Debug, PartialEq)]
struct Drag {
    layer: String,
    kind: DragKind,
    start: [f64; 4],
    at: Pos2,
}

#[derive(Default)]
pub struct SceneUi {
    pub open: bool,
    /// 고치는 구성 id ("" = 없음)
    sel: String,
    draft: Value,
    undo: Vec<Value>,
    redo: Vec<Value>,
    gesture: Option<Value>,
    changed_at: Option<f64>,
    /// 고른 칸 id
    layer: Option<String>,
    drag: Option<Drag>,
    /// 이번 프레임에 붙은 안내선 (세로선이면 true, 화면 비율 자리)
    guides: Vec<(bool, f64)>,
    grid: bool,
    grid_set: bool,
    /// 캔버스에서 볼 출력 (main · overlay)
    view: String,
    msg: String,
    win_rect: Option<Rect>,
    /// 다음에 열 때 고를 구성
    want: Option<String>,
    /// 마지막 캔버스 자리 (headless 시험이 이 좌표로 끈다)
    pub(crate) canvas_seen: Option<Rect>,
    /// Alt+누름으로 아래 칸을 골랐음 → 다음 끌기는 맨 위 칸 대신 고른 칸을 옮긴다 (보통 누름이면 풀림)
    alt_pick: bool,
}

/// 칸 자리 표시 · 인스펙터에 쓸 소스 이름 (camera:… · src-… · overlay:… → 이름)
pub fn source_names(c: &Core) -> Vec<(String, String)> {
    c.sources_status()
        .into_iter()
        .map(|r| {
            let n = if r.user {
                r.name.clone()
            } else if r.kind == "camera" || r.kind == "phone" {
                format!("{} · {}", tr(&r.name), r.detail)
            } else if r.id.starts_with("overlay:") && !r.detail.is_empty() {
                format!("{} · {}", tr(&r.name), r.detail)
            } else {
                tr(&r.name)
            };
            (r.id, n)
        })
        .collect()
}

/// 칸에 넣을 수 있는 소스 (id, 이름)
fn choices(c: &Core) -> Vec<(String, String)> {
    let mut v = vec![(String::new(), tr("빈 칸"))];
    for (id, n) in source_names(c) {
        if matches!(id.as_str(), "draw") {
            continue;
        }
        v.push((id, n));
    }
    v
}

/// 처음 넣을 카메라 (장치 카메라 먼저, 없으면 휴대폰)
fn first_camera(c: &Core) -> Option<String> {
    let rows = c.sources_status();
    rows.iter().find(|r| r.kind == "camera" && !r.id.ends_with(':')).or(rows.iter().find(|r| r.kind == "phone")).map(|r| r.id.clone())
}

fn sample() -> Value {
    json!({ "kind": "bible", "ref": "요한복음 3:16", "title": "요한복음 3:16", "parts": [{ "book": 43, "chapter": 3, "verse": 16, "text": "하나님이 세상을 이처럼 사랑하사 독생자를 주셨으니 이는 그를 믿는 자마다 멸망하지 않고 영생을 얻게 하려 하심이라" }] })
}

/// 붙기: v 가 후보 가운데 thr 안에 있으면 가장 가까운 것으로 (붙은 후보도 돌려줌)
fn snap1(v: f64, cands: &[f64], thr: f64) -> Option<f64> {
    cands.iter().copied().filter(|c| (c - v).abs() <= thr).min_by(|a, b| (a - v).abs().total_cmp(&(b - v).abs()))
}

/// 끌기 계산 (시험할 수 있게 화면과 떼어 둠): 시작 자리 + 움직인 양(화면 비율) → 새 자리 · 붙은 안내선.
/// others = 다른 칸 자리, thr = 붙는 거리(화면 비율, [가로, 세로]), grid = 격자 칸 수 (0 이면 없음), keep = 비율 지키기 (모서리 + Shift)
/// Alt+누름: 누른 자리에 겹친 칸들(위 → 아래) 가운데 지금 고른 칸 바로 아래 칸. 맨 아래면 다시 맨 위로, 고른 칸이 없으면 맨 위
pub(crate) fn cycle_pick(under: &[String], cur: Option<&str>) -> Option<String> {
    if under.is_empty() {
        return None;
    }
    match cur.and_then(|c| under.iter().position(|x| x == c)) {
        Some(i) => Some(under[(i + 1) % under.len()].clone()),
        None => Some(under[0].clone()),
    }
}

pub(crate) fn drag_rect(start: [f64; 4], kind: u8, d: [f64; 2], others: &[[f64; 4]], thr: [f64; 2], grid: f64, keep: bool) -> ([f64; 4], Vec<(bool, f64)>) {
    let mut guides = vec![];
    let cand = |vert: bool| -> Vec<f64> {
        let mut c = vec![0.0, 0.5, 1.0];
        for o in others {
            if vert {
                c.extend([o[0], o[0] + o[2], o[0] + o[2] / 2.0]);
            } else {
                c.extend([o[1], o[1] + o[3], o[1] + o[3] / 2.0]);
            }
        }
        if grid > 0.0 {
            c.extend((0..=grid as usize).map(|i| i as f64 / grid));
        }
        c
    };
    let (cx, cy) = (cand(true), cand(false));
    let [x0, y0, w0, h0] = start;
    if kind == 0 {
        // 옮기기: 왼 · 가운데 · 오른 가운데 가장 가까이 붙는 것
        let mut x = x0 + d[0];
        let mut y = y0 + d[1];
        if thr[0] > 0.0 {
            let best = [(x, 0.0), (x + w0 / 2.0, w0 / 2.0), (x + w0, w0)].into_iter().filter_map(|(e, off)| snap1(e, &cx, thr[0]).map(|s| (s, off, (s - e).abs()))).min_by(|a, b| a.2.total_cmp(&b.2));
            if let Some((s, off, _)) = best {
                x = s - off;
                guides.push((true, s));
            }
            let best = [(y, 0.0), (y + h0 / 2.0, h0 / 2.0), (y + h0, h0)].into_iter().filter_map(|(e, off)| snap1(e, &cy, thr[1]).map(|s| (s, off, (s - e).abs()))).min_by(|a, b| a.2.total_cmp(&b.2));
            if let Some((s, off, _)) = best {
                y = s - off;
                guides.push((false, s));
            }
        }
        let r = scenes::clamp_rect([x, y, w0, h0]);
        return (r, guides);
    }
    // 크기 바꾸기: 움직이는 변만
    let (mut l, mut t, mut r, mut b) = (x0, y0, x0 + w0, y0 + h0);
    let min = scenes::MIN_SIZE;
    let mut edge = |v: f64, vert: bool| -> f64 {
        let c = if vert { &cx } else { &cy };
        let th = if vert { thr[0] } else { thr[1] };
        if th > 0.0 {
            if let Some(s) = snap1(v, c, th) {
                guides.push((vert, s));
                return s;
            }
        }
        v
    };
    if kind & 1 != 0 {
        l = edge((x0 + d[0]).clamp(0.0, r - min), true);
    }
    if kind & 2 != 0 {
        r = edge((x0 + w0 + d[0]).clamp(l + min, 1.0), true);
    }
    if kind & 4 != 0 {
        t = edge((y0 + d[1]).clamp(0.0, b - min), false);
    }
    if kind & 8 != 0 {
        b = edge((y0 + h0 + d[1]).clamp(t + min, 1.0), false);
    }
    let mut out = [l, t, (r - l).max(min), (b - t).max(min)];
    // 모서리 + Shift: 처음 가로:세로 비율 그대로 (폭 기준)
    if keep && (kind & 3 != 0) && (kind & 12 != 0) && w0 > 0.0 {
        let h = out[2] * h0 / w0;
        if kind & 4 != 0 {
            out[1] = b - h;
        }
        out[3] = h;
        guides.clear();
    }
    (scenes::clamp_rect(out), guides)
}

impl SceneUi {
    /// 창이 마우스 아래에 있으면 실행 취소는 이 창 것 (큐시트 실행 취소 대신)
    pub fn owns_undo(&self, ctx: &egui::Context) -> bool {
        self.open && self.win_rect.zip(ctx.pointer_latest_pos()).is_some_and(|(r, p)| r.contains(p))
    }

    fn load(&mut self, s: &Value) {
        self.sel = s["id"].as_str().unwrap_or("").to_string();
        self.draft = s.clone();
        self.undo.clear();
        self.redo.clear();
        self.gesture = None;
        self.changed_at = None;
        self.drag = None;
        self.layer = s["layers"].as_array().and_then(|a| a.iter().rev().find(|l| l["source"] == "live").or(a.last())).and_then(|l| l["id"].as_str()).map(String::from);
    }

    fn flush(&mut self, core: &Shared) {
        if self.changed_at.take().is_none() || self.draft.is_null() {
            return;
        }
        let r = core.lock().save_scene(&self.draft);
        match r {
            Ok(n) => {
                // 정리된 값(칸 id 등)으로 — 고르기 · 실행 취소가 어긋나지 않게 id 는 그대로
                if n["id"] == self.draft["id"] {
                    self.draft = n;
                }
                self.msg.clear();
            }
            Err(e) => self.msg = e.to_string(),
        }
    }

    fn apply_history(&mut self, undo: bool) {
        let (from, to) = if undo { (&mut self.undo, &mut self.redo) } else { (&mut self.redo, &mut self.undo) };
        if let Some(v) = from.pop() {
            to.push(std::mem::replace(&mut self.draft, v));
            self.gesture = None;
            self.changed_at = Some(0.0);
        }
    }

    fn layers(&self) -> Vec<Value> {
        self.draft["layers"].as_array().cloned().unwrap_or_default()
    }

    fn layer_idx(&self, id: &str) -> Option<usize> {
        self.draft["layers"].as_array().and_then(|a| a.iter().position(|l| l["id"] == id))
    }

    fn sel_layer_mut(&mut self) -> Option<&mut Value> {
        let id = self.layer.clone()?;
        self.draft["layers"].as_array_mut()?.iter_mut().find(|l| l["id"] == id.as_str())
    }

    /// 메뉴 '화면 구성 편집' · 소스 탭 단추: 열고 (있으면) 그 구성을
    pub fn open_with(&mut self, id: &str) {
        self.open = true;
        if !id.is_empty() && id != scenes::FULL && id != self.sel {
            self.sel = String::new();
            self.draft = Value::Null;
            self.want = Some(id.to_string());
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, core: &Shared, web_dir: &std::path::Path, data_dir: &std::path::Path) {
        // 점검용: --scene-open [기본 틀] — 창을 열고, 틀 이름을 주면 그 틀로 새 구성을 하나 만든다
        static ARG: std::sync::Once = std::sync::Once::new();
        ARG.call_once(|| {
            if let Some(i) = std::env::args().position(|x| x == "--scene-open") {
                self.open = true;
                if let Some(k) = std::env::args().nth(i + 1).filter(|k| scenes::PRESETS.iter().any(|(p, _)| p == k)) {
                    let cam = first_camera(&core.lock());
                    let n = scenes::new_scene(&k, "", cam.as_deref());
                    if let Ok(saved) = core.lock().save_scene(&n) {
                        self.load(&saved);
                    }
                }
            }
        });
        if !self.open {
            if self.changed_at.is_some() {
                self.flush(core);
            }
            self.win_rect = None;
            self.drag = None;
            return;
        }
        if !self.grid_set {
            self.grid = true;
            self.grid_set = true;
        }
        if self.view.is_empty() {
            self.view = "main".into();
        }
        let list = core.lock().scenes();
        if let Some(w) = self.want.take() {
            if let Some(s) = list.iter().find(|s| s["id"] == w.as_str()) {
                self.load(&s.clone());
            }
        }
        // 고친 구성이 사라졌으면(다른 PC 동기화 · 지우기) 비움
        if !self.sel.is_empty() && !list.iter().any(|s| s["id"] == self.sel.as_str()) && self.changed_at.is_none() {
            self.sel.clear();
            self.draft = Value::Null;
        }
        if self.draft.is_null() {
            if let Some(s) = list.first() {
                self.load(&s.clone());
            }
        }
        let t_now = ctx.input(|i| i.time);
        let pc = paint::Ctx { web_dir, data_dir };
        let before = self.draft.clone();
        let mut open = self.open;
        let screen = ctx.content_rect();
        let size = Vec2::new((screen.width() - 40.0).clamp(320.0, 1480.0), (screen.height() - widgets::FLOAT_TOP - 48.0).clamp(180.0, 860.0));
        let win = crate::widgets_win::window(ctx, "scene-editor", tr("화면 구성"), [size.x, size.y]).open(&mut open).collapsible(false).vscroll(false).show(ctx, |ui| {
            if ui.available_width() < 1050.0 {
                let tab = crate::widgets_win::editor_tabs(ui, "scene", &[tr("화면 구성"), tr("미리보기"), tr("칸 속성")], None);
                egui::ScrollArea::vertical().id_salt(("scene-compact", tab)).auto_shrink([false, false]).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    match tab {
                        0 => self.list_column(ui, core, &list),
                        1 if self.draft.is_null() => self.empty_center(ui, core),
                        1 => self.center_column(ui, core, &pc),
                        _ if !self.draft.is_null() => self.inspector(ui, core),
                        _ => self.empty_center(ui, core),
                    }
                });
                return;
            }
            let h = ui.available_height().max(180.0);
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 14.0;
                ui.allocate_ui_with_layout(Vec2::new(220.0, h), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_min_size(Vec2::new(220.0, h));
                    self.list_column(ui, core, &list);
                });
                let cw = (ui.available_width() - 330.0 - 14.0).max(360.0);
                ui.allocate_ui_with_layout(Vec2::new(cw, h), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_min_size(Vec2::new(cw, h));
                    if self.draft.is_null() {
                        self.empty_center(ui, core);
                    } else {
                        self.center_column(ui, core, &pc);
                    }
                });
                ui.allocate_ui_with_layout(Vec2::new(330.0, h), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_min_size(Vec2::new(320.0, h));
                    if !self.draft.is_null() {
                        egui::ScrollArea::vertical().id_salt("scene-inspect").auto_shrink([false, false]).show(ui, |ui| {
                            ui.set_width(318.0);
                            self.inspector(ui, core);
                        });
                    }
                });
            });
        });
        self.win_rect = win.as_ref().map(|w| w.response.rect);
        self.open = open;
        // 바뀐 것: 실행 취소 기록 · 자동 저장
        let any_down = ctx.input(|i| i.pointer.any_down());
        if self.draft != before && !before.is_null() && before["id"] == self.draft["id"] {
            if self.gesture.is_none() {
                self.gesture = Some(before);
            }
            self.changed_at = Some(t_now);
            self.redo.clear();
        }
        if !any_down {
            if let Some(g) = self.gesture.take() {
                if g != self.draft {
                    self.undo.push(g);
                    if self.undo.len() > 200 {
                        self.undo.remove(0);
                    }
                }
            }
        }
        if self.owns_undo(ctx) && !ctx.egui_wants_keyboard_input() {
            let redo = ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z)) || i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Y)));
            if redo {
                self.apply_history(false);
            } else if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Z))) {
                self.apply_history(true);
            }
        }
        if let Some(t0) = self.changed_at {
            if !any_down && t_now - t0 >= SAVE_DELAY {
                self.flush(core);
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(120));
            }
        }
        if !self.open {
            self.flush(core);
        }
    }

    /// 왼쪽: 구성 목록
    fn list_column(&mut self, ui: &mut egui::Ui, core: &Shared, list: &[Value]) {
        let p = pal();
        let used: Vec<(String, String, String)> = {
            let c = core.lock();
            outs(&c).into_iter().map(|(o, n)| { let u = c.output_scene(&o); (o, n, u) }).collect()
        };
        ui.label(theme::rich(tr("화면 구성"), 13.0, Weight::SemiBold, p.muted));
        ui.add_space(4.0);
        let mut pick: Option<Value> = None;
        let row_h = 46.0;
        // 기본 구성 (고칠 수 없음 — 쓰는 곳 표시만)
        let base_row = |ui: &mut egui::Ui, name: &str, sub: &str, on: bool, uses: Vec<&str>| -> egui::Response {
            let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), row_h), Sense::click());
            let bg = if on { p.sel } else if resp.hovered() { p.hover } else { Color32::TRANSPARENT };
            ui.painter().rect_filled(r, theme::cr(theme::Rad::Md), bg);
            if on {
                ui.painter().rect_filled(Rect::from_min_size(r.min, Vec2::new(3.0, r.height())), theme::cr(theme::Rad::Xs), crate::widgets_win::sel_mark());
            }
            // 쓰는 곳 표시를 먼저 오른쪽에 그리고, 이름은 그 왼쪽까지만 (긴 이름 · 영어에서 겹치지 않게)
            let mut x = r.right() - 8.0;
            for u in uses {
                let rr = widgets::text_at(ui, Pos2::new(x, r.top() + 14.0), Align2::RIGHT_CENTER, u, 10.5, Weight::SemiBold, p.text2);
                ui.painter().rect_stroke(rr.expand2(Vec2::new(4.0, 1.0)), theme::cr(theme::Rad::Xs), Stroke::new(1.0, p.line), StrokeKind::Outside);
                x = rr.left() - 12.0;
            }
            // 이름은 말줄임(…)으로 자르고, 잘렸으면 마우스를 올리면 전체 (전에는 글자 가운데서 잘렸다)
            let room = (x - (r.left() + 10.0)).max(40.0);
            let cut = widgets::text_elided(ui, Pos2::new(r.left() + 10.0, r.top() + 14.0), Align2::LEFT_CENTER, name, theme::font(13.0, Weight::SemiBold), p.text, room);
            let resp = if cut { resp.on_hover_text(name) } else { resp };
            let pt2 = ui.painter().with_clip_rect(Rect::from_min_max(r.min, Pos2::new(r.right() - 6.0, r.bottom())).intersect(ui.clip_rect()));
            pt2.text(Pos2::new(r.left() + 10.0, r.top() + 32.0), Align2::LEFT_CENTER, sub, theme::font(11.0, Weight::Regular), p.muted);
            resp
        };
        let uses_of = |id: &str| -> Vec<String> { used.iter().filter(|(_, _, u)| u == id).map(|(o, n, _)| if o == "overlay" { "OBS".to_string() } else { tr(n) }).collect() };
        let base_uses = uses_of(scenes::FULL);
        let r = base_row(ui, &tr("전체 화면 (기본)"), &tr("템플릿 그대로 · 고칠 수 없음"), false, base_uses.iter().rev().map(String::as_str).collect());
        r.on_hover_text(tr("구성을 고르지 않은 출력은 늘 이 모양입니다. 고치려면 새 구성을 만드세요."));
        ui.add_space(2.0);
        let list_h = (ui.available_height() - 150.0).max(120.0);
        egui::ScrollArea::vertical().id_salt("scene-list").max_height(list_h).auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            for s in list {
                let id = s["id"].as_str().unwrap_or("");
                let on = self.sel == id;
                let shown = if on { &self.draft } else { s };
                let uses = uses_of(id);
                let sub = tr_args("칸 {1}개", &[&scenes::summary(shown)]);
                let resp = base_row(ui, shown["name"].as_str().unwrap_or(""), &sub, on, uses.iter().rev().map(String::as_str).collect());
                if resp.clicked() && !on {
                    pick = Some(s.clone());
                }
            }
            if list.is_empty() {
                ui.add(egui::Label::new(theme::rich(tr("아직 만든 구성이 없습니다. 아래 '새 구성' 에서 틀을 고르세요."), 11.5, Weight::Regular, p.muted)).wrap());
            }
        });
        if let Some(s) = pick {
            self.flush(core);
            self.load(&s);
        }
        ui.add_space(6.0);
        // 새 구성 (기본 틀)
        let mut new_kind: Option<&str> = None;
        ui.menu_button(theme::rich(tr("+ 새 구성"), 13.0, Weight::SemiBold, p.text), |ui| {
            for (k, n) in scenes::PRESETS {
                if ui.add(Btn::new(tr(n)).icon(preset_icon(k)).size(Size::Sm).min_width(190.0)).clicked() {
                    new_kind = Some(k);
                    ui.close();
                }
            }
        });
        if let Some(k) = new_kind {
            self.flush(core);
            let cam = first_camera(&core.lock());
            let n = scenes::new_scene(k, "", cam.as_deref());
            let r = core.lock().save_scene(&n);
            match r {
                Ok(s) => self.load(&s),
                Err(e) => self.msg = e.to_string(),
            }
        }
        if !self.draft.is_null() {
            ui.horizontal(|ui| {
                if ui.add(Btn::new(tr("복제")).icon("copy").size(Size::Sm)).clicked() {
                    self.flush(core);
                    let r = core.lock().duplicate_scene(&self.sel);
                    match r {
                        Ok(s) => self.load(&s),
                        Err(e) => self.msg = e.to_string(),
                    }
                }
                if widgets::confirm_delete(ui, Id::new(("scene-del", &self.sel)), &tr("지우기"), &tr("이 구성을 지울까요? 쓰던 출력은 기본 화면으로 돌아갑니다.")) {
                    let id = std::mem::take(&mut self.sel);
                    self.changed_at = None;
                    core.lock().delete_scene(&id);
                    self.draft = Value::Null;
                }
            });
        }
        if !self.msg.is_empty() {
            ui.add(egui::Label::new(theme::rich(&self.msg, 11.5, Weight::Regular, p.danger_text)).wrap());
        }
    }

    fn empty_center(&mut self, ui: &mut egui::Ui, _core: &Shared) {
        let p = pal();
        ui.add_space(40.0);
        ui.vertical_centered(|ui| {
            ui.label(theme::rich(tr("화면 구성"), 16.0, Weight::SemiBold, p.text));
            ui.add_space(6.0);
            ui.add(egui::Label::new(theme::rich(tr("카메라 · 송출 중인 순서 · 웹 페이지 같은 소스를 한 화면에 나눠 놓습니다. 왼쪽 아래 '새 구성' 에서 틀을 고른 뒤 칸에 소스를 넣으면 끝입니다."), 12.5, Weight::Regular, p.muted)).wrap());
        });
    }

    /// 가운데: 이름 · 쓰는 곳 · 틀 바꾸기 · 캔버스
    fn center_column(&mut self, ui: &mut egui::Ui, core: &Shared, pc: &paint::Ctx) {
        let p = pal();
        // 이름 · 쓰는 곳
        ui.horizontal_wrapped(|ui| {
            let mut name = self.draft["name"].as_str().unwrap_or("").to_string();
            if ui.add(egui::TextEdit::singleline(&mut name).desired_width(220.0).char_limit(40)).changed() {
                self.draft["name"] = json!(name);
            }
            ui.add_space(8.0);
            ui.label(theme::rich(tr("쓰는 곳"), 12.0, Weight::Regular, p.muted));
            let list = outs(&core.lock());
            for (o, n) in &list {
                let o = o.as_str();
                let on = core.lock().output_scene(o) == self.sel;
                let tip = if on { tr_args("누르면 {1} 출력이 기본 화면으로 돌아갑니다", &[n]) } else { tr_args("{1} 출력에 이 구성을 씁니다", &[n]) };
                let b = ui.add(Btn::new(n.clone()).icon(if o == "overlay" { "radio" } else { "monitor" }).size(Size::Sm).selected(on).tooltip(tip));
                widgets::mark(&format!("scene-use-{o}"), b.rect);
                if b.clicked() {
                    self.flush(core);
                    let r = core.lock().set_output_scene(o, if on { "" } else { &self.sel });
                    if let Err(e) = r {
                        self.msg = e.to_string();
                    }
                    if !on {
                        self.view = o.to_string();
                    }
                }
            }
        });
        ui.add_space(6.0);
        // 틀 바꾸기 (칸을 그 틀로 — 실행 취소 가능)
        ui.horizontal_wrapped(|ui| {
            ui.label(theme::rich(tr("틀"), 12.0, Weight::Regular, p.muted));
            for (k, n) in scenes::PRESETS {
                if ui.add(Btn::new(tr(n)).icon(preset_icon(k)).size(Size::Sm).tooltip(tr("칸을 이 틀로 바꿉니다 (Ctrl+Z 로 되돌리기)"))).clicked() {
                    // 지금 칸에 넣어 둔 카메라를 새 틀의 카메라 칸에 그대로
                    let cam = self.layers().iter().find_map(|l| l["source"].as_str().filter(|s| s.starts_with("camera:")).map(String::from)).or_else(|| first_camera(&core.lock()));
                    self.draft["layers"] = json!(scenes::preset_layers(k, cam.as_deref()).into_iter().filter_map(|l| scenes::normalize_layer(&l).ok()).collect::<Vec<_>>());
                    self.layer = self.layers().iter().rev().find(|l| l["source"] == "live").and_then(|l| l["id"].as_str()).map(String::from);
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.checkbox(&mut self.grid, tr("격자에 붙기")).on_hover_text(tr("칸을 끌 때 격자 · 화면 가운데 · 다른 칸 가장자리에 붙습니다. Alt 를 누르고 끌면 붙지 않습니다. Alt 를 누르고 (끌지 않고) 누르면 겹쳐 가려진 아래 칸을 차례로 고릅니다."));
            });
        });
        ui.add_space(6.0);
        // 볼 출력 (그 출력의 템플릿 · 비율로 그림)
        ui.horizontal_wrapped(|ui| {
            ui.label(theme::rich(tr("미리 보기"), 12.0, Weight::Regular, p.muted));
            let mut v = self.view.clone();
            let list = outs(&core.lock());
            if !list.iter().any(|(k, _)| *k == v) {
                v = "main".into();
            }
            let items: Vec<(String, &str)> = list.iter().map(|(k, n)| (k.clone(), n.as_str())).collect();
            widgets::segmented(ui, &mut v, &items);
            self.view = v;
        });
        // 고르기와 시간은 읽는 차례대로, 미리 보기 출력과는 다른 양식 줄에 둔다.
        crate::widgets_win::field(ui, &tr("이 구성으로 바뀔 때"), |ui| {
                let cur = wp_core::scenectl::normalize(&self.draft["transition"]);
                if let Some(nv) = crate::outputs_ui::transition_pick(ui, "scene", cur.as_ref(), Some(&tr("출력 설정 따라감"))) {
                    match nv {
                        Some(t) => self.draft["transition"] = t,
                        None => {
                            self.draft.as_object_mut().map(|o| o.remove("transition"));
                        }
                    }
                }
        });
        ui.add_space(6.0);
        // 캔버스
        let (stage_t, scene, slide, names, version) = {
            let c = core.lock();
            let out = self.view.as_str();
            let deck = c.pres.live.as_ref().map(|l| &l.deck).or(c.pres.preview.as_ref());
            let stage_t = c.stage_template_for_deck(out, c.pres.live.as_ref().map(|l| &l.deck));
            let slide = deck.and_then(|d| d.slides.get(d.index)).cloned().unwrap_or_else(sample);
            let version = deck.map(|d| d.passage["versionName"].as_str().unwrap_or("").to_string()).unwrap_or_default();
            (stage_t, c.effective_scene_for(&self.draft, out, deck), slide, source_names(&c), version)
        };
        let (tw, th) = crate::slide_layout::stage_size(&stage_t);
        let avail = ui.available_size() - Vec2::new(0.0, 26.0);
        let w = avail.x.min(avail.y * tw / th).max(200.0);
        let (outer, resp) = ui.allocate_exact_size(Vec2::new(avail.x, w * th / tw), Sense::click_and_drag());
        let canvas = Rect::from_center_size(outer.center(), Vec2::new(w, w * th / tw));
        self.canvas_seen = Some(canvas);
        crate::widgets::mark("scene-canvas", canvas);
        let name_of = |id: &str| names.iter().find(|(k, _)| k == id).map(|(_, n)| n.clone()).unwrap_or_else(|| id.to_string());
        paint::paint_scene(ui, pc, canvas, &scene, Some(&slide), &stage_t, &version, true, false, false, &name_of);
        let _ = paint::take_hits();
        self.canvas_interact(ui, canvas, &resp, &name_of);
        ui.add_space(4.0);
        ui.add(egui::Label::new(theme::rich(tr("맨 위에는 늘 오버레이 · 회중 메시지 · 로고 화면 · 그리기가 나옵니다 (칸에 직접 넣으면 그 자리 차례로)."), 11.0, Weight::Regular, p.muted)).wrap());
    }

    fn canvas_interact(&mut self, ui: &mut egui::Ui, canvas: Rect, resp: &egui::Response, name_of: &dyn Fn(&str) -> String) {
        let p = pal();
        // 고른 칸 색: 전문가 테마의 노랑은 '다음' 에만 쓰므로 밝은 회색으로
        let sel_col = if p.bevel { Color32::from_gray(232) } else { crate::widgets_win::sel_mark() };
        let layers = self.layers();
        let to_screen = |r: [f64; 4]| Rect::from_min_size(canvas.min + Vec2::new(r[0] as f32 * canvas.width(), r[1] as f32 * canvas.height()), Vec2::new(r[2] as f32 * canvas.width(), r[3] as f32 * canvas.height()));
        let painter = ui.painter_at(canvas.expand(6.0));
        // 칸 테두리 · 이름표 (빈 칸은 점선 + '소스 고르기')
        for l in &layers {
            let id = l["id"].as_str().unwrap_or("");
            let r = to_screen(scenes::rect_of(l));
            let sel = self.layer.as_deref() == Some(id);
            let src = l["source"].as_str().unwrap_or("");
            let hidden = l["visible"] == false;
            if src.is_empty() {
                let dash = egui::Shape::dashed_line(&[r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()], Stroke::new(1.5, p.muted), 6.0, 4.0);
                painter.extend(dash);
                painter.text(r.center(), Align2::CENTER_CENTER, tr("소스 고르기"), theme::font(12.0, Weight::SemiBold), p.muted);
            } else if !sel {
                painter.rect_stroke(r.shrink(0.5), 0.0, Stroke::new(1.0, if hidden { p.muted2 } else { Color32::from_white_alpha(70) }), StrokeKind::Inside);
            }
            let label = if src.is_empty() { tr("빈 칸") } else { name_of(src) };
            let label = if hidden { tr_args("{1} (숨김)", &[&label]) } else { label };
            let g = painter.layout_no_wrap(label, theme::font(10.5, Weight::SemiBold), if sel && p.bevel { Color32::from_gray(24) } else { Color32::WHITE });
            let chip = Rect::from_min_size(r.min + Vec2::new(3.0, 3.0), g.size() + Vec2::new(8.0, 4.0));
            // 화면 전체 칸의 이름표는 고른 때만 (작은 칸 이름표와 같은 구석에 겹치지 않게)
            let full = scenes::rect_of(l) == [0.0, 0.0, 1.0, 1.0];
            if chip.width() < r.width() - 4.0 && (sel || !full || layers.len() == 1) {
                painter.rect_filled(chip, 3.0, if sel { sel_col } else { Color32::from_black_alpha(150) });
                painter.galley(chip.min + Vec2::new(4.0, 2.0), g, Color32::WHITE);
            }
        }
        // 고른 칸: 테두리 + 손잡이 8개
        let sel_rect = self.layer.as_ref().and_then(|id| layers.iter().find(|l| l["id"] == id.as_str())).map(|l| (to_screen(scenes::rect_of(l)), l["locked"] == true));
        let handles = |r: Rect| -> [(u8, Pos2); 8] {
            [(5, r.left_top()), (4, r.center_top()), (6, r.right_top()), (2, r.right_center()), (10, r.right_bottom()), (8, r.center_bottom()), (9, r.left_bottom()), (1, r.left_center())]
        };
        if let Some((r, locked)) = sel_rect {
            painter.rect_stroke(r, 0.0, Stroke::new(2.0, sel_col), StrokeKind::Inside);
            if !locked {
                for (_, h) in handles(r) {
                    let hr = Rect::from_center_size(h, Vec2::splat(9.0));
                    painter.rect_filled(hr, 1.5, Color32::WHITE);
                    painter.rect_stroke(hr, 1.5, Stroke::new(1.5, sel_col), StrokeKind::Inside);
                }
            } else {
                crate::icons::paint(ui, Rect::from_min_size(r.right_top() + Vec2::new(-20.0, 4.0), Vec2::splat(16.0)), "lock", Color32::WHITE);
            }
        }
        // 안내선
        for (vert, at) in &self.guides {
            let s = Stroke::new(1.0, Color32::from_rgb(255, 64, 200));
            if *vert {
                let x = canvas.left() + *at as f32 * canvas.width();
                painter.vline(x, canvas.y_range(), s);
            } else {
                let y = canvas.top() + *at as f32 * canvas.height();
                painter.hline(canvas.x_range(), y, s);
            }
        }
        // 마우스: 손잡이 위면 크기 바꾸기, 칸 위면 옮기기
        let pos = ui.ctx().pointer_latest_pos();
        let handle_at = |pt: Pos2| -> Option<u8> {
            let (r, locked) = sel_rect?;
            if locked {
                return None;
            }
            handles(r).into_iter().find(|(_, h)| h.distance(pt) <= 7.0).map(|(k, _)| k)
        };
        let top_at = |pt: Pos2, unlocked: bool| -> Option<String> {
            layers.iter().rev().filter(|l| l["visible"] != false && !(unlocked && l["locked"] == true)).find(|l| to_screen(scenes::rect_of(l)).contains(pt)).and_then(|l| l["id"].as_str()).map(String::from)
        };
        if self.drag.is_none() {
            if let Some(pt) = pos.filter(|pt| canvas.expand(6.0).contains(*pt) && resp.hovered()) {
                match handle_at(pt) {
                    Some(k) => ui.ctx().set_cursor_icon(match k {
                        5 | 10 => CursorIcon::ResizeNwSe,
                        6 | 9 => CursorIcon::ResizeNeSw,
                        1 | 2 => CursorIcon::ResizeHorizontal,
                        _ => CursorIcon::ResizeVertical,
                    }),
                    None if top_at(pt, true).is_some() => ui.ctx().set_cursor_icon(CursorIcon::Grab),
                    None => {}
                }
            }
        }
        if resp.drag_started() {
            // 끌기는 조금 움직인 뒤 시작되므로, 칸 고르기는 처음 누른 자리로
            if let Some(pt) = ui.input(|i| i.pointer.press_origin()).or(resp.interact_pointer_pos()) {
                let start_layer = match handle_at(pt) {
                    Some(k) => self.layer.clone().map(|id| (id, DragKind::Resize(k))),
                    None => {
                        // Alt+누름으로 아래 칸을 골라 두었으면 그 칸 (그 자리 안일 때), 아니면 누른 자리의 맨 위 칸 (잠근 칸은 건너뜀)
                        let picked = self.alt_pick.then(|| self.layer.clone()).flatten().filter(|id| layers.iter().any(|l| l["id"] == id.as_str() && l["locked"] != true && to_screen(scenes::rect_of(l)).contains(pt)));
                        picked.or_else(|| top_at(pt, true)).map(|id| (id, DragKind::Move))
                    }
                };
                if let Some((id, kind)) = start_layer {
                    if let Some(l) = layers.iter().find(|l| l["id"] == id.as_str()) {
                        self.layer = Some(id.clone());
                        self.drag = Some(Drag { layer: id, kind, start: scenes::rect_of(l), at: pt });
                    }
                }
            }
        }
        if let (Some(d), Some(pt)) = (self.drag.clone(), pos) {
            if resp.dragged() || ui.input(|i| i.pointer.primary_down()) {
                if d.kind == DragKind::Move {
                    ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
                }
                let dv = pt - d.at;
                let free = ui.input(|i| i.modifiers.alt);
                let keep = ui.input(|i| i.modifiers.shift);
                let others: Vec<[f64; 4]> = layers.iter().filter(|l| l["id"] != d.layer.as_str() && l["visible"] != false).map(scenes::rect_of).collect();
                let thr = if free { [0.0, 0.0] } else { [(SNAP_PX / canvas.width()) as f64, (SNAP_PX / canvas.height()) as f64] };
                let kind = match d.kind {
                    DragKind::Move => 0,
                    DragKind::Resize(k) => k,
                };
                let (r, g) = drag_rect(d.start, kind, [(dv.x / canvas.width()) as f64, (dv.y / canvas.height()) as f64], &others, thr, if self.grid && !free { GRID } else { 0.0 }, keep);
                self.guides = g;
                if let Some(l) = self.sel_layer_mut() {
                    let r4 = r.map(|x| (x * 10000.0).round() / 10000.0);
                    if l["rect"] != json!(r4) {
                        l["rect"] = json!(r4);
                    }
                }
            }
        }
        if resp.drag_stopped() || (self.drag.is_some() && !ui.input(|i| i.pointer.primary_down())) {
            self.drag = None;
            self.guides.clear();
        }
        // 누르기: 고르기 (잠근 칸도 고를 수 있다). Alt+누름 = 그 자리에 겹친 칸을 위에서 아래로 차례로 (가려진 칸 고르기)
        // Alt+끌기는 그대로 '붙지 않음' — 누름(움직이지 않음)과 끌기는 따로라 겹치지 않는다
        if resp.clicked() {
            if let Some(pt) = resp.interact_pointer_pos() {
                let alt = ui.input(|i| i.modifiers.alt);
                if alt {
                    let under: Vec<String> = layers.iter().rev().filter(|l| l["visible"] != false && to_screen(scenes::rect_of(l)).contains(pt)).filter_map(|l| l["id"].as_str().map(String::from)).collect();
                    self.layer = cycle_pick(&under, self.layer.as_deref());
                    self.alt_pick = true;
                } else if handle_at(pt).is_none() {
                    self.layer = top_at(pt, false);
                    self.alt_pick = false;
                }
            }
        }
    }

    /// 오른쪽: 칸 목록 + 고른 칸 속성
    fn inspector(&mut self, ui: &mut egui::Ui, core: &Shared) {
        let p = pal();
        let (choices, templates, names, stage_t) = {
            let c = core.lock();
            let out = self.view.as_str();
            (choices(&c), c.settings.templates().iter().map(|t| (t["id"].as_str().unwrap_or("").to_string(), wp_core::i18n::tr_name(t["name"].as_str().unwrap_or("")))).collect::<Vec<_>>(), source_names(&c), c.stage_template_for_deck(out, c.pres.live.as_ref().map(|l| &l.deck)))
        };
        let name_of = |id: &str| -> String {
            if id.is_empty() {
                return tr("빈 칸");
            }
            names.iter().find(|(k, _)| k == id).map(|(_, n)| n.clone()).unwrap_or_else(|| id.to_string())
        };
        // ---- 칸 목록 (위가 맨 앞)
        if crate::prolayout_ui::section(ui, "scene-layers", &tr("칸 (위가 맨 앞)"), true) {
            let layers = self.layers();
            let mut act: Option<(&str, String)> = None;
            egui::Frame::new().inner_margin(egui::Margin { left: 6, right: 6, top: 4, bottom: 6 }).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                for l in layers.iter().rev() {
                    let id = l["id"].as_str().unwrap_or("").to_string();
                    let sel = self.layer.as_deref() == Some(id.as_str());
                    let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 26.0), Sense::click());
                    ui.painter().rect_filled(r, theme::cr(theme::Rad::Sm), if sel { p.sel } else if resp.hovered() { p.hover } else { Color32::TRANSPARENT });
                    let vis = l["visible"] != false;
                    let lock = l["locked"] == true;
                    let eye = Rect::from_min_size(r.min + Vec2::new(4.0, 5.0), Vec2::splat(16.0));
                    let lk = Rect::from_min_size(r.min + Vec2::new(24.0, 5.0), Vec2::splat(16.0));
                    // 고른 줄은 아이콘을 밝게 (흐리면 잘 안 보임)
                    crate::icons::paint(ui, eye, if vis { "eye" } else { "eye-off" }, if sel { p.text } else if vis { p.text2 } else { p.muted2 });
                    crate::icons::paint(ui, lk, "lock", if lock { p.text } else if sel { p.text2 } else { p.muted2 });
                    let src = l["source"].as_str().unwrap_or("");
                    let mut nm = name_of(src);
                    if let Some(n) = l["name"].as_str() {
                        nm = format!("{n} · {nm}");
                    }
                    let tr_r = widgets::text_at(ui, Pos2::new(r.left() + 48.0, r.center().y), Align2::LEFT_CENTER, &nm, 12.0, if sel { Weight::SemiBold } else { Weight::Regular }, if vis { p.text } else { p.muted });
                    let _ = tr_r;
                    if resp.clicked() {
                        let pt = resp.interact_pointer_pos().unwrap_or(r.center());
                        if eye.expand(3.0).contains(pt) {
                            act = Some(("vis", id.clone()));
                        } else if lk.expand(3.0).contains(pt) {
                            act = Some(("lock", id.clone()));
                        } else {
                            self.layer = Some(id.clone());
                        }
                    }
                    resp.on_hover_text(tr("눈: 보이기 · 숨기기, 자물쇠: 잠그기 (캔버스에서 옮기지 않음)"));
                }
            });
            if let Some((k, id)) = act {
                if let Some(l) = self.draft["layers"].as_array_mut().and_then(|a| a.iter_mut().find(|l| l["id"] == id.as_str())) {
                    if k == "vis" {
                        l["visible"] = json!(l["visible"] == false);
                    } else {
                        l["locked"] = json!(l["locked"] != true);
                    }
                }
            }
            // 칸 더하기 · 차례 · 지우기
            ui.horizontal_wrapped(|ui| {
                if ui.add(Btn::new(tr("칸 더하기")).icon("plus").size(Size::Sm).enabled(self.layers().len() < scenes::MAX_LAYERS)).clicked() {
                    let n = scenes::normalize_layer(&json!({ "source": "", "rect": [0.35, 0.35, 0.3, 0.3] })).unwrap();
                    self.layer = n["id"].as_str().map(String::from);
                    if let Some(a) = self.draft["layers"].as_array_mut() {
                        a.push(n);
                    }
                }
                let idx = self.layer.as_deref().and_then(|id| self.layer_idx(id));
                let len = self.layers().len();
                let mv = |s: &mut Self, to: usize| {
                    if let (Some(i), Some(a)) = (idx, s.draft["layers"].as_array_mut()) {
                        let l = a.remove(i);
                        a.insert(to.min(a.len()), l);
                    }
                };
                if ui.add(Btn::icon_only("chevrons-up").size(Size::Sm).enabled(idx.is_some_and(|i| i + 1 < len)).tooltip(tr("맨 앞으로"))).clicked() {
                    mv(self, len);
                }
                if ui.add(Btn::icon_only("arrow-up").size(Size::Sm).enabled(idx.is_some_and(|i| i + 1 < len)).tooltip(tr("앞으로"))).clicked() {
                    mv(self, idx.unwrap_or(0) + 1);
                }
                if ui.add(Btn::icon_only("arrow-down").size(Size::Sm).enabled(idx.is_some_and(|i| i > 0)).tooltip(tr("뒤로"))).clicked() {
                    mv(self, idx.unwrap_or(1).saturating_sub(1));
                }
                if ui.add(Btn::icon_only("chevrons-down").size(Size::Sm).enabled(idx.is_some_and(|i| i > 0)).tooltip(tr("맨 뒤로"))).clicked() {
                    mv(self, 0);
                }
                if ui.add(Btn::icon_only("trash-2").size(Size::Sm).enabled(idx.is_some()).tooltip(tr("칸 지우기 (Ctrl+Z 로 되돌리기)"))).clicked() {
                    if let (Some(i), Some(a)) = (idx, self.draft["layers"].as_array_mut()) {
                        a.remove(i);
                    }
                    self.layer = None;
                }
            });
        }
        let Some(lid) = self.layer.clone() else {
            ui.add_space(8.0);
            ui.add(egui::Label::new(theme::rich(tr("캔버스나 위 목록에서 칸을 누르면 여기서 고칩니다."), 11.5, Weight::Regular, p.muted)).wrap());
            return;
        };
        let Some(mut l) = self.layers().into_iter().find(|l| l["id"] == lid.as_str()) else {
            self.layer = None;
            return;
        };
        let orig = l.clone();
        let (tw, th) = crate::slide_layout::stage_size(&stage_t);
        // ---- 소스
        if crate::prolayout_ui::section(ui, "scene-src", &tr("소스"), true) {
            egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 10, top: 6, bottom: 8 }).show(ui, |ui| {
                let cur = l["source"].as_str().unwrap_or("").to_string();
                let mut pick = cur.clone();
                egui::ComboBox::from_id_salt(("scene-src", &lid)).width(290.0).selected_text(name_of(&cur)).show_ui(ui, |ui| {
                    for (id, n) in &choices {
                        ui.selectable_value(&mut pick, id.clone(), n);
                    }
                    if !cur.is_empty() && !choices.iter().any(|(id, _)| *id == cur) {
                        ui.selectable_value(&mut pick, cur.clone(), name_of(&cur));
                    }
                });
                if pick != cur {
                    l["source"] = json!(pick);
                }
                let src = l["source"].as_str().unwrap_or("").to_string();
                let src = src.as_str();
                if src == "live" || src == "background" {
                    ui.add_space(4.0);
                    ui.label(theme::rich(tr("글 모양 (템플릿)"), 11.5, Weight::Regular, p.muted));
                    let tpl = l["tpl"].as_str().unwrap_or("").to_string();
                    let label = |t: &str| -> String {
                        match t {
                            "" => tr("이 출력의 템플릿 그대로"),
                            "main" => tr("예배당 템플릿"),
                            "overlay" => tr("OBS 자막 템플릿 (아래 띠)"),
                            id => templates.iter().find(|(k, _)| k == id).map(|(_, n)| n.clone()).unwrap_or_else(|| id.to_string()),
                        }
                    };
                    let mut nt = tpl.clone();
                    egui::ComboBox::from_id_salt(("scene-tpl", &lid)).width(290.0).selected_text(label(&tpl)).show_ui(ui, |ui| {
                        for t in ["", "main", "overlay"] {
                            ui.selectable_value(&mut nt, t.to_string(), label(t));
                        }
                        ui.separator();
                        for (id, n) in &templates {
                            ui.selectable_value(&mut nt, id.clone(), n);
                        }
                    });
                    if nt != tpl {
                        l["tpl"] = json!(nt);
                    }
                    ui.add(egui::Label::new(theme::rich(tr("작은 칸에 넣으면 글자를 칸 크기에 맞춰 다시 맞춥니다."), 11.0, Weight::Regular, p.muted)).wrap());
                }
                if src.starts_with("camera:") || src.starts_with("src-") || src == "media" {
                    ui.add_space(4.0);
                    let mut fit = l["fit"].as_str().unwrap_or("cover").to_string();
                    let a = tr("칸 채우기");
                    let b = tr("다 보이기");
                    ui.horizontal(|ui| {
                        ui.label(theme::rich(tr("맞추기"), 11.5, Weight::Regular, p.muted));
                        widgets::segmented(ui, &mut fit, &[("cover".to_string(), a.as_str()), ("contain".to_string(), b.as_str())]);
                    });
                    if l["fit"] != fit.as_str() {
                        l["fit"] = json!(fit);
                    }
                    if src.starts_with("camera:") || src.starts_with("src-") {
                        ui.add(egui::Label::new(theme::rich(tr("조작 화면에는 이름 상자만 보이고, 실제 영상 · 웹 페이지는 출력 화면에 나옵니다."), 11.0, Weight::Regular, p.muted)).wrap());
                    }
                }
            });
        }
        // ---- 자리 · 크기 (무대 px)
        if crate::prolayout_ui::section(ui, "scene-rect", &tr("자리 · 크기"), true) {
            egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 10, top: 6, bottom: 8 }).show(ui, |ui| {
                let mut r = scenes::rect_of(&l);
                let mut px = [r[0] * tw as f64, r[1] * th as f64, r[2] * tw as f64, r[3] * th as f64];
                let before = px;
                egui::Grid::new(("scene-rect-g", &lid)).num_columns(4).spacing([6.0, 4.0]).show(ui, |ui| {
                    for (i, n) in ["X", "Y", tr("폭").as_str(), tr("높이").as_str()].iter().enumerate() {
                        ui.label(theme::rich(*n, 11.5, Weight::Regular, p.muted));
                        let max = if i % 2 == 0 { tw as f64 } else { th as f64 };
                        ui.add(egui::DragValue::new(&mut px[i]).range(0.0..=max).speed(1.0).max_decimals(0));
                        if i == 1 {
                            ui.end_row();
                        }
                    }
                    ui.end_row();
                });
                if px != before {
                    r = scenes::clamp_rect([px[0] / tw as f64, px[1] / th as f64, px[2] / tw as f64, px[3] / th as f64]);
                    l["rect"] = json!(r.map(|x| (x * 10000.0).round() / 10000.0));
                }
                ui.horizontal(|ui| {
                    if ui.add(Btn::new(tr("화면 전체")).icon("maximize").size(Size::Sm)).clicked() {
                        l["rect"] = json!([0.0, 0.0, 1.0, 1.0]);
                    }
                    if ui.add(Btn::new(tr("가운데로")).size(Size::Sm)).clicked() {
                        let r = scenes::rect_of(&l);
                        l["rect"] = json!(scenes::clamp_rect([(1.0 - r[2]) / 2.0, (1.0 - r[3]) / 2.0, r[2], r[3]]).map(|x| (x * 10000.0).round() / 10000.0));
                    }
                    if ui.add(Btn::new(tr("16:9 로")).size(Size::Sm).tooltip(tr("폭은 그대로 두고 높이를 16:9 비율로"))).clicked() {
                        let r = scenes::rect_of(&l);
                        let h = r[2] * tw as f64 * 9.0 / 16.0 / th as f64;
                        l["rect"] = json!(scenes::clamp_rect([r[0], r[1], r[2], h]).map(|x| (x * 10000.0).round() / 10000.0));
                    }
                });
            });
        }
        // ---- 자르기 (%)
        if crate::prolayout_ui::section(ui, "scene-crop", &tr("자르기"), false) {
            egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 10, top: 6, bottom: 8 }).show(ui, |ui| {
                let c = scenes::crop_of(&l);
                let mut pc = c.map(|x| (x * 100.0).round());
                let before = pc;
                egui::Grid::new(("scene-crop-g", &lid)).num_columns(4).spacing([6.0, 4.0]).show(ui, |ui| {
                    for (i, n) in ["왼쪽", "위", "오른쪽", "아래"].iter().enumerate() {
                        ui.label(theme::rich(tr(n), 11.5, Weight::Regular, p.muted));
                        ui.add(egui::DragValue::new(&mut pc[i]).range(0.0..=90.0).speed(0.5).suffix("%").max_decimals(0));
                        if i == 1 {
                            ui.end_row();
                        }
                    }
                    ui.end_row();
                });
                if pc != before {
                    let mut n = l.clone();
                    n["crop"] = json!(pc.map(|x| x / 100.0));
                    let c2 = scenes::crop_of(&n);
                    if c2.iter().any(|x| *x > 0.0) {
                        l["crop"] = json!(c2.map(|x| (x * 10000.0).round() / 10000.0));
                    } else if let Some(o) = l.as_object_mut() {
                        o.remove("crop");
                    }
                }
                ui.add(egui::Label::new(theme::rich(tr("소스의 가장자리를 잘라 내고 남은 부분을 칸에 채웁니다 (예: 카메라에서 설교자만)."), 11.0, Weight::Regular, p.muted)).wrap());
            });
        }
        // ---- 모양: 투명도 · 둥글기 · 테두리
        if crate::prolayout_ui::section(ui, "scene-look", &tr("모양"), true) {
            egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 10, top: 6, bottom: 8 }).show(ui, |ui| {
                egui::Grid::new(("scene-look-g", &lid)).num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                    ui.label(theme::rich(tr("투명도"), 11.5, Weight::Regular, p.muted));
                    let mut op = (l["opacity"].as_f64().unwrap_or(1.0) * 100.0).round();
                    if ui.add(egui::Slider::new(&mut op, 0.0..=100.0).suffix("%").max_decimals(0)).changed() {
                        l["opacity"] = json!(op / 100.0);
                    }
                    ui.end_row();
                    ui.label(theme::rich(tr("모서리 둥글기"), 11.5, Weight::Regular, p.muted));
                    let mut rd = l["radius"].as_f64().unwrap_or(0.0);
                    if ui.add(egui::DragValue::new(&mut rd).range(0.0..=400.0).speed(1.0).suffix(" px").max_decimals(0)).changed() {
                        l["radius"] = json!(rd);
                    }
                    ui.end_row();
                    ui.label(theme::rich(tr("테두리"), 11.5, Weight::Regular, p.muted));
                    ui.horizontal(|ui| {
                        let mut bw = l["border"]["width"].as_f64().unwrap_or(0.0);
                        if ui.add(egui::DragValue::new(&mut bw).range(0.0..=60.0).speed(0.5).suffix(" px").max_decimals(0)).changed() {
                            l["border"]["width"] = json!(bw);
                        }
                        let col = l["border"]["color"].as_str().and_then(paint::parse_color).unwrap_or(Color32::WHITE);
                        let mut c = [col.r(), col.g(), col.b()];
                        if ui.color_edit_button_srgb(&mut c).changed() {
                            l["border"]["color"] = json!(format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]));
                        }
                    });
                    ui.end_row();
                    ui.label(theme::rich(tr("칸 이름"), 11.5, Weight::Regular, p.muted));
                    let mut nm = l["name"].as_str().unwrap_or("").to_string();
                    if ui.add(egui::TextEdit::singleline(&mut nm).desired_width(170.0).char_limit(40).hint_text(tr("예: 설교자"))).changed() {
                        l["name"] = json!(nm);
                    }
                    ui.end_row();
                });
            });
        }
        if l != orig {
            if let Some(slot) = self.sel_layer_mut() {
                *slot = l;
            }
        }
    }
}

fn preset_icon(k: &str) -> &'static str {
    match k {
        "split" => "columns-2",
        "pip" => "picture-in-picture-2",
        "lower" => "panel-bottom",
        "three" => "layout-panel-left",
        _ => "square",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alt_click_cycles_through_stacked_cells() {
        let under: Vec<String> = ["top", "mid", "bottom"].iter().map(|s| s.to_string()).collect();
        assert_eq!(cycle_pick(&under, None).as_deref(), Some("top"));
        assert_eq!(cycle_pick(&under, Some("top")).as_deref(), Some("mid"));
        assert_eq!(cycle_pick(&under, Some("mid")).as_deref(), Some("bottom"));
        assert_eq!(cycle_pick(&under, Some("bottom")).as_deref(), Some("top"), "맨 아래 다음은 다시 맨 위");
        assert_eq!(cycle_pick(&under, Some("elsewhere")).as_deref(), Some("top"));
        assert_eq!(cycle_pick(&[], Some("top")), None);
    }

    #[test]
    fn move_snaps_to_center_and_edges() {
        // 가운데 가까이 옮기면 가운데에 붙는다
        let (r, g) = drag_rect([0.1, 0.1, 0.2, 0.2], 0, [0.295, 0.0], &[], [0.01, 0.01], 0.0, false);
        assert!((r[0] + r[2] / 2.0 - 0.5).abs() < 1e-9, "{r:?}");
        assert!(g.iter().any(|(v, x)| *v && (*x - 0.5).abs() < 1e-9));
        // 붙기를 끄면(Alt) 그대로
        let (r, g) = drag_rect([0.1, 0.1, 0.2, 0.2], 0, [0.295, 0.0], &[], [0.0, 0.0], 0.0, false);
        assert!((r[0] - 0.395).abs() < 1e-9 && g.is_empty());
        // 화면 밖으로는 못 나간다
        let (r, _) = drag_rect([0.7, 0.7, 0.3, 0.3], 0, [0.5, 0.5], &[], [0.0, 0.0], 0.0, false);
        assert_eq!(r, [0.7, 0.7, 0.3, 0.3]);
        // 다른 칸 오른쪽 가장자리에 붙는다
        let (r, _) = drag_rect([0.0, 0.0, 0.2, 0.2], 0, [0.405, 0.0], &[[0.2, 0.5, 0.4, 0.3]], [0.01, 0.01], 0.0, false);
        assert!((r[0] - 0.6).abs() < 1e-9 || (r[0] + r[2] - 0.6).abs() < 1e-9, "{r:?}");
    }

    #[test]
    fn resize_edges_grid_and_keep_ratio() {
        // 오른쪽 아래 모서리를 끌어 키우기 + 격자(24칸)에 붙기
        let (r, _) = drag_rect([0.0, 0.0, 0.5, 0.5], 10, [0.163, 0.0], &[], [0.01, 0.01], 24.0, false);
        assert!((r[0] + r[2] - 16.0 / 24.0).abs() < 1e-9, "{r:?}");
        assert!((r[3] - 0.5).abs() < 1e-9);
        // 왼쪽 변을 오른쪽 변 너머로 끌어도 가장 작은 폭은 남는다
        let (r, _) = drag_rect([0.2, 0.2, 0.2, 0.2], 1, [0.9, 0.0], &[], [0.0, 0.0], 0.0, false);
        assert!(r[2] >= scenes::MIN_SIZE - 1e-9 && r[0] + r[2] <= 0.4 + 1e-9, "{r:?}");
        // 모서리 + Shift: 비율 그대로
        let (r, _) = drag_rect([0.1, 0.1, 0.4, 0.2], 10, [0.1, 0.3], &[], [0.0, 0.0], 0.0, true);
        assert!((r[2] / r[3] - 2.0).abs() < 1e-9, "{r:?}");
    }
}

#[cfg(test)]
#[path = "scene_ui_tests.rs"]
mod ui_tests;
