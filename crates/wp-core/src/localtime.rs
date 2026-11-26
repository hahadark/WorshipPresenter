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
        // 맥·리눅스: SQLite 의 'localtime' 변환(운영체제 localtime_r)으로 차이를 구한다 (전에는 0 이라 맥에서 예약·시각이 UTC 였다)
        rusqlite::Connection::open_in_memory()
            .and_then(|c| c.query_row("SELECT CAST(strftime('%s','now','localtime') AS INTEGER) - CAST(strftime('%s','now') AS INTEGER)", [], |r| r.get::<_, i64>(0)))
            .map(|s| ((s as f64 / 900.0).round() as i64) * 900_000)
            .unwrap_or(0)
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

/// "11:00" · "11.00" · "1100" · "11시" · "11시 30분" · "오전 9시" · "오후 2:30" → 하루 안의 분 (0~1439)
pub fn parse_hm(s: &str) -> Option<i64> {
    let t: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    let (pm, am, t) = if let Some(r) = t.strip_prefix("오후") { (true, false, r.to_string()) } else if let Some(r) = t.strip_prefix("오전") { (false, true, r.to_string()) } else { (false, false, t) };
    let t = t.trim_end_matches('분');
    let (h, m) = if let Some((h, m)) = t.split_once(|c| c == ':' || c == '.' || c == '시') {
        (h.parse::<i64>().ok()?, if m.is_empty() { 0 } else { m.parse::<i64>().ok()? })
    } else if t.chars().all(|c| c.is_ascii_digit()) && (3..=4).contains(&t.len()) {
        let (h, m) = t.split_at(t.len() - 2);
        (h.parse::<i64>().ok()?, m.parse::<i64>().ok()?)
    } else if t.chars().all(|c| c.is_ascii_digit()) && (1..=2).contains(&t.len()) {
        (t.parse::<i64>().ok()?, 0)
    } else {
        return None;
    };
    let h = if pm && h < 12 { h + 12 } else if am && h == 12 { 0 } else { h };
    ((0..24).contains(&h) && (0..60).contains(&m)).then_some(h * 60 + m)
}

/// 오늘 그 시각까지 남은 밀리초 (local_now_ms = UTC ms + 시간대). 이미 지났으면 None
pub fn ms_until(hm_min: i64, local_now_ms: i64) -> Option<i64> {
    let day = local_now_ms.div_euclid(86_400_000) * 86_400_000;
    let left = day + hm_min * 60_000 - local_now_ms;
    (left > 0).then_some(left)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hm_parse_and_until() {
        for (s, m) in [("11:00", 660), ("9:05", 545), ("11.30", 690), ("1100", 660), ("930", 570), ("11시", 660), ("11시 30분", 690), ("오후 2:30", 870), ("오후2시", 840), ("오전 12시", 0), ("23:59", 1439), ("7", 420)] {
            assert_eq!(parse_hm(s), Some(m), "{s}");
        }
        for s in ["", "25:00", "11:60", "열한시", "11:3x", "12345"] {
            assert_eq!(parse_hm(s), None, "{s}");
        }
        // 2026-11-29 10:50:30 (지역) → 11:00 까지 9분 30초, 10:00 은 이미 지남
        let now = 20786 * 86_400_000 + (10 * 60 + 50) * 60_000 + 30_000;
        assert_eq!(ms_until(660, now), Some(570_000));
        assert_eq!(ms_until(600, now), None);
        assert_eq!(ms_until(650, now), None, "같은 분이라도 지났으면 없음");
    }

    #[test]
    fn local_offset_is_quarter_hours() {
        let o = local_offset_ms();
        assert_eq!(o % 900_000, 0);
        assert!(o.abs() <= 14 * 3_600_000);
    }
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
