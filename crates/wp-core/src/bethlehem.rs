//! [트랙 C] 베들레헴 성경 폴더(BethlehemWin)의 .bdb 번역본 찾기·한꺼번에 가져오기 (Electron bethlehem.js + core findBethlehem·importBethlehem)

use crate::core::Core;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub const SOURCE: &str = "베들레헴 성경 (bethlehembible.org) 데이터 파일";

/// 알려진 번역본: 파일 이름 → (id, 이름, 언어, 저작권, 라이선스)
const KNOWN: &[(&str, &str, &str, &str, &str, &str)] = &[
    ("개역한글.bdb", "krv", "개역한글", "ko", "대한성서공회 (저작재산권 보호기간 만료)", "공공 영역"),
    ("개역개정.bdb", "nkrv", "개역개정", "ko", "© 대한성서공회", "대한성서공회 이용 조건에 따름"),
    ("쉬운성경.bdb", "easy", "쉬운성경", "ko", "© 아가페출판사", "권리자 이용 조건에 따름"),
    ("우리말.bdb", "woorimal", "우리말성경", "ko", "© 두란노서원", "권리자 이용 조건에 따름"),
    ("현대인.bdb", "klb", "현대인의성경", "ko", "© 생명의말씀사", "권리자 이용 조건에 따름"),
    ("공동번역.bdb", "common", "공동번역", "ko", "© 대한성서공회", "대한성서공회 이용 조건에 따름"),
    ("ESV.bdb", "esv", "ESV", "en", "© Crossway", "Crossway 이용 조건에 따름"),
    ("NIV2011.bdb", "niv2011", "NIV (2011)", "en", "© Biblica, Inc.", "Biblica 이용 조건에 따름"),
    ("KJV.bdb", "kjv", "KJV", "en", "Public Domain", "공공 영역"),
];

fn has_bdb(d: &Path) -> bool {
    std::fs::read_dir(d).map(|it| it.flatten().any(|e| e.file_name().to_string_lossy().to_lowercase().ends_with(".bdb"))).unwrap_or(false)
}

/// 흔한 설치 위치에서 BethlehemWin 폴더 찾기
pub fn find_dirs(extra: &[PathBuf]) -> Vec<PathBuf> {
    let mut c: Vec<PathBuf> = extra.to_vec();
    for d in "CDEFGHIJ".chars() {
        c.push(PathBuf::from(format!("{d}:\\BethlehemWin")));
        c.push(PathBuf::from(format!("{d}:\\베들레헴\\BethlehemWin")));
    }
    if let Some(home) = std::env::var_os("USERPROFILE") {
        for sub in ["Desktop", "Documents", "Downloads", "바탕 화면"] {
            c.push(PathBuf::from(&home).join(sub).join("BethlehemWin"));
        }
    }
    let mut seen = std::collections::HashSet::new();
    c.into_iter().filter(|d| seen.insert(d.to_string_lossy().to_lowercase()) && d.is_dir() && has_bdb(d)).collect()
}

fn fallback_id(base: &str) -> String {
    let id: String = base.to_lowercase().chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-').collect();
    if !id.is_empty() { return id; }
    let hex: String = base.as_bytes().iter().map(|b| format!("{b:02x}")).collect();
    format!("b{}", &hex[..hex.len().min(10)])
}

/// 이 파일의 번역본 정보 (알려진 것이면 그대로, 아니면 파일 이름으로)
pub fn meta_for(file: &str) -> Value {
    if let Some(k) = KNOWN.iter().find(|k| k.0 == file) {
        return json!({ "id": k.1, "name": k.2, "language": k.3, "copyright": k.4, "license": k.5, "source": SOURCE });
    }
    let base = file.strip_suffix(".bdb").or(file.strip_suffix(".BDB")).unwrap_or(file);
    let ko = base.chars().any(|c| ('가'..='힣').contains(&c));
    json!({ "id": fallback_id(base), "name": base, "language": if ko { "ko" } else { "en" }, "copyright": "", "license": "베들레헴 성경 데이터 (권리자 이용 조건에 따름)", "source": SOURCE })
}

/// 폴더 안의 .bdb 목록 (알려진 번역본 먼저)
pub fn list_bibles(dir: &Path) -> Vec<Value> {
    let mut v: Vec<Value> = std::fs::read_dir(dir)
        .map(|it| it.flatten().filter_map(|e| {
            let f = e.file_name().to_string_lossy().to_string();
            if !f.to_lowercase().ends_with(".bdb") { return None; }
            let m = meta_for(&f);
            let known = KNOWN.iter().any(|k| k.0 == f);
            Some(json!({ "file": f, "known": known, "id": m["id"], "name": m["name"], "size": e.metadata().map(|m| m.len()).unwrap_or(0) }))
        }).collect())
        .unwrap_or_default();
    v.sort_by(|a, b| b["known"].as_bool().cmp(&a["known"].as_bool()).then_with(|| a["name"].as_str().cmp(&b["name"].as_str())));
    v
}

/// 고른 파일들을 한꺼번에 가져온다 → 파일마다 결과
pub fn import_files(core: &mut Core, dir: &Path, files: &[String]) -> Vec<Value> {
    let list = list_bibles(dir);
    let mut out = vec![];
    for f in files {
        if !list.iter().any(|x| x["file"] == f.as_str()) {
            out.push(json!({ "file": f, "ok": false, "error": "파일 없음" }));
            continue;
        }
        let a = crate::importer::analyze_file(&dir.join(f));
        match crate::importer::commit(core, &a, &meta_for(f)) {
            Ok(n) => out.push(json!({ "file": f, "ok": true, "verses": n })),
            Err(e) => out.push(json!({ "file": f, "ok": false, "error": e.into_iter().take(2).collect::<Vec<_>>().join(" / ") })),
        }
    }
    // 쓰던 번역본이 없으면 개역개정(있으면) 또는 처음 가져온 것
    let ok: Vec<String> = out.iter().filter(|r| r["ok"] == json!(true)).filter_map(|r| meta_for(r["file"].as_str()?)["id"].as_str().map(String::from)).collect();
    if !ok.is_empty() && !core.store.has_version(&core.version_id()) {
        let id = if ok.iter().any(|x| x == "nkrv") { "nkrv".to_string() } else { ok[0].clone() };
        core.set_version(&id);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn meta() {
        assert_eq!(meta_for("개역개정.bdb")["id"], json!("nkrv"));
        assert_eq!(meta_for("새번역.bdb")["language"], json!("ko"));
        assert!(meta_for("새번역.bdb")["id"].as_str().unwrap().starts_with('b'));
        assert_eq!(meta_for("NASB.bdb")["id"], json!("nasb"));
    }
}

#[cfg(test)]
mod real {
    /// WP_DATA(임시 데이터 폴더)·WP_BETH(BethlehemWin) 이 있을 때만: cargo test -p wp-core real_beth -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_beth() {
        let (Ok(d), Ok(b)) = (std::env::var("WP_DATA"), std::env::var("WP_BETH")) else { return };
        let mut core = crate::core::Core::open(d.into(), std::env::temp_dir(), None).unwrap();
        let dirs = super::find_dirs(&[]);
        println!("찾은 폴더: {:?}", dirs);
        let r = super::import_files(&mut core, std::path::Path::new(&b), &["개역한글.bdb".into(), "없는파일.bdb".into()]);
        println!("{}", serde_json::to_string(&r).unwrap());
        assert_eq!(r[0]["ok"], serde_json::json!(true));
        assert_eq!(core.store.resolve_reference("krv", "요3:16")["ok"], serde_json::json!(true));
    }
}
