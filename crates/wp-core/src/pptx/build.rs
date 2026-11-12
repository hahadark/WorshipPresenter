//! [트랙 A] .pptx/.ppsx → 캐시 폴더 (Electron 판 pptxbuild.js 의 직접 쓴 파서 경로)
//!   slide-NNN.svg / .png      슬라이드 그림 (PowerPoint 없이 '화면 그대로' 슬라이드)
//!   slide-NNN-bg.svg / .png   꺼낸 요소를 뺀 배경 그림
//!   slide-NNN-eMM.svg         요소 하나짜리 그림 (선·그라데이션·특수 모양)
//!   media-N.ext               PPT 안의 그림 그대로 (SVG 는 이 이름을 href 로 — 서버가 보낼 때 data: 로 넣음)
//!   layers.json               { v:4, w, h, engine:'rs', bgs, slides:[[요소…]], hide:{}, texts }
//!   meta.json                 hymns 캐시 모양 (mode 'pptx')

use super::svg::{element_svg, slide_svg};
use super::{slide_text, Doc, Fill, Shape};
use once_cell::sync::Lazy;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

fn r2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn ext_of(part: &str) -> String {
    part.rsplit('.').next().unwrap_or("").to_lowercase()
}
fn ok_ext(e: &str) -> bool {
    matches!(e, "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "svg")
}

/// 글상자 기록 (pptlayers 가 편집기 글상자로 바꾼다)
fn text_record(sh: &Shape) -> Option<Value> {
    let t = sh.text.as_ref()?;
    let runs: Vec<&super::Run> = t.paras.iter().flat_map(|p| p.runs.iter()).filter(|r| !r.br && !r.text.trim().is_empty()).collect();
    let main = runs.iter().max_by_key(|r| r.text.trim().chars().count())?;
    let first = t.paras.iter().find(|p| p.runs.iter().any(|r| !r.text.trim().is_empty())).unwrap_or(&t.paras[0]);
    let mut lh = match (first.ln_spc_pct, first.ln_spc_pts) {
        (Some(p), _) => p,
        (_, Some(pts)) if main.sz > 0.0 => pts / main.sz / 1.2,
        _ => 0.0,
    };
    let tps: Vec<&super::Para> = t.paras.iter().filter(|p| p.runs.iter().any(|r| !r.br && !r.text.trim().is_empty())).collect();
    if tps.len() > 1 && main.sz > 0.0 {
        let gaps: Vec<f64> = (1..tps.len()).map(|i| tps[i].spc_bef + tps[i - 1].spc_aft).collect();
        let avg = gaps.iter().sum::<f64>() / gaps.len() as f64;
        let short = tps.iter().filter(|p| p.runs.iter().map(|r| r.text.as_str()).collect::<String>().trim().chars().count() <= 40).count() as f64 / tps.len() as f64;
        if avg > 0.0 && short >= 0.6 {
            lh = if lh > 0.0 { lh } else { 1.0 } + (avg / main.sz / 1.2).min(1.5);
        }
    }
    let align = match first.algn.as_str() { "ctr" | "dist" => 2, "r" => 3, _ => 1 };
    let anchor = match t.body.anchor.as_str() { "ctr" => 3, "b" => 4, _ => 1 };
    Some(json!({
        "kind": "text", "id": sh.id, "l": r2(sh.x), "t": r2(sh.y), "w": r2(sh.w), "h": r2(sh.h),
        "text": t.lines().join("\n").trim_end(), "size": r2(if main.sz > 0.0 { main.sz } else { 18.0 }), "bold": main.b, "italic": main.i,
        "color": main.color.map(|c| c.hex()).unwrap_or("#000000".into()), "font": main.latin, "fontEa": main.ea, "align": align, "anchor": anchor,
        "ml": r2(t.body.l_ins), "mr": r2(t.body.r_ins), "mt": r2(t.body.t_ins), "mb": r2(t.body.b_ins), "lh": r2(lh), "wrap": if t.body.wrap { 1 } else { 0 },
    }))
}

/// 도형을 어떻게 다룰지: skip · bg · text · pic · shape · element
pub fn classify(sh: &Shape, pkg_has: &dyn Fn(&str) -> bool) -> &'static str {
    if sh.origin != "slide" {
        return "bg";
    }
    if sh.rot % 360.0 != 0.0 || sh.in_rot_group {
        return "bg";
    }
    if sh.ph.as_ref().is_some_and(|p| ["sldNum", "ftr", "dt"].contains(&p.ty.as_str())) {
        return "bg";
    }
    if ["table", "chart", "diagram", "graphic", "ole"].contains(&sh.kind.as_str()) {
        return if (sh.kind == "chart" && sh.chart.is_none() || sh.kind == "graphic") && sh.pic.is_none() { "skip" } else { "bg" }; // [트랙 A] 읽은 차트는 배경에 그림
    }
    let has_text = sh.text.as_ref().is_some_and(|t| t.has_text);
    if has_text && sh.text.as_ref().is_some_and(|t| t.body.vert != "horz") {
        return "bg";
    }
    if sh.kind == "pic" {
        let Some(Fill::Blip { part, .. }) = &sh.pic else { return "skip" };
        let ok = part.as_deref().is_some_and(|p| ok_ext(&ext_of(p)) && pkg_has(p));
        if !ok || sh.flip_h || sh.flip_v {
            return "bg";
        }
        if !sh.prst.is_empty() && sh.prst != "rect" {
            return "element";
        }
        return "pic";
    }
    let body = sh.fill.is_some() || sh.line.is_some();
    if !body {
        return if has_text { "text" } else { "skip" };
    }
    if sh.flip_h || sh.flip_v {
        return "bg";
    }
    if ["rect", "roundRect", "ellipse", "flowChartProcess", "flowChartConnector"].contains(&sh.prst.as_str()) && matches!(sh.fill, Some(Fill::Solid(_))) && sh.line.is_none() {
        return "shape";
    }
    "element"
}

