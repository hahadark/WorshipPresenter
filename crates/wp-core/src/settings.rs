//! 설정 (settings.json). Electron 판과 같은 JSON 을 그대로 쓴다.
//! 모르는 칸도 지우지 않도록 serde_json::Value 로 들고 있고, 기본값을 깊게 합친다.

use anyhow::Result;
use once_cell::sync::Lazy;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

pub static DEFAULTS: Lazy<Value> = Lazy::new(|| serde_json::from_str(include_str!("../data/defaults.json")).unwrap());
pub static TEMPLATE_BASE: Lazy<Value> = Lazy::new(|| serde_json::from_str(include_str!("../data/template-base.json")).unwrap());
pub static SHORTCUTS: Lazy<Value> = Lazy::new(|| serde_json::from_str(include_str!("../data/shortcuts.json")).unwrap());

/// 기본값 위에 저장 값을 깊게 합친다 (settings.js deepMergeDefaults)
pub fn deep_merge_defaults(def: &Value, val: &Value) -> Value {
    if val.is_null() {
        return def.clone();
    }
    match (def, val) {
        (Value::Object(d), Value::Object(v)) => {
            let mut out = d.clone();
            for (k, x) in v {
                let merged = match d.get(k) {
                    Some(dk) => deep_merge_defaults(dk, x),
                    None => x.clone(),
                };
                out.insert(k.clone(), merged);
            }
            Value::Object(out)
        }
        // 기본값이 묶음·목록인데 파일에는 다른 모양(글·숫자 등)이 들어 있으면 기본값으로 — 손으로 고치다 망가진 설정에서
        // d["a"]["b"] = … 같은 자리가 panic 하지 않게 (serde_json 은 목록·글에 열쇠로 쓰면 panic 한다)
        (Value::Object(_), _) | (Value::Array(_), _) if std::mem::discriminant(def) != std::mem::discriminant(val) => def.clone(),
        _ => val.clone(),
    }
}

/// templates.js merge: 객체끼리는 깊게, 그 밖은 덮어씀
pub fn merge_into(target: &mut Value, patch: &Value) {
    if let (Value::Object(t), Value::Object(p)) = (&mut *target, patch) {
        for (k, v) in p {
            match t.get_mut(k) {
                Some(tv) if v.is_object() && tv.is_object() => merge_into(tv, v),
                _ => {
                    t.insert(k.clone(), v.clone());
                }
            }
        }
    }
}

/// 저장된 템플릿에 새 버전의 기본 칸을 채운다 (templates.js normalizeTemplate)
pub fn normalize_template(t: &Value) -> Value {
    let mut out = TEMPLATE_BASE.clone();
    merge_into(&mut out, t);
    let anim = out["animation"]["type"].as_str().unwrap_or("").to_string();
    out["animation"]["type"] = json!(if anim == "fade" || anim == "dissolve" { "dissolve" } else { "cut" });
    let pm = out["pptMode"].as_str().unwrap_or("").to_string();
    if !["", "image", "lyrics"].contains(&pm.as_str()) {
        out["pptMode"] = json!("");
    }
    out
}

enum BgJob {
    Write(PathBuf, String, std::sync::Arc<BgState>),
    Sync(std::sync::mpsc::Sender<()>),
}
static BG_STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// 실패한 뒤 다시 해 보기까지 (ms). 디스크가 찼거나 폴더를 못 쓸 때 1초마다 다시 쓰며 로그를 채우지 않게
const BG_RETRY_MS: i64 = 10_000;
/// 백그라운드 저장 상태 (설정 하나마다)
#[derive(Default)]
struct BgState {
    failed: std::sync::atomic::AtomicBool,
    /// 실패한 뒤 다시 해 볼 시각 (ms)
    retry_at: std::sync::atomic::AtomicI64,
    /// (로그에 남긴 마지막 오류, 아직 사용자에게 알리지 않은 오류)
    err: parking_lot::Mutex<(Option<String>, Option<String>)>,
}
/// 설정 파일 쓰기 일꾼 (차례대로 하나씩)
static BG: Lazy<parking_lot::Mutex<std::sync::mpsc::Sender<BgJob>>> = Lazy::new(|| {
    let (tx, rx) = std::sync::mpsc::channel::<BgJob>();
    BG_STARTED.store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = std::thread::Builder::new().name("settings-save".into()).spawn(move || {
        for job in rx {
            match job {
                BgJob::Write(file, txt, st) => {
                    let tmp = file.with_extension("json.tmp");
                    let r = (|| -> std::io::Result<()> {
                        if let Some(d) = file.parent() {
                            fs::create_dir_all(d)?;
                        }
                        fs::write(&tmp, txt)?;
                        fs::rename(&tmp, &file)
                    })();
                    let mut last = st.err.lock();
                    if let Err(e) = r {
                        let msg = e.to_string();
                        if last.0.as_deref() != Some(msg.as_str()) {
                            log::error!("설정 저장 실패: {msg} ({})", file.display());
                            last.0 = Some(msg.clone());
                            last.1 = Some(msg);
                        }
                        st.retry_at.store(crate::util::now_ms() + BG_RETRY_MS, std::sync::atomic::Ordering::Relaxed);
                        st.failed.store(true, std::sync::atomic::Ordering::Relaxed);
                    } else if last.0.take().is_some() {
                        last.1 = None;
                        log::info!("설정 저장이 다시 됩니다 ({})", file.display());
                    }
                }
                BgJob::Sync(done) => {
                    let _ = done.send(());
                }
            }
        }
    });
    parking_lot::Mutex::new(tx)
});
/// 맡겨 둔 저장이 모두 끝날 때까지 (최대 5초)
fn bg_wait() {
    if !BG_STARTED.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let (tx, rx) = std::sync::mpsc::channel();
    if BG.lock().send(BgJob::Sync(tx)).is_ok() {
        let _ = rx.recv_timeout(std::time::Duration::from_secs(5));
    }
}

