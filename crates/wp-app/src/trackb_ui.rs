//! [트랙 B] 조작 화면: 찬양 · 라이브러리 · 오버레이·회중 메시지 · 즉석 자막 · 무대 화면 · 매크로 · 큐시트 관리
//! 도구 줄(위)에서 창을 열고 닫는다. 단축키: Ctrl+M 메시지 · Ctrl+T 즉석 자막 · Ctrl+P 인쇄 · Ctrl+Z/Y 실행 취소 · Alt+1~9 매크로

use crate::app::App;
use eframe::egui::{self, Color32, Key, KeyboardShortcut, Modifiers, RichText};
use serde_json::{json, Value};
use std::process::Command;
use wp_core::presentation::Deck;

#[derive(Default)]
pub struct TbState {
    pub songs: bool,
    pub lib: bool,
    pub overlays: bool,
    pub message: bool,
    pub quick: bool,
    pub stage: bool,
    pub macros: bool,
    pub cues: bool,
    song_q: String,
    song_edit: Option<Value>,
    import_text: String,
    lib_kind: String,
    lib_q: String,
    msg_text: String,
    msg_tpl: Option<Value>,
    msg_fields: Vec<(String, String)>,
    msg_sec: i64,
    msg_obs: bool,
    quick_text: String,
    timer_min: String,
    stage_msg: String,
    pin_text: String,
    macro_name: String,
    macro_msg: String,
    stage_child: Option<std::process::Child>,
    pub note: String,
}

fn err(r: anyhow::Result<impl Sized>) -> String {
    match r {
        Ok(_) => String::new(),
        Err(e) => e.to_string(),
    }
}

