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

// ---------------- 연결된 카메라 실시간 목록 ----------------
// Electron 판은 조작 화면(같은 Chromium 세션·출처)에서 getUserMedia → enumerateDevices 로 목록을 얻었다.
// 출력 화면(render.js)은 deviceId 만으로 카메라를 연다 ({ deviceId: { exact } }, 이름으로 찾지 않음).
// Chromium/WebView2 의 deviceId 는 프로필·출처마다 달라서, Rust 판은 예배당 창과 같은 wp-output(WebView2)·
// 같은 주소(http://127.0.0.1:포트)의 도우미 페이지가 목록을 얻어 /api/command 'cameraDevices' 로 보낸다.

/// 한 번에 받는 카메라 수 상한
pub const MAX_DEVICES: usize = 32;
/// deviceId 길이 상한 (Electron presentation.js 와 같음)
pub const MAX_ID: usize = 200;
/// 이름 길이 상한 (Electron presentation.js 와 같음)
pub const MAX_LABEL: usize = 100;
/// 오류 글 길이 상한
pub const MAX_ERROR: usize = 300;
/// 카메라 찾기 요청이 유효한 시간 (밀리초)
pub const PROBE_TTL_MS: i64 = 60_000;

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Device {
    pub device_id: String,
    pub label: String,
}

/// 마지막으로 받은 장치 목록 (코어에 하나, 조작 화면 셋이 함께 씀)
#[derive(Clone, Debug, Default)]
pub struct DeviceList {
    pub devices: Vec<Device>,
    /// 받은 시각 (0 = 아직 찾지 않음)
    pub updated_at: i64,
    /// 도우미가 알린 오류 (권한 거절·장치 없음 등)
    pub error: String,
    /// 찾는 중: (일회용 토큰, 시작 시각)
    probe: Option<(String, i64)>,
}

/// 화면에 보일 카메라 하나
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub device_id: String,
    pub label: String,
    /// 마지막 찾기에서 연결돼 있었음
    pub connected: bool,
    /// 설정(cameras)에 세부 설정이 있음
    pub configured: bool,
}

fn strip_controls(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).collect::<String>().trim().to_string()
}

/// 도우미가 보낸 args 를 검사한다 → (장치 목록, 오류 글)
/// - devices 는 배열, 최대 MAX_DEVICES 개 (넘으면 거절)
/// - deviceId: 1~MAX_ID 자, 보이는 ASCII 만 (넘거나 이상하면 거절). 권한이 없으면 브라우저가 빈 id 를 주므로 빈 id 는 건너뜀
/// - kind 가 있으면 videoinput 만
/// - label: 제어 문자를 빼고 MAX_LABEL 자로 자름
pub fn parse_devices(args: &Value) -> Result<(Vec<Device>, String), String> {
    let error: String = strip_controls(args["error"].as_str().unwrap_or("")).chars().take(MAX_ERROR).collect();
    let list = match &args["devices"] {
        Value::Null => return Ok((vec![], error)),
        Value::Array(a) => a,
        _ => return Err("devices 는 배열이어야 합니다.".into()),
    };
    if list.len() > MAX_DEVICES {
        return Err(format!("카메라가 너무 많습니다 (최대 {MAX_DEVICES}개)."));
    }
    let mut out: Vec<Device> = vec![];
    for d in list {
        if !d.is_object() {
            return Err("장치 항목이 올바르지 않습니다.".into());
        }
        if let Some(kind) = d.get("kind").filter(|k| !k.is_null()) {
            if kind.as_str() != Some("videoinput") {
                continue;
            }
        }
        let id = match &d["deviceId"] {
            Value::String(s) => s.as_str(),
            Value::Null => "",
            _ => return Err("deviceId 는 글자여야 합니다.".into()),
        };
        if id.is_empty() {
            continue;
        }
        if id.chars().count() > MAX_ID || !id.chars().all(|c| c.is_ascii_graphic()) {
            return Err("deviceId 가 너무 길거나 올바르지 않습니다.".into());
        }
        let label = match &d["label"] {
            Value::String(s) => strip_controls(s).chars().take(MAX_LABEL).collect(),
            Value::Null => String::new(),
            _ => return Err("label 은 글자여야 합니다.".into()),
        };
        if !out.iter().any(|x| x.device_id == id) {
            out.push(Device { device_id: id.to_string(), label });
        }
    }
    Ok((out, error))
}

