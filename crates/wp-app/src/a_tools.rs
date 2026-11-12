//! [트랙 A] 도구 창: 설교 포인트 (Electron control/points.js) · 예배 전 화면 (control/preservice.js) · 홈페이지 주보 (control/home.js)
//! 메뉴 도구 → 'a:points' · 'a:preservice' · 'a:home'. 오래 걸리는 일(AI · 홈페이지 받기)은 다른 스레드에서, 결과는 여기 상태로.

use crate::app::{Act, App};
use eframe::egui::{self, Color32, RichText};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde_json::{json, Value};
use wp_core::{homefeed, hymns, preservice, sermonpoints as sp};

#[derive(Default)]
struct Points {
    open: bool,
    text: String,
    title: String,
    verse: String,
    speaker: String,
    extra: String,
    answer: String,
    data: Option<Value>,
    numbering: String,
    theme: bool,
    application: bool,
    busy: bool,
}

#[derive(Default)]
struct Pre {
    open: bool,
    loaded: bool,
    card_folder: String,
    groups: Vec<Value>,
    use_group: Vec<bool>,
    bundle_dir: String,
    bundle: Option<Value>,
    use_card_line: bool,
    qt_text: String,
    qt: Option<Value>,
    countdown: String,
    auto_sec: f64,
}

#[derive(Default)]
struct Home {
    open: bool,
    url: String,
    feed: Option<Value>,
    b: usize,
    service: usize,
    news: Vec<bool>,
    events: bool,
    prayers: bool,
    busy: bool,
}

#[derive(Default)]
struct St {
    points: Points,
    pre: Pre,
    home: Home,
}

static ST: Lazy<Mutex<St>> = Lazy::new(|| Mutex::new(St { points: Points { numbering: "word".into(), theme: true, application: true, ..Default::default() }, ..Default::default() }));

/// 메뉴 'a:…' → 창 열기 (모르는 id 면 false)
pub fn handle_menu(id: &str) -> bool {
    let mut s = ST.lock();
    match id {
        "a:points" => s.points.open = true,
        "a:preservice" => s.pre.open = true,
        "a:home" => s.home.open = true,
        _ => return false,
    }
    true
}

/// 폴더 고르기 창 (Electron pickFolder 명령)
pub fn pick_folder(title: &str) -> Option<String> {
    rfd::FileDialog::new().set_title(title).pick_folder().map(|p| p.to_string_lossy().to_string())
}

fn open_link(url: &str) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).creation_flags(0x0800_0000).spawn();
    }
    #[cfg(not(windows))]
    let _ = url;
}

/// 점검용: --open-a points|preservice|home
pub fn open_from_args() {
    let a: Vec<String> = std::env::args().collect();
    if let Some(v) = a.iter().position(|x| x == "--open-a").and_then(|i| a.get(i + 1)) {
        for id in v.split(',') {
            handle_menu(&format!("a:{id}"));
        }
    }
    // 점검용: 원고 파일을 넣고 규칙으로 찾기 · 홈페이지 주소
    let mut s = ST.lock();
    if let Some(f) = a.iter().position(|x| x == "--points-file").and_then(|i| a.get(i + 1)) {
        if let Ok(t) = sp::read_text_file(std::path::Path::new(f)) {
            s.points.data = sp::find_points(&t, &json!({})).ok();
            s.points.text = t;
        }
    }
    if let Some(u) = a.iter().position(|x| x == "--home-url").and_then(|i| a.get(i + 1)) {
        if let Ok(f) = homefeed::fetch_feed(u) {
            s.home.news = vec![true; f["bulletins"][0]["news"].as_array().map(|x| x.len()).unwrap_or(0)];
            s.home.feed = Some(f);
            s.home.url = u.clone();
        }
    }
}

pub fn show(app: &mut App, ctx: &egui::Context, acts: &mut Vec<Act>) {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(open_from_args);
    points_win(app, ctx, acts);
    pre_win(app, ctx);
    home_win(app, ctx, acts);
}