impl App {
    pub(crate) fn trackb_shortcuts(&mut self, ctx: &egui::Context) {
        let sc = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(m, k)));
        if sc(Modifiers::CTRL, Key::M) { self.tb.message = true; }
        if sc(Modifiers::CTRL, Key::T) { self.tb.quick = true; }
        if sc(Modifiers::CTRL, Key::L) { crate::lyrics_ui::open(); }
        if sc(Modifiers::CTRL, Key::P) { self.print_cue(); }
        if !ctx.egui_wants_keyboard_input() {
            if sc(Modifiers::CTRL, Key::Z) { self.core.lock().undo_items(false); self.edit = None; }
            if sc(Modifiers::CTRL, Key::Y) || sc(Modifiers::CTRL | Modifiers::SHIFT, Key::Z) { self.core.lock().undo_items(true); self.edit = None; }
        }
        let keys = [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8, Key::Num9];
        for (i, k) in keys.iter().enumerate() {
            if sc(Modifiers::ALT, *k) {
                let id = self.core.lock().settings.get()["macros"][i]["id"].as_str().map(String::from);
                if let Some(id) = id {
                    let mut c = self.core.lock();
                    if let Err(e) = c.run_macro(&id) { c.notify(e.to_string(), "warn"); }
                }
            }
        }
        // 예약 실행 (분이 바뀔 때만 확인)
        let now = chrono_now();
        let mut c = self.core.lock();
        c.check_schedule(now.0, &now.1, &now.2);
    }

    pub(crate) fn print_cue(&mut self) { // [트랙 C] 메뉴에서 부름
        let r = self.core.lock().print_cue_file();
        match r {
            Ok(p) => { let _ = Command::new("explorer").arg(p).spawn(); }
            Err(e) => self.tb.note = e.to_string(),
        }
    }

    /// 위 도구 줄
    pub(crate) fn trackb_toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let t = &mut self.tb;
            ui.toggle_value(&mut t.songs, wp_core::i18n::tr("찬양"));
            ui.toggle_value(&mut t.lib, wp_core::i18n::tr("라이브러리"));
            ui.toggle_value(&mut t.overlays, wp_core::i18n::tr("오버레이"));
            ui.toggle_value(&mut t.message, wp_core::i18n::tr("회중 메시지 Ctrl+M"));
            ui.toggle_value(&mut t.quick, wp_core::i18n::tr("즉석 자막 Ctrl+T"));
            ui.toggle_value(&mut t.stage, wp_core::i18n::tr("무대 화면"));
            ui.toggle_value(&mut t.macros, wp_core::i18n::tr("매크로"));
            ui.toggle_value(&mut t.cues, wp_core::i18n::tr("큐시트 관리"));
            if ui.button(wp_core::i18n::tr("인쇄 Ctrl+P")).clicked() { self.print_cue(); }
            if ui.button("↶").on_hover_text(wp_core::i18n::tr("실행 취소 Ctrl+Z")).clicked() { self.core.lock().undo_items(false); self.edit = None; }
            if ui.button("↷").on_hover_text(wp_core::i18n::tr("다시 Ctrl+Y")).clicked() { self.core.lock().undo_items(true); self.edit = None; }
            if !self.tb.note.is_empty() { ui.label(RichText::new(&self.tb.note).small().color(Color32::from_rgb(0xff, 0xb0, 0x4a))); }
        });
    }

    pub(crate) fn trackb_windows(&mut self, ctx: &egui::Context) {
        // 점검용: --open songs,lib,overlays,message,quick,stage,macros,cues
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            let a: Vec<String> = std::env::args().collect();
            if let Some(v) = a.iter().position(|x| x == "--open").and_then(|i| a.get(i + 1)) {
                for w in v.split(',') {
                    match w { "songs" => self.tb.songs = true, "lib" => self.tb.lib = true, "overlays" => self.tb.overlays = true, "message" => self.tb.message = true, "quick" => self.tb.quick = true, "stage" => self.tb.stage = true, "macros" => self.tb.macros = true, "cues" => self.tb.cues = true, "lyrics" => crate::lyrics_ui::open(), _ => {} }
                }
            }
        });
        self.songs_window(ctx);
        self.lib_window(ctx);
        self.overlays_window(ctx);
        self.message_window(ctx);
        self.quick_window(ctx);
        self.stage_window(ctx);
        self.macros_window(ctx);
        self.cues_window(ctx);
        self.lyrics_window(ctx);
        self.music_tick(ctx);
        self.system_windows(ctx);
    }

    fn add_to_cue(&mut self, it: Value) {
        let at = self.sel.map(|s| s + 1);
        let mut c = self.core.lock();
        c.add_item(it, at);
        let n = c.cue.as_ref().map(|x| x.items.len()).unwrap_or(0);
        self.sel = Some(at.unwrap_or(n.saturating_sub(1)).min(n.saturating_sub(1)));
        self.edit = None;
    }

    // ---------------- 찬양 ----------------
    fn songs_window(&mut self, ctx: &egui::Context) {
        let mut open = self.tb.songs;
        egui::Window::new(wp_core::i18n::tr("찬양 라이브러리")).open(&mut open).default_size([520.0, 560.0]).show(ctx, |ui|  self.songs_panel(ui));
        self.tb.songs = open;
    }

    // ---------------- 라이브러리 ----------------
    fn lib_window(&mut self, ctx: &egui::Context) {
        let mut open = self.tb.lib;
        if self.tb.lib_kind.is_empty() { self.tb.lib_kind = "image".into(); }
        egui::Window::new(wp_core::i18n::tr("라이브러리 (그림·영상·음악)")).open(&mut open).default_size([520.0, 520.0]).show(ctx, |ui|  self.library_panel(ui));
        self.tb.lib = open;
    }

    // ---------------- 왼쪽 탭에도 쓰는 패널 ----------------
    pub(crate) fn songs_panel(&mut self, ui: &mut egui::Ui) {
        // Electron library.js drawSongs: 검색칸 · + 새 찬양 · 가사 찾기 · 가져오기 ▾ / 목록 줄(🎵 · 굵은 제목 · 흐린 첫 줄 · 고치기)
        use crate::theme::{pal, rich, Weight};
        use crate::widgets::Btn;
        use wp_core::i18n::tr;
        let p = pal();
        let bar_off = ui.data(|d| d.get_temp::<bool>(egui::Id::new("lib-bar-off"))).unwrap_or(false);
        let paste_id = egui::Id::new("song-paste-open");
        let mut paste_open = ui.data(|d| d.get_temp::<bool>(paste_id)).unwrap_or(false);
        if !bar_off {
            // .lib-bar: panel2 바탕 · 여백 7px 8px · 아래 line 선 · 높이 30 칸
            let bar = egui::Frame::new().fill(p.panel2).inner_margin(egui::Margin::symmetric(8, 7)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    let bw = 98.0 + 110.0 + 112.0 + 18.0;
                    let w = (ui.available_width() - bw).max(140.0);
                    ui.add_sized([w, 30.0], egui::TextEdit::singleline(&mut self.tb.song_q).hint_text(tr("찬양 제목·가사로 찾기")).vertical_align(egui::Align::Center).margin(egui::Margin::symmetric(9, 4)));
                    if ui.add(Btn::new(tr("새 찬양")).icon("plus").height(30.0).tooltip(tr("새 찬양 가사 입력"))).clicked() {
                        self.tb.song_edit = Some(json!({ "title": "", "lyrics": "", "credit": "", "tags": "", "arrangement": "" }));
                    }
                    if ui.add(Btn::new(tr("가사 찾기")).icon("globe").height(30.0).tooltip(tr("벅스 · 지니 · 멜론에서 가사를 찾아 찬양 슬라이드로 (한 곡씩, Ctrl+L)"))).clicked() {
                        crate::lyrics_ui::open();
                    }
                    let imp = ui.add(Btn::new(tr("가져오기")).icon("download").caret().height(30.0).tooltip(tr("파일 · 폴더 통째로 · 여러 곡 붙여넣기")));
                    egui::Popup::menu(&imp).show(|ui| {
                        ui.set_min_width(260.0);
                        if ui.button(tr("파일 고르기… (OpenLP · ChordPro · ProPresenter 6 · PPT · txt)")).clicked() {
                            ui.close();
                            if let Some(fs) = rfd::FileDialog::new().add_filter(tr("찬양 파일"), &wp_core::songimport::SONG_EXT).pick_files() {
                                self.import_song_paths(&fs);
                            }
                        }
                        if ui.button(tr("폴더 통째로… (하위 폴더까지)")).clicked() {
                            ui.close();
                            if let Some(d) = rfd::FileDialog::new().pick_folder() {
                                let mut fs = vec![];
                                collect_song_files(&d, &mut fs, 0);
                                if fs.is_empty() { self.tb.note = tr("그 폴더에 찬양 파일이 없습니다."); } else { self.import_song_paths(&fs); }
                            }
                        }
                        if ui.button(tr("여러 곡 붙여넣기…")).clicked() {
                            paste_open = true;
                            ui.close();
                        }
                    });
                });
            });
            ui.painter().hline(bar.response.rect.x_range(), bar.response.rect.bottom() - 0.5, egui::Stroke::new(1.0, p.line));
        }
        if paste_open {
            egui::Frame::new().inner_margin(egui::Margin::symmetric(10, 6)).show(ui, |ui| {
                ui.label(rich(tr("가사 붙여 넣어 가져오기 (여러 곡은 '# 제목' 줄로 나눔)"), 12.0, Weight::Regular, p.muted));
                ui.add(egui::TextEdit::multiline(&mut self.tb.import_text).desired_rows(5).desired_width(f32::INFINITY));
                ui.horizontal(|ui| {
                    if ui.add(Btn::new(tr("가져오기")).kind(crate::widgets::Kind::Primary).height(28.0)).clicked() {
                        let r = self.core.lock().import_song_text(&self.tb.import_text, 2);
                        self.tb.note = match r { Ok(v) => format!("{}곡 저장 · 같은 제목 {}곡 건너뜀", v["saved"], v["skipped"]), Err(e) => e.to_string() };
                        paste_open = false;
                    }
                    if ui.add(Btn::new(tr("닫기")).height(28.0)).clicked() { paste_open = false; }
                });
            });
        }
        ui.data_mut(|d| d.insert_temp(paste_id, paste_open));
        // .lib-body: 목록 줄
        let list = self.core.lock().list_songs(&self.tb.song_q);
        let sel_id = egui::Id::new("song-sel");
        let mut sel: Option<String> = ui.data(|d| d.get_temp(sel_id)).flatten();
        ui.add_space(4.0);
        if list.is_empty() {
            egui::Frame::new().inner_margin(egui::Margin::symmetric(12, 12)).show(ui, |ui| {
                ui.label(rich(if self.tb.song_q.trim().is_empty() { tr("저장한 찬양이 없습니다. ＋ 새 찬양 으로 가사를 입력하거나, 큐시트의 찬양 순서에서 '라이브러리에 저장'을 누르세요.") } else { tr("찾는 찬양이 없습니다.") }, 13.0, Weight::Regular, p.muted));
            });
        }
        for s in &list {
            let id = s["id"].as_str().unwrap_or("").to_string();
            let (row, edit) = song_row(ui, s["title"].as_str().unwrap_or(""), s["first"].as_str().unwrap_or(""), sel.as_deref() == Some(id.as_str()));
            if edit.clicked() {
                self.tb.song_edit = self.core.lock().get_song(&id);
            } else if row.double_clicked() {
                let it = self.core.lock().song_item(&id);
                if let Some(it) = it { self.add_to_cue(it); }
            } else if row.clicked() {
                sel = Some(id.clone());
            }
            row.on_hover_text(tr("두 번 누르면 큐시트에 넣습니다"));
        }
        ui.data_mut(|d| d.insert_temp(sel_id, sel));
        if !self.tb.note.is_empty() {
            ui.add_space(4.0);
            ui.label(rich(self.tb.note.clone(), 12.0, Weight::Regular, p.muted));
        }
        if let Some(mut e) = self.tb.song_edit.take() {
            let mut close = false;
            let mut saved: Option<Value> = None;
            ui.add_space(6.0);
            crate::widgets::card_frame().inner_margin(egui::Margin::same(10)).show(ui, |ui| {
                ui.label(rich(if e["id"].as_str().is_some() { tr("찬양 고치기") } else { tr("새 찬양") }, 14.0, Weight::SemiBold, p.text));
                let e = &mut e;
                let mut s = |k: &str, ui: &mut egui::Ui, multi: bool| {
                    let mut v = e[k].as_str().unwrap_or("").to_string();
                    let r = if multi { ui.add(egui::TextEdit::multiline(&mut v).desired_rows(8).desired_width(f32::INFINITY)) } else { ui.add(egui::TextEdit::singleline(&mut v).desired_width(f32::INFINITY)) };
                    if r.changed() { e[k] = json!(v); }
                };
                ui.label(tr("제목")); s("title", ui, false);
                ui.label(tr("가사 (빈 줄로 슬라이드, [1절] [후렴] 이름표)")); s("lyrics", ui, true);
                ui.label(tr("곡 정보 (작사·작곡·저작권)")); s("credit", ui, false);
                ui.label(tr("기본 부를 순서 (예: 1 C 2 C)")); s("arrangement", ui, false);
                crate::media_ui::song_arrangements(ui, &mut self.core.lock(), e);
                let e2 = e.clone();
                ui.horizontal(|ui| {
                    if ui.add(Btn::new(tr("저장")).kind(crate::widgets::Kind::Primary)).clicked() {
                        let r = self.core.lock().save_song(&e2);
                        match r { Ok(v) => { saved = Some(v); self.tb.note = tr("찬양을 저장했습니다."); } Err(x) => self.tb.note = x.to_string() }
                    }
                    if e2["id"].as_str().is_some() && ui.add(Btn::new(tr("지우기")).kind(crate::widgets::Kind::Danger)).clicked() {
                        if let Some(id) = e2["id"].as_str() { self.tb.note = err(self.core.lock().delete_song(id)); close = true; }
                    }
                    if ui.add(Btn::new(tr("닫기"))).clicked() { close = true; }
                });
            });
            self.tb.song_edit = if close { None } else { Some(saved.unwrap_or(e)) };
        }
    }

    /// 찬양 파일 여러 개 가져오기 (결과는 tb.note)
    fn import_song_paths(&mut self, fs: &[std::path::PathBuf]) {
        let r = self.core.lock().import_song_files(fs);
        self.tb.note = match r {
            Ok(v) => {
                let errs: Vec<String> = v["errors"].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(String::from)).collect();
                format!("{}곡 저장 · 같은 제목 {}곡 건너뜀{}", v["saved"], v["skipped"], if errs.is_empty() { String::new() } else { format!(" · 못 읽음: {}", errs.join(" / ")) })
            }
            Err(e) => e.to_string(),
        };
    }

    pub(crate) fn library_panel(&mut self, ui: &mut egui::Ui) {

        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.tb.lib_kind, "image".to_string(), wp_core::i18n::tr("그림·영상"));
            ui.selectable_value(&mut self.tb.lib_kind, "audio".to_string(), wp_core::i18n::tr("음악"));
            if ui.button(wp_core::i18n::tr("폴더 더하기…")).clicked() {
                if let Some(d) = rfd::FileDialog::new().pick_folder() {
                    let k = self.tb.lib_kind.clone();
                    self.tb.note = err(self.core.lock().add_lib_folder(&k, &d));
                }
            }
            if ui.button(wp_core::i18n::tr("파일 넣기…")).on_hover_text(wp_core::i18n::tr("폴더 밖 파일도 (끌어다 놓은 파일처럼 기억)")).clicked() {
                if let Some(fs) = rfd::FileDialog::new().pick_files() {
                    let items = self.core.lock().add_dropped_files(&fs);
                    for it in items { let mut x = wp_core::core::new_item(it["type"].as_str().unwrap_or("image"), ""); wp_core::settings::merge_into(&mut x, &it); self.add_to_cue(x); }
                }
            }
        });
        let kind = self.tb.lib_kind.clone();
        let folders = wp_core::library::folders(self.core.lock().settings.get(), &kind);
        if folders.is_empty() {
            ui.label(RichText::new(wp_core::i18n::tr_args("등록한 {1} 폴더가 없습니다. '폴더 더하기…' 로 교회 {2} 폴더를 등록하세요.", &[&(if kind == "audio" { wp_core::i18n::tr_s("음악") } else { wp_core::i18n::tr_s("그림·영상") }), &(if kind == "audio" { wp_core::i18n::tr_s("음악") } else { wp_core::i18n::tr_s("사진") })])).color(Color32::GRAY));
        }
        for f in &folders {
            ui.horizontal(|ui| {
                ui.label(wp_core::i18n::tr_args("📁 {1} ({2}개){3}", &[&(f["name"].as_str().unwrap_or("")), &(f["count"]), &(if f["exists"] == false { wp_core::i18n::tr_s(" · 폴더 없음") } else { "" })]));
                if ui.small_button(wp_core::i18n::tr("빼기")).clicked() { self.core.lock().remove_lib_folder(&kind, f["path"].as_str().unwrap_or("")); }
            });
        }
        ui.add(egui::TextEdit::singleline(&mut self.tb.lib_q).hint_text(wp_core::i18n::tr("파일 이름으로 찾기")));
        let files = self.core.lock().search_lib(&kind, &self.tb.lib_q, "");
        if kind == "image" {
            self.lib_grid(ui, &files); // 썸네일 격자
        } else {
            egui::ScrollArea::vertical().id_salt("libfiles").show(ui, |ui| {                 for f in files.iter().take(300) {                     ui.horizontal(|ui| {                         ui.label(format!("{}{}", if f["video"] == true { "🎞 " } else { "" }, f["name"].as_str().unwrap_or("")));                         let url = f["url"].as_str().unwrap_or("").to_string();                         let name = f["name"].as_str().unwrap_or("").to_string();                         if kind == "image" {                             if ui.small_button(wp_core::i18n::tr("순서로")).clicked() {                                 let it = if f["video"] == true { json!({ "type": "video", "title": name, "file": url, "sound": true, "volume": 1.0, "loop": false, "endAction": "hold" }) } else { json!({ "type": "image", "title": name, "image": url }) };                                 let mut x = wp_core::core::new_item(it["type"].as_str().unwrap(), &name); wp_core::settings::merge_into(&mut x, &it);                                 self.add_to_cue(x);                             }                             if ui.small_button(wp_core::i18n::tr("배경으로")).clicked() {                                 let ty = if f["video"] == true { "video" } else { "image" };                                 self.core.lock().set_background_media(Some(json!({ "type": ty, "url": url, "label": name, "fit": "cover" })));                             }                         } else if ui.small_button("▶").on_hover_text(wp_core::i18n::tr("바로 재생 (조작 PC)")).clicked() {                             crate::music_ui::play_now(&url, &name);                         } else if ui.small_button(wp_core::i18n::tr("고른 순서 배경 음악")).clicked() {                             if let Some(i) = self.sel {                                 let mut c = self.core.lock();                                 c.remember_items();                                 if let Some(cue) = &mut c.cue { if let Some(it) = cue.items.get_mut(i) { it["music"] = json!(url); } }                                 c.save_cue();                                 self.edit = None;                             }                         }                     });                 }             });
        }
        if ui.button(wp_core::i18n::tr("배경 미디어 끄기")).clicked() { self.core.lock().set_background_media(None); }
    }

    // ---------------- 오버레이 ----------------
    fn overlays_window(&mut self, ctx: &egui::Context) {
        let mut open = self.tb.overlays;
        egui::Window::new(wp_core::i18n::tr("오버레이")).open(&mut open).default_size([460.0, 420.0]).show(ctx, |ui| {
            self.overlays_panel(ui);
        });
        self.tb.overlays = open;
    }

    pub(crate) fn overlays_panel(&mut self, ui: &mut egui::Ui) {
            ui.horizontal_wrapped(|ui| {
                for (ty, label) in [("clock", "시계"), ("countdown", "카운트다운"), ("banner", "배너"), ("text", "글자"), ("logo", "로고")] {
                    if ui.button(format!("+ {label}")).clicked() {
                        let base = match ty { "countdown" => json!({ "type": ty, "target": "5", "doneText": "" }), "banner" => json!({ "type": ty, "text": "환영합니다", "bg": "rgba(0,0,0,0.7)", "full": true }), "text" => json!({ "type": ty, "text": "글자" }), "logo" => json!({ "type": ty, "image": "/shared/assets/sa-shield.png" }), _ => json!({ "type": ty }) };
                        self.tb.note = err(self.core.lock().save_overlay(&base));
                    }
                }
                if ui.button(wp_core::i18n::tr("모두 끄기·다시 켜기")).clicked() { self.core.lock().toggle_overlays_off(); }
            });
            let list = self.core.lock().overlays();
            egui::ScrollArea::vertical().id_salt("ovl").show(ui, |ui| {
                for o in list {
                    let id = o["id"].as_str().unwrap_or("").to_string();
                    let mut v = o.clone();
                    let mut changed = false;
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            let mut on = o["on"].as_bool().unwrap_or(false);
                            if ui.checkbox(&mut on, o["name"].as_str().unwrap_or("")).changed() { let _ = self.core.lock().toggle_overlay(&id, Some(on)); }
                            ui.label(RichText::new(ov_type_ko(o["type"].as_str().unwrap_or(""))).small().color(Color32::GRAY));
                            for (out, lb) in [("main", "예배당"), ("overlay", "OBS")] {
                                let mut has = o["outputs"].as_array().is_some_and(|a| a.iter().any(|x| x == out));
                                if ui.checkbox(&mut has, lb).changed() {
                                    let mut outs: Vec<Value> = o["outputs"].as_array().cloned().unwrap_or_default();
                                    outs.retain(|x| x != out);
                                    if has { outs.push(json!(out)); }
                                    v["outputs"] = json!(outs);
                                    changed = true;
                                }
                            }
                            if ui.small_button(wp_core::i18n::tr("지우기")).clicked() { self.core.lock().delete_overlay(&id); }
                        });
                        ui.horizontal(|ui| {
                            for k in ["text", "target"] {
                                if (k == "text" && ["banner", "text"].contains(&o["type"].as_str().unwrap_or(""))) || (k == "target" && o["type"] == "countdown") {
                                    let mut s = v[k].as_str().unwrap_or("").to_string();
                                    if ui.add(egui::TextEdit::singleline(&mut s).desired_width(200.0).hint_text(if k == "target" { wp_core::i18n::tr_s("HH:MM 또는 분") } else { wp_core::i18n::tr_s("글") })).changed() { v[k] = json!(s); changed = true; }
                                }
                            }
                            egui::ComboBox::from_id_salt(format!("pos{id}")).selected_text(pos_ko(v["position"].as_str().unwrap_or(""))).show_ui(ui, |ui| {
                                for p in wp_core::overlays::POSITIONS { if ui.selectable_label(v["position"] == p, pos_ko(p)).clicked() { v["position"] = json!(p); changed = true; } }
                            });
                            let mut size = v["size"].as_f64().unwrap_or(44.0);
                            if ui.add(egui::DragValue::new(&mut size).range(10.0..=800.0).prefix("크기 ")).changed() { v["size"] = json!(size); changed = true; }
                        });
                    });
                    if changed { self.tb.note = err(self.core.lock().save_overlay(&v)); }
                }
            });
    }

    pub(crate) fn library_kind_panel(&mut self, ui: &mut egui::Ui, kind: &str) {
        self.tb.lib_kind = kind.into();
        self.library_panel(ui);
    }

    // ---------------- 회중 메시지 ----------------
    fn message_window(&mut self, ctx: &egui::Context) {
        let mut open = self.tb.message;
        egui::Window::new(wp_core::i18n::tr("회중 메시지")).open(&mut open).default_size([420.0, 320.0]).show(ctx, |ui| {
            let tpls = self.core.lock().settings.get()["messageTemplates"].as_array().cloned().unwrap_or_default();
            ui.horizontal_wrapped(|ui| {
                for t in &tpls {
                    if ui.small_button("×").on_hover_text(wp_core::i18n::tr("이 틀 지우기")).clicked() {
                        let id = t["id"].as_str().unwrap_or("").to_string();
                        self.core.lock().delete_message_template(&id);
                    }
                    if ui.button(t["name"].as_str().unwrap_or("")).clicked() {
                        let text = t["text"].as_str().unwrap_or("").to_string();
                        self.tb.msg_fields = wp_core::overlays::template_fields(&text).into_iter().map(|f| (f, String::new())).collect();
                        self.tb.msg_sec = t["seconds"].as_i64().unwrap_or(0);
                        self.tb.msg_text = text;
                        self.tb.msg_tpl = Some(t.clone());
                    }
                }
            });
            ui.add(egui::TextEdit::multiline(&mut self.tb.msg_text).desired_rows(2).desired_width(f32::INFINITY).hint_text(wp_core::i18n::tr("보일 글 ({이름} 같은 칸은 아래에서 채움)")));
            // 틀 버튼뿐 아니라 직접 입력하거나 고친 칸도 즉시 반영하고 기존 입력값은 유지한다.
            let fields = wp_core::overlays::template_fields(&self.tb.msg_text);
            self.tb.msg_fields = fields.into_iter().map(|key| {
                let value = self.tb.msg_fields.iter().find(|(k, _)| k == &key).map(|(_, v)| v.clone()).unwrap_or_default();
                (key, value)
            }).collect();
            for (k, v) in self.tb.msg_fields.iter_mut() {
                ui.horizontal(|ui| { ui.label(k.as_str()); ui.text_edit_singleline(v); });
            }
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut self.tb.msg_sec).range(0..=3600).suffix(wp_core::i18n::tr("초 (0=계속)")));
                ui.checkbox(&mut self.tb.msg_obs, wp_core::i18n::tr("OBS 자막에도"));
            });
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new(wp_core::i18n::tr("보이기")).fill(Color32::from_rgb(0xb0, 0x2a, 0x2a))).clicked() {
                    let text = wp_core::overlays::fill_template(&self.tb.msg_text, &self.tb.msg_fields);
                    let mut outs = vec!["main".to_string()];
                    if self.tb.msg_obs { outs.push("overlay".into()); }
                    self.tb.note = err(self.core.lock().show_message(&text, &outs, self.tb.msg_sec));
                }
                if ui.button(wp_core::i18n::tr("지우기")).clicked() { self.core.lock().clear_message(); }
                if ui.button(wp_core::i18n::tr("틀로 저장")).clicked() {
                    let name: String = self.tb.msg_text.chars().take(12).collect();
                    let id = self.tb.msg_tpl.as_ref().and_then(|t| t["id"].as_str()).unwrap_or("").to_string();
                    let t = json!({ "id": id, "name": name, "text": self.tb.msg_text, "seconds": self.tb.msg_sec });
                    self.tb.note = err(self.core.lock().save_message_template(&t));
                }
            });
        });
        self.tb.message = open;
    }

    // ---------------- 즉석 자막 ----------------
    fn quick_window(&mut self, ctx: &egui::Context) {
        let mut open = self.tb.quick;
        egui::Window::new(wp_core::i18n::tr("즉석 자막")).open(&mut open).default_size([420.0, 200.0]).show(ctx, |ui| {
            let r = ui.add(egui::TextEdit::multiline(&mut self.tb.quick_text).desired_rows(3).desired_width(f32::INFINITY).hint_text(wp_core::i18n::tr("쓰고 Ctrl+Enter → 바로 송출")));
            let send = (r.has_focus() && ui.input(|i| i.modifiers.ctrl && i.key_pressed(Key::Enter))) | ui.button(wp_core::i18n::tr("바로 송출")).clicked();
            let add = ui.button(wp_core::i18n::tr("큐시트에 공지로 넣기")).clicked();
            if send && !self.tb.quick_text.trim().is_empty() {
                let slide = json!({ "kind": "text", "ref": "", "title": "", "label": "즉석", "parts": [{ "book": 0, "chapter": 0, "verse": 0, "text": self.tb.quick_text.trim() }] });
                let d = Deck { passage: json!({ "title": "즉석 자막", "itemTitle": "즉석 자막", "versionName": "", "cueIndex": -1, "cueId": "", "itemType": "text" }), slides: vec![slide], index: 0 };
                self.core.lock().send_deck(d);
            }
            if add {
                let mut it = wp_core::core::new_item("text", "즉석 자막");
                it["body"] = json!(self.tb.quick_text.trim());
                self.add_to_cue(it);
            }
        });
        self.tb.quick = open;
    }

    // ---------------- 무대 화면 ----------------
    fn stage_window(&mut self, ctx: &egui::Context) {
        let mut open = self.tb.stage;
        egui::Window::new(wp_core::i18n::tr("무대 화면")).open(&mut open).default_size([440.0, 360.0]).show(ctx, |ui| {
            self.workspace_stage(ui);
        });
        self.tb.stage = open;
    }

    pub(crate) fn workspace_stage(&mut self, ui: &mut egui::Ui) {
            let (timer, port, presets) = { let c = self.core.lock(); (c.b.stage.timer_json(), c.port, c.settings.get()["stagePresets"].as_array().cloned().unwrap_or_default()) };
            let rem = timer["remaining"].as_i64().unwrap_or(0);
            ui.label(RichText::new(format!("타이머 {}{:02}:{:02} {}", if rem < 0 { "-" } else { "" }, rem.abs() / 60000, (rem.abs() / 1000) % 60, if timer["running"] == true { wp_core::i18n::tr_s("· 진행 중") } else if timer["duration"].as_i64().unwrap_or(0) > 0 { "· 멈춤" } else { "" })).size(20.0));
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.tb.timer_min).desired_width(50.0).hint_text(wp_core::i18n::tr("분")));
                let m: f64 = self.tb.timer_min.parse().unwrap_or(30.0);
                if ui.button(wp_core::i18n::tr("시작")).clicked() { self.tb.note = err(self.core.lock().stage_timer("start", m)); }
                if ui.button(wp_core::i18n::tr("멈춤")).clicked() { let _ = self.core.lock().stage_timer("pause", 0.0); }
                if ui.button(wp_core::i18n::tr("계속")).clicked() { let _ = self.core.lock().stage_timer("resume", 0.0); }
                if ui.button(wp_core::i18n::tr("+1분")).clicked() { let _ = self.core.lock().stage_timer("add", 1.0); }
                if ui.button(wp_core::i18n::tr("지우기")).clicked() { let _ = self.core.lock().stage_timer("reset", 0.0); }
            });
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.tb.stage_msg).hint_text(wp_core::i18n::tr("무대 메시지 (설교자에게만)")));
                if ui.button(wp_core::i18n::tr("보내기")).clicked() { self.core.lock().stage_message(&self.tb.stage_msg.clone()); }
                if ui.button(wp_core::i18n::tr("지우기")).clicked() { self.tb.stage_msg.clear(); self.core.lock().stage_message(""); }
            });
            ui.separator();
            ui.label(wp_core::i18n::tr("고정 (지금 슬라이드 대신 계속 보이기)"));
            ui.horizontal(|ui| {
                if ui.button(wp_core::i18n::tr("순서표")).clicked() { let _ = self.core.lock().stage_pin_set(Some(&json!({ "mode": "order", "name": "순서표" }))); }
                if ui.button(wp_core::i18n::tr("풀기")).clicked() { let _ = self.core.lock().stage_pin_set(None); }
            });
            ui.add(egui::TextEdit::multiline(&mut self.tb.pin_text).desired_rows(2).desired_width(f32::INFINITY).hint_text(wp_core::i18n::tr("고정할 글")));
            ui.horizontal(|ui| {
                if ui.button(wp_core::i18n::tr("글 고정")).clicked() { let _ = self.core.lock().stage_pin_set(Some(&json!({ "mode": "text", "text": self.tb.pin_text, "name": "" }))); }
                if ui.button(wp_core::i18n::tr("프리셋으로 저장")).clicked() {
                    let name: String = self.tb.pin_text.chars().take(16).collect();
                    self.tb.note = err(self.core.lock().save_stage_preset(&json!({ "mode": "text", "name": name, "text": self.tb.pin_text })));
                }
            });
            ui.horizontal_wrapped(|ui| {
                for p in &presets {
                    let r = ui.small_button(p["name"].as_str().unwrap_or(""));
                    if r.clicked() { let _ = self.core.lock().stage_pin_set(Some(p)); }
                    r.on_hover_text(wp_core::i18n::tr("오른쪽 클릭: 지우기")).context_menu(|ui| { if ui.button(wp_core::i18n::tr("이 프리셋 지우기")).clicked() { self.core.lock().delete_stage_preset(p["id"].as_str().unwrap_or("")); ui.close(); } }); // [트랙 A] deleteStagePreset
                }
            });
            ui.separator();
            ui.horizontal(|ui| {
                let running = self.tb.stage_child.as_mut().is_some_and(|c| matches!(c.try_wait(), Ok(None)));
                if ui.button(if running { wp_core::i18n::tr_s("무대 화면 창 닫기") } else { wp_core::i18n::tr_s("무대 화면 창 열기") }).clicked() {
                    if let Some(mut ch) = self.tb.stage_child.take() { let _ = ch.kill(); }
                    if !running {
                        let exe = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("wp-output.exe"))).unwrap_or_default();
                        let windowed = self.out.monitors.len() < 3;
                        let mut cmd = Command::new(exe);
                        cmd.args(["--url", &format!("http://127.0.0.1:{port}/outputs/stage?window=1")]);
                        if windowed { cmd.arg("--windowed"); } else { cmd.args(["--monitor", "2"]); }
                        self.tb.stage_child = cmd.spawn().ok();
                    }
                }
                ui.label(RichText::new(format!("http://127.0.0.1:{port}/outputs/stage")).small());
            });
    }

    // ---------------- 매크로 ----------------
    fn macros_window(&mut self, ctx: &egui::Context) {
        let mut open = self.tb.macros;
        egui::Window::new(wp_core::i18n::tr("매크로 (Alt+1~9)")).open(&mut open).default_size([420.0, 320.0]).show(ctx, |ui| {
            let list = self.core.lock().settings.get()["macros"].as_array().cloned().unwrap_or_default();
            for (i, m) in list.iter().enumerate() {
                ui.horizontal(|ui| {
                    let id = m["id"].as_str().unwrap_or("").to_string();
                    if ui.button(format!("{} {}", if i < 9 { format!("Alt+{}", i + 1) } else { String::new() }, m["name"].as_str().unwrap_or(""))).clicked() {
                        let mut c = self.core.lock();
                        if let Err(e) = c.run_macro(&id) { c.notify(e.to_string(), "warn"); }
                    }
                    ui.label(RichText::new(actions_ko(&wp_core::overlays::macro_actions(m), m["music"].as_str().is_some_and(|x| !x.is_empty()))).small().color(Color32::GRAY));
                    if ui.small_button(wp_core::i18n::tr("지우기")).clicked() { self.core.lock().delete_macro(&id); }
                });
            }
            ui.separator();
            ui.label(wp_core::i18n::tr("새 매크로: 지금 상태를 담기"));
            ui.horizontal(|ui| { ui.label(wp_core::i18n::tr("이름")); ui.text_edit_singleline(&mut self.tb.macro_name); });
            ui.horizontal(|ui| { ui.label(wp_core::i18n::tr("무대 메시지")); ui.text_edit_singleline(&mut self.tb.macro_msg); });
            if ui.button(wp_core::i18n::tr("저장 (로고·배경·오버레이 상태 + 무대 메시지)")).clicked() {
                let mut c = self.core.lock();
                // Electron 매크로는 순서의 '송출할 때 자동으로' 와 같은 칸: logo on|off · bg hide|show · ovOn(하나) · ovOff('*') · stageMsg
                let on: Vec<String> = c.overlays().into_iter().filter(|o| o["on"] == true).filter_map(|o| o["id"].as_str().map(String::from)).collect();
                let mut a = json!({ "logo": if c.layers.logo { "on" } else { "off" }, "bg": if c.layers.hide_bg { "hide" } else { "show" } });
                match on.first() {
                    Some(id) => a["ovOn"] = json!(id),
                    None => a["ovOff"] = json!("*"),
                }
                if !self.tb.macro_msg.trim().is_empty() {
                    a["stageMsg"] = json!(self.tb.macro_msg.trim());
                }
                let m = json!({ "name": self.tb.macro_name, "actions": a });
                self.tb.note = err(c.save_macro(&m));
            }
            // 예약 실행: 요일·시각에 매크로 (시스템 시간대)
            ui.separator();
            ui.label(RichText::new(wp_core::i18n::tr("예약 실행 (요일·시각이 되면 매크로)")).strong());
            let sched = self.core.lock().settings.get()["schedule"].as_array().cloned().unwrap_or_default();
            for s in &sched {
                let mut e = s.clone();
                let mut changed = false;
                let mut del = false;
                ui.horizontal_wrapped(|ui| {
                    let mut on = e["enabled"].as_bool().unwrap_or(true);
                    if ui.checkbox(&mut on, "").changed() { e["enabled"] = json!(on); changed = true; }
                    let days: Vec<u64> = e["days"].as_array().into_iter().flatten().filter_map(|d| d.as_u64()).collect();
                    for (k, nm) in wp_core::schedule::DAY_NAMES.iter().enumerate() {
                        let has = days.contains(&(k as u64));
                        if ui.selectable_label(has, *nm).clicked() {
                            let mut d2 = days.clone();
                            if has { d2.retain(|x| *x != k as u64) } else { d2.push(k as u64) }
                            e["days"] = json!(d2);
                            changed = true;
                        }
                    }
                    let mut t = e["time"].as_str().unwrap_or("").to_string();
                    let r = ui.add(egui::TextEdit::singleline(&mut t).desired_width(52.0));
                    if r.changed() { e["time"] = json!(t); }
                    if r.lost_focus() { changed = true; }
                    let cur = e["macroId"].as_str().unwrap_or("").to_string();
                    let name = list.iter().find(|m| m["id"] == cur.as_str()).and_then(|m| m["name"].as_str()).unwrap_or("(매크로 고르기)").to_string();
                    egui::ComboBox::from_id_salt(format!("sc-{}", e["id"])).selected_text(name).show_ui(ui, |ui| {
                        for m in &list {
                            if ui.selectable_label(m["id"] == cur.as_str(), m["name"].as_str().unwrap_or("")).clicked() { e["macroId"] = m["id"].clone(); changed = true; }
                        }
                    });
                    if ui.small_button(wp_core::i18n::tr("지우기")).clicked() { del = true; }
                });
                let id = e["id"].as_str().unwrap_or("").to_string();
                if del { self.core.lock().delete_schedule(&id); }
                else if changed { self.tb.note = err(self.core.lock().save_schedule(&e)); }
            }
            if ui.button(wp_core::i18n::tr("예약 더하기")).clicked() {
                let first = list.first().map(|m| m["id"].clone()).unwrap_or(json!(""));
                self.tb.note = err(self.core.lock().save_schedule(&json!({ "days": [0], "time": "10:55", "macroId": first, "enabled": true })));
            }
        });
        self.tb.macros = open;
    }

    // ---------------- 큐시트 관리 ----------------
    fn cues_window(&mut self, ctx: &egui::Context) {
        let mut open = self.tb.cues;
        egui::Window::new(wp_core::i18n::tr("큐시트 관리")).open(&mut open).default_size([460.0, 460.0]).show(ctx, |ui| {
            let (cue, trash, times) = { let c = self.core.lock(); (c.cue.clone(), c.trash_list(), c.planned_times()) };
            // 큐시트 없는 새 데이터 폴더에서도 가져오기를 시작할 수 있다.
                    if ui.button(wp_core::i18n::tr("가져오기…")).clicked() {
                        if let Some(p) = rfd::FileDialog::new().add_filter("큐시트", &["json"]).pick_file() {
                            let r = self.core.lock().import_cue_file(&p);
                            self.tb.note = match r { Ok(n) => format!("큐시트 {n}개를 가져왔습니다."), Err(e) => e.to_string() };
                        }
                    }

            if let Some(cue) = &cue {
                let mut meta = json!({});
                let mut ch = false;
                egui::Grid::new("cuemeta").num_columns(2).show(ui, |ui| {
                    for (k, label, v) in [("name", "이름", cue.name.clone()), ("serviceDate", "예배 날짜", cue.service_date.clone()), ("startTime", "시작 시각", cue.start_time.clone()), ("memo", "메모", cue.memo.clone())] {
                        ui.label(wp_core::i18n::tr(label));
                        let mut s = v;
                        if ui.text_edit_singleline(&mut s).changed() { meta[k] = json!(s); ch = true; }
                        ui.end_row();
                    }
                });
                ui.horizontal(|ui| {
                    let mut p = cue.pinned; if ui.checkbox(&mut p, wp_core::i18n::tr("고정")).changed() { meta["pinned"] = json!(p); ch = true; }
                    let mut t = cue.template; if ui.checkbox(&mut t, wp_core::i18n::tr("서식 (새 큐시트 바탕)")).changed() { meta["template"] = json!(t); ch = true; }
                });
                if ch { self.tb.note = err(self.core.lock().update_cue_meta(&meta)); }
                ui.horizontal_wrapped(|ui| {
                    if ui.button(wp_core::i18n::tr("복제")).clicked() { self.tb.note = err(self.core.lock().duplicate_cue(&cue.id)); }
                    if ui.button(wp_core::i18n::tr("내보내기…")).clicked() {
                        if let Some(p) = rfd::FileDialog::new().set_file_name(format!("{}.wpcue.json", cue.name)).save_file() { self.tb.note = err(self.core.lock().export_cue_file(&p)); }
                    }
                    if ui.button(RichText::new(wp_core::i18n::tr("휴지통으로")).color(Color32::from_rgb(0xff, 0x8a, 0x8a))).clicked() { self.tb.note = err(self.core.lock().trash_cue(&cue.id)); self.sel = None; self.edit = None; }
                });
                ui.separator();
                ui.label(wp_core::i18n::tr("진행 시계 (순서별 예정 시간, 분)"));
                egui::ScrollArea::vertical().id_salt("durs").max_height(160.0).show(ui, |ui| {
                    for (i, it) in cue.items.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(times.get(i).cloned().unwrap_or_default()).monospace());
                            duration_editor(self, ui, i, it);
                            ui.label(it["title"].as_str().unwrap_or(""));
                            if i > 0 && ui.small_button(wp_core::i18n::tr("맨 위로")).clicked() { self.core.lock().move_item_to(i, 0); }
                        });
                    }
                });
            }
            ui.separator();
            ui.label(wp_core::i18n::tr_args("휴지통 ({1}개)", &[&(trash.len())]));
            for t in &trash {
                ui.horizontal(|ui| {
                    ui.label(format!("{} · {}", t["name"].as_str().unwrap_or(""), t["serviceDate"].as_str().unwrap_or("")));
                    let id = t["id"].as_str().unwrap_or("");
                    if ui.small_button(wp_core::i18n::tr("되살리기")).clicked() { self.tb.note = err(self.core.lock().restore_cue(id)); }
                    if ui.small_button(wp_core::i18n::tr("영구 삭제")).clicked() { self.tb.note = err(self.core.lock().purge_cues(Some(id))); }
                });
            }
            if !trash.is_empty() && ui.button(wp_core::i18n::tr("휴지통 비우기")).clicked() { self.tb.note = err(self.core.lock().purge_cues(None)); }
        });
        self.tb.cues = open;
    }
}

