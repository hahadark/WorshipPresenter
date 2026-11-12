//! [트랙 B] 코어에 붙는 상태와 명령: 찬양 · 라이브러리 · 영상 · 오버레이 · 메시지 · 무대 · 매크로 · 큐시트 관리

use crate::core::{Core, Ev};
use crate::cues::{self, Undo};
use crate::library::{self, Video};
use crate::overlays::{self, Message, Stage};
use crate::songs;
use crate::util::now_ms;
use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct TrackB {
    pub stage: Stage,
    pub message: Option<Message>,
    pub saved_overlays: Vec<String>,
    pub video: Option<Video>,
    pub undo: Undo,
    pub fired: Vec<String>,
    /// 영상 키를 만든 LIVE version (같은 슬라이드면 이어서)
    pub video_for: Option<(u64, usize)>,
    /// 편집칸 글 고치기 묶음 (실행 취소: 같은 순서를 이어서 치면 한 번에)
    pub typing: Option<(String, std::time::Instant)>,
    /// 여러 PC 동기화 상태
    pub sync: crate::sync::SyncStatus,
    /// 화면에 그리기 · 강조
    pub draw: crate::draw::Draw,
}

impl Core {
    /// 열 때 한 번 (Songs 테이블)
    pub fn trackb_init(&mut self) {
        if let Err(e) = songs::ensure_schema(&self.store.db) {
            log::error!("Songs 테이블: {e}");
        }
    }

    fn changed(&mut self, live: bool) {
        if live {
            self.emit(Ev::Live);
        }
        self.emit(Ev::Ui);
    }

    // ---------------- 찬양 ----------------
    pub fn list_songs(&self, q: &str) -> Vec<Value> {
        songs::list_songs(&self.store.db, q)
    }
    pub fn get_song(&self, id: &str) -> Option<Value> {
        songs::get_song(&self.store.db, id)
    }
    pub fn save_song(&mut self, s: &Value) -> Result<Value> {
        let r = songs::save_song(&self.store.db, s)?;
        self.emit(Ev::Ui);
        Ok(r)
    }
    pub fn delete_song(&mut self, id: &str) -> Result<()> {
        songs::delete_song(&self.store.db, id)?;
        self.emit(Ev::Ui);
        Ok(())
    }
    pub fn save_song_arrangement(&mut self, id: &str, name: &str, order: &str) -> Result<Value> {
        songs::save_arrangement(&self.store.db, id, name, order)
    }
    pub fn import_song_text(&mut self, text: &str, lines: usize) -> Result<Value> {
        let list = crate::songimport::parse_song_text(text, "", lines); // 되풀이를 찾아 [1절]·[후렴] (Electron 과 같게)
        if list.is_empty() {
            bail!("가사를 찾지 못했습니다. 첫 줄에 제목, 빈 줄 다음에 가사를 붙여 넣으세요 ('# 제목' 줄로 여러 곡).");
        }
        songs::save_imported(&self.store.db, &list)
    }
    /// 찬양 파일들 가져오기 (OpenLyrics · ChordPro · ProPresenter 6 · PPT · txt) → { saved, skipped, errors }
    pub fn import_song_files(&mut self, files: &[PathBuf]) -> Result<Value> {
        let (list, errors) = crate::songimport::parse_song_files(files);
        let mut r = if list.is_empty() { json!({ "saved": 0, "skipped": 0 }) } else { songs::save_imported(&self.store.db, &list)? };
        r["errors"] = json!(errors);
        self.emit(Ev::Ui);
        Ok(r)
    }
    /// 찬양 → 큐시트 순서 (songId 로 이어 둔다)
    pub fn song_item(&self, id: &str) -> Option<Value> {
        let s = self.get_song(id)?;
        let mut it = crate::core::new_item("song", s["title"].as_str().unwrap_or("찬양"));
        it["songId"] = json!(id);
        it["lyrics"] = s["lyrics"].clone();
        it["credit"] = s["credit"].clone();
        it["order"] = s["arrangement"].clone();
        Some(it)
    }

