//! 찬송가·교독문·찬양 PPT 폴더 (.pps .ppt .pptx .ppsx) — Electron 판 hymns.js
//! - 폴더를 등록해 두면 번호·제목으로 찾아 큐시트에 넣는다
//! - 처음 쓸 때 PowerPoint 로 슬라이드를 PNG 로 내보내 데이터 폴더의 hymn-cache 에 둔다 (다음부터 바로)
//! - Electron 판이 이미 만들어 둔 캐시(%APPDATA%\WorshipCue\data\hymn-cache)가 있으면 그것을 읽기만 해서 다시 쓴다
//! - PowerPoint 없이 .pptx 를 직접 그리는 것(pptxbuild)은 아직 (다음 작업)

use crate::core::Core;
use crate::slides::{self, Chunk, SplitOpts};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use regex::Regex;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

static EXT: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\.(pps|ppt|ppsx|pptx)$").unwrap());
static NUM_NAME: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(\d{1,4})\s*[._-]?\s*(.+)$").unwrap());
static READING: Lazy<Regex> = Lazy::new(|| Regex::new(r"^(.+?)\s*(\d{1,4})\s*\((.+)\)$").unwrap());

#[derive(Debug, Clone, PartialEq)]
pub struct HymnFile {
    pub file: PathBuf,
    pub number: Option<i64>,
    pub title: String,
    pub name: String,
    pub folder: String,
}

/// "010 전능왕 오셔서.pps" → (10, "전능왕 오셔서")
pub fn parse_name(file: &Path) -> (Option<i64>, String, String) {
    let fname = file.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let base = EXT.replace(&fname, "").trim().to_string();
    if let Some(m) = NUM_NAME.captures(&base) {
        return (m[1].parse().ok(), m[2].trim().to_string(), base.clone());
    }
    if let Some(m) = READING.captures(&base) {
        return (m[2].parse().ok(), format!("{} {} ({})", m[1].trim(), &m[2], &m[3]), base.clone());
    }
    (None, base.clone(), base)
}

pub fn list_folder(dir: &Path, folder: &str) -> Vec<HymnFile> {
    let Ok(rd) = std::fs::read_dir(dir) else { return vec![] };
    let mut v: Vec<HymnFile> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && EXT.is_match(&p.to_string_lossy()))
        .map(|p| {
            let (number, title, name) = parse_name(&p);
            HymnFile { file: p, number, title, name, folder: folder.to_string() }
        })
        .collect();
    v.sort_by(|a, b| a.number.unwrap_or(1_000_000_000).cmp(&b.number.unwrap_or(1_000_000_000)).then(a.title.cmp(&b.title)));
    v
}

/// 등록한 폴더 [{name, path, render, count, exists}]
pub fn folders(core: &Core) -> Vec<Value> {
    core.settings.get()["hymnFolders"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|f| {
            let p = PathBuf::from(f["path"].as_str().unwrap_or(""));
            let mut o = f.clone();
            if o["render"].is_null() { o["render"] = json!("auto"); }
            o["count"] = json!(list_folder(&p, "").len());
            o["exists"] = json!(p.is_dir());
            o
        })
        .collect()
}

pub fn add_folder(core: &mut Core, dir: &Path, name: Option<&str>) -> Result<Value, String> {
    if !dir.is_dir() {
        return Err(crate::i18n::tr("폴더를 찾을 수 없습니다.").into());
    }
    let n = list_folder(dir, "").len();
    if n == 0 {
        return Err(crate::i18n::tr("이 폴더에 .pps / .ppt / .pptx 파일이 없습니다.").into());
    }
    let label = name.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| dir.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default());
    let ds = dir.to_string_lossy().to_string();
    let l2 = label.clone();
    core.settings.update(|d| {
        let mut list: Vec<Value> = d["hymnFolders"].as_array().cloned().unwrap_or_default();
        list.retain(|f| f["path"].as_str() != Some(&ds));
        list.push(json!({ "name": l2, "path": ds, "render": "auto" }));
        d["hymnFolders"] = json!(list);
    });
    Ok(json!({ "name": label, "path": dir.to_string_lossy(), "count": n }))
}

pub fn remove_folder(core: &mut Core, dir: &str) {
    let dir = dir.to_string();
    core.settings.update(|d| {
        let mut list: Vec<Value> = d["hymnFolders"].as_array().cloned().unwrap_or_default();
        list.retain(|f| f["path"].as_str() != Some(&dir));
        d["hymnFolders"] = json!(list);
    });
}

/// 번호(10, 010) 또는 제목 일부로 찾기. 번호가 정확히 같은 것이 맨 앞
pub fn search(core: &Core, query: &str) -> Vec<HymnFile> {
    let q = query.trim();
    let num: Option<i64> = if !q.is_empty() && q.len() <= 4 && q.chars().all(|c| c.is_ascii_digit()) { q.parse().ok() } else { None };
    let key: String = q.chars().filter(|c| !c.is_whitespace()).collect();
    let mut out = vec![];
    for f in core.settings.get()["hymnFolders"].as_array().into_iter().flatten() {
        let name = f["name"].as_str().unwrap_or("");
        for h in list_folder(Path::new(f["path"].as_str().unwrap_or("")), name) {
            let hit = q.is_empty()
                || match num {
                    Some(n) => h.number == Some(n) || h.number.is_some_and(|x| x.to_string().starts_with(q.trim_start_matches('0')) && !q.trim_start_matches('0').is_empty()),
                    None => h.title.chars().filter(|c| !c.is_whitespace()).collect::<String>().contains(&key),
                };
            if hit {
                out.push(h);
            }
        }
    }
    if let Some(n) = num {
        out.sort_by_key(|h| h.number != Some(n));
    }
    out.truncate(300);
    out
}

fn norm(p: &str) -> String {
    p.replace('/', "\\").to_lowercase()
}

/// 열어도 되는 PPT: 등록한 폴더 · 붙여 넣은 슬라이드 폴더 · 끌어다 놓은 파일
pub fn is_allowed(core: &Core, file: &str) -> bool {
    if !EXT.is_match(file) {
        return false;
    }
    let f = norm(file);
    let s = core.settings.get();
    let under = |d: &str| !d.is_empty() && f.starts_with(&(norm(d).trim_end_matches('\\').to_string() + "\\"));
    if s["hymnFolders"].as_array().into_iter().flatten().any(|x| under(x["path"].as_str().unwrap_or(""))) {
        return true;
    }
    if s["droppedFiles"].as_array().into_iter().flatten().any(|x| x.as_str().map(norm).as_deref() == Some(f.as_str())) {
        return true;
    }
    let pasted = [core.data_dir.join("pasted"), electron_data().map(|d| d.join("pasted")).unwrap_or_default()];
    pasted.iter().any(|d| under(&d.to_string_lossy()))
}

pub fn electron_data() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("WorshipCue").join("data"))
}

/// 캐시 폴더들: 우리 것 먼저, 그다음 Electron 판 (읽기만)
pub fn cache_dirs(core: &Core) -> Vec<PathBuf> {
    let mut v = vec![core.data_dir.join("hymn-cache")];
    if let Some(e) = electron_data() {
        let e = e.join("hymn-cache");
        if e.is_dir() && e != v[0] {
            v.push(e);
        }
    }
    v
}

static META: Lazy<Mutex<HashMap<String, Value>>> = Lazy::new(|| Mutex::new(HashMap::new()));

pub const BUILDING: &str = "PPT 를 그림으로 만드는 중입니다… 끝나면 자동으로 다시 보입니다.";

/// 만드는 중인 파일 (같은 파일을 두 번 만들지 않게)
static PENDING: Lazy<Mutex<std::collections::HashSet<String>>> = Lazy::new(|| Mutex::new(Default::default()));
/// 코어 (백그라운드 일이 끝나면 다시 잡으려고). main 에서 install 로 넣는다
static SHARED: once_cell::sync::OnceCell<std::sync::Weak<parking_lot::Mutex<Core>>> = once_cell::sync::OnceCell::new();

pub fn install(shared: &crate::core::Shared) {
    let _ = SHARED.set(std::sync::Arc::downgrade(shared));
    crate::pptx::build::warm_fonts();
}

