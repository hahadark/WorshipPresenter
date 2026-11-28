//! [웹판] 클라우드에 붙기 (3단계) — 계약 docs/web/architecture.md 4 · 5절.
//!
//! - 설정: Worker 가 /control/ 페이지 머리에 넣는 `window.WP_CLOUD = {church, role, authed, demo}`. 없으면(tools/web/serve.js) 내 PC 시험.
//! - 송출 주체: 주인 쿠키로 `/bridge` 에 붙어 설치판과 같은 메시지(hello · render · draw)를 보낸다. `hello.source = 'web'`.
//!   설치판이 이미 주체면 '대기'(보기 전용 — 클라우드 출력에 보내지 않음)로 붙고, 주인이 [넘겨받기]를 눌러야만(claim) 주체가 된다.
//!   넘겨받은 동안 설치판이 다시 붙어도 대기. [설치판에 돌려주기](release). 끊기면 1 → 2 → … 15초 간격으로 다시 잇는다.
//! - 위쪽 가운데 작은 띠: 지금 상태와 단추 (넘겨받기 · 돌려주기 · 교회 관리 · 로그인).
use eframe::egui;
use eframe::wasm_bindgen::{self, prelude::*, JsCast as _};
use serde_json::{json, Value};
use std::cell::RefCell;
use wp_core::core::{Ev, OutputClient, Shared};

#[derive(Clone, Debug, Default)]
pub struct Cfg {
    pub church: String,
    pub role: String,
    pub authed: bool,
    pub demo: bool,
}

fn read_cfg() -> Option<Cfg> {
    let w = web_sys::window()?;
    let v = js_sys::Reflect::get(&w, &JsValue::from_str("WP_CLOUD")).ok()?;
    if v.is_undefined() || v.is_null() {
        return None;
    }
    let s = js_sys::JSON::stringify(&v).ok()?.as_string()?;
    let j: Value = serde_json::from_str(&s).ok()?;
    let g = |k: &str| j[k].as_str().unwrap_or("").to_string();
    Some(Cfg { church: g("church"), role: g("role"), authed: j["authed"].as_bool().unwrap_or(false), demo: j["demo"].as_bool().unwrap_or(false) })
}

thread_local! {
    /// 코어 (띠의 단추가 쓴다 — 송출 주체가 아닌 편집자도)
    static CORE: RefCell<Option<Shared>> = const { RefCell::new(None) };
    static CFG: Option<Cfg> = read_cfg();
    static BR: RefCell<Option<Bridge>> = const { RefCell::new(None) };
    static UI: RefCell<UiState> = RefCell::new(UiState::default());
}

pub fn cfg() -> Option<Cfg> {
    CFG.with(|c| c.clone())
}
/// 로그인한 클라우드 조작 화면인가
pub fn authed() -> bool {
    cfg().is_some_and(|c| c.authed)
}
/// 둘러보기 (tiers/demo.mjs 의 demo 역할 — 견본 교회, 방문자마다 따로인 중계실)
pub fn demo() -> bool {
    cfg().is_some_and(|c| c.demo || c.role == "demo")
}
pub fn church() -> String {
    cfg().map(|c| c.church).unwrap_or_default()
}
/// 역할이 할 수 있는 일 (cloud/src/protocol.mjs CAPS 와 같게)
pub fn can(cap: &str) -> bool {
    let role = cfg().map(|c| c.role).unwrap_or_default();
    match role.as_str() {
        "owner" => true,
        "member-editor" => matches!(cap, "view" | "remote" | "editor" | "read" | "write" | "ai"),
        "member-viewer" => matches!(cap, "view" | "read"),
        "demo" => matches!(cap, "view" | "remote" | "editor" | "source" | "read" | "write"),
        _ => false,
    }
}
/// 출력 창 주소 앞머리 (클라우드는 어댑터가, 내 PC 시험은 local-live.js 가 같은 브라우저 송출을 받는다)
pub fn outputs_base() -> &'static str {
    "/outputs/"
}

// ---------------- HTTP ----------------

/// 같은 곳의 API → (상태 코드, JSON). 쓰기에는 X-WorshipCue: 1 (쓰기 보호). 닿지 못하면 (0, Null)
pub async fn api(method: &str, path: &str, body: Option<&Value>, headers: &[(&str, &str)]) -> (u16, Value) {
    let (st, text) = api_text(method, path, body.map(|b| b.to_string()), headers).await;
    (st, serde_json::from_str(&text).unwrap_or(Value::Null))
}

