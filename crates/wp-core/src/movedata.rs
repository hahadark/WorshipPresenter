//! 다른 컴퓨터로 옮기기: 자료 전체(bible.db · settings.json · media 폴더)를 파일 하나(.wpmove, zip)로 내보내고 가져온다.
//! (2026-11-27 예배송출의 '전체 자료를 한 파일로 저장해 새 컴퓨터로 옮기기' 를 참고해 더함, 코드는 새로 씀)
//!
//! - 내보내기: DB 는 VACUUM INTO 로 일관된 사본(백업과 같은 방법), 코어 잠금 없이 다른 스레드에서.
//! - 가져오기: 바로 바꾸지 않고 백업 되돌리기와 같은 예약(bible.restore.db · settings.restore.json)으로 → 다시 시작할 때 적용.
//!   지금 자료는 bible.pre-restore.db · settings.pre-restore.json 으로 남는다. media 파일은 이름이 같은 것이 없을 때만 넣는다.
//! - 옮겨지지 않는 것: 라이브러리에 '파일 위치'로 넣은 그림·영상(그 PC 의 경로), 찬송가 PPT 폴더, 암호로 저장한 비밀 글(그 PC 에서만 풀림).

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub const EXT: &str = "wpmove";
const FORMAT: &str = "worshippresenter-move";

fn media_files(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut out = vec![];
    let mut stack = vec![(String::new(), dir.to_path_buf())];
    while let Some((rel, d)) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let r = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
            match e.file_type() {
                Ok(t) if t.is_dir() => stack.push((r, e.path())),
                Ok(t) if t.is_file() => out.push((r, e.path())),
                _ => {}
            }
        }
    }
    out.sort();
    out
}

/// 자료 전체 → out 파일 하나. { size, media }
pub fn export_to(data_dir: &Path, out: &Path) -> Result<Value> {
    let _work = crate::sync::file_work().try_lock().context(crate::i18n::tr("백업 또는 복원이 진행 중입니다."))?;
    let tmp_db = std::env::temp_dir().join(format!("wpmove-{}-{}.db", std::process::id(), crate::util::now_ms()));
    let _ = std::fs::remove_file(&tmp_db);
    let result = (|| -> Result<Value> {
        {
            let c = rusqlite::Connection::open_with_flags(data_dir.join("bible.db"), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
            c.busy_timeout(std::time::Duration::from_secs(5))?;
            c.execute("VACUUM INTO ?", [tmp_db.to_string_lossy().to_string()])?;
        }
        let part = out.with_extension(format!("{EXT}.part"));
        let media = media_files(&data_dir.join("media"));
        {
            let f = std::fs::File::create(&part).with_context(|| crate::i18n::tr_args("파일을 만들지 못했습니다: {1}", &[&part.display()]))?;
            let mut z = zip::ZipWriter::new(std::io::BufWriter::new(f));
            let opt = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated).large_file(true);
            let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored).large_file(true);
            z.start_file("manifest.json", opt)?;
            z.write_all(serde_json::to_string_pretty(&json!({ "format": FORMAT, "version": 1, "createdAt": crate::util::iso_now(), "host": crate::sync::hostname(), "app": env!("CARGO_PKG_VERSION"), "media": media.len() }))?.as_bytes())?;
            z.start_file("bible.db", opt)?;
            std::io::copy(&mut std::fs::File::open(&tmp_db)?, &mut z)?;
            let st = data_dir.join("settings.json");
            if st.exists() {
                z.start_file("settings.json", opt)?;
                z.write_all(&std::fs::read(&st)?)?;
            }
            for (rel, p) in &media {
                // 그림·영상은 이미 압축돼 있어 다시 줄이지 않는다
                let packed = matches!(p.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref(), Some("png" | "jpg" | "jpeg" | "webp" | "gif" | "mp4" | "webm" | "mov" | "mp3" | "m4a" | "zip"));
                z.start_file(format!("media/{rel}"), if packed { stored } else { opt })?;
                std::io::copy(&mut std::fs::File::open(p)?, &mut z)?;
            }
            z.finish()?.flush()?;
        }
        let _ = std::fs::remove_file(out);
        std::fs::rename(&part, out)?;
        Ok(json!({ "size": std::fs::metadata(out).map(|m| m.len()).unwrap_or(0), "media": media.len() }))
    })();
    let _ = std::fs::remove_file(&tmp_db);
    if result.is_err() {
        let _ = std::fs::remove_file(out.with_extension(format!("{EXT}.part")));
    }
    result
}

