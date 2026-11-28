//! 점검용 `--parity <cases.json> --parity-out <폴더>`: 조작 화면의 슬라이드 그림(paint_slide)을 창 전체에 하나씩 그려
//! `rust-<이름>.png` 로 저장하고, 같은 경우를 웹 출력(render.js)으로 그릴 수 있게 `resolved.json`(템플릿·슬라이드)을 남긴다.
//! 웹 출력과 나란히 비교해 위치·정렬·크기·배경이 같은지 확인하는 데 쓴다. 켜지 않으면 아무것도 안 한다.
//!
//! cases.json: `[{ "name": "notice-br", "item": 13, "slide": 0, "output": "main", "tpl": "템플릿 id"(생략 가능),
//!               "patch": { 템플릿에 덮어쓸 값 }, "slideJson": { 슬라이드를 직접 }, "slidePatch": {…}, "hideBg": false, "logo": false }]`

use crate::app::App;
use eframe::egui;
use parking_lot::Mutex;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

struct St {
    out: PathBuf,
    cases: Vec<Value>,
    idx: usize,
    shown_at: Instant,
    asked: bool,
}

static STATE: once_cell::sync::Lazy<Mutex<Option<St>>> = once_cell::sync::Lazy::new(|| Mutex::new(None));
static SAVED: AtomicBool = AtomicBool::new(false);
static INIT: AtomicBool = AtomicBool::new(false);

fn arg(name: &str) -> Option<String> {
    let a: Vec<String> = std::env::args().collect();
    a.iter().position(|x| x == name).and_then(|i| a.get(i + 1).cloned())
}

/// 점검 모드가 켜졌는가
pub fn active() -> bool {
    static ON: once_cell::sync::Lazy<bool> = once_cell::sync::Lazy::new(|| arg("--parity").is_some());
    *ON
}

/// JSON 깊은 덮어쓰기 (객체는 안쪽까지, 나머지는 바꿔 넣기)
pub fn merge(dst: &mut Value, patch: &Value) {
    if patch.is_null() {
        return;
    }
    match (dst, patch) {
        (Value::Object(d), Value::Object(p)) => {
            for (k, v) in p {
                merge(d.entry(k.clone()).or_insert(Value::Null), v);
            }
        }
        (d, p) => *d = p.clone(),
    }
}

fn resolve(app: &App, cases: &[Value]) -> Vec<Value> {
    let mut out = vec![];
    let mut c = app.core.lock();
    for case in cases {
        let output = case["output"].as_str().unwrap_or("main");
        let (mut slide, deck) = if case["slideJson"].is_object() {
            (case["slideJson"].clone(), None)
        } else if let Some(i) = case["item"].as_u64() {
            let k = case["slide"].as_u64().unwrap_or(0) as usize;
            c.preview_item_at(i as usize, k);
            // 바로 송출이면 미리보기 없이 LIVE 로 간다
            let d = c.pres.preview.clone().or_else(|| c.pres.live.as_ref().map(|l| l.deck.clone()));
            (d.as_ref().and_then(|d| d.slides.get(k).cloned()).unwrap_or(Value::Null), d)
        } else {
            (Value::Null, None)
        };
        merge(&mut slide, &case["slidePatch"]);
        let mut tpl = match case["tpl"].as_str() {
            Some(id) => c.settings.template(id).unwrap_or(Value::Null),
            None => c.template_for_deck(output, deck.as_ref()),
        };
        merge(&mut tpl, &case["patch"]);
        let ver = case["versionName"].as_str().map(str::to_string).unwrap_or_else(|| deck.as_ref().and_then(|d| d.passage["versionName"].as_str()).unwrap_or("").to_string());
        out.push(json!({
            "name": case["name"], "output": output, "template": tpl, "slide": slide, "versionName": ver,
            "hideBg": case["hideBg"].as_bool().unwrap_or(false), "logo": case["logo"].as_bool().unwrap_or(false),
            "pipTemplate": if case["pip"].as_bool().unwrap_or(false) { c.template_for_deck("main", deck.as_ref()) } else { Value::Null },
            // 화면 구성 (로드맵 2단계): 칸의 tplId 를 템플릿 값으로 (출력 데이터의 layer.template 와 같게)
            "scene": if case["scene"].is_object() {
                let mut sc = case["scene"].clone();
                for l in sc["layers"].as_array_mut().into_iter().flatten() {
                    if let Some(t) = l["tplId"].as_str().and_then(|id| c.settings.template(id)) {
                        l["template"] = t;
                    }
                }
                sc
            } else { Value::Null },
        }));
    }
    out
}

/// 본 그림의 맞춘 크기를 먼저 읽고(read) 그 뒤에 덧그림(pip)을 그린다 — 차례를 한곳에 못 박아 둔다
fn fit_before_pip(read: impl FnOnce() -> f32, pip: impl FnOnce()) -> f32 {
    let fit = read();
    pip();
    fit
}

