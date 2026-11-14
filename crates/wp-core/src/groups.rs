//! 슬라이드 그룹 색과 글 정리 — Electron `shared/groups.js` (BCGroup) 를 옮김.
//! 이름표(1절·후렴·브릿지…)마다 같은 색 (FreeShow 방식). 조작 화면 목록·격자가 함께 쓴다.
use once_cell::sync::Lazy;
use regex::Regex;

/// (이름표 규칙, 색) — groups.js GROUPS 와 같은 차례
static GROUPS: Lazy<Vec<(Regex, &'static str)>> = Lazy::new(|| {
    [
        (r"(?i)^(후렴|chorus|refrain|c)\d*$", "#e5484d"),
        (r"(?i)^(브릿지|브리지|bridge|b)\d*$", "#a35bff"),
        (r"(?i)^(전주|인트로|intro|pre|prechorus|간주|interlude)\d*$", "#2fb5a5"),
        (r"(?i)^(엔딩|마무리|ending|end|outro|태그|tag|코다|coda)\d*$", "#e3963d"),
        (r"(?i)^(\d+)절$|^(verse|v)\s*\d+$", "#4f8cff"),
        (r"(?i)^(제목|title)$", "#8b93a3"),
    ]
    .into_iter()
    .map(|(re, c)| (Regex::new(re).unwrap(), c))
    .collect()
});

/// 그 밖의 이름표 색 (groups.js 기본)
const OTHER: &str = "#8a8fa0";

/// 이름표 → 그룹 열쇠 ('1절-2', '후렴 (이어서)' 도 같은 그룹)
pub fn key(label: &str) -> String {
    static TAIL: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s*\(이어서\)\s*$").unwrap());
    static SUB: Lazy<Regex> = Lazy::new(|| Regex::new(r"-\d+$").unwrap());
    let k = TAIL.replace(label, "");
    let k = SUB.replace(&k, "");
    k.chars().filter(|c| !c.is_whitespace()).collect()
}

/// 이름표의 색 (빈 이름표·숫자뿐인 이름표는 None)
pub fn color(label: &str) -> Option<&'static str> {
    let k = key(label);
    if k.is_empty() || k.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(GROUPS.iter().find(|(re, _)| re.is_match(&k)).map(|(_, c)| *c).unwrap_or(OTHER))
}

/// 서식 표시(**굵게** · {색:#e63946}…{/색} · {크기:150}…{/크기})를 뺀 글
pub fn strip_fmt(t: &str) -> String {
    static FMT: Lazy<Regex> = Lazy::new(|| Regex::new(r"\*\*|\{색:#[0-9a-fA-F]{3,8}\}|\{/색\}|\{크기:\d{2,3}\}|\{/크기\}").unwrap());
    FMT.replace_all(t, "").into_owned()
}

/// 가사 안의 코드 [G] [Am7] [D/F#] 지우기 (코드만 있던 줄은 없앤다)
pub fn strip_chords(t: &str) -> String {
    static CHORD: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"\[([A-G](?:#|b|♯|♭)?(?:maj|min|m|M|sus|dim|aug|add|°|ø)?\d{0,2}(?:(?:sus|add|maj|b|#|\+|-)\d{1,2})*(?:/[A-G](?:#|b|♯|♭)?)?)\]").unwrap()
    });
    static SPACES: Lazy<Regex> = Lazy::new(|| Regex::new(r"[ \t]{2,}").unwrap());
    if !CHORD.is_match(t) {
        return t.to_string();
    }
    t.split('\n')
        .filter_map(|l| {
            let had = CHORD.is_match(l);
            let s = SPACES.replace_all(&CHORD.replace_all(l, ""), " ").trim_end().to_string();
            (!(had && s.trim().is_empty())).then_some(s)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_colors_like_electron() {
        assert_eq!(color("후렴"), Some("#e5484d"));
        assert_eq!(color("후렴 (이어서)"), Some("#e5484d"));
        assert_eq!(color("1절-2"), Some("#4f8cff"));
        assert_eq!(color("Bridge"), Some("#a35bff"));
        assert_eq!(color("제목"), Some("#8b93a3"));
        assert_eq!(color("배경"), Some("#8a8fa0"));
        assert_eq!(color("3"), None);
        assert_eq!(color(""), None);
    }

    #[test]
    fn strips_format_and_chords() {
        assert_eq!(strip_fmt("**굵게** {색:#e63946}빨강{/색} {크기:150}크게{/크기}"), "굵게 빨강 크게");
        assert_eq!(strip_chords("[G]주 [D/F#]사랑\n[Am7]\n[1절]"), "주 사랑\n[1절]");
    }
}
