//! [트랙 A] 찬송가·교독문 PPT 와 베들레헴 찬송가: 폴더 등록·읽는 방식 · 찾기 · 찬송가 순서 편집칸 · PPT 도구(요소 꺼내기·나누기)
//! · 클립보드 붙여 넣기(Ctrl+V: 글·그림·PowerPoint 슬라이드) · 받은 찬양 PPT 지켜보기

use crate::app::{Act, App};
use eframe::egui::{self, Color32, RichText};
use serde_json::{json, Value};
use wp_core::{clippaste, hymnal, hymns, pptsplit, songinbox};

const Q: &str = "hymn-q";
const RES: &str = "hymn-res";
const OPEN: &str = "hymn-open";
const SRC: &str = "hymn-src"; // ppt | beth
const ANSWER: &str = "hymn-ai-answer";
const CONFIRM: &str = "hymn-ppt-confirm"; // 다시 꺼내기·원래대로 확인 ("again:<순서 id>" | "off:<id>")

fn id(k: &str) -> egui::Id {
    egui::Id::new(k)
}

fn get<T: Clone + Default + Send + Sync + 'static>(ui: &egui::Ui, k: &str) -> T {
    ui.data(|d| d.get_temp(id(k)).unwrap_or_default())
}
fn set<T: Clone + Send + Sync + 'static>(ui: &egui::Ui, k: &str, v: T) {
    ui.data_mut(|d| d.insert_temp(id(k), v));
}

/// 큐시트 아래 '찬송가 찾기' + 붙여 넣기 + 고른 PPT 순서의 도구
/// 어느 탭이든 늘 (Ctrl+V 붙여 넣기 받기)
pub fn global(app: &mut App, ui: &mut egui::Ui, acts: &mut Vec<Act>) {
    paste(app, ui, acts);
}

/// 왼쪽 '찬송가' 탭: 찬송가·교독문 PPT · 베들레헴 찾기, 붙여 넣기, 온라인에서 찾기
pub fn tab(app: &mut App, ui: &mut egui::Ui, acts: &mut Vec<Act>) {
    set(ui, OPEN, true);
    search_box(app, ui, acts);
}

/// 실제 PPT 파일을 가진 순서만 기존 나누기 도구를 열 수 있다 (베들레헴 제외).
pub(crate) fn can_open_ppt_tools(it: &Value) -> bool {
    it["type"] == "hymn" && it["file"].as_str().is_some_and(|f| !f.trim().is_empty())
}

/// 실행하지 않고 기존 PPT 도구 칸을 펼친다. API 호출은 해당 단추를 누를 때만 한다.
pub(crate) fn open_ppt_tools(ctx: &egui::Context, it: &Value) {
    ctx.data_mut(|d| d.insert_temp(id("ppt-tools-request"), it["id"].as_str().unwrap_or("").to_string()));
}

/// 찬송가 순서 편집칸 안의 접는 칸: PPT 요소 꺼내기 · 나누기
pub fn ppt_box(app: &mut App, ui: &mut egui::Ui, i: usize, it: &Value) {
    if !can_open_ppt_tools(it) {
        return;
    }
    let requested = ui.data_mut(|d| {
        let key = id("ppt-tools-request");
        let matches = d.get_temp::<String>(key).is_some_and(|v| v == it["id"].as_str().unwrap_or(""));
        if matches { d.remove::<String>(key); }
        matches
    });
    let response = egui::CollapsingHeader::new(RichText::new(wp_core::i18n::tr("PPT 도구 (요소 따로 꺼내기 · 자동으로 나누기)")).color(Color32::from_rgb(0xf0, 0x9a, 0x4a)))
        .id_salt("ppt-box").open(requested.then_some(true)).show(ui, |ui| ppt_tools(app, ui, i, it));
    if requested { response.header_response.scroll_to_me(Some(egui::Align::Center)); }
}

fn search_box(app: &mut App, ui: &mut egui::Ui, acts: &mut Vec<Act>) {
    // 왼쪽 패널을 넓히지 않게: 지금 폭 안에서만
    let w = ui.available_width().max(200.0);
    ui.scope(|ui| {
        ui.set_max_width(w);
        search_inner(app, ui, acts);
    });
}

