//! AI 도우미 창들의 공통 부분 + 예배 전 큐시트 점검
//!  - 'ai:settings' AI 설정 (키 · 제공자 · 모델 · 주소 · 기능별 동의)
//!  - 동의 창: 글을 밖으로 보내는 기능을 처음 쓸 때 무엇이 어디로 가는지 알린다 (lyrics_ui '인터넷 자료 사용 안내' 와 같은 방식)
//!  - 'ai:check' 예배 전 큐시트 점검 (규칙 · 주보 비교 · AI 교정)

use crate::app::App;
use crate::widgets::{Btn, Kind, Size};
use eframe::egui::{self, RichText};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde_json::Value;
use wp_core::cuecheck::Finding;
use wp_core::i18n::{tr, tr_args, tr_s};

#[derive(Default)]
struct Settings {
    open: bool,
    key: String,
    model: String,
    endpoint: String,
    loaded: bool,
}

#[derive(Default)]
struct Check {
    open: bool,
    bulletin: String,
    show_bulletin: bool,
    found: Vec<Finding>,
    ai_found: Vec<Finding>,
    ran: bool,
    busy: bool,
    note: String,
    hide_info: bool,
    /// 점검한 큐시트 (바뀌면 다시 점검)
    cue_id: String,
}

#[derive(Default)]
struct St {
    settings: Settings,
    check: Check,
    /// 동의를 묻는 중인 기능 (aihelp::FEATURES 의 이름)
    consent: Option<String>,
    /// 방금 동의한 기능 — 그 창이 한 번 가져가 하던 일을 이어서
    granted: Option<String>,
}

static ST: Lazy<Mutex<St>> = Lazy::new(|| Mutex::new(St::default()));

pub fn handle_menu(id: &str) -> bool {
    if crate::sermon_ui::handle_menu(id) {
        return true;
    }
    let mut s = ST.lock();
    match id {
        "ai:settings" => {
            s.settings.open = true;
            s.settings.loaded = false;
        }
        "ai:check" => {
            s.check.open = true;
            s.check.ran = false;
        }
        _ => return false,
    }
    true
}

/// 이 기능의 AI 를 써도 되나: 키가 없으면 안내, 동의 전이면 동의 창을 띄우고 false
pub fn ready(app: &App, feature: &str) -> bool {
    let c = app.core.lock();
    if !c.ai_cfg().has_key() && std::env::var("WP_AI_MOCK").is_err() {
        drop(c);
        app.core.lock().notify(tr(wp_core::aihelp::NEED_KEY), "warn");
        ST.lock().settings.open = true;
        return false;
    }
    if c.ai_consented(feature) {
        return true;
    }
    drop(c);
    ST.lock().consent = Some(feature.to_string());
    false
}

/// 방금 이 기능에 동의했나 (한 번만 true)
pub fn take_granted(feature: &str) -> bool {
    let mut s = ST.lock();
    if s.granted.as_deref() == Some(feature) {
        s.granted = None;
        true
    } else {
        false
    }
}

/// AI 단추 (키가 없으면 흐리게 + 'AI 키가 필요합니다')
pub fn ai_button(ui: &mut egui::Ui, app: &App, label: &str, busy: bool) -> bool {
    let has = app.core.lock().ai_cfg().has_key() || std::env::var("WP_AI_MOCK").is_ok();
    let text = if busy { tr("AI 가 읽는 중…") } else { tr(label) };
    let r = ui.add(Btn::new(text).icon("sparkles").enabled(!busy).tooltip(if has { String::new() } else { tr("AI 키가 필요합니다") }));
    if r.clicked() && !has {
        app.core.lock().notify(tr(wp_core::aihelp::NEED_KEY), "warn");
        ST.lock().settings.open = true;
        return false;
    }
    r.clicked() && !busy
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        open_from_args();
        crate::sermon_ui::open_from_args();
    });
    consent_win(app, ctx);
    crate::sermon_ui::show(app, ctx);
    settings_win(app, ctx);
    check_win(app, ctx);
}

