//! [트랙 D] 도구 창 2: 주보 → 큐시트 · 출력 설정 · OBS 원격 조작
//! 메뉴(트랙 C menubar)의 "d:bulletin" · "d:output" · "d:obs" 가 `open()` 을 부른다.

use crate::app::App;
use eframe::egui::{self, Color32, RichText};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde_json::{json, Value};
use wp_core::bulletin;
use wp_core::core::Shared;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Win {
    Bulletin,
    Output,
    Obs,
}

#[derive(Default)]
struct St {
    open: Vec<Win>,
    text: String,
    extra: String,
    answer: String,
    key: String,
    file: Option<std::path::PathBuf>,
    msg: String,
    obs_pw: String,
    obs_status: Value,
    obs_busy: bool,
}

static ST: Lazy<Mutex<St>> = Lazy::new(|| Mutex::new(St::default()));
static BG: Lazy<Mutex<Option<(String, Value)>>> = Lazy::new(|| Mutex::new(None));

pub fn open(w: Win) {
    let mut s = ST.lock();
    if !s.open.contains(&w) {
        s.open.push(w);
    }
}

/// 메뉴 id → 창 (못 알아들으면 false)
pub fn handle_menu(id: &str) -> bool {
    match id {
        "d:bulletin" => open(Win::Bulletin),
        "d:output" => open(Win::Output),
        "d:obs" => open(Win::Obs),
        _ => return false,
    }
    true
}

/// 매 프레임: 예배당 창 자동 켜기 · 상태 알리기 · 창 그리기
pub fn frame(app: &mut App, ctx: &egui::Context) {
    let running = app.out.running();
    // 점검 실행(--shot)에서는 사용자 모니터에 전체 화면 창을 띄우지 않는다
    let testing = std::env::args().any(|x| x == "--shot");
    if wp_core::trackd::take_output_on_request() && !running && !testing {
        let port = app.core.lock().port;
        app.out.open(port, app.monitor, false);
        wp_core::trackd::output_toggled(&mut app.core.lock(), false, true);
    }
    let running = app.out.running();
    let mon = app.out.monitors.iter().find(|m| m.index == app.monitor).map(|m| json!({ "index": m.index, "name": m.name, "width": m.size.0, "height": m.size.1, "primary": m.primary }));
    wp_core::trackd::set_desktop_status(json!({ "running": running, "monitor": mon }));
    ime_on_focus(app, ctx);
    // 점검용: --open-d bulletin|output|obs
    static ARGS: std::sync::Once = std::sync::Once::new();
    ARGS.call_once(|| {
        if let Some(w) = std::env::args().position(|x| x == "--open-d").and_then(|i| std::env::args().nth(i + 1)) {
            handle_menu(&format!("d:{w}"));
        }
    });
    if let Some((k, v)) = BG.lock().take() {
        let mut s = ST.lock();
        match k.as_str() {
            "obs" => {
                s.obs_status = v;
                s.obs_busy = false;
            }
            _ => s.msg = v.as_str().unwrap_or("").to_string(),
        }
    }
    let core = app.core.clone();
    let open = ST.lock().open.clone();
    for w in open {
        let mut keep = true;
        let title = match w {
            Win::Bulletin => "주보 → 큐시트",
            Win::Output => "출력 설정 (예배당 화면 · OBS)",
            Win::Obs => "OBS 원격 조작",
        };
        egui::Window::new(title).open(&mut keep).default_width(560.0).collapsible(false).show(ctx, |ui| match w {
            Win::Bulletin => bulletin_ui(ui, &core),
            Win::Output => output_ui(ui, app),
            Win::Obs => obs_ui(ui, &core),
        });
        if !keep {
            ST.lock().open.retain(|x| *x != w);
        }
    }
}

/// 예배당 창을 켜고 끈 뒤 부를 것 (끄면 OBS 도 숨김)
pub fn toggled(core: &Shared, before: bool, after: bool) {
    wp_core::trackd::output_toggled(&mut core.lock(), before, after);
}

fn note(ui: &mut egui::Ui, s: &St) {
    if !s.msg.is_empty() {
        let col = if s.msg.starts_with('✗') { Color32::from_rgb(0xff, 0x8a, 0x8a) } else { Color32::from_rgb(0x8a, 0xe0, 0xa8) };
        ui.label(RichText::new(&s.msg).color(col));
    }
}

