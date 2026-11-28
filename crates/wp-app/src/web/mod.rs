//! [웹판] 브라우저(wasm32) 시작점. 설치판의 main() · prepare() 가 하는 일 가운데 브라우저에서 되는 것만 한다.
//! 설계 · 판정 · 남은 일: docs/web/egui-web.md
//!
//! 설치판과 다른 점
//! - 창 → 페이지의 <canvas id="wp_canvas"> (eframe WebRunner, WebGL)
//! - 자료 폴더 → 브라우저 안 저장 (설정 = localStorage, 성경·큐시트·찬양 DB = 메모리 SQLite)
//! - 스레드 → 없음. 0.2초 타이머(flow)는 setInterval, 코어 알림은 같은 스레드의 비동기 일
//! - 로컬 웹 서버 → 없음. 출력 화면은 같은 브라우저의 다른 창으로 (BroadcastChannel)
use crate::{app, imgcache, theme};
use eframe::egui;
use eframe::App as _;
use eframe::wasm_bindgen::{self, prelude::*, JsCast as _};
use std::path::PathBuf;
use std::sync::Arc;
use wp_core::core::{Core, Ev};
use wp_core::lockcheck::Mutex;

pub mod bible;
pub mod cloud;
pub mod fonts;
pub mod live;
pub mod media;
pub mod menu;
pub mod store;

/// 브라우저 안에서 쓰는 가짜 경로 (실제 폴더가 아니라 저장소 열쇠의 앞머리)
pub const DATA_DIR: &str = "/wp/data";
pub const WEB_DIR: &str = "/wp/web";

fn now_ms() -> f64 {
    web_sys::window().and_then(|w| w.performance()).map(|p| p.now()).unwrap_or(0.0)
}

/// 페이지에 진행 상황을 알린다 (index.html 의 window.wpBoot)
fn boot(stage: &str, detail: &str) {
    let Some(w) = web_sys::window() else { return };
    if let Ok(f) = js_sys::Reflect::get(&w, &JsValue::from_str("wpBoot")) {
        if let Some(f) = f.dyn_ref::<js_sys::Function>() {
            let _ = f.call2(&JsValue::NULL, &JsValue::from_str(stage), &JsValue::from_str(detail));
        }
    }
}

/// 같은 곳의 글 파일을 받는다 (견본 자료 등). 없거나 실패하면 None
async fn fetch_text(url: &str) -> Option<String> {
    let w = web_sys::window()?;
    let res = wasm_bindgen_futures::JsFuture::from(w.fetch_with_str(url)).await.ok()?;
    let res: web_sys::Response = res.dyn_into().ok()?;
    if !res.ok() {
        return None;
    }
    wasm_bindgen_futures::JsFuture::from(res.text().ok()?).await.ok()?.as_string()
}

thread_local! {
    /// 점검용으로 페이지에 내보내는 상태 (window.wpDebug())
    static DEBUG: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
    /// 주소에 ?debug 가 있을 때만 점검 상태를 만든다 (프레임마다 드는 일을 평소에는 하지 않게)
    static DEBUG_ON: bool = web_sys::window().and_then(|w| w.location().search().ok()).is_some_and(|s| s.contains("debug"));
    /// 점검용 (?debug): wpMenu(id) 로 넣은 메뉴 항목 — 다음 프레임에 설치판 메뉴 처리기로
    static MENU_Q: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
    static SHARED_DBG: std::cell::RefCell<Option<wp_core::core::Shared>> = const { std::cell::RefCell::new(None) };
}

/// 설치판 App 을 그대로 감싸고, 프레임마다 점검용 상태(검색 칸 글 · 큐시트 순서 글)를 남긴다
struct WebApp(app::App);