#[cfg(test)]
mod tests {
    /// 덧그림(pip)이 '바로 앞 크기' 를 덮어써도 본 그림의 크기가 남아야 한다
    #[test]
    fn fit_is_read_before_the_pip_overwrites_it() {
        let last = std::cell::Cell::new(56.0f32);
        let fit = super::fit_before_pip(|| last.replace(0.0), || last.set(72.0));
        assert_eq!(fit, 56.0);
        assert_eq!(last.get(), 72.0, "pip 이 남긴 값은 부르는 쪽이 버린다");
    }
}

/// 점검 모드면 슬라이드 하나를 창 전체에 그리고 true (그 프레임은 다른 화면을 그리지 않는다)
pub fn run(app: &App, ui: &mut egui::Ui) -> bool {
    if !active() {
        return false;
    }
    let ctx = ui.ctx().clone();
    if !INIT.swap(true, Ordering::SeqCst) {
        let out = PathBuf::from(arg("--parity-out").unwrap_or_else(|| "parity-out".into()));
        let _ = std::fs::create_dir_all(&out);
        let cases: Vec<Value> = arg("--parity").and_then(|p| std::fs::read_to_string(p).ok()).and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        let resolved = resolve(app, &cases);
        let _ = std::fs::write(out.join("resolved.json"), serde_json::to_string_pretty(&resolved).unwrap_or_default());
        let _ = std::fs::write(out.join("port.txt"), app.core.lock().port.to_string());
        *STATE.lock() = Some(St { out, cases: resolved, idx: 0, shown_at: Instant::now(), asked: false });
    }
    let mut g = STATE.lock();
    let Some(st) = g.as_mut() else { return true };
    if SAVED.swap(false, Ordering::SeqCst) {
        st.idx += 1;
        st.shown_at = Instant::now();
        st.asked = false;
    }
    if st.idx >= st.cases.len() {
        let _ = std::fs::write(st.out.join("done.txt"), "ok");
        // --parity-hold: 웹 쪽 비교가 끝날 때까지(close.txt) 서버를 띄워 둔다
        let hold = std::env::args().any(|x| x == "--parity-hold");
        if !hold || st.out.join("close.txt").exists() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(500));
        return true;
    }
    let case = st.cases[st.idx].clone();
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0.0, egui::Color32::BLACK);
    let c = crate::paint::Ctx { web_dir: &app.web_dir, data_dir: &app.data_dir };
    let slide = case["slide"].is_object().then_some(&case["slide"]);
    if case["scene"].is_object() {
        crate::paint::paint_scene(ui, &c, rect, &case["scene"], slide, &case["template"], case["versionName"].as_str().unwrap_or(""), true, case["logo"].as_bool().unwrap_or(false), case["hideBg"].as_bool().unwrap_or(false), &|s: &str| s.to_string());
    } else {
        crate::paint::paint_slide(ui, &c, rect, slide, &case["template"], case["versionName"].as_str().unwrap_or(""), true, case["logo"].as_bool().unwrap_or(false), case["hideBg"].as_bool().unwrap_or(false));
    }
    // 맞춘 글자 크기는 본 그림(자막)의 것 — 웹 쪽 parityRender 도 본 그림의 fit 크기를 돌려준다.
    // 그림 우상단(pip)을 그린 뒤에 읽으면 pip 안의 글(예: 제목 슬라이드 72)이 덮어써서 '72/56' 처럼 다르게 보였다
    // (2.0.3~2.1.1 의 obs-pip-score · obs-pip-image '차이'. 그림은 같았고 재는 자리만 틀렸다)
    let fit = fit_before_pip(|| crate::paint::last_fit(), || {
        if case["pipTemplate"].is_object() {
            let mut sl = case["slide"].clone();
            sl["caption"] = json!("");
            crate::paint::paint_pip(ui, &c, rect, &sl, &case["pipTemplate"]);
        }
    });
    let _ = crate::paint::take_hits();
    let _ = crate::paint::last_fit(); // pip 이 남긴 값은 버린다 (다음 경우로 새지 않게)
    if !st.asked && st.shown_at.elapsed().as_secs_f32() > 1.5 {
        st.asked = true;
        let path = st.out.join(format!("rust-{}.png", case["name"].as_str().unwrap_or("case")));
        // 고른 글자 크기를 fit.txt 에 (웹 fit 크기와 비교)
        let fit_log = ();
        if let Ok(mut fh) = std::fs::OpenOptions::new().create(true).append(true).open(st.out.join("fit.txt")) {
            use std::io::Write;
            let _ = writeln!(fh, "{} {}", case["name"].as_str().unwrap_or(""), fit);
        }
        let c2 = ctx.clone();
        ctx.request_screenshot(move |img| {
            let [w, h] = img.size;
            let px: Vec<u8> = img.pixels.iter().flat_map(|p| p.to_array()).collect();
            if let Some(buf) = image::RgbaImage::from_raw(w as u32, h as u32, px) {
                let _ = buf.save(&path);
            }
            SAVED.store(true, Ordering::SeqCst);
            let _ = fit_log;
            c2.request_repaint();
        });
    }
    ctx.request_repaint();
    true
}
