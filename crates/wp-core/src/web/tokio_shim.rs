//! [웹판] tokio 에서 브라우저에 없는 것(fs · net)만 메운 모듈. 나머지는 진짜 tokio 를 그대로 내보낸다.
//! 쓰는 법: 서버 모듈 맨 위에 `#[cfg(target_arch = "wasm32")] use crate::web::tokio_shim as tokio;`
pub use ::tokio::*;

fn unsupported<T>() -> std::io::Result<T> {
    Err(std::io::Error::new(std::io::ErrorKind::Unsupported, "웹판에는 파일·소켓이 없습니다"))
}

pub mod fs {
    use std::path::{Path, PathBuf};
    pub async fn canonicalize(_p: impl AsRef<Path>) -> std::io::Result<PathBuf> {
        super::unsupported()
    }
    pub async fn read(_p: impl AsRef<Path>) -> std::io::Result<Vec<u8>> {
        super::unsupported()
    }
    pub async fn metadata(_p: impl AsRef<Path>) -> std::io::Result<std::fs::Metadata> {
        super::unsupported()
    }
    pub struct File;
    impl File {
        pub async fn open(_p: impl AsRef<Path>) -> std::io::Result<File> {
            super::unsupported()
        }
        pub async fn seek(&mut self, _pos: std::io::SeekFrom) -> std::io::Result<u64> {
            super::unsupported()
        }
        pub async fn read_exact(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
            super::unsupported()
        }
        pub async fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
            super::unsupported()
        }
    }
}

pub mod io {
    pub use ::tokio::io::*;
    /// 자리만 (메서드는 fs::File 에 직접 있다)
    pub trait AsyncReadExt {}
    pub trait AsyncSeekExt {}
}
