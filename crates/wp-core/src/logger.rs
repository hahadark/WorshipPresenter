//! 오류·송출 기록 로그 (Electron logger.js): 데이터 폴더의 logs\YYYY-MM-DD.log, 30일 보관 [트랙 B]
//! log 크레이트(log::info! 등)를 이 파일로 받는다. 송출 기록은 target "live" → [LIVE].
//! 개인정보(가사·본문)는 쓰지 않고, 송출 제목 정도만.

use parking_lot::Mutex;
use std::io::Write;
use std::path::{Path, PathBuf};

struct FileLog {
    dir: PathBuf,
    lock: Mutex<()>,
    echo: bool,
}

/// 지역 시각 "YYYY-MM-DD", "HH:MM:SS"
fn local_stamp() -> (String, String) {
    let ms = crate::util::now_ms() + crate::localtime::local_offset_ms();
    let secs = ms.div_euclid(1000);
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    // 날짜 (civil_from_days)
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    (format!("{y:04}-{m:02}-{d:02}"), format!("{:02}:{:02}:{:02}", rem / 3600, (rem % 3600) / 60, rem % 60))
}

impl log::Log for FileLog {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Info
    }
    fn log(&self, r: &log::Record) {
        if !self.enabled(r.metadata()) {
            return;
        }
        // 다른 크레이트(그래픽 드라이버 등)의 로그는 오류만
        let ours = r.target().starts_with("wp_") || r.target() == "live";
        if !ours && r.level() > log::Level::Error {
            return;
        }
        let level = if r.target() == "live" {
            "LIVE"
        } else {
            match r.level() {
                log::Level::Error => "ERROR",
                log::Level::Warn => "WARN",
                _ => "INFO",
            }
        };
        let (day, time) = local_stamp();
        let line = format!("{day} {time} [{level}] {}\n", r.args());
        // panic hook 이 로그를 쓰는 도중의 panic 에서도 멈추지 않게 (기다림 제한)
        let _g = self.lock.try_lock_for(std::time::Duration::from_millis(300));
        if self.echo {
            eprint!("{line}");
        }
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(self.dir.join(format!("{day}.log"))) {
            let _ = f.write_all(line.as_bytes());
        }
    }
    fn flush(&self) {}
}

/// 로그 시작 (한 번). 30일 넘은 .log 는 지운다
pub fn init(data_dir: &Path) {
    let dir = data_dir.join("logs");
    let _ = std::fs::create_dir_all(&dir);
    let cutoff = std::time::SystemTime::now() - std::time::Duration::from_secs(30 * 86400);
    for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
        let old = e.metadata().and_then(|m| m.modified()).is_ok_and(|t| t < cutoff);
        if old && e.file_name().to_string_lossy().ends_with(".log") {
            let _ = std::fs::remove_file(e.path());
        }
    }
    let echo = cfg!(debug_assertions);
    if log::set_boxed_logger(Box::new(FileLog { dir, lock: Mutex::new(()), echo })).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
}

/// 프로그램 오류(panic)를 logs\ 에 남긴다 (어느 스레드든). 원래 hook(표준 오류 출력)도 그대로 부른다.
/// 조작 화면 프레임의 panic 은 app.rs 가 catch_unwind 로 받아 계속 돌아가므로, 기록이 유일한 흔적이다.
pub fn install_panic_hook() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let th = std::thread::current();
        let name = th.name().unwrap_or("-").to_string();
        let bt = std::backtrace::Backtrace::force_capture().to_string();
        // 같은 자리가 매 프레임 되풀이되면 로그가 커지므로 처음 몇 번만 백트레이스까지
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if n < 5 {
            log::error!("프로그램 오류(panic) [스레드 {name}] {info}\n{}", bt.lines().take(40).collect::<Vec<_>>().join("\n"));
        } else if n < 200 {
            log::error!("프로그램 오류(panic) [스레드 {name}] {info}");
        }
        prev(info);
    }));
}

/// 로그 폴더
pub fn dir(data_dir: &Path) -> PathBuf {
    data_dir.join("logs")
}

/// 최근 days 일 로그 (오래된 것부터), 합쳐서 max_bytes 를 넘으면 앞을 자른다
pub fn recent_logs(dir: &Path, days: usize, max_bytes: usize) -> String {
    let mut files: Vec<String> = match std::fs::read_dir(dir) {
        Ok(rd) => rd.flatten().map(|e| e.file_name().to_string_lossy().to_string()).filter(|f| f.len() == 14 && f.ends_with(".log") && f.as_bytes()[4] == b'-').collect(),
        Err(_) => return "(로그 폴더를 읽지 못했습니다)".into(),
    };
    files.sort();
    let start = files.len().saturating_sub(days);
    let text = files[start..]
        .iter()
        .map(|f| match std::fs::read_to_string(dir.join(f)) {
            Ok(t) => format!("===== {f} =====\n{t}"),
            Err(_) => format!("===== {f} (읽기 실패) =====\n"),
        })
        .collect::<Vec<_>>()
        .join("\n");
    if text.is_empty() {
        return "(로그 없음)".into();
    }
    if text.len() > max_bytes {
        let keep = max_bytes / 2;
        let mut cut = text.len() - keep;
        while !text.is_char_boundary(cut) {
            cut += 1;
        }
        return format!("(앞부분 생략)\n{}", &text[cut..]);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stamp_and_recent() {
        let (d, t) = local_stamp();
        assert_eq!((d.len(), t.len()), (10, 8));
        let dir = std::env::temp_dir().join(format!("wp-logs-{}", crate::util::now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("2026-11-10.log"), "가\n").unwrap();
        std::fs::write(dir.join("2026-11-11.log"), "나".repeat(1000)).unwrap();
        std::fs::write(dir.join("other.txt"), "x").unwrap();
        let r = recent_logs(&dir, 7, 100_000);
        assert!(r.starts_with("===== 2026-11-10.log"));
        assert!(!r.contains("other"));
        assert!(recent_logs(&dir, 1, 100_000).starts_with("===== 2026-11-11.log"));
        assert!(recent_logs(&dir, 7, 200).starts_with("(앞부분 생략)"));
    }
}
