//! [트랙 C] 테마 프리셋 (Electron themes.js 를 그대로 뽑은 data/themes.json: 구세군 5 + 방송 스타일 5)
//! 테마마다 예배당(main)·OBS 자막(overlay) 템플릿 한 쌍. applyTheme · ensureTheme · OBS 보기(setObsView)

use crate::core::{Core, Ev};
use once_cell::sync::Lazy;
use serde_json::{json, Value};

pub static THEMES: Lazy<Vec<Value>> = Lazy::new(|| serde_json::from_str(include_str!("../data/themes.json")).expect("themes.json"));

const OUTPUTS: [&str; 2] = ["main", "overlay"];

pub fn list_themes() -> &'static [Value] {
    &THEMES
}

fn find(id: &str) -> Result<&'static Value, String> {
    THEMES.iter().find(|t| t["id"] == id).ok_or_else(|| "테마를 찾을 수 없습니다.".to_string())
}

/// 테마 적용: 테마 템플릿을 만들거나 기본값으로 되돌린 뒤 출력에 지정 (outputs 가 비면 둘 다)
pub fn apply_theme(c: &mut Core, id: &str, outputs: &[&str]) -> Result<Vec<String>, String> {
    let theme = find(id)?;
    let outs: Vec<&str> = if outputs.is_empty() { OUTPUTS.to_vec() } else { outputs.iter().copied().filter(|o| OUTPUTS.contains(o)).collect() };
    c.settings.update(|d| {
        for o in &outs {
            let tpl = crate::settings::normalize_template(&theme[*o]);
            let tid = tpl["id"].clone();
            let list = d["templates"].as_array_mut().unwrap();
            match list.iter().position(|x| x["id"] == tid) {
                Some(i) => list[i] = tpl,
                None => list.push(tpl),
            }
            d["activeTemplates"][*o] = tid;
        }
    });
    c.emit(Ev::Live);
    c.emit(Ev::Ui);
    Ok(outs.iter().map(|s| s.to_string()).collect())
}

/// 순서별 테마: 테마의 두 템플릿이 없으면 만들고 id 를 돌려준다 (이미 고친 템플릿은 그대로)
pub fn ensure_theme(c: &mut Core, id: &str) -> Result<Value, String> {
    // 내 테마: 이미 설정에 있는 템플릿 id 를 그대로 돌려준다 (새로 만들거나 고치지 않는다)
    if let Some(ct) = custom_record(c.settings.get(), id) {
        let mut ids = json!({});
        for o in OUTPUTS {
            let tid = ct["slots"][o].as_str().unwrap_or("");
            if c.settings.template(tid).is_some() {
                ids[o] = json!(tid);
            }
        }
        return Ok(ids);
    }
    let theme = find(id)?;
    let mut ids = json!({});
    let mut added = false;
    c.settings.update(|d| {
        for o in OUTPUTS {
            let tpl = crate::settings::normalize_template(&theme[o]);
            ids[o] = tpl["id"].clone();
            let list = d["templates"].as_array_mut().unwrap();
            if !list.iter().any(|x| x["id"] == tpl["id"]) {
                list.push(tpl);
                added = true;
            }
        }
    });
    if added {
        c.emit(Ev::Ui);
    }
    Ok(ids)
}

// ---------------------------------------------------------------------------------------------
// 테마 관리 (2026-11-16): 내 테마(customThemes) · 비파괴 적용 · 적용 되돌리기 · JSON 가져오기/내보내기
// 원본 Electron 판에는 없는 기능이다 (원본은 고정 프리셋 10개 + applyTheme 덮어쓰기뿐). docs/theme-management.md
//
// 저장 모양 (settings.json, 모르는 칸은 그대로 둔다):
//   customThemes: [{ id, name, desc, createdAt, updatedAt, slots: { main, overlay, overlayLyrics } }]
//   slots 의 값은 templates 안 템플릿 id. 내 테마의 템플릿은 늘 새 id (다른 템플릿 원본을 건드리지 않는다)
// ---------------------------------------------------------------------------------------------

/// 출력 칸 (activeTemplates 열쇠): 예배당 · OBS 자막 · OBS 찬송가·찬양
pub const SLOTS: [&str; 3] = ["main", "overlay", "overlayLyrics"];
/// 내보내기 파일 형식 이름과 지금 버전
pub const EXPORT_FORMAT: &str = "worshippresenter-theme";
pub const EXPORT_VERSION: u64 = 1;
/// 가져오기 파일 크기 한도 (바이트)
pub const IMPORT_MAX_BYTES: usize = 1024 * 1024;
const NAME_MAX: usize = 60;
const DESC_MAX: usize = 300;

pub fn slot_label(slot: &str) -> &'static str {
    match slot {
        "main" => "예배당",
        "overlay" => "OBS 자막",
        _ => "OBS 찬송가·찬양",
    }
}

fn custom_list(s: &Value) -> &[Value] {
    s["customThemes"].as_array().map(|a| a.as_slice()).unwrap_or(&[])
}

fn custom_record<'a>(s: &'a Value, id: &str) -> Option<&'a Value> {
    custom_list(s).iter().find(|t| t["id"] == id && !id.is_empty())
}

pub fn is_builtin(id: &str) -> bool {
    THEMES.iter().any(|t| t["id"] == id)
}

fn stored<'a>(s: &'a Value, tid: &str) -> Option<&'a Value> {
    s["templates"].as_array()?.iter().find(|t| t["id"] == tid && !tid.is_empty())
}

/// 기본 테마의 처음 디자인 (정규화한 것)
fn pristine(theme: &Value, slot: &str) -> Option<Value> {
    theme[slot].is_object().then(|| crate::settings::normalize_template(&theme[slot]))
}

