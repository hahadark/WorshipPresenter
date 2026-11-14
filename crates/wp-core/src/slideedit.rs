//! [트랙 C] 슬라이드 편집 (Electron core/slideedit.js · control/cue.js decorate · shared/slides.js slideOp 등)
//! 큐시트 순서의 edits = { '*': 공통, '0': 첫 슬라이드, 'x<id>': 덧붙인 제목 카드 } · skip · slideOrder · extra
//! 데이터 모양은 Electron 판과 같아 웹 출력(render.js)이 그대로 그린다.

use crate::hymns::merge_edit;
use crate::slides;
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::{json, Map, Value};

fn num(v: &Value, a: f64, b: f64, d: f64) -> f64 {
    v.as_f64().filter(|x| x.is_finite()).map(|x| x.clamp(a, b)).unwrap_or(d)
}
static COLOR: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)^#[0-9a-f]{3,8}$").unwrap());
fn color(v: &Value) -> Option<String> {
    v.as_str().filter(|s| COLOR.is_match(s)).map(String::from)
}
fn pick(v: &Value, list: &[&str]) -> Option<String> {
    v.as_str().filter(|s| list.contains(s)).map(String::from)
}

/// 글자 슬라이드 종류 (편집 열쇠 = 글 덩어리 번호 src)
pub fn is_chunk(ty: &str) -> bool {
    matches!(ty, "song" | "text" | "title")
}

/// 원래 슬라이드 차례 (끌어서 바꾼 slideOrder, 빠진 번호는 뒤에)
pub fn order_of(it: &Value, n: usize) -> Vec<usize> {
    let mut out = vec![];
    for k in it["slideOrder"].as_array().into_iter().flatten().filter_map(|x| x.as_u64()) {
        let k = k as usize;
        if k < n && !out.contains(&k) {
            out.push(k);
        }
    }
    for k in 0..n {
        if !out.contains(&k) {
            out.push(k);
        }
    }
    out
}

fn extra_slide(e: &Value) -> Value {
    let mut s = slides::title_slide(e["text"].as_str().unwrap_or(""), 1);
    s["label"] = json!("덧붙임");
    s["ekey"] = json!(format!("x{}", e["id"].as_str().unwrap_or("")));
    s["xid"] = e["id"].clone();
    if let Some(o) = s.as_object_mut() {
        o.remove("src");
    }
    s
}

/// 꾸미기: 건너뛰기 · 차례 · 덧붙인 제목 카드 · 슬라이드별 편집(edit · bg). with_skipped 면 건너뛴 것도 skipped 표시로 남긴다
pub fn decorate(it: &Value, slides: Vec<Value>, with_skipped: bool) -> Vec<Value> {
    let ty = it["type"].as_str().unwrap_or("");
    let mut out: Vec<Value>;
    if is_chunk(ty) {
        out = slides;
        for s in out.iter_mut() {
            let src = s["src"].as_u64().unwrap_or(0);
            s["src"] = json!(src);
            s["ekey"] = json!(src.to_string());
        }
    } else {
        let n = slides.len();
        let skip: Vec<u64> = it["skip"].as_array().into_iter().flatten().filter_map(|x| x.as_u64()).collect();
        let extra = it["extra"].as_array().cloned().unwrap_or_default();
        let extra_at = |at: i64| extra.iter().filter(|e| e["at"].as_i64() == Some(at)).map(extra_slide).collect::<Vec<_>>();
        out = extra_at(-1);
        for k in order_of(it, n) {
            let mut s = slides[k].clone();
            s["ekey"] = json!(k.to_string());
            s["base"] = json!(k);
            if skip.contains(&(k as u64)) {
                if with_skipped {
                    s["skipped"] = json!(true);
                    out.push(s);
                }
            } else {
                out.push(s);
            }
            out.extend(extra_at(k as i64));
        }
        let last = n as i64 - 1;
        out.extend(extra.iter().filter(|e| e["at"].as_i64().is_some_and(|a| a > last)).map(extra_slide));
        if !with_skipped && !out.iter().any(|s| !s["skipped"].as_bool().unwrap_or(false)) {
            out = slides.into_iter().take(1).collect();
        }
    }
    if it["edits"].is_object() {
        for s in out.iter_mut() {
            let key = s["ekey"].as_str().unwrap_or("").to_string();
            let Some(mut e) = merge_edit(&it["edits"], &key) else { continue };
            if s["pptBgMissing"].as_bool().unwrap_or(false) {
                if let Some(items) = e["items"].as_array_mut() { items.retain(|x| !crate::pptx::layers::is_ppt_item(x["id"].as_str().unwrap_or(""))); }
            }
            if let Some(bg) = e["bg"].as_str().filter(|x| !x.is_empty()) {
                s["bg"] = json!(bg);
            }
            if let Some(o) = e.as_object_mut() {
                o.remove("bg");
            }
            if s["pptBgMissing"].as_bool().unwrap_or(false) {
                if let Some(items) = e["items"].as_array_mut() {
                    items.retain(|x| !crate::pptx::layers::is_ppt_item(x["id"].as_str().unwrap_or("")));
                }
            }
            s["edit"] = e;
        }
    }
    out
}

// ---------------- 정리 (normalizeEdit) ----------------
fn rect(r: &Value) -> Option<Map<String, Value>> {
    if !r.is_object() {
        return None;
    }
    let mut o = Map::new();
    o.insert("x".into(), json!(num(&r["x"], -50.0, 150.0, 0.0)));
    o.insert("y".into(), json!(num(&r["y"], -50.0, 150.0, 0.0)));
    o.insert("w".into(), json!(num(&r["w"], 1.0, 200.0, 50.0)));
    o.insert("h".into(), json!(num(&r["h"], 1.0, 200.0, 20.0)));
    Some(o)
}

fn style(s: &Value) -> Map<String, Value> {
    let mut o = Map::new();
    if !s.is_object() {
        return o;
    }
    if !s["size"].is_null() && s["size"] != "" {
        o.insert("size".into(), json!(num(&s["size"], 8.0, 400.0, 72.0)));
    }
    for b in ["autoFit", "bold", "italic", "shadow"] {
        if let Some(v) = s[b].as_bool() {
            o.insert(b.into(), json!(v));
        }
    }
    if let Some(c) = color(&s["color"]) { o.insert("color".into(), json!(c)); }
    if let Some(c) = color(&s["boxColor"]) { o.insert("boxColor".into(), json!(c)); }
    if let Some(a) = pick(&s["align"], &["left", "center", "right"]) { o.insert("align".into(), json!(a)); }
    if let Some(a) = pick(&s["valign"], &["top", "middle", "bottom"]) { o.insert("valign".into(), json!(a)); }
    if !s["lineHeight"].is_null() && s["lineHeight"] != "" { o.insert("lineHeight".into(), json!(num(&s["lineHeight"], 0.8, 3.0, 1.4))); }
    if !s["boxOpacity"].is_null() { o.insert("boxOpacity".into(), json!(num(&s["boxOpacity"], 0.0, 1.0, 0.6))); }
    if !s["radius"].is_null() { o.insert("radius".into(), json!(num(&s["radius"], 0.0, 400.0, 12.0))); }
    if !s["padding"].is_null() { o.insert("padding".into(), json!(num(&s["padding"], 0.0, 300.0, 24.0))); }
    o
}