fn consent_win(app: &mut App, ctx: &egui::Context) {
    let Some(feature) = ST.lock().consent.clone() else { return };
    let cfg = app.core.lock().ai_cfg();
    let mut open = true;
    let mut agreed = false;
    let mut cancel = false;
    crate::widgets::float_window(ctx, "ai-consent", tr("AI 로 글 보내기 안내"), [480.0, 300.0]).open(&mut open).show(ctx, |ui| {
        ui.label(RichText::new(tr(wp_core::aihelp::feature_name(&feature))).strong());
        ui.add_space(4.0);
        ui.label(tr_args("이 기능은 {1} 을(를) AI 제공자에게 보냅니다.", &[&tr(wp_core::aihelp::feature_what(&feature))]));
        ui.label(tr_args("보내는 곳: {1} (모델 {2})", &[&cfg.host(), &cfg.model()]));
        ui.add_space(4.0);
        ui.label(RichText::new(tr("· 음성은 PC 밖으로 보내지 않습니다 (음성 인식은 이 PC 안에서 합니다).\n· 보낸 글은 그 제공자의 약관에 따라 처리됩니다. 개인 정보(이름·연락처)가 든 글은 보내기 전에 지우세요.\n· 요금은 쓴 만큼 그 제공자 계정에서 나갑니다.\n· 동의는 도구 → AI 설정에서 언제든지 끌 수 있습니다.")).small());
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.add(Btn::new(tr("동의하고 계속")).kind(Kind::Primary)).clicked() {
                agreed = true;
            }
            if ui.button(tr("취소")).clicked() {
                cancel = true;
            }
        });
    });
    if agreed {
        app.core.lock().set_ai_consent(&feature, true);
        let mut s = ST.lock();
        s.consent = None;
        s.granted = Some(feature);
    } else if cancel || !open {
        ST.lock().consent = None;
    }
}

