//! 출력 목록 · 출력마다 다른 화면 구성 (로드맵 3단계, docs/roadmap-sources.md '3단계 한 것')
//!
//! - 내장 출력: `main`(예배당) · `overlay`(방송 OBS). 주소 /outputs/main · /outputs/overlay (그대로).
//! - 추가 출력 (설정 `extraOutputs[]`, 덧붙인 키): `{ id: "out-…", name, kind: "main"|"overlay", template, monitor, window }`.
//!   주소 /outputs/<id>. `kind` = 어느 내장 출력처럼 그리는지 (템플릿 고르는 규칙 · 오버레이 · 배경 미디어 · 그림 우상단).
//!   `template` 이 비면 그 내장 출력의 템플릿을 그대로 쓴다. `monitor`(모니터 번호, -1 = 없음) · `window`(모니터에 띄우기)는 조작 화면이 창을 띄울 때만 쓴다.
//! - 화면 구성을 고르는 차례 (출력마다 따로):
//!   순서의 `scenes[출력]` → 설정 `typeScenes[순서 종류][출력]` → 설정 `outputScenes[출력]` → 기본 `full`.
//!   순서 · 종류 칸에 `"full"` 을 넣으면 '이 순서는 전체 화면(기본)' 으로 못 박는다. 없는 구성 id 는 건너뛴다 (지운 구성 · 다른 PC 에서 온 큐시트).
//! - 소리 (설정 `output.audioOutput`, 기본 `main`): 영상 · 유튜브 소리는 이 출력 하나에서만 난다 (그 출력이 모니터 창일 때 — 2.0.8 과 같음).
use anyhow::{bail, Result};
use serde_json::{json, Value};

/// 추가 출력 수 상한
pub const MAX_EXTRA: usize = 8;

/// 내장 출력 (id, 이름)
pub const BUILTIN: [(&str, &str); 2] = [("main", "예배당"), ("overlay", "방송 (OBS)")];

/// 순서 종류별 기본 구성을 고를 수 있는 종류 (순서 종류 type, 이름)
pub const SCENE_TYPES: [(&str, &str); 10] = [
    ("bible", "성경"),
    ("song", "찬양"),
    ("hymn", "찬송가 · PPT"),
    ("text", "공지"),
    ("title", "제목"),
    ("image", "그림"),
    ("video", "영상"),
    ("camera", "카메라"),
    ("slideshow", "슬라이드쇼"),
    ("blank", "빈 화면"),
];

pub fn type_name(t: &str) -> &'static str {
    SCENE_TYPES.iter().find(|(k, _)| *k == t).map(|(_, n)| *n).unwrap_or("순서")
}