/// (요일 0=일, "HH:MM", 날짜 키) — 시스템 시간대
fn chrono_now() -> (u32, String, String) {
    wp_core::localtime::local_now()
}

#[cfg(test)]
mod tests {
    #[test]
    fn now_shape() {
        let (wd, hm, _) = super::chrono_now();
        assert!(wd < 7 && hm.len() == 5);
    }
}

impl App {
    /// LIVE 영상 조작 줄 (영상 슬라이드를 송출 중일 때만): 재생·멈춤 · 처음부터 · ±10초 · 소리 크기 · 반복
    pub(crate) fn video_bar(&mut self, ui: &mut egui::Ui) {
        // 음악 재생 줄 · 트랙 B 알림 (위 도구 줄이 메뉴로 바뀌어 여기에)
        self.music_bar(ui);
        if !self.tb.note.is_empty() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&self.tb.note).small().color(Color32::from_rgb(0xff, 0xb0, 0x4a)));
                if ui.small_button(wp_core::i18n::tr("닫기")).clicked() { self.tb.note.clear(); }
            });
        }
        let v = self.core.lock().b.video.clone();
        let Some(v) = v else { return };
        let mut act: Option<(&str, f64)> = None;
        ui.horizontal(|ui| {
            ui.label(RichText::new(wp_core::i18n::tr("영상")).strong().color(Color32::from_rgb(0xd9, 0x6a, 0x6a)));
            if ui.button(if v.playing { wp_core::i18n::tr_s("⏸ 멈춤") } else { wp_core::i18n::tr_s("▶ 재생") }).clicked() { act = Some((if v.playing { "pause" } else { "play" }, 0.0)); }
            if ui.button(wp_core::i18n::tr("⏮ 처음부터")).clicked() { act = Some(("restart", 0.0)); }
            if ui.button(wp_core::i18n::tr("정지")).clicked() { act = Some(("stop", 0.0)); }
            if ui.button(wp_core::i18n::tr("페이드아웃")).clicked() { act = Some(("fadeout", 3000.0)); }
            let pos = v.position();
            if ui.button(wp_core::i18n::tr("−10초")).clicked() { act = Some(("seek", (pos - 10.0).max(0.0))); }
            if ui.button(wp_core::i18n::tr("+10초")).clicked() { act = Some(("seek", pos + 10.0)); }
            let p = pos.max(0.0) as i64;
            ui.label(RichText::new(format!("{:02}:{:02}{}", p / 60, p % 60, if v.ended { wp_core::i18n::tr_s(" (끝)") } else { "" })).monospace());
            ui.separator();
            let mut vol = v.volume.unwrap_or(1.0) as f32;
            if ui.add(egui::Slider::new(&mut vol, 0.0..=1.0).text(wp_core::i18n::tr("소리"))).changed() { act = Some(("volume", vol as f64)); }
            let mut lp = v.looped.unwrap_or(false);
            if ui.checkbox(&mut lp, wp_core::i18n::tr("반복")).changed() { act = Some(("loop", if lp { 1.0 } else { 0.0 })); }
        });
        let url = self.core.lock().pres.live.as_ref().and_then(|l| l.deck.slides.get(l.deck.index)).and_then(|s| s["image"].as_str()).unwrap_or("").to_owned();
        if let Some(dur) = crate::media_ui::video_duration(ui.ctx(), &url) {
            let mut pos = if v.looped == Some(true) { v.position() % dur } else { v.position().min(dur) };
            if ui.add(egui::Slider::new(&mut pos, 0.0..=dur).text(wp_core::i18n::tr("재생 위치"))).changed() { act = Some(("seek", pos)); }
            ui.label(wp_core::i18n::tr_args("전체 {1}초 · 남은 {2}초", &[&format!("{dur:.0}"), &format!("{:.0}", (dur-pos).max(0.0))]));
        } else { ui.label(wp_core::i18n::tr("영상 길이 확인 불가")); }
        if let Some((a, x)) = act {
            let mut c = self.core.lock();
            if let Err(e) = c.video_control(a, x) { c.notify(e.to_string(), "warn"); }
        }
        if let Some((at, ms)) = v.fade {
            if wp_core::util::now_ms() as f64 >= at as f64 + ms { let _ = self.core.lock().video_control("stop", 0.0); }
        }
        if v.playing { ui.ctx().request_repaint_after(std::time::Duration::from_millis(500)); }
    }
}

