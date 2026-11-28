//! [트랙 A] PPT 모아보기 그림 (Electron control/pptsplit.js sheets · core pptSplitSheets)
//! 슬라이드 16장(160장 넘으면 25장)을 번호와 함께 한 장에 → JPEG. 글자가 없는(그림만 있는) PPT 를 AI 가 보고 나누게.

use crate::core::Core;
use serde_json::Value;
use std::fmt::Write;
use std::path::{Path, PathBuf};

/// 슬라이드 그림 주소(/hymn/키/파일) → 실제 파일
pub fn slide_file(core: &Core, url: &str) -> Option<PathBuf> {
    let r = url.strip_prefix("/hymn/")?;
    let (key, file) = r.split_once('/')?;
    crate::hymns::cache_path(core, key, file)
}

fn b64_file(p: &Path) -> Option<(String, String)> {
    let bytes = std::fs::read(p).ok()?;
    let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let mime = match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "svg" => "image/svg+xml",
        _ => "image/png",
    };
    Some((mime.into(), crate::hymns::b64(&bytes)))
}

/// 모아보기 한 장의 SVG (칸마다 왼쪽 위 노란 번호)
fn sheet_svg(cells: &[(usize, Option<PathBuf>, String, bool)], cols: usize, tw: f64, th: f64) -> String {
    let rows = cells.len().div_ceil(cols);
    let (w, h) = (cols as f64 * tw, rows as f64 * th);
    let mut s = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\"><rect width=\"{w}\" height=\"{h}\" fill=\"#444\"/>");
    for (j, (num, img, txt, score)) in cells.iter().enumerate() {
        let (x, y) = ((j % cols) as f64 * tw, (j / cols) as f64 * th);
        let _ = write!(s, "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"{}\"/>", x + 1.0, y + 1.0, tw - 2.0, th - 2.0, if *score { "#fff" } else { "#000" });
        match img.as_ref().and_then(|p| b64_file(p)) {
            Some((mime, data)) => {
                let _ = write!(s, "<image x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" preserveAspectRatio=\"xMidYMid meet\" xlink:href=\"data:{mime};base64,{data}\"/>", x + 1.0, y + 1.0, tw - 2.0, th - 2.0);
            }
            None => {
                for (z, l) in txt.lines().filter(|l| !l.trim().is_empty()).take(7).enumerate() {
                    let l: String = l.chars().take(26).collect();
                    let _ = write!(s, "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"15\" font-family=\"'Malgun Gothic', sans-serif\" fill=\"#ddd\">{}</text>", x + 10.0, y + 50.0 + z as f64 * 19.0, crate::pptx::svg::esc(&l));
                }
            }
        }
        let lab = num.to_string();
        let lw = 13.0 * lab.len() as f64 + 12.0;
        let _ = write!(s, "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{lw:.1}\" height=\"28\" fill=\"#ffd400\"/><text x=\"{:.1}\" y=\"{:.1}\" font-size=\"22\" font-weight=\"bold\" font-family=\"'Malgun Gothic', sans-serif\" fill=\"#000\">{lab}</text>", x + 2.0, y + 2.0, x + 8.0, y + 24.0);
    }
    s.push_str("</svg>");
    s
}

/// 슬라이드들 → 모아보기 그림들 [(from, to, jpeg)] (모두 합쳐 약 4.6MB 를 넘지 않게 화질을 낮춘다)
pub fn build(slides: &[(Option<PathBuf>, String, bool)]) -> Result<Vec<(usize, usize, Vec<u8>)>, String> {
    let n = slides.len();
    if n == 0 {
        return Err(crate::i18n::tr("슬라이드가 없습니다.").into());
    }
    let cols = if n > 160 { 5 } else { 4 };
    let per = cols * cols;
    let tw = if n > 160 { 300.0 } else { 384.0 };
    let th = (tw * 9.0 / 16.0_f64).round();
    let mut pngs: Vec<(usize, usize, image::RgbaImage)> = vec![];
    let tmp = std::env::temp_dir();
    for s in (0..n).step_by(per) {
        let cells: Vec<(usize, Option<PathBuf>, String, bool)> = (s..(s + per).min(n)).map(|k| (k + 1, slides[k].0.clone(), slides[k].1.clone(), slides[k].2)).collect();
        let svg = sheet_svg(&cells, cols, tw, th);
        let w = (cols as f64 * tw) as u32;
        let png = crate::pptx::build::render_png(&svg, &tmp, w)?;
        let img = image::load_from_memory(&png).map_err(|e| e.to_string())?.to_rgba8();
        pngs.push((s + 1, (s + per).min(n), img));
    }
    let mut out = vec![];
    for q in [72u8, 55, 40] {
        out.clear();
        for (a, b, img) in &pngs {
            let rgb = image::DynamicImage::ImageRgba8(img.clone()).to_rgb8();
            let mut buf = std::io::Cursor::new(Vec::new());
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, q).encode_image(&rgb).map_err(|e| e.to_string())?;
            out.push((*a, *b, buf.into_inner()));
        }
        if out.iter().map(|x| x.2.len()).sum::<usize>() * 4 / 3 < 6_200_000 {
            break;
        }
    }
    Ok(out)
}

