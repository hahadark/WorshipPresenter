//! 화면 구성 (로드맵 2단계, docs/roadmap-sources.md)
//!
//! - 설정 `scenes[]` : 사용자가 만든 구성. 칸(layer)마다 소스 하나 · 자리(rect) · 자르기(crop) · 투명도 · 둥글기 · 테두리 · 보이기 · 잠그기 · 글 모양(tpl).
//! - 설정 `outputScenes` : { main, overlay } — 출력마다 쓰는 구성 id (없거나 모르는 id 면 기본 구성 `full`).
//! - 둘 다 **덧붙인 키**라 옛 2.0.x 는 모른 채 그대로 둔다 (지우지도, 죽지도 않음).
//! - 기본 구성 `full` 은 저장하지 않고 코어가 계산한다 (sources::default_scene). 구성을 하나도 고르지 않은 사람은 2.0.8 과 같다.
//! - 사용자 구성에 없는 오버레이 · 회중 메시지 · 로고 화면 · 그리기 (OBS 자막이면 그림 우상단) 는 맨 위에 저절로 붙는다
//!   — 구성을 바꿔도 이름 자막 · 회중 메시지 · 로고 화면 단추가 그대로 듣게.
use anyhow::{bail, Result};
use serde_json::{json, Value};

/// 구성 수 상한
pub const MAX_SCENES: usize = 30;
/// 구성 하나의 칸 수 상한
pub const MAX_LAYERS: usize = 16;
/// 칸의 가장 작은 폭 · 높이 (화면 비율)
pub const MIN_SIZE: f64 = 0.02;
/// 기본 구성 id (저장하지 않음)
pub const FULL: &str = "full";

/// 기본 틀 (id, 이름)
pub const PRESETS: [(&str, &str); 5] = [("full", "전체"), ("split", "좌우 나눔"), ("pip", "작은 화면 (구석)"), ("lower", "아래 자막"), ("three", "3분할")];

fn clean_text(s: &str, max: usize) -> String {
    s.chars().filter(|c| !c.is_control()).take(max).collect::<String>().trim().to_string()
}

fn ok_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

fn num(v: &Value, d: f64) -> f64 {
    v.as_f64().filter(|x| x.is_finite()).unwrap_or(d)
}

/// 칸의 소스 식별자: 빈 칸 "" · 내장 소스 · overlay:<id> · camera:<deviceId> · 사용자 소스 src-…
pub fn ok_source(s: &str) -> bool {
    if s.is_empty() {
        return true;
    }
    if s.len() > 300 || s.chars().any(|c| c.is_control()) {
        return false;
    }
    matches!(s, "background" | "media" | "slide-bg" | "live" | "message" | "logo-screen" | "draw" | "pip") || s.starts_with("overlay:") || s.starts_with("camera:") || s.starts_with("src-")
}

/// 자리 [x, y, w, h] (화면 비율): 화면 안에, 가장 작은 크기 이상
pub fn clamp_rect(r: [f64; 4]) -> [f64; 4] {
    let w = r[2].clamp(MIN_SIZE, 1.0);
    let h = r[3].clamp(MIN_SIZE, 1.0);
    let x = r[0].clamp(0.0, 1.0 - w);
    let y = r[1].clamp(0.0, 1.0 - h);
    [x, y, w, h]
}

pub fn rect_of(l: &Value) -> [f64; 4] {
    match l["rect"].as_array() {
        Some(a) if a.len() == 4 => clamp_rect([num(&a[0], 0.0), num(&a[1], 0.0), num(&a[2], 1.0), num(&a[3], 1.0)]),
        _ => [0.0, 0.0, 1.0, 1.0],
    }
}

/// 자르기 [왼, 위, 오른, 아래] (소스 비율). 한 방향 합이 0.9 를 넘지 않게
pub fn crop_of(l: &Value) -> [f64; 4] {
    let mut c = match l["crop"].as_array() {
        Some(a) if a.len() == 4 => [num(&a[0], 0.0), num(&a[1], 0.0), num(&a[2], 0.0), num(&a[3], 0.0)],
        _ => [0.0; 4],
    };
    for x in c.iter_mut() {
        *x = x.clamp(0.0, 0.9);
    }
    if c[0] + c[2] > 0.9 {
        let k = 0.9 / (c[0] + c[2]);
        c[0] *= k;
        c[2] *= k;
    }
    if c[1] + c[3] > 0.9 {
        let k = 0.9 / (c[1] + c[3]);
        c[1] *= k;
        c[3] *= k;
    }
    c
}