    // ---------------- 라이브러리 ----------------
    pub fn add_lib_folder(&mut self, kind: &str, dir: &Path) -> Result<Value> {
        let mut out = Ok(Value::Null);
        self.settings.update(|d| out = library::add_folder(d, kind, dir, None));
        self.emit(Ev::Ui);
        out
    }
    pub fn remove_lib_folder(&mut self, kind: &str, dir: &str) {
        self.settings.update(|d| library::remove_folder(d, kind, dir));
        self.emit(Ev::Ui);
    }
    pub fn search_lib(&self, kind: &str, q: &str, folder: &str) -> Vec<Value> {
        library::search_files(self.settings.get(), kind, q, folder)
    }
    pub fn add_dropped_files(&mut self, paths: &[PathBuf]) -> Vec<Value> {
        let mut items = vec![];
        self.settings.update(|d| items = library::add_dropped(d, paths));
        items
    }
    /// 배경 미디어 (그림·영상·카메라). None 이면 끔
    pub fn set_background_media(&mut self, m: Option<Value>) {
        self.layers.media = m;
        self.changed(true);
    }

    // ---------------- 영상 ----------------
    /// LIVE 슬라이드가 영상이면 재생 상태를 맞춘다 (출력 데이터 만들 때 부름)
    pub fn sync_video_state(&mut self) {
        let cur = self.pres.live.as_ref().and_then(|l| {
            let s = l.deck.slides.get(l.deck.index)?;
            (s["kind"] == "video").then(|| (l.sent_at as u64, l.deck.index, s.clone()))
        });
        match cur {
            Some((sent, idx, s)) => {
                if self.b.video_for != Some((sent, idx)) {
                    let mut v = Video::start(&format!("{}#{}", s["image"].as_str().unwrap_or(""), now_ms()));
                    v.volume = s["video"]["volume"].as_f64();
                    v.looped = s["video"]["loop"].as_bool();
                    self.b.video = Some(v);
                    self.b.video_for = Some((sent, idx));
                }
            }
            None => {
                self.b.video = None;
                self.b.video_for = None;
            }
        }
    }
    pub fn video_payload(&self) -> Value {
        self.b.video.as_ref().map(|v| v.payload()).unwrap_or(Value::Null)
    }
    pub fn video_control(&mut self, act: &str, arg: f64) -> Result<()> {
        let Some(v) = self.b.video.as_mut() else { bail!("송출 중인 영상이 없습니다.") };
        v.control(act, arg)?;
        self.changed(true);
        Ok(())
    }
    /// 출력 화면이 알리는 '영상 끝남' → 끝나면(endAction): hold 그대로 · black 글자 숨김 · next 다음 슬라이드
    pub fn video_ended(&mut self, key: &str) {
        let Some(v) = self.b.video.as_mut() else { return };
        if v.key != key || v.ended {
            return;
        }
        v.ended = true;
        v.playing = false;
        let act = self.pres.live.as_ref().and_then(|l| l.deck.slides.get(l.deck.index)).and_then(|s| s["video"]["endAction"].as_str().map(String::from)).unwrap_or_default();
        match act.as_str() {
            "black" => self.pres.clear(),
            "next" => {
                self.step(1);
                return;
            }
            _ => {}
        }
        self.changed(true);
    }

    /// 출력에 겹칠 오버레이: 켜진 것 + 회중 메시지
    pub fn overlays_for(&self, output: &str) -> Vec<Value> {
        let mut o: Vec<Value> = self.overlays().into_iter().filter(|x| x["on"] == true && x["outputs"].as_array().is_some_and(|a| a.iter().any(|y| y == output))).collect();
        o.extend(overlays::message_overlay(&self.b.message, output));
        o
    }

