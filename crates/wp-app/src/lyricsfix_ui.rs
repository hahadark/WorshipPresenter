//! 가사 자동 정리 창 (메뉴 AI 'ai:lyrics' · 찬양 편집칸 '가사 자동 정리…')
//! 붙여 넣은 가사 → 절 · 후렴 · 브리지 · 줄 바꿈 · 띄어쓰기 (규칙, 무료 / AI 선택) → 고치기 전후를 나란히 → 받아들이기

use crate::app::{Act, App};
use crate::widgets::{Btn, Kind, Size};
use eframe::egui::{self, RichText};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde_json::json;
use wp_core::i18n::{tr, tr_args};
use wp_core::lyricsclean::{self as lc, Cleaned};

#[derive(Default)]
struct St {
    open: bool,
    src: String,
    result: Option<Cleaned>,
    /// 받아들일 찬양 순서 (번호, id)
    target: Option<(usize, String)>,
    busy: bool,
    usage: String,
}

static ST: Lazy<Mutex<St>> = Lazy::new(|| Mutex::new(St::default()));

pub fn handle_menu(id: &str) -> bool {
    if crate::ai_ui::dry() {
        return id == "ai:lyrics"; // 메뉴 시험: 창을 열지 않고 받는지만
    }
    if id == "ai:lyrics" {
        let mut s = ST.lock();
        s.open = true;
        return true;
    }
    false
}

/// 글로 연다 (규칙으로 정리해 둔 채)
pub fn open_text(text: &str) {
    let mut s = ST.lock();
    s.open = true;
    s.src = text.to_string();
    s.result = Some(lc::clean(text));
    s.target = None;
}

/// 찬양 편집칸에서: 그 순서의 가사로 연다
pub fn open_for(index: usize, item: &serde_json::Value) {
    let mut s = ST.lock();
    s.open = true;
    s.src = item["lyrics"].as_str().unwrap_or("").to_string();
    s.target = Some((index, item["id"].as_str().unwrap_or("").to_string()));
    s.result = Some(lc::clean(&s.src));
    s.usage.clear();
}

