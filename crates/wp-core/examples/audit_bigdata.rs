//! 점검용: 큰 큐시트(순서 100개 · 슬라이드 수천 장)와 큰 찬양 라이브러리(3000곡)를 만든다.
//! 반드시 사본 폴더에: `cargo run -p wp-core --example audit_bigdata --offline -- --data <임시 폴더>`
use serde_json::json;
use std::path::PathBuf;
use wp_core::core::{new_item, Core};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let Some(data) = a.iter().position(|x| x == "--data").and_then(|i| a.get(i + 1)).map(PathBuf::from) else {
        eprintln!("--data <폴더> 가 필요합니다 (사용자 실제 자료에는 쓰지 마세요)");
        return;
    };
    let mut c = Core::open(data.clone(), data.join("web"), None).expect("코어 열기");
    c.new_cue("큰 큐시트 점검 (순서 100개)").expect("새 큐시트");
    let refs = ["시 119:1-176", "창 1:1-31", "요 3:1-36", "롬 8:1-39", "마 5:1-48", "사 53:1-12", "계 21:1-27", "시 23:1-6"];
    let long_line = "아주 긴 가사 한 줄 ".repeat(40);
    for i in 0..100 {
        let it = match i % 5 {
            0 => {
                let mut it = new_item("bible", &format!("성경 봉독 {i}"));
                it["ref"] = json!(refs[(i / 5) % refs.len()]);
                it
            }
            1 | 2 => {
                let mut it = new_item("song", &format!("찬양 {i} 🙂"));
                let verses: Vec<String> = (1..=12).map(|v| format!("[{v}절]\n{i}번 찬양의 {v}절 첫 줄 🙏\n둘째 줄 주님을 찬양합니다\n셋째 줄 할렐루야\n넷째 줄 아멘")).collect();
                it["lyrics"] = json!(format!("{}\n\n[후렴]\n{}", verses.join("\n\n"), long_line));
                it
            }
            3 => {
                let mut it = new_item("text", &format!("광고 {i}"));
                it["body"] = json!((1..=20).map(|k| format!("{k}. 오늘 오후 2시 교사 모임이 있습니다\n본당 2층 소예배실")).collect::<Vec<_>>().join("\n---\n"));
                it
            }
            _ => {
                let mut it = new_item("title", &format!("순서 {i}"));
                it["subtitle"] = json!("말씀과 찬양으로 드리는 예배");
                it
            }
        };
        if let Some(q) = &mut c.cue {
            q.items.push(it);
        }
    }
    c.save_cue();
    let t0 = std::time::Instant::now();
    for i in 0..3000 {
        let lyrics: String = (1..=4).map(|v| format!("[{v}절]\n곡 {i} 의 {v}절 가사 첫 줄\n둘째 줄 은혜 사랑 평강\n셋째 줄")).collect::<Vec<_>>().join("\n\n");
        let _ = wp_core::songs::save_song(&c.store.db, &json!({ "title": format!("점검 찬양 {i:04} 은혜"), "lyrics": lyrics, "credit": "점검", "tags": "점검" }));
    }
    c.flush();
    println!("큐시트 순서 {}개, 찬양 3000곡 넣음 ({:.1}초)", c.cue.as_ref().map(|q| q.items.len()).unwrap_or(0), t0.elapsed().as_secs_f32());
}
