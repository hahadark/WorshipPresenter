//! 시스템 시간대 (예약 실행 · 진행 시계). 윈도우는 GetLocalTime − GetSystemTime 차이로 구한다. [트랙 B]

/// 지금 시스템 시간대의 UTC 와의 차이 (밀리초, 서울 = +9시간)
pub fn local_offset_ms() -> i64 {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::SYSTEMTIME;
        use windows_sys::Win32::System::SystemInformation::{GetLocalTime, GetSystemTime};
        let mut l: SYSTEMTIME = unsafe { std::mem::zeroed() };
        let mut u: SYSTEMTIME = unsafe { std::mem::zeroed() };
        unsafe {
            GetSystemTime(&mut u);
            GetLocalTime(&mut l);
        }
        let mins = |t: &SYSTEMTIME| days_from_civil(t.wYear as i64, t.wMonth as i64, t.wDay as i64) * 1440 + t.wHour as i64 * 60 + t.wMinute as i64;
        // 두 호출 사이에 분이 넘어가도 15분 단위로 맞춘다 (시간대는 15분 배수)
        let d = mins(&l) - mins(&u);
        ((d as f64 / 15.0).round() as i64) * 15 * 60_000
    }
    #[cfg(not(windows))]
    {
        0
    }
}

/// 1970-01-01 부터의 날 수 (Howard Hinnant days_from_civil)
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// 지역 시각: (요일 0=일, "HH:MM", 날짜 키)
pub fn local_now() -> (u32, String, String) {
    split(crate::util::now_ms() + local_offset_ms())
}

pub fn split(local_ms: i64) -> (u32, String, String) {
    let mins = local_ms.div_euclid(60000);
    let days = mins.div_euclid(1440);
    let wd = (days + 4).rem_euclid(7) as u32; // 1970-01-01 은 목요일
    (wd, format!("{:02}:{:02}", mins.rem_euclid(1440) / 60, mins % 60), days.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn civil() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2026, 11, 12), 20769);
        // 2026-11-12 (목) 13:05
        let (wd, hm, _) = split(20769 * 86_400_000 + (13 * 60 + 5) * 60_000);
        assert_eq!((wd, hm.as_str()), (4, "13:05"));
        let off = local_offset_ms();
        assert!(off.abs() <= 14 * 3_600_000 && off % 900_000 == 0);
    }
}