/// 그림 만들기 (PowerPoint 또는 직접 읽기) — 코어 잠금 없이 다른 스레드에서
pub fn build_meta(data_dir: &Path, file: &str, render: &str) -> Result<Value, String> {
    let lower = file.to_lowercase();
    let xml = lower.ends_with(".pptx") || lower.ends_with(".ppsx");
    let pp = has_powerpoint();
    if xml && (!pp || render == "direct") {
        return crate::pptx::build::build_cache(data_dir, file);
    }
    if !xml && render != "slides" {
        if let Ok(m) = crate::pps::build_cache(data_dir, file) {
            return Ok(m);
        }
    }
    if pp {
        return export_powerpoint(data_dir, file);
    }
    if xml { crate::pptx::build::build_cache(data_dir, file) } else { crate::pps::build_cache(data_dir, file) }
}

/// 꿝에서 F6/F5 로 넘어갈 순서가 아직 그림을 안 만든 PPT 일 때 기억해 두는 것.
/// Electron `cue.js onCueNext` 는 `previewItem` 이 `loadHymn` 을 기다린 뒤 송출하므로 처음 열는 PPT 라도 못 초 뒤 자동으로 다음 순서가 나간다.
/// Rust 는 만들기를 기다리지 않고(코어 잠금) BUILDING 을 돌려주므로, 요청을 기억해 두고 만들기가 끝나면 이어서 송출한다.
/// 그 사이 다른 송출·넘김이 있었거나(version) 다른 큐시트를 열었으면 버린다 (Electron `if (app.state.live.version !== version) return`).
struct Deferred {
    key: String,
    cue_id: String,
    index: usize,
    dir: i64,
    version: u64,
}
static DEFERRED: Lazy<Mutex<Option<Deferred>>> = Lazy::new(|| Mutex::new(None));

/// index 순서가 만드는 중인 PPT 면 '끝나면 이어서 송출' 을 기억하고 true
pub fn defer_advance(core: &Core, index: usize, dir: i64) -> bool {
    let Some(cue) = core.cue.as_ref() else { return false };
    let Some(it) = cue.items.get(index) else { return false };
    let Some(file) = it["file"].as_str().filter(|_| it["type"] == "hymn") else { return false };
    if !is_building(file) {
        return false;
    }
    *DEFERRED.lock() = Some(Deferred { key: norm(file), cue_id: cue.id.clone(), index, dir, version: core.pres.version });
    true
}

/// 기억한 넘김이 이 파일이고 아직 유효하면 꺼내서 돌려준다 (다른 파일이면 그대로 둠)
fn take_deferred(key: &str, cue_id: &str, version: u64) -> Option<(usize, i64)> {
    let mut g = DEFERRED.lock();
    if g.as_ref().is_some_and(|d| d.key == key) {
        let d = g.take().unwrap();
        return (d.cue_id == cue_id && d.version == version).then_some((d.index, d.dir));
    }
    None
}

/// 만들기가 실패했을 때: 이 파일에 기억한 넘김은 버린다
fn clear_deferred(key: &str) {
    let mut g = DEFERRED.lock();
    if g.as_ref().is_some_and(|d| d.key == key) {
        *g = None;
    }
}

/// 만들기가 꿝났을 때: 기억한 넘김이 있으면 이어서 송출 (trigger::advance_live_cue 의 송출 분기와 같게) → 송출했으면 true
fn finish_deferred(c: &mut Core, key: &str) -> bool {
    let cue_id = c.cue.as_ref().map(|q| q.id.clone()).unwrap_or_default();
    let Some((index, dir)) = take_deferred(key, &cue_id, c.pres.version) else { return false };
    match c.item_deck(index) {
        Ok(mut deck) => {
            if dir < 0 { deck.index = deck.slides.len().saturating_sub(1); }
            c.pres.preview = Some(deck.clone());
            c.send_deck(deck);
            true
        }
        Err(e) => {
            c.notify(e, "warn");
            false
        }
    }
}

fn start_build(core: &Core, file: &str) {
    let key = norm(file);
    if !PENDING.lock().insert(key.clone()) {
        return;
    }
    let data_dir = core.data_dir.clone();
    let render = folder_render(core, file);
    let file = file.to_string();
    let tx = core.tx.clone();
    spawn_big(move || {
        let r = build_meta(&data_dir, &file, &render);
        PENDING.lock().remove(&key);
        let Some(shared) = SHARED.get().and_then(|w| w.upgrade()) else { return };
        let mut c = shared.lock();
        match r {
            Ok(m) => {
                let stamp = format!("{}|{}", norm(&file), file_stamp(Path::new(&file)));
                META.lock().insert(stamp, with_captions(&c, m));
                let name = Path::new(&file).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                if finish_deferred(&mut c, &key) {
                    // 꿝에서 F6/F5 로 기다리던 다음 순서를 이어서 송출했다 (Electron onCueNext 와 같은 결과)
                    c.notify(crate::i18n::tr_args("'{1}' 슬라이드를 만들어 다음 순서로 넘어갔습니다.", &[&name]), "ok");
                } else {
                    // 이 파일 순서를 보고 있었으면 미리보기를 다시
                    let cue_id = c.cue.as_ref().map(|q| q.id.clone()).unwrap_or_default();
                    let pv = c.pres.preview.as_ref().filter(|p| p.passage["cueId"] == cue_id.as_str()).and_then(|p| p.passage["cueIndex"].as_i64()); // 열린 큐시트의 미리보기만
                    let want = c.cue.as_ref().and_then(|q| q.items.iter().position(|it| it["file"].as_str().map(norm).as_deref() == Some(key.as_str())));
                    if let Some(i) = want.filter(|i| c.pres.preview.is_none() || pv == Some(*i as i64)) {
                        c.preview_item(i);
                    }
                    c.notify(crate::i18n::tr_args("'{1}' 슬라이드를 만들었습니다.", &[&name]), "ok");
                }
            }
            Err(e) => {
                clear_deferred(&key);
                c.notify(e, "error");
            }
        }
        let _ = tx.send(crate::core::Ev::Ui);
    });
}

/// 등록 폴더의 읽는 방식: auto | slides(PowerPoint) | direct(PowerPoint 없이)
pub fn folder_render(core: &Core, file: &str) -> String {
    let f = norm(file);
    for x in core.settings.get()["hymnFolders"].as_array().into_iter().flatten() {
        let d = norm(x["path"].as_str().unwrap_or(""));
        if !d.is_empty() && f.starts_with(&(d.trim_end_matches('\\').to_string() + "\\")) {
            return x["render"].as_str().unwrap_or("auto").to_string();
        }
    }
    "auto".into()
}

pub fn set_folder_render(core: &mut Core, dir: &str, render: &str) {
    let (dir, render) = (dir.to_string(), render.to_string());
    core.settings.update(|d| {
        if let Some(a) = d["hymnFolders"].as_array_mut() {
            for f in a.iter_mut() {
                if f["path"].as_str() == Some(&dir) { f["render"] = json!(render); }
            }
        }
    });
}

/// 큰 스택으로 스레드 (resvg·XML 이 깊게 들어가 기본 2MB 로는 넘칠 수 있다 — 넘치면 프로그램 전체가 죽는다)
pub fn spawn_big(f: impl FnOnce() + Send + 'static) {
    let _ = std::thread::Builder::new().stack_size(64 * 1024 * 1024).spawn(f);
}

/// 지금 코어 (백그라운드 일이 끝난 뒤 다시 잡으려고)
pub fn shared() -> Option<crate::core::Shared> {
    SHARED.get().and_then(|w| w.upgrade())
}

/// 백그라운드로 일을 돌리고, 끝나면 코어를 잡아 done 을 부른다 (화면이 멈추지 않게)
pub fn run_bg<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static, done: impl FnOnce(&mut Core, T) + Send + 'static) {
    spawn_big(move || {
        let r = work();
        if let Some(sh) = shared() {
            let mut c = sh.lock();
            done(&mut c, r);
            c.emit(crate::core::Ev::Ui);
        }
    });
}

