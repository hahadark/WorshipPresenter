//! 예배 후 정리 창 (메뉴 도구 'ai:recap')
//! 그날 사용 기록(송출 시각) + 받아 적은 글 → 순서별 시간 표시 · 유튜브 설명 · 설교 요약 재료 · 쇼츠 후보 (복사 · 저장)
//! 녹음 파일을 주면 PC 안에서 글로 바꾸고(whisper.cpp), 그 글만 AI 로 요약 (키 · 동의 필요). 음성은 밖으로 보내지 않는다.

use crate::app::App;
use crate::widgets::{Btn, Kind, Size};
use eframe::egui::{self, RichText};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use wp_core::i18n::{tr, tr_args};
use wp_core::recap;

#[derive(Default)]
struct St {
    open: bool,
    day: String,
    start: String,
    tab: usize,
    out: [String; 4],
    /// 녹음 받아 적기: (진행 0~1, 글)
    rec_busy: Option<f32>,
    rec_text: String,
    rec_note: String,
    summary: String,
    sum_busy: bool,
}

static ST: Lazy<Mutex<St>> = Lazy::new(|| Mutex::new(St::default()));
static CANCEL: AtomicBool = AtomicBool::new(false);

pub fn handle_menu(id: &str) -> bool {
    if id == "ai:recap" {
        let mut s = ST.lock();
        s.open = true;
        if s.day.is_empty() {
            s.day = wp_core::stt::local_day_hms().0;
        }
        return true;
    }
    false
}

const TABS: [&str; 4] = ["순서별 시간 표시", "유튜브 설명", "설교 요약 재료", "쇼츠 후보"];

fn build(app: &App, s: &mut St) {
    let c = app.core.lock();
    let f = wp_core::usage::Filter { from: s.day.clone(), to: s.day.clone(), kinds: vec![], q: String::new() };
    let rows = wp_core::usage::list(&c.store.db, &f);
    let events = recap::events_of(&rows, wp_core::localtime::local_offset_ms());
    let said_file = std::fs::read_to_string(wp_core::stt::transcript_path(&c.data_dir, &s.day)).unwrap_or_default();
    let mut said = recap::parse_transcript(&said_file);
    // 녹음을 받아 적은 글도 (녹음 시각 = 영상 시작 기준)
    if said.is_empty() && !s.rec_text.is_empty() {
        said = recap::parse_transcript(&s.rec_text);
    }
    let start = recap::parse_clock(&s.start).or(events.first().map(|e| e.sec)).unwrap_or(0);
    if s.start.trim().is_empty() {
        if let Some(e) = events.first() {
            s.start = recap::hms(e.sec);
        }
    }
    let cue_name = c.cue.as_ref().map(|q| q.name.clone()).unwrap_or_default();
    let items = c.cue.as_ref().map(|q| q.items.clone()).unwrap_or_default();
    drop(c);
    if events.is_empty() {
        let m = tr_args("{1} 에 송출한 기록이 없습니다 (도구 → 사용 기록이 켜져 있어야 합니다).", &[&s.day]);
        s.out = [m.clone(), m.clone(), m.clone(), m];
        return;
    }
    s.out[0] = recap::chapters(&events, start);
    s.out[1] = recap::youtube_description(if cue_name.is_empty() { &s.day } else { &cue_name }, &events, start);
    s.out[2] = recap::sermon_material(&items, &events, &said);
    let sh = recap::shorts(&events, &said, start);
    s.out[3] = if sh.is_empty() { tr("후보가 없습니다.") } else { format!("{}\n\n{}", tr("영상 안 시각 (시작 ~ 끝) · 까닭 — 편집 프로그램에서 잘라 쓰세요"), recap::shorts_text(&sh)) };
}

/// 녹음 파일 → 16kHz 모노 (rodio 디코더: wav · mp3 · m4a · flac)
fn decode(path: &std::path::Path) -> anyhow::Result<Vec<f32>> {
    use rodio::Source;
    let f = std::fs::File::open(path)?;
    let d = rodio::Decoder::try_from(f).map_err(|e| anyhow::anyhow!("소리 파일을 읽지 못했습니다: {e}"))?;
    let ch = d.channels().get() as usize;
    let rate = d.sample_rate().get() as f32;
    let step = rate / 16000.0;
    let mut mono: Vec<f32> = Vec::new();
    let mut acc = 0.0f32;
    let mut k = 0usize;
    for x in d {
        acc += x;
        k += 1;
        if k == ch {
            mono.push(acc / ch as f32);
            acc = 0.0;
            k = 0;
        }
    }
    let n = (mono.len() as f32 / step) as usize;
    Ok((0..n).map(|i| mono[((i as f32 * step) as usize).min(mono.len() - 1)]).collect())
}