impl App {
    /// 큐시트 순서 끌어서 옮기기: 끌기 시작하면 번호를 싣고, 놓은 줄 자리로 (실행 취소 됨)
    pub(crate) fn cue_row_dnd(&mut self, ui: &egui::Ui, r: &egui::Response, i: usize) {
        if r.drag_started() || r.dragged() {
            r.dnd_set_drag_payload(i);
        }
        if let Some(from) = r.dnd_hover_payload::<usize>() {
            if *from != i {
                let y = if *from < i { r.rect.bottom() } else { r.rect.top() };
                ui.painter().hline(r.rect.x_range(), y, egui::Stroke::new(2.5, Color32::from_rgb(0x5b, 0x9c, 0xf6)));
            }
        }
        if let Some(from) = r.dnd_release_payload::<usize>() {
            if *from != i {
                self.core.lock().move_item_to(*from, i);
                self.sel = Some(i);
                self.edit = None;
            }
        }
    }
}

/// 오버레이 종류 이름
fn ov_type_ko(t: &str) -> &'static str {
    match t { "clock" => "시계", "countdown" => "카운트다운", "banner" => "배너", "text" => "글자", "logo" => "로고", _ => "오버레이" }
}

/// 자리 이름 (top-left → 왼쪽 위)
fn pos_ko(p: &str) -> String {
    match p {
        "top-left" => "왼쪽 위", "top-center" => "가운데 위", "top-right" => "오른쪽 위",
        "middle-left" => "왼쪽 가운데", "center" => "한가운데", "middle-right" => "오른쪽 가운데",
        "bottom-left" => "왼쪽 아래", "bottom-center" => "가운데 아래", "bottom-right" => "오른쪽 아래",
        x => x,
    }.to_string()
}