pub struct Settings {
    pub file: PathBuf,
    pub backup: PathBuf,
    pub data: Value,
    dirty: bool,
    bg: std::sync::Arc<BgState>,
}

impl Settings {
    pub fn load(dir: &Path) -> Self {
        let file = dir.join("settings.json");
        let backup = dir.join("settings.backup.json");
        let mut data = None;
        for f in [&file, &backup] {
            let Ok(txt) = fs::read_to_string(f) else { continue };
            let Ok(raw) = serde_json::from_str::<Value>(txt.trim_start_matches('\u{feff}')) else {
                log::error!("설정 읽기 실패: {}", f.display());
                continue;
            };
            if !raw.is_object() {
                // JSON 이기는 하지만 설정 묶음이 아니다 ([1,2,3] · "글") — 읽지 못한 것으로 보고 백업 파일을 본다
                log::error!("설정 읽기 실패 (모양이 설정이 아님): {}", f.display());
                continue;
            }
            let mut d = deep_merge_defaults(&DEFAULTS, &raw);
            let mut sc = DEFAULTS["shortcuts"].clone();
            merge_into(&mut sc, &raw["shortcuts"]);
            d["shortcuts"] = sc;
            let tpls: Vec<Value> = match raw["templates"].as_array() {
                Some(a) if !a.is_empty() => a.iter().map(normalize_template).collect(),
                _ => DEFAULTS["templates"].as_array().cloned().unwrap_or_default(),
            };
            d["templates"] = json!(tpls);
            if !d["templates"].as_array().unwrap().iter().any(|t| t["id"] == "overlay-lyrics") {
                if let Some(p) = DEFAULTS["templates"].as_array().and_then(|a| a.iter().find(|t| t["id"] == "overlay-lyrics")) {
                    d["templates"].as_array_mut().unwrap().push(normalize_template(p));
                }
            }
            if f == &file {
                let _ = fs::copy(f, &backup);
            }
            data = Some(d);
            break;
        }
        Self { file, backup, data: data.unwrap_or_else(|| DEFAULTS.clone()), dirty: false, bg: Default::default() }
    }

    pub fn get(&self) -> &Value {
        &self.data
    }

    pub fn update(&mut self, f: impl FnOnce(&mut Value)) {
        f(&mut self.data);
        self.dirty = true;
    }

    /// 바뀐 것이 있으면 원자적으로 저장 (임시 파일 → 이름 바꾸기). 백그라운드에 맡긴 저장이 있으면 그것이 끝난 뒤에 돌아온다
    pub fn save_if_dirty(&mut self) -> Result<()> {
        if !self.dirty && !self.bg.failed.load(std::sync::atomic::Ordering::Relaxed) {
            bg_wait();
            return Ok(());
        }
        self.save_now()
    }

    /// 바뀐 것이 있으면 글로만 바꿔(빠름) 파일 쓰기는 백그라운드 스레드에 맡긴다.
    /// 조작 화면이 1초마다 부른다 — 파일 쓰기(백신 검사·느린 디스크·원드라이브)가 0.4~0.9초 걸리는 동안
    /// 코어 잠금을 쥐고 있어 화면과 출력 요청이 멈칫하던 것을 없앤다. 끌 때는 save_if_dirty 가 끝까지 기다린다.
    pub fn save_bg(&mut self) {
        if self.bg.failed.load(std::sync::atomic::Ordering::Relaxed) {
            self.dirty = true; // 지난 백그라운드 저장이 실패했으면 다시 — 다만 10초에 한 번만
            if crate::util::now_ms() < self.bg.retry_at.load(std::sync::atomic::Ordering::Relaxed) {
                return;
            }
            self.bg.failed.store(false, std::sync::atomic::Ordering::Relaxed);
        }
        if !self.dirty {
            return;
        }
        let Ok(txt) = serde_json::to_string_pretty(&self.data) else { return };
        if BG.lock().send(BgJob::Write(self.file.clone(), txt, self.bg.clone())).is_ok() {
            self.dirty = false;
        } else if let Err(e) = self.save_now() {
            log::error!("설정 저장 실패: {e}");
        }
    }

