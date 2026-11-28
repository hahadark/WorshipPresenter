//! [웹판] 4단계: 웹 조작 화면에 미디어 붙이기 — 4B 부품(docs/web/media.md 7절)을 브라우저 도우미(web/app/media-glue.js)로 부른다.
//!
//! - 카메라: 설치판의 '카메라 찾기'(camscan::probe)는 숨은 도우미 창을 띄우는데, 웹은 이 페이지가 직접
//!   enumerateDevices 로 찾아 같은 길(`Core::receive_camera_devices`, 일회용 토큰)로 넣는다 → 카메라 설정 창 · 라이브러리 카메라 탭 ·
//!   큐시트 카메라 편집칸이 그대로 돈다. 교회 중계실에 붙은 휴대폰(`/rtc/phones`)도 `phone:<id>` 로 함께. 이름(label)은 설정
//!   `cameras[id].label` 에 남아 다른 PC 의 클라우드 출력이 이름으로 찾는다.
//! - 파일: [파일 넣기…](띠) 또는 끌어다 놓기 → 교회 저장소(2C)에 올려 큐시트 순서로 — 그림 · 영상 · 소리(빈 화면 + 배경 음악) ·
//!   PDF(쪽 그림 슬라이드쇼, pdf.js) · 글꼴(교회 글꼴) · PPT(올리기만, 그림은 설치판이).
//! - 휴대폰 카메라: 링크(POST /api/v1/media/phone-link)와 QR, 붙어 있는 휴대폰 목록.
use eframe::egui::{self, Color32, Vec2};
use eframe::wasm_bindgen::{prelude::*, JsCast as _};
use serde_json::{json, Value};
use std::cell::RefCell;
use wp_core::core::{Ev, Shared};

#[derive(Default)]
struct St {
    busy: usize,
    phone_open: bool,
    phone_link: Option<Value>,
    phones: Vec<Value>,
    phones_at: f64,
}

thread_local! {
    static ST: RefCell<St> = RefCell::new(St::default());
}

fn glue() -> Option<JsValue> {
    let w = web_sys::window()?;
    let g = js_sys::Reflect::get(&w, &JsValue::from_str("wpMedia")).ok()?;
    (!g.is_undefined()).then_some(g)
}

/// window.wpMedia[name](...args) → 글 (Promise 면 기다림)
async fn call(name: &str, args: &[JsValue]) -> Option<String> {
    let g = glue()?;
    let f: js_sys::Function = js_sys::Reflect::get(&g, &JsValue::from_str(name)).ok()?.dyn_into().ok()?;
    let arr = js_sys::Array::new();
    for a in args {
        arr.push(a);
    }
    let r = f.apply(&g, &arr).ok()?;
    let r = if r.has_type::<js_sys::Promise>() { wasm_bindgen_futures::JsFuture::from(js_sys::Promise::from(r)).await.ok()? } else { r };
    r.as_string().or_else(|| r.as_f64().map(|n| n.to_string()))
}

pub fn available() -> bool {
    glue().is_some()
}

// ---------------- 카메라 ----------------

/// camscan::probe 의 웹 자리: 토큰을 만들고, 이 페이지가 찾은 카메라 + 붙어 있는 휴대폰을 코어에 넣는다
pub fn probe_cameras(core: Shared) {
    let token = {
        let mut c = core.lock();
        if c.camera_probing() {
            return;
        }
        c.start_camera_probe()
    };
    wasm_bindgen_futures::spawn_local(async move {
        let cams: Value = call("cameras", &[]).await.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_else(|| json!({ "devices": [], "error": "이 브라우저에서 카메라를 찾지 못했습니다." }));
        let mut devices = cams["devices"].as_array().cloned().unwrap_or_default();
        if super::cloud::authed() {
            let ph: Value = call("phones", &[]).await.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(Value::Null);
            for p in ph["phones"].as_array().into_iter().flatten() {
                if let Some(id) = p["id"].as_str() {
                    let name = p["name"].as_str().filter(|s| !s.is_empty()).unwrap_or("이름 없음");
                    devices.push(json!({ "deviceId": format!("phone:{id}"), "label": format!("휴대폰 · {name}"), "kind": "videoinput" }));
                }
            }
        }
        let args = json!({ "token": token, "devices": devices, "error": cams["error"] });
        let mut c = core.lock();
        if let Err(e) = c.receive_camera_devices(&args) {
            c.cancel_camera_probe(&e);
        }
        c.emit(Ev::Ui);
    });
}