pub async fn api_text(method: &str, path: &str, body: Option<String>, headers: &[(&str, &str)]) -> (u16, String) {
    let Some(w) = web_sys::window() else { return (0, String::new()) };
    let init = web_sys::RequestInit::new();
    init.set_method(method);
    init.set_credentials(web_sys::RequestCredentials::SameOrigin);
    let h = web_sys::Headers::new().unwrap();
    if method != "GET" && method != "HEAD" {
        let _ = h.set("x-worshipcue", "1");
    }
    if let Some(b) = &body {
        let _ = h.set("content-type", "application/json");
        init.set_body(&JsValue::from_str(b));
    }
    for (k, v) in headers {
        let _ = h.set(k, v);
    }
    init.set_headers(&h);
    let p = w.fetch_with_str_and_init(path, &init);
    let Ok(res) = wasm_bindgen_futures::JsFuture::from(p).await else { return (0, String::new()) };
    let Ok(res) = res.dyn_into::<web_sys::Response>() else { return (0, String::new()) };
    let st = res.status();
    let text = match res.text() {
        Ok(t) => wasm_bindgen_futures::JsFuture::from(t).await.ok().and_then(|v| v.as_string()).unwrap_or_default(),
        Err(_) => String::new(),
    };
    if st == 401 && authed() {
        // 로그인이 끝났다 (쿠키 만료 · 열쇠 지움) → 한 번 알리고 로그인 화면으로
        UI.with(|u| u.borrow_mut().logged_out = true);
    }
    (st, text)
}

// ---------------- 송출 주체 (/bridge) ----------------

#[derive(Clone, Debug, PartialEq, Default)]
pub enum State {
    #[default]
    Off,
    Connecting,
    /// 주체 (claimed = 주인이 넘겨받음 → 다시 붙는 설치판은 대기)
    Active { claimed: bool },
    /// 대기 — 다른 주체(설치판)가 송출 중. 클라우드 출력에 보내지 않는다
    Standby { by: String },
}

struct Bridge {
    ws: Option<web_sys::WebSocket>,
    state: State,
    core: Shared,
    delay: i32,
    viewers: usize,
}

#[derive(Default)]
struct UiState {
    /// 점검용 (?debug): 단추 자리 — 끝에서 끝 시험이 누르려고
    rects: Vec<(String, egui::Rect)>,
    confirm_claim: bool,
    logged_out: bool,
    note: String,
}

fn with<R>(f: impl FnOnce(&mut Bridge) -> R) -> Option<R> {
    BR.with(|b| b.borrow_mut().as_mut().map(f))
}

pub fn state() -> State {
    with(|b| b.state.clone()).unwrap_or_default()
}

/// 지금 클라우드 출력으로 보내도 되는가 (주체일 때만)
pub fn active() -> bool {
    matches!(state(), State::Active { .. })
}

/// 주체일 때만 보낸다 (대기 · 끊김이면 버림 — 다시 주체가 되면 sync 로 전부 다시 보낸다)
pub fn send(msg: &Value) {
    with(|b| {
        if !matches!(b.state, State::Active { .. }) {
            return;
        }
        if let Some(ws) = &b.ws {
            if ws.ready_state() == web_sys::WebSocket::OPEN {
                let _ = ws.send_with_str(&msg.to_string());
            }
        }
    });
}

fn send_raw(msg: &Value) {
    with(|b| {
        if let Some(ws) = &b.ws {
            if ws.ready_state() == web_sys::WebSocket::OPEN {
                let _ = ws.send_with_str(&msg.to_string());
            }
        }
    });
}

pub fn claim() {
    send_raw(&json!({ "t": "claim" }));
}
pub fn release() {
    send_raw(&json!({ "t": "release" }));
}

/// 주인으로 로그인했으면 송출 주체로 붙는다 (편집자 · 보기만은 붙지 않는다 — source 권한 없음)
pub fn set_core(core: Shared) {
    CORE.with(|c| *c.borrow_mut() = Some(core));
}

