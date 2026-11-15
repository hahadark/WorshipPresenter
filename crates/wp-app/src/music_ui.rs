//! [트랙 B] 음악 재생 (조작 PC 스피커, Electron 판 library.js audio 와 같은 동작)
//! - 순서의 배경 음악(item.music): 그 순서를 송출하면 반복 재생 (소리는 musicVolume), 0.6초 동안 커지며 시작
//! - '다른 화면을 송출하면 페이드아웃'(fadeOnLeave): 다른 순서를 송출하면 2.5초 동안 줄이며 끔 (시작 2초 안에는 그대로)
//! - 순서의 자동 동작 stopMusic: 송출하면 2.5초 페이드아웃 · 음악 순서(type audio): 송출하면 재생
//! - 라이브러리 음악 ▶ 바로 재생 · 위 도구 줄 아래 재생 줄 (재생·멈춤 · 정지 · 페이드아웃 · 위치 · 소리)

use crate::app::App;
use eframe::egui::{self, Color32, RichText};
use rodio::{Decoder, MixerDeviceSink, Player};
use serde_json::Value;
use std::cell::RefCell;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct Music {
    /// 소리 장치는 프로그램 끝까지 (끝날 때 cpal 이 스레드 지역 COM 정리 뒤에 닫히면 멈춰서 일부러 놓아 둔다)
    sink: Option<&'static MixerDeviceSink>,
    player: Option<Player>,
    pub url: String,
    pub title: String,
    path: PathBuf,
    pub base: f32,
    pub looped: bool,
    fade_on_leave: bool,
    started: Option<Instant>,
    /// (시작 시각, 시작 소리, 끝 소리, 길이 ms, 끝나면 멈춤)
    fade: Option<(Instant, f32, f32, u64, bool)>,
    last_live: Option<(i64, String, i64)>,
    pub err: String,
}

thread_local! {
    pub static MUSIC: RefCell<Music> = RefCell::new(Music { base: 1.0, ..Default::default() });
}

/// '/lib/file?p=…' · 'C:\…' → 파일 경로
pub fn path_of(url: &str) -> Option<PathBuf> {
    let p = if let Some(q) = url.strip_prefix("/lib/file?p=") {
        PathBuf::from(percent_encoding::percent_decode_str(q).decode_utf8().ok()?.to_string())
    } else {
        PathBuf::from(url)
    };
    p.is_file().then_some(p)
}

impl Music {
    fn ensure(&mut self) -> Result<(), String> {
        if self.player.is_none() {
            let sink = rodio::DeviceSinkBuilder::open_default_sink().map_err(|e| format!("소리 장치를 열 수 없습니다: {e}"))?;
            let mut sink = sink;
            sink.log_on_drop(false);
            let sink: &'static MixerDeviceSink = Box::leak(Box::new(sink));
            self.player = Some(Player::connect_new(sink.mixer()));
            self.sink = Some(sink);
        }
        Ok(())
    }

    fn append(&mut self) -> Result<(), String> {
        let f = std::fs::File::open(&self.path).map_err(|e| e.to_string())?;
        let d = Decoder::try_from(f).map_err(|e| format!("이 음악 파일을 읽을 수 없습니다: {e}"))?;
        self.player.as_ref().unwrap().append(d);
        Ok(())
    }

    pub fn play(&mut self, url: &str, title: &str, volume: f32, looped: bool, fade_on_leave: bool) {
        self.err.clear();
        let Some(path) = path_of(url) else {
            self.err = format!("음악 파일을 찾을 수 없습니다: {title}");
            return;
        };
        if let Err(e) = self.ensure() {
            self.err = e;
            return;
        }
        let p = self.player.as_ref().unwrap();
        p.clear();
        (self.url, self.title, self.path, self.base, self.looped, self.fade_on_leave) = (url.into(), title.into(), path, volume.clamp(0.0, 1.0), looped, fade_on_leave);
        if let Err(e) = self.append() {
            self.err = e;
            self.url.clear();
            return;
        }
        let p = self.player.as_ref().unwrap();
        p.set_volume(0.0);
        p.play();
        self.started = Some(Instant::now());
        self.fade = Some((Instant::now(), 0.0, self.base, 600, false));
    }