fn ok_extra_id(s: &str) -> bool {
    s.starts_with("out-") && s.len() <= 40 && s.len() > 4 && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// 추가 출력 하나 정리 (모르는 칸은 그대로)
pub fn normalize_extra(v: &Value) -> Result<Value> {
    if !v.is_object() {
        bail!("{}", crate::i18n::tr("출력 모양이 맞지 않습니다."));
    }
    let mut o = v.as_object().cloned().unwrap_or_default();
    let id = v["id"].as_str().filter(|x| ok_extra_id(x)).map(String::from).unwrap_or_else(|| crate::util::short_id("out-"));
    o.insert("id".into(), json!(id));
    let name: String = v["name"].as_str().unwrap_or("").chars().filter(|c| !c.is_control()).take(40).collect::<String>().trim().to_string();
    o.insert("name".into(), json!(if name.is_empty() { crate::i18n::tr("새 출력") } else { name }));
    o.insert("kind".into(), json!(if v["kind"] == "overlay" { "overlay" } else { "main" }));
    let t = v["template"].as_str().unwrap_or("");
    o.insert("template".into(), json!(if t.len() <= 64 && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') { t } else { "" }));
    o.insert("monitor".into(), json!(v["monitor"].as_i64().filter(|m| (0..64).contains(m)).unwrap_or(-1)));
    o.insert("window".into(), json!(v["window"].as_bool().unwrap_or(false)));
    // 예배당 화면과 함께 켜고 끄기 (로드맵 4단계, 기본 꺼짐 — 없으면 칸을 넣지 않아 옛 모양 그대로)
    if v["followMain"] == true {
        o.insert("followMain".into(), json!(true));
    } else {
        o.remove("followMain");
    }
    Ok(Value::Object(o))
}

/// 설정의 추가 출력 (깨진 것 · 겹친 id 는 건너뜀)
pub fn extra_outputs(settings: &Value) -> Vec<Value> {
    let mut out: Vec<Value> = vec![];
    for x in settings["extraOutputs"].as_array().into_iter().flatten() {
        if let Ok(n) = normalize_extra(x) {
            if n["id"] == x["id"] && !out.iter().any(|o| o["id"] == n["id"]) {
                out.push(n);
            }
        }
        if out.len() >= MAX_EXTRA {
            break;
        }
    }
    out
}

pub fn find_extra(settings: &Value, id: &str) -> Option<Value> {
    if !id.starts_with("out-") {
        return None;
    }
    extra_outputs(settings).into_iter().find(|o| o["id"] == id)
}

/// 모든 출력 id (예배당 · 방송 · 추가 출력 차례)
pub fn all_ids(settings: &Value) -> Vec<String> {
    let mut v: Vec<String> = BUILTIN.iter().map(|(k, _)| k.to_string()).collect();
    v.extend(extra_outputs(settings).iter().filter_map(|o| o["id"].as_str().map(String::from)));
    v
}

/// 출력 (id, 이름) — 내장 출력 이름은 한국어 원문 (화면이 tr 로), 추가 출력 이름은 사용자 글
pub fn list(settings: &Value) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = BUILTIN.iter().map(|(k, n)| (k.to_string(), n.to_string())).collect();
    v.extend(extra_outputs(settings).iter().map(|o| (o["id"].as_str().unwrap_or("").to_string(), o["name"].as_str().unwrap_or("").to_string())));
    v
}

/// 있는 출력인가 (main · overlay · 추가 출력)
pub fn is_output(settings: &Value, id: &str) -> bool {
    id == "main" || id == "overlay" || find_extra(settings, id).is_some()
}

/// 이 출력을 어느 내장 출력처럼 그리나 (모르는 출력은 main — 옛 서버도 모르는 이름은 main 으로 받았다)
pub fn kind_of(settings: &Value, id: &str) -> &'static str {
    match id {
        "overlay" => "overlay",
        "main" => "main",
        _ => match find_extra(settings, id) {
            Some(o) if o["kind"] == "overlay" => "overlay",
            _ => "main",
        },
    }
}

/// 소리 내는 출력 (설정 output.audioOutput, 없거나 지운 출력이면 main)
pub fn audio_output(settings: &Value) -> String {
    let a = settings["output"]["audioOutput"].as_str().unwrap_or("");
    if !a.is_empty() && is_output(settings, a) {
        a.to_string()
    } else {
        "main".into()
    }
}

pub fn set_audio_output(settings: &mut Value, id: &str) -> Result<()> {
    if !is_output(settings, id) {
        bail!("{}", crate::i18n::tr("모르는 출력입니다."));
    }
    if !settings["output"].is_object() {
        settings["output"] = json!({});
    }
    if id == "main" {
        settings["output"].as_object_mut().unwrap().remove("audioOutput");
    } else {
        settings["output"]["audioOutput"] = json!(id);
    }
    Ok(())
}

pub fn save_extra(settings: &mut Value, v: &Value) -> Result<Value> {
    let n = normalize_extra(v)?;
    if !settings["extraOutputs"].is_array() {
        settings["extraOutputs"] = json!([]);
    }
    let list = settings["extraOutputs"].as_array_mut().unwrap();
    match list.iter().position(|x| x["id"] == n["id"]) {
        Some(i) => list[i] = n.clone(),
        None => {
            if list.len() >= MAX_EXTRA {
                bail!("{}", crate::i18n::tr("출력을 더 만들 수 없습니다. 쓰지 않는 출력을 지워 주세요."));
            }
            list.push(n.clone())
        }
    }
    Ok(n)
}

/// 새 추가 출력 (이름 · 어느 출력처럼)
pub fn new_extra(name: &str, kind: &str) -> Value {
    normalize_extra(&json!({ "name": name, "kind": kind })).unwrap_or(Value::Null)
}

/// 지우기: 그 출력의 구성 지정 · 종류별 지정 · 소리 출력도 같이 정리 (순서에 남은 지정은 두어도 모르는 출력이라 안 씀)
pub fn delete_extra(settings: &mut Value, id: &str) {
    if let Some(l) = settings["extraOutputs"].as_array_mut() {
        l.retain(|x| x["id"] != id);
    }
    if let Some(m) = settings["outputScenes"].as_object_mut() {
        m.remove(id);
    }
    if let Some(m) = settings["typeScenes"].as_object_mut() {
        for (_, v) in m.iter_mut() {
            if let Some(o) = v.as_object_mut() {
                o.remove(id);
            }
        }
    }
    if settings["output"]["audioOutput"] == id {
        settings["output"].as_object_mut().map(|o| o.remove("audioOutput"));
    }
    crate::scenectl::forget_output(settings, id);
}

/// 구성을 어디에서 골랐나 (조작 화면 표시용)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum From {
    /// 순서에 직접
    Item,
    /// 순서 종류 기본
    Type,
    /// 출력 기본
    Output,
    /// 아무것도 안 고름 → full
    Default,
    /// 외부 단추 · 리모컨이 '지금 한 번' 고름 (로드맵 4단계, 다음 순서까지)
    Once,
}

