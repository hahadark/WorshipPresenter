//! 말로 시키기 창 (메뉴 AI 'ai:say' · 찾기칸(Ctrl+K)에 '>' 로 시작하는 말 + Enter)
//! 말 → 계획(규칙 또는 AI) → 할 일 목록을 보여 주고 → [실행] 을 눌러야 한다. 송출 · 삭제 · 화면 지우기는 단계에 없다.

use crate::app::{Act, App};
use crate::widgets::{Btn, Kind, Size};
use eframe::egui::{self, RichText};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use wp_core::i18n::{tr, tr_args};
use wp_core::saycmd::{self, Step};

#[derive(Default)]
struct St {
    open: bool,
    text: String,
    steps: Vec<(bool, Step)>,
    dropped: Vec<String>,
    say: String,
    log: Vec<String>,
    busy: bool,
    /// 열자마자 규칙으로 계획 세우기
    auto: bool,
}

static ST: crate::uistate::Global<St> = crate::uistate::Global::new(|| St::default());

pub fn handle_menu(id: &str) -> bool {
    if crate::ai_ui::dry() {
        return id == "ai:say"; // 메뉴 시험: 창을 열지 않고 받는지만
    }
    if id == "ai:say" {
        ST.lock().open = true;
        return true;
    }
    false
}

/// 찾기칸에서 넘겨받기
pub fn open_with(text: &str) {
    let mut s = ST.lock();
    s.open = true;
    s.text = text.to_string();
    s.steps.clear();
    s.dropped.clear();
    s.log.clear();
    s.say.clear();
    s.auto = !text.trim().is_empty();
}

fn set_plan(s: &mut St, plan: &serde_json::Value, ctx: &saycmd::Ctx) {
    let (ok, bad) = saycmd::validate(plan, ctx);
    s.steps = ok.into_iter().map(|x| (true, x)).collect();
    s.dropped = bad;
    s.say = plan["say"].as_str().unwrap_or("").to_string();
    s.log.clear();
}

fn rule(app: &App, s: &mut St) {
    let ctx = app.core.lock().say_ctx();
    match saycmd::rule_plan(&s.text, &ctx) {
        Some(p) => set_plan(s, &p, &ctx),
        None => {
            s.steps.clear();
            s.dropped.clear();
            s.say = tr("규칙으로는 알아듣지 못했습니다. 'AI 로 알아듣기' 를 쓰거나 더 짧게 말해 보세요 (예: 요3:16 보여줘 · 지난주 큐시트 복사 · 예배 전 점검).");
        }
    }
}

pub fn show(app: &mut App, ctx: &egui::Context, acts: &mut Vec<Act>) {
    let mut st = ST.lock();
    if !st.open {
        return;
    }
    let mut run_ai = crate::ai_ui::take_granted("say");
    let mut run = false;
    let mut open = true;
    let s = &mut *st;
    if std::mem::take(&mut s.auto) {
        rule(app, s);
    }
    crate::widgets_win::window(ctx, "say", tr("말로 시키기"), [600.0, 460.0]).open(&mut open).show(ctx, |ui| {
        let p = crate::theme::pal();
        ui.label(RichText::new(tr("하고 싶은 일을 말로 쓰면 프로그램의 명령으로 바꿔 보여 줍니다. [실행] 을 눌러야 합니다. 송출 · 삭제 · 화면 지우기는 말로 시킬 수 없습니다. 찾기칸에서 '>' 로 시작해도 됩니다.")).small().color(p.muted));
        let r = ui.add(egui::TextEdit::singleline(&mut s.text).desired_width(f32::INFINITY).hint_text(tr("예) 지난주 큐시트 복사해서 설교 제목을 '은혜' 로, 본문은 롬 8:28 로 바꿔")));
        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        ui.horizontal_wrapped(|ui| {
            if ui.add(Btn::new(tr("규칙으로 알아듣기")).size(Size::Sm).kind(Kind::Primary)).clicked() || enter {
                rule(app, s);
            }
            if crate::ai_ui::ai_button(ui, app, "AI 로 알아듣기", s.busy) && crate::ai_ui::ready(app, "say") {
                run_ai = true;
            }
        });
        if !s.say.is_empty() {
            ui.label(RichText::new(wp_core::i18n::tr_dyn(&s.say)).color(p.text2));
        }
        if !s.steps.is_empty() {
            crate::widgets::group_head(ui, &tr("할 일 (차례대로)"));
            for (k, (on, st)) in s.steps.iter_mut().enumerate() {
                ui.checkbox(on, format!("{}. {}", k + 1, wp_core::i18n::tr_dyn(&st.label)));
            }
        }
        if !s.dropped.is_empty() {
            crate::widgets::group_head(ui, &tr("뺀 것"));
            for d in &s.dropped {
                ui.label(RichText::new(format!("· {}", wp_core::i18n::tr_dyn(d))).small().color(p.muted));
            }
            ui.label(RichText::new(tr(saycmd::FORBIDDEN_HINT)).small().color(p.muted));
        }
        if !s.steps.is_empty() {
            let n = s.steps.iter().filter(|x| x.0).count();
            ui.horizontal(|ui| {
                if ui.add(Btn::new(tr_args("실행 ({1}개)", &[&n])).kind(Kind::Primary).enabled(n > 0)).clicked() {
                    run = true;
                }
                ui.label(RichText::new(tr("순서 고치기는 Ctrl+Z 한 번으로 되돌립니다.")).small().color(p.muted));
            });
        }
        if !s.log.is_empty() {
            ui.separator();
            for l in &s.log {
                ui.label(RichText::new(wp_core::i18n::tr_dyn(l)).small());
            }
        }
    });
    if run_ai && !s.busy {
        let (text, cfg, cx) = (s.text.clone(), app.core.lock().ai_cfg(), app.core.lock().say_ctx());
        s.busy = true;
        wp_core::hymns::run_bg(
            move || saycmd::ask_ai(&cfg, &text, &cx).map(|(v, _)| (v, cx)),
            |core, r| {
                let mut s = ST.lock();
                s.busy = false;
                match r {
                    Ok((v, cx)) => set_plan(&mut s, &v, &cx),
                    Err(e) => core.notify(e, "error"),
                }
            },
        );
    }
    if run {
        let steps: Vec<Step> = s.steps.iter().filter(|x| x.0).map(|x| x.1.clone()).collect();
        let (log, ui_acts) = app.core.lock().run_say_steps(&steps);
        s.log = log;
        s.steps.clear();
        app.sel = None;
        app.edit = None;
        for (k, v) in ui_acts {
            match k.as_str() {
                "tool" => {
                    let id = match v.as_str() {
                        "points" => "a:points".to_string(),
                        "bulletin" => "d:bulletin".to_string(),
                        "history" => "c:history".to_string(),
                        t => format!("ai:{t}"),
                    };
                    drop_and_open(&id);
                }
                "search" => {
                    app.search = v.clone();
                    acts.push(Act::SearchList(v));
                    app.focus_search = true;
                }
                _ => {}
            }
        }
    }
    if !open {
        st.open = false;
    }
}

fn drop_and_open(id: &str) {
    // 다른 창 열기 (메뉴와 같은 길) — 이 창의 잠금을 잡은 채 부르지 않도록 다음 프레임에
    let id = id.to_string();
    PENDING.lock().push(id);
}

static PENDING: crate::uistate::Global<Vec<String>> = crate::uistate::Global::new(Vec::new);

/// 메뉴 처리기로 넘길 창 열기 (track_c 의 메뉴 처리와 같은 id)
pub fn take_pending() -> Vec<String> {
    std::mem::take(&mut *PENDING.lock())
}
