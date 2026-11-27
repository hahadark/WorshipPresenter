//! 유튜브 주소 재생 (예배송출의 '유튜브 주소 재생' 을 참고해 새로 씀, API 키 없음).
//! 영상 순서(video)의 file 에 유튜브 주소를 넣으면 슬라이드에 `youtube: { id, start }` 가 붙고,
//! 출력 화면(render.js · output.js)은 <video> 대신 유튜브 임베드(youtube-nocookie.com)를 그린다.
//! 재생 · 멈춤 · ±10초 · 소리 · 끝나면은 보통 영상과 같은 코어 상태(trackb.rs video)를 따르므로 예배당 · OBS 화면이 함께 움직인다.
//! 인터넷이 있어야 하고, 올린 사람이 '퍼가기' 를 막은 영상은 재생되지 않는다.

/// 유튜브 영상 id (11자: 영문 · 숫자 · - · _)
fn valid_id(s: &str) -> bool {
    s.len() == 11 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// "90" · "1m30s" · "1h2m3s" · "2:30" → 초
fn parse_time(t: &str) -> Option<u32> {
    let t = t.trim().trim_end_matches('s');
    if t.is_empty() {
        return None;
    }
    if let Ok(n) = t.parse::<u32>() {
        return Some(n);
    }
    if t.contains(':') {
        let mut sec = 0u32;
        for p in t.split(':') {
            sec = sec.checked_mul(60)?.checked_add(p.parse().ok()?)?;
        }
        return Some(sec);
    }
    let (mut sec, mut num) = (0u32, String::new());
    for c in t.chars().chain(std::iter::once('s')) {
        if c.is_ascii_digit() {
            num.push(c);
            continue;
        }
        let n: u32 = if num.is_empty() { return None } else { num.parse().ok()? };
        num.clear();
        sec += n * match c {
            'h' => 3600,
            'm' => 60,
            's' => 1,
            _ => return None,
        };
    }
    Some(sec)
}

/// 유튜브 주소 → (영상 id, 시작 초). 유튜브 주소가 아니면 None.
/// youtube.com/watch?v= · youtu.be/ · /shorts/ · /live/ · /embed/ · m. · music. · youtube-nocookie.com, t= · start= 시작 시각
pub fn parse(url: &str) -> Option<(String, u32)> {
    let u = url.trim();
    let rest = u.strip_prefix("https://").or_else(|| u.strip_prefix("http://")).unwrap_or(u);
    let (host, path) = rest.split_once('/').unwrap_or((rest, ""));
    let host = host.to_ascii_lowercase();
    let host = host.strip_prefix("www.").or_else(|| host.strip_prefix("m.")).or_else(|| host.strip_prefix("music.")).unwrap_or(&host);
    let (path, query) = path.split_once('?').map(|(a, b)| (a, b)).unwrap_or((path, ""));
    let query = query.split('#').next().unwrap_or("");
    let q = |k: &str| query.split('&').find_map(|kv| kv.split_once('=').filter(|(a, _)| *a == k).map(|(_, v)| v.to_string()));
    let id = match host {
        "youtu.be" => path.split('/').next().map(String::from),
        "youtube.com" | "youtube-nocookie.com" => {
            let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
            match segs.as_slice() {
                ["watch", ..] => q("v"),
                ["shorts" | "live" | "embed" | "v", id, ..] => Some(id.to_string()),
                _ => None,
            }
        }
        _ => None,
    }?;
    if !valid_id(&id) {
        return None;
    }
    let start = q("t").or_else(|| q("start")).and_then(|t| parse_time(&t)).unwrap_or(0);
    Some((id, start))
}

pub fn is_youtube(url: &str) -> bool {
    parse(url).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn urls() {
        let id = "dQw4w9WgXcQ";
        for u in [
            format!("https://www.youtube.com/watch?v={id}"),
            format!("youtube.com/watch?feature=share&v={id}"),
            format!("https://youtu.be/{id}"),
            format!("https://m.youtube.com/shorts/{id}?si=abc"),
            format!("https://www.youtube.com/live/{id}"),
            format!("https://www.youtube-nocookie.com/embed/{id}"),
            format!("https://music.youtube.com/watch?v={id}&list=x"),
        ] {
            assert_eq!(parse(&u), Some((id.to_string(), 0)), "{u}");
        }
        assert_eq!(parse(&format!("https://youtu.be/{id}?t=90")).unwrap().1, 90);
        assert_eq!(parse(&format!("https://www.youtube.com/watch?v={id}&t=1m30s")).unwrap().1, 90);
        assert_eq!(parse(&format!("https://www.youtube.com/embed/{id}?start=75")).unwrap().1, 75);
        assert_eq!(parse_time("1h2m3s"), Some(3723));
        assert_eq!(parse_time("2:30"), Some(150));
        assert!(parse("https://example.com/watch?v=dQw4w9WgXcQ").is_none());
        assert!(parse("https://www.youtube.com/watch?v=short").is_none());
        assert!(parse("file:///C:/영상/a.mp4").is_none());
        assert!(parse("https://www.youtube.com/@channel").is_none());
    }
}
