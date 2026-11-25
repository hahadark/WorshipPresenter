//! [트랙 B] 라이브러리 폴더(그림·영상·음악) · 끌어다 놓은 파일 · 영상 재생 상태

use crate::util::now_ms;
use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub const IMAGE: [&str; 7] = ["png", "jpg", "jpeg", "webp", "gif", "bmp", "svg"];
pub const VIDEO: [&str; 4] = ["mp4", "m4v", "webm", "mov"];
pub const AUDIO: [&str; 9] = ["mp3", "wav", "m4a", "aac", "ogg", "oga", "flac", "opus", "wma"];

fn ext(p: &Path) -> String {
    p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase()
}
pub fn is_video(p: &Path) -> bool {
    VIDEO.contains(&ext(p).as_str())
}
pub fn is_image(p: &Path) -> bool {
    IMAGE.contains(&ext(p).as_str())
}
pub fn is_audio(p: &Path) -> bool {
    AUDIO.contains(&ext(p).as_str())
}

fn key(kind: &str) -> &'static str {
    if kind == "audio" { "audioFolders" } else { "imageFolders" }
}
fn want(kind: &str, p: &Path) -> bool {
    if kind == "audio" { is_audio(p) } else { is_image(p) || is_video(p) }
}

/// 폴더 안 파일 (한 단계 아래까지, 최대 3000개)
pub fn list_files(kind: &str, dir: &Path) -> Vec<PathBuf> {
    fn walk(kind: &str, d: &Path, depth: u8, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(d) else { return };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || name.starts_with("~$") {
                continue;
            }
            let p = e.path();
            if p.is_dir() {
                if depth < 1 {
                    walk(kind, &p, depth + 1, out)
                }
            } else if want(kind, &p) {
                out.push(p)
            }
            if out.len() >= 3000 {
                return;
            }
        }
    }
    let mut out = vec![];
    walk(kind, dir, 0, &mut out);
    out.sort();
    out
}

/// 출력 화면이 쓰는 주소 (/lib/file?p=…)
pub fn file_url(p: &Path) -> String {
    let s = p.to_string_lossy();
    let mut enc = String::new();
    for b in s.as_bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(b) {
            enc.push(*b as char)
        } else {
            enc.push_str(&format!("%{:02X}", b))
        }
    }
    format!("/lib/file?p={enc}")
}

pub fn folders(settings: &Value, kind: &str) -> Vec<Value> {
    settings[key(kind)]
        .as_array()
        .into_iter()
        .flatten()
        .map(|f| {
            let p = PathBuf::from(f["path"].as_str().unwrap_or(""));
            json!({ "name": f["name"], "path": f["path"], "exists": p.is_dir(), "count": list_files(kind, &p).len() })
        })
        .collect()
}

pub fn add_folder(settings: &mut Value, kind: &str, dir: &Path, name: Option<&str>) -> Result<Value> {
    if !dir.is_dir() {
        bail!("폴더를 찾을 수 없습니다.");
    }
    let n = list_files(kind, dir).len();
    if n == 0 {
        bail!(if kind == "audio" { "이 폴더에 음악 파일(mp3·wav·m4a 등)이 없습니다." } else { "이 폴더에 그림·영상 파일(png·jpg·mp4 등)이 없습니다." });
    }
    let label = name.map(String::from).unwrap_or_else(|| dir.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default());
    let ds = dir.to_string_lossy().to_string();
    let k = key(kind);
    if !settings[k].is_array() {
        settings[k] = json!([]);
    }
    let l = settings[k].as_array_mut().unwrap();
    l.retain(|f| f["path"] != ds.as_str());
    l.push(json!({ "name": label, "path": ds }));
    Ok(json!({ "name": label, "path": ds, "count": n }))
}

pub fn remove_folder(settings: &mut Value, kind: &str, dir: &str) {
    if let Some(l) = settings[key(kind)].as_array_mut() {
        l.retain(|f| f["path"] != dir);
    }
}

/// 파일 이름으로 찾기 (folder 가 비면 모든 폴더)
pub fn search_files(settings: &Value, kind: &str, query: &str, folder: &str) -> Vec<Value> {
    let sq = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_lowercase();
    let q = sq(query);
    let mut out = vec![];
    for f in settings[key(kind)].as_array().into_iter().flatten() {
        let fp = f["path"].as_str().unwrap_or("");
        if !folder.is_empty() && fp != folder {
            continue;
        }
        for p in list_files(kind, Path::new(fp)) {
            let name = p.file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
            if !q.is_empty() && !crate::textnorm::fuzzy_contains(&name, query) { // 띄어쓰기 무시 · 초성도
                continue;
            }
            out.push(json!({ "path": p.to_string_lossy(), "name": name, "folder": f["name"], "url": file_url(&p), "video": is_video(&p) }));
            if out.len() >= 1000 {
                return out;
            }
        }
    }
    out
}

