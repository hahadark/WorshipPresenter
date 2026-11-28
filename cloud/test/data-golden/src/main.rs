//! 설치판 장절 파서(crates/wp-core/src/parser.rs)의 결과를 JSON 으로 적는다 — 웹판 cloud/src/data/parser.mjs 대조용.
#[path = "../../../../crates/wp-core/src/books.rs"]
#[allow(dead_code)]
mod books;
#[path = "../../../../crates/wp-core/src/parser.rs"]
#[allow(dead_code)]
mod parser;

use parser::{looks_like_reference, normalize_input, parse_reference, Parsed};
use serde_json::{json, Value};

fn main() {
    let file = std::env::args().nth(1).expect("cases.json");
    let cases: Vec<String> = serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap();
    let out: Vec<Value> = cases
        .iter()
        .map(|input| {
            let r = match parse_reference(input) {
                Parsed::Ok { book, ranges } => json!({ "ok": true, "book": book.id, "ranges": ranges }),
                Parsed::Err { error, book, candidates, incomplete, ambiguous, rest } => json!({
                    "ok": false, "error": error, "book": book.map(|b| b.id),
                    "candidates": candidates.iter().map(|b| b.id).collect::<Vec<_>>(),
                    "incomplete": incomplete, "ambiguous": ambiguous, "rest": rest,
                }),
            };
            let (bk, cands) = books::resolve_book(input);
            json!({
                "input": input, "normalized": normalize_input(input), "parsed": r, "looksLike": looks_like_reference(input),
                "resolveBook": { "book": bk.map(|b| b.id), "candidates": cands.iter().map(|b| b.id).collect::<Vec<_>>() },
            })
        })
        .collect();
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}