    pub fn stop(&mut self) {
        if let Some(p) = &self.player {
            p.clear();
        }
        self.url.clear();
        self.fade = None;
    }

    pub fn fade_out(&mut self, ms: u64) {
        let Some(p) = &self.player else { return };
        if self.url.is_empty() || p.is_paused() {
            return;
        }
        self.fade = Some((Instant::now(), p.volume() as f32, 0.0, ms, true));
    }

    pub fn toggle(&mut self) {
        if let Some(p) = &self.player {
            if self.url.is_empty() {
                return;
            }
            if p.is_paused() { p.play() } else { p.pause() }
        }
    }

    pub fn set_volume(&mut self, v: f32) {
        self.base = v.clamp(0.0, 1.0);
        self.fade = None;
        if let Some(p) = &self.player {
            p.set_volume(self.base as _);
        }
    }

    pub fn position(&self) -> Duration {
        self.player.as_ref().map(|p| p.get_pos()).unwrap_or_default()
    }

    pub fn seek(&mut self, d: Duration) {
        if let Some(p) = &self.player {
            let _ = p.try_seek(d);
        }
    }

    pub fn playing(&self) -> bool {
        !self.url.is_empty() && self.player.as_ref().is_some_and(|p| !p.is_paused())
    }

    /// 매 프레임: 페이드 진행 · 끝나면 반복 또는 정리
    pub fn tick(&mut self) {
        let Some(p) = &self.player else { return };
        if let Some((t0, from, to, ms, stop)) = self.fade {
            let k = (t0.elapsed().as_millis() as f32 / ms.max(1) as f32).min(1.0);
            p.set_volume((from + (to - from) * k) as _);
            if k >= 1.0 {
                self.fade = None;
                if stop {
                    self.stop();
                    return;
                }
            }
        }
        if !self.url.is_empty() && p.empty() {
            if self.looped {
                if let Err(e) = self.append() {
                    self.err = e;
                    self.url.clear();
                }
            } else {
                self.url.clear();
            }
        }
    }

    /// 다른 화면을 송출할 때: '페이드아웃' 표시된 음악이면 줄이며 끄기 (방금 시작한 음악은 그대로)
    pub fn on_visual_send(&mut self) {
        if self.playing() && self.fade_on_leave && self.started.is_some_and(|s| s.elapsed() > Duration::from_secs(2)) {
            self.fade_out(2500);
        }
    }
}

/// 새로 송출한 순서의 음악 동작 (Electron cue.js runActions · playAudio)
pub fn on_send(m: &mut Music, item: Option<&Value>) {
    m.on_visual_send();
    let Some(it) = item else { return };
    let music = it["music"].as_str().unwrap_or("");
    if it["type"] == "audio" {
        let f = it["file"].as_str().unwrap_or("");
        m.play(f, it["title"].as_str().unwrap_or(""), it["volume"].as_f64().unwrap_or(1.0) as f32, it["loop"].as_bool().unwrap_or(false), it["fadeOnLeave"].as_bool() != Some(false));
    } else if !music.is_empty() {
        m.play(music, &format!("{} · 배경 음악", it["title"].as_str().unwrap_or("")), it["musicVolume"].as_f64().unwrap_or(0.8) as f32, true, true);
    } else if it["actions"]["stopMusic"].as_bool().unwrap_or(false) {
        m.fade_out(2500);
    }
}

