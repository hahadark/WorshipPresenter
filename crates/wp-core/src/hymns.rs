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
        return Err("폴더를 찾을 수 없습니다.".into());
    }
    let n = list_folder(dir, "").len();
    if n == 0 {
        return Err("이 폴더에 .pps / .ppt / .pptx 파일이 없습니다.".into());
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
                // 이 파일 순서를 보고 있었으면 미리보기를 다시
                let pv = c.pres.preview.as_ref().and_then(|p| p.passage["cueIndex"].as_i64());
                let want = c.cue.as_ref().and_then(|q| q.items.iter().position(|it| it["file"].as_str().map(norm).as_deref() == Some(key.as_str())));
                let name = Path::new(&file).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                if let Some(i) = want.filter(|i| pv.is_none() || pv == Some(*i as i64)) {
                    c.preview_item(i);
                }
                c.notify(format!("'{name}' 슬라이드를 만들었습니다."), "ok");
            }
            Err(e) => c.notify(e, "error"),
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
pub fn ppt_text_layers(core: &Core, file: &str, screen: (f64, f64), again: bool) -> Result<Value, String> {
    let meta = load(core, file)?;
    if !meta["layers"].as_bool().unwrap_or(false) && !file.to_lowercase().ends_with('x') {
        return Err("옛 형식(.ppt/.pps)은 요소를 꺼낼 수 없습니다. PowerPoint 에서 .pptx 로 저장한 뒤 넣어 주세요.".into());
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
                run_bg(move || crate::pptx::build::build_cache(&dd, &f), move |c, r: Result<Value, String>| {
                    PENDING.lock().remove(&lk);
                    META.lock().clear(); // layerBgs 를 다시 읽게
                    match r {
                        Ok(_) => c.notify("PPT 요소를 꺼냈습니다. 다시 누르면 슬라이드 편집기에 들어갑니다.", "ok"),
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
        return Err("이 PC 에 PowerPoint 가 없어 PPT 를 그림으로 만들 수 없습니다. (PowerPoint 없이 직접 그리기는 Rust 판 다음 작업)".into());
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
        return Err(format!("PowerPoint 로 슬라이드를 내보내지 못했습니다: {}", String::from_utf8_lossy(&out.stderr).lines().next().unwrap_or("")));
    };
    let nums: Vec<f64> = line[3..].split_whitespace().filter_map(|x| x.parse().ok()).collect();
    let n = nums.first().copied().unwrap_or(0.0) as usize;
    if n == 0 {
        return Err("슬라이드가 없는 파일입니다.".into());
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
        return Err("등록한 찬송가 폴더 안의 파일만 열 수 있습니다.".into());
    }
    if !Path::new(file).is_file() {
        return Err(format!("파일이 없습니다: {file}"));
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
    // 건너뛰기 (다 건너뛰면 첫 장만)

    let skip: Vec<i64> = it["skip"].as_array().into_iter().flatten().filter_map(|x| x.as_i64()).collect();
    if !skip.is_empty() {
        let kept: Vec<Value> = out.iter().enumerate().filter(|(k, _)| !skip.contains(&(*k as i64))).map(|(_, s)| s.clone()).collect();
        out = if kept.is_empty() { out.into_iter().take(1).collect() } else { kept };
    }
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
pub fn new_hymn_item(h: &HymnFile) -> Value {
    let mut it = crate::core::new_item("hymn", &h.title);
    it["file"] = json!(h.file.to_string_lossy());
    it["number"] = json!(h.number.unwrap_or(0));
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
        let parts = vec![crate::pptsplit::Part { a: 1, b: 2, name: "찬송".into(), kind: "hymn".into() }, crate::pptsplit::Part { a: 3, b: 3, name: "기도".into(), kind: "prayer".into() }];
        let items = split_items(&json!({ "type": "hymn", "file": "x.pptx", "title": "전체" }), &parts);
        assert_eq!(items[0]["range"], "1-2");
        assert_eq!(items[1]["title"], "기도");
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