/// PPT 요소 따로 꺼내기 (pptTextLayers): → { key, bgs, slides: [[편집기 항목…]] }
/// 아직 꺼낸 적이 없으면 백그라운드로 만들고 BUILDING 을 돌려준다 (끝나면 알림)
/// again=true: 캐시를 지우고 요소·배경 그림을 다시 만든다 (cue.js pptTextOn mode 'bg' — 순서의 edits 는 손대지 않는다). 끝나면 이 파일을 보던 미리보기도 다시.
pub fn ppt_text_layers(core: &Core, file: &str, screen: (f64, f64), again: bool) -> Result<Value, String> {
    let meta = load(core, file)?;
    if !meta["layers"].as_bool().unwrap_or(false) && !file.to_lowercase().ends_with('x') {
        return Err(crate::i18n::tr("옛 형식(.ppt/.pps)은 요소를 꺼낼 수 없습니다. PowerPoint 에서 .pptx 로 저장한 뒤 넣어 주세요.").into());
    }
    if again {
        let dir = core.data_dir.join("hymn-cache").join(cache_key(file, "pptx-rs1"));
        let _ = std::fs::remove_dir_all(dir);
    }
    match read_layers_k(core, &meta).filter(|_| !again) {
        Some((key, raw)) => {
            let bgs = layer_bgs(core, &meta).unwrap_or_default();
            let items = crate::pptx::layers::all_items(&raw, screen, &format!("/hymn/{key}/"));
            Ok(json!({ "key": key, "bgs": bgs, "slides": items, "w": raw["w"], "h": raw["h"] }))
        }
        None => {
            let lk = format!("layers:{}", norm(file));
            if PENDING.lock().insert(lk.clone()) {
                let (dd, f) = (core.data_dir.clone(), file.to_string());
                let f2 = f.clone();
                let by_pp = meta["mode"] == "slides" && has_powerpoint();
                run_bg(move || {
                    let m = crate::pptx::build::build_cache(&dd, &f)?;
                    // PowerPoint 로 그린 PPT 면 배경도 PowerPoint 로 (픽셀 그대로, 직접 그린 배경은 실패할 때만)
                    if by_pp {
                        let dir = dd.join("hymn-cache").join(m["key"].as_str().unwrap_or(""));
                        if let Err(e) = powerpoint_bgs(&f, &dir) {
                            log::warn!("PowerPoint 배경 그림을 만들지 못해 직접 그린 배경을 씁니다: {e}");
                        }
                    }
                    Ok(m)
                }, move |c, r: Result<Value, String>| {
                    PENDING.lock().remove(&lk);
                    META.lock().clear(); // layerBgs 를 다시 읽게
                    match r {
                        Ok(_) => {
                            // 이 파일의 순서를 미리보기하고 있었으면 새 배경 그림으로 다시 (cue.js previewItem(i, {keepIndex}))
                            let key = norm(&f2);
                            let cue_id = c.cue.as_ref().map(|q| q.id.clone()).unwrap_or_default();
                            let pv = c.pres.preview.as_ref().filter(|p| p.passage["cueId"] == cue_id.as_str()).and_then(|p| p.passage["cueIndex"].as_i64());
                            let same = pv.and_then(|i| c.cue.as_ref().and_then(|q| q.items.get(i as usize)).map(|it| it["file"].as_str().map(norm).as_deref() == Some(key.as_str())));
                            if let (Some(i), Some(true)) = (pv, same) { c.preview_item(i as usize); }
                            let msg = if again { "요소를 숨긴 PPT 배경 그림을 다시 만들었습니다." } else { "PPT 요소를 꺼냈습니다. 다시 누르면 슬라이드 편집기에 들어갑니다." };
                            c.notify(crate::i18n::tr(msg), "ok");
                        }
                        Err(e) => c.notify(e, "error"),
                    }
                });
            }
            Err(BUILDING.into())
        }
    }
}

/// 슬라이드마다 글자 (나누기용) — PPT 안의 글자 · .pps 는 슬라이드 글자 · 없으면 빈 글
pub fn slide_texts(core: &Core, file: &str) -> Result<Vec<String>, String> {
    let meta = load(core, file)?;
    if let Some(a) = meta["slideTexts"].as_array().filter(|a| !a.is_empty()) {
        return Ok(a.iter().map(|t| t.as_str().unwrap_or("").to_string()).collect());
    }
    let n = meta["slides"].as_array().or(meta["scores"].as_array()).map(|a| a.len()).unwrap_or(0);
    let texts: Vec<String> = meta["texts"].as_array().into_iter().flatten().map(|t| t.as_array().into_iter().flatten().filter_map(|x| x.as_str()).collect::<Vec<_>>().join("\n")).collect();
    Ok(if texts.len() >= n { texts } else { vec![String::new(); n] })
}

/// 나눈 결과(parts) → 큐시트에 넣을 찬송가(PPT) 순서들 (같은 파일, 범위만 다르게)
pub fn split_items(it: &Value, parts: &[crate::pptsplit::Part]) -> Vec<Value> {
    parts.iter().map(|p| {
        let mut x = it.clone();
        x["id"] = json!(crate::util::short_id("i"));
        x["title"] = json!(p.name);
        x["range"] = json!(if p.a == p.b { p.a.to_string() } else { format!("{}-{}", p.a, p.b) });
        if let Some(o) = x.as_object_mut() { o.remove("edits"); o.remove("skip"); o.remove("slideOrder"); }
        x
    }).collect()
}

/// 꺼낸 요소를 순서의 편집(edits)에 넣는다 (Electron cue.js pptTextOn): 슬라이드마다 { pptBg:true, items:[꺼낸 것 + 사용자가 덧붙인 것] }
/// → (꺼낸 슬라이드 수, 글상자, 그림, 도형)
pub fn apply_layers(it: &mut Value, layers: &Value, slide_count: usize) -> (usize, usize, usize, usize) {
    let (a0, n) = match it["range"].as_str().and_then(range_of) {
        Some((a, b)) => (a - 1, b.min(slide_count).saturating_sub(a - 1)),
        None => (0, slide_count),
    };
    let mut edits = it["edits"].as_object().cloned().unwrap_or_default();
    let (mut sl, mut t, mut p, mut s) = (0, 0, 0, 0);
    for k in 0..n {
        let items = layers["slides"][a0 + k].as_array().cloned().unwrap_or_default();
        let mut e = edits.get(&k.to_string()).cloned().unwrap_or(json!({}));
        let keep: Vec<Value> = e["items"].as_array().into_iter().flatten().filter(|x| !crate::pptx::layers::is_ppt_item(x["id"].as_str().unwrap_or(""))).cloned().collect();
        if items.is_empty() {
            if let Some(o) = e.as_object_mut() { o.remove("pptBg"); }
            e["items"] = json!(keep);
        } else if items.len() + keep.len() > 60 {
            continue;
        } else {
            for x in &items {
                match x["type"].as_str() { Some("image") => p += 1, Some("shape") => s += 1, _ => t += 1 }
            }
            let mut all = items;
            all.extend(keep);
            e["items"] = json!(all);
            e["pptBg"] = json!(true);
            sl += 1;
        }
        if e["items"].as_array().is_some_and(|a| a.is_empty()) {
            if let Some(o) = e.as_object_mut() { o.remove("items"); }
        }
        if e.as_object().is_some_and(|o| o.is_empty()) { edits.remove(&k.to_string()); } else { edits.insert(k.to_string(), e); }
    }
    if edits.is_empty() {
        if let Some(o) = it.as_object_mut() { o.remove("edits"); }
    } else {
        it["edits"] = Value::Object(edits);
    }
    (sl, t, p, s)
}

/// 이 순서에서 요소로 꺼낸 슬라이드 수 (cue.js pptTextCount): edits 값 중 pptBg 가 참인 것
pub fn ppt_text_count(it: &Value) -> usize {
    it["edits"].as_object().into_iter().flatten().filter(|(_, e)| e["pptBg"].as_bool().unwrap_or(false)).count()
}

/// 원래 PPT 그림으로 (cue.js pptTextOff): 꺼낸 요소(p\d+[tis]\d+)·pptBg 를 무고, 덧붙인 글상자·그림과 다른 편집(skip 등)은 그대로.
/// 무언가 바뀜었으면 true.
pub fn ppt_text_off(it: &mut Value) -> bool {
    let Some(mut edits) = it["edits"].as_object().cloned() else { return false };
    let mut changed = false;
    for k in edits.keys().cloned().collect::<Vec<_>>() {
        let Some(e) = edits.get_mut(&k) else { continue };
        let Some(o) = e.as_object_mut() else { continue };
        if o.remove("pptBg").is_some() { changed = true; }
        if let Some(items) = o.get("items").and_then(|a| a.as_array()).cloned() {
            let keep: Vec<Value> = items.iter().filter(|x| !crate::pptx::layers::is_ppt_item(x["id"].as_str().unwrap_or(""))).cloned().collect();
            if keep.len() != items.len() { changed = true; }
            if keep.is_empty() { o.remove("items"); } else { o.insert("items".into(), json!(keep)); }
        }
        if o.is_empty() { edits.remove(&k); }
    }
    if edits.is_empty() {
        if let Some(o) = it.as_object_mut() { o.remove("edits"); }
    } else {
        it["edits"] = Value::Object(edits);
    }
    changed
}