static ID_OK: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)^[a-z0-9]{2,30}$").unwrap());
static FONT_OK: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[\w가-힣\u{3131}-\u{318e} .+\-()]{1,60}$").unwrap());

fn item(x: &Value, i: usize) -> Option<Value> {
    let ty = x["type"].as_str().filter(|t| ["text", "image", "shape", "camera"].contains(t))?;
    let r = rect(x).unwrap();
    let id = x["id"].as_str().filter(|s| ID_OK.is_match(s)).map(String::from).unwrap_or_else(|| format!("e{}{}", i, crate::util::to36(rand::random::<u32>() as u64)));
    let mut o = r;
    o.insert("id".into(), json!(id));
    o.insert("type".into(), json!(ty));
    o.insert("opacity".into(), json!(num(&x["opacity"], 0.05, 1.0, 1.0)));
    o.insert("radius".into(), json!(num(&x["radius"], 0.0, 1000.0, 0.0)));
    o.insert("below".into(), json!(x["below"].as_bool().unwrap_or(false)));
    let s = |k: &str, max: usize| json!(x[k].as_str().unwrap_or("").chars().take(max).collect::<String>());
    match ty {
        "text" => {
            o.insert("text".into(), s("text", 2000));
            o.insert("size".into(), json!(num(&x["size"], 8.0, 400.0, 60.0)));
            o.insert("color".into(), json!(color(&x["color"]).unwrap_or("#ffffff".into())));
            o.insert("fill".into(), json!(color(&x["fill"]).unwrap_or_default()));
            o.insert("bold".into(), json!(x["bold"].as_bool().unwrap_or(false)));
            o.insert("italic".into(), json!(x["italic"].as_bool().unwrap_or(false)));
            o.insert("align".into(), json!(pick(&x["align"], &["left", "center", "right"]).unwrap_or("center".into())));
            o.insert("valign".into(), json!(pick(&x["valign"], &["top", "middle", "bottom"]).unwrap_or("middle".into())));
            o.insert("padding".into(), json!(num(&x["padding"], 0.0, 300.0, 12.0)));
            if let Some(f) = x["font"].as_str().map(str::trim).filter(|f| FONT_OK.is_match(f)) {
                o.insert("font".into(), json!(f));
            }
            if x["fit"].as_bool().unwrap_or(false) { o.insert("fit".into(), json!(true)); }
            if x["shadow"] == json!(false) { o.insert("shadow".into(), json!(false)); }
            if !x["lineHeight"].is_null() && x["lineHeight"] != "" { o.insert("lineHeight".into(), json!(num(&x["lineHeight"], 0.8, 3.0, 1.3))); }
        }
        "image" => {
            o.insert("src".into(), s("src", 1500));
            o.insert("fit".into(), json!(if x["fit"] == "contain" { "contain" } else { "cover" }));
            let c = &x["crop"];
            if c.is_object() {
                let (l, t, r, b) = (num(&c["l"], 0.0, 0.95, 0.0), num(&c["t"], 0.0, 0.95, 0.0), num(&c["r"], 0.0, 0.95, 0.0), num(&c["b"], 0.0, 0.95, 0.0));
                if (l + t + r + b) > 0.0 && l + r < 0.96 && t + b < 0.96 {
                    o.insert("crop".into(), json!({ "l": l, "t": t, "r": r, "b": b }));
                }
            }
        }
        "camera" => {
            o.insert("deviceId".into(), s("deviceId", 200));
            o.insert("label".into(), s("label", 100));
            o.insert("fit".into(), json!(if x["fit"] == "contain" { "contain" } else { "cover" }));
        }
        _ => {
            o.insert("fill".into(), json!(color(&x["fill"]).unwrap_or("#1d3557".into())));
            o.insert("ellipse".into(), json!(x["ellipse"].as_bool().unwrap_or(false)));
        }
    }
    Some(Value::Object(o))
}

/// 한 슬라이드 편집 정리 (빈 편집이면 None)
pub fn normalize_edit(e: &Value) -> Option<Value> {
    if !e.is_object() {
        return None;
    }
    let mut o = Map::new();
    if let Some(b) = rect(&e["box"]) { o.insert("box".into(), Value::Object(b)); }
    let st = style(&e["style"]);
    if !st.is_empty() { o.insert("style".into(), Value::Object(st)); }
    for b in ["hideMain", "allOutputs", "pptBg"] {
        if e[b].as_bool().unwrap_or(false) { o.insert(b.into(), json!(true)); }
    }
    if let Some(bg) = e["bg"].as_str().filter(|x| !x.is_empty()) {
        o.insert("bg".into(), json!(bg.chars().take(1500).collect::<String>()));
    }
    let items: Vec<Value> = e["items"].as_array().into_iter().flatten().take(60).enumerate().filter_map(|(i, x)| item(x, i)).collect();
    if !items.is_empty() { o.insert("items".into(), json!(items)); }
    if o.is_empty() { None } else { Some(Value::Object(o)) }
}

static KEY_OK: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)^(\*|\d{1,4}|x[a-z0-9]{1,16})$").unwrap());

/// 모든 편집 정리 (빈 것은 지움)
pub fn normalize_edits(edits: &Value) -> Option<Value> {
    let m = edits.as_object()?;
    let mut out = Map::new();
    for (k, v) in m.iter().take(1200) {
        if !KEY_OK.is_match(k) { continue; }
        if let Some(e) = normalize_edit(v) { out.insert(k.clone(), e); }
    }
    if out.is_empty() { None } else { Some(Value::Object(out)) }
}

/// 순서의 편집 하나를 바꾼다 (f 가 고침). 빈 편집이면 지운다
pub fn update_edit(it: &mut Value, key: &str, f: impl FnOnce(&mut Value)) {
    if !it["edits"].is_object() {
        it["edits"] = json!({});
    }
    let mut e = it["edits"][key].clone();
    if !e.is_object() { e = json!({}); }
    f(&mut e);
    match normalize_edit(&e) {
        Some(n) => it["edits"][key] = n,
        None => { it["edits"].as_object_mut().unwrap().remove(key); }
    }
    if it["edits"].as_object().is_some_and(|m| m.is_empty()) {
        it.as_object_mut().unwrap().remove("edits");
    }
}

/// 덧붙일 새 요소
pub fn new_item(ty: &str) -> Value {
    let id = format!("u{}", crate::util::to36(rand::random::<u32>() as u64));
    match ty {
        "image" => json!({ "id": id, "type": "image", "x": 30, "y": 25, "w": 40, "h": 40, "src": "", "fit": "contain" }),
        "shape" => json!({ "id": id, "type": "shape", "x": 10, "y": 70, "w": 80, "h": 20, "fill": "#1d3557", "opacity": 0.8, "below": true }),
        _ => json!({ "id": id, "type": "text", "x": 20, "y": 10, "w": 60, "h": 15, "text": "글상자", "size": 60, "color": "#ffffff", "align": "center", "valign": "middle" }),
    }
}

