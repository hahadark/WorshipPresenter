//! [트랙 D] OBS 원격 조작 (obs-websocket 5, Electron 판 obs.js)
//! 요청마다 짧게 연결한다: Hello → Identify(비밀번호면 sha256 인증) → Identified → Request → RequestResponse.
//! 같은 PC·내부망이라 몇 ms 면 끝나고, 연결이 끊긴 상태를 따로 관리할 필요가 없다.

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

struct Conn {
    ws: WebSocket<TcpStream>,
    seq: u64,
}

fn read_json(ws: &mut WebSocket<TcpStream>) -> Result<Value, String> {
    loop {
        match ws.read().map_err(|e| format!("OBS 연결이 끊겼습니다: {e}"))? {
            Message::Text(t) => return serde_json::from_str(t.as_str()).map_err(|e| e.to_string()),
            Message::Binary(b) => return serde_json::from_slice(&b).map_err(|e| e.to_string()),
            Message::Close(f) => return Err(format!("OBS 가 연결을 닫았습니다{}", f.map(|f| format!(" ({}: {})", u16::from(f.code), f.reason)).unwrap_or_default())),
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
            return Err("OBS 웹소켓 응답이 이상합니다 (obs-websocket 5 가 필요합니다).".into());
        }
        let mut d = json!({ "rpcVersion": 1, "eventSubscriptions": 0 });
        if let Some(a) = hello["d"]["authentication"].as_object() {
            if c.password.is_empty() {
                return Err("OBS 웹소켓 비밀번호를 넣으세요.".into());
            }
            d["authentication"] = json!(auth_string(&c.password, a.get("salt").and_then(|x| x.as_str()).unwrap_or(""), a.get("challenge").and_then(|x| x.as_str()).unwrap_or("")));
        }
        ws.send(Message::text(json!({ "op": 1, "d": d }).to_string())).map_err(|e| e.to_string())?;
        let r = read_json(&mut ws).map_err(|e| if e.contains("4009") { "OBS 비밀번호가 맞지 않습니다.".to_string() } else { e })?;
        if r["op"] != 2 {
            return Err("OBS 인증에 실패했습니다.".into());
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
}

impl Drop for Conn {
    fn drop(&mut self) {
        let _ = self.ws.close(None);
    }
}

/// 연결 확인 + 장면 목록 · 지금 장면 · 방송/녹화 상태
pub fn status(c: &Conf) -> Result<Value, String> {
    let mut k = Conn::open(c)?;
    let sc = k.request("GetSceneList", Value::Null)?;
    let st = k.request("GetStreamStatus", Value::Null).unwrap_or(Value::Null);
    let rc = k.request("GetRecordStatus", Value::Null).unwrap_or(Value::Null);
    let mut scenes: Vec<String> = sc["scenes"].as_array().into_iter().flatten().filter_map(|s| s["sceneName"].as_str().map(String::from)).collect();
    scenes.reverse(); // OBS 는 아래부터 준다
    Ok(json!({ "connected": true, "scenes": scenes, "scene": sc["currentProgramSceneName"], "streaming": st["outputActive"].as_bool().unwrap_or(false), "recording": rc["outputActive"].as_bool().unwrap_or(false) }))
}

/// 동작: scene(scene) · stream(on) · record(on)
pub fn action(c: &Conf, a: &Value) -> Result<Value, String> {
    let mut k = Conn::open(c)?;
    match a["action"].as_str().unwrap_or("") {
        "scene" => {
            let s = a["scene"].as_str().filter(|x| !x.is_empty()).ok_or("장면 이름이 없습니다.")?;
            k.request("SetCurrentProgramScene", json!({ "sceneName": s }))?;
        }
        "stream" => {
            k.request(if a["on"].as_bool().unwrap_or(true) { "StartStream" } else { "StopStream" }, Value::Null)?;
        }
        "record" => {
            k.request(if a["on"].as_bool().unwrap_or(true) { "StartRecord" } else { "StopRecord" }, Value::Null)?;
        }
        other => return Err(format!("모르는 OBS 동작: {other}")),
    }
    Ok(json!({ "ok": true }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    /// 가짜 OBS: 비밀번호 'pw' 로 인증, 장면 2개, 장면 바꾸기 기록
    fn fake_obs() -> (u16, std::sync::Arc<parking_lot::Mutex<Vec<String>>>) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let log = std::sync::Arc::new(parking_lot::Mutex::new(vec![]));
        let lg = log.clone();
        std::thread::spawn(move || {
            for s in l.incoming().flatten() {
                let lg = lg.clone();
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
                        let data = match ty.as_str() {
                            "GetSceneList" => json!({ "currentProgramSceneName": "설교", "scenes": [{ "sceneName": "설교" }, { "sceneName": "찬양" }] }),
                            "GetStreamStatus" => json!({ "outputActive": true }),
                            "GetRecordStatus" => json!({ "outputActive": false }),
                            _ => json!({}),
                        };
                        let ok = ty != "Bad";
                        ws.send(Message::text(json!({ "op": 7, "d": { "requestType": ty, "requestId": m["d"]["requestId"], "requestStatus": { "result": ok, "code": 100, "comment": "x" }, "responseData": data } }).to_string())).unwrap();
                    }
                });
            }
        });
        (port, log)
    }

    #[test]
    fn auth_known() {
        // obs-websocket 문서의 계산 방식 (값은 고정)
        assert_eq!(auth_string("pw", "SALT", "CH").len(), 44);
        assert_ne!(auth_string("pw", "SALT", "CH"), auth_string("pw2", "SALT", "CH"));
    }

    #[test]
    fn fake_server() {
        let (port, log) = fake_obs();
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
        let bad = Conf { password: "nope".into(), ..c.clone() };
        assert!(status(&bad).unwrap_err().contains("비밀번호"));
        let none = Conf { password: String::new(), ..c };
        assert!(status(&none).unwrap_err().contains("비밀번호"));
    }
}
