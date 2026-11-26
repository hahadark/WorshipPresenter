//! [트랙 B] 조작 화면 창: 여러 PC 동기화·백업 · 연결 점검·로그 묶음 · 화면에 그리기·강조 · 카메라 설정
//! 메뉴 id: b:sync · b:diagnose · b:logbundle · b:logs · b:draw · b:cameras

use crate::app::App;
use crate::paint::{self, paint_slide};
use eframe::egui::{self, Color32, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde_json::{json, Value};
use wp_core::diagnose::Desktop;

#[derive(Default)]
pub struct SysState {
    pub sync: bool,
    pub diag: bool,
    pub draw: bool,
    pub cameras: bool,
    diag_result: Value,
    note: String,
    restore_busy: bool,
    // 그리기
    pen_color: usize,
    pen_width: f32,
    spot_mode: bool,
    spot_r: f32,
    stroke_id: Option<String>,
    stroke_pts: Vec<[f64; 2]>,
    seq: u64,
    // 카메라
    cam_edit: Option<(String, Value)>,
    /// 점검용: 그리기 판 자리 · 넣은 단계
    canvas: Option<Rect>,
    inject_step: usize,
}

pub static ST: Lazy<Mutex<SysState>> = Lazy::new(|| Mutex::new(SysState { pen_width: 0.006, spot_r: 0.18, ..Default::default() }));

const COLORS: [(&str, &str); 6] = [("#ff3b30", "빨강"), ("#ffd60a", "노랑"), ("#34c759", "초록"), ("#0a84ff", "파랑"), ("#ffffff", "흰색"), ("#000000", "검정")];

/// 메뉴에서 고른 트랙 B 항목 (처리했으면 true)
pub fn handle_menu(app: &mut App, id: &str) -> bool {
    let mut st = ST.lock();
    match id {
        "b:sync" => st.sync = true,
        "b:diagnose" => {
            st.diag = true;
            st.diag_result = Value::Null;
        }
        "b:draw" => st.draw = true,
        "b:cameras" => st.cameras = true,
        "b:logs" => {
            drop(st);
            let d = wp_core::logger::dir(&app.data_dir);
            let _ = std::fs::create_dir_all(&d);
            let _ = std::process::Command::new("explorer").arg(d).spawn();
        }
        "b:logbundle" => {
            drop(st);
            app.save_log_bundle();
        }
        _ => return false,
    }
    true
}

fn ago(iso: &str) -> String {
    if iso.is_empty() { wp_core::i18n::tr("없음") } else { iso.replace('T', " ").chars().take(19).collect::<String>() + " (UTC)" }
}

impl App {
    fn desktop(&mut self) -> Desktop {
        let running = self.out.running();
        let m = self.out.monitors.iter().find(|m| m.index == self.monitor).cloned();
        // OBS 원격 조작 상태는 OBS 원격 조작 창이 마지막으로 받아 둔 캠시만 쓴다 (연결 점검이 OBS 에 직접 붙지 않게).
        // 연결된 것이 확인된 때만 캠시를 넘기고, 그 외에는 Null(미확인) 로 두어 '확인해 보세요' 경고가 나오게 한다.
        let obs_ws = {
            let settings = self.core.lock().settings.get().clone();
            let (enabled, connected, _) = crate::trackd_ui::obs_state(&settings);
            if enabled && connected { json!({ "connected": true }) } else { Value::Null }
        };
        Desktop {
            monitors: self.out.monitors.len(),
            output_running: running,
            output_monitor: m.as_ref().map(|m| format!("{} {}×{}", m.name, m.size.0, m.size.1)).unwrap_or_default(),
            same_as_control: m.is_some_and(|m| m.primary),
            obs_ws,
        }
    }

    pub(crate) fn save_log_bundle(&mut self) {
        let d = self.desktop();
        let (name, text) = wp_core::diagnose::log_bundle(&mut self.core.lock(), &d, 7);
        let Some(p) = rfd::FileDialog::new().set_file_name(&name).add_filter("글", &["txt"]).save_file() else { return };
        let r = std::fs::write(&p, text);
        let mut c = self.core.lock();
        match r {
            Ok(_) => c.notify(wp_core::i18n::tr_args("로그 묶음을 저장했습니다: {1}", &[&(p.display())]), "ok"),
            Err(e) => c.notify(wp_core::i18n::tr_args("로그 묶음 저장 실패: {1}", &[&e]), "error"),
        }
    }

    pub(crate) fn system_windows(&mut self, ctx: &egui::Context) {
        // 점검용: --open-b sync,diagnose,draw,cameras
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            if let Some(v) = std::env::args().position(|x| x == "--open-b").and_then(|i| std::env::args().nth(i + 1)) {
                for w in v.split(',') {
                    handle_menu(self, &format!("b:{w}"));
                }
            }
        });
        self.sync_window(ctx);
        self.diag_window(ctx);
        self.draw_window(ctx);
        self.cameras_window(ctx);
    }

    // ---------------- 여러 PC 동기화 · 백업 ----------------
    fn sync_window(&mut self, ctx: &egui::Context) {
        let mut open = ST.lock().sync;
        if !open {
            return;
        }
        crate::widgets::float_window(ctx, "sync", wp_core::i18n::tr("여러 PC 동기화 · 백업"), [540.0, 460.0]).open(&mut open).show(ctx, |ui| {
            let info = self.core.lock().sync_info();
            ui.label(RichText::new(wp_core::i18n::tr("원드라이브 같은 동기화 폴더를 고르면 찬양 · 큐시트 · 오버레이를 그 폴더의 WorshipCue-sync.json 으로 주고받습니다 (기존 워십프리젠터와 같은 파일). 시작할 때 읽고, 바뀌면 1분 안에, 끝날 때 씁니다.")).small());
            ui.horizontal(|ui| {
                let dir = info["dir"].as_str().unwrap_or("");
                ui.label(if dir.is_empty() { RichText::new(wp_core::i18n::tr("동기화 꺼짐")).color(Color32::GRAY) } else { RichText::new(format!("📁 {dir}")).strong() });
            });
            let mut note = String::new();
            ui.horizontal(|ui| {
                if ui.button(wp_core::i18n::tr("폴더 고르기…")).clicked() {
                    if let Some(d) = rfd::FileDialog::new().pick_folder() {
                        let r = self.core.lock().set_sync_dir(&d.to_string_lossy());
                        note = match r { Ok(_) => "동기화 폴더를 정하고 주고받았습니다.".into(), Err(e) => e.to_string() };
                    }
                }
                if !info["dir"].as_str().unwrap_or("").is_empty() {
                    if ui.button(wp_core::i18n::tr("지금 동기화")).clicked() {
                        let r = self.core.lock().sync_now();
                        note = match r { Ok(v) => format!("받음: 찬양 {} · 큐시트 {} · 오버레이 {} / 보냄: 찬양 {} · 큐시트 {}", v["imported"]["songs"], v["imported"]["cues"], v["imported"]["overlays"], v["exported"]["songs"], v["exported"]["cues"]), Err(e) => e.to_string() };
                    }
                    if ui.button(wp_core::i18n::tr("동기화 끄기")).clicked() {
                        let _ = self.core.lock().set_sync_dir("");
                    }
                }
            });
            egui::Grid::new("syncinfo").num_columns(2).show(ui, |ui| {
                ui.label(wp_core::i18n::tr("이 PC")); ui.label(info["host"].as_str().unwrap_or("")); ui.end_row();
                ui.label(wp_core::i18n::tr("마지막으로 받음")); ui.label(ago(info["lastImport"].as_str().unwrap_or(""))); ui.end_row();
                ui.label(wp_core::i18n::tr("마지막으로 보냄")); ui.label(ago(info["lastExport"].as_str().unwrap_or(""))); ui.end_row();
                ui.label(wp_core::i18n::tr("동기화 파일을 쓴 PC")); ui.label(format!("{} {}", info["remoteHost"].as_str().unwrap_or(""), ago(info["remoteAt"].as_str().unwrap_or("")))); ui.end_row();
            });
            if let Some(e) = info["error"].as_str().filter(|x| !x.is_empty()) {
                ui.label(RichText::new(e).color(Color32::from_rgb(0xff, 0x8a, 0x8a)));
            }
            ui.separator();
            ui.label(RichText::new(wp_core::i18n::tr("백업 (하루 한 번 자동 · 10일치 보관)")).strong());
            let restore_busy = ST.lock().restore_busy;
            if restore_busy {
                ui.label(RichText::new("되돌리기 예약 파일을 복사하고 있습니다. 끝날 때까지 기다려 주세요.").color(Color32::from_rgb(0xff, 0xb0, 0x4a)));
            }
            if info["restorePending"] == true {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(wp_core::i18n::tr("되돌리기 예약됨 — 프로그램을 다시 시작하면 적용됩니다.")).color(Color32::from_rgb(0xff, 0xb0, 0x4a)));
                    if ui.add_enabled(!restore_busy, egui::Button::new(wp_core::i18n::tr("예약 취소"))).clicked() { self.core.lock().cancel_restore(); }
                });
            }
            ui.horizontal(|ui| {
                if ui.button(wp_core::i18n::tr("지금 백업")).clicked() {
// 큰 DB 를 복사하는 동안 화면이 멈추지 않게 따로 스레드에서
                    let dir = self.data_dir.clone();
                    let ctx2 = ui.ctx().clone();
                    note = "백업하는 중…".into();
                    std::thread::spawn(move || {
                        let r = wp_core::sync::backup_files(&dir);
                        ST.lock().note = match r { Ok(b) => format!("백업했습니다: {}", b["day"].as_str().unwrap_or("")), Err(e) => format!("백업 실패: {e}") };
                        ctx2.request_repaint();
                    });
                }
                if ui.button(wp_core::i18n::tr("백업 폴더 열기")).clicked() {
                    let d = self.core.lock().backup_dir();
                    let _ = std::fs::create_dir_all(&d);
                    let _ = std::process::Command::new("explorer").arg(d).spawn();
                }
            });
            let list = self.core.lock().backups();
            egui::ScrollArea::vertical().id_salt("backups").max_height(160.0).show(ui, |ui| {
                for b in &list {
                    ui.horizontal(|ui| {
                        ui.label(format!("{} · {:.1}MB{}", b["day"].as_str().unwrap_or(""), b["size"].as_f64().unwrap_or(0.0) / 1048576.0, if b["settings"].as_str().unwrap_or("").is_empty() { "" } else { wp_core::i18n::tr_s(" · 설정 포함") }));
                        if ui.add_enabled(!restore_busy && info["restorePending"] != true, egui::Button::new(wp_core::i18n::tr("이 날로 되돌리기")).small()).on_hover_text(wp_core::i18n::tr("다음 시작 때 이 백업으로 바꿉니다 (지금 것은 bible.pre-restore.db 로 남김)")).clicked() {
                            // 큰 파일 복사는 별도 스레드에서. 복사 중 중복 예약과 취소는 막는다.
                            let (dir, day, ctx2) = (self.data_dir.clone(), b["day"].as_str().unwrap_or("").to_string(), ui.ctx().clone());
                            ST.lock().restore_busy = true;
                            note = "되돌리기 예약하는 중…".into();
                            std::thread::spawn(move || {
                                let r = wp_core::sync::restore_files(&dir, &day);
                                let mut state = ST.lock();
                                state.note = match r { Ok(_) => format!("{day} 로 되돌리기를 예약했습니다. 프로그램을 다시 시작하세요."), Err(e) => e.to_string() };
                                state.restore_busy = false;
                                drop(state);
                                ctx2.request_repaint();
                            });
                        }
                    });
                }
                if list.is_empty() { ui.label(RichText::new(wp_core::i18n::tr("아직 백업이 없습니다.")).color(Color32::GRAY)); }
            });
            // 다른 컴퓨터로 옮기기: 자료 전체를 파일 하나로 (wp_core::movedata)
            ui.separator();
            ui.label(RichText::new(wp_core::i18n::tr("다른 컴퓨터로 옮기기")).strong());
            ui.label(RichText::new(wp_core::i18n::tr("성경·찬양·큐시트·설정·올린 그림을 파일 하나로 저장합니다. 새 컴퓨터에서 '옮기기 파일 가져오기' 뒤 프로그램을 다시 시작하세요.")).small());
            ui.horizontal(|ui| {
                let b = ui.add_enabled(!restore_busy, egui::Button::new(wp_core::i18n::tr("옮기기 파일 만들기…")));
                crate::widgets::mark("move-export", b.rect);
                if b.clicked() {
                    let name = format!("워십프리젠터-자료-{}.{}", wp_core::util::iso_now().get(..10).unwrap_or(""), wp_core::movedata::EXT);
                    if let Some(f) = rfd::FileDialog::new().set_file_name(name).add_filter(wp_core::i18n::tr("워십프리젠터 옮기기 파일"), &[wp_core::movedata::EXT]).save_file() {
                        let (dir, ctx2) = (self.data_dir.clone(), ui.ctx().clone());
                        ST.lock().restore_busy = true;
                        note = wp_core::i18n::tr("옮기기 파일을 만드는 중…");
                        std::thread::spawn(move || {
                            let r = wp_core::movedata::export_to(&dir, &f);
                            let mut state = ST.lock();
                            state.note = match r { Ok(v) => wp_core::i18n::tr_args("옮기기 파일을 만들었습니다 ({1}MB · 그림 {2}개): {3}", &[&format!("{:.1}", v["size"].as_f64().unwrap_or(0.0) / 1048576.0), &v["media"], &f.display()]), Err(e) => format!("{e:#}") };
                            state.restore_busy = false;
                            drop(state);
                            ctx2.request_repaint();
                        });
                    }
                }
                let b = ui.add_enabled(!restore_busy && info["restorePending"] != true, egui::Button::new(wp_core::i18n::tr("옮기기 파일 가져오기…"))).on_hover_text(wp_core::i18n::tr("다음 시작 때 이 컴퓨터의 자료를 그 파일의 것으로 바꿉니다 (지금 것은 bible.pre-restore.db 로 남김). 같은 이름의 그림은 덮지 않습니다."));
                crate::widgets::mark("move-import", b.rect);
                if b.clicked() {
                    if let Some(f) = rfd::FileDialog::new().add_filter(wp_core::i18n::tr("워십프리젠터 옮기기 파일"), &[wp_core::movedata::EXT]).pick_file() {
                        let (dir, ctx2) = (self.data_dir.clone(), ui.ctx().clone());
                        ST.lock().restore_busy = true;
                        note = wp_core::i18n::tr("옮기기 파일을 읽는 중…");
                        std::thread::spawn(move || {
                            let r = wp_core::movedata::import_from(&dir, &f);
                            let mut state = ST.lock();
                            state.note = match r { Ok(v) => wp_core::i18n::tr_args("가져오기를 예약했습니다 (그림 {1}개 더함) — 프로그램을 다시 시작하면 적용됩니다.", &[&v["media"]]), Err(e) => format!("{e:#}") };
                            state.restore_busy = false;
                            drop(state);
                            ctx2.request_repaint();
                        });
                    }
                }
            });
            if !note.is_empty() { ST.lock().note = note; }
            let n = ST.lock().note.clone();
            if !n.is_empty() { ui.label(RichText::new(wp_core::i18n::tr_dyn(&n)).small().color(Color32::from_rgb(0x8a, 0xe0, 0xa8))); }
        });
        ST.lock().sync = open;
    }

    // ---------------- 연결 점검 · 로그 ----------------
    fn diag_window(&mut self, ctx: &egui::Context) {
        let mut open = ST.lock().diag;
        if !open {
            return;
        }
        if ST.lock().diag_result.is_null() {
            let d = self.desktop();
            let r = wp_core::diagnose::diagnose(&mut self.core.lock(), &d);
            ST.lock().diag_result = r;
        }
        crate::widgets::float_window(ctx, "diag", wp_core::i18n::tr("연결 점검 (예배 전 점검)"), [600.0, 560.0]).open(&mut open).show(ctx, |ui| {
            let r = ST.lock().diag_result.clone();
            ui.horizontal(|ui| {
                let (f, w) = (r["summary"]["fail"].as_u64().unwrap_or(0), r["summary"]["warn"].as_u64().unwrap_or(0));
                let (t, c) = if f > 0 { (wp_core::i18n::tr_args("문제 {1}곳 · 확인 {2}곳", &[&f, &w]), Color32::from_rgb(0xff, 0x6b, 0x6b)) } else if w > 0 { (wp_core::i18n::tr_args("확인할 곳 {1}곳", &[&w]), Color32::from_rgb(0xff, 0xb0, 0x4a)) } else { (wp_core::i18n::tr("모두 괜찮습니다"), Color32::from_rgb(0x8a, 0xe0, 0xa8)) };
                ui.label(RichText::new(t).strong().color(c));
                if ui.button(wp_core::i18n::tr("다시 점검")).clicked() { ST.lock().diag_result = Value::Null; }
                if ui.button(wp_core::i18n::tr("로그 묶음 저장…")).on_hover_text(wp_core::i18n::tr("문제를 알릴 때 보낼 글 파일 (열쇠·PIN·AI 키는 가림)")).clicked() { self.save_log_bundle(); }
                if ui.button(wp_core::i18n::tr("로그 폴더")).clicked() { handle_menu(self, "b:logs"); }
            });
            ui.separator();
            crate::firewall_ui::row(ui); // 휴대폰 리모컨 · 내부망 출력 · 휴대폰 카메라가 들어오는 길
            ui.separator();
            egui::ScrollArea::vertical().id_salt("diag").auto_shrink([false, false]).show(ui, |ui| {
                for x in r["items"].as_array().into_iter().flatten() {
                    let (mark, col) = match x["status"].as_str().unwrap_or("") {
                        "ok" => ("✔", Color32::from_rgb(0x8a, 0xe0, 0xa8)),
                        "warn" => ("!", Color32::from_rgb(0xff, 0xb0, 0x4a)),
                        "fail" => ("✖", Color32::from_rgb(0xff, 0x6b, 0x6b)),
                        _ => ("i", Color32::GRAY),
                    };
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(mark).strong().color(col));
                        ui.label(RichText::new(wp_core::i18n::tr_dyn_deep(x["title"].as_str().unwrap_or(""))).strong());
                        ui.label(wp_core::i18n::tr_dyn_deep(x["detail"].as_str().unwrap_or("")));
                    });
                    for f in x["fix"].as_array().into_iter().flatten().filter_map(|f| f.as_str()) {
                        ui.label(RichText::new(format!("    · {}", wp_core::i18n::tr_dyn_deep(f))).small().color(Color32::LIGHT_GRAY));
                    }
                    ui.add_space(4.0);
                }
            });
        });
        ST.lock().diag = open;
    }

    // ---------------- 화면에 그리기 · 강조 ----------------
    fn draw_window(&mut self, ctx: &egui::Context) {
        let mut open = ST.lock().draw;
        if !open {
            return;
        }
        crate::widgets::float_window(ctx, "draw", wp_core::i18n::tr("화면에 그리기 · 강조"), [720.0, 500.0]).open(&mut open).show(ctx, |ui| {
            let mut st = ST.lock();
            ui.horizontal(|ui| {
                ui.selectable_value(&mut st.spot_mode, false, wp_core::i18n::tr("펜"));
                ui.selectable_value(&mut st.spot_mode, true, wp_core::i18n::tr("강조 (원 하나만 밝게)"));
                ui.separator();
                for (k, (hex, name)) in COLORS.iter().enumerate() {
                    let c = paint::parse_color(hex).unwrap_or(Color32::RED);
                    let (r, resp) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::click());
                    ui.painter().circle_filled(r.center(), 8.0, c);
                    if st.pen_color == k { ui.painter().circle_stroke(r.center(), 10.0, Stroke::new(2.0, Color32::WHITE)); }
                    if resp.on_hover_text(*name).clicked() { st.pen_color = k; }
                }
                ui.add(egui::Slider::new(&mut st.pen_width, 0.002..=0.03).text("굵기").show_value(false));
                if st.spot_mode { ui.add(egui::Slider::new(&mut st.spot_r, 0.05..=0.5).text("원 크기").show_value(false)); }
            });
            drop(st);
            ui.horizontal(|ui| {
                if ui.button(wp_core::i18n::tr("되돌리기")).clicked() { self.core.lock().draw_undo(); }
                if ui.button(wp_core::i18n::tr("모두 지우기")).clicked() { self.core.lock().draw_clear(); }
                if ui.button(wp_core::i18n::tr("강조 끄기")).clicked() { self.core.lock().draw_spot(None); }
                ui.label(RichText::new(wp_core::i18n::tr("예배당 화면 · OBS 화면에 바로 보입니다 (무대 화면은 빼고)")).small().color(Color32::GRAY));
            });
            // 바탕: 지금 LIVE 슬라이드
            let (live, tpl, d, logo, hide_bg) = {
                let c = self.core.lock();
                (c.pres.live.clone(), c.template_for("main"), c.draw_json(), c.layers.logo, c.layers.hide_bg)
            };
            let tw = tpl["width"].as_f64().unwrap_or(1920.0) as f32;
            let th = tpl["height"].as_f64().unwrap_or(1080.0) as f32;
            let w = ui.available_width();
            let h = (w * th / tw).min(ui.available_height() - 8.0).max(100.0);
            let (resp, painter) = ui.allocate_painter(Vec2::new(h * tw / th, h), Sense::click_and_drag());
            let rect = resp.rect;
            ST.lock().canvas = Some(rect);
            let pc = paint::Ctx { web_dir: &self.web_dir, data_dir: &self.data_dir };
            let slide = live.as_ref().and_then(|l| l.deck.slides.get(l.deck.index));
            let ver = live.as_ref().map(|l| l.deck.passage["versionName"].as_str().unwrap_or("").to_string()).unwrap_or_default();
            paint_slide(ui, &pc, rect, slide, &tpl, &ver, live.as_ref().is_some_and(|l| l.visible), logo, hide_bg);
            // 지금 그려진 것
            if let Some(sp) = d["spot"].as_object() {
                let (x, y, r) = (sp["x"].as_f64().unwrap_or(0.5) as f32, sp["y"].as_f64().unwrap_or(0.5) as f32, sp["r"].as_f64().unwrap_or(0.18) as f32);
                // 출력 화면처럼: 원 밖만 어둡게 (egui 에는 구멍 뚫린 도형이 없어 가로 띠로 나눠 칠한다)
                let (cx, cy, rr) = (rect.left() + x * rect.width(), rect.top() + y * rect.height(), r * rect.height());
                let dim = Color32::from_black_alpha(168);
                let step = 2.0;
                let mut yy = rect.top();
                while yy < rect.bottom() {
                    let y2 = (yy + step).min(rect.bottom());
                    let dy = (yy + step / 2.0 - cy).abs();
                    if dy >= rr {
                        painter.rect_filled(Rect::from_min_max(Pos2::new(rect.left(), yy), Pos2::new(rect.right(), y2)), 0.0, dim);
                    } else {
                        let half = (rr * rr - dy * dy).sqrt();
                        painter.rect_filled(Rect::from_min_max(Pos2::new(rect.left(), yy), Pos2::new((cx - half).max(rect.left()), y2)), 0.0, dim);
                        painter.rect_filled(Rect::from_min_max(Pos2::new((cx + half).min(rect.right()), yy), Pos2::new(rect.right(), y2)), 0.0, dim);
                    }
                    yy = y2;
                }
                painter.circle_stroke(Pos2::new(rect.left() + x * rect.width(), rect.top() + y * rect.height()), r * rect.height(), Stroke::new(2.0, Color32::from_white_alpha(160)));
            }
            for s in d["strokes"].as_array().into_iter().flatten() {
                let col = s["color"].as_str().and_then(paint::parse_color).unwrap_or(Color32::RED).gamma_multiply(s["alpha"].as_f64().unwrap_or(1.0) as f32);
                let pts: Vec<Pos2> = s["pts"].as_array().into_iter().flatten().map(|p| Pos2::new(rect.left() + p[0].as_f64().unwrap_or(0.0) as f32 * rect.width(), rect.top() + p[1].as_f64().unwrap_or(0.0) as f32 * rect.height())).collect();
                let wpx = (s["width"].as_f64().unwrap_or(0.006) as f32 * rect.height()).max(1.0);
                painter.add(egui::Shape::line(pts, Stroke::new(wpx, col)));
            }
            painter.rect_stroke(rect, 2.0, Stroke::new(1.0, Color32::from_gray(90)), egui::StrokeKind::Outside);
            // 입력
            let to01 = |p: Pos2| [((p.x - rect.left()) / rect.width()) as f64, ((p.y - rect.top()) / rect.height()) as f64];
            let mut st = ST.lock();
            if st.spot_mode {
                if let Some(p) = resp.interact_pointer_pos().filter(|_| resp.clicked() || resp.dragged()) {
                    let q = to01(p);
                    let r = st.spot_r as f64;
                    drop(st);
                    self.core.lock().draw_spot(Some((q[0], q[1], r)));
                }
            } else {
                if resp.drag_started() {
                    st.seq += 1;
                    st.stroke_id = Some(format!("r{}-{}", wp_core::util::now_ms(), st.seq));
                    st.stroke_pts.clear();
                }
                if resp.dragged() {
                    if let Some(p) = resp.interact_pointer_pos() {
                        st.stroke_pts.push(to01(p));
                        let s = json!({ "id": st.stroke_id, "pts": st.stroke_pts, "color": COLORS[st.pen_color].0, "width": st.pen_width, "alpha": 1.0 });
                        drop(st);
                        let _ = self.core.lock().draw_stroke(&s);
                        st = ST.lock();
                    }
                }
                if resp.drag_stopped() {
                    st.stroke_id = None;
                    st.stroke_pts.clear();
                }
            }
        });
        ST.lock().draw = open;
    }

    // ---------------- 카메라 설정 ----------------
    fn cameras_window(&mut self, ctx: &egui::Context) {
        let mut open = ST.lock().cameras;
        if !open {
            return;
        }
        crate::widgets::float_window(ctx, "cameras", wp_core::i18n::tr("카메라 설정"), [520.0, 440.0]).open(&mut open).show(ctx, |ui| {
            ui.label(RichText::new(wp_core::i18n::tr("카메라(웹캠 · 캡처보드)마다 해상도 · 프레임 · 좌우 반전 · 밝기를 정합니다. 출력 화면이 카메라를 열 때 씁니다.")).small());
            // 화면 셋 공용 목록(camscan): 연결된 카메라 → 설정에 있는 카메라 → 큐시트의 카메라 순서
            crate::camscan::scan_bar(ui, &self.core);
            let (saved, list) = {
                let c = self.core.lock();
                (c.settings.get()["cameras"].as_object().cloned().unwrap_or_default(), crate::camscan::choices(&c))
            };
            if list.is_empty() {
                ui.label(RichText::new(wp_core::i18n::tr("아직 카메라가 없습니다. '카메라 찾기'를 누르거나 큐시트에 카메라 순서를 넣으면 여기에 나옵니다.")).color(Color32::GRAY));
            }
            let mut st = ST.lock();
            let mut preview: Option<(String, String)> = None;
            for c in &list {
                let (id, label) = (&c.device_id, &c.label);
                ui.horizontal(|ui| {
                    if ui.selectable_label(st.cam_edit.as_ref().is_some_and(|e| &e.0 == id), crate::camscan::display_name(c)).clicked() {
                        let mut p = saved.get(id).cloned().unwrap_or(json!({ "label": label, "res": "auto", "fps": 0, "exact": false, "mirror": false, "brightness": 100, "contrast": 100, "saturation": 100 }));
                        if p["label"].as_str().unwrap_or("").is_empty() { p["label"] = json!(label); }
                        st.cam_edit = Some((id.clone(), p));
                    }
                    if ui.small_button(wp_core::i18n::tr("미리보기")).clicked() { preview = Some((id.clone(), label.clone())); }
                });
            }
            if let Some((id, label)) = preview { crate::camscan::preview(&self.core, &id, &label); }
            ui.separator();
            let mut action: Option<(String, Option<Value>)> = None;
            if let Some((id, p)) = &mut st.cam_edit {
                egui::Grid::new("cam").num_columns(2).show(ui, |ui| {
                    ui.label(wp_core::i18n::tr("해상도"));
                    let mut res = p["res"].as_str().unwrap_or("auto").to_string();
                    egui::ComboBox::from_id_salt("cam-res").selected_text(if res == "auto" { "자동".into() } else { format!("{res}p") }).show_ui(ui, |ui| {
                        for r in ["auto", "720", "1080", "1440", "2160"] { ui.selectable_value(&mut res, r.to_string(), if r == "auto" { "자동".into() } else { format!("{r}p") }); }
                    });
                    p["res"] = json!(res);
                    ui.end_row();
                    ui.label(wp_core::i18n::tr("프레임"));
                    let mut fps = p["fps"].as_i64().unwrap_or(0);
                    egui::ComboBox::from_id_salt("cam-fps").selected_text(if fps == 0 { "자동".into() } else { format!("{fps}fps") }).show_ui(ui, |ui| {
                        for f in [0, 24, 25, 30, 50, 60] { ui.selectable_value(&mut fps, f, if f == 0 { "자동".into() } else { format!("{f}fps") }); }
                    });
                    p["fps"] = json!(fps);
                    ui.end_row();
                    let mut b = |ui: &mut egui::Ui, k: &str, label: &str| { let mut v = p[k].as_bool().unwrap_or(false); ui.checkbox(&mut v, wp_core::i18n::tr(label)); p[k] = json!(v); };
                    ui.label(""); b(ui, "exact", "해상도·프레임을 꼭 맞춤 (안 되면 안 열림)"); ui.end_row();
                    ui.label(""); b(ui, "mirror", "좌우 반전"); ui.end_row();
                    for (k, label, lo, hi) in [("brightness", "밝기", 50, 200), ("contrast", "대비", 50, 200), ("saturation", "채도", 0, 200)] {
                        ui.label(wp_core::i18n::tr(label));
                        let mut v = p[k].as_i64().unwrap_or(100);
                        ui.add(egui::Slider::new(&mut v, lo..=hi).suffix("%"));
                        p[k] = json!(v);
                        ui.end_row();
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button(wp_core::i18n::tr("저장")).clicked() { action = Some((id.clone(), Some(p.clone()))); }
                    if ui.button(wp_core::i18n::tr("기본값으로 (설정 지우기)")).clicked() { action = Some((id.clone(), None)); }
                });
            }
            if let Some((id, p)) = action {
                if p.is_none() { st.cam_edit = None; }
                drop(st);
                self.core.lock().set_camera_prefs(&id, p.as_ref());
            }
        });
        ST.lock().cameras = open;
    }
}