fn transcribe_file(app: &App, path: std::path::PathBuf, ctx: egui::Context) {
    let data = app.core.lock().data_dir.clone();
    let model = app.core.lock().settings.get()["stt"]["model"].as_str().map(String::from).unwrap_or_else(|| wp_core::stt::default_model().to_string());
    let Some(exe) = wp_core::stt::engine_path(&data) else {
        app.core.lock().notify(tr("음성 인식 엔진이 없습니다. 도구 → 음성 인식 창에서 먼저 받으세요."), "warn");
        return;
    };
    CANCEL.store(false, Ordering::SeqCst);
    ST.lock().rec_busy = Some(0.0);
    std::thread::spawn(move || {
        let r = (|| -> anyhow::Result<String> {
            let pcm = decode(&path)?;
            let srv = wp_core::stt::Server::start(&exe, &wp_core::stt::model_path(&data, &model), "ko", wp_core::stt::threads())?;
            const SEG: usize = 16000 * 12; // 엔진 소리 문맥(15초)보다 짧게
            let n = pcm.len().div_ceil(SEG).max(1);
            let mut out = String::new();
            let mut prev = String::new();
            for (k, seg) in pcm.chunks(SEG).enumerate() {
                if CANCEL.load(Ordering::SeqCst) {
                    anyhow::bail!("멈췄습니다.");
                }
                if wp_core::stt::rms(seg) >= 0.004 {
                    let prompt = format!("{} {}", wp_core::stt::bible_prompt(), prev.chars().rev().take(60).collect::<String>().chars().rev().collect::<String>());
                    let t = srv.transcribe(&wp_core::stt::wav_bytes(seg, 16000), &prompt)?;
                    if !t.is_empty() {
                        out.push_str(&format!("{}\t{}\n", recap::hms((k * 12) as i64), t));
                        prev = t;
                    }
                }
                ST.lock().rec_busy = Some((k + 1) as f32 / n as f32);
                ctx.request_repaint();
            }
            Ok(out)
        })();
        let mut s = ST.lock();
        s.rec_busy = None;
        match r {
            Ok(t) => {
                s.rec_note = tr_args("녹음을 글로 바꿨습니다 ({1}줄). 시각은 녹음 처음부터 잰 것입니다.", &[&t.lines().count()]);
                s.rec_text = t;
            }
            Err(e) => s.rec_note = wp_core::i18n::tr_dyn(&e.to_string()),
        }
        ctx.request_repaint();
    });
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    let mut st = ST.lock();
    if !st.open {
        return;
    }
    let mut run_sum = crate::ai_ui::take_granted("recap");
    let mut open = true;
    let mut rebuild = st.out.iter().all(|x| x.is_empty());
    let mut pick_rec = false;
    let s = &mut *st;
    crate::widgets::float_window(ctx, "recap", tr("예배 후 정리"), [700.0, 640.0]).open(&mut open).show(ctx, |ui| {
        let p = crate::theme::pal();
        ui.label(RichText::new(tr("그날 송출한 기록(사용 기록)과 받아 적은 설교로 순서별 시간 표시 · 유튜브 설명 · 설교 요약 재료 · 쇼츠 후보(시간 구간)를 만듭니다. 복사하거나 저장해 쓰세요.")).small().color(p.muted));
        ui.horizontal_wrapped(|ui| {
            ui.label(tr("날짜"));
            rebuild |= ui.add(egui::TextEdit::singleline(&mut s.day).desired_width(96.0)).lost_focus();
            ui.label(tr("영상 시작 시각"));
            rebuild |= ui.add(egui::TextEdit::singleline(&mut s.start).desired_width(80.0).hint_text("10:58:30")).on_hover_text(tr("방송·녹화를 시작한 시각. 비우면 첫 송출 시각")).lost_focus();
            if ui.add(Btn::new(tr("다시 만들기")).icon("refresh-cw").size(Size::Sm)).clicked() {
                rebuild = true;
            }
        });
        ui.horizontal(|ui| {
            for (i, t) in TABS.iter().enumerate() {
                if ui.add(Btn::new(tr(t)).selected(s.tab == i).size(Size::Sm)).clicked() {
                    s.tab = i;
                }
            }
        });
        let tab = s.tab;
        egui::ScrollArea::vertical().id_salt("rc-out").max_height(300.0).show(ui, |ui| {
            ui.add(egui::TextEdit::multiline(&mut s.out[tab]).desired_rows(14).desired_width(f32::INFINITY));
        });
        ui.horizontal(|ui| {
            if ui.add(Btn::new(tr("복사")).icon("copy").size(Size::Sm)).clicked() {
                crate::widgets::copy_notice(ui.ctx(), s.out[tab].clone(), tr("복사했습니다."));
            }
            if ui.add(Btn::new(tr("저장…")).icon("download").size(Size::Sm)).clicked() {
                let name = format!("{}-{}.txt", s.day, tr(TABS[tab]).replace(' ', "-"));
                if let Some(f) = rfd::FileDialog::new().set_file_name(&name).add_filter("txt", &["txt"]).save_file() {
                    match std::fs::write(&f, s.out[tab].as_bytes()) {
                        Ok(()) => app.core.lock().notify(tr_args("저장했습니다: {1}", &[&f.display()]), "ok"),
                        Err(e) => app.core.lock().notify(e.to_string(), "error"),
                    }
                }
            }
        });
        ui.separator();
        ui.label(RichText::new(tr("녹음 파일로 설교 요약")).strong());
        ui.label(RichText::new(tr("녹음(wav · mp3 · m4a · flac)을 이 PC 안에서 글로 바꾼 뒤, 그 글만 AI 로 요약합니다. 음성은 PC 밖으로 나가지 않습니다.")).small().color(p.muted));
        ui.horizontal_wrapped(|ui| {
            if let Some(f) = s.rec_busy {
                ui.add(egui::ProgressBar::new(f).desired_width(240.0).show_percentage());
                if ui.add(Btn::new(tr("멈추기")).size(Size::Sm)).clicked() {
                    CANCEL.store(true, Ordering::SeqCst);
                }
            } else if ui.add(Btn::new(tr("녹음 파일 글로 바꾸기…")).icon("mic")).clicked() {
                pick_rec = true;
            }
            if crate::ai_ui::ai_button(ui, app, "AI 로 요약", s.sum_busy) && !s.rec_text.trim().is_empty() && crate::ai_ui::ready(app, "recap") {
                run_sum = true;
            }
        });
        if !s.rec_note.is_empty() {
            ui.label(RichText::new(s.rec_note.clone()).small().color(p.text2));
        }
        if !s.rec_text.is_empty() {
            egui::CollapsingHeader::new(tr("받아 적은 녹음 글")).id_salt("rc-rec").show(ui, |ui| {
                egui::ScrollArea::vertical().id_salt("rc-rect").max_height(120.0).show(ui, |ui| {
                    ui.add(egui::TextEdit::multiline(&mut s.rec_text).desired_width(f32::INFINITY));
                });
            });
        }
        if !s.summary.is_empty() {
            ui.label(RichText::new(tr("요약")).strong());
            ui.add(egui::TextEdit::multiline(&mut s.summary).desired_rows(6).desired_width(f32::INFINITY));
            if ui.add(Btn::new(tr("요약 복사")).icon("copy").size(Size::Sm)).clicked() {
                crate::widgets::copy_notice(ui.ctx(), s.summary.clone(), tr("복사했습니다."));
            }
        }
    });
    if rebuild {
        build(app, s);
    }
    if run_sum && !s.sum_busy && !s.rec_text.trim().is_empty() {
        let (text, cfg) = (s.rec_text.clone(), app.core.lock().ai_cfg());
        let title = s.out[2].lines().next().unwrap_or("").trim_start_matches("설교:").trim().to_string();
        s.sum_busy = true;
        wp_core::hymns::run_bg(
            move || recap::ask_summary(&cfg, &text, &title),
            |core, r| {
                let mut s = ST.lock();
                s.sum_busy = false;
                match r {
                    Ok((t, _)) => s.summary = t,
                    Err(e) => core.notify(e, "error"),
                }
            },
        );
    }
    drop(st);
    if pick_rec {
        if let Some(f) = rfd::FileDialog::new().add_filter(tr("녹음"), &["wav", "mp3", "m4a", "flac", "ogg"]).pick_file() {
            transcribe_file(app, f, ctx.clone());
        }
    }
    if !open {
        ST.lock().open = false;
    }
}

/// 점검용: --recap-day YYYY-MM-DD [--recap-start HH:MM:SS]
pub fn open_from_args() {
    let a: Vec<String> = std::env::args().collect();
    if let Some(d) = a.iter().position(|x| x == "--recap-day").and_then(|i| a.get(i + 1)) {
        let mut s = ST.lock();
        s.open = true;
        s.day = d.clone();
        if let Some(t) = a.iter().position(|x| x == "--recap-start").and_then(|i| a.get(i + 1)) {
            s.start = t.clone();
        }
        s.tab = 1;
    }
}
