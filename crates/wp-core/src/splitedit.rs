//! [갈래 C] 한 PPT 를 여러 순서로 나누기 — 손으로 고치는 쪽 (Electron cue.js openSplit 의 toggleAt · carve · add · merge).
//! 슬라이드 번호는 1부터. 자동 나누기(규칙·AI)는 pptsplit.rs.
use crate::pptsplit::Part;

/// "3" · "2-5" · "2~5" → (작은 쪽, 큰 쪽). 1보다 작거나 모양이 다르면 None (cue.js rangeOf)
pub fn range_of(r: &str) -> Option<(usize, usize)> {
    let t = r.trim();
    let (a, b) = match t.split_once(['-', '~']) {
        Some((a, b)) => (a.trim().parse::<usize>().ok()?, b.trim().parse::<usize>().ok()?),
        None => {
            let a = t.parse::<usize>().ok()?;
            (a, a)
        }
    };
    if a < 1 || b < 1 {
        return None;
    }
    Some((a.min(b), a.max(b)))
}

/// 범위 글 ("3" 또는 "2-5")
pub fn range_text(a: usize, b: usize) -> String {
    if a == b {
        a.to_string()
    } else {
        format!("{a}-{b}")
    }
}

/// n 번 슬라이드에서 시작하는 순서의 기본 이름: 그 슬라이드 글의 첫 줄(30자), 없으면 "제목 n" (cue.js nameAt)
pub fn name_at(texts: &[String], title: &str, n: usize) -> String {
    let line = texts.get(n.wrapping_sub(1)).and_then(|t| t.lines().map(str::trim).find(|l| !l.is_empty()));
    match line {
        Some(l) => l.chars().take(30).collect(),
        None => format!("{title} {n}"),
    }
}

fn part(a: usize, b: usize, name: String) -> Part {
    Part { a, b, name, kind: String::new() }
}

/// 두 번 누름: n 번 슬라이드에서 새 순서를 시작한다. 이미 시작이면 앞 순서와 합친다 (cue.js toggleAt)
pub fn toggle_at(parts: &mut Vec<Part>, n: usize, total: usize, texts: &[String], title: &str) {
    if n < 1 || n > total {
        return;
    }
    if parts.is_empty() {
        if n > 1 {
            parts.push(part(1, n - 1, name_at(texts, title, 1)));
            parts.push(part(n, total, name_at(texts, title, n)));
        } else {
            parts.push(part(1, total, name_at(texts, title, 1)));
        }
        return;
    }
    let Some(k) = parts.iter().position(|p| n >= p.a && n <= p.b) else {
        // 어느 순서에도 안 든 슬라이드: 다음 순서 앞까지
        let end = parts.iter().filter(|p| p.a > n).map(|p| p.a - 1).min().unwrap_or(total);
        parts.push(part(n, end, name_at(texts, title, n)));
        parts.sort_by_key(|p| p.a);
        return;
    };
    if parts[k].a == n {
        if k > 0 && parts[k - 1].b + 1 == n {
            parts[k - 1].b = parts[k].b;
            parts.remove(k);
        }
        return;
    }
    let end = parts[k].b;
    parts[k].b = n - 1;
    parts.insert(k + 1, part(n, end, name_at(texts, title, n)));
}

/// 겹치는 범위를 비운다 — 손으로 더한 범위가 자동으로 나눈 것보다 앞선다 (cue.js carve)
pub fn carve(parts: &mut Vec<Part>, a: usize, b: usize) {
    let mut out = Vec::with_capacity(parts.len() + 1);
    for p in parts.drain(..) {
        if p.b < a || p.a > b {
            out.push(p);
            continue;
        }
        if p.a < a {
            out.push(Part { b: a - 1, ..p.clone() });
        }
        if p.b > b {
            let name = if p.a < a { format!("{} {}", p.name, crate::i18n::tr("(이어서)")) } else { p.name.clone() };
            out.push(Part { a: b + 1, name, ..p });
        }
    }
    *parts = out;
}

/// '범위 더하기': 범위·이름으로 순서를 더한다. 다음에 고를 슬라이드(없으면 None)를 돌려준다 (cue.js add)
pub fn add_range(parts: &mut Vec<Part>, range: &str, name: &str, total: usize, title: &str) -> Result<Option<usize>, String> {
    let r = range_of(range).filter(|r| r.1 <= total).ok_or_else(|| crate::i18n::tr_args("범위를 1-{1} 안에서 쓰세요.", &[&total]))?;
    let name = if name.trim().is_empty() { format!("{title} {}", range_text(r.0, r.1)) } else { name.trim().to_string() };
    carve(parts, r.0, r.1);
    parts.push(part(r.0, r.1, name));
    parts.sort_by_key(|p| p.a);
    Ok((r.1 < total).then_some(r.1 + 1))
}