fn settings_win(app: &mut App, ctx: &egui::Context) {
    let mut st = ST.lock();
    if !st.settings.open {
        return;
    }
    if !st.settings.loaded {
        let c = app.core.lock().ai_cfg();
        st.settings.model = c.model.clone();
        st.settings.endpoint = c.endpoint.clone();
        st.settings.loaded = true;
    }
    let mut open = true;
    let s = &mut st.settings;
    let core = app.core.clone();
    crate::widgets::float_window(ctx, "ai-settings", tr("AI 설정"), [560.0, 520.0]).open(&mut open).show(ctx, |ui| {
        let cfg = core.lock().ai_cfg();
        ui.label(RichText::new(tr("주보 → 큐시트 · PPT 나누기 · 설교 포인트 · 예배 전 점검 · 가사 정리 · 자막 번역 · 예배 후 정리 · 말로 시키기가 이 키를 함께 씁니다. 키가 없어도 규칙으로 하는 기능은 모두 됩니다.")).small().color(crate::theme::pal().muted));
        ui.add_space(4.0);
        ui.label(RichText::new(tr("키")).strong());
        ui.horizontal_wrapped(|ui| {
            if cfg.has_key() {
                ui.label(RichText::new(tr(if cfg!(target_os = "macos") { "키: 저장됨 (맥 키체인에 보관)" } else { "키: 저장됨 (Windows 보호 기능으로 암호화)" })).small().color(crate::theme::pal().ok_text));
                if ui.add(Btn::new(tr("키 지우기")).size(Size::Sm)).clicked() {
                    core.lock().set_ai(Some(""), None, None);
                }
            } else {
                ui.add(egui::TextEdit::singleline(&mut s.key).password(true).desired_width(300.0).hint_text("sk-…"));
                if ui.add(Btn::new(tr("키 저장")).kind(Kind::Primary).enabled(!s.key.trim().is_empty())).clicked() {
                    let k = std::mem::take(&mut s.key);
                    core.lock().set_ai(Some(&k), None, None);
                }
            }
        });
        ui.add_space(4.0);
        ui.label(RichText::new(tr("제공자 (OpenAI 호환 주소)")).strong());
        ui.horizontal_wrapped(|ui| {
            for (name, ep, model) in wp_core::aihelp::PROVIDERS {
                let on = cfg.endpoint() == ep;
                if ui.add(Btn::new(name).selected(on).tooltip(ep)).clicked() {
                    s.endpoint = ep.to_string();
                    s.model = model.to_string();
                    core.lock().set_ai(None, Some(model), Some(if ep == wp_core::bulletin::DEFAULT_ENDPOINT { "" } else { ep }));
                }
            }
        });
        egui::Grid::new("ai-set-grid").num_columns(2).show(ui, |ui| {
            ui.label(tr("주소"));
            ui.add(egui::TextEdit::singleline(&mut s.endpoint).desired_width(380.0).hint_text(wp_core::bulletin::DEFAULT_ENDPOINT));
            ui.end_row();
            ui.label(tr("모델"));
            ui.add(egui::TextEdit::singleline(&mut s.model).desired_width(380.0).hint_text(wp_core::bulletin::DEFAULT_MODEL));
            ui.end_row();
        });
        if ui.add(Btn::new(tr("주소 · 모델 적용"))).clicked() {
            let (m, e) = (s.model.trim().to_string(), s.endpoint.trim().to_string());
            core.lock().set_ai(None, Some(&m), Some(&e));
            core.lock().notify(tr("AI 설정을 저장했습니다."), "ok");
        }
        ui.label(RichText::new(tr_args("지금: {1} · {2}", &[&cfg.host(), &cfg.model()])).small());
        ui.separator();
        ui.label(RichText::new(tr("글을 밖으로 보내도 된다고 동의한 기능")).strong());
        for (f, name, what) in wp_core::aihelp::FEATURES {
            let mut on = core.lock().ai_consented(f);
            if ui.checkbox(&mut on, tr(name)).on_hover_text(tr_args("보내는 것: {1}", &[&tr(what)])).changed() {
                core.lock().set_ai_consent(f, on);
            }
        }
    });
    if !open {
        st.settings.open = false;
    }
}

// ---------------- 예배 전 큐시트 점검 ----------------

/// 점검 표시: 색이 아니라 모양·밝기로 (빨강=송출 중 · 노랑=다음 규칙)
///  꼭 고칠 것 = 밝은 글자색으로 채운 원 + 굵은 느낌표 · 살펴볼 것 = 흐린 테두리 원 + 느낌표 · 참고 = 작은 흐린 점
fn level_mark(ui: &mut egui::Ui, level: &str) {
    let p = crate::theme::pal();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
    let c = rect.center();
    let painter = ui.painter();
    let bang = |painter: &egui::Painter, col: egui::Color32, w: f32| {
        painter.line_segment([c + egui::vec2(0.0, -4.0), c + egui::vec2(0.0, 1.0)], egui::Stroke::new(w, col));
        painter.circle_filled(c + egui::vec2(0.0, 3.8), w * 0.6, col);
    };
    match level {
        "error" => {
            painter.circle_filled(c, 7.0, p.text);
            bang(painter, p.panel, 2.2);
        }
        "warn" => {
            painter.circle_stroke(c, 6.5, egui::Stroke::new(1.3, p.muted));
            bang(painter, p.muted, 1.5);
        }
        _ => {
            painter.circle_filled(c, 2.5, p.muted2);
        }
    }
}

fn level_text(l: &str) -> egui::Color32 {
    let p = crate::theme::pal();
    if l == "info" { p.muted } else { p.text }
}

/// 점검을 다시 돌린다 (규칙 + 주보)
fn run_rules(app: &App, c: &mut Check) {
    c.found = app.core.lock().cue_check(&c.bulletin);
    c.ran = true;
}