pub fn start_bridge(core: Shared) {
    if !authed() || !can("source") {
        return;
    }
    BR.with(|b| *b.borrow_mut() = Some(Bridge { ws: None, state: State::Connecting, core, delay: 1000, viewers: 0 }));
    connect();
}

fn ws_url() -> String {
    let loc = web_sys::window().map(|w| w.location());
    let (proto, host) = loc.map(|l| (l.protocol().unwrap_or_default(), l.host().unwrap_or_default())).unwrap_or_default();
    format!("{}//{}/bridge", if proto == "https:" { "wss:" } else { "ws:" }, host)
}

fn connect() {
    let Ok(ws) = web_sys::WebSocket::new(&ws_url()) else {
        retry();
        return;
    };
    let onmsg = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(|e: web_sys::MessageEvent| {
        if let Some(s) = e.data().as_string() {
            if s == "pong" {
                return;
            }
            if let Ok(m) = serde_json::from_str::<Value>(&s) {
                on_message(m);
            }
        }
    });
    ws.set_onmessage(Some(onmsg.as_ref().unchecked_ref()));
    onmsg.forget();
    let onclose = Closure::<dyn FnMut(web_sys::CloseEvent)>::new(|e: web_sys::CloseEvent| {
        let code = e.code();
        if code == 4003 {
            UI.with(|u| u.borrow_mut().logged_out = true);
            with(|b| b.state = State::Off);
            return;
        }
        with(|b| {
            b.ws = None;
            b.state = State::Connecting;
        });
        retry();
    });
    ws.set_onclose(Some(onclose.as_ref().unchecked_ref()));
    onclose.forget();
    with(|b| {
        b.ws = Some(ws);
        b.state = State::Connecting;
    });
}

fn retry() {
    let d = with(|b| {
        let d = b.delay;
        b.delay = (b.delay * 2).min(15_000);
        d
    })
    .unwrap_or(5000);
    wp_core::web::defer(d + (js_sys::Math::random() * 500.0) as i32, connect);
}

fn hello() {
    let (lang, outputs) = with(|b| {
        let c = b.core.lock();
        let mut o: Vec<String> = vec!["main".into(), "overlay".into(), "stage".into()];
        for x in wp_core::outputs::extra_outputs(c.settings.get()) {
            if let Some(id) = x["id"].as_str() {
                o.push(id.into());
            }
        }
        (c.settings.get()["uiLang"].as_str().unwrap_or("ko").to_string(), o)
    })
    .unwrap_or_default();
    send_raw(&json!({ "t": "hello", "v": 1, "app": format!("워십프리젠터 웹 {}", env!("CARGO_PKG_VERSION")), "lang": lang, "outputs": outputs,
        "caps": { "remote": false, "editor": false, "media": false }, "source": "web" }));
}

fn on_message(m: Value) {
    match m["t"].as_str().unwrap_or("") {
        "ready" => {
            with(|b| b.delay = 1000);
            hello();
            if m["standby"].as_bool().unwrap_or(false) {
                set_standby(&m["active"]);
            } else {
                with(|b| b.state = State::Active { claimed: false });
                super::live::push_all();
            }
        }
        "standby" => set_standby(&m["active"]),
        "active" => {
            let claimed = m["claimed"].as_bool().unwrap_or(false);
            with(|b| b.state = State::Active { claimed });
        }
        "sync" => super::live::push_all(),
        "viewers" => {
            let list = m["list"].as_array().cloned().unwrap_or_default();
            with(|b| {
                b.viewers = list.len();
                let mut c = b.core.lock();
                // 클라우드로 보는 화면도 '연결된 출력' 에 (설치판 SSE 와 같은 칸) — 번호가 겹치지 않게 큰 수를 더한다
                c.outputs.retain(|id, _| *id < CLOUD_ID);
                for v in &list {
                    let id = CLOUD_ID + v["id"].as_u64().unwrap_or(0);
                    let ack = v["ack"].as_object().map(|a| json!({ "version": a.get("version"), "at": a.get("at"), "width": a.get("width"), "height": a.get("height"), "visible": a.get("visible") }));
                    c.outputs.insert(id, OutputClient { client: v["client"].as_str().unwrap_or("main").into(), kind: v["kind"].as_str().unwrap_or("browser").into(), connected_at: v["since"].as_i64().unwrap_or(0), ack });
                }
                c.emit(Ev::Outputs);
            });
        }
        "cmd" => {
            let key = m["args"]["key"].as_str().unwrap_or("").to_string();
            let dur = m["args"]["duration"].as_f64();
            with(|b| {
                let mut c = b.core.lock();
                match m["cmd"].as_str() {
                    Some("videoMeta") => c.video_meta(&key, dur),
                    Some("videoEnded") => c.video_ended(&key, dur),
                    _ => {}
                }
            });
        }
        // 웹 조작 화면은 리모컨 · 편집기 · 미디어 요청을 받지 않는다 (hello.caps 를 모두 꺼 두었다)
        "req" => send_raw(&json!({ "t": "res", "id": m["id"], "status": 501, "headers": {}, "body": { "ok": false, "error": "웹 조작 화면은 이 요청을 받지 않습니다. 설치판에서 쓰세요." } })),
        "denied" => UI.with(|u| u.borrow_mut().note = m["error"].as_str().unwrap_or("").to_string()),
        _ => {}
    }
}

