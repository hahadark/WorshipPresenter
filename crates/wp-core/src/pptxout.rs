//! 큐시트 → PPT 파일 하나 (예배 순서대로, 16:9)
//! - 가사 · 성경 · 공지 · 제목: 조작 화면과 같은 슬라이드 나눔(item_deck)으로 글 슬라이드 (검정 바탕 · 흰 글씨 · 맑은 고딕)
//! - 찬송가·PPT · 그림 · 악보: 이미 만들어 둔 그림을 한 장 그림으로 (PPT 는 원래 모양의 그림)
//! - 영상 · 카메라 · 음악 · 슬라이드 편집으로 덧붙인 요소는 넣지 않는다 (결과에 알린다)
//! PowerPoint 없이 만든다. 예비 PC · 다른 프로그램으로 넘기거나 문제가 생겼을 때 대신 띄울 용도.

use crate::core::Core;
use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};

const W: i64 = 12_192_000; // 16:9 (EMU)
const H: i64 = 6_858_000;

/// 슬라이드 한 장
#[derive(Debug, Clone)]
pub enum Out {
    /// 본문 · 작은 글(장절 · 곡 제목) · 제목 슬라이드인가
    Text { body: String, caption: String, title: bool },
    /// 그림 (바이트, 확장자 png|jpeg, 바탕색 RRGGBB)
    Image { bytes: Vec<u8>, ext: &'static str, bg: String, w: u32, h: u32 },
    Blank,
}

fn esc(s: &str) -> String {
    s.chars()
        .filter(|c| !matches!(*c as u32, 0..=8 | 11 | 12 | 14..=31))
        .map(|c| match c {
            '&' => "&amp;".to_string(),
            '<' => "&lt;".to_string(),
            '>' => "&gt;".to_string(),
            '"' => "&quot;".to_string(),
            c => c.to_string(),
        })
        .collect()
}

/// "%EC%9D%80" → 은
fn pct_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() && b[i + 1].is_ascii_hexdigit() && b[i + 2].is_ascii_hexdigit() {
            let hx = |c: u8| (c as char).to_digit(16).unwrap_or(0) as u8;
            out.push(hx(b[i + 1]) * 16 + hx(b[i + 2]));
            i += 3;
            continue;
        }
        out.push(if b[i] == b'+' { b' ' } else { b[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

/// 그림이 있는 곳 (조작 화면 잠금 안에서 빨리 찾고, 읽기·변환은 잠금 밖에서)
#[derive(Debug, Clone)]
pub enum Src {
    File(PathBuf),
    /// 찬송가 직접 그리기 캐시의 SVG (같은 폴더의 media-*.png 를 읽는다)
    Svg(PathBuf),
    Bytes(Vec<u8>),
}

/// 슬라이드의 그림 주소 → 그림이 있는 곳 (/hymn/키/파일 · /lib/file?p= · file:/// · /hymnal/score/책/번호.png · 절대 경로)
pub fn image_source(core: &Core, url: &str) -> Option<Src> {
    if let Some(rest) = url.strip_prefix("/hymn/") {
        let (key, file) = rest.split_once('/')?;
        let fp = crate::hymns::cache_path(core, key, file)?;
        return Some(if fp.extension().is_some_and(|e| e.eq_ignore_ascii_case("svg")) { Src::Svg(fp) } else { Src::File(fp) });
    }
    if let Some(q) = url.strip_prefix("/lib/file?p=") {
        return Some(Src::File(PathBuf::from(pct_decode(q.split('&').next().unwrap_or("")))));
    }
    if let Some(rest) = url.strip_prefix("/hymnal/score/") {
        let (book, file) = rest.split_once('/')?;
        let n: i64 = file.strip_suffix(".png")?.parse().ok()?;
        return crate::hymnal::score(core, &pct_decode(book), n).map(Src::Bytes);
    }
    if let Some(p) = url.strip_prefix("file:///") {
        return Some(Src::File(PathBuf::from(pct_decode(p))));
    }
    Path::new(url).is_absolute().then(|| Src::File(PathBuf::from(url)))
}

/// 그림 읽기 → PPT 가 받는 꼴(PNG · JPEG 는 그대로, 그 밖 webp · gif · bmp · svg 는 PNG 로)
pub fn load_image(src: &Src) -> Option<(Vec<u8>, &'static str)> {
    let b = match src {
        Src::Svg(fp) => {
            let svg = std::fs::read_to_string(fp).ok()?;
            return crate::pptx::build::render_png(&svg, fp.parent()?, 1920).ok().map(|b| (b, "png"));
        }
        Src::File(p) => std::fs::read(p).ok()?,
        Src::Bytes(b) => b.clone(),
    };
    if b.starts_with(&[0x89, b'P', b'N', b'G']) {
        return Some((b, "png"));
    }
    if b.starts_with(&[0xFF, 0xD8]) {
        return Some((b, "jpeg"));
    }
    if b.windows(4).take(512).any(|w| w == b"<svg") {
        let svg = String::from_utf8_lossy(&b).to_string();
        let dir = match src {
            Src::File(p) => p.parent().map(Path::to_path_buf).unwrap_or_default(),
            _ => PathBuf::from("."),
        };
        return crate::pptx::build::render_png(&svg, &dir, 1920).ok().map(|x| (x, "png"));
    }
    let img = image::load_from_memory(&b).ok()?;
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some((out.into_inner(), "png"))
}

fn dims(b: &[u8]) -> (u32, u32) {
    image::ImageReader::new(std::io::Cursor::new(b)).with_guessed_format().ok().and_then(|r| r.into_dimensions().ok()).unwrap_or((1920, 1080))
}

/// "**인도자**: 글" 같은 굵게 표시를 뺀다
fn plain(s: &str) -> String {
    s.replace("**", "")
}

fn hex_of(c: &str) -> String {
    let t = c.trim().trim_start_matches('#');
    if t.len() == 6 && t.chars().all(|c| c.is_ascii_hexdigit()) { t.to_uppercase() } else { "000000".into() }
}

/// 글 크기(1/100 pt): 줄 수와 가장 긴 줄로 고른다. PowerPoint 에서는 넘치면 줄여 맞춤(normAutofit)도 켠다
fn font_size(body: &str, title: bool) -> i64 {
    let lines: Vec<&str> = body.lines().collect();
    let n = lines.len().max(1);
    let longest = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    let by_lines = match n {
        1..=2 => 4800,
        3 => 4400,
        4 => 4000,
        5..=6 => 3400,
        7..=8 => 2800,
        _ => 2400,
    };
    let by_width = match longest {
        0..=18 => 6000,
        19..=24 => 4800,
        25..=30 => 4000,
        31..=40 => 3200,
        _ => 2600,
    };
    let s = by_lines.min(by_width);
    if title { s.max(4400).min(6000) } else { s }
}

fn para_xml(text: &str, sz: i64, color: &str, bold: bool, algn: &str) -> String {
    let lines: Vec<&str> = if text.is_empty() { vec![""] } else { text.split('\n').collect() };
    lines
        .iter()
        .map(|l| {
            let rpr = format!(r#"<a:rPr lang="ko-KR" altLang="en-US" sz="{sz}" b="{}" dirty="0"><a:solidFill><a:srgbClr val="{color}"/></a:solidFill><a:latin typeface="Malgun Gothic"/><a:ea typeface="맑은 고딕"/></a:rPr>"#, if bold { 1 } else { 0 });
            if l.is_empty() {
                format!(r#"<a:p><a:pPr algn="{algn}"/><a:endParaRPr lang="ko-KR" sz="{sz}"/></a:p>"#)
            } else {
                format!(r#"<a:p><a:pPr algn="{algn}"/><a:r>{rpr}<a:t>{}</a:t></a:r></a:p>"#, esc(l))
            }
        })
        .collect()
}

fn text_box(id: u32, name: &str, x: i64, y: i64, w: i64, h: i64, paras: &str, anchor: &str, autofit: bool) -> String {
    format!(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="{name}"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{w}" cy="{h}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/></p:spPr><p:txBody><a:bodyPr wrap="square" lIns="91440" tIns="45720" rIns="91440" bIns="45720" anchor="{anchor}">{}</a:bodyPr><a:lstStyle/>{paras}</p:txBody></p:sp>"#,
        if autofit { "<a:normAutofit/>" } else { "<a:noAutofit/>" }
    )
}

fn slide_xml(o: &Out, pic_rid: Option<&str>) -> String {
    let (bg, shapes) = match o {
        Out::Text { body, caption, title } => {
            let sz = font_size(body, *title);
            let mut sh = text_box(2, "본문", 457_200, if *title { 1_600_000 } else { 380_000 }, W - 914_400, if *title { 2_400_000 } else { H - 1_200_000 }, &para_xml(body, sz, "FFFFFF", *title, "ctr"), if *title { "b" } else { "ctr" }, true);
            if !caption.is_empty() {
                if *title {
                    sh.push_str(&text_box(3, "부제", 457_200, 4_100_000, W - 914_400, 1_200_000, &para_xml(caption, 2800, "D0D0D0", false, "ctr"), "t", true));
                } else {
                    sh.push_str(&text_box(3, "장절", 457_200, H - 760_000, W - 914_400, 560_000, &para_xml(caption, 2000, "BFBFBF", false, "r"), "ctr", false));
                }
            }
            ("000000".to_string(), sh)
        }
        Out::Image { bg, w, h, .. } => {
            // 비율을 지켜 화면 안에 맞춤
            let (iw, ih) = ((*w).max(1) as f64, (*h).max(1) as f64);
            let k = (W as f64 / iw).min(H as f64 / ih);
            let (cw, ch) = ((iw * k).round() as i64, (ih * k).round() as i64);
            let (x, y) = ((W - cw) / 2, (H - ch) / 2);
            let pic = format!(
                r#"<p:pic><p:nvPicPr><p:cNvPr id="2" name="그림"/><p:cNvPicPr><a:picLocks noChangeAspect="1"/></p:cNvPicPr><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed="{}"/><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr><a:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{cw}" cy="{ch}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr></p:pic>"#,
                pic_rid.unwrap_or("rId2")
            );
            (bg.clone(), pic)
        }
        Out::Blank => ("000000".to_string(), String::new()),
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:bg><p:bgPr><a:solidFill><a:srgbClr val="{bg}"/></a:solidFill><a:effectLst/></p:bgPr></p:bg><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>{shapes}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>"#
    )
}

const THEME: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="WorshipPresenter"><a:themeElements><a:clrScheme name="WorshipPresenter"><a:dk1><a:srgbClr val="000000"/></a:dk1><a:lt1><a:srgbClr val="FFFFFF"/></a:lt1><a:dk2><a:srgbClr val="1F1F1F"/></a:dk2><a:lt2><a:srgbClr val="E7E6E6"/></a:lt2><a:accent1><a:srgbClr val="4472C4"/></a:accent1><a:accent2><a:srgbClr val="ED7D31"/></a:accent2><a:accent3><a:srgbClr val="A5A5A5"/></a:accent3><a:accent4><a:srgbClr val="FFC000"/></a:accent4><a:accent5><a:srgbClr val="5B9BD5"/></a:accent5><a:accent6><a:srgbClr val="70AD47"/></a:accent6><a:hlink><a:srgbClr val="0563C1"/></a:hlink><a:folHlink><a:srgbClr val="954F72"/></a:folHlink></a:clrScheme><a:fontScheme name="WorshipPresenter"><a:majorFont><a:latin typeface="Malgun Gothic"/><a:ea typeface="맑은 고딕"/><a:cs typeface=""/></a:majorFont><a:minorFont><a:latin typeface="Malgun Gothic"/><a:ea typeface="맑은 고딕"/><a:cs typeface=""/></a:minorFont></a:fontScheme><a:fmtScheme name="Office"><a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:fillStyleLst><a:lnStyleLst><a:ln w="6350"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="12700"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="19050"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln></a:lnStyleLst><a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst><a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:bgFillStyleLst></a:fmtScheme></a:themeElements><a:objectDefaults/><a:extraClrSchemeLst/></a:theme>"#;

const MASTER: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sldMaster xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:bg><p:bgRef idx="1001"><a:schemeClr val="bg1"/></p:bgRef></p:bg><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr></p:spTree></p:cSld><p:clrMap bg1="dk1" tx1="lt1" bg2="dk2" tx2="lt2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/><p:sldLayoutIdLst><p:sldLayoutId id="2147483649" r:id="rId1"/></p:sldLayoutIdLst><p:txStyles><p:titleStyle><a:lvl1pPr><a:defRPr sz="4400"/></a:lvl1pPr></p:titleStyle><p:bodyStyle><a:lvl1pPr><a:defRPr sz="3200"/></a:lvl1pPr></p:bodyStyle><p:otherStyle><a:lvl1pPr><a:defRPr sz="1800"/></a:lvl1pPr></p:otherStyle></p:txStyles></p:sldMaster>"#;

const LAYOUT: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sldLayout xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" type="blank" preserve="1"><p:cSld name="빈 화면"><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr></p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>"#;

/// 슬라이드들 → PPTX 바이트
pub fn write_pptx(slides: &[Out], title: &str) -> Result<Vec<u8>, String> {
    let mut zw = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opt: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let put = |zw: &mut zip::ZipWriter<std::io::Cursor<Vec<u8>>>, name: &str, data: &[u8]| -> Result<(), String> {
        zw.start_file(name, opt).map_err(|e| e.to_string())?;
        zw.write_all(data).map_err(|e| e.to_string())
    };
    let n = slides.len();
    let (mut has_png, mut has_jpeg) = (false, false);
    for s in slides {
        if let Out::Image { ext, .. } = s {
            if *ext == "png" { has_png = true } else { has_jpeg = true }
        }
    }
    let mut ct = String::from(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>"#);
    if has_png {
        ct.push_str(r#"<Default Extension="png" ContentType="image/png"/>"#);
    }
    if has_jpeg {
        ct.push_str(r#"<Default Extension="jpeg" ContentType="image/jpeg"/>"#);
    }
    ct.push_str(r#"<Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/><Override PartName="/ppt/slideMasters/slideMaster1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"/><Override PartName="/ppt/slideLayouts/slideLayout1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/><Override PartName="/ppt/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/><Override PartName="/ppt/presProps.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presProps+xml"/><Override PartName="/ppt/viewProps.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.viewProps+xml"/><Override PartName="/ppt/tableStyles.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.tableStyles+xml"/><Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/><Override PartName="/docProps/app.xml" ContentType="application/vnd.openxmlformats-officedocument.extended-properties+xml"/>"#);
    for i in 1..=n {
        ct.push_str(&format!(r#"<Override PartName="/ppt/slides/slide{i}.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/>"#));
    }
    ct.push_str("</Types>");
    put(&mut zw, "[Content_Types].xml", ct.as_bytes())?;
    put(&mut zw, "_rels/.rels", br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="ppt/presentation.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/><Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties" Target="docProps/app.xml"/></Relationships>"#)?;
    let now = crate::util::iso_now();
    let now = now.split('.').next().unwrap_or("").to_string() + "Z";
    put(&mut zw, "docProps/core.xml", format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><dc:title>{}</dc:title><dc:creator>워십프리젠터</dc:creator><dcterms:created xsi:type="dcterms:W3CDTF">{now}</dcterms:created><dcterms:modified xsi:type="dcterms:W3CDTF">{now}</dcterms:modified></cp:coreProperties>"#, esc(title)).as_bytes())?;
    put(&mut zw, "docProps/app.xml", format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"><Application>WorshipPresenter</Application><Slides>{n}</Slides></Properties>"#).as_bytes())?;
    // presentation.xml
    let ids: String = (1..=n).map(|i| format!(r#"<p:sldId id="{}" r:id="rId{}"/>"#, 255 + i, i + 1)).collect();
    put(&mut zw, "ppt/presentation.xml", format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:presentation xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" saveSubsetFonts="1"><p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst><p:sldIdLst>{ids}</p:sldIdLst><p:sldSz cx="{W}" cy="{H}"/><p:notesSz cx="6858000" cy="9144000"/><p:defaultTextStyle><a:lvl1pPr><a:defRPr lang="ko-KR"/></a:lvl1pPr></p:defaultTextStyle></p:presentation>"#).as_bytes())?;
    let mut prels = String::from(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="slideMasters/slideMaster1.xml"/>"#);
    for i in 1..=n {
        prels.push_str(&format!(r#"<Relationship Id="rId{}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide{i}.xml"/>"#, i + 1));
    }
    let k = n + 2;
    prels.push_str(&format!(r#"<Relationship Id="rId{k}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="theme/theme1.xml"/><Relationship Id="rId{}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/presProps" Target="presProps.xml"/><Relationship Id="rId{}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/viewProps" Target="viewProps.xml"/><Relationship Id="rId{}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/tableStyles" Target="tableStyles.xml"/></Relationships>"#, k + 1, k + 2, k + 3));
    put(&mut zw, "ppt/_rels/presentation.xml.rels", prels.as_bytes())?;
    let pml = r#"xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main""#;
    put(&mut zw, "ppt/presProps.xml", format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:presentationPr {pml}/>"#).as_bytes())?;
    put(&mut zw, "ppt/viewProps.xml", format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:viewPr {pml}><p:normalViewPr><p:restoredLeft sz="15620"/><p:restoredTop sz="94660"/></p:normalViewPr><p:gridSpacing cx="72008" cy="72008"/></p:viewPr>"#).as_bytes())?;
    put(&mut zw, "ppt/tableStyles.xml", br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<a:tblStyleLst xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" def="{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}"/>"#)?;
    put(&mut zw, "ppt/theme/theme1.xml", THEME.as_bytes())?;
    put(&mut zw, "ppt/slideMasters/slideMaster1.xml", MASTER.as_bytes())?;
    put(&mut zw, "ppt/slideMasters/_rels/slideMaster1.xml.rels", br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="../theme/theme1.xml"/></Relationships>"#)?;
    put(&mut zw, "ppt/slideLayouts/slideLayout1.xml", LAYOUT.as_bytes())?;
    put(&mut zw, "ppt/slideLayouts/_rels/slideLayout1.xml.rels", br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="../slideMasters/slideMaster1.xml"/></Relationships>"#)?;
    for (i, s) in slides.iter().enumerate() {
        let i = i + 1;
        let mut rels = String::from(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>"#);
        if let Out::Image { bytes, ext, .. } = s {
            let media = format!("image{i}.{ext}");
            put(&mut zw, &format!("ppt/media/{media}"), bytes)?;
            rels.push_str(&format!(r#"<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/{media}"/>"#));
        }
        rels.push_str("</Relationships>");
        put(&mut zw, &format!("ppt/slides/slide{i}.xml"), slide_xml(s, Some("rId2")).as_bytes())?;
        put(&mut zw, &format!("ppt/slides/_rels/slide{i}.xml.rels"), rels.as_bytes())?;
    }
    let cur = zw.finish().map_err(|e| e.to_string())?;
    Ok(cur.into_inner())
}

/// 내보낼 슬라이드 한 장 (그림은 아직 읽지 않음)
#[derive(Debug, Clone)]
pub enum Plan {
    Ready(Out),
    Image { src: Src, bg: String },
}

/// 조작 화면이 만든 슬라이드 하나 → Plan (못 넣는 것은 None)
pub fn slide_plan(core: &Core, s: &Value) -> Option<Plan> {
    let kind = s["kind"].as_str().unwrap_or("");
    let parts = s["parts"].as_array().cloned().unwrap_or_default();
    let text = |body: String, caption: String, title: bool| Some(Plan::Ready(Out::Text { body, caption, title }));
    match kind {
        "bible" => {
            let body = parts
                .iter()
                .map(|p| {
                    let t = p["text"].as_str().unwrap_or("").trim().to_string();
                    match p["verse"].as_i64() {
                        Some(v) if v > 0 => format!("{v} {t}"),
                        _ => t,
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            text(plain(&body), s["ref"].as_str().unwrap_or("").to_string(), false)
        }
        "text" => text(plain(&parts.iter().filter_map(|p| p["text"].as_str()).collect::<Vec<_>>().join("\n")), String::new(), false),
        "title" => text(s["ref"].as_str().unwrap_or("").to_string(), s["subtitle"].as_str().unwrap_or("").to_string(), true),
        "image" | "score" => {
            let src = image_source(core, s["image"].as_str().unwrap_or(""))?;
            let bg = if kind == "score" { s["bg"].as_str().filter(|b| b.starts_with('#')).map(hex_of).unwrap_or_else(|| "FFFFFF".into()) } else { "000000".into() };
            Some(Plan::Image { src, bg })
        }
        "blank" => Some(Plan::Ready(Out::Blank)),
        _ => None, // camera · video · 그 밖
    }
}

/// 내보내기 계획: (큐시트 이름, 슬라이드, 넣은 순서 수, 뺀 순서 글, 못 넣은 장 수)
pub struct CuePlan {
    pub name: String,
    pub slides: Vec<Plan>,
    pub items: usize,
    pub skipped: Vec<String>,
    pub dropped: usize,
}

impl Core {
    /// 열린 큐시트 → 내보내기 계획 (조작 화면 잠금 안에서: 슬라이드 나눔과 그림 위치만)
    pub fn cue_pptx_plan(&mut self) -> Result<CuePlan, String> {
        let Some(cue) = self.cue.clone() else { return Err(crate::i18n::tr("큐시트를 먼저 여세요.").into()) };
        let mut p = CuePlan { name: cue.name.clone(), slides: vec![], items: 0, skipped: vec![], dropped: 0 };
        for (i, it) in cue.items.iter().enumerate() {
            let title = it["title"].as_str().unwrap_or("").to_string();
            let ty = it["type"].as_str().unwrap_or("");
            if matches!(ty, "audio" | "video" | "camera") {
                p.skipped.push(format!("{title}: {}", crate::i18n::tr("영상·음악·카메라는 PPT 에 넣지 않습니다")));
                continue;
            }
            match self.item_deck(i) {
                Ok(deck) => {
                    let before = p.slides.len();
                    for s in &deck.slides {
                        match slide_plan(self, s) {
                            Some(x) => p.slides.push(x),
                            None => p.dropped += 1,
                        }
                    }
                    if p.slides.len() > before {
                        p.items += 1;
                    } else {
                        p.skipped.push(format!("{title}: {}", crate::i18n::tr("넣을 수 있는 슬라이드가 없습니다")));
                    }
                }
                Err(e) => p.skipped.push(format!("{title}: {e}")),
            }
        }
        if p.slides.is_empty() {
            return Err(crate::i18n::tr("PPT 에 넣을 슬라이드가 없습니다.").into());
        }
        Ok(p)
    }

    /// 열린 큐시트 → PPT 파일 (한 번에. 화면에서는 cue_pptx_plan 뒤 finish_pptx 를 다른 스레드에서)
    pub fn export_cue_pptx(&mut self, path: &Path) -> Result<Value, String> {
        let plan = self.cue_pptx_plan()?;
        finish_pptx(plan, path)
    }
}

/// 계획 → 그림 읽기 · PPT 쓰기 (조작 화면 잠금 밖에서). 결과 { slides, items, skipped, dropped, path }
pub fn finish_pptx(plan: CuePlan, path: &Path) -> Result<Value, String> {
    let mut dropped = plan.dropped;
    let mut outs: Vec<Out> = vec![];
    for x in plan.slides {
        match x {
            Plan::Ready(o) => outs.push(o),
            Plan::Image { src, bg } => match load_image(&src) {
                Some((bytes, ext)) => {
                    let (w, h) = dims(&bytes);
                    outs.push(Out::Image { bytes, ext, bg, w, h });
                }
                None => dropped += 1,
            },
        }
    }
    if outs.is_empty() {
        return Err(crate::i18n::tr("PPT 에 넣을 슬라이드가 없습니다.").into());
    }
    let bytes = write_pptx(&outs, &plan.name)?;
    // 임시 파일에 쓴 뒤 바꾼다. PowerPoint 가 같은 파일을 열고 있으면 바꾸기가 실패하므로 그대로 알린다
    let tmp: PathBuf = path.with_extension("pptx.tmp");
    std::fs::write(&tmp, &bytes).map_err(|e| format!("{}: {e}", crate::i18n::tr("파일을 쓰지 못했습니다")))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("{}: {e}", crate::i18n::tr("파일을 쓰지 못했습니다 (PowerPoint 에서 열려 있으면 닫고 다시)"))
    })?;
    Ok(json!({ "slides": outs.len(), "items": plan.items, "skipped": plan.skipped, "dropped": dropped, "path": path.to_string_lossy() }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_and_size() {
        assert_eq!(pct_decode("C%3A%5C%EC%9D%80%ED%98%9C.png"), "C:\\은혜.png");
        assert_eq!(pct_decode("a%2"), "a%2");
        assert_eq!(font_size("한 줄", false), 4800);
        assert!(font_size("가\n나\n다\n라\n마\n바\n사\n아\n자", false) <= 2400);
        assert!(font_size(&"가".repeat(50), false) <= 2600);
        assert_eq!(esc("<a & \"b\">\u{1}"), "&lt;a &amp; &quot;b&quot;&gt;");
    }

    /// 큐시트 하나를 통째로: 찬양(부를 순서) · 제목 · 공지 · 그림 · 빈 화면은 넣고, 음악 · 장절이 빈 성경은 빼고 알린다
    #[test]
    fn cue_to_pptx() {
        let dir = std::env::temp_dir().join(format!("wp-pptxcue-{}", crate::util::short_id("t")));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("bible.db"), b"").unwrap();
        let mut c = Core::open(dir.clone(), dir.clone(), None).unwrap();
        let img = dir.join("광고.png");
        image::RgbaImage::from_pixel(160, 90, image::Rgba([0, 128, 255, 255])).save(&img).unwrap();
        c.new_cue("주일 예배").unwrap();
        let mut song = crate::core::new_item("song", "은혜");
        song["lyrics"] = json!("[1절]\n내가 누려왔던\n\n[후렴]\n그 모든 것이 은혜");
        song["order"] = json!("1절 후렴 후렴");
        let mut title = crate::core::new_item("title", "설교");
        title["subtitle"] = json!("은혜의 삶");
        let mut text = crate::core::new_item("text", "광고");
        text["body"] = json!("다음 주 성찬식");
        let mut pic = crate::core::new_item("image", "광고 그림");
        pic["image"] = json!(img.to_string_lossy());
        let audio = crate::core::new_item("audio", "반주");
        let bible = crate::core::new_item("bible", "성경");
        for it in [song, title, text, pic, crate::core::new_item("blank", "빈 화면"), audio, bible] {
            c.add_item(it, None);
        }
        let f = dir.join("내보낸 예배.pptx");
        let r = c.export_cue_pptx(&f).unwrap();
        assert_eq!(r["items"], 5, "{r}");
        assert_eq!(r["skipped"].as_array().unwrap().len(), 2, "음악 · 빈 성경: {r}");
        let n = r["slides"].as_u64().unwrap();
        assert!(n >= 6, "찬양 3장(1절 후렴 후렴) + 제목 + 공지 + 그림 + 빈 화면: {r}");
        let meta = crate::pptx::build::build_cache(&dir, &f.to_string_lossy()).unwrap();
        assert_eq!(meta["slides"].as_array().map(|a| a.len() as u64), Some(n));
        assert!(meta.to_string().contains("그 모든 것이 은혜"));
        assert!(!dir.join("내보낸 예배.pptx.tmp").exists());
        drop(c);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// 만든 PPT 를 이 프로그램의 PPT 읽기(직접 그리기)로 다시 읽어 장 수와 글을 확인한다
    #[test]
    fn written_pptx_reads_back() {
        let mut png = std::io::Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(40, 30, image::Rgba([255, 0, 0, 255])).write_to(&mut png, image::ImageFormat::Png).unwrap();
        let slides = vec![
            Out::Text { body: "새 예배".into(), caption: "2026-11-29".into(), title: true },
            Out::Text { body: "내가 누려왔던 모든 것들이\n내가 지나왔던 모든 시간이".into(), caption: String::new(), title: false },
            Out::Text { body: "16 하나님이 세상을 이처럼 사랑하사 <&>".into(), caption: "요한복음 3:16".into(), title: false },
            Out::Image { bytes: png.into_inner(), ext: "png", bg: "FFFFFF".into(), w: 40, h: 30 },
            Out::Blank,
        ];
        let bytes = write_pptx(&slides, "점검 & 예배").unwrap();
        let dir = std::env::temp_dir().join(format!("wp-pptxout-{}", crate::util::short_id("t")));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("out.pptx");
        std::fs::write(&f, &bytes).unwrap();
        // zip 구조
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes.clone())).unwrap();
        for p in ["[Content_Types].xml", "ppt/presentation.xml", "ppt/slides/slide5.xml", "ppt/media/image4.png", "ppt/slides/_rels/slide4.xml.rels"] {
            assert!(z.by_name(p).is_ok(), "{p}");
        }
        // 모든 XML 이 올바른 XML 인지
        for i in 0..z.len() {
            let mut e = z.by_index(i).unwrap();
            let name = e.name().map(|x| x.to_string()).unwrap_or_default();
            if name.ends_with(".xml") || name.ends_with(".rels") {
                let mut s = String::new();
                std::io::Read::read_to_string(&mut e, &mut s).unwrap();
                roxmltree::Document::parse(&s).unwrap_or_else(|x| panic!("{name}: {x}"));
            }
        }
        // 이 프로그램의 PPT 직접 읽기로 다시 읽기
        let meta = crate::pptx::build::build_cache(&dir, &f.to_string_lossy()).unwrap();
        assert_eq!(meta["slides"].as_array().map(|a| a.len()), Some(5), "{meta}");
        let texts = meta.to_string();
        assert!(texts.contains("내가 누려왔던"), "{texts}");
        assert!(texts.contains("요한복음 3:16"), "{texts}");
        let _ = std::fs::remove_dir_all(dir);
    }
}
