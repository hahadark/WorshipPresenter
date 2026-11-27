//! 소스 · 화면 구성 (로드맵 1단계, docs/roadmap-sources.md)
//!
//! - **소스**: 화면에 나올 수 있는 것 하나. 문자열 식별자로 부른다.
//!   내장 소스(지금 있는 기능에 이름만 붙인 것: `background` · `media` · `slide-bg` · `live` · `overlay:<id>` · `message` ·
//!   `logo-screen` · `draw` · `pip`, 카메라 `camera:<deviceId>`)는 저장하지 않고 그때그때 계산한다.
//!   사용자 소스(웹 페이지 · 화면 캡처)는 설정 `sources[]` 에 덧붙인다 (옛 판은 모르는 키를 그대로 둔다).
//! - **화면 구성(scene)**: 소스를 아래에서 위로 쌓은 칸 목록. 1단계에서는 기본 구성 `full` 하나 (지금 출력과 같은 차례).
//!   2단계에서 칸마다 rect · crop · opacity, 3단계에서 출력마다 다른 구성을 쓴다 — 칸의 모양은 미리 그렇게 잡았다.
use crate::core::{Core, Ev};
use anyhow::{bail, Result};
use serde_json::{json, Value};

/// 사용자 소스 수 상한
pub const MAX_SOURCES: usize = 40;
/// 주소 길이 상한
pub const MAX_URL: usize = 2000;

/// 사용자 소스 종류: 웹 페이지.
/// 화면 캡처(capture)는 예배당 창(WebView2)이 허락 창 없이 열 방법이 없어 넣지 않았다 (docs/roadmap-sources.md (마)).
/// 그 종류의 항목이 설정에 있어도(새 판에서 만든 것) 지우지 않고 목록에서만 건너뛴다.
pub const USER_KINDS: [&str; 1] = ["web"];

/// 출력 이름 (예배당 · OBS 자막)
pub const OUTPUTS: [&str; 2] = ["main", "overlay"];

/// 내장 소스 (식별자, 종류, 이름). 기본 구성에서 쌓는 차례와 같다 — 오버레이 · 사용자 소스는 그 사이에 끼운다.
pub const BUILTIN: [(&str, &str, &str); 9] = [
    ("background", "background", "템플릿 배경"),
    ("media", "media", "배경 미디어"),
    ("slide-bg", "slide-bg", "순서 배경 영상"),
    ("live", "live", "송출 중인 순서"),
    ("message", "message", "회중 메시지"),
    ("logo-screen", "logo-screen", "로고 화면"),
    ("draw", "draw", "그리기"),
    ("pip", "pip", "그림 우상단 (OBS 자막)"),
    ("camera", "camera", "카메라"),
];

/// http · https 주소만 (javascript: · file: · data: 는 거절)
pub fn check_url(url: &str) -> Result<String> {
    let u = url.trim();
    if u.is_empty() {
        bail!("{}", crate::i18n::tr("주소를 넣어 주세요."));
    }
    if u.len() > MAX_URL || u.chars().any(|c| c.is_control() || c == ' ') {
        bail!("{}", crate::i18n::tr("주소 모양이 맞지 않습니다."));
    }
    let lower = u.to_ascii_lowercase();
    let with_scheme = if lower.starts_with("http://") || lower.starts_with("https://") {
        u.to_string()
    } else if lower.contains("://") || lower.starts_with("javascript:") || lower.starts_with("data:") || lower.starts_with("file:") {
        bail!("{}", crate::i18n::tr("http 나 https 로 시작하는 웹 주소만 넣을 수 있습니다."));
    } else {
        format!("https://{u}")
    };
    let host = with_scheme.split("://").nth(1).unwrap_or("").split(['/', '?', '#']).next().unwrap_or("");
    if host.is_empty() || host.starts_with(':') || host.contains('@') {
        bail!("{}", crate::i18n::tr("주소 모양이 맞지 않습니다."));
    }
    Ok(with_scheme)
}

fn rand_id() -> String {
    format!("src-{:x}", crate::util::now_ms() ^ (std::process::id() as i64).rotate_left(20) ^ rand_salt())
}

fn rand_salt() -> i64 {
    use std::sync::atomic::{AtomicI64, Ordering};
    static N: AtomicI64 = AtomicI64::new(0);
    N.fetch_add(0x9E37, Ordering::Relaxed)
}

