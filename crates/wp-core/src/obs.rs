//! [트랙 D] OBS 원격 조작 (obs-websocket 5, Electron 판 obs.js)
//! 요청마다 짧게 연결한다: Hello → Identify(비밀번호면 sha256 인증) → Identified → Request → RequestResponse.
//! 같은 PC·내부망이라 몇 ms 면 끝나고, 연결이 끊긴 상태를 따로 관리할 필요가 없다.
//!
//! 상태 캐시(`cached`): Electron 판은 상시 연결·이벤트로 `core 'obs'` 상태를 늘 갖고 있지만, Rust 판은
//! `start_poller` 가 설정 obs.enabled 일 때 10초마다 `status` 를 받아 캐시한다. 조작 화면 상태줄·OBS 창·
//! 순서 편집칸(장면 콤보)이 모두 이 캐시를 읽는다. 끊기면 경고 알림 1회, 다시 붙으면 초기화(Electron logger.warn 과 같음).

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::net::TcpStream;
use std::time::Duration;
use tungstenite::{Message, WebSocket};

/// base64(sha256(base64(sha256(password + salt)) + challenge))
pub fn auth_string(password: &str, salt: &str, challenge: &str) -> String {
    let secret = crate::dpapi::b64(&Sha256::digest(format!("{password}{salt}").as_bytes()));
    crate::dpapi::b64(&Sha256::digest(format!("{secret}{challenge}").as_bytes()))
}

#[derive(Clone, Debug, Default)]
pub struct Conf {
    pub host: String,
    pub port: u16,
    pub password: String,
}

/// 설정 obs {enabled, host, port, password, passwordEnc} → 연결 정보 (비밀번호는 DPAPI 로 풀기)
pub fn conf_from(settings: &Value) -> Conf {
    let o = &settings["obs"];
    let mut pw = o["password"].as_str().unwrap_or("").to_string();
    if let Some(e) = o["passwordEnc"].as_str().filter(|x| !x.is_empty()) {
        if let Some(p) = crate::dpapi::decrypt(e) {
            pw = p;
        }
    }
    Conf { host: o["host"].as_str().filter(|x| !x.is_empty()).unwrap_or("127.0.0.1").to_string(), port: o["port"].as_u64().unwrap_or(4455) as u16, password: pw }
}

/// 설정에서 OBS 원격 조작을 켰는지
pub fn enabled(settings: &Value) -> bool {
    settings["obs"]["enabled"].as_bool().unwrap_or(false)
}

struct Conn {
    ws: WebSocket<TcpStream>,
    seq: u64,
}

fn read_json(ws: &mut WebSocket<TcpStream>) -> Result<Value, String> {
    loop {
        match ws.read().map_err(|e| format!("OBS 연결이 끊겼습니다: {e}"))? {
            Message::Text(t) => return serde_json::from_str(t.as_str()).map_err(|e| e.to_string()),
            Message::Binary(b) => return serde_json::from_slice(&b).map_err(|e| e.to_string()),
            Message::Close(f) => return Err(crate::i18n::tr_args("OBS 가 연결을 닫았습니다{1}", &[&(f.map(|f| format!(" ({}: {})", u16::from(f.code), f.reason)).unwrap_or_default())])),
            _ => {}
        }
    }
}

