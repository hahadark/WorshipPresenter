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
    Update,
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
    /// OBS 창에서 누른 동작(장면·방송·녹화)이 실패한 글. 상태 자체는 `wp_core::obs::cached()` 가 한 출처
    obs_err: String,
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

/// 그 창이 열려 있는지 (상태 점을 눌러 열었는지 시험용)
pub fn is_open(w: Win) -> bool {
    ST.lock().open.contains(&w)
}

/// 순서 편집칸(⚡ 송출할 때 자동으로)의 OBS 줄이 쓰는 상태: (설정에서 켬, 연결됨, 장면 목록).
/// 출처는 `wp_core::obs::cached()` — 백그라운드 폴러(obs.enabled 면 10초마다)·OBS 창·자동 동작이 모두 같은 캐시를 쓴다
/// (Electron `BCObs.state()` 와 같은 역할). OBS 창을 열지 않아도 장면 콤보가 채워진다.
pub fn obs_state(settings: &Value) -> (bool, bool, Vec<String>) {
    let k = wp_core::obs::cached();
    let enabled = wp_core::obs::enabled(settings);
    let connected = k["connected"].as_bool().unwrap_or(false);
    let scenes = k["scenes"].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(String::from)).collect();
    (enabled, connected, scenes)
}

/// 조작 화면 상태줄용 OBS 한 줄: 설정에서 켰을 때만 Some((글, 경고인지)).
/// Electron 상태줄의 방송/녹화/끊김 표시와 같은 역할 (obs.js `core 'obs'` 이벤트 → 상태줄)
pub fn obs_status_line(settings: &Value) -> Option<(String, bool)> {
    if !wp_core::obs::enabled(settings) {
        return None;
    }
    let k = wp_core::obs::cached();
    if k["connected"] != true {
        // 아직 한 번도 확인하지 못했으면(at == 0) 확인 중, 아니면 끊김
        return Some(if k["at"].as_i64().unwrap_or(0) == 0 { (wp_core::i18n::tr("OBS 확인 중…"), false) } else { (wp_core::i18n::tr("OBS 끊김"), true) });
    }
    let mut parts = vec![wp_core::i18n::tr("OBS 연결됨")];
    if k["streaming"] == true {
        let t = k["streamTime"].as_str().unwrap_or("");
        // OBS 가 보낸 글이라 글자 경계가 아닐 수 있다 → get 으로 (바이트 자르기 panic 막기)
        parts.push(match t.get(..8) { Some(t8) => wp_core::i18n::tr_args("방송 중 {1}", &[&t8]), None => wp_core::i18n::tr("방송 중") });
    }
    if k["recording"] == true {
        parts.push(wp_core::i18n::tr("녹화 중"));
    }
    Some((parts.join(" · "), false))
}

/// 장비(X32 · PTZ) 등록 창 (도구 → Companion·장비)
pub fn open_devices() {
    crate::tools_ui::open(crate::tools_ui::Tool::Devices);
}

/// 메뉴 id → 창 (못 알아들으면 false)
pub fn handle_menu(id: &str) -> bool {
    match id {
        "d:bulletin" => open(Win::Bulletin),
        "d:output" => open(Win::Output),
        "d:obs" => open(Win::Obs),
        "d:update" => open(Win::Update),
        "d:reimport" => wp_core::firstrun::menu_ask(),
        _ => return false,
    }
    true
}