/// 요소를 숨긴 배경 그림이 하나도 없는가 (cue.js pptTextRow 의 missing: 다른 PC·캐시 지움) — 꺼낸 순서에서만 의미 있다
pub fn layer_bgs_missing(core: &Core, file: &str) -> bool {
    match load(core, file) {
        Ok(meta) => !meta["layerBgs"].as_array().is_some_and(|a| a.iter().any(|b| b.as_str().is_some_and(|s| !s.is_empty()))),
        Err(_) => false,
    }
}

pub fn is_building(file: &str) -> bool {
    PENDING.lock().contains(&norm(file))
}

fn file_stamp(file: &Path) -> String {
    std::fs::metadata(file)
        .map(|m| format!("{}|{:?}", m.len(), m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis())))
        .unwrap_or_default()
}

/// 캐시에서 이 파일의 meta.json 찾기 (Electron 판 캐시 열쇠는 파일 크기·시각으로 만들어져 같은 식으로 다시 못 만드니, meta 의 file 로 찾는다)
fn find_cached(core: &Core, file: &str) -> Option<Value> {
    let f = norm(file);
    let mut best: Option<(std::time::SystemTime, Value)> = None;
    for dir in cache_dirs(core) {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let mf = e.path().join("meta.json");
            let Ok(txt) = std::fs::read_to_string(&mf) else { continue };
            let Ok(m) = serde_json::from_str::<Value>(txt.trim_start_matches('\u{feff}')) else { continue };
            if m["file"].as_str().map(norm).as_deref() != Some(f.as_str()) {
                continue;
            }
            // 원본보다 오래된 캐시는 쓰지 않는다
            let ct = std::fs::metadata(&mf).and_then(|x| x.modified()).ok();
            let ft = std::fs::metadata(file).and_then(|x| x.modified()).ok();
            if let (Some(c), Some(fm)) = (ct, ft) {
                if c < fm { continue; }
            }
            let t = ct.unwrap_or(std::time::UNIX_EPOCH);
            if best.as_ref().is_none_or(|b| t > b.0) {
                best = Some((t, m));
            }
        }
    }
    best.map(|b| b.1)
}

/// PowerPoint 설치 여부
pub fn has_powerpoint() -> bool {
    static PP: Lazy<bool> = Lazy::new(|| {
        #[cfg(windows)]
        {
            std::process::Command::new("reg").args(["query", r"HKCR\PowerPoint.Application\CurVer"]).output().map(|o| o.status.success()).unwrap_or(false)
        }
        #[cfg(not(windows))]
        {
            false
        }
    });
    *PP
}

/// Electron pptrender.js 의 PS 와 같은 스크립트: 슬라이드마다 PNG + 글자(texts.json)
const PS_EXPORT: &str = r#"
$ErrorActionPreference = 'Stop'
$pp = New-Object -ComObject PowerPoint.Application
try {
  $p = $pp.Presentations.Open($env:WC_PPT_FILE, -1, 0, 0)
  $sw = $p.PageSetup.SlideWidth; $sh = $p.PageSetup.SlideHeight
  $outW = 1920; $outH = [int](1920 * $sh / $sw)
  $n = 0
  $texts = New-Object System.Collections.ArrayList
  function Get-Texts($shapes) {
    $out = @()
    foreach ($sh in $shapes) {
      try {
        if ($sh.Type -eq 6) { $out += Get-Texts $sh.GroupItems; continue }
        if ($sh.Type -eq 14) { $pt = $sh.PlaceholderFormat.Type; if ($pt -eq 13 -or $pt -eq 15 -or $pt -eq 16) { continue } }
        if ($sh.HasTextFrame -and $sh.TextFrame.HasText) {
          $out += [pscustomobject]@{ top = [double]$sh.Top; left = [double]$sh.Left; text = [string]$sh.TextFrame.TextRange.Text }
        }
      } catch { }
    }
    return $out
  }
  foreach ($s in $p.Slides) {
    if ($s.SlideShowTransition.Hidden) { continue }
    $n++
    $s.Export((Join-Path $env:WC_PPT_OUT ('slide-{0:D3}.png' -f $n)), 'PNG', $outW, $outH)
    $parts = @(Get-Texts $s.Shapes | Sort-Object top, left | ForEach-Object { $_.text })
    [void]$texts.Add(($parts -join [char]10))
  }
  $json = ConvertTo-Json -InputObject @($texts) -Compress
  [IO.File]::WriteAllText((Join-Path $env:WC_PPT_OUT 'texts.json'), $json, (New-Object Text.UTF8Encoding $false))
  $p.Close()
  Write-Output "OK $n $outW $outH"
} finally {
  if ($pp.Presentations.Count -eq 0) { $pp.Quit() }
}
"#;

/// Electron pptrender.js PS_BG: hide.json 의 도형을 숨기고 슬라이드마다 slide-NNN-bg.png (파일은 저장하지 않음)
const PS_BG: &str = r#"
$ErrorActionPreference = 'Stop'
$hide = Get-Content -LiteralPath $env:WC_PPT_HIDE -Raw -Encoding UTF8 | ConvertFrom-Json
$pp = New-Object -ComObject PowerPoint.Application
try {
  $p = $pp.Presentations.Open($env:WC_PPT_FILE, -1, 0, 0)
  $slideW = $p.PageSetup.SlideWidth; $slideH = $p.PageSetup.SlideHeight
  $outW = 1920; $outH = [int](1920 * $slideH / $slideW)
  function Get-Flat($shapes) {
    $out = New-Object System.Collections.ArrayList
    foreach ($x in $shapes) {
      try {
        [void]$out.Add($x)
        if ($x.Type -eq 6) { foreach ($y in (Get-Flat $x.GroupItems)) { [void]$out.Add($y) } }
      } catch { }
    }
    return ,$out
  }
  $n = 0
  foreach ($s in $p.Slides) {
    if ($s.SlideShowTransition.Hidden) { continue }
    $n++
    $ids = @()
    $prop = $hide.PSObject.Properties[[string]$n]
    if ($prop) { $ids = @($prop.Value | ForEach-Object { [string]$_ }) }
    if ($ids.Count -gt 0) {
      foreach ($x in (Get-Flat $s.Shapes)) {
        try { if ($ids -contains [string]$x.Id) { $x.Visible = 0 } } catch { }
      }
    }
    $s.Export((Join-Path $env:WC_PPT_OUT ('slide-{0:D3}-bg.png' -f $n)), 'PNG', $outW, $outH)
  }
  $p.Saved = -1
  $p.Close()
  Write-Output "OK $n $outW $outH"
} finally {
  if ($pp.Presentations.Count -eq 0) { $pp.Quit() }
}
"#;

/// PowerPoint 로 요소를 숨긴 배경 그림을 dir(layers.json 이 있는 캐시 폴더)에 만든다 → 슬라이드 수
pub fn powerpoint_bgs(file: &str, dir: &Path) -> Result<usize, String> {
    let lj = dir.join("layers.json");
    let mut raw: Value = std::fs::read_to_string(&lj).ok().and_then(|t| serde_json::from_str(t.trim_start_matches('\u{feff}')).ok()).ok_or("layers.json 이 없습니다.")?;
    let hide = dir.join("hide.json");
    std::fs::write(&hide, raw["hide"].to_string()).map_err(|e| e.to_string())?;
    let script = dir.join("bg.ps1");
    std::fs::write(&script, format!("\u{feff}{PS_BG}")).map_err(|e| e.to_string())?;
    let mut cmd = std::process::Command::new("powershell");
    cmd.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"]).arg(&script).env("WC_PPT_FILE", file).env("WC_PPT_OUT", dir).env("WC_PPT_HIDE", &hide);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let out = cmd.output().map_err(|e| format!("PowerPoint 를 부르지 못했습니다: {e}"))?;
    let _ = std::fs::remove_file(&script);
    let so = String::from_utf8_lossy(&out.stdout).to_string();
    let n = so.lines().find_map(|l| l.strip_prefix("OK ")).and_then(|l| l.split_whitespace().next()).and_then(|x| x.parse::<usize>().ok())
        .ok_or_else(|| format!("PowerPoint 배경 그림 실패: {}", String::from_utf8_lossy(&out.stderr).lines().find(|l| !l.trim().is_empty()).unwrap_or("")))?;
    // 배경 그림 이름은 직접 그린 것과 같다 (slide-NNN-bg.png) → 표시만
    let bgs: Vec<Value> = (1..=n).map(|i| json!(format!("slide-{i:03}-bg.png"))).collect();
    raw["bgs"] = json!(bgs);
    raw["bgFrom"] = json!("powerpoint");
    std::fs::write(&lj, raw.to_string()).map_err(|e| e.to_string())?;
    Ok(n)
}

