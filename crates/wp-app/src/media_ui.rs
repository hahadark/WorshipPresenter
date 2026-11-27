//! 미디어 직접 송출과 저장한 찬양 순서 위젯.
use crate::app::App;
use eframe::egui;
use serde_json::{json, Value};
use wp_core::{core::Core, presentation::Deck, i18n::tr};

pub fn video_deck(f: &Value) -> Deck {
    Deck { passage: json!({"title":f["name"],"itemTitle":f["name"],"itemType":"video","cueIndex":-1,"cueId":""}),
        slides: vec![json!({"kind":"video","image":f["url"],"ref":f["name"],"label":tr("영상"),"parts":[],"caption":"","video":{"volume":1,"sound":true,"loop":false,"endAction":"hold"}})], index:0 }
}
/// 검색 결과의 경로로 실제 형식을 확인한다. audio 결과에는 video 플래그가 없을 수 있다.
fn matches_tab(f: &Value, audio: bool) -> bool {
    let path = std::path::Path::new(f["path"].as_str().unwrap_or(""));
    if audio { wp_core::library::is_audio(path) } else { f["video"] == true }
}

pub(crate) fn media_item(f: &Value, audio: bool) -> Value {
    let mut it = wp_core::core::new_item(if audio { "audio" } else { "video" }, f["name"].as_str().unwrap_or(""));
    let options = if audio {
        json!({"file":f["url"],"volume":1,"loop":false,"fadeOnLeave":true})
    } else {
        json!({"file":f["url"],"sound":true,"volume":1,"loop":false,"endAction":"hold"})
    };
    wp_core::settings::merge_into(&mut it, &options);
    it
}