impl Conn {
    fn open(c: &Conf) -> Result<Self, String> {
        let addr = format!("{}:{}", c.host, c.port);
        let sock = std::net::ToSocketAddrs::to_socket_addrs(&addr).map_err(|e| e.to_string())?.next().ok_or("OBS 주소를 찾지 못했습니다.")?;
        let tcp = TcpStream::connect_timeout(&sock, Duration::from_secs(3)).map_err(|_| format!("OBS({addr}) 에 연결하지 못했습니다. OBS 의 도구 → WebSocket 서버 설정에서 서버를 켰는지 확인하세요."))?;
        tcp.set_read_timeout(Some(Duration::from_secs(5))).ok();
        let (mut ws, _) = tungstenite::client::client_with_config(format!("ws://{addr}/"), tcp, None).map_err(|e| format!("OBS 웹소켓 연결 실패: {e}"))?;
        let hello = read_json(&mut ws)?;
        if hello["op"] != 0 {
            return Err(crate::i18n::tr("OBS 웹소켓 응답이 이상합니다 (obs-websocket 5 가 필요합니다).").into());
        }
        let mut d = json!({ "rpcVersion": 1, "eventSubscriptions": 0 });
        if let Some(a) = hello["d"]["authentication"].as_object() {
            if c.password.is_empty() {
                return Err(crate::i18n::tr("OBS 웹소켓 비밀번호를 넣으세요.").into());
            }
            d["authentication"] = json!(auth_string(&c.password, a.get("salt").and_then(|x| x.as_str()).unwrap_or(""), a.get("challenge").and_then(|x| x.as_str()).unwrap_or("")));
        }
        ws.send(Message::text(json!({ "op": 1, "d": d }).to_string())).map_err(|e| e.to_string())?;
        let r = read_json(&mut ws).map_err(|e| if e.contains("4009") { "OBS 비밀번호가 맞지 않습니다.".to_string() } else { e })?;
        if r["op"] != 2 {
            return Err(crate::i18n::tr("OBS 인증에 실패했습니다.").into());
        }
        Ok(Self { ws, seq: 0 })
    }

    fn request(&mut self, ty: &str, data: Value) -> Result<Value, String> {
        self.seq += 1;
        let id = format!("wp{}", self.seq);
        let mut d = json!({ "requestType": ty, "requestId": id });
        if !data.is_null() {
            d["requestData"] = data;
        }
        self.ws.send(Message::text(json!({ "op": 6, "d": d }).to_string())).map_err(|e| e.to_string())?;
        loop {
            let m = read_json(&mut self.ws)?;
            if m["op"] == 7 && m["d"]["requestId"] == id.as_str() {
                let st = &m["d"]["requestStatus"];
                if st["result"] != true {
                    return Err(format!("OBS: {}", st["comment"].as_str().unwrap_or("요청 실패")));
                }
                return Ok(m["d"]["responseData"].clone());
            }
        }
    }

    /// 방송(stream)·녹화(record)가 켜져 있는지 (GetStreamStatus / GetRecordStatus 의 outputActive)
    fn output_active(&mut self, what: &str) -> Result<bool, String> {
        let r = self.request(if what == "stream" { "GetStreamStatus" } else { "GetRecordStatus" }, Value::Null)?;
        Ok(r["outputActive"].as_bool().unwrap_or(false))
    }

    fn status(&mut self) -> Result<Value, String> {
        let sc = self.request("GetSceneList", Value::Null)?;
        let st = self.request("GetStreamStatus", Value::Null).unwrap_or(Value::Null);
        let rc = self.request("GetRecordStatus", Value::Null).unwrap_or(Value::Null);
        let mut scenes: Vec<String> = sc["scenes"].as_array().into_iter().flatten().filter_map(|s| s["sceneName"].as_str().map(String::from)).collect();
        scenes.reverse(); // OBS 는 아래부터 준다
        Ok(json!({ "connected": true, "scenes": scenes, "scene": sc["currentProgramSceneName"], "streaming": st["outputActive"].as_bool().unwrap_or(false), "streamTime": st["outputTimecode"].as_str().unwrap_or(""), "recording": rc["outputActive"].as_bool().unwrap_or(false) }))
    }
}

impl Drop for Conn {
    fn drop(&mut self) {
        let _ = self.ws.close(None);
    }
}

// ---------------- 상태 캐시 ----------------

fn empty_status() -> Value {
    json!({ "connected": false, "scenes": [], "scene": "", "streaming": false, "recording": false, "at": 0 })
}

/// 마지막으로 알아낸 OBS 상태 (Electron `ObsLink.st` 역할). `{connected, scenes, scene, streaming, recording, error?, at}`
static CACHE: Lazy<Mutex<Value>> = Lazy::new(|| Mutex::new(empty_status()));

/// 상태 캐시 읽기 (조작 화면·순서 편집칸용. 네트워크 안 함)
pub fn cached() -> Value {
    CACHE.lock().clone()
}