    /// 백그라운드 저장이 실패했으면 그 오류 글 (같은 오류가 이어지는 동안 한 번만) — 조작 화면 알림용
    pub fn take_save_error(&self) -> Option<String> {
        self.bg.err.lock().1.take()
    }

    pub fn save_now(&mut self) -> Result<()> {
        bg_wait(); // 같은 임시 파일을 쓰므로 맡겨 둔 저장이 끝난 뒤에
        self.bg.failed.store(false, std::sync::atomic::Ordering::Relaxed);
        if let Some(d) = self.file.parent() {
            fs::create_dir_all(d)?;
        }
        let tmp = self.file.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_string_pretty(&self.data)?)?;
        fs::rename(&tmp, &self.file)?;
        self.dirty = false;
        Ok(())
    }

    pub fn templates(&self) -> Vec<Value> {
        self.data["templates"].as_array().cloned().unwrap_or_default()
    }

    pub fn template(&self, id: &str) -> Option<Value> {
        self.data["templates"].as_array()?.iter().find(|t| t["id"] == id).cloned()
    }

    pub fn str(&self, path: &[&str]) -> String {
        let mut v = &self.data;
        for p in path {
            v = &v[*p];
        }
        v.as_str().unwrap_or("").to_string()
    }
}

#[cfg(test)]
mod bg_save_tests {
    use super::*;

    /// 백그라운드 저장: 맡긴 뒤 save_if_dirty 가 끝까지 기다리고, 파일에 바뀐 값이 들어 있다
    #[test]
    fn save_bg_writes_file_and_flush_waits() {
        let dir = std::env::temp_dir().join(format!("wp-settings-bg-{}", crate::util::short_id("t")));
        fs::create_dir_all(&dir).unwrap();
        let mut st = Settings::load(&dir);
        for i in 0..20 {
            st.update(|d| d["auditE"] = json!(i));
            st.save_bg();
        }
        st.save_if_dirty().unwrap(); // 바뀐 것은 없지만 맡긴 저장이 끝날 때까지 기다린다
        let v: Value = serde_json::from_str(&fs::read_to_string(dir.join("settings.json")).unwrap()).unwrap();
        assert_eq!(v["auditE"], json!(19));
        assert!(!dir.join("settings.json.tmp").exists());
        // 바로 저장(save_now)과 섞여도 마지막 값이 남는다
        st.update(|d| d["auditE"] = json!(100));
        st.save_bg();
        st.update(|d| d["auditE"] = json!(101));
        st.save_now().unwrap();
        let v: Value = serde_json::from_str(&fs::read_to_string(dir.join("settings.json")).unwrap()).unwrap();
        assert_eq!(v["auditE"], json!(101));
        fs::remove_dir_all(&dir).ok();
    }

    /// 쓸 수 없는 자리(폴더 자리에 파일이 있음): 오류는 한 번만 알리고, 바로 되풀이하지 않으며, 끌 때 저장은 오류를 돌려준다
    #[test]
    fn save_bg_failure_is_reported_once_and_backs_off() {
        let dir = std::env::temp_dir().join(format!("wp-settings-bgfail-{}", crate::util::short_id("t")));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("막힘"), b"x").unwrap();
        let mut st = Settings::load(&dir);
        st.file = dir.join("막힘").join("settings.json");
        st.update(|d| d["auditE"] = json!(1));
        st.save_bg();
        bg_wait();
        let e = st.take_save_error();
        assert!(e.is_some(), "실패하면 알릴 오류 글이 있어야 함");
        assert!(st.take_save_error().is_none(), "같은 오류는 한 번만");
        // 바로 다시 부르면 쓰지 않고(10초 뒤에 다시) 바뀐 것은 그대로 남아 있다
        st.save_bg();
        bg_wait();
        assert!(st.take_save_error().is_none());
        assert!(st.dirty);
        assert!(st.save_if_dirty().is_err(), "끌 때 저장은 실패를 돌려준다");
        // 자리가 풀리면 다시 저장된다
        st.file = dir.join("settings.json");
        st.save_now().unwrap();
        let v: Value = serde_json::from_str(&fs::read_to_string(dir.join("settings.json")).unwrap()).unwrap();
        assert_eq!(v["auditE"], json!(1));
        fs::remove_dir_all(&dir).ok();
    }
}
