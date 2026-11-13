//! 미디어 직접 송출과 저장한 찬양 순서 위젯.
use crate::app::App;
use eframe::egui;
use serde_json::{json, Value};
use wp_core::{core::Core, presentation::Deck, i18n::tr};

pub fn video_deck(f: &Value) -> Deck {
    Deck { passage: json!({"title":f["name"],"itemTitle":f["name"],"itemType":"video","cueIndex":-1,"cueId":""}),
        slides: vec![json!({"kind":"video","image":f["url"],"ref":f["name"],"label":tr("영상"),"parts":[],"caption":"","video":{"volume":1,"sound":true,"loop":false,"endAction":"hold"}})], index:0 }
}
impl App {
    pub(crate) fn media_panel(&mut self, ui: &mut egui::Ui) {
        self.video_bar(ui);
        let id = ui.id().with("media_search");
        let mut q = ui.ctx().data_mut(|d| d.get_temp::<String>(id).unwrap_or_default());
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut q).hint_text(tr("파일 이름으로 찾기")));
            if ui.button(tr("폴더…")).clicked() { self.tb.lib = true; }
        });
        ui.ctx().data_mut(|d| d.insert_temp(id, q.clone()));
        let files: Vec<Value> = self.core.lock().search_lib("image", &q, "").into_iter().filter(|f| f["video"] == true).collect();
        egui::ScrollArea::vertical().id_salt("media_videos").show(ui, |ui| {
            for f in &files {
                ui.horizontal(|ui| {
                    if ui.button(tr("▶ 송출")).clicked() { self.core.lock().send_deck(video_deck(f)); }
                    if ui.selectable_label(false, f["name"].as_str().unwrap_or("")).clicked() { self.core.lock().send_deck(video_deck(f)); }
                    if ui.button(tr("큐시트에")).clicked() {
                        let mut it = wp_core::core::new_item("video", f["name"].as_str().unwrap_or(""));
                        wp_core::settings::merge_into(&mut it, &json!({"file":f["url"],"sound":true,"volume":1,"loop":false,"endAction":"hold"}));
                        self.add_lib_item(it);
                    }
                });
            }
            if files.is_empty() { ui.label(tr("라이브러리에 영상이 없습니다.")); }
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
    fn direct_video_has_no_cue_binding() {
        let d = video_deck(&json!({"name":"영상","url":"/lib/file?p=test.mp4"}));
        assert_eq!(d.passage["cueId"], "");
        assert_eq!(d.passage["cueIndex"], -1);
        assert_eq!(d.slides[0]["video"]["sound"], true);
    }
}

/// 파일마다 한 번만 백그라운드 메타데이터 조회. 도구가 없으면 길이를 추측하지 않는다.
pub fn video_duration(ctx: &egui::Context, url: &str) -> Option<f64> {
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