pub fn show(app: &mut App, ctx: &egui::Context, acts: &mut Vec<Act>) {
    let mut st = ST.lock();
    if !st.open {
        return;
    }
    let mut run_ai = crate::ai_ui::take_granted("lyrics");
    let mut accept = false;
    let mut as_new = false;
    let mut open = true;
    let s = &mut *st;
    // 받을 순서가 아직 그대로인지
    let target_ok = s.target.as_ref().is_some_and(|(i, id)| app.core.lock().cue.as_ref().and_then(|c| c.items.get(*i)).is_some_and(|it| it["id"] == id.as_str() && it["type"] == "song"));
    crate::widgets::float_window(ctx, "lyrics-fix", tr("가사 자동 정리"), [860.0, 640.0]).keep_on_outside().open(&mut open).show(ctx, |ui| {
        let p = crate::theme::pal();
        ui.label(RichText::new(tr("붙여 넣은 가사를 절 · 후렴 · 브리지로 나누고, 코드 · 작사/작곡 · 저작권 줄을 빼고, 긴 줄과 띄어쓰기를 다듬습니다. 받아들이기 전에는 아무것도 바뀌지 않습니다.")).small().color(p.muted));
        ui.horizontal_wrapped(|ui| {
            if ui.add(Btn::new(tr("규칙으로 정리 (AI 없이)")).kind(Kind::Primary)).clicked() {
                s.result = Some(lc::clean(&s.src));
                s.usage.clear();
            }
            if crate::ai_ui::ai_button(ui, app, "AI 로 정리", s.busy) && crate::ai_ui::ready(app, "lyrics") {
                run_ai = true;
            }
            if !s.usage.is_empty() {
                ui.label(RichText::new(s.usage.clone()).small().color(p.muted));
            }
        });
        ui.columns(2, |cols| {
            cols[0].label(RichText::new(tr("고치기 전 (붙여 넣기)")).strong());
            egui::ScrollArea::vertical().id_salt("lf-src").max_height(420.0).show(&mut cols[0], |ui| {
                ui.add(egui::TextEdit::multiline(&mut s.src).desired_rows(20).desired_width(f32::INFINITY).hint_text(tr("가사를 붙여 넣으세요")));
            });
            cols[1].label(RichText::new(tr("고친 뒤 (고칠 수 있음)")).strong());
            match s.result.as_mut() {
                None => {
                    cols[1].label(RichText::new(tr("정리하면 여기에 나옵니다.")).color(p.muted));
                }
                Some(r) => {
                    egui::ScrollArea::vertical().id_salt("lf-out").max_height(380.0).show(&mut cols[1], |ui| {
                        ui.add(egui::TextEdit::multiline(&mut r.lyrics).desired_rows(18).desired_width(f32::INFINITY));
                    });
                    cols[1].horizontal(|ui| {
                        ui.label(tr("부를 순서"));
                        ui.add(egui::TextEdit::singleline(&mut r.order).desired_width(f32::INFINITY).hint_text(tr("비우면 가사 순서대로")));
                    });
                }
            }
        });
        if let Some(r) = &s.result {
            if !r.notes.is_empty() {
                ui.label(RichText::new(tr_args("고친 것: {1}", &[&r.notes.iter().map(|n| wp_core::i18n::tr_dyn(n)).collect::<Vec<_>>().join(" · ")])).small().color(p.text2));
            }
            let diff = lc::side_by_side(&s.src, &r.lyrics);
            let changed = diff.iter().filter(|d| !d.2).count();
            ui.label(RichText::new(tr_args("바뀐 줄 {1}개 (위 두 칸을 나란히 비교하세요)", &[&changed])).small().color(p.muted));
            ui.horizontal_wrapped(|ui| {
                if target_ok {
                    let n = s.target.as_ref().map(|t| t.0 + 1).unwrap_or(0);
                    if ui.add(Btn::new(tr_args("받아들이기 ({1}번 찬양 순서에)", &[&n])).kind(Kind::Primary)).clicked() {
                        accept = true;
                    }
                }
                if ui.add(Btn::new(tr("새 찬양 순서로 넣기")).icon("plus")).clicked() {
                    as_new = true;
                }
                if ui.add(Btn::new(tr("복사")).icon("copy").size(Size::Sm)).clicked() {
                    crate::widgets::copy_notice(ui.ctx(), r.lyrics.clone(), tr("정리한 가사를 복사했습니다."));
                }
            });
        }
    });
    if run_ai && !s.busy {
        let (text, cfg) = (s.src.clone(), app.core.lock().ai_cfg());
        s.busy = true;
        wp_core::hymns::run_bg(
            move || lc::ask_ai(&cfg, &text),
            |core, r| {
                let mut s = ST.lock();
                s.busy = false;
                match r {
                    Ok((c, u)) => {
                        s.result = Some(c);
                        s.usage = wp_core::aihelp::usage_text(&u);
                    }
                    Err(e) => core.notify(e, "error"),
                }
            },
        );
    }
    if let Some(r) = s.result.clone() {
        if accept {
            if let Some((i, _)) = s.target.clone() {
                if let Some(mut it) = app.core.lock().cue.as_ref().and_then(|c| c.items.get(i).cloned()) {
                    it["lyrics"] = json!(r.lyrics);
                    it["order"] = json!(r.order);
                    acts.push(Act::UpdateItem(i, it));
                    app.edit = None;
                    app.core.lock().notify(tr("정리한 가사로 바꿨습니다 (Ctrl+Z 로 되돌림)."), "ok");
                    s.open = false;
                }
            }
        }
        if as_new {
            let title = r.lyrics.lines().find(|l| !l.starts_with('[') && !l.trim().is_empty()).unwrap_or("찬양").chars().take(20).collect::<String>();
            // 넣기만 하고 미리보기·송출은 하지 않는다
            let mut it = wp_core::core::new_item("song", &title);
            it["lyrics"] = json!(r.lyrics);
            it["order"] = json!(r.order);
            let at = app.sel.map(|i| i + 1);
            let first = app.core.lock().add_items(vec![it], at);
            app.sel = Some(first);
            app.edit = None;
            crate::cue_ui::request_scroll(ctx);
        }
    }
    if !open {
        st.open = false;
    }
}