    // ---------------- 오버레이 ----------------
    pub fn overlays(&self) -> Vec<Value> {
        self.settings.get()["overlays"].as_array().cloned().unwrap_or_default()
    }
    pub fn save_overlay(&mut self, o: &Value) -> Result<Value> {
        let mut out = Ok(Value::Null);
        self.settings.update(|d| out = overlays::save_overlay(d, o));
        self.changed(true);
        out
    }
    pub fn delete_overlay(&mut self, id: &str) {
        self.settings.update(|d| overlays::delete_overlay(d, id));
        self.changed(true);
    }
    pub fn toggle_overlay(&mut self, id: &str, on: Option<bool>) -> Result<Value> {
        let mut out = Ok(Value::Null);
        self.settings.update(|d| out = overlays::toggle_overlay(d, id, on));
        self.changed(true);
        out
    }
    pub fn toggle_overlays_off(&mut self) {
        let mut saved = std::mem::take(&mut self.b.saved_overlays);
        self.settings.update(|d| overlays::toggle_all_off(d, &mut saved));
        self.b.saved_overlays = saved;
        self.changed(true);
    }

    // ---------------- 회중 메시지 ----------------
    pub fn show_message(&mut self, text: &str, outputs: &[String], seconds: i64) -> Result<()> {
        self.b.message = Some(overlays::make_message(text, outputs, seconds, 0.0)?);
        self.changed(true);
        Ok(())
    }
    pub fn clear_message(&mut self) {
        self.b.message = None;
        self.changed(true);
    }
    pub fn save_message_template(&mut self, t: &Value) -> Result<String> {
        let mut out = Ok(String::new());
        self.settings.update(|d| out = overlays::save_message_template(d, t));
        out
    }
    pub fn delete_message_template(&mut self, id: &str) {
        self.settings.update(|d| {
            if let Some(l) = d["messageTemplates"].as_array_mut() {
                l.retain(|x| x["id"] != id)
            }
        });
    }

    // ---------------- 무대 화면 ----------------
    pub fn stage_timer(&mut self, act: &str, minutes: f64) -> Result<()> {
        self.b.stage.timer_action(act, minutes)?;
        self.changed(true);
        Ok(())
    }
    pub fn stage_message(&mut self, text: &str) {
        self.b.stage.message = text.chars().take(300).collect();
        self.changed(true);
    }
    pub fn stage_pin_set(&mut self, p: Option<&Value>) {
        self.b.stage.set_pin(p);
        self.changed(true);
    }
    pub fn save_stage_preset(&mut self, p: &Value) -> Result<String> {
        let mut out = Ok(String::new());
        self.settings.update(|d| out = overlays::save_stage_preset(d, p));
        out
    }
    /// stagePayload 의 pin: 순서표면 지금 큐시트의 순서들
    pub fn stage_pin_json(&self) -> Value {
        let Some(p) = &self.b.stage.pin else { return Value::Null };
        if p["mode"] == "text" {
            return json!({ "mode": "text", "name": p["name"], "text": p["text"] });
        }
        let cur = self.pres.live.as_ref().filter(|l| self.cue.as_ref().is_some_and(|c| l.deck.passage["cueId"] == c.id.as_str())).and_then(|l| l.deck.passage["cueIndex"].as_i64()).unwrap_or(-1);
        let items: Vec<Value> = self.cue.as_ref().map(|c| c.items.iter().map(|it| json!({ "title": it["title"], "type": it["type"], "note": it["note"].as_str().unwrap_or("") })).collect()).unwrap_or_default();
        json!({ "mode": "order", "name": p["name"], "cueName": self.cue.as_ref().map(|c| c.name.clone()).unwrap_or_default(), "current": cur, "items": items })
    }