/// 클라우드로 보는 화면을 core.outputs 에 넣을 때 더하는 수 (같은 브라우저 출력 번호와 겹치지 않게)
pub const CLOUD_ID: u64 = 1 << 40;

fn set_standby(active: &Value) {
    let by = match active["source"].as_str() {
        Some("web") => "다른 웹 조작 화면".to_string(),
        _ => format!("설치판{}", active["app"].as_str().filter(|s| !s.is_empty()).map(|s| format!(" ({s})")).unwrap_or_default()),
    };
    with(|b| {
        b.state = State::Standby { by };
        // 대기 중에는 클라우드로 보는 화면이 내 것이 아니다
        let mut c = b.core.lock();
        c.outputs.retain(|id, _| *id < CLOUD_ID);
        c.emit(Ev::Outputs);
    });
}

/// 점검용: 띠의 단추 자리 {이름: [x, y]} (가운데)
pub fn debug_rects() -> Value {
    UI.with(|u| Value::Object(u.borrow().rects.iter().map(|(n, r)| (n.clone(), json!([r.center().x, r.center().y]))).collect()))
}

// ---------------- 위쪽 가운데 띠 ----------------

fn open_tab(url: &str) {
    if let Some(w) = web_sys::window() {
        let _ = w.open_with_url_and_target(url, "_blank");
    }
}