impl App {
    pub(crate) fn media_panel(&mut self, ui: &mut egui::Ui) {
        // 기존 video_bar는 music_bar도 포함한다. 탭과 관계없이 현재 두 재생 상태를 유지한다.
        self.video_bar(ui);
        let panel_id = ui.id().with("media");
        let tab_id = panel_id.with("audio_tab");
        let mut audio = ui.ctx().data_mut(|d| d.get_temp::<bool>(tab_id).unwrap_or(false));
        ui.horizontal(|ui| {
            ui.selectable_value(&mut audio, false, tr("영상"));
            ui.selectable_value(&mut audio, true, tr("음악"));
        });
        ui.ctx().data_mut(|d| d.insert_temp(tab_id, audio));
        let kind = if audio { "audio" } else { "image" };
        // 검색어와 선택 폴더는 탭별로 보관한다.
        let search_id = panel_id.with((kind, "search"));
        let folder_id = panel_id.with((kind, "folder"));
        let window_id = panel_id.with((kind, "folders_open"));
        let mut q = ui.ctx().data_mut(|d| d.get_temp::<String>(search_id).unwrap_or_default());
        let mut folder = ui.ctx().data_mut(|d| d.get_temp::<String>(folder_id).unwrap_or_default());
        let mut folders_open = ui.ctx().data_mut(|d| d.get_temp::<bool>(window_id).unwrap_or(false));
        let folders = wp_core::library::folders(self.core.lock().settings.get(), kind);
        if !folder.is_empty() && !folders.iter().any(|f| f["path"].as_str() == Some(folder.as_str())) { folder.clear(); }
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut q).hint_text(tr("파일 이름으로 찾기")));
            if ui.button(tr("폴더…")).clicked() { folders_open = true; }
        });
        let folder_label = folders.iter().find(|f| f["path"].as_str() == Some(folder.as_str()))
            .and_then(|f| f["name"].as_str()).map(String::from).unwrap_or_else(|| tr("모든 폴더"));
        egui::ComboBox::from_id_salt(folder_id).selected_text(folder_label).show_ui(ui, |ui| {
            ui.selectable_value(&mut folder, String::new(), tr("모든 폴더"));
            for f in &folders {
                ui.selectable_value(&mut folder, f["path"].as_str().unwrap_or("").to_owned(), f["name"].as_str().unwrap_or(""));
            }
        });
        // 기존 폴더 등록/제거 UI를 선택한 미디어 종류로 연다. 데이터 변경은 그 UI에서만 한다.
        crate::widgets::close_on_outside(ui.ctx(), window_id, &mut folders_open);
        egui::Window::new(tr("라이브러리 폴더")).id(window_id).default_pos(crate::widgets::float_pos(ui.ctx(), window_id, 600.0)).open(&mut folders_open)
            .default_size([600.0, 440.0]).collapsible(false).show(ui.ctx(), |ui| self.library_kind_panel(ui, kind));
        ui.ctx().data_mut(|d| {
            d.insert_temp(search_id, q.clone());
            d.insert_temp(folder_id, folder.clone());
            d.insert_temp(window_id, folders_open);
        });
        let files: Vec<Value> = self.core.lock().search_lib(kind, &q, &folder).into_iter().filter(|f| matches_tab(f, audio)).collect();
        egui::ScrollArea::vertical().id_salt(("media_files", kind)).show(ui, |ui| {
            for f in &files {
                ui.horizontal(|ui| {
                    let play = ui.button(if audio { tr("▶ 재생") } else { tr("▶ 송출") }).clicked();
                    let row = ui.selectable_label(false, f["name"].as_str().unwrap_or(""));
                    // 줄을 두 번 눌러도 재생·송출은 한 번만 (두 번째 클릭이 다시 처음부터 틀지 않게)
                    if play || crate::widgets::click2(ui, ("media-row", audio, f["url"].as_str().unwrap_or("")), &row) == crate::widgets::Click::Single {
                        if audio {
                            crate::music_ui::play_now(f["url"].as_str().unwrap_or(""), f["name"].as_str().unwrap_or(""));
                        } else { self.core.lock().send_deck(video_deck(f)); }
                    }
                    ui.weak(f["folder"].as_str().unwrap_or(""));
                    if ui.button(tr("큐시트에")).clicked() { self.add_lib_item(media_item(f, audio)); }
                });
            }
            if files.is_empty() {
                ui.label(if !q.is_empty() { tr("찾는 파일이 없습니다.") } else if audio { tr("라이브러리에 음악이 없습니다.") } else { tr("라이브러리에 영상이 없습니다.") });
            }
        });
    }
}
/// 큐 편집기의 songId/order 또는 라이브러리의 id/arrangement에 적용한다.
/// 순서가 바뀌면 true. 저장/삭제는 기존 songs API만 사용한다.
pub fn song_arrangements(ui: &mut egui::Ui, core: &mut Core, song: &mut Value) -> bool {
    let cue = song.get("songId").is_some();
    let id = song[if cue {"songId"} else {"id"}].as_str().unwrap_or("").to_owned();
    if id.is_empty() { return false; }
    let key = if cue {"order"} else {"arrangement"};
    let order = song[key].as_str().unwrap_or("").to_owned();
    let Some(saved) = core.get_song(&id) else { return false };
    let mut changed = false;
    ui.push_id((&id, "arrangements"), |ui| {
        ui.label(tr("저장한 순서"));
        for a in saved["arrangements"].as_array().into_iter().flatten() {
            let name = a["name"].as_str().unwrap_or("");
            let value = a["order"].as_str().unwrap_or("");
            ui.horizontal(|ui| {
                if ui.selectable_label(order == value, name).on_hover_text(value).clicked() {
                    song[key] = json!(value); changed = true;
                }
                if ui.small_button(tr("지우기")).clicked() {
                    match core.save_song_arrangement(&id, name, "") {
                        Ok(s) => song["arrangements"] = s["arrangements"].clone(),
                        Err(e) => core.notify(e.to_string(), "warn"),
                    }
                }
            });
        }
        let name_id = ui.id().with("arrangement_name");
        let mut name = ui.ctx().data_mut(|d| d.get_temp::<String>(name_id).unwrap_or_default());
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut name).hint_text(tr("부를 순서 이름")));
            if ui.add_enabled(!name.trim().is_empty() && !order.trim().is_empty(), egui::Button::new(tr("이 부를 순서 이름 붙여 저장"))).clicked() {
                match core.save_song_arrangement(&id, name.trim(), &order) {
                    Ok(s) => song["arrangements"] = s["arrangements"].clone(),
                    Err(e) => core.notify(e.to_string(), "warn"),
                }
            }
        });
        ui.ctx().data_mut(|d| d.insert_temp(name_id, name));
    });
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn music_tab_uses_audio_extension_not_video_flag() {
        assert!(matches_tab(&json!({"path":"C:/media/praise.MP3"}), true));
        assert!(!matches_tab(&json!({"path":"C:/media/movie.mp4","video":true}), true));
        assert!(!matches_tab(&json!({"path":"C:/media/image.png","video":false}), true));
        assert!(matches_tab(&json!({"path":"C:/media/movie.mp4","video":true}), false));
    }
    #[test]
    fn music_queue_item_keeps_audio_contract() {
        let f = json!({"name":"찬양", "url":"/lib/file?p=praise.mp3"});
        let it = media_item(&f, true);
        assert_eq!(it["type"], "audio");
        assert_eq!(it["file"], f["url"]);
        assert_eq!(it["volume"], 1);
        assert_eq!(it["fadeOnLeave"], true);
        assert_eq!(it["loop"], false);
        assert!(it.get("image").is_none());
    }
    #[test]
    fn duration_falls_back_without_ffprobe_and_prefers_probe() {
        assert_eq!(video_duration(&egui::Context::default(), "/media/no-probe.mp4", Some(125.5)), Some(125.5));
        assert_eq!(preferred_video_duration(Some(120.0), Some(125.5)), Some(120.0));
        for invalid in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert_eq!(preferred_video_duration(None, Some(invalid)), None);
        }
        assert_eq!(preferred_video_duration(None, None), None);
    }
    #[test]
    fn direct_video_has_no_cue_binding() {
        let d = video_deck(&json!({"name":"영상","url":"/lib/file?p=test.mp4"}));
        assert_eq!(d.passage["cueId"], "");
        assert_eq!(d.passage["cueIndex"], -1);
        assert_eq!(d.slides[0]["video"]["sound"], true);
    }
}

