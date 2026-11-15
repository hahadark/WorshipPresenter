//! [트랙 A] PowerPoint 97-2003 (.ppt / .pps) 읽기 — 그림(악보·배경)과 슬라이드 글자만 (Electron 판 ppt.js extractPps)
//! OLE 복합 문서(CFB): 'Pictures' 스트림에 그림, 'PowerPoint Document' 스트림에 글자

use serde_json::{json, Value};
use std::io::Read;
use std::path::Path;

pub struct Pic {
    pub ext: &'static str,
    pub data: Vec<u8>,
}

fn u16le(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn u32le(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

pub fn pictures(pics: &[u8]) -> Vec<Pic> {
    let mut out = vec![];
    let mut off = 0usize;
    while off + 8 <= pics.len() {
        let ty = u16le(pics, off + 2);
        let len = u32le(pics, off + 4) as usize;
        let end = (off + 8 + len).min(pics.len());
        let body = &pics[off + 8..end];
        let ext = match ty {
            0xF01E => Some("png"),
            0xF01D | 0xF02A => Some("jpg"),
            _ => None,
        };
        if let Some(ext) = ext {
            let sig: &[u8] = if ext == "png" { &[0x89, 0x50, 0x4E, 0x47] } else { &[0xFF, 0xD8, 0xFF] };
            if let Some(k) = find(&body[..body.len().min(80)], sig).filter(|k| *k < 64) {
                out.push(Pic { ext, data: body[k..].to_vec() });
            }
        }
        off += 8 + len;
    }
    out
}

fn decode_text(doc: &[u8], ty: u16, a: usize, b: usize) -> String {
    let s = if ty == 0x0FA0 {
        let u: Vec<u16> = doc[a..b].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&u)
    } else {
        doc[a..b].iter().map(|&c| c as char).collect()
    };
    s.replace('\r', "\n").replace('\u{b}', "\n").trim().to_string()
}

/// 슬라이드마다 글자들
pub fn slide_texts(doc: &[u8]) -> Vec<Vec<String>> {
    let mut slides: Vec<Vec<String>> = vec![];
    fn walk(doc: &[u8], start: usize, end: usize, in_list: bool, slides: &mut Vec<Vec<String>>) {
        let mut o = start;
        while o + 8 <= end {
            let vi = u16le(doc, o);
            let ty = u16le(doc, o + 2);
            let len = u32le(doc, o + 4) as usize;
            let (ver, inst) = (vi & 0xF, vi >> 4);
            let body = o + 8;
            let next = body + len;
            if next > end {
                break;
            }
            if ty == 0x0FF0 && ver == 0xF {
                if inst == 0 {
                    walk(doc, body, next, true, slides);
                }
            } else if in_list && ty == 0x03F3 {
                slides.push(vec![]);
            } else if in_list && (ty == 0x0FA0 || ty == 0x0FA8) && !slides.is_empty() {
                let t = decode_text(doc, ty, body, next);
                if !t.is_empty() {
                    slides.last_mut().unwrap().push(t);
                }
            } else if ver == 0xF && !in_list {
                walk(doc, body, next, false, slides);
            }
            o = next;
        }
    }
    walk(doc, 0, doc.len(), false, &mut slides);
    if slides.iter().any(|s| !s.is_empty()) {
        return slides;
    }
    // 새 PowerPoint 로 저장한 파일: 각 SlideContainer 의 도형 안에 글자
    fn collect(doc: &[u8], start: usize, end: usize, acc: &mut Vec<String>) {
        let mut o = start;
        while o + 8 <= end {
            let vi = u16le(doc, o);
            let ty = u16le(doc, o + 2);
            let len = u32le(doc, o + 4) as usize;
            let body = o + 8;
            let next = body + len;
            if next > end {
                break;
            }
            if vi & 0xF == 0xF {
                collect(doc, body, next, acc);
            } else if ty == 0x0FA0 || ty == 0x0FA8 {
                let t = decode_text(doc, ty, body, next);
                if !t.is_empty() {
                    acc.push(t);
                }
            }
            o = next;
        }
    }
    let mut by = vec![];
    let mut o = 0;
    while o + 8 <= doc.len() {
        let vi = u16le(doc, o);
        let ty = u16le(doc, o + 2);
        let len = u32le(doc, o + 4) as usize;
        let next = o + 8 + len;
        if next > doc.len() {
            break;
        }
        if ty == 0x03EE && vi & 0xF == 0xF {
            let mut acc = vec![];
            collect(doc, o + 8, next, &mut acc);
            by.push(acc);
        }
        o = next;
    }
    if by.is_empty() { slides } else { by }
}

fn png_size(d: &[u8]) -> (u32, u32) {
    if d.len() > 24 { (u32::from_be_bytes([d[16], d[17], d[18], d[19]]), u32::from_be_bytes([d[20], d[21], d[22], d[23]])) } else { (0, 0) }
}

/// .pps/.ppt → (배경, 악보들, 슬라이드 글자)
pub fn extract(file: &Path) -> Result<(Option<Pic>, Vec<Pic>, Vec<Vec<String>>), String> {
    let f = std::fs::File::open(file).map_err(|e| e.to_string())?;
    let mut c = cfb::CompoundFile::open(f).map_err(|_| "PowerPoint 97-2003(.ppt/.pps) 파일이 아닙니다.".to_string())?;
    let mut read = |name: &str| -> Option<Vec<u8>> {
        let mut s = c.open_stream(name).ok()?;
        let mut b = vec![];
        s.read_to_end(&mut b).ok()?;
        Some(b)
    };
    let pics = read("/Pictures").map(|p| pictures(&p)).unwrap_or_default();
    let texts = read("/PowerPoint Document").map(|d| slide_texts(&d)).unwrap_or_default();
    let mut pics = pics;
    let mut background = None;
    if !pics.is_empty() {
        let (w, h) = png_size(&pics[0].data);
        let opaque = pics[0].ext == "jpg" || (w > 0 && (w as f64) / (h.max(1) as f64) < 1.5);
        if pics.len() == 1 || opaque {
            background = Some(pics.remove(0));
        }
    }
    Ok((background, pics, texts))
}

/// 캐시에 그림·글자를 꺼내 두고 meta (mode 'auto')
pub fn build_cache(data_dir: &Path, file: &str) -> Result<Value, String> {
    let key = crate::hymns::cache_key(file, "auto-rs1");
    let dir = data_dir.join("hymn-cache").join(&key);
    if let Ok(t) = std::fs::read_to_string(dir.join("meta.json")) {
        if let Ok(v) = serde_json::from_str::<Value>(&t) {
            return Ok(v);
        }
    }
    let (bg, scores, texts) = extract(Path::new(file))?;
    if scores.is_empty() && !texts.iter().any(|t| !t.is_empty()) {
        return Err(crate::i18n::tr("파일에서 그림도 글자도 꺼내지 못했습니다.").into());
    }
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let url = |f: &str| format!("/hymn/{key}/{f}");
    let background = bg.map(|b| {
        let f = format!("bg.{}", b.ext);
        let _ = std::fs::write(dir.join(&f), &b.data);
        url(&f)
    });
    let mut list = vec![];
    for (i, p) in scores.iter().enumerate() {
        let f = format!("score-{:02}.{}", i + 1, p.ext);
        std::fs::write(dir.join(&f), &p.data).map_err(|e| e.to_string())?;
        list.push(url(&f));
    }
    let (number, title, name) = crate::hymns::parse_name(Path::new(file));
    let meta = json!({ "key": key, "number": number, "title": title, "name": name, "file": file, "mode": "auto", "background": background, "scores": list, "texts": texts });
    std::fs::write(dir.join("meta.json"), meta.to_string()).map_err(|e| e.to_string())?;
    Ok(meta)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn blip() {
        // 그림 기록 하나: [ver/inst 2][type F01E][len 4] + 머리 17바이트 + PNG 서명
        let mut body = vec![0u8; 17];
        body.extend([0x89, 0x50, 0x4E, 0x47, 1, 2, 3]);
        let mut rec = vec![0, 0];
        rec.extend(0xF01Eu16.to_le_bytes());
        rec.extend((body.len() as u32).to_le_bytes());
        rec.extend(&body);
        let p = pictures(&rec);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].ext, "png");
        assert_eq!(&p[0].data[..4], &[0x89, 0x50, 0x4E, 0x47]);
    }
}

