//! 클라우드 자료 동기화 (웹판 2단계 B, docs/web/sync.md): 설치판의 큐시트 · 찬양 · 템플릿 · 테마 · 교회 공통 설정을
//! 클라우드(D1 문서, docs/web/data-api.md)와 맞춘다.
//! - 기본 꺼짐. '클라우드 연결'(cloud.rs) 의 주소 · 연결 열쇠 · 켜기를 같이 쓰고, 그 위에 `cloud.sync` 하나만 더한다.
//! - 처음 한 번 전체를 올리고, 그 뒤로는 바뀐 것만 `POST /api/v1/sync` (mutationId 를 먼저 저장해 두고 보냄 → 두 번 안 들어감).
//!   받기는 `GET /api/v1/sync?after=<seq>`. 따로 도는 스레드라 조작 화면을 막지 않고, 인터넷이 끊기면 뒤에서 다시 잇기만 한다.
//! - 충돌(판이 다름)은 덮어쓰지 않는다: 클라우드 판이 원래 자리에, 이 PC 에서 고친 것은 '충돌 사본' 으로 남기고 알림 한 줄.
//!   클라우드에서 지운 것은 휴지통(큐시트) · 사본 파일(그 밖)로 남기고, 이 PC 에서 고치던 것이면 지우지 않고 되살린다.
//! - 이 PC 의 파일 주소(`/lib/file?p=C:\…` · 절대 경로 · file:/// · /media/…)는 클라우드로 보낼 때 그림·영상 저장소
//!   (`POST /api/v1/assets`, sha256)에 올리고 `/api/v1/assets/{id}` 로 바꾼다. 올리지 못한 것은 `wp-local:<표>` 로 가린다
//!   → PC 경로 · 사용자 이름이 클라우드 문서에 남지 않는다. 받을 때는 거꾸로 이 PC 의 원래 주소로 되돌린다.
//! - 출력 자료(cloud.rs 가 중계실로 보내는 render)도 같은 한 겹(`for_output`)을 지난다: 올린 것은 자산 주소, 아직이면 설치판 중계
//!   (`/lib/file?t=<표>` — 경로 대신 표, cloud.rs 가 `relay_query` 로 되돌림).

use crate::core::{Core, Ev, Shared};
use crate::db::Cue;
use crate::util::{now_ms, short_id, to36};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

pub mod api;
#[cfg(test)]
mod tests;

pub use api::{Api, ApiErr, Http};

/// 맞추는 문서 종류 (클라우드 kind). 그 밖의 종류(overlay · macro · usage …)는 받아도 건너뛴다
pub const KINDS: [&str; 5] = ["cue", "song", "template", "theme", "settings"];
/// 교회 공통 설정 문서 id
pub const CHURCH: &str = "church";
/// 교회 공통 설정으로 올리는 칸 (기기마다 다른 것 — 모니터 · 포트 · 장비 · 열쇠 · 폴더 · 창 배치 · 화면 언어 — 은 넣지 않는다)
pub const CHURCH_KEYS: &[&str] = &[
    "overlays", "macros", "schedule", "stagePresets", "messageTemplates", "people", "scenes", "sources", "outputScenes", "typeScenes", "sceneTransition", "outputTransitions",
    "split", "hymnBackground", "nameTagSeconds", "logoScreenImage", "songSearchUrl",
];
/// 문서 하나 한도 (서버 DATA_LIMITS.document = 1 MiB)
const DOC_MAX: usize = 1 << 20;
/// 한 번에 올리는 개수 · 양 (서버: 20개 · 4 MiB)
const BATCH: usize = 20;
const BATCH_BYTES: usize = 3 << 20;
/// 파일 하나 한도 (이보다 크면 올리지 않고 가린다)
pub const ASSET_MAX: u64 = 2 << 30;
/// 가림표로 올라간 문서를 다시 올려 볼 간격
const PARTIAL_RETRY_MS: i64 = 10 * 60 * 1000;

fn t(ko: &str, en: &str) -> String {
    if crate::i18n::is_en() { en.to_string() } else { ko.to_string() }
}

// ---------- 설정 ----------
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncConf {
    /// 클라우드 연결이 켜져 있고(cloud.enabled) 동기화도 켰고(cloud.sync) 주소 · 열쇠가 맞음
    pub on: bool,
    pub url: String,
    pub key: String,
    pub church: String,
}

pub fn sync_conf(core: &Core) -> SyncConf {
    let c = crate::cloud::conf(core);
    let church = c.key.split_once('.').map(|x| x.0.to_string()).unwrap_or_default();
    let on = c.enabled && wanted(core) && crate::cloud::key_ok(&c.key) && crate::cloud::bridge_url(&c.url).is_ok();
    SyncConf { on, url: c.url, key: c.key, church }
}

/// 설정 `cloud.sync` (기본 꺼짐)
pub fn wanted(core: &Core) -> bool {
    core.settings.get()["cloud"]["sync"].as_bool().unwrap_or(false)
}

pub fn set_wanted(core: &mut Core, on: bool) {
    core.settings.update(|d| {
        if !d["cloud"].is_object() {
            d["cloud"] = json!({});
        }
        d["cloud"]["sync"] = json!(on);
    });
    core.emit(Ev::Ui);
}

// ---------- 화면에 보일 상태 ----------
#[derive(Clone, Debug, Default)]
pub struct Status {
    /// off · syncing · ok · offline · error
    pub state: String,
    pub error: String,
    pub last_ok: i64,
    pub pushed: u64,
    pub pulled: u64,
    pub conflicts: u64,
    pub uploaded: u64,
    /// 처음 올리기 · 큰 묶음 진행 ("120/950")
    pub progress: String,
    /// 그림·영상 저장소 상태: '' (씀) · missing (클라우드에 아직 없음) · quota (한도) · error
    pub assets: String,
}
static STATUS: Lazy<Mutex<Status>> = Lazy::new(|| Mutex::new(Status { state: "off".into(), ..Default::default() }));
static STARTED: AtomicBool = AtomicBool::new(false);

pub fn status() -> Status {
    STATUS.lock().clone()
}
fn set_state(state: &str, error: &str) {
    let mut s = STATUS.lock();
    s.state = state.into();
    s.error = error.into();
    if state == "ok" {
        s.last_ok = now_ms();
        s.progress.clear();
    }
}

