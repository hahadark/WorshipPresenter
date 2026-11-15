//! [갈래 C] 큐시트 관리 창 (Electron control/cuemanager.js).
//!  - 위: ＋ 새 큐시트(기본 순서) · 빈 큐시트 · 서식에서… · 주보로 (AI) · 가져오기… · ⏰ 예배 진행 시계
//!  - 왼쪽: 찾기 · 보기(전체·고정·서식·휴지통) · 정렬 · 모두 고르기·고른 것 내보내기/휴지통 · 목록
//!    (한 번 = 고르기, 두 번 = 그 큐시트 열기 — 휴지통 보기 제외)
//!  - 오른쪽: 고른 큐시트의 이름·날짜·메모·고정·서식, 열기·다음 주로 복제·그대로 복제·순서지 복사·내보내기·휴지통, 순서 미리보기
//! 코어는 wp_core::cuemgr.
use crate::app::App;
use crate::theme::{pal, rich, Weight};
use crate::widgets::{self, Btn, Click, Kind, Size};
use eframe::egui::{self, RichText};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use wp_core::cuemgr;
use wp_core::i18n::{tr, tr_args};

#[derive(Clone, Default)]
struct St {
    init: bool,
    /// 전체 all · 고정 pinned · 서식 template · 휴지통 trash
    view: String,
    /// date · updated · name
    sort: String,
    q: String,
    sel: String,
    checked: BTreeSet<String>,
    /// 오른쪽 입력칸이 보이는 큐시트 (바뀌면 칸을 다시 채운다)
    form_id: String,
    name: String,
    date: String,
    start: String,
    memo: String,
}

fn sid() -> egui::Id {
    egui::Id::new("wp-cuemgr")
}

/// 창을 열 때: 고를 큐시트 (없으면 열린 큐시트)
pub(crate) fn open_with(ctx: &egui::Context, select: &str) {
    let mut st: St = ctx.data(|d| d.get_temp(sid())).unwrap_or_default();
    st.sel = select.to_string();
    st.checked.clear();
    st.form_id.clear();
    ctx.data_mut(|d| d.insert_temp(sid(), st));
}

fn ok(app: &App, msg: String) {
    app.core.lock().notify(msg, "ok");
}
fn fail(app: &App, e: impl std::fmt::Display) {
    app.core.lock().notify(e.to_string(), "error");
}

/// 파일 이름에 못 쓰는 글자 바꾸기 (cuemanager.js download)
fn file_name(name: &str) -> String {
    name.chars().map(|c| if "\\/:*?\"<>|".contains(c) { '_' } else { c }).collect()
}

impl App {
    /// 그 큐시트를 열고 관리 창을 닫는다 (cuemanager.js openCue)
    fn cm_open_cue(&mut self, id: &str, open: &mut bool) {
        self.core.lock().open_cue(id);
        self.sel = None;
        self.edit = None;
        *open = false;
    }