/// 사용자 소스 하나 정리 (모르는 칸은 그대로 둔다 — 새 판이 덧붙인 칸을 옛 판이 지우지 않게, 그 반대도)
pub fn normalize_source(v: &Value) -> Result<Value> {
    let kind = v["kind"].as_str().unwrap_or("");
    if !USER_KINDS.contains(&kind) {
        bail!("{}", crate::i18n::tr("모르는 소스 종류입니다."));
    }
    let mut o = v.as_object().cloned().unwrap_or_default();
    let id = v["id"].as_str().filter(|x| !x.is_empty() && x.len() <= 64 && x.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')).map(String::from).unwrap_or_else(rand_id);
    o.insert("id".into(), json!(id));
    o.insert("kind".into(), json!(kind));
    let name: String = v["name"].as_str().unwrap_or("").chars().filter(|c| !c.is_control()).take(60).collect::<String>().trim().to_string();
    let name = if name.is_empty() { crate::i18n::tr(if kind == "web" { "웹 페이지" } else { "화면 캡처" }) } else { name };
    o.insert("name".into(), json!(name));
    if kind == "web" {
        o.insert("url".into(), json!(check_url(v["url"].as_str().unwrap_or(""))?));
    } else {
        let t = &v["target"];
        let ty = t["type"].as_str().filter(|x| *x == "screen" || *x == "window").unwrap_or("screen");
        let tn: String = t["name"].as_str().unwrap_or("").chars().filter(|c| !c.is_control()).take(200).collect();
        o.insert("target".into(), json!({ "type": ty, "name": tn }));
    }
    let outs: Vec<Value> = v["outputs"].as_array().map(|a| a.iter().filter(|x| x.as_str().is_some_and(|s| OUTPUTS.contains(&s))).cloned().collect()).unwrap_or_else(|| vec![json!("main")]);
    let mut uniq: Vec<Value> = vec![];
    for x in outs {
        if !uniq.contains(&x) {
            uniq.push(x);
        }
    }
    o.insert("outputs".into(), json!(uniq));
    o.insert("on".into(), json!(v["on"].as_bool().unwrap_or(false)));
    o.insert("place".into(), json!(if v["place"] == "over" { "over" } else { "under" }));
    o.insert("opacity".into(), json!(v["opacity"].as_f64().filter(|x| x.is_finite()).unwrap_or(1.0).clamp(0.0, 1.0)));
    o.insert("fit".into(), json!(if v["fit"] == "contain" { "contain" } else { "cover" }));
    Ok(Value::Object(o))
}

/// 설정의 사용자 소스 (깨진 것은 건너뜀)
pub fn user_sources(settings: &Value) -> Vec<Value> {
    settings["sources"].as_array().into_iter().flatten().filter_map(|x| normalize_source(x).ok().filter(|n| n["id"] == x["id"])).collect()
}

pub fn save_source(settings: &mut Value, v: &Value) -> Result<Value> {
    let n = normalize_source(v)?;
    if !settings["sources"].is_array() {
        settings["sources"] = json!([]);
    }
    let list = settings["sources"].as_array_mut().unwrap();
    match list.iter().position(|x| x["id"] == n["id"]) {
        Some(i) => list[i] = n.clone(),
        None => {
            if list.len() >= MAX_SOURCES {
                bail!("{}", crate::i18n::tr("소스를 더 만들 수 없습니다. 쓰지 않는 소스를 지워 주세요."));
            }
            list.push(n.clone())
        }
    }
    Ok(n)
}

pub fn delete_source(settings: &mut Value, id: &str) {
    if let Some(l) = settings["sources"].as_array_mut() {
        l.retain(|x| x["id"] != id);
    }
}

/// 켜기 · 끄기 (None 이면 뒤집기)
pub fn toggle_source(settings: &mut Value, id: &str, on: Option<bool>) -> Result<Value> {
    let Some(o) = settings["sources"].as_array_mut().and_then(|l| l.iter_mut().find(|x| x["id"] == id)) else { bail!("{}", crate::i18n::tr("소스를 찾을 수 없습니다.")) };
    let next = on.unwrap_or(!o["on"].as_bool().unwrap_or(false));
    o["on"] = json!(next);
    Ok(o.clone())
}

/// 이 출력에 켜 둔 사용자 소스
fn user_on<'a>(list: &'a [Value], output: &'a str, place: &'a str) -> impl Iterator<Item = &'a Value> + 'a {
    list.iter().filter(move |s| s["on"] == true && s["place"] == place && s["outputs"].as_array().is_some_and(|a| a.iter().any(|x| x == output)))
}

/// 기본 구성 `full`: 1단계 전 output.js 가 손으로 끼우던 차례 그대로.
/// background → media → (사용자 소스 under) → slide-bg → live → (사용자 소스 over) → overlay:<id>… → message → logo-screen → draw (→ pip, OBS 자막만)
pub fn default_scene(output: &str, overlays: &[Value], user: &[Value]) -> Value {
    let l = |id: &str| json!({ "id": id, "source": id });
    let mut layers = vec![l("background"), l("media")];
    layers.extend(user_on(user, output, "under").map(|s| l(s["id"].as_str().unwrap_or(""))));
    layers.push(l("slide-bg"));
    layers.push(l("live"));
    layers.extend(user_on(user, output, "over").map(|s| l(s["id"].as_str().unwrap_or(""))));
    for o in overlays {
        let id = o["id"].as_str().unwrap_or("");
        if id == "wpmessage" {
            continue; // 회중 메시지는 자기 칸 (아래)
        }
        layers.push(json!({ "id": format!("overlay:{id}"), "source": format!("overlay:{id}") }));
    }
    layers.push(l("message"));
    layers.push(l("logo-screen"));
    layers.push(l("draw"));
    if output == "overlay" {
        layers.push(l("pip"));
    }
    json!({ "id": "full", "name": "전체 화면 (기본)", "layers": layers })
}

/// 출력 데이터의 `sources`: 그리는 데 설정이 필요한 사용자 소스만 (이 출력에 켠 것)
pub fn payload_sources(output: &str, user: &[Value]) -> Value {
    let mut m = serde_json::Map::new();
    for s in user.iter().filter(|s| s["on"] == true && s["outputs"].as_array().is_some_and(|a| a.iter().any(|x| x == output))) {
        let id = s["id"].as_str().unwrap_or("").to_string();
        let mut o = json!({ "kind": s["kind"], "name": s["name"], "opacity": s["opacity"], "fit": s["fit"] });
        if s["kind"] == "web" {
            o["url"] = s["url"].clone();
        } else {
            o["target"] = s["target"].clone();
        }
        m.insert(id, o);
    }
    Value::Object(m)
}