/// 칸 값 하나 → 쓸 수 있는 구성 id ("full" 은 그대로, 없는 구성 · 빈 값은 None)
fn usable(settings: &Value, v: &Value) -> Option<String> {
    let id = v.as_str().filter(|x| !x.is_empty())?;
    if id == crate::scenes::FULL || crate::scenes::find_scene(settings, id).is_some() {
        Some(id.to_string())
    } else {
        None
    }
}

/// 출력의 화면 구성 id 와 고른 곳. passage = 송출 중(또는 미리보기)인 순서의 정보 (scenes · itemType)
pub fn resolve_scene(settings: &Value, output: &str, passage: Option<&Value>) -> (String, From) {
    if let Some(p) = passage {
        if let Some(id) = usable(settings, &p["scenes"][output]) {
            return (id, From::Item);
        }
        let ty = p["itemType"].as_str().unwrap_or("");
        if !ty.is_empty() {
            if let Some(id) = usable(settings, &settings["typeScenes"][ty][output]) {
                return (id, From::Type);
            }
        }
    }
    let id = crate::scenes::output_scene_id(settings, output);
    if id == crate::scenes::FULL {
        (id, From::Default)
    } else {
        (id, From::Output)
    }
}

/// 큐시트 순서 하나(item JSON)의 출력별 구성 — 시간표 · 큐시트 표시용 (passage 대신 item 을 바로 봄)
pub fn item_scene(settings: &Value, output: &str, item: &Value) -> (String, From) {
    resolve_scene(settings, output, Some(&json!({ "scenes": item["scenes"], "itemType": item["type"] })))
}

/// 이 순서가 출력 기본과 다른 구성을 쓰는 출력들: [(출력 id, 구성 id)]
pub fn item_differs(settings: &Value, item: &Value) -> Vec<(String, String)> {
    all_ids(settings)
        .into_iter()
        .filter_map(|o| {
            let (id, from) = item_scene(settings, &o, item);
            let base = crate::scenes::output_scene_id(settings, &o);
            (matches!(from, From::Item | From::Type) && id != base).then_some((o, id))
        })
        .collect()
}

