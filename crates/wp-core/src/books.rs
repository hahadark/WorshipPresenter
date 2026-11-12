//! 개신교 성경 66권 (data/books.json 은 Electron 판 books.js 에서 그대로 뽑은 것)

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use unicode_normalization::UnicodeNormalization;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Book {
    pub id: i64,
    pub name: String,
    pub short: String,
    pub aliases: Vec<String>,
    pub osis: String,
    pub en: String,
    pub chapters: i64,
    pub testament: String,
    pub single_chapter: bool,
}

pub static BOOKS: Lazy<Vec<Book>> =
    Lazy::new(|| serde_json::from_str(include_str!("../data/books.json")).expect("books.json"));

pub fn key(s: &str) -> String {
    s.nfc()
        .collect::<String>()
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '.')
        .collect()
}

static EXACT: Lazy<HashMap<String, usize>> = Lazy::new(|| {
    let mut m = HashMap::new();
    for (i, b) in BOOKS.iter().enumerate() {
        let mut names = vec![b.name.clone(), b.short.clone()];
        names.extend(b.aliases.iter().cloned());
        names.push(b.osis.clone());
        names.push(b.en.clone());
        for n in names {
            m.entry(key(&n)).or_insert(i);
        }
    }
    m
});

pub fn get_book(id: i64) -> Option<&'static Book> {
    BOOKS.iter().find(|b| b.id == id)
}

/// 책 이름 해석. 정확히 일치하면 그 책, 아니면 접두어 후보 (여럿이면 고르지 않고 돌려준다)
pub fn resolve_book(input: &str) -> (Option<&'static Book>, Vec<&'static Book>) {
    let k = key(input);
    if k.is_empty() {
        return (None, vec![]);
    }
    if let Some(&i) = EXACT.get(&k) {
        return (Some(&BOOKS[i]), vec![]);
    }
    let mut cands = vec![];
    for b in BOOKS.iter() {
        let mut names = vec![&b.name, &b.short, &b.en, &b.osis];
        names.extend(b.aliases.iter());
        if names.iter().any(|n| key(n).starts_with(&k)) {
            cands.push(b);
        }
    }
    if cands.len() == 1 {
        return (Some(cands[0]), vec![]);
    }
    (None, cands)
}

/// 번역본 언어에 맞는 책 이름 (영어: 'John' · 'John ', 그 밖: '요한복음' · '요')
pub fn book_names(b: &Book, lang: &str) -> (String, String) {
    if lang.to_lowercase().starts_with("en") {
        (b.en.clone(), format!("{} ", b.en))
    } else {
        (b.name.clone(), b.short.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resolve() {
        assert_eq!(BOOKS.len(), 66);
        assert_eq!(resolve_book("요").0.map(|b| b.id), Some(43));
        assert_eq!(resolve_book("John").0.map(|b| b.id), Some(43));
        assert_eq!(resolve_book("창세기").0.map(|b| b.id), Some(1));
        assert!(resolve_book("요한").0.is_none());
    }
}