fn search_inner(app: &mut App, ui: &mut egui::Ui, acts: &mut Vec<Act>) {
    let mut open: bool = get(ui, OPEN);
    ui.horizontal(|ui| {
        ui.label(RichText::new(wp_core::i18n::tr("찬송가·교독문")).strong().color(Color32::from_rgb(0xf0, 0x9a, 0x4a)));
        open = true;
        if ui.small_button(wp_core::i18n::tr("붙여 넣기")).on_hover_text(wp_core::i18n::tr("클립보드의 PowerPoint 슬라이드·그림을 새 순서로 (글은 Ctrl+V)")).clicked() {
            grab_clipboard(app);
        }
        let st = songinbox::status(&app.core.lock());
        let watching = st["watching"].as_bool().unwrap_or(false);
        let label = if watching { format!("받는 중 ({}분)", st["secondsLeft"].as_u64().unwrap_or(0) / 60 + 1) } else { "온라인에서 찾기".into() };
        if ui
            .small_button(label)
            .on_hover_text(wp_core::i18n::tr_args("다운로드 폴더({1})에 새 찬양 PPT 가 들어오면 '{2}' 로 옮겨 큐시트에 넣습니다 (20분). 다시 누르면 그만", &[&(st["downloads"].as_str().unwrap_or("")), &(st["target"].as_str().unwrap_or(""))]))
            .clicked()
        {
            if watching {
                songinbox::disarm();
            } else {
                let q: String = get(ui, Q);
                let tpl = app.core.lock().settings.str(&["songSearchUrl"]);
                if let Ok(url) = songinbox::online_search_url(&tpl, &q) {
                    open_url(&url);
                }
                if let Err(e) = songinbox::arm(&app.core) {
                    app.core.lock().notify(e, "error");
                }
            }
        }
    });
    if !open {
        return;
    }
    let mut src: String = get(ui, SRC);
    if src.is_empty() {
        src = "ppt".into();
    }
    let mut q: String = get(ui, Q);
    let mut res: Vec<Value> = get(ui, RES);
    let mut refresh = false;
    ui.horizontal(|ui| {
        for (k, l) in [("ppt", "PPT 폴더"), ("beth", "베들레헴 찬송가")] {
            if ui.selectable_label(src == k, l).clicked() {
                src = k.into();
                refresh = true;
            }
        }
    });
    let folders = hymns::folders(&app.core.lock());
    ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 24.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if src == "ppt" && ui.button(wp_core::i18n::tr("폴더 추가…")).clicked() {
            if let Some(dir) = rfd::FileDialog::new().set_title("찬송가·교독문 PPT 폴더").pick_folder() {
                let mut c = app.core.lock();
                match hymns::add_folder(&mut c, &dir, None) {
                    Ok(v) => c.notify(wp_core::i18n::tr_args("'{1}' 폴더를 넣었습니다 ({2}개)", &[&(v["name"].as_str().unwrap_or("")), &(v["count"])]), "ok"),
                    Err(e) => c.notify(e, "error"),
                }
                refresh = true;
            }
        }
        if src == "beth" && ui.button(wp_core::i18n::tr("가져오기…")).on_hover_text(wp_core::i18n::tr("베들레헴 BethlehemWin 폴더의 찬송가 가사(.hdb)·악보(.cmp)를 데이터 폴더로 복사")).clicked() {
            if let Some(dir) = rfd::FileDialog::new().set_title("BethlehemWin 폴더").pick_folder() {
                let mut c = app.core.lock();
                let done = hymnal::import_from(&c, &dir);
                if done.is_empty() {
                    c.notify(wp_core::i18n::tr("그 폴더에 찬송가 파일(새찬송가.hdb 등)이 없습니다."), "warn");
                } else {
                    c.notify(wp_core::i18n::tr_args("찬송가 {1}권을 가져왔습니다.", &[&(done.len())]), "ok");
                }
            }
        }
        let r = ui.add(egui::TextEdit::singleline(&mut q).hint_text(if src == "ppt" { wp_core::i18n::tr_s("번호(10) 또는 제목") } else { wp_core::i18n::tr_s("번호(10장) · 교독문 1 · 제목·가사") }).desired_width(ui.available_width().max(40.0)));
        refresh |= r.changed() || (res.is_empty() && src == "ppt" && !folders.is_empty() && q.is_empty());
    });
    if refresh {
        let c = app.core.lock();
        res = if src == "ppt" {
            hymns::search(&c, &q).into_iter().take(80).map(|h| json!({ "file": h.file.to_string_lossy(), "number": h.number, "title": h.title, "folder": h.folder })).collect()
        } else {
            hymnal::search(&c, &q, 60)
        };
    }
    if src == "ppt" {
        if folders.is_empty() {
            ui.label(RichText::new(wp_core::i18n::tr("찬송가 PPT(.pps .ppt .pptx) 폴더를 넣으세요.")).small().color(Color32::GRAY));
        }
        for f in &folders {
            ui.horizontal(|ui| {
                let name = f["name"].as_str().unwrap_or("");
                let path = f["path"].as_str().unwrap_or("").to_string();
                let warn = !f["exists"].as_bool().unwrap_or(false);
                ui.label(RichText::new(wp_core::i18n::tr_args("{1} {2}개{3}", &[&name, &(f["count"]), &(if warn { wp_core::i18n::tr_s(" (폴더 없음)") } else { "" })])).small().color(if warn { Color32::from_rgb(0xff, 0x8a, 0x8a) } else { Color32::GRAY }));
                // 폴더별 읽는 방식
                let cur = f["render"].as_str().unwrap_or("auto").to_string();
                let label = |r: &str| match r {
                    "slides" => "PowerPoint 로 그대로",
                    "direct" => "PowerPoint 없이",
                    _ => "자동",
                };
                egui::ComboBox::from_id_salt(format!("render-{path}")).selected_text(RichText::new(label(&cur)).small()).width(130.0).show_ui(ui, |ui| {
                    for r in ["auto", "slides", "direct"] {
                        let tip = match r {
                            "slides" => "PowerPoint 로 슬라이드를 화면 그대로 그림으로 (PowerPoint 필요)",
                            "direct" => ".pptx 를 직접 읽어 그림으로 (.pps 는 악보·배경을 나눠 꺼냄)",
                            _ => ".pps 는 악보·배경을 나눠 꺼내고, .pptx 는 PowerPoint 가 있으면 그것으로",
                        };
                        if ui.selectable_label(cur == r, label(r)).on_hover_text(tip).clicked() {
                            hymns::set_folder_render(&mut app.core.lock(), &path, r);
                        }
                    }
                });
                if ui.small_button("×").on_hover_text(wp_core::i18n::tr("폴더 빼기 (파일은 그대로)")).clicked() {
                    hymns::remove_folder(&mut app.core.lock(), &path);
                    res.clear();
                }
            });
        }
        default_bg_row(app, ui);
    } else {
        let books = hymnal::books(&app.core.lock());
        if books.is_empty() {
            ui.label(RichText::new(wp_core::i18n::tr("베들레헴 성경의 찬송가 파일을 찾지 못했습니다. '가져오기…' 로 BethlehemWin 폴더를 고르세요.")).small().color(Color32::GRAY));
        } else {
            let t = books.iter().map(|b| format!("{} {}장{}", b["name"].as_str().unwrap_or(""), b["count"], if b["scores"].as_bool().unwrap_or(false) { " · 악보" } else { "" })).collect::<Vec<_>>().join("  ");
            ui.label(RichText::new(t).small().color(Color32::GRAY));
        }
    }
    egui::ScrollArea::vertical().id_salt("hymn-res").max_height(150.0).show(ui, |ui| {
        for h in &res {
            let t = if src == "ppt" {
                format!("{}{}  · {}", h["number"].as_i64().map(|n| format!("{n}  ")).unwrap_or_default(), h["title"].as_str().unwrap_or(""), h["folder"].as_str().unwrap_or(""))
            } else {
                format!("{}  · {}", h["label"].as_str().unwrap_or(""), h["book"].as_str().unwrap_or(""))
            };
            let tip = if app.core.lock().direct_mode() { wp_core::i18n::tr("눌러서 큐시트에 넣고 바로 송출 (이미 있으면 그 순서를 송출)") } else { wp_core::i18n::tr("눌러서 큐시트에 넣고 미리보기 (이미 있으면 그 순서를 미리보기)") };
            if ui.selectable_label(false, t).on_hover_text(tip).clicked() {
                if src == "ppt" {
                    // 제목은 "10장 전능왕 오셔서" (cue.js 1527) · 번호 없으면 null
                    let num = h["number"].as_i64();
                    acts.push(Act::AddItem("hymn", Some(json!({ "title": hymns::item_title(num, h["title"].as_str().unwrap_or("")), "file": h["file"], "number": num, "background": "default", "obsLyrics": "" }))));
                } else {
                    let r = hymnal::get(&app.core.lock(), h["book"].as_str().unwrap_or(""), h["number"].as_i64().unwrap_or(0));
                    match r {
                        Ok(v) if v["kind"] == "reading" => acts.push(Act::AddItem("text", Some(json!({ "title": v["title"], "body": v["body"] })))),
                        Ok(v) => acts.push(Act::AddItem("song", Some(json!({ "title": v["title"], "lyrics": v["lyrics"], "order": v["order"], "credit": v["credit"], "creditShow": "", "scoreUrl": v["score"] })))),
                        Err(e) => app.core.lock().notify(e, "error"),
                    }
                }
            }
        }
    });
    set(ui, Q, q);
    set(ui, RES, res);
    set(ui, SRC, src);
}