/// 캐시 폴더 이름: 파일 경로 · 크기 · 시각 · 방식
pub fn cache_key(file: &str, tag: &str) -> String {
    hex16(&format!("{}|{}|{tag}", norm(file), file_stamp(Path::new(file))))
}

fn hex16(s: &str) -> String {
    // 작은 FNV-1a 64비트 → 16자리 (캐시 폴더 이름. Electron 은 sha1 앞 16자리)
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", h)
}

fn export_powerpoint(data_dir: &Path, file: &str) -> Result<Value, String> {
    if !has_powerpoint() {
        return Err(crate::i18n::tr("이 PC 에 PowerPoint 가 없어 PPT 를 그림으로 만들 수 없습니다. (PowerPoint 없이 직접 그리기는 Rust 판 다음 작업)").into());
    }
    let key = cache_key(file, "slides-rs1");
    let dir = data_dir.join("hymn-cache").join(&key);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let script = dir.join("export.ps1");
    std::fs::write(&script, format!("\u{feff}{}", PS_EXPORT)).map_err(|e| e.to_string())?;
    let mut cmd = std::process::Command::new("powershell");
    cmd.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"]).arg(&script).env("WC_PPT_FILE", file).env("WC_PPT_OUT", &dir);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let out = cmd.output().map_err(|e| format!("PowerPoint 를 부르지 못했습니다: {e}"))?;
    let _ = std::fs::remove_file(&script);
    let so = String::from_utf8_lossy(&out.stdout).to_string();
    let Some(line) = so.lines().find(|l| l.starts_with("OK ")) else {
        return Err(crate::i18n::tr_args("PowerPoint 로 슬라이드를 내보내지 못했습니다: {1}", &[&(String::from_utf8_lossy(&out.stderr).lines().next().unwrap_or(""))]));
    };
    let nums: Vec<f64> = line[3..].split_whitespace().filter_map(|x| x.parse().ok()).collect();
    let n = nums.first().copied().unwrap_or(0.0) as usize;
    if n == 0 {
        return Err(crate::i18n::tr("슬라이드가 없는 파일입니다.").into());
    }
    let texts: Value = std::fs::read_to_string(dir.join("texts.json")).ok().and_then(|t| serde_json::from_str(t.trim_start_matches('\u{feff}')).ok()).unwrap_or(json!([]));
    let (number, title, name) = parse_name(Path::new(file));
    let meta = json!({
        "key": key, "number": number, "title": title, "name": name, "file": file, "mode": "slides", "background": null, "scores": [], "texts": [],
        "slides": (1..=n).map(|i| format!("/hymn/{key}/slide-{i:03}.png")).collect::<Vec<_>>(),
        "slideTexts": texts, "aspect": nums.get(1).copied().unwrap_or(16.0) / nums.get(2).copied().unwrap_or(9.0),
        "layers": file.to_lowercase().ends_with("x"),
    });
    std::fs::write(dir.join("meta.json"), meta.to_string()).map_err(|e| e.to_string())?;
    Ok(meta)
}

/// 파일 → meta (캐시에 있으면 바로, 없으면 PowerPoint 로 만든다) + captions
pub fn load(core: &Core, file: &str) -> Result<Value, String> {
    if !is_allowed(core, file) {
        return Err(crate::i18n::tr("등록한 찬송가 폴더 안의 파일만 열 수 있습니다.").into());
    }
    if !Path::new(file).is_file() {
        return Err(crate::i18n::tr_args("파일이 없습니다: {1}", &[&file]));
    }
    let stamp = format!("{}|{}", norm(file), file_stamp(Path::new(file)));
    if let Some(m) = META.lock().get(&stamp) {
        return Ok(m.clone());
    }
    let meta = match find_cached(core, file) {
        Some(m) => m,
        None => {
            // 코어 잠금 밖에서 만든다: 이번에는 '만드는 중' 으로 돌려주고, 끝나면 알림 + 미리보기 다시
            start_build(core, file);
            return Err(BUILDING.into());
        }
    };
    let mut meta = with_captions(core, meta);
    if let Some(b) = layer_bgs(core, &meta) {
        meta["layerBgs"] = json!(b);
    }
    META.lock().insert(stamp, meta.clone());
    Ok(meta)
}

/// 슬라이드마다 OBS 자막(가사만)용 글자: PPT 안의 글자 (새찬송가 가사 DB 는 트랙 A 다음 작업)
pub fn with_captions(core: &Core, mut meta: Value) -> Value {
    let own: Vec<String> = if meta["slides"].is_array() {
        meta["slideTexts"].as_array().into_iter().flatten().map(|t| t.as_str().unwrap_or("").to_string()).collect()
    } else {
        let n = meta["scores"].as_array().map(|a| a.len()).unwrap_or(0);
        let t = meta["texts"].as_array().cloned().unwrap_or_default();
        if t.len() == n { t.iter().map(|x| x.as_array().into_iter().flatten().filter_map(|l| l.as_str()).collect::<Vec<_>>().join("\n")).collect() } else { vec![] }
    };
    let re = Regex::new(r"\n\s*\n").unwrap();
    if own.iter().any(|t| !t.trim().is_empty()) {
        meta["captions"] = json!(own.iter().map(|t| re.replace_all(t, "\n").trim().to_string()).collect::<Vec<_>>());
        meta["captionSource"] = json!("PPT 안의 글자");
        return meta;
    }
    meta["captions"] = json!([]);
    meta["captionSource"] = json!("");
    // 새찬송가 폴더의 악보 PPT: 베들레헴 새찬송가 가사 DB 를 슬라이드 수에 맞춰
    let file = meta["file"].as_str().unwrap_or("").to_string();
    let folder_name = core.settings.get()["hymnFolders"].as_array().into_iter().flatten()
        .find(|x| { let d = norm(x["path"].as_str().unwrap_or("")); !d.is_empty() && norm(&file).starts_with(&(d.trim_end_matches('\\').to_string() + "\\")) })
        .map(|x| format!("{} {}", x["name"].as_str().unwrap_or(""), x["path"].as_str().unwrap_or(""))).unwrap_or_default();
    let n = meta["slides"].as_array().or(meta["scores"].as_array()).map(|a| a.len()).unwrap_or(0);
    let num = meta["number"].as_i64().unwrap_or(0);
    if folder_name.contains("새찬송가") && num > 0 && n > 0 {
        if let Some((v, c)) = crate::hymnal::lyrics(core, num) {
            meta["captions"] = json!(crate::hymnal::align_lyrics(&v, &c, n));
            meta["captionSource"] = json!(format!("새찬송가 가사 DB ({num}장)"));
        }
    }
    meta
}

fn range_of(r: &str) -> Option<(usize, usize)> {
    let re = Regex::new(r"^\s*(\d+)\s*(?:[-~]\s*(\d+))?\s*$").unwrap();
    let m = re.captures(r)?;
    let a: usize = m[1].parse().ok()?;
    let b: usize = m.get(2).and_then(|x| x.as_str().parse().ok()).unwrap_or(a);
    if a < 1 { return None; }
    Some((a.min(b), a.max(b)))
}

/// 기본 배경 설정(settings.hymnBackground) 값 → 사람이 읽는 이름 (cue.js hymnBgSel: 원래 배경·템플릿 배경·흰 배경·지정 이미지)
pub fn default_bg_label(v: &str) -> &'static str {
    match v {
        "" | "original" => "원래 배경",
        "template" => "템플릿 배경",
        "#ffffff" => "흰 배경",
        _ => "지정 이미지",
    }
}

/// 지금 기본 배경 설정 값 (없으면 original — cue.js 와 같은 기본값)
pub fn default_bg(core: &Core) -> String {
    core.settings.get()["hymnBackground"].as_str().filter(|s| !s.is_empty()).unwrap_or("original").to_string()
}