    // ---------------- 매크로 · 예약 ----------------
    pub fn save_macro(&mut self, m: &Value) -> Result<String> {
        let mut out = Ok(String::new());
        self.settings.update(|d| out = overlays::save_macro(d, m));
        self.emit(Ev::Ui);
        out
    }
    pub fn delete_macro(&mut self, id: &str) {
        self.settings.update(|d| overlays::delete_macro(d, id));
        self.emit(Ev::Ui);
    }
    /// 매크로 실행 (이름 또는 id). 자동 동작(item.actions)도 같은 틀로
    pub fn run_macro(&mut self, key: &str) -> Result<()> {
        let m = self.settings.get()["macros"].as_array().and_then(|l| l.iter().find(|x| x["id"] == key || x["name"] == key).cloned());
        let Some(m) = m else { bail!("매크로를 찾을 수 없습니다: {key}") };
        self.run_actions(&m["actions"])?;
        self.notify(format!("매크로 실행: {}", m["name"].as_str().unwrap_or("")), "ok");
        Ok(())
    }
    pub fn run_actions(&mut self, a: &Value) -> Result<()> {
        let flag = |k: &str| a[k].as_bool().unwrap_or(false);
        if flag("clearAll") {
            self.clear_all();
        }
        if flag("hideText") {
            self.pres.clear();
        }
        if let Some(b) = a["logo"].as_bool() {
            self.layers.logo = b;
        }
        if let Some(b) = a["bgOff"].as_bool() {
            self.layers.hide_bg = b;
        }
        if flag("overlaysOff") {
            self.toggle_overlays_off();
        }
        for (k, on) in [("overlayOn", true), ("overlayOff", false)] {
            for id in a[k].as_array().into_iter().flatten().filter_map(|x| x.as_str()) {
                let _ = self.toggle_overlay(id, Some(on));
            }
        }
        if let Some(t) = a["message"].as_str().filter(|x| !x.is_empty()) {
            self.show_message(t, &["main".into(), "overlay".into()], a["messageSeconds"].as_i64().unwrap_or(30))?;
        }
        if let Some(t) = a["stageMessage"].as_str() {
            self.stage_message(t);
        }
        if let Some(m) = a["timer"].as_f64().filter(|x| *x > 0.0) {
            self.stage_timer("start", m)?;
        }
        if let Some(i) = a["cueItem"].as_u64() {
            if let Ok(d) = self.item_deck(i as usize) {
                self.pres.preview = Some(d.clone());
                self.send_deck(d);
            }
        }
        if let Some(sm) = a["stagePin"].as_object() {
            self.stage_pin_set(Some(&Value::Object(sm.clone())));
        }
        self.changed(true);
        Ok(())
    }
    /// 30초마다 부름: 예약 실행
    pub fn check_schedule(&mut self, weekday: u32, hhmm: &str, day_key: &str) {
        let sc = self.settings.get()["schedule"].clone();
        let mut fired = std::mem::take(&mut self.b.fired);
        let due = overlays::due_schedules(&sc, weekday, hhmm, &mut fired, day_key);
        self.b.fired = fired;
        for m in due {
            if let Err(e) = self.run_macro(&m) {
                self.notify(format!("예약 실행 실패: {e}"), "warn");
            }
        }
    }

