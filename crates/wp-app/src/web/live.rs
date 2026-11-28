//! [웹판] 같은 브라우저(같은 PC)의 출력 화면으로 송출한다 — 계약 docs/web/architecture.md 5절.
//!
//! - `BroadcastChannel('wp-live')` 로 `{t:'render', client, data, church}` · `{t:'draw', data, church}` 를 보낸다.
//!   data 는 설치판 SSE 와 같은 `Core::output_payload` · `stage_payload` · `draw_json` 그대로 → web/output 을 고치지 않는다.
//! - 새 출력 탭이 `{t:'sync', church}` 를 보내면 모든 화면을 다시 보낸다.
//! - 출력 탭(web/app/local-live.js · 클라우드 어댑터)이 `{t:'out', id, client, kind, ack?}` 를 2초마다 보내면
//!   설치판처럼 '연결된 출력' (core.outputs) 에 넣고, 6초 넘게 소식이 없으면 뺀다.
//! - 예배당 화면 켜기 = `window.open('/outputs/main')` (누른 직후에만 열 수 있다 — 브라우저의 팝업 규칙).
//!   다른 모니터가 있고 '창 배치' 권한을 받았으면 그 화면으로 옮긴다. 전체 화면은 출력 창에서 두 번 누르기 · F11.
//! 인터넷이 없어도 같은 브라우저 안에서 돈다 (BroadcastChannel 은 네트워크를 쓰지 않는다).
use crate::outwin::Monitor;
use eframe::wasm_bindgen::{self, prelude::*, JsCast as _};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use wp_core::core::{Ev, OutputClient, Shared};

const CHANNEL: &str = "wp-live";
/// 같은 PC 송출의 교회 표시: 로그인한 교회 ID (둘러보기 · 내 PC 시험은 빈 글). 클라우드 어댑터 · local-live.js 가 같은 교회 것만 받는다
fn church() -> String {
    super::cloud::church()
}

struct Live {
    bc: web_sys::BroadcastChannel,
    core: Shared,
    /// 화면마다 마지막으로 보낸 글 (같으면 다시 보내지 않음)
    last: HashMap<String, String>,
    last_draw: String,
    /// 출력 탭 id → 마지막 소식 (ms)
    seen: HashMap<u64, f64>,
    popup: Option<web_sys::Window>,
}

thread_local! {
    static LIVE: RefCell<Option<Live>> = const { RefCell::new(None) };
}

fn now() -> f64 {
    js_sys::Date::now()
}

/// 지금 송출할 화면들: main · overlay · stage + 설정의 추가 출력
fn clients(core: &wp_core::core::Core) -> Vec<String> {
    let mut v: Vec<String> = ["main", "overlay", "stage"].iter().map(|s| s.to_string()).collect();
    for o in wp_core::outputs::extra_outputs(core.settings.get()) {
        if let Some(id) = o["id"].as_str() {
            v.push(id.to_string());
        }
    }
    v
}

fn post(bc: &web_sys::BroadcastChannel, msg: &Value) {
    // 구조화 복제로 보낸다 (받는 쪽 어댑터가 data 를 그대로 render 이벤트로 넘긴다)
    match js_sys::JSON::parse(&msg.to_string()) {
        Ok(v) => {
            let _ = bc.post_message(&v);
        }
        Err(e) => log::warn!("송출 글을 만들지 못함: {e:?}"),
    }
}

impl Live {
    /// 바뀐 화면만 (force 면 모두)
    fn push(&mut self, force: bool) {
        if !super::offline::is_primary() {
            return; // 6단계: 같은 브라우저의 다른 탭이 송출 중 — 이 탭은 보기 전용
        }
        let (renders, draw) = {
            let c = self.core.lock();
            let r: Vec<(String, Value)> = clients(&c).into_iter().map(|cl| {
                let d = if cl == "stage" { c.stage_payload() } else { c.output_payload(&cl) };
                (cl, d)
            }).collect();
            (r, c.draw_json())
        };
        for (cl, data) in renders {
            let s = data.to_string();
            if !force && self.last.get(&cl) == Some(&s) {
                continue;
            }
            self.last.insert(cl.clone(), s);
            post(&self.bc, &json!({ "t": "render", "client": cl, "data": data, "church": church() }));
            super::cloud::send(&json!({ "t": "render", "client": cl, "data": data })); // 클라우드 출력 (송출 주체일 때만)
        }
        let ds = draw.to_string();
        if force || ds != self.last_draw {
            self.last_draw = ds;
            post(&self.bc, &json!({ "t": "draw", "data": draw, "church": church() }));
            super::cloud::send(&json!({ "t": "draw", "data": draw }));
        }
    }

