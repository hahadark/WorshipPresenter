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

fn pick_templates(c: &Core, a: &Value) -> Vec<Value> {
    match a["templates"].as_array() {
        Some(ids) => ids.iter().filter_map(|x| if x.is_object() { Some(x.clone()) } else { x.as_str().and_then(|id| c.settings.template(id)) }).collect(),
        None => vec![c.template_for("main"), c.template_for("overlay")],
    }
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
        "setTemplate" => { c.set_template(a["output"].as_str().unwrap_or("main"), a["id"].as_str().unwrap_or("")); json!(true) }
        // ---- 일괄 나눔 대조 (tools/flow-compare/split-batch.js): 송출 없이 슬라이드만 만든다 ----
        // 장절 → 본문(병행 번역 포함) + 슬라이드. templates: 템플릿 id 목록 (없으면 지금 쓰는 것)
        "bibleBuild" => {
            let v = a["versionId"].as_str().filter(|x| !x.is_empty()).map(String::from).unwrap_or_else(|| c.version_id());
            let res = c.store.resolve_reference(&v, a["query"].as_str().unwrap_or(""));
            if res["ok"] != json!(true) { return Err(res["error"].as_str().unwrap_or("본문을 찾지 못했습니다.").to_string()); }
            let pass = wp_core::flow::with_parallel(c, res["passage"].clone());
            let tpls = pick_templates(c, a);
            let opts = wp_core::slides::SplitOpts { mode: a["mode"].as_str().unwrap_or("auto").into(), count: a["count"].as_u64().unwrap_or(1) as usize, templates: tpls.clone() };
            let slides = wp_core::slides::build_bible(&pass, &opts);
            json!({ "passage": pass, "slides": slides, "templates": tpls })
        }
        // 순서(찬양·공지·교독문 등) → 슬라이드
        "itemBuild" => {
            let tpls = pick_templates(c, a);
            let opts = wp_core::slides::SplitOpts { mode: a["mode"].as_str().unwrap_or("auto").into(), count: a["count"].as_u64().unwrap_or(1) as usize, templates: tpls.clone() };
            json!({ "slides": wp_core::slides::build_item(&a["item"], None, &opts), "templates": tpls })
        }
        // 슬라이드 하나의 높이: (글 덩어리 높이, 본문 영역 높이)
        "measure" => {
            let (h, area) = wp_core::measure::content_height(&a["slide"], &a["tpl"], a["size"].as_f64().unwrap_or(0.0) as f32, a["ver"].as_str().unwrap_or(""));
            json!({ "h": h, "area": area })
        }
        "textWidth" => json!(a["texts"].as_array().into_iter().flatten().map(|x| wp_core::measure::text_width(x.as_str().unwrap_or(""), a["size"].as_f64().unwrap_or(0.0) as f32, a["bold"].as_bool().unwrap_or(false), 0.0)).collect::<Vec<f32>>()),
        "cueItems" => json!(c.cue.as_ref().map(|q| q.items.clone()).unwrap_or_default()),
        "templates" => json!(c.settings.templates()),
        // ---- 조작 흐름 대조 (tools/flow-compare/features.js) ----
        // 순서 송출 (조작 화면의 두 번 누르기와 같은 길: 미리보기 → 송출 → 순서의 '송출할 때 자동으로')
        "sendCueItem" => json!(c.send_item(idx())),
        "saveOverlay" => c.save_overlay(&a["overlay"]).map_err(|e| e.to_string())?,
        "toggleOverlay" => c.toggle_overlay(a["id"].as_str().unwrap_or(""), a["on"].as_bool()).map_err(|e| e.to_string())?,
        "overlays" => json!(c.overlays()),
        "setObsView" => wp_core::themes::set_obs_view(c, a["full"].as_bool(), a["pip"].as_bool()),
        "videoControl" => { c.video_control(a["action"].as_str().unwrap_or(""), a["value"].as_f64().unwrap_or(0.0)).map_err(|e| e.to_string())?; json!(true) }
        "stagePinSet" => { c.stage_pin_set(if a.is_null() || a.as_object().is_some_and(|o| o.is_empty()) { None } else { Some(a) }).map_err(|e| e.to_string())?; json!(true) }
        "saveMacro" => json!(c.save_macro(&a["macro"]).map_err(|e| e.to_string())?),
        "runMacro" => { c.run_macro(a["id"].as_str().or(a["name"].as_str()).unwrap_or("")).map_err(|e| e.to_string())?; json!(true) }
        // 즉석 자막 (조작 화면 trackb_ui.rs quick_send 와 같은 만들기)
        "quickText" => {
            let text = a["text"].as_str().unwrap_or("").replace('\r', "").trim().to_string();
            let opts = c.split_opts(None);
            let slides = wp_core::slides::build_text("", false, &wp_core::slides::text_chunks(&text, 1), &opts);
            let n = slides.len();
            if n == 0 { return Err("송출할 글을 쓰세요.".into()); }
            c.send_deck(wp_core::presentation::Deck { passage: json!({ "title": "즉석 자막", "itemTitle": "즉석 자막", "versionName": "", "cueIndex": -1, "cueId": "", "itemType": "text" }), slides, index: 0 });
            json!(n)
        }
        "listCues" => json!(c.store.list_cues().iter().map(|q| q.id.clone()).collect::<Vec<_>>()),
        "cueOf" => json!(c.store.get_cue(a["id"].as_str().unwrap_or("")).map(|q| q.items).unwrap_or_default()),
        "songs" => json!(c.list_songs("").iter().filter_map(|s| s["id"].as_str().and_then(|id| c.get_song(id))).collect::<Vec<_>>()),
        "plannedTimes" => json!(c.planned_times()),
        "clearMessage" => { c.clear_message(); json!(true) }
        "toggleOverlaysOff" => { c.toggle_overlays_off(); json!(true) }
        // 큐시트 끝에 순서 넣기 (비교용 성경·공지 순서)
        // 대조하며 끝에 넣은 순서들을 걷어 낸다 (앞의 count 개만 남김)
        "truncateItems" => { let keep = a["count"].as_u64().unwrap_or(0) as usize; while c.cue.as_ref().is_some_and(|q| q.items.len() > keep) { let n = c.cue.as_ref().map(|q| q.items.len()).unwrap_or(0); c.remove_item(n - 1); } json!(keep) }
        "addItem" => { let n = c.cue.as_ref().map(|q| q.items.len()).unwrap_or(0); c.add_item(a["item"].clone(), Some(n)); json!(n) }
        // 설정 맨 위 키를 그대로 바꾼다 (split · parallelVersionId 등)
        "updateSettings" => {
            let patch = a["patch"].clone();
            c.settings.update(|d| { if let (Some(d), Some(p)) = (d.as_object_mut(), patch.as_object()) { for (k, v) in p { d.insert(k.clone(), v.clone()); } } });
            json!(true)
        }
        "stageMessage" => { c.stage_message(a["text"].as_str().unwrap_or("")); json!(true) }
        "stageTimer" => { c.stage_timer(a["op"].as_str().unwrap_or(""), a["minutes"].as_f64().unwrap_or(0.0)).map_err(|e| e.to_string())?; json!(true) }
        // ---- 찬송가·PPT 대조 (tools/flow-compare/hymnal.js) ----
        // 두 번 눌러 송출과 같은 길 (그림을 만드는 중이면 false — 다 만들면 이어서 송출)
        "sendItemReal" => json!(c.send_item(idx())),
        "loadHymn" => wp_core::hymns::load(c, a["file"].as_str().unwrap_or(""))?,
        // PPT 요소 따로 꺼내기 (처음에는 '만드는 중' 오류 — 다시 부르면 결과)
        "pptTextLayers" => wp_core::hymns::ppt_text_layers(c, a["file"].as_str().unwrap_or(""), (a["screen"]["w"].as_f64().unwrap_or(1920.0), a["screen"]["h"].as_f64().unwrap_or(1080.0)), a["again"].as_bool().unwrap_or(false))?,
        "isBuilding" => json!(wp_core::hymns::is_building(a["file"].as_str().unwrap_or(""))),
        "toast" => json!(c.toast.as_ref().map(|t| json!({ "text": t.0, "kind": t.1 }))),
        "searchHymns" => json!(wp_core::hymns::search(c, a["query"].as_str().unwrap_or("")).into_iter().map(|h| json!({ "file": h.file.to_string_lossy(), "number": h.number, "title": h.title, "name": h.name, "folder": h.folder })).collect::<Vec<_>>()),
        "hymnalBooks" => json!(wp_core::hymnal::books(c)),
        "hymnalSearch" => json!(wp_core::hymnal::search(c, a["query"].as_str().unwrap_or(""), a["limit"].as_u64().unwrap_or(60) as usize)),
        "hymnalGet" => wp_core::hymnal::get(c, a["book"].as_str().unwrap_or("새찬송가"), a["number"].as_i64().unwrap_or(0))?,
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
    wp_core::hymns::install(&shared); // 찬송가 그림을 다 만들면 미리보기·송출을 잇는다 (앱 main 과 같게)
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