/// 순서(찬송가 PPT) → 모아보기 재료 (슬라이드 그림 파일 · 글자 · 악보인지). 그리기(build)는 잠금 밖에서
pub fn inputs(core: &Core, file: &str) -> Result<Vec<(Option<PathBuf>, String, bool)>, String> {
    let meta = crate::hymns::load(core, file)?;
    let texts = crate::hymns::slide_texts(core, file).unwrap_or_default();
    let (list, score) = match meta["slides"].as_array().filter(|a| !a.is_empty()) {
        Some(a) => (a.clone(), false),
        None => (meta["scores"].as_array().cloned().unwrap_or_default(), true),
    };
    let n = list.len().max(texts.len());
    let slides: Vec<(Option<PathBuf>, String, bool)> = (0..n)
        .map(|k| (list.get(k).and_then(Value::as_str).and_then(|u| slide_file(core, u)), texts.get(k).cloned().unwrap_or_default(), score))
        .collect();
    Ok(slides)
}

/// data: 주소로 (AI 에 보낼 때)
pub fn as_data_urls(sheets: &[(usize, usize, Vec<u8>)]) -> Vec<(usize, usize, String)> {
    sheets.iter().map(|(a, b, j)| (*a, *b, format!("data:image/jpeg;base64,{}", crate::hymns::b64(j)))).collect()
}

/// 채팅 AI 에 올리려고 파일로: 다운로드 폴더(없으면 데이터 폴더)의 'WorshipPresenter 모아보기' (지난번 그림은 지움)
pub fn save(core: &Core, sheets: &[(usize, usize, Vec<u8>)]) -> Result<PathBuf, String> {
    let base = dirs::download_dir().unwrap_or_else(|| core.data_dir.clone());
    let dir = base.join("WorshipPresenter 모아보기");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let old = regex::Regex::new(r"^슬라이드 \d+-\d+\.jpg$").unwrap();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            if old.is_match(&e.file_name().to_string_lossy()) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    for (a, b, j) in sheets {
        std::fs::write(dir.join(format!("슬라이드 {a:03}-{b:03}.jpg")), j).map_err(|e| e.to_string())?;
    }
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sheets_from_text_only() {
        let slides: Vec<(Option<PathBuf>, String, bool)> = (0..20).map(|k| (None, format!("{}절 가사\n둘째 줄", k + 1), false)).collect();
        let out = build(&slides).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!((out[0].0, out[0].1, out[1].0, out[1].1), (1, 16, 17, 20));
        assert!(out[0].2.starts_with(&[0xFF, 0xD8]));
    }
}

#[cfg(test)]
mod real_tests {
    /// 실제 그림으로: WP_SHEET_DIR(slide-*.png 가 있는 폴더) WP_SHEET_OUT(jpg)
    #[test]
    #[ignore]
    fn sheet_real() {
        let (Ok(dir), Ok(out)) = (std::env::var("WP_SHEET_DIR"), std::env::var("WP_SHEET_OUT")) else { return };
        let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).filter(|p| { let n = p.file_name().unwrap().to_string_lossy().to_string(); n.starts_with("slide-") && n.ends_with(".png") && !n.contains("-bg") }).collect();
        files.sort();
        let mut slides: Vec<(Option<std::path::PathBuf>, String, bool)> = files.into_iter().map(|p| (Some(p), String::new(), false)).collect();
        slides.push((None, "그림 없는 슬라이드\n글자만".into(), false));
        let t0 = web_time::Instant::now();
        let s = super::build(&slides).unwrap();
        println!("모아보기 {}장 {:.1}초, {}바이트", s.len(), t0.elapsed().as_secs_f32(), s[0].2.len());
        std::fs::write(&out, &s[0].2).unwrap();
    }
}
