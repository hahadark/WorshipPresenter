//! 클라우드 자료 길 (docs/web/data-api.md 3절 · 그림·영상 올리기 계약 — docs/web/sync.md 4절).
//! 동기화 엔진은 이 꼴(`Api`)만 본다 → 단위 시험은 메모리 안의 가짜 서버로, 실제는 `Http`(ureq)로.

use serde_json::{json, Value};
use std::io::Read;
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub enum ApiErr {
    /// 인터넷 · 클라우드가 잠시 안 됨 (다시 해 볼 만함)
    Net(String),
    /// 열쇠가 틀림 · 지워짐 · 권한 없음 — 설정을 고칠 때까지 멈춤
    Auth(String),
    /// 그 길이 없음 (그림·영상 길이 아직 없는 클라우드 · 자료 길이 없는 옛 클라우드)
    Missing,
    /// 무료 범위 안전장치 (507 quota) — 올리기를 멈춘다
    Quota,
    /// 그 밖의 답
    Http(u16, String),
}

impl std::fmt::Display for ApiErr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApiErr::Net(e) => write!(f, "{e}"),
            ApiErr::Auth(e) => write!(f, "{e}"),
            ApiErr::Missing => write!(f, "없는 길 (404)"),
            ApiErr::Quota => write!(f, "클라우드 저장 한도 (507)"),
            ApiErr::Http(c, e) => write!(f, "HTTP {c} {e}"),
        }
    }
}

/// 자료 길. 모두 교회 하나(열쇠의 교회) 안에서만
pub trait Api {
    /// `GET /api/v1/sync?after=` → `{changes, next, more, head, reset}`
    fn pull(&self, after: i64) -> Result<Value, ApiErr>;
    /// `POST /api/v1/sync {mutations}` → `{results, head}` (20개까지)
    fn push(&self, mutations: &[Value]) -> Result<Value, ApiErr>;
    /// `GET /api/v1/documents?include=payload&deleted=1&cursor=` → `{documents, next}` (reset 뒤 전체 다시 맞추기)
    fn list(&self, cursor: &str) -> Result<Value, ApiErr>;
    /// `GET /api/v1/assets?sha256=` → 있으면 id
    fn asset_find(&self, sha: &str) -> Result<Option<String>, ApiErr>;
    /// 파일 올리기 (90 MB 넘으면 나눠서) → id. 같은 sha256 이 있으면 서버가 본문을 버리고 그 id
    fn asset_put(&self, file: &Path, sha: &str, name: &str, ctype: &str, size: u64) -> Result<String, ApiErr>;
    /// `GET /api/v1/assets/{id}` → (바이트, content-type)
    fn asset_get(&self, id: &str) -> Result<(Vec<u8>, String), ApiErr>;
}

/// 한 번에 올리는 크기 한도 (계약: 90 MB 이하는 한 번에, 넘으면 나눠 올리기)
pub const ONE_SHOT: u64 = 90 * 1024 * 1024;

pub struct Http {
    base: String,
    auth: String,
    agent: ureq::Agent,
}

impl Http {
    /// base = 클라우드 주소 (https://… 끝 / 없이), key = 설치판 연결 열쇠 '교회.열쇠'
    pub fn new(base: &str, key: &str) -> Self {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(10)))
            .timeout_global(Some(Duration::from_secs(120)))
            .user_agent(format!("WorshipPresenter/{} (cloud sync)", env!("CARGO_PKG_VERSION")))
            .build()
            .into();
        Http { base: base.trim_end_matches('/').to_string(), auth: format!("Bearer {key}"), agent }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    fn json_of(resp: ureq::http::Response<ureq::Body>) -> Result<Value, ApiErr> {
        let code = resp.status().as_u16();
        let mut resp = resp;
        let v: Value = resp.body_mut().with_config().limit(64 << 20).read_json().unwrap_or(Value::Null);
        match code {
            200..=299 => Ok(v),
            _ => Err(status_err(code, &v)),
        }
    }
}

fn status_err(code: u16, v: &Value) -> ApiErr {
    let msg = v["error"].as_str().unwrap_or("").to_string();
    match code {
        401 | 403 => ApiErr::Auth(if msg.is_empty() { format!("HTTP {code}") } else { msg }),
        404 => ApiErr::Missing,
        507 => ApiErr::Quota,
        408 | 429 | 500..=599 => ApiErr::Net(format!("HTTP {code} {msg}")),
        _ => ApiErr::Http(code, msg),
    }
}

fn net(e: ureq::Error) -> ApiErr {
    ApiErr::Net(e.to_string())
}

/// 머리글에 넣는 파일 이름 (encodeURIComponent 와 같게, 경로 없이)
pub fn enc_name(name: &str) -> String {
    name.bytes().map(|c| if c.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&c) { (c as char).to_string() } else { format!("%{c:02X}") }).collect()
}

impl Api for Http {
    fn pull(&self, after: i64) -> Result<Value, ApiErr> {
        let r = self.agent.get(self.url(&format!("/api/v1/sync?after={after}&limit=200"))).header("authorization", &self.auth).call().map_err(net)?;
        Self::json_of(r)
    }