fn round4(x: f64) -> f64 {
    (x * 10000.0).round() / 10000.0
}

/// 칸 하나 정리 (모르는 칸은 그대로 둔다 — 새 판이 덧붙인 값을 지우지 않게)
pub fn normalize_layer(v: &Value) -> Result<Value> {
    if !v.is_object() {
        bail!("{}", crate::i18n::tr("칸 모양이 맞지 않습니다."));
    }
    let mut o = v.as_object().cloned().unwrap_or_default();
    let id = v["id"].as_str().filter(|x| ok_id(x)).map(String::from).unwrap_or_else(|| crate::util::short_id("l-"));
    o.insert("id".into(), json!(id));
    let src = v["source"].as_str().unwrap_or("");
    o.insert("source".into(), json!(if ok_source(src) { src } else { "" }));
    let name = clean_text(v["name"].as_str().unwrap_or(""), 40);
    if name.is_empty() {
        o.remove("name");
    } else {
        o.insert("name".into(), json!(name));
    }
    o.insert("rect".into(), json!(rect_of(v).map(round4)));
    let c = crop_of(v);
    if c.iter().any(|x| *x > 0.0) {
        o.insert("crop".into(), json!(c.map(round4)));
    } else {
        o.remove("crop");
    }
    o.insert("opacity".into(), json!(round4(num(&v["opacity"], 1.0).clamp(0.0, 1.0))));
    o.insert("visible".into(), json!(v["visible"].as_bool().unwrap_or(true)));
    o.insert("locked".into(), json!(v["locked"].as_bool().unwrap_or(false)));
    o.insert("radius".into(), json!(num(&v["radius"], 0.0).clamp(0.0, 400.0).round()));
    let bw = num(&v["border"]["width"], 0.0).clamp(0.0, 60.0).round();
    let bc = v["border"]["color"].as_str().filter(|x| x.len() == 7 && x.starts_with('#') && x[1..].chars().all(|c| c.is_ascii_hexdigit())).unwrap_or("#ffffff");
    o.insert("border".into(), json!({ "width": bw, "color": bc }));
    o.insert("fit".into(), json!(if v["fit"] == "contain" { "contain" } else { "cover" }));
    let tpl = v["tpl"].as_str().unwrap_or("");
    o.insert("tpl".into(), json!(if tpl.is_empty() || tpl == "main" || tpl == "overlay" || ok_id(tpl) { tpl } else { "" }));
    o.remove("template"); // 출력 데이터에만 붙이는 값 — 저장하지 않음
    Ok(Value::Object(o))
}

/// 구성 하나 정리
pub fn normalize_scene(v: &Value) -> Result<Value> {
    if !v.is_object() {
        bail!("{}", crate::i18n::tr("화면 구성 모양이 맞지 않습니다."));
    }
    let mut o = v.as_object().cloned().unwrap_or_default();
    let id = v["id"].as_str().filter(|x| ok_id(x) && *x != FULL).map(String::from).unwrap_or_else(|| crate::util::short_id("sc-"));
    o.insert("id".into(), json!(id));
    let name = clean_text(v["name"].as_str().unwrap_or(""), 40);
    o.insert("name".into(), json!(if name.is_empty() { crate::i18n::tr("새 화면 구성") } else { name }));
    let mut layers: Vec<Value> = vec![];
    for l in v["layers"].as_array().into_iter().flatten().take(MAX_LAYERS) {
        if let Ok(mut n) = normalize_layer(l) {
            // 같은 칸 id 가 둘이면 뒤의 것에 새 id
            if layers.iter().any(|x| x["id"] == n["id"]) {
                n["id"] = json!(crate::util::short_id("l-"));
            }
            layers.push(n);
        }
    }
    o.insert("layers".into(), json!(layers));
    Ok(Value::Object(o))
}