/// 순서에 출력별 구성 지정 ("" = 지정 없음, 따라감). item["scenes"] 를 고친다 (빈 묶음이면 칸을 지움 — 저장 모양 그대로)
pub fn set_item_scene(item: &mut Value, output: &str, id: &str) {
    if !item["scenes"].is_object() {
        item["scenes"] = json!({});
    }
    let m = item["scenes"].as_object_mut().unwrap();
    if id.is_empty() {
        m.remove(output);
    } else {
        m.insert(output.into(), json!(id));
    }
    if m.is_empty() {
        item.as_object_mut().map(|o| o.remove("scenes"));
    }
}

/// 순서 종류별 기본 구성 ("" = 지정 없음)
pub fn set_type_scene(settings: &mut Value, ty: &str, output: &str, id: &str) -> Result<()> {
    if !SCENE_TYPES.iter().any(|(k, _)| *k == ty) {
        bail!("{}", crate::i18n::tr("모르는 순서 종류입니다."));
    }
    if !is_output(settings, output) {
        bail!("{}", crate::i18n::tr("모르는 출력입니다."));
    }
    if !(id.is_empty() || id == crate::scenes::FULL || crate::scenes::find_scene(settings, id).is_some()) {
        bail!("{}", crate::i18n::tr("화면 구성을 찾을 수 없습니다."));
    }
    if !settings["typeScenes"].is_object() {
        settings["typeScenes"] = json!({});
    }
    let m = settings["typeScenes"].as_object_mut().unwrap();
    let e = m.entry(ty.to_string()).or_insert_with(|| json!({}));
    if !e.is_object() {
        *e = json!({});
    }
    let o = e.as_object_mut().unwrap();
    if id.is_empty() {
        o.remove(output);
    } else {
        o.insert(output.into(), json!(id));
    }
    if o.is_empty() {
        m.remove(ty);
    }
    Ok(())
}

