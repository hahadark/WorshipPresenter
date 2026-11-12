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
        Self { file, backup, data: data.unwrap_or_else(|| DEFAULTS.clone()), dirty: false }
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