static FONTDB: Lazy<Arc<resvg::usvg::fontdb::Database>> = Lazy::new(|| {
    let mut db = resvg::usvg::fontdb::Database::new();
    db.load_system_fonts();
    db.set_sans_serif_family("Malgun Gothic");
    Arc::new(db)
});

/// 시스템 글꼴 목록을 미리 읽어 둔다 (처음 그리기가 몇 초 늦어지지 않게, 시작할 때 백그라운드에서)
pub fn warm_fonts() {
    std::thread::Builder::new().name("font-warm".into()).spawn(|| {
        let t0 = std::time::Instant::now();
        let db = &*FONTDB;
        // 맑은 고딕 파일도 한 번 열어 디스크 캐시에 올린다
        let _ = crate::measure::text_width("가", 10.0, false, 0.0);
        log::info!("글꼴 {}개 미리 읽음 ({:.1}초)", db.len(), t0.elapsed().as_secs_f32());
    }).ok();
}

/// 시스템 글꼴 하나의 파일 내용 (조작 화면 egui 가 PPT 글꼴로 그릴 때). 이름은 '구세군체 Bold' 처럼 전체 이름도 받는다
pub fn font_bytes(name: &str) -> Option<(Vec<u8>, u32)> {
    use resvg::usvg::fontdb::{Family, Query, Weight};
    let db = &*FONTDB;
    let want = name.trim();
    let bold = want.to_lowercase().ends_with(" bold");
    let base = if bold { want[..want.len() - 5].trim() } else { want };
    let id = db
        .faces()
        .find(|f| f.families.iter().any(|(n, _)| n.eq_ignore_ascii_case(want)) || f.post_script_name.eq_ignore_ascii_case(&want.replace(' ', "")))
        .map(|f| f.id)
        .or_else(|| db.query(&Query { families: &[Family::Name(base)], weight: if bold { Weight::BOLD } else { Weight::NORMAL }, ..Default::default() }))?;
    let mut out = None;
    db.with_face_data(id, |data, index| out = Some((data.to_vec(), index)));
    out
}

/// SVG 글 → PNG (가로 width px). 그림 href 는 dir 기준으로 읽는다
pub fn render_png(svg: &str, dir: &Path, width: u32) -> Result<Vec<u8>, String> {
    let mut opt = resvg::usvg::Options { resources_dir: Some(dir.to_path_buf()), ..Default::default() };
    opt.fontdb = FONTDB.clone();
    let tree = resvg::usvg::Tree::from_str(svg, &opt).map_err(|e| e.to_string())?;
    let sz = tree.size();
    let scale = width as f32 / sz.width().max(1.0);
    let h = (sz.height() * scale).round().max(1.0) as u32;
    let mut pm = resvg::tiny_skia::Pixmap::new(width, h).ok_or("그림 크기 오류")?;
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pm.as_mut());
    pm.encode_png().map_err(|e| e.to_string())
}