/// 조작 화면 위에 그리는 작은 띠 (WebApp::ui 가 프레임마다 부른다)
pub fn ui(ctx: &egui::Context) {
    let Some(c) = cfg() else { return };
    UI.with(|u| u.borrow_mut().rects.clear());
    let note_rect = |name: &str, r: &egui::Response| UI.with(|u| u.borrow_mut().rects.push((name.to_string(), r.rect)));
    let st = state();
    let (text, color): (String, egui::Color32) = if demo() {
        ("둘러보기 — 견본 교회 (고친 것은 하루 안에 처음으로 돌아갑니다)".into(), egui::Color32::from_rgb(0x8b, 0x90, 0x99))
    } else if !c.authed {
        return;
    } else {
        match &st {
            State::Off if !can("source") => (format!("{} · {}", c.church, if can("write") { "편집자 — 송출은 주인만" } else { "보기만" }), egui::Color32::from_rgb(0x8b, 0x90, 0x99)),
            State::Off | State::Connecting => ("클라우드에 잇는 중…".into(), egui::Color32::from_rgb(0xe0, 0xa0, 0x60)),
            State::Active { claimed: true } => ("클라우드 송출: 이 웹 화면 (넘겨받음)".into(), egui::Color32::from_rgb(0xe5, 0x48, 0x4d)),
            State::Active { claimed: false } => ("클라우드 송출: 이 웹 화면".into(), egui::Color32::from_rgb(0xe5, 0x48, 0x4d)),
            State::Standby { by } => (format!("보기 전용 — {by} 이(가) 송출 중 (클라우드 출력에 보내지 않음)"), egui::Color32::from_rgb(0xe0, 0xa0, 0x60)),
        }
    };
    let logged_out = UI.with(|u| u.borrow().logged_out);
    egui::Area::new(egui::Id::new("wp-cloud-band")).anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 3.0)).order(egui::Order::Foreground).show(ctx, |ui| {
        egui::Frame::new().fill(egui::Color32::from_rgb(0x22, 0x24, 0x28)).stroke(egui::Stroke::new(1.0, color)).corner_radius(10.0).inner_margin(egui::Margin::symmetric(10, 3)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("●").color(color).size(11.0));
                if logged_out {
                    ui.label(egui::RichText::new("로그인이 끝났습니다").size(12.0));
                    if ui.small_button("다시 로그인").clicked() {
                        if let Some(w) = web_sys::window() {
                            let _ = w.location().set_href("/login?next=/control/");
                        }
                    }
                    return;
                }
                ui.label(egui::RichText::new(text).size(12.0));
                if demo() {
                    if ui.small_button("우리 교회로 시작").clicked() {
                        if let Some(w) = web_sys::window() {
                            let _ = w.location().set_href("/login?next=/control/");
                        }
                    }
                    return;
                }
                match st {
                    State::Standby { .. } if can("source") => {
                        let b = ui.small_button("넘겨받기…").on_hover_text("이 웹 화면이 클라우드 출력의 송출을 맡습니다. 설치판의 클라우드 송출은 멈춥니다(설치판 자체의 예배당 창 · OBS 는 그대로).");
                        note_rect("claim", &b);
                        if b.clicked() {
                            UI.with(|u| u.borrow_mut().confirm_claim = true);
                        }
                    }
                    State::Active { claimed: true } => {
                        let b = ui.small_button("설치판에 돌려주기");
                        note_rect("release", &b);
                        if b.clicked() {
                            release();
                        }
                    }
                    _ => {}
                }
                if wp_core::web::ai::waiting() > 0 {
                    ui.label(egui::RichText::new("AI 답 기다리는 중…").size(12.0)); // 기다리는 동안에도 다른 조작이 된다
                }
                // 4단계: 파일 넣기 (그림 · 영상 · 소리 · PDF · 글꼴 · PPT → 교회 저장소 → 큐시트 순서) · 휴대폰 카메라
                if can("write") && super::media::available() && !demo() {
                    let n = super::media::busy();
                    if n > 0 {
                        ui.label(egui::RichText::new(format!("올리는 중… {n}")).size(12.0));
                    }
                    let b = ui.small_button("파일 넣기…").on_hover_text("그림 · 영상 · 소리 · PDF · 글꼴 · PowerPoint 를 교회 저장소에 올려 큐시트 끝에 넣습니다. 화면에 끌어다 놓아도 됩니다.");
                    note_rect("files", &b);
                    if b.clicked() {
                        if let Some(core) = CORE.with(|c| c.borrow().clone()) {
                            super::media::pick_and_put(core);
                        }
                    }
                    let b = ui.small_button("휴대폰 카메라…");
                    note_rect("phone", &b);
                    if b.clicked() {
                        super::media::open_phone();
                    }
                }
                if can("manage") && ui.small_button("교회 관리").on_hover_text("사람 · 기기 열쇠(리모컨 · 편집기 PIN) · AI 설정 — 새 탭").clicked() {
                    open_tab("/owner");
                }
            });
        });
    });
    // 넘겨받기 확인
    if UI.with(|u| u.borrow().confirm_claim) {
        let mut open = true;
        let mut close = false;
        egui::Window::new("송출 넘겨받기").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0)).open(&mut open).show(ctx, |ui| {
            ui.label("설치판이 지금 클라우드 출력(예배당 · OBS · 무대 화면)에 송출하고 있습니다.");
            ui.label("넘겨받으면 클라우드 출력은 이 웹 화면의 것을 보여 주고, 설치판이 다시 이어져도 대기로 남습니다.");
            ui.label(egui::RichText::new("예배 중이면 설치판을 쓰는 사람과 먼저 이야기하세요.").strong());
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let b = ui.button("넘겨받기");
                note_rect("confirm", &b);
                if b.clicked() {
                    claim();
                    close = true;
                }
                if ui.button("취소").clicked() {
                    close = true;
                }
            });
        });
        if close || !open {
            UI.with(|u| u.borrow_mut().confirm_claim = false);
        }
    }
    let note = UI.with(|u| std::mem::take(&mut u.borrow_mut().note));
    if !note.is_empty() {
        with(|b| b.core.lock().notify(note, "error"));
    }
}