fn check_win(app: &mut App, ctx: &egui::Context) {
    let mut st = ST.lock();
    if !st.check.open {
        return;
    }
    let cur_id = app.core.lock().cue.as_ref().map(|q| q.id.clone()).unwrap_or_default();
    if st.check.cue_id != cur_id {
        st.check.cue_id = cur_id;
        st.check.ran = false;
        st.check.ai_found.clear();
    }
    if !st.check.ran {
        if st.check.bulletin.is_empty() {
            // '주보로 큐시트 만들기 → 순서 붙여 넣기' 에 남겨 둔 글이 있으면 그걸로 비교
            st.check.bulletin = app.core.lock().quick_draft().1;
        }
        run_rules(app, &mut st.check);
    }
    let mut goto: Option<usize> = None;
    let mut run_ai = take_granted_locked(&mut st, "cuecheck");
    let mut open = true;
    let c = &mut st.check;
    let cue_name = app.core.lock().cue.as_ref().map(|q| q.name.clone()).unwrap_or_default();
    crate::widgets::float_window(ctx, "cue-check", tr("예배 전 큐시트 점검"), [560.0, 560.0]).keep_on_outside().open(&mut open).show(ctx, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(if cue_name.is_empty() { tr("열린 큐시트 없음") } else { cue_name.clone() }).strong());
            if ui.add(Btn::new(tr("다시 점검")).icon("refresh-cw").size(Size::Sm)).clicked() {
                c.ai_found.clear();
                run_rules(app, c);
            }
            if ui.add(Btn::new(tr("주보와 비교")).icon("clipboard-paste").size(Size::Sm).selected(c.show_bulletin)).clicked() {
                c.show_bulletin = !c.show_bulletin;
            }
            ui.checkbox(&mut c.hide_info, tr("참고는 숨기기"));
        });
        if c.show_bulletin {
            ui.label(RichText::new(tr("주보의 예배 순서를 붙여 넣으면 큐시트와 다른 곳(빠진 순서 · 차례)을 찾습니다. 한 줄에 순서 하나씩.")).small().color(crate::theme::pal().muted));
            ui.add(egui::TextEdit::multiline(&mut c.bulletin).desired_rows(5).desired_width(f32::INFINITY).hint_text(tr("예) 예배로 부름\n찬송 8장\n성경봉독 요3:16")));
            ui.horizontal(|ui| {
                if ui.add(Btn::new(tr("주보 파일 열기…")).size(Size::Sm)).clicked() {
                    if let Some(f) = rfd::FileDialog::new().add_filter("주보", &["hwp", "hwpx", "docx", "txt", "md"]).pick_file() {
                        match std::fs::read(&f).map_err(|e| e.to_string()).and_then(|b| wp_core::bulletin::read_bulletin(&b, &f.to_string_lossy())) {
                            Ok(wp_core::bulletin::Input::Text(t)) => c.bulletin = t,
                            Ok(_) => app.core.lock().notify(tr("PDF·사진 주보는 글을 꺼낼 수 없습니다. 순서를 붙여 넣으세요."), "warn"),
                            Err(e) => app.core.lock().notify(e, "error"),
                        }
                    }
                }
                if ui.add(Btn::new(tr("비교하기")).kind(Kind::Primary).size(Size::Sm)).clicked() {
                    run_rules(app, c);
                }
            });
        }
        ui.horizontal_wrapped(|ui| {
            if ai_button(ui, app, "AI 로 오타 · 줄 나눔 보기", c.busy) && ready(app, "cuecheck") {
                run_ai = true;
            }
            if !c.note.is_empty() {
                ui.label(RichText::new(tr_dyn_s(&c.note)).small().color(crate::theme::pal().muted));
            }
        });
        ui.separator();
        let all: Vec<&Finding> = c.found.iter().chain(c.ai_found.iter()).filter(|x| !(c.hide_info && x.level == "info")).collect();
        let (e, w) = (all.iter().filter(|x| x.level == "error").count(), all.iter().filter(|x| x.level == "warn").count());
        if all.is_empty() {
            ui.label(RichText::new(tr("고칠 곳이 없습니다. 예배 준비가 됐습니다.")).color(crate::theme::pal().ok_text));
        } else {
            ui.label(tr_args("꼭 고칠 것 {1} · 살펴볼 것 {2} · 모두 {3}", &[&e, &w, &all.len()]));
        }
        egui::ScrollArea::vertical().id_salt("cc-list").show(ui, |ui| {
            for (k, x) in all.iter().enumerate() {
                let r = ui
                    .push_id(("cc", k), |ui| {
                        ui.horizontal(|ui| {
                            level_mark(ui, x.level);
                            ui.vertical(|ui| {
                                let t = ui.add(egui::Label::new({
                                    let t = RichText::new(tr_dyn_s(&x.msg)).color(level_text(x.level));
                                    if x.level == "error" { t.strong() } else { t }
                                }).sense(egui::Sense::click()));
                                if !x.fix.is_empty() {
                                    ui.label(RichText::new(tr_dyn_s(&x.fix)).small().color(crate::theme::pal().muted));
                                }
                                t
                            })
                            .inner
                        })
                        .inner
                    })
                    .inner;
                if x.index.is_some() {
                    let r = r.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tr("누르면 그 순서로 갑니다"));
                    if r.clicked() {
                        goto = x.index;
                    }
                }
            }
        });
    });
    if run_ai && !c.busy {
        let items = app.core.lock().cue.as_ref().map(|q| q.items.clone()).unwrap_or_default();
        let cfg = app.core.lock().ai_cfg();
        c.busy = true;
        c.note = tr("AI 가 가사 · 공지를 읽는 중…");
        wp_core::hymns::run_bg(
            move || wp_core::cuecheck::ask_ai(&cfg, &items),
            |core, r| {
                let mut s = ST.lock();
                s.check.busy = false;
                match r {
                    Ok((v, u)) => {
                        s.check.note = if v.is_empty() { tr("AI: 오타 · 줄 나눔 문제를 찾지 못했습니다.") } else { tr_args("AI 가 {1}곳을 찾았습니다 {2}", &[&v.len(), &wp_core::aihelp::usage_text(&u)]) };
                        s.check.ai_found = v;
                    }
                    Err(e) => {
                        s.check.note.clear();
                        core.notify(e, "error");
                    }
                }
            },
        );
    }
    if !open {
        st.check.open = false;
    }
    drop(st);
    if let Some(i) = goto {
        let n = app.core.lock().cue.as_ref().map(|q| q.items.len()).unwrap_or(0);
        if i < n {
            app.sel = Some(i);
            app.edit = None;
            crate::cue_ui::request_scroll(ctx);
        }
    }
}

fn take_granted_locked(st: &mut St, feature: &str) -> bool {
    if st.granted.as_deref() == Some(feature) {
        st.granted = None;
        true
    } else {
        false
    }
}

/// 코어가 만든 한국어 글(순서 이름이 든 것)을 그대로 — 영어 화면에서는 사전에 있는 틀만 바뀐다
fn tr_dyn_s(s: &str) -> String {
    wp_core::i18n::tr_dyn(s).to_string()
}

#[allow(dead_code)]
pub fn findings_json(v: &[Finding]) -> Vec<Value> {
    v.iter().map(|x| x.json()).collect()
}

/// 점검용: --open-ai check,settings
pub fn open_from_args() {
    let a: Vec<String> = std::env::args().collect();
    if let Some(v) = a.iter().position(|x| x == "--open-ai").and_then(|i| a.get(i + 1)) {
        for id in v.split(',') {
            handle_menu(&format!("ai:{id}"));
        }
    }
}
