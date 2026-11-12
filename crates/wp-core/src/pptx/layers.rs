//! [트랙 A] PPT 요소 따로 고치기: layers.json 의 요소 → 슬라이드 편집기의 글상자·그림·도형 (Electron 판 pptlayers.js)
//! 편집기 글상자 = { id:'p{k}t{n}', type:'text', x, y, w, h (화면 %), text, size (1080 줄 화면 px), … }

use serde_json::{json, Value};

fn r2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// BGR 정수(PowerPoint RGB) 또는 '#rrggbb' → #rrggbb
pub fn bgr_hex(c: &Value) -> String {
    if let Some(s) = c.as_str() {
        if s.len() == 7 && s.starts_with('#') && s[1..].chars().all(|c| c.is_ascii_hexdigit()) {
            return s.to_lowercase();
        }
    }
    match c.as_f64() {
        Some(n) if (0.0..=16777215.0).contains(&n) => {
            let n = n as u32;
            format!("#{:02x}{:02x}{:02x}", n & 255, (n >> 8) & 255, (n >> 16) & 255)
        }
        _ => "#ffffff".into(),
    }
}

pub fn font_name(s: &str) -> String {
    let t = s.trim();
    let ok = !t.is_empty() && t.chars().count() <= 60 && !t.starts_with('+') && t.chars().all(|c| c.is_alphanumeric() || " .+-()_".contains(c));
    if ok { t.to_string() } else { String::new() }
}

/// 슬라이드 그림이 화면에서 차지하는 자리 (%)
pub fn fit_rect(sw: f64, sh: f64, cw: f64, ch: f64) -> (f64, f64, f64, f64) {
    let (sa, ca) = (sw / sh, cw / ch);
    if !(sa > 0.0) || !(ca > 0.0) {
        return (0.0, 0.0, 100.0, 100.0);
    }
    if sa >= ca {
        let h = ca / sa * 100.0;
        (0.0, (100.0 - h) / 2.0, 100.0, h)
    } else {
        let w = sa / ca * 100.0;
        ((100.0 - w) / 2.0, 0.0, w, 100.0)
    }
}

fn is_file_name(f: &str) -> bool {
    let re = regex::Regex::new(r"^(slide-\d{3,4}-e\d{2,3}\.(png|svg)|media-\d{1,4}\.(png|jpe?g|gif|bmp|webp|svg))$").unwrap();
    re.is_match(f)
}

fn hangul(s: &str) -> bool {
    s.chars().any(|c| ('\u{ac00}'..='\u{d7a3}').contains(&c) || ('\u{3131}'..='\u{318e}').contains(&c))
}