/// 찬송가 기본 배경 (settings.hymnBackground) — Electron 고르기 창의 '찬송가 폴더 · 기본 배경 설정' (cue.js 1478-1481 · 1556-1558)
/// 원래 배경 · 흰 배경 · 템플릿 배경 · 지정한 이미지(다른 이미지로…). 순서의 '설정대로' 가 이 값을 따른다.
fn default_bg_row(app: &mut App, ui: &mut egui::Ui) {
    let cur = hymns::default_bg(&app.core.lock());
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(wp_core::i18n::tr("기본 배경")).small().color(Color32::GRAY)).on_hover_text(wp_core::i18n::tr("악보는 투명 그림이라 어떤 배경 위에도 겹쳐 보입니다. 악보 글씨가 검은색이라 밝은 배경을 권합니다. 순서마다 따로 바꿀 수도 있습니다 (편집칸의 '배경')."));
        let custom = !matches!(cur.as_str(), "original" | "#ffffff" | "template");
        let name = |v: &str| -> String {
            match v {
                "original" => wp_core::i18n::tr("찬송가 원래 배경"),
                "#ffffff" => wp_core::i18n::tr("흰 배경 (악보가 가장 잘 보임)"),
                "template" => wp_core::i18n::tr("템플릿 배경 (테마·빠른 설정의 배경)"),
                _ => wp_core::i18n::tr("지정한 이미지"),
            }
        };
        egui::ComboBox::from_id_salt("hymn-default-bg").selected_text(RichText::new(name(&cur)).small()).width(200.0).show_ui(ui, |ui| {
            for v in ["original", "#ffffff", "template"] {
                if ui.selectable_label(cur == v, name(v)).clicked() {
                    hymns::set_default_bg(&mut app.core.lock(), v);
                }
            }
            if custom {
                ui.selectable_label(true, name(&cur)).on_hover_text(&cur);
            }
        });
        if ui.small_button(wp_core::i18n::tr("다른 이미지로…")).on_hover_text(wp_core::i18n::tr("그림 파일을 데이터 폴더로 복사해 기본 배경으로")).clicked() {
            if let Some(p) = rfd::FileDialog::new().add_filter("그림", &["png", "jpg", "jpeg", "webp", "gif"]).pick_file() {
                let mut c = app.core.lock();
                match hymns::set_default_bg_image(&mut c, &p) {
                    Ok(_) => c.notify(wp_core::i18n::tr("찬송가 기본 배경을 바꿨습니다."), "ok"),
                    Err(e) => c.notify(e, "error"),
                }
            }
        }
    });
}