fn result_msg(r: Result<Value, String>) -> String {
    match r {
        Ok(v) => {
            let w: Vec<String> = v["warnings"].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(String::from)).collect();
            format!("✓ '{}' 큐시트를 만들어 열었습니다 ({}개 순서){}", v["name"].as_str().unwrap_or(""), v["count"], if w.is_empty() { String::new() } else { format!("\n확인할 것: {}", w.join(" · ")) })
        }
        Err(e) => format!("✗ {e}"),
    }
}

fn bulletin_ui(ui: &mut egui::Ui, core: &Shared) {
    let mut s = ST.lock();
    ui.label(RichText::new("① 주보 파일(hwp · hwpx · docx · txt)을 고르거나 예배 순서를 붙여 넣으세요. PDF·사진은 AI 로만 읽습니다.").small());
    ui.horizontal(|ui| {
        if ui.button("주보 파일 고르기…").clicked() {
            if let Some(p) = rfd::FileDialog::new().add_filter("주보", &["hwp", "hwpx", "docx", "txt", "md", "pdf", "png", "jpg", "jpeg", "webp"]).pick_file() {
                match wp_core::core::Core::read_bulletin_file(&p) {
                    Ok(bulletin::Input::Text(t)) => {
                        s.text = t;
                        s.file = None;
                        s.msg = format!("✓ {} 에서 글을 꺼냈습니다.", p.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default());
                    }
                    Ok(bulletin::Input::File { .. }) => {
                        s.file = Some(p.clone());
                        s.msg = format!("{} 은(는) 글을 꺼낼 수 없어 AI 로 읽습니다.", p.display());
                    }
                    Err(e) => s.msg = format!("✗ {e}"),
                }
            }
        }
        if let Some(f) = &s.file {
            ui.label(RichText::new(f.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default()).small().color(Color32::LIGHT_BLUE));
        }
    });
    ui.add(egui::TextEdit::multiline(&mut s.text).desired_rows(10).desired_width(f32::INFINITY).hint_text("예)\n2026-11-15 주일 예배 (11:00)\n예배로 부름 - 홍길동 목사\n찬송 8장 *\n대표기도  김철수 장로\n성경봉독 요 3:16-18\n설교 “선한 목자” 홍길동 목사\n광고\n축도"));
    ui.separator();
    ui.label(RichText::new("② 만드는 방법").strong());
    ui.horizontal(|ui| {
        if ui.button("규칙으로 바로 (AI 없이, 무료)").clicked() {
            let t = s.text.clone();
            s.msg = result_msg(core.lock().quick_make_cue(&t));
        }
        let has_key = core.lock().ai_settings()["hasKey"].as_bool().unwrap_or(false);
        if ui.add_enabled(has_key, egui::Button::new("AI 로 (오픈라우터, 유료)")).on_disabled_hover_text("아래에 오픈라우터 키를 넣으세요").clicked() {
            let input = match &s.file {
                Some(f) => wp_core::core::Core::read_bulletin_file(f),
                None => Ok(bulletin::Input::Text(s.text.clone())),
            };
            match input {
                Ok(i) => wp_core::trackd::ai_make_cue(core, i, s.extra.clone()),
                Err(e) => s.msg = format!("✗ {e}"),
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label("AI 추가 요청");
        ui.text_edit_singleline(&mut s.extra);
    });
    ui.collapsing("채팅 AI 로 (키 없이 · 챗GPT·클로드·제미나이)", |ui| {
        ui.label(RichText::new("프롬프트를 복사해 채팅 AI 에 주보와 함께 붙여 넣고, 받은 답(JSON)을 아래에 붙여 넣으세요.").small());
        ui.horizontal(|ui| {
            if ui.button("프롬프트 복사").clicked() {
                ui.ctx().copy_text(bulletin::chat_prompt(&s.extra));
                s.msg = "✓ 프롬프트를 복사했습니다.".into();
            }
            if ui.button("샘플 파일 저장…").clicked() {
                if let Some(p) = rfd::FileDialog::new().set_file_name(bulletin::SAMPLE_NAME).save_file() {
                    let _ = std::fs::write(&p, serde_json::to_string_pretty(&bulletin::sample()).unwrap_or_default());
                }
            }
            for (n, url) in bulletin::CHAT_LINKS {
                if ui.small_button(n).clicked() {
                    let _ = wp_core::trackd::open_link(url);
                }
            }
        });
        ui.add(egui::TextEdit::multiline(&mut s.answer).desired_rows(5).desired_width(f32::INFINITY).hint_text("채팅 AI 의 답을 여기에 붙여 넣기"));
        if ui.button("답으로 큐시트 만들기").clicked() {
            let a = s.answer.clone();
            s.msg = result_msg(core.lock().chat_ai_import(&a));
        }
    });
    ui.collapsing("AI 설정 (오픈라우터)", |ui| {
        let st = core.lock().ai_settings();
        ui.label(if st["hasKey"] == true { "키: 저장됨 (Windows 보호 기능으로 암호화)" } else { "키: 없음" });
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut s.key).password(true).hint_text("sk-or-…"));
            if ui.button("키 저장").clicked() {
                let k = s.key.clone();
                core.lock().set_ai(Some(&k), None, None);
                s.key.clear();
            }
            if ui.button("키 지우기").clicked() {
                core.lock().set_ai(Some(""), None, None);
            }
        });
        let cur = st["model"].as_str().unwrap_or("").to_string();
        egui::ComboBox::from_id_salt("ai-model").selected_text(&cur).width(380.0).show_ui(ui, |ui| {
            for (id, name) in bulletin::RECOMMENDED {
                if ui.selectable_label(cur == id, name).clicked() {
                    core.lock().set_ai(None, Some(id), None);
                }
            }
        });
    });
    note(ui, &s);
}

