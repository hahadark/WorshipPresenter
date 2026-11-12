//! [트랙 C] 테마 프리셋 (Electron themes.js 를 그대로 뽑은 data/themes.json: 구세군 5 + 방송 스타일 5)
//! 테마마다 예배당(main)·OBS 자막(overlay) 템플릿 한 쌍. applyTheme · ensureTheme · OBS 보기(setObsView)

use crate::core::{Core, Ev};
use once_cell::sync::Lazy;
use serde_json::{json, Value};

pub static THEMES: Lazy<Vec<Value>> = Lazy::new(|| serde_json::from_str(include_str!("../data/themes.json")).expect("themes.json"));

const OUTPUTS: [&str; 2] = ["main", "overlay"];

pub fn list_themes() -> &'static [Value] {
    &THEMES
}

fn find(id: &str) -> Result<&'static Value, String> {
    THEMES.iter().find(|t| t["id"] == id).ok_or_else(|| "테마를 찾을 수 없습니다.".to_string())
}

/// 테마 적용: 테마 템플릿을 만들거나 기본값으로 되돌린 뒤 출력에 지정 (outputs 가 비면 둘 다)
pub fn apply_theme(c: &mut Core, id: &str, outputs: &[&str]) -> Result<Vec<String>, String> {
    let theme = find(id)?;
    let outs: Vec<&str> = if outputs.is_empty() { OUTPUTS.to_vec() } else { outputs.iter().copied().filter(|o| OUTPUTS.contains(o)).collect() };
    c.settings.update(|d| {
        for o in &outs {
            let tpl = crate::settings::normalize_template(&theme[*o]);
            let tid = tpl["id"].clone();
            let list = d["templates"].as_array_mut().unwrap();
            match list.iter().position(|x| x["id"] == tid) {
                Some(i) => list[i] = tpl,
                None => list.push(tpl),
            }
            d["activeTemplates"][*o] = tid;
        }
    });
    c.emit(Ev::Live);
    c.emit(Ev::Ui);
    Ok(outs.iter().map(|s| s.to_string()).collect())
}

/// 순서별 테마: 테마의 두 템플릿이 없으면 만들고 id 를 돌려준다 (이미 고친 템플릿은 그대로)
pub fn ensure_theme(c: &mut Core, id: &str) -> Result<Value, String> {
    let theme = find(id)?;
    let mut ids = json!({});
    let mut added = false;
    c.settings.update(|d| {
        for o in OUTPUTS {
            let tpl = crate::settings::normalize_template(&theme[o]);
            ids[o] = tpl["id"].clone();
            let list = d["templates"].as_array_mut().unwrap();
            if !list.iter().any(|x| x["id"] == tpl["id"]) {
                list.push(tpl);
                added = true;
            }
        }
    });
    if added {
        c.emit(Ev::Ui);
    }
    Ok(ids)
}

/// OBS 자막 보기: full = '슬라이드 그대로' (예배당 템플릿으로), pip = 그림 슬라이드 우상단
pub fn set_obs_view(c: &mut Core, full: Option<bool>, pip: Option<bool>) -> Value {
    if let Some(f) = full {
        c.obs_full = f;
    }
    if let Some(p) = pip {
        c.settings.update(|d| d["output"]["obsPip"] = json!(p));
    }
    c.emit(Ev::Live);
    c.emit(Ev::Ui);
    obs_view(c)
}

pub fn obs_view(c: &Core) -> Value {
    json!({ "full": c.obs_full, "pip": c.settings.get()["output"]["obsPip"].as_bool().unwrap_or(false) })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn presets() {
        assert_eq!(THEMES.len(), 10);
        let r = &THEMES[0];
        assert_eq!(r["main"]["id"], json!("theme-sa-redshield-main"));
        assert_eq!(r["overlay"]["background"]["type"], json!("transparent"));
        assert!(THEMES.iter().all(|t| t["main"]["width"].is_number() && t["name"].is_string()));
    }
}
