//! 장비 직접 제어 (Electron 판 devices.js): Behringer X32·M32 (OSC, UDP 10023) · PTZ 카메라 (VISCA over IP)
//! X32 동작: ch1 on · ch1 off · ch5 fader 0.75 · bus2 off · main off · main fader 0.8 · scene 3 · dca1 off
//! PTZ 동작: 프리셋 번호 (0~254) 또는 home
//! 설정: settings.devices = [{ id, name, kind: 'x32'|'visca', host, port, proto: 'sony'|'raw' }]

use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::{json, Value};
use std::net::UdpSocket;
use std::sync::atomic::{AtomicU32, Ordering};

#[derive(Debug, Clone, PartialEq)]
pub enum OscArg {
    I(i32),
    F(f32),
}

#[derive(Debug, Clone, PartialEq)]
pub struct OscCmd {
    pub address: String,
    pub args: Vec<OscArg>,
    pub text: String,
}

fn pad4(mut b: Vec<u8>) -> Vec<u8> {
    let n = 4 - (b.len() % 4); // 늘 1~4 바이트 0 (OSC 문자열 끝 표시)
    b.extend(std::iter::repeat(0u8).take(n));
    b
}

pub fn osc_message(address: &str, args: &[OscArg]) -> Vec<u8> {
    let tags: String = std::iter::once(',').chain(args.iter().map(|a| match a { OscArg::I(_) => 'i', OscArg::F(_) => 'f' })).collect();
    let mut out = pad4(address.as_bytes().to_vec());
    out.extend(pad4(tags.into_bytes()));
    for a in args {
        match a {
            OscArg::I(v) => out.extend(v.to_be_bytes()),
            OscArg::F(v) => out.extend(v.to_be_bytes()),
        }
    }
    out
}

static SCENE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^scene (\d{1,2})$").unwrap());
static CH: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(ch|bus|dca|mtx|auxin|fxrtn|main)\s?(\d{0,2}) (on|off|mute|unmute|fader (\d(?:\.\d+)?))$").unwrap());
static WS: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s+").unwrap());

/// "ch1 on, ch5 fader 0.75, scene 3" → OSC 명령들 (모르는 말이면 오류)
pub fn parse_x32(text: &str) -> Result<Vec<OscCmd>, String> {
    let mut out = vec![];
    for raw in text.split([',', '\n']).map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let s = WS.replace_all(&raw.to_lowercase(), " ").to_string();
        if let Some(m) = SCENE.captures(&s) {
            out.push(OscCmd { address: "/-action/goscene".into(), args: vec![OscArg::I(m[1].parse().unwrap())], text: raw.into() });
            continue;
        }
        if let Some(m) = CH.captures(&s) {
            let kind = &m[1];
            let num: u32 = m[2].parse().unwrap_or(0);
            let lim = match kind { "ch" => 32, "bus" => 16, "dca" => 8, "mtx" => 6, _ => 8 };
            if kind != "main" && (num == 0 || num > lim) {
                return Err(format!("X32: '{raw}' — {kind} 번호는 1~{lim}"));
            }
            let base = if kind == "main" { "/main/st".to_string() } else if kind == "dca" { format!("/dca/{num}") } else { format!("/{kind}/{num:02}") };
            let mix_on = if kind == "dca" { format!("{base}/on") } else { format!("{base}/mix/on") };
            let fader = if kind == "dca" { format!("{base}/fader") } else { format!("{base}/mix/fader") };
            match &m[3] {
                "on" | "unmute" => out.push(OscCmd { address: mix_on, args: vec![OscArg::I(1)], text: raw.into() }),
                "off" | "mute" => out.push(OscCmd { address: mix_on, args: vec![OscArg::I(0)], text: raw.into() }),
                _ => {
                    let v: f32 = m.get(4).and_then(|x| x.as_str().parse().ok()).unwrap_or(-1.0);
                    if !(0.0..=1.0).contains(&v) {
                        return Err(format!("X32: '{raw}' — fader 는 0~1"));
                    }
                    out.push(OscCmd { address: fader, args: vec![OscArg::F(v)], text: raw.into() });
                }
            }
            continue;
        }
        return Err(format!("X32: '{raw}' 를 알아듣지 못했습니다 (예: ch1 on · ch5 fader 0.75 · main off · scene 3)"));
    }
    Ok(out)
}