fn open_url(url: &str) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).creation_flags(0x0800_0000).spawn();
    }
    #[cfg(not(windows))]
    let _ = url;
}

fn to_parts(v: &Value) -> Vec<pptsplit::Part> {
    v["parts"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|p| pptsplit::Part { a: p["a"].as_u64().unwrap_or(1) as usize, b: p["b"].as_u64().unwrap_or(1) as usize, name: p["name"].as_str().unwrap_or("").into(), kind: p["kind"].as_str().unwrap_or("title").into() })
        .collect()
}

/// 고른 PPT 순서의 도구: 요소 따로 꺼내기 · 자동으로 나누기 (규칙 · API · 채팅 AI)
fn ppt_tools(app: &mut App, ui: &mut egui::Ui, i: usize, it: &Value) {
    let file = it["file"].as_str().unwrap_or("").to_string();
    let title = it["title"].as_str().unwrap_or("").to_string();
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("PPT:").small().color(Color32::GRAY));
        if hymns::is_building(&file) {
            ui.label(RichText::new(wp_core::i18n::tr("그림 만드는 중…")).small().color(Color32::from_rgb(0xff, 0xb0, 0x4a)));
        }
        // cue.js pptTextRow: 아직 안 꺼냈으면 '요소 따로 꺼내기' 하나, 꺼냈으면 'N장을 요소로 · 다시 꺼내기 · 원래대로' (+ 배경 그림이 없으면 '배경 다시 만들기')
        // 다시 꺼내기·원래대로는 고친 PPT 글·그림이 사라지므로 Electron 처럼 묻고 진행한다 (confirm → 줄 아래 예/아니요)
        let on = hymns::ppt_text_count(it);
        let item_id = it["id"].as_str().unwrap_or("").to_string();
        let mut confirm: String = get(ui, CONFIRM); // "again:<id>" | "off:<id>" | ""
        if on == 0 {
            if ui.small_button(wp_core::i18n::tr("요소 따로 꺼내기")).on_hover_text(wp_core::i18n::tr("PPT 의 글상자·그림·도형을 하나씩 꺼내 슬라이드 편집기에서 고칠 수 있게")).clicked() {
                extract_layers(app, i, it);
            }
        } else {
            let missing = hymns::layer_bgs_missing(&app.core.lock(), &file);
            if missing {
                ui.label(RichText::new(wp_core::i18n::tr("요소를 숨긴 배경 그림이 없습니다 (다른 PC · 캐시 지움)")).small().color(Color32::from_rgb(0xff, 0x8a, 0x8a)));
                if ui.small_button(wp_core::i18n::tr("배경 다시 만들기")).on_hover_text(wp_core::i18n::tr("요소를 숨긴 PPT 배경 그림만 다시 만듭니다 (고친 것은 그대로)")).clicked() {
                    let mut c = app.core.lock();
                    match hymns::ppt_text_layers(&c, &file, (1920.0, 1080.0), true) {
                        Ok(_) => { c.preview_item(i); c.notify(wp_core::i18n::tr("요소를 숨긴 PPT 배경 그림을 다시 만들었습니다."), "ok"); }
                        Err(e) => { let k = if e == hymns::BUILDING { "info" } else { "error" }; c.notify(e, k); }
                    }
                }
            } else {
                ui.label(RichText::new(wp_core::i18n::tr_args("{1}장을 요소로", &[&on])).small().color(Color32::GRAY));
            }
            if ui.small_button(wp_core::i18n::tr("다시 꺼내기")).on_hover_text(wp_core::i18n::tr("PPT 에서 요소를 다시 꺼냄 (고친 PPT 글·그림은 원래대로)")).clicked() {
                confirm = format!("again:{item_id}");
            }
            if ui.small_button(wp_core::i18n::tr("원래대로")).on_hover_text(wp_core::i18n::tr("꺼낸 요소를 뽑고 원래 PPT 그림으로")).clicked() {
                confirm = format!("off:{item_id}");
            }
        }
        set(ui, CONFIRM, confirm);
        if ui.small_button(wp_core::i18n::tr("자동으로 나누기")).on_hover_text(wp_core::i18n::tr("한 PPT 를 순서 이름·찬송 장·성경 장절로 여러 순서로 나눕니다 (AI 없이)")).clicked() {
            let mut c = app.core.lock();
            match hymns::slide_texts(&c, &file).and_then(|t| pptsplit::rule_split(&t, &title)) {
                Ok(parts) if parts.len() > 1 => replace_with_parts(&mut c, i, it, &parts),
                Ok(_) => c.notify(wp_core::i18n::tr("나눌 곳을 찾지 못했습니다. AI 로 나누기를 써 보세요."), "warn"),
                Err(e) => c.notify(e, "error"),
            }
        }
        if ui.small_button(wp_core::i18n::tr("AI 로 나누기")).on_hover_text(wp_core::i18n::tr("주보 AI 와 같은 오픈라우터 키·모델")).clicked() {
            let prep = {
                let c = app.core.lock();
                let ai = c.settings.get()["ai"].clone();
                let key = c.ai_key(); // [트랙 D] DPAPI 로 암호화한 keyEnc 를 풀어 씀
                let inp = wp_core::pptsheets::inputs(&c, &file).unwrap_or_default(); // [트랙 A] 글자 없는 PPT 는 모아보기 그림을 같이
                hymns::slide_texts(&c, &file).map(|t| (t, ai, key, inp))
            };
            match prep {
                Ok((texts, ai, key, inp)) => {
                    let model = ai["model"].as_str().unwrap_or("").to_string();
                    let endpoint = ai["endpoint"].as_str().unwrap_or("").to_string();
                    let (it2, t2) = (it.clone(), title.clone());
                    app.core.lock().notify(wp_core::i18n::tr("AI 에게 나누기를 묻는 중…"), "info");
                    hymns::run_bg(
                        move || {
                            let blank = texts.iter().all(|t| t.trim().is_empty());
                            let sheets = if blank { wp_core::pptsheets::build(&inp).map(|s| wp_core::pptsheets::as_data_urls(&s)).unwrap_or_default() } else { vec![] };
                            pptsplit::ask_split(&key, &model, &endpoint, &texts, &t2, "", &sheets)
                        },
                        move |c, r| match r {
                            Ok(v) => {
                                let idx = c.cue.as_ref().and_then(|q| q.items.iter().position(|x| x["id"] == it2["id"]));
                                if let Some(i) = idx {
                                    replace_with_parts(c, i, &it2, &to_parts(&v));
                                }
                            }
                            Err(e) => c.notify(e, "error"),
                        },
                    );
                }
                Err(e) => app.core.lock().notify(e, "error"),
            }
        }
        if ui.small_button(wp_core::i18n::tr("모아보기 그림 저장")).on_hover_text(wp_core::i18n::tr("슬라이드 16장씩 번호를 붙인 그림 — 글자가 없는 PPT 를 채팅 AI 에 올릴 때")).clicked() {
            match wp_core::pptsheets::inputs(&app.core.lock(), &file) {
                Ok(inp) => {
                    app.core.lock().notify(wp_core::i18n::tr("모아보기 그림을 만드는 중…"), "info");
                    hymns::run_bg(move || wp_core::pptsheets::build(&inp), |c, r| match r.and_then(|s| wp_core::pptsheets::save(c, &s).map(|d| (d, s.len()))) {
                        Ok((dir, n)) => {
                            open_url(&dir.to_string_lossy());
                            c.notify(wp_core::i18n::tr_args("모아보기 그림 {1}장을 '{2}' 에 저장했습니다. 채팅 AI 에 프롬프트와 함께 올리세요.", &[&n, &(dir.display())]), "ok");
                        }
                        Err(e) => c.notify(e, "error"),
                    });
                }
                Err(e) => app.core.lock().notify(e, "error"),
            }
        }
        if ui.small_button(wp_core::i18n::tr("채팅 AI 프롬프트 복사")).on_hover_text(wp_core::i18n::tr("ChatGPT·Claude·Gemini 에 붙여 넣고, 답을 아래 칸에 붙여 넣으세요")).clicked() {
            let r = {
                let c = app.core.lock();
                // [트랙 A] 글자가 없는 PPT: 모아보기 그림 수를 프롬프트에 (그림은 '모아보기 저장' 으로)
                hymns::slide_texts(&c, &file).and_then(|t| { let blank = t.iter().all(|x| x.trim().is_empty()); let n = if blank { t.len().div_ceil(if t.len() > 160 { 25 } else { 16 }) } else { 0 }; pptsplit::chat_prompt(&t, &title, "", n) })
            };
            match r {
                Ok(p) => {
                    crate::widgets::copy_notice(ui.ctx(), p, wp_core::i18n::tr("프롬프트를 복사했습니다. 채팅 AI 에 붙여 넣고, 답을 'AI 답' 칸에 붙여 넣으세요."));
                }
                Err(e) => app.core.lock().notify(e, "error"),
            }
        }
    });
    // 확인 줄 (cue.js 의 confirm): 이 순서에 대한 것만
    let confirm: String = get(ui, CONFIRM);
    if let Some((kind, id)) = confirm.split_once(':').filter(|(_, id)| *id == it["id"].as_str().unwrap_or("")) {
        let q = if kind == "again" {
            "PPT 의 요소를 다시 꺼낼까요? 이 순서에서 고친 PPT 글·그림은 원래 PPT 대로 돌아갑니다 (덧붙인 글상자·그림은 그대로)."
        } else {
            "원래 PPT 그림으로 되돌릴까요? 꺼내서 고친 PPT 글·그림은 사라지고, 덧붙인 글상자·그림은 그대로 둡니다."
        };
        let (kind, _id) = (kind.to_string(), id.to_string());
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(wp_core::i18n::tr(q)).small().color(Color32::from_rgb(0xff, 0xb0, 0x4a)));
            if ui.small_button(wp_core::i18n::tr("예")).clicked() {
                set(ui, CONFIRM, String::new());
                if kind == "again" {
                    extract_layers(app, i, it);
                } else {
                    let mut c = app.core.lock();
                    let mut x = it.clone();
                    if hymns::ppt_text_off(&mut x) {
                        if let Some(cue) = &mut c.cue {
                            if i < cue.items.len() { cue.items[i] = x; }
                        }
                        c.save_cue();
                        c.preview_item(i);
                    }
                    c.notify(wp_core::i18n::tr("원래 PPT 그림으로 되돌렸습니다."), "ok");
                }
            }
            if ui.small_button(wp_core::i18n::tr("아니요")).clicked() {
                set(ui, CONFIRM, String::new());
            }
        });
    }
    let mut ans: String = get(ui, ANSWER);
    ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 24.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let go = ui.small_button(wp_core::i18n::tr("나누기")).clicked();
        ui.add(egui::TextEdit::singleline(&mut ans).hint_text(wp_core::i18n::tr("채팅 AI 답 (JSON) 붙여 넣기")).desired_width(ui.available_width().max(40.0)));
        if go && !ans.trim().is_empty() {
            let mut c = app.core.lock();
            match hymns::slide_texts(&c, &file).and_then(|t| pptsplit::parse_answer(&ans, t.len(), &title)) {
                Ok(parts) => {
                    replace_with_parts(&mut c, i, it, &parts);
                    ans.clear();
                }
                Err(e) => c.notify(e, "error"),
            }
        }
    });
    set(ui, ANSWER, ans);
}