// ---------------- 설교 포인트 ----------------
fn meta(p: &Points) -> Value {
    json!({ "title": p.title.trim(), "verse": p.verse.trim(), "speaker": p.speaker.trim() })
}

fn points_win(app: &mut App, ctx: &egui::Context, acts: &mut Vec<Act>) {
    let mut st = ST.lock();
    if !st.points.open {
        return;
    }
    let mut open = true;
    let p = &mut st.points;
    egui::Window::new("설교 포인트 슬라이드").open(&mut open).default_size([720.0, 640.0]).show(ctx, |ui| {
        ui.label(RichText::new("설교 원고(또는 자막)에서 대지를 뽑아, 예배 중에 띄울 '제목' 순서 하나로 만듭니다.").small().color(Color32::GRAY));
        ui.horizontal(|ui| {
            if ui.button("원고 파일 열기…").clicked() {
                if let Some(f) = rfd::FileDialog::new().add_filter("설교 원고", &["hwp", "hwpx", "docx", "txt", "md", "srt", "vtt"]).pick_file() {
                    match sp::read_text_file(&f) {
                        Ok(t) => p.text = t,
                        Err(e) => app.core.lock().notify(e, "error"),
                    }
                }
            }
            ui.label(RichText::new("hwp · hwpx · docx · txt · md · srt · vtt").small().color(Color32::GRAY));
        });
        egui::ScrollArea::vertical().id_salt("pt-text").max_height(160.0).show(ui, |ui| {
            ui.add(egui::TextEdit::multiline(&mut p.text).desired_rows(7).desired_width(f32::INFINITY).hint_text("설교 원고를 붙여 넣으세요 (AI 답 JSON 을 붙여 넣어도 됩니다)"));
        });
        egui::Grid::new("pt-meta").num_columns(2).show(ui, |ui| {
            for (l, v, h) in [("제목", &mut p.title, "비우면 원고 머리의 '제목:'"), ("본문", &mut p.verse, "예: 롬 1:16-17"), ("설교자", &mut p.speaker, ""), ("AI 추가 요청", &mut p.extra, "예: 대지를 3개로")] {
                ui.label(l);
                ui.add(egui::TextEdit::singleline(v).hint_text(h).desired_width(420.0));
                ui.end_row();
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui.button("규칙으로 찾기").on_hover_text("'첫째·둘째', '1. 2.', 'Ⅰ. Ⅱ.', '대지 1' 표시를 찾습니다 (AI 없이)").clicked() {
                match sp::find_points(&p.text, &meta(p)) {
                    Ok(d) => p.data = Some(d),
                    Err(e) => app.core.lock().notify(e, "warn"),
                }
            }
            if ui.add_enabled(!p.busy, egui::Button::new(if p.busy { "AI 가 읽는 중…" } else { "AI 로 뽑기" })).on_hover_text("주보 AI 와 같은 오픈라우터 키·모델").clicked() {
                let (key, ai) = {
                    let c = app.core.lock();
                    (c.ai_key(), c.settings.get()["ai"].clone())
                };
                let (text, m, extra) = (p.text.clone(), meta(p), p.extra.clone());
                p.busy = true;
                hymns::run_bg(
                    move || sp::ask_points(&key, ai["model"].as_str().unwrap_or(""), ai["endpoint"].as_str().unwrap_or(""), &text, &m, &extra),
                    |c, r| {
                        let mut s = ST.lock();
                        s.points.busy = false;
                        match r {
                            Ok(v) => s.points.data = Some(v["data"].clone()),
                            Err(e) => c.notify(e, "error"),
                        }
                    },
                );
            }
            if ui.button("채팅 AI 프롬프트 복사").on_hover_text("챗GPT·클로드·제미나이에 붙여 넣고, 답을 아래 칸에 붙여 넣으세요").clicked() {
                match sp::chat_prompt(&p.text, &meta(p), p.extra.trim()) {
                    Ok(t) => {
                        ui.ctx().copy_text(t);
                        app.core.lock().notify("프롬프트를 복사했습니다. 채팅 AI 에 붙여 넣고, 답을 'AI 답' 칸에 붙여 넣으세요.", "ok");
                    }
                    Err(e) => app.core.lock().notify(e, "warn"),
                }
            }
            for (n, u) in wp_core::bulletin::CHAT_LINKS {
                if ui.small_button(n).on_hover_text(u).clicked() {
                    open_link(u);
                }
            }
        });
        ui.horizontal(|ui| {
            let go = ui.button("답 읽기").clicked();
            ui.add(egui::TextEdit::singleline(&mut p.answer).hint_text("채팅 AI 답 (JSON) 붙여 넣기").desired_width(f32::INFINITY));
            if go {
                match sp::parse_answer(&p.answer, &meta(p)) {
                    Ok(d) => {
                        p.data = Some(d);
                        p.answer.clear();
                    }
                    Err(e) => app.core.lock().notify(e, "error"),
                }
            }
        });
        ui.separator();
        let Some(d) = p.data.as_mut() else {
            ui.label(RichText::new("대지를 찾으면 여기서 고칠 수 있습니다.").color(Color32::GRAY));
            return;
        };
        egui::ScrollArea::vertical().id_salt("pt-data").max_height(240.0).show(ui, |ui| {
            let mut field = |ui: &mut egui::Ui, v: &mut Value, label: &str| {
                let mut t = v.as_str().unwrap_or("").to_string();
                ui.horizontal(|ui| {
                    ui.label(RichText::new(label).small());
                    if ui.add(egui::TextEdit::singleline(&mut t).desired_width(f32::INFINITY)).changed() {
                        *v = json!(t);
                    }
                });
            };
            field(ui, &mut d["title"], "설교 제목");
            field(ui, &mut d["verse"], "본문");
            field(ui, &mut d["speaker"], "설교자");
            field(ui, &mut d["theme"], "핵심 한 문장");
            let mut del = None;
            let n = d["points"].as_array().map(|a| a.len()).unwrap_or(0);
            for i in 0..n {
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("대지 {}", i + 1)).strong());
                        if ui.small_button("빼기").clicked() {
                            del = Some(i);
                        }
                    });
                    field(ui, &mut d["points"][i]["heading"], "제목");
                    field(ui, &mut d["points"][i]["body"], "핵심 문장");
                });
            }
            if let Some(i) = del {
                d["points"].as_array_mut().unwrap().remove(i);
            }
            if n < 6 && ui.small_button("+ 대지 더하기").clicked() {
                if !d["points"].is_array() {
                    d["points"] = json!([]);
                }
                d["points"].as_array_mut().unwrap().push(json!({ "heading": "", "body": "" }));
            }
            field(ui, &mut d["application"], "이번 주 적용");
        });
        ui.horizontal(|ui| {
            ui.label("번호");
            for (v, l) in [("word", "첫째, 둘째"), ("num", "1. 2."), ("none", "없음")] {
                ui.radio_value(&mut p.numbering, v.to_string(), l);
            }
            ui.checkbox(&mut p.theme, "핵심 장");
            ui.checkbox(&mut p.application, "적용 장");
            if ui.button(RichText::new("큐시트에 넣기").strong()).clicked() {
                match sp::to_cue_item(p.data.as_ref().unwrap(), &p.numbering, p.theme, p.application) {
                    Ok(it) => {
                        acts.push(Act::AddItem("title", Some(it.clone())));
                        app.core.lock().notify(format!("설교 포인트를 큐시트에 넣었습니다 ({}).", it["note"].as_str().unwrap_or("")), "ok");
                    }
                    Err(e) => app.core.lock().notify(e, "error"),
                }
            }
        });
    });
    if !open {
        st.points.open = false;
    }
}