/// 기본 테마 + 내 테마를 한 모양으로. 항목:
/// `{ id, name, desc, group, builtin, slots: { main: 템플릿, overlay: 템플릿, overlayLyrics?: 템플릿 }, modified: [칸…] }`
/// 기본 테마의 템플릿은 설정에 이미 있으면(사용자가 고쳤을 수 있음) 그것을, 없으면 처음 디자인을 준다.
/// `modified` 는 처음 디자인과 달라진 칸 (기본 테마만). 기본 테마에는 찬송가 칸이 없다.
pub fn list_all(s: &Value) -> Vec<Value> {
    let mut out = vec![];
    for t in THEMES.iter() {
        let mut slots = json!({});
        let mut modified = vec![];
        for o in OUTPUTS {
            let Some(orig) = pristine(t, o) else { continue };
            match stored(s, orig["id"].as_str().unwrap_or("")) {
                Some(cur) => {
                    if crate::settings::normalize_template(cur) != orig {
                        modified.push(o);
                    }
                    slots[o] = cur.clone();
                }
                None => slots[o] = orig,
            }
        }
        out.push(json!({ "id": t["id"], "name": t["name"], "desc": t["desc"], "group": t["group"], "builtin": true, "slots": slots, "modified": modified }));
    }
    for t in custom_list(s) {
        let mut slots = json!({});
        for o in SLOTS {
            if let Some(cur) = t["slots"][o].as_str().and_then(|tid| stored(s, tid)) {
                slots[o] = cur.clone();
            }
        }
        if !slots.as_object().is_some_and(|m| !m.is_empty()) {
            continue; // 템플릿이 모두 사라진 기록은 보여 주지 않는다 (기록 자체는 지우지 않음)
        }
        out.push(json!({ "id": t["id"], "name": t["name"].as_str().unwrap_or(""), "desc": t["desc"].as_str().unwrap_or(""), "group": "내 테마", "builtin": false, "slots": slots, "modified": [] }));
    }
    out
}

/// 이 테마(list_all 항목)가 지금 쓰이는 출력 칸
pub fn used_slots(s: &Value, theme: &Value) -> Vec<&'static str> {
    SLOTS.iter().copied().filter(|o| theme["slots"][*o]["id"].as_str().is_some_and(|tid| s["activeTemplates"][*o] == tid)).collect()
}

/// 적용·원복을 되돌리는 데 필요한 것만 담는다: 건드린 출력 칸의 전 값, 건드린 템플릿의 전 모양(없었으면 None)
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ApplyUndo {
    pub theme_name: String,
    pub active: Vec<(String, Value)>,
    pub templates: Vec<(String, Option<Value>)>,
    /// 적용 직후 상태. 그 뒤 편집한 디자인을 되돌리기로 덮어쓰지 않는다.
    pub expected_active: Vec<(String, Value)>,
    pub expected_templates: Vec<(String, Option<Value>)>,
}

/// 적용 방식
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApplyMode {
    /// 이미 있는(고쳤을 수 있는) 템플릿은 그대로 두고 출력에 지정만 한다
    Keep,
    /// 기본 테마 템플릿을 처음 디자인으로 다시 만든 뒤 지정한다 (원본 applyTheme 과 같은 동작)
    Reset,
}

fn theme_name(s: &Value, id: &str) -> String {
    custom_record(s, id).map(|t| t["name"].as_str().unwrap_or("").to_string()).or_else(|| find(id).ok().map(|t| t["name"].as_str().unwrap_or("").to_string())).unwrap_or_default()
}