/// PPT 의 글상자·그림·도형을 꺼내 이 순서의 edits 에 (cue.js pptTextOn mode 'on'). 캐시가 없으면 백그라운드로 만들고 '만드는 중' 알림.
fn extract_layers(app: &mut App, i: usize, it: &Value) {
    let file = it["file"].as_str().unwrap_or("").to_string();
    let mut c = app.core.lock();
    let n = hymns::load(&c, &file).map(|m| m["slides"].as_array().map(|a| a.len()).unwrap_or(0)).unwrap_or(0);
    match hymns::ppt_text_layers(&c, &file, (1920.0, 1080.0), false) {
        Ok(l) => {
            let mut x = it.clone();
            let (sl, t, p, s) = hymns::apply_layers(&mut x, &l, n);
            if let Some(cue) = &mut c.cue {
                if i < cue.items.len() {
                    cue.items[i] = x;
                }
            }
            c.save_cue();
            c.preview_item(i);
            if t + p + s > 0 {
                c.notify(wp_core::i18n::tr_args("슬라이드 {1}장에서 글상자 {2}개 · 그림 {3}개 · 도형 {4}개를 꺼냈습니다.", &[&sl, &t, &p, &s]), "ok");
            } else {
                c.notify(wp_core::i18n::tr("따로 꺼낼 요소를 찾지 못했습니다 (글이 그림으로 되어 있거나 돌린 요소)."), "warn");
            }
        }
        Err(e) => {
            let k = if e == hymns::BUILDING { "info" } else { "error" };
            c.notify(e, k);
        }
    }
}

