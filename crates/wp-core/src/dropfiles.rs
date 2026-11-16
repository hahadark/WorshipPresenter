//! 탐색기에서 끌어다 놓은(또는 복사해 붙인) 파일·폴더 분류와 큐시트 순서 만들기
//! (Electron control/dropfiles.js addPaths · core addDroppedFiles 와 같은 규칙)
use crate::library::{file_url, is_audio, is_image, is_video};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// pptx · ppt · pps · ppsx → 찬송가·PPT 순서
    Ppt,
    Image,
    Video,
    Audio,
    /// txt → 찬양(가사) 순서
    Text,
    Folder,
    Missing,
    Other,
}

#[derive(Clone, Debug)]
pub struct Dropped {
    pub kind: Kind,
    pub path: PathBuf,
    /// 파일 이름 (확장자 포함)
    pub name: String,
    /// 확장자 뺀 이름
    pub title: String,
}

fn ext(p: &Path) -> String {
    p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase()
}

pub fn classify(p: &Path) -> Kind {
    if !p.exists() {
        return Kind::Missing;
    }
    if p.is_dir() {
        return Kind::Folder;
    }
    match ext(p).as_str() {
        "pptx" | "ppt" | "pps" | "ppsx" => Kind::Ppt,
        "txt" => Kind::Text,
        _ if is_video(p) => Kind::Video,
        _ if is_image(p) => Kind::Image,
        _ if is_audio(p) => Kind::Audio,
        _ => Kind::Other,
    }
}

pub fn describe(paths: &[PathBuf]) -> Vec<Dropped> {
    paths
        .iter()
        .map(|p| Dropped {
            kind: classify(p),
            path: p.clone(),
            name: p.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_else(|| p.to_string_lossy().to_string()),
            title: p.file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_default(),
        })
        .collect()
}

pub fn is_media(k: Kind) -> bool {
    matches!(k, Kind::Image | Kind::Video)
}

/// 큐시트에 넣을 수 있는 것 (ppt · 그림 · 영상 · 음악 · txt)
pub fn is_cue_kind(k: Kind) -> bool {
    matches!(k, Kind::Ppt | Kind::Image | Kind::Video | Kind::Audio | Kind::Text)
}

/// 그림·영상의 화면 주소 (/lib/file?p=…) — 설정의 droppedFiles 에 등록돼 있어야 예배당 화면에 보인다
pub fn url_of(d: &Dropped) -> String {
    file_url(&d.path)
}

/// 파일 하나 → 큐시트 순서 (addPaths 의 addItem 들). 넣을 수 없는 종류는 None
pub fn cue_item(d: &Dropped) -> Option<Value> {
    let mut it = match d.kind {
        Kind::Ppt => {
            let (number, title, _) = crate::hymns::parse_name(&d.path);
            crate::hymns::new_hymn_item(&crate::hymns::HymnFile { file: d.path.clone(), number, title: if title.is_empty() { d.title.clone() } else { title }, name: d.title.clone(), folder: String::new() })
        }
        Kind::Image => {
            let mut it = crate::core::new_item("image", &d.title);
            it["image"] = json!(url_of(d));
            it
        }
        Kind::Video => {
            let mut it = crate::core::new_item("video", &d.title);
            it["file"] = json!(url_of(d));
            it["sound"] = json!(true);
            it["volume"] = json!(1);
            it["endAction"] = json!("hold");
            it
        }
        Kind::Audio => {
            let mut it = crate::core::new_item("audio", &d.title);
            it["file"] = json!(url_of(d));
            it
        }
        Kind::Text => {
            let bytes = std::fs::read(&d.path).unwrap_or_default();
            let text = String::from_utf8_lossy(&bytes).replace('\r', "").trim_start_matches('\u{feff}').trim().to_string();
            let mut it = crate::core::new_item("song", &d.title);
            it["lyrics"] = json!(text);
            it
        }
        _ => return None,
    };
    if it["title"].as_str().unwrap_or("").is_empty() {
        it["title"] = json!(d.title);
    }
    Some(it)
}