/// 설정의 사용자 구성 (깨진 것은 건너뜀, 저장 형식은 그대로)
pub fn user_scenes(settings: &Value) -> Vec<Value> {
    settings["scenes"].as_array().into_iter().flatten().filter_map(|x| normalize_scene(x).ok().filter(|n| n["id"] == x["id"])).collect()
}

pub fn find_scene(settings: &Value, id: &str) -> Option<Value> {
    user_scenes(settings).into_iter().find(|s| s["id"] == id)
}

pub fn save_scene(settings: &mut Value, v: &Value) -> Result<Value> {
    let n = normalize_scene(v)?;
    if !settings["scenes"].is_array() {
        settings["scenes"] = json!([]);
    }
    let list = settings["scenes"].as_array_mut().unwrap();
    match list.iter().position(|x| x["id"] == n["id"]) {
        Some(i) => list[i] = n.clone(),
        None => {
            if list.len() >= MAX_SCENES {
                bail!("{}", crate::i18n::tr("화면 구성을 더 만들 수 없습니다. 쓰지 않는 구성을 지워 주세요."));
            }
            list.push(n.clone())
        }
    }
    Ok(n)
}

/// 지우기: 그 구성을 쓰던 출력은 기본 구성으로
pub fn delete_scene(settings: &mut Value, id: &str) {
    if let Some(l) = settings["scenes"].as_array_mut() {
        l.retain(|x| x["id"] != id);
    }
    if let Some(m) = settings["outputScenes"].as_object_mut() {
        m.retain(|_, v| v != id);
    }
    // 순서 종류별 기본도 (3단계). 순서에 남은 지정은 없는 구성이라 건너뛴다 (outputs::resolve_scene)
    if let Some(m) = settings["typeScenes"].as_object_mut() {
        for (_, v) in m.iter_mut() {
            if let Some(o) = v.as_object_mut() {
                o.retain(|_, x| x != id);
            }
        }
        m.retain(|_, v| v.as_object().is_some_and(|o| !o.is_empty()));
    }
}

/// 복제: 새 id · '이름 (사본)', 칸 id 는 그대로 (구성마다 따로라 겹쳐도 됨)
pub fn duplicate_scene(settings: &mut Value, id: &str) -> Result<Value> {
    let Some(mut s) = find_scene(settings, id) else { bail!("{}", crate::i18n::tr("화면 구성을 찾을 수 없습니다.")) };
    s["id"] = json!(crate::util::short_id("sc-"));
    s["name"] = json!(crate::i18n::tr_args("{1} (사본)", &[&s["name"].as_str().unwrap_or("").to_string()]));
    save_scene(settings, &s)
}

/// 출력이 쓰는 구성 id (없거나 지운 구성이면 기본 `full`)
pub fn output_scene_id(settings: &Value, output: &str) -> String {
    let id = settings["outputScenes"][output].as_str().unwrap_or("");
    if id.is_empty() || id == FULL || find_scene(settings, id).is_none() {
        FULL.to_string()
    } else {
        id.to_string()
    }
}

/// 출력에 구성 고르기 (full · "" = 기본으로)
pub fn set_output_scene(settings: &mut Value, output: &str, id: &str) -> Result<()> {
    if !crate::outputs::is_output(settings, output) {
        bail!("{}", crate::i18n::tr("모르는 출력입니다."));
    }
    if !(id.is_empty() || id == FULL || find_scene(settings, id).is_some()) {
        bail!("{}", crate::i18n::tr("화면 구성을 찾을 수 없습니다."));
    }
    if !settings["outputScenes"].is_object() {
        settings["outputScenes"] = json!({});
    }
    let m = settings["outputScenes"].as_object_mut().unwrap();
    if id.is_empty() || id == FULL {
        m.remove(output);
    } else {
        m.insert(output.into(), json!(id));
    }
    Ok(())
}

fn layer(source: &str, rect: [f64; 4]) -> Value {
    json!({ "id": crate::util::short_id("l-"), "source": source, "rect": rect })
}