// ---------------- 예배 전 화면 ----------------
fn pre_win(app: &mut App, ctx: &egui::Context) {
    let mut st = ST.lock();
    if !st.pre.open {
        return;
    }
    let p = &mut st.pre;
    if !p.loaded {
        let i = preservice::info(&app.core.lock());
        p.card_folder = i["cardFolder"].as_str().unwrap_or("").into();
        p.bundle_dir = i["bundleDir"].as_str().unwrap_or("").into();
        p.auto_sec = i["autoSec"].as_f64().unwrap_or(8.0);
        p.use_card_line = true;
        p.loaded = true;
    }
    let mut open = true;
    egui::Window::new("예배 전 화면 만들기").open(&mut open).default_size([640.0, 560.0]).show(ctx, |ui| {
        ui.label(RichText::new("지난주 말씀카드 + 이번 주 QT 첫날 본문 → 자동으로 넘기며 반복하는 슬라이드쇼 순서 (큐시트 맨 앞에)").small().color(Color32::GRAY));
        ui.label(RichText::new("말씀카드 그림").strong());
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut p.card_folder).hint_text("설교쇼츠 편집기에서 카드 PNG 를 저장한 폴더").desired_width(360.0));
            if ui.button("고르기…").clicked() {
                if let Some(f) = pick_folder("말씀카드 그림 폴더") {
                    p.card_folder = f;
                }
            }
            if ui.button("불러오기").clicked() {
                match preservice::cards(&mut app.core.lock(), &p.card_folder) {
                    Ok(v) => {
                        p.groups = v["groups"].as_array().cloned().unwrap_or_default();
                        p.use_group = (0..p.groups.len()).map(|i| i == 0).collect();
                    }
                    Err(e) => app.core.lock().notify(e, "error"),
                }
            }
        });
        for (i, g) in p.groups.iter().enumerate() {
            let n = g["files"].as_array().map(|a| a.len()).unwrap_or(0);
            ui.checkbox(&mut p.use_group[i], format!("{}  ({}장){}", g["base"].as_str().unwrap_or(""), n, if i == 0 { " · 가장 최근" } else { "" }));
        }
        ui.separator();
        ui.label(RichText::new("말씀 글 (설교쇼츠 '홈페이지 묶음')").strong());
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut p.bundle_dir).hint_text("…\\홈페이지보내기").desired_width(360.0));
            if ui.button("고르기…").clicked() {
                if let Some(f) = pick_folder("홈페이지 묶음 폴더") {
                    p.bundle_dir = f;
                }
            }
            if ui.button("불러오기").clicked() {
                p.bundle = preservice::bundle(&mut app.core.lock(), &p.bundle_dir);
                if p.bundle.is_none() {
                    app.core.lock().notify("이 폴더에서 오늘 이전의 묶음(날짜 폴더의 bundle.json)을 찾지 못했습니다.", "warn");
                }
            }
        });
        if let Some(b) = &p.bundle {
            ui.checkbox(&mut p.use_card_line, format!("{} · {}", b["date"].as_str().unwrap_or(""), b["cardLine"].as_str().unwrap_or("")));
        }
        ui.separator();
        ui.label(RichText::new("이번 주 QT 첫날 본문").strong());
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::multiline(&mut p.qt_text).desired_rows(2).hint_text("마 6:9-13, 또는 QT 자료(JSON · '1일째 · 제목 (장절)')").desired_width(420.0));
            if ui.button("본문 찾기").clicked() {
                match preservice::qt(&mut app.core.lock(), &p.qt_text) {
                    Ok(q) => p.qt = Some(q),
                    Err(e) => app.core.lock().notify(e, "warn"),
                }
            }
        });
        if let Some(q) = &p.qt {
            ui.label(RichText::new(format!("{} · {}절", q["passage"]["title"].as_str().unwrap_or(""), q["passage"]["verses"].as_array().map(|a| a.len()).unwrap_or(0))).color(Color32::from_rgb(0xff, 0xd4, 0x79)));
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("예배 시작");
            ui.add(egui::TextEdit::singleline(&mut p.countdown).hint_text("11:00").desired_width(60.0));
            ui.label("넘김");
            ui.add(egui::DragValue::new(&mut p.auto_sec).range(3.0..=60.0).suffix("초"));
            if ui.button(RichText::new("만들어 큐시트 맨 앞에").strong()).clicked() {
                let images: Vec<Value> = p.groups.iter().zip(&p.use_group).filter(|(_, u)| **u).flat_map(|(g, _)| g["files"].as_array().cloned().unwrap_or_default()).map(|f| f["url"].clone()).collect();
                let line = p.bundle.as_ref().filter(|_| p.use_card_line).map(|b| b["cardLine"].clone()).unwrap_or(json!(""));
                let a = json!({ "images": images, "cardLine": line, "qt": p.qt.clone().unwrap_or(Value::Null), "countdown": p.countdown.trim(), "autoSec": p.auto_sec });
                let mut c = app.core.lock();
                match preservice::item(&mut c, &a) {
                    Ok(it) => {
                        let mut x = wp_core::core::new_item("slideshow", "예배 전 화면");
                        wp_core::settings::merge_into(&mut x, &it);
                        c.add_item(x, Some(0));
                        c.notify(format!("예배 전 화면을 큐시트 맨 앞에 넣었습니다 ({}).", it["note"].as_str().unwrap_or("")), "ok");
                    }
                    Err(e) => c.notify(e, "warn"),
                }
            }
        });
    });
    if !open {
        st.pre.open = false;
    }
}

