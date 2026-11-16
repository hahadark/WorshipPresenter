//! [점검 A] 외부 연동 명령: `POST /api/command` (머리글 `X-WorshipCue: 1`) — Electron 판 core.js `command()` 와 같은 이름·인자·응답 봉투.
//!
//! Electron 판 README 가 외부 연동 길로 적어 둔 것: "외부 연동 명령: POST http://127.0.0.1:37528/api/command (헤더 X-WorshipCue: 1)
//! — listCues, getCue, saveCue, nextSlide, toggleLive …". Rust 판은 조작 화면이 웹이 아니라서 이 길을 '영상 끝남' 만 받게 막아 두었는데,
//! 그러면 Electron 판에 맞춰 만든 바깥 도구(스크립트·Companion 일반 HTTP 단추·자동화)가 2.0 으로 올린 뒤 조용히 멈춘다.
//!
//! 보안 경계는 Electron 판과 같다(넓히지 않는다): 조작 서버는 127.0.0.1 에만 열리고, Host 이름을 검사하고(server.rs host_check),
//! `X-WorshipCue: 1` 머리글이 있어야 한다(다른 웹사이트의 교차 출처 요청은 사전 확인에서 막힌다). 내부망 출력 서버·휴대폰 리모컨 서버에는 이 길이 없다.
//! 받는 명령은 Electron 판 189개 가운데 송출·무대·오버레이·큐시트 읽기/저장처럼 바깥에서 쓸 만한 것만이다(파일 열기·설정 통째 바꾸기·AI·가져오기는 받지 않는다).
use crate::core::{Core, Ev};
use crate::db::Cue;
use serde_json::{json, Value};

/// Electron 판 명령 이름 전부 (core.js commands). 여기 있는데 아래 `run` 이 모르면 '지원하지 않음', 여기도 없으면 '알 수 없는 명령'.
const ELECTRON_COMMANDS: &str = "drawStroke drawUndo drawClear drawSpot parallelVerses searchReference searchText getStructure getChapter getRange adjacentVerse getBooks previewReference setPreview setPreviewIndex sendLive outputOff nextSlide previousSlide triggerInfo newTriggerKey gotoSlide extendLive clearLive restoreLive toggleLive getLiveState getState setTemplate saveTemplate duplicateTemplate listThemes applyTheme ensureTheme deleteTemplate setObsView updateStatus updateAction lanOutputInfo newLanKey setCameraPrefs updateSettings setShortcuts getDisplays setOutputWindow deviceRun saveDevice deleteDevice companionPress setCompanion imeKorean setStageWindow toggleOutputWindow getGlobalShortcutStatus clearHistory listVersions analyzeImport commitImport openBethlehemSetup findBethlehem importBethlehem importBethlehemHymns hymnalBooks hymnalGet listHymnFolders addHymnFolder removeHymnFolder searchHymns loadHymn setHymnFolderRender songOnlineSearch songInboxStatus hymnLyricsStatus addDroppedFiles clipboardInfo pasteNow pasteTextPlan pasteSlides uiReady logClientError openLink aiSettings setAi quickMakeCue aiMakeCue chatAiPrompt chatAiImport preServiceInfo preServiceCards preServiceBundle preServiceQt preServiceItem homeFeedInfo homeFeed homeMakeCue homeNewsItem sermonPointsFind sermonPointsAI sermonPointsPrompt sermonPointsImport sermonPointsItem pptTextLayers pptSplitRule pptSplitAI pptSplitPrompt pptSplitImport pptSplitSheets usageLog usageCsv clearUsageLog syncInfo setSyncDir syncNow backups backupNow restoreBackup cancelRestore openPath obsStatus setObs obsAction diagnose logBundle remoteStatus setRemote setBackgroundMedia toggleBackground toggleLogoScreen toggleOverlaysOff clearAll stageTimer saveMacro saveSchedule deleteMacro runMacro showMessage clearMessage saveMessageTemplate deleteMessageTemplate stagePinSet saveStagePreset deleteStagePreset stageMessage videoControl videoEnded listSongs importSongs parseSongText lyricsSearch lyricsGet lyricsArrange saveImportedSongs getSong saveSong saveSongArrangement deleteSong listLibFolders addLibFolder removeLibFolder searchLibFiles listOverlays saveOverlay deleteOverlay toggleOverlay saveWorkspaces setActiveWorkspace saveDockLayout songInboxStop pickFolder listCues updateCueMeta trashCues restoreCues purgeCues exportCues importCues cueText getCue saveCue newCue duplicateCue deleteCue setActiveCue listCommentaries analyzeCommentary commitCommentary deleteCommentary commentaryFor deleteVersion";

/// 이 판이 받는 명령 (문서·시험용)
pub const SUPPORTED: &[&str] = &[
    "nextSlide", "previousSlide", "gotoSlide", "setPreview", "previewReference", "sendLive", "clearLive", "restoreLive", "toggleLive", "getLiveState", "getState", "clearAll", "toggleBackground", "toggleLogoScreen",
    "toggleOverlaysOff", "listOverlays", "toggleOverlay", "stageMessage", "stageTimer", "stagePinSet", "showMessage", "clearMessage", "setObsView", "setTemplate", "runMacro", "videoControl",
    "drawClear", "searchReference", "triggerInfo", "remoteStatus", "listCues", "getCue", "saveCue", "videoEnded", "videoMeta", "cameraDevices",
];