/// zip 안의 media/ 이름 → 자료 폴더 안의 안전한 상대 경로 (.. · 절대 경로 · 드라이브 글자는 버림)
fn safe_media(name: &str) -> Option<PathBuf> {
    let rel = name.strip_prefix("media/")?;
    let mut p = PathBuf::new();
    for c in rel.split(['/', '\\']) {
        if c.is_empty() || c == "." || c == ".." || c.contains(':') {
            return None;
        }
        p.push(c);
    }
    (!p.as_os_str().is_empty()).then_some(p)
}

/// 옮기기 파일 → 다음 시작 때 바꿔 끼울 예약. { media(넣은 수), skipped(이름이 같아 둔 수), restartNeeded }
pub fn import_from(data_dir: &Path, file: &Path) -> Result<Value> {
    let _work = crate::sync::file_work().try_lock().context(crate::i18n::tr("백업 또는 복원이 진행 중입니다."))?;
    let pending = data_dir.join("bible.restore.db");
    if pending.exists() {
        bail!(crate::i18n::tr("이미 예약된 되돌리기가 있습니다. 먼저 예약을 취소하세요."));
    }
    let mut z = zip::ZipArchive::new(std::io::BufReader::new(std::fs::File::open(file)?)).map_err(|_| anyhow::anyhow!(crate::i18n::tr("워십프리젠터 옮기기 파일이 아닙니다.")))?;
    let manifest: Value = {
        let mut s = String::new();
        z.by_name("manifest.json").map_err(|_| anyhow::anyhow!(crate::i18n::tr("워십프리젠터 옮기기 파일이 아닙니다.")))?.take(1 << 20).read_to_string(&mut s)?;
        serde_json::from_str(&s).unwrap_or(Value::Null)
    };
    if manifest["format"] != FORMAT {
        bail!(crate::i18n::tr("워십프리젠터 옮기기 파일이 아닙니다."));
    }
    let tmp = data_dir.join("bible.restore.db.tmp");
    let settings_tmp = data_dir.join("settings.restore.json.tmp");
    let settings_pending = data_dir.join("settings.restore.json");
    let clean = || {
        let _ = std::fs::remove_file(&tmp);
        let _ = std::fs::remove_file(&settings_tmp);
    };
    clean();
    let r = (|| -> Result<Value> {
        std::io::copy(&mut z.by_name("bible.db").map_err(|_| anyhow::anyhow!(crate::i18n::tr("옮기기 파일에 자료(bible.db)가 없습니다.")))?, &mut std::fs::File::create(&tmp)?)?;
        // 망가진 DB 로 바꿔 끼우지 않게 먼저 열어 본다
        {
            let c = rusqlite::Connection::open_with_flags(&tmp, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
            let ok: String = c.query_row("PRAGMA quick_check(1)", [], |r| r.get(0))?;
            if ok != "ok" {
                bail!(crate::i18n::tr("옮기기 파일의 자료가 손상되었습니다."));
            }
        }
        if let Ok(mut s) = z.by_name("settings.json") {
            let mut buf = vec![];
            s.read_to_end(&mut buf)?;
            serde_json::from_slice::<Value>(&buf).map_err(|_| anyhow::anyhow!(crate::i18n::tr("옮기기 파일의 설정이 손상되었습니다.")))?;
            std::fs::write(&settings_tmp, &buf)?;
        }
        // media: 이름이 같은 파일이 이미 있으면 그대로 둔다 (같은 /media/ 주소를 쓰는 지금 자료를 덮지 않게)
        let media_dir = data_dir.join("media");
        let (mut added, mut skipped) = (0usize, 0usize);
        for i in 0..z.len() {
            let mut e = z.by_index(i)?;
            if !e.is_file() {
                continue;
            }
            let Some(rel) = e.name().ok().and_then(|n| safe_media(&n)) else { continue };
            let dst = media_dir.join(&rel);
            if dst.exists() {
                skipped += 1;
                continue;
            }
            if let Some(d) = dst.parent() {
                std::fs::create_dir_all(d)?;
            }
            let part = dst.with_extension("wpmove-part");
            std::io::copy(&mut e, &mut std::fs::File::create(&part)?)?;
            std::fs::rename(&part, &dst)?;
            added += 1;
        }
        // 설정 → DB 차례로 게시 (DB 파일이 예약 완료 표시, sync::restore_files 와 같음)
        if settings_tmp.exists() {
            std::fs::rename(&settings_tmp, &settings_pending)?;
        } else {
            let _ = std::fs::remove_file(&settings_pending);
        }
        std::fs::rename(&tmp, &pending)?;
        Ok(json!({ "media": added, "skipped": skipped, "from": manifest["host"], "createdAt": manifest["createdAt"], "restartNeeded": true }))
    })();
    if r.is_err() {
        clean();
        let _ = std::fs::remove_file(&settings_pending);
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("wp-move-{tag}-{}-{}", std::process::id(), crate::util::now_ms()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// PC1 에서 내보내고 PC2 에서 가져오면: 다시 시작할 때 PC1 의 찬양 · 설정 · media 그림이 PC2 에
    #[test]
    fn export_then_import_on_another_pc() {
        let pc1 = tmpdir("pc1");
        let pc2 = tmpdir("pc2");
        {
            let c = rusqlite::Connection::open(pc1.join("bible.db")).unwrap();
            crate::songs::ensure_schema(&c).unwrap();
            crate::songs::save_song(&c, &json!({ "title": "옮겨 갈 찬양", "lyrics": "가사" })).unwrap();
        }
        std::fs::write(pc1.join("settings.json"), r#"{"churchName":"○○교회"}"#).unwrap();
        std::fs::create_dir_all(pc1.join("media").join("sub")).unwrap();
        std::fs::write(pc1.join("media").join("logo.png"), b"png1").unwrap();
        std::fs::write(pc1.join("media").join("sub").join("bg.jpg"), b"jpg1").unwrap();
        // PC2 에는 같은 이름 그림이 이미 있다 → 덮지 않음
        {
            let c = rusqlite::Connection::open(pc2.join("bible.db")).unwrap();
            crate::songs::ensure_schema(&c).unwrap();
        }
        std::fs::write(pc2.join("settings.json"), r#"{"churchName":"PC2"}"#).unwrap();
        std::fs::create_dir_all(pc2.join("media")).unwrap();
        std::fs::write(pc2.join("media").join("logo.png"), b"pc2-logo").unwrap();

        let file = pc1.join(format!("옮기기.{EXT}"));
        let r = export_to(&pc1, &file).unwrap();
        assert_eq!(r["media"], 2);
        assert!(file.exists() && !pc1.join(format!("옮기기.{EXT}.part")).exists());

        let r = import_from(&pc2, &file).unwrap();
        assert_eq!((r["media"].as_u64(), r["skipped"].as_u64()), (Some(1), Some(1)), "{r}");
        assert_eq!(std::fs::read(pc2.join("media").join("logo.png")).unwrap(), b"pc2-logo", "같은 이름은 그대로");
        assert_eq!(std::fs::read(pc2.join("media").join("sub").join("bg.jpg")).unwrap(), b"jpg1");
        assert!(pc2.join("bible.restore.db").exists() && pc2.join("settings.restore.json").exists());
        // 두 번째 예약은 거절
        assert!(import_from(&pc2, &file).is_err());

        // 다시 시작할 때 적용 (sync::apply_pending_restore)
        assert!(crate::sync::apply_pending_restore(&pc2));
        let c = rusqlite::Connection::open(pc2.join("bible.db")).unwrap();
        assert!(crate::songs::find_by_title(&c, "옮겨 갈 찬양").is_some());
        assert!(std::fs::read_to_string(pc2.join("settings.json")).unwrap().contains("○○교회"));
        assert!(pc2.join("bible.pre-restore.db").exists(), "지금 자료는 남겨 둔다");
        drop(c);
        let _ = std::fs::remove_dir_all(&pc1);
        let _ = std::fs::remove_dir_all(&pc2);
    }

    #[test]
    fn rejects_other_zip_and_bad_names() {
        let d = tmpdir("bad");
        let f = d.join("x.zip");
        {
            let mut z = zip::ZipWriter::new(std::fs::File::create(&f).unwrap());
            z.start_file("manifest.json", zip::write::SimpleFileOptions::default()).unwrap();
            z.write_all(br#"{"format":"other"}"#).unwrap();
            z.finish().unwrap();
        }
        assert!(import_from(&d, &f).is_err());
        assert!(!d.join("bible.restore.db").exists());
        std::fs::write(d.join("y.wpmove"), b"not a zip").unwrap();
        assert!(import_from(&d, &d.join("y.wpmove")).is_err());
        assert_eq!(safe_media("media/a/b.png"), Some(PathBuf::from("a").join("b.png")));
        for bad in ["media/../x", "media/", "media/C:/x", "bible.db", "media/a/../../x"] {
            assert_eq!(safe_media(bad), None, "{bad}");
        }
        let _ = std::fs::remove_dir_all(&d);
    }
}