/// 넣을 수 없는 파일 이름 모음 (그 밖 종류 · 없는 파일)
pub fn rejected(list: &[Dropped]) -> Vec<String> {
    list.iter().filter(|d| matches!(d.kind, Kind::Other | Kind::Missing)).map(|d| d.name.clone()).collect()
}

/// 끌고 있을 때의 안내 글 (dropfiles.js hint) — where: "slideedit" | "lib" | "hymns" | "songs" | "bg" | "cue"
pub fn hint(where_: &str) -> &'static str {
    match where_ {
        "slideedit" => "슬라이드에 그림·영상 덧붙이기",
        "lib" => "폴더 → 라이브러리에 등록 · 파일 → 큐시트에 넣기",
        "hymns" => "폴더 → 찬송가·PPT 폴더로 등록 · 파일 → 큐시트에 넣기",
        "songs" => "찬양 파일·폴더 가져오기 (OpenLP · ChordPro · ProPresenter · PPT · txt)",
        "bg" => "그림·영상 → 이 순서 배경 · 그 밖의 파일 → 큐시트에 넣기",
        _ => "PPT·그림·영상·음악·가사(txt) → 큐시트에 넣기",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        let d = std::env::temp_dir().join(format!("wp-drop-{}-{}", std::process::id(), crate::util::short_id("d")));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn classifies_by_extension() {
        let d = tmp();
        for (n, k) in [("a.pptx", Kind::Ppt), ("a.PPS", Kind::Ppt), ("a.ppt", Kind::Ppt), ("a.png", Kind::Image), ("a.mp4", Kind::Video), ("a.mp3", Kind::Audio), ("a.txt", Kind::Text), ("a.zip", Kind::Other)] {
            std::fs::write(d.join(n), "x").unwrap();
            assert_eq!(classify(&d.join(n)), k, "{n}");
        }
        assert_eq!(classify(&d), Kind::Folder);
        assert_eq!(classify(&d.join("없음.png")), Kind::Missing);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn cue_items_follow_dropfiles_js() {
        let d = tmp();
        std::fs::write(d.join("010 전능왕 오셔서.pps"), "x").unwrap();
        std::fs::write(d.join("a.png"), "x").unwrap();
        std::fs::write(d.join("b.mp4"), "x").unwrap();
        std::fs::write(d.join("c.mp3"), "x").unwrap();
        std::fs::write(d.join("d.txt"), "\u{feff}1절\r\n주님\r\n").unwrap();
        std::fs::write(d.join("e.zip"), "x").unwrap();
        let list = describe(&[d.join("010 전능왕 오셔서.pps"), d.join("a.png"), d.join("b.mp4"), d.join("c.mp3"), d.join("d.txt"), d.join("e.zip")]);
        let items: Vec<Option<Value>> = list.iter().map(cue_item).collect();
        assert_eq!(items[0].as_ref().unwrap()["type"], "hymn");
        assert_eq!(items[0].as_ref().unwrap()["number"], 10);
        assert!(items[0].as_ref().unwrap()["title"].as_str().unwrap().contains("전능왕"));
        let img = items[1].as_ref().unwrap();
        assert_eq!((img["type"].as_str(), img["title"].as_str()), (Some("image"), Some("a")));
        assert!(img["image"].as_str().unwrap().starts_with("/lib/file?p="));
        let v = items[2].as_ref().unwrap();
        assert_eq!((v["type"].as_str(), v["sound"].as_bool(), v["endAction"].as_str()), (Some("video"), Some(true), Some("hold")));
        assert_eq!(items[3].as_ref().unwrap()["type"], "audio");
        let s = items[4].as_ref().unwrap();
        assert_eq!((s["type"].as_str(), s["lyrics"].as_str()), (Some("song"), Some("1절\n주님")));
        assert!(items[5].is_none());
        assert_eq!(rejected(&list), vec!["e.zip".to_string()]);
        let _ = std::fs::remove_dir_all(&d);
    }
}