fn replace_with_parts(c: &mut wp_core::core::Core, i: usize, it: &Value, parts: &[pptsplit::Part]) {
    let items = hymns::split_items(it, parts);
    let n = items.len();
    if let Some(cue) = &mut c.cue {
        if i < cue.items.len() {
            cue.items.splice(i..=i, items);
        }
    }
    c.save_cue();
    c.notify(wp_core::i18n::tr_args("PPT 를 순서 {1}개로 나눴습니다.", &[&n]), "ok");
}

/// 클립보드의 PowerPoint 슬라이드 · 그림 → 새 순서 (백그라운드)
fn grab_clipboard(app: &mut App) {
    let dd = app.core.lock().data_dir.clone();
    app.core.lock().notify(wp_core::i18n::tr("클립보드를 읽는 중…"), "info");
    hymns::run_bg(
        move || {
            let (slides, image, _) = clippaste::clipboard_info();
            if slides {
                clippaste::grab_slides(&dd, false).map(|(f, _)| ("slides", f.to_string_lossy().to_string()))
            } else if image {
                clippaste::grab_image(&dd).map(|u| ("image", u))
            } else {
                Err(wp_core::i18n::tr("클립보드에 PowerPoint 슬라이드나 그림이 없습니다. (글은 Ctrl+V)").to_string())
            }
        },
        |c, r| match r {
            Ok(("slides", f)) => {
                let h = hymns::HymnFile { number: None, title: "붙여 넣은 슬라이드".into(), name: String::new(), folder: String::new(), file: std::path::PathBuf::from(&f) };
                c.add_item(hymns::new_hymn_item(&h), None);
                c.notify(wp_core::i18n::tr("PowerPoint 슬라이드를 새 순서로 넣었습니다."), "ok");
            }
            Ok((_, url)) => {
                let mut it = wp_core::core::new_item("image", "붙여 넣은 그림");
                it["image"] = json!(url);
                c.add_item(it, None);
                c.notify(wp_core::i18n::tr("그림을 새 순서로 넣었습니다."), "ok");
            }
            Err(e) => c.notify(e, "warn"),
        },
    );
}

