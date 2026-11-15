//! [트랙 B] 웹에서 가사 찾기 (Ctrl+L): 벅스·지니·멜론에서 찾고 → 고른 곡의 가사를 [1절]·[후렴] 으로 정리 →
//! 고른 찬양 순서에 넣기 · 찬양 라이브러리에 저장 · 새 찬양 순서로. 네트워크는 백그라운드 스레드에서.

use crate::app::App;
use eframe::egui::{self, Color32, RichText};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde_json::{json, Value};

#[derive(Default)]
pub struct LyricsState {
    pub open: bool,
    query: String,
    busy: String,
    results: Vec<Value>,
    errors: Vec<String>,
    picked: Option<Value>,
    lyrics: String,
    order: String,
    lines: usize,
    note: String,
}

pub static ST: Lazy<Mutex<LyricsState>> = Lazy::new(|| Mutex::new(LyricsState { lines: 2, ..Default::default() }));

pub fn open() {
    ST.lock().open = true;
}

fn run_search(ctx: egui::Context, q: String) {
    {
        let mut s = ST.lock();
        s.busy = format!("'{q}' 찾는 중…");
        s.results.clear();
        s.errors.clear();
    }
    std::thread::spawn(move || {
        let r = wp_core::lyricsweb::search(&q);
        let mut s = ST.lock();
        s.results = r["results"].as_array().cloned().unwrap_or_default();
        s.errors = r["errors"].as_array().into_iter().flatten().filter_map(|e| e.as_str().map(String::from)).collect();
        s.busy = if s.results.is_empty() { "찾은 곡이 없습니다.".into() } else { String::new() };
        ctx.request_repaint();
    });
}

fn run_lyrics(ctx: egui::Context, item: Value) {
    {
        let mut s = ST.lock();
        s.busy = format!("'{}' 가사 읽는 중…", item["title"].as_str().unwrap_or(""));
        s.picked = Some(item.clone());
    }
    std::thread::spawn(move || {
        let r = wp_core::lyricsweb::lyrics(item["site"].as_str().unwrap_or(""), item["id"].as_str().unwrap_or(""));
        let mut s = ST.lock();
        match r {
            Ok(t) if !t.is_empty() => {
                let (l, o) = wp_core::lyricsweb::arrange(&t, s.lines);
                s.lyrics = l;
                s.order = o;
                s.busy.clear();
            }
            Ok(_) => s.busy = format!("{} 에 이 곡의 가사가 없습니다. 다른 결과를 고르세요.", item["siteName"].as_str().unwrap_or("")),
            Err(e) => s.busy = format!("가사를 읽지 못했습니다 ({e}). 사이트에서 복사해 붙여 넣으세요."),
        }
        ctx.request_repaint();
    });
}

/// 곡 제목에서 (Feat. …) 같은 꼬리를 뗀다
fn clean_title(t: &str) -> String {
    let t = t.split(" (Feat").next().unwrap_or(t);
    t.split(" (feat").next().unwrap_or(t).trim().to_string()
}

