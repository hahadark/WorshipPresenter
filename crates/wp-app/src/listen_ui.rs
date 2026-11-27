//! 음성 인식 창 (메뉴 도구 'ai:listen'): 설교 중 성경 구절 따라가기 · 방송 실시간 자막(·영어 번역)
//!  - 기본 꺼짐. 켜면 조작 화면 오른쪽 아래에 '듣는 중' 표시
//!  - 마이크 → 16kHz 모노 → 쉼에서 자름(stt::Chunker) → PC 안 whisper-server → 글 → 장절 찾기(voicefollow::Follower)
//!  - 찾은 구절은 목록에 쌓이고, '찾으면 바로 다음 칸에' 를 켰을 때만 '다음(미리보기)' 에 올린다. 송출은 절대 하지 않는다
//!  - 방송 자막: 받아 적은 글을 오버레이 배너로 (기본 꺼짐 · 기본은 방송 출력 /outputs/overlay 에만, 예배당 화면은 따로 켜야).
//!    영어 번역은 AI 키가 있고 동의했을 때만 — 받아 적은 '글' 만 보낸다 (음성은 보내지 않음)
//!  - 엔진·모델은 처음 켤 때 받는 곳·크기·라이선스를 보여 주고 동의 뒤 받는다 (SHA-256 대조)

use crate::app::App;
use crate::widgets::{Btn, Kind, Size};
use eframe::egui::{self, RichText};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use wp_core::i18n::{tr, tr_args};
use wp_core::stt;

#[derive(Default)]
pub struct St {
    pub open: bool,
    /// 지금 듣는 중
    pub listening: bool,
    starting: bool,
    status: String,
    err: String,
    /// 받는 중: (이름, 받은, 전체)
    dl: Option<(String, u64, u64)>,
    ask_download: bool,
    pub lines: VecDeque<(String, String)>,
    /// 찾은 구절 (시각, 장절, 다음 칸에 올렸나)
    found: Vec<(String, String, bool)>,
    /// 조각마다 (소리 길이 초, 인식에 걸린 초)
    stats: VecDeque<(f32, f32)>,
    level: f32,
    model: String,
    last_en: String,
}

pub static ST: Lazy<Mutex<St>> = Lazy::new(|| Mutex::new(St::default()));
static STOP: Lazy<Arc<AtomicBool>> = Lazy::new(|| Arc::new(AtomicBool::new(false)));
static CANCEL_DL: AtomicBool = AtomicBool::new(false);

pub fn handle_menu(id: &str) -> bool {
    if id == "ai:listen" {
        ST.lock().open = true;
        return true;
    }
    false
}

pub fn listening() -> bool {
    ST.lock().listening
}

fn settings_model(app: &App) -> String {
    app.core.lock().settings.get()["stt"]["model"].as_str().map(String::from).unwrap_or_else(|| stt::default_model().to_string())
}

fn auto_next(app: &App) -> bool {
    app.core.lock().settings.get()["stt"]["autoNext"].as_bool().unwrap_or(false)
}

/// 이 PC 에 엔진·모델이 다 있나
fn ready(data: &std::path::Path, model: &str) -> (bool, bool) {
    (stt::engine_path(data).is_some(), stt::model_path(data, model).is_file())
}

fn start_download(data: std::path::PathBuf, model: String, ctx: egui::Context) {
    CANCEL_DL.store(false, Ordering::SeqCst);
    std::thread::spawn(move || {
        let prog = |name: &str| {
            let n = name.to_string();
            let c = ctx.clone();
            move |got: u64, all: u64| {
                ST.lock().dl = Some((n.clone(), got, all));
                if got % (1 << 22) < (1 << 16) {
                    c.request_repaint();
                }
            }
        };
        let cancel = || CANCEL_DL.load(Ordering::SeqCst);
        let mut r: anyhow::Result<()> = Ok(());
        if stt::engine_path(&data).is_none() {
            r = stt::install_engine(&data, &prog(stt::ENGINE_WIN.name), &cancel).map(|_| ());
        }
        if r.is_ok() {
            let m = stt::model(&model);
            r = stt::download(m, &stt::model_path(&data, &model), &prog(m.name), &cancel);
        }
        let mut s = ST.lock();
        s.dl = None;
        match r {
            Ok(()) => s.status = tr("받기를 마쳤습니다. '듣기 시작' 을 누르세요."),
            Err(e) => s.err = wp_core::i18n::tr_dyn(&e.to_string()),
        }
        ctx.request_repaint();
    });
}