/// Ctrl+V (글 입력칸 밖): 글 → 성경/찬양/공지 · 글이 없으면 슬라이드·그림
fn paste(app: &mut App, ui: &egui::Ui, acts: &mut Vec<Act>) {
    let ctx = ui.ctx().clone();
    if ctx.memory(|m| m.focused().is_some()) || ctx.input(|i| i.modifiers.shift) {
        return; // Ctrl+Shift+V 는 '종류 골라 붙여 넣기' 창 (parity2_ui)
    }
    let text = ctx.input(|i| i.events.iter().find_map(|e| if let egui::Event::Paste(t) = e { Some(t.clone()) } else { None }));
    let ctrl_v = ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Key { key: egui::Key::V, pressed: true, modifiers, .. } if modifiers.command)));
    match text.filter(|t| !t.trim().is_empty()) {
        Some(t) => {
            let plan = {
                let mut c = app.core.lock();
                let v = c.version_id();
                let store = &mut c.store;
                let cell = std::cell::RefCell::new(store);
                let is_ref = |l: &str| cell.borrow_mut().resolve_reference(&v, l)["ok"] == json!(true);
                clippaste::plan_text(&t, "", &is_ref)
            };
            if let Some((_, items)) = plan {
                for x in items {
                    let ty: &'static str = match x["type"].as_str() {
                        Some("bible") => "bible",
                        Some("song") => "song",
                        Some("title") => "title",
                        _ => "text",
                    };
                    acts.push(Act::AddItem(ty, Some(x["extra"].clone())));
                }
            }
        }
        None if ctrl_v => grab_clipboard(app),
        None => {}
    }
}