pub fn type_scene(settings: &Value, ty: &str, output: &str) -> String {
    settings["typeScenes"][ty][output].as_str().unwrap_or("").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_scenes() -> Value {
        let mut s = json!({});
        crate::scenes::save_scene(&mut s, &json!({ "id": "sc-a", "name": "본문 전체", "layers": [{ "source": "live" }] })).unwrap();
        crate::scenes::save_scene(&mut s, &json!({ "id": "sc-b", "name": "카메라 + 자막", "layers": [{ "source": "camera:c1" }, { "source": "live", "tpl": "overlay" }] })).unwrap();
        s
    }

    #[test]
    fn resolve_order_item_type_output_default() {
        let mut s = with_scenes();
        let p = json!({ "itemType": "bible", "scenes": { "overlay": "sc-b" } });
        assert_eq!(resolve_scene(&s, "overlay", Some(&p)), ("sc-b".into(), From::Item));
        assert_eq!(resolve_scene(&s, "main", Some(&p)), ("full".into(), From::Default));
        set_type_scene(&mut s, "bible", "main", "sc-a").unwrap();
        assert_eq!(resolve_scene(&s, "main", Some(&p)), ("sc-a".into(), From::Type));
        crate::scenes::set_output_scene(&mut s, "main", "sc-b").unwrap();
        assert_eq!(resolve_scene(&s, "main", Some(&json!({ "itemType": "song" }))), ("sc-b".into(), From::Output));
        // 순서에 'full' 을 못 박으면 종류 · 출력 기본보다 먼저
        let p2 = json!({ "itemType": "bible", "scenes": { "main": "full" } });
        assert_eq!(resolve_scene(&s, "main", Some(&p2)), ("full".into(), From::Item));
        // 없는 구성 id 는 건너뜀 (다른 PC 에서 온 큐시트)
        let p3 = json!({ "itemType": "bible", "scenes": { "main": "sc-gone" } });
        assert_eq!(resolve_scene(&s, "main", Some(&p3)), ("sc-a".into(), From::Type));
        assert_eq!(resolve_scene(&s, "main", None), ("sc-b".into(), From::Output));
        assert!(set_type_scene(&mut s, "nope", "main", "sc-a").is_err());
        assert!(set_type_scene(&mut s, "bible", "out-none", "sc-a").is_err());
        assert!(set_type_scene(&mut s, "bible", "main", "sc-none").is_err());
        set_type_scene(&mut s, "bible", "main", "").unwrap();
        assert!(s["typeScenes"].get("bible").is_none(), "빈 묶음은 지움");
    }

    #[test]
    fn extra_outputs_save_kind_delete() {
        let mut s = with_scenes();
        let o = save_extra(&mut s, &new_extra("로비 화면", "main")).unwrap();
        let id = o["id"].as_str().unwrap().to_string();
        assert!(id.starts_with("out-"));
        assert_eq!(o["monitor"], -1);
        assert_eq!(o["window"], false);
        assert!(is_output(&s, &id));
        assert_eq!(kind_of(&s, &id), "main");
        let b = save_extra(&mut s, &new_extra("둘째 방송", "overlay")).unwrap();
        assert_eq!(kind_of(&s, b["id"].as_str().unwrap()), "overlay");
        assert_eq!(all_ids(&s).len(), 4);
        assert_eq!(kind_of(&s, "out-none"), "main");
        assert!(!is_output(&s, "out-none"));
        crate::scenes::set_output_scene(&mut s, &id, "sc-a").unwrap();
        set_type_scene(&mut s, "song", &id, "sc-b").unwrap();
        set_audio_output(&mut s, &id).unwrap();
        assert_eq!(audio_output(&s), id);
        delete_extra(&mut s, &id);
        assert!(!is_output(&s, &id));
        assert!(s["outputScenes"].get(&id).is_none());
        assert!(s["typeScenes"]["song"].get(&id).is_none());
        assert_eq!(audio_output(&s), "main", "지운 출력이 소리 출력이면 예배당으로");
        // 상한
        for i in 0..MAX_EXTRA {
            let _ = save_extra(&mut s, &new_extra(&format!("{i}"), "main"));
        }
        assert_eq!(extra_outputs(&s).len(), MAX_EXTRA);
        assert!(save_extra(&mut s, &new_extra("넘침", "main")).is_err());
    }

    #[test]
    fn item_scene_marks_and_set() {
        let mut s = with_scenes();
        let mut it = json!({ "type": "bible", "title": "성경봉독" });
        assert!(item_differs(&s, &it).is_empty(), "아무것도 안 고르면 표시 없음");
        set_item_scene(&mut it, "overlay", "sc-b");
        assert_eq!(it["scenes"], json!({ "overlay": "sc-b" }));
        assert_eq!(item_differs(&s, &it), vec![("overlay".to_string(), "sc-b".to_string())]);
        set_type_scene(&mut s, "bible", "main", "sc-a").unwrap();
        assert_eq!(item_differs(&s, &it).len(), 2);
        // 출력 기본과 같으면 표시 안 함
        crate::scenes::set_output_scene(&mut s, "main", "sc-a").unwrap();
        assert_eq!(item_differs(&s, &it).len(), 1);
        set_item_scene(&mut it, "overlay", "");
        assert!(it.get("scenes").is_none(), "빈 지정은 칸을 지움 (옛 모양 그대로)");
    }

    #[test]
    fn old_settings_and_bad_values() {
        let s = json!({ "extraOutputs": [7, { "id": "bad id" }, { "id": "out-1", "name": "로비", "kind": "x", "monitor": 99, "future": true }, { "id": "out-1" }], "output": { "audioOutput": "out-gone" } });
        let l = extra_outputs(&s);
        assert_eq!(l.len(), 1);
        assert_eq!(l[0]["kind"], "main");
        assert_eq!(l[0]["monitor"], -1);
        assert_eq!(l[0]["future"], true, "모르는 칸은 그대로");
        assert_eq!(audio_output(&s), "main");
        assert_eq!(audio_output(&json!({})), "main");
        assert_eq!(all_ids(&json!({})), ["main", "overlay"]);
    }
}