/// 파일 → 캐시 폴더에 그림·요소를 만들고 meta 를 돌려준다
pub fn build_cache(data_dir: &Path, file: &str) -> Result<Value, String> {
    let key = crate::hymns::cache_key(file, "pptx-rs1");
    let dir = data_dir.join("hymn-cache").join(&key);
    if let Ok(t) = std::fs::read_to_string(dir.join("meta.json")) {
        if let Ok(v) = serde_json::from_str::<Value>(&t) {
            return Ok(v);
        }
    }
    let bytes = std::fs::read(file).map_err(|e| format!("파일을 읽지 못했습니다: {e}"))?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let r = build_into(&bytes, &dir)?;
    let (number, title, name) = crate::hymns::parse_name(Path::new(file));
    let url = |f: &str| format!("/hymn/{key}/{f}");
    let meta = json!({
        "key": key, "number": number, "title": title, "name": name, "file": file, "mode": "pptx", "background": null, "scores": [], "texts": [],
        "slides": r.slides.iter().map(|f| url(f)).collect::<Vec<_>>(), "slideTexts": r.texts, "aspect": r.w / r.h, "layers": true,
    });
    std::fs::write(dir.join("meta.json"), meta.to_string()).map_err(|e| e.to_string())?;
    Ok(meta)
}

pub struct Built {
    pub w: f64,
    pub h: f64,
    pub slides: Vec<String>,
    pub texts: Vec<String>,
}