/// 듣기 시작: 엔진을 띄우고 마이크를 연다 (모두 작업 스레드에서)
fn start(app: &App, ctx: &egui::Context) {
    let data = app.core.lock().data_dir.clone();
    let model = settings_model(app);
    let Some(exe) = stt::engine_path(&data) else { return };
    STOP.store(false, Ordering::SeqCst);
    {
        let mut s = ST.lock();
        s.starting = true;
        s.err.clear();
        s.status = tr("음성 인식 엔진을 켜는 중…");
        s.model = model.clone();
    }
    let ctx = ctx.clone();
    let stop = STOP.clone();
    std::thread::Builder::new()
        .name("wp-listen".into())
        .spawn(move || {
            if let Err(e) = worker(&data, &exe, &model, &stop, &ctx) {
                let mut s = ST.lock();
                s.err = wp_core::i18n::tr_dyn(&e.to_string());
            }
            let mut s = ST.lock();
            s.listening = false;
            s.starting = false;
            s.status = tr("꺼짐");
            ctx.request_repaint();
        })
        .ok();
}

pub fn stop() {
    STOP.store(true, Ordering::SeqCst);
}

/// 마이크 소리 (여러 채널 → 모노, 장치 표본율 → 16kHz) 를 보낼 통로
fn open_mic(tx: std::sync::mpsc::Sender<Vec<f32>>) -> anyhow::Result<(rodio::cpal::Stream, String)> {
    use rodio::cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    let host = rodio::cpal::default_host();
    let dev = host.default_input_device().ok_or_else(|| anyhow::anyhow!("마이크(입력 장치)를 찾지 못했습니다. Windows 소리 설정에서 입력 장치를 확인하세요."))?;
    let name = dev.description().map(|d| d.name().to_string()).unwrap_or_default();
    let cfg = dev.default_input_config()?;
    let ch = cfg.channels() as usize;
    let rate = cfg.sample_rate() as f32;
    let fmt = cfg.sample_format();
    let step = rate / 16000.0;
    let mut pos = 0.0f32;
    let mut push = move |mono: Vec<f32>| {
        // 단순 솎기(선형 보간 없이 평균): 말소리 인식에는 충분
        let mut out = Vec::with_capacity((mono.len() as f32 / step) as usize + 1);
        while (pos as usize) < mono.len() {
            let i = pos as usize;
            let j = ((pos + step) as usize).min(mono.len()).max(i + 1);
            out.push(mono[i..j].iter().sum::<f32>() / (j - i) as f32);
            pos += step;
        }
        pos -= mono.len() as f32;
        let _ = tx.send(out);
    };
    let err = |e| log::warn!("마이크 오류: {e}");
    let stream = match fmt {
        rodio::cpal::SampleFormat::F32 => dev.build_input_stream::<f32, _, _>(cfg.config(), move |d: &[f32], _| push(d.chunks(ch).map(|f| f.iter().sum::<f32>() / ch as f32).collect()), err, None)?,
        rodio::cpal::SampleFormat::I16 => dev.build_input_stream::<i16, _, _>(cfg.config(), move |d: &[i16], _| push(d.chunks(ch).map(|f| f.iter().map(|x| *x as f32 / 32768.0).sum::<f32>() / ch as f32).collect()), err, None)?,
        other => anyhow::bail!("지원하지 않는 마이크 형식입니다 ({other:?})."),
    };
    stream.play()?;
    Ok((stream, name))
}