#[cfg(test)]
mod real {
    /// WP_PPS_LIST(파일 경로 한 줄씩)의 .pps/.ppt 를 읽어 Electron ppt.js extractPps 와 같은 모양의 요약 JSON 을 WP_PPS_OUT 에 쓴다 (나란히 비교용)
    /// cargo test -p wp-core pps_dump -- --ignored
    #[test]
    #[ignore]
    fn pps_dump() {
        let fnv = |b: &[u8]| { let mut h: u32 = 0x811c9dc5; for x in b { h ^= *x as u32; h = h.wrapping_mul(0x01000193); } format!("{h:x}") };
        let list = std::fs::read_to_string(std::env::var("WP_PPS_LIST").unwrap()).unwrap();
        let mut out = vec![];
        for f in list.trim_start_matches('\u{feff}').lines().filter(|l| !l.trim().is_empty()) {
            match super::extract(std::path::Path::new(f)) {
                Ok((bg, scores, texts)) => out.push(serde_json::json!({ "f": f, "bg": bg.map(|b| format!("{}:{}:{}", b.ext, b.data.len(), fnv(&b.data))), "scores": scores.iter().map(|p| format!("{}:{}:{}", p.ext, p.data.len(), fnv(&p.data))).collect::<Vec<_>>(), "texts": texts })),
                Err(e) => out.push(serde_json::json!({ "f": f, "err": e })),
            }
        }
        std::fs::write(std::env::var("WP_PPS_OUT").unwrap(), serde_json::to_string(&out).unwrap()).unwrap();
    }

    /// WP_PPS_DIR 의 .pps 들을 읽어 본다: cargo test -p wp-core real_pps -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_pps() {
        let dir = std::env::var("WP_PPS_DIR").unwrap();
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let (bg, scores, texts) = super::extract(&e.path()).unwrap();
            eprintln!("{}: 배경 {} · 악보 {}장 · 글 {}장 · 첫 글 {:?}", e.file_name().to_string_lossy(), bg.map(|b| b.ext).unwrap_or("없음"), scores.len(), texts.len(), texts.iter().flatten().next().map(|t| t.chars().take(30).collect::<String>()));
        }
        // 새찬송가 1장 가사를 악보 장수에 맞추기 (베들레헴 가사 DB 가 있으면)
        let db = dirs::document_dir().unwrap().join("BethlehemWin").join("새찬송가.hdb");
        if db.exists() {
            let c = rusqlite::Connection::open_with_flags(&db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
            let h: String = c.query_row("SELECT htext FROM hymnal WHERE chapter = 1 ORDER BY id LIMIT 1", [], |r| r.get(0)).unwrap();
            let (v, ch) = crate::hymnal::parse_htext(&h);
            let a = crate::hymnal::align_lyrics(&v, &ch, 4);
            eprintln!("1장 절 {}개 · 후렴 {:?} → 4장: {:?}", v.len(), ch, a);
        }
    }
}
