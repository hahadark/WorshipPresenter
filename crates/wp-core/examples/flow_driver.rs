//! 예배 흐름 비교용 점검 서버 (tools/flow-compare 가 씀). 조작 화면(egui) 없이 코어만 띄운다.
//!
//! `cargo run -p wp-core --example flow_driver --offline -- --data <임시 폴더> [--web <web 폴더>]`
//!
//! - 출력 서버(예배당·OBS·무대 웹 화면)는 앱과 같은 `server::serve` 로 띄운다. 포트는 `<data>\flow-port.txt`.
//! - 조작은 `POST http://127.0.0.1:<포트+1000 근처>/flow` (주소는 `<data>\flow-ctl.txt`) 에 `{ "cmd": …, "args": … }`.
//!   명령 이름은 Electron `core.js`/`cue.js` 와 맞췄다: openCue · slidesFor · sendItem · nextSlide · previousSlide ·
//!   toggleLive · clearLive · restoreLive · clearAll · toggleLogo · toggleBg · showMessage · stageMessage · stageTimer · state.
//! - 사용자 자료는 쓰지 않는다: 반드시 `--data` 를 준다.
use axum::{Json, Router, routing::post};
use parking_lot::Mutex;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;
use wp_core::core::{Core, Shared};

fn arg(name: &str) -> Option<String> {
    let a: Vec<String> = std::env::args().collect();
    a.iter().position(|x| x == name).and_then(|i| a.get(i + 1).cloned())
}

fn deck_json(d: &wp_core::presentation::Deck) -> Value {
    json!({ "passage": d.passage, "slides": d.slides, "index": d.index })
}

fn run(c: &mut Core, cmd: &str, a: &Value) -> Result<Value, String> {
    let idx = || a["index"].as_u64().unwrap_or(0) as usize;
    Ok(match cmd {
        "openCue" => { c.open_cue(a["id"].as_str().unwrap_or("")); json!({ "items": c.cue.as_ref().map(|q| q.items.len()).unwrap_or(0) }) }
        "slidesFor" => deck_json(&c.item_deck(idx())?),
        "previewItem" => json!(c.preview_item(idx())),
        "sendItem" => {
            if !c.preview_item(idx()) { return Err("미리보기 실패".into()); }
            c.send_preview();
            json!(true)
        }
        "nextSlide" => { c.step(1); json!(true) }
        "previousSlide" => { c.step(-1); json!(true) }
        "gotoSlide" => { c.goto_live(idx()); json!(true) }
        "toggleLive" => { c.toggle_clear(); json!(true) }
        "clearLive" => { c.pres.clear(); c.emit(wp_core::core::Ev::Live); json!(true) }
        "restoreLive" => { c.unclear_bg(); c.pres.restore(); c.emit(wp_core::core::Ev::Live); json!(true) }
        "clearAll" => { c.clear_all(); json!(true) }
        "toggleLogo" => { c.toggle_logo(); json!(true) }
        "toggleBg" => { c.toggle_bg(); json!(true) }
        "showMessage" => {
            let outs: Vec<String> = a["outputs"].as_array().map(|v| v.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_else(|| vec!["main".into()]);
            c.show_message(a["text"].as_str().unwrap_or(""), &outs, a["seconds"].as_i64().unwrap_or(0)).map_err(|e| e.to_string())?;
            json!(true)
        }
        "stageMessage" => { c.stage_message(a["text"].as_str().unwrap_or("")); json!(true) }
        "stageTimer" => { c.stage_timer(a["op"].as_str().unwrap_or(""), a["minutes"].as_f64().unwrap_or(0.0)).map_err(|e| e.to_string())?; json!(true) }
        "state" => json!({
            "live": c.pres.live.as_ref().map(|l| json!({ "visible": l.visible, "deck": deck_json(&l.deck) })),
            "preview": c.pres.preview.as_ref().map(deck_json),
            "main": c.output_payload("main"),
            "overlay": c.output_payload("overlay"),
            "stage": c.stage_payload(),
        }),
        _ => return Err(format!("모르는 명령: {cmd}")),
    })
}

fn main() -> anyhow::Result<()> {
    let data = PathBuf::from(arg("--data").ok_or_else(|| anyhow::anyhow!("--data <임시 폴더> 가 필요합니다"))?);
    let web = arg("--web").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("web"));
    let core = Core::open(data.clone(), web, None)?;
    let shared: Shared = Arc::new(Mutex::new(core));
    wp_core::flow::install(&shared);
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build()?;
    let port = rt.block_on(wp_core::server::serve(shared.clone()))?;
    std::fs::write(data.join("flow-port.txt"), port.to_string())?;
    let s2 = shared.clone();
    let app = Router::new().route("/flow", post(move |Json(b): Json<Value>| {
        let s = s2.clone();
        async move {
            let r = tokio::task::spawn_blocking(move || run(&mut s.lock(), b["cmd"].as_str().unwrap_or(""), &b["args"])).await.unwrap();
            Json(match r { Ok(v) => json!({ "ok": true, "result": v }), Err(e) => json!({ "ok": false, "error": e }) })
        }
    }));
    rt.block_on(async move {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let addr = l.local_addr()?;
        std::fs::write(data.join("flow-ctl.txt"), format!("http://{addr}/flow"))?;
        println!("출력 http://127.0.0.1:{port} · 조작 http://{addr}/flow");
        axum::serve(l, app).await?;
        anyhow::Ok(())
    })
}
