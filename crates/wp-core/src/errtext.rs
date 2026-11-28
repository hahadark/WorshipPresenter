//! 사용자에게 보이는 오류 글 다듬기: 운영체제 · 라이브러리가 만든 날것의 오류("(os error 2)", "Connection refused" …)를
//! "무슨 일인지 + 어떻게 하면 되는지" 로. 상태 줄 알림(Core::notify)이 모두 거친다. 원래 글은 로그에 그대로 남긴다.

/// (찾을 글 — 소문자로 비교, 무슨 일, 어떻게 하면 되는지)
const KNOWN: &[(&[&str], &str, &str)] = &[
    (&["다른 프로세스가 파일을 사용 중", "being used by another process"], "다른 프로그램이 이 파일을 쓰는 중입니다.", "그 프로그램을 닫고 다시 해 보세요."),
    (&["액세스가 거부", "access is denied", "permission denied"], "파일이나 폴더에 쓸 권한이 없습니다.", "다른 프로그램이 쓰고 있지 않은지, 읽기 전용 폴더가 아닌지 확인하세요."),
    (&["지정된 파일을 찾을 수 없", "지정된 경로를 찾을 수 없", "cannot find the file", "cannot find the path", "no such file or directory"], "파일이나 폴더를 찾지 못했습니다.", "옮기거나 지우지 않았는지 확인하고 다시 골라 주세요."),
    (&["디스크 공간이 부족", "not enough space on the disk", "no space left"], "디스크 공간이 부족합니다.", "필요 없는 파일을 지워 공간을 만든 뒤 다시 해 보세요."),
    (&["연결을 거부", "connection refused", "actively refused"], "상대 프로그램이 연결을 받지 않습니다.", "그 프로그램(OBS 등)이 켜져 있고 주소 · 포트가 맞는지 확인하세요."),
    (&["시간 제한", "시간이 초과", "timed out", "timeout"], "응답이 없습니다 (시간 초과).", "네트워크 연결을 확인하고 다시 해 보세요."),
    (&["호스트를 알 수 없", "no such host", "dns error", "failed to lookup address", "name or service not known"], "주소를 찾지 못했습니다.", "인터넷 연결과 주소를 확인하세요."),
    (&["database is locked", "database table is locked"], "자료 파일이 잠겨 있습니다.", "워십프리젠터가 두 개 켜져 있지 않은지 확인하세요."),
    (&["database disk image is malformed", "file is not a database"], "자료 파일이 손상되었습니다.", "파일 → 여러 PC 동기화 · 백업에서 최근 백업으로 되돌려 보세요."),
];

/// 날것의 오류 글을 사용자 글로. 알아보는 원인이면 고치는 법을 덧붙이고, "(os error N)" 같은 내부 번호는 뗀다.
/// 이미 다듬은 글(고치는 법이 붙은 글)은 그대로 둔다.
pub fn humanize(msg: &str) -> String {
    let mut s = strip_os_code(msg);
    let low = s.to_lowercase();
    for (needles, what, how) in KNOWN {
        if needles.iter().any(|n| low.contains(n)) {
            let (what, how) = (crate::i18n::tr(what), crate::i18n::tr(how));
            if s.contains(&how) {
                break;
            }
            // 앞 글("저장 실패: ") 은 남기고 그 뒤의 날것 오류를 무슨 일 · 고치는 법으로 바꾼다
            let head = s.split_once(": ").map(|(h, _)| h.trim().to_string()).filter(|h| !h.is_empty() && h.chars().count() <= 60);
            s = match head {
                Some(h) => format!("{h}: {what} {how}"),
                None => format!("{what} {how}"),
            };
            break;
        }
    }
    s
}

/// "… (os error 32)" · "(os error 2)" 를 뗀다
fn strip_os_code(msg: &str) -> String {
    let mut out = msg.to_string();
    while let Some(i) = out.find("(os error ") {
        let Some(j) = out[i..].find(')') else { break };
        out.replace_range(i..i + j + 1, "");
    }
    out.replace("  ", " ").trim().trim_end_matches(':').trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::humanize;
    #[test]
    fn raw_errors_become_what_and_how() {
        assert_eq!(humanize("저장 실패: 지정된 파일을 찾을 수 없습니다. (os error 2)"), "저장 실패: 파일이나 폴더를 찾지 못했습니다. 옮기거나 지우지 않았는지 확인하고 다시 골라 주세요.");
        assert_eq!(humanize("다른 프로세스가 파일을 사용 중이기 때문에 프로세스가 액세스 할 수 없습니다. (os error 32)"), "다른 프로그램이 이 파일을 쓰는 중입니다. 그 프로그램을 닫고 다시 해 보세요.");
        assert!(humanize("OBS: Connection refused (os error 10061)").starts_with("OBS: 상대 프로그램이 연결을 받지 않습니다."));
        assert!(humanize("x: io: Access is denied. (os error 5)").contains("권한"));
        // 모르는 오류는 내부 번호만 뗀다
        assert_eq!(humanize("순서가 없습니다. (os error 9999)"), "순서가 없습니다.");
        assert_eq!(humanize("큐시트를 먼저 여세요."), "큐시트를 먼저 여세요.");
        // 이미 고치는 법이 붙은 글은 그대로
        let done = humanize("저장 실패: Access is denied.");
        assert_eq!(humanize(&done), done);
    }
}