    fn on_message(&mut self, m: Value) {
        if m["church"].as_str().unwrap_or("") != church() {
            return;
        }
        match m["t"].as_str() {
            Some("sync") => self.push(true),
            Some("out") => {
                let Some(id) = m["id"].as_u64() else { return };
                // 클라우드로도 붙은 출력은 중계실의 viewers 로 센다 (두 번 세지 않게)
                if m["cloud"].as_bool().unwrap_or(false) && super::cloud::active() {
                    return;
                }
                let client = m["client"].as_str().unwrap_or("main").to_string();
                let kind = m["kind"].as_str().unwrap_or("browser").to_string();
                let first = !self.seen.contains_key(&id);
                self.seen.insert(id, now());
                let mut c = self.core.lock();
                let o = c.outputs.entry(id).or_insert_with(|| OutputClient { client, kind, connected_at: now() as i64, ack: None });
                if m["ack"].is_object() {
                    let a = &m["ack"];
                    o.ack = Some(json!({ "version": a["version"], "at": now() as i64, "width": a["width"], "height": a["height"], "visible": a["visible"], "latency": a["latency"] }));
                }
                if first || m["ack"].is_object() {
                    c.emit(Ev::Outputs);
                }
            }
            Some("bye") => {
                if let Some(id) = m["id"].as_u64() {
                    self.seen.remove(&id);
                    let mut c = self.core.lock();
                    c.outputs.remove(&id);
                    c.emit(Ev::Outputs);
                }
            }
            Some("cmd") if super::offline::is_primary() => {
                // 출력 화면의 영상 알림 (설치판 /api/command 의 videoMeta · videoEnded 만)
                let key = m["args"]["key"].as_str().unwrap_or("").to_string();
                let dur = m["args"]["duration"].as_f64();
                let mut c = self.core.lock();
                match m["cmd"].as_str() {
                    Some("videoMeta") => c.video_meta(&key, dur),
                    Some("videoEnded") => c.video_ended(&key, dur),
                    _ => {}
                }
            }
            _ => {}
        }
    }

    /// 소식이 끊긴 출력 탭 빼기
    fn prune(&mut self) {
        let t = now();
        let gone: Vec<u64> = self.seen.iter().filter(|(_, at)| t - **at > 6000.0).map(|(id, _)| *id).collect();
        if gone.is_empty() {
            return;
        }
        let mut c = self.core.lock();
        for id in gone {
            self.seen.remove(&id);
            c.outputs.remove(&id);
        }
        c.emit(Ev::Outputs);
    }
}

/// 시작: 채널을 열고 코어의 송출 알림(Ev::Live · Ev::Draw)을 듣는다
pub fn start(core: Shared) {
    let bc = match web_sys::BroadcastChannel::new(CHANNEL) {
        Ok(b) => b,
        Err(e) => {
            log::warn!("BroadcastChannel 을 열지 못함 — 같은 PC 송출 없음: {e:?}");
            return;
        }
    };
    let onmsg = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(|e: web_sys::MessageEvent| {
        let v: Value = js_sys::JSON::stringify(&e.data()).ok().and_then(|s| s.as_string()).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(Value::Null);
        LIVE.with(|l| {
            if let Some(l) = l.borrow_mut().as_mut() {
                l.on_message(v);
            }
        });
    });
    bc.set_onmessage(Some(onmsg.as_ref().unchecked_ref()));
    onmsg.forget();
    let mut rx = core.lock().tx.subscribe();
    LIVE.with(|l| *l.borrow_mut() = Some(Live { bc, core, last: HashMap::new(), last_draw: String::new(), seen: HashMap::new(), popup: None }));
    with(|l| l.push(true));
    wasm_bindgen_futures::spawn_local(async move {
        loop {
            match rx.recv().await {
                Ok(Ev::Live) | Ok(Ev::Draw) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => with(|l| l.push(false)),
                Ok(_) => {}
                Err(_) => break,
            }
        }
    });
    let tick = Closure::<dyn FnMut()>::new(|| with(|l| l.prune()));
    if let Some(w) = web_sys::window() {
        let _ = w.set_interval_with_callback_and_timeout_and_arguments_0(tick.as_ref().unchecked_ref(), 2000);
    }
    tick.forget();
}