/// ffprobe 결과를 우선하고, 없거나 조회 중이면 출력 화면이 확인한 길이를 쓴다.
pub fn video_duration(ctx: &egui::Context, url: &str, reported: Option<f64>) -> Option<f64> {
    preferred_video_duration(probe_video_duration(ctx, url), reported)
}
fn preferred_video_duration(probed: Option<f64>, reported: Option<f64>) -> Option<f64> {
    probed.filter(|d| d.is_finite() && *d > 0.0)
        .or_else(|| reported.filter(|d| d.is_finite() && *d > 0.0))
}
/// 파일마다 한 번만 백그라운드 메타데이터를 조회한다.
fn probe_video_duration(ctx: &egui::Context, url: &str) -> Option<f64> {
    use once_cell::sync::Lazy;
    use parking_lot::Mutex;
    use std::collections::HashMap;
    static LENGTHS: Lazy<Mutex<HashMap<String, Option<f64>>>> = Lazy::new(|| Mutex::new(HashMap::new()));
    let mut lengths = LENGTHS.lock();
    if let Some(d) = lengths.get(url) { return *d; }
    lengths.insert(url.into(), None);
    let Some(encoded) = url.strip_prefix("/lib/file?p=") else { return None };
    let mut bytes = Vec::new(); let mut input = encoded.as_bytes().iter().copied();
    while let Some(b) = input.next() {
        if b == b'%' {
            let (Some(a), Some(b)) = (input.next(), input.next()) else { return None };
            let (Some(a), Some(b)) = ((a as char).to_digit(16), (b as char).to_digit(16)) else { return None };
            bytes.push((a * 16 + b) as u8);
        } else { bytes.push(b); }
    }
    let Ok(path) = String::from_utf8(bytes) else { return None };
    let key = url.to_owned(); let ctx = ctx.clone();
    std::thread::spawn(move || {
        let Some(ffmpeg) = wp_core::thumbs::ffmpeg() else { return };
        let probe = ffmpeg.with_file_name(if cfg!(windows) {"ffprobe.exe"} else {"ffprobe"});
        let mut cmd = std::process::Command::new(probe);
        cmd.args(["-v", "error", "-show_entries", "format=duration", "-of", "default=noprint_wrappers=1:nokey=1"]).arg(path);
        #[cfg(windows)] { use std::os::windows::process::CommandExt; cmd.creation_flags(0x08000000); }
        if let Ok(out) = cmd.output() {
            if out.status.success() {
                let d = String::from_utf8_lossy(&out.stdout).trim().parse::<f64>().ok().filter(|d| d.is_finite() && *d > 0.0);
                LENGTHS.lock().insert(key, d); ctx.request_repaint();
            }
        }
    });
    None
}