fn output_ui(ui: &mut egui::Ui, app: &mut App) {
    let core = app.core.clone();
    let out = core.lock().settings.get()["output"].clone();
    let mut set = |k: &str, v: bool| {
        let k = k.to_string();
        let mut c = core.lock();
        c.settings.update(|d| d["output"][&k] = json!(v));
        c.emit(wp_core::core::Ev::Live);
        c.emit(wp_core::core::Ev::Ui);
    };
    let flag = |k: &str, d: bool| out[k].as_bool().unwrap_or(d);
    let mut a = flag("autoOnSend", true);
    if ui.checkbox(&mut a, "송출하면 예배당 화면 창을 자동으로 켜기").changed() {
        set("autoOnSend", a);
    }
    let mut l = flag("linkObs", true);
    if ui.checkbox(&mut l, "예배당 화면을 끄면 OBS 화면도 같이 숨기기").changed() {
        set("linkObs", l);
    }
    let mut cur = flag("cursorHidden", true);
    if ui.checkbox(&mut cur, "출력 화면에서 마우스 커서 숨기기").changed() {
        set("cursorHidden", cur);
    }
    let mut pip = flag("obsPip", false);
    if ui.checkbox(&mut pip, "OBS 자막: 그림·악보 슬라이드를 오른쪽 위에 작게").changed() {
        set("obsPip", pip);
    }
    ui.separator();
    ui.label(RichText::new("모니터").strong());
    if ui.small_button("모니터 다시 찾기").clicked() {
        app.out.refresh_monitors();
    }
    for m in app.out.monitors.clone() {
        let t = format!("{}  {}×{}{}", m.name, m.size.0, m.size.1, if m.primary { "  (주 모니터 · 조작 화면)" } else { "" });
        if ui.radio(app.monitor == m.index, t).clicked() {
            app.monitor = m.index;
            core.lock().settings.update(|d| d["rsOutputMonitor"] = json!(m.index));
            if app.out.running() {
                let port = core.lock().port;
                app.out.open(port, m.index, false);
            }
        }
    }
    let running = app.out.running();
    let held = core.lock().obs_hold;
    ui.label(format!("예배당 창: {}{}", if running { "켜짐" } else { "꺼짐" }, if held { " · OBS 화면 숨김 중" } else { "" }));
    ui.horizontal(|ui| {
        if ui.button(if running { "예배당 창 끄기" } else { "예배당 창 켜기" }).clicked() {
            if running {
                app.out.close();
            } else {
                let port = core.lock().port;
                app.out.open(port, app.monitor, false);
            }
            toggled(&core, running, !running);
        }
        if ui.button("시험용 작은 창").clicked() {
            let port = core.lock().port;
            app.out.open(port, app.monitor, true);
        }
    });
}

fn obs_bg(conf: wp_core::obs::Conf, then_act: Option<Value>) {
    ST.lock().obs_busy = true;
    std::thread::spawn(move || {
        if let Some(a) = then_act {
            if let Err(e) = wp_core::obs::action(&conf, &a) {
                *BG.lock() = Some(("obs".into(), json!({ "error": e })));
                return;
            }
        }
        let v = wp_core::obs::status(&conf).unwrap_or_else(|e| json!({ "error": e }));
        *BG.lock() = Some(("obs".into(), v));
    });
}

