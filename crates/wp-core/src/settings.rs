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

pub struct Settings {
    pub file: PathBuf,
    pub backup: PathBuf,
    pub data: Value,
    dirty: bool,
    /// 읽을 때 생긴 일 (설정 파일이 깨져 백업본·기본값으로 시작함). 켠 뒤 화면에 알린다
    pub load_note: Option<String>,
}

impl Settings {
    pub fn load(dir: &Path) -> Self {
        let file = dir.join("settings.json");
        let backup = dir.join("settings.backup.json");
        let mut data = None;
        let mut broken: Option<PathBuf> = None; // 깨진 settings.json 을 보존해 둔 곳
        let mut main_bad = false;
        for f in [&file, &backup] {
            let Ok(txt) = fs::read_to_string(f) else {
                if f == &file && f.exists() {
                    main_bad = true;
                    log::error!("설정 파일을 읽지 못했습니다: {}", f.display());
                }
                continue;
            };
            let Ok(raw) = serde_json::from_str::<Value>(txt.trim_start_matches('\u{feff}')) else {
                log::error!("설정 읽기 실패(내용이 깨짐): {}", f.display());
                if f == &file {
                    // 깨진 파일은 지우거나 덮어쓰지 않고 따로 둔다 (손으로 되살릴 수 있게)
                    main_bad = true;
                    let ts: String = crate::util::iso_now().chars().take(19).map(|c| if c == ':' || c == 'T' { '-' } else { c }).collect();
                    let keep = dir.join(format!("settings.broken-{ts}.json"));
                    match fs::copy(f, &keep) {
                        Ok(_) => {
                            log::error!("깨진 설정 파일을 보존했습니다: {}", keep.display());
                            broken = Some(keep);
                        }
                        Err(e) => log::error!("깨진 설정 파일 보존 실패: {e}"),
                    }
                }
                continue;
            };
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
        let load_note = if main_bad {
            let kept = broken.map(|p| crate::i18n::tr_args(" 깨진 파일은 {1} 로 남겨 두었습니다.", &[&p.display()])).unwrap_or_default();
            let msg = if data.is_some() {
                crate::i18n::tr("설정 파일(settings.json)이 깨져 있어 바로 앞에 저장된 백업 설정으로 켰습니다. 가장 최근에 바꾼 설정 일부가 빠졌을 수 있습니다.")
            } else {
                crate::i18n::tr("설정 파일(settings.json)이 깨져 있고 백업 설정도 없어 기본 설정으로 켰습니다.")
            };
            log::error!("{msg}{kept}");
            Some(format!("{msg}{kept}"))
        } else {
            None
        };
        Self { file, backup, data: data.unwrap_or_else(|| DEFAULTS.clone()), dirty: false, load_note }
    }

    pub fn get(&self) -> &Value {
        &self.data
    }

    pub fn update(&mut self, f: impl FnOnce(&mut Value)) {
        f(&mut self.data);
        self.dirty = true;
    }

    /// 바뀐 것이 있으면 원자적으로 저장 (임시 파일 → 이름 바꾸기)
    pub fn save_if_dirty(&mut self) -> Result<()> {
        if !self.dirty {
            return Ok(());
        }
        self.save_now()
    }

    pub fn save_now(&mut self) -> Result<()> {
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
mod broken_tests {
    use super::*;

    #[test]
    fn broken_settings_is_kept_and_reported() {
        let d = std::env::temp_dir().join(format!("wp-broken-{}", rand::random::<u64>()));
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("settings.backup.json"), r#"{"port":40001}"#).unwrap();
        fs::write(d.join("settings.json"), r#"{"port":40002,"#).unwrap(); // 쓰다 끊긴 파일
        let s = Settings::load(&d);
        assert_eq!(s.get()["port"], 40001, "백업본으로 켠다");
        let note = s.load_note.clone().expect("알림이 있어야 함");
        assert!(note.contains("settings.broken-"), "{note}");
        let kept: Vec<_> = fs::read_dir(&d).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().starts_with("settings.broken-")).collect();
        assert_eq!(kept.len(), 1);
        assert_eq!(fs::read_to_string(kept[0].path()).unwrap(), r#"{"port":40002,"#, "깨진 내용 그대로 보존");
        // 백업도 없으면 기본 설정 + 알림
        let d2 = std::env::temp_dir().join(format!("wp-broken2-{}", rand::random::<u64>()));
        fs::create_dir_all(&d2).unwrap();
        fs::write(d2.join("settings.json"), "not json").unwrap();
        let s2 = Settings::load(&d2);
        assert!(s2.load_note.is_some());
        assert_eq!(s2.get()["port"], DEFAULTS["port"]);
        // 멀쩡하면 알림 없음
        let d3 = std::env::temp_dir().join(format!("wp-broken3-{}", rand::random::<u64>()));
        fs::create_dir_all(&d3).unwrap();
        fs::write(d3.join("settings.json"), "{}").unwrap();
        assert!(Settings::load(&d3).load_note.is_none());
        for x in [d, d2, d3] {
            let _ = fs::remove_dir_all(x);
        }
    }
}
