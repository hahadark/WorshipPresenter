//! [트랙 A] 온라인에서 받은 찬양 PPT 자동 추가 (Electron 판 songinbox.js)
//! '온라인에서 찾기'를 누르면 정해진 시간 동안 다운로드 폴더를 지켜보고, 새 PPT 가 다 받아지면
//! 찬양 PPT 폴더로 옮기고(폴더는 자동 등록) 큐시트에 찬송가(PPT) 순서로 넣는다.

use crate::core::{Core, Shared};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use regex::Regex;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const WATCH: Duration = Duration::from_secs(20 * 60);

static PPT: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\.(pps|ppt|ppsx|pptx)$").unwrap());
static PARTIAL: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\.(crdownload|part|tmp|download|partial)$").unwrap());

#[derive(Default)]
struct State {
    until: Option<Instant>,
    running: bool,
    seen: HashMap<String, u128>,
    pending: HashMap<String, u64>,
}

static ST: Lazy<Mutex<State>> = Lazy::new(|| Mutex::new(State::default()));

pub fn downloads_dir(core: &Core) -> PathBuf {
    let s = core.settings.str(&["songDownloadDir"]);
    if !s.is_empty() {
        return PathBuf::from(s);
    }
    dirs::download_dir().unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join("Downloads"))
}

pub fn target_dir(core: &Core) -> PathBuf {
    let s = core.settings.str(&["songInboxDir"]);
    if !s.is_empty() {
        return PathBuf::from(s);
    }
    dirs::document_dir().unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join("Documents")).join("WorshipCue 찬양 PPT")
}

pub fn status(core: &Core) -> Value {
    let st = ST.lock();
    let left = st.until.map(|u| u.saturating_duration_since(Instant::now()).as_secs()).unwrap_or(0);
    json!({ "watching": left > 0, "secondsLeft": left, "downloads": downloads_dir(core).to_string_lossy(), "target": target_dir(core).to_string_lossy() })
}

fn list(dir: &Path) -> Vec<(String, u128, u64)> {
    let Ok(rd) = std::fs::read_dir(dir) else { return vec![] };
    rd.flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if !PPT.is_match(&name) || PARTIAL.is_match(&name) || name.starts_with("~$") {
                return None;
            }
            let m = e.metadata().ok().filter(|m| m.is_file())?;
            let mt = m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_millis();
            Some((name, mt, m.len()))
        })
        .collect()
}

/// 검색 주소 틀의 {q} 에 검색어 (http/https 만)
pub fn online_search_url(template: &str, query: &str) -> Result<String, String> {
    let t = template.trim();
    if !(t.starts_with("http://") || t.starts_with("https://")) || t.contains(char::is_whitespace) {
        return Err("온라인 검색 주소가 올바르지 않습니다 (http:// 또는 https:// 로 시작).".into());
    }
    let q: String = query.trim().bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect();
    Ok(if t.contains("{q}") { t.replacen("{q}", &q, 1) } else { t.to_string() })
}

/// 지켜보기 시작 (이미 보고 있으면 시간만 늘린다)
pub fn arm(shared: &Shared) -> Result<Value, String> {
    let dir = downloads_dir(&shared.lock());
    if !dir.is_dir() {
        return Err(format!("다운로드 폴더를 찾을 수 없습니다: {}", dir.display()));
    }
    let start = {
        let mut st = ST.lock();
        st.until = Some(Instant::now() + WATCH);
        if st.running {
            false
        } else {
            st.running = true;
            st.seen = list(&dir).into_iter().map(|(n, m, _)| (n, m)).collect();
            st.pending.clear();
            true
        }
    };
    if start {
        let weak = std::sync::Arc::downgrade(shared);
        crate::hymns::spawn_big(move || loop {
            std::thread::sleep(Duration::from_millis(1500));
            let Some(sh) = weak.upgrade() else { return };
            if !tick(&sh) {
                ST.lock().running = false;
                sh.lock().emit(crate::core::Ev::Ui);
                return;
            }
        });
    }
    let c = shared.lock();
    c.emit(crate::core::Ev::Ui);
    Ok(status(&c))
}

pub fn disarm() {
    ST.lock().until = None;
}

/// 한 번 살펴보기 → 계속 지켜볼지
fn tick(shared: &Shared) -> bool {
    if ST.lock().until.is_none_or(|u| Instant::now() > u) {
        return false;
    }
    let dir = downloads_dir(&shared.lock());
    for (name, mt, size) in list(&dir) {
        let ready = {
            let mut st = ST.lock();
            if st.seen.get(&name) == Some(&mt) {
                continue;
            }
            // 크기가 두 번 연속 같으면 다 받은 것
            match st.pending.get(&name) {
                Some(&last) if last == size && size > 0 => {
                    st.pending.remove(&name);
                    st.seen.insert(name.clone(), mt);
                    true
                }
                _ => {
                    st.pending.insert(name.clone(), size);
                    false
                }
            }
        };
        if ready {
            let mut c = shared.lock();
            match take(&mut c, &dir.join(&name)) {
                Ok(dest) => {
                    let h = crate::hymns::HymnFile { number: crate::hymns::parse_name(&dest).0, title: crate::hymns::parse_name(&dest).1, name: String::new(), folder: String::new(), file: dest.clone() };
                    c.add_item(crate::hymns::new_hymn_item(&h), None);
                    c.notify(format!("받은 찬양 PPT '{}' 를 큐시트에 넣었습니다.", h.title), "ok");
                }
                Err(e) => c.notify(format!("{name}: {e}"), "error"),
            }
        }
    }
    true
}

