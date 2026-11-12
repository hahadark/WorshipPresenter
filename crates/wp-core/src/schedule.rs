//! 예약 실행 편집 (settings.schedule: [{ id, days: [0~6], time: "HH:MM", macroId, enabled }]) [트랙 B]
//! 실행 자체는 overlays::due_schedules · Core::check_schedule 이 한다.

use crate::core::{Core, Ev};
use crate::util::short_id;
use anyhow::{bail, Result};
use serde_json::{json, Value};

/// 칸 정리: 요일은 0~6 겹치지 않게, 시각은 HH:MM
pub fn normalize(s: &Value) -> Result<Value> {
    let time = s["time"].as_str().unwrap_or("").trim().to_string();
    let ok = time.len() == 5
        && time.as_bytes()[2] == b':'
        && time[..2].parse::<u32>().is_ok_and(|h| h < 24)
        && time[3..].parse::<u32>().is_ok_and(|m| m < 60);
    if !ok {
        bail!("시각은 HH:MM (예: 10:55) 으로 쓰세요.");
    }
    let mut days: Vec<u64> = s["days"].as_array().into_iter().flatten().filter_map(|d| d.as_u64()).filter(|d| *d < 7).collect();
    days.sort();
    days.dedup();
    let id = s["id"].as_str().filter(|x| !x.is_empty()).map(String::from).unwrap_or_else(|| short_id("sc"));
    Ok(json!({ "id": id, "days": days, "time": time, "macroId": s["macroId"].as_str().unwrap_or(""), "enabled": s["enabled"].as_bool().unwrap_or(true) }))
}

pub fn save(d: &mut Value, s: &Value) -> Result<String> {
    let n = normalize(s)?;
    let id = n["id"].as_str().unwrap().to_string();
    if !d["schedule"].is_array() {
        d["schedule"] = json!([]);
    }
    let list = d["schedule"].as_array_mut().unwrap();
    match list.iter().position(|x| x["id"] == id.as_str()) {
        Some(i) => list[i] = n,
        None => list.push(n),
    }
    Ok(id)
}

pub fn delete(d: &mut Value, id: &str) {
    if let Some(l) = d["schedule"].as_array_mut() {
        l.retain(|x| x["id"] != id);
    }
}

pub const DAY_NAMES: [&str; 7] = ["일", "월", "화", "수", "목", "금", "토"];

impl Core {
    pub fn save_schedule(&mut self, s: &Value) -> Result<String> {
        let mut out = Ok(String::new());
        self.settings.update(|d| out = save(d, s));
        self.emit(Ev::Ui);
        out
    }
    pub fn delete_schedule(&mut self, id: &str) {
        self.settings.update(|d| delete(d, id));
        self.emit(Ev::Ui);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn edit() {
        let mut d = json!({});
        let id = save(&mut d, &json!({ "days": [0, 0, 9, 3], "time": "10:55", "macroId": "m1" })).unwrap();
        assert_eq!(d["schedule"][0]["days"], json!([0, 3]));
        assert!(save(&mut d, &json!({ "time": "25:00" })).is_err());
        save(&mut d, &json!({ "id": id, "days": [1], "time": "09:00", "macroId": "m1", "enabled": false })).unwrap();
        assert_eq!(d["schedule"].as_array().unwrap().len(), 1);
        assert_eq!(d["schedule"][0]["enabled"], json!(false));
        delete(&mut d, &id);
        assert!(d["schedule"].as_array().unwrap().is_empty());
    }
}