/// camscan::preview 의 웹 자리: 4B 의 '이 PC 카메라 맞추기' 화면을 새 탭으로 (그 화면에서 미리 보고 이름을 맞춘다)
pub fn preview_cameras() {
    if let Some(w) = web_sys::window() {
        let _ = w.open_with_url_and_target(if super::cloud::authed() { "/cameras" } else { "about:blank" }, "_blank");
    }
}

// ---------------- 파일 → 순서 ----------------

pub const ACCEPT: &str = ".png,.jpg,.jpeg,.jfif,.gif,.webp,.avif,.bmp,.svg,.mp4,.m4v,.webm,.mov,.ogv,.mp3,.m4a,.aac,.wav,.ogg,.oga,.opus,.flac,.pdf,.ttf,.otf,.ttc,.woff,.woff2,.pptx,.ppsx,.ppt,.pps";

/// [파일 넣기…]: 브라우저 파일 고르기 (누른 직후라야 열린다)
pub fn pick_and_put(core: Shared) {
    wasm_bindgen_futures::spawn_local(async move {
        let hs: Vec<f64> = call("pick", &[JsValue::from_str(ACCEPT), JsValue::TRUE]).await.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        put_all(core, hs).await;
    });
}

/// 끌어다 놓은 파일 (eframe 이 브라우저 파일을 준다 — 바이트는 기다려 읽음) → 같은 길
pub fn dropped(core: Shared, files: Vec<std::sync::Arc<dyn egui::DroppedFile + Send + Sync>>) {
    wasm_bindgen_futures::spawn_local(async move {
        let mut hs = vec![];
        for f in files {
            let name = f.path().to_string_lossy().to_string();
            let Ok(bytes) = f.bytes_async().await else { continue };
            let u8 = js_sys::Uint8Array::from(&bytes[..]);
            let take = glue().and_then(|g| js_sys::Reflect::get(&g, &JsValue::from_str("take")).ok()).and_then(|f| f.dyn_into::<js_sys::Function>().ok());
            if let Some(t) = take {
                if let Ok(h) = t.call3(&JsValue::NULL, &JsValue::from_str(&name), &u8, &JsValue::from_str("")) {
                    if let Some(h) = h.as_f64() {
                        hs.push(h);
                    }
                }
            }
        }
        put_all(core, hs).await;
    });
}

/// 점검용 (?debug): 이미 맡겨 둔 파일 손잡이들을 넣는다 (wpMedia.take 뒤)
pub fn put_handles(core: Shared, hs: Vec<f64>) {
    wasm_bindgen_futures::spawn_local(put_all(core, hs));
}

async fn put_all(core: Shared, hs: Vec<f64>) {
    if hs.is_empty() {
        return;
    }
    ST.with(|s| s.borrow_mut().busy += hs.len());
    core.lock().emit(Ev::Ui);
    let mut added = 0;
    for h in hs {
        let r: Value = call("put", &[JsValue::from_f64(h)]).await.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(json!({ "ok": false, "error": "파일을 넣지 못했습니다." }));
        ST.with(|s| s.borrow_mut().busy -= 1);
        let mut c = core.lock();
        if !r["ok"].as_bool().unwrap_or(false) {
            c.notify(r["error"].as_str().unwrap_or("파일을 넣지 못했습니다.").to_string(), "error");
            continue;
        }
        let item = match r["kind"].as_str().unwrap_or("") {
            "image" | "video" | "pdf" => Some(r["item"].clone()),
            // 소리: 빈 화면 순서의 배경 음악으로 (설치판의 '음악' 칸과 같은 자리)
            "audio" => Some(json!({ "type": "blank", "title": r["title"], "mode": "background", "music": r["url"], "musicVolume": 0.8 })),
            _ => {
                c.notify(r["note"].as_str().unwrap_or("올렸습니다.").to_string(), "info");
                None
            }
        };
        if let Some(mut it) = item {
            if c.cue.is_none() {
                c.notify("열린 큐시트가 없어 순서로 넣지 못했습니다. 큐시트를 먼저 여세요.", "error");
                continue;
            }
            // 빠진 공통 칸은 코어가 새 순서를 만들 때와 같게
            for (k, v) in [("note", json!("")), ("duration", json!(0)), ("tplMain", json!("")), ("tplOverlay", json!(""))] {
                if it.get(k).is_none() {
                    it[k] = v;
                }
            }
            if it.get("id").is_none() {
                it["id"] = json!(wp_core::util::short_id("i"));
            }
            c.add_item(it, None);
            added += 1;
        }
        c.emit(Ev::Ui);
    }
    if added > 0 {
        core.lock().notify(format!("파일 {added}개를 큐시트 끝에 넣었습니다."), "ok");
    }
}