/// Electron `command()` 이 내는 결과: Ok(result) → `{ok:true,result,liveVersion}`, Raw(v) → v 그대로(명령이 `{ok:false,…}` 를 돌려준 것), Err → `{ok:false,error,liveVersion}`
pub enum Out {
    Ok(Value),
    Raw(Value),
}

/// `/api/state` 와 `getState`: Electron `getState()` 의 맨 위 키를 채운다. 출력 화면용 키(`output_payload("main")`: 카메라 도우미가 `cameras` 를 읽는다)도 그대로 둔다.
/// 비밀(외부 단추 열쇠 · 리모컨 PIN · 내부망 열쇠 · AI 키 · 동기화 폴더 · OBS 암호)이 든 설정은 통째로 주지 않고, 바깥 도구가 볼 만한 것만 준다
/// (Electron 판은 설정 전부를 준다 — 여기서는 더 좁다).
pub fn state_json(c: &Core) -> Value {
    let mut v = c.output_payload("main");
    let s = c.settings.get();
    let keep = ["schema", "port", "versionId", "parallelVersionId", "searchMode", "split", "navMode", "activeTemplates", "activeCueId", "cueAdvance", "cueEdgeHide", "cueTimes", "uiLang", "previewMode"];
    let mut settings = serde_json::Map::new();
    for k in keep {
        if let Some(x) = s.get(k) {
            settings.insert(k.into(), x.clone());
        }
    }
    settings.insert("output".into(), json!({ "enabled": s["output"]["enabled"], "autoOnSend": s["output"]["autoOnSend"], "linkObs": s["output"]["linkObs"], "obsPip": s["output"]["obsPip"] }));
    settings.insert("templates".into(), json!(c.settings.templates().iter().map(|t| json!({ "id": t["id"], "name": t["name"] })).collect::<Vec<_>>()));
    settings.insert("overlays".into(), json!(c.overlays()));
    let extra = json!({
        "obsView": crate::themes::obs_view(c),
        "live": c.pres.live_state(),
        "stage": c.stage_payload(),
        "layers": c.layers_payload("main"),
        "screen": Value::Null,
        "preview": c.pres.preview.as_ref().map(|d| json!({ "passage": d.passage, "index": d.index, "count": d.slides.len() })),
        "versions": [],
        "commentaries": [],
        "settings": settings,
        "shortcutLabels": {},
        "defaultShortcuts": {},
        "outputs": c.output_status(),
        "obs": Value::Null,
        "message": c.b.message.as_ref().map(|_| json!(true)),
        "port": c.port,
        "portWarning": "",
        "dataDir": "",
        "app": "rust",
    });
    if let (Some(o), Some(e)) = (v.as_object_mut(), extra.as_object()) {
        for (k, x) in e {
            o.insert(k.clone(), x.clone());
        }
    }
    v
}

fn cue_json(q: &Cue) -> Value {
    serde_json::to_value(q).unwrap_or(Value::Null)
}

fn err<T>(m: impl Into<String>) -> Result<T, String> {
    Err(m.into())
}