fn worker(data: &std::path::Path, exe: &std::path::Path, model: &str, stop: &AtomicBool, ctx: &egui::Context) -> anyhow::Result<()> {
    let server = stt::Server::start(exe, &stt::model_path(data, model), "ko", stt::threads())?;
    let (tx, rx) = std::sync::mpsc::channel::<Vec<f32>>();
    // 점검용: WP_LISTEN_WAV=<16kHz 모노 wav> 면 마이크 대신 그 파일을 실제 속도로 흘려 넣는다
    let (_stream, mic) = match std::env::var("WP_LISTEN_WAV") {
        Ok(p) => {
            let b = std::fs::read(&p)?;
            let i = b.windows(4).position(|w| w == b"data").map(|i| i + 8).unwrap_or(44);
            let pcm: Vec<f32> = b[i..].chunks(2).filter(|c| c.len() == 2).map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0).collect();
            std::thread::spawn(move || {
                for c in pcm.chunks(1600) {
                    if tx.send(c.to_vec()).is_err() {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
            });
            (None, format!("파일 {p}"))
        }
        Err(_) => {
            let (st, name) = open_mic(tx)?;
            (Some(st), name)
        }
    };
    {
        let mut s = ST.lock();
        s.listening = true;
        s.starting = false;
        s.status = tr_args("듣는 중 · 마이크: {1} · 모델 {2}", &[&mic, &model]);
    }
    ctx.request_repaint();
    let mut chunker = stt::Chunker::default();
    let mut follower = wp_core::voicefollow::Follower::default();
    let mut prev = String::new();
    while !stop.load(Ordering::SeqCst) {
        let Ok(buf) = rx.recv_timeout(std::time::Duration::from_millis(300)) else {
            crate::listen_ui::on_idle();
            continue;
        };
        ST.lock().level = stt::rms(&buf);
        let Some(chunk) = chunker.push(&buf) else { continue };
        let secs = chunk.len() as f32 / 16000.0;
        let t0 = std::time::Instant::now();
        // 앞 조각 끝을 프롬프트로 (문맥) + 성경 이름
        let prompt = format!("{} {}", stt::bible_prompt(), prev.chars().rev().take(60).collect::<String>().chars().rev().collect::<String>());
        let text = match server.transcribe(&stt::wav_bytes(&chunk, 16000), &prompt) {
            Ok(t) => t,
            Err(e) => {
                log::warn!("음성 인식: {e}");
                continue;
            }
        };
        let took = t0.elapsed().as_secs_f32();
        if text.is_empty() {
            continue;
        }
        prev = text.clone();
        let (day, hms) = stt::local_day_hms();
        stt::append_transcript(data, &day, &hms, &text);
        let found = follower.feed(&text, wp_core::util::now_ms());
        {
            let mut s = ST.lock();
            s.stats.push_back((secs, took));
            while s.stats.len() > 30 {
                s.stats.pop_front();
            }
            s.lines.push_back((hms.clone(), text.clone()));
            while s.lines.len() > 200 {
                s.lines.pop_front();
            }
        }
        on_text(&text);
        if !found.is_empty() {
            let Some(core) = wp_core::hymns::shared() else { continue };
            let auto = core.lock().settings.get()["stt"]["autoNext"].as_bool().unwrap_or(false);
            for f in found {
                let mut put = false;
                if auto {
                    let mut c = core.lock();
                    match c.preview_passage_quiet(&f.reference) {
                        Ok(t) => {
                            c.notify(tr_args("들은 구절을 다음 칸에 올렸습니다: {1} (송출은 직접)", &[&t]), "ok");
                            put = true;
                        }
                        Err(e) => log::info!("구절 따라가기: {} — {e}", f.reference),
                    }
                }
                ST.lock().found.push((hms.clone(), f.reference.clone(), put));
            }
        }
        ctx.request_repaint();
    }
    drop(server);
    Ok(())
}

/// 받아 적은 글 한 줄이 생길 때: 방송 자막 (켜져 있으면)
fn on_text(text: &str) {
    let Some(core) = wp_core::hymns::shared() else { return };
    let (on, outs, translate, size, only_en, cfg, consent) = {
        let c = core.lock();
        let s = c.settings.get();
        let (on, outs, tr_on, size) = wp_core::voicefollow::caption_settings(&s);
        (on, outs, tr_on, size, s["stt"]["captions"]["onlyEn"].as_bool().unwrap_or(false), c.ai_cfg(), c.ai_consented("translate"))
    };
    if !on {
        return;
    }
    let can_translate = translate && consent && (cfg.has_key() || std::env::var("WP_AI_MOCK").is_ok());
    if !can_translate || !only_en {
        wp_core::voicefollow::set_caption(text, &outs, size);
        core.lock().overlays_changed();
    }
    if can_translate {
        let ko = text.to_string();
        std::thread::spawn(move || {
            match wp_core::aihelp::chat(&cfg, &wp_core::voicefollow::translate_system(), &ko, false, 200) {
                Ok((en, _)) => {
                    let en = en.lines().next().unwrap_or("").trim().to_string();
                    wp_core::voicefollow::set_caption(&wp_core::voicefollow::caption_text(&ko, Some(&en), only_en), &outs, size);
                    ST.lock().last_en = en;
                }
                Err(e) => {
                    log::warn!("자막 번역: {e}");
                    if only_en {
                        wp_core::voicefollow::set_caption(&ko, &outs, size);
                    }
                }
            }
            if let Some(core) = wp_core::hymns::shared() {
                core.lock().overlays_changed();
            }
        });
    }
}

/// 조용할 때: 자막이 오래되면 내린다
fn on_idle() {
    if let Some(c) = wp_core::voicefollow::caption() {
        if wp_core::util::now_ms() - c.at > wp_core::voicefollow::CAPTION_HOLD_MS {
            wp_core::voicefollow::set_caption("", &[], 0.0);
            if let Some(core) = wp_core::hymns::shared() {
                core.lock().overlays_changed();
            }
        }
    }
}

/// 자막 칸 설정 바꾸기
fn set_cap(app: &App, k: &str, v: serde_json::Value) {
    let k = k.to_string();
    app.core.lock().settings.update(|d| {
        if !d["stt"].is_object() {
            d["stt"] = serde_json::json!({});
        }
        if !d["stt"]["captions"].is_object() {
            d["stt"]["captions"] = serde_json::json!({});
        }
        d["stt"]["captions"][k] = v;
    });
}

/// 방송 자막 칸 (창 안)
fn captions_ui(app: &App, ui: &mut egui::Ui) {
    let p = crate::theme::pal();
    let s = app.core.lock().settings.get().clone();
    let (mut on, outs, mut translate, mut size) = wp_core::voicefollow::caption_settings(&s);
    let mut only_en = s["stt"]["captions"]["onlyEn"].as_bool().unwrap_or(false);
    let mut to_main = outs.iter().any(|o| o == "main");
    let mut to_obs = outs.iter().any(|o| o == "overlay");
    ui.label(RichText::new(tr("방송 실시간 자막")).strong());
    ui.label(RichText::new(tr("받아 적은 글을 방송 출력(OBS 자막 /outputs/overlay) 아래쪽에 띄웁니다. 예배당 화면에는 기본으로 나가지 않습니다. 말이 8초 멈추면 내려갑니다.")).small().color(p.muted));
    ui.horizontal_wrapped(|ui| {
        if ui.checkbox(&mut on, tr("자막 켜기")).changed() {
            set_cap(app, "on", serde_json::json!(on));
            if !on {
                wp_core::voicefollow::set_caption("", &[], 0.0);
                app.core.lock().overlays_changed();
            }
        }
        let a = ui.checkbox(&mut to_obs, tr("방송(OBS)")).changed();
        let b = ui.checkbox(&mut to_main, tr("예배당 화면에도")).changed();
        if a || b {
            let mut v = vec![];
            if to_obs {
                v.push("overlay");
            }
            if to_main {
                v.push("main");
            }
            set_cap(app, "outputs", serde_json::json!(v));
        }
        ui.label(tr("글자 크기"));
        if ui.add(egui::DragValue::new(&mut size).range(20.0..=90.0)).changed() {
            set_cap(app, "size", serde_json::json!(size));
        }
    });
    ui.horizontal_wrapped(|ui| {
        let has = app.core.lock().ai_cfg().has_key() || std::env::var("WP_AI_MOCK").is_ok();
        let r = ui.add_enabled(has, egui::Checkbox::new(&mut translate, tr("영어 번역 (AI)"))).on_disabled_hover_text(tr("AI 키가 필요합니다"));
        if r.changed() {
            if translate && !crate::ai_ui::ready(app, "translate") {
                // 동의 창이 뜬다. 동의하면 다시 켜면 된다
            } else {
                set_cap(app, "translate", serde_json::json!(translate));
            }
        }
        if !has {
            ui.label(RichText::new(tr("AI 키가 필요합니다")).small().color(p.muted));
        }
        if translate && ui.checkbox(&mut only_en, tr("번역만 (한국어 줄 빼기)")).changed() {
            set_cap(app, "onlyEn", serde_json::json!(only_en));
        }
    });
    if crate::ai_ui::take_granted("translate") {
        set_cap(app, "translate", serde_json::json!(true));
    }
    let en = ST.lock().last_en.clone();
    if let Some(c) = wp_core::voicefollow::caption() {
        ui.label(RichText::new(tr_args("지금 자막: {1}", &[&c.text.replace('\n', " / ")])).small().color(p.text2));
    } else if !en.is_empty() {
        ui.label(RichText::new(tr_args("마지막 번역: {1}", &[&en])).small().color(p.muted));
    }
}

/// 조작 화면 오른쪽 아래 '듣는 중' 표시 (빨강·노랑은 쓰지 않음)
fn listening_badge(ctx: &egui::Context) {
    let (on, level) = {
        let s = ST.lock();
        (s.listening, s.level)
    };
    if !on {
        return;
    }
    let p = crate::theme::pal();
    egui::Area::new(egui::Id::new("wp-listening"))
        .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-16.0, -36.0))
        .order(egui::Order::Foreground)
        .interactable(true)
        .show(ctx, |ui| {
            egui::Frame::new().fill(p.panel2).stroke(egui::Stroke::new(1.0, p.line2)).corner_radius(6.0).inner_margin(egui::Margin::symmetric(10, 5)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    crate::icons::show(ui, "mic", 14.0, p.text); // 노랑(다음)·빨강(송출)은 쓰지 않는다
                    let r = ui.add(egui::Label::new(RichText::new(tr("듣는 중 (음성 인식)")).color(p.text).small()).sense(egui::Sense::click()));
                    // 소리 크기 막대
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(36.0, 6.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 2.0, p.sunken);
                    let w = (level * 12.0).clamp(0.0, 1.0) * rect.width();
                    ui.painter().rect_filled(egui::Rect::from_min_size(rect.min, egui::vec2(w, rect.height())), 2.0, p.text2);
                    if r.on_hover_text(tr("눌러서 음성 인식 창 열기")).clicked() {
                        ST.lock().open = true;
                    }
                });
            });
        });
    ctx.request_repaint_after(std::time::Duration::from_millis(250));
}