impl App {
    pub(crate) fn lyrics_window(&mut self, ctx: &egui::Context) {
        let mut open = ST.lock().open;
        if !open {
            return;
        }
        let sel_song = self.sel.and_then(|i| self.core.lock().cue.as_ref().and_then(|c| c.items.get(i).cloned())).filter(|it| it["type"] == "song");
        crate::widgets::float_window(ctx, "lyrics", wp_core::i18n::tr("웹에서 가사 찾기 (Ctrl+L)"), [560.0, 620.0]).open(&mut open).show(ctx, |ui| {
            let mut st = ST.lock();
            let mut search = false;
            ui.horizontal(|ui| {
                let r = ui.add(egui::TextEdit::singleline(&mut st.query).hint_text(wp_core::i18n::tr("곡 제목 (가수 이름을 붙이면 더 정확)")).desired_width(330.0));
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) { search = true; }
                if ui.button(wp_core::i18n::tr("찾기")).clicked() { search = true; }
                ui.label(RichText::new(wp_core::i18n::tr("벅스·지니·멜론")).small().color(Color32::GRAY));
            });
            if search && !st.query.trim().is_empty() {
                let q = st.query.trim().to_string();
                drop(st);
                run_search(ctx.clone(), q);
                st = ST.lock();
            }
            if !st.busy.is_empty() { ui.label(RichText::new(&st.busy).color(Color32::from_rgb(0xff, 0xb0, 0x4a))); }
            for e in &st.errors { ui.label(RichText::new(wp_core::i18n::tr_args("못 읽음 · {1}", &[&e])).small().color(Color32::GRAY)); }
            let mut pick: Option<Value> = None;
            egui::ScrollArea::vertical().id_salt("lyr-res").max_height(180.0).show(ui, |ui| {
                for x in &st.results {
                    let on = st.picked.as_ref().is_some_and(|p| p["site"] == x["site"] && p["id"] == x["id"]);
                    let t = format!("[{}] {} — {}  · {}", x["siteName"].as_str().unwrap_or(""), x["title"].as_str().unwrap_or(""), x["artist"].as_str().unwrap_or(""), x["album"].as_str().unwrap_or(""));
                    if ui.selectable_label(on, t).clicked() { pick = Some(x.clone()); }
                }
            });
            if let Some(p) = pick {
                drop(st);
                run_lyrics(ctx.clone(), p);
                st = ST.lock();
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.label(wp_core::i18n::tr("슬라이드마다"));
                let mut n = st.lines;
                if ui.add(egui::DragValue::new(&mut n).range(1..=6).suffix(wp_core::i18n::tr("줄"))).changed() {
                    st.lines = n;
                    // 이름표가 없는 원문일 때만 다시 나눔 (이미 정리한 글은 그대로)
                }
                ui.label(wp_core::i18n::tr("부를 순서"));
                ui.add(egui::TextEdit::singleline(&mut st.order).desired_width(240.0).hint_text(wp_core::i18n::tr("비우면 가사 순서대로")));
            });
            ui.add(egui::TextEdit::multiline(&mut st.lyrics).desired_rows(14).desired_width(f32::INFINITY).hint_text(wp_core::i18n::tr("곡을 고르면 가사가 [1절]·[후렴] 으로 정리되어 여기 나옵니다. 바로 고쳐도 됩니다.")));
            let has = !st.lyrics.trim().is_empty();
            let title = st.picked.as_ref().map(|p| clean_title(p["title"].as_str().unwrap_or(""))).unwrap_or_default();
            let artist = st.picked.as_ref().map(|p| p["artist"].as_str().unwrap_or("").to_string()).unwrap_or_default();
            let (lyrics, order) = (st.lyrics.clone(), st.order.clone());
            drop(st);
            ui.horizontal(|ui| {
                let label = if sel_song.is_some() { wp_core::i18n::tr_s("고른 찬양 순서에 넣기") } else { wp_core::i18n::tr_s("새 찬양 순서로 넣기") };
                if ui.add_enabled(has, egui::Button::new(label)).clicked() {
                    let mut c = self.core.lock();
                    c.remember_items();
                    match (self.sel, &sel_song) {
                        (Some(i), Some(_)) => {
                            if let Some(cue) = &mut c.cue {
                                if let Some(it) = cue.items.get_mut(i) {
                                    it["lyrics"] = json!(lyrics);
                                    it["order"] = json!(order);
                                    if it["title"].as_str().is_none_or(|t| t.is_empty() || t == "찬양") { it["title"] = json!(title); }
                                    if it["credit"].as_str().is_none_or(|t| t.is_empty()) && !artist.is_empty() { it["credit"] = json!(artist); }
                                }
                            }
                            c.save_cue();
                            drop(c);
                            self.edit = None;
                            ST.lock().note = "고른 찬양 순서에 가사를 넣었습니다.".into();
                        }
                        _ => {
                            drop(c);
                            let mut it = wp_core::core::new_item("song", &title);
                            it["lyrics"] = json!(lyrics);
                            it["order"] = json!(order);
                            it["credit"] = json!(artist);
                            self.add_song_item(it);
                            ST.lock().note = "새 찬양 순서로 넣었습니다.".into();
                        }
                    }
                }
                if ui.add_enabled(has, egui::Button::new(wp_core::i18n::tr("찬양 라이브러리에 저장"))).clicked() {
                    let r = self.core.lock().save_song(&json!({ "title": title, "lyrics": lyrics, "credit": artist, "arrangement": order }));
                    ST.lock().note = match r { Ok(_) => format!("'{title}' 을(를) 찬양 라이브러리에 저장했습니다."), Err(e) => e.to_string() };
                }
            });
            let note = ST.lock().note.clone();
            if !note.is_empty() { ui.label(RichText::new(note).small().color(Color32::from_rgb(0x8a, 0xe0, 0xa8))); }
            ui.label(RichText::new(wp_core::i18n::tr("고른 곡 하나씩만 읽습니다. 사이트 구조가 바뀌어 못 읽으면 가사를 복사해 붙여 넣으세요.")).small().color(Color32::GRAY));
        });
        ST.lock().open = open;
    }

    fn add_song_item(&mut self, it: Value) {
        // 넣고 곧바로 미리보기(바로 송출이면 송출) · 같은 것이 이미 있으면 그 순서를 고름 · 연타 무시 (wp_core::pick)
        let at = self.sel.map(|s| s + 1);
        let r = self.core.lock().add_picked_item(it, at, false);
        crate::app::picked_sel(&mut self.sel, &mut self.edit, r);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn title() {
        assert_eq!(super::clean_title("은혜 (Feat. 지선)"), "은혜");
        assert_eq!(super::clean_title("주 하나님 지으신 모든 세계"), "주 하나님 지으신 모든 세계");
    }
}