/// 매크로 동작 요약 (Electron cue.js actSummary 와 같은 칸: 순서의 '송출할 때 자동으로')
fn actions_ko(a: &Value, music: bool) -> String {
    let mut out: Vec<String> = vec![];
    let s = |k: &str| a[k].as_str().unwrap_or("");
    if music { out.push("배경 음악".into()); }
    if let Some(m) = a["timer"].as_f64().filter(|x| *x > 0.0) { out.push(format!("⏱ 타이머 {m}분")); }
    if !s("stageMsg").is_empty() { out.push(format!("무대 메시지 '{}'", s("stageMsg").chars().take(12).collect::<String>())); }
    if a["clearStage"] == true { out.push("무대 메시지 지움".into()); }
    match s("logo") { "on" => out.push(wp_core::i18n::tr_s("로고 켜기").into()), "off" => out.push(wp_core::i18n::tr_s("로고 끄기").into()), _ => {} }
    match s("bg") { "hide" => out.push(wp_core::i18n::tr_s("배경 끄기").into()), "show" => out.push(wp_core::i18n::tr_s("배경 켜기").into()), _ => {} }
    if !s("ovOn").is_empty() { out.push(format!("오버레이 켜기: {}", s("ovOn"))); }
    if s("ovOff") == "*" { out.push("오버레이 모두 끄기".into()); } else if !s("ovOff").is_empty() { out.push(format!("오버레이 끄기: {}", s("ovOff"))); }
    if a["stopMusic"] == true { out.push("음악 멈춤".into()); }
    match s("stagePin") { "" => {}, "live" => out.push("무대 고정 풀기".into()), "order" => out.push("무대: 순서표".into()), p => out.push(format!("무대 고정: {p}")) }
    if !s("macro").is_empty() { out.push(format!("매크로: {}", s("macro"))); }
    if !s("companion").is_empty() { out.push(format!("Companion {}", s("companion"))); }
    if let Some(n) = a["dev"].as_array().map(|x| x.len()).filter(|n| *n > 0) { out.push(format!("장비 {n}개")); }
    if !s("obsScene").is_empty() { out.push(format!("OBS 장면: {}", s("obsScene"))); }
    if !s("obsStream").is_empty() { out.push(format!("OBS 방송 {}", if s("obsStream") == "start" { "시작" } else { "끝" })); }
    if !s("obsRecord").is_empty() { out.push(format!("OBS 녹화 {}", if s("obsRecord") == "start" { "시작" } else { "끝" })); }
    if out.is_empty() { "동작 없음".into() } else { out.join(" · ") }
}