/// 명령 하나 실행
pub fn run(c: &mut Core, cmd: &str, a: &Value) -> Result<Out, String> {
    let live = |c: &Core| c.pres.live_state();
    let step = |c: &mut Core, d: i64| {
        if c.pres.live.is_none() {
            return Out::Raw(json!({ "ok": false, "reason": "no-live", "state": c.pres.live_state() }));
        }
        // 순서 끝에서는 코어가 다음 순서로 넘긴다 (Electron 은 조작 화면이 넘기고 여기서는 {ok:false,reason:'end'} 를 낸다)
        c.step(d);
        Out::Ok(json!({ "ok": true, "state": c.pres.live_state() }))
    };
    Ok(Out::Ok(match cmd {
        // ---- 송출 ----
        "nextSlide" => return Ok(step(c, 1)),
        "previousSlide" => return Ok(step(c, -1)),
        "gotoSlide" => {
            if c.pres.live.is_none() {
                return Ok(Out::Raw(json!({ "ok": false, "reason": "no-live" })));
            }
            c.goto_live(a["index"].as_f64().unwrap_or(0.0).max(0.0) as usize);
            json!({ "ok": true, "state": live(c) })
        }
        // 미리보기에 올리기: 슬라이드는 부르는 쪽이 만들어 준다 (Electron presentation.setPreview({ passage, slides, index }))
        "setPreview" | "previewReference" => {
            let d = deck_of(a)?;
            let (index, count) = (d.index, d.slides.len());
            c.pres.set_preview(d.passage, d.slides, d.index);
            c.emit(Ev::Ui);
            json!({ "index": index, "count": count })
        }
        // 송출: 슬라이드를 주면 그것을, 안 주면 미리보기에 있는 것을 (Electron sendLive)
        "sendLive" => {
            if a["slides"].is_array() {
                let d = deck_of(a)?;
                c.send_deck(d);
            } else {
                if c.pres.preview.is_none() {
                    return err("미리보기에 본문이 없습니다.");
                }
                c.send_preview();
            }
            live(c)
        }
        "clearLive" => {
            if c.pres.live.as_ref().is_some_and(|l| l.visible) {
                c.toggle_clear();
            }
            live(c)
        }
        "restoreLive" => {
            if c.pres.live.as_ref().is_some_and(|l| !l.visible) {
                c.toggle_clear();
            }
            live(c)
        }
        "toggleLive" => {
            if c.pres.live.is_some() {
                c.toggle_clear();
            }
            live(c)
        }
        "getLiveState" => live(c),
        "getState" => state_json(c),
        "clearAll" => {
            c.clear_all();
            c.layers_payload("main")
        }
        "toggleBackground" => {
            c.toggle_bg();
            c.layers_payload("main")
        }
        "toggleLogoScreen" => {
            c.toggle_logo();
            c.layers_payload("main")
        }
        "setTemplate" => {
            let slot = a["output"].as_str().unwrap_or("");
            let id = a["templateId"].as_str().unwrap_or("");
            if !["main", "overlay"].contains(&slot) {
                return err("출력 대상이 올바르지 않습니다.");
            }
            if !c.settings.templates().iter().any(|t| t["id"] == id) {
                return err("템플릿을 찾을 수 없습니다.");
            }
            c.set_template(slot, id);
            json!({ "output": slot, "templateId": id })
        }
        "setObsView" => crate::themes::set_obs_view(c, a["full"].as_bool(), a["pip"].as_bool()),
        // ---- 오버레이 · 회중 메시지 ----
        "toggleOverlaysOff" => {
            c.toggle_overlays_off();
            c.layers_payload("main")
        }
        "listOverlays" => json!(c.overlays()),
        "toggleOverlay" => c.toggle_overlay(a["id"].as_str().unwrap_or(""), a["on"].as_bool()).map_err(|e| e.to_string())?,
        "showMessage" => {
            let outs: Vec<String> = a["outputs"].as_array().map(|v| v.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_else(|| vec!["main".into()]);
            let text = a["text"].as_str().unwrap_or("");
            c.show_message(text, &outs, a["seconds"].as_i64().unwrap_or(0)).map_err(|e| e.to_string())?;
            json!({ "text": text.trim(), "outputs": outs })
        }
        "clearMessage" => {
            c.clear_message();
            json!({})
        }
        // ---- 무대 화면 ----
        "stageMessage" => {
            c.stage_message(a["text"].as_str().unwrap_or(""));
            json!({ "message": c.b.stage.message })
        }
        "stageTimer" => {
            c.stage_timer(a["action"].as_str().unwrap_or(""), a["minutes"].as_f64().unwrap_or(0.0)).map_err(|e| e.to_string())?;
            c.b.stage.timer_json()
        }
        "stagePinSet" => {
            let none = a.is_null() || a.as_object().is_some_and(|o| o.is_empty());
            c.stage_pin_set(if none { None } else { Some(a) }).map_err(|e| e.to_string())?;
            c.stage_pin_json()
        }
        // ---- 매크로 · 영상 · 그리기 ----
        "runMacro" => {
            c.run_macro(a["id"].as_str().filter(|x| !x.is_empty()).or(a["name"].as_str()).unwrap_or("")).map_err(|e| e.to_string())?;
            json!(true)
        }
        "videoControl" => {
            c.video_control(a["action"].as_str().unwrap_or(""), a["value"].as_f64().unwrap_or(0.0)).map_err(|e| e.to_string())?;
            c.video_payload()
        }
        // 출력 화면의 메타데이터: key 는 현재 재생 키, duration 은 양수 초 단위 길이.
        "videoMeta" => {
            c.video_meta(a["key"].as_str().unwrap_or(""), a["duration"].as_f64());
            Value::Null
        }
        "videoEnded" => {
            c.video_ended(a["key"].as_str().unwrap_or(""), a["duration"].as_f64());
            Value::Null
        }
        "drawClear" => {
            c.draw_clear();
            json!(true)
        }
        // ---- 읽기 ----
        "searchReference" => {
            let v = a["versionId"].as_str().filter(|x| !x.is_empty()).map(String::from).unwrap_or_else(|| c.version_id());
            let r = c.store.resolve_reference(&v, a["query"].as_str().unwrap_or(""));
            if r["ok"] == json!(false) {
                return Ok(Out::Raw(r));
            }
            r
        }
        "triggerInfo" => crate::trigger::info(c),
        "remoteStatus" => crate::remote::status(c),
        // ---- 큐시트 ----
        // Electron db.js listCues 와 같은 모양 (count · preview 는 앞 40개 순서의 종류·제목)
        "listCues" => {
            let list = c.store.list_cues();
            json!(list.iter().map(|s| {
                let q = c.store.get_cue(&s.id);
                let items = q.as_ref().map(|q| q.items.as_slice()).unwrap_or(&[]);
                let preview: Vec<Value> = items.iter().take(40).map(|x| {
                    let sub = x["ref"].as_str().filter(|t| !t.is_empty()).or(x["subtitle"].as_str().filter(|t| !t.is_empty())).map(String::from)
                        .or_else(|| x.get("number").filter(|n| !n.is_null()).map(|n| format!("{}장", n.as_str().map(String::from).unwrap_or_else(|| n.to_string())))).unwrap_or_default();
                    json!({ "type": x["type"], "title": x["title"].as_str().unwrap_or(""), "sub": sub })
                }).collect();
                json!({ "id": s.id, "name": s.name, "serviceDate": s.service_date, "startTime": q.as_ref().map(|q| q.start_time.clone()).unwrap_or_default(), "updatedAt": s.updated_at, "createdAt": "",
                    "count": s.count, "memo": q.as_ref().map(|q| q.memo.clone()).unwrap_or_default(), "pinned": s.pinned, "template": s.template, "deletedAt": "", "preview": preview })
            }).collect::<Vec<_>>())
        }
        "getCue" => c.store.get_cue(a["id"].as_str().unwrap_or("")).map(|q| cue_json(&q)).ok_or("큐시트를 찾을 수 없습니다.")?,
        "saveCue" => save_cue(c, &a["cue"])?,
        _ => {
            return if ELECTRON_COMMANDS.split(' ').any(|n| n == cmd) {
                err(crate::i18n::tr_args("이 명령은 2.0 판의 외부 연동에서 받지 않습니다: {1}", &[&cmd]))
            } else {
                err(crate::i18n::tr_args("알 수 없는 명령: {1}", &[&cmd]))
            };
        }
    }))
}

/// 큐시트 저장 (Electron normalizeCue 의 뼈대: 이름 · 날짜 · 시작 시각 · 순서 배열). 있던 큐시트는 안 준 값(메모·고정·서식)을 지킨다.
/// 조작 화면에 열려 있는 큐시트면 열린 것도 새로 읽는다. Electron 과 달리 열려 있는 큐시트를 바꾸지는 않는다(조작 화면은 사람이 쓰고 있다).
fn save_cue(c: &mut Core, v: &Value) -> Result<Value, String> {
    let items = v["items"].as_array().ok_or("큐시트 형식이 올바르지 않습니다.")?;
    if items.len() > 2000 || items.iter().any(|it| !it.is_object() || it["type"].as_str().is_none_or(|t| t.is_empty())) {
        return err("큐시트 형식이 올바르지 않습니다.");
    }
    let name: String = v["name"].as_str().unwrap_or("").trim().chars().take(200).collect();
    if name.is_empty() {
        return err("큐시트 이름을 입력하세요.");
    }
    let id = v["id"].as_str().filter(|x| !x.is_empty() && x.len() <= 64 && x.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')).map(String::from);
    let old = id.as_deref().and_then(|id| c.store.get_cue(id));
    let st = |k: &str, d: &str| v[k].as_str().map(String::from).unwrap_or_else(|| d.to_string());
    let cue = Cue {
        id: id.unwrap_or_else(|| crate::util::short_id("c")),
        name,
        service_date: st("serviceDate", old.as_ref().map(|o| o.service_date.as_str()).unwrap_or("")),
        start_time: st("startTime", old.as_ref().map(|o| o.start_time.as_str()).unwrap_or("")),
        items: items.clone(),
        memo: st("memo", old.as_ref().map(|o| o.memo.as_str()).unwrap_or("")),
        pinned: v["pinned"].as_bool().unwrap_or(old.as_ref().is_some_and(|o| o.pinned)),
        template: v["template"].as_bool().unwrap_or(old.as_ref().is_some_and(|o| o.template)),
    };
    c.store.save_cue(&cue).map_err(|e| e.to_string())?;
    c.cues = c.store.list_cues();
    if c.cue.as_ref().is_some_and(|q| q.id == cue.id) {
        c.cue = c.store.get_cue(&cue.id);
    }
    c.emit(Ev::Ui);
    Ok(cue_json(&cue))
}

/// 글자 수로 자르기 (Electron String.slice)
fn cut(v: &Value, n: usize) -> String {
    let t = match v { Value::String(t) => t.clone(), Value::Number(x) => x.to_string(), _ => String::new() };
    t.chars().take(n).collect()
}

/// 바깥에서 준 슬라이드 하나를 Electron presentation.js validateSlides 와 같은 모양으로 (아는 키만 남기고 기본값을 채운다).
/// 슬라이드 편집(edit)은 바깥에서 받지 않는다.
fn slide_of(s: &Value, i: usize) -> Result<Value, String> {
    const KINDS: [&str; 8] = ["bible", "text", "title", "image", "blank", "score", "video", "camera"];
    let kind = s["kind"].as_str().filter(|k| KINDS.contains(k)).unwrap_or("bible");
    let bad = || format!("슬라이드 {} 형식 오류", i + 1);
    let parts = s["parts"].as_array().ok_or_else(bad)?;
    if parts.is_empty() && (kind == "bible" || kind == "text") {
        return Err(bad());
    }
    let num = |v: &Value| v.as_f64().map(|x| json!(x as i64)).unwrap_or(json!(0));
    let mut out = json!({
        "kind": kind, "label": cut(&s["label"], 200), "subtitle": cut(&s["subtitle"], 300), "image": cut(&s["image"], 300), "bg": cut(&s["bg"], 300),
        "mode": if s["mode"] == "background" { "background" } else { "black" }, "ref": cut(&s["ref"], 300), "title": cut(&s["title"], 300),
    });
    if let Some(t) = s["caption"].as_str() { out["caption"] = json!(t.chars().take(2000).collect::<String>()); }
    if kind == "camera" {
        out["camera"] = json!({ "deviceId": cut(&s["camera"]["deviceId"], 200), "label": cut(&s["camera"]["label"], 100), "fit": if s["camera"]["fit"] == "contain" { "contain" } else { "cover" } });
    }
    if kind == "video" {
        let end = s["video"]["endAction"].as_str().filter(|e| ["hold", "black", "next"].contains(e)).unwrap_or("hold");
        out["video"] = json!({ "volume": s["video"]["volume"].as_f64().unwrap_or(0.0).clamp(0.0, 1.0), "sound": s["video"]["sound"].as_bool().unwrap_or(false), "loop": s["video"]["loop"].as_bool().unwrap_or(false), "endAction": end });
    }
    if s["footer"].as_str().is_some_and(|t| !t.is_empty()) { out["footer"] = json!(cut(&s["footer"], 300)); }
    if let Some(t) = s["countdown"]["target"].as_str() {
        let b = t.as_bytes();
        let ok = matches!(b.len(), 4 | 5) && b[b.len() - 3] == b':' && b.iter().enumerate().all(|(k, ch)| k == b.len() - 3 || ch.is_ascii_digit());
        if ok { out["countdown"] = json!({ "target": t, "label": cut(&s["countdown"]["label"], 60) }); }
    }
    if let Some(n) = s["src"].as_u64() { out["src"] = json!(n); }
    let mut ps = Vec::with_capacity(parts.len());
    for p in parts {
        let text = match &p["text"] { Value::String(t) => t.clone(), Value::Null => String::new(), v => v.to_string() };
        if text.chars().count() > 5000 {
            return Err("본문이 너무 깁니다.".into());
        }
        let mut q = json!({ "book": num(&p["book"]), "chapter": num(&p["chapter"]), "verse": num(&p["verse"]), "text": text,
            "partIndex": num(&p["partIndex"]), "partCount": num(&p["partCount"]), "heading": p["heading"].as_str().unwrap_or("") });
        if let Some(t) = p["text2"].as_str().filter(|t| !t.is_empty()) { q["text2"] = json!(t.chars().take(5000).collect::<String>()); }
        ps.push(q);
    }
    out["parts"] = json!(ps);
    Ok(out)
}

/// 바깥에서 준 미리보기·송출 덩어리 (passage · slides · index): Electron validateSlides · validatePassage 와 같은 규칙 (2000장까지).
fn deck_of(a: &Value) -> Result<crate::presentation::Deck, String> {
    let raw = a["slides"].as_array().filter(|s| !s.is_empty()).ok_or("슬라이드가 없습니다.")?;
    if raw.len() > 2000 {
        return err("슬라이드가 너무 많습니다.");
    }
    let slides = raw.iter().enumerate().map(|(i, s)| slide_of(s, i)).collect::<Result<Vec<_>, _>>()?;
    let p = &a["passage"];
    if !p.is_object() {
        return err("본문 정보가 없습니다.");
    }
    let st = |k: &str, n: usize| cut(&p[k], n);
    let pos = |v: &Value| if v.is_object() { json!({ "chapter": v["chapter"].as_i64().unwrap_or(0), "verse": v["verse"].as_i64().unwrap_or(0) }) } else { Value::Null };
    let auto = p["auto"]["sec"].as_f64().filter(|x| *x >= 2.0).map(|x| json!({ "sec": (x.round() as i64).min(3600), "loop": p["auto"]["loop"].as_bool().unwrap_or(false) })).unwrap_or(Value::Null);
    // 큐시트 순서가 아닌 송출: 순서 넘기기·순서의 자동 동작이 붙지 않게 큐시트 자리는 바깥에서 받지 않는다 (Electron 은 받는다 — 여기서는 더 좁다)
    let passage = json!({
        "title": st("title", 300), "versionId": st("versionId", 100), "versionName": st("versionName", 100),
        "book": if p["book"].is_object() { json!({ "id": p["book"]["id"].as_i64().unwrap_or(0), "name": cut(&p["book"]["name"], 100), "short": cut(&p["book"]["short"], 100) }) } else { Value::Null },
        "first": pos(&p["first"]), "last": pos(&p["last"]), "query": st("query", 300),
        "cueId": "", "cueIndex": -1, "itemType": st("itemType", 20), "lyrics": p["lyrics"].as_bool().unwrap_or(false), "credit": st("credit", 300),
        "itemTitle": st("itemTitle", 300), "note": st("note", 1000),
        "tpl": if p["tpl"].is_object() { json!({ "main": cut(&p["tpl"]["main"], 100), "overlay": cut(&p["tpl"]["overlay"], 100) }) } else { Value::Null },
        "auto": auto, "hideAfter": p["hideAfter"].as_f64().filter(|x| *x >= 1.0).map(|x| (x.round() as i64).min(3600)).unwrap_or(0), "edgeHide": p["edgeHide"].as_bool().unwrap_or(false),
    });
    let index = (a["index"].as_f64().unwrap_or(0.0).max(0.0) as usize).min(slides.len() - 1);
    Ok(crate::presentation::Deck { passage, slides, index })
}

/// 응답 봉투 (Electron command(): ok · result · liveVersion · error)
pub fn respond(c: &mut Core, cmd: &str, args: &Value) -> Value {
    if !matches!(cmd, "cameraDevices" | "videoMeta") && !ELECTRON_COMMANDS.split(' ').any(|n| n == cmd) {
        return json!({ "ok": false, "error": crate::i18n::tr_args("알 수 없는 명령: {1}", &[&cmd]) }); // Electron: liveVersion 없이
    }
    let out = run(c, cmd, args);
    let ver = c.pres.version;
    match out {
        Ok(Out::Ok(result)) => json!({ "ok": true, "result": result, "liveVersion": ver }),
        Ok(Out::Raw(v)) => v,
        Err(e) => json!({ "ok": false, "error": e, "liveVersion": ver }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn core() -> Core {
        let dir = std::env::temp_dir().join(format!("wp-extcmd-{}-{}", crate::util::now_ms(), rand::random::<u32>()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("bible.db"), b"").unwrap(); // 빈 DB (기존 자료 복사 안 함)
        Core::open(dir.clone(), dir, None).unwrap()
    }

    #[test]
    fn envelope_like_electron() {
        let mut c = core();
        // 없는 명령 · Electron 에는 있지만 받지 않는 명령
        let r = respond(&mut c, "noSuchCommand", &json!({}));
        assert_eq!(r["ok"], false);
        assert_eq!(r["error"], "알 수 없는 명령: noSuchCommand");
        let r = respond(&mut c, "openPath", &json!({ "path": "C:\\" }));
        assert_eq!(r["ok"], false);
        assert!(r["error"].as_str().unwrap().contains("openPath"));
        for n in ["updateSettings", "setRemote", "newTriggerKey", "setAi", "restoreBackup", "addDroppedFiles", "pickFolder", "deleteCue"] {
            assert!(!SUPPORTED.contains(&n), "{n} 은 바깥에서 받지 않는다");
            assert_eq!(respond(&mut c, n, &json!({}))["ok"], false);
        }
        for n in SUPPORTED.iter().filter(|n| !matches!(**n, "cameraDevices" | "videoMeta")) {
            assert!(ELECTRON_COMMANDS.split(' ').any(|x| x == *n), "{n} 은 Electron 에 없는 이름");
        }
        // 송출 중이 아닐 때: Electron 처럼 결과가 그대로 {ok:false, reason}
        let r = respond(&mut c, "nextSlide", &json!({}));
        assert_eq!((r["ok"].clone(), r["reason"].clone()), (json!(false), json!("no-live")));
        assert_eq!(respond(&mut c, "gotoSlide", &json!({ "index": 3 }))["reason"], "no-live");
        let r = respond(&mut c, "toggleLive", &json!({}));
        assert_eq!((r["ok"].clone(), r["result"]["hasLive"].clone()), (json!(true), json!(false)));
        assert!(r["liveVersion"].is_number());
    }

    /// 바깥에서 만든 슬라이드를 미리보기·송출 (Electron sendLive({ passage, slides, index }))
    #[test]
    fn send_custom_slides() {
        let mut c = core();
        let slide = |t: &str| json!({ "kind": "text", "ref": "", "label": "1", "parts": [{ "book": 0, "chapter": 0, "verse": 0, "text": t }] });
        assert_eq!(respond(&mut c, "sendLive", &json!({}))["ok"], false, "미리보기가 비었으면 오류");
        assert_eq!(respond(&mut c, "sendLive", &json!({ "slides": [] }))["ok"], false);
        assert_eq!(respond(&mut c, "sendLive", &json!({ "slides": ["글"] }))["ok"], false);
        let r = respond(&mut c, "sendLive", &json!({ "passage": { "title": "바깥 자막", "cueId": "꾸민 것", "cueIndex": 3 }, "slides": [slide("하나"), slide("둘")], "index": 0 }));
        assert_eq!((r["ok"].clone(), r["result"]["count"].clone(), r["result"]["index"].clone()), (json!(true), json!(2), json!(0)), "{r}");
        assert_eq!(r["result"]["passage"]["cueIndex"], -1, "큐시트 자리는 바깥에서 못 정한다");
        assert_eq!(respond(&mut c, "nextSlide", &json!({}))["result"]["state"]["index"], 1);
        assert_eq!(respond(&mut c, "gotoSlide", &json!({ "index": 0 }))["result"]["state"]["index"], 0);
        assert_eq!(respond(&mut c, "clearLive", &json!({}))["result"]["visible"], false);
        assert_eq!(respond(&mut c, "clearLive", &json!({}))["result"]["visible"], false, "두 번 숨겨도 숨김");
        assert_eq!(respond(&mut c, "restoreLive", &json!({}))["result"]["visible"], true);
        assert_eq!(respond(&mut c, "toggleLive", &json!({}))["result"]["visible"], false);
        // 미리보기에 올린 뒤 송출 (결과는 Electron 처럼 { index, count })
        let r = respond(&mut c, "setPreview", &json!({ "passage": { "title": "다음 것" }, "slides": [slide("셋")] }));
        assert_eq!((r["ok"].clone(), r["result"]["index"].clone(), r["result"]["count"].clone()), (json!(true), json!(0), json!(1)));
        let r = respond(&mut c, "sendLive", &json!({}));
        assert_eq!((r["result"]["count"].clone(), r["result"]["visible"].clone()), (json!(1), json!(true)));
    }

    /// 바깥 슬라이드는 Electron validateSlides 처럼 아는 키만 남기고 기본값을 채운다
    #[test]
    fn custom_slides_are_normalized() {
        let mut c = core();
        let r = respond(&mut c, "sendLive", &json!({ "passage": { "title": "가", "auto": { "sec": 1 }, "hideAfter": 4000, "모르는 키": 1 },
            "slides": [{ "kind": "이상한 종류", "label": 3, "parts": [{ "text": "본문", "text2": "second" }], "edit": { "boxes": [] }, "onclick": "x", "mode": "background",
                "countdown": { "target": "9:30", "label": "예배" }, "video": { "volume": 9 } },
              { "kind": "video", "image": "/library/a.mp4", "parts": [], "video": { "volume": 9, "endAction": "지움" } }] }));
        assert_eq!(r["ok"], true, "{r}");
        let s = &r["result"]["slide"];
        assert_eq!((s["kind"].clone(), s["label"].clone(), s["mode"].clone(), s["subtitle"].clone(), s["bg"].clone()), (json!("bible"), json!("3"), json!("background"), json!(""), json!("")));
        assert!(s.get("edit").is_none() && s.get("onclick").is_none() && s.get("video").is_none());
        assert_eq!(s["countdown"]["target"], "9:30");
        assert_eq!(s["parts"][0], json!({ "book": 0, "chapter": 0, "verse": 0, "text": "본문", "partIndex": 0, "partCount": 0, "heading": "", "text2": "second" }));
        let n = &r["result"]["nextSlide"];
        assert_eq!(n["video"], json!({ "volume": 1.0, "sound": false, "loop": false, "endAction": "hold" }));
        let p = &r["result"]["passage"];
        assert_eq!((p["auto"].clone(), p["hideAfter"].clone(), p["tpl"].clone(), p["cueIndex"].clone()), (Value::Null, json!(3600), Value::Null, json!(-1)));
        assert!(p.get("모르는 키").is_none());
        // 형식이 틀린 것
        assert_eq!(respond(&mut c, "sendLive", &json!({ "passage": {}, "slides": [{ "kind": "text", "parts": [] }] }))["error"], "슬라이드 1 형식 오류");
        assert_eq!(respond(&mut c, "sendLive", &json!({ "slides": [{ "kind": "title", "parts": [] }] }))["error"], "본문 정보가 없습니다.");
        assert_eq!(respond(&mut c, "sendLive", &json!({ "passage": {}, "slides": [{ "kind": "text", "parts": [{ "text": "가".repeat(5001) }] }] }))["error"], "본문이 너무 깁니다.");
        assert_eq!(respond(&mut c, "sendLive", &json!({ "passage": {}, "slides": [{ "kind": "title", "parts": [] }] }))["ok"], true);
    }

    #[test]
    fn video_metadata_remembers_duration_and_rejects_stale_or_invalid() {
        let mut c = core();
        respond(&mut c, "sendLive", &json!({ "passage": {}, "slides": [{ "kind": "video", "image": "/media/test.mp4", "parts": [] }] }));
        c.sync_video_state();
        let key = c.b.video.as_ref().unwrap().key.clone();
        assert_eq!(c.video_duration("/media/test.mp4"), None);
        respond(&mut c, "videoMeta", &json!({ "key": "old", "duration": 90.0 }));
        assert_eq!(c.video_duration("/media/test.mp4"), None);
        respond(&mut c, "videoMeta", &json!({ "key": key, "duration": 125.5 }));
        assert_eq!(c.video_duration("/media/test.mp4"), Some(125.5));
        for d in [None, Some(0.0), Some(-1.0), Some(f64::NAN), Some(f64::INFINITY)] {
            c.video_meta(&key, d);
        }
        assert_eq!(c.video_duration("/media/test.mp4"), Some(125.5));
        c.b.video = None;
        c.video_meta(&key, Some(30.0));
        assert_eq!(c.video_duration("/media/test.mp4"), Some(125.5));
        assert_eq!(c.video_duration("/media/other.mp4"), None);
    }
    #[test]
    fn stage_overlay_commands() {
        let mut c = core();
        assert_eq!(respond(&mut c, "stageMessage", &json!({ "text": "5분 남음" }))["result"]["message"], "5분 남음");
        let r = respond(&mut c, "stageTimer", &json!({ "action": "start", "minutes": 2 }));
        assert_eq!((r["result"]["running"].clone(), r["result"]["duration"].clone()), (json!(true), json!(120_000)));
        assert_eq!(respond(&mut c, "stageTimer", &json!({ "action": "reset" }))["result"]["duration"], 0);
        assert_eq!(respond(&mut c, "stageTimer", &json!({ "action": "nope" }))["ok"], false);
        let r = respond(&mut c, "setObsView", &json!({ "full": true }));
        assert_eq!(r["result"]["full"], true);
        assert_eq!(respond(&mut c, "setObsView", &json!({ "full": false }))["result"]["full"], false);
        assert_eq!(respond(&mut c, "toggleBackground", &json!({}))["result"]["hideBg"], true);
        assert_eq!(respond(&mut c, "toggleBackground", &json!({}))["result"]["hideBg"], false);
        assert_eq!(respond(&mut c, "toggleLogoScreen", &json!({}))["result"]["logo"], true);
        assert_eq!(respond(&mut c, "clearAll", &json!({}))["result"]["logo"], false);
        assert_eq!(respond(&mut c, "toggleOverlay", &json!({ "id": "없는 것" }))["ok"], false);
        assert_eq!(respond(&mut c, "runMacro", &json!({ "name": "없는 매크로" }))["ok"], false);
        assert_eq!(respond(&mut c, "setTemplate", &json!({ "output": "stage", "templateId": "worship" }))["ok"], false);
        assert_eq!(respond(&mut c, "setTemplate", &json!({ "output": "main", "templateId": "없는 템플릿" }))["ok"], false);
        assert_eq!(respond(&mut c, "stagePinSet", &json!({ "mode": "order", "name": "순서표" }))["result"]["mode"], "order");
        assert!(respond(&mut c, "stagePinSet", &json!({}))["result"].is_null());
    }

    #[test]
    fn cue_roundtrip_and_state() {
        let mut c = core();
        // 형식이 틀린 것은 거절
        assert_eq!(respond(&mut c, "saveCue", &json!({ "cue": { "name": "", "items": [] } }))["ok"], false);
        assert_eq!(respond(&mut c, "saveCue", &json!({ "cue": { "name": "가", "items": [{ "title": "종류 없음" }] } }))["ok"], false);
        assert_eq!(respond(&mut c, "saveCue", &json!({ "cue": { "name": "가" } }))["ok"], false);
        let r = respond(&mut c, "saveCue", &json!({ "cue": { "name": "외부에서 만든 큐시트", "serviceDate": "2026-11-22", "items": [{ "id": "i1", "type": "title", "title": "예배로 부름" }] } }));
        assert_eq!(r["ok"], true, "{r}");
        let id = r["result"]["id"].as_str().unwrap().to_string();
        let list = respond(&mut c, "listCues", &json!({}));
        assert!(list["result"].as_array().unwrap().iter().any(|q| q["id"] == id.as_str() && q["count"] == 1));
        // 열어 둔 큐시트를 바깥에서 고치면 열린 것도 바뀐다 · 안 준 메모는 지킨다
        c.open_cue(&id);
        let mut q = respond(&mut c, "getCue", &json!({ "id": id }))["result"].clone();
        assert_eq!(q["items"][0]["title"], "예배로 부름");
        q["items"].as_array_mut().unwrap().push(json!({ "id": "i2", "type": "blank", "title": "끝" }));
        q["memo"] = json!("메모");
        assert_eq!(respond(&mut c, "saveCue", &json!({ "cue": q }))["ok"], true);
        assert_eq!(c.cue.as_ref().unwrap().items.len(), 2);
        let mut q2 = respond(&mut c, "getCue", &json!({ "id": id }))["result"].clone();
        q2.as_object_mut().unwrap().remove("memo");
        respond(&mut c, "saveCue", &json!({ "cue": q2 }));
        assert_eq!(c.store.get_cue(&id).unwrap().memo, "메모");
        assert_eq!(respond(&mut c, "getCue", &json!({ "id": "없는 것" }))["error"], "큐시트를 찾을 수 없습니다.");
        let l = respond(&mut c, "listCues", &json!({}))["result"][0].clone();
        assert_eq!((l["preview"][0]["type"].clone(), l["preview"][0]["title"].clone(), l["memo"].clone()), (json!("title"), json!("예배로 부름"), json!("메모")));
        // 상태: Electron getState 의 맨 위 키가 있고, 비밀은 없다
        c.settings.update(|d| { d["triggerKey"] = json!("SECRETKEY123"); d["remote"] = json!({ "enabled": false, "pin": "135790", "port": 37540 }); });
        let s = state_json(&c);
        for k in ["obsView", "live", "stage", "layers", "screen", "preview", "versions", "commentaries", "settings", "shortcutLabels", "defaultShortcuts", "outputs", "obs", "message", "port", "portWarning", "dataDir", "cameras"] {
            assert!(s.get(k).is_some(), "{k} 없음");
        }
        let text = s.to_string();
        assert!(!text.contains("SECRETKEY123") && !text.contains("135790"), "비밀이 상태에 들어 있다");
        assert_eq!(s["settings"]["activeCueId"], id.as_str());
    }
}
