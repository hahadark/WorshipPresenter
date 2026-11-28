//! [웹판 6단계] 오프라인 대비의 브라우저 쪽 — `window.wpOffline` (cloud/client/offline/offline.js) 를 부르는 얇은 층과
//! 조작 화면 위의 작은 상태 칸('연결 끊김 · 올릴 것 N개' · 저장 멈춤 · '새 버전 있음'). 설계: docs/web/offline.md
//!
//! - 연결 상태(Net)는 store.rs 가 자료 API 의 답으로 정한다: 닿지 않음(0 · 502~504) = 끊김, 등급 오류 코드(drive_* · tier_*) = 저장 멈춤.
//!   둘은 다른 칸 · 다른 글로 보인다 (등급 오류를 '인터넷 끊김' 으로 말하지 않는다).
//! - 송출 상태 띠(cloud.rs)는 그대로 두고 그 아래에 붙는다.
use eframe::egui;
use eframe::wasm_bindgen::{prelude::*, JsCast as _};
use serde_json::{json, Value};
use std::cell::RefCell;

#[derive(Clone, Debug, PartialEq, Default)]
pub enum Net {
    #[default]
    Online,
    /// 교회 클라우드에 닿지 않음 (인터넷 끊김 · 서버가 답하지 않음)
    Offline,
    /// 닿기는 하지만 교회 저장소가 받지 않음 (구글 드라이브 끊김 · 가득 참 · 옮기는 중 …)
    Blocked { code: String, msg: String },
}

#[derive(Default)]
struct St {
    net: Net,
    pending: usize,
    /// 교회 자료를 한 번이라도 받았나 (아니면 '아직 못 받음')
    loaded: bool,
    js: Value,
    info: bool,
    confirm_update: bool,
    rects: Vec<(String, egui::Rect)>,
}

thread_local! {
    static ST: RefCell<St> = RefCell::new(St::default());
}

fn obj() -> Option<JsValue> {
    let w = web_sys::window()?;
    let o = js_sys::Reflect::get(&w, &JsValue::from_str("wpOffline")).ok()?;
    if o.is_undefined() || o.is_null() {
        None
    } else {
        Some(o)
    }
}

/// 이 페이지에 오프라인 보관함이 있나 (클라우드 /control/ 에만 끼워진다)
pub fn available() -> bool {
    obj().is_some()
}

fn func(o: &JsValue, name: &str) -> Option<js_sys::Function> {
    js_sys::Reflect::get(o, &JsValue::from_str(name)).ok()?.dyn_into().ok()
}

async fn kv_call(name: &str, args: &[JsValue]) -> Option<JsValue> {
    let o = obj()?;
    let kv = js_sys::Reflect::get(&o, &JsValue::from_str("kv")).ok()?;
    let f = func(&kv, name)?;
    let arr = js_sys::Array::new();
    for a in args {
        arr.push(a);
    }
    let p = f.apply(&kv, &arr).ok()?;
    wasm_bindgen_futures::JsFuture::from(js_sys::Promise::from(p)).await.ok()
}

pub async fn kv_get(key: &str) -> Option<String> {
    kv_call("get", &[JsValue::from_str(key)]).await?.as_string()
}

/// 앞머리가 같은 것 모두 (열쇠, 값)
pub async fn kv_list(prefix: &str) -> Vec<(String, String)> {
    let Some(s) = kv_call("list", &[JsValue::from_str(prefix)]).await.and_then(|v| v.as_string()) else { return vec![] };
    let v: Value = serde_json::from_str(&s).unwrap_or(Value::Null);
    v.as_array().into_iter().flatten().filter_map(|x| Some((x[0].as_str()?.to_string(), x[1].as_str()?.to_string()))).collect()
}

/// 한 묶음으로 쓰기 [["put", 열쇠, 값] | ["del", 열쇠]] — 보관함이 없으면 Ok (메모리만)
pub async fn kv_batch(ops: &Value) -> Result<(), String> {
    if !available() {
        return Ok(());
    }
    match kv_call("batch", &[JsValue::from_str(&ops.to_string())]).await.and_then(|v| v.as_string()) {
        Some(s) if s == "ok" => Ok(()),
        Some(s) => Err(s),
        None => Err("보관함 오류".into()),
    }
}

/// 자주 쓰는 그림을 미리 받아 두기 (서비스 워커가 담는다)
pub fn prefetch(urls: &[String]) {
    let Some(o) = obj() else { return };
    if let Some(f) = func(&o, "prefetch") {
        let _ = f.call1(&o, &JsValue::from_str(&json!(urls).to_string()));
    }
}

/// 새 판이 있나 물어보기 (바꾸지는 않는다) — 다시 이어졌을 때
pub fn check() {
    let Some(o) = obj() else { return };
    if let Some(f) = func(&o, "check") {
        let _ = f.call0(&o);
    }
}

fn apply_update() -> bool {
    let Some(o) = obj() else { return false };
    func(&o, "applyUpdate").and_then(|f| f.call0(&o).ok()).and_then(|v| v.as_bool()).unwrap_or(false)
}