/// 매 프레임: 예배당 창 자동 켜기 · 상태 알리기 · 창 그리기
pub fn frame(app: &mut App, ctx: &egui::Context) {
    output_exit(app, ctx);
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
    wp_core::i18n::set_lang(app.core.lock().settings.get()["uiLang"].as_str().unwrap_or("ko"));
    ime_on_focus(app, ctx);
    // 점검용: --open-d bulletin|output|obs
    static ARGS: std::sync::Once = std::sync::Once::new();
    ARGS.call_once(|| {
        if let Some(w) = std::env::args().position(|x| x == "--open-d").and_then(|i| std::env::args().nth(i + 1)) {
            handle_menu(&format!("d:{w}"));
        }
    });
    // OBS 상태 폴러 (한 번만 시작. obs.enabled 일 때만 네트워크를 쓴다)
    wp_core::obs::start_poller(app.core.clone());
    if let Some((k, v)) = BG.lock().take() {
        let mut s = ST.lock();
        match k.as_str() {
            "obs" => {
                s.obs_err = v["error"].as_str().unwrap_or("").to_string();
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
            Win::Update => "새 버전",
        };
        // 떠 있는 창 기본 자리: 큐시트·메뉴줄을 가리지 않게 (widgets::float_pos)
        let wid = egui::Id::unique(("wp-float", title));
        egui::Window::new(wp_core::i18n::tr(title)).id(wid).default_pos(crate::widgets::float_pos(ctx, wid, 560.0)).open(&mut keep).default_width(560.0).collapsible(false).show(ctx, |ui| match w {
            Win::Bulletin => bulletin_ui(ui, &core),
            Win::Output => output_ui(ui, app),
            Win::Obs => obs_ui(ui, &core),
            Win::Update => update_ui(ui, &core),
        });
        if !keep {
            ST.lock().open.retain(|x| *x != w);
        }
    }
}

/// 예배당 창을 켜고 끈 뒤 부를 것 (끄면 OBS 도 숨김)
/// 예배당 창이 스스로 닫혔을 때 (창을 눌러 고른 뒤 Esc · 오류):
/// Electron main.js onOutputKey — `setOutput({ enabled: false })` (끄면 OBS 화면도 숨김 = outputToggled) + 조작 화면에 초점.
/// 상태 줄·아래 줄 단추는 out.running() 을 보므로 자동으로 '꺼짐' 이 되고, 알림의 [예배당 화면 켜기] 로 다시 켬다.
pub(crate) fn output_exit(app: &mut App, ctx: &egui::Context) {
    use wp_core::i18n::tr;
    let Some(code) = app.out.take_exit() else { return };
    toggled(&app.core, true, false);
    let mut c = app.core.lock();
    if code == Some(crate::outwin::EXIT_ESC) {
        log::info!("예배당 화면 창에서 Esc → 예배당 화면 끔");
        c.notify_act(tr("예배당 화면에서 Esc 를 눌러 예배당 화면을 껐습니다."), "info", "output-on");
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    } else {
        log::warn!("예배당 화면 창이 닫혔습니다 (종료 코드 {code:?})");
        c.notify_act(tr("예배당 화면 창이 닫혔습니다."), "warn", "output-on");
    }
    ctx.request_repaint();
}

pub fn toggled(core: &Shared, before: bool, after: bool) {
    wp_core::trackd::output_toggled(&mut core.lock(), before, after);
}

fn note(ui: &mut egui::Ui, s: &St) {
    if !s.msg.is_empty() {
        let col = if s.msg.starts_with('✗') { Color32::from_rgb(0xff, 0x8a, 0x8a) } else { Color32::from_rgb(0x8a, 0xe0, 0xa8) };
        ui.label(RichText::new(wp_core::i18n::tr_dyn(&s.msg)).color(col));
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
    ui.label(RichText::new(wp_core::i18n::tr("① 주보 파일(hwp · hwpx · docx · txt)을 고르거나 예배 순서를 붙여 넣으세요. PDF·사진은 AI 로만 읽습니다.")).small());
    ui.horizontal(|ui| {
        if ui.button(wp_core::i18n::tr("주보 파일 고르기…")).clicked() {
            if let Some(p) = rfd::FileDialog::new().add_filter("주보", &["hwp", "hwpx", "docx", "txt", "md", "pdf", "png", "jpg", "jpeg", "webp"]).pick_file() {
                match wp_core::core::Core::read_bulletin_file(&p) {
                    Ok(bulletin::Input::Text(t)) => {
                        s.text = t;
                        s.file = None;
                        s.msg = wp_core::i18n::tr_args("✓ {1} 에서 글을 꺼냈습니다.", &[&(p.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default())]);
                    }
                    Ok(bulletin::Input::File { .. }) => {
                        s.file = Some(p.clone());
                        s.msg = wp_core::i18n::tr_args("{1} 은(는) 글을 꺼낼 수 없어 AI 로 읽습니다.", &[&(p.display())]);
                    }
                    Err(e) => s.msg = format!("✗ {e}"),
                }
            }
        }
        if let Some(f) = &s.file {
            ui.label(RichText::new(f.file_name().map(|x| x.to_string_lossy().to_string()).unwrap_or_default()).small().color(Color32::LIGHT_BLUE));
        }
    });
    ui.add(egui::TextEdit::multiline(&mut s.text).desired_rows(10).desired_width(f32::INFINITY).hint_text(wp_core::i18n::tr("예)\n2026-11-15 주일 예배 (11:00)\n예배로 부름 - 홍길동 목사\n찬송 8장 *\n대표기도  김철수 장로\n성경봉독 요 3:16-18\n설교 “선한 목자” 홍길동 목사\n광고\n축도")));
    ui.separator();
    ui.label(RichText::new(wp_core::i18n::tr("② 만드는 방법")).strong());
    ui.horizontal(|ui| {
        if ui.button(wp_core::i18n::tr("규칙으로 바로 (AI 없이, 무료)")).clicked() {
            let t = s.text.clone();
            s.msg = result_msg(core.lock().quick_make_cue(&t));
        }
        let has_key = core.lock().ai_settings()["hasKey"].as_bool().unwrap_or(false);
        if ui.add_enabled(has_key, egui::Button::new(wp_core::i18n::tr("AI 로 (오픈라우터, 유료)"))).on_disabled_hover_text(wp_core::i18n::tr("아래에 오픈라우터 키를 넣으세요")).clicked() {
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
        ui.label(wp_core::i18n::tr("AI 추가 요청"));
        ui.text_edit_singleline(&mut s.extra);
    });
    ui.collapsing(wp_core::i18n::tr("채팅 AI 로 (키 없이 · 챗GPT·클로드·제미나이)"), |ui| {
        ui.label(RichText::new(wp_core::i18n::tr("프롬프트를 복사해 채팅 AI 에 주보와 함께 붙여 넣고, 받은 답(JSON)을 아래에 붙여 넣으세요.")).small());
        ui.horizontal(|ui| {
            if ui.button(wp_core::i18n::tr("프롬프트 복사")).clicked() {
                crate::widgets::copy_notice(ui.ctx(), bulletin::chat_prompt(&s.extra), wp_core::i18n::tr("프롬프트를 복사했습니다."));
                s.msg = wp_core::i18n::tr("✓ 프롬프트를 복사했습니다.").into();
            }
            if ui.button(wp_core::i18n::tr("샘플 파일 저장…")).clicked() {
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
        ui.add(egui::TextEdit::multiline(&mut s.answer).desired_rows(5).desired_width(f32::INFINITY).hint_text(wp_core::i18n::tr("채팅 AI 의 답을 여기에 붙여 넣기")));
        if ui.button(wp_core::i18n::tr("답으로 큐시트 만들기")).clicked() {
            let a = s.answer.clone();
            s.msg = result_msg(core.lock().chat_ai_import(&a));
        }
    });
    ui.collapsing(wp_core::i18n::tr("AI 설정 (오픈라우터)"), |ui| {
        let st = core.lock().ai_settings();
        ui.label(if st["hasKey"] == true { wp_core::i18n::tr_s("키: 저장됨 (Windows 보호 기능으로 암호화)") } else { wp_core::i18n::tr_s("키: 없음") });
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut s.key).password(true).hint_text("sk-or-…"));
            if ui.button(wp_core::i18n::tr("키 저장")).clicked() {
                let k = s.key.clone();
                core.lock().set_ai(Some(&k), None, None);
                s.key.clear();
            }
            if ui.button(wp_core::i18n::tr("키 지우기")).clicked() {
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
    if ui.checkbox(&mut a, wp_core::i18n::tr("송출하면 예배당 화면 창을 자동으로 켜기")).changed() {
        set("autoOnSend", a);
    }
    let mut l = flag("linkObs", true);
    if ui.checkbox(&mut l, wp_core::i18n::tr("예배당 화면을 끄면 OBS 화면도 같이 숨기기")).changed() {
        set("linkObs", l);
    }
    let mut cur = flag("cursorHidden", true);
    if ui.checkbox(&mut cur, wp_core::i18n::tr("출력 화면에서 마우스 커서 숨기기")).changed() {
        set("cursorHidden", cur);
    }
    let mut pip = flag("obsPip", false);
    if ui.checkbox(&mut pip, wp_core::i18n::tr("OBS 자막: 그림·악보 슬라이드를 오른쪽 위에 작게")).changed() {
        set("obsPip", pip);
    }
    ui.separator();
    ui.label(RichText::new(wp_core::i18n::tr("모니터")).strong());
    if ui.small_button(wp_core::i18n::tr("모니터 다시 찾기")).clicked() {
        app.out.refresh_monitors();
    }
    for m in app.out.monitors.clone() {
        let t = format!("{}  {}×{}{}", m.name, m.size.0, m.size.1, if m.primary { wp_core::i18n::tr_s("  (주 모니터 · 조작 화면)") } else { "" });
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
    ui.label(wp_core::i18n::tr_args("예배당 화면: {1}{2}", &[&(if running { wp_core::i18n::tr_s("켜짐") } else { wp_core::i18n::tr_s("꺼짐") }), &(if held { wp_core::i18n::tr_s(" · OBS 화면 숨김 중") } else { "" })]));
    ui.horizontal(|ui| {
        if ui.button(if running { wp_core::i18n::tr_s("예배당 화면 끄기") } else { wp_core::i18n::tr_s("예배당 화면 켜기") }).clicked() {
            if running {
                app.out.close();
            } else {
                let port = core.lock().port;
                app.out.open(port, app.monitor, false);
            }
            toggled(&core, running, !running);
        }
        if ui.button(wp_core::i18n::tr("시험용 작은 창")).clicked() {
            let port = core.lock().port;
            app.out.open(port, app.monitor, true);
        }
    });
    ui.separator();
    let port = core.lock().port;
    obs_urls(ui, port);
}

/// OBS 에 넣을 주소 3줄 + [복사] (원본 설정 → 출력 'OBS 브라우저 소스 주소' 와 같은 역할, 평가 5번).
/// 복사하면 '… 주소를 복사했습니다' 알림
pub(crate) fn obs_urls(ui: &mut egui::Ui, port: u16) {
    use crate::theme::{self, pal, Weight};
    use crate::widgets::{self, Btn, Size};
    use wp_core::i18n::tr;
    let p = pal();
    ui.label(theme::rich(tr("OBS에 넣을 주소"), 14.0, Weight::Bold, p.text));
    ui.add_space(4.0);
    for (i, (label, path)) in [("예배당 화면 그대로", "/outputs/main"), ("투명 자막", "/outputs/overlay"), ("무대 화면", "/outputs/stage")].into_iter().enumerate() {
        let url = format!("http://127.0.0.1:{port}{path}");
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            // 영어 이름은 길다 ('Transparent lower third') — 잘리지 않게 칸을 넓히고, 그래도 잘리면 마우스를 올려 볼 수 있게
            let lw = if wp_core::i18n::is_en() { 164.0 } else { 120.0 };
            ui.allocate_ui_with_layout(egui::vec2(lw, 26.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.set_min_width(lw);
                ui.add(egui::Label::new(theme::rich(tr(label), 13.0, Weight::Regular, p.text2)).truncate()).on_hover_text(tr(label));
            });
            egui::Frame::new().fill(p.sunken).stroke(egui::Stroke::new(1.0, p.line)).corner_radius(egui::CornerRadius::same(4)).inner_margin(egui::Margin::symmetric(8, 4)).show(ui, |ui| {
                ui.set_min_width(if wp_core::i18n::is_en() { 270.0 } else { 300.0 });
                ui.add(egui::Label::new(egui::RichText::new(&url).monospace().color(p.text)).selectable(true));
            });
            let b = ui.add(Btn::new(tr("복사")).icon("copy").size(Size::Sm).height(26.0).tooltip(tr("주소를 복사해 OBS 브라우저 소스에 붙여 넣습니다")));
            widgets::mark(&format!("obs-copy-{i}"), b.rect);
            if b.clicked() {
                widgets::copy_notice(ui.ctx(), url.clone(), crate::track_c::obs_copied_msg(label));
            }
        });
    }
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.label(theme::rich(tr("OBS → 소스 ＋ → 브라우저 → 주소 붙여 넣기 · 너비·높이 1920×1080"), 12.0, Weight::Regular, p.muted));
        if ui.link(tr("도움말 › OBS 연결 안내")).clicked() {
            crate::parity_ui::handle_menu("a:help-obs");
        }
    });
}

/// OBS 창의 동작·연결 확인을 백그라운드에서 (코어 잠금 없이). 상태는 `obs::status` 가 캐시에 남기고, 동작 오류만 BG 로 돌려준다
fn obs_bg(conf: wp_core::obs::Conf, then_act: Option<Value>) {
    ST.lock().obs_busy = true;
    std::thread::spawn(move || {
        let mut err = String::new();
        if let Some(a) = then_act {
            if let Err(e) = wp_core::obs::action(&conf, &a) {
                err = e;
            }
        }
        let _ = wp_core::obs::status(&conf);
        *BG.lock() = Some(("obs".into(), json!({ "error": err })));
    });
}

fn obs_ui(ui: &mut egui::Ui, core: &Shared) {
    let o = core.lock().settings.get()["obs"].clone();
    let mut s = ST.lock();
    let mut en = o["enabled"].as_bool().unwrap_or(false);
    let mut host = o["host"].as_str().unwrap_or("127.0.0.1").to_string();
    let mut port = o["port"].as_u64().unwrap_or(4455).to_string();
    let mut changed = ui.checkbox(&mut en, wp_core::i18n::tr("OBS 원격 조작 쓰기 (OBS → 도구 → WebSocket 서버 설정에서 서버 켜기)")).changed();
    ui.horizontal(|ui| {
        ui.label(wp_core::i18n::tr("주소"));
        changed |= ui.add(egui::TextEdit::singleline(&mut host).desired_width(140.0)).lost_focus();
        ui.label(wp_core::i18n::tr("포트"));
        changed |= ui.add(egui::TextEdit::singleline(&mut port).desired_width(60.0)).lost_focus();
    });
    let p: u16 = port.parse().unwrap_or(4455);
    ui.horizontal(|ui| {
        ui.label(wp_core::i18n::tr("비밀번호"));
        ui.add(egui::TextEdit::singleline(&mut s.obs_pw).password(true).desired_width(160.0).hint_text(if o["passwordEnc"].as_str().is_some_and(|x| !x.is_empty()) { "저장됨" } else { "" }));
        if ui.button(wp_core::i18n::tr("저장")).clicked() {
            let pw = s.obs_pw.clone();
            core.lock().set_obs(en, &host, p, Some(&pw));
            s.obs_pw.clear();
            wp_core::obs::kick();
        }
    });
    if changed {
        core.lock().set_obs(en, &host, p, None);
        wp_core::obs::kick(); // 폴러가 새 설정으로 바로 한 번 확인
    }
    let conf = wp_core::obs::conf_from(core.lock().settings.get());
    let busy = s.obs_busy;
    let mut check = false;
    ui.horizontal(|ui| {
        check = ui.add_enabled(!busy, egui::Button::new(wp_core::i18n::tr("연결 확인"))).on_disabled_hover_text(wp_core::i18n::tr("확인하는 중입니다. 잠시 기다리세요")).clicked();
        if busy {
            ui.spinner();
        }
        if en {
            ui.label(RichText::new(wp_core::i18n::tr_args("{1}초마다 자동으로 확인합니다", &[&wp_core::obs::POLL_SECS])).small().color(crate::theme::pal().muted));
        }
    });
    if check {
        drop(s);
        obs_bg(conf.clone(), None);
        s = ST.lock();
    }
    // 상태는 폴러·동작·연결 확인이 모두 남기는 한 캐시에서 (순서 편집칸 장면 콤보와 같은 출처)
    let st = wp_core::obs::cached();
    if !s.obs_err.is_empty() {
        ui.label(RichText::new(format!("✗ {}", wp_core::i18n::tr_dyn(&s.obs_err))).color(crate::theme::pal().danger_text));
    }
    let at = st["at"].as_i64().unwrap_or(0);
    ui.ctx().request_repaint_after(std::time::Duration::from_secs(1)); // 'N초 전 확인' 이 보이는 동안
    let ago = if at > 0 { wp_core::i18n::tr_args(" ({1}초 전 확인)", &[&((wp_core::util::now_ms() - at).max(0) / 1000)]) } else { String::new() };
    if let Some(e) = st["error"].as_str() {
        ui.label(RichText::new(format!("✗ {e}{ago}")).color(crate::theme::pal().danger_text));
    } else if st["connected"] == true {
        ui.label(RichText::new(wp_core::i18n::tr_args("✓ 연결됨 · 지금 장면: {1} · 방송 {2} · 녹화 {3}", &[&(st["scene"].as_str().unwrap_or("")), &(if st["streaming"] == true { wp_core::i18n::tr_s("중") } else { wp_core::i18n::tr_s("꺼짐") }), &(if st["recording"] == true { wp_core::i18n::tr_s("중") } else { wp_core::i18n::tr_s("꺼짐") })]) + &ago).color(crate::theme::pal().ok_text));
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
            // Electron obspanel.js: 방송은 on 을 명시, 녹화는 on 없이(토글). 코어 action 이 지금 상태와 같으면 요청을 생략하므로
            // 표시가 조금 오래됐어도 OBS 가 "output already active" 오류를 내지 않는다
            if ui.button(if streaming { wp_core::i18n::tr_s("방송 끝내기") } else { wp_core::i18n::tr_s("방송 시작") }).clicked() {
                act = Some(json!({ "action": "stream", "on": !streaming }));
            }
            if ui.button(if recording { wp_core::i18n::tr_s("녹화 끝내기") } else { wp_core::i18n::tr_s("녹화 시작") }).clicked() {
                act = Some(json!({ "action": "record" }));
            }
        });
        if let Some(a) = act {
            s.obs_err.clear();
            drop(s);
            obs_bg(conf, Some(a));
            return;
        }
    } else if en && at == 0 {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(wp_core::i18n::tr("OBS 확인 중…"));
        });
    }
    // 개발자 용어(actions·obsScene) 대신 화면에 보이는 이름으로 (휴리스틱 평가 12번)
    ui.label(RichText::new(wp_core::i18n::tr("순서 편집칸의 '⚡ 송출할 때 자동으로'에서 OBS 장면·방송·녹화를 고르면, 그 순서를 송출할 때 OBS 도 함께 바뀝니다.")).small().color(crate::theme::pal().muted));
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
pub(crate) fn update_ui(ui: &mut egui::Ui, core: &Shared) {
    let st = wp_core::updates::status(core.lock().settings.get());
    ui.label(wp_core::i18n::tr_args(
        "지금 버전: {1}",
        &[&(st["current"].as_str().unwrap_or(""))],
    ));
    if st["releaseHold"] == true {
        ui.colored_label(Color32::YELLOW, st["holdReason"].as_str().unwrap_or(""));
    }
    if let Some(e) = st["downloadError"].as_str() {
        ui.colored_label(Color32::YELLOW, e);
    }
    match st["state"].as_str().unwrap_or("idle") {
        "checking" => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(wp_core::i18n::tr("확인하는 중…"));
            });
        }
        "available" | "ready" | "cancelled" => {
            ui.label(
                RichText::new(wp_core::i18n::tr_args(
                    "새 버전 {1} 이 나왔습니다{2}",
                    &[
                        &(st["version"].as_str().unwrap_or("")),
                        &(if st["skipped"] == true {
                            wp_core::i18n::tr_s(" (건너뛰기로 함)")
                        } else {
                            ""
                        }),
                    ],
                ))
                .strong()
                .color(Color32::from_rgb(0x8f, 0xb8, 0xff)),
            );
            egui::ScrollArea::vertical()
                .max_height(260.0)
                .show(ui, |ui| {
                    ui.label(st["notes"].as_str().unwrap_or(""));
                });
            ui.horizontal(|ui| {
                if let Some(u) = st["notesUrl"]
                    .as_str()
                    .filter(|u| u.starts_with("https://github.com/"))
                {
                    if ui.button(wp_core::i18n::tr("릴리스 설명 열기")).clicked() {
                        let _ = wp_core::trackd::open_link(u);
                    }
                }
                if st["skipped"] == true {
                    if ui.button(wp_core::i18n::tr("건너뛰기 취소")).clicked() {
                        wp_core::updates::unskip(core);
                    }
                } else if ui.button(wp_core::i18n::tr("이 버전 건너뛰기")).clicked() {
                    wp_core::updates::skip(core);
                }
            });
        }
        "downloading" | "verifying" => {
            ui.label(if st["state"] == "verifying" {
                wp_core::i18n::tr("SHA-256 검증 중…")
            } else {
                wp_core::i18n::tr("다운로드 중…")
            });
            ui.add(
                egui::ProgressBar::new(st["percent"].as_f64().unwrap_or(0.0) as f32 / 100.0)
                    .show_percentage(),
            );
            ui.label(wp_core::i18n::tr_args("{1} / {2} 바이트", &[&st["received"], &st["total"]]));
            if ui.button(wp_core::i18n::tr("다운로드 취소")).clicked() {
                wp_core::updates::cancel();
            }
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
        "installing" => {
            ui.label(wp_core::i18n::tr("설치 프로그램에 명시적 설치 요청을 전달했습니다."));
        }
        "latest" => {
            ui.label(
                RichText::new(wp_core::i18n::tr("지금이 최신 버전입니다."))
                    .color(Color32::from_rgb(0x8a, 0xe0, 0xa8)),
            );
        }
        "error" => {
            ui.label(
                RichText::new(st["error"].as_str().unwrap_or(""))
                    .color(Color32::from_rgb(0xff, 0x8a, 0x8a)),
            );
        }
        _ => {}
    }
    ui.separator();
    if st["state"] == "ready" { ui.label(wp_core::i18n::tr("SHA-256 검증 완료. 설치 준비됨. 자동 설치하지 않습니다.")); }
    if st["state"] == "cancelled" { ui.label(wp_core::i18n::tr("다운로드를 취소했습니다.")); }
    // [갈래 D] 포터블·개발 실행: 받기·설치 단추가 왜 꺼져 있는지 보이게 (전에는 '받을 새 버전이 없거나…' 라고만 떠서 이유를 알 수 없었다)
    let portable = st["portable"] == true;
    if portable {
        ui.label(RichText::new(wp_core::i18n::tr(wp_core::updates::PORTABLE_NOTE)).color(Color32::from_rgb(0xff, 0xc8, 0x6a)));
    }
    let why_off = if portable { wp_core::updates::PORTABLE_NOTE } else { "받을 새 버전이 없거나 이미 받는 중입니다" };
    ui.horizontal(|ui| {
        if ui.add_enabled(st["canDownload"] == true, egui::Button::new(wp_core::i18n::tr("검증하여 다운로드"))).on_disabled_hover_text(wp_core::i18n::tr(why_off)).clicked() {
            if let Err(e) = wp_core::updates::download(core) { core.lock().notify(e, "warn"); }
        }
        if ui.add_enabled(st["canInstall"] == true, egui::Button::new(wp_core::i18n::tr("지금 설치 요청 (프로그램 종료)"))).on_disabled_hover_text(wp_core::i18n::tr("먼저 새 버전을 받아 검증을 마쳐야 합니다")).clicked() {
            match wp_core::updates::install_now(core) {
                Ok(()) => ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close),
                Err(e) => core.lock().notify(e, "warn"),
            }
        }
    });
    let mut repo = st["repo"].as_str().unwrap_or("").to_string();
    ui.horizontal(|ui| {
        ui.label(wp_core::i18n::tr("확인할 저장소"));
        if ui
            .add(
                egui::TextEdit::singleline(&mut repo)
                    .hint_text(wp_core::i18n::tr(
                        "비워 두면 hahadark/WorshipPresenter",
                    ))
                    .desired_width(320.0),
            )
            .changed()
        {
            core.lock()
                .settings
                .update(|d| d["updateRepo"] = json!(repo));
        }
    });
    let mut auto = core.lock().settings.get()["autoUpdate"]
        .as_bool()
        .unwrap_or(true);
    if ui
        .checkbox(&mut auto, wp_core::i18n::tr("켤 때 새 버전 확인"))
        .changed()
    {
        core.lock()
            .settings
            .update(|d| d["autoUpdate"] = json!(auto));
    }
    let mut tel = core.lock().settings.get()["telemetry"]
        .as_bool()
        .unwrap_or(true);
    if ui
        .checkbox(
            &mut tel,
            wp_core::i18n::tr(
                "익명 사용 통계 보내기 (하루 한 번: 무작위 설치 번호 · 버전 · 언어 · OS)",
            ),
        )
        .changed()
    {
        core.lock().settings.update(|d| d["telemetry"] = json!(tel));
    }
    if ui.button(wp_core::i18n::tr("지금 확인")).clicked() {
        wp_core::updates::check(core, true);
    }
}