/// 기본 틀로 칸 목록 만들기. camera = 카메라 칸에 처음 넣을 카메라 소스 (없으면 빈 칸 — 고르면 끝)
pub fn preset_layers(kind: &str, camera: Option<&str>) -> Vec<Value> {
    let cam = camera.unwrap_or("");
    let full = [0.0, 0.0, 1.0, 1.0];
    match kind {
        // 좌우 나눔: 왼쪽 카메라 · 오른쪽 송출 중인 순서 (배경은 전체)
        "split" => vec![layer("background", full), layer(cam, [0.0, 0.0, 0.5, 1.0]), layer("live", [0.5, 0.0, 0.5, 1.0])],
        // 작은 화면: 전체 화면 그대로 + 오른쪽 아래 카메라 (16:9 비율, 폭 30%)
        "pip" => {
            let mut v = vec![layer("background", full), layer("media", full), layer("slide-bg", full), layer("live", full)];
            let mut c = layer(cam, [0.67, 0.65, 0.3, 0.3]);
            c["radius"] = json!(16);
            c["border"] = json!({ "width": 4, "color": "#ffffff" });
            v.push(c);
            v
        }
        // 아래 자막: 카메라 전체 + 송출 중인 순서를 OBS 자막 모양(아래 띠)으로
        "lower" => {
            let mut l = layer("live", full);
            l["tpl"] = json!("overlay");
            vec![layer(cam, full), l]
        }
        // 3분할: 왼쪽 큰 칸(송출 중인 순서) + 오른쪽 위 · 아래 작은 칸 둘 (카메라 · 빈 칸)
        "three" => vec![layer("background", full), layer("live", [0.0, 0.0, 2.0 / 3.0, 1.0]), layer(cam, [2.0 / 3.0, 0.0, 1.0 / 3.0, 0.5]), layer("", [2.0 / 3.0, 0.5, 1.0 / 3.0, 0.5])],
        // 전체: 기본 구성과 같은 차례
        _ => vec![layer("background", full), layer("media", full), layer("slide-bg", full), layer("live", full)],
    }
}

/// 새 구성 (기본 틀에서)
pub fn new_scene(kind: &str, name: &str, camera: Option<&str>) -> Value {
    let nm = if name.trim().is_empty() { PRESETS.iter().find(|(k, _)| *k == kind).map(|(_, n)| crate::i18n::tr(n)).unwrap_or_else(|| crate::i18n::tr("새 화면 구성")) } else { name.to_string() };
    normalize_scene(&json!({ "name": nm, "layers": preset_layers(kind, camera) })).unwrap_or(Value::Null)
}

/// 출력에 보낼 구성: 사용자 칸 + (구성에 없으면) 오버레이 · 회중 메시지 · 로고 화면 · 그리기 · 그림 우상단을 맨 위에.
/// 칸의 글 모양(tpl)은 `resolve` 가 템플릿 값으로 바꿔 `template` 에 붙인다 (출력 데이터에만).
pub fn effective_scene(scene: &Value, output: &str, overlays: &[Value], resolve: &dyn Fn(&str) -> Option<Value>) -> Value {
    let mut layers: Vec<Value> = vec![];
    for l in scene["layers"].as_array().into_iter().flatten() {
        let mut l = l.clone();
        l.as_object_mut().map(|o| o.remove("locked"));
        let tpl = l["tpl"].as_str().unwrap_or("").to_string();
        if !tpl.is_empty() {
            if let Some(t) = resolve(&tpl) {
                l["template"] = t;
            }
        }
        layers.push(l);
    }
    let has = |layers: &[Value], s: &str| layers.iter().any(|l| l["source"] == s);
    let auto = |id: &str| json!({ "id": id, "source": id, "auto": true });
    for o in overlays {
        let id = o["id"].as_str().unwrap_or("");
        if id == "wpmessage" {
            continue;
        }
        let s = format!("overlay:{id}");
        if !has(&layers, &s) {
            layers.push(auto(&s));
        }
    }
    for s in ["message", "logo-screen", "draw"] {
        if !has(&layers, s) {
            layers.push(auto(s));
        }
    }
    if output == "overlay" && !has(&layers, "pip") {
        layers.push(auto("pip"));
    }
    json!({ "id": scene["id"], "name": scene["name"], "layers": layers })
}

