//! [트랙 C] 송출 기록(Ctrl+H) · 사용 기록(저작권 보고 CSV) · 정보·라이선스 창

use crate::app::Act;
use crate::theme::pal;
use crate::widgets;
use eframe::egui::{self, RichText};
use wp_core::core::Shared;
use wp_core::usage;

#[derive(Default)]
pub struct HistoryUi {
    pub open: bool,
    pub usage_open: bool,
    pub about_open: bool,
    from: String,
    to: String,
    q: String,
    by_title: bool,
    msg: String,
}

fn local_time(ms: i64) -> String {
    // 기록 시각 (UTC ms) → 이 PC 시간대로 표시
    let iso = {
        let t = ms + wp_core::localtime::local_offset_ms();
        let secs = t / 1000;
        let days = secs.div_euclid(86400);
        let rem = secs.rem_euclid(86400);
        let z = days + 719468;
        let era = z.div_euclid(146097);
        let doe = z - era * 146097;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let mo = if mp < 10 { mp + 3 } else { mp - 9 };
        format!("{:02}-{:02} {:02}:{:02}", mo, d, rem / 3600, (rem % 3600) / 60)
    };
    iso
}

impl HistoryUi {
    pub fn show(&mut self, ctx: &egui::Context, core: &Shared, acts: &mut Vec<Act>) {
        if self.open {
            let hist = core.lock().settings.get()["history"].as_array().cloned().unwrap_or_default();
            let mut open = self.open;
            widgets::float_window(ctx, "history", wp_core::i18n::tr("송출 기록"), [520.0, 520.0]).open(&mut open).show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(wp_core::i18n::tr_args("{1}개 (최근 300개)", &[&(hist.len())]));
                    // 되살릴 수 없으므로 줄 안에서 한 번 더 묻는다 (휴리스틱 평가 3번)
                    let q = wp_core::i18n::tr_args("송출 기록 {1}개가 지워지고 되살릴 수 없습니다.", &[&(hist.len())]);
                    if !hist.is_empty() && widgets::confirm_delete(ui, egui::Id::unique("hist-clear"), &wp_core::i18n::tr("기록 지우기"), &q) {
                        wp_core::templates::clear_history(&mut core.lock().settings);
                    }
                });
                let direct = core.lock().direct_mode();
                ui.label(RichText::new(wp_core::i18n::tr(if direct { "누르면 바로 송출" } else { "누르면 미리보기, 두 번 누르면 송출" })).small().color(pal().muted));
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    for h in &hist {
                        let q = h["query"].as_str().unwrap_or("").to_string();
                        let t = format!("{}   {}  ·  {}", local_time(h["at"].as_i64().unwrap_or(0)), h["title"].as_str().unwrap_or(""), h["versionName"].as_str().unwrap_or(""));
                        let r = ui.add(egui::Label::new(t).sense(egui::Sense::click()));
                        if !q.is_empty() {
                            match widgets::click2(ui, ("hist", h["at"].as_i64().unwrap_or(0), &q), &r) {
                                widgets::Click::Double => acts.push(Act::SendRef(q, None)),
                                widgets::Click::Single => acts.push(Act::PreviewRef(q, None)),
                                widgets::Click::None => {}
                            }
                        }
                    }
                });
            });
            self.open = open;
        }
        if self.usage_open {
            let mut open = self.usage_open;
            let f = usage::Filter { from: self.from.trim().into(), to: self.to.trim().into(), kinds: vec![], q: self.q.clone() };
            widgets::float_window(ctx, "usage", wp_core::i18n::tr("사용 기록 (저작권 보고)"), [720.0, 540.0]).open(&mut open).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(wp_core::i18n::tr("부터"));
                    ui.add(egui::TextEdit::singleline(&mut self.from).hint_text("2026-11-01").desired_width(90.0));
                    ui.label(wp_core::i18n::tr("까지"));
                    ui.add(egui::TextEdit::singleline(&mut self.to).hint_text("2026-11-30").desired_width(90.0));
                    ui.add(egui::TextEdit::singleline(&mut self.q).hint_text(wp_core::i18n::tr("제목 찾기")).desired_width(140.0));
                    ui.checkbox(&mut self.by_title, wp_core::i18n::tr("제목별 횟수"));
                });
                ui.horizontal(|ui| {
                    if ui.button(wp_core::i18n::tr("CSV 저장…")).clicked() {
                        let csv = usage::csv(&core.lock().store.db, &f, self.by_title);
                        let name = format!("WorshipCue-사용기록{}.csv", if self.by_title { wp_core::i18n::tr_s("-제목별") } else { "" });
                        if let Some(p) = rfd::FileDialog::new().set_file_name(&name).add_filter("CSV", &["csv"]).save_file() {
                            self.msg = match std::fs::write(&p, csv) { Ok(()) => format!("저장했습니다: {}", p.display()), Err(e) => format!("저장 실패: {e}") };
                        }
                    }
                    ui.label(RichText::new(&self.msg).small());
                });
                // 저작권 보고 자료라 되살릴 수 없다: 줄 안 확인 + 먼저 CSV 로 보관하라는 안내 (휴리스틱 평가 3번)
                let all = usage::list(&core.lock().store.db, &usage::Filter::default()).len();
                let q = wp_core::i18n::tr_args("사용 기록 {1}줄이 모두 지워지고 되살릴 수 없습니다. 먼저 'CSV 저장…'으로 보관해 두세요.", &[&all]);
                let id = egui::Id::unique("usage-clear");
                if widgets::confirm_armed(ui, id) {
                    if widgets::confirm_row(ui, id, &q) {
                        let n = usage::clear(&core.lock().store.db, "");
                        self.msg = wp_core::i18n::tr_args("{1}줄을 지웠습니다.", &[&n]);
                    }
                } else {
                    let tip = if all > 0 { wp_core::i18n::tr("누르면 한 번 더 묻습니다") } else { wp_core::i18n::tr("지울 사용 기록이 없습니다") };
                    if ui.add(widgets::Btn::new(wp_core::i18n::tr("모두 지우기")).icon("trash-2").kind(widgets::Kind::Danger).size(widgets::Size::Sm).enabled(all > 0).tooltip(tip)).clicked() {
                        widgets::confirm_arm(ui, id, true);
                    }
                }
                ui.separator();
                let db = &core.lock().store.db;
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    egui::Grid::new("usage-grid").striped(true).num_columns(4).show(ui, |ui| {
                        if self.by_title {
                            for s in usage::summary(db, &f) {
                                ui.label(usage::kind_name(&s.kind));
                                ui.label(&s.title);
                                ui.label(wp_core::i18n::tr_args("{1}번", &[&s.count]));
                                ui.label(RichText::new(&s.credit).small());
                                ui.end_row();
                            }
                        } else {
                            for r in usage::list(db, &f).iter().take(1000) {
                                ui.label(local_time(usage::iso_ms(&r.at).unwrap_or(0)));
                                ui.label(usage::kind_name(&r.kind));
                                ui.label(&r.title);
                                ui.label(RichText::new(&r.cue).small());
                                ui.end_row();
                            }
                        }
                    });
                });
            });
            self.usage_open = open;
        }
        if self.about_open {
            let mut open = self.about_open;
            widgets::float_window(ctx, "about", wp_core::i18n::tr("정보"), [520.0, 420.0]).open(&mut open).show(ctx, |ui| {
                ui.heading(wp_core::i18n::tr("워십프리젠터 (Rust 판)"));
                ui.label(wp_core::i18n::tr_args("버전 {1} · 라이선스 GPL-3.0-or-later", &[&(env!("CARGO_PKG_VERSION"))]));
                ui.separator();
                ui.label(wp_core::i18n::tr("가져다 쓴 공개 코드·글꼴"));
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let notices = include_str!("../../../THIRD_PARTY_NOTICES.md");
                    ui.label(RichText::new(notices).small());
                    ui.separator();
                    ui.label(RichText::new("Rust: egui/eframe (MIT/Apache-2.0) · wry/tao (MIT/Apache-2.0) · axum·tokio (MIT) · rusqlite (MIT) · ab_glyph (Apache-2.0)").small());
                });
            });
            self.about_open = open;
        }
    }
}
