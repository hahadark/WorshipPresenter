//! [웹판] rfd::FileDialog 와 같은 모양. 브라우저는 '고를 때까지 기다리는' 대화상자를 줄 수 없어서 항상 None 을 돌려준다.
//! 파일을 받는 길은 끌어다 놓기(조작 화면 dnd.rs)와 웹 전용 파일 고르기(wp-app web 모듈)로 따로 둔다.
use std::path::{Path, PathBuf};

thread_local! {
    /// 마지막으로 열려던 대화상자 (조작 화면이 '웹판에서는 끌어다 놓으세요' 안내를 띄우는 데 쓴다)
    static LAST: std::cell::RefCell<Option<Asked>> = const { std::cell::RefCell::new(None) };
}

#[derive(Clone, Debug, Default)]
pub struct Asked {
    pub kind: &'static str, // "file" | "files" | "folder" | "save"
    pub title: String,
    pub exts: Vec<String>,
    pub file_name: String,
}

/// 조작 화면이 한 번 가져간다
pub fn take_asked() -> Option<Asked> {
    LAST.with(|l| l.borrow_mut().take())
}

#[derive(Default, Clone, Debug)]
pub struct FileDialog {
    title: String,
    exts: Vec<String>,
    file_name: String,
}

impl FileDialog {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn add_filter(mut self, _name: impl Into<String>, exts: &[impl ToString]) -> Self {
        self.exts.extend(exts.iter().map(|e| e.to_string()));
        self
    }
    pub fn set_title(mut self, t: impl Into<String>) -> Self {
        self.title = t.into();
        self
    }
    pub fn set_file_name(mut self, n: impl Into<String>) -> Self {
        self.file_name = n.into();
        self
    }
    pub fn set_directory(self, _p: impl AsRef<Path>) -> Self {
        self
    }
    fn ask(self, kind: &'static str) {
        LAST.with(|l| *l.borrow_mut() = Some(Asked { kind, title: self.title, exts: self.exts, file_name: self.file_name }));
    }
    pub fn pick_file(self) -> Option<PathBuf> {
        self.ask("file");
        None
    }
    pub fn pick_files(self) -> Option<Vec<PathBuf>> {
        self.ask("files");
        None
    }
    pub fn pick_folder(self) -> Option<PathBuf> {
        self.ask("folder");
        None
    }
    pub fn save_file(self) -> Option<PathBuf> {
        self.ask("save");
        None
    }
}
