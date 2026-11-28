//! [웹판] 브라우저 안 작은 파일 저장소. std::fs 와 같은 이름의 함수 몇 개만 흉내 낸다.
//!
//! 브라우저에서 std::fs 는 컴파일은 되지만 부르면 모두 '지원하지 않음' 오류다. 파일을 꼭 써야 하는 모듈은
//! 맨 위에서 `use std::fs;` 대신 `use crate::web::vfs as fs;` 를 웹일 때만 고르면(조건부 컴파일) 코드를 고치지 않고 돈다.
//! 지금 이 길을 쓰는 곳: settings.rs (settings.json).
//!
//! 저장: 메모리에 두고, 글 파일(.json)은 브라우저 localStorage 에도 쓴다(열쇠 `wpfs:<경로>`). 다시 열면 거기서 읽는다.
//! 큰 파일(그림·영상)은 여기 두지 않는다 — 저장 층(IndexedDB · 클라우드)의 몫이다. docs/web/egui-web.md
use parking_lot::Mutex;
use std::collections::HashMap;
use std::io;
use std::path::Path;

static FILES: once_cell::sync::Lazy<Mutex<HashMap<String, Vec<u8>>>> = once_cell::sync::Lazy::new(|| Mutex::new(HashMap::new()));
const PREFIX: &str = "wpfs:";

fn key(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

fn not_found(p: &Path) -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, format!("없는 파일: {}", p.display()))
}

/// localStorage 에 두는 파일인가 (작은 글 파일만)
fn persisted(k: &str) -> bool {
    k.ends_with(".json")
}

pub fn exists(p: impl AsRef<Path>) -> bool {
    let k = key(p.as_ref());
    if FILES.lock().contains_key(&k) {
        return true;
    }
    persisted(&k) && storage().and_then(|s| s.get_item(&format!("{PREFIX}{k}")).ok().flatten()).is_some()
}

pub fn read(p: impl AsRef<Path>) -> io::Result<Vec<u8>> {
    let p = p.as_ref();
    let k = key(p);
    if let Some(b) = FILES.lock().get(&k) {
        return Ok(b.clone());
    }
    if persisted(&k) {
        if let Some(t) = storage().and_then(|s| s.get_item(&format!("{PREFIX}{k}")).ok().flatten()) {
            FILES.lock().insert(k, t.clone().into_bytes());
            return Ok(t.into_bytes());
        }
    }
    Err(not_found(p))
}

pub fn read_to_string(p: impl AsRef<Path>) -> io::Result<String> {
    String::from_utf8(read(p)?).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

pub fn write(p: impl AsRef<Path>, data: impl AsRef<[u8]>) -> io::Result<()> {
    let k = key(p.as_ref());
    let data = data.as_ref();
    if persisted(&k) {
        // 임시 파일(.json.tmp)은 persisted 가 아니라 메모리에만 — rename 때 진짜 이름으로 한 번만 쓴다
        let txt = std::str::from_utf8(data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let st = storage().ok_or_else(|| io::Error::new(io::ErrorKind::Unsupported, "브라우저 저장소를 쓸 수 없습니다"))?;
        st.set_item(&format!("{PREFIX}{k}"), txt).map_err(|_| io::Error::new(io::ErrorKind::StorageFull, "브라우저 저장 공간이 가득 찼거나 막혀 있습니다"))?;
    }
    FILES.lock().insert(k, data.to_vec());
    Ok(())
}

pub fn rename(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()> {
    let b = FILES.lock().remove(&key(from.as_ref())).ok_or_else(|| not_found(from.as_ref()))?;
    write(to, b)
}

pub fn copy(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<u64> {
    let b = read(from)?;
    let n = b.len() as u64;
    write(to, b)?;
    Ok(n)
}

pub fn remove_file(p: impl AsRef<Path>) -> io::Result<()> {
    let k = key(p.as_ref());
    FILES.lock().remove(&k);
    if persisted(&k) {
        if let Some(s) = storage() {
            let _ = s.remove_item(&format!("{PREFIX}{k}"));
        }
    }
    Ok(())
}

/// 폴더는 없다 (경로가 곧 열쇠)
pub fn create_dir_all(_p: impl AsRef<Path>) -> io::Result<()> {
    Ok(())
}