fn read_js() -> Value {
    let Some(o) = obj() else { return Value::Null };
    func(&o, "state").and_then(|f| f.call0(&o).ok()).and_then(|v| v.as_string()).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(Value::Null)
}

/// 2초마다 서비스 워커 상태(새 판 있음 …)를 읽어 둔다. `wake` 가 바뀌면(브라우저의 online 사건) on_online 을 부른다
pub fn start(on_online: impl Fn() + 'static) {
    if !available() {
        return;
    }
    let mut last_wake = -1i64;
    let tick = Closure::<dyn FnMut()>::new(move || {
        let js = read_js();
        let wake = js["wake"].as_i64().unwrap_or(0);
        if last_wake >= 0 && wake != last_wake {
            on_online();
        }
        last_wake = wake;
        ST.with(|s| s.borrow_mut().js = js);
    });
    if let Some(w) = web_sys::window() {
        let _ = w.set_interval_with_callback_and_timeout_and_arguments_0(tick.as_ref().unchecked_ref(), 2000);
    }
    tick.forget();
}

thread_local! {
    /// 이 탭이 이 교회의 조작 화면 주인(송출 · 저장)인가 — None = 아직 모름
    static PRIMARY: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

/// 송출 · 저장해도 되는 탭인가 (offline.js 의 탭 잠금). 보관함이 없는 페이지(내 PC 시험)는 늘 그렇다
pub fn is_primary() -> bool {
    !available() || PRIMARY.with(|p| p.get()) == Some(true)
}
/// 다른 탭이 쓰는 중이라 보기 전용인가
pub fn is_secondary() -> bool {
    available() && PRIMARY.with(|p| p.get()) == Some(false)
}

/// 이 탭이 주인이면 f 를 부른다 (잠금이 정해질 때까지 0.2초마다 본다). 보기 전용이면 부르지 않는다 — 앞 탭이 닫혀 잠금을 받으면 페이지가 다시 열린다
pub fn when_primary(f: impl FnOnce() + 'static) {
    if !available() {
        PRIMARY.with(|p| p.set(Some(true)));
        f();
        return;
    }
    let tab = read_js()["tab"].as_str().unwrap_or("none").to_string();
    match tab.as_str() {
        "primary" | "none" => {
            PRIMARY.with(|p| p.set(Some(true)));
            f();
        }
        "secondary" | "promoted" => {
            PRIMARY.with(|p| p.set(Some(false)));
            log::info!("같은 브라우저의 다른 탭이 이 교회 조작 화면을 쓰는 중 — 이 탭은 보기 전용");
        }
        _ => wp_core::web::defer(200, move || when_primary(f)),
    }
}

pub fn net() -> Net {
    ST.with(|s| s.borrow().net.clone())
}
pub fn set_net(n: Net) {
    ST.with(|s| s.borrow_mut().net = n);
}
pub fn set_pending(n: usize) {
    ST.with(|s| s.borrow_mut().pending = n);
}
pub fn set_loaded() {
    ST.with(|s| s.borrow_mut().loaded = true);
}

/// 점검용 (?debug): {net, pending, update, sw, version, controlled, buttons}
pub fn debug() -> Value {
    ST.with(|s| {
        let s = s.borrow();
        let net = match &s.net {
            Net::Online => json!("online"),
            Net::Offline => json!("offline"),
            Net::Blocked { code, .. } => json!(format!("blocked:{code}")),
        };
        json!({ "net": net, "tab": if is_secondary() { "secondary" } else if is_primary() { "primary" } else { "pending" }, "pending": s.pending, "loaded": s.loaded, "update": s.js["update"], "sw": s.js["sw"], "version": s.js["version"], "controlled": s.js["controlled"],
            "buttons": Value::Object(s.rects.iter().map(|(n, r)| (n.clone(), json!([r.center().x, r.center().y]))).collect()) })
    })
}

const ORANGE: egui::Color32 = egui::Color32::from_rgb(0xe0, 0xa0, 0x60);
const GRAY: egui::Color32 = egui::Color32::from_rgb(0x8b, 0x90, 0x99);

/// 송출 띠 아래의 작은 칸. live = 지금 송출 중 (새 판으로 바꾸기 전에 한 번 묻는다)
pub fn ui(ctx: &egui::Context, live: bool) {
    let (net, pending, loaded, update) = ST.with(|s| {
        let mut s = s.borrow_mut();
        s.rects.clear();
        (s.net.clone(), s.pending, s.loaded, s.js["update"].as_bool().unwrap_or(false))
    });
    let secondary = is_secondary();
    if net == Net::Online && !update && !secondary {
        ST.with(|s| s.borrow_mut().info = false);
        return;
    }
    let note = |name: &str, r: &egui::Response| ST.with(|s| s.borrow_mut().rects.push((name.to_string(), r.rect)));
    egui::Area::new(egui::Id::new("wp-offline-chip")).anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 31.0)).order(egui::Order::Foreground).show(ctx, |ui| {
        ui.horizontal(|ui| {
            let (text, color) = match &net {
                Net::Offline if !loaded => ("연결 끊김 · 교회 자료를 아직 받지 못했습니다".to_string(), ORANGE),
                Net::Offline => (format!("연결 끊김 · 올릴 것 {pending}개"), ORANGE),
                Net::Blocked { msg, .. } => (format!("저장 멈춤 · {msg} · 올릴 것 {pending}개"), GRAY),
                Net::Online => (String::new(), GRAY),
            };
            if !text.is_empty() {
                let r = egui::Frame::new().fill(egui::Color32::from_rgb(0x22, 0x24, 0x28)).stroke(egui::Stroke::new(1.0, color)).corner_radius(10.0).inner_margin(egui::Margin::symmetric(10, 3)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("●").color(color).size(11.0));
                        ui.label(egui::RichText::new(text).size(12.0));
                    });
                });
                let resp = r.response.interact(egui::Sense::click()).on_hover_text("눌러서 자세히");
                note("net", &resp);
                if resp.clicked() {
                    ST.with(|s| {
                        let mut s = s.borrow_mut();
                        s.info = !s.info;
                    });
                }
            }
            if secondary {
                egui::Frame::new().fill(egui::Color32::from_rgb(0x22, 0x24, 0x28)).stroke(egui::Stroke::new(1.0, GRAY)).corner_radius(10.0).inner_margin(egui::Margin::symmetric(10, 3)).show(ui, |ui| {
                    ui.label(egui::RichText::new("보기 전용 · 이 브라우저의 다른 탭이 이 교회 조작 화면을 쓰는 중 (그 탭을 닫으면 이 탭이 이어받음)").size(12.0));
                })
                .response
                .on_hover_text("같은 브라우저에서 두 탭이 함께 송출하거나 고친 것을 올리면 서로 덮어씁니다. 이 탭에서 고친 것은 저장되지 않고, 출력 창 · 클라우드에도 보내지 않습니다.");
            }
            if update {
                egui::Frame::new().fill(egui::Color32::from_rgb(0x22, 0x24, 0x28)).stroke(egui::Stroke::new(1.0, GRAY)).corner_radius(10.0).inner_margin(egui::Margin::symmetric(10, 3)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("새 버전 있음").size(12.0));
                        let b = ui.small_button("바꾸기").on_hover_text("조작 화면을 새 판으로 다시 엽니다. 출력 창은 마지막 화면을 유지합니다.");
                        note("update", &b);
                        if b.clicked() {
                            if live {
                                ST.with(|s| s.borrow_mut().confirm_update = true);
                            } else {
                                apply_update();
                            }
                        }
                    });
                });
            }
        });
    });
    if ST.with(|s| s.borrow().info) && net != Net::Online {
        egui::Area::new(egui::Id::new("wp-offline-info")).anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 58.0)).order(egui::Order::Foreground).show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_max_width(460.0);
                match &net {
                    Net::Blocked { code, .. } => {
                        ui.label("교회 클라우드에는 닿지만 저장소가 고친 것을 받지 않고 있습니다.");
                        if code == "drive_disconnected" {
                            ui.label("주인이 주인 화면(교회 관리)에서 구글 드라이브를 다시 연결하면 모아 둔 것을 올립니다.");
                        } else {
                            ui.label("잠시 뒤 저절로 다시 올려 봅니다.");
                        }
                        ui.label("고친 것은 이 브라우저에 남아 있습니다 (새로 고쳐도 사라지지 않음).");
                    }
                    _ => {
                        ui.label("인터넷이 끊긴 동안");
                        ui.label("· 고친 큐시트 · 찬양은 이 브라우저에 모아 두었다가, 다시 이어지면 올립니다. 그사이 다른 곳에서 같은 것을 고쳤으면 '충돌 사본' 으로 둘 다 남깁니다.");
                        ui.label("· 같은 브라우저에서 연 출력 창은 계속 받습니다. 클라우드 출력 · 리모컨 · OBS 는 마지막 화면을 유지합니다.");
                        ui.label("· 다음 · 이전 같은 조작은 모아 두지 않습니다 — 다시 이어지면 그때의 화면을 보냅니다.");
                        ui.label("· 휴대폰 카메라는 새로 연결할 수 없습니다. 그동안은 설치판의 내부망 휴대폰 카메라를 쓰세요.");
                    }
                }
            });
        });
    }
    if ST.with(|s| s.borrow().confirm_update) {
        let (mut open, mut close) = (true, false);
        egui::Window::new("새 버전으로 바꾸기").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0)).open(&mut open).show(ctx, |ui| {
            ui.label("지금 송출 중입니다. 바꾸면 조작 화면을 다시 엽니다 (몇 초).");
            ui.label("출력 창 · 클라우드 출력은 마지막 화면을 그대로 둡니다. 예배가 끝난 뒤에 바꿔도 됩니다.");
            ui.horizontal(|ui| {
                let b = ui.button("지금 바꾸기");
                note("update-confirm", &b);
                if b.clicked() {
                    apply_update();
                    close = true;
                }
                if ui.button("나중에").clicked() {
                    close = true;
                }
            });
        });
        if close || !open {
            ST.with(|s| s.borrow_mut().confirm_update = false);
        }
    }
}