#[cfg(test)]
mod tests_ko {
    #[test]
    fn labels() {
        assert_eq!(super::pos_ko("top-left"), "왼쪽 위");
        assert_eq!(super::actions_ko(&serde_json::json!({ "logo": "off", "stageMsg": "홍길동 어린이 부모님" }), false), "무대 메시지 '홍길동 어린이 부모님' · 로고 끄기");
        // 예전 Rust 칸으로 저장된 매크로도 변환해 요약
        let old = serde_json::json!({ "actions": { "logo": false, "bgOff": true } });
        assert_eq!(super::actions_ko(&wp_core::overlays::macro_actions(&old), false), "로고 끄기 · 배경 끄기");
    }
}

/// 관리창과 순서 우클릭 메뉴가 함께 쓰는 예정 시간 편집. 예약/auto 등은 건드리지 않는다.
pub(crate) fn duration_editor(app: &mut App, ui: &mut egui::Ui, i: usize, it: &Value) {
    let mut minutes = it["duration"].as_f64().or_else(|| it["duration"].as_str().and_then(|s| s.parse().ok())).unwrap_or(0.0);
    if ui.add(egui::DragValue::new(&mut minutes).range(0.0..=600.0).max_decimals(0).speed(1.0).suffix(wp_core::i18n::tr("분"))).changed() {
        let mut c = app.core.lock();
        // 프레임 도중 큐/순서가 바뀌었다면 다른 항목에 쓰지 않는다.
        if !c.cue.as_ref().and_then(|q| q.items.get(i)).is_some_and(|x| x["id"] == it["id"]) { return; }
        c.remember_items();
        if let Some(x) = c.cue.as_mut().and_then(|q| q.items.get_mut(i)) { set_duration(x, minutes); }
        c.save_cue();
        // 같은 프레임의 이전 스냅샷으로 편집칸이 duration을 되돌리지 않게 갱신한다.
        app.edit = c.cue.as_ref().and_then(|q| q.items.get(i)).map(|x| (x["id"].as_str().unwrap_or("").to_string(), x.clone()));
    }
}