/// 상태 캐시에 넣기: Ok 면 그대로, Err 면 끊김 + 오류 글 (장면 목록은 마지막 것을 남긴다 — 편집칸 콤보가 비지 않게)
fn remember(r: &Result<Value, String>) {
    let mut c = CACHE.lock();
    match r {
        Ok(v) => {
            if c["connected"] != true {
                log::info!("OBS 연결됨");
            }
            let mut v = v.clone();
            v["at"] = json!(crate::util::now_ms());
            *c = v;
        }
        Err(e) => {
            // 같은 오류가 10초마다 되풀이되므로 바뀔 때만 로그에 (처음부터 못 붙은 경우는 알림이 없어 로그가 유일한 기록)
            if c["error"].as_str() != Some(e.as_str()) {
                log::warn!("OBS 연결 실패: {e}");
            }
            c["connected"] = json!(false);
            c["streaming"] = json!(false);
            c["recording"] = json!(false);
            c["error"] = json!(e);
            c["at"] = json!(crate::util::now_ms());
        }
    }
}

/// 설정에서 끄면 캐시를 비운다 (오류 글도 지움)
pub fn forget() {
    let mut v = empty_status();
    v["at"] = json!(crate::util::now_ms());
    *CACHE.lock() = v;
}

// ---------------- 조작 ----------------

/// 연결 확인 + 장면 목록 · 지금 장면 · 방송/녹화 상태 (결과는 캐시에도 남는다)
pub fn status(c: &Conf) -> Result<Value, String> {
    let r = Conn::open(c).and_then(|mut k| k.status());
    remember(&r);
    r
}

/// 동작: scene(scene) · stream(on) · record(on)
/// stream·record 는 Electron obs.js:158-166 과 같게: `on` 이 boolean 이 아니면 토글, 지금 상태(GetStreamStatus/GetRecordStatus)와
/// 같으면 요청을 보내지 않는다 (이미 방송 중인데 '시작' 이 와도 OBS 오류 없음). 돌려주는 값: `{ok, skipped?, streaming|recording?}`
pub fn action(c: &Conf, a: &Value) -> Result<Value, String> {
    let mut k = match Conn::open(c) {
        Ok(k) => k,
        Err(e) => {
            remember(&Err(e.clone()));
            return Err(e);
        }
    };
    let out = match a["action"].as_str().unwrap_or("") {
        "scene" => {
            let s = a["scene"].as_str().filter(|x| !x.is_empty()).ok_or("장면 이름이 없습니다.")?;
            k.request("SetCurrentProgramScene", json!({ "sceneName": s }))?;
            CACHE.lock()["scene"] = json!(s);
            json!({ "ok": true })
        }
        what @ ("stream" | "record") => {
            let cur = k.output_active(what)?;
            let want = a["on"].as_bool().unwrap_or(!cur);
            let key = if what == "stream" { "streaming" } else { "recording" };
            if want == cur {
                CACHE.lock()[key] = json!(cur);
                json!({ "ok": true, "skipped": true, key: cur })
            } else {
                k.request(&format!("{}{}", if want { "Start" } else { "Stop" }, if what == "stream" { "Stream" } else { "Record" }), Value::Null)?;
                CACHE.lock()[key] = json!(want);
                json!({ "ok": true, key: want })
            }
        }
        other => return Err(crate::i18n::tr_args("모르는 OBS 동작: {1}", &[&other])),
    };
    Ok(out)
}

// ---------------- 백그라운드 상태 폴링 ----------------

/// 상태 폴링 간격 (Electron 은 상시 연결 + 5초 폴링. Rust 판은 짧은 연결을 10초마다)
pub const POLL_SECS: u64 = 10;

/// 폴러의 '끊김 경고 1회' 기억
#[derive(Default, Debug, Clone)]
pub struct PollState {
    /// 지난 번에 연결돼 있었는지
    pub was_connected: bool,
    /// 끊김 경고를 이미 띄웠는지 (다시 붙으면 false)
    pub warned: bool,
}

