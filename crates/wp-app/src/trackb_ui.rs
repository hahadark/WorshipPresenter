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
            ui.toggle_value(&mut t.songs, "찬양");
            ui.toggle_value(&mut t.lib, "라이브러리");
            ui.toggle_value(&mut t.overlays, "오버레이");
            ui.toggle_value(&mut t.message, "회중 메시지 Ctrl+M");
            ui.toggle_value(&mut t.quick, "즉석 자막 Ctrl+T");
            ui.toggle_value(&mut t.stage, "무대 화면");
            ui.toggle_value(&mut t.macros, "매크로");
            ui.toggle_value(&mut t.cues, "큐시트 관리");
            if ui.button("인쇄 Ctrl+P").clicked() { self.print_cue(); }
            if ui.button("↶").on_hover_text("실행 취소 Ctrl+Z").clicked() { self.core.lock().undo_items(false); self.edit = None; }
            if ui.button("↷").on_hover_text("다시 Ctrl+Y").clicked() { self.core.lock().undo_items(true); self.edit = None; }
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
        egui::Window::new("찬양 라이브러리").open(&mut open).default_size([520.0, 560.0]).show(ctx, |ui|  self.songs_panel(ui));
        self.tb.songs = open;
    }

    // ---------------- 라이브러리 ----------------
    fn lib_window(&mut self, ctx: &egui::Context) {
        let mut open = self.tb.lib;
        if self.tb.lib_kind.is_empty() { self.tb.lib_kind = "image".into(); }
        egui::Window::new("라이브러리 (그림·영상·음악)").open(&mut open).default_size([520.0, 520.0]).show(ctx, |ui|  self.library_panel(ui));
        self.tb.lib = open;
    }

    // ---------------- 왼쪽 탭에도 쓰는 패널 ----------------
    pub(crate) fn songs_panel(&mut self, ui: &mut egui::Ui) {

        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.tb.song_q).hint_text("제목·가사로 찾기").desired_width(260.0));
            if ui.button("새 찬양").clicked() { self.tb.song_edit = Some(json!({ "title": "", "lyrics": "", "credit": "", "tags": "", "arrangement": "" })); }
            if ui.button("웹에서 가사 찾기 Ctrl+L").clicked() { crate::lyrics_ui::open(); }
        });
        let list = self.core.lock().list_songs(&self.tb.song_q);
        ui.label(RichText::new(format!("{}곡", list.len())).small());
        egui::ScrollArea::vertical().id_salt("songlist").max_height(200.0).show(ui, |ui| {
            for s in &list {
                ui.horizontal(|ui| {
                    let r = ui.selectable_label(false, format!("{}  ·  {}", s["title"].as_str().unwrap_or(""), s["first"].as_str().unwrap_or("")));
                    let id = s["id"].as_str().unwrap_or("").to_string();
                    if r.clicked() { self.tb.song_edit = self.core.lock().get_song(&id); }
                    if r.double_clicked() || ui.small_button("큐시트에").clicked() {
                        let it = self.core.lock().song_item(&id);
                        if let Some(it) = it { self.add_to_cue(it); }
                    }
                });
            }
        });
        ui.separator();
        if let Some(e) = &mut self.tb.song_edit {
            let mut s = |k: &str, ui: &mut egui::Ui, multi: bool| {
                let mut v = e[k].as_str().unwrap_or("").to_string();
                let r = if multi { ui.add(egui::TextEdit::multiline(&mut v).desired_rows(8).desired_width(f32::INFINITY)) } else { ui.add(egui::TextEdit::singleline(&mut v).desired_width(f32::INFINITY)) };
                if r.changed() { e[k] = json!(v); }
            };
            ui.label("제목"); s("title", ui, false);
            ui.label("가사 (빈 줄로 슬라이드, [1절] [후렴] 이름표)"); s("lyrics", ui, true);
            ui.label("곡 정보 (작사·작곡·저작권)"); s("credit", ui, false);
            ui.label("기본 부를 순서 (예: 1 C 2 C)"); s("arrangement", ui, false);
            ui.label(RichText::new(format!("저장한 부를 순서: {}", e["arrangements"].as_array().map(|a| a.iter().map(|x| format!("{}={}", x["name"].as_str().unwrap_or(""), x["order"].as_str().unwrap_or(""))).collect::<Vec<_>>().join(" · ")).unwrap_or_default())).small());
            let e2 = e.clone();
            ui.horizontal(|ui| {
                if ui.button("저장").clicked() {
                    let r = self.core.lock().save_song(&e2);
                    match r { Ok(v) => { self.tb.song_edit = Some(v); self.tb.note = "찬양을 저장했습니다.".into(); } Err(x) => self.tb.note = x.to_string() }
                }
                if ui.button("이 부를 순서 이름 붙여 저장").clicked() {
                    if let (Some(id), Some(o)) = (e2["id"].as_str(), e2["arrangement"].as_str()) {
                        let name = format!("순서 {}", e2["arrangements"].as_array().map(|a| a.len() + 1).unwrap_or(1));
                        let r = self.core.lock().save_song_arrangement(id, &name, o);
                        if let Ok(v) = r { self.tb.song_edit = Some(v); }
                    }
                }
                if ui.button(RichText::new("지우기").color(Color32::from_rgb(0xff, 0x8a, 0x8a))).clicked() {
                    if let Some(id) = e2["id"].as_str() { self.tb.note = err(self.core.lock().delete_song(id)); self.tb.song_edit = None; }
                }
            });
        }
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("파일에서 가져오기…").on_hover_text("OpenLP(OpenLyrics .xml) · ChordPro(.cho) · ProPresenter 6(.pro6) · PowerPoint(.pptx .ppt .pps) · 글(.txt)").clicked() {
                if let Some(fs) = rfd::FileDialog::new().add_filter("찬양 파일", &wp_core::songimport::SONG_EXT).pick_files() {
                    let r = self.core.lock().import_song_files(&fs);
                    self.tb.note = match r {
                        Ok(v) => {
                            let errs: Vec<String> = v["errors"].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(String::from)).collect();
                            format!("{}곡 저장 · 같은 제목 {}곡 건너뜀{}", v["saved"], v["skipped"], if errs.is_empty() { String::new() } else { format!(" · 못 읽음: {}", errs.join(" / ")) })
                        }
                        Err(e) => e.to_string(),
                    };
                }
            }
            ui.label(RichText::new("OpenLP · ChordPro · ProPresenter 6 · PPT · txt").small().color(Color32::GRAY));
        });
        ui.collapsing("가사 붙여 넣어 가져오기 (여러 곡은 '# 제목' 줄로 나눔)", |ui| {
            ui.add(egui::TextEdit::multiline(&mut self.tb.import_text).desired_rows(5).desired_width(f32::INFINITY));
            if ui.button("가져오기").clicked() {
                let r = self.core.lock().import_song_text(&self.tb.import_text, 2);
                self.tb.note = match r { Ok(v) => format!("{}곡 저장 · 같은 제목 {}곡 건너뜀", v["saved"], v["skipped"]), Err(e) => e.to_string() };
            }
        });
    }

    pub(crate) fn library_panel(&mut self, ui: &mut egui::Ui) {

        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.tb.lib_kind, "image".to_string(), "그림·영상");
            ui.selectable_value(&mut self.tb.lib_kind, "audio".to_string(), "음악");
            if ui.button("폴더 더하기…").clicked() {
                if let Some(d) = rfd::FileDialog::new().pick_folder() {
                    let k = self.tb.lib_kind.clone();
                    self.tb.note = err(self.core.lock().add_lib_folder(&k, &d));
                }
            }
            if ui.button("파일 넣기…").on_hover_text("폴더 밖 파일도 (끌어다 놓은 파일처럼 기억)").clicked() {
                if let Some(fs) = rfd::FileDialog::new().pick_files() {
                    let items = self.core.lock().add_dropped_files(&fs);
                    for it in items { let mut x = wp_core::core::new_item(it["type"].as_str().unwrap_or("image"), ""); wp_core::settings::merge_into(&mut x, &it); self.add_to_cue(x); }
                }
            }
        });
        let kind = self.tb.lib_kind.clone();
        let folders = wp_core::library::folders(self.core.lock().settings.get(), &kind);
        for f in &folders {
            ui.horizontal(|ui| {
                ui.label(format!("📁 {} ({}개){}", f["name"].as_str().unwrap_or(""), f["count"], if f["exists"] == false { " · 폴더 없음" } else { "" }));
                if ui.small_button("빼기").clicked() { self.core.lock().remove_lib_folder(&kind, f["path"].as_str().unwrap_or("")); }
            });
        }
        ui.add(egui::TextEdit::singleline(&mut self.tb.lib_q).hint_text("파일 이름으로 찾기"));
        let files = self.core.lock().search_lib(&kind, &self.tb.lib_q, "");
        egui::ScrollArea::vertical().id_salt("libfiles").show(ui, |ui| {
            for f in files.iter().take(300) {
                ui.horizontal(|ui| {
                    ui.label(format!("{}{}", if f["video"] == true { "🎞 " } else { "" }, f["name"].as_str().unwrap_or("")));
                    let url = f["url"].as_str().unwrap_or("").to_string();
                    let name = f["name"].as_str().unwrap_or("").to_string();
                    if kind == "image" {
                        if ui.small_button("순서로").clicked() {
                            let it = if f["video"] == true { json!({ "type": "video", "title": name, "file": url, "sound": true, "volume": 1.0, "loop": false, "endAction": "hold" }) } else { json!({ "type": "image", "title": name, "image": url }) };
                            let mut x = wp_core::core::new_item(it["type"].as_str().unwrap(), &name); wp_core::settings::merge_into(&mut x, &it);
                            self.add_to_cue(x);
                        }
                        if ui.small_button("배경으로").clicked() {
                            let ty = if f["video"] == true { "video" } else { "image" };
                            self.core.lock().set_background_media(Some(json!({ "type": ty, "url": url, "label": name, "fit": "cover" })));
                        }
                    } else if ui.small_button("▶").on_hover_text("바로 재생 (조작 PC)").clicked() {
                        crate::music_ui::play_now(&url, &name);
                    } else if ui.small_button("고른 순서 배경 음악").clicked() {
                        if let Some(i) = self.sel {
                            let mut c = self.core.lock();
                            c.remember_items();
                            if let Some(cue) = &mut c.cue { if let Some(it) = cue.items.get_mut(i) { it["music"] = json!(url); } }
                            c.save_cue();
                            self.edit = None;
                        }
                    }
                });
            }
        });
        if ui.button("배경 미디어 끄기").clicked() { self.core.lock().set_background_media(None); }
    }

    // ---------------- 오버레이 ----------------
    fn overlays_window(&mut self, ctx: &egui::Context) {
        let mut open = self.tb.overlays;
        egui::Window::new("오버레이").open(&mut open).default_size([460.0, 420.0]).show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                for (ty, label) in [("clock", "시계"), ("countdown", "카운트다운"), ("banner", "배너"), ("text", "글자"), ("logo", "로고")] {
                    if ui.button(format!("+ {label}")).clicked() {
                        let base = match ty { "countdown" => json!({ "type": ty, "target": "5", "doneText": "" }), "banner" => json!({ "type": ty, "text": "환영합니다", "bg": "rgba(0,0,0,0.7)", "full": true }), "text" => json!({ "type": ty, "text": "글자" }), "logo" => json!({ "type": ty, "image": "/shared/assets/sa-shield.png" }), _ => json!({ "type": ty }) };
                        self.tb.note = err(self.core.lock().save_overlay(&base));
                    }
                }
                if ui.button("모두 끄기·다시 켜기").clicked() { self.core.lock().toggle_overlays_off(); }
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
                            if ui.small_button("지우기").clicked() { self.core.lock().delete_overlay(&id); }
                        });
                        ui.horizontal(|ui| {
                            for k in ["text", "target"] {
                                if (k == "text" && ["banner", "text"].contains(&o["type"].as_str().unwrap_or(""))) || (k == "target" && o["type"] == "countdown") {
                                    let mut s = v[k].as_str().unwrap_or("").to_string();
                                    if ui.add(egui::TextEdit::singleline(&mut s).desired_width(200.0).hint_text(if k == "target" { "HH:MM 또는 분" } else { "글" })).changed() { v[k] = json!(s); changed = true; }
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
        });
        self.tb.overlays = open;
    }

    // ---------------- 회중 메시지 ----------------
    fn message_window(&mut self, ctx: &egui::Context) {
        let mut open = self.tb.message;
        egui::Window::new("회중 메시지").open(&mut open).default_size([420.0, 320.0]).show(ctx, |ui| {
            let tpls = self.core.lock().settings.get()["messageTemplates"].as_array().cloned().unwrap_or_default();
            ui.horizontal_wrapped(|ui| {
                for t in &tpls {
                    if ui.small_button("×").on_hover_text("이 틀 지우기").clicked() {
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
            ui.add(egui::TextEdit::multiline(&mut self.tb.msg_text).desired_rows(2).desired_width(f32::INFINITY).hint_text("보일 글 ({이름} 같은 칸은 아래에서 채움)"));
            for (k, v) in self.tb.msg_fields.iter_mut() {
                ui.horizontal(|ui| { ui.label(k.as_str()); ui.text_edit_singleline(v); });
            }
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut self.tb.msg_sec).range(0..=3600).suffix("초 (0=계속)"));
                ui.checkbox(&mut self.tb.msg_obs, "OBS 자막에도");
            });
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new("보이기").fill(Color32::from_rgb(0xb0, 0x2a, 0x2a))).clicked() {
                    let text = wp_core::overlays::fill_template(&self.tb.msg_text, &self.tb.msg_fields);
                    let mut outs = vec!["main".to_string()];
                    if self.tb.msg_obs { outs.push("overlay".into()); }
                    self.tb.note = err(self.core.lock().show_message(&text, &outs, self.tb.msg_sec));
                }
                if ui.button("지우기").clicked() { self.core.lock().clear_message(); }
                if ui.button("틀로 저장").clicked() {
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
        egui::Window::new("즉석 자막").open(&mut open).default_size([420.0, 200.0]).show(ctx, |ui| {
            let r = ui.add(egui::TextEdit::multiline(&mut self.tb.quick_text).desired_rows(3).desired_width(f32::INFINITY).hint_text("쓰고 Ctrl+Enter → 바로 송출"));
            let send = (r.has_focus() && ui.input(|i| i.modifiers.ctrl && i.key_pressed(Key::Enter))) | ui.button("바로 송출").clicked();
            let add = ui.button("큐시트에 공지로 넣기").clicked();
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
        egui::Window::new("무대 화면").open(&mut open).default_size([440.0, 360.0]).show(ctx, |ui| {
            let (timer, port, presets) = { let c = self.core.lock(); (c.b.stage.timer_json(), c.port, c.settings.get()["stagePresets"].as_array().cloned().unwrap_or_default()) };
            let rem = timer["remaining"].as_i64().unwrap_or(0);
            ui.label(RichText::new(format!("타이머 {}{:02}:{:02} {}", if rem < 0 { "-" } else { "" }, rem.abs() / 60000, (rem.abs() / 1000) % 60, if timer["running"] == true { "· 진행 중" } else if timer["duration"].as_i64().unwrap_or(0) > 0 { "· 멈춤" } else { "" })).size(20.0));
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.tb.timer_min).desired_width(50.0).hint_text("분"));
                let m: f64 = self.tb.timer_min.parse().unwrap_or(30.0);
                let mut c = self.core.lock();
                if ui.button("시작").clicked() { self.tb.note = err(c.stage_timer("start", m)); }
                if ui.button("멈춤").clicked() { let _ = c.stage_timer("pause", 0.0); }
                if ui.button("계속").clicked() { let _ = c.stage_timer("resume", 0.0); }
                if ui.button("+1분").clicked() { let _ = c.stage_timer("add", 1.0); }
                if ui.button("지우기").clicked() { let _ = c.stage_timer("reset", 0.0); }
            });
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.tb.stage_msg).hint_text("무대 메시지 (설교자에게만)"));
                if ui.button("보내기").clicked() { self.core.lock().stage_message(&self.tb.stage_msg.clone()); }
                if ui.button("지우기").clicked() { self.tb.stage_msg.clear(); self.core.lock().stage_message(""); }
            });
            ui.separator();
            ui.label("고정 (지금 슬라이드 대신 계속 보이기)");
            ui.horizontal(|ui| {
                let mut c = self.core.lock();
                if ui.button("순서표").clicked() { c.stage_pin_set(Some(&json!({ "mode": "order", "name": "순서표" }))); }
                if ui.button("풀기").clicked() { c.stage_pin_set(None); }
            });
            ui.add(egui::TextEdit::multiline(&mut self.tb.pin_text).desired_rows(2).desired_width(f32::INFINITY).hint_text("고정할 글"));
            ui.horizontal(|ui| {
                if ui.button("글 고정").clicked() { self.core.lock().stage_pin_set(Some(&json!({ "mode": "text", "text": self.tb.pin_text, "name": "" }))); }
                if ui.button("프리셋으로 저장").clicked() {
                    let name: String = self.tb.pin_text.chars().take(16).collect();
                    self.tb.note = err(self.core.lock().save_stage_preset(&json!({ "mode": "text", "name": name, "text": self.tb.pin_text })));
                }
            });
            ui.horizontal_wrapped(|ui| {
                for p in &presets { if ui.small_button(p["name"].as_str().unwrap_or("")).clicked() { self.core.lock().stage_pin_set(Some(p)); } }
            });
            ui.separator();
            ui.horizontal(|ui| {
                let running = self.tb.stage_child.as_mut().is_some_and(|c| matches!(c.try_wait(), Ok(None)));
                if ui.button(if running { "무대 화면 창 닫기" } else { "무대 화면 창 열기" }).clicked() {
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
        });
        self.tb.stage = open;
    }

    // ---------------- 매크로 ----------------
    fn macros_window(&mut self, ctx: &egui::Context) {
        let mut open = self.tb.macros;
        egui::Window::new("매크로 (Alt+1~9)").open(&mut open).default_size([420.0, 320.0]).show(ctx, |ui| {
            let list = self.core.lock().settings.get()["macros"].as_array().cloned().unwrap_or_default();
            for (i, m) in list.iter().enumerate() {
                ui.horizontal(|ui| {
                    let id = m["id"].as_str().unwrap_or("").to_string();
                    if ui.button(format!("{} {}", if i < 9 { format!("Alt+{}", i + 1) } else { String::new() }, m["name"].as_str().unwrap_or(""))).clicked() {
                        let mut c = self.core.lock();
                        if let Err(e) = c.run_macro(&id) { c.notify(e.to_string(), "warn"); }
                    }
                    ui.label(RichText::new(actions_ko(&m["actions"])).small().color(Color32::GRAY));
                    if ui.small_button("지우기").clicked() { self.core.lock().delete_macro(&id); }
                });
            }
            ui.separator();
            ui.label("새 매크로: 지금 상태를 담기");
            ui.horizontal(|ui| { ui.label("이름"); ui.text_edit_singleline(&mut self.tb.macro_name); });
            ui.horizontal(|ui| { ui.label("메시지"); ui.text_edit_singleline(&mut self.tb.macro_msg); });
            if ui.button("저장 (로고·배경·오버레이 상태 + 메시지)").clicked() {
                let mut c = self.core.lock();
                let on: Vec<Value> = c.overlays().into_iter().filter(|o| o["on"] == true).map(|o| o["id"].clone()).collect();
                let m = json!({ "name": self.tb.macro_name, "actions": { "logo": c.layers.logo, "bgOff": c.layers.hide_bg, "overlayOn": on, "message": self.tb.macro_msg } });
                self.tb.note = err(c.save_macro(&m));
            }
            // 예약 실행: 요일·시각에 매크로 (시스템 시간대)
            ui.separator();
            ui.label(RichText::new("예약 실행 (요일·시각이 되면 매크로)").strong());
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
                    if ui.small_button("지우기").clicked() { del = true; }
                });
                let id = e["id"].as_str().unwrap_or("").to_string();
                if del { self.core.lock().delete_schedule(&id); }
                else if changed { self.tb.note = err(self.core.lock().save_schedule(&e)); }
            }
            if ui.button("예약 더하기").clicked() {
                let first = list.first().map(|m| m["id"].clone()).unwrap_or(json!(""));
                self.tb.note = err(self.core.lock().save_schedule(&json!({ "days": [0], "time": "10:55", "macroId": first, "enabled": true })));
            }
        });
        self.tb.macros = open;
    }

    // ---------------- 큐시트 관리 ----------------
    fn cues_window(&mut self, ctx: &egui::Context) {
        let mut open = self.tb.cues;
        egui::Window::new("큐시트 관리").open(&mut open).default_size([460.0, 460.0]).show(ctx, |ui| {
            let (cue, trash, times) = { let c = self.core.lock(); (c.cue.clone(), c.trash_list(), c.planned_times()) };
            if let Some(cue) = &cue {
                let mut meta = json!({});
                let mut ch = false;
                egui::Grid::new("cuemeta").num_columns(2).show(ui, |ui| {
                    for (k, label, v) in [("name", "이름", cue.name.clone()), ("serviceDate", "예배 날짜", cue.service_date.clone()), ("startTime", "시작 시각", cue.start_time.clone()), ("memo", "메모", cue.memo.clone())] {
                        ui.label(label);
                        let mut s = v;
                        if ui.text_edit_singleline(&mut s).changed() { meta[k] = json!(s); ch = true; }
                        ui.end_row();
                    }
                });
                ui.horizontal(|ui| {
                    let mut p = cue.pinned; if ui.checkbox(&mut p, "고정").changed() { meta["pinned"] = json!(p); ch = true; }
                    let mut t = cue.template; if ui.checkbox(&mut t, "서식 (새 큐시트 바탕)").changed() { meta["template"] = json!(t); ch = true; }
                });
                if ch { self.tb.note = err(self.core.lock().update_cue_meta(&meta)); }
                ui.horizontal_wrapped(|ui| {
                    if ui.button("복제").clicked() { self.tb.note = err(self.core.lock().duplicate_cue(&cue.id)); }
                    if ui.button("내보내기…").clicked() {
                        if let Some(p) = rfd::FileDialog::new().set_file_name(format!("{}.wpcue.json", cue.name)).save_file() { self.tb.note = err(self.core.lock().export_cue_file(&p)); }
                    }
                    if ui.button("가져오기…").clicked() {
                        if let Some(p) = rfd::FileDialog::new().add_filter("큐시트", &["json"]).pick_file() {
                            let r = self.core.lock().import_cue_file(&p);
                            self.tb.note = match r { Ok(n) => format!("큐시트 {n}개를 가져왔습니다."), Err(e) => e.to_string() };
                        }
                    }
                    if ui.button(RichText::new("휴지통으로").color(Color32::from_rgb(0xff, 0x8a, 0x8a))).clicked() { self.tb.note = err(self.core.lock().trash_cue(&cue.id)); self.sel = None; self.edit = None; }
                });
                ui.separator();
                ui.label("진행 시계 (순서별 예정 시간, 분)");
                egui::ScrollArea::vertical().id_salt("durs").max_height(160.0).show(ui, |ui| {
                    for (i, it) in cue.items.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(times.get(i).cloned().unwrap_or_default()).monospace());
                            let mut d = it["duration"].as_f64().unwrap_or(0.0);
                            if ui.add(egui::DragValue::new(&mut d).range(0.0..=240.0).suffix("분")).changed() {
                                let mut c = self.core.lock();
                                c.remember_items();
                                if let Some(cc) = &mut c.cue { if let Some(x) = cc.items.get_mut(i) { x["duration"] = json!(d); } }
                                c.save_cue();
                            }
                            ui.label(it["title"].as_str().unwrap_or(""));
                            if i > 0 && ui.small_button("맨 위로").clicked() { self.core.lock().move_item_to(i, 0); }
                        });
                    }
                });
            }
            ui.separator();
            ui.label(format!("휴지통 ({}개)", trash.len()));
            for t in &trash {
                ui.horizontal(|ui| {
                    ui.label(format!("{} · {}", t["name"].as_str().unwrap_or(""), t["serviceDate"].as_str().unwrap_or("")));
                    let id = t["id"].as_str().unwrap_or("");
                    if ui.small_button("되살리기").clicked() { self.tb.note = err(self.core.lock().restore_cue(id)); }
                    if ui.small_button("영구 삭제").clicked() { self.tb.note = err(self.core.lock().purge_cues(Some(id))); }
                });
            }
            if !trash.is_empty() && ui.button("휴지통 비우기").clicked() { self.tb.note = err(self.core.lock().purge_cues(None)); }
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
                if ui.small_button("닫기").clicked() { self.tb.note.clear(); }
            });
        }
        let v = self.core.lock().b.video.clone();
        let Some(v) = v else { return };
        let mut act: Option<(&str, f64)> = None;
        ui.horizontal(|ui| {
            ui.label(RichText::new("영상").strong().color(Color32::from_rgb(0xd9, 0x6a, 0x6a)));
            if ui.button(if v.playing { "⏸ 멈춤" } else { "▶ 재생" }).clicked() { act = Some((if v.playing { "pause" } else { "play" }, 0.0)); }
            if ui.button("⏮ 처음부터").clicked() { act = Some(("restart", 0.0)); }
            let pos = v.position();
            if ui.button("−10초").clicked() { act = Some(("seek", (pos - 10.0).max(0.0))); }
            if ui.button("+10초").clicked() { act = Some(("seek", pos + 10.0)); }
            let p = pos.max(0.0) as i64;
            ui.label(RichText::new(format!("{:02}:{:02}{}", p / 60, p % 60, if v.ended { " (끝)" } else { "" })).monospace());
            ui.separator();
            let mut vol = v.volume.unwrap_or(1.0) as f32;
            if ui.add(egui::Slider::new(&mut vol, 0.0..=1.0).text("소리")).changed() { act = Some(("volume", vol as f64)); }
            let mut lp = v.looped.unwrap_or(false);
            if ui.checkbox(&mut lp, "반복").changed() { act = Some(("loop", if lp { 1.0 } else { 0.0 })); }
        });
        if let Some((a, x)) = act {
            let mut c = self.core.lock();
            if let Err(e) = c.video_control(a, x) { c.notify(e.to_string(), "warn"); }
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

/// 매크로 동작 요약 (JSON 대신 사람이 읽는 글)
fn actions_ko(a: &Value) -> String {
    let mut out: Vec<String> = vec![];
    let b = |k: &str| a[k].as_bool();
    if b("clearAll") == Some(true) { out.push("모두 지우기".into()); }
    if b("hideText") == Some(true) { out.push("글자 숨김".into()); }
    if let Some(x) = b("logo") { out.push(if x { "로고 켜기" } else { "로고 끄기" }.into()); }
    if let Some(x) = b("bgOff") { out.push(if x { "배경 끄기" } else { "배경 켜기" }.into()); }
    if b("overlaysOff") == Some(true) { out.push("오버레이 모두 끄기".into()); }
    let n = |k: &str| a[k].as_array().map(|x| x.len()).unwrap_or(0);
    if n("overlayOn") > 0 { out.push(format!("오버레이 {}개 켜기", n("overlayOn"))); }
    if n("overlayOff") > 0 { out.push(format!("오버레이 {}개 끄기", n("overlayOff"))); }
    if let Some(t) = a["message"].as_str().filter(|x| !x.is_empty()) { out.push(format!("메시지 '{}'", t.chars().take(16).collect::<String>())); }
    if let Some(t) = a["stageMessage"].as_str().filter(|x| !x.is_empty()) { out.push(format!("무대 메시지 '{}'", t.chars().take(12).collect::<String>())); }
    if let Some(m) = a["timer"].as_f64().filter(|x| *x > 0.0) { out.push(format!("타이머 {m}분")); }
    if let Some(i) = a["cueItem"].as_u64() { out.push(format!("{}번 순서 송출", i + 1)); }
    if a["stagePin"].is_object() { out.push("무대 화면 고정".into()); }
    if out.is_empty() { "동작 없음".into() } else { out.join(" · ") }
}

#[cfg(test)]
mod tests_ko {
    #[test]
    fn labels() {
        assert_eq!(super::pos_ko("top-left"), "왼쪽 위");
        assert_eq!(super::actions_ko(&serde_json::json!({ "logo": false, "message": "홍길동 어린이 부모님" })), "로고 끄기 · 메시지 '홍길동 어린이 부모님'");
    }
}