/// 프리셋 불러오기 / 처음 자리 (VISCA 명령 바이트)
pub fn visca_command(action: &str) -> Result<Vec<u8>, String> {
    let s = action.trim().to_lowercase();
    if s == "home" {
        return Ok(vec![0x81, 0x01, 0x06, 0x04, 0xff]);
    }
    match s.parse::<u32>() {
        Ok(n) if n <= 254 => Ok(vec![0x81, 0x01, 0x04, 0x3f, 0x02, n as u8, 0xff]),
        _ => Err("PTZ: 프리셋 번호는 0~254 (또는 home)".into()),
    }
}

/// Sony VISCA over IP 머리글 (명령 0x0100 · 길이 · 순번)
pub fn visca_packet(cmd: &[u8], seq: u32, proto: &str) -> Vec<u8> {
    if proto == "raw" {
        return cmd.to_vec();
    }
    let mut h = vec![0x01, 0x00];
    h.extend((cmd.len() as u16).to_be_bytes());
    h.extend(seq.to_be_bytes());
    h.extend(cmd);
    h
}

pub fn default_port(kind: &str) -> u16 {
    if kind == "x32" { 10023 } else { 52381 }
}

static SEQ: AtomicU32 = AtomicU32::new(1);

fn send_udp(host: &str, port: u16, packets: &[Vec<u8>]) -> std::io::Result<usize> {
    let sock = UdpSocket::bind("0.0.0.0:0")?;
    for (i, p) in packets.iter().enumerate() {
        if i > 0 {
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
        sock.send_to(p, (host, port))?;
    }
    Ok(packets.len())
}

/// 한 장비에 동작 보내기 (devices 목록에서 id 로 찾음)
pub fn run(devices: &Value, device: &str, action: &str) -> Result<Value, String> {
    let d = devices
        .as_array()
        .and_then(|a| a.iter().find(|x| x["id"] == device))
        .ok_or("장비를 찾을 수 없습니다. 도구 → 장비 에서 다시 고르세요.")?;
    let kind = d["kind"].as_str().unwrap_or("");
    let host = d["host"].as_str().unwrap_or("");
    let name = d["name"].as_str().unwrap_or("");
    let port = d["port"].as_u64().map(|p| p as u16).filter(|p| *p > 0).unwrap_or_else(|| default_port(kind));
    let packets: Vec<Vec<u8>> = match kind {
        "x32" => parse_x32(action)?.iter().map(|c| osc_message(&c.address, &c.args)).collect(),
        "visca" => vec![visca_packet(&visca_command(action)?, SEQ.fetch_add(1, Ordering::Relaxed), d["proto"].as_str().unwrap_or("sony"))],
        _ => return Err("알 수 없는 장비 종류입니다.".into()),
    };
    if packets.is_empty() {
        return Err("보낼 동작이 없습니다.".into());
    }
    send_udp(host, port, &packets).map_err(|e| format!("{name}: 보내지 못했습니다 ({host}:{port}) — {e}"))?;
    log::info!("장비 '{name}': {}", action.chars().take(120).collect::<String>());
    Ok(json!({ "sent": packets.len() }))
}

static HOST_OK: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[\w.-]{1,200}$").unwrap());
static ID_OK: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)^[a-z0-9]{3,30}$").unwrap());