// ---------------- 글 덩어리 고치기 (slides.js slideOp · reorderChunks · splitChunks) ----------------
static BLANK: Lazy<Regex> = Lazy::new(|| Regex::new(r"\n\s*\n").unwrap());
static DASH: Lazy<Regex> = Lazy::new(|| Regex::new(r"\n\s*-{3,}\s*\n").unwrap());
static LABEL: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\[(.{1,20})\]$").unwrap());

fn split_parts(song: bool, text: &str) -> Vec<String> {
    let t = text.replace('\r', "");
    (if song { BLANK.split(&t).map(String::from).collect::<Vec<_>>() } else { DASH.split(&t).map(String::from).collect() })
}
fn is_label(song: bool, l: &str) -> bool {
    song && LABEL.is_match(l.trim())
}
fn has_content(song: bool, b: &str) -> bool {
    b.split('\n').any(|l| !l.trim().is_empty() && !is_label(song, l))
}
fn join_parts(song: bool, parts: &[String]) -> String {
    parts.iter().map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect::<Vec<_>>().join(if song { "\n\n" } else { "\n---\n" })
}

/// 넣기('add') · 복제('dup') · 지우기('del') · 위('up') · 아래('down'). k = 슬라이드(덩어리) 번호
pub fn slide_op(song: bool, text: &str, op: &str, k: usize, new_text: &str) -> String {
    let mut parts = split_parts(song, text);
    let idx: Vec<usize> = parts.iter().enumerate().filter(|(_, b)| has_content(song, b)).map(|(i, _)| i).collect();
    let at = idx.get(k).copied();
    let labels_of = |b: &str| b.split('\n').filter(|l| is_label(song, l)).map(String::from).collect::<Vec<_>>();
    let body_of = |b: &str| b.split('\n').filter(|l| !is_label(song, l)).collect::<Vec<_>>().join("\n").trim().to_string();
    match op {
        "add" | "dup" => {
            let pos = at.map(|a| a + 1).unwrap_or(parts.len());
            let t = if op == "dup" { at.map(|a| body_of(&parts[a])).unwrap_or_else(|| new_text.into()) } else { new_text.into() };
            parts.insert(pos, t);
        }
        "del" => {
            let Some(a) = at else { return text.into() };
            let labels = labels_of(&parts[a]);
            if let Some(&next) = idx.get(k + 1) {
                if !labels.is_empty() && next == a + 1 && labels_of(&parts[next]).is_empty() {
                    parts[next] = format!("{}\n{}", labels.join("\n"), parts[next]);
                }
            }
            parts.remove(a);
        }
        "up" | "down" => {
            let other = if op == "up" { k.checked_sub(1).and_then(|j| idx.get(j)) } else { idx.get(k + 1) };
            let (Some(a), Some(&o)) = (at, other) else { return text.into() };
            parts.swap(a, o);
        }
        _ => return text.into(),
    }
    join_parts(song, &parts)
}

/// 차례 바꾸기: perm[i] = 새 i 번째 자리에 올 원래 덩어리 번호
pub fn reorder_chunks(song: bool, text: &str, perm: &[usize]) -> String {
    let parts = split_parts(song, text);
    let idx: Vec<usize> = parts.iter().enumerate().filter(|(_, b)| has_content(song, b)).map(|(i, _)| i).collect();
    let mut sorted = perm.to_vec();
    sorted.sort();
    if perm.len() != idx.len() || sorted.iter().enumerate().any(|(i, v)| *v != i) {
        return text.into();
    }
    let mut out = parts.clone();
    for (i, &pos) in idx.iter().enumerate() {
        out[pos] = parts[idx[perm[i]]].clone();
    }
    join_parts(song, &out)
}

/// c 번째 덩어리 앞에서 둘로 → (앞 글, 뒤 글)
pub fn split_chunks(song: bool, text: &str, c: usize) -> Option<(String, String)> {
    let parts = split_parts(song, text);
    let idx: Vec<usize> = parts.iter().enumerate().filter(|(_, b)| has_content(song, b)).map(|(i, _)| i).collect();
    if c == 0 || c >= idx.len() {
        return None;
    }
    Some((join_parts(song, &parts[..idx[c]]), join_parts(song, &parts[idx[c]..])))
}

/// 편집 번호 옮기기 (slideOpEdits)
pub fn slide_op_edits(edits: &Value, op: &str, k: usize) -> Value {
    let Some(m) = edits.as_object() else { return Value::Null };
    let mut out = Map::new();
    for (key, v) in m {
        let Ok(n) = key.parse::<usize>() else { out.insert(key.clone(), v.clone()); continue };
        let nn = match op {
            "add" | "dup" => Some(if n > k { n + 1 } else { n }),
            "del" => if n == k { None } else { Some(if n > k { n - 1 } else { n }) },
            "up" | "down" => {
                let o = if op == "up" { k.wrapping_sub(1) } else { k + 1 };
                Some(if n == k { o } else if n == o { k } else { n })
            }
            _ => Some(n),
        };
        if let Some(nn) = nn { out.insert(nn.to_string(), v.clone()); }
    }
    if op == "dup" {
        if let Some(e) = m.get(&k.to_string()) { out.insert((k + 1).to_string(), e.clone()); }
    }
    if out.is_empty() { Value::Null } else { Value::Object(out) }
}

/// 구역 나누기: c 번째부터의 편집 번호를 0 부터로 → (앞, 뒤)
pub fn split_edits(edits: &Value, c: usize) -> (Value, Value) {
    let (mut a, mut b) = (Map::new(), Map::new());
    for (k, v) in edits.as_object().into_iter().flatten() {
        if k == "*" { a.insert(k.clone(), v.clone()); b.insert(k.clone(), v.clone()); continue; }
        let Ok(n) = k.parse::<usize>() else { continue };
        if n < c { a.insert(n.to_string(), v.clone()); } else { b.insert((n - c).to_string(), v.clone()); }
    }
    let f = |m: Map<String, Value>| if m.is_empty() { Value::Null } else { Value::Object(m) };
    (f(a), f(b))
}