/// 기본 배경 설정 바꾸기 (cue.js updateSettings {hymnBackground}): original | #ffffff | template | 그림 주소
pub fn set_default_bg(core: &mut Core, v: &str) {
    let v = v.to_string();
    core.settings.update(|d| d["hymnBackground"] = json!(v));
}

/// 기본 배경을 지정 이미지로: 그림을 <데이터>/media/hymnbg-… 으로 복사해 '/media/…' 주소를 설정에 (Electron pickImage → POST /api/media → /media/… 와 같은 모양)
pub fn set_default_bg_image(core: &mut Core, src: &Path) -> Result<String, String> {
    if !crate::library::is_image(src) {
        return Err(crate::i18n::tr("그림 파일(png·jpg·webp·gif)만 기본 배경으로 쓸 수 있습니다.").into());
    }
    let dir = core.data_dir.join("media");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let ext = src.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_else(|| "png".into());
    let name = format!("hymnbg-{}.{ext}", crate::util::now_ms());
    std::fs::copy(src, dir.join(&name)).map_err(|e| crate::i18n::tr_args("그림을 복사하지 못했습니다: {1}", &[&e]))?;
    let url = format!("/media/{name}");
    set_default_bg(core, &url);
    Ok(url)
}

/// 찬송가 순서의 배경 (cue.js hymnBg)
pub fn hymn_bg(core: &Core, it: &Value, meta: &Value) -> String {
    let mut b = it["background"].as_str().unwrap_or("default").to_string();
    if b == "default" || b.is_empty() {
        b = core.settings.get()["hymnBackground"].as_str().unwrap_or("original").to_string();
    }
    match b.as_str() {
        "original" => meta["background"].as_str().unwrap_or("").to_string(),
        "template" => String::new(),
        _ => b,
    }
}

/// 찬송가 순서 → 슬라이드 (cue.js buildSlides 의 hymn 분기 + 범위 + 건너뛰기)
pub fn build_slides(core: &Core, it: &Value, opts: &SplitOpts) -> Result<Vec<Value>, String> {
    let file = it["file"].as_str().unwrap_or("");
    if file.is_empty() {
        return Ok(vec![]);
    }
    let meta = load(core, file)?;
    let title = it["title"].as_str().unwrap_or("");
    let bg = hymn_bg(core, it, &meta);
    let caps: Vec<String> = match it["obsLyrics"].as_str().filter(|s| !s.trim().is_empty()) {
        Some(l) => slides::lyrics_chunks(l).into_iter().map(|c| c.text).collect(),
        None => meta["captions"].as_array().into_iter().flatten().map(|x| x.as_str().unwrap_or("").to_string()).collect(),
    };
    let cap = |k: usize| caps.get(k).cloned().unwrap_or_default();
    let list = |k: &str| meta[k].as_array().cloned().unwrap_or_default();
    let mut out: Vec<Value> = if !list("slides").is_empty() {
        list("slides").iter().enumerate().map(|(k, img)| json!({ "kind": "image", "image": img, "ref": title, "label": (k + 1).to_string(), "parts": [], "caption": cap(k), "_o": k })).collect()
    } else if !list("scores").is_empty() {
        list("scores").iter().enumerate().map(|(k, img)| json!({ "kind": "score", "image": img, "bg": bg, "ref": title, "label": (k + 1).to_string(), "parts": [], "caption": cap(k) })).collect()
    } else if list("texts").iter().any(|t| t.as_array().is_some_and(|a| !a.is_empty())) {
        let chunks: Vec<Chunk> = list("texts").iter().enumerate().map(|(k, t)| Chunk { label: (k + 1).to_string(), text: t.as_array().into_iter().flatten().filter_map(|l| l.as_str()).collect::<Vec<_>>().join("\n"), src: None }).collect();
        let mut v = slides::build_text(title, false, &chunks, opts);
        for s in v.iter_mut() { s["bg"] = json!(bg); }
        v
    } else {
        vec![]
    };
    // 범위 '5-9': 이름표는 원래 번호 그대로
    if let Some((a, b)) = it["range"].as_str().and_then(range_of) {
        out = out.into_iter().skip(a - 1).take(b + 1 - a).collect();
    }
    // PPT 요소 따로 꺼내기 (Electron cue.js buildSlides + decorate): 키는 범위를 적용한 뒤의 슬라이드 번호
    let bgs = list("layerBgs");
    for (j, s) in out.iter_mut().enumerate() {
        let o = s["_o"].as_u64().map(|x| x as usize).unwrap_or(j);
        if let Some(m) = s.as_object_mut() { m.remove("_o"); }
        let e = &it["edits"][j.to_string()];
        let mut edit = merge_edit(&it["edits"], &j.to_string());
        if e["pptBg"].as_bool().unwrap_or(false) {
            match bgs.get(o).and_then(|x| x.as_str()).filter(|x| !x.is_empty()) {
                Some(b) => {
                    // 목록 썸네일은 원래 슬라이드 그림 (배경만이면 빈 칸처럼 보인다)
                    s["thumb"] = s["image"].clone();
                    s["image"] = json!(b);
                }
                None => {
                    // 배경 그림이 없으면 원래 그림 그대로 + 꺼낸 요소는 빼고 (겹쳐 보이지 않게)
                    s["pptBgMissing"] = json!(true);
                    if let Some(items) = edit.as_mut().and_then(|x| x["items"].as_array_mut()) {
                        items.retain(|x| !crate::pptx::layers::is_ppt_item(x["id"].as_str().unwrap_or("")));
                    }
                }
            }
        }
        if let Some(ed) = edit.filter(|x| x.as_object().is_some_and(|o| !o.is_empty())) {
            s["edit"] = ed;
        }
    }
    // 공통 편집기로 원본 차례·덧붙인 카드·건너뛰기를 함께 처리한다.
    let out = crate::slideedit::decorate(it, out, false);
    Ok(out)
}

/// 공통('*') + 슬라이드별 편집 합치기 (Electron slides.js mergeEdit 과 같음)
pub fn merge_edit(edits: &Value, key: &str) -> Option<Value> {
    let a = edits.get("*").filter(|v| v.is_object());
    let b = edits.get(key).filter(|v| v.is_object());
    let mut o = match (a, b) {
        (None, None) => return None,
        (Some(x), None) | (None, Some(x)) => return Some(x.clone()),
        (Some(a), Some(b)) => {
            let mut o = a.clone();
            for (k, v) in b.as_object().unwrap() { o[k] = v.clone(); }
            o
        }
    };
    let mut style = a.unwrap()["style"].as_object().cloned().unwrap_or_default();
    for (k, v) in b.unwrap()["style"].as_object().cloned().unwrap_or_default() { style.insert(k, v); }
    o["style"] = Value::Object(style);
    let mut items = a.unwrap()["items"].as_array().cloned().unwrap_or_default();
    items.extend(b.unwrap()["items"].as_array().cloned().unwrap_or_default());
    o["items"] = json!(items);
    Some(o)
}

/// 꺼낸 요소 정보 (layers.json) — 없으면 None
pub fn read_layers(core: &Core, meta: &Value) -> Option<Value> {
    read_layers_k(core, meta).map(|x| x.1)
}

/// (layers.json 이 있는 캐시 열쇠, 내용): 이 캐시 → (PowerPoint 로 만든 것이면) 직접 읽어 만든 캐시
pub fn read_layers_k(core: &Core, meta: &Value) -> Option<(String, Value)> {
    let file = meta["file"].as_str().unwrap_or("");
    let keys = [meta["key"].as_str().map(String::from), (!file.is_empty()).then(|| cache_key(file, "pptx-rs1"))];
    for key in keys.into_iter().flatten() {
        if let Some(p) = cache_path(core, &key, "layers.json") {
            if let Some(v) = std::fs::read_to_string(p).ok().and_then(|t| serde_json::from_str::<Value>(t.trim_start_matches('\u{feff}')).ok()) {
                return Some((key, v));
            }
        }
    }
    None
}

/// 요소를 뺀 배경 그림 주소들 (슬라이드마다)
pub fn layer_bgs(core: &Core, meta: &Value) -> Option<Vec<String>> {
    let (key, raw) = read_layers_k(core, meta)?;
    let key = key.as_str();
    let bgs = raw["bgs"].as_array()?;
    let n = meta["slides"].as_array().map(|a| a.len()).unwrap_or(0);
    Some((0..n).map(|k| bgs.get(k).and_then(|b| b.as_str()).filter(|b| !b.is_empty()).map(|b| format!("/hymn/{key}/{b}")).unwrap_or_default()).collect())
}