/// 구성이 쓰는 사용자 소스 id (웹 페이지 등 — 켜 두지 않아도 구성에 넣었으면 그린다)
pub fn used_user_sources(scene: &Value) -> Vec<String> {
    scene["layers"].as_array().into_iter().flatten().filter(|l| l["visible"] != false).filter_map(|l| l["source"].as_str().filter(|s| s.starts_with("src-")).map(String::from)).collect()
}

/// 구성이 쓰는 카메라 (deviceId)
pub fn used_cameras(scene: &Value) -> Vec<String> {
    scene["layers"].as_array().into_iter().flatten().filter(|l| l["visible"] != false).filter_map(|l| l["source"].as_str().and_then(|s| s.strip_prefix("camera:")).map(String::from)).collect()
}

/// 이 칸을 웹 출력이 자기 상자(자리 · 자르기 · 글 모양 따로)로 그려야 하나 — 아니면 예전처럼 무대에 바로 (parity 그대로)
pub fn needs_box(l: &Value) -> bool {
    rect_of(l) != [0.0, 0.0, 1.0, 1.0] || crop_of(l).iter().any(|x| *x > 0.0) || l["template"].is_object()
}

/// 무대(템플릿 크기) 안 칸 상자의 크기에 맞춘 '가상 템플릿' (글자 다시 맞추기, 웹 render.js sceneTemplate 과 같은 규칙):
/// 높이는 그 템플릿 높이 그대로, 폭은 칸 비율대로 → 글자 크기는 칸 높이에 비례하고, 넘치면 맞춤(fit)이 줄인다.
pub fn box_template(t: &Value, box_w: f64, box_h: f64) -> Value {
    let mut v = t.clone();
    let th = num(&t["height"], 1080.0).max(1.0);
    if box_w > 0.0 && box_h > 0.0 {
        v["width"] = json!((th * box_w / box_h).round().max(1.0));
        v["height"] = json!(th);
    }
    v
}