/// 출력 데이터의 `sources` (2단계): 켜 둔 것 + 이 출력이 쓰는 구성에 넣은 사용자 소스
pub fn payload_sources_for(output: &str, user: &[Value], scene_id: &str, settings: &Value) -> Value {
    let mut v = payload_sources(crate::outputs::kind_of(settings, output), user);
    if scene_id == crate::scenes::FULL {
        return v;
    }
    let Some(sc) = crate::scenes::find_scene(settings, scene_id) else { return v };
    let m = v.as_object_mut().unwrap();
    for id in crate::scenes::used_user_sources(&sc) {
        if m.contains_key(&id) {
            continue;
        }
        if let Some(s) = user.iter().find(|s| s["id"] == id.as_str()) {
            let mut o = json!({ "kind": s["kind"], "name": s["name"], "opacity": s["opacity"], "fit": s["fit"] });
            if s["kind"] == "web" {
                o["url"] = s["url"].clone();
            } else {
                o["target"] = s["target"].clone();
            }
            m.insert(id, o);
        }
    }
    v
}

/// 조작 화면 '소스' 목록의 한 줄
#[derive(Clone, Debug, PartialEq)]
pub struct SourceRow {
    pub id: String,
    pub kind: String,
    /// 화면에 보일 이름 (한국어 원문 — 조작 화면이 tr 로 바꿈)
    pub name: String,
    /// 덧붙임 설명 (지금 내용 등, 사용자 글이 들 수 있음 — 번역하지 않음)
    pub detail: String,
    /// 지금 화면에 나가고 있는 출력 (main · overlay)
    pub on_air: Vec<&'static str>,
    /// 사용자 소스 (지우기 · 켜기 가능)
    pub user: bool,
    /// 사용자 소스: 켜 둠
    pub enabled: bool,
}

fn kind_word(kind: &str) -> &'static str {
    match kind {
        "bible" => "성경",
        "song" | "hymn" => "가사",
        "notice" | "text" => "공지 글",
        "title" => "제목",
        "image" => "그림",
        "score" | "ppt" | "pdf" => "PDF · PPT",
        "video" => "영상",
        "youtube" => "유튜브",
        "camera" => "카메라",
        "blank" => "빈 화면",
        "countdown" => "타이머",
        _ => "",
    }
}

fn overlay_word(t: &str) -> &'static str {
    match t {
        "name" => "이름 자막",
        "clock" => "시계",
        "countdown" => "타이머",
        "logo" => "로고",
        _ => "글",
    }
}

