//! Companion 단추 누르기 (Bitfocus Companion 3 이상 HTTP API, Electron 판 companion.js)
//!   POST http://<주소>:<포트>/api/location/<페이지>/<줄>/<칸>/press
//! 단추 위치는 '페이지/줄/칸' (예: 1/0/2). 여러 개는 쉼표로.

use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Loc {
    pub page: u32,
    pub row: u32,
    pub col: u32,
}

static LOC: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(\d{1,3})/(\d{1,2})/(\d{1,2})$").unwrap());

/// "1/0/2, 2/1/0" → 위치들 (잘못된 것은 뺀다, 최대 10개)
pub fn parse_locations(s: &str) -> Vec<Loc> {
    s.split(|c: char| c == ',' || c.is_whitespace())
        .filter_map(|x| LOC.captures(x.trim()).and_then(|m| Some(Loc { page: m[1].parse().ok()?, row: m[2].parse().ok()?, col: m[3].parse().ok()? }))) // 너무 큰 숫자는 뺀다 (panic 아님)
        .take(10)
        .collect()
}

/// 아주 작은 HTTP/1.1 요청 (본문 없음 또는 JSON). 상태 번호를 돌려준다
pub fn http_request(method: &str, host: &str, port: u16, path: &str, body: Option<&str>, timeout: Duration) -> std::io::Result<(u16, String)> {
    let addr = (host, port).to_socket_addrs()?.next().ok_or_else(|| std::io::Error::other("주소 없음"))?;
    let mut s = TcpStream::connect_timeout(&addr, timeout)?;
    s.set_read_timeout(Some(timeout))?;
    s.set_write_timeout(Some(timeout))?;
    let b = body.unwrap_or("");
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{b}",
        b.len()
    );
    s.write_all(req.as_bytes())?;
    let mut buf = Vec::new();
    let _ = s.read_to_end(&mut buf);
    let txt = String::from_utf8_lossy(&buf).to_string();
    let code = txt.split_whitespace().nth(1).and_then(|c| c.parse().ok()).ok_or_else(|| std::io::Error::other("응답 없음"))?;
    let body = txt.split_once("\r\n\r\n").map(|x| x.1.to_string()).unwrap_or_default();
    Ok((code, body))
}

/// 단추 누르기: 하나라도 실패하면 이유를 모아 알린다. companion = settings.companion {host, port}
pub fn press(companion: &Value, locs: &str) -> Result<Value, String> {
    let list = parse_locations(locs);
    if list.is_empty() {
        return Err(crate::i18n::tr("Companion 단추 위치를 '페이지/줄/칸' 으로 쓰세요 (예: 1/0/2).").into());
    }
    let host = companion["host"].as_str().filter(|x| !x.is_empty()).unwrap_or("127.0.0.1").to_string();
    let port = companion["port"].as_u64().filter(|p| *p > 0).unwrap_or(8000) as u16;
    let mut errs = vec![];
    for l in &list {
        let path = format!("/api/location/{}/{}/{}/press", l.page, l.row, l.col);
        match http_request("POST", &host, port, &path, None, Duration::from_secs(4)) {
            Ok((c, _)) if (200..300).contains(&c) => {}
            Ok((404, _)) => errs.push(format!("{}/{}/{}: 그 위치에 단추가 없거나 Companion 이 3 보다 옛 버전입니다", l.page, l.row, l.col)),
            Ok((c, _)) => errs.push(format!("{}/{}/{}: HTTP {c}", l.page, l.row, l.col)),
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut || e.kind() == std::io::ErrorKind::WouldBlock => errs.push(format!("{}/{}/{}: 응답이 없습니다", l.page, l.row, l.col)),
            Err(_) => errs.push(format!("{}/{}/{}: Companion 에 연결하지 못했습니다 ({host}:{port})", l.page, l.row, l.col)),
        }
    }
    if !errs.is_empty() {
        return Err(errs.join(" · "));
    }
    Ok(json!({ "pressed": list.len() }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    #[test]
    fn locations() {
        assert_eq!(parse_locations("1/0/2, 2/1/0 x 9/9"), vec![Loc { page: 1, row: 0, col: 2 }, Loc { page: 2, row: 1, col: 0 }]);
    }
    #[test]
    fn press_fake_companion() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let h = std::thread::spawn(move || {
            let mut got = vec![];
            for _ in 0..2 {
                let (mut s, _) = l.accept().unwrap();
                let mut b = [0u8; 1024];
                let n = s.read(&mut b).unwrap();
                let req = String::from_utf8_lossy(&b[..n]).to_string();
                let line = req.lines().next().unwrap().to_string();
                let code = if line.contains("/9/0/0/") { "404 Not Found" } else { "200 OK" };
                s.write_all(format!("HTTP/1.1 {code}\r\nContent-Length: 0\r\n\r\n").as_bytes()).unwrap();
                got.push(line);
            }
            got
        });
        let c = json!({ "host": "127.0.0.1", "port": port });
        let r = press(&c, "1/0/2, 9/0/0");
        let got = h.join().unwrap();
        assert_eq!(got[0], "POST /api/location/1/0/2/press HTTP/1.1");
        assert!(r.unwrap_err().contains("9/0/0"));
    }
}