/// 구성 요약 (목록에 보일 글): 칸 수
pub fn summary(scene: &Value) -> usize {
    scene["layers"].as_array().map(|a| a.len()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_layer_clamps_and_keeps_unknown() {
        let l = normalize_layer(&json!({ "source": "camera:abc", "rect": [0.9, -1, 0.5, 2], "crop": [0.6, 0, 0.6, 0], "opacity": 3, "radius": -5, "border": { "width": 999, "color": "red" }, "tpl": "x y", "future": 1, "template": {} })).unwrap();
        assert!(l["id"].as_str().unwrap().starts_with("l-"));
        assert_eq!(l["rect"], json!([0.5, 0.0, 0.5, 1.0]), "화면 안으로");
        let c = l["crop"].as_array().unwrap();
        assert!((c[0].as_f64().unwrap() + c[2].as_f64().unwrap() - 0.9).abs() < 1e-3, "자르기 합 0.9 까지");
        assert_eq!(l["opacity"], 1.0);
        assert_eq!(l["radius"], 0.0);
        assert_eq!(l["border"], json!({ "width": 60.0, "color": "#ffffff" }));
        assert_eq!(l["tpl"], "");
        assert_eq!(l["future"], 1, "모르는 칸은 그대로");
        assert!(l.get("template").is_none(), "출력용 값은 저장 안 함");
        // 모르는 소스 → 빈 칸
        assert_eq!(normalize_layer(&json!({ "source": "javascript:x" })).unwrap()["source"], "");
        assert!(normalize_layer(&json!(3)).is_err());
    }

    #[test]
    fn save_dup_delete_and_output_choice() {
        let mut s = json!({});
        let a = save_scene(&mut s, &new_scene("lower", "", Some("camera:cam1"))).unwrap();
        let id = a["id"].as_str().unwrap().to_string();
        assert_eq!(a["name"], "아래 자막");
        assert_eq!(a["layers"][0]["source"], "camera:cam1");
        assert_eq!(a["layers"][1]["tpl"], "overlay");
        assert_eq!(output_scene_id(&s, "overlay"), FULL);
        set_output_scene(&mut s, "overlay", &id).unwrap();
        assert_eq!(output_scene_id(&s, "overlay"), id);
        assert!(set_output_scene(&mut s, "overlay", "sc-none").is_err());
        assert!(set_output_scene(&mut s, "stage", &id).is_err());
        let d = duplicate_scene(&mut s, &id).unwrap();
        assert_ne!(d["id"], a["id"]);
        assert_eq!(d["name"], "아래 자막 (사본)");
        assert_eq!(user_scenes(&s).len(), 2);
        delete_scene(&mut s, &id);
        assert_eq!(output_scene_id(&s, "overlay"), FULL, "지운 구성 → 기본");
        assert!(s["outputScenes"].get("overlay").is_none());
        set_output_scene(&mut s, "main", d["id"].as_str().unwrap()).unwrap();
        set_output_scene(&mut s, "main", "full").unwrap();
        assert_eq!(output_scene_id(&s, "main"), FULL);
        // 고른 id 가 설정에서 사라져도(옛 판이 고친 것 등) 기본으로
        s["outputScenes"]["main"] = json!("sc-gone");
        assert_eq!(output_scene_id(&s, "main"), FULL);
    }

    #[test]
    fn presets_shape() {
        for (k, _) in PRESETS {
            let sc = new_scene(k, "", None);
            assert!(!sc["layers"].as_array().unwrap().is_empty(), "{k}");
            assert!(sc["layers"].as_array().unwrap().iter().any(|l| l["source"] == "live"), "{k}: 송출 중인 순서 칸");
        }
        let sp = new_scene("split", "", Some("camera:c"));
        assert_eq!(sp["layers"][1]["rect"], json!([0.0, 0.0, 0.5, 1.0]));
        assert_eq!(sp["layers"][2]["rect"], json!([0.5, 0.0, 0.5, 1.0]));
        let pip = new_scene("pip", "", None);
        assert_eq!(pip["layers"][4]["source"], "", "카메라가 없으면 빈 칸");
        let th = new_scene("three", "", None);
        assert_eq!(th["layers"].as_array().unwrap().len(), 4);
    }

    #[test]
    fn effective_scene_adds_top_layers_and_templates() {
        let sc = new_scene("lower", "", Some("camera:c1"));
        let ov = vec![json!({ "id": "o1" }), json!({ "id": "wpmessage" })];
        let e = effective_scene(&sc, "overlay", &ov, &|t| (t == "overlay").then(|| json!({ "id": "ovt", "width": 1920, "height": 1080 })));
        let ids: Vec<&str> = e["layers"].as_array().unwrap().iter().map(|l| l["source"].as_str().unwrap()).collect();
        assert_eq!(ids, ["camera:c1", "live", "overlay:o1", "message", "logo-screen", "draw", "pip"]);
        assert_eq!(e["layers"][1]["template"]["id"], "ovt");
        assert!(e["layers"][0].get("locked").is_none());
        assert_eq!(used_cameras(&sc), ["c1"]);
        assert!(needs_box(&e["layers"][1]), "다른 글 모양이면 자기 상자");
        assert!(!needs_box(&e["layers"][0]), "전체 · 자르기 없음 → 무대에 바로");
    }

    #[test]
    fn box_template_keeps_height() {
        let t = json!({ "width": 1920, "height": 1080, "font": { "size": 72 } });
        let b = box_template(&t, 960.0, 1080.0);
        assert_eq!(b["width"], 960.0);
        assert_eq!(b["height"], 1080.0);
        let p = box_template(&t, 576.0, 324.0);
        assert_eq!(p["width"], 1920.0, "같은 비율이면 그대로");
        assert_eq!(b["font"]["size"], 72);
    }

    #[test]
    fn old_settings_without_scenes() {
        let s = json!({ "scenes": [{ "id": "bad id!" }, 7, { "id": "sc-ok", "name": "a", "layers": [{ "source": "live" }] }], "outputScenes": { "main": "sc-ok" } });
        assert_eq!(user_scenes(&s).len(), 1);
        assert_eq!(output_scene_id(&s, "main"), "sc-ok");
        assert_eq!(output_scene_id(&json!({}), "main"), FULL);
    }
}