/// 찬송가 순서 편집칸 (파일 · 번호 · 배경 · 범위 · OBS 가사) → 바뀌었으면 true
pub fn editor(ui: &mut egui::Ui, e: &mut Value, core: &wp_core::core::Shared) -> bool {
    let mut changed = false;
    let file = e["file"].as_str().unwrap_or("").to_string();
    ui.label(RichText::new(wp_core::i18n::tr("PPT 파일")).small());
    ui.horizontal(|ui| {
        let short = file.rsplit(['\\', '/']).next().unwrap_or("");
        ui.label(RichText::new(if short.is_empty() { wp_core::i18n::tr_s("(없음)") } else { short }).small()).on_hover_text(&file);
        if ui.button(wp_core::i18n::tr("고르기…")).clicked() {
            if let Some(p) = rfd::FileDialog::new().add_filter("PPT", &["pptx", "ppsx", "ppt", "pps"]).pick_file() {
                let (num, title, _) = hymns::parse_name(&p);
                // 등록 폴더 밖의 파일도 열리게 — Electron 은 폴더 밖 파일을 끌어다 놓기(addDroppedFiles → settings.droppedFiles)로만 받고
                // hymns.js isAllowed 가 그것을 허용한다. 고르기 창으로 고른 파일도 같은 목록에 넣어 is_allowed 가 거부하지 않게 한다.
                core.lock().add_dropped_files(std::slice::from_ref(&p));
                e["file"] = json!(p.to_string_lossy());
                e["number"] = json!(num);
                if e["title"].as_str().is_none_or(|t| t.is_empty() || t == "찬송") {
                    e["title"] = json!(hymns::item_title(num, &title));
                }
                changed = true;
            }
        }
    });
    ui.label(RichText::new(wp_core::i18n::tr("등록한 찬송가 폴더·붙여 넣은 슬라이드 폴더 안의 파일과 여기서 고른 파일이 열립니다")).small().color(Color32::GRAY));
    ui.label(RichText::new(wp_core::i18n::tr("배경 (악보 그림일 때)")).small());
    let mut bg = e["background"].as_str().unwrap_or("default").to_string();
    // '설정대로' 에는 지금 기본 배경 이름을 같이 (cue.js hymnBgSel: "기본값 (원래 배경)")
    let def = wp_core::i18n::tr_args("설정대로 ({1})", &[&wp_core::i18n::tr(hymns::default_bg_label(&hymns::default_bg(&core.lock())))]);
    ui.horizontal_wrapped(|ui| {
        for (v, l) in [("default", def.as_str()), ("original", "파일 배경"), ("template", "템플릿 배경"), ("#ffffff", "흰색"), ("#000000", "검정")] {
            if ui.radio_value(&mut bg, v.to_string(), l).changed() {
                e["background"] = json!(bg.clone());
                changed = true;
            }
        }
    });
    ui.label(RichText::new(wp_core::i18n::tr("슬라이드 범위 (예: 5-9, 비우면 전부)")).small());
    let mut r = e["range"].as_str().unwrap_or("").to_string();
    if ui.add(egui::TextEdit::singleline(&mut r).desired_width(f32::INFINITY)).changed() {
        e["range"] = json!(r);
        changed = true;
    }
    ui.label(RichText::new(wp_core::i18n::tr("OBS 자막 가사 (비우면 PPT 안의 글자 · 새찬송가 가사. 빈 줄로 슬라이드마다)")).small());
    let mut o = e["obsLyrics"].as_str().unwrap_or("").to_string();
    if ui.add(egui::TextEdit::multiline(&mut o).desired_rows(4).desired_width(f32::INFINITY)).changed() {
        e["obsLyrics"] = json!(o);
        changed = true;
    }
    changed
}

#[cfg(test)]
mod cue_ppt_menu_tests {
    use super::*;

    #[test]
    fn only_file_backed_hymns_enable_split_entry() {
        assert!(can_open_ppt_tools(&json!({"type":"hymn", "file":"C:/test.pptx"})));
        for item in [json!({"type":"hymn", "hymnal":true}), json!({"type":"hymn","file":" "}), json!({"type":"song","file":"C:/test.pptx"}), json!({"type":"audio"})] {
            assert!(!can_open_ppt_tools(&item));
        }
    }

    #[test]
    fn split_entry_only_requests_existing_tools_for_target_item() {
        let ctx = egui::Context::default();
        open_ppt_tools(&ctx, &json!({"id":"ppt-target"}));
        assert_eq!(ctx.data(|d| d.get_temp::<String>(id("ppt-tools-request"))), Some("ppt-target".into()));
    }
}
