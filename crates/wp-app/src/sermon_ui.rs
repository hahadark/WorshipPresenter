//! 설교 원고 → 슬라이드 (메뉴 AI 'ai:sermon')
//! 원고를 붙여 넣거나 파일(txt · md · docx · hwp · hwpx)로 → 제목 · 본문 성경 · 설교 포인트 · 원고 속 구절 · 인용문 후보를 미리 보고 → 고른 것만 큐시트에 (고른 순서 다음)
//! '설교 포인트' 창(a_tools.rs)과 같은 원고 · 같은 대지 규칙(sermonpoints.rs)을 쓴다.

use crate::app::App;
use crate::widgets::{Btn, Kind, Size};
use eframe::egui::{self, RichText};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde_json::{json, Value};
use wp_core::i18n::{tr, tr_args};
use wp_core::sermonslides::{self as ss, Proposal};

#[derive(Default)]
struct St {
    open: bool,
    text: String,
    title: String,
    verse: String,
    speaker: String,
    list: Vec<Proposal>,
    note: String,
    busy: bool,
    /// 미리 보기로 펼친 후보
    peek: Option<usize>,
}

static ST: Lazy<Mutex<St>> = Lazy::new(|| Mutex::new(St::default()));

pub fn handle_menu(id: &str) -> bool {
    if crate::ai_ui::dry() {
        return id == "ai:sermon"; // 메뉴 시험: 창을 열지 않고 받는지만
    }
    if id == "ai:sermon" {
        ST.lock().open = true;
        return true;
    }
    false
}

/// 설교 포인트 창에서 넘겨받기
pub fn open_with(text: &str, title: &str, verse: &str, speaker: &str) {
    let mut s = ST.lock();
    s.open = true;
    if !text.trim().is_empty() {
        s.text = text.to_string();
    }
    s.title = title.to_string();
    s.verse = verse.to_string();
    s.speaker = speaker.to_string();
    s.list.clear();
}

fn meta(s: &St) -> Value {
    json!({ "title": s.title.trim(), "verse": s.verse.trim(), "speaker": s.speaker.trim() })
}

fn kind_name(k: &str) -> &'static str {
    match k {
        "title" => "제목",
        "bible" => "성경",
        "points" => "설교 포인트",
        _ => "인용",
    }
}

fn kind_icon(k: &str) -> &'static str {
    match k {
        "title" => "text",
        "bible" => "book-open",
        "points" => "presentation",
        _ => "megaphone",
    }
}