fn set_duration(it: &mut Value, minutes: f64) {
    // Electron itemAction('duration')와 같은 정수 분 범위.
    it["duration"] = json!(if minutes.is_finite() { minutes.clamp(0.0, 600.0).trunc() as u64 } else { 0 });
}

#[cfg(test)]
mod cue_duration_tests {
    use super::*;

    #[test]
    fn duration_matches_electron_limits_and_preserves_metadata() {
        let original = json!({"id":"i1", "type":"hymn", "duration":"5", "auto":{"sec":8,"loop":true}, "schedule":{"time":"11:00"}, "range":"2-7", "note":"진행 메모"});
        for (input, expected) in [(-1.0, 0), (0.0, 0), (12.9, 12), (600.0, 600), (900.0, 600), (f64::NAN, 0)] {
            let mut item = original.clone();
            set_duration(&mut item, input);
            assert_eq!(item["duration"], json!(expected));
            item["duration"] = original["duration"].clone();
            assert_eq!(item, original);
        }
    }
}

/// 찬양 목록 줄 (.lib-row: 여백 6px 10px · 아래 line-soft 선 · 음표 아이콘 · 굵은 제목 · 흐린 첫 줄 · 오른쪽 '고치기')
fn song_row(ui: &mut egui::Ui, title: &str, first: &str, selected: bool) -> (egui::Response, egui::Response) {
    use crate::theme::{font, pal, Weight};
    use eframe::egui::{Pos2, Rect, Sense, Stroke, Vec2};
    let p = pal();
    let w = ui.available_width();
    let tg = ui.painter().layout_no_wrap(title.to_string(), font(14.0, Weight::Bold), p.text);
    let fg = (!first.is_empty()).then(|| ui.painter().layout_no_wrap(first.to_string(), font(12.0, Weight::Regular), p.muted));
    let h = 12.0 + tg.size().y + fg.as_ref().map(|g| g.size().y).unwrap_or(0.0);
    let (r, row) = ui.allocate_exact_size(Vec2::new(w, h.max(34.0)), Sense::click());
    let eg = ui.painter().layout_no_wrap(wp_core::i18n::tr("고치기"), font(12.0, Weight::Regular), p.text);
    let er = Rect::from_min_size(Pos2::new(r.right() - 10.0 - eg.size().x - 18.0, r.center().y - 12.0), Vec2::new(eg.size().x + 18.0, 24.0));
    let edit = ui.interact(er, row.id.with("edit"), Sense::click());
    let pt = ui.painter();
    if selected { pt.rect_filled(r, 0.0, p.sel); } else if row.hovered() || edit.hovered() { pt.rect_filled(r, 0.0, p.hover); }
    pt.hline(r.x_range(), r.bottom() - 0.5, Stroke::new(1.0, p.line_soft));
    crate::icons::paint(ui, Rect::from_center_size(Pos2::new(r.left() + 10.0 + 10.0, r.center().y), Vec2::splat(16.0)), "music", crate::theme::type_ink("song"));
    let tx = r.left() + 10.0 + 20.0 + 8.0;
    let clip = Rect::from_min_max(Pos2::new(tx, r.top()), Pos2::new(er.left() - 8.0, r.bottom()));
    let pt = ui.painter().with_clip_rect(clip);
    let mut y = r.center().y - (h - 12.0) / 2.0;
    let th = tg.size().y;
    pt.galley(Pos2::new(tx, y), tg, p.text);
    y += th;
    if let Some(g) = fg { pt.galley(Pos2::new(tx, y), g, p.muted); }
    let (bg, border) = if edit.hovered() { (p.hover, p.line2) } else { (p.panel2, p.line) };
    ui.painter().rect(er, eframe::egui::CornerRadius::same(crate::theme::RADIUS), bg, Stroke::new(p.border_w, border), eframe::egui::StrokeKind::Inside);
    ui.painter().galley(er.center() - eg.size() / 2.0, eg, p.text);
    if row.hovered() || edit.hovered() { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
    (row, edit.on_hover_text(wp_core::i18n::tr("가사 고치기")))
}

/// 폴더 안 찬양 파일 모으기 (하위 폴더까지, 깊이 8까지)
fn collect_song_files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>, depth: usize) {
    if depth > 8 { return; }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let path = e.path();
        if path.is_dir() { collect_song_files(&path, out, depth + 1); continue; }
        let ext = path.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
        if wp_core::songimport::SONG_EXT.contains(&ext.as_str()) { out.push(path); }
    }
}