/// [갈래 D] 첫 화면: 1.x(Electron 판) 자료가 이 판 자료보다 최근에 바뀌었으면 다시 가져올지 묻는다.
/// 다시 가져오면 지금 이 판 자료는 지우지 않고 data\pre-reimport\ 에 보관한 뒤 프로그램이 다시 켜진다.
pub fn migrate_ask(ctx: &egui::Context, data_dir: &std::path::Path) {
    let Some(a) = wp_core::firstrun::ask() else {
        start_warning(ctx, data_dir);
        return;
    };
    let tr = wp_core::i18n::tr;
    let when = |ms: &Value| {
        wp_core::firstrun::local_time(ms.as_i64().unwrap_or(0))
    };
    egui::Modal::new(egui::Id::new("wp-migrate-ask")).show(ctx, |ui| {
        ui.set_max_width(560.0);
        if a["reason"] == "none" {
            ui.heading(tr("1.x 자료 다시 가져오기"));
            ui.add_space(6.0);
            ui.label(tr("이 PC 에서 예전 워십프리젠터(1.x) 자료를 찾지 못했습니다."));
            ui.add_space(8.0);
            if ui.button(tr("닫기")).clicked() {
                wp_core::firstrun::keep_current(data_dir);
            }
            return;
        }
        ui.heading(if a["reason"] == "menu" { tr("1.x 자료 다시 가져오기") } else { tr("예전 워십프리젠터(1.x) 자료가 더 최근에 바뀌었습니다") });
        ui.add_space(6.0);
        ui.label(if a["reason"] == "menu" {
            tr("예전 워십프리젠터(1.x)의 큐시트·찬양·설정을 다시 가져와 지금 자료 대신 씁니다.")
        } else if a["reason"] == "nomarker" {
            tr("이 PC 에는 예전에 만들어진 2.x 자료가 있어서, 그 뒤 1.x 에서 만든 큐시트·찬양·설정을 가져오지 않았습니다.")
        } else {
            tr("자료를 가져온 뒤 1.x 판과 이 판 양쪽에서 자료가 바뀌었습니다.")
        });
        ui.add_space(4.0);
        egui::Grid::new("wp-migrate-grid").num_columns(3).spacing([16.0, 4.0]).show(ui, |ui| {
            ui.label("");
            ui.strong(tr("큐시트 · 찬양"));
            ui.strong(tr("마지막으로 바뀜"));
            ui.end_row();
            ui.label(tr("1.x 자료"));
            ui.label(format!("{} · {}", a["electronCues"], a["electronSongs"]));
            ui.label(when(&a["electronChanged"]));
            ui.end_row();
            ui.label(tr("지금 자료"));
            ui.label(format!("{} · {}", a["rustCues"], a["rustSongs"]));
            ui.label(when(&a["rustChanged"]));
            ui.end_row();
        });
        ui.add_space(6.0);
        ui.label(RichText::new(wp_core::i18n::tr_args("다시 가져와도 지금 자료는 지우지 않고 {1} 에 보관합니다.", &[&data_dir.join(wp_core::firstrun::ASIDE_DIR).display()])).small());
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button(tr("1.x 자료 다시 가져오기 (프로그램이 다시 켜집니다)")).clicked() {
                match wp_core::firstrun::request_reimport_and_restart(data_dir) {
                    Ok(()) => ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close),
                    Err(e) => log::warn!("다시 가져오기 요청 실패: {e}"),
                }
            }
            if ui.button(tr("지금 자료 그대로 쓰기")).clicked() {
                wp_core::firstrun::keep_current(data_dir);
            }
        });
    });
}

/// [갈래 D] 켤 때 생긴 중요한 문제를 [확인] 을 누를 때까지 보인다 (깨진 설정 파일 · 1.x 자료 복사 실패)
fn start_warning(ctx: &egui::Context, data_dir: &std::path::Path) {
    let warns = wp_core::firstrun::start_warnings();
    if warns.is_empty() {
        return;
    }
    let tr = wp_core::i18n::tr;
    egui::Modal::new(egui::Id::new("wp-start-warning")).show(ctx, |ui| {
        ui.set_max_width(620.0);
        ui.heading(tr("켜는 중에 문제가 있었습니다"));
        ui.add_space(6.0);
        for w in &warns {
            ui.label(w);
            ui.add_space(4.0);
        }
        ui.label(RichText::new(wp_core::i18n::tr_args("자료 폴더: {1}", &[&data_dir.display()])).small());
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button(tr("확인")).clicked() {
                wp_core::firstrun::clear_start_warnings();
            }
            if ui.button(tr("자료 폴더 열기")).clicked() {
                let _ = std::process::Command::new("explorer").arg(data_dir).spawn();
            }
        });
    });
}