// ---------- 파일 주소 ----------
fn pct_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let h = |c: u8| (c as char).to_digit(16);
            if let (Some(a), Some(b2)) = (h(b[i + 1]), h(b[i + 2])) {
                let x = (a * 16 + b2) as u8;
                out.push(x);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

/// 이 PC 의 절대 경로처럼 보이나 (C:\ · C:/ · \\서버\ · 맥 /Users/ · /Volumes/)
pub fn is_pc_path(s: &str) -> bool {
    let b = s.as_bytes();
    (b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/'))
        || (s.starts_with("\\\\") && s.len() > 3)
        || s.starts_with("/Users/")
        || s.starts_with("/Volumes/")
        || s.starts_with("/home/")
}

/// 설치판 자료 안의 '이 PC 의 파일' 주소 → 그 파일. `/media/…` 는 자료 폴더의 media 안 파일
pub fn local_file(media_dir: &Path, s: &str) -> Option<PathBuf> {
    let s = s.trim();
    if let Some(q) = s.strip_prefix("/lib/file?p=") {
        let p = pct_decode(q.split(['&', '#']).next().unwrap_or(""));
        return (!p.is_empty()).then(|| PathBuf::from(p));
    }
    if let Some(n) = s.strip_prefix("/media/") {
        let n = n.split(['?', '#']).next().unwrap_or("");
        let n = pct_decode(n);
        if n.is_empty() || n.contains(['/', '\\']) || n.contains("..") {
            return None;
        }
        return Some(media_dir.join(n));
    }
    if let Some(p) = s.strip_prefix("file:///") {
        let p = pct_decode(p);
        // file:///C:/… 는 C:/… , file:///Users/… 는 /Users/…
        return Some(PathBuf::from(if p.as_bytes().get(1) == Some(&b':') { p } else { format!("/{p}") }));
    }
    is_pc_path(s).then(|| PathBuf::from(s))
}

fn ext_of(p: &Path) -> String {
    p.extension().and_then(|e| e.to_str()).filter(|e| e.len() <= 8 && e.chars().all(|c| c.is_ascii_alphanumeric())).map(|e| format!(".{}", e.to_ascii_lowercase())).unwrap_or_default()
}

fn ctype_of(ext: &str) -> &'static str {
    match ext {
        ".png" => "image/png",
        ".jpg" | ".jpeg" => "image/jpeg",
        ".gif" => "image/gif",
        ".webp" => "image/webp",
        ".bmp" => "image/bmp",
        ".svg" => "image/svg+xml",
        ".mp4" | ".m4v" => "video/mp4",
        ".webm" => "video/webm",
        ".mov" => "video/quicktime",
        ".mp3" => "audio/mpeg",
        ".wav" => "audio/wav",
        ".m4a" => "audio/mp4",
        ".ogg" => "audio/ogg",
        ".pdf" => "application/pdf",
        ".pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        ".ppsx" => "application/vnd.openxmlformats-officedocument.presentationml.slideshow",
        ".ppt" | ".pps" => "application/vnd.ms-powerpoint",
        _ => "application/octet-stream",
    }
}

/// 클라우드 그림·영상 저장소가 받는 종류 · 크기 (docs/web/assets-api.md 2절). 그 밖(pptx · 너무 큰 그림 …)은 올리지 않고 가린다
pub fn upload_ok(ext: &str, size: u64) -> bool {
    const MI: u64 = 1 << 20;
    let max = match ext {
        ".svg" => 10 * MI,
        ".png" | ".jpg" | ".jpeg" | ".jfif" | ".gif" | ".webp" | ".avif" | ".bmp" => 50 * MI,
        ".mp4" | ".m4v" | ".webm" | ".mov" | ".ogv" => 2 << 30,
        ".mp3" | ".m4a" | ".aac" | ".wav" | ".ogg" | ".oga" | ".opus" | ".flac" => 500 * MI,
        ".ttf" | ".otf" | ".ttc" | ".woff" | ".woff2" => 50 * MI,
        ".pdf" => 200 * MI,
        // PowerPoint (2026-11-28 사관님 '슬라이드를 온라인에 저장') — 클라우드는 내려받기로만 내보낸다 (attachment)
        ".pptx" | ".ppsx" | ".ppt" | ".pps" => 500 * MI,
        _ => 0,
    };
    size > 0 && size <= max
}

fn ext_of_ctype(ct: &str) -> &'static str {
    let ct = ct.split(';').next().unwrap_or("").trim();
    match ct {
        "image/png" => ".png",
        "image/jpeg" => ".jpg",
        "image/gif" => ".gif",
        "image/webp" => ".webp",
        "image/svg+xml" => ".svg",
        "video/mp4" => ".mp4",
        "video/webm" => ".webm",
        "video/quicktime" => ".mov",
        "audio/mpeg" => ".mp3",
        "audio/wav" => ".wav",
        "application/pdf" => ".pdf",
        _ => "",
    }
}

/// 자산 주소. 확장자를 끝에 붙인다 (render.js isVideo 는 주소 끝의 .mp4 · .webm 로 영상을 가린다)
pub fn asset_url(id: &str, ext: &str) -> String {
    if ext.is_empty() { format!("/api/v1/assets/{id}") } else { format!("/api/v1/assets/{id}?e={ext}") }
}

/// `/api/v1/assets/{id}[?e=.png]` → (id, 확장자)
pub fn parse_asset(s: &str) -> Option<(String, String)> {
    let rest = s.trim().strip_prefix("/api/v1/assets/")?;
    let (id, q) = rest.split_once('?').unwrap_or((rest, ""));
    if id.is_empty() || id.len() > 128 || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return None;
    }
    let ext = q.split('&').find_map(|kv| kv.strip_prefix("e=")).unwrap_or("").to_string();
    let ext = if ext.starts_with('.') && ext.len() <= 9 && ext[1..].chars().all(|c| c.is_ascii_alphanumeric()) { ext } else { String::new() };
    Some((id.to_string(), ext))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// 문서 본문의 지문 (serde_json 은 칸을 이름 차례로 쓴다 → 같은 내용이면 같은 지문)
pub fn digest(v: &Value) -> String {
    hex(&Sha256::digest(v.to_string().as_bytes())[..16])
}

fn sha256_file(p: &Path) -> Option<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(p).ok()?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        match f.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => h.update(&buf[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return None,
        }
    }
    Some(hex(&h.finalize()))
}

/// 글 칸 하나하나를 f 로 바꾼다 (None 은 그대로). 칸 이름은 바꾸지 않는다
fn map_strings(v: &mut Value, f: &mut dyn FnMut(&str) -> Option<String>) {
    match v {
        Value::String(s) => {
            if let Some(n) = f(s) {
                *s = n;
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|x| map_strings(x, f)),
        Value::Object(o) => o.values_mut().for_each(|x| map_strings(x, f)),
        _ => {}
    }
}

/// 긴 글 안에 낀 주소 (CSS url(…) 같은 것): `pat` 로 시작해 따옴표 · 괄호 · 빈칸 앞까지를 f 로 바꾼다
fn replace_embedded(s: &str, pat: &str, f: &mut dyn FnMut(&str) -> Option<String>) -> Option<String> {
    if !s.contains(pat) {
        return None;
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    let mut changed = false;
    while let Some(i) = rest.find(pat) {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        // 파일 이름의 괄호는 file_url 이 그대로 둔다 ("배경 (1).png") → 짝이 맞지 않는 ')' 에서만 끊는다
        let mut depth = 0i32;
        let end = tail
            .char_indices()
            .find(|&(_, c)| match c {
                '(' => {
                    depth += 1;
                    false
                }
                ')' if depth > 0 => {
                    depth -= 1;
                    false
                }
                ')' | '"' | '\'' => true,
                c => c.is_whitespace(),
            })
            .map(|x| x.0)
            .unwrap_or(tail.len());
        let url = &tail[..end];
        match f(url) {
            Some(n) => {
                out.push_str(&n);
                changed = true;
            }
            None => out.push_str(url),
        }
        rest = &tail[end..];
    }
    out.push_str(rest);
    changed.then_some(out)
}

// ---------- 설치판 문서 ----------
pub fn key(kind: &str, id: &str) -> String {
    format!("{kind}/{id}")
}

/// 클라우드 문서 id 규칙 (cloud/src/data/routes.mjs validDocId)
pub fn valid_doc_id(id: &str) -> bool {
    let b = id.as_bytes();
    !b.is_empty() && b.len() <= 128 && (b[0].is_ascii_alphanumeric() || b[0] == b'_') && b.iter().all(|c| c.is_ascii_alphanumeric() || b"_.:~@-".contains(c))
}

fn song_id_ok(id: &str) -> bool {
    (4..=40).contains(&id.len()) && id.chars().all(|c| c.is_ascii_alphanumeric())
}

pub fn cue_payload(c: &Cue) -> Value {
    json!({ "id": c.id, "name": c.name, "serviceDate": c.service_date, "startTime": c.start_time, "items": c.items, "memo": c.memo, "pinned": c.pinned, "template": c.template })
}

fn song_payload(mut s: Value) -> Value {
    if let Some(o) = s.as_object_mut() {
        o.remove("updatedAt");
    }
    s
}

/// 교회 공통 설정 (CHURCH_KEYS 만, 오버레이의 켜짐 여부는 PC 마다라 뺀다)
pub fn church_settings(s: &Value) -> Value {
    let mut o = serde_json::Map::new();
    for k in CHURCH_KEYS {
        if let Some(v) = s.get(*k).filter(|v| !v.is_null()) {
            let mut v = v.clone();
            if *k == "overlays" {
                for x in v.as_array_mut().into_iter().flatten() {
                    if let Some(m) = x.as_object_mut() {
                        m.remove("on");
                    }
                }
            }
            o.insert(k.to_string(), v);
        }
    }
    Value::Object(o)
}

fn list_of<'a>(s: &'a Value, k: &str) -> &'a [Value] {
    s[k].as_array().map(|a| a.as_slice()).unwrap_or(&[])
}

fn list_key(kind: &str) -> &'static str {
    if kind == "template" { "templates" } else { "customThemes" }
}