/// 실제 일: pptx 바이트 → dir 에 파일들
pub fn build_into(bytes: &[u8], dir: &Path) -> Result<Built, String> {
    let mut doc = Doc::open(bytes)?;
    if doc.slides.is_empty() {
        return Err("슬라이드가 없는 파일입니다.".into());
    }
    // 그림 복사 (같은 그림은 한 번)
    let mut names: HashMap<String, String> = HashMap::new();
    let mut slides_out = vec![];
    let mut texts = vec![];
    let mut bgs = vec![];
    let mut recs_all = vec![];
    for k in 0..doc.slides.len() {
        let slide = match doc.slide(k) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("슬라이드 {} 를 읽지 못함: {e}", k + 1);
                continue;
            }
        };
        // 이 슬라이드가 쓰는 그림을 먼저 캐시에
        let mut parts: Vec<String> = vec![];
        let mut add = |f: &Option<Fill>| {
            if let Some(Fill::Blip { part: Some(p), .. }) = f {
                parts.push(p.clone());
            }
        };
        add(&Some(slide.bg.clone()));
        for sh in &slide.shapes {
            add(&sh.fill);
            add(&sh.pic);
        }
        for p in parts {
            if names.contains_key(&p) || !ok_ext(&ext_of(&p)) {
                continue;
            }
            if let Some(b) = doc.pkg.raw(&p) {
                let e = ext_of(&p);
                let name = format!("media-{}.{}", names.len() + 1, if e == "jpeg" { "jpg".into() } else { e });
                let _ = std::fs::write(dir.join(&name), b);
                names.insert(p, name);
            }
        }
        let embed = |p: &str| names.get(p).cloned();
        let has = |p: &str| names.contains_key(p);
        let tag = format!("{:03}", k + 1);
        let mut recs = vec![];
        let mut skip = vec![false; slide.shapes.len()];
        let mut m = 0;
        for (i, sh) in slide.shapes.iter().enumerate() {
            let how = classify(sh, &has);
            if how == "bg" {
                continue;
            }
            skip[i] = true;
            if how == "skip" {
                continue;
            }
            let bx = (r2(sh.x), r2(sh.y), r2(sh.w), r2(sh.h));
            let body = match how {
                "pic" => {
                    let Some(Fill::Blip { part, crop, alpha }) = &sh.pic else { unreachable!() };
                    let mut v = json!({ "kind": "image", "id": sh.id, "l": bx.0, "t": bx.1, "w": bx.2, "h": bx.3, "file": part.as_deref().and_then(embed), "fit": "cover", "opacity": alpha });
                    if let Some(c) = crop {
                        v["crop"] = json!({ "l": c[0], "t": c[1], "r": c[2], "b": c[3] });
                    }
                    Some(v)
                }
                "shape" => {
                    let Some(Fill::Solid(c)) = &sh.fill else { unreachable!() };
                    let ss = sh.w.min(sh.h);
                    Some(json!({ "kind": "shape", "id": sh.id, "l": bx.0, "t": bx.1, "w": bx.2, "h": bx.3, "fill": c.hex(), "ellipse": sh.prst == "ellipse" || sh.prst == "flowChartConnector", "radius": if sh.prst == "roundRect" { r2(ss * sh.adj / 100000.0) } else { 0.0 }, "opacity": c.a }))
                }
                "element" => element_svg(sh, &embed).map(|svg| {
                    m += 1;
                    let f = format!("slide-{tag}-e{m:02}.svg");
                    let _ = std::fs::write(dir.join(&f), svg);
                    let pad = sh.line.as_ref().map(|l| l.w).unwrap_or(0.0) + 1.0;
                    json!({ "kind": "image", "id": sh.id, "l": r2(sh.x - pad), "t": r2(sh.y - pad), "w": r2(sh.w + 2.0 * pad), "h": r2(sh.h + 2.0 * pad), "file": f, "fit": "contain", "opacity": 1 })
                }),
                _ => None,
            };
            let tr = if sh.text.as_ref().is_some_and(|t| t.has_text) { text_record(sh) } else { None };
            if let Some(b) = body {
                recs.push(b);
            }
            if let Some(t) = tr {
                recs.push(t);
            }
        }
        let full = slide_svg(&slide, &embed, &|_| false);
        let bg = slide_svg(&slide, &embed, &|i| skip[i]);
        let f_full = format!("slide-{tag}.svg");
        let f_bg = format!("slide-{tag}-bg.svg");
        std::fs::write(dir.join(&f_full), &full).map_err(|e| e.to_string())?;
        std::fs::write(dir.join(&f_bg), &bg).map_err(|e| e.to_string())?;
        // PNG (조작 화면 미리보기 · 출력 화면): 1920 폭
        let png = format!("slide-{tag}.png");
        match render_png(&full, dir, 1920) {
            Ok(b) => {
                let _ = std::fs::write(dir.join(&png), b);
                slides_out.push(png);
            }
            Err(e) => {
                log::warn!("PNG 를 만들지 못해 SVG 로: {e}");
                slides_out.push(f_full.clone());
            }
        }
        let bg_png = format!("slide-{tag}-bg.png");
        bgs.push(match render_png(&bg, dir, 1920) {
            Ok(b) => {
                let _ = std::fs::write(dir.join(&bg_png), b);
                bg_png
            }
            Err(_) => f_bg,
        });
        texts.push(slide_text(&slide));
        recs_all.push(recs);
    }
    if slides_out.is_empty() {
        return Err("슬라이드를 그리지 못했습니다.".into());
    }
    let layers = json!({ "v": 4, "w": doc.w, "h": doc.h, "engine": "rs", "bgs": bgs, "slides": recs_all, "hide": {}, "texts": texts });
    std::fs::write(dir.join("layers.json"), layers.to_string()).map_err(|e| e.to_string())?;
    Ok(Built { w: doc.w, h: doc.h, slides: slides_out, texts })
}

#[cfg(test)]
mod tests {
    /// 실제 파일로: WP_PPTX=<파일> WP_OUT=<폴더> cargo test -p wp-core real_pptx -- --ignored
    #[test]
    #[ignore]
    fn real_pptx() {
        let f = std::env::var("WP_PPTX").unwrap();
        let out = std::path::PathBuf::from(std::env::var("WP_OUT").unwrap());
        std::fs::create_dir_all(&out).unwrap();
        let t0 = std::time::Instant::now();
        let r = super::build_into(&std::fs::read(&f).unwrap(), &out).unwrap();
        eprintln!("슬라이드 {}장 {:?} ({}x{}pt)", r.slides.len(), t0.elapsed(), r.w, r.h);
        for t in r.texts.iter().take(3) { eprintln!("글: {}", t.chars().take(80).collect::<String>()); }
    }
}