/// 테마를 **명시한** 출력 칸에만 적용한다 (기본·내 테마 모두). 테마에 없는 칸은 건너뛴다
/// (기본 테마에는 찬송가 칸이 없으므로 overlayLyrics 지정은 바뀌지 않는다).
/// 돌려주는 것: (실제로 바꾼 칸, 되돌리기 정보). 아무 칸도 못 바꾸면 Err 이고 설정은 그대로다.
pub fn apply_theme_ex(c: &mut Core, id: &str, slots: &[&str], mode: ApplyMode) -> Result<(Vec<String>, ApplyUndo), String> {
    let s = c.settings.get();
    // 먼저 할 일을 다 계산한다 (실패하면 설정을 건드리지 않게)
    let mut plan: Vec<(&'static str, String, Option<Value>)> = vec![]; // (칸, 템플릿 id, 새로 넣거나 덮을 템플릿)
    if let Some(ct) = custom_record(s, id) {
        for o in SLOTS.iter().copied().filter(|o| slots.contains(o)) {
            if let Some(tid) = ct["slots"][o].as_str().filter(|tid| stored(s, tid).is_some()) {
                plan.push((o, tid.to_string(), None));
            }
        }
    } else {
        let theme = find(id)?;
        for o in OUTPUTS.iter().copied().filter(|o| slots.contains(o)) {
            let Some(orig) = pristine(theme, o) else { continue };
            let tid = orig["id"].as_str().unwrap_or("").to_string();
            let write = mode == ApplyMode::Reset || stored(s, &tid).is_none();
            plan.push((o, tid, write.then_some(orig)));
        }
    }
    if plan.is_empty() {
        return Err(crate::i18n::tr("이 테마에는 고른 출력에 쓸 디자인이 없습니다."));
    }
    let mut undo = ApplyUndo { theme_name: theme_name(s, id), ..Default::default() };
    for (o, tid, write) in &plan {
        undo.active.push((o.to_string(), s["activeTemplates"][*o].clone()));
        if write.is_some() {
            undo.templates.push((tid.clone(), stored(s, tid).cloned()));
        }
    }
    c.settings.update(|d| {
        if !d["templates"].is_array() {
            d["templates"] = json!([]);
        }
        if !d["activeTemplates"].is_object() {
            d["activeTemplates"] = json!({});
        }
        for (o, tid, write) in &plan {
            if let Some(tpl) = write {
                let list = d["templates"].as_array_mut().unwrap();
                match list.iter().position(|x| x["id"] == tid.as_str()) {
                    Some(i) => list[i] = tpl.clone(),
                    None => list.push(tpl.clone()),
                }
            }
            d["activeTemplates"][*o] = json!(tid);
        }
    });
    capture_undo_expected(c, &mut undo);
    c.emit(Ev::Live);
    c.emit(Ev::Ui);
    Ok((plan.iter().map(|p| p.0.to_string()).collect(), undo))
}

/// 기본 테마의 템플릿을 처음 디자인으로 되돌린다 (출력 지정은 바꾸지 않는다). 화면에서 따로 확인받고 부른다.
pub fn restore_original(c: &mut Core, id: &str, slots: &[&str]) -> Result<ApplyUndo, String> {
    let theme = find(id)?;
    let s = c.settings.get();
    let mut undo = ApplyUndo { theme_name: theme_name(s, id), ..Default::default() };
    let mut todo = vec![];
    for o in OUTPUTS.iter().copied().filter(|o| slots.contains(o)) {
        let Some(orig) = pristine(theme, o) else { continue };
        let tid = orig["id"].as_str().unwrap_or("").to_string();
        // 설정에 없는 템플릿은 이미 처음 디자인이다 (새로 넣지 않는다)
        if let Some(cur) = stored(s, &tid).filter(|cur| **cur != orig) {
            undo.templates.push((tid.clone(), Some(cur.clone())));
            todo.push((tid, orig));
        }
    }
    if todo.is_empty() {
        return Err(crate::i18n::tr("처음 디자인에서 바뀐 것이 없습니다."));
    }
    c.settings.update(|d| {
        if let Some(list) = d["templates"].as_array_mut() {
            for (tid, orig) in &todo {
                if let Some(i) = list.iter().position(|x| x["id"] == tid.as_str()) {
                    list[i] = orig.clone();
                }
            }
        }
    });
    capture_undo_expected(c, &mut undo);
    c.emit(Ev::Live);
    c.emit(Ev::Ui);
    Ok(undo)
}

fn capture_undo_expected(c: &Core, undo: &mut ApplyUndo) {
    let s = c.settings.get();
    undo.expected_active = undo.active.iter().map(|(slot, _)| (slot.clone(), s["activeTemplates"][slot].clone())).collect();
    undo.expected_templates = undo.templates.iter().map(|(id, _)| (id.clone(), stored(s, id).cloned())).collect();
}

/// 적용·원복 되돌리기: 기록된 출력 칸과 템플릿만 전 상태로 (다른 설정은 건드리지 않는다)
pub fn undo_apply(c: &mut Core, u: &ApplyUndo) -> Result<(), String> {
    let s = c.settings.get();
    let changed = u.expected_active.iter().any(|(slot, v)| s["activeTemplates"][slot] != *v)
        || u.expected_templates.iter().any(|(id, v)| stored(s, id).cloned() != *v);
    if changed {
        return Err(crate::i18n::tr("테마 적용 뒤 디자인이나 출력이 바뀌어 되돌리지 않았습니다. 지금 디자인을 먼저 내 테마로 저장하세요."));
    }
    c.settings.update(|d| {
        for (o, prev) in &u.active {
            d["activeTemplates"][o.as_str()] = prev.clone();
        }
        let in_use = |d: &Value, tid: &str| d["activeTemplates"].as_object().is_some_and(|m| m.values().any(|v| v == tid));
        for (tid, before) in &u.templates {
            // 적용하면서 새로 넣었던 템플릿은 뺀다. 그 사이 다른 출력 칸이 쓰게 됐으면 남긴다 (빈 출력 방지)
            let keep_added = before.is_none() && in_use(d, tid);
            let Some(list) = d["templates"].as_array_mut() else { continue };
            let pos = list.iter().position(|x| x["id"] == tid.as_str());
            match (pos, before) {
                (Some(i), Some(b)) => list[i] = b.clone(),
                (None, Some(b)) => list.push(b.clone()),
                (Some(i), None) if !keep_added => {
                    list.remove(i);
                }
                _ => {}
            }
        }
    });
    c.emit(Ev::Live);
    c.emit(Ev::Ui);
    Ok(())
}

fn clean_text(s: &str, max: usize) -> String {
    s.chars().filter(|ch| !ch.is_control()).collect::<String>().trim().chars().take(max).collect()
}

/// 이미 있는 이름이면 뒤에 (2), (3)… 을 붙인다 (기본 테마 이름과도 겹치지 않게)
fn unique_name(s: &Value, want: &str, except: &str) -> String {
    let taken = |n: &str| THEMES.iter().any(|t| t["name"] == n) || custom_list(s).iter().any(|t| t["name"] == n && t["id"] != except);
    if !taken(want) {
        return want.to_string();
    }
    (2..).map(|k| format!("{want} ({k})")).find(|n| !taken(n)).unwrap()
}

fn new_theme_id(s: &Value) -> String {
    loop {
        let id = crate::util::short_id("ct-");
        if custom_record(s, &id).is_none() && !is_builtin(&id) && !SLOTS.iter().any(|o| stored(s, &tpl_id(&id, o)).is_some()) {
            return id;
        }
    }
}

fn tpl_id(theme_id: &str, slot: &str) -> String {
    format!("tpl-{theme_id}-{slot}")
}

/// 내 테마를 만들어 넣는다: 템플릿을 새 id 로 복사하고 기록을 덧붙인다. 원본 템플릿과 출력 지정은 그대로.
fn insert_custom(c: &mut Core, name: &str, desc: &str, tpls: Vec<(&'static str, Value)>) -> Result<String, String> {
    let name = clean_text(name, NAME_MAX);
    if name.is_empty() {
        return Err(crate::i18n::tr("테마 이름을 입력하세요."));
    }
    if tpls.is_empty() {
        return Err(crate::i18n::tr("저장할 출력 디자인이 없습니다."));
    }
    let s = c.settings.get();
    let id = new_theme_id(s);
    let name = unique_name(s, &name, "");
    let now = crate::util::iso_now();
    let mut slots = json!({});
    let mut add = vec![];
    for (o, src) in tpls {
        let mut t = crate::settings::normalize_template(&src);
        t["id"] = json!(tpl_id(&id, o));
        t["name"] = json!(format!("{name} · {}", slot_label(o)));
        t["preset"] = json!(false);
        t["theme"] = json!(id);
        slots[o] = t["id"].clone();
        add.push(t);
    }
    let rec = json!({ "id": id, "name": name, "desc": clean_text(desc, DESC_MAX), "createdAt": now, "updatedAt": now, "slots": slots });
    c.settings.update(|d| {
        if !d["templates"].is_array() {
            d["templates"] = json!([]);
        }
        d["templates"].as_array_mut().unwrap().extend(add);
        if !d["customThemes"].is_array() {
            d["customThemes"] = json!([]);
        }
        d["customThemes"].as_array_mut().unwrap().push(rec);
    });
    c.emit(Ev::Ui);
    Ok(id)
}

/// 지금 출력에 쓰는 디자인(예배당 · OBS 자막 · OBS 찬송가)을 한 묶음의 내 테마로 저장한다. 새 테마 id 를 돌려준다.
pub fn save_current_as(c: &mut Core, name: &str, desc: &str) -> Result<String, String> {
    let s = c.settings.get();
    let tpls: Vec<(&'static str, Value)> = SLOTS.iter().filter_map(|o| s["activeTemplates"][*o].as_str().and_then(|tid| stored(s, tid)).map(|t| (*o, t.clone()))).collect();
    insert_custom(c, name, desc, tpls)
}

/// 기본 테마나 내 테마를 복제해 새 내 테마를 만든다 (지금 보이는 디자인 그대로). 원본은 건드리지 않는다.
pub fn duplicate_theme(c: &mut Core, src_id: &str, name: &str) -> Result<String, String> {
    let all = list_all(c.settings.get());
    let src = all.iter().find(|t| t["id"] == src_id).ok_or_else(|| crate::i18n::tr("테마를 찾을 수 없습니다."))?;
    let tpls: Vec<(&'static str, Value)> = SLOTS.iter().filter(|o| src["slots"][**o].is_object()).map(|o| (*o, src["slots"][*o].clone())).collect();
    let desc = src["desc"].as_str().unwrap_or("").to_string();
    insert_custom(c, name, &desc, tpls)
}

/// 내 테마 이름·설명 바꾸기 (기본 테마는 안 됨). 템플릿 이름도 같이 맞춘다. 실제로 쓰인 이름을 돌려준다.
pub fn rename_theme(c: &mut Core, id: &str, name: &str, desc: &str) -> Result<String, String> {
    if is_builtin(id) {
        return Err(crate::i18n::tr("기본 테마는 이름을 바꿀 수 없습니다. 복제해서 쓰세요."));
    }
    let s = c.settings.get();
    let rec = custom_record(s, id).ok_or_else(|| crate::i18n::tr("테마를 찾을 수 없습니다."))?.clone();
    let name = clean_text(name, NAME_MAX);
    if name.is_empty() {
        return Err(crate::i18n::tr("테마 이름을 입력하세요."));
    }
    let name = unique_name(s, &name, id);
    let desc = clean_text(desc, DESC_MAX);
    let (n2, id2) = (name.clone(), id.to_string());
    c.settings.update(|d| {
        if let Some(r) = d["customThemes"].as_array_mut().and_then(|a| a.iter_mut().find(|t| t["id"] == id2.as_str())) {
            r["name"] = json!(n2);
            r["desc"] = json!(desc);
            r["updatedAt"] = json!(crate::util::iso_now());
        }
        if let Some(list) = d["templates"].as_array_mut() {
            for o in SLOTS {
                // 이 테마가 만든 템플릿만 (theme 표시가 같은 것)
                if let Some(t) = rec["slots"][o].as_str().and_then(|tid| list.iter_mut().find(|t| t["id"] == tid && t["theme"] == id2.as_str())) {
                    t["name"] = json!(format!("{n2} · {}", slot_label(o)));
                }
            }
        }
    });
    c.emit(Ev::Ui);
    Ok(name)
}

/// 내 테마 삭제 (기본 테마는 안 됨). **테마 묶음 기록만** 지운다: 그 디자인(템플릿)은 지금 출력이나
/// 큐시트 순서가 쓰고 있을 수 있으므로 '출력 템플릿' 목록에 그대로 남긴다 (거기서 따로 지울 수 있다).
/// 남은 템플릿 수를 돌려준다.
pub fn delete_theme(c: &mut Core, id: &str) -> Result<usize, String> {
    if is_builtin(id) {
        return Err(crate::i18n::tr("기본 테마는 지울 수 없습니다."));
    }
    let s = c.settings.get();
    let rec = custom_record(s, id).ok_or_else(|| crate::i18n::tr("테마를 찾을 수 없습니다."))?;
    let kept = SLOTS.iter().filter(|o| rec["slots"][**o].as_str().is_some_and(|tid| stored(s, tid).is_some())).count();
    let id2 = id.to_string();
    c.settings.update(|d| {
        if let Some(a) = d["customThemes"].as_array_mut() {
            a.retain(|t| t["id"] != id2.as_str());
        }
    });
    c.emit(Ev::Ui);
    Ok(kept)
}

/// 템플릿이 가리키는 그림·영상·로고 파일 (경로만. 파일 내용은 내보내지 않는다)
pub fn linked_files(tpl: &Value) -> Vec<String> {
    [&tpl["background"]["image"], &tpl["background"]["video"], &tpl["logo"]["image"]].iter().filter_map(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect()
}

fn dedup(mut v: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    v.retain(|x| seen.insert(x.clone()));
    v
}

/// 테마 하나를 내보내기 JSON 글로. 테마 이름·설명과 그 템플릿만 담는다 (다른 설정은 전혀 넣지 않는다).
pub fn export_theme(s: &Value, id: &str) -> Result<String, String> {
    let all = list_all(s);
    let t = all.iter().find(|t| t["id"] == id).ok_or_else(|| crate::i18n::tr("테마를 찾을 수 없습니다."))?;
    let mut slots = json!({});
    let mut files = vec![];
    for o in SLOTS {
        if t["slots"][o].is_object() {
            // 템플릿 기본 칸만 골라 담는다 (템플릿에 붙어 있을 수 있는 모르는 칸은 내보내지 않는다)
            let tpl = sanitize_template(&t["slots"][o]);
            files.extend(linked_files(&tpl));
            slots[o] = tpl;
        }
    }
    let doc = json!({
        "format": EXPORT_FORMAT,
        "version": EXPORT_VERSION,
        "exportedAt": crate::util::iso_now(),
        "theme": { "name": t["name"], "desc": t["desc"], "slots": slots },
        "linkedFiles": dedup(files),
        "note": "그림·영상·로고는 파일 경로만 들어 있습니다. 파일 자체는 들어 있지 않습니다.",
    });
    serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())
}

/// 가져온 결과
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Imported {
    pub id: String,
    pub name: String,
    pub slots: Vec<String>,
    /// 테마가 가리키는 그림·영상·로고 경로 (이 PC 에 있는지는 화면에서 확인)
    pub linked_files: Vec<String>,
}

/// 가져오기 글을 검사해 (이름, 설명, 칸별 템플릿) 으로. 설정을 보지도 고치지도 않는다.
pub fn parse_import(text: &str) -> Result<(String, String, Vec<(&'static str, Value)>), String> {
    use crate::i18n::{tr, tr_args};
    if text.len() > IMPORT_MAX_BYTES {
        return Err(tr_args("파일이 너무 큽니다 (최대 {1}KB). 테마 파일이 맞는지 확인하세요.", &[&(IMPORT_MAX_BYTES / 1024)]));
    }
    let v: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|_| tr("테마 파일을 읽을 수 없습니다 (JSON 형식이 아닙니다)."))?;
    if v["format"] != EXPORT_FORMAT {
        return Err(tr("워십프리젠터 테마 파일이 아닙니다."));
    }
    match v["version"].as_u64() {
        Some(n) if (1..=EXPORT_VERSION).contains(&n) => {}
        Some(n) => return Err(tr_args("이 프로그램보다 새 버전(버전 {1})의 테마 파일입니다. 프로그램을 업데이트한 뒤 다시 가져오세요.", &[&n])),
        None => return Err(tr("테마 파일의 버전을 알 수 없습니다.")),
    }
    let th = &v["theme"];
    let name = clean_text(th["name"].as_str().unwrap_or(""), NAME_MAX);
    if name.is_empty() {
        return Err(tr("테마 파일에 이름이 없습니다."));
    }
    let desc = clean_text(th["desc"].as_str().unwrap_or(""), DESC_MAX);
    if !th["slots"].is_object() {
        return Err(tr("테마 파일에 출력 디자인이 없습니다."));
    }
    let mut tpls = vec![];
    for o in SLOTS {
        let t = &th["slots"][o];
        if t.is_null() {
            continue;
        }
        if !t.is_object() {
            return Err(tr_args("테마 파일의 '{1}' 디자인이 올바르지 않습니다.", &[&tr(slot_label(o))]));
        }
        tpls.push((o, sanitize_template(t)));
    }
    if tpls.is_empty() {
        return Err(tr("테마 파일에 출력 디자인이 없습니다."));
    }
    Ok((name, desc, tpls))
}

/// 템플릿을 기본 칸 모양에 맞춘다: 기본 템플릿에 있는 칸만, 같은 종류의 값만 받는다 (모르는 칸·다른 모양은 버림).
fn sanitize_template(t: &Value) -> Value {
    fn pick(base: &Value, src: &Value) -> Value {
        match (base, src) {
            (Value::Object(b), Value::Object(s)) => Value::Object(b.iter().map(|(k, bv)| (k.clone(), s.get(k).map(|sv| pick(bv, sv)).unwrap_or_else(|| bv.clone()))).collect()),
            (Value::Number(_), Value::Number(n)) if n.as_f64().is_some_and(f64::is_finite) => src.clone(),
            (Value::String(_), Value::String(s)) if s.len() <= 2000 => src.clone(),
            (Value::Bool(_), Value::Bool(_)) => src.clone(),
            _ => base.clone(),
        }
    }
    let mut out = crate::settings::normalize_template(&pick(&crate::settings::TEMPLATE_BASE, t));
    use crate::templates::clamp_int;
    out["width"] = json!(clamp_int(&out["width"], 320, 7680, 1920));
    out["height"] = json!(clamp_int(&out["height"], 180, 4320, 1080));
    let size = clamp_int(&out["font"]["size"], 8, 400, 72);
    out["font"]["size"] = json!(size);
    out["font"]["minSize"] = json!(clamp_int(&out["font"]["minSize"], 8, size, 44.min(size)));
    out
}

/// 테마 파일 글을 내 테마로 가져온다. 늘 새 id 로 넣으므로 기본 테마·이미 있는 테마·지금 출력을 덮어쓰지 않는다.
/// 검사에 실패하면 설정은 한 글자도 바뀌지 않는다.
pub fn import_theme(c: &mut Core, text: &str) -> Result<Imported, String> {
    let (name, desc, tpls) = parse_import(text)?;
    let slots: Vec<String> = tpls.iter().map(|t| t.0.to_string()).collect();
    let files = dedup(tpls.iter().flat_map(|t| linked_files(&t.1)).collect());
    let id = insert_custom(c, &name, &desc, tpls)?;
    let name = theme_name(c.settings.get(), &id);
    Ok(Imported { id, name, slots, linked_files: files })
}

/// OBS 자막 보기: full = '슬라이드 그대로' (예배당 템플릿으로), pip = 그림 슬라이드 우상단
pub fn set_obs_view(c: &mut Core, full: Option<bool>, pip: Option<bool>) -> Value {
    if let Some(f) = full {
        c.obs_full = f;
    }
    if let Some(p) = pip {
        c.settings.update(|d| d["output"]["obsPip"] = json!(p));
    }
    c.emit(Ev::Live);
    c.emit(Ev::Ui);
    obs_view(c)
}

pub fn obs_view(c: &Core) -> Value {
    json!({ "full": c.obs_full, "pip": c.settings.get()["output"]["obsPip"].as_bool().unwrap_or(false) })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn presets() {
        assert_eq!(THEMES.len(), 10);
        let r = &THEMES[0];
        assert_eq!(r["main"]["id"], json!("theme-sa-redshield-main"));
        assert_eq!(r["overlay"]["background"]["type"], json!("transparent"));
        assert!(THEMES.iter().all(|t| t["main"]["width"].is_number() && t["name"].is_string()));
    }
}

#[cfg(test)]
mod manage_tests {
    use super::*;

    fn core() -> (Core, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("wp-themes-{}-{}", std::process::id(), crate::util::short_id("t")));
        std::fs::create_dir_all(&dir).unwrap();
        drop(crate::db::BibleStore::open(&dir.join("bible.db")).unwrap());
        (Core::open(dir.clone(), dir.clone(), None).unwrap(), dir)
    }
    fn tpl(c: &Core, id: &str) -> Value {
        c.settings.template(id).unwrap_or(Value::Null)
    }
    fn edit(c: &mut Core, id: &str, f: impl FnOnce(&mut Value)) {
        c.settings.update(|d| f(d["templates"].as_array_mut().unwrap().iter_mut().find(|t| t["id"] == id).unwrap()));
    }
    const RM: &str = "theme-sa-redshield-main";

    #[test]
    fn keep_apply_preserves_edits_and_reset_is_explicit() {
        let (mut c, dir) = core();
        let other = THEMES[1]["id"].as_str().unwrap().to_string();
        apply_theme_ex(&mut c, "sa-redshield", &["main", "overlay"], ApplyMode::Keep).unwrap();
        // 사용자가 템플릿을 고쳤다
        edit(&mut c, RM, |t| t["font"]["size"] = json!(99));
        apply_theme_ex(&mut c, &other, &["main"], ApplyMode::Keep).unwrap();
        let (outs, _) = apply_theme_ex(&mut c, "sa-redshield", &["main"], ApplyMode::Keep).unwrap();
        assert_eq!(outs, vec!["main"]);
        assert_eq!(tpl(&c, RM)["font"]["size"], json!(99), "일반 적용은 고친 디자인을 지우지 않는다");
        let all = list_all(c.settings.get());
        assert_eq!(all.iter().find(|t| t["id"] == "sa-redshield").unwrap()["modified"], json!(["main"]));
        // 처음 디자인으로: 명시적으로 불러야만
        let before_active = c.settings.get()["activeTemplates"].clone();
        let u = restore_original(&mut c, "sa-redshield", &["main", "overlay"]).unwrap();
        assert_ne!(tpl(&c, RM)["font"]["size"], json!(99));
        assert_eq!(c.settings.get()["activeTemplates"], before_active, "원복은 출력 지정을 바꾸지 않는다");
        assert!(restore_original(&mut c, "sa-redshield", &["main", "overlay"]).is_err(), "바뀐 것이 없으면 할 일 없음");
        undo_apply(&mut c, &u).unwrap();
        assert_eq!(tpl(&c, RM)["font"]["size"], json!(99), "원복도 되돌릴 수 있다");
        // 예전 계약: apply_theme 은 원본처럼 main+overlay 를 덮어쓰고 찬송가 칸은 건드리지 않는다
        let ly = c.settings.get()["activeTemplates"]["overlayLyrics"].clone();
        assert_eq!(apply_theme(&mut c, "sa-redshield", &[]).unwrap(), vec!["main", "overlay"]);
        assert_ne!(tpl(&c, RM)["font"]["size"], json!(99));
        assert_eq!(c.settings.get()["activeTemplates"]["overlayLyrics"], ly);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn undo_restores_only_touched_slots_and_templates() {
        let (mut c, dir) = core();
        let s0 = c.settings.get().clone();
        let (_, u) = apply_theme_ex(&mut c, "sa-redshield", &["overlay"], ApplyMode::Keep).unwrap();
        assert_eq!(c.settings.get()["activeTemplates"]["main"], s0["activeTemplates"]["main"], "고르지 않은 칸은 그대로");
        assert_eq!(c.settings.get()["activeTemplates"]["overlayLyrics"], s0["activeTemplates"]["overlayLyrics"]);
        assert_eq!(c.settings.get()["activeTemplates"]["overlay"], json!("theme-sa-redshield-overlay"));
        // 적용 뒤 다른 설정을 바꿨다 — 되돌리기가 이것을 덮으면 안 된다
        c.settings.update(|d| d["uiLang"] = json!("en"));
        undo_apply(&mut c, &u).unwrap();
        let mut want = s0.clone();
        want["uiLang"] = json!("en");
        assert_eq!(c.settings.get(), &want, "출력 칸과 새로 넣은 템플릿만 되돌린다");
        // Reset 적용의 되돌리기는 덮어쓴 템플릿도 되살린다
        apply_theme_ex(&mut c, "sa-redshield", &["main"], ApplyMode::Keep).unwrap();
        edit(&mut c, RM, |t| t["align"] = json!("left"));
        let (_, u2) = apply_theme_ex(&mut c, "sa-redshield", &["main"], ApplyMode::Reset).unwrap();
        assert_ne!(tpl(&c, RM)["align"], json!("left"));
        undo_apply(&mut c, &u2).unwrap();
        assert_eq!(tpl(&c, RM)["align"], json!("left"));
        // 기본 테마에는 찬송가 칸이 없다: 그 칸만 고르면 실패하고 설정 불변, 셋 다 고르면 둘만 바뀐다
        let snap = c.settings.get().clone();
        assert!(apply_theme_ex(&mut c, "sa-redshield", &["overlayLyrics"], ApplyMode::Keep).is_err());
        assert!(apply_theme_ex(&mut c, "없는 테마", &SLOTS, ApplyMode::Keep).is_err());
        assert_eq!(c.settings.get(), &snap);
        let (outs, _) = apply_theme_ex(&mut c, "sa-redshield", &SLOTS, ApplyMode::Keep).unwrap();
        assert_eq!(outs, vec!["main", "overlay"]);
        assert_eq!(c.settings.get()["activeTemplates"]["overlayLyrics"], snap["activeTemplates"]["overlayLyrics"]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn undo_after_intervening_design_edit_keeps_the_edit_and_all_settings() {
        let (mut c, dir) = core();
        let (_, undo) = apply_theme_ex(&mut c, "sa-redshield", &["main"], ApplyMode::Keep).unwrap();
        edit(&mut c, RM, |t| t["font"]["size"] = json!(99));
        let edited = c.settings.get().clone();
        assert!(undo_apply(&mut c, &undo).is_err());
        assert_eq!(c.settings.get(), &edited, "적용 뒤 고친 새 템플릿을 지우면 안 됨");
        let (_, reset) = apply_theme_ex(&mut c, "sa-redshield", &["main"], ApplyMode::Reset).unwrap();
        edit(&mut c, RM, |t| t["font"]["size"] = json!(88));
        let edited = c.settings.get().clone();
        assert!(undo_apply(&mut c, &reset).is_err());
        assert_eq!(c.settings.get(), &edited, "원복 뒤 다시 고친 것을 덮으면 안 됨");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn custom_theme_lifecycle() {
        let (mut c, dir) = core();
        c.settings.update(|d| d["futureKey"] = json!({ "x": 1 }));
        let active0 = c.settings.get()["activeTemplates"].clone();
        let n0 = c.settings.templates().len();
        let id = save_current_as(&mut c, "  주일 예배  ", "설명").unwrap();
        assert_eq!(c.settings.get()["activeTemplates"], active0, "저장은 출력을 바꾸지 않는다");
        let all = list_all(c.settings.get());
        let t = all.iter().find(|t| t["id"] == id.as_str()).unwrap().clone();
        assert_eq!(t["name"], json!("주일 예배"));
        assert_eq!(t["builtin"], json!(false));
        let n_slots = t["slots"].as_object().unwrap().len();
        assert!(n_slots >= 2);
        assert_eq!(c.settings.templates().len(), n0 + n_slots, "새 id 의 템플릿으로 복사");
        // 원본 템플릿과 독립
        let src_id = active0["main"].as_str().unwrap().to_string();
        let src0 = tpl(&c, &src_id);
        let my_main = t["slots"]["main"]["id"].as_str().unwrap().to_string();
        assert_ne!(my_main, src_id);
        edit(&mut c, &my_main, |t| t["font"]["size"] = json!(55));
        assert_eq!(tpl(&c, &src_id), src0);
        // 같은 이름 → (2), 기본 테마 복제
        let id2 = save_current_as(&mut c, "주일 예배", "").unwrap();
        assert_ne!(id, id2);
        assert_eq!(theme_name(c.settings.get(), &id2), "주일 예배 (2)");
        let id3 = duplicate_theme(&mut c, "sa-redshield", "빨강 고친 것").unwrap();
        assert!(c.settings.template(RM).is_none(), "복제는 기본 테마 템플릿을 설정에 넣지 않는다");
        // 이름 바꾸기 · 기본 테마 보호
        assert_eq!(rename_theme(&mut c, &id3, "새 이름", "새 설명").unwrap(), "새 이름");
        assert!(tpl(&c, &tpl_id(&id3, "main"))["name"].as_str().unwrap().starts_with("새 이름"));
        assert!(rename_theme(&mut c, "sa-redshield", "x", "").is_err());
        assert!(delete_theme(&mut c, "sa-redshield").is_err());
        assert!(rename_theme(&mut c, &id3, "   ", "").is_err());
        // 내 테마 적용 · 순서별 테마 · 되돌리기
        let (outs, u) = apply_theme_ex(&mut c, &id, &SLOTS, ApplyMode::Keep).unwrap();
        assert_eq!(outs.len(), n_slots);
        assert_eq!(c.settings.get()["activeTemplates"]["main"], json!(my_main));
        assert_eq!(used_slots(c.settings.get(), &list_all(c.settings.get()).into_iter().find(|t| t["id"] == id.as_str()).unwrap()).len(), n_slots);
        assert_eq!(ensure_theme(&mut c, &id).unwrap()["main"], json!(my_main), "순서별 테마로도 쓸 수 있다");
        // 쓰는 중에 지워도 출력이 깨지지 않는다: 묶음 기록만 지우고 템플릿은 남긴다
        assert_eq!(delete_theme(&mut c, &id).unwrap(), n_slots);
        assert!(c.settings.template(&my_main).is_some(), "쓰고 있을 수 있는 템플릿은 지우지 않는다");
        assert!(list_all(c.settings.get()).iter().all(|t| t["id"] != id.as_str()));
        undo_apply(&mut c, &u).unwrap();
        assert_eq!(c.settings.get()["activeTemplates"], active0);
        assert_eq!(tpl(&c, &src_id), src0, "삭제가 다른 템플릿을 건드리지 않는다");
        assert_eq!(c.settings.get()["futureKey"], json!({ "x": 1 }), "모르는 설정 칸 보존");
        // 저장했다 다시 읽어도 customThemes 가 남는다
        c.settings.save_now().unwrap();
        let re = crate::settings::Settings::load(&dir);
        assert_eq!(re.get()["customThemes"].as_array().unwrap().len(), 2);
        assert_eq!(re.get()["futureKey"], json!({ "x": 1 }));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn export_has_no_secrets_and_import_is_safe() {
        let (mut c, dir) = core();
        c.settings.update(|d| {
            d["ai"] = json!({ "apiKey": "SECRET-API-KEY-123" });
            d["obsPasswordTest"] = json!("SECRET-OBS-PW");
            d["remotePinTest"] = json!("SECRET-PIN-9");
            d["history"] = json!([{ "title": "SECRET-HISTORY" }]);
            let a = d["activeTemplates"]["main"].clone();
            let t = d["templates"].as_array_mut().unwrap().iter_mut().find(|t| t["id"] == a).unwrap();
            t["background"]["image"] = json!("C:\\그림\\배경.png");
            t["logo"]["image"] = json!("C:\\그림\\로고.png");
            t["privateNote"] = json!("SECRET-IN-TEMPLATE");
        });
        let id = save_current_as(&mut c, "내보낼 테마", "설명").unwrap();
        let txt = export_theme(c.settings.get(), &id).unwrap();
        assert!(!txt.contains("SECRET"), "비밀·기록·모르는 칸이 섞이면 안 된다");
        let v: Value = serde_json::from_str(&txt).unwrap();
        assert_eq!(v["format"], json!(EXPORT_FORMAT));
        assert_eq!(v["version"], json!(1));
        let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(|k| k.as_str()).collect();
        keys.sort();
        assert_eq!(keys, ["exportedAt", "format", "linkedFiles", "note", "theme", "version"], "정해진 칸만");
        assert_eq!(v["linkedFiles"], json!(["C:\\그림\\배경.png", "C:\\그림\\로고.png"]));
        assert!(v["theme"]["slots"]["main"].is_object() && v["theme"]["slots"]["overlay"].is_object());
        assert!(export_theme(c.settings.get(), "sa-redshield").is_ok(), "기본 테마도 내보낼 수 있다");
        // 가져오기: 같은 이름·id 가 이미 있어도 새 id
        let before = c.settings.get().clone();
        let im = import_theme(&mut c, &txt).unwrap();
        assert_ne!(im.id, id);
        assert_eq!(im.name, "내보낼 테마 (2)");
        assert_eq!(im.linked_files.len(), 2);
        let after = c.settings.get().clone();
        assert_eq!(after["activeTemplates"], before["activeTemplates"], "가져오기는 출력을 바꾸지 않는다");
        for t in before["templates"].as_array().unwrap() {
            assert!(after["templates"].as_array().unwrap().contains(t), "있던 템플릿은 그대로");
        }
        assert_eq!(after["customThemes"].as_array().unwrap().len(), 2);
        assert_eq!(tpl(&c, &tpl_id(&im.id, "main"))["background"]["image"], json!("C:\\그림\\배경.png"));
        // 기본 테마 id 를 흉내 낸 파일도 기본 테마를 덮지 않는다
        let fake = json!({ "format": EXPORT_FORMAT, "version": 1, "theme": { "id": "sa-redshield", "name": "가짜", "slots": { "main": { "id": "theme-sa-redshield-main", "font": { "size": 12 }, "evil": { "apiKey": "x" }, "padding": "깨짐", "width": 999999 } } } }).to_string();
        let im2 = import_theme(&mut c, &fake).unwrap();
        assert!(!is_builtin(&im2.id));
        let t = tpl(&c, &tpl_id(&im2.id, "main"));
        assert!(t["evil"].is_null(), "모르는 칸은 버린다");
        assert!(t["padding"].is_object() && t["width"] == json!(7680));
        assert_eq!(t["font"]["size"], json!(12));
        assert!(c.settings.template("theme-sa-redshield-main").is_none());
        // 실패하면 설정 불변
        let snap = c.settings.get().clone();
        let big = format!("{{\"format\":\"{EXPORT_FORMAT}\",\"version\":1,\"pad\":\"{}\"}}", "a".repeat(IMPORT_MAX_BYTES));
        for bad in [
            "아무 글".to_string(),
            "[1,2]".to_string(),
            json!({ "format": "other", "version": 1 }).to_string(),
            json!({ "format": EXPORT_FORMAT, "version": 99, "theme": { "name": "a", "slots": { "main": {} } } }).to_string(),
            json!({ "format": EXPORT_FORMAT, "theme": { "name": "a", "slots": { "main": {} } } }).to_string(),
            json!({ "format": EXPORT_FORMAT, "version": 1, "theme": { "name": "  ", "slots": { "main": {} } } }).to_string(),
            json!({ "format": EXPORT_FORMAT, "version": 1, "theme": { "name": "a", "slots": {} } }).to_string(),
            json!({ "format": EXPORT_FORMAT, "version": 1, "theme": { "name": "a", "slots": { "main": "글" } } }).to_string(),
            json!({ "format": EXPORT_FORMAT, "version": 1, "theme": { "name": "a" } }).to_string(),
            big,
        ] {
            assert!(import_theme(&mut c, &bad).is_err(), "받으면 안 됨: {}", bad.chars().take(60).collect::<String>());
            assert_eq!(c.settings.get(), &snap, "실패한 가져오기는 설정을 바꾸지 않는다");
        }
        let _ = std::fs::remove_dir_all(dir);
    }
}
