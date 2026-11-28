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
    /// 출력 화면이 확인한 파일별 길이(초). 사용자 자료에는 저장하지 않는다.
    pub video_lengths: std::collections::HashMap<String, f64>,
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
    /// 연결된 카메라 실시간 목록 (출력 창 도우미가 보낸 마지막 결과)
    pub cams: crate::cameras::DeviceList,
    /// 화면 구성 '지금 한 번' (로드맵 4단계, 다음 순서로 넘어갈 때까지)
    pub scene_once: crate::scenectl::SceneOnce,
}

impl Core {
    /// 열 때 한 번 (Songs 테이블)
    pub fn trackb_init(&mut self) {
        if let Err(e) = songs::ensure_schema(&self.store.db) {
            log::error!("Songs 테이블: {e}");
        }
        // 예전 Rust 칸으로 저장된 매크로 → Electron 칸 (순서의 '송출할 때 자동으로' 와 같은 형식)
        let mut n = 0;
        self.settings.update(|d| n = overlays::migrate_macros(d));
        if n > 0 {
            log::info!("매크로 {n}개를 Electron 형식으로 바꿈");
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
        // 찬양팀용 곡 정보가 있으면 진행 메모로 (큐시트 목록·인쇄에 보임)
        it["note"] = json!(crate::songs::meta_line(&s));
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
    /// 페이드 완료를 실제 코어 상태에 반영한다. 새 출력 연결도 숨김·정지 상태를 받는다.
    pub(crate) fn finish_video_fade(&mut self, now: i64) -> bool {
        let due = self.b.video.as_ref().and_then(|v| v.fade)
            .is_some_and(|(at, ms)| now as f64 >= at as f64 + ms);
        if !due { return false; }
        let same_video = self.pres.live.as_ref().is_some_and(|l| {
            self.b.video_for == Some((l.sent_at as u64, l.deck.index))
                && l.deck.slides.get(l.deck.index).is_some_and(|s| s["kind"] == "video")
        });
        if !same_video {
            if let Some(v) = &mut self.b.video { v.fade = None; }
            return false;
        }
        if let Some(v) = &mut self.b.video {
            let _ = v.control("stop", 0.0);
            v.at = now;
        }
        self.pres.clear();
        self.changed(true);
        true
    }

    pub fn video_payload(&self) -> Value {
        self.b.video.as_ref().map(|v| {
            let mut payload = v.payload();
            // 조작 패널이 닫혀 있어도 출력은 페이드 종료 뒤 멈춘 상태로 동기화한다.
            if v.fade.is_some_and(|(at, ms)| now_ms() as f64 >= at as f64 + ms) {
                payload["playing"] = json!(false);
                payload["offset"] = json!(0);
            }
            payload
        }).unwrap_or(Value::Null)
    }
    /// 현재 영상 키와 일치하는 유한한 양수 길이만 기억한다. 늦게 도착한 이전 영상 알림은 무시한다.
    pub fn video_meta(&mut self, key: &str, duration: Option<f64>) {
        let Some(duration) = duration.filter(|d| d.is_finite() && *d > 0.0) else { return };
        if !self.b.video.as_ref().is_some_and(|v| v.key == key) { return; }
        let Some(url) = self.pres.live.as_ref().and_then(|l| l.deck.slides.get(l.deck.index))
            .filter(|s| s["kind"] == "video").and_then(|s| s["image"].as_str()).filter(|s| !s.is_empty()) else { return };
        self.b.video_lengths.insert(url.to_owned(), duration);
        self.emit(Ev::Ui);
    }
    pub fn video_duration(&self, url: &str) -> Option<f64> {
        self.b.video_lengths.get(url).copied()
    }
    pub fn video_control(&mut self, act: &str, arg: f64) -> Result<()> {
        let Some(v) = self.b.video.as_mut() else { bail!("송출 중인 영상이 없습니다.") };
        v.control(act, arg)?;
        if act == "stop" { self.pres.clear(); }
        if act == "play" || act == "restart" { self.pres.restore(); }
        self.changed(true);
        Ok(())
    }
    /// 출력 화면이 알리는 '영상 끝남' → 끝나면(endAction): hold 그대로 · black 글자 숨김 · next 다음 슬라이드
    /// duration: 출력 화면이 잰 영상 길이(초). 끝난 자리를 기억해 둬야 다른 출력이 처음으로 돌아가지 않는다 (Electron 과 같게)
    pub fn video_ended(&mut self, key: &str, duration: Option<f64>) {
        let Some(v) = self.b.video.as_mut() else { return };
        if v.key != key || v.ended {
            return;
        }
        let pos = v.position();
        // 코어 시계로는 아직 한참 남았는데 '끝남' 이 오면 (캐시된 다른 파일 · 어긋난 출력) 믿지 않는다
        if let Some(d) = duration.filter(|d| *d > 0.0) {
            if v.playing && pos + 3.0 < d {
                log::warn!("영상 끝남 알림 무시: 코어 위치 {pos:.1}초 < 길이 {d:.1}초");
                return;
            }
        }
        v.ended = true;
        v.playing = false;
        v.offset = duration.filter(|d| *d > 0.0).unwrap_or(pos);
        v.at = now_ms();
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
        // 추가 출력(로드맵 3단계)은 같은 종류 내장 출력(예배당 · 방송)의 오버레이 · 회중 메시지를 받는다
        let output = crate::outputs::kind_of(self.settings.get(), output);
        let mut o: Vec<Value> = self.overlays().into_iter().filter(|x| x["on"] == true && x["outputs"].as_array().is_some_and(|a| a.iter().any(|y| y == output))).collect();
        o.extend(overlays::message_overlay(&self.b.message, output));
        o.extend(crate::voicefollow::caption_overlay(output)); // 실시간 자막 (기본은 방송 출력에만)
        o
    }

    /// 출력 데이터의 overlays: 화면 구성에 'caption' 칸을 직접 넣었으면 자막을 어느 출력에 띄웠든 이 출력에도 (그 칸에 그림)
    pub fn overlays_with_scene(&self, output: &str, scene: &Value) -> Vec<Value> {
        let mut o = self.overlays_for(output);
        let has_cap = scene["layers"].as_array().is_some_and(|a| a.iter().any(|l| l["source"] == "caption" && l["auto"] != true));
        if has_cap && !o.iter().any(|x| x["id"] == crate::voicefollow::CAPTION_ID) {
            o.extend(crate::voicefollow::caption_overlay_any());
        }
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
    // ---------------- 이름 자막 ----------------
    pub fn people(&self) -> Vec<Value> {
        overlays::people(&self.settings.get())
    }
    pub fn save_people(&mut self, list: &[Value]) {
        self.settings.update(|d| overlays::save_people(d, list));
        self.emit(Ev::Ui);
    }
    /// 이름 자막을 저절로 내릴 때까지 초 (설정 nameTagSeconds, 0 = 계속)
    pub fn name_tag_seconds(&self) -> f64 {
        self.settings.get()["nameTagSeconds"].as_f64().unwrap_or(0.0).clamp(0.0, 600.0)
    }
    pub fn show_name_tag(&mut self, name: &str, role: &str) -> Result<Value> {
        let sec = self.name_tag_seconds();
        let mut out = Ok(Value::Null);
        let now = now_ms();
        self.settings.update(|d| out = overlays::show_name(d, name, role, sec, now));
        self.changed(true);
        out
    }
    /// 명단의 i 번째 사람
    pub fn show_person(&mut self, i: usize) -> Result<Value> {
        let p = self.people().get(i).cloned().ok_or_else(|| anyhow::anyhow!(crate::i18n::tr("명단에 없는 사람입니다.")))?;
        self.show_name_tag(p["name"].as_str().unwrap_or(""), p["role"].as_str().unwrap_or(""))
    }
    pub fn hide_name_tag(&mut self) {
        let mut was = false;
        self.settings.update(|d| was = overlays::hide_name(d));
        if was {
            self.changed(true);
        }
    }
    pub fn name_tag_on(&self) -> Option<Value> {
        overlays::name_on(&self.settings.get())
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
    /// 무대 타이머를 '그 시각까지'로 시작 ("11:00" · "11시 30분" · "오후 2시"). 예배 시작·설교 끝 시각에 맞출 때.
    /// (2026-11-26 예배송출의 '시각으로 맞추는 타이머' 를 참고해 더함, 코드는 새로 씀)
    pub fn stage_timer_until(&mut self, at: &str) -> Result<()> {
        let Some(hm) = crate::localtime::parse_hm(at) else { anyhow::bail!(crate::i18n::tr("시각을 '11:00' 처럼 적어 주세요.")) };
        let now = crate::util::now_ms() + crate::localtime::local_offset_ms();
        let Some(ms) = crate::localtime::ms_until(hm, now) else { anyhow::bail!(crate::i18n::tr("이미 지난 시각입니다. 앞으로 12시간 안의 시각을 적어 주세요 (예: 11:00).")) };
        self.stage_timer("start", ms as f64 / 60000.0)
    }
    /// 즉석 자막이 지금 송출 중인가 (quick_send 가 만든 묶음: cueIndex -1 · 제목 '즉석 자막')
    pub fn quick_is_live(&self) -> bool {
        self.pres.live.as_ref().is_some_and(|l| l.visible && l.deck.passage["cueIndex"] == -1 && l.deck.passage["itemTitle"] == "즉석 자막")
    }
    /// 즉석 자막을 내린다: 앞에 송출하던 화면(`prev`)으로 돌아가거나, 없으면 글자만 숨긴다.
    /// 되돌리는 것은 새 송출이 아니므로 송출 기록 · 사용 기록(저작권 보고)에 다시 넣지 않는다.
    pub fn quick_take_down(&mut self, prev: Option<crate::presentation::Deck>) {
        if !self.quick_is_live() {
            return;
        }
        let back = prev.is_some_and(|d| self.pres.send_live(Some(d)));
        if !back {
            self.pres.clear();
        }
        self.emit(Ev::Live);
        self.emit(Ev::Ui);
    }
    pub fn stage_message(&mut self, text: &str) {
        self.b.stage.message = text.chars().take(300).collect();
        self.changed(true);
    }
    /// 무대 화면 고정 (Electron core.js stagePinSet): {mode:'order'|'text', text, name} · {preset: id}(settings.stagePresets 에서 찾음) · None(지금 화면으로).
    /// 프리셋을 못 찾으면 고정을 바꾸지 않고 오류.
    pub fn stage_pin_set(&mut self, p: Option<&Value>) -> Result<()> {
        let preset = p.and_then(|p| p["preset"].as_str()).filter(|x| !x.is_empty()).map(String::from);
        let resolved = match &preset {
            Some(id) => {
                let found = self.settings.get()["stagePresets"].as_array().and_then(|l| l.iter().find(|x| x["id"] == id.as_str()).cloned());
                match found {
                    Some(x) => Some(x),
                    None => bail!("무대 화면 프리셋을 찾을 수 없습니다."),
                }
            }
            None => p.cloned(),
        };
        self.b.stage.set_pin(resolved.as_ref());
        self.changed(true);
        Ok(())
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
    /// 매크로 실행 (이름 또는 id) — 매크로 창 · Alt+1~9 · 예약 · 외부 단추 /api/trigger/macro
    pub fn run_macro(&mut self, key: &str) -> Result<()> {
        self.run_macro_depth(key, 0)
    }
    /// Electron macropanel.js run + cue.js runActions: 매크로의 actions 는 순서의 '송출할 때 자동으로' 와 같은 칸이라
    /// 같은 실행기(trackd::run_item_actions)를 쓴다. depth: 매크로 안의 매크로는 한 번까지(depth > 1 이면 실행 안 함).
    /// music · musicVolume 은 조작 화면이 틀어 주도록 pending_audio 에 담는다 (순서의 배경 음악과 같은 길).
    pub fn run_macro_depth(&mut self, key: &str, depth: u8) -> Result<()> {
        let m = self.settings.get()["macros"].as_array().and_then(|l| l.iter().find(|x| x["id"] == key || x["name"] == key.trim()).cloned());
        let Some(m) = m else { bail!(crate::i18n::tr_args("매크로 '{1}' 을(를) 찾을 수 없습니다. 송출 → 매크로 · 예약 실행에서 이름을 확인하세요.", &[&key])) };
        if depth > 1 {
            return Ok(());
        }
        let name = m["name"].as_str().unwrap_or("").to_string();
        let actions = overlays::macro_actions(&m);
        let music = m["music"].as_str().unwrap_or("");
        if !music.is_empty() {
            self.pending_audio = Some(json!({ "title": name, "music": music, "musicVolume": m["musicVolume"].as_f64().unwrap_or(0.8) }));
        } else if actions["stopMusic"] == true {
            self.pending_audio = Some(json!({ "title": name, "actions": { "stopMusic": true } }));
        }
        crate::trackd::run_item_actions(self, &actions, depth + 1);
        log::info!("매크로 '{name}'");
        self.notify(crate::i18n::tr_args("매크로 실행: {1}", &[&name]), "ok");
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
                self.notify(crate::i18n::tr_args("예약 실행 실패: {1}", &[&e]), "warn");
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
    pub fn print_cue_file(&mut self) -> Result<PathBuf> {
        let Some(mut cue) = self.cue.clone() else { bail!("열린 큐시트가 없습니다.") };
        let options: cues::PrintOptions = serde_json::from_value(self.settings.get()["printCueOptions"].clone()).unwrap_or_default();
        // 부를 순서가 비어 있으면 연결된 찬양의 기본 순서를 따른다.
        for it in &mut cue.items {
            if it["type"] == "song" && it["order"].as_str().unwrap_or("").trim().is_empty() {
                if let Some(song) = it["songId"].as_str().and_then(|id| songs::get_song(&self.store.db, id)) {
                    it["order"] = song["arrangement"].clone();
                }
            }
        }
        let version = self.settings.get()["versionId"].as_str().unwrap_or("").to_string();
        let mut errors = Vec::new();
        let html = cues::print_html_with(&cue, &options, |it| {
            let v = it["versionId"].as_str().filter(|s| !s.is_empty()).unwrap_or(&version);
            let r = self.store.resolve_reference(v, it["ref"].as_str().unwrap_or(""));
            if r["ok"] != true {
                let e = r["error"].as_str().unwrap_or("성경 본문을 불러오지 못했습니다.").to_string();
                errors.push(e.clone());
                bail!("{e}");
            }
            Ok(r["passage"]["verses"].as_array().cloned().unwrap_or_default())
        });
        for e in errors { self.notify(e, "warn"); }
        let p = self.data_dir.join("print-cue.html");
        std::fs::write(&p, html)?;
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
    /// 되돌리기(redo 면 다시 하기) 단추 툴팁·알림에 쓸 말: 무엇이 되돌려지는지 (할 것이 없으면 None)
    pub fn undo_label(&self, redo: bool) -> Option<String> {
        let c = self.cue.as_ref()?;
        let target = self.b.undo.peek(&c.id, redo)?;
        // 되돌리기: 그 전 모습(target) → 지금(cur) 으로 바꾼 일을 취소 / 다시 하기: 지금 → target 으로 다시
        Some(if redo { cues::describe_change(&c.items, target) } else { cues::describe_change(target, &c.items) })
    }
    /// 되돌리기 · 다시 하기. 결과를 알림으로 보인다 (원본 cue.js undo: '실행 취소 (N번 더 가능)')
    pub fn undo_items(&mut self, redo: bool) -> bool {
        use crate::i18n::{tr, tr_args};
        let Some((id, cur)) = self.cue.as_ref().map(|c| (c.id.clone(), c.items.clone())) else { return false };
        let label = self.undo_label(redo);
        let r = if redo { self.b.undo.redo(&id, &cur) } else { self.b.undo.undo(&id, &cur) };
        match r {
            Some(items) => {
                if let Some(c) = &mut self.cue {
                    c.items = items;
                }
                self.save_cue();
                let label = label.unwrap_or_default();
                let left = self.b.undo.depth(&id, false);
                let msg = if redo { tr_args("다시 했습니다: {1}", &[&label]) } else { tr_args("되돌렸습니다: {1} ({2}번 더 되돌릴 수 있음)", &[&label, &left]) };
                self.notify(msg, "ok");
                true
            }
            None => {
                self.notify(if redo { tr("다시 할 것이 없습니다") } else { tr("더 되돌릴 것이 없습니다") }, "info");
                false
            }
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

#[cfg(test)]
mod tests_video {
    use crate::core::Core;
    use crate::library::Video;
    #[test]
    fn ended_keeps_position_and_ignores_stale() {
        let dir = std::env::temp_dir().join(format!("wp-vid-{}-{}", crate::util::now_ms(), rand::random::<u32>()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("bible.db"), b"").unwrap();
        let mut c = Core::open(dir.clone(), dir, None).unwrap();
        c.b.video = Some(Video::start("k1"));
        // 방금 시작했는데 8초짜리가 끝났다고 함 → 무시
        c.video_ended("k1", Some(8.0));
        assert!(c.b.video.as_ref().unwrap().playing);
        // 다른 열쇠 → 무시
        c.b.video.as_mut().unwrap().offset = 7.5;
        c.video_ended("k2", Some(8.0));
        assert!(c.b.video.as_ref().unwrap().playing);
        // 끝 근처에서 끝남 → 멈추고 끝 자리 기억
        c.video_ended("k1", Some(8.0));
        let v = c.b.video.as_ref().unwrap();
        assert!(!v.playing && v.ended);
        assert_eq!(v.offset, 8.0);
        assert_eq!(c.video_payload()["offset"], 8.0);
    }
}

#[cfg(test)]
mod tests_name_tag {
    // 이름 자막: 명단에서 띄우고 · 출력(OBS)에 실리고 · 정한 초가 지나면 저절로 내려간다 · 리모컨에서도
    use crate::core::Core;
    use serde_json::json;
    #[test]
    fn show_expire_remote() {
        let dir = std::env::temp_dir().join(format!("wp-name-{}-{}", crate::util::now_ms(), rand::random::<u32>()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("bible.db"), b"").unwrap();
        let mut c = Core::open(dir.clone(), dir.clone(), None).unwrap();
        c.save_people(&[json!({ "name": "김택근", "role": "사관" }), json!({ "name": "홍길동", "role": "기도" })]);
        c.show_person(0).unwrap();
        assert_eq!(c.overlays_for("overlay").iter().filter(|o| o["type"] == "name").count(), 1);
        assert_eq!(c.overlays_for("main").iter().filter(|o| o["type"] == "name").count(), 0);
        assert!(c.show_person(9).is_err());
        // 저절로 내리기
        c.settings.update(|d| d["nameTagSeconds"] = json!(5));
        c.show_person(1).unwrap();
        let at = c.name_tag_on().unwrap()["hideAt"].as_i64().unwrap();
        crate::flow::tick(&mut c, at - 1);
        assert!(c.name_tag_on().is_some());
        crate::flow::tick(&mut c, at + 1);
        assert!(c.name_tag_on().is_none(), "5초 뒤 내려감");
        // 리모컨
        let st = crate::remote::state_for(&c);
        assert_eq!(st["names"]["people"].as_array().unwrap().len(), 2);
        crate::remote::act(&mut c, &json!({ "action": "name", "index": 0 })).unwrap();
        assert_eq!(crate::remote::state_for(&c)["names"]["on"]["name"], "김택근");
        crate::remote::act(&mut c, &json!({ "action": "name", "index": -1 })).unwrap();
        assert!(crate::remote::state_for(&c)["names"]["on"].is_null());
        drop(c);
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[cfg(test)]
mod tests_double_send {
    // [트랙 B] 미리보기를 끈 채 두 번 눌러 송출해도 한 번만 송출 (기록 한 줄)
    use crate::core::{new_item, Core};
    #[test]
    fn once() {
        let dir = std::env::temp_dir().join(format!("wp-dbl-{}-{}", crate::util::now_ms(), rand::random::<u32>()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("bible.db"), b"").unwrap();
        let mut c = Core::open(dir.clone(), dir, None).unwrap();
        c.settings.update(|d| d["previewMode"] = serde_json::json!(false));
        c.new_cue("t").unwrap();
        c.add_item(new_item("title", "예배로 부름"), None);
        let h0 = c.settings.get()["history"].as_array().map(|a| a.len()).unwrap_or(0);
        let v0 = c.pres.version;
        c.preview_item(0); // 미리보기 끔 → 바로 송출
        c.send_preview(); // 두 번 누름의 송출
        assert_eq!(c.pres.version, v0 + 1);
        assert_eq!(c.settings.get()["history"].as_array().unwrap().len(), h0 + 1);
    }
}


#[cfg(test)]
mod tests_fade_completion {
    use super::*;
    use crate::presentation::Deck;

    fn video_core() -> Core {
        let dir = std::env::temp_dir().join(format!("wp-fade-final-{}", rand::random::<u64>()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("bible.db"), b"").unwrap();
        let mut c = Core::open(dir.clone(), dir, None).unwrap();
        c.send_deck(Deck { passage: json!({"title":"시험 영상","itemType":"video"}), slides: vec![json!({"kind":"video","image":"/media/test.mp4","video":{"volume":1,"sound":true}})], index:0 });
        c.b.video.as_mut().unwrap().fade = Some((1000, 3000.0));
        c
    }

    #[test]
    fn fade_timer_hides_actual_live_and_new_output_without_resending() {
        let mut c = video_core();
        let sent = c.pres.live.as_ref().unwrap().sent_at;
        let history = c.settings.get()["history"].clone();
        crate::flow::tick(&mut c, 3999);
        assert!(c.pres.live.as_ref().unwrap().visible);
        crate::flow::tick(&mut c, 4000);
        assert!(!c.pres.live.as_ref().unwrap().visible);
        assert_eq!(c.output_payload("main")["visible"], false);
        assert_eq!(c.video_payload()["playing"], false);
        assert_eq!(c.video_payload()["offset"], 0.0);
        assert!(c.b.video.as_ref().unwrap().fade.is_none());
        assert_eq!(c.pres.live.as_ref().unwrap().sent_at, sent);
        assert_eq!(c.settings.get()["history"], history);
        let version = c.pres.version;
        crate::flow::tick(&mut c, 5000);
        assert_eq!(c.pres.version, version);
        let dir = c.data_dir.clone(); drop(c); let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn stale_fade_must_not_hide_a_new_title() {
        let mut c = video_core();
        c.send_deck(Deck { passage: json!({"title":"새 화면","itemType":"title"}), slides: vec![json!({"kind":"title","parts":[{"text":"새 화면"}]})], index:0 });
        crate::flow::tick(&mut c, 5000);
        assert!(c.pres.live.as_ref().unwrap().visible);
        assert_eq!(c.pres.live.as_ref().unwrap().deck.passage["title"], "새 화면");
        let dir = c.data_dir.clone(); drop(c); let _ = std::fs::remove_dir_all(dir);
    }
}

#[cfg(test)]
mod tests_message_expiry {
    //! 검토 1113-3 결함 4: 초를 정한 회중 메시지는 만료되면 지워지고 출력에 새 render(Ev::Live) 가 가야 한다
    use crate::core::{Core, Ev};
    use crate::util::now_ms;

    fn core() -> Core {
        let dir = std::env::temp_dir().join(format!("wp-msg-{}-{}", now_ms(), rand::random::<u32>()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("bible.db"), b"").unwrap();
        Core::open(dir.clone(), dir, None).unwrap()
    }

    #[test]
    fn timed_message_expires_with_live_event() {
        let mut c = core();
        c.show_message("유아실 홍길동 부모님", &["main".into(), "overlay".into()], 10).unwrap();
        let until = c.b.message.as_ref().unwrap().until;
        assert_eq!(c.output_payload("main")["overlays"].as_array().unwrap().iter().filter(|o| o["id"] == "wpmessage").count(), 1);
        let mut rx = c.tx.subscribe();
        // 아직 시간 안: 그대로
        crate::flow::tick(&mut c, until - 1);
        assert!(c.b.message.is_some());
        assert!(rx.try_recv().is_err(), "만료 전에는 이벤트가 없어야 함");
        // 만료: 지워지고 Ev::Live
        crate::flow::tick(&mut c, until + 1);
        assert!(c.b.message.is_none(), "만료된 메시지는 지워야 함");
        let mut live = false;
        while let Ok(e) = rx.try_recv() { if matches!(e, Ev::Live) { live = true; } }
        assert!(live, "만료 시 출력에 새 render 를 보내야 함");
        assert_eq!(c.output_payload("main")["overlays"].as_array().unwrap().iter().filter(|o| o["id"] == "wpmessage").count(), 0);
        // 시간을 안 정한 메시지(0초)는 지우기 전까지 남는다
        c.show_message("계속", &["main".into()], 0).unwrap();
        crate::flow::tick(&mut c, now_ms() + 86_400_000);
        assert!(c.b.message.is_some());
        let dir = c.data_dir.clone(); drop(c); let _ = std::fs::remove_dir_all(dir);
    }
}

#[cfg(test)]
mod tests_macro {
    //! 검토 1113-3 결함 1: 매크로는 Electron 과 같은 칸(순서의 '송출할 때 자동으로')으로 저장·실행
    use crate::core::Core;
    use serde_json::json;
    fn core() -> Core {
        let dir = std::env::temp_dir().join(format!("wp-macro-{}-{}", crate::util::now_ms(), rand::random::<u32>()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("bible.db"), b"").unwrap();
        Core::open(dir.clone(), dir, None).unwrap()
    }

    #[test]
    fn electron_macro_runs_through_item_actions() {
        let mut c = core();
        let ov = c.save_overlay(&json!({ "id": "clock1", "type": "clock" })).unwrap();
        assert_eq!(ov["on"], false);
        c.layers.logo = true;
        // Electron 에서 복사해 온 매크로 (core.js saveMacro 가 저장하는 모양 그대로)
        c.settings.update(|d| d["macros"] = json!([{ "id": "mc1", "name": "예배 시작", "color": "", "actions": { "logo": "off", "bg": "hide", "stageMsg": "환영합니다", "ovOn": "clock1", "timer": 5, "stagePin": "order" } }]));
        c.run_macro("예배 시작").unwrap();
        assert!(!c.layers.logo, "logo:'off' 가 적용돼야 함");
        assert!(c.layers.hide_bg, "bg:'hide' 가 적용돼야 함");
        assert_eq!(c.b.stage.message, "환영합니다");
        assert!(c.b.stage.timer.running);
        assert_eq!(c.overlays()[0]["on"], true, "ovOn 이 적용돼야 함");
        assert_eq!(c.b.stage.pin.as_ref().unwrap()["mode"], "order");
        assert!(c.take_pending_audio().is_none());
        // id 로도 찾는다
        c.run_macro("mc1").unwrap();
        assert!(c.run_macro("없는 매크로").is_err());
    }

    #[test]
    fn macro_music_goes_to_pending_audio_and_nested_depth_limited() {
        let mut c = core();
        c.settings.update(|d| {
            d["macros"] = json!([
                { "id": "mcmusic", "name": "음악", "actions": {}, "music": "/media/a.mp3", "musicVolume": 0.5 },
                { "id": "mcnest1", "name": "겹1", "actions": { "macro": "mcnest2", "logo": "on" } },
                { "id": "mcnest2", "name": "겹2", "actions": { "macro": "mcnest1", "stageMsg": "둘" } },
            ])
        });
        c.run_macro("mcmusic").unwrap();
        let a = c.take_pending_audio().unwrap();
        assert_eq!(a["music"], "/media/a.mp3");
        assert_eq!(a["musicVolume"], 0.5);
        // 매크로 안의 매크로는 한 번까지 (Electron macropanel.js depth > 1 이면 실행 안 함) — 무한 반복하지 않는다
        c.run_macro("mcnest1").unwrap();
        assert!(c.layers.logo);
        assert_eq!(c.b.stage.message, "둘");
    }

    #[test]
    fn save_macro_normalizes_like_electron_and_converts_legacy() {
        let mut c = core();
        // Electron 칸: 범위·형식 정리 (timer 240 상한, 모르는 칸 버림)
        let id = c.save_macro(&json!({ "name": "  정리  ", "actions": { "timer": 999, "logo": "on", "bg": "maybe", "junk": 1, "ovOff": "*" }, "startup": true })).unwrap();
        let m = c.settings.get()["macros"][0].clone();
        assert_eq!(m["id"], id.as_str());
        assert_eq!(m["name"], "정리");
        assert_eq!(m["actions"], json!({ "timer": 240, "logo": "on", "ovOff": "*" }));
        assert_eq!(m["startup"], true);
        // 예전 Rust 칸으로 저장 요청 → Electron 칸으로 바꿔 저장
        c.save_macro(&json!({ "id": "mcold1", "name": "옛것", "actions": { "logo": false, "bgOff": true, "overlaysOff": true, "stageMessage": "안내", "stagePin": { "mode": "order" } } })).unwrap();
        let m = c.settings.get()["macros"][1].clone();
        assert_eq!(m["actions"], json!({ "logo": "off", "bg": "hide", "ovOff": "*", "stageMsg": "안내", "stagePin": "order" }));
        // startup 은 하나만
        c.save_macro(&json!({ "name": "둘째 시작", "actions": { "logo": "off" }, "startup": true })).unwrap();
        let l = c.settings.get()["macros"].clone();
        assert_eq!(l[0]["startup"], false);
        assert_eq!(l[2]["startup"], true);
        // Electron 매크로에 없는 동작(회중 메시지)만 있으면 저장 거부 (Electron: '매크로에 넣을 동작이 없습니다')
        assert!(c.save_macro(&json!({ "name": "빈", "actions": { "message": "글" } })).is_err());
        assert!(c.save_macro(&json!({ "name": "", "actions": { "logo": "on" } })).is_err());
    }

    #[test]
    fn legacy_macros_migrated_on_open() {
        let dir = std::env::temp_dir().join(format!("wp-macro-mig-{}-{}", crate::util::now_ms(), rand::random::<u32>()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("bible.db"), b"").unwrap();
        std::fs::write(dir.join("settings.json"), serde_json::to_vec(&json!({ "macros": [
            { "id": "mcleg", "name": "옛", "actions": { "logo": true, "overlayOn": ["o1"], "timer": 3 } },
            { "id": "mcnew", "name": "새", "actions": { "logo": "off" } },
        ] })).unwrap()).unwrap();
        let c = Core::open(dir.clone(), dir, None).unwrap();
        let l = c.settings.get()["macros"].clone();
        assert_eq!(l[0]["actions"], json!({ "logo": "on", "ovOn": "o1", "timer": 3 }));
        assert_eq!(l[1]["actions"], json!({ "logo": "off" }), "Electron 칸은 그대로");
        // 실행도 된다
        let mut c = c;
        c.run_macro("mcleg").unwrap();
        assert!(c.layers.logo);
    }
}