/// 슬라이드 k 의 요소 → 편집기 항목들
pub fn to_items(raw: &Value, k: usize, screen: (f64, f64), url_base: &str) -> Vec<Value> {
    let w = raw["w"].as_f64().filter(|v| *v > 0.0).unwrap_or(960.0);
    let h = raw["h"].as_f64().filter(|v| *v > 0.0).unwrap_or(540.0);
    let list = raw["slides"][k].as_array().cloned().unwrap_or_default();
    let fr = fit_rect(w, h, screen.0.max(1.0), screen.1.max(1.0));
    let pt = fr.3 / 100.0 * 1080.0 / h;
    let rect = |b: &Value| {
        let n = |k: &str| b[k].as_f64().unwrap_or(0.0);
        json!({ "x": r2(fr.0 + n("l") / w * fr.2), "y": r2(fr.1 + n("t") / h * fr.3), "w": r2((n("w") / w * fr.2).max(1.0)), "h": r2((n("h") / h * fr.3).max(1.0)) })
    };
    let op = |v: &Value| v.as_f64().map(|x| x.clamp(0.05, 1.0)).unwrap_or(1.0);
    let crop_v = |v: &Value| r2(v.as_f64().unwrap_or(0.0).clamp(0.0, 0.95));
    let mut out = vec![];
    for (n, b) in list.iter().enumerate() {
        let mut it = rect(b);
        match b["kind"].as_str() {
            Some("image") => {
                let f = b["file"].as_str().unwrap_or("");
                if !is_file_name(f) || url_base.is_empty() {
                    continue;
                }
                it["id"] = json!(format!("p{k}i{n}"));
                it["type"] = json!("image");
                it["src"] = json!(format!("{url_base}{f}"));
                it["fit"] = json!(if b["fit"] == "cover" { "cover" } else { "contain" });
                it["opacity"] = json!(op(&b["opacity"]));
                it["radius"] = json!(0);
                it["below"] = json!(false);
                let c = &b["crop"];
                if c.is_object() && ["l", "t", "r", "b"].iter().any(|k| c[*k].as_f64().unwrap_or(0.0) > 0.0) {
                    it["crop"] = json!({ "l": crop_v(&c["l"]), "t": crop_v(&c["t"]), "r": crop_v(&c["r"]), "b": crop_v(&c["b"]) });
                }
            }
            Some("shape") => {
                it["id"] = json!(format!("p{k}s{n}"));
                it["type"] = json!("shape");
                it["fill"] = json!(bgr_hex(&b["fill"]));
                it["ellipse"] = json!(b["ellipse"].as_bool().unwrap_or(false));
                it["radius"] = json!(((b["radius"].as_f64().unwrap_or(0.0)) * pt).round().clamp(0.0, 1000.0));
                it["opacity"] = json!(op(&b["opacity"]));
                it["below"] = json!(false);
            }
            _ => {
                let text = b["text"].as_str().unwrap_or("").replace("\r\n", "\n").replace(['\r', '\u{b}'], "\n");
                let text = text.trim_end();
                if text.trim().is_empty() {
                    continue;
                }
                let (fa, fe) = (font_name(b["font"].as_str().unwrap_or("")), font_name(b["fontEa"].as_str().unwrap_or("")));
                let font = if hangul(text) { if fe.is_empty() { fa } else { fe } } else if fa.is_empty() { fe } else { fa };
                let lh = b["lh"].as_f64().filter(|v| *v > 0.0).map(|v| r2(1.2 * v).clamp(0.8, 3.0)).unwrap_or(1.2);
                let onoff = |v: &Value| v.as_bool().unwrap_or(false) || v.as_f64() == Some(-1.0);
                it["id"] = json!(format!("p{k}t{n}"));
                it["type"] = json!("text");
                it["text"] = json!(text.chars().take(2000).collect::<String>());
                it["size"] = json!((((b["size"].as_f64().unwrap_or(18.0)) * pt * 10.0).round() / 10.0).clamp(8.0, 400.0));
                it["color"] = json!(bgr_hex(&b["color"]));
                it["bold"] = json!(onoff(&b["bold"]));
                it["italic"] = json!(onoff(&b["italic"]));
                it["align"] = json!(match b["align"].as_i64() { Some(2) => "center", Some(3) => "right", _ => "left" });
                it["valign"] = json!(match b["anchor"].as_i64() { Some(3) => "middle", Some(4) | Some(5) => "bottom", _ => "top" });
                it["padding"] = json!(((b["ml"].as_f64().unwrap_or(7.2)) * pt).round().clamp(0.0, 300.0));
                if !font.is_empty() {
                    it["font"] = json!(font);
                }
                it["fit"] = json!(true);
                it["shadow"] = json!(false);
                it["lineHeight"] = json!(lh);
                it["opacity"] = json!(1);
                it["radius"] = json!(0);
                it["below"] = json!(false);
            }
        }
        out.push(it);
    }
    out
}

pub fn all_items(raw: &Value, screen: (f64, f64), url_base: &str) -> Vec<Vec<Value>> {
    let n = raw["slides"].as_array().map(|a| a.len()).unwrap_or(0);
    (0..n).map(|k| to_items(raw, k, screen, url_base)).collect()
}

pub fn is_ppt_item(id: &str) -> bool {
    regex::Regex::new(r"^p\d+[tis]\d+$").unwrap().is_match(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn items() {
        assert_eq!(bgr_hex(&json!(255)), "#ff0000");
        assert_eq!(fit_rect(16.0, 9.0, 16.0, 9.0), (0.0, 0.0, 100.0, 100.0));
        let raw = json!({ "w": 960, "h": 540, "slides": [[{ "kind": "text", "l": 96, "t": 54, "w": 480, "h": 100, "text": "주님 찬양", "size": 40, "color": "#ffffff", "align": 2, "anchor": 3 }, { "kind": "image", "l": 0, "t": 0, "w": 10, "h": 10, "file": "media-1.png" }]] });
        let it = to_items(&raw, 0, (1920.0, 1080.0), "/hymn/abc/");
        assert_eq!(it.len(), 2);
        assert_eq!(it[0]["id"], "p0t0");
        assert_eq!(it[0]["x"], 10.0);
        assert_eq!(it[0]["size"], 80.0);
        assert_eq!(it[0]["align"], "center");
        assert_eq!(it[1]["src"], "/hymn/abc/media-1.png");
        assert!(is_ppt_item("p3t12"));
    }
}