/// 모든 화면을 다시 보낸다 (새 출력 탭 · 클라우드 sync)
pub fn push_all() {
    with(|l| l.push(true));
}

fn with(f: impl FnOnce(&mut Live)) {
    LIVE.with(|l| {
        if let Some(l) = l.borrow_mut().as_mut() {
            f(l);
        }
    });
}

// ---------------- 예배당 화면 창 (outwin.rs 의 웹 자리) ----------------

/// 화면 목록: 브라우저는 '창 배치' 권한 전에는 화면 수만 알려 준다 (screen.isExtended)
pub fn monitors() -> Vec<Monitor> {
    let Some(w) = web_sys::window() else { return vec![] };
    let (sw, sh) = w.screen().map(|s| (s.width().unwrap_or(1920), s.height().unwrap_or(1080))).unwrap_or((1920, 1080));
    let extended = w.screen().ok().and_then(|s| js_sys::Reflect::get(&s, &JsValue::from_str("isExtended")).ok()).and_then(|v| v.as_bool()).unwrap_or(false);
    let mut v = vec![Monitor { index: 0, name: "이 화면".into(), size: (sw, sh), primary: true }];
    if extended {
        v.push(Monitor { index: 1, name: "다른 화면".into(), size: (sw, sh), primary: false });
    }
    v
}

pub fn window_open() -> bool {
    let mut open = false;
    with(|l| {
        if let Some(p) = &l.popup {
            open = !p.closed().unwrap_or(true);
            if !open {
                l.popup = None;
            }
        }
    });
    open
}

/// 출력 창 열기. 팝업이 막히면 오류 글
pub fn open_window(client: &str, monitor: usize, windowed: bool) -> Result<(), String> {
    let w = web_sys::window().ok_or("창 없음")?;
    let url = format!("{}{client}?window=1", super::cloud::outputs_base());
    let feat = if windowed { "popup,width=960,height=540" } else { "popup,width=1280,height=720" };
    let p = w.open_with_url_and_target_and_features(&url, &format!("wp-out-{client}"), feat).map_err(|e| format!("{e:?}"))?;
    let Some(p) = p else { return Err("브라우저가 새 창을 막았습니다. 주소 표시줄의 팝업 허용을 눌러 주세요.".into()) };
    if monitor > 0 && !windowed {
        // 다른 화면으로 옮기기: 창 배치 권한(getScreenDetails)이 있으면. 없으면 브라우저가 묻는다 (한 번 허락하면 다음부터 바로)
        let p2 = p.clone();
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = move_to_other_screen(&p2).await {
                log::info!("출력 창을 다른 화면으로 옮기지 못함: {e:?}");
            }
        });
    }
    with(|l| l.popup = Some(p));
    Ok(())
}

async fn move_to_other_screen(p: &web_sys::Window) -> Result<(), JsValue> {
    let w = web_sys::window().ok_or(JsValue::NULL)?;
    let f = js_sys::Reflect::get(&w, &JsValue::from_str("getScreenDetails"))?;
    let f: js_sys::Function = f.dyn_into()?;
    let det = wasm_bindgen_futures::JsFuture::from(js_sys::Promise::from(f.call0(&w)?)).await?;
    let screens: js_sys::Array = js_sys::Reflect::get(&det, &JsValue::from_str("screens"))?.dyn_into()?;
    let cur = js_sys::Reflect::get(&det, &JsValue::from_str("currentScreen"))?;
    let num = |o: &JsValue, k: &str| js_sys::Reflect::get(o, &JsValue::from_str(k)).ok().and_then(|v| v.as_f64()).unwrap_or(0.0);
    let other = screens.iter().find(|s| num(s, "availLeft") != num(&cur, "availLeft") || num(s, "availTop") != num(&cur, "availTop"));
    if let Some(s) = other {
        p.move_to(num(&s, "availLeft") as i32, num(&s, "availTop") as i32)?;
        p.resize_to(num(&s, "availWidth") as i32, num(&s, "availHeight") as i32)?;
    }
    Ok(())
}

pub fn close_window() {
    with(|l| {
        if let Some(p) = l.popup.take() {
            let _ = p.close();
        }
    });
}
