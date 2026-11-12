//! [트랙 C] 템플릿 저장·복제·삭제 (Electron core.js saveTemplate·duplicateTemplate·deleteTemplate)

use crate::settings::{normalize_template, Settings};
use crate::util::{now_ms, to36};
use serde_json::{json, Value};

/// 정수로 반올림해 범위 안으로 (core.js clampInt). 숫자가 아니면 def
pub fn clamp_int(v: &Value, min: i64, max: i64, def: i64) -> i64 {
    let n = match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    };
    match n {
        Some(x) if x.is_finite() => (x.round() as i64).clamp(min, max),
        _ => def,
    }
}

/// 템플릿 저장 (새 것이면 id 를 만들고 preset=false). 저장된 템플릿을 돌려준다
pub fn save_template(st: &mut Settings, tpl: &Value) -> Result<Value, String> {
    let mut t = normalize_template(tpl);
    if t["name"].as_str().map(|s| s.trim().is_empty()).unwrap_or(true) {
        return Err(crate::i18n::tr("템플릿 이름을 입력하세요.").into());
    }
    t["width"] = json!(clamp_int(&t["width"], 320, 7680, 1920));
    t["height"] = json!(clamp_int(&t["height"], 180, 4320, 1080));
    let size = clamp_int(&t["font"]["size"], 8, 400, 72);
    t["font"]["size"] = json!(size);
    t["font"]["minSize"] = json!(clamp_int(&t["font"]["minSize"], 8, size, 44.min(size)));
    let id = t["id"].as_str().unwrap_or("").to_string();
    let mut out = t.clone();
    st.update(|d| {
        let list = d["templates"].as_array_mut().expect("templates");
        match list.iter().position(|x| x["id"] == id.as_str()).filter(|_| !id.is_empty()) {
            Some(i) => {
                out["preset"] = list[i]["preset"].clone();
                list[i] = out.clone();
            }
            None => {
                if id.is_empty() {
                    out["id"] = json!(format!("tpl-{}", to36(now_ms() as u64)));
                }
                out["preset"] = json!(false);
                list.push(out.clone());
            }
        }
    });
    Ok(out)
}

pub fn duplicate_template(st: &mut Settings, id: &str, name: Option<&str>) -> Result<Value, String> {
    let src = st.template(id).ok_or("템플릿을 찾을 수 없습니다.")?;
    let mut copy = src.clone();
    copy["id"] = json!(format!("tpl-{}", to36(now_ms() as u64)));
    copy["name"] = json!(name.map(String::from).unwrap_or_else(|| format!("{} 복사본", src["name"].as_str().unwrap_or(""))));
    copy["preset"] = json!(false);
    let c2 = copy.clone();
    st.update(|d| d["templates"].as_array_mut().unwrap().push(c2));
    Ok(copy)
}

pub fn delete_template(st: &mut Settings, id: &str) -> Result<(), String> {
    let s = st.get();
    if s["templates"].as_array().map(|a| a.len()).unwrap_or(0) <= 1 {
        return Err(crate::i18n::tr("마지막 템플릿은 삭제할 수 없습니다.").into());
    }
    let in_use: Vec<&str> = s["activeTemplates"]
        .as_object()
        .map(|o| o.iter().filter(|(_, v)| *v == id).map(|(k, _)| if k == "main" { "전체 화면" } else { "OBS 자막" }).collect())
        .unwrap_or_default();
    if !in_use.is_empty() {
        return Err(crate::i18n::tr_args("현재 {1} 출력에서 사용 중인 템플릿입니다. 다른 템플릿을 먼저 지정하세요.", &[&(in_use.join(", "))]));
    }
    let id = id.to_string();
    st.update(|d| d["templates"].as_array_mut().unwrap().retain(|t| t["id"] != id.as_str()));
    Ok(())
}

/// 송출 기록 지우기 (clearHistory)
pub fn clear_history(st: &mut Settings) {
    st.update(|d| d["history"] = json!([]));
}

#[cfg(test)]
mod tests {
    use super::*;
    fn settings() -> Settings {
        let dir = std::env::temp_dir().join(format!("wpc-tpl-{}", now_ms()));
        Settings::load(&dir)
    }
    #[test]
    fn clamp() {
        assert_eq!(clamp_int(&json!(9999), 8, 400, 72), 400);
        assert_eq!(clamp_int(&json!("50.6"), 8, 400, 72), 51);
        assert_eq!(clamp_int(&json!(null), 8, 400, 72), 72);
    }
    #[test]
    fn save_dup_delete() {
        let mut s = settings();
        let n0 = s.templates().len();
        assert!(save_template(&mut s, &json!({ "name": " " })).is_err());
        let t = save_template(&mut s, &json!({ "name": "새 템플릿", "font": { "size": 1000, "minSize": 2000 } })).unwrap();
        assert_eq!(t["font"]["size"], 400);
        assert_eq!(t["font"]["minSize"], 400);
        assert_eq!(t["preset"], false);
        assert_eq!(s.templates().len(), n0 + 1);
        let id = t["id"].as_str().unwrap().to_string();
        let mut t2 = t.clone();
        t2["name"] = json!("바꾼 이름");
        save_template(&mut s, &t2).unwrap();
        assert_eq!(s.template(&id).unwrap()["name"], "바꾼 이름");
        assert_eq!(s.templates().len(), n0 + 1);
        let d = duplicate_template(&mut s, &id, None).unwrap();
        assert_eq!(d["name"], "바꾼 이름 복사본");
        assert!(delete_template(&mut s, "worship").is_err()); // 사용 중
        delete_template(&mut s, &id).unwrap();
        assert!(s.template(&id).is_none());
        clear_history(&mut s);
        assert_eq!(s.get()["history"], json!([]));
    }
}