/// 목록의 ↑: 앞 순서와 합친다 (바로 이어질 때만)
pub fn merge_up(parts: &mut Vec<Part>, k: usize) {
    if k > 0 && k < parts.len() && parts[k - 1].b + 1 == parts[k].a {
        parts[k - 1].b = parts[k].b;
        parts.remove(k);
    }
}

/// 아직 어느 순서에도 안 든 슬라이드 수
pub fn left_over(parts: &[Part], total: usize) -> usize {
    (1..=total).filter(|n| !parts.iter().any(|p| *n >= p.a && *n <= p.b)).count()
}

/// n 번 슬라이드가 든 순서 번호
pub fn owner(parts: &[Part], n: usize) -> Option<usize> {
    parts.iter().position(|p| n >= p.a && n <= p.b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rs(parts: &[Part]) -> Vec<(usize, usize)> {
        parts.iter().map(|p| (p.a, p.b)).collect()
    }
    fn texts() -> Vec<String> {
        ["예배로 부름", "", "찬송 10장\n전능왕", "가사", "설교", "축도"].iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn range_of_reads_single_and_span() {
        assert_eq!(range_of(" 3 "), Some((3, 3)));
        assert_eq!(range_of("5-2"), Some((2, 5)));
        assert_eq!(range_of("2~4"), Some((2, 4)));
        assert_eq!(range_of("0"), None);
        assert_eq!(range_of("a-3"), None);
    }

    #[test]
    fn first_toggle_splits_whole_deck_in_two_and_names_from_text() {
        let mut p = vec![];
        toggle_at(&mut p, 3, 6, &texts(), "주일");
        assert_eq!(rs(&p), [(1, 2), (3, 6)]);
        assert_eq!((p[0].name.as_str(), p[1].name.as_str()), ("예배로 부름", "찬송 10장"));
        // 첫 장을 처음 두 번 누르면 통째로 하나
        let mut q = vec![];
        toggle_at(&mut q, 1, 6, &texts(), "주일");
        assert_eq!(rs(&q), [(1, 6)]);
    }

    #[test]
    fn toggle_inside_splits_and_toggle_on_start_merges_back() {
        let mut p = vec![];
        toggle_at(&mut p, 3, 6, &texts(), "주일");
        toggle_at(&mut p, 5, 6, &texts(), "주일");
        assert_eq!(rs(&p), [(1, 2), (3, 4), (5, 6)]);
        toggle_at(&mut p, 3, 6, &texts(), "주일"); // 이미 시작 → 앞과 합침
        assert_eq!(rs(&p), [(1, 4), (5, 6)]);
        toggle_at(&mut p, 1, 6, &texts(), "주일"); // 맨 앞 시작은 합칠 앞이 없다
        assert_eq!(rs(&p), [(1, 4), (5, 6)]);
        toggle_at(&mut p, 9, 6, &texts(), "주일"); // 범위 밖
        assert_eq!(rs(&p), [(1, 4), (5, 6)]);
    }

    #[test]
    fn toggle_on_unowned_slide_fills_up_to_next_part() {
        let mut p = vec![part(4, 6, "뒤".into())];
        toggle_at(&mut p, 2, 6, &texts(), "주일");
        assert_eq!(rs(&p), [(2, 3), (4, 6)]);
        assert_eq!(p[0].name, "주일 2", "글이 없는 슬라이드는 제목과 번호");
    }

    #[test]
    fn add_range_carves_overlaps_and_returns_next_slide() {
        let mut p = vec![part(1, 6, "전체".into())];
        let next = add_range(&mut p, "3-4", "찬송", 6, "주일").unwrap();
        assert_eq!(next, Some(5));
        assert_eq!(rs(&p), [(1, 2), (3, 4), (5, 6)]);
        assert_eq!(p[1].name, "찬송");
        assert!(p[2].name.starts_with("전체 "), "뒤에 남은 조각은 '(이어서)': {}", p[2].name);
        assert_eq!(add_range(&mut p, "5-6", "", 6, "주일").unwrap(), None);
        assert_eq!(p[2].name, "주일 5-6");
        assert!(add_range(&mut p, "5-9", "", 6, "주일").is_err());
        assert!(add_range(&mut p, "", "", 6, "주일").is_err());
    }

    #[test]
    fn merge_up_only_when_adjacent_and_left_over_counts() {
        let mut p = vec![part(1, 2, "a".into()), part(4, 5, "b".into()), part(6, 6, "c".into())];
        assert_eq!(left_over(&p, 6), 1);
        merge_up(&mut p, 1); // 3번이 비어 있어 이어지지 않는다
        assert_eq!(p.len(), 3);
        merge_up(&mut p, 2);
        assert_eq!(rs(&p), [(1, 2), (4, 6)]);
        assert_eq!(owner(&p, 5), Some(1));
        assert_eq!(owner(&p, 3), None);
    }
}
