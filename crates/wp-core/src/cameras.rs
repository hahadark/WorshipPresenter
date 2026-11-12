//! 카메라마다 세부 설정 (Electron core.js setCameraPrefs → settings.cameras) [트랙 B]
//! { deviceId: { label, res: auto|720|1080|1440|2160, fps: 0|24|25|30|50|60, exact, mirror, brightness 50~200, contrast 50~200, saturation 0~200 } }
//! 출력 화면(render.js)이 outputPayload.cameras 로 받아 카메라를 열 때 쓴다.

use crate::core::{Core, Ev};
use serde_json::{json, Value};

fn num(v: &Value, lo: i64, hi: i64, d: i64) -> i64 {
    v.as_f64().filter(|x| x.is_finite()).map(|x| (x.round() as i64).clamp(lo, hi)).unwrap_or(d)
}

pub fn norm_prefs(p: &Value) -> Value {
    let res = p["res"].as_str().filter(|r| ["auto", "720", "1080", "1440", "2160"].contains(r)).unwrap_or("auto");
    let fps = p["fps"].as_i64().filter(|f| [0, 24, 25, 30, 50, 60].contains(f)).unwrap_or(0);
    json!({
        "label": p["label"].as_str().unwrap_or("").chars().take(200).collect::<String>(),
        "res": res, "fps": fps,
        "exact": p["exact"].as_bool().unwrap_or(false), "mirror": p["mirror"].as_bool().unwrap_or(false),
        "brightness": num(&p["brightness"], 50, 200, 100), "contrast": num(&p["contrast"], 50, 200, 100), "saturation": num(&p["saturation"], 0, 200, 100),
    })
}

/// prefs 가 None 이면 그 카메라 설정을 지운다. 30대를 넘으면 오래된 것부터
pub fn set(d: &mut Value, device_id: &str, prefs: Option<&Value>) {
    let id: String = device_id.chars().take(300).collect();
    if !d["cameras"].is_object() {
        d["cameras"] = json!({});
    }
    let cams = d["cameras"].as_object_mut().unwrap();
    match prefs {
        None => {
            cams.remove(&id);
        }
        Some(p) => {
            cams.insert(id, norm_prefs(p));
            while cams.len() > 30 {
                let first = cams.keys().next().cloned().unwrap();
                cams.remove(&first);
            }
        }
    }
}

impl Core {
    pub fn set_camera_prefs(&mut self, device_id: &str, prefs: Option<&Value>) -> Value {
        self.settings.update(|d| set(d, device_id, prefs));
        self.emit(Ev::Live);
        self.emit(Ev::Ui);
        self.settings.get()["cameras"].clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prefs() {
        let mut d = json!({});
        set(&mut d, "cam1", Some(&json!({ "label": "캡처보드", "res": "1080", "fps": 30, "mirror": true, "brightness": 999, "saturation": -5 })));
        assert_eq!(d["cameras"]["cam1"]["res"], "1080");
        assert_eq!(d["cameras"]["cam1"]["brightness"], 200);
        assert_eq!(d["cameras"]["cam1"]["saturation"], 0);
        set(&mut d, "cam2", Some(&json!({ "res": "999", "fps": 7 })));
        assert_eq!(d["cameras"]["cam2"]["res"], "auto");
        assert_eq!(d["cameras"]["cam2"]["fps"], 0);
        set(&mut d, "cam1", None);
        assert!(d["cameras"].get("cam1").is_none());
    }
}