/// 한 바퀴: `conf` 가 None(설정에서 끔)이면 캐시를 비우고, 아니면 상태를 받아 캐시한다.
/// 돌려주는 값: 사용자에게 보일 알림 (끊긴 순간 1회 warn · 다시 붙은 순간 1회 ok). 네트워크 중 코어 잠금은 쥐지 않는다.
pub fn poll_step(conf: Option<&Conf>, ps: &mut PollState) -> Option<(String, &'static str)> {
    let Some(c) = conf else {
        let k = cached();
        if ps.was_connected || ps.warned || k["connected"] == true || k.get("error").is_some() {
            forget();
        }
        *ps = PollState::default();
        return None;
    };
    match status(c) {
        Ok(_) => {
            let back = ps.warned;
            ps.was_connected = true;
            ps.warned = false;
            back.then(|| (crate::i18n::tr("OBS 에 다시 연결됐습니다."), "ok"))
        }
        Err(e) => {
            let was = ps.was_connected;
            ps.was_connected = false;
            if was && !ps.warned {
                ps.warned = true;
                Some((crate::i18n::tr_args("OBS 연결이 끊겼습니다: {1}", &[&e]), "warn"))
            } else {
                None
            }
        }
    }
}

static KICK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 다음 폴링을 바로 (설정을 바꿨거나 '연결 확인' 을 눌렀을 때)
pub fn kick() {
    KICK.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// 백그라운드 폴러 시작 (한 번만). 설정 obs.enabled 이면 `POLL_SECS` 마다 상태를 받아 캐시하고, 끊기면 경고 1회.
/// 설정은 코어를 아주 짧게 잠가 읽고, 네트워크는 잠금 없이 한다. `kick()` 으로 다음 바퀴를 바로 돌릴 수 있다.
pub fn start_poller(shared: crate::core::Shared) {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(move || {
        std::thread::Builder::new()
            .name("obs-poll".into())
            .spawn(move || {
                let mut ps = PollState::default();
                loop {
                    let conf = {
                        let c = shared.lock();
                        let s = c.settings.get();
                        enabled(s).then(|| conf_from(s))
                    };
                    let on = conf.is_some();
                    let note = poll_step(conf.as_ref(), &mut ps);
                    if on || note.is_some() {
                        let mut c = shared.lock();
                        if let Some((m, k)) = note {
                            c.notify(m, k);
                        }
                        c.emit(crate::core::Ev::Ui);
                    }
                    // 1초씩 자다가 kick 이 오면 바로 다음 바퀴
                    for _ in 0..POLL_SECS {
                        std::thread::sleep(Duration::from_secs(1));
                        if KICK.swap(false, std::sync::atomic::Ordering::Relaxed) {
                            break;
                        }
                    }
                }
            })
            .ok();
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::Arc;

    /// 가짜 OBS: 비밀번호 'pw' 로 인증, 장면 2개, 요청 기록. 방송(처음 켜짐)·녹화(처음 꺼짐) 상태를 Start/Stop 에 따라 바꾼다.
    fn fake_obs() -> (u16, Arc<Mutex<Vec<String>>>, Arc<Mutex<(bool, bool)>>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let log = Arc::new(Mutex::new(vec![]));
        let outs = Arc::new(Mutex::new((true, false)));
        let (lg, os) = (log.clone(), outs.clone());
        std::thread::spawn(move || {
            for s in l.incoming().flatten() {
                let (lg, os) = (lg.clone(), os.clone());
                std::thread::spawn(move || {
                    let mut ws = tungstenite::accept(s).unwrap();
                    ws.send(Message::text(json!({ "op": 0, "d": { "rpcVersion": 1, "authentication": { "salt": "SALT", "challenge": "CH" } } }).to_string())).unwrap();
                    let id = read_json(&mut ws).unwrap();
                    if id["d"]["authentication"] != auth_string("pw", "SALT", "CH").as_str() {
                        let _ = ws.close(Some(tungstenite::protocol::CloseFrame { code: 4009.into(), reason: "auth".into() }));
                        let _ = ws.flush();
                        return;
                    }
                    ws.send(Message::text(json!({ "op": 2, "d": { "negotiatedRpcVersion": 1 } }).to_string())).unwrap();
                    while let Ok(m) = read_json(&mut ws) {
                        let ty = m["d"]["requestType"].as_str().unwrap_or("").to_string();
                        lg.lock().push(format!("{ty} {}", m["d"]["requestData"]));
                        let mut o = os.lock();
                        let data = match ty.as_str() {
                            "GetSceneList" => json!({ "currentProgramSceneName": "설교", "scenes": [{ "sceneName": "설교" }, { "sceneName": "찬양" }] }),
                            "GetStreamStatus" => json!({ "outputActive": o.0, "outputTimecode": "00:01:02.000" }),
                            "GetRecordStatus" => json!({ "outputActive": o.1 }),
                            "StartStream" => {
                                o.0 = true;
                                json!({})
                            }
                            "StopStream" => {
                                o.0 = false;
                                json!({})
                            }
                            "StartRecord" => {
                                o.1 = true;
                                json!({})
                            }
                            "StopRecord" => {
                                o.1 = false;
                                json!({})
                            }
                            _ => json!({}),
                        };
                        drop(o);
                        let ok = ty != "Bad";
                        ws.send(Message::text(json!({ "op": 7, "d": { "requestType": ty, "requestId": m["d"]["requestId"], "requestStatus": { "result": ok, "code": 100, "comment": "x" }, "responseData": data } }).to_string())).unwrap();
                    }
                });
            }
        });
        (port, log, outs)
    }

    fn sent(log: &Arc<Mutex<Vec<String>>>, prefix: &str) -> usize {
        log.lock().iter().filter(|x| x.starts_with(prefix)).count()
    }

    /// 아무도 안 듣는 포트 (끊김 흉내)
    fn dead_conf() -> Conf {
        let dead = TcpListener::bind("127.0.0.1:0").unwrap();
        let dport = dead.local_addr().unwrap().port();
        drop(dead);
        Conf { host: "127.0.0.1".into(), port: dport, password: "pw".into() }
    }

    /// 캐시는 프로세스 전체 하나라 테스트끼리 섞이지 않게 차례로 돈다
    static SERIAL: Mutex<()> = Mutex::new(());

    #[test]
    fn auth_known() {
        // obs-websocket 문서의 계산 방식 (값은 고정)
        assert_eq!(auth_string("pw", "SALT", "CH").len(), 44);
        assert_ne!(auth_string("pw", "SALT", "CH"), auth_string("pw2", "SALT", "CH"));
    }

    #[test]
    fn fake_server() {
        let _g = SERIAL.lock();
        let (port, log, _) = fake_obs();
        let c = Conf { host: "127.0.0.1".into(), port, password: "pw".into() };
        let s = status(&c).unwrap();
        assert_eq!(s["scenes"], json!(["찬양", "설교"]));
        assert_eq!(s["scene"], "설교");
        assert_eq!(s["streaming"], true);
        action(&c, &json!({ "action": "scene", "scene": "찬양" })).unwrap();
        action(&c, &json!({ "action": "record", "on": true })).unwrap();
        let l = log.lock().clone();
        assert!(l.iter().any(|x| x.starts_with("SetCurrentProgramScene") && x.contains("찬양")));
        assert!(l.iter().any(|x| x.starts_with("StartRecord")));
        assert_eq!(cached()["scene"], "찬양", "장면 바꾸면 캐시도 따라간다");
        let bad = Conf { password: "nope".into(), ..c.clone() };
        assert!(status(&bad).unwrap_err().contains("비밀번호"));
        let none = Conf { password: String::new(), ..c };
        assert!(status(&none).unwrap_err().contains("비밀번호"));
    }

    /// Electron obs.js:158-166 — 같은 상태면 요청 생략, on 이 boolean 이 아니면 토글
    #[test]
    fn stream_record_skip_and_toggle() {
        let _g = SERIAL.lock();
        let (port, log, outs) = fake_obs();
        let c = Conf { host: "127.0.0.1".into(), port, password: "pw".into() };
        // 방송 켜진 상태에서 '시작' → 생략 (StartStream 안 보냄)
        let r = action(&c, &json!({ "action": "stream", "on": true })).unwrap();
        assert_eq!(r["skipped"], true);
        assert_eq!(r["streaming"], true);
        assert_eq!(sent(&log, "StartStream"), 0);
        assert_eq!(sent(&log, "GetStreamStatus"), 1);
        // on 없음 → 토글: 켜져 있으니 StopStream
        let r = action(&c, &json!({ "action": "stream" })).unwrap();
        assert_eq!(r["streaming"], false);
        assert_eq!(sent(&log, "StopStream"), 1);
        assert!(!outs.lock().0);
        // 꺼진 상태에서 '끝' → 생략
        let r = action(&c, &json!({ "action": "stream", "on": false })).unwrap();
        assert_eq!(r["skipped"], true);
        assert_eq!(sent(&log, "StopStream"), 1);
        // 녹화: 꺼져 있음 → on 없음이면 StartRecord, 다시 'start' 면 생략, boolean 아니면 토글 → StopRecord
        let r = action(&c, &json!({ "action": "record" })).unwrap();
        assert_eq!(r["recording"], true);
        assert_eq!(sent(&log, "StartRecord"), 1);
        assert_eq!(action(&c, &json!({ "action": "record", "on": true })).unwrap()["skipped"], true);
        assert_eq!(sent(&log, "StartRecord"), 1);
        action(&c, &json!({ "action": "record", "on": "뭔가" })).unwrap();
        assert_eq!(sent(&log, "StopRecord"), 1);
        assert!(!outs.lock().1);
        // 캐시도 따라간다
        let k = cached();
        assert_eq!(k["streaming"], false);
        assert_eq!(k["recording"], false);
    }

    /// 상태 캐시: status 성공 → connected·장면, 실패 → connected false + error (장면 목록은 남김), 끔 → 비움.
    /// 끊김 경고는 연결돼 있다가 끊긴 순간 1회, 다시 붙으면 초기화.
    #[test]
    fn status_cache_and_poll_once_warning() {
        let _g = SERIAL.lock();
        let (port, _, _) = fake_obs();
        let c = Conf { host: "127.0.0.1".into(), port, password: "pw".into() };
        let d = dead_conf();
        let mut ps = PollState::default();
        // 처음 붙음: 알림 없음
        assert!(poll_step(Some(&c), &mut ps).is_none());
        let k = cached();
        assert_eq!(k["connected"], true);
        assert_eq!(k["scenes"], json!(["찬양", "설교"]));
        assert_eq!(k["streamTime"], "00:01:02.000");
        assert!(k["at"].as_i64().unwrap() > 0);
        assert!(ps.was_connected && !ps.warned);
        // 끊김: 첫 번째만 warn, 두 번째는 조용
        let n = poll_step(Some(&d), &mut ps).expect("끊긴 순간 경고 1회");
        assert_eq!(n.1, "warn");
        assert!(n.0.contains("끊겼습니다"));
        assert!(poll_step(Some(&d), &mut ps).is_none());
        let k = cached();
        assert_eq!(k["connected"], false);
        assert!(k["error"].as_str().is_some());
        assert_eq!(k["scenes"], json!(["찬양", "설교"]), "장면 목록은 편집칸용으로 남긴다");
        // 다시 붙음: ok 알림 1회, 경고 기억 초기화 → 또 끊기면 다시 warn
        let n = poll_step(Some(&c), &mut ps).expect("다시 연결 알림");
        assert_eq!(n.1, "ok");
        assert!(cached().get("error").is_none());
        assert!(poll_step(Some(&d), &mut ps).is_some());
        // 설정에서 끔: 캐시 비움, 알림 없음, 기억 초기화
        assert!(poll_step(None, &mut ps).is_none());
        assert_eq!(cached()["connected"], false);
        assert_eq!(cached()["scenes"], json!([]));
        assert!(cached().get("error").is_none());
        assert!(!ps.was_connected && !ps.warned);
        // 연결된 적이 없이 처음부터 안 붙으면 경고하지 않는다 — Electron 도 was 일 때만 logger.warn
        assert!(poll_step(Some(&d), &mut ps).is_none());
        assert!(cached()["error"].as_str().is_some(), "오류 글은 OBS 창에 보이도록 남긴다");
    }
}