/// 끌어다 놓은 파일 기억 (최근 500개, 등록 폴더 밖이어도 열리게) → 큐시트 순서 제안
pub fn add_dropped(settings: &mut Value, paths: &[PathBuf]) -> Vec<Value> {
    if !settings["droppedFiles"].is_array() {
        settings["droppedFiles"] = json!([]);
    }
    let mut items = vec![];
    for p in paths {
        let s = p.to_string_lossy().to_string();
        let l = settings["droppedFiles"].as_array_mut().unwrap();
        l.retain(|x| x != s.as_str());
        l.push(json!(s));
        let skip = l.len().saturating_sub(500);
        l.drain(..skip);
        let title = p.file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_default();
        if is_video(p) {
            items.push(json!({ "type": "video", "title": title, "file": file_url(p), "sound": true, "volume": 1.0, "loop": false, "endAction": "hold" }))
        } else if is_image(p) {
            items.push(json!({ "type": "image", "title": title, "image": file_url(p) }))
        } else if is_audio(p) {
            items.push(json!({ "type": "blank", "mode": "background", "title": title, "music": file_url(p) }))
        }
    }
    items
}

/// 영상 재생 상태 (출력 화면이 render.js syncVideo 로 맞춘다)
#[derive(Clone, Debug)]
pub struct Video {
    pub key: String,
    pub playing: bool,
    pub offset: f64,
    pub at: i64,
    pub volume: Option<f64>,
    pub looped: Option<bool>,
    pub ended: bool,
    pub fade: Option<(i64, f64)>,
}

impl Video {
    pub fn start(key: &str) -> Self {
        Self { key: key.into(), playing: true, offset: 0.0, at: now_ms(), volume: None, looped: None, ended: false, fade: None }
    }
    pub fn position(&self) -> f64 {
        self.offset + if self.playing { (now_ms() - self.at) as f64 / 1000.0 } else { 0.0 }
    }
    pub fn payload(&self) -> Value {
        let mut v = json!({ "key": self.key, "playing": self.playing, "offset": self.offset, "at": self.at, "now": now_ms() });
        if let Some((at, ms)) = self.fade { v["fade"] = json!({"at":at,"ms":ms}); }
        if let Some(x) = self.volume {
            v["volume"] = json!(x);
        }
        if let Some(x) = self.looped {
            v["loop"] = json!(x);
        }
        v
    }
    /// play | pause | toggle | restart | seek(초) | volume(0~1) | loop(1/0) | stop
    pub fn control(&mut self, act: &str, arg: f64) -> Result<()> {
        let pos = self.position();
        let now = now_ms();
        if act != "fadeout" { self.fade = None; }
        match act {
            "play" => {
                self.playing = true;
                self.offset = pos;
                self.at = now;
                self.ended = false;
            }
            "pause" => {
                self.playing = false;
                self.offset = pos;
                self.at = now;
            }
            "toggle" => {
                self.playing = !self.playing;
                self.offset = pos;
                self.at = now;
            }
            "restart" => {
                self.playing = true;
                self.offset = 0.0;
                self.at = now;
                self.ended = false;
            }
            "seek" => {
                self.offset = arg.max(0.0);
                self.at = now;
            }
            "volume" => self.volume = Some(arg.clamp(0.0, 1.0)),
            "loop" => self.looped = Some(arg > 0.0),
            "fadeout" => self.fade = Some((now, if arg.is_finite() { arg.clamp(1.0, 60000.0) } else { 3000.0 })),
            "stop" => {
                self.playing = false;
                self.offset = 0.0;
                self.at = now;
            }
            _ => bail!("알 수 없는 영상 조작입니다."),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fade_matches_output_contract_and_stop_resets() {
        let mut v = Video::start("test");
        v.control("fadeout", 3000.0).unwrap();
        assert_eq!(v.payload()["fade"]["ms"], 3000.0);
        assert!(v.payload()["fade"]["at"].as_i64().is_some());
        v.control("stop", 0.0).unwrap();
        assert!(!v.playing);
        assert_eq!(v.offset, 0.0);
        assert!(v.payload().get("fade").is_none());
    }
    #[test]
    fn folders_and_drop() {
        let dir = std::env::temp_dir().join(format!("wp-lib-{}", crate::util::short_id("t")));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a 사진.png"), b"x").unwrap();
        std::fs::write(dir.join("sub").join("b.mp4"), b"x").unwrap();
        std::fs::write(dir.join("c.txt"), b"x").unwrap();
        let mut s = json!({});
        assert_eq!(add_folder(&mut s, "image", &dir, None).unwrap()["count"], 2);
        assert!(add_folder(&mut s, "audio", &dir, None).is_err());
        assert_eq!(search_files(&s, "image", "사진", "").len(), 1);
        assert!(search_files(&s, "image", "", "").iter().any(|x| x["video"] == true));
        let it = add_dropped(&mut s, &[dir.join("a 사진.png")]);
        assert_eq!(it[0]["type"], "image");
        assert!(file_url(&dir.join("a 사진.png")).contains("%EC"));
        remove_folder(&mut s, "image", &dir.to_string_lossy());
        assert_eq!(folders(&s, "image").len(), 0);
    }
    #[test]
    fn video() {
        let mut v = Video::start("k");
        v.control("pause", 0.0).unwrap();
        v.control("seek", 12.5).unwrap();
        assert_eq!(v.position(), 12.5);
        v.control("volume", 3.0).unwrap();
        assert_eq!(v.payload()["volume"], 1.0);
    }
}