/// 설치판의 문서 하나 (없거나 휴지통이면 None)
pub fn local_one(core: &Core, kind: &str, id: &str) -> Option<Value> {
    match kind {
        "cue" => {
            let live: Option<String> = core.store.db.query_row("SELECT deleted_at FROM Cuesheets WHERE id = ?", [id], |r| r.get(0)).ok();
            if live.as_deref() != Some("") {
                return None;
            }
            core.store.get_cue(id).map(|c| cue_payload(&c))
        }
        "song" => crate::songs::get_song(&core.store.db, id).map(song_payload),
        "template" | "theme" => list_of(core.settings.get(), list_key(kind)).iter().find(|x| x["id"] == id).cloned(),
        "settings" if id == CHURCH => Some(church_settings(core.settings.get())),
        _ => None,
    }
}

/// 설치판의 문서 전체: "종류/id" → 본문
pub fn local_docs(core: &Core) -> BTreeMap<String, Value> {
    let mut out = BTreeMap::new();
    let db = &core.store.db;
    let ids = |sql: &str| -> Vec<String> {
        db.prepare(sql).and_then(|mut st| st.query_map([], |r| r.get::<_, String>(0)).map(|it| it.filter_map(|x| x.ok()).collect())).unwrap_or_default()
    };
    for id in ids("SELECT id FROM Cuesheets WHERE deleted_at = ''") {
        if valid_doc_id(&id) {
            if let Some(c) = core.store.get_cue(&id) {
                out.insert(key("cue", &id), cue_payload(&c));
            }
        }
    }
    for id in ids("SELECT id FROM Songs") {
        if valid_doc_id(&id) {
            if let Some(s) = crate::songs::get_song(db, &id) {
                out.insert(key("song", &id), song_payload(s));
            }
        }
    }
    let s = core.settings.get();
    for kind in ["template", "theme"] {
        for x in list_of(s, list_key(kind)) {
            if let Some(id) = x["id"].as_str().filter(|i| valid_doc_id(i)) {
                out.insert(key(kind, id), x.clone());
            }
        }
    }
    out.insert(key("settings", CHURCH), church_settings(s));
    out
}