fn obs_ui(ui: &mut egui::Ui, core: &Shared) {
    let o = core.lock().settings.get()["obs"].clone();
    let mut s = ST.lock();
    let mut en = o["enabled"].as_bool().unwrap_or(false);
    let mut host = o["host"].as_str().unwrap_or("127.0.0.1").to_string();
    let mut port = o["port"].as_u64().unwrap_or(4455).to_string();
    let mut changed = ui.checkbox(&mut en, "OBS 원격 조작 쓰기 (OBS → 도구 → WebSocket 서버 설정에서 서버 켜기)").changed();
    ui.horizontal(|ui| {
        ui.label("주소");
        changed |= ui.add(egui::TextEdit::singleline(&mut host).desired_width(140.0)).lost_focus();
        ui.label("포트");
        changed |= ui.add(egui::TextEdit::singleline(&mut port).desired_width(60.0)).lost_focus();
    });
    let p: u16 = port.parse().unwrap_or(4455);
    ui.horizontal(|ui| {
        ui.label("비밀번호");
        ui.add(egui::TextEdit::singleline(&mut s.obs_pw).password(true).desired_width(160.0).hint_text(if o["passwordEnc"].as_str().is_some_and(|x| !x.is_empty()) { "저장됨" } else { "" }));
        if ui.button("저장").clicked() {
            let pw = s.obs_pw.clone();
            core.lock().set_obs(en, &host, p, Some(&pw));
            s.obs_pw.clear();
        }
    });
    if changed {
        core.lock().set_obs(en, &host, p, None);
    }
    let conf = wp_core::obs::conf_from(core.lock().settings.get());
    let busy = s.obs_busy;
    let mut check = false;
    ui.horizontal(|ui| {
        check = ui.add_enabled(!busy, egui::Button::new("연결 확인")).clicked();
        if busy {
            ui.spinner();
        }
    });
    if check {
        drop(s);
        obs_bg(conf.clone(), None);
        s = ST.lock();
    }
    let st = s.obs_status.clone();
    if let Some(e) = st["error"].as_str() {
        ui.label(RichText::new(format!("✗ {e}")).color(Color32::from_rgb(0xff, 0x8a, 0x8a)));
    } else if st["connected"] == true {
        ui.label(RichText::new(format!("✓ 연결됨 · 지금 장면: {} · 방송 {} · 녹화 {}", st["scene"].as_str().unwrap_or(""), if st["streaming"] == true { "중" } else { "꺼짐" }, if st["recording"] == true { "중" } else { "꺼짐" })).color(Color32::from_rgb(0x8a, 0xe0, 0xa8)));
        let mut act = None;
        ui.horizontal_wrapped(|ui| {
            for sc in st["scenes"].as_array().into_iter().flatten().filter_map(|x| x.as_str()) {
                if ui.selectable_label(st["scene"] == sc, sc).clicked() {
                    act = Some(json!({ "action": "scene", "scene": sc }));
                }
            }
        });
        ui.horizontal(|ui| {
            let streaming = st["streaming"] == true;
            let recording = st["recording"] == true;
            if ui.button(if streaming { "방송 끝내기" } else { "방송 시작" }).clicked() {
                act = Some(json!({ "action": "stream", "on": !streaming }));
            }
            if ui.button(if recording { "녹화 끝내기" } else { "녹화 시작" }).clicked() {
                act = Some(json!({ "action": "record", "on": !recording }));
            }
        });
        if let Some(a) = act {
            drop(s);
            obs_bg(conf, Some(a));
            return;
        }
    }
    ui.label(RichText::new("큐시트 순서의 '송출할 때 자동으로'(actions)에 obsScene · obsStream · obsRecord 를 두면 그 순서를 송출할 때 OBS 가 같이 움직입니다.").small().color(Color32::GRAY));
}

/// 한글 입력 자동 전환 (설정 imeAuto, 윈도우): 글 입력칸에 새로 들어가면 입력기를 한글로.
/// egui 는 칸의 종류를 알 수 없어서, 장절·낱말 찾기 칸(search-box)만 빼고 모든 글 입력칸에 적용한다.
fn ime_on_focus(app: &App, ctx: &egui::Context) {
    static LAST: Lazy<Mutex<Option<egui::Id>>> = Lazy::new(|| Mutex::new(None));
    let focused = ctx.memory(|m| m.focused());
    let mut last = LAST.lock();
    if focused == *last {
        return;
    }
    *last = focused;
    let Some(id) = focused else { return };
    if !app.core.lock().settings.get()["imeAuto"].as_bool().unwrap_or(true) {
        return;
    }
    let is_text = egui::text_edit::TextEditState::load(ctx, id).is_some();
    if is_text && id != egui::Id::new("search-box") {
        wp_core::winkeys::ime_korean();
    }
}