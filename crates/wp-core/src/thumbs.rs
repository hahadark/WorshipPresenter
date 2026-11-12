//! 라이브러리 썸네일 (그림 · 영상) — 데이터 폴더 thumbs\ 에 JPEG 로 캐시 [트랙 B]
//! 캐시 이름은 원본 경로 · 크기 · 고친 시각으로 정한다 (원본이 바뀌면 새로 만든다).
//! 그림은 image 크레이트로, 영상은 ffmpeg 가 있으면 1초 자리 한 장 (없으면 만들지 않는다).
//! 만드는 일은 오래 걸릴 수 있으니 조작 화면이 백그라운드 스레드에서 부른다.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

pub const THUMB_W: u32 = 320;
pub const THUMB_H: u32 = 180;

fn fnv(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// 캐시 파일 자리 (원본이 없으면 None)
pub fn thumb_path(data_dir: &Path, src: &Path) -> Option<PathBuf> {
    let m = std::fs::metadata(src).ok()?;
    let mt = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_nanos()).unwrap_or(0);
    let key = format!("v3|{}|{}|{}", src.to_string_lossy().to_lowercase(), m.len(), mt); // 방향과 짧은 간격의 파일 수정 반영
    Some(data_dir.join("thumbs").join(format!("{:016x}.jpg", fnv(&key))))
}

/// 이미 만든 썸네일이 있으면 그 자리
pub fn cached(data_dir: &Path, src: &Path) -> Option<PathBuf> {
    thumb_path(data_dir, src).filter(|p| p.exists())
}

fn is_video(p: &Path) -> bool {
    crate::library::is_video(p)
}

/// ffmpeg 위치 (PATH 또는 winget 링크)
pub fn ffmpeg() -> Option<PathBuf> {
    let exe = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    if let Some(la) = std::env::var_os("LOCALAPPDATA") {
        dirs.push(PathBuf::from(la).join("Microsoft").join("WinGet").join("Links"));
    }
    dirs.into_iter().map(|d| d.join(exe)).find(|p| p.is_file())
}

/// 썸네일 만들기 → 캐시 자리
pub fn make(data_dir: &Path, src: &Path) -> Result<PathBuf> {
    let out = thumb_path(data_dir, src).context("원본 파일이 없습니다.")?;
    if out.exists() {
        return Ok(out);
    }
    std::fs::create_dir_all(out.parent().unwrap())?;
    let tmp = out.with_extension("tmp.jpg");
    if is_video(src) {
        let Some(ff) = ffmpeg() else { bail!("영상 썸네일을 만들려면 ffmpeg 가 필요합니다.") };
        let mut cmd = std::process::Command::new(ff);
        cmd.args(["-y", "-loglevel", "error", "-ss", "1", "-i"]).arg(src).args(["-frames:v", "1", "-vf", &format!("scale={THUMB_W}:{THUMB_H}:force_original_aspect_ratio=decrease"), "-q:v", "4"]).arg(&tmp);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // 콘솔 창 없이
        }
        let st = cmd.status()?;
        if !st.success() || !tmp.exists() {
            // 1초보다 짧은 영상: 첫 장면
            let _ = std::fs::remove_file(&tmp);
            let mut c2 = std::process::Command::new(ffmpeg().unwrap());
            c2.args(["-y", "-loglevel", "error", "-i"]).arg(src).args(["-frames:v", "1", "-vf", &format!("scale={THUMB_W}:{THUMB_H}:force_original_aspect_ratio=decrease")]).arg(&tmp);
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                c2.creation_flags(0x0800_0000);
            }
            if !c2.status()?.success() || !tmp.exists() {
                bail!("영상에서 장면을 꺼내지 못했습니다.");
            }
        }
    } else {
        // 사진기 세로 사진: EXIF 방향대로 돌린다 (출력 화면의 브라우저도 그렇게 보인다)
        use image::ImageDecoder;
        let mut dec = image::ImageReader::open(src)?.with_guessed_format()?.into_decoder().context("그림을 읽지 못했습니다.")?;
        let orient = dec.orientation().unwrap_or(image::metadata::Orientation::NoTransforms);
        let mut img = image::DynamicImage::from_decoder(dec).context("그림을 읽지 못했습니다.")?;
        img.apply_orientation(orient);
        let t = img.thumbnail(THUMB_W, THUMB_H).to_rgb8();
        t.save_with_format(&tmp, image::ImageFormat::Jpeg)?;
    }
    std::fs::rename(&tmp, &out)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn image_thumb_cached() {
        let dir = std::env::temp_dir().join(format!("wp-thumb-{}-{}", crate::util::now_ms(), rand::random::<u32>()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("큰 사진.png");
        image::RgbImage::from_fn(1600, 900, |x, y| image::Rgb([(x % 256) as u8, (y % 256) as u8, 128])).save(&src).unwrap();
        assert!(cached(&dir, &src).is_none());
        let t = make(&dir, &src).unwrap();
        let im = image::open(&t).unwrap();
        assert_eq!((im.width(), im.height()), (320, 180));
        assert_eq!(cached(&dir, &src), Some(t.clone()));
        // 원본이 바뀌면 다른 자리
        std::thread::sleep(std::time::Duration::from_millis(20));
        image::RgbImage::from_fn(800, 800, |_, _| image::Rgb([0, 0, 0])).save(&src).unwrap();
        assert!(cached(&dir, &src).is_none());
        let square = make(&dir, &src).unwrap();
        let portrait = image::open(square).unwrap();
        assert_eq!((portrait.width(), portrait.height()), (180, 180));
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(thumb_path(&dir, &dir.join("없는 파일.png")).is_none());
    }
}