impl eframe::App for WebApp {
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        // 끌어다 놓은 파일: 브라우저는 경로 없이 바이트만 준다 → 교회 저장소에 올려 순서로 (설치판 dnd 는 경로를 쓰므로 여기서 가져간다)
        if !raw.dropped_files.is_empty() {
            media::dropped(self.0.core.clone(), std::mem::take(&mut raw.dropped_files));
        }
        self.0.raw_input_hook(ctx, raw);
    }
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        for id in MENU_Q.with(|q| std::mem::take(&mut *q.borrow_mut())) {
            crate::track_c::handle_menu(&mut self.0, &id);
        }
        self.0.ui(ui, frame);
        cloud::ui(ui.ctx()); // 클라우드 상태 띠 (보기 전용 · 넘겨받기 · 교회 관리)
        media::ui(ui.ctx(), &self.0.core); // 휴대폰 카메라 창
        if !DEBUG_ON.with(|d| *d) {
            return;
        }
        let (cue, focus, live) = {
            let g = self.0.core.lock();
            let l = g.pres.live_state();
            let live = serde_json::json!({ "version": l["version"], "hasLive": l["hasLive"], "visible": l["visible"], "title": l["passage"]["title"], "index": l["index"], "outputs": g.outputs.len() });
            (g.cue.as_ref().map(|c| serde_json::json!({ "id": c.id, "items": c.items })), ui.ctx().memory(|m| m.focused()).map(|i| format!("{i:?}")), live)
        };
        let (cams, versions, probing) = {
            let g = self.0.core.lock();
            (crate::camscan::choices(&g).iter().map(|c| json_pair(&c.device_id, &c.label)).collect::<Vec<_>>(), g.store.list_versions().iter().map(|v| v.id.clone()).collect::<Vec<_>>(), g.camera_probing())
        };
        let s = serde_json::json!({ "search": self.0.search, "cue": cue, "focus": focus, "live": live, "cloud": format!("{:?}", cloud::state()), "cloudButtons": cloud::debug_rects(), "cloudTrace": cloud::trace_dump(), "pointer": ui.ctx().input(|i| serde_json::json!({ "down": i.pointer.any_down(), "pos": i.pointer.latest_pos().map(|p| [p.x, p.y]), "drag": i.pointer.is_decidedly_dragging(), "focused": i.focused })), "wantsPtr": ui.ctx().egui_wants_pointer_input(), "frame": ui.ctx().cumulative_frame_nr(), "aiWaiting": wp_core::web::ai::waiting(), "mediaBusy": media::busy(), "cameras": cams, "camProbing": probing, "versions": versions }).to_string();
        DEBUG.with(|d| *d.borrow_mut() = s);
    }
}

fn json_pair(a: &str, b: &str) -> serde_json::Value {
    serde_json::json!([a, b])
}

