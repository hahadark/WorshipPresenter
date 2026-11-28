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
    /// 송출 기록 목록에서 키로 고른 줄 (control.js openHistory 의 sel)
    pub(crate) sel: usize,
    was_open: bool,
    scroll_sel: bool,
    /// 다시 띄운 본문 (검색칸 글, 본문) — app.rs 가 검색칸·절 목록에 반영한다
    pub(crate) loaded: Option<(String, serde_json::Value)>,
}

/// 기록 한 줄을 다시 띄운다 (control.js:1357 load): 기록된 제목을 기록된 번역본으로 다시 찾아 미리보기에 올린다.
/// 큐시트 순서에서 나간 기록이어도 그때의 순서 번호를 쓰지 않고 본문 자체를 띄우므로, 그 뒤 큐시트가 바뀌어도 다른 순서가 나가지 않는다.
/// 찾지 못하면(찬양·제목 같은 성경이 아닌 기록, 지워진 번역본) 알림만 내고 미리보기·LIVE 는 그대로 둔다.
fn load(core: &Shared, h: &serde_json::Value, send: bool, acts: &mut Vec<Act>) -> Option<(String, serde_json::Value)> {
    let s = |k: &str| h[k].as_str().unwrap_or("").trim().to_string();
    let (title, query, vid) = (s("title"), s("query"), s("versionId"));
    let mut c = core.lock();
    let v = if vid.is_empty() { c.version_id() } else { vid };
    let r = c.store.resolve_reference(&v, &title);
    if r["ok"] != serde_json::json!(true) {
        let e = r["error"].as_str().map(str::to_string).unwrap_or_else(|| wp_core::i18n::tr("본문을 찾지 못했습니다."));
        c.notify(format!("{title}: {e}"), "warn");
        return None;
    }
    drop(c);
    let q = if query.is_empty() { title } else { query };
    let pg = r["passage"].clone();
    acts.push(Act::PreviewPassage(pg.clone(), q.clone(), None));
    if send {
        acts.push(Act::SendPreview);
    }
    Some((q, pg))
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
        use crate::cue_ui::Nav;
        if !self.open {
            if std::mem::take(&mut self.was_open) {
                crate::cue_ui::nav_release(ctx, Nav::History);
            }
        }
        if self.open {
            let hist = core.lock().settings.get()["history"].as_array().cloned().unwrap_or_default();
            // 열면 목록이 키보드를 갖고 첫 줄을 고른다 (control.js:1356 `let sel = 0; … list.focus()`)
            if !std::mem::replace(&mut self.was_open, true) {
                self.sel = 0;
                self.scroll_sel = true;
                crate::cue_ui::nav_set(ctx, Nav::History);
            }
            self.sel = self.sel.min(hist.len().saturating_sub(1));
            crate::cue_ui::nav_touch(ctx, Nav::History);
            // 목록 키 (control.js:1364): ↑↓ 고르기 · Enter = 그 줄을 다시 띄우고 창 닫기. 글 쓰는 칸·메뉴가 열려 있으면 받지 않는다
            let mut close = false;
            if crate::cue_ui::nav_owner(ctx) == Nav::History && !egui::Popup::is_any_open(ctx) && !hist.is_empty() {
                use egui::{Key, Modifiers};
                let plain = ctx.input(|i| !i.modifiers.alt && !i.modifiers.command && !i.modifiers.ctrl);
                let (down, up, enter) = if plain {
                    ctx.input_mut(|i| (i.consume_key(Modifiers::NONE, Key::ArrowDown), i.consume_key(Modifiers::NONE, Key::ArrowUp), i.consume_key(Modifiers::NONE, Key::Enter)))
                } else {
                    (false, false, false)
                };
                if down {
                    self.sel = (self.sel + 1).min(hist.len() - 1);
                    self.scroll_sel = true;
                } else if up {
                    self.sel = self.sel.saturating_sub(1);
                    self.scroll_sel = true;
                } else if enter {
                    // 원본 load: 창을 닫고 다시 띄운다 (찾지 못해도 창은 닫히고 알림이 뜨다)
                    if let Some(l) = load(core, &hist[self.sel], false, acts) {
                        self.loaded = Some(l);
                    }
                    close = true;
                }
            }
            let key_owner = crate::cue_ui::nav_owner(ctx) == Nav::History;
            let mut open = self.open;
            let win = crate::widgets_win::window(ctx, "history", wp_core::i18n::tr("송출 기록"), [520.0, 520.0]).open(&mut open).show(ctx, |ui| {
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
                if hist.is_empty() {
                    widgets::empty_state(ui, &wp_core::i18n::tr("아직 송출한 기록이 없습니다. 송출하면 여기에 차례대로 남습니다."));
                }
                egui::ScrollArea::vertical().auto_shrink([false, !hist.is_empty()]).show(ui, |ui| {
                    for (i, h) in hist.iter().enumerate() {
                        let title = h["title"].as_str().unwrap_or("");
                        let t = format!("{}   {}  ·  {}", local_time(h["at"].as_i64().unwrap_or(0)), title, h["versionName"].as_str().unwrap_or(""));
                        let r = ui.add(egui::Label::new(t).sense(egui::Sense::click()));
                        widgets::mark(&format!("hist-row-{i}"), r.rect);
                        if i == self.sel {
                            // .row.sel — 키보드가 목록에 있을 때만 진하게
                            let c = if key_owner { pal().accent } else { pal().line2 };
                            ui.painter().rect_stroke(r.rect.expand2(egui::vec2(3.0, 1.0)), crate::theme::cr(crate::theme::Rad::Sm), egui::Stroke::new(1.5, c), egui::StrokeKind::Outside);
                            if std::mem::take(&mut self.scroll_sel) {
                                r.scroll_to_me(None);
                            }
                        }
                        // 한 번 = 다시 띄움(바로 송출 모드면 송출), 두 번 = 송출. 기록된 제목·번역본으로 찾는다 (control.js load)
                        match widgets::click2(ui, ("hist", h["at"].as_i64().unwrap_or(0), title), &r) {
                            widgets::Click::Double => {
                                self.sel = i;
                                if let Some(l) = load(core, h, true, acts) {
                                    self.loaded = Some(l);
                                }
                            }
                            widgets::Click::Single => {
                                self.sel = i;
                                if let Some(l) = load(core, h, false, acts) {
                                    self.loaded = Some(l);
                                }
                            }
                            widgets::Click::None => {}
                        }
                    }
                });
            });
            // 창을 누르면 키보드를 갖고, 창 밖을 누르면 놓는다 (모달이 아니라 떠 있는 창이라 다른 목록을 누를 수 있다)
            if ctx.input(|i| i.pointer.any_pressed()) {
                if win.as_ref().is_some_and(|w| w.response.contains_pointer()) {
                    crate::cue_ui::nav_set(ctx, Nav::History);
                } else {
                    crate::cue_ui::nav_release(ctx, Nav::History);
                }
            }
            self.open = open && !close;
        }
        if self.usage_open {
            let mut open = self.usage_open;
            let f = usage::Filter { from: self.from.trim().into(), to: self.to.trim().into(), kinds: vec![], q: self.q.clone() };
            crate::widgets_win::window(ctx, "usage", wp_core::i18n::tr("사용 기록 (저작권 보고)"), [720.0, 540.0]).open(&mut open).show(ctx, |ui| {
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
                    ui.label(RichText::new(wp_core::i18n::tr_dyn(&self.msg)).small());
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
            crate::widgets_win::window(ctx, "about", wp_core::i18n::tr("정보"), [520.0, 420.0]).open(&mut open).show(ctx, |ui| {
                ui.heading(wp_core::i18n::tr("워십프리젠터 (Rust 판)"));
                ui.label(wp_core::i18n::tr_args("버전 {1} · 라이선스 GPL-3.0-or-later", &[&(env!("CARGO_PKG_VERSION"))]));
                ui.separator();
                ui.label(wp_core::i18n::tr("가져다 쓴 공개 코드·글꼴"));
                egui::ScrollArea::vertical().show(ui, |ui| {
                    // 마크다운 표 · 링크 기호를 빼고 글로 (머리는 굵게, 표 한 줄은 '이름 · 버전 · 라이선스')
                    for (head, line) in plain_notices(include_str!("../../../THIRD_PARTY_NOTICES.md")) {
                        if head {
                            ui.add_space(4.0);
                            ui.label(RichText::new(line).strong());
                        } else if line.is_empty() {
                            ui.add_space(2.0);
                        } else {
                            ui.add(egui::Label::new(RichText::new(line).small()).wrap());
                        }
                    }
                    ui.separator();
                    ui.label(RichText::new("Rust: egui/eframe (MIT/Apache-2.0) · wry/tao (MIT/Apache-2.0) · axum·tokio (MIT) · rusqlite (MIT) · ab_glyph (Apache-2.0)").small());
                });
            });
            self.about_open = open;
        }
    }
}

/// THIRD_PARTY_NOTICES.md → (머리인가, 글) 줄. 표 구분 줄은 빼고, 칸은 ' · ' 로, [글](주소) 는 '글 (주소)' 로, ** · ` 는 뺀다
fn plain_notices(md: &str) -> Vec<(bool, String)> {
    let link = |t: &str| -> String {
        let mut out = String::new();
        let mut rest = t;
        while let Some(a) = rest.find('[') {
            let Some(b) = rest[a..].find("](").map(|x| a + x) else { break };
            let Some(c) = rest[b..].find(')').map(|x| b + x) else { break };
            out.push_str(&rest[..a]);
            out.push_str(&rest[a + 1..b]);
            out.push_str(" (");
            out.push_str(&rest[b + 2..c]);
            out.push(')');
            rest = &rest[c + 1..];
        }
        out.push_str(rest);
        out.replace("**", "").replace('`', "")
    };
    let mut v = vec![];
    for l in md.lines() {
        let t = l.trim();
        if t.starts_with('|') {
            let cells: Vec<&str> = t.trim_matches('|').split('|').map(|c| c.trim()).collect();
            if cells.iter().all(|c| c.chars().all(|ch| ch == '-' || ch == ':' || ch == ' ')) {
                continue;
            }
            v.push((false, link(&cells.join(" · "))));
        } else if let Some(h) = t.strip_prefix('#') {
            v.push((true, link(h.trim_start_matches('#').trim())));
        } else {
            v.push((false, link(t)));
        }
    }
    v
}

#[cfg(test)]
mod notice_tests {
    #[test]
    fn 라이선스_목록은_표_기호_없이_보인다() {
        let v = super::plain_notices("# 제목
| 이름 | 라이선스 |
|---|---|
| [egui](https://x.y) | MIT |");
        assert_eq!(v[0], (true, "제목".to_string()));
        assert_eq!(v[1].1, "이름 · 라이선스");
        assert_eq!(v[2].1, "egui (https://x.y) · MIT");
        assert!(v.iter().all(|(_, l)| !l.contains('|')));
    }
}