impl Core {
    /// 화면이 붙어 있는 출력 (예배당 창 · OBS 브라우저 소스가 연결됨). 조작 화면은 여기에 든 출력만 빨강(송출 중)으로
    pub fn connected_outputs(&self) -> Vec<&'static str> {
        OUTPUTS.iter().copied().filter(|o| self.outputs.values().any(|c| c.client == *o)).collect()
    }

    /// 화면이 붙어 있는 출력 id (추가 출력 포함, 로드맵 3단계)
    pub fn connected_output_ids(&self) -> Vec<String> {
        crate::outputs::all_ids(self.settings.get()).into_iter().filter(|o| self.outputs.values().any(|c| &c.client == o)).collect()
    }

    /// 출력 목록 (id, 이름): 예배당 · 방송 (OBS) · 추가 출력
    pub fn output_list(&self) -> Vec<(String, String)> {
        crate::outputs::list(self.settings.get())
    }

    pub fn extra_outputs(&self) -> Vec<Value> {
        crate::outputs::extra_outputs(self.settings.get())
    }

    pub fn save_extra_output(&mut self, v: &Value) -> Result<Value> {
        let mut out = Ok(Value::Null);
        self.settings.update(|d| out = crate::outputs::save_extra(d, v));
        self.emit(Ev::Live);
        self.emit(Ev::Ui);
        out
    }

    pub fn delete_extra_output(&mut self, id: &str) {
        self.settings.update(|d| crate::outputs::delete_extra(d, id));
        self.emit(Ev::Live);
        self.emit(Ev::Ui);
    }

    pub fn set_audio_output(&mut self, id: &str) -> Result<()> {
        let mut out = Ok(());
        self.settings.update(|d| out = crate::outputs::set_audio_output(d, id));
        self.emit(Ev::Live);
        self.emit(Ev::Ui);
        out
    }

    /// 순서 종류별 기본 구성 ("" = 지정 없음)
    pub fn set_type_scene(&mut self, ty: &str, output: &str, id: &str) -> Result<()> {
        let mut out = Ok(());
        self.settings.update(|d| out = crate::outputs::set_type_scene(d, ty, output, id));
        self.emit(Ev::Live);
        self.emit(Ev::Ui);
        out
    }

    /// 열린 큐시트 순서 i 에 출력별 구성 지정 ("" = 따라감, "full" = 전체 화면 못 박기). 실행 취소에 남는다
    pub fn set_item_scene(&mut self, i: usize, output: &str, id: &str) -> Result<()> {
        let s = self.settings.get();
        if !crate::outputs::is_output(s, output) {
            bail!("{}", crate::i18n::tr("모르는 출력입니다."));
        }
        if !(id.is_empty() || id == crate::scenes::FULL || crate::scenes::find_scene(s, id).is_some()) {
            bail!("{}", crate::i18n::tr("화면 구성을 찾을 수 없습니다."));
        }
        if self.cue.as_ref().and_then(|c| c.items.get(i)).is_none() {
            bail!("{}", crate::i18n::tr("순서를 찾을 수 없습니다."));
        }
        self.remember_items();
        if let Some(it) = self.cue.as_mut().and_then(|c| c.items.get_mut(i)) {
            crate::outputs::set_item_scene(it, output, id);
        }
        self.save_cue();
        self.emit(Ev::Live);
        self.emit(Ev::Ui);
        Ok(())
    }

    /// 출력의 지금 구성과 고른 곳 (LIVE 또는 미리보기 묶음)
    pub fn scene_choice(&self, output: &str, live: bool) -> (String, crate::outputs::From) {
        let deck = if live { self.pres.live.as_ref().map(|l| &l.deck) } else { self.pres.preview.as_ref() };
        self.scene_choice_for_deck(output, deck)
    }

    /// 이 출력의 화면 구성: 고른 구성(설정 outputScenes) → 없으면 기본 구성 `full` (2.0.8 과 같음)
    pub fn scene_for(&self, output: &str) -> Value {
        self.scene_for_deck(output, self.pres.live.as_ref().map(|l| &l.deck))
    }

    /// 미리보기처럼 LIVE 가 아닌 묶음의 구성 (칸의 글 모양 'main' · 'overlay' 를 그 묶음의 템플릿으로)
    pub fn scene_for_deck(&self, output: &str, deck: Option<&crate::presentation::Deck>) -> Value {
        let s = self.settings.get();
        let id = self.scene_id_for_deck(output, deck);
        let overlays = self.overlays_for(output);
        match crate::scenes::find_scene(s, &id) {
            Some(sc) => self.effective_scene_for(&sc, output, deck),
            // 추가 출력은 같은 종류 내장 출력의 기본 구성 (켜 둔 웹 페이지 · 그림 우상단 포함)
            None => default_scene(crate::outputs::kind_of(s, output), &overlays, &user_sources(s)),
        }
    }

    /// 이 출력이 지금 쓸 구성 id (로드맵 3단계): 순서의 지정 → 순서 종류 기본 → 출력 기본 → full.
    /// 송출 중인 순서가 열린 큐시트의 순서면 그 순서의 지금 값을 본다 (인스펙터에서 바꾸면 바로 반영)
    pub fn scene_id_for_deck(&self, output: &str, deck: Option<&crate::presentation::Deck>) -> String {
        self.scene_choice_for_deck(output, deck).0
    }

    pub fn scene_choice_for_deck(&self, output: &str, deck: Option<&crate::presentation::Deck>) -> (String, crate::outputs::From) {
        let s = self.settings.get();
        let p = deck.map(|d| {
            let p = &d.passage;
            let mut v = json!({ "itemType": p["itemType"], "scenes": p["scenes"] });
            let i = p["cueIndex"].as_i64().unwrap_or(-1);
            if i >= 0 {
                if let Some(it) = self.cue.as_ref().filter(|c| p["cueId"] == c.id.as_str()).and_then(|c| c.items.get(i as usize)).filter(|it| it["type"] == p["itemType"]) {
                    v["scenes"] = it["scenes"].clone();
                }
            }
            v
        });
        crate::outputs::resolve_scene(s, output, p.as_ref())
    }

    /// 출력 무대의 템플릿 (output_payload 의 template): 'OBS 전체' 면 방송 쪽도 예배당 템플릿
    pub fn stage_template_for_deck(&self, output: &str, deck: Option<&crate::presentation::Deck>) -> Value {
        let s = self.settings.get();
        let own_tpl = crate::outputs::find_extra(s, output).is_some_and(|o| o["template"].as_str().is_some_and(|t| !t.is_empty() && self.settings.template(t).is_some()));
        if self.obs_full && crate::outputs::kind_of(s, output) == "overlay" && !own_tpl {
            self.template_for_deck("main", deck)
        } else {
            self.template_for_deck(output, deck)
        }
    }

    /// 구성 하나(편집 중인 것 포함)를 이 출력에 보낼 모양으로: 맨 위 칸을 붙이고 칸 글 모양을 템플릿으로
    pub fn effective_scene_for(&self, sc: &Value, output: &str, deck: Option<&crate::presentation::Deck>) -> Value {
        let overlays = self.overlays_for(output);
        // 무대 템플릿: output_payload 의 template 과 같게 ('OBS 전체' 면 OBS 도 예배당 템플릿)
        let stage_id = self.stage_template_for_deck(output, deck)["id"].clone();
        let resolve = |t: &str| -> Option<Value> {
            let v = match t {
                "main" | "overlay" => self.template_for_deck(t, deck),
                id => self.settings.template(id)?,
            };
            // 무대 템플릿과 같으면 붙이지 않는다 → 웹 출력이 무대에 바로 그림 (기본 구성과 같은 그림)
            (!v.is_null() && v["id"] != stage_id).then_some(v)
        };
        crate::scenes::effective_scene(sc, crate::outputs::kind_of(self.settings.get(), output), &overlays, &resolve)
    }

    pub fn scenes(&self) -> Vec<Value> {
        crate::scenes::user_scenes(self.settings.get())
    }

    pub fn output_scene(&self, output: &str) -> String {
        crate::scenes::output_scene_id(self.settings.get(), output)
    }

    pub fn save_scene(&mut self, v: &Value) -> Result<Value> {
        let mut out = Ok(Value::Null);
        self.settings.update(|d| out = crate::scenes::save_scene(d, v));
        self.emit(Ev::Live);
        self.emit(Ev::Ui);
        out
    }

    pub fn delete_scene(&mut self, id: &str) {
        self.settings.update(|d| crate::scenes::delete_scene(d, id));
        self.emit(Ev::Live);
        self.emit(Ev::Ui);
    }

    pub fn duplicate_scene(&mut self, id: &str) -> Result<Value> {
        let mut out = Ok(Value::Null);
        self.settings.update(|d| out = crate::scenes::duplicate_scene(d, id));
        self.emit(Ev::Ui);
        out
    }

    /// 출력(main · overlay)에 구성 고르기 ("" · full = 기본)
    pub fn set_output_scene(&mut self, output: &str, id: &str) -> Result<()> {
        let mut out = Ok(());
        self.settings.update(|d| out = crate::scenes::set_output_scene(d, output, id));
        self.emit(Ev::Live);
        self.emit(Ev::Ui);
        out
    }

    pub fn user_sources(&self) -> Vec<Value> {
        user_sources(self.settings.get())
    }

    pub fn save_source(&mut self, v: &Value) -> Result<Value> {
        let mut out = Ok(Value::Null);
        self.settings.update(|d| out = save_source(d, v));
        self.emit(Ev::Live);
        self.emit(Ev::Ui);
        out
    }

    pub fn delete_source(&mut self, id: &str) {
        self.settings.update(|d| delete_source(d, id));
        self.emit(Ev::Live);
        self.emit(Ev::Ui);
    }

    pub fn toggle_source(&mut self, id: &str, on: Option<bool>) -> Result<Value> {
        let mut out = Ok(Value::Null);
        self.settings.update(|d| out = toggle_source(d, id, on));
        self.emit(Ev::Live);
        self.emit(Ev::Ui);
        out
    }

    /// 조작 화면 '소스' 목록: 무엇이 있고 지금 어디에 나가고 있는지
    pub fn sources_status(&self) -> Vec<SourceRow> {
        let s = self.settings.get();
        let live = self.pres.live.as_ref();
        let shown = live.is_some_and(|l| l.visible) && !self.obs_hold;
        let slide = live.and_then(|l| l.deck.slides.get(l.deck.index));
        let both: Vec<&'static str> = OUTPUTS.to_vec();
        let mut rows: Vec<SourceRow> = vec![];
        let row = |id: &str, kind: &str, name: &str, detail: String, on_air: Vec<&'static str>| SourceRow { id: id.into(), kind: kind.into(), name: name.into(), detail, on_air, user: false, enabled: false };
        // 송출 중인 순서 (성경 · 가사 · 공지 · 그림 · PDF · PPT · 영상 · 유튜브 · 카메라 슬라이드)
        let (live_detail, live_on) = match live {
            Some(l) => {
                let p = &l.deck.passage;
                let ty = slide.and_then(|x| x["kind"].as_str()).map(|k| match k {
                    "text" => p["itemType"].as_str().unwrap_or("text"),
                    "video" if crate::youtube::is_youtube(slide.and_then(|x| x["image"].as_str()).unwrap_or("")) => "youtube",
                    _ => k,
                }).unwrap_or("");
                let ty = if p["lyrics"] == true && ty == "text" { "song" } else { ty };
                let title = p["itemTitle"].as_str().filter(|x| !x.is_empty()).or(p["title"].as_str()).unwrap_or("");
                let w = kind_word(ty);
                (if w.is_empty() { title.to_string() } else { format!("{w} · {title}") }, if shown { both.clone() } else { vec![] })
            }
            None => (String::new(), vec![]),
        };
        rows.push(row("live", "live", "송출 중인 순서", live_detail, live_on));
        // 템플릿 배경 (+ 템플릿 로고)
        let tname = |o: &str| self.template_for(o)["name"].as_str().unwrap_or("").to_string();
        rows.push(row("background", "background", "템플릿 배경", format!("{} / OBS {}", tname("main"), tname("overlay")), if self.layers.hide_bg { vec![] } else { both.clone() }));
        // 배경 미디어 (예배당만)
        let media = self.layers.media.as_ref();
        let md = media.map(|m| {
            let ty = m["type"].as_str().unwrap_or("");
            let what = m["label"].as_str().or(m["url"].as_str()).or(m["deviceId"].as_str()).unwrap_or("");
            format!("{} · {}", kind_word(ty), what.rsplit(['/', '\\']).next().unwrap_or(what))
        });
        rows.push(row("media", "media", "배경 미디어", md.unwrap_or_default(), if media.is_some() && !self.layers.hide_bg { vec!["main"] } else { vec![] }));
        // 순서 배경 영상
        let sbg = slide.and_then(|x| x["bg"].as_str()).filter(|b| crate::library::is_video(std::path::Path::new(b.split('?').next().unwrap_or(b)))).map(String::from);
        rows.push(row("slide-bg", "slide-bg", "순서 배경 영상", sbg.clone().unwrap_or_default(), if sbg.is_some() && shown && !self.layers.hide_bg { both.clone() } else { vec![] }));
        // 오버레이 하나하나 (이름 자막 · 시계 · 타이머 · 로고 · 글)
        for o in self.overlays() {
            let id = o["id"].as_str().unwrap_or("");
            let t = o["type"].as_str().unwrap_or("text");
            let on: Vec<&'static str> = if o["on"] == true { OUTPUTS.iter().copied().filter(|x| o["outputs"].as_array().is_some_and(|a| a.iter().any(|y| y == x))).collect() } else { vec![] };
            let nm = o["name"].as_str().filter(|x| !x.is_empty()).or(o["text"].as_str()).unwrap_or("");
            rows.push(SourceRow { id: format!("overlay:{id}"), kind: format!("overlay-{t}"), name: overlay_word(t).into(), detail: nm.to_string(), on_air: on, user: false, enabled: false });
        }
        // 회중 메시지
        let msg_on: Vec<&'static str> = OUTPUTS.iter().copied().filter(|o| !crate::overlays::message_overlay(&self.b.message, o).is_empty()).collect();
        rows.push(row("message", "message", "회중 메시지", self.b.message.as_ref().map(|m| m.text.clone()).unwrap_or_default(), msg_on));
        // 로고 화면
        rows.push(row("logo-screen", "logo-screen", "로고 화면", String::new(), if self.layers.logo { both.clone() } else { vec![] }));
        // 그리기
        rows.push(row("draw", "draw", "그리기", String::new(), if self.b.draw.is_empty() { vec![] } else { both.clone() }));
        // 카메라 (장치별 · 휴대폰): 어느 출력이 지금 쓰는지
        let mut cam_ids: Vec<(String, String)> = vec![];
        let items: Vec<Value> = self.cue.as_ref().map(|c| c.items.clone()).unwrap_or_default();
        let mut devs = self.b.cams.devices.clone();
        devs.extend(crate::phonecam::devices());
        for c in crate::cameras::choices(&devs, s, &items) {
            cam_ids.push((c.device_id, c.label));
        }
        let used = |o: &str| -> Vec<String> {
            let mut v = vec![];
            let t = self.template_for(o);
            if t["background"]["type"] == "camera" && !self.layers.hide_bg {
                v.push(t["background"]["camera"]["deviceId"].as_str().unwrap_or("").to_string());
            }
            if o == "main" && !self.layers.hide_bg {
                if let Some(m) = media.filter(|m| m["type"] == "camera") {
                    v.push(m["deviceId"].as_str().unwrap_or("").to_string());
                }
            }
            if shown {
                if let Some(x) = slide.filter(|x| x["kind"] == "camera") {
                    v.push(x["deviceId"].as_str().unwrap_or("").to_string());
                }
            }
            v
        };
        let (mut um, mut uo) = (used("main"), used("overlay"));
        // 고른 화면 구성에 넣은 카메라
        let scene_of = |o: &str| crate::scenes::find_scene(s, &crate::scenes::output_scene_id(s, o));
        if let Some(sc) = scene_of("main") {
            um.extend(crate::scenes::used_cameras(&sc));
        }
        if let Some(sc) = scene_of("overlay") {
            uo.extend(crate::scenes::used_cameras(&sc));
        }
        for id in um.iter().chain(uo.iter()) {
            if !cam_ids.iter().any(|(d, _)| d == id) {
                cam_ids.push((id.clone(), if id.is_empty() { "기본 카메라".into() } else { id.chars().take(12).collect() }));
            }
        }
        for (id, label) in cam_ids {
            let on: Vec<&'static str> = [("main", &um), ("overlay", &uo)].into_iter().filter(|(_, u)| u.contains(&id)).map(|(o, _)| o).collect();
            let kind = if id.starts_with("phone:") { "phone" } else { "camera" };
            rows.push(SourceRow { id: format!("camera:{id}"), kind: kind.into(), name: if kind == "phone" { "휴대폰 카메라".into() } else { "카메라".into() }, detail: label, on_air: on, user: false, enabled: false });
        }
        // 사용자 소스 (웹 페이지 · 화면 캡처)
        for u in user_sources(s) {
            let uid = u["id"].as_str().unwrap_or("");
            let on: Vec<&'static str> = OUTPUTS
                .iter()
                .copied()
                .filter(|x| match scene_of(x) {
                    Some(sc) => crate::scenes::used_user_sources(&sc).iter().any(|y| y == uid),
                    None => u["on"] == true && u["outputs"].as_array().is_some_and(|a| a.iter().any(|y| y == x)),
                })
                .collect();
            let detail = if u["kind"] == "web" { u["url"].as_str().unwrap_or("").to_string() } else { u["target"]["name"].as_str().unwrap_or("").to_string() };
            rows.push(SourceRow {
                id: u["id"].as_str().unwrap_or("").into(),
                kind: u["kind"].as_str().unwrap_or("").into(),
                name: u["name"].as_str().unwrap_or("").into(),
                detail,
                on_air: on,
                user: true,
                enabled: u["on"] == true,
            });
        }
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_rules() {
        assert_eq!(check_url("example.org/a").unwrap(), "https://example.org/a");
        assert_eq!(check_url(" http://127.0.0.1:8080/x ").unwrap(), "http://127.0.0.1:8080/x");
        for bad in ["", "javascript:alert(1)", "file:///C:/x", "data:text/html,hi", "ftp://x", "https://", "https://a b", "https://u@evil", "https://:80"] {
            assert!(check_url(bad).is_err(), "{bad}");
        }
        assert!(check_url(&format!("https://a.kr/{}", "x".repeat(MAX_URL))).is_err());
    }

    #[test]
    fn normalize_and_keep_unknown() {
        let n = normalize_source(&json!({ "kind": "web", "url": "church.kr", "on": true, "outputs": ["overlay", "x", "overlay"], "future": { "a": 1 } })).unwrap();
        assert!(n["id"].as_str().unwrap().starts_with("src-"));
        assert_eq!(n["url"], "https://church.kr");
        assert_eq!(n["outputs"], json!(["overlay"]));
        assert_eq!(n["place"], "under");
        assert_eq!(n["future"]["a"], 1, "모르는 칸은 그대로");
        assert!(normalize_source(&json!({ "kind": "rtmp" })).is_err());
        assert!(normalize_source(&json!({ "kind": "capture", "target": { "type": "window" } })).is_err(), "화면 캡처는 아직 없음");
        let c = normalize_source(&json!({ "kind": "web", "url": "a.kr" })).unwrap();
        assert_eq!(c["outputs"], json!(["main"]));
    }

    #[test]
    fn save_toggle_delete() {
        let mut s = json!({});
        let a = save_source(&mut s, &json!({ "kind": "web", "url": "https://a.kr", "name": "주보" })).unwrap();
        let id = a["id"].as_str().unwrap().to_string();
        assert_eq!(user_sources(&s).len(), 1);
        assert_eq!(toggle_source(&mut s, &id, None).unwrap()["on"], true);
        let mut b = a.clone();
        b["url"] = json!("https://b.kr");
        save_source(&mut s, &b).unwrap();
        assert_eq!(user_sources(&s)[0]["url"], "https://b.kr");
        delete_source(&mut s, &id);
        assert!(user_sources(&s).is_empty());
        assert!(toggle_source(&mut s, &id, Some(true)).is_err());
        // 깨진 항목은 목록에서 건너뛴다 (저장 형식은 그대로 둠)
        s["sources"] = json!([{ "kind": "web", "url": "javascript:x", "id": "src-1" }, 3, { "kind": "web", "url": "a.kr", "id": "src-2" }]);
        assert_eq!(user_sources(&s).len(), 1);
    }

    #[test]
    fn default_scene_order_matches_old_output() {
        let ov = vec![json!({ "id": "o1" }), json!({ "id": "wpmessage" }), json!({ "id": "o2" })];
        let user = vec![
            normalize_source(&json!({ "id": "src-u", "kind": "web", "url": "a.kr", "on": true, "outputs": ["main"] })).unwrap(),
            normalize_source(&json!({ "id": "src-v", "kind": "web", "url": "b.kr", "on": true, "outputs": ["main", "overlay"], "place": "over" })).unwrap(),
            normalize_source(&json!({ "id": "src-w", "kind": "web", "url": "c.kr", "on": false, "outputs": ["main"] })).unwrap(),
        ];
        let ids = |sc: &Value| sc["layers"].as_array().unwrap().iter().map(|l| l["source"].as_str().unwrap().to_string()).collect::<Vec<_>>();
        assert_eq!(ids(&default_scene("main", &ov, &user)), ["background", "media", "src-u", "slide-bg", "live", "src-v", "overlay:o1", "overlay:o2", "message", "logo-screen", "draw"]);
        assert_eq!(ids(&default_scene("overlay", &[], &user)), ["background", "media", "slide-bg", "live", "src-v", "message", "logo-screen", "draw", "pip"]);
        // 칸에는 2단계용 rect · crop · opacity 가 없다 → 웹 출력은 전체 화면 그대로
        assert!(default_scene("main", &[], &[])["layers"].as_array().unwrap().iter().all(|l| l.get("rect").is_none() && l.get("opacity").is_none()));
        let ps = payload_sources("overlay", &user);
        assert!(ps.get("src-v").is_some() && ps.get("src-u").is_none() && ps.get("src-w").is_none());
        assert_eq!(ps["src-v"]["url"], "https://b.kr");
    }

    #[test]
    fn old_settings_keep_unknown_keys() {
        // 옛 판(2.0.x)과 같은 읽기: 기본값 합치기가 새 키(sources · scenes)를 지우지 않는다
        let dir = std::env::temp_dir().join(format!("wp-sources-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("settings.json"), r#"{"sources":[{"id":"src-a","kind":"web","url":"https://a.kr","on":true,"outputs":["main"]}],"scenes":[{"id":"x"}]}"#).unwrap();
        let st = crate::settings::Settings::load(&dir);
        assert_eq!(st.get()["sources"][0]["id"], "src-a");
        assert_eq!(st.get()["scenes"][0]["id"], "x");
        assert_eq!(user_sources(st.get()).len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn core() -> Core {
        let dir = std::env::temp_dir().join(format!("wp-sources-core-{}", crate::util::short_id("t")));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("bible.db"), b"").unwrap(); // 실제 자료를 복사하지 않게 빈 DB
        Core::open(dir.clone(), dir, None).unwrap()
    }

    #[test]
    fn payload_and_status_from_core() {
        let mut c = core();
        // 옛 칸은 그대로, scene · sources 만 덧붙음
        let p = c.output_payload("main");
        for k in ["template", "slide", "layers", "overlays", "video", "cameras", "scene", "sources"] {
            assert!(p.get(k).is_some(), "{k}");
        }
        assert_eq!(p["scene"]["id"], "full");
        assert_eq!(p["sources"], json!({}));
        // 웹 소스를 켜면 그 출력의 구성 · 데이터에만 들어간다
        let s = c.save_source(&json!({ "kind": "web", "name": "홈", "url": "https://church.kr", "on": true, "outputs": ["overlay"] })).unwrap();
        let id = s["id"].as_str().unwrap().to_string();
        assert!(c.output_payload("main")["sources"].get(&id).is_none());
        let po = c.output_payload("overlay");
        assert_eq!(po["sources"][&id]["url"], "https://church.kr");
        assert!(po["scene"]["layers"].as_array().unwrap().iter().any(|l| l["source"] == id.as_str()));
        // 목록: 내장 소스 + 웹 소스, 웹 소스는 OBS 에 나가는 중, 보내는 화면이 없으니 연결된 출력 없음
        let rows = c.sources_status();
        for id in ["live", "background", "media", "slide-bg", "message", "logo-screen", "draw"] {
            assert!(rows.iter().any(|r| r.id == id), "{id}");
        }
        let w = rows.iter().find(|r| r.id == id).unwrap();
        assert!(w.user && w.enabled && w.on_air == vec!["overlay"]);
        assert!(rows.iter().find(|r| r.id == "live").unwrap().on_air.is_empty(), "송출 전");
        assert!(c.connected_outputs().is_empty());
        // 로고 화면 켬 → 두 출력
        c.layers.logo = true;
        assert_eq!(c.sources_status().iter().find(|r| r.id == "logo-screen").unwrap().on_air, vec!["main", "overlay"]);
        c.toggle_source(&id, Some(false)).unwrap();
        assert!(c.output_payload("overlay")["sources"].get(&id).is_none());
    }

    /// 로드맵 3단계 시험 기준: 큐시트 하나에서 성경봉독 · 찬양 순서마다 예배당 · 방송이 각자 다른 구성으로
    #[test]
    fn per_item_and_type_scenes_follow_live_item() {
        let mut c = core();
        let text = c.save_scene(&json!({ "id": "sc-text", "name": "본문 전체", "layers": [{ "source": "background" }, { "source": "live" }] })).unwrap();
        let camsub = c.save_scene(&json!({ "id": "sc-camsub", "name": "카메라 + 아래 자막", "layers": [{ "source": "camera:c1" }, { "source": "live", "tpl": "overlay" }] })).unwrap();
        let corner = c.save_scene(&json!({ "id": "sc-corner", "name": "가사 + 구석 카메라", "layers": [{ "source": "background" }, { "source": "live" }, { "source": "camera:c1", "rect": [0.72, 0.7, 0.25, 0.25] }] })).unwrap();
        assert_eq!((text["id"].as_str(), camsub["id"].as_str(), corner["id"].as_str()), (Some("sc-text"), Some("sc-camsub"), Some("sc-corner")));
        c.new_cue("주일").unwrap();
        let mut bible = crate::core::new_item("title", "성경봉독");
        bible["type"] = json!("text");
        bible["text"] = json!("태초에 하나님이 천지를 창조하시니라");
        c.add_item(bible, None);
        c.add_item(crate::core::new_item("title", "찬양"), None);
        c.add_item(crate::core::new_item("title", "광고"), None);
        // 순서 0 (공지 = 성경봉독): 예배당 본문 전체 · 방송 카메라 + 자막 — 순서에 직접
        c.set_item_scene(0, "main", "sc-text").unwrap();
        c.set_item_scene(0, "overlay", "sc-camsub").unwrap();
        // 순서 1 (제목 = 찬양 자리): 종류 기본으로 — 방송만 구석 카메라
        c.set_type_scene("title", "overlay", "sc-corner").unwrap();
        // 순서 2 는 광고 (제목) 이지만 순서에 '전체 화면' 을 못 박음
        c.set_item_scene(2, "overlay", "full").unwrap();
        let sid = |c: &Core, o: &str| c.output_payload(o)["scene"]["id"].as_str().unwrap_or("").to_string();
        assert_eq!((sid(&c, "main"), sid(&c, "overlay")), ("full".into(), "full".into()), "송출 전 = 출력 기본");
        assert!(c.send_item(0));
        assert_eq!((sid(&c, "main"), sid(&c, "overlay")), ("sc-text".into(), "sc-camsub".into()));
        let po = c.output_payload("overlay");
        assert_eq!(po["scene"]["layers"][0]["source"], "camera:c1");
        assert!(po["scene"]["layers"][1]["template"].is_null(), "방송 출력의 자막 칸 = 방송 무대 템플릿 그대로 (무대에 바로)");
        assert!(c.send_item(1));
        assert_eq!((sid(&c, "main"), sid(&c, "overlay")), ("full".into(), "sc-corner".into()), "종류 기본");
        assert!(c.send_item(2));
        assert_eq!(sid(&c, "overlay"), "full", "순서에 못 박은 전체 화면");
        // 송출 중에 순서 지정을 바꾸면 바로 (다시 송출하지 않아도)
        c.set_item_scene(2, "overlay", "").unwrap();
        assert_eq!(sid(&c, "overlay"), "sc-corner");
        // 표시용: 기본과 다른 출력
        let items = c.cue.as_ref().unwrap().items.clone();
        assert_eq!(crate::outputs::item_differs(c.settings.get(), &items[0]).len(), 2);
        // 실행 취소로 순서 지정이 돌아간다
        assert!(items[0]["scenes"].is_object());
    }

    #[test]
    fn extra_output_payload_and_audio() {
        let mut c = core();
        let lobby = c.save_extra_output(&crate::outputs::new_extra("로비 화면", "main")).unwrap();
        let lid = lobby["id"].as_str().unwrap().to_string();
        let obs2 = c.save_extra_output(&crate::outputs::new_extra("둘째 방송", "overlay")).unwrap();
        let oid = obs2["id"].as_str().unwrap().to_string();
        let p = c.output_payload(&lid);
        assert_eq!(p["output"], lid.as_str());
        assert_eq!(p["template"]["id"], c.output_payload("main")["template"]["id"], "템플릿이 비면 예배당 템플릿");
        assert_eq!(p["audio"], false);
        assert_eq!(c.output_payload("main")["audio"], true, "소리는 예배당 하나");
        assert_eq!(c.output_payload("overlay")["audio"], false);
        assert_eq!(c.output_payload(&oid)["template"]["id"], c.output_payload("overlay")["template"]["id"]);
        assert!(c.output_payload(&oid)["scene"]["layers"].as_array().unwrap().iter().any(|l| l["source"] == "pip"), "방송처럼 → 그림 우상단 칸");
        c.set_audio_output(&lid).unwrap();
        assert_eq!((c.output_payload("main")["audio"].as_bool(), c.output_payload(&lid)["audio"].as_bool()), (Some(false), Some(true)));
        // 'OBS 화면 숨김' 은 추가 출력에 안 미친다
        c.obs_hold = true;
        assert_eq!(c.output_payload("overlay")["held"], true);
        assert_eq!(c.output_payload(&lid)["held"], false);
        c.obs_hold = false;
        // 추가 출력에 자기 구성
        c.save_scene(&json!({ "id": "sc-l", "name": "로비", "layers": [{ "source": "live", "rect": [0, 0, 0.5, 1] }] })).unwrap();
        c.set_output_scene(&lid, "sc-l").unwrap();
        assert_eq!(c.output_payload(&lid)["scene"]["id"], "sc-l");
        assert_eq!(c.output_payload("main")["scene"]["id"], "full");
        // 지우면 소리 · 구성 지정도 정리
        c.delete_extra_output(&lid);
        assert_eq!(c.output_payload("main")["audio"], true);
        assert!(c.settings.get()["outputScenes"].get(&lid).is_none());
        assert_eq!(c.output_list().len(), 3);
    }
}