impl DeviceList {
    /// 찾는 중인가 (토큰이 아직 유효)
    pub fn probing(&self, now: i64) -> bool {
        self.probe.as_ref().is_some_and(|(_, at)| now - at < PROBE_TTL_MS)
    }
    /// 새 찾기를 시작하고 일회용 토큰을 돌려준다
    pub fn start_probe(&mut self, now: i64) -> String {
        let token = format!("{:016x}{:016x}", rand::random::<u64>(), rand::random::<u64>());
        self.probe = Some((token.clone(), now));
        token
    }
    /// 찾기를 그만둔다 (도우미를 띄우지 못했을 때)
    pub fn cancel_probe(&mut self, error: &str) {
        self.probe = None;
        self.error = error.chars().take(MAX_ERROR).collect();
    }
    /// 도우미가 보낸 결과를 받는다. 토큰이 맞아야 하고, 한 번 쓰면 끝난다.
    /// 이름이 빈 장치는 전에 받은 이름을 이어 쓴다 (권한이 잠깐 없을 때 이름이 사라지지 않게).
    pub fn receive(&mut self, args: &Value, now: i64) -> Result<usize, String> {
        let token = args["token"].as_str().unwrap_or("");
        match &self.probe {
            Some((t, at)) if !token.is_empty() && t == token && now - at < PROBE_TTL_MS => {}
            _ => return Err("카메라 찾기 요청이 아니거나 시간이 지났습니다.".into()),
        }
        let (mut devices, error) = parse_devices(args)?;
        for d in devices.iter_mut().filter(|d| d.label.is_empty()) {
            if let Some(old) = self.devices.iter().find(|o| o.device_id == d.device_id) {
                d.label = old.label.clone();
            }
        }
        self.probe = None;
        self.devices = devices;
        self.error = error;
        self.updated_at = now;
        Ok(self.devices.len())
    }
}

fn short(id: &str) -> String {
    id.chars().take(12).collect()
}

/// 고를 수 있는 카메라: 연결된 것(마지막 찾기 순서) → 설정에만 있는 것 → 큐시트에만 있는 것.
/// 조작 화면 셋(라이브러리 카메라 탭 · 큐시트 카메라 편집칸 · 카메라 설정 창)이 모두 이 목록을 쓴다.
pub fn choices(live: &[Device], settings: &Value, cue_items: &[Value]) -> Vec<Choice> {
    let saved = settings["cameras"].as_object();
    let saved_label = |id: &str| saved.and_then(|m| m.get(id)).and_then(|p| p["label"].as_str()).filter(|x| !x.is_empty()).map(String::from);
    let configured = |id: &str| saved.is_some_and(|m| m.contains_key(id));
    let mut v: Vec<Choice> = vec![];
    for d in live {
        let label = if d.label.is_empty() { saved_label(&d.device_id).unwrap_or_else(|| short(&d.device_id)) } else { d.label.clone() };
        v.push(Choice { device_id: d.device_id.clone(), label, connected: true, configured: configured(&d.device_id) });
    }
    for (id, _) in saved.into_iter().flatten() {
        if !id.is_empty() && !v.iter().any(|c| &c.device_id == id) {
            v.push(Choice { device_id: id.clone(), label: saved_label(id).unwrap_or_else(|| short(id)), connected: false, configured: true });
        }
    }
    for it in cue_items.iter().filter(|it| it["type"] == "camera") {
        let id = it["deviceId"].as_str().unwrap_or("");
        if !id.is_empty() && !v.iter().any(|c| c.device_id == id) {
            let label = it["label"].as_str().filter(|x| !x.is_empty()).map(String::from).unwrap_or_else(|| short(id));
            v.push(Choice { device_id: id.to_string(), label, connected: false, configured: false });
        }
    }
    v
}