/// 바뀐 것이 있나 싸게 보는 서명
pub fn local_sig(core: &Core) -> String {
    let db = &core.store.db;
    let a: (i64, Option<String>) = db.query_row("SELECT COUNT(*), MAX(updated_at) FROM Songs", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap_or((0, None));
    let b: (i64, Option<String>, Option<String>) = db.query_row("SELECT COUNT(*), MAX(updated_at), MAX(deleted_at) FROM Cuesheets WHERE deleted_at = ''", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap_or((0, None, None));
    let c: (i64, Option<String>) = db.query_row("SELECT COUNT(*), MAX(deleted_at) FROM Cuesheets WHERE deleted_at != ''", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap_or((0, None));
    let s = core.settings.get();
    let rest = digest(&json!([s["templates"], s["customThemes"], church_settings(s)]));
    format!("{}|{}|{}|{}|{}|{}|{rest}", a.0, a.1.unwrap_or_default(), b.0, b.1.unwrap_or_default(), c.0, c.1.unwrap_or_default())
}

fn refresh_cues(core: &mut Core, id: &str) {
    core.cues = core.store.list_cues();
    if core.cue.as_ref().is_some_and(|c| c.id == id) {
        core.cue = core.store.get_cue(id);
    }
}

fn conflict_suffix() -> String {
    let (_, hm, _) = crate::localtime::local_now();
    format!(" ({} {hm})", t("충돌 사본", "conflict copy"))
}

/// 클라우드 판을 설치판에 넣기 (payload 는 이미 이 PC 의 주소로 되돌린 것)
fn apply_put(core: &mut Core, kind: &str, id: &str, p: &Value) -> Result<(), String> {
    let g = |k: &str| p[k].as_str().unwrap_or("").to_string();
    match kind {
        "cue" => {
            let name = g("name");
            let c = Cue {
                id: id.to_string(),
                name: if name.trim().is_empty() { t("(이름 없음)", "(untitled)") } else { name },
                service_date: g("serviceDate"),
                start_time: g("startTime"),
                items: p["items"].as_array().cloned().unwrap_or_default(),
                memo: g("memo"),
                pinned: p["pinned"].as_bool().unwrap_or(false),
                template: p["template"].as_bool().unwrap_or(false),
            };
            core.store.save_cue(&c).map_err(|e| e.to_string())?;
            // 휴지통에 있던 것이면 꺼낸다 (save_cue 는 휴지통 표시를 그대로 둔다)
            crate::cues::restore(&core.store, id).map_err(|e| e.to_string())?;
            refresh_cues(core, id);
        }
        "song" => {
            if !song_id_ok(id) {
                return Err(format!("찬양 id 는 영문 · 숫자 4~40자여야 합니다: {id}"));
            }
            let mut s = p.clone();
            s["id"] = json!(id);
            if s["title"].as_str().is_none_or(|x| x.trim().is_empty()) {
                s["title"] = json!(t("(제목 없음)", "(untitled)"));
            }
            crate::songs::save_song(&core.store.db, &s).map_err(|e| e.to_string())?;
        }
        "template" | "theme" => {
            if !p.is_object() {
                return Err("본문이 객체가 아닙니다.".into());
            }
            let mut x = p.clone();
            x["id"] = json!(id);
            let lk = list_key(kind);
            core.settings.update(|d| {
                if !d[lk].is_array() {
                    d[lk] = json!([]);
                }
                let a = d[lk].as_array_mut().unwrap();
                match a.iter().position(|y| y["id"] == id) {
                    Some(i) => a[i] = x,
                    None => a.push(x),
                }
            });
            core.emit(Ev::Live);
        }
        "settings" => {
            let p = p.clone();
            core.settings.update(|d| {
                for k in CHURCH_KEYS {
                    let Some(v) = p.get(*k) else { continue };
                    let mut v = v.clone();
                    if *k == "overlays" {
                        // 켜짐 여부는 이 PC 것
                        let mine = d["overlays"].as_array().cloned().unwrap_or_default();
                        for o in v.as_array_mut().into_iter().flatten() {
                            let on = mine.iter().find(|m| m["id"] == o["id"]).and_then(|m| m["on"].as_bool()).unwrap_or(false);
                            o["on"] = json!(on);
                        }
                    }
                    d[*k] = v;
                }
            });
            core.emit(Ev::Live);
        }
        _ => return Err("모르는 종류".into()),
    }
    core.emit(Ev::Ui);
    Ok(())
}

/// 클라우드에서 지운 것: 큐시트는 휴지통, 그 밖은 사본 파일을 남기고 지운다. 쓰는 중인 템플릿 · 기본 템플릿 · 설정은 지우지 않는다 → false
fn apply_delete(core: &mut Core, kind: &str, id: &str, dir: &Path) -> Result<bool, String> {
    let keep_file = |name: &str, v: &Value| {
        let d = dir.join("removed");
        let _ = std::fs::create_dir_all(&d);
        let _ = std::fs::write(d.join(format!("{name}-{}.json", now_ms())), serde_json::to_string_pretty(v).unwrap_or_default());
    };
    match kind {
        "cue" => {
            crate::cues::trash(&core.store, id).map_err(|e| e.to_string())?;
            refresh_cues(core, id);
        }
        "song" => {
            if let Some(s) = crate::songs::get_song(&core.store.db, id) {
                keep_file(&format!("song-{id}"), &s);
            }
            crate::songs::delete_song(&core.store.db, id).map_err(|e| e.to_string())?;
        }
        "template" | "theme" => {
            let s = core.settings.get();
            let lk = list_key(kind);
            let Some(x) = list_of(s, lk).iter().find(|x| x["id"] == id).cloned() else { return Ok(true) };
            if kind == "template" {
                let active = s["activeTemplates"].as_object().is_some_and(|o| o.values().any(|v| v == id));
                let in_theme = list_of(s, "customThemes").iter().any(|th| th["slots"].as_object().is_some_and(|o| o.values().any(|v| v == id)));
                if x["preset"] == true || active || in_theme {
                    return Ok(false);
                }
            }
            keep_file(&format!("{kind}-{id}"), &x);
            core.settings.update(|d| {
                if let Some(a) = d[lk].as_array_mut() {
                    a.retain(|y| y["id"] != id);
                }
            });
            core.emit(Ev::Live);
        }
        _ => return Ok(false),
    }
    core.emit(Ev::Ui);
    Ok(true)
}

/// 이 PC 에서 고친 것을 '충돌 사본' 으로 남긴다 → 사본 이름 (설정은 파일로)
fn make_copy(core: &mut Core, kind: &str, local: &Value, dir: &Path) -> Result<String, String> {
    let suf = conflict_suffix();
    match kind {
        "cue" => {
            let mut p = local.clone();
            let name = format!("{}{suf}", p["name"].as_str().unwrap_or(""));
            p["name"] = json!(name);
            let id = short_id("c");
            apply_put(core, "cue", &id, &p)?;
            Ok(name)
        }
        "song" => {
            let mut p = local.clone();
            let name = format!("{}{suf}", p["title"].as_str().unwrap_or(""));
            p["title"] = json!(name);
            let id = short_id("s");
            apply_put(core, "song", &id, &p)?;
            Ok(name)
        }
        "template" | "theme" => {
            let mut p = local.clone();
            let name = format!("{}{suf}", p["name"].as_str().unwrap_or(""));
            p["name"] = json!(name);
            if kind == "template" {
                p["preset"] = json!(false);
            }
            let id = format!("{}-{}{}", if kind == "template" { "tpl" } else { "theme" }, to36(now_ms() as u64), to36(rand::random::<u16>() as u64));
            apply_put(core, kind, &id, &p)?;
            Ok(name)
        }
        _ => {
            let d = dir.join("conflicts");
            let _ = std::fs::create_dir_all(&d);
            let f = d.join(format!("settings-{}.json", now_ms()));
            std::fs::write(&f, serde_json::to_string_pretty(local).unwrap_or_default()).map_err(|e| e.to_string())?;
            Ok(f.to_string_lossy().to_string())
        }
    }
}

// ---------- 동기화 기록 (data/cloudsync/state.json) ----------
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Entry {
    /// 클라우드 판 번호 (마지막으로 맞춘 것)
    pub rev: i64,
    /// 그때 설치판 본문의 지문
    #[serde(default)]
    pub hash: String,
    #[serde(default)]
    pub deleted: bool,
    /// 이 PC 가 담을 수 없는 문서 (찬양 id 규칙 밖 등) — 올리지도 지우지도 않는다
    #[serde(default)]
    pub skip: bool,
    /// 올리기를 서버가 거절한 본문 지문 (같은 것을 계속 보내지 않게)
    #[serde(default)]
    pub failed: String,
    /// 파일을 다 올리지 못해 가림표로 올라감 → 나중에 다시
    #[serde(default)]
    pub partial: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Mutation {
    #[serde(rename = "mutationId")]
    pub mutation_id: String,
    pub kind: String,
    pub id: String,
    pub op: String,
    pub base: i64,
    #[serde(default)]
    pub payload: Option<Value>,
    /// 설치판 본문 지문 (보내지 않음)
    #[serde(default)]
    pub hash: String,
    #[serde(default)]
    pub partial: bool,
}

impl Mutation {
    fn wire(&self) -> Value {
        let mut v = json!({ "mutationId": self.mutation_id, "kind": self.kind, "id": self.id, "op": self.op, "baseRevision": self.base });
        if let Some(p) = &self.payload {
            v["payload"] = p.clone();
        }
        v
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct FileRec {
    pub size: u64,
    pub mtime: i64,
    pub sha: String,
    #[serde(default)]
    pub asset: String,
    /// 클라우드가 이 파일을 받지 않음 (종류 · 크기) — 파일이 바뀔 때까지 다시 올리지 않는다
    #[serde(default)]
    pub bad: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct State {
    pub v: u32,
    pub church: String,
    pub url: String,
    /// 이 PC 의 무작위 이름 (mutationId 앞머리)
    pub device: String,
    pub counter: u64,
    /// 가림표를 만드는 소금 (경로를 짐작하지 못하게)
    pub salt: String,
    /// 받은 마지막 seq
    pub cursor: i64,
    pub docs: BTreeMap<String, Entry>,
    /// 보내기 전에 저장해 둔 것 (답을 못 받았으면 같은 mutationId 로 다시)
    pub outbox: Vec<Mutation>,
    /// 파일 경로 → 크기 · 고친 시각 · sha256 · 자산 id
    pub files: BTreeMap<String, FileRec>,
    /// sha256 → 자산 id
    pub assets: BTreeMap<String, String>,
    /// 자산 id → 이 PC 의 원래 주소 (받을 때 되돌림)
    pub origins: BTreeMap<String, String>,
    /// 가림표 → 이 PC 의 원래 주소
    pub tokens: BTreeMap<String, String>,
    /// 처음 전체 올리기를 마쳤나
    pub seeded: bool,
}

impl State {
    fn fresh(church: &str, url: &str) -> State {
        State {
            v: 1,
            church: church.into(),
            url: url.into(),
            device: format!("pc{}", to36(rand::random::<u64>() >> 16)),
            salt: hex(&rand::random::<[u8; 16]>()),
            ..Default::default()
        }
    }

    fn file(dir: &Path) -> PathBuf {
        dir.join("state.json")
    }

    /// 같은 교회 · 같은 주소의 기록. 교회나 주소가 바뀌면 처음부터 (다른 교회 자료와 섞이지 않게)
    pub fn load(dir: &Path, church: &str, url: &str) -> State {
        let s: Option<State> = std::fs::read_to_string(Self::file(dir)).ok().and_then(|t| serde_json::from_str(&t).ok());
        match s {
            Some(s) if s.church == church && s.url == url && s.v == 1 => s,
            Some(old) => {
                // 앞 교회의 기록은 지우지 않고 옆으로 (나중에 원인 찾기)
                let _ = std::fs::rename(Self::file(dir), dir.join(format!("state-{}-{}.json", old.church, now_ms())));
                State::fresh(church, url)
            }
            None => State::fresh(church, url),
        }
    }

    pub fn save(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let tmp = dir.join("state.json.tmp");
        std::fs::write(&tmp, serde_json::to_vec(self).unwrap_or_default())?;
        std::fs::rename(&tmp, Self::file(dir))
    }

    fn next_id(&mut self) -> String {
        self.counter += 1;
        format!("{}-{}-{}", self.device, to36(now_ms() as u64), to36(self.counter))
    }

    fn token(&mut self, orig: &str) -> String {
        let tok = hex(&Sha256::digest(format!("{}\u{0}{orig}", self.salt).as_bytes())[..12]);
        self.tokens.insert(tok.clone(), orig.to_string());
        format!("wp-local:{tok}")
    }
}

// ---------- 출력 자료의 파일 주소 (cloud.rs 가 부름) ----------
#[derive(Default)]
struct OutCache {
    on: bool,
    media_dir: PathBuf,
    salt: String,
    /// 파일 경로 → (크기, 고친 시각, 자산 id)
    known: HashMap<String, (u64, i64, String)>,
    /// 중계 표 → 파일 경로
    tokens: HashMap<String, String>,
    /// 올려 달라고 쌓인 것 (파일 경로, 원래 주소)
    want: Vec<(String, String)>,
    /// 영상 열쇠: 가린 것 → 원래 ('주소#시각' 이라 PC 경로가 들어 있다)
    vkeys: HashMap<String, String>,
}
static OUT: Lazy<Mutex<OutCache>> = Lazy::new(|| Mutex::new(OutCache { salt: hex(&rand::random::<[u8; 16]>()), ..Default::default() }));

fn mtime_ms(m: &std::fs::Metadata) -> i64 {
    m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// 클라우드로 보내는 출력 자료 한 겹: 동기화가 꺼져 있으면 그대로(1단계와 같음).
/// 켜져 있으면 이 PC 파일 주소를 → 올린 것은 `/api/v1/assets/{id}`, 아직이면 `/lib/file?t=<표>`(설치판 중계, 경로 대신 표) + 올리기 줄에.
pub fn for_output(mut v: Value) -> Value {
    let mut o = OUT.lock();
    if !o.on {
        return v;
    }
    // 영상 열쇠(video.key = '주소#시각'): 출력 화면이 videoMeta · videoEnded 로 그대로 돌려보낸다 → 가린 열쇠로, 받을 때 되돌림(relay_command)
    if let Some(k) = v["video"]["key"].as_str().filter(|k| !k.starts_with("vk-")).map(String::from) {
        let vk = format!("vk-{}", hex(&Sha256::digest(format!("{}\u{0}{k}", o.salt).as_bytes())[..12]));
        if o.vkeys.len() > 500 {
            o.vkeys.clear();
        }
        o.vkeys.insert(vk.clone(), k);
        v["video"]["key"] = json!(vk);
    }
    let mut f = |s: &str| -> Option<String> {
        let p = local_file(&o.media_dir, s)?;
        let ps = p.to_string_lossy().to_string();
        let media = s.trim().starts_with("/media/");
        // 1단계 중계 주소: /media/… 는 이름뿐이라 그대로, 그 밖은 경로 대신 표
        let relay = if media {
            s.trim().to_string()
        } else {
            let tok = hex(&Sha256::digest(format!("{}\u{0}{ps}", o.salt).as_bytes())[..12]);
            o.tokens.insert(tok.clone(), ps.clone());
            format!("/lib/file?t={tok}{}", ext_of(&p))
        };
        let meta = std::fs::metadata(&p).ok();
        if let Some(m) = &meta {
            if let Some((size, mt, id)) = o.known.get(&ps) {
                if *size == m.len() && *mt == mtime_ms(m) {
                    // 클라우드에 있음 → 자산 주소. 클라우드에서 없어지면 Worker 가 fallback(중계)으로 302 (assets-api.md 5절)
                    return Some(format!("/api/v1/assets/{id}?fallback={}", api::enc_name(&relay)));
                }
            }
        }
        if meta.as_ref().is_some_and(|m| m.is_file()) && !o.want.iter().any(|w| w.0 == ps) && o.want.len() < 200 {
            o.want.push((ps.clone(), s.to_string()));
        }
        (!media).then_some(relay)
    };
    map_strings(&mut v, &mut |s| {
        if local_file(Path::new(""), s).is_some() || s.trim().starts_with("/media/") {
            return f(s);
        }
        replace_embedded(s, "/lib/file?p=", &mut f)
    });
    v
}

/// 클라우드가 청한 중계 주소의 표(`t`)를 이 PC 의 경로(`p`)로 되돌린다 (cloud.rs 의 미디어 요청)
pub fn relay_query(q: &Value) -> Value {
    let Some(tk) = q["t"].as_str() else { return q.clone() };
    let tok = tk.split('.').next().unwrap_or("");
    let path = OUT.lock().tokens.get(tok).cloned().unwrap_or_default();
    json!({ "p": path })
}

/// 출력 화면의 영상 알림(cloud.rs view_command): 가린 영상 열쇠 → 원래 열쇠
pub fn relay_command(m: &Value) -> Value {
    let mut m = m.clone();
    if let Some(k) = m["args"]["key"].as_str().filter(|k| k.starts_with("vk-")) {
        if let Some(orig) = OUT.lock().vkeys.get(k).cloned() {
            m["args"]["key"] = json!(orig);
        }
    }
    m
}

/// 미디어 요청 전체 (query 만 relay_query 로)
pub fn relay_request(m: &Value) -> Value {
    let mut m = m.clone();
    if m["query"]["t"].is_string() {
        m["query"] = relay_query(&m["query"]);
    }
    m
}

// ---------- 엔진 ----------
#[derive(Default, Debug, Clone)]
pub struct Report {
    pub pulled: usize,
    pub pushed: usize,
    pub uploaded: usize,
    /// 충돌 사본 이름
    pub conflicts: Vec<String>,
    /// 클라우드에서 지웠지만 이 PC 에서 고치던 것 · 쓰는 중이라 남긴 것
    pub kept: Vec<String>,
    pub errors: Vec<String>,
}

pub struct Engine<'a> {
    core: &'a Shared,
    api: &'a dyn Api,
    pub st: State,
    dir: PathBuf,
    media_dir: PathBuf,
    /// 그림·영상 올리기를 멈춘 까닭 (missing · quota · error)
    pub assets_off: Option<String>,
    assets_off_at: Option<Instant>,
    last_sig: String,
    pub report: Report,
}

impl<'a> Engine<'a> {
    pub fn new(core: &'a Shared, api: &'a dyn Api, church: &str, url: &str) -> Self {
        let (dir, media_dir) = {
            let c = core.lock();
            (c.data_dir.join("cloudsync"), c.media_dir.clone())
        };
        let st = State::load(&dir, church, url);
        Engine { core, api, st, dir, media_dir, assets_off: None, assets_off_at: None, last_sig: String::new(), report: Report::default() }
    }

    fn save(&self) {
        if let Err(e) = self.st.save(&self.dir) {
            log::warn!("클라우드 동기화 기록을 쓰지 못함: {e}");
        }
    }

    /// 한 바퀴: 못 받은 답 다시 → 받기 → 바뀐 것 올리기
    pub fn tick(&mut self) -> Result<(), ApiErr> {
        self.report = Report::default();
        // 그림·영상 길이 없거나 한도였으면 30분마다 다시 본다
        if self.assets_off_at.is_some_and(|t| Instant::now() >= t) {
            self.assets_off = None;
            self.assets_off_at = None;
        }
        self.flush_outbox()?;
        self.pull()?;
        self.push_local()?;
        self.publish_out();
        Ok(())
    }

    fn flush_outbox(&mut self) -> Result<(), ApiErr> {
        if self.st.outbox.is_empty() {
            return Ok(());
        }
        let batch = self.st.outbox.clone();
        let wire: Vec<Value> = batch.iter().map(Mutation::wire).collect();
        let r = self.api.push(&wire)?;
        self.results(&batch, &r)?;
        self.st.outbox.clear();
        self.save();
        Ok(())
    }

    fn pull(&mut self) -> Result<(), ApiErr> {
        for _ in 0..10_000 {
            let r = self.api.pull(self.st.cursor)?;
            if r["ok"] != true {
                return Err(ApiErr::Http(200, r["error"].as_str().unwrap_or("").into()));
            }
            if r["reset"] == true {
                // 변경 기록이 잘렸거나(after 가 너무 옛것) 교회를 옮김 → 목록부터 전체를 다시 맞춘다
                self.resync(r["head"].as_i64().unwrap_or(0))?;
                continue;
            }
            for c in r["changes"].as_array().into_iter().flatten() {
                if c["superseded"] == true {
                    continue;
                }
                self.remote_change(c)?;
            }
            if let Some(n) = r["next"].as_i64() {
                self.st.cursor = n.max(self.st.cursor);
            }
            self.save();
            if r["more"] != true {
                break;
            }
        }
        Ok(())
    }

    /// reset 뒤: 클라우드 문서 전체를 받아 하나씩 맞춘다. 판 번호는 믿지 않고(옮긴 교회는 판이 처음부터일 수 있다) 지문으로 본다.
    /// 이 PC 에서 고치고 아직 안 올린 것은 충돌 규칙대로 사본으로 남기고, 클라우드에 없는 이 PC 문서는 지우지 않고 새로 올린다.
    fn resync(&mut self, head: i64) -> Result<(), ApiErr> {
        log::info!("클라우드 동기화: 변경 기록을 처음부터 다시 맞춤 (reset, head {head})");
        let mut remote: Vec<Value> = vec![];
        let mut cursor = String::new();
        for _ in 0..100_000 {
            let r = self.api.list(&cursor)?;
            if r["ok"] != true {
                return Err(ApiErr::Http(200, r["error"].as_str().unwrap_or("").into()));
            }
            remote.extend(r["documents"].as_array().cloned().unwrap_or_default());
            match r["next"].as_str().filter(|n| !n.is_empty()) {
                Some(n) => cursor = n.to_string(),
                None => break,
            }
        }
        let mut seen = std::collections::HashSet::new();
        for d in &remote {
            let (kind, id) = (d["kind"].as_str().unwrap_or(""), d["id"].as_str().unwrap_or(""));
            if !KINDS.contains(&kind) || id.is_empty() || (kind == "settings" && id != CHURCH) {
                continue;
            }
            let k = key(kind, id);
            seen.insert(k.clone());
            let rev = d["revision"].as_i64().unwrap_or(0);
            let deleted = d["deleted"] == true;
            if deleted && self.st.docs.get(&k).is_some_and(|e| e.deleted && e.rev == rev) {
                continue;
            }
            self.report.pulled += 1;
            self.settle(kind, id, rev, (!deleted).then(|| d["payload"].clone()), false);
        }
        // 클라우드에 없는 것: 기록만 지운다 → 이 PC 에 있으면 다음 올리기에서 새로 (지우지 않음)
        self.st.docs.retain(|k, _| seen.contains(k));
        self.st.outbox.clear();
        self.st.cursor = head;
        self.last_sig.clear();
        self.save();
        Ok(())
    }

    fn remote_change(&mut self, c: &Value) -> Result<(), ApiErr> {
        let (kind, id) = (c["kind"].as_str().unwrap_or(""), c["id"].as_str().unwrap_or(""));
        let rev = c["revision"].as_i64().unwrap_or(0);
        if !KINDS.contains(&kind) || id.is_empty() || (kind == "settings" && id != CHURCH) {
            return Ok(());
        }
        let k = key(kind, id);
        if self.st.docs.get(&k).is_some_and(|e| e.rev >= rev) {
            return Ok(()); // 이미 맞춘 것 (내가 올린 것이 다시 온 것 포함)
        }
        self.report.pulled += 1;
        let remote = if c["op"] == "delete" { None } else { Some(c["payload"].clone()) };
        self.settle(kind, id, rev, remote, false);
        Ok(())
    }

    /// 클라우드 판(rev, None 이면 지움)을 설치판에 맞춘다. 설치판에서도 고쳤으면 사본을 남긴다
    fn settle(&mut self, kind: &str, id: &str, rev: i64, remote: Option<Value>, force_changed: bool) {
        let k = key(kind, id);
        let e = self.st.docs.get(&k).cloned();
        let l0 = local_one(&self.core.lock(), kind, id);
        if let (Some(r), Some(l)) = (&remote, &l0) {
            // 같은 내용이면 판 번호만 맞춘다 (다른 PC 가 같은 자료를 먼저 올린 경우 · 처음 맞추기)
            let (lc, _) = self.to_cloud(l, false);
            if lc == *r {
                self.st.docs.insert(k, Entry { rev, hash: digest(l), ..Default::default() });
                return;
            }
        }
        let rl = remote.as_ref().map(|r| self.from_cloud(r));
        let dir = self.dir.clone();
        let mut core = self.core.lock();
        let l = local_one(&core, kind, id);
        let changed = force_changed
            || l.as_ref().map(digest) != l0.as_ref().map(digest)
            || match (&e, &l) {
                (Some(e), Some(l)) => e.deleted || digest(l) != e.hash,
                (Some(e), None) => !e.deleted,
                (None, Some(_)) => true,
                (None, None) => false,
            };
        let name = |v: &Value| v["name"].as_str().or(v["title"].as_str()).unwrap_or(id).to_string();
        match rl {
            Some(r) => {
                if changed {
                    if let Some(l) = &l {
                        match make_copy(&mut core, kind, l, &dir) {
                            Ok(n) => self.report.conflicts.push(n),
                            Err(err) => {
                                // 사본을 못 만들면 덮어쓰지 않는다 (다음에 다시)
                                self.report.errors.push(format!("{k}: {err}"));
                                return;
                            }
                        }
                    }
                }
                match apply_put(&mut core, kind, id, &r) {
                    Ok(()) => {
                        let now = local_one(&core, kind, id);
                        self.st.docs.insert(k, Entry { rev, hash: now.as_ref().map(digest).unwrap_or_default(), ..Default::default() });
                    }
                    Err(err) => {
                        self.report.errors.push(format!("{k}: {err}"));
                        self.st.docs.insert(k, Entry { rev, skip: l.is_none(), hash: l.as_ref().map(digest).unwrap_or_default(), ..Default::default() });
                    }
                }
            }
            None => {
                let Some(lv) = &l else {
                    self.st.docs.insert(k, Entry { rev, deleted: true, ..Default::default() });
                    return;
                };
                if changed {
                    // 이 PC 에서 고치던 것 → 지우지 않는다 (다음 올리기에서 되살림)
                    self.report.kept.push(name(lv));
                    self.st.docs.insert(k, Entry { rev, deleted: true, ..Default::default() });
                    return;
                }
                match apply_delete(&mut core, kind, id, &dir) {
                    Ok(true) => {
                        self.st.docs.insert(k, Entry { rev, deleted: true, ..Default::default() });
                    }
                    Ok(false) => {
                        self.report.kept.push(name(lv));
                        self.st.docs.insert(k, Entry { rev, deleted: true, ..Default::default() });
                    }
                    Err(err) => self.report.errors.push(format!("{k}: {err}")),
                }
            }
        }
    }

    fn push_local(&mut self) -> Result<(), ApiErr> {
        let (docs, sig) = {
            let c = self.core.lock();
            (local_docs(&c), local_sig(&c))
        };
        let retry_partial = self.assets_off.is_none() && self.st.docs.values().any(|e| e.partial > 0 && now_ms() - e.partial > PARTIAL_RETRY_MS);
        if sig == self.last_sig && self.st.seeded && !retry_partial && self.report.pulled == 0 {
            return Ok(());
        }
        let mut todo: Vec<(String, String, String, i64, Option<Value>)> = vec![]; // (kind, id, op, base, local)
        for (k, l) in &docs {
            let (kind, id) = k.split_once('/').unwrap();
            let h = digest(l);
            match self.st.docs.get(k) {
                None => todo.push((kind.into(), id.into(), "put".into(), 0, Some(l.clone()))),
                Some(e) if e.skip => {}
                Some(e) if e.deleted => todo.push((kind.into(), id.into(), "put".into(), e.rev, Some(l.clone()))),
                Some(e) if e.hash != h && e.failed != h => todo.push((kind.into(), id.into(), "put".into(), e.rev, Some(l.clone()))),
                Some(e) if e.hash == h && retry_partial && e.partial > 0 && now_ms() - e.partial > PARTIAL_RETRY_MS => todo.push((kind.into(), id.into(), "put".into(), e.rev, Some(l.clone()))),
                _ => {}
            }
        }
        for (k, e) in &self.st.docs {
            if !e.deleted && !e.skip && !docs.contains_key(k) {
                let (kind, id) = k.split_once('/').unwrap();
                if kind != "settings" {
                    todo.push((kind.into(), id.into(), "delete".into(), e.rev, None));
                }
            }
        }
        // 설정 · 템플릿 · 테마 먼저 (큐시트가 그것을 가리킴), 지우기는 마지막
        let order = |k: &str, op: &str| (op == "delete") as u8 * 10 + KINDS.iter().position(|x| *x == k).map(|i| [4u8, 3, 1, 2, 0][i]).unwrap_or(9);
        todo.sort_by_key(|x| order(&x.0, &x.2));
        let total = todo.len();
        let mut done = 0;
        let mut batch: Vec<Mutation> = vec![];
        let mut bytes = 0usize;
        for (kind, id, op, base, l) in todo {
            let k = key(&kind, &id);
            let (payload, partial, hash) = match &l {
                Some(l) => {
                    let (p, partial) = self.to_cloud(l, true);
                    let size = p.to_string().len();
                    if size > DOC_MAX {
                        let h = digest(l);
                        self.report.errors.push(format!("{k}: {}", t("너무 커서 올리지 않았습니다 (1 MiB 넘음).", "too large to upload (over 1 MiB).")));
                        let e = self.st.docs.entry(k).or_default();
                        e.failed = h;
                        continue;
                    }
                    (Some(p), partial, digest(l))
                }
                None => (None, false, String::new()),
            };
            let size = payload.as_ref().map(|p| p.to_string().len()).unwrap_or(0) + 200;
            if !batch.is_empty() && (batch.len() >= BATCH || bytes + size > BATCH_BYTES) {
                done += batch.len();
                self.send_batch(std::mem::take(&mut batch))?;
                bytes = 0;
                self.progress(done, total);
            }
            bytes += size;
            let mutation_id = self.st.next_id();
            batch.push(Mutation { mutation_id, kind, id, op, base, payload, hash, partial });
        }
        if !batch.is_empty() {
            self.send_batch(batch)?;
        }
        self.st.seeded = true;
        self.last_sig = {
            let c = self.core.lock();
            local_sig(&c)
        };
        self.save();
        Ok(())
    }

    fn progress(&self, done: usize, total: usize) {
        if total > BATCH {
            STATUS.lock().progress = format!("{done}/{total}");
        }
    }

    fn send_batch(&mut self, batch: Vec<Mutation>) -> Result<(), ApiErr> {
        // 보내기 전에 저장 (답을 못 받으면 다음에 같은 mutationId 로)
        self.st.outbox = batch.clone();
        self.save();
        let wire: Vec<Value> = batch.iter().map(Mutation::wire).collect();
        let r = self.api.push(&wire)?;
        self.results(&batch, &r)?;
        self.st.outbox.clear();
        self.save();
        Ok(())
    }

    fn results(&mut self, batch: &[Mutation], r: &Value) -> Result<(), ApiErr> {
        if r["ok"] != true {
            return Err(ApiErr::Http(200, r["error"].as_str().unwrap_or("").into()));
        }
        let res = r["results"].as_array().cloned().unwrap_or_default();
        for (i, m) in batch.iter().enumerate() {
            let x = res.get(i).filter(|x| x["mutationId"] == m.mutation_id.as_str()).cloned().or_else(|| res.iter().find(|x| x["mutationId"] == m.mutation_id.as_str()).cloned()).unwrap_or(Value::Null);
            let k = key(&m.kind, &m.id);
            match x["status"].as_str().unwrap_or("") {
                "applied" | "duplicate" => {
                    self.report.pushed += 1;
                    let rev = x["revision"].as_i64().unwrap_or(m.base + 1);
                    let partial = if m.partial { now_ms() } else { 0 };
                    self.st.docs.insert(k, Entry { rev, hash: m.hash.clone(), deleted: m.op == "delete", partial, ..Default::default() });
                }
                "conflict" => {
                    let cur = &x["current"];
                    if cur.is_null() {
                        self.st.docs.remove(&k); // 클라우드에 없음 → 다음에 새로 만들기
                        continue;
                    }
                    let rev = cur["revision"].as_i64().unwrap_or(0);
                    if cur["deleted"] == true {
                        if m.op == "delete" {
                            self.st.docs.insert(k, Entry { rev, deleted: true, ..Default::default() });
                        } else {
                            // 클라우드에서 지웠지만 이 PC 에서 고침 → 지우지 않고 다음에 되살린다
                            self.report.kept.push(m.payload.as_ref().and_then(|p| p["name"].as_str().or(p["title"].as_str())).unwrap_or(&m.id).to_string());
                            self.st.docs.insert(k, Entry { rev, deleted: true, ..Default::default() });
                        }
                        continue;
                    }
                    self.report.pulled += 1;
                    self.settle(&m.kind, &m.id, rev, Some(cur["payload"].clone()), true);
                }
                "missing" => {
                    self.st.docs.remove(&k);
                }
                _ => {
                    let err = x["error"].as_str().unwrap_or("답 없음").to_string();
                    self.report.errors.push(format!("{k}: {err}"));
                    let e = self.st.docs.entry(k).or_default();
                    e.failed = m.hash.clone();
                }
            }
        }
        Ok(())
    }

    fn assets_fail(&mut self, e: ApiErr) {
        let why = match e {
            ApiErr::Missing => "missing",
            ApiErr::Quota => "quota",
            ApiErr::Auth(_) => "auth",
            _ => "error",
        };
        let pause = match why {
            "quota" => 6 * 3600, // 그 예배 동안은 중계로 (assets-api.md 5절)
            "error" => 300,
            _ => 1800,
        };
        log::info!("클라우드 그림·영상 올리기 멈춤 ({why}, {pause}초): {e}");
        self.assets_off = Some(why.into());
        self.assets_off_at = Some(Instant::now() + Duration::from_secs(pause));
        STATUS.lock().assets = why.into();
    }

    /// 파일 하나 → 자산 id (upload=false 면 묻기만)
    fn asset_of(&mut self, orig: &str, p: &Path, upload: bool) -> Option<(String, String)> {
        let meta = std::fs::metadata(p).ok().filter(|m| m.is_file())?;
        let (size, mt) = (meta.len(), mtime_ms(&meta));
        let ps = p.to_string_lossy().to_string();
        let ext = ext_of(p);
        if let Some(f) = self.st.files.get(&ps).filter(|f| f.size == size && f.mtime == mt) {
            if !f.asset.is_empty() {
                return Some((f.asset.clone(), ext));
            }
            if f.bad {
                return None;
            }
        }
        if size > ASSET_MAX || !upload_ok(&ext, size) {
            return None;
        }
        let sha = match self.st.files.get(&ps).filter(|f| f.size == size && f.mtime == mt && !f.sha.is_empty()) {
            Some(f) => f.sha.clone(),
            None => sha256_file(p)?,
        };
        let mut id = self.st.assets.get(&sha).cloned().unwrap_or_default();
        if id.is_empty() {
            if self.assets_off.is_some() {
                self.st.files.insert(ps, FileRec { size, mtime: mt, sha, asset: String::new(), bad: false });
                return None;
            }
            match self.api.asset_find(&sha) {
                Ok(Some(x)) => id = x,
                Ok(None) if upload => {
                    let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| format!("file{ext}"));
                    match self.api.asset_put(p, &sha, &name, ctype_of(&ext), size) {
                        Ok(x) => {
                            self.report.uploaded += 1;
                            STATUS.lock().uploaded += 1;
                            id = x;
                        }
                        Err(ApiErr::Http(code, why)) => {
                            // 이 파일만 (종류 · 크기 · sha256 다름) — 다른 파일은 계속 올린다
                            log::info!("클라우드가 파일을 받지 않음 ({code} {why}): {}", p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default());
                            self.st.files.insert(ps, FileRec { size, mtime: mt, sha, asset: String::new(), bad: true });
                            return None;
                        }
                        Err(e) => {
                            self.assets_fail(e);
                            return None;
                        }
                    }
                }
                Ok(None) => {
                    self.st.files.insert(ps, FileRec { size, mtime: mt, sha, asset: String::new(), bad: false });
                    return None;
                }
                Err(e) => {
                    self.assets_fail(e);
                    return None;
                }
            }
        }
        STATUS.lock().assets.clear();
        self.st.assets.insert(sha.clone(), id.clone());
        self.st.files.insert(ps, FileRec { size, mtime: mt, sha, asset: id.clone(), bad: false });
        self.st.origins.entry(id.clone()).or_insert_with(|| orig.trim().to_string());
        Some((id, ext))
    }

    fn cloud_str(&mut self, s: &str, upload: bool, partial: &mut bool) -> Option<String> {
        let p = local_file(&self.media_dir, s)?;
        match self.asset_of(s, &p, upload) {
            Some((id, ext)) => Some(asset_url(&id, &ext)),
            None => {
                *partial = true;
                Some(self.st.token(s.trim()))
            }
        }
    }

    /// 설치판 본문 → 클라우드 본문 (파일 주소를 자산 주소 · 가림표로). (본문, 가림표가 있나)
    pub fn to_cloud(&mut self, v: &Value, upload: bool) -> (Value, bool) {
        let mut out = v.clone();
        let mut partial = false;
        let mut strs: Vec<String> = vec![];
        collect_strings(&out, &mut strs);
        let mut map: HashMap<String, Option<String>> = HashMap::new();
        for s in strs {
            if map.contains_key(&s) {
                continue;
            }
            let n = if local_file(&self.media_dir, &s).is_some() {
                self.cloud_str(&s, upload, &mut partial)
            } else if s.contains("/lib/file?p=") {
                let mut parts: Vec<(String, Option<String>)> = vec![];
                let _ = replace_embedded(&s, "/lib/file?p=", &mut |u| {
                    parts.push((u.to_string(), None));
                    None
                });
                for pt in &mut parts {
                    pt.1 = self.cloud_str(&pt.0, upload, &mut partial);
                }
                replace_embedded(&s, "/lib/file?p=", &mut |u| parts.iter().find(|x| x.0 == u).and_then(|x| x.1.clone()))
            } else {
                None
            };
            map.insert(s, n);
        }
        map_strings(&mut out, &mut |s| map.get(s).cloned().flatten());
        (out, partial)
    }

    /// 클라우드 본문 → 설치판 본문 (자산 주소 · 가림표를 이 PC 의 원래 주소로, 모르는 자산은 받아서 media 폴더에)
    pub fn from_cloud(&mut self, v: &Value) -> Value {
        let mut out = v.clone();
        let mut strs: Vec<String> = vec![];
        collect_strings(&out, &mut strs);
        let mut map: HashMap<String, Option<String>> = HashMap::new();
        for s in strs {
            if map.contains_key(&s) {
                continue;
            }
            let n = if let Some(tok) = s.trim().strip_prefix("wp-local:") {
                self.st.tokens.get(tok).cloned()
            } else if let Some((id, ext)) = parse_asset(&s) {
                self.local_of_asset(&id, &ext)
            } else if s.contains("/api/v1/assets/") {
                let mut parts: Vec<(String, Option<String>)> = vec![];
                let _ = replace_embedded(&s, "/api/v1/assets/", &mut |u| {
                    parts.push((u.to_string(), None));
                    None
                });
                for pt in &mut parts {
                    pt.1 = parse_asset(&pt.0).and_then(|(id, ext)| self.local_of_asset(&id, &ext));
                }
                replace_embedded(&s, "/api/v1/assets/", &mut |u| parts.iter().find(|x| x.0 == u).and_then(|x| x.1.clone()))
            } else {
                None
            };
            map.insert(s, n);
        }
        map_strings(&mut out, &mut |s| map.get(s).cloned().flatten());
        out
    }

    fn local_of_asset(&mut self, id: &str, ext: &str) -> Option<String> {
        if let Some(o) = self.st.origins.get(id) {
            // 원래 파일이 아직 있으면 그 주소, 없어졌으면 받아 둔다
            if local_file(&self.media_dir, o).is_some_and(|p| p.is_file()) {
                return Some(o.clone());
            }
        }
        let (bytes, ct) = match self.api.asset_get(id) {
            Ok(x) => x,
            Err(e) => {
                log::info!("클라우드 파일을 받지 못함 {id}: {e}");
                return None;
            }
        };
        let sha = hex(&Sha256::digest(&bytes));
        let ext = if ext.is_empty() { ext_of_ctype(&ct).to_string() } else { ext.to_string() };
        let name = format!("cloud-{}{ext}", &sha[..16]);
        let f = self.media_dir.join(&name);
        if !f.is_file() {
            let tmp = self.media_dir.join(format!("{name}.part"));
            if std::fs::write(&tmp, &bytes).and_then(|_| std::fs::rename(&tmp, &f)).is_err() {
                return None;
            }
        }
        let local = format!("/media/{name}");
        if let Ok(m) = std::fs::metadata(&f) {
            self.st.files.insert(f.to_string_lossy().to_string(), FileRec { size: m.len(), mtime: mtime_ms(&m), sha: sha.clone(), asset: id.to_string(), bad: false });
        }
        self.st.assets.insert(sha, id.to_string());
        self.st.origins.insert(id.to_string(), local.clone());
        Some(local)
    }

    /// 출력 한 겹이 쓰는 표를 채운다
    pub fn publish_out(&self) {
        let mut o = OUT.lock();
        o.on = true;
        o.media_dir = self.media_dir.clone();
        o.known = self.st.files.iter().filter(|(_, f)| !f.asset.is_empty()).map(|(p, f)| (p.clone(), (f.size, f.mtime, f.asset.clone()))).collect();
    }

    /// 출력 화면에 쓰인 파일 중 아직 안 올린 것을 올린다 → 올린 것이 있으면 true (다시 보내게)
    pub fn serve_output_wants(&mut self) -> bool {
        let want: Vec<(String, String)> = std::mem::take(&mut OUT.lock().want);
        if want.is_empty() || self.assets_off.is_some() {
            return false;
        }
        let mut any = false;
        for (p, orig) in want {
            if self.asset_of(&orig, Path::new(&p), true).is_some() {
                any = true;
            }
        }
        if any {
            self.save();
            self.publish_out();
        }
        any
    }
}

fn collect_strings(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::String(s) => out.push(s.clone()),
        Value::Array(a) => a.iter().for_each(|x| collect_strings(x, out)),
        Value::Object(o) => o.values().for_each(|x| collect_strings(x, out)),
        _ => {}
    }
}

/// 동기화를 끄면 출력 한 겹도 끈다 (1단계와 똑같이)
fn out_off() {
    let mut o = OUT.lock();
    o.on = false;
    o.known.clear();
    o.want.clear();
}

// ---------- 스레드 ----------
/// 조작 서버를 띄운 뒤 한 번 (server.rs, cloud::init 옆). 꺼져 있으면 설정만 1초마다 본다
pub fn init(core: Shared) {
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let _ = std::thread::Builder::new().name("wp-cloudsync".into()).spawn(move || run(core));
}

/// 한 바퀴 뒤 알림 한 줄
fn tell(core: &Shared, r: &Report) {
    if !r.conflicts.is_empty() {
        let names = r.conflicts.iter().take(3).cloned().collect::<Vec<_>>().join(", ");
        let msg = if crate::i18n::is_en() {
            format!("Cloud sync: {} item(s) were changed both here and in the cloud — kept this PC's version as a conflict copy: {names}", r.conflicts.len())
        } else {
            format!("클라우드 동기화: 이 PC 와 클라우드에서 함께 고친 {}개를 '충돌 사본' 으로 남겼습니다 — {names}", r.conflicts.len())
        };
        core.lock().notify(msg, "warn");
    } else if !r.kept.is_empty() {
        let names = r.kept.iter().take(3).cloned().collect::<Vec<_>>().join(", ");
        core.lock().notify(t(&format!("클라우드에서 지운 것 중 이 PC 에서 고치던 · 쓰는 중인 것은 지우지 않았습니다: {names}"), &format!("Kept items deleted in the cloud that are being edited or used here: {names}")), "info");
    }
    if !r.errors.is_empty() {
        log::warn!("클라우드 동기화 문제 {}개: {}", r.errors.len(), r.errors.iter().take(5).cloned().collect::<Vec<_>>().join(" / "));
    }
}

fn run(core: Shared) {
    let mut rx = core.lock().tx.subscribe();
    loop {
        let cf = sync_conf(&core.lock());
        if !cf.on {
            out_off();
            set_state("off", "");
            std::thread::sleep(Duration::from_secs(1));
            continue;
        }
        let api = Http::new(&cf.url, &cf.key);
        let mut eng = Engine::new(&core, &api, &cf.church, &cf.url);
        eng.publish_out();
        let mut next = Instant::now();
        let mut backoff = 5u64;
        let mut dirty_at: Option<Instant> = None;
        let mut sig_at = Instant::now();
        let mut sig = String::new();
        let mut stuck = false;
        loop {
            std::thread::sleep(Duration::from_millis(500));
            if sync_conf(&core.lock()) != cf {
                break;
            }
            // 화면에서 고친 것 (Ev::Ui 는 자주 오므로 서명이 바뀐 때만, 2초에 한 번)
            let mut ui = false;
            while let Ok(e) = rx.try_recv() {
                ui |= matches!(e, Ev::Ui);
            }
            if ui && dirty_at.is_none() && sig_at.elapsed() > Duration::from_secs(2) {
                sig_at = Instant::now();
                let s = local_sig(&core.lock());
                if s != sig {
                    sig = s;
                    dirty_at = Some(Instant::now());
                }
            }
            if !stuck && eng.serve_output_wants() {
                core.lock().emit(Ev::Live);
            }
            let due = Instant::now() >= next || dirty_at.is_some_and(|d| d.elapsed() > Duration::from_secs(3));
            if stuck || !due {
                continue;
            }
            dirty_at = None;
            set_state("syncing", "");
            match eng.tick() {
                Ok(()) => {
                    backoff = 5;
                    next = Instant::now() + Duration::from_secs(30);
                    {
                        let mut s = STATUS.lock();
                        s.pushed += eng.report.pushed as u64;
                        s.pulled += eng.report.pulled as u64;
                        s.conflicts += eng.report.conflicts.len() as u64;
                        if eng.assets_off.is_none() {
                            s.assets.clear();
                        }
                    }
                    set_state("ok", "");
                    tell(&core, &eng.report);
                    sig = local_sig(&core.lock());
                }
                Err(ApiErr::Auth(e)) => {
                    set_state("error", &t(&format!("클라우드가 연결 열쇠를 받지 않습니다 ({e}). 클라우드 연결 창에서 열쇠를 다시 넣어 주세요."), &format!("The cloud rejected the connection key ({e}). Enter the key again in the Cloud connection window.")));
                    stuck = true; // 설정이 바뀔 때까지 멈춤
                }
                Err(ApiErr::Missing) => {
                    set_state("error", &t("이 클라우드에는 자료 동기화 길이 없습니다 (클라우드를 새 버전으로).", "This cloud has no data sync API yet (update the cloud)."));
                    next = Instant::now() + Duration::from_secs(600);
                }
                Err(e) => {
                    set_state("offline", &t(&format!("클라우드와 맞추지 못했습니다 — 뒤에서 다시 해 봅니다 ({e})"), &format!("Could not sync with the cloud — retrying in the background ({e})")));
                    next = Instant::now() + Duration::from_secs(backoff + rand::random::<u64>() % 5);
                    backoff = (backoff * 2).min(300);
                    tell(&core, &eng.report);
                }
            }
        }
    }
}