/// 점검용 (--draw-test): 그리기 판에 진짜 마우스 입력을 흉내 낸다 (eframe raw_input_hook → egui 의 보통 입력 처리)
/// 30 프레임 기다린 뒤 대각선으로 한 줄 긋고, 강조 모드로 바꿔 한 번 누른다.
pub fn inject(ctx: &egui::Context, raw: &mut egui::RawInput) {
    static ON: Lazy<bool> = Lazy::new(|| std::env::args().any(|a| a == "--draw-test"));
    if !*ON {
        return;
    }
    let mut st = ST.lock();
    let Some(r) = st.canvas else { return };
    let k = st.inject_step;
    st.inject_step += 1;
    let at = |fx: f32, fy: f32| Pos2::new(r.left() + fx * r.width(), r.top() + fy * r.height());
    let btn = |pos: Pos2, pressed: bool| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
    let n = 16;
    match k {
        30 => raw.events.push(egui::Event::PointerMoved(at(0.15, 0.25))),
        31 => raw.events.push(btn(at(0.15, 0.25), true)),
        32..=47 => {
            let t = (k - 31) as f32 / n as f32;
            raw.events.push(egui::Event::PointerMoved(at(0.15 + 0.7 * t, 0.25 + 0.5 * t)));
        }
        48 => raw.events.push(btn(at(0.85, 0.75), false)),
        55 => st.spot_mode = true,
        60 => raw.events.push(egui::Event::PointerMoved(at(0.5, 0.5))),
        61 => raw.events.push(btn(at(0.5, 0.5), true)),
        62 => raw.events.push(btn(at(0.5, 0.5), false)),
        63 => st.spot_mode = false,
        _ => {}
    }
    if k < 70 {
        ctx.request_repaint();
    }
}