pub fn busy() -> usize {
    ST.with(|s| s.borrow().busy)
}

// ---------------- 휴대폰 카메라 창 ----------------

pub fn open_phone() {
    ST.with(|s| s.borrow_mut().phone_open = true);
}

fn qr(ui: &mut egui::Ui, text: &str, size: f32) {
    let Ok(code) = qrcode::QrCode::new(text.as_bytes()) else { return };
    let w = code.width();
    let colors = code.to_colors();
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 4.0, Color32::WHITE);
    let cell = size / (w as f32 + 4.0);
    for (i, c) in colors.iter().enumerate() {
        if *c == qrcode::Color::Dark {
            let (x, y) = ((i % w) as f32 + 2.0, (i / w) as f32 + 2.0);
            p.rect_filled(egui::Rect::from_min_size(rect.min + Vec2::new(x * cell, y * cell), Vec2::splat(cell + 0.3)), 0.0, Color32::BLACK);
        }
    }
}

fn refresh_phones() {
    ST.with(|s| s.borrow_mut().phones_at = js_sys::Date::now());
    wasm_bindgen_futures::spawn_local(async {
        let v: Value = call("phones", &[]).await.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(Value::Null);
        ST.with(|s| s.borrow_mut().phones = v["phones"].as_array().cloned().unwrap_or_default());
    });
}

pub fn ui(ctx: &egui::Context, core: &Shared) {
    if !ST.with(|s| s.borrow().phone_open) {
        return;
    }
    if js_sys::Date::now() - ST.with(|s| s.borrow().phones_at) > 5000.0 {
        refresh_phones();
    }
    let mut open = true;
    egui::Window::new("휴대폰 카메라").collapsible(false).resizable(false).open(&mut open).show(ctx, |ui| {
        ui.label("휴대폰으로 아래 QR 을 찍어 열면 그 휴대폰이 카메라가 됩니다 (같은 Wi-Fi 권장, 링크는 24시간).");
        let link = ST.with(|s| s.borrow().phone_link.clone());
        match &link {
            Some(l) if l["ok"].as_bool() == Some(true) => {
                let url = l["url"].as_str().unwrap_or("").to_string();
                qr(ui, &url, 200.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(&url).small());
                    if ui.small_button("복사").clicked() {
                        ui.ctx().copy_text(url.clone());
                    }
                });
            }
            Some(l) => {
                ui.colored_label(Color32::from_rgb(0xe0, 0xa0, 0x60), l["error"].as_str().unwrap_or("링크를 만들지 못했습니다."));
            }
            None => {}
        }
        if ui.button(if link.is_some() { "새 링크 만들기" } else { "링크 · QR 만들기" }).clicked() {
            wasm_bindgen_futures::spawn_local(async {
                let v: Value = call("phoneLink", &[JsValue::from_str("")]).await.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(json!({ "ok": false }));
                ST.with(|s| s.borrow_mut().phone_link = Some(v));
            });
        }
        ui.separator();
        let phones = ST.with(|s| s.borrow().phones.clone());
        if phones.is_empty() {
            ui.label(egui::RichText::new("붙어 있는 휴대폰이 없습니다.").weak());
        }
        for p in phones {
            let id = p["id"].as_str().unwrap_or("").to_string();
            let name = p["name"].as_str().filter(|s| !s.is_empty()).unwrap_or("이름 없음").to_string();
            ui.horizontal(|ui| {
                ui.label(format!("휴대폰 · {name}"));
                if ui.small_button("순서로 넣기").clicked() {
                    let did = format!("phone:{id}");
                    let label = format!("휴대폰 · {name}");
                    let mut c = core.lock();
                    // 이름을 설정에 남긴다 (다른 PC 의 클라우드 출력이 이름으로 찾는다 — docs/web/media.md 2절)
                    let prefs = json!({ "label": label });
                    c.set_camera_prefs(&did, Some(&prefs));
                    let it = json!({ "id": wp_core::util::short_id("i"), "type": "camera", "title": label, "note": "", "deviceId": did, "label": label, "fit": "cover", "duration": 0, "tplMain": "", "tplOverlay": "" });
                    if c.cue.is_some() {
                        c.add_item(it, None);
                    } else {
                        c.notify("열린 큐시트가 없습니다.", "error");
                    }
                    c.emit(Ev::Ui);
                }
            });
        }
    });
    if !open {
        ST.with(|s| s.borrow_mut().phone_open = false);
    }
}