/// /hymn/<key>/<file> → 캐시 파일 (우리 것 → Electron 판)
pub fn cache_path(core: &Core, key: &str, file: &str) -> Option<PathBuf> {
    let ok_key = key.len() == 16 && key.chars().all(|c| c.is_ascii_hexdigit());
    let ok_file = !file.is_empty() && file.chars().all(|c| c.is_alphanumeric() || "._-".contains(c)) && !file.contains("..");
    if !ok_key || !ok_file {
        return None;
    }
    cache_dirs(core).into_iter().map(|d| d.join(key).join(file)).find(|p| p.is_file())
}

static HREF: Lazy<Regex> = Lazy::new(|| Regex::new(r#"href="(media-\d{1,4}\.[A-Za-z0-9]{2,5})""#).unwrap());

/// 캐시의 SVG 를 보낼 글: 안의 href="media-N.ext" 를 같은 폴더 그림의 data: 로 (Electron svgToSend)
pub fn svg_to_send(fp: &Path) -> Option<String> {
    if !fp.extension().is_some_and(|e| e.eq_ignore_ascii_case("svg")) {
        return None;
    }
    let svg = std::fs::read_to_string(fp).ok()?;
    let dir = fp.parent()?.to_path_buf();
    Some(HREF.replace_all(&svg, |c: &regex::Captures| {
        let name = &c[1];
        match std::fs::read(dir.join(name)) {
            Ok(b) => {
                let ext = name.rsplit('.').next().unwrap_or("png").to_lowercase();
                let mime = match ext.as_str() { "jpg" | "jpeg" => "image/jpeg", "svg" => "image/svg+xml", "gif" => "image/gif", "webp" => "image/webp", "bmp" => "image/bmp", _ => "image/png" };
                format!("href=\"data:{};base64,{}\"", mime, b64(&b))
            }
            Err(_) => c[0].to_string(),
        }
    }).to_string())
}

pub fn b64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(data.len() * 4 / 3 + 4);
    for ch in data.chunks(3) {
        let b = [ch[0], *ch.get(1).unwrap_or(&0), *ch.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        s.push(T[(n >> 18) as usize & 63] as char);
        s.push(T[(n >> 12) as usize & 63] as char);
        s.push(if ch.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        s.push(if ch.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    s
}

/// 큐시트에 넣을 찬송가 순서
/// 큐시트 순서 제목 (cue.js openHymnPicker 1527·1533): 번호가 있고 교독문이 아니면 "10장 전능왕 오셔서"
/// — 무대 화면·인쇄·사용 기록·'큐시트를 글로' 에 Electron 과 같은 제목이 나오게.
pub fn item_title(number: Option<i64>, title: &str) -> String {
    match number {
        Some(n) if !title.starts_with("교독문") => format!("{n}장 {title}"),
        _ => title.to_string(),
    }
}

pub fn new_hymn_item(h: &HymnFile) -> Value {
    let mut it = crate::core::new_item("hymn", &item_title(h.number, &h.title));
    it["file"] = json!(h.file.to_string_lossy());
    it["number"] = json!(h.number); // 없으면 null (Electron 과 같음. 표시 쪽은 n > 0 으로 거른다)
    it["background"] = json!("default");
    it["obsLyrics"] = json!("");
    it
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names() {
        assert_eq!(parse_name(Path::new("010 전능왕 오셔서.pps")), (Some(10), "전능왕 오셔서".into(), "010 전능왕 오셔서".into()));
        assert_eq!(parse_name(Path::new("교독문 13 (시편 23편).pptx")).0, Some(13));
        assert_eq!(parse_name(Path::new("clip-20261110.pptx")).0, None);
        // 순서 제목 'N장 ' 접두어 (cue.js 1527): 교독문은 번호가 있어도 그대로, 번호 없으면 그대로
        assert_eq!(item_title(Some(10), "전능왕 오셔서"), "10장 전능왕 오셔서");
        assert_eq!(item_title(Some(13), "교독문 13 (시편 23편)"), "교독문 13 (시편 23편)");
        assert_eq!(item_title(None, "붙여 넣은 슬라이드"), "붙여 넣은 슬라이드");
        // 기본 배경 이름 (cue.js hymnBgSel 의 { original: '원래 배경', template: '템플릿 배경', '#ffffff': '흰 배경' } || '지정 이미지')
        assert_eq!(default_bg_label(""), "원래 배경");
        assert_eq!(default_bg_label("original"), "원래 배경");
        assert_eq!(default_bg_label("#ffffff"), "흰 배경");
        assert_eq!(default_bg_label("template"), "템플릿 배경");
        assert_eq!(default_bg_label("/media/hymnbg-1.png"), "지정 이미지");
        let it = new_hymn_item(&HymnFile { number: Some(10), title: "전능왕 오셔서".into(), name: "010 전능왕 오셔서".into(), folder: "새찬송가".into(), file: PathBuf::from(r"C:\h\010 전능왕 오셔서.pps") });
        assert_eq!(it["title"], "10장 전능왕 오셔서");
        assert_eq!(it["number"], 10);
        let it = new_hymn_item(&HymnFile { number: None, title: "clip".into(), name: "clip".into(), folder: String::new(), file: PathBuf::from(r"C:\h\clip.pptx") });
        assert!(it["number"].is_null());
        assert_eq!(range_of("5-9"), Some((5, 9)));
        assert_eq!(range_of(" 3 "), Some((3, 3)));
        assert_eq!(range_of("x"), None);
        assert_eq!(b64(b"Man"), "TWFu");
        assert_eq!(b64(b"Ma"), "TWE=");
        assert_eq!(hex16("a").len(), 16);
    }
    #[test]
    fn layers_into_edits() {
        let mut it = json!({ "type": "hymn", "range": "2-3", "edits": { "0": { "items": [{ "id": "mine", "type": "text" }, { "id": "p1t0", "type": "text" }] } } });
        let l = json!({ "slides": [[], [{ "id": "p1t0", "type": "text" }, { "id": "p1i1", "type": "image" }], []] });
        let r = apply_layers(&mut it, &l, 3);
        assert_eq!(r, (1, 1, 1, 0));
        assert_eq!(it["edits"]["0"]["pptBg"], true);
        let ids: Vec<&str> = it["edits"]["0"]["items"].as_array().unwrap().iter().map(|x| x["id"].as_str().unwrap()).collect();
        assert_eq!(ids, vec!["p1t0", "p1i1", "mine"]); // 꺼낸 것 + 사용자가 덧붙인 것 (예전에 꺼낸 것은 새로)
        // 다시 꺼내기: 고친 PPT 글은 새 것으로 바뀜다 → UI 는 ppt_text_count > 0 이면 반드시 확인을 받는다 (cue.js pptTextOn again)
        assert_eq!(ppt_text_count(&it), 1);
        it["edits"]["0"]["items"][0]["text"] = json!("고친 글");
        apply_layers(&mut it, &l, 3);
        assert!(it["edits"]["0"]["items"][0]["text"].is_null());
        // 원래대로 (pptTextOff): pptBg 와 p\d+ 요소만 무고 덧붙인 것·다른 편집은 그대로
        it["edits"]["1"] = json!({ "pptBg": true, "items": [{ "id": "p2t0", "type": "text" }] });
        it["edits"]["2"] = json!({ "skip": true });
        assert!(ppt_text_off(&mut it));
        assert_eq!(ppt_text_count(&it), 0);
        let ids: Vec<&str> = it["edits"]["0"]["items"].as_array().unwrap().iter().map(|x| x["id"].as_str().unwrap()).collect();
        assert_eq!(ids, vec!["mine"]);
        assert!(it["edits"]["0"]["pptBg"].is_null());
        assert!(it["edits"].get("1").is_none()); // 버젨 것만 남으면 키 자체를 지움
        assert_eq!(it["edits"]["2"]["skip"], true);
        assert!(!ppt_text_off(&mut it)); // 더 지울 것이 없으면 false
        let mut none = json!({ "type": "hymn", "edits": { "0": { "pptBg": true, "items": [{ "id": "p1t0" }] } } });
        assert!(ppt_text_off(&mut none));
        assert!(none.get("edits").is_none()); // 전부 버져지면 edits 자체를 지움
        let parts = vec![crate::pptsplit::Part { a: 1, b: 2, name: "찬송".into(), kind: "hymn".into() }, crate::pptsplit::Part { a: 3, b: 3, name: "기도".into(), kind: "prayer".into() }];
        let items = split_items(&json!({ "type": "hymn", "file": "x.pptx", "title": "전체" }), &parts);
        assert_eq!(items[0]["range"], "1-2");
        assert_eq!(items[1]["title"], "기도");
    }
}

#[cfg(test)]
mod default_bg_tests {
    use super::*;
    /// 기존 사용자 자료를 복사하지 않도록 뱈 DB 가 있는 고유 임시 폴더를 쓴다.
    fn core() -> Core {
        let dir = std::env::temp_dir().join(format!("wp-hymnbg-{}", crate::util::short_id("t")));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("bible.db"), b"").unwrap();
        Core::open(dir.clone(), dir, None).unwrap()
    }
    #[test]
    fn default_bg_setting_and_image() {
        let mut c = core();
        assert_eq!(default_bg(&c), "original"); // 없으면 original (cue.js `s.hymnBackground || 'original'`)
        set_default_bg(&mut c, "#ffffff");
        assert_eq!(c.settings.get()["hymnBackground"], "#ffffff");
        // 순서의 '설정대로'(default) 는 이 값을 따른다 (hymn_bg)
        assert_eq!(hymn_bg(&c, &json!({ "background": "default" }), &json!({ "background": "/hymn/k/bg.png" })), "#ffffff");
        set_default_bg(&mut c, "original");
        assert_eq!(hymn_bg(&c, &json!({}), &json!({ "background": "/hymn/k/bg.png" })), "/hymn/k/bg.png");
        // 지정 이미지: media 폴더로 복사하고 /media/ 주소를 설정에
        let src = c.data_dir.join("pick.png");
        std::fs::write(&src, b"\x89PNG").unwrap();
        let url = set_default_bg_image(&mut c, &src).unwrap();
        assert!(url.starts_with("/media/hymnbg-") && url.ends_with(".png"));
        assert!(c.data_dir.join("media").join(url.trim_start_matches("/media/")).is_file());
        assert_eq!(default_bg(&c), url);
        assert_eq!(default_bg_label(&default_bg(&c)), "지정 이미지");
        let dir = c.data_dir.clone();
        assert!(set_default_bg_image(&mut c, &dir.join("x.pptx")).is_err());
        drop(c);
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[cfg(test)]
mod deferred_tests {
    use super::*;
    use crate::db::Cue;
    fn core() -> Core {
        let dir = std::env::temp_dir().join(format!("wp-hymndefer-{}", crate::util::short_id("t")));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("bible.db"), b"").unwrap();
        Core::open(dir.clone(), dir, None).unwrap()
    }
    /// 꿝에서 F6 → 만드는 중인 PPT: 기억해 두고, 큐시트·송출 version 이 그대로일 때만 이어서 송출 (cue.js onCueNext)
    #[test]
    fn defer_advance_until_built() {
        let mut c = core();
        let file = r"C:\hymn-defer-test\010 전능왕.pptx";
        c.cue = Some(Cue { id: "q1".into(), name: "q1".into(), service_date: String::new(), start_time: String::new(), items: vec![json!({ "type": "title", "title": "예배" }), json!({ "type": "hymn", "title": "10장 전능왕", "file": file })], memo: String::new(), pinned: false, template: false });
        // 만드는 중이 아니면 기억하지 않는다 (그냥 오류로)
        assert!(!defer_advance(&c, 1, 1));
        assert!(!defer_advance(&c, 0, 1)); // 찬송가가 아닌 순서
        PENDING.lock().insert(norm(file));
        assert!(defer_advance(&c, 1, 1));
        let key = norm(file);
        // 다른 파일의 만들기가 꿝나도 건들지 않는다
        assert_eq!(take_deferred("c:\\other.pptx", "q1", c.pres.version), None);
        assert!(DEFERRED.lock().is_some());
        // 그 사이 다른 송출(version 바뀜)이 있었으면 버린다
        assert_eq!(take_deferred(&key, "q1", c.pres.version + 1), None);
        assert!(DEFERRED.lock().is_none());
        // 다른 큐시트를 열었으면 버린다
        assert!(defer_advance(&c, 1, -1));
        assert_eq!(take_deferred(&key, "q2", c.pres.version), None);
        // 그대로면 순서·방향을 돌려준다
        assert!(defer_advance(&c, 1, -1));
        assert_eq!(take_deferred(&key, "q1", c.pres.version), Some((1, -1)));
        // finish_deferred: 파일을 열 수 없으면 송출하지 않고 경고만 (예전 미리보기가 나가지 않게)
        assert!(defer_advance(&c, 1, 1));
        assert!(!finish_deferred(&mut c, &key));
        assert!(c.pres.live.is_none());
        // 만들기 실패 → 기억도 버림
        assert!(defer_advance(&c, 1, 1));
        clear_deferred(&key);
        assert!(DEFERRED.lock().is_none());
        PENDING.lock().remove(&key);
        let dir = c.data_dir.clone();
        drop(c);
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[cfg(test)]
mod timing_tests {
    /// 출시 빌드로 첫 그리기 시간 재기: cargo test --release -p wp-core first_render_timing -- --ignored --nocapture
    #[test]
    #[ignore]
    fn first_render_timing() {
        let src = std::env::var("WP_TIMING_FILE").unwrap_or_else(|_| format!(r"{}\WorshipCue\data\pasted\clip-20261110-210506.pptx", std::env::var("APPDATA").unwrap()));
        let dir = std::env::temp_dir().join(format!("wp-timing-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("t.pptx");
        std::fs::copy(&src, &file).unwrap();
        for (mode, tag) in [("direct", "direct-1st"), ("direct", "direct-2nd"), ("slides", "powerpoint")] {
            let data = dir.join(tag);
            let t0 = std::time::Instant::now();
            let r = super::build_meta(&data, &file.to_string_lossy(), mode);
            println!("{tag}: {:.2}s ok={} slides={}", t0.elapsed().as_secs_f32(), r.is_ok(), r.as_ref().ok().and_then(|m| m["slides"].as_array().map(|a| a.len())).unwrap_or(0));
            if let Err(e) = r { println!("  오류: {e}"); }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod render_dump_tests {
    /// 파일 하나를 직접 그려 폴더에 남긴다: WP_RENDER_FILE · WP_RENDER_OUT
    #[test]
    #[ignore]
    fn render_dump() {
        let (Ok(file), Ok(out)) = (std::env::var("WP_RENDER_FILE"), std::env::var("WP_RENDER_OUT")) else { return };
        let r = super::build_meta(std::path::Path::new(&out), &file, "direct");
        println!("{}", r.map(|m| m.to_string()).unwrap_or_else(|e| e));
    }
}

#[cfg(test)]
mod ps_bg_tests {
    /// PowerPoint 로 요소 숨긴 배경: cargo test --release -p wp-core ps_bg_real -- --ignored --nocapture (WP_BG_OUT 에 남김)
    #[test]
    #[ignore]
    fn ps_bg_real() {
        if !super::has_powerpoint() { return; }
        let src = std::env::var("WP_BG_FILE").unwrap_or_else(|_| format!(r"{}\WorshipCue\data\pasted\clip-20261110-210506.pptx", std::env::var("APPDATA").unwrap()));
        let out = std::path::PathBuf::from(std::env::var("WP_BG_OUT").unwrap_or_else(|_| std::env::temp_dir().join("wp-psbg").to_string_lossy().to_string()));
        let _ = std::fs::remove_dir_all(&out);
        std::fs::create_dir_all(&out).unwrap();
        let file = out.join("t.pptx");
        std::fs::copy(&src, &file).unwrap();
        let m = crate::pptx::build::build_cache(&out, &file.to_string_lossy()).unwrap();
        let dir = out.join("hymn-cache").join(m["key"].as_str().unwrap());
        let raw: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("layers.json")).unwrap()).unwrap();
        println!("hide: {}", raw["hide"]);
        let t0 = std::time::Instant::now();
        let n = super::powerpoint_bgs(&file.to_string_lossy(), &dir).unwrap();
        println!("PowerPoint 배경 {n}장 ({:.1}초) → {}", t0.elapsed().as_secs_f32(), dir.display());
        let raw: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("layers.json")).unwrap()).unwrap();
        assert_eq!(raw["bgFrom"], "powerpoint");
        assert!(dir.join("slide-001-bg.png").is_file());
    }
}