/// 받은 파일을 찬양 PPT 폴더로 옮긴다 (폴더가 등록돼 있지 않으면 등록)
pub fn take(core: &mut Core, src: &Path) -> Result<PathBuf, String> {
    let target = target_dir(core);
    std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
    let ext = src.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let stem = src.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let base = Regex::new(r"\s*\(\d+\)$").unwrap().replace(&stem, "").to_string();
    let mut dest = target.join(format!("{base}{ext}"));
    let mut k = 2;
    while dest.exists() {
        dest = target.join(format!("{base} ({k}){ext}"));
        k += 1;
    }
    if std::fs::rename(src, &dest).is_err() {
        std::fs::copy(src, &dest).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(src);
    }
    let ts = target.to_string_lossy().to_string();
    let registered = core.settings.get()["hymnFolders"].as_array().into_iter().flatten().any(|f| f["path"].as_str() == Some(ts.as_str()));
    if !registered {
        let t2 = ts.clone();
        core.settings.update(|d| {
            let mut l: Vec<Value> = d["hymnFolders"].as_array().cloned().unwrap_or_default();
            l.push(json!({ "name": "찬양 PPT (다운로드)", "path": t2, "render": "slides" }));
            d["hymnFolders"] = json!(l);
        });
    }
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn url() {
        assert_eq!(online_search_url("https://x.com/search/{q}", "주 사랑").unwrap(), "https://x.com/search/%EC%A3%BC%20%EC%82%AC%EB%9E%91");
        assert!(online_search_url("ftp://x", "a").is_err());
    }
}

#[cfg(test)]
mod real_watch_tests {
    //! 실제 폴더로 지켜보기 시험: cargo test -p wp-core real_inbox -- --ignored --nocapture
    use super::*;
    use std::sync::Arc;

    #[test]
    #[ignore]
    fn real_inbox() {
        let root = std::env::temp_dir().join(format!("wp-inbox-{}", std::process::id()));
        let (data, dl, inbox) = (root.join("data"), root.join("Downloads"), root.join("찬양 PPT"));
        for d in [&data, &dl] { std::fs::create_dir_all(d).unwrap(); }
        std::fs::write(data.join("bible.db"), b"").unwrap(); // 빈 DB: 기존 자료를 복사해 오지 않게
        let core = Core::open(data.clone(), root.join("web"), None).unwrap();
        let shared: Shared = Arc::new(parking_lot::Mutex::new(core));
        {
            let mut c = shared.lock();
            let (a, b) = (dl.to_string_lossy().to_string(), inbox.to_string_lossy().to_string());
            c.settings.update(|d| { d["songDownloadDir"] = json!(a); d["songInboxDir"] = json!(b); });
            c.new_cue("시험").unwrap();
        }
        // 지켜보기 전부터 있던 파일은 건드리지 않는다
        std::fs::write(dl.join("예전 곡.pptx"), b"old").unwrap();
        let st = arm(&shared).unwrap();
        println!("지켜보기: {st}");
        // 브라우저처럼: 받는 중(.crdownload) → 이름 바꿈
        let src = std::env::var("WP_INBOX_PPTX").unwrap_or_else(|_| r"F:\aside\WorshipCue\tests\fixtures\chart-table.pptx".into());
        std::fs::copy(&src, dl.join("주 사랑 (1).pptx.crdownload")).unwrap();
        std::thread::sleep(Duration::from_millis(2000));
        std::fs::rename(dl.join("주 사랑 (1).pptx.crdownload"), dl.join("주 사랑 (1).pptx")).unwrap();
        let t0 = Instant::now();
        let mut done = false;
        while t0.elapsed() < Duration::from_secs(15) {
            std::thread::sleep(Duration::from_millis(500));
            let c = shared.lock();
            if c.cue.as_ref().is_some_and(|q| !q.items.is_empty()) { done = true; break; }
        }
        disarm();
        let c = shared.lock();
        let items = c.cue.as_ref().map(|q| q.items.clone()).unwrap_or_default();
        println!("{:.1}초 뒤 큐시트 순서: {}", t0.elapsed().as_secs_f32(), serde_json::to_string(&items).unwrap());
        assert!(done, "받은 파일이 큐시트에 들어가지 않음");
        assert!(inbox.join("주 사랑.pptx").is_file(), "찬양 PPT 폴더로 옮겨지지 않음");
        assert!(!dl.join("주 사랑 (1).pptx").exists());
        assert!(dl.join("예전 곡.pptx").exists(), "예전 파일을 옮김");
        assert_eq!(items[0]["type"], "hymn");
        assert!(c.settings.get()["hymnFolders"].as_array().unwrap().iter().any(|f| f["path"].as_str() == Some(&*inbox.to_string_lossy())));
        drop(c);
        let _ = std::fs::remove_dir_all(&root);
    }
}