// ---------------- 홈페이지 주보 ----------------
fn home_win(app: &mut App, ctx: &egui::Context, acts: &mut Vec<Act>) {
    let mut st = ST.lock();
    if !st.home.open {
        return;
    }
    let h = &mut st.home;
    if h.url.is_empty() {
        h.url = app.core.lock().settings.get()["home"]["feedUrl"].as_str().unwrap_or("").to_string();
    }
    let mut open = true;
    egui::Window::new("홈페이지 주보").open(&mut open).default_size([620.0, 520.0]).show(ctx, |ui| {
        ui.label(RichText::new("교회 홈페이지의 주보 JSON → 큐시트 · 광고 슬라이드").small().color(Color32::GRAY));
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut h.url).hint_text("https://교회주소/bulletin/feed.json").desired_width(420.0));
            if ui.add_enabled(!h.busy, egui::Button::new(if h.busy { "받는 중…" } else { "받기" })).clicked() {
                h.busy = true;
                let url = h.url.trim().to_string();
                hymns::run_bg(
                    move || homefeed::fetch_feed(&url).map(|f| (f, url)),
                    |c, r| {
                        let mut s = ST.lock();
                        s.home.busy = false;
                        match r {
                            Ok((f, url)) => {
                                let u: String = url.chars().take(500).collect();
                                c.settings.update(|d| {
                                    if !d["home"].is_object() {
                                        d["home"] = json!({});
                                    }
                                    d["home"]["feedUrl"] = json!(u);
                                });
                                // 열어 둔 큐시트의 예배 날짜와 같은 주보 (없으면 가장 최근)
                                let date = c.cue.as_ref().map(|q| q.service_date.clone()).unwrap_or_default();
                                let list = f["bulletins"].as_array().cloned().unwrap_or_default();
                                s.home.b = list.iter().position(|b| !date.is_empty() && b["date"] == date.as_str()).unwrap_or(0);
                                s.home.service = 0;
                                s.home.news = vec![true; list.get(s.home.b).and_then(|b| b["news"].as_array()).map(|a| a.len()).unwrap_or(0)];
                                c.notify(format!("주보 {}개를 받았습니다 ({}).", list.len(), f["church"].as_str().unwrap_or("")), "ok");
                                s.home.feed = Some(f);
                            }
                            Err(e) => c.notify(e, "error"),
                        }
                    },
                );
            }
        });
        let Some(f) = h.feed.clone() else {
            ui.label(RichText::new("주소를 넣고 받기를 누르세요.").color(Color32::GRAY));
            return;
        };
        let list = f["bulletins"].as_array().cloned().unwrap_or_default();
        let lab = |b: &Value| format!("{} {}", b["date"].as_str().unwrap_or(""), b["issue"].as_str().unwrap_or("")).trim().to_string();
        let before = h.b;
        egui::ComboBox::from_label("주보").selected_text(list.get(h.b).map(lab).unwrap_or_default()).show_ui(ui, |ui| {
            for (i, b) in list.iter().enumerate() {
                ui.selectable_value(&mut h.b, i, lab(b));
            }
        });
        let Some(b) = list.get(h.b).cloned() else { return };
        if before != h.b || h.news.len() != b["news"].as_array().map(|a| a.len()).unwrap_or(0) {
            h.news = vec![true; b["news"].as_array().map(|a| a.len()).unwrap_or(0)];
            h.service = 0;
        }
        if let Some(t) = b["sermon"]["title"].as_str().filter(|x| !x.is_empty()) {
            ui.label(format!("설교: {} {}", t, b["sermon"]["passage"].as_str().unwrap_or("")));
        }
        let svcs = b["services"].as_array().cloned().unwrap_or_default();
        ui.horizontal_wrapped(|ui| {
            ui.label("예배");
            for (i, v) in svcs.iter().enumerate() {
                ui.selectable_value(&mut h.service, i, format!("{} {} ({}순서)", v["name"].as_str().unwrap_or(""), v["time"].as_str().unwrap_or(""), v["items"].as_array().map(|a| a.len()).unwrap_or(0)));
            }
        });
        ui.label(RichText::new("광고 (교회 소식 순서에 넣을 것)").strong());
        egui::ScrollArea::vertical().id_salt("home-news").max_height(160.0).show(ui, |ui| {
            for (i, n) in b["news"].as_array().cloned().unwrap_or_default().iter().enumerate() {
                if let Some(v) = h.news.get_mut(i) {
                    ui.checkbox(v, n["title"].as_str().filter(|x| !x.is_empty()).unwrap_or(n["body"].as_str().unwrap_or("")));
                }
            }
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut h.events, format!("이번 주 일정 ({})", b["events"].as_array().map(|a| a.len()).unwrap_or(0)));
            ui.checkbox(&mut h.prayers, format!("기도 제목 ({})", b["prayers"].as_array().map(|a| a.len()).unwrap_or(0)));
        });
        let sel = json!({ "news": h.news.iter().enumerate().filter(|(_, v)| **v).map(|(i, _)| i).collect::<Vec<_>>(), "events": h.events, "prayers": h.prayers });
        ui.horizontal(|ui| {
            if ui.add_enabled(!svcs.is_empty(), egui::Button::new(RichText::new("이 예배로 큐시트 만들기").strong())).clicked() {
                let mut c = app.core.lock();
                match homefeed::make_cue(&mut c, &b, h.service, &sel) {
                    Ok(r) => {
                        let warn = r["warnings"].as_array().map(|a| a.len()).unwrap_or(0);
                        c.notify(format!("홈페이지 주보로 큐시트를 만들었습니다{}{}.", if r["newsPlaced"] == json!(true) { " (교회 소식에 광고)" } else { "" }, if warn > 0 { format!(" · 확인할 것 {warn}개") } else { String::new() }), "ok");
                    }
                    Err(e) => c.notify(e, "error"),
                }
            }
            if ui.button("광고만 순서로 넣기").clicked() {
                match homefeed::news_item(&b, &sel) {
                    Ok(it) => acts.push(Act::AddItem("text", Some(it))),
                    Err(e) => app.core.lock().notify(e, "warn"),
                }
            }
        });
    });
    if !open {
        st.home.open = false;
    }
}