    fn push(&self, mutations: &[Value]) -> Result<Value, ApiErr> {
        let r = self.agent.post(self.url("/api/v1/sync")).header("authorization", &self.auth).send_json(json!({ "mutations": mutations })).map_err(net)?;
        Self::json_of(r)
    }

    fn list(&self, cursor: &str) -> Result<Value, ApiErr> {
        let c = if cursor.is_empty() { String::new() } else { format!("&cursor={}", enc_name(cursor)) };
        let r = self.agent.get(self.url(&format!("/api/v1/documents?include=payload&deleted=1&limit=200{c}"))).header("authorization", &self.auth).call().map_err(net)?;
        Self::json_of(r)
    }

    fn asset_find(&self, sha: &str) -> Result<Option<String>, ApiErr> {
        let r = self.agent.get(self.url(&format!("/api/v1/assets?sha256={sha}"))).header("authorization", &self.auth).call().map_err(net)?;
        let v = Self::json_of(r)?;
        Ok(v["asset"]["id"].as_str().map(String::from))
    }

    fn asset_put(&self, file: &Path, sha: &str, name: &str, ctype: &str, size: u64) -> Result<String, ApiErr> {
        let name = enc_name(name);
        if size <= ONE_SHOT {
            let f = std::fs::File::open(file).map_err(|e| ApiErr::Http(0, e.to_string()))?;
            let r = self
                .agent
                .post(self.url("/api/v1/assets"))
                .header("authorization", &self.auth)
                .header("content-type", ctype)
                .header("x-asset-sha256", sha)
                .header("x-asset-name", &name)
                .header("content-length", size.to_string())
                .config()
                .timeout_global(Some(Duration::from_secs(1800)))
                .build()
                .send(ureq::SendBody::from_owned_reader(f))
                .map_err(net)?;
            let v = Self::json_of(r)?;
            return v["id"].as_str().map(String::from).ok_or_else(|| ApiErr::Http(200, "id 없음".into()));
        }
        // 나눠 올리기 (R2 multipart)
        let r = self
            .agent
            .post(self.url("/api/v1/assets/uploads"))
            .header("authorization", &self.auth)
            .header("content-type", ctype)
            .header("x-asset-sha256", sha)
            .header("x-asset-name", &name)
            .header("x-asset-size", size.to_string())
            .send_empty()
            .map_err(net)?;
        let v = Self::json_of(r)?;
        if v["existed"] == true {
            return v["id"].as_str().map(String::from).ok_or_else(|| ApiErr::Http(200, "id 없음".into()));
        }
        let up = v["uploadId"].as_str().unwrap_or("").to_string();
        let part = v["partSize"].as_u64().filter(|n| *n >= 1 << 20).unwrap_or(16 << 20) as usize;
        if up.is_empty() {
            return Err(ApiErr::Http(200, "uploadId 없음".into()));
        }
        let go = || -> Result<String, ApiErr> {
            let mut f = std::fs::File::open(file).map_err(|e| ApiErr::Http(0, e.to_string()))?;
            let mut buf = vec![0u8; part];
            let mut n = 1u32;
            loop {
                let mut got = 0;
                while got < part {
                    match f.read(&mut buf[got..]) {
                        Ok(0) => break,
                        Ok(k) => got += k,
                        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                        Err(e) => return Err(ApiErr::Http(0, e.to_string())),
                    }
                }
                if got == 0 {
                    break;
                }
                let r = self
                    .agent
                    .put(self.url(&format!("/api/v1/assets/uploads/{up}/parts/{n}")))
                    .header("authorization", &self.auth)
                    .config()
                    .timeout_global(Some(Duration::from_secs(600)))
                    .build()
                    .send(&buf[..got])
                    .map_err(net)?;
                Self::json_of(r)?;
                n += 1;
                if got < part {
                    break;
                }
            }
            let r = self.agent.post(self.url(&format!("/api/v1/assets/uploads/{up}/complete"))).header("authorization", &self.auth).send_empty().map_err(net)?;
            let v = Self::json_of(r)?;
            v["id"].as_str().map(String::from).ok_or_else(|| ApiErr::Http(200, "id 없음".into()))
        };
        let r = go();
        if r.is_err() {
            let _ = self.agent.delete(self.url(&format!("/api/v1/assets/uploads/{up}"))).header("authorization", &self.auth).call();
        }
        r
    }

    fn asset_get(&self, id: &str) -> Result<(Vec<u8>, String), ApiErr> {
        let mut r = self
            .agent
            .get(self.url(&format!("/api/v1/assets/{}", enc_name(id))))
            .header("authorization", &self.auth)
            .config()
            .timeout_global(Some(Duration::from_secs(1800)))
            .build()
            .call()
            .map_err(net)?;
        let code = r.status().as_u16();
        if !(200..=299).contains(&code) {
            let v: Value = r.body_mut().read_json().unwrap_or(Value::Null);
            return Err(status_err(code, &v));
        }
        let ty = r.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
        let b = r.body_mut().with_config().limit(super::ASSET_MAX).read_to_vec().map_err(net)?;
        Ok((b, ty))
    }
}