pub fn start() {
    // 브라우저에서는 오류(panic)가 나면 프로그램 전체가 멈춘다 (설치판처럼 그 프레임만 버릴 수 없다).
    // 콘솔과 화면에 까닭을 남겨, 새로 고침하라고 알린다. 출력 창은 마지막 화면을 그대로 둔다.
    std::panic::set_hook(Box::new(|info| {
        let msg = format!("{info}");
        log::error!("오류로 멈춤: {msg}");
        boot("error", &format!("오류로 조작 화면이 멈췄습니다. 새로 고침(F5 가 아니라 주소 표시줄의 새로 고침)을 눌러 주세요. 출력 창은 마지막 화면을 유지합니다.

{msg}"));
        if let Some(d) = web_sys::window().and_then(|w| w.document()).and_then(|d| d.get_element_by_id("boot")) {
            let _ = d.class_list().remove_1("done");
        }
    }));
    {
        let f = Closure::<dyn Fn() -> String>::new(|| DEBUG.with(|d| d.borrow().clone()));
        if let Some(w) = web_sys::window() {
            let _ = js_sys::Reflect::set(&w, &JsValue::from_str("wpDebug"), f.as_ref());
        }
        f.forget();
    }
    if DEBUG_ON.with(|d| *d) {
        // 점검용 (?debug): wpAiTest('cuecheck') → 설치판과 같은 aihelp::chat 길(run_bg → 웹은 /api/v1/ai/chat)로 한 번 부르고, 답을 wpAiResult 에
        let f = Closure::<dyn Fn(String)>::new(|feature: String| {
            let system = match feature.as_str() {
                "lyrics" => wp_core::lyricsclean::ai_system(),
                "recap" => wp_core::recap::summary_system(),
                _ => wp_core::cuecheck::ai_system(),
            };
            wp_core::hymns::run_bg(
                move || wp_core::aihelp::chat(&wp_core::aihelp::AiCfg::default(), &system, "점검: 요 3:16 · 찬양 두 곡", true, 200),
                |_core, r| {
                    let v = match r { Ok((t, u)) => serde_json::json!({ "ok": true, "text": t, "usage": u }), Err(e) => serde_json::json!({ "ok": false, "error": e }) };
                    if let Some(w) = web_sys::window() {
                        let _ = js_sys::Reflect::set(&w, &JsValue::from_str("wpAiResult"), &JsValue::from_str(&v.to_string()));
                    }
                },
            );
        });
        if let Some(w) = web_sys::window() {
            let _ = js_sys::Reflect::set(&w, &JsValue::from_str("wpAiTest"), f.as_ref());
        }
        f.forget();
        // wpMenu('c:scenes') · wpSceneMake('split') → 구성 id · wpScene('main', id, hold) → 송출 구성 바꾸기 (설치판 trigger_scene 과 같은 길)
        let m = Closure::<dyn Fn(String)>::new(|id: String| MENU_Q.with(|q| q.borrow_mut().push(id)));
        let mk = Closure::<dyn Fn(String) -> String>::new(|kind: String| {
            SHARED_DBG.with(|s| s.borrow().as_ref().map(|sh| {
                let n = wp_core::scenes::new_scene(&kind, "", None);
                sh.lock().save_scene(&n).map(|v| v["id"].as_str().unwrap_or("").to_string()).unwrap_or_else(|e| format!("오류: {e}"))
            })).unwrap_or_default()
        });
        let sc = Closure::<dyn Fn(String, String, bool) -> String>::new(|out: String, id: String, hold: bool| {
            SHARED_DBG.with(|s| s.borrow().as_ref().map(|sh| sh.lock().trigger_scene(&out, &id, hold).unwrap_or_else(|e| format!("오류: {e}")))).unwrap_or_default()
        });
        if let Some(w) = web_sys::window() {
            let _ = js_sys::Reflect::set(&w, &JsValue::from_str("wpMenu"), m.as_ref());
            let _ = js_sys::Reflect::set(&w, &JsValue::from_str("wpSceneMake"), mk.as_ref());
            let _ = js_sys::Reflect::set(&w, &JsValue::from_str("wpScene"), sc.as_ref());
        }
        m.forget();
        mk.forget();
        sc.forget();
        // wpProbeCameras() · wpPutFiles([손잡이…]) · wpAddItem(json) — 카메라 찾기 · 파일 넣기 · 순서 넣기 (화면 단추와 같은 길)
        let pc = Closure::<dyn Fn()>::new(|| SHARED_DBG.with(|s| if let Some(sh) = s.borrow().as_ref() { crate::camscan::probe(sh) }));
        let pf = Closure::<dyn Fn(String)>::new(|hs: String| {
            let hs: Vec<f64> = serde_json::from_str(&hs).unwrap_or_default();
            SHARED_DBG.with(|s| if let Some(sh) = s.borrow().as_ref() { media::put_handles(sh.clone(), hs) });
        });
        let ai = Closure::<dyn Fn(String)>::new(|it: String| {
            SHARED_DBG.with(|s| if let Some(sh) = s.borrow().as_ref() { if let Ok(v) = serde_json::from_str(&it) { let mut c = sh.lock(); c.add_item(v, None); c.emit(Ev::Ui); } });
        });
        if let Some(w) = web_sys::window() {
            let _ = js_sys::Reflect::set(&w, &JsValue::from_str("wpProbeCameras"), pc.as_ref());
            let _ = js_sys::Reflect::set(&w, &JsValue::from_str("wpPutFiles"), pf.as_ref());
            let _ = js_sys::Reflect::set(&w, &JsValue::from_str("wpAddItem"), ai.as_ref());
        }
        let aw = Closure::<dyn Fn() -> f64>::new(|| wp_core::web::ai::waiting() as f64);
        // 프레임과 상관없이 지금 상태 (화면이 다시 그려지지 않아도 보임)
        let ct = Closure::<dyn Fn() -> String>::new(|| serde_json::json!({ "state": format!("{:?}", cloud::state()), "trace": cloud::trace_dump() }).to_string());
        if let Some(w) = web_sys::window() {
            let _ = js_sys::Reflect::set(&w, &JsValue::from_str("wpCloudNow"), ct.as_ref());
        }
        ct.forget();
        let oc = Closure::<dyn Fn(String)>::new(|id: String| SHARED_DBG.with(|s| if let Some(sh) = s.borrow().as_ref() { sh.lock().open_cue(&id) }));
        // wpSaveSource(json) → 인스펙터 소스 탭의 [더하기] 와 같은 길 (Core::save_source)
        let ss = Closure::<dyn Fn(String) -> String>::new(|v: String| SHARED_DBG.with(|s| s.borrow().as_ref().map(|sh| {
            let v: serde_json::Value = serde_json::from_str(&v).unwrap_or_default();
            let mut c = sh.lock();
            let r = c.save_source(&v).map(|x| x["id"].as_str().unwrap_or("").to_string()).unwrap_or_else(|e| format!("오류: {e}"));
            c.emit(Ev::Live);
            r
        })).unwrap_or_default());
        if let Some(w) = web_sys::window() {
            let _ = js_sys::Reflect::set(&w, &JsValue::from_str("wpSaveSource"), ss.as_ref());
        }
        ss.forget();
        if let Some(w) = web_sys::window() {
            let _ = js_sys::Reflect::set(&w, &JsValue::from_str("wpAiWaiting"), aw.as_ref());
            let _ = js_sys::Reflect::set(&w, &JsValue::from_str("wpOpenCue"), oc.as_ref());
        }
        aw.forget();
        oc.forget();
        pc.forget();
        pf.forget();
        ai.forget();
    }
    eframe::WebLogger::init(log::LevelFilter::Info).ok();
    let t0 = now_ms();
    // 어디서 열었나: 로그인한 클라우드(/control/) · 둘러보기(/demo/) · 내 PC 시험(tools/web/serve.js)
    let authed = cloud::authed();
    wp_core::web::ai::enable(authed && cloud::can("ai"));
    // 자료 폴더 이름(브라우저 저장소 열쇠 앞머리)은 교회마다 따로 — 같은 브라우저에서 교회 · 둘러보기 설정이 섞이지 않게
    let data_dir = if authed { format!("/wp/c/{}/data", cloud::church()) } else { DATA_DIR.to_string() };
    wasm_bindgen_futures::spawn_local(async move {
        boot("progress", "글꼴 받는 중…");
        let font_bytes = fonts::load_first("").await;
        // 견본 자료: 로그인하지 않았을 때만 (브라우저 안 DB 가 비었을 때 넣는다 — wp_core::web::seed)
        let seed: Option<serde_json::Value> = if authed { None } else { fetch_text("sample/seed.json").await.and_then(|t| serde_json::from_str(&t).ok()) };
        log::info!("글꼴 {:.1}MB 받음 ({:.0}ms)", font_bytes as f64 / 1048576.0, now_ms() - t0);
        let canvas = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("wp_canvas"))
            .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok());
        let Some(canvas) = canvas else {
            boot("error", "페이지에 <canvas id=\"wp_canvas\"> 가 없습니다");
            return;
        };
        let r = eframe::WebRunner::new()
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(move |cc| {
                    let t1 = now_ms();
                    theme::install_fonts(&cc.egui_ctx);
                    egui_extras::install_image_loaders(&cc.egui_ctx);
                    imgcache::install(&cc.egui_ctx);
                    theme::apply(&cc.egui_ctx, theme::ThemeId::Pro);
                    let t2 = now_ms();
                    let core = Core::open(PathBuf::from(&data_dir), PathBuf::from(WEB_DIR), None).map_err(|e| format!("{e:#}"))?;
                    let mut core = core;
                    if let Some(seed) = &seed {
                        match wp_core::web::seed::apply(&mut core, seed) {
                            Ok(n) => log::info!("견본 자료: 번역본 {} · 찬양 {} · 큐시트 {}", n.0, n.1, n.2),
                            Err(e) => log::warn!("견본 자료를 넣지 못함: {e:#}"),
                        }
                    }
                    if !authed {
                        store::restore(&mut core); // 고친 큐시트 · 찬양 (브라우저 저장소) 가 견본보다 앞선다
                    }
                    let shared = Arc::new(Mutex::new(core));
                    wp_core::hymns::install(&shared); // 백그라운드 일(run_bg)이 끝나면 코어를 다시 잡으려고 (웹은 다음 차례에 같은 스레드)
                    cloud::set_core(shared.clone());
                    cloud::set_ctx(cc.egui_ctx.clone());
                    SHARED_DBG.with(|s| *s.borrow_mut() = Some(shared.clone()));
                    if authed {
                        store::cloud_start(shared.clone()); // 큐시트 · 찬양: /api/v1/documents · sync
                        bible::start(shared.clone()); // 성경: 교회 번역본 (IndexedDB 에 보관)
                        cloud::start_bridge(shared.clone()); // 송출 주체 (/bridge, 주인만)
                    } else {
                        store::start(shared.clone()); // 둘러보기 · 시험: localStorage
                    }
                    fonts::load_later(String::new(), cc.egui_ctx.clone());
                    live::start(shared.clone()); // 같은 브라우저의 출력 창으로 송출 (BroadcastChannel 'wp-live')
                    {
                        // 송출 중인가 (페이지가 닫힐 때 묻기 — index.html beforeunload)
                        let c = shared.clone();
                        let f = Closure::<dyn Fn() -> bool>::new(move || c.try_lock().is_some_and(|g| g.pres.live.as_ref().is_some_and(|l| l.visible)));
                        if let Some(w) = web_sys::window() {
                            let _ = js_sys::Reflect::set(&w, &JsValue::from_str("wpIsLive"), f.as_ref());
                        }
                        f.forget();
                        // 웹 기본 단축키: 즉석 자막 Ctrl+T 는 브라우저가 '새 탭' 으로 가져가 막을 수 없다 → 처음 한 번 Ctrl+Shift+Y 로
                        let mut g = shared.lock();
                        if g.settings.get()["shortcuts"]["quickText"].is_null() {
                            g.settings.update(|d| d["shortcuts"]["quickText"] = serde_json::json!("Ctrl+Shift+Y"));
                        }
                    }
                    wp_core::i18n::set_lang(shared.lock().settings.get()["uiLang"].as_str().unwrap_or("ko"));
                    // 코어가 바뀌면 화면을 다시 그린다 (설치판은 스레드, 여기는 같은 스레드의 비동기 일)
                    let mut rx = shared.lock().tx.subscribe();
                    let ctx = cc.egui_ctx.clone();
                    wasm_bindgen_futures::spawn_local(async move {
                        loop {
                            match rx.recv().await {
                                Ok(Ev::Ui) | Ok(Ev::Live) | Ok(Ev::Outputs) | Ok(Ev::Draw) => ctx.request_repaint(),
                                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => ctx.request_repaint(),
                                Err(_) => break,
                            }
                        }
                    });
                    // 자동 넘김 · 글자 자동 숨김 · 메시지 만료 (설치판 flow::install 의 0.2초 스레드)
                    {
                        let c = shared.clone();
                        let tick = Closure::<dyn FnMut()>::new(move || {
                            if let Some(mut g) = c.try_lock() {
                                wp_core::flow::tick(&mut g, wp_core::util::now_ms());
                            }
                        });
                        if let Some(w) = web_sys::window() {
                            let _ = w.set_interval_with_callback_and_timeout_and_arguments_0(tick.as_ref().unchecked_ref(), 200);
                        }
                        tick.forget();
                    }
                    let app = app::App::new(shared, PathBuf::from(WEB_DIR), PathBuf::from(&data_dir));
                    let t3 = now_ms();
                    log::info!("웹 시작: wasm 시작→창 {:.0}ms · 글꼴 {:.0}ms · 자료·화면 {:.0}ms", t1 - t0, t2 - t1, t3 - t2);
                    boot("ready", &format!("{:.0}", t3));
                    Ok(Box::new(WebApp(app)))
                }),
            )
            .await;
        if let Err(e) = r {
            boot("error", &format!("{e:?}"));
        }
    });
}