impl App {
    /// 매 프레임: 송출이 바뀌었는지 보고 음악 동작 · 페이드 진행
    pub(crate) fn music_tick(&mut self, ctx: &egui::Context) {
        let (key, item, audio) = {
            let mut c = self.core.lock();
            let audio = c.take_pending_audio();
            let (key, item) = match &c.pres.live {
                Some(l) => {
                    let ci = l.deck.passage["cueIndex"].as_i64().unwrap_or(-1);
                    let cid = l.deck.passage["cueId"].as_str().unwrap_or("").to_string();
                    let it = c.cue.as_ref().filter(|q| q.id == cid && ci >= 0).and_then(|q| q.items.get(ci as usize).cloned());
                    (Some((l.sent_at, cid, ci)), it)
                }
                None => (None, None),
            };
            (key, item, audio)
        };
        MUSIC.with_borrow_mut(|m| {
            if key.is_some() && key != m.last_live {
                // 같은 순서 안에서 슬라이드만 넘긴 것은 새 송출이 아니다 (sent_at 이 같음)
                let first = m.last_live.is_none();
                m.last_live = key.clone();
                if !first || item.is_some() {
                    on_send(m, item.as_ref());
                }
            }
            // 음악 순서는 시각 LIVE를 바꾸지 않는다. 코어가 전달한 음악 명령만 소비한다.
            if let Some(audio) = audio.as_ref() {
                on_send(m, Some(audio));
            }
            m.tick();
            if m.playing() || m.fade.is_some() {
                ctx.request_repaint_after(Duration::from_millis(100));
            }
        });
    }

    /// 재생 줄 (음악이 있을 때만)
    pub(crate) fn music_bar(&mut self, ui: &mut egui::Ui) {
        MUSIC.with_borrow_mut(|m| {
            if !m.err.is_empty() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(wp_core::i18n::tr_dyn(&m.err)).small().color(Color32::from_rgb(0xff, 0x8a, 0x8a)));
                    if ui.small_button(wp_core::i18n::tr("닫기")).clicked() { m.err.clear(); }
                });
            }
            if m.url.is_empty() {
                return;
            }
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("🔊 {}", m.title)).strong().color(Color32::from_rgb(0x8a, 0xe0, 0xa8)));
                let paused = !m.playing();
                if ui.button(if paused { "▶" } else { "⏸" }).clicked() { m.toggle(); }
                if ui.button("■").on_hover_text(wp_core::i18n::tr("정지")).clicked() { m.stop(); return; }
                if ui.button(wp_core::i18n::tr("페이드아웃")).on_hover_text(wp_core::i18n::tr("3초 동안 줄이며 끄기")).clicked() { m.fade_out(3000); }
                let pos = m.position().as_secs();
                if ui.small_button(wp_core::i18n::tr("−10초")).clicked() { m.seek(Duration::from_secs(pos.saturating_sub(10))); }
                if ui.small_button(wp_core::i18n::tr("+10초")).clicked() { m.seek(Duration::from_secs(pos + 10)); }
                ui.label(RichText::new(format!("{}:{:02}", pos / 60, pos % 60)).monospace());
                let mut v = m.base;
                if ui.add(egui::Slider::new(&mut v, 0.0..=1.0).text("소리")).changed() { m.set_volume(v); }
                ui.checkbox(&mut m.looped, wp_core::i18n::tr("반복"));
            });
        });
    }
}

/// 라이브러리 ▶ 바로 재생
pub fn play_now(url: &str, title: &str) {
    MUSIC.with_borrow_mut(|m| m.play(url, title, 1.0, false, false));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paths() {
        let me = std::env::current_exe().unwrap();
        let enc = percent_encoding::utf8_percent_encode(&me.to_string_lossy(), percent_encoding::NON_ALPHANUMERIC).to_string();
        assert_eq!(path_of(&format!("/lib/file?p={enc}")), Some(me.clone()));
        assert_eq!(path_of("/lib/file?p=C%3A%5Cno-such.mp3"), None);
    }
    #[test]
    fn send_without_file_reports() {
        let mut m = Music { base: 1.0, ..Default::default() };
        on_send(&mut m, Some(&serde_json::json!({ "type": "title", "title": "찬양", "music": "/lib/file?p=C%3A%5Cno-such.mp3" })));
        assert!(m.err.contains("찾을 수 없습니다"));
        assert!(m.url.is_empty());
    }
}