    pub(crate) fn cuemgr_window(&mut self, ctx: &egui::Context, open: &mut bool) {
        if !*open {
            // 닫혔다: 다음에 열면 열린 큐시트를 고르게
            let mut st: St = ctx.data(|d| d.get_temp(sid())).unwrap_or_default();
            if st.init {
                st.init = false;
                st.sel.clear();
                ctx.data_mut(|d| d.insert_temp(sid(), st));
            }
            return;
        }
        let mut st: St = ctx.data(|d| d.get_temp(sid())).unwrap_or_default();
        let current = self.core.lock().cue.as_ref().map(|c| c.id.clone()).unwrap_or_default();
        if !st.init {
            st.init = true;
            let saved = self.core.lock().settings.get()["cueMgr"].clone();
            st.view = saved["view"].as_str().filter(|v| ["all", "pinned", "template", "trash"].contains(v)).unwrap_or("all").to_string();
            st.sort = saved["sort"].as_str().filter(|v| ["date", "updated", "name"].contains(v)).unwrap_or("date").to_string();
            if st.sel.is_empty() {
                st.sel = current.clone();
            }
            st.checked.clear();
            st.form_id.clear();
        }
        let (all, trash) = {
            let c = self.core.lock();
            (c.cue_list(false), c.cue_list(true))
        };
        let list = cuemgr::rows(&all, &trash, &st.view, &st.sort, &st.q);
        let pool: &Vec<Value> = if st.view == "trash" { &trash } else { &all };
        if !pool.iter().any(|c| c["id"] == st.sel.as_str()) {
            st.sel = list.first().and_then(|c| c["id"].as_str()).unwrap_or("").to_string();
        }
        let sel_cue = pool.iter().find(|c| c["id"] == st.sel.as_str()).cloned();
        st.checked.retain(|id| pool.iter().any(|c| c["id"] == id.as_str()));
        let times_on = self.core.lock().settings.get()["cueTimes"] == true;
        let remember = |app: &App, st: &St| {
            let (v, s) = (st.view.clone(), st.sort.clone());
            app.core.lock().settings.update(|d| d["cueMgr"] = json!({ "view": v, "sort": s }));
        };
        let p = pal();
        let mut keep = *open;
        widgets::float_window(ctx, "cues", tr("큐시트 관리"), [900.0, 600.0]).open(&mut keep).show(ctx, |ui| {
            // ---------------- 위 ----------------
            ui.horizontal_wrapped(|ui| {
                let b = ui.add(Btn::new(tr("새 큐시트")).icon("plus").kind(Kind::Primary).tooltip(tr("오늘 날짜로 기본 예배 순서가 든 큐시트를 만들어 엽니다")));
                widgets::mark("cm-new", b.rect);
                if b.clicked() {
                    let r = self.core.lock().new_cue_default(false); // 잠금을 먼저 푼다 (가지 안에서 다시 잠그므로)
                    match r {
                        Ok(_) => *open = false,
                        Err(e) => fail(self, e),
                    }
                    self.sel = None;
                    self.edit = None;
                }
                let b = ui.add(Btn::new(tr("빈 큐시트")).tooltip(tr("순서가 없는 큐시트를 만들어 엽니다")));
                widgets::mark("cm-new-empty", b.rect);
                if b.clicked() {
                    let r = self.core.lock().new_cue_default(true); // 잠금을 먼저 푼다 (가지 안에서 다시 잠그므로)
                    match r {
                        Ok(_) => *open = false,
                        Err(e) => fail(self, e),
                    }
                    self.sel = None;
                    self.edit = None;
                }
                let templates: Vec<&Value> = all.iter().filter(|c| c["template"] == true).collect();
                if !templates.is_empty() {
                    let mut pick: Option<String> = None;
                    egui::ComboBox::from_id_salt("cm-from-tpl").selected_text(tr("서식에서…")).show_ui(ui, |ui| {
                        for t in &templates {
                            let r = ui.selectable_label(false, t["name"].as_str().unwrap_or(""));
                            widgets::mark(&format!("cm-tpl-{}", t["name"].as_str().unwrap_or("")), r.rect);
                            if r.clicked() {
                                pick = t["id"].as_str().map(String::from);
                            }
                        }
                    });
                    if let Some(id) = pick {
                        let r = self.core.lock().new_cue_from_template(&id); // 잠금을 먼저 푼다 (가지 안에서 다시 잠그므로)
                        match r {
                            Ok(_) => *open = false,
                            Err(e) => fail(self, e),
                        }
                        self.sel = None;
                        self.edit = None;
                    }
                }
                if ui.add(Btn::new(tr("주보로 (AI)")).icon("sparkles").tooltip(tr("주보 글·사진에서 예배 순서를 읽어 큐시트를 만듭니다"))).clicked() {
                    *open = false;
                    crate::trackd_ui::handle_menu("d:bulletin");
                }
                if ui.add(Btn::new(tr("가져오기…")).icon("folder-open").tooltip(tr("내보낸 큐시트 파일(.json)을 가져옵니다 (여러 개 고를 수 있음)"))).clicked() {
                    if let Some(fs) = rfd::FileDialog::new().add_filter(tr("큐시트"), &["json"]).pick_files() {
                        let (n, first, errs) = self.core.lock().import_cue_files(&fs);
                        for e in errs {
                            fail(self, e);
                        }
                        if n > 0 {
                            ok(self, tr_args("큐시트 {1}개를 가져왔습니다.", &[&n]));
                            st.view = "all".into();
                            st.sel = first;
                        }
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut on = times_on;
                    let r = ui.checkbox(&mut on, tr("⏰ 예배 진행 시계")).on_hover_text(tr("큐시트 위 ⏰ 시작 시각 · 순서별 예정 시간 · 몇 분 늦음"));
                    widgets::mark("cm-times", r.rect);
                    if r.changed() {
                        self.core.lock().settings.update(|d| d["cueTimes"] = json!(on));
                    }
                });
            });
            ui.separator();
            // ---------------- 왼쪽 · 오른쪽 ----------------
            let left_w = (ui.available_width() * 0.46).clamp(300.0, 420.0);
            let h = ui.available_height().max(360.0);
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(egui::vec2(left_w, h), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_width(left_w);
                    let r = ui.add(egui::TextEdit::singleline(&mut st.q).hint_text(tr("이름 · 날짜 · 메모 · 순서 제목으로 찾기")).desired_width(f32::INFINITY));
                    widgets::mark("cm-q", r.rect);
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        let n = |k: &str| match k {
                            "pinned" => all.iter().filter(|c| c["pinned"] == true).count(),
                            "template" => all.iter().filter(|c| c["template"] == true).count(),
                            "trash" => trash.len(),
                            _ => all.len(),
                        };
                        for (k, l) in [("all", "전체"), ("pinned", "고정"), ("template", "서식"), ("trash", "휴지통")] {
                            let b = ui.add(Btn::new(format!("{} {}", tr(l), n(k))).size(Size::Sm).selected(st.view == k));
                            widgets::mark(&format!("cm-view-{k}"), b.rect);
                            if b.clicked() && st.view != k {
                                st.view = k.into();
                                st.checked.clear();
                                remember(self, &st);
                            }
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        let label = |k: &str| tr(match k {
                            "updated" => "최근 고친 순",
                            "name" => "이름순",
                            _ => "예배 날짜순",
                        });
                        let before = st.sort.clone();
                        egui::ComboBox::from_id_salt("cm-sort").selected_text(label(&st.sort)).width(110.0).show_ui(ui, |ui| {
                            for k in ["date", "updated", "name"] {
                                ui.selectable_value(&mut st.sort, k.to_string(), label(k));
                            }
                        });
                        if before != st.sort {
                            remember(self, &st);
                        }
                        let mut every = !list.is_empty() && list.iter().all(|c| st.checked.contains(c["id"].as_str().unwrap_or("")));
                        let r = ui.checkbox(&mut every, tr("모두"));
                        widgets::mark("cm-check-all", r.rect);
                        if r.changed() {
                            for c in &list {
                                let id = c["id"].as_str().unwrap_or("").to_string();
                                if every {
                                    st.checked.insert(id);
                                } else {
                                    st.checked.remove(&id);
                                }
                            }
                        }
                        let ids: Vec<String> = st.checked.iter().cloned().collect();
                        if !ids.is_empty() {
                            ui.label(RichText::new(tr_args("{1}개", &[&ids.len()])).strong());
                            if st.view == "trash" {
                                let b = ui.add(Btn::new(tr("되살리기")).size(Size::Sm));
                                widgets::mark("cm-restore-sel", b.rect);
                                if b.clicked() {
                                    let r = self.core.lock().restore_cues(&ids); // 잠금을 먼저 푼다 (가지 안에서 다시 잠그므로)
                                    match r {
                                        Ok(n) => {
                                            st.checked.clear();
                                            self.core.lock().notify(tr_args("{1}개를 되살렸습니다.", &[&n]), "ok");
                                        }
                                        Err(e) => self.core.lock().notify(e.to_string(), "error"),
                                    }
                                }
                                if widgets::confirm_delete(ui, egui::Id::new("cm-purge-sel"), &tr("아주 지우기"), &tr_args("{1}개를 아주 지울까요? 되돌릴 수 없습니다.", &[&ids.len()])) {
                                    let r = self.core.lock().purge_cue_ids(&ids);
                                    if let Err(e) = r {
                                        fail(self, e);
                                    }
                                    st.checked.clear();
                                }
                            } else {
                                if ui.add(Btn::new(tr("내보내기")).size(Size::Sm)).clicked() {
                                    let name = file_name(&tr_args("큐시트 {1}개 ({2})", &[&ids.len(), &cuemgr::today()]));
                                    if let Some(f) = rfd::FileDialog::new().set_file_name(format!("{name}.json")).save_file() {
                                        let r = self.core.lock().export_cues_to(&ids, &f); // 잠금을 먼저 푼다 (가지 안에서 다시 잠그므로)
                                        match r {
                                            Ok(n) => self.core.lock().notify(tr_args("큐시트 {1}개를 파일 하나로 내보냈습니다.", &[&n]), "ok"),
                                            Err(e) => self.core.lock().notify(e.to_string(), "error"),
                                        }
                                    }
                                }
                                let yes = widgets::confirm_delete(ui, egui::Id::new("cm-trash-sel"), &tr("휴지통"), &tr_args("고른 큐시트 {1}개를 휴지통으로 옮길까요?", &[&ids.len()]));
                                widgets::mark_confirm("cm-bulk"); // 오른쪽의 낱개 휴지통 단추와 가려 누르게
                                if yes {
                                    let r = self.core.lock().trash_cues(&ids); // 잠금을 먼저 푼다 (가지 안에서 다시 잠그므로)
                                    match r {
                                        Ok(n) => self.core.lock().notify(tr_args("{1}개를 휴지통으로 옮겼습니다.", &[&n]), "ok"),
                                        Err(e) => self.core.lock().notify(e.to_string(), "error"),
                                    }
                                    st.checked.clear();
                                    self.sel = None;
                                    self.edit = None;
                                }
                            }
                        }
                    });
                    ui.add_space(2.0);
                    let now = wp_core::util::now_ms();
                    let mut open_id: Option<String> = None;
                    egui::ScrollArea::vertical().id_salt("cm-list").auto_shrink([false, false]).show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        if list.is_empty() {
                            let t = if st.view == "trash" { tr("휴지통이 비어 있습니다.") } else if !st.q.trim().is_empty() { tr("찾는 큐시트가 없습니다.") } else { tr("큐시트가 없습니다.") };
                            ui.add_space(16.0);
                            ui.label(rich(t, 13.0, Weight::Regular, p.muted));
                        }
                        for c in &list {
                            let id = c["id"].as_str().unwrap_or("").to_string();
                            let name = c["name"].as_str().unwrap_or("");
                            let is_cur = id == current;
                            let w = ui.available_width();
                            let (rect, row) = ui.allocate_exact_size(egui::vec2(w, 44.0), egui::Sense::click());
                            widgets::mark(&format!("cm-row-{name}"), rect);
                            let hot = row.hovered();
                            if ui.is_rect_visible(rect) {
                                let bg = if id == st.sel { p.sel } else if hot { p.hover } else { egui::Color32::TRANSPARENT };
                                ui.painter().rect_filled(rect, egui::CornerRadius::same(6), bg);
                                if is_cur {
                                    ui.painter().rect_filled(egui::Rect::from_min_size(rect.min, egui::vec2(3.0, rect.height())), 0.0, p.accent);
                                }
                                let x = rect.left() + 34.0;
                                let mut head = String::new();
                                if c["pinned"] == true {
                                    head.push_str("📌 ");
                                }
                                if c["template"] == true {
                                    head.push_str("📄 ");
                                }
                                head.push_str(name);
                                let when = if st.view == "trash" { tr_args("지운 지 {1}", &[&cuemgr::ago(c["deletedAt"].as_str().unwrap_or(""), now)]) } else { cuemgr::ago(c["updatedAt"].as_str().unwrap_or(""), now) };
                                let wg = ui.painter().layout_no_wrap(when, crate::theme::font(11.5, Weight::Regular), p.muted);
                                let right = rect.right() - 8.0 - wg.size().x;
                                ui.painter().galley(egui::pos2(right, rect.top() + 6.0), wg, p.muted);
                                let clip = egui::Rect::from_min_max(egui::pos2(x, rect.top()), egui::pos2(right - 6.0, rect.bottom()));
                                let pt = ui.painter().with_clip_rect(clip);
                                let hg = pt.layout_no_wrap(head, crate::theme::font(13.5, Weight::SemiBold), p.text);
                                let hw = hg.size().x;
                                pt.galley(egui::pos2(x, rect.top() + 5.0), hg, p.text);
                                if is_cur {
                                    let og = pt.layout_no_wrap(tr("열림"), crate::theme::font(11.0, Weight::SemiBold), p.accent);
                                    pt.galley(egui::pos2(x + hw + 8.0, rect.top() + 7.0), og, p.accent);
                                }
                                let date = c["serviceDate"].as_str().unwrap_or("");
                                let d = if date.is_empty() { tr("날짜 없음") } else { format!("{date} ({})", tr(cuemgr::day_of(date))) };
                                let sub = format!("{d} · {}{}", tr_args("{1}개 순서", &[&c["count"]]), if c["memo"].as_str().is_some_and(|m| !m.is_empty()) { " · 📝" } else { "" });
                                let sg = pt.layout_no_wrap(sub, crate::theme::font(11.5, Weight::Regular), p.muted);
                                pt.galley(egui::pos2(x, rect.top() + 24.0), sg, p.muted);
                            }
                            // 고르기 칸 (줄 위에 놓아 줄 클릭보다 먼저 받는다)
                            let cb_rect = egui::Rect::from_center_size(egui::pos2(rect.left() + 17.0, rect.center().y), egui::vec2(20.0, 20.0));
                            let mut on = st.checked.contains(&id);
                            let cb = ui.put(cb_rect, egui::Checkbox::without_text(&mut on));
                            widgets::mark(&format!("cm-chk-{name}"), cb_rect);
                            if cb.changed() {
                                if on {
                                    st.checked.insert(id.clone());
                                } else {
                                    st.checked.remove(&id);
                                }
                            }
                            // cuemanager.js:171-173 한 번 = 고르기 · 두 번 = 그 큐시트 열기 (휴지통 보기 제외)
                            match widgets::click2(ui, ("cm-row", id.as_str()), &row) {
                                Click::Single => st.sel = id.clone(),
                                Click::Double => {
                                    st.sel = id.clone();
                                    if st.view != "trash" {
                                        open_id = Some(id.clone());
                                    }
                                }
                                Click::None => {}
                            }
                        }
                    });
                    if let Some(id) = open_id {
                        self.cm_open_cue(&id, open);
                    }
                });
                ui.separator();
                ui.vertical(|ui| {
                    match &sel_cue {
                        Some(c) => self.cm_detail(ui, &mut st, c, &current, times_on, open),
                        None => {
                            ui.add_space(24.0);
                            ui.label(rich(tr("왼쪽에서 큐시트를 고르세요."), 13.0, Weight::Regular, p.muted));
                        }
                    }
                });
            });
        });
        *open &= keep;
        ctx.data_mut(|d| d.insert_temp(sid(), st));
    }

    /// 오른쪽: 고른 큐시트 (cuemanager.js detail)
    fn cm_detail(&mut self, ui: &mut egui::Ui, st: &mut St, c: &Value, current: &str, times_on: bool, open: &mut bool) {
        let p = pal();
        let id = c["id"].as_str().unwrap_or("").to_string();
        let s = |k: &str| c[k].as_str().unwrap_or("").to_string();
        let trashed = !s("deletedAt").is_empty();
        if st.form_id != id {
            st.form_id = id.clone();
            st.name = s("name");
            st.date = s("serviceDate");
            st.start = s("startTime");
            st.memo = s("memo");
        }
        let mut meta = json!({});
        let mut changed = false;
        // 입력칸은 다 쓰고 나갈 때(Enter·다른 곳 누름) 저장 — 원본 'change' 사건과 같게
        let done = |r: &egui::Response| r.lost_focus();
        egui::Grid::new("cm-form").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
            ui.label(tr("이름"));
            let r = ui.add_enabled(!trashed, egui::TextEdit::singleline(&mut st.name).desired_width(f32::INFINITY));
            widgets::mark("cm-name", r.rect);
            if done(&r) && st.name.trim() != s("name") && !st.name.trim().is_empty() {
                meta["name"] = json!(st.name.trim());
                changed = true;
            }
            ui.end_row();
            ui.label(tr("예배 날짜"));
            ui.horizontal(|ui| {
                let r = ui.add_enabled(!trashed, egui::TextEdit::singleline(&mut st.date).desired_width(110.0).hint_text("2026-11-15"));
                widgets::mark("cm-date", r.rect);
                let day = cuemgr::day_of(&st.date);
                if !day.is_empty() {
                    ui.label(rich(tr_args("{1}요일", &[&tr(day)]), 12.0, Weight::Regular, p.muted));
                } else if !st.date.trim().is_empty() {
                    ui.label(rich(tr("2026-11-15 처럼 쓰세요"), 12.0, Weight::Regular, p.warn));
                }
                if done(&r) && st.date.trim() != s("serviceDate") && (st.date.trim().is_empty() || !day.is_empty()) {
                    meta["serviceDate"] = json!(st.date.trim());
                    changed = true;
                }
            });
            ui.end_row();
            if times_on && !trashed {
                ui.label(tr("시작 시각"));
                let r = ui.add(egui::TextEdit::singleline(&mut st.start).desired_width(110.0).hint_text("11:00"));
                if done(&r) && st.start.trim() != s("startTime") {
                    meta["startTime"] = json!(st.start.trim());
                    changed = true;
                }
                ui.end_row();
            }
            ui.label(tr("메모"));
            let r = ui.add_enabled(!trashed, egui::TextEdit::multiline(&mut st.memo).desired_rows(2).desired_width(f32::INFINITY).hint_text(tr("예: 성찬 주일 · 찬양팀 B · 순서지 인쇄 완료")));
            widgets::mark("cm-memo", r.rect);
            if done(&r) && st.memo != s("memo") {
                meta["memo"] = json!(st.memo);
                changed = true;
            }
            ui.end_row();
        });
        if !trashed {
            ui.horizontal_wrapped(|ui| {
                let mut pin = c["pinned"] == true;
                let r = ui.checkbox(&mut pin, tr("📌 맨 위에 고정"));
                widgets::mark("cm-pin", r.rect);
                if r.changed() {
                    meta["pinned"] = json!(pin);
                    changed = true;
                }
                let mut tpl = c["template"] == true;
                let r = ui.checkbox(&mut tpl, tr("📄 서식으로 쓰기")).on_hover_text(tr("새 큐시트를 만들 때 바탕으로 고를 수 있습니다 (예: 주일 낮 예배 · 수요 예배 · 새벽 예배 순서)"));
                widgets::mark("cm-tpl", r.rect);
                if r.changed() {
                    meta["template"] = json!(tpl);
                    changed = true;
                }
            });
        }
        if changed {
            // 시작 시각은 진행 시계 칸이라 따로 (코어 검증 목록 밖)
            if let Some(t) = meta["startTime"].as_str() {
                let _ = wp_core::cues::update_meta(&self.core.lock().store, &id, &json!({ "startTime": t }));
                if id == current {
                    if let Some(q) = self.core.lock().cue.as_mut() {
                        q.start_time = t.to_string();
                    }
                }
            }
            let r = self.core.lock().update_cue_meta_of(&id, &meta);
            if let Err(e) = r {
                fail(self, e);
            }
        }
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            if trashed {
                let b = ui.add(Btn::new(tr("되살리기")).icon("undo-2").kind(Kind::Primary));
                widgets::mark("cm-restore", b.rect);
                if b.clicked() {
                    let r = self.core.lock().restore_cues(&[id.clone()]); // 잠금을 먼저 푼다 (가지 안에서 다시 잠그므로)
                    match r {
                        Ok(_) => {
                            st.view = "all".into();
                            let (v, so) = (st.view.clone(), st.sort.clone());
                            self.core.lock().settings.update(|d| d["cueMgr"] = json!({ "view": v, "sort": so }));
                            self.core.lock().notify(tr("되살렸습니다."), "ok");
                        }
                        Err(e) => self.core.lock().notify(e.to_string(), "error"),
                    }
                }
                if widgets::confirm_delete(ui, egui::Id::new(("cm-purge", id.as_str())), &tr("아주 지우기"), &tr("아주 지울까요? 되돌릴 수 없습니다.")) {
                    let r = self.core.lock().purge_cue_ids(&[id.clone()]); // 잠금을 먼저 푼다 (가지 안에서 다시 잠그므로)
                    match r {
                        Ok(_) => self.core.lock().notify(tr("아주 지웠습니다."), "ok"),
                        Err(e) => self.core.lock().notify(e.to_string(), "error"),
                    }
                }
                ui.label(rich(tr("휴지통의 큐시트는 30일 뒤 저절로 지워집니다"), 12.0, Weight::Regular, p.muted));
                return;
            }
            let b = ui.add(Btn::new(tr("열기")).kind(Kind::Primary).tooltip(tr("이 큐시트를 엽니다 (목록에서 두 번 눌러도)")));
            widgets::mark("cm-open", b.rect);
            if b.clicked() {
                self.cm_open_cue(&id, open);
            }
            let b = ui.add(Btn::new(tr("다음 주 순서로 복제")).icon("copy").tooltip(tr("날짜를 7일 뒤로 바꾼 복사본 (이름 속 날짜도)")));
            widgets::mark("cm-dup-next", b.rect);
            if b.clicked() {
                let r = self.core.lock().duplicate_cue_as(&id, None, None, true); // 잠금을 먼저 푼다 (가지 안에서 다시 잠그므로)
                match r {
                    Ok(n) => {
                        st.sel = n.id.clone();
                        self.core.lock().notify(tr_args("'{1}' 를 만들었습니다.", &[&n.name]), "ok");
                    }
                    Err(e) => self.core.lock().notify(e.to_string(), "error"),
                }
            }
            let b = ui.add(Btn::new(tr("그대로 복제")).icon("copy"));
            widgets::mark("cm-dup", b.rect);
            if b.clicked() {
                let r = self.core.lock().duplicate_cue_as(&id, None, None, false); // 잠금을 먼저 푼다 (가지 안에서 다시 잠그므로)
                match r {
                    Ok(n) => {
                        st.sel = n.id;
                        self.core.lock().notify(tr("복제했습니다."), "ok");
                    }
                    Err(e) => self.core.lock().notify(e.to_string(), "error"),
                }
            }
            let b = ui.add(Btn::new(tr("순서지 복사")).icon("clipboard-copy").tooltip(tr("순서지를 글로 복사 (단톡방·문서에 붙이기)")));
            widgets::mark("cm-text", b.rect);
            if b.clicked() {
                let r = self.core.lock().cue_text_of(&id);
                if let Some(t) = r {
                    widgets::copy_notice(ui.ctx(), t, tr("순서지를 글로 복사했습니다 — 단톡방·문서에 붙여 넣으세요."));
                }
            }
            if ui.add(Btn::new(tr("내보내기")).icon("download")).clicked() {
                if let Some(f) = rfd::FileDialog::new().set_file_name(format!("{}.json", file_name(&s("name")))).save_file() {
                    let r = self.core.lock().export_cues_to(&[id.clone()], &f); // 잠금을 먼저 푼다 (가지 안에서 다시 잠그므로)
                    match r {
                        Ok(_) => self.core.lock().notify(tr("내보냈습니다."), "ok"),
                        Err(e) => self.core.lock().notify(e.to_string(), "error"),
                    }
                }
            }
            if widgets::confirm_delete(ui, egui::Id::new(("cm-trash", id.as_str())), &tr("휴지통"), &tr_args("'{1}' 을(를) 휴지통으로 옮길까요? (30일 안에 되살릴 수 있습니다)", &[&s("name")])) {
                let r = self.core.lock().trash_cues(&[id.clone()]); // 잠금을 먼저 푼다 (가지 안에서 다시 잠그므로)
                match r {
                    Ok(_) => self.core.lock().notify(tr("휴지통으로 옮겼습니다."), "ok"),
                    Err(e) => self.core.lock().notify(e.to_string(), "error"),
                }
                if id == current {
                    self.sel = None;
                    self.edit = None;
                }
            }
        });
        ui.add_space(6.0);
        // 순서 미리보기 (.cm-prev)
        let count = c["count"].as_u64().unwrap_or(0) as usize;
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr_args("순서 {1}개", &[&count])).strong());
            if times_on && !s("startTime").is_empty() {
                ui.label(rich(tr_args("· 시작 {1}", &[&s("startTime")]), 12.0, Weight::Regular, p.muted));
            }
        });
        let prev = c["preview"].as_array().cloned().unwrap_or_default();
        let is_open = id == current;
        egui::ScrollArea::vertical().id_salt("cm-prev").auto_shrink([false, false]).show(ui, |ui| {
            for (i, x) in prev.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(rich(format!("{}.", i + 1), 12.0, Weight::Regular, p.muted));
                    let ty = x["type"].as_str().unwrap_or("");
                    ui.add(crate::icons::image(crate::icons::for_type(ty), 14.0, crate::theme::type_ink(ty)));
                    ui.label(x["title"].as_str().unwrap_or(""));
                    let sub = x["sub"].as_str().unwrap_or("");
                    if !sub.is_empty() {
                        ui.label(rich(format!("— {sub}"), 12.0, Weight::Regular, p.muted));
                    }
                    // 진행 시계: 열린 큐시트면 순서별 예정 시간(분)을 여기서 고친다
                    if times_on && is_open {
                        let it = self.core.lock().cue.as_ref().and_then(|q| q.items.get(i)).cloned();
                        if let Some(it) = it {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| crate::trackb_ui::duration_editor(self, ui, i, &it));
                        }
                    }
                });
            }
            if count > prev.len() {
                ui.label(rich(tr_args("… {1}개 더", &[&(count - prev.len())]), 12.0, Weight::Regular, p.muted));
            }
        });
    }
}