/// 설정 정리 (normalizeDevice)
pub fn normalize_device(x: &Value) -> Result<Value, String> {
    let kind = match x["kind"].as_str() { Some(k @ ("x32" | "visca")) => k, _ => return Err("장비 종류를 고르세요 (X32 · PTZ 카메라).".into()) };
    let host = x["host"].as_str().unwrap_or("").trim().to_string();
    if !HOST_OK.is_match(&host) {
        return Err("장비 IP 주소를 입력하세요 (예: 192.168.1.50).".into());
    }
    let name: String = x["name"].as_str().unwrap_or("").trim().chars().take(40).collect();
    let name = if name.is_empty() { if kind == "x32" { "X32".into() } else { "PTZ 카메라".to_string() } } else { name };
    let port = x["port"].as_f64().or_else(|| x["port"].as_str().and_then(|s| s.parse().ok())).map(|p| p.round().clamp(1.0, 65535.0) as u16).filter(|p| *p > 0).unwrap_or(default_port(kind));
    let id = x["id"].as_str().filter(|i| ID_OK.is_match(i)).map(String::from).unwrap_or_else(|| crate::util::short_id("dv"));
    let mut d = json!({ "id": id, "name": name, "kind": kind, "host": host, "port": port });
    if kind == "visca" {
        d["proto"] = json!(if x["proto"] == "raw" { "raw" } else { "sony" });
    }
    Ok(d)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn osc_bytes() {
        // "/ch/01/mix/on" (13) → 16 바이트, ",i" → 4, 값 4
        let m = osc_message("/ch/01/mix/on", &[OscArg::I(1)]);
        assert_eq!(m.len(), 24);
        assert_eq!(&m[..13], b"/ch/01/mix/on");
        assert_eq!(&m[16..18], b",i");
        assert_eq!(&m[20..], &[0, 0, 0, 1]);
        let c = parse_x32("ch1 on, ch5 fader 0.75, main off, scene 3, dca2 off").unwrap();
        assert_eq!(c[0].address, "/ch/01/mix/on");
        assert_eq!(c[1].address, "/ch/05/mix/fader");
        assert_eq!(c[1].args, vec![OscArg::F(0.75)]);
        assert_eq!(c[2].address, "/main/st/mix/on");
        assert_eq!(c[3].address, "/-action/goscene");
        assert_eq!(c[4].address, "/dca/2/on");
        assert!(parse_x32("ch40 on").is_err());
        assert!(parse_x32("hello").is_err());
    }
    #[test]
    fn visca_bytes() {
        assert_eq!(visca_command("home").unwrap(), vec![0x81, 0x01, 0x06, 0x04, 0xff]);
        let p = visca_packet(&visca_command("3").unwrap(), 7, "sony");
        assert_eq!(p, vec![0x01, 0x00, 0x00, 0x07, 0, 0, 0, 7, 0x81, 0x01, 0x04, 0x3f, 0x02, 3, 0xff]);
        assert!(visca_command("300").is_err());
    }
    #[test]
    fn udp_send_to_fake_device() {
        let srv = UdpSocket::bind("127.0.0.1:0").unwrap();
        srv.set_read_timeout(Some(std::time::Duration::from_secs(2))).unwrap();
        let port = srv.local_addr().unwrap().port();
        let devs = json!([{ "id": "dvx", "name": "X32", "kind": "x32", "host": "127.0.0.1", "port": port }, { "id": "dvp", "name": "캠", "kind": "visca", "host": "127.0.0.1", "port": port, "proto": "raw" }]);
        assert_eq!(run(&devs, "dvx", "ch1 off, scene 2").unwrap()["sent"], 2);
        let mut buf = [0u8; 256];
        let n = srv.recv(&mut buf).unwrap();
        assert_eq!(&buf[..n], osc_message("/ch/01/mix/on", &[OscArg::I(0)]).as_slice());
        let n = srv.recv(&mut buf).unwrap();
        assert_eq!(&buf[..n], osc_message("/-action/goscene", &[OscArg::I(2)]).as_slice());
        run(&devs, "dvp", "5").unwrap();
        let n = srv.recv(&mut buf).unwrap();
        assert_eq!(&buf[..n], &[0x81, 0x01, 0x04, 0x3f, 0x02, 5, 0xff]);
        assert!(normalize_device(&json!({ "kind": "x32", "host": "192.168.1.5" })).unwrap()["port"] == 10023);
    }
}