/// 후보를 펼쳐 볼 때의 글 (화면에 나갈 장)
fn peek_text(p: &Proposal) -> String {
    let it = &p.item;
    match p.kind {
        "title" => format!("{}\n{}", it["title"].as_str().unwrap_or(""), it["subtitle"].as_str().unwrap_or("")),
        "bible" => format!("{} (번역본은 지금 고른 것으로)", it["ref"].as_str().unwrap_or("")),
        "points" => format!("{}\n{}\n---\n{}", it["title"].as_str().unwrap_or(""), it["subtitle"].as_str().unwrap_or(""), it["more"].as_str().unwrap_or("")),
        _ => it["body"].as_str().unwrap_or("").to_string(),
    }
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let mut st = ST.lock();
    if !st.open {
        return;
    }
    let mut run_ai = crate::ai_ui::take_granted("sermon");
    let mut insert = false;
    let mut to_points = false;
    let mut open = true;
    let s = &mut *st;
    crate::widgets::float_window(ctx, "sermon-slides", tr("설교 원고 → 슬라이드"), [700.0, 660.0]).keep_on_outside().open(&mut open).show(ctx, |ui| {
        ui.label(RichText::new(tr("원고에서 제목 · 본문 성경 · 설교 포인트(대지) · 원고 속 성경 구절 · 인용문을 뽑아 큐시트 순서 묶음으로 만듭니다. 넣기 전에 고를 수 있습니다.")).small().color(crate::theme::pal().muted));
        ui.horizontal_wrapped(|ui| {
            if ui.add(Btn::new(tr("원고 파일 열기…")).icon("folder-open")).clicked() {
                if let Some(f) = rfd::FileDialog::new().add_filter("설교 원고", &["txt", "md", "docx", "hwp", "hwpx", "srt", "vtt"]).pick_file() {
                    match wp_core::sermonpoints::read_text_file(&f) {
                        Ok(t) => {
                            s.text = t;
                            s.list.clear();
                        }
                        Err(e) => app.core.lock().notify(e, "error"),
                    }
                }
            }
            ui.label(RichText::new("txt · md · docx · hwp · hwpx").small().color(crate::theme::pal().muted));
        });
        egui::ScrollArea::vertical().id_salt("ss-text").max_height(150.0).show(ui, |ui| {
            ui.add(egui::TextEdit::multiline(&mut s.text).desired_rows(6).desired_width(f32::INFINITY).hint_text(tr("설교 원고를 붙여 넣으세요")));
        });
        egui::Grid::new("ss-meta").num_columns(2).show(ui, |ui| {
            for (l, v, h) in [("제목", &mut s.title, "비우면 원고 머리의 '제목:' 또는 '# 제목'"), ("본문", &mut s.verse, "예: 요 15:1-8"), ("설교자", &mut s.speaker, "")] {
                ui.label(tr(l));
                ui.add(egui::TextEdit::singleline(v).hint_text(tr(h)).desired_width(420.0));
                ui.end_row();
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui.add(Btn::new(tr("규칙으로 만들기 (AI 없이)")).kind(Kind::Primary)).clicked() {
                match ss::rule_plan(&s.text, &meta(s)) {
                    Ok((l, note)) => {
                        s.list = l;
                        s.note = note;
                    }
                    Err(e) => app.core.lock().notify(e, "warn"),
                }
            }
            if crate::ai_ui::ai_button(ui, app, "AI 로 만들기", s.busy) && crate::ai_ui::ready(app, "sermon") {
                run_ai = true;
            }
            if ui.add(Btn::new(tr("설교 포인트 창에서 대지 고치기")).icon("square-pen")).on_hover_text(tr("같은 원고로 '설교 포인트 슬라이드' 창을 엽니다")).clicked() {
                to_points = true;
            }
        });
        if !s.note.is_empty() {
            ui.label(RichText::new(wp_core::i18n::tr_dyn(&s.note)).small().color(crate::theme::pal().text2));
        }
        ui.separator();
        if s.list.is_empty() {
            ui.label(RichText::new(tr("만들면 여기서 넣을 순서를 고를 수 있습니다.")).color(crate::theme::pal().muted));
            return;
        }
        let n_on = s.list.iter().filter(|p| p.on).count();
        ui.horizontal(|ui| {
            ui.label(tr_args("후보 {1}개 · 넣을 것 {2}개", &[&s.list.len(), &n_on]));
            if ui.add(Btn::new(tr("모두 고르기")).size(Size::Sm)).clicked() {
                s.list.iter_mut().for_each(|p| p.on = true);
            }
            if ui.add(Btn::new(tr("모두 빼기")).size(Size::Sm)).clicked() {
                s.list.iter_mut().for_each(|p| p.on = false);
            }
        });
        egui::ScrollArea::vertical().id_salt("ss-list").max_height(260.0).show(ui, |ui| {
            let mut peek = s.peek;
            for (k, p) in s.list.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    ui.checkbox(&mut p.on, "");
                    crate::icons::show(ui, kind_icon(p.kind), 14.0, crate::theme::pal().muted);
                    ui.label(RichText::new(tr(kind_name(p.kind))).small().color(crate::theme::pal().muted));
                    if ui.add(egui::Label::new(wp_core::i18n::tr_dyn(&p.label)).sense(egui::Sense::click()).truncate()).on_hover_text(tr("눌러서 내용 보기")).clicked() {
                        peek = if peek == Some(k) { None } else { Some(k) };
                    }
                });
                if peek == Some(k) {
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        if p.kind == "bible" {
                            let mut r = p.item["ref"].as_str().unwrap_or("").to_string();
                            ui.horizontal(|ui| {
                                ui.label(tr("장절"));
                                if ui.text_edit_singleline(&mut r).changed() {
                                    p.item["ref"] = json!(r);
                                }
                            });
                        } else if p.kind == "quote" {
                            let mut b = p.item["body"].as_str().unwrap_or("").to_string();
                            if ui.add(egui::TextEdit::multiline(&mut b).desired_rows(3).desired_width(f32::INFINITY)).changed() {
                                p.item["body"] = json!(b);
                            }
                        } else {
                            ui.label(RichText::new(peek_text(p)).small());
                        }
                    });
                }
            }
            s.peek = peek;
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let at = app.sel.map(|i| tr_args("{1}번 순서 다음에", &[&(i + 1)])).unwrap_or_else(|| tr("큐시트 끝에"));
            if ui.add(Btn::new(tr_args("고른 {1}개 큐시트에 넣기", &[&n_on])).kind(Kind::Primary).enabled(n_on > 0)).clicked() {
                insert = true;
            }
            ui.label(RichText::new(at).small().color(crate::theme::pal().muted));
        });
    });
    if run_ai && !s.busy {
        let (text, m, cfg) = (s.text.clone(), meta(s), app.core.lock().ai_cfg());
        s.busy = true;
        wp_core::hymns::run_bg(
            move || ss::ask_ai(&cfg, &text, &m),
            |core, r| {
                let mut s = ST.lock();
                s.busy = false;
                match r {
                    Ok((l, u)) => {
                        s.list = l;
                        s.note = wp_core::aihelp::usage_text(&u);
                    }
                    Err(e) => core.notify(e, "error"),
                }
            },
        );
    }
    if insert {
        let items = ss::chosen_items(&s.list);
        let n = items.len();
        let at = app.sel.map(|i| i + 1);
        let mut c = app.core.lock();
        let first = c.add_items(items, at);
        c.notify(tr_args("설교 순서 {1}개를 큐시트에 넣었습니다 (Ctrl+Z 로 한 번에 되돌림).", &[&n]), "ok");
        drop(c);
        app.sel = Some(first);
        app.edit = None;
        crate::cue_ui::request_scroll(ctx);
    }
    if to_points {
        let (t, a, b, c) = (s.text.clone(), s.title.clone(), s.verse.clone(), s.speaker.clone());
        drop(st);
        crate::a_tools::open_points_with(&t, &a, &b, &c);
        st = ST.lock();
    }
    if !open {
        st.open = false;
    }
}

/// 점검용: --sermon-file <원고> (규칙으로 만들어 둔 채 창을 연다)
pub fn open_from_args() {
    let a: Vec<String> = std::env::args().collect();
    if let Some(f) = a.iter().position(|x| x == "--sermon-file").and_then(|i| a.get(i + 1)) {
        if let Ok(t) = wp_core::sermonpoints::read_text_file(std::path::Path::new(f)) {
            let mut s = ST.lock();
            s.open = true;
            if let Ok((l, note)) = ss::rule_plan(&t, &json!({})) {
                s.list = l;
                s.note = note;
            }
            s.peek = Some(2);
            s.text = t;
        }
    }
}