    // ---------------- 큐시트 관리 ----------------
    fn refresh_cues(&mut self) {
        self.cues = self.store.list_cues();
        if let Some(id) = self.cue.as_ref().map(|c| c.id.clone()) {
            self.cue = self.store.get_cue(&id);
        }
        self.emit(Ev::Ui);
    }
    pub fn trash_list(&self) -> Vec<Value> {
        cues::list_trash(&self.store)
    }
    pub fn trash_cue(&mut self, id: &str) -> Result<()> {
        cues::trash(&self.store, id)?;
        if self.cue.as_ref().is_some_and(|c| c.id == id) {
            self.cue = None;
        }
        self.refresh_cues();
        Ok(())
    }
    pub fn restore_cue(&mut self, id: &str) -> Result<()> {
        cues::restore(&self.store, id)?;
        self.refresh_cues();
        Ok(())
    }
    pub fn purge_cues(&mut self, id: Option<&str>) -> Result<usize> {
        let n = cues::purge(&self.store, id)?;
        self.refresh_cues();
        Ok(n)
    }
    pub fn duplicate_cue(&mut self, id: &str) -> Result<()> {
        let n = cues::duplicate(&self.store, id, None)?;
        self.cues = self.store.list_cues();
        self.open_cue(&n.id);
        Ok(())
    }
    pub fn update_cue_meta(&mut self, m: &Value) -> Result<()> {
        let Some(id) = self.cue.as_ref().map(|c| c.id.clone()) else { bail!("열린 큐시트가 없습니다.") };
        cues::update_meta(&self.store, &id, m)?;
        self.refresh_cues();
        Ok(())
    }
    pub fn export_cue_file(&self, path: &Path) -> Result<()> {
        let Some(c) = &self.cue else { bail!("열린 큐시트가 없습니다.") };
        std::fs::write(path, serde_json::to_string_pretty(&cues::export(&self.store, &[c.id.clone()]))?)?;
        Ok(())
    }
    pub fn import_cue_file(&mut self, path: &Path) -> Result<usize> {
        let txt = std::fs::read_to_string(path)?;
        let n = cues::import(&self.store, &serde_json::from_str(txt.trim_start_matches('\u{feff}'))?)?;
        self.refresh_cues();
        Ok(n)
    }
    pub fn print_cue_file(&self) -> Result<PathBuf> {
        let Some(c) = &self.cue else { bail!("열린 큐시트가 없습니다.") };
        let p = self.data_dir.join("print-cue.html");
        std::fs::write(&p, cues::print_html(c))?;
        Ok(p)
    }
    pub fn planned_times(&self) -> Vec<String> {
        self.cue.as_ref().map(|c| cues::planned_times(&c.start_time, &c.items)).unwrap_or_default()
    }
    /// 순서를 바꾸기 전에 부름 (실행 취소용)
    pub fn remember_items(&mut self) {
        if let Some(c) = &self.cue {
            let (id, items) = (c.id.clone(), c.items.clone());
            self.b.undo.push(&id, &items);
        }
    }
    pub fn undo_items(&mut self, redo: bool) -> bool {
        let Some((id, cur)) = self.cue.as_ref().map(|c| (c.id.clone(), c.items.clone())) else { return false };
        let r = if redo { self.b.undo.redo(&id, &cur) } else { self.b.undo.undo(&id, &cur) };
        match r {
            Some(items) => {
                if let Some(c) = &mut self.cue {
                    c.items = items;
                }
                self.save_cue();
                true
            }
            None => false,
        }
    }
    /// 편집칸에서 글을 고칠 때 (실행 취소 묶음): 다른 순서이거나 1.5초 넘게 쉬었다 치면 새 묶음
    pub fn remember_typing(&mut self, item_id: &str) {
        let fresh = match &self.b.typing {
            Some((id, t)) => id != item_id || t.elapsed().as_millis() > 1500,
            None => true,
        };
        if fresh {
            self.remember_items();
        }
        self.b.typing = Some((item_id.to_string(), std::time::Instant::now()));
    }
    /// 끌어서 순서 옮기기
    pub fn move_item_to(&mut self, from: usize, to: usize) {
        self.remember_items();
        if let Some(c) = &mut self.cue {
            if from < c.items.len() {
                let it = c.items.remove(from);
                let to = to.min(c.items.len());
                c.items.insert(to, it);
            }
        }
        self.save_cue();
    }
}

#[cfg(test)]
mod tests_typing {
    use crate::core::{new_item, Core};
    fn core() -> Core {
        let dir = std::env::temp_dir().join(format!("wp-typing-{}-{}", crate::util::now_ms(), rand::random::<u32>()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("bible.db"), b"").unwrap();
        Core::open(dir.clone(), dir, None).unwrap()
    }
    #[test]
    fn typing_groups_and_drag() {
        let mut c = core();
        c.new_cue("시험").unwrap();
        for t in ["가", "나", "다"] {
            c.add_item(new_item("title", t), None);
        }
        // 같은 순서를 이어서 치면 한 묶음 → 실행 취소 한 번에 처음 글로
        let id = c.cue.as_ref().unwrap().items[0]["id"].as_str().unwrap().to_string();
        for s in ["가1", "가12", "가123"] {
            c.remember_typing(&id);
            c.cue.as_mut().unwrap().items[0]["title"] = serde_json::json!(s);
        }
        assert!(c.undo_items(false));
        assert_eq!(c.cue.as_ref().unwrap().items[0]["title"], "가");
        // 끌어서 옮기기: 0 → 2
        c.move_item_to(0, 2);
        let t: Vec<&str> = c.cue.as_ref().unwrap().items.iter().map(|x| x["title"].as_str().unwrap()).collect();
        assert_eq!(t, ["나", "다", "가"]);
        assert!(c.undo_items(false));
        assert_eq!(c.cue.as_ref().unwrap().items[0]["title"], "가");
    }
}