impl Core {
    /// 카메라 찾기 시작 → 도우미 페이지에 넘길 토큰
    pub fn start_camera_probe(&mut self) -> String {
        let t = self.b.cams.start_probe(crate::util::now_ms());
        self.emit(Ev::Ui);
        t
    }
    pub fn cancel_camera_probe(&mut self, error: &str) {
        self.b.cams.cancel_probe(error);
        self.emit(Ev::Ui);
    }
    pub fn camera_probing(&self) -> bool {
        self.b.cams.probing(crate::util::now_ms())
    }
    /// /api/command 'cameraDevices'
    pub fn receive_camera_devices(&mut self, args: &Value) -> Result<usize, String> {
        let n = self.b.cams.receive(args, crate::util::now_ms())?;
        self.emit(Ev::Ui);
        Ok(n)
    }
    /// 조작 화면 셋이 함께 쓰는 카메라 목록 (열린 큐시트 + 저장된 모든 큐시트의 카메라 순서 포함)
    pub fn camera_choices(&self) -> Vec<Choice> {
        let mut items: Vec<Value> = self.cue.as_ref().map(|c| c.items.clone()).unwrap_or_default();
        for c in self.cues.iter().filter(|c| self.cue.as_ref().map(|o| &o.id) != Some(&c.id)).filter_map(|c| self.store.get_cue(&c.id)) {
            items.extend(c.items.into_iter().filter(|it| it["type"] == "camera"));
        }
        // 연결된 휴대폰 카메라(phone:<id>)도 함께 — 출력 화면(render.js)이 WebRTC 로 받는다
        let mut live = self.b.cams.devices.clone();
        live.extend(crate::phonecam::devices());
        choices(&live, &self.settings.get(), &items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_limits() {
        let ok = parse_devices(&json!({ "devices": [
            { "deviceId": "abc123", "label": "  USB 캡처\u{0007}보드 ", "kind": "videoinput" },
            { "deviceId": "mic1", "label": "마이크", "kind": "audioinput" },
            { "deviceId": "", "label": "권한 없음" },
            { "deviceId": "abc123", "label": "중복" },
            { "deviceId": "x", "label": "가".repeat(300) },
        ], "error": "NotAllowedError\n권한" })).unwrap();
        assert_eq!(ok.0.len(), 2);
        assert_eq!(ok.0[0], Device { device_id: "abc123".into(), label: "USB 캡처보드".into() });
        assert_eq!(ok.0[1].label.chars().count(), MAX_LABEL);
        assert_eq!(ok.1, "NotAllowedError권한");
        // 개수 초과
        let many: Vec<Value> = (0..MAX_DEVICES + 1).map(|i| json!({ "deviceId": format!("d{i}") })).collect();
        assert!(parse_devices(&json!({ "devices": many })).is_err());
        let max: Vec<Value> = (0..MAX_DEVICES).map(|i| json!({ "deviceId": format!("d{i}") })).collect();
        assert_eq!(parse_devices(&json!({ "devices": max })).unwrap().0.len(), MAX_DEVICES);
        // id 길이·글자·형식
        assert!(parse_devices(&json!({ "devices": [{ "deviceId": "a".repeat(MAX_ID + 1) }] })).is_err());
        assert!(parse_devices(&json!({ "devices": [{ "deviceId": "a".repeat(MAX_ID) }] })).is_ok());
        assert!(parse_devices(&json!({ "devices": [{ "deviceId": "공백 id" }] })).is_err());
        assert!(parse_devices(&json!({ "devices": [{ "deviceId": 5 }] })).is_err());
        assert!(parse_devices(&json!({ "devices": [{ "deviceId": "a", "label": [] }] })).is_err());
        assert!(parse_devices(&json!({ "devices": "x" })).is_err());
        assert!(parse_devices(&json!({ "devices": ["x"] })).is_err());
        // 장치 없음 (NotFoundError) 도 받는다
        assert_eq!(parse_devices(&json!({ "devices": [], "error": "NotFoundError" })).unwrap(), (vec![], "NotFoundError".into()));
    }

    #[test]
    fn probe_token_and_merge() {
        let mut l = DeviceList::default();
        // 찾기 전 보낸 것은 거절
        assert!(l.receive(&json!({ "token": "", "devices": [] }), 0).is_err());
        let t = l.start_probe(1_000);
        assert!(l.probing(1_000));
        assert!(l.receive(&json!({ "token": "틀림", "devices": [] }), 1_100).is_err());
        assert_eq!(l.receive(&json!({ "token": t, "devices": [{ "deviceId": "a", "label": "캠 A" }, { "deviceId": "b", "label": "캠 B" }] }), 1_200), Ok(2));
        assert!(!l.probing(1_200));
        assert_eq!(l.updated_at, 1_200);
        // 같은 토큰은 다시 못 씀
        assert!(l.receive(&json!({ "token": t, "devices": [] }), 1_300).is_err());
        // 새 찾기: 이름이 빈 장치는 전 이름을 이어 쓰고, 빠진 장치는 목록에서 사라짐
        let t2 = l.start_probe(2_000);
        assert_eq!(l.receive(&json!({ "token": t2, "devices": [{ "deviceId": "a", "label": "" }, { "deviceId": "c", "label": "캠 C" }], "error": "" }), 2_100), Ok(2));
        assert_eq!(l.devices, vec![Device { device_id: "a".into(), label: "캠 A".into() }, Device { device_id: "c".into(), label: "캠 C".into() }]);
        // 시간이 지난 토큰
        let t3 = l.start_probe(3_000);
        assert!(l.receive(&json!({ "token": t3, "devices": [] }), 3_000 + PROBE_TTL_MS).is_err());
        assert_eq!(l.devices.len(), 2, "거절된 결과는 목록을 바꾸지 않음");
        // 잘못된 결과도 목록을 바꾸지 않음
        let t4 = l.start_probe(4_000);
        assert!(l.receive(&json!({ "token": t4, "devices": [{ "deviceId": "a".repeat(MAX_ID + 1) }] }), 4_001).is_err());
        assert_eq!(l.devices.len(), 2);
    }

    #[test]
    fn choices_merge_live_saved_cue() {
        let live = vec![Device { device_id: "a".into(), label: "캠 A".into() }, Device { device_id: "b".into(), label: "".into() }];
        let settings = json!({ "cameras": { "b": { "label": "저장된 B" }, "z": { "label": "" } } });
        let items = vec![json!({ "type": "camera", "deviceId": "a", "label": "옛 이름" }), json!({ "type": "camera", "deviceId": "q", "label": "큐 카메라" }), json!({ "type": "camera", "deviceId": "" }), json!({ "type": "title", "deviceId": "t" })];
        let v = choices(&live, &settings, &items);
        let ids: Vec<(&str, &str, bool, bool)> = v.iter().map(|c| (c.device_id.as_str(), c.label.as_str(), c.connected, c.configured)).collect();
        assert_eq!(ids, vec![("a", "캠 A", true, false), ("b", "저장된 B", true, true), ("z", "z", false, true), ("q", "큐 카메라", false, false)]);
    }

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