pub fn show(app: &mut App, ctx: &egui::Context) {
    listening_badge(ctx);
    // 점검용: --listen-start (창을 열고 바로 듣기 시작)
    static AUTO: std::sync::Once = std::sync::Once::new();
    AUTO.call_once(|| {
        if std::env::args().any(|a| a == "--listen-start") {
            ST.lock().open = true;
            start(app, ctx);
        }
    });
    if !ST.lock().open {
        return;
    }
    let data = app.core.lock().data_dir.clone();
    let model = settings_model(app);
    let (has_engine, has_model) = ready(&data, &model);
    let mut open = true;
    let mut do_start = false;
    let mut do_stop = false;
    let mut put_next: Option<String> = None;
    crate::widgets::float_window(ctx, "listen", tr("음성 인식: 구절 따라가기 · 실시간 자막"), [580.0, 700.0]).keep_on_outside().open(&mut open).show(ctx, |ui| {
        let p = crate::theme::pal();
        ui.label(RichText::new(tr("마이크 소리를 이 PC 안에서 글로 바꾸고, '요한복음 15장 5절' 같은 말을 찾아 목록에 보여 줍니다. 음성은 PC 밖으로 나가지 않습니다. 찾은 구절은 '다음' 칸까지만 올리고 송출은 하지 않습니다.")).small().color(p.muted));
        ui.add_space(4.0);
        // 준비
        let mut s = ST.lock();
        if !has_engine || !has_model {
            ui.label(RichText::new(tr("처음 한 번 음성 인식 엔진과 모델을 받아야 합니다.")).strong());
            if !stt::CAN_DOWNLOAD_ENGINE && !has_engine {
                ui.label(tr("맥: 터미널에서 'brew install whisper-cpp' 로 엔진을 설치한 뒤 이 창을 다시 여세요."));
            }
            if let Some((n, got, all)) = &s.dl {
                ui.label(tr_args("받는 중: {1}", &[n]));
                ui.add(egui::ProgressBar::new(*got as f32 / (*all).max(1) as f32).text(format!("{:.0} / {:.0} MB", *got as f64 / 1e6, *all as f64 / 1e6)));
                if ui.add(Btn::new(tr("멈추기")).size(Size::Sm)).clicked() {
                    CANCEL_DL.store(true, Ordering::SeqCst);
                }
            } else if s.ask_download {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.label(RichText::new(tr("받을 파일")).strong());
                    let mut list = vec![];
                    if !has_engine && stt::CAN_DOWNLOAD_ENGINE {
                        list.push(&stt::ENGINE_WIN);
                    }
                    if !has_model {
                        list.push(stt::model(&model));
                    }
                    let mut total = 0u64;
                    for a in &list {
                        total += a.size;
                        ui.label(format!("· {} — {:.0} MB · {}", a.name, a.size as f64 / 1e6, a.license));
                        ui.label(RichText::new(format!("   {}", a.url)).small().color(p.muted));
                    }
                    ui.label(RichText::new(tr_args("모두 약 {1} MB. 받은 뒤 SHA-256 을 대조해 다르면 쓰지 않습니다. 자료 폴더의 stt 에 둡니다.", &[&format!("{:.0}", total as f64 / 1e6)])).small());
                    ui.horizontal(|ui| {
                        if ui.add(Btn::new(tr("동의하고 받기")).kind(Kind::Primary)).clicked() {
                            s.ask_download = false;
                            s.err.clear();
                            start_download(data.clone(), model.clone(), ctx.clone());
                        }
                        if ui.button(tr("취소")).clicked() {
                            s.ask_download = false;
                        }
                    });
                });
            } else if (has_engine || stt::CAN_DOWNLOAD_ENGINE) && ui.add(Btn::new(tr("엔진 · 모델 받기…")).icon("download")).clicked() {
                s.ask_download = true;
            }
        }
        // 모델 고르기
        ui.horizontal_wrapped(|ui| {
            ui.label(tr("모델"));
            for m in stt::MODELS {
                let on = model == m.id;
                let have = stt::model_path(&data, m.id).is_file();
                let label = format!("{}{}", m.id, if have { "" } else { " (안 받음)" });
                if ui.add(Btn::new(label).selected(on).size(Size::Sm).tooltip(tr(m.name))).clicked() && !s.listening {
                    let id = m.id.to_string();
                    app.core.lock().settings.update(|d| {
                        if !d["stt"].is_object() {
                            d["stt"] = serde_json::json!({});
                        }
                        d["stt"]["model"] = serde_json::json!(id);
                    });
                }
            }
        });
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            let can = has_engine && has_model && !s.starting;
            if s.listening {
                if ui.add(Btn::new(tr("듣기 멈추기")).icon("pause")).clicked() {
                    do_stop = true;
                }
            } else if ui.add(Btn::new(tr("듣기 시작")).icon("mic").kind(Kind::Primary).enabled(can)).clicked() {
                do_start = true;
            }
            let mut auto = auto_next(app);
            if ui.checkbox(&mut auto, tr("찾으면 바로 '다음' 칸에 올리기")).on_hover_text(tr("켜도 '다음(미리보기)' 까지만 — 송출은 직접 합니다")).changed() {
                app.core.lock().settings.update(|d| {
                    if !d["stt"].is_object() {
                        d["stt"] = serde_json::json!({});
                    }
                    d["stt"]["autoNext"] = serde_json::json!(auto);
                });
            }
        });
        if !s.status.is_empty() {
            ui.label(RichText::new(s.status.clone()).small().color(p.text2));
        }
        if !s.err.is_empty() {
            ui.label(RichText::new(s.err.clone()).small().color(p.text));
        }
        if !s.stats.is_empty() {
            let n = s.stats.len() as f32;
            let (a, b) = s.stats.iter().fold((0.0, 0.0), |(x, y), (p, q)| (x + p, y + q));
            ui.label(RichText::new(tr_args("조각 평균 {1}초 · 인식 평균 {2}초 (최근 {3}개)", &[&format!("{:.1}", a / n), &format!("{:.1}", b / n), &(n as usize)])).small().color(p.muted));
        }
        drop(s);
        ui.separator();
        captions_ui(app, ui);
        ui.separator();
        let s = ST.lock();
        ui.label(RichText::new(tr("찾은 구절")).strong());
        if s.found.is_empty() {
            ui.label(RichText::new(tr("아직 없습니다.")).small().color(p.muted));
        }
        egui::ScrollArea::vertical().id_salt("ls-found").max_height(150.0).show(ui, |ui| {
            for (hms, r, put) in s.found.iter().rev() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(hms.as_str()).small().color(p.muted));
                    ui.label(RichText::new(r.as_str()).strong());
                    if *put {
                        ui.label(RichText::new(tr("다음 칸에 올림")).small().color(p.muted));
                    } else if ui.add(Btn::new(tr("다음 칸에 올리기")).size(Size::Sm)).clicked() {
                        put_next = Some(r.clone());
                    }
                });
            }
        });
        ui.add_space(4.0);
        ui.label(RichText::new(tr("받아 적은 글")).strong());
        egui::ScrollArea::vertical().id_salt("ls-lines").max_height(180.0).stick_to_bottom(true).show(ui, |ui| {
            for (hms, t) in &s.lines {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(hms.as_str()).small().color(p.muted));
                    ui.label(t.as_str());
                });
            }
        });
    });
    if do_start {
        start(app, ctx);
    }
    if do_stop {
        stop();
    }
    if let Some(r) = put_next {
        let mut c = app.core.lock();
        match c.preview_passage_quiet(&r) {
            Ok(t) => {
                c.notify(tr_args("다음 칸에 올렸습니다: {1} (송출은 직접)", &[&t]), "ok");
                drop(c);
                if let Some(x) = ST.lock().found.iter_mut().rev().find(|x| x.1 == r) {
                    x.2 = true;
                }
            }
            Err(e) => c.notify(e, "warn"),
        }
    }
    if !open {
        ST.lock().open = false;
    }
}