/// 큐시트 순서 하나에 편집 동작 적용. 결과로 (고친 순서, 나눴으면 뒤 순서)
pub fn apply_op(it: &Value, op: &str, k: usize) -> (Value, Option<Value>) {
    let mut it = it.clone();
    let ty = it["type"].as_str().unwrap_or("").to_string();
    let (field, song) = match ty.as_str() { "song" => ("lyrics", true), "text" => ("body", false), _ => ("", false) };
    if op == "split" {
        if !field.is_empty() {
            if let Some((a, b)) = split_chunks(song, it[field].as_str().unwrap_or(""), k) {
                let (ea, eb) = split_edits(&it["edits"], k);
                let mut back = it.clone();
                it[field] = json!(a);
                back[field] = json!(b);
                back["id"] = json!(crate::util::short_id("i"));
                back["title"] = json!(format!("{} (이어서)", it["title"].as_str().unwrap_or("")));
                set_or_remove(&mut it, "edits", ea);
                set_or_remove(&mut back, "edits", eb);
                if song { back.as_object_mut().unwrap().remove("songId"); back["order"] = json!(""); it["order"] = json!(""); }
                return (it, Some(back));
            }
        }
        return (it, None);
    }
    if !field.is_empty() {
        let t = slide_op(song, it[field].as_str().unwrap_or(""), op, k, "새 슬라이드");
        it[field] = json!(t);
        let e = slide_op_edits(&it["edits"], op, k);
        set_or_remove(&mut it, "edits", e);
    } else if is_chunk(&ty) {
        let pages = slides::title_pages(&it);
        if (op == "del" && pages.len() <= 1) || k >= pages.len() || (op == "up" && k == 0) || (op == "down" && k + 1 >= pages.len()) { return (it, None); }
        let text = slide_op(false, &pages.join("
---
"), op, k, "새 슬라이드");
        let pages: Vec<String> = slides::text_chunks(&text, 0).into_iter().map(|c| c.text).collect();
        set_title_pages(&mut it, &pages);
        let e = slide_op_edits(&it["edits"], op, k);
        set_or_remove(&mut it, "edits", e);
    } else {
        // 그 밖: 건너뛰기('skip') · 차례(slideOrder)
        match op {
            "skip" => {
                let mut sk: Vec<u64> = it["skip"].as_array().into_iter().flatten().filter_map(|x| x.as_u64()).collect();
                if let Some(p) = sk.iter().position(|x| *x == k as u64) { sk.remove(p); } else { sk.push(k as u64); sk.sort(); }
                set_or_remove(&mut it, "skip", if sk.is_empty() { Value::Null } else { json!(sk) });
            }
            _ => {}
        }
    }
    (it, None)
}


/// Electron fromTitlePages: 첫 장은 제목·부제, 나머지는 more 에 저장한다.
fn set_title_pages(it: &mut Value, pages: &[String]) {
    let pages: Vec<_> = pages.iter().map(|p| p.trim()).filter(|p| !p.is_empty()).collect();
    let mut first = pages.first().copied().unwrap_or("제목").lines();
    it["title"] = json!(first.next().unwrap_or("제목").trim());
    it["subtitle"] = json!(first.collect::<Vec<_>>().join(" ").trim());
    it["more"] = json!(pages.get(1..).unwrap_or_default().join("
---
"));
}

/// 제목 장·덧붙인 카드의 본문을 원래 Electron 필드에 반영한다.
pub fn set_card_text(it: &mut Value, key: &str, text: &str) {
    if let Some(id) = key.strip_prefix('x') {
        if let Some(ex) = it["extra"].as_array_mut() {
            if let Some(e) = ex.iter_mut().find(|e| e["id"].as_str() == Some(id)) { e["text"] = json!(text); }
        }
    } else if it["type"] == "title" {
        let mut pages = slides::title_pages(it);
        if let Some(p) = key.parse::<usize>().ok().and_then(|k| pages.get_mut(k)) { *p = text.into(); }
        set_title_pages(it, &pages);
    }
}

/// 보이는 슬라이드 키를 옮긴다. 덧붙임 at 는 바로 앞 원본 번호로 다시 계산한다.
/// 글 덩어리의 편집도 함께 재번호화하며, 성경·부를 순서 지정 찬양은 원본처럼 막는다.
pub fn move_slide(it: &Value, keys: &[String], from: usize, to: usize) -> Value {
    let out = it.clone();
    if from >= keys.len() || to >= keys.len() || from == to || it["type"] == "bible"
        || (it["type"] == "song" && !it["order"].as_str().unwrap_or("").trim().is_empty()) { return out; }
    let mut next = keys.to_vec();
    if next.iter().enumerate().any(|(i, k)| next[..i].contains(k)) { return out; }
    let moved = next.remove(from); next.insert(to, moved);
    reorder(it, &next)
}

/// 편집기 목록 차례(건너뛴 것 포함)를 `next` 로 바꾼 순서. 글 덩어리는 글·꾸미기 번호를, 그 밖은 slideOrder·덧붙임 at 를 고친다
/// (Electron slideedit.js moveSlides 의 적용 부분). 안 맞으면 그대로 돌려준다
pub fn reorder(it: &Value, next: &[String]) -> Value {
    let mut out = it.clone();
    let next = next.to_vec();
    if is_chunk(it["type"].as_str().unwrap_or("")) {
        let Some(perm) = next.iter().map(|k| k.parse::<usize>().ok()).collect::<Option<Vec<_>>>() else { return out };
        let mut sorted = perm.clone(); sorted.sort();
        if sorted.iter().enumerate().any(|(i,k)| i != *k) { return out; }
        if it["type"] == "title" {
            let pages = slides::title_pages(it);
            if pages.len() != perm.len() { return out; }
            set_title_pages(&mut out, &perm.iter().map(|k| pages[*k].clone()).collect::<Vec<_>>());
        } else {
            let song = it["type"] == "song";
            let field = if song { "lyrics" } else { "body" };
            let text = it[field].as_str().unwrap_or("");
            if split_parts(song,text).iter().filter(|p| has_content(song,p)).count() != perm.len() { return out; }
            out[field] = json!(reorder_chunks(song, text, &perm));
        }
        let mut edits = Map::new();
        for (key, value) in it["edits"].as_object().into_iter().flatten() {
            if let Ok(k) = key.parse::<usize>() {
                if let Some(pos) = perm.iter().position(|v| *v == k) { edits.insert(pos.to_string(), value.clone()); }
            } else { edits.insert(key.clone(), value.clone()); }
        }
        set_or_remove(&mut out, "edits", if edits.is_empty() { Value::Null } else { Value::Object(edits) });
    } else {
        let mut at: i64 = -1; let mut bases = vec![]; let mut extra = vec![];
        for key in next {
            if let Some(id) = key.strip_prefix('x') {
                if let Some(e) = it["extra"].as_array().into_iter().flatten().find(|e| e["id"].as_str() == Some(id)) {
                    let mut e = e.clone(); e["at"] = json!(at); extra.push(e);
                }
            } else if let Ok(base) = key.parse::<usize>() { at = base as i64; bases.push(base); }
        }
        set_or_remove(&mut out, "extra", if extra.is_empty() { Value::Null } else { json!(extra) });
        let natural = bases.iter().enumerate().all(|(i,k)| i == *k);
        set_or_remove(&mut out, "slideOrder", if natural { Value::Null } else { json!(bases) });
    }
    out
}

/// 요소를 화면 백분율로 이동. 크기와 나머지 속성은 그대로 보존한다.
pub fn translate_item(item: &mut Value, dx: f64, dy: f64) {
    for (key, delta) in [("x", dx), ("y", dy)] {
        let value = item[key].as_f64().unwrap_or(0.0) + delta;
        if value.is_finite() { item[key] = json!(value.clamp(-50.0, 150.0)); }
    }
}

/// 원래 슬라이드 차례 바꾸기 (그 밖 종류): 보이는 차례에서 from → to
pub fn move_order(it: &mut Value, n: usize, from: usize, to: usize) {
    let mut ord = order_of(it, n);
    if from >= ord.len() || to >= ord.len() { return; }
    let k = ord.remove(from);
    ord.insert(to, k);
    let natural = ord.iter().enumerate().all(|(i, v)| i == *v);
    set_or_remove(it, "slideOrder", if natural { Value::Null } else { json!(ord) });
}

/// 차례(seq: 원래 슬라이드 번호 · 덧붙인 슬라이드 "x<id>")를 순서에 적는다 (Electron slideedit.js applySeq).
/// 덧붙인 것은 바로 앞 원래 슬라이드 번호(at)로, 원래 슬라이드 차례는 slideOrder 로 (원래 차례면 지움)
fn apply_seq(it: &mut Value, seq: &[String], ex: &[Value]) {
    let mut at: i64 = -1;
    let (mut order, mut bases) = (vec![], vec![]);
    for z in seq {
        if let Some(id) = z.strip_prefix('x') {
            if let Some(e) = ex.iter().find(|e| e["id"].as_str() == Some(id)) {
                let mut e = e.clone();
                e["at"] = json!(at);
                order.push(e);
            }
        } else if let Ok(b) = z.parse::<usize>() {
            at = b as i64;
            bases.push(b);
        }
    }
    set_or_remove(it, "extra", if order.is_empty() { Value::Null } else { json!(order) });
    let natural = bases.iter().enumerate().all(|(i, k)| i == *k);
    set_or_remove(it, "slideOrder", if natural { Value::Null } else { json!(bases) });
}

/// 격자·목록에 마우스를 올렸을 때 뜨는 단추의 동작 (Electron slideedit.js quickOp → slideOp · extraOp).
/// `keys` 는 편집기 목록 차례(건너뛴 것 포함)의 ekey, `key` 는 누른 슬라이드의 ekey.
/// op: add(이 다음에 새 슬라이드) · dup(복제) · del(지우기, 그 밖 종류의 원래 슬라이드는 건너뛰기) · skip · up · down.
/// 돌려주는 값: (고친 순서, 고른 슬라이드의 새 ekey). 안 되면 사용자에게 보일 이유(한국어 원문).
pub fn quick_op(it: &Value, keys: &[String], key: &str, op: &str) -> Result<(Value, String), &'static str> {
    let ty = it["type"].as_str().unwrap_or("");
    if is_chunk(ty) {
        let n = keys.len();
        let k: usize = key.parse().map_err(|_| "이 슬라이드는 고칠 수 없습니다.")?;
        if op == "skip" { return Err("글 슬라이드는 건너뛰기 대신 지우기를 쓰세요."); }
        if op == "del" && n <= 1 { return Err("마지막 슬라이드는 지울 수 없습니다. 순서 자체를 지우세요."); }
        if (op == "up" && k == 0) || (op == "down" && k + 1 >= n) || (op != "add" && k >= n) { return Err(""); }
        let (a, _) = apply_op(it, op, k);
        let after = n + usize::from(op == "add" || op == "dup") - usize::from(op == "del");
        let pick = match op { "add" | "dup" | "down" => k + 1, "up" => k - 1, _ => k.min(after.saturating_sub(1)) };
        return Ok((a, pick.min(after.saturating_sub(1)).to_string()));
    }
    let pos = keys.iter().position(|x| x == key).ok_or("이 슬라이드를 찾지 못했습니다.")?;
    let xid = key.strip_prefix('x');
    let mut out = it.clone();
    let mut ex: Vec<Value> = it["extra"].as_array().cloned().unwrap_or_default();
    if (op == "del" || op == "skip") && xid.is_none() {
        // 원래 슬라이드: 건너뛰기 ↔ 되살리기
        let base: usize = key.parse().map_err(|_| "이 슬라이드는 고칠 수 없습니다.")?;
        let skip: Vec<u64> = it["skip"].as_array().into_iter().flatten().filter_map(|x| x.as_u64()).collect();
        let skipped = skip.contains(&(base as u64));
        let live = keys.iter().filter(|x| !x.starts_with('x')).filter_map(|x| x.parse::<u64>().ok()).filter(|b| !skip.contains(b)).count();
        if !skipped && live <= 1 && ex.is_empty() { return Err("모든 슬라이드를 건너뛸 수는 없습니다."); }
        let (a, _) = apply_op(it, "skip", base);
        return Ok((a, key.to_string()));
    }
    if op == "skip" { return Err("덧붙인 슬라이드는 건너뛰기 대신 지우기를 쓰세요."); }
    if (op == "up" || op == "down") && xid.is_none() && ty == "bible" { return Err("성경 본문의 차례는 바꿀 수 없습니다. 덧붙인 슬라이드만 옮길 수 있습니다."); }
    if op == "dup" && xid.is_none() { return Err("원래 슬라이드는 복제할 수 없습니다. ＋ 슬라이드로 사이에 새 슬라이드를 넣으세요."); }
    let mut seq = keys.to_vec();
    let pick;
    match op {
        "add" | "dup" => {
            let t = crate::util::to36(crate::util::now_ms() as u64);
            let id = format!("{}{}", &t[t.len().saturating_sub(6)..], &crate::util::to36(rand::random::<u32>() as u64 + 36 * 36 * 36)[..4]);
            let src = if op == "dup" { ex.iter().find(|e| e["id"].as_str() == xid).cloned() } else { None };
            ex.push(json!({ "id": &id, "at": 0, "text": src.as_ref().and_then(|s| s["text"].as_str()).unwrap_or("새 슬라이드") }));
            if let Some(s) = &src {
                let from = format!("x{}", s["id"].as_str().unwrap_or(""));
                if let Some(e) = it["edits"].get(&from).cloned() { out["edits"][format!("x{id}")] = e; }
            }
            seq.insert(pos + 1, format!("x{id}"));
            pick = format!("x{id}");
        }
        "del" => {
            let id = xid.unwrap_or("");
            seq.retain(|z| z != key);
            ex.retain(|e| e["id"].as_str() != Some(id));
            if let Some(o) = out["edits"].as_object_mut() { o.remove(key); if o.is_empty() { out.as_object_mut().unwrap().remove("edits"); } }
            pick = keys.get(pos.saturating_sub(1)).filter(|x| x.as_str() != key).cloned().or_else(|| seq.first().cloned()).unwrap_or_else(|| "0".into());
        }
        "up" | "down" => {
            let to = if op == "up" { pos.checked_sub(1) } else { Some(pos + 1).filter(|t| *t < seq.len()) };
            let Some(to) = to else { return Err("") };
            seq.swap(pos, to);
            pick = key.to_string();
        }
        _ => return Err(""),
    }
    apply_seq(&mut out, &seq, &ex);
    Ok((out, pick))
}

/// 여러 장을 끌어 다른 슬라이드의 앞·뒤에 놓기 (Electron slideedit.js moveSlides).
/// `keys` 는 편집기 목록 차례(건너뛴 것 포함)의 ekey, `moved` 는 옮길 ekey, `target` 은 놓을 자리의 ekey.
/// 안 되면 사용자에게 보일 까닭 (빈 글이면 바뀐 것 없음)
pub fn move_keys(it: &Value, keys: &[String], moved: &[String], target: &str, after: bool) -> Result<Value, &'static str> {
    if it["type"] == "bible" {
        return Err("성경 본문의 차례는 바꿀 수 없습니다.");
    }
    if it["type"] == "song" && !it["order"].as_str().unwrap_or("").trim().is_empty() {
        return Err("부를 순서가 정해진 찬양입니다. 편집칸의 '부를 순서' 에서 차례를 바꾸세요.");
    }
    let mut mv: Vec<String> = vec![];
    for k in moved {
        if !mv.contains(k) {
            mv.push(k.clone());
        }
    }
    if mv.is_empty() || mv.iter().any(|k| k == target) {
        return Err("");
    }
    let rest: Vec<String> = keys.iter().filter(|k| !mv.contains(k)).cloned().collect();
    let Some(mut pos) = rest.iter().position(|k| k == target) else { return Err("") };
    if after {
        pos += 1;
    }
    // 옮길 것은 목록 차례대로
    mv.sort_by_key(|k| keys.iter().position(|x| x == k).unwrap_or(usize::MAX));
    let next: Vec<String> = rest[..pos].iter().chain(mv.iter()).chain(rest[pos..].iter()).cloned().collect();
    if next == keys {
        return Err("");
    }
    let out = reorder(it, &next);
    if &out == it {
        return Err("");
    }
    Ok(out)
}

/// `move_keys` 뒤 옮긴 슬라이드의 새 키 (새 자리에서 다시 고르려고): 글 덩어리는 번호가 새 자리로 바뀌고, 그 밖은 그대로
pub fn keys_after_move(chunk: bool, keys: &[String], moved: &[String], target: &str, after: bool) -> Vec<String> {
    let mut mv: Vec<String> = vec![];
    for k in moved {
        if !mv.contains(k) {
            mv.push(k.clone());
        }
    }
    mv.sort_by_key(|k| keys.iter().position(|x| x == k).unwrap_or(usize::MAX));
    if !chunk {
        return mv;
    }
    let rest: Vec<&String> = keys.iter().filter(|k| !mv.contains(k)).collect();
    let pos = rest.iter().position(|k| k.as_str() == target).map(|p| if after { p + 1 } else { p }).unwrap_or(0);
    (pos..pos + mv.len()).map(|x| x.to_string()).collect()
}

/// 고른 여러 장 지우기 (Electron selAction('del')): 글 순서는 지우고, 그 밖 순서의 원래 슬라이드는 건너뛰기, 덞붙인 것은 지운다.
/// 뒤에서부터 처리해 앞 번호가 밀리지 않게 한다. 돌려주는 것: (고친 순서, 처리한 장 수)
pub fn delete_keys(it: &Value, keys: &[String], del: &[String]) -> Result<(Value, usize), &'static str> {
    let chunk = is_chunk(it["type"].as_str().unwrap_or(""));
    let num = |k: &str| k.chars().filter(|c| c.is_ascii_digit()).collect::<String>().parse::<u64>().unwrap_or(0);
    let mut ds: Vec<String> = vec![];
    for k in del {
        if !ds.contains(k) {
            ds.push(k.clone());
        }
    }
    ds.sort_by(|a, b| num(b).cmp(&num(a)));
    let mut cur = it.clone();
    let mut keys = keys.to_vec();
    let mut n = 0;
    for k in ds {
        let (a, _) = quick_op(&cur, &keys, &k, "del")?;
        cur = a;
        n += 1;
        if chunk {
            let len = keys.len().saturating_sub(1);
            keys = (0..len).map(|x| x.to_string()).collect();
        } else if k.starts_with('x') {
            keys.retain(|x| x != &k);
        }
    }
    Ok((cur, n))
}

/// 여러 장 함께 꾸미기 (Electron slideedit.js propagate): 기준 슬라이드에서 바뀐 것(before → after)만 다른 고른 슬라이드에 그대로.
/// 글상자·그림·도형(items)은 id 별로 넣기·바꾸기·빼기, style·box 는 바뀐 항목만
pub fn propagate(it: &mut Value, others: &[String], before: &Value, after: &Value) {
    let empty = Map::new();
    let b = before.as_object().unwrap_or(&empty);
    let a = after.as_object().unwrap_or(&empty);
    let mut fields: Vec<&String> = vec![];
    for f in b.keys().chain(a.keys()) {
        if !fields.contains(&f) {
            fields.push(f);
        }
    }
    for k in others.iter().filter(|k| k.as_str() != "*") {
        if !it["edits"].is_object() {
            it["edits"] = json!({});
        }
        if !it["edits"][k].is_object() {
            it["edits"][k] = json!({});
        }
        let e = it["edits"][k].as_object_mut().unwrap();
        for f in &fields {
            let (bv, av) = (b.get(*f), a.get(*f));
            if bv == av {
                continue;
            }
            if f.as_str() == "items" {
                let id = |x: &Value| x["id"].clone();
                let bl = bv.and_then(|v| v.as_array()).cloned().unwrap_or_default();
                let al = av.and_then(|v| v.as_array()).cloned().unwrap_or_default();
                let mut list = e.get("items").and_then(|v| v.as_array()).cloned().unwrap_or_default();
                for x in &bl {
                    if !al.iter().any(|y| id(y) == id(x)) {
                        list.retain(|y| id(y) != id(x));
                    }
                }
                for x in &al {
                    if bl.iter().any(|y| y == x) {
                        continue;
                    }
                    match list.iter().position(|y| id(y) == id(x)) {
                        Some(j) => list[j] = x.clone(),
                        None => list.push(x.clone()),
                    }
                }
                e.insert("items".into(), json!(list));
                continue;
            }
            if (f.as_str() == "style" || f.as_str() == "box") && av.is_some_and(|v| v.is_object()) {
                let bo = bv.and_then(|v| v.as_object()).cloned().unwrap_or_default();
                let ao = av.and_then(|v| v.as_object()).cloned().unwrap_or_default();
                let mut cur = e.get(*f).and_then(|v| v.as_object()).cloned().unwrap_or_default();
                for s in bo.keys().chain(ao.keys()) {
                    if bo.get(s) == ao.get(s) {
                        continue;
                    }
                    match ao.get(s) {
                        Some(v) => {
                            cur.insert(s.clone(), v.clone());
                        }
                        None => {
                            cur.remove(s);
                        }
                    }
                }
                e.insert((*f).clone(), Value::Object(cur));
                continue;
            }
            match av {
                Some(v) => {
                    e.insert((*f).clone(), v.clone());
                }
                None => {
                    e.remove(*f);
                }
            }
        }
    }
}

fn set_or_remove(it: &mut Value, k: &str, v: Value) {
    if v.is_null() { if let Some(o) = it.as_object_mut() { o.remove(k); } } else { it[k] = v; }
}

/// 편집을 이 출력의 템플릿에 반영 (render.js editTemplate 근사): 글상자 위치·크기·꾸미기
/// 투명 배경(OBS 자막) 출력은 '모든 출력에 적용' 일 때만
pub fn edited_template(t: &Value, e: &Value) -> Value {
    let mut t = t.clone();
    let transparent = t["background"]["type"] == "transparent";
    if transparent && !e["allOutputs"].as_bool().unwrap_or(false) {
        return t;
    }
    let (w, h) = (t["width"].as_f64().unwrap_or(1920.0), t["height"].as_f64().unwrap_or(1080.0));
    if e["box"].is_object() {
        let b = &e["box"];
        let (x, y, bw, bh) = (b["x"].as_f64().unwrap_or(0.0), b["y"].as_f64().unwrap_or(0.0), b["w"].as_f64().unwrap_or(50.0), b["h"].as_f64().unwrap_or(20.0));
        t["safeArea"] = json!(0);
        t["padding"] = json!({ "left": w * x / 100.0, "top": h * y / 100.0, "right": w - w * (x + bw) / 100.0, "bottom": h - h * (y + bh) / 100.0 });
    }
    let s = &e["style"];
    let k = h / 1080.0;
    if let Some(z) = s["size"].as_f64() {
        t["font"]["size"] = json!(z * k);
        t["font"]["minSize"] = json!((z * k * 0.6).min(z * k));
    }
    if let Some(v) = s["autoFit"].as_bool() { t["font"]["autoFit"] = json!(v); }
    if let Some(c) = s["color"].as_str() { t["font"]["color"] = json!(c); }
    if let Some(b) = s["bold"].as_bool() { t["font"]["weight"] = json!(if b { 700 } else { 400 }); }
    if let Some(a) = s["align"].as_str() { t["align"] = json!(a); }
    if let Some(a) = s["valign"].as_str() { t["valign"] = json!(a); }
    if let Some(l) = s["lineHeight"].as_f64() { t["font"]["lineHeight"] = json!(l); }
    if let Some(sh) = s["shadow"].as_bool() { t["shadow"]["enabled"] = json!(sh); }
    if s["boxColor"].is_string() || s["boxOpacity"].is_number() {
        t["box"]["enabled"] = json!(true);
        if let Some(c) = s["boxColor"].as_str() { t["box"]["color"] = json!(c); }
        if let Some(o) = s["boxOpacity"].as_f64() { t["box"]["opacity"] = json!(o); }
    }
    if let Some(r) = s["radius"].as_f64() { t["box"]["radius"] = json!(r * k); }
    if let Some(p) = s["padding"].as_f64() { t["box"]["padding"] = json!(p * k); }
    t
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decorate_skip_order_extra() {
        let it = json!({ "type": "image", "skip": [1], "slideOrder": [2, 0, 1], "extra": [{ "id": "a1", "at": 0, "text": "덧" }], "edits": { "0": { "bg": "#ff0000" } } });
        let sl = vec![json!({"kind":"image","ref":"a"}), json!({"kind":"image","ref":"b"}), json!({"kind":"image","ref":"c"})];
        let out = decorate(&it, sl, false);
        let refs: Vec<&str> = out.iter().map(|s| s["ref"].as_str().unwrap()).collect();
        assert_eq!(refs, vec!["c", "a", "덧"]);
        assert_eq!(out[1]["bg"], json!("#ff0000"));
        assert_eq!(out[2]["ekey"], json!("xa1"));
    }
    #[test]
    fn quick_ops_like_electron() {
        // 글 순서: ＋ 뒤에 새 슬라이드, ✕ 지우기, 마지막 한 장은 못 지움
        let it = json!({"type":"text","body":"가\n---\n나"});
        let keys = vec!["0".to_string(), "1".to_string()];
        let (a, pick) = quick_op(&it, &keys, "0", "add").unwrap();
        assert_eq!(a["body"], json!("가\n---\n새 슬라이드\n---\n나"));
        assert_eq!(pick, "1");
        let (d, _) = quick_op(&it, &keys, "1", "del").unwrap();
        assert_eq!(d["body"], json!("가"));
        assert!(quick_op(&d, &["0".into()], "0", "del").is_err());
        // 성경: ✕ = 건너뛰기, ＋ = 덧붙인 카드, 덧붙인 카드 복제·지우기
        let b = json!({"type":"bible"});
        let keys: Vec<String> = vec!["0".into(), "1".into(), "2".into()];
        let (s, _) = quick_op(&b, &keys, "1", "del").unwrap();
        assert_eq!(s["skip"], json!([1]));
        let (s2, _) = quick_op(&s, &keys, "1", "skip").unwrap();
        assert!(s2.get("skip").is_none());
        assert!(quick_op(&b, &keys, "0", "dup").is_err());
        let (x, pick) = quick_op(&b, &keys, "0", "add").unwrap();
        assert_eq!(x["extra"][0]["at"], json!(0));
        assert_eq!(x["extra"][0]["text"], json!("새 슬라이드"));
        assert!(x.get("slideOrder").is_none());
        let keys2: Vec<String> = vec!["0".into(), pick.clone(), "1".into(), "2".into()];
        let (y, _) = quick_op(&x, &keys2, &pick, "dup").unwrap();
        assert_eq!(y["extra"].as_array().unwrap().len(), 2);
        let (z, _) = quick_op(&x, &keys2, &pick, "del").unwrap();
        assert!(z.get("extra").is_none());
        // 모두 건너뛰기는 막는다
        let one = json!({"type":"image","skip":[1]});
        assert!(quick_op(&one, &["0".into(), "1".into()], "0", "skip").is_err());
    }
    #[test]
    fn ops() {
        let t = "[1절]\n가\n\n나\n\n[후렴]\n다";
        assert_eq!(slide_op(true, t, "del", 0, ""), "[1절]\n나\n\n[후렴]\n다");
        assert_eq!(slide_op(true, t, "down", 1, ""), "[1절]\n가\n\n[후렴]\n다\n\n나");
        assert_eq!(slide_op(false, "a\n---\nb", "dup", 0, ""), "a\n---\na\n---\nb");
        assert_eq!(reorder_chunks(false, "a\n---\nb\n---\nc", &[2, 0, 1]), "c\n---\na\n---\nb");
        assert_eq!(split_chunks(false, "a\n---\nb\n---\nc", 1), Some(("a".into(), "b\n---\nc".into())));
        let e = slide_op_edits(&json!({ "*": {}, "0": {"a":1}, "2": {"b":1} }), "del", 1);
        assert_eq!(e["1"], json!({"b":1}));
        let (a, b) = split_edits(&json!({ "0": {"a":1}, "3": {"b":1} }), 2);
        assert_eq!(a["0"], json!({"a":1}));
        assert_eq!(b["1"], json!({"b":1}));
    }
    #[test]
    fn title_cards_roundtrip_and_last_page() {
        let it = json!({"type":"title","title":"첫째","subtitle":"부제","more":"둘째\n설명","edits":{"0":{"bg":"#fff"},"1":{"hideMain":true}}});
        let (added,_) = apply_op(&it,"add",0);
        assert_eq!(slides::title_pages(&added),vec!["첫째\n부제","새 슬라이드","둘째\n설명"]);
        assert_eq!(added["edits"]["2"],it["edits"]["1"]);
        let (deleted,_) = apply_op(&added,"del",1);
        assert_eq!(deleted,it);
        let only = json!({"type":"title","title":"하나"});
        assert_eq!(apply_op(&only,"del",0).0,only);
        let mut edited=it.clone(); set_card_text(&mut edited,"1","바뀐 제목\n바뀐 부제");
        assert_eq!(slides::title_pages(&edited)[1],"바뀐 제목\n바뀐 부제");
    }
    #[test]
    fn move_many_delete_many_and_propagate_like_electron() {
        let k = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let it = json!({"type":"text","body":"a\n---\nb\n---\nc\n---\nd","edits":{"3":{"bg":"#fff"}}});
        let keys = k(&["0", "1", "2", "3"]);
        // a·c 를 d 뒤로 → b d a c (옮긴 것은 목록 차례대로), 꼾미기 번호도 따라감
        let m = move_keys(&it, &keys, &k(&["2", "0"]), "3", true).unwrap();
        assert_eq!(m["body"], "b\n---\nd\n---\na\n---\nc");
        assert_eq!(m["edits"]["1"], json!({"bg":"#fff"}));
        assert_eq!(keys_after_move(true, &keys, &k(&["2", "0"]), "3", true), k(&["2", "3"]));
        assert_eq!(keys_after_move(false, &keys, &k(&["2", "0"]), "3", true), k(&["0", "2"]));
        // 앞에 놓기
        assert_eq!(move_keys(&it, &keys, &k(&["3"]), "0", false).unwrap()["body"], "d\n---\na\n---\nb\n---\nc");
        // 제자리·자기 자신 위·성경은 안 됨
        assert_eq!(move_keys(&it, &keys, &k(&["1"]), "0", true), Err(""));
        assert_eq!(move_keys(&it, &keys, &k(&["1"]), "1", true), Err(""));
        assert!(move_keys(&json!({"type":"bible"}), &keys, &k(&["1"]), "0", false).unwrap_err().contains("성경"));
        // 그림: 원래 슬라이드 차례는 slideOrder 로
        let img = json!({"type":"slideshow","images":["a","b","c"]});
        assert_eq!(move_keys(&img, &k(&["0", "1", "2"]), &k(&["0"]), "2", true).unwrap()["slideOrder"], json!([1, 2, 0]));
        // 여러 장 지우기: 글은 지우고, 그림은 건너뛰기
        let (d, n) = delete_keys(&it, &keys, &k(&["1", "3"])).unwrap();
        assert_eq!((d["body"].as_str().unwrap(), n), ("a\n---\nc", 2));
        let (s, _) = delete_keys(&img, &k(&["0", "1", "2"]), &k(&["0", "2"])).unwrap();
        assert_eq!(s["skip"], json!([0, 2]));
        // 함께 꾸미기: 바뀐 것만 다른 슬라이드에
        let mut p = json!({"edits":{"1":{"bg":"#000","style":{"size":50,"bold":true}}}});
        propagate(&mut p, &k(&["1", "2"]), &json!({"style":{"size":60}}), &json!({"style":{"size":80},"hideMain":true,"items":[{"id":"t1","type":"text"}]}));
        assert_eq!(p["edits"]["1"], json!({"bg":"#000","style":{"size":80,"bold":true},"hideMain":true,"items":[{"id":"t1","type":"text"}]}));
        assert_eq!(p["edits"]["2"], json!({"style":{"size":80},"hideMain":true,"items":[{"id":"t1","type":"text"}]}));
    }
    #[test]
    fn drag_reorder_preserves_edits_and_extra_anchors() {
        let it=json!({"type":"text","body":"a\n---\nb\n---\nc","edits":{"*":{"hideMain":true},"0":{"bg":"#fff"}}});
        let keys=vec!["0".into(),"1".into(),"2".into()];
        let moved=move_slide(&it,&keys,0,2);
        assert_eq!(moved["body"],"b\n---\nc\n---\na");
        assert_eq!(moved["edits"]["2"],it["edits"]["0"]);
        assert_eq!(moved["edits"]["*"],it["edits"]["*"]);
        let image=json!({"type":"image","extra":[{"id":"ab","at":0,"text":"카드"}],"edits":{"xab":{"hideMain":true}}});
        let ks=vec!["0".into(),"xab".into(),"1".into()];
        let mut moved=move_slide(&image,&ks,1,0);
        assert_eq!(moved["extra"][0]["at"],-1);
        set_card_text(&mut moved,"xab","새 내용");
        assert_eq!(moved["extra"][0]["text"],"새 내용");
        assert_eq!(moved["edits"],image["edits"]);
        let mut bible=image.clone(); bible["type"]=json!("bible");
        assert_eq!(move_slide(&bible,&ks,0,2),bible);
    }
    #[test]
    fn element_properties_and_drag_are_electron_compatible() {
        let mut item=json!({"id":"ab","type":"image","x":10,"y":20,"w":30,"h":40,"opacity":0.4,"crop":{"l":0.1,"t":0.2,"r":0.3,"b":0.1}});
        translate_item(&mut item,5.0,-7.0);
        assert_eq!(item["x"],15.0); assert_eq!(item["y"],13.0);
        assert_eq!(item["w"],30); assert_eq!(item["opacity"],0.4);
        let edit=normalize_edit(&json!({"items":[item,{"id":"cd","type":"text","font":"맑은 고딕","opacity":0.5}]})).unwrap();
        assert_eq!(edit["items"][0]["crop"]["l"],0.1);
        assert_eq!(edit["items"][1]["font"],"맑은 고딕");
        assert_eq!(edit["items"][1]["opacity"],0.5);
    }
    #[test]
    fn title_drag_and_ppt_missing_regression() {
        let it=json!({"type":"title","title":"가","subtitle":"부제","more":"나","edits":{"0":{"bg":"#fff"}}});
        let moved=move_slide(&it,&["0".into(),"1".into()],0,1);
        assert_eq!(slides::title_pages(&moved),vec!["나","가\n부제"]);
        assert_eq!(moved["edits"]["1"],it["edits"]["0"]);
        assert_eq!(apply_op(&it,"up",0).0,it);
        assert_eq!(apply_op(&it,"down",1).0,it);
        let ppt=json!({"type":"hymn","slideOrder":[1,0],"extra":[{"id":"ab","at":1,"text":"추가"}],"edits":{"0":{"items":[{"id":"p0t0","type":"text"},{"id":"my0","type":"text"}]}}});
        let slides=vec![json!({"kind":"image","pptBgMissing":true}),json!({"kind":"image"})];
        let decorated=decorate(&ppt,slides,true);
        assert_eq!(decorated[0]["base"],1); assert_eq!(decorated[1]["ekey"],"xab");
        assert_eq!(decorated[2]["base"],0);
        assert_eq!(decorated[2]["edit"]["items"].as_array().unwrap().len(),1);
        assert_eq!(decorated[2]["edit"]["items"][0]["id"],"my0");
    }
    #[test]
    fn normalize() {
        let e = normalize_edit(&json!({ "box": { "x": 500, "y": 5 }, "style": { "size": 1, "color": "red" }, "items": [{ "type": "text", "text": "a" }, { "type": "bad" }] })).unwrap();
        assert_eq!(e["box"]["x"], json!(150.0));
        assert_eq!(e["style"]["size"], json!(8.0));
        assert!(e["style"]["color"].is_null());
        assert_eq!(e["items"].as_array().unwrap().len(), 1);
        assert!(normalize_edit(&json!({})).is_none());
    }
}
