//! 작은 도우미

use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// ISO 8601 UTC 시각 (JS new Date().toISOString() 과 같은 모양)
pub fn iso_now() -> String {
    let ms = now_ms();
    let secs = ms.div_euclid(1000);
    let milli = ms.rem_euclid(1000);
    let days = secs.div_euclid(86400);
    let rem = secs.rem_euclid(86400);
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // 1970-01-01 기준 날짜 계산 (Howard Hinnant civil_from_days)
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z", y, mo, d, h, m, s, milli)
}

/// 짧은 무작위 id (Electron 판과 같은 base36 모양)
pub fn short_id(prefix: &str) -> String {
    let n = now_ms() as u64;
    let r: u32 = rand::random::<u32>();
    format!("{}{}{}", prefix, to36(n), &to36(r as u64)[..3.min(to36(r as u64).len())])
}

pub fn to36(mut n: u64) -> String {
    const D: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if n == 0 {
        return "0".into();
    }
    let mut v = vec![];
    while n > 0 {
        v.push(D[(n % 36) as usize]);
        n /= 36;
    }
    v.reverse();
    String::from_utf8(v).unwrap()
}

/// 이 프로세스의 메모리 (작업 집합 MB, 전용 MB). 윈도우가 아니면 (0, 0)
pub fn mem_mb() -> (f64, f64) {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX};
        let mut c: PROCESS_MEMORY_COUNTERS_EX = std::mem::zeroed();
        c.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
        if K32GetProcessMemoryInfo(windows_sys::Win32::System::Threading::GetCurrentProcess(), &mut c as *mut _ as *mut PROCESS_MEMORY_COUNTERS, c.cb) != 0 {
            return (c.WorkingSetSize as f64 / 1048576.0, c.PrivateUsage as f64 / 1048576.0);
        }
    }
    (0.0, 0.0)
}

/// 점검용: WP_DEBUG_MEM=1 이면 단계 이름과 메모리를 로그에 (무엇이 메모리를 차지하는지 찾기)
pub fn mem_log(stage: &str) {
    static ON: once_cell::sync::Lazy<bool> = once_cell::sync::Lazy::new(|| std::env::var_os("WP_DEBUG_MEM").is_some());
    if *ON {
        let (ws, pv) = mem_mb();
        log::info!("메모리 [{stage}] 작업 집합 {ws:.0}MB · 전용 {pv:.0}MB");
    }
}
