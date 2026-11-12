//! [트랙 D] 도구 창: 휴대폰 리모컨 · 스트림덱·외부 단추 · Companion·장비 · 내부망 출력
//! 상단 메뉴 줄(트랙 C)이 생기면 `open(Tool::…)` 을 메뉴 항목에서 부르면 된다.

use eframe::egui::{self, Color32, RichText, Vec2};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde_json::{json, Value};
use wp_core::core::{Ev, Shared};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Remote,
    Trigger,
    Devices,
    Lan,
}

#[derive(Default)]
struct St {
    open: Vec<Tool>,
    msg: String,
    // 장비 넣기 칸
    dv_kind: String,
    dv_name: String,
    dv_host: String,
    dv_port: String,
    dv_proto: String,
    dv_action: String,
    comp_loc: String,
}

static ST: Lazy<Mutex<St>> = Lazy::new(|| Mutex::new(St { dv_kind: "x32".into(), dv_proto: "sony".into(), ..Default::default() }));
/// 다른 스레드(네트워크)에서 끝난 결과 글
static RESULT: Lazy<Mutex<Option<String>>> = Lazy::new(|| Mutex::new(None));

pub fn open(t: Tool) {
    let mut s = ST.lock();
    if !s.open.contains(&t) {
        s.open.push(t);
    }
}

/// 임시: 기존 상단 줄에 두는 '도구' 단추 (상단 메뉴 줄이 생기면 그쪽으로)
pub fn menu_button(ui: &mut egui::Ui) {
    ui.menu_button(wp_core::i18n::tr("도구"), |ui| {
        for (t, label) in [(Tool::Remote, "휴대폰 리모컨"), (Tool::Trigger, "스트림덱·외부 단추"), (Tool::Devices, "Companion·장비 (X32·PTZ)"), (Tool::Lan, "내부망 출력 (다른 PC 의 OBS)")] {
            if ui.button(label).clicked() {
                open(t);
                ui.close();
            }
        }
    });
}

fn qr(ui: &mut egui::Ui, text: &str, size: f32) {
    let Ok(code) = qrcode::QrCode::new(text.as_bytes()) else { return };
    let w = code.width();
    let colors = code.to_colors();
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 4.0, Color32::WHITE);
    let quiet = 2.0;
    let cell = size / (w as f32 + 2.0 * quiet);
    for (i, c) in colors.iter().enumerate() {
        if *c == qrcode::Color::Dark {
            let (x, y) = ((i % w) as f32 + quiet, (i / w) as f32 + quiet);
            p.rect_filled(egui::Rect::from_min_size(rect.min + Vec2::new(x * cell, y * cell), Vec2::splat(cell + 0.3)), 0.0, Color32::BLACK);
        }
    }
}

fn copy_row(ui: &mut egui::Ui, label: &str, url: &str) {
    ui.horizontal(|ui| {
        if !label.is_empty() {
            ui.label(RichText::new(wp_core::i18n::tr(label)).small());
        }
        ui.label(RichText::new(url).monospace().small());
        if ui.small_button(wp_core::i18n::tr("복사")).clicked() {
            ui.ctx().copy_text(url.to_string());
        }
    });
}

fn changed(core: &Shared, f: impl FnOnce(&mut Value)) {
    let mut c = core.lock();
    c.settings.update(f);
    c.emit(Ev::Ui);
}

/// 매 프레임: 열린 도구 창을 그린다
pub fn show(ctx: &egui::Context, core: &Shared) {
    // 점검용: --tool remote|trigger|devices|lan
    static ARGS: std::sync::Once = std::sync::Once::new();
    ARGS.call_once(|| {
        let a: Vec<String> = std::env::args().collect();
        for (i, x) in a.iter().enumerate() {
            if x == "--tool" {
                match a.get(i + 1).map(|s| s.as_str()) { Some("remote") => open(Tool::Remote), Some("trigger") => open(Tool::Trigger), Some("devices") => open(Tool::Devices), Some("lan") => open(Tool::Lan), _ => {} }
            }
        }
    });
    if let Some(r) = RESULT.lock().take() {
        ST.lock().msg = r;
    }
    let open = ST.lock().open.clone();
    for t in open {
        let mut keep = true;
        let title = match t {
            Tool::Remote => "휴대폰 리모컨",
            Tool::Trigger => "스트림덱·외부 단추",
            Tool::Devices => "Companion·장비",
            Tool::Lan => "내부망 출력",
        };
        egui::Window::new(title).open(&mut keep).default_width(520.0).collapsible(false).show(ctx, |ui| match t {
            Tool::Remote => remote(ui, core),
            Tool::Trigger => trigger(ui, core),
            Tool::Devices => devices(ui, core),
            Tool::Lan => lan(ui, core),
        });
        if !keep {
            ST.lock().open.retain(|x| *x != t);
        }
    }
}

fn remote(ui: &mut egui::Ui, core: &Shared) {
    let (st, mut en, port) = {
        let c = core.lock();
        let r = &c.settings.get()["remote"];
        (wp_core::remote::status(&c), r["enabled"].as_bool().unwrap_or(false), c.port)
    };
    ui.label(wp_core::i18n::tr("같은 Wi-Fi 의 휴대폰·태블릿 브라우저에서 다음·이전·숨김·순서 송출을 합니다."));
    if ui.checkbox(&mut en, wp_core::i18n::tr("휴대폰 리모컨 켜기")).changed() {
        changed(core, |d| {
            d["remote"]["enabled"] = json!(en);
            if d["remote"]["pin"].as_str().unwrap_or("").is_empty() {
                d["remote"]["pin"] = json!(wp_core::remote::new_pin());
            }
        });
    }
    ui.horizontal(|ui| {
        ui.label("PIN");
        ui.label(RichText::new(st["pin"].as_str().unwrap_or("-")).strong().size(20.0).monospace());
        if ui.button(wp_core::i18n::tr("새 PIN")).clicked() {
            changed(core, |d| d["remote"]["pin"] = json!(wp_core::remote::new_pin()));
        }
        ui.separator();
        ui.label(wp_core::i18n::tr("포트"));
        let mut p = st["port"].as_u64().unwrap_or(37540).to_string();
        if ui.add(egui::TextEdit::singleline(&mut p).desired_width(60.0)).lost_focus() {
            if let Ok(n) = p.trim().parse::<u16>() {
                changed(core, |d| d["remote"]["port"] = json!(n));
            }
        }
    });
    let err = st["error"].as_str().unwrap_or("");
    if !err.is_empty() {
        ui.colored_label(Color32::from_rgb(0xff, 0x8a, 0x8a), err);
    }
    if st["running"] == json!(true) {
        let urls: Vec<String> = st["urls"].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(String::from)).collect();
        ui.horizontal(|ui| {
            if let Some(u) = urls.first() {
                qr(ui, u, 150.0);
            }
            ui.vertical(|ui| {
                ui.label(RichText::new(wp_core::i18n::tr("휴대폰 카메라로 QR 을 찍거나 주소를 여세요:")).small());
                for u in &urls {
                    copy_row(ui, "", u);
                }
                if urls.is_empty() {
                    ui.label(RichText::new(wp_core::i18n::tr("이 PC 의 내부망 주소를 찾지 못했습니다 (Wi-Fi·유선 연결 확인).")).small().color(Color32::GRAY));
                }
            });
        });
    } else if en {
        ui.label(RichText::new(wp_core::i18n::tr("서버를 켜는 중…")).small());
    }
    ui.separator();
    ui.label(RichText::new(wp_core::i18n::tr("OBS 도킹 (이 PC 안에서 PIN 없이): OBS → 도킹 → 사용자 정의 브라우저 도킹에 넣기")).small());
    copy_row(ui, "", &format!("http://127.0.0.1:{port}/dock"));
}

fn trigger(ui: &mut egui::Ui, core: &Shared) {
    let (info, port) = {
        let mut c = core.lock();
        let i = wp_core::trigger::info(&mut c);
        (i, c.port)
    };
    let key = info["key"].as_str().unwrap_or("").to_string();
    ui.label(wp_core::i18n::tr("스트림덱(웹 요청)·Companion·다른 프로그램에서 아래 주소를 부르면 그 동작을 합니다 (이 PC 안에서만)."));
    ui.horizontal(|ui| {
        ui.label(wp_core::i18n::tr("열쇠"));
        ui.label(RichText::new(&key).monospace());
        if ui.button(wp_core::i18n::tr("새 열쇠 (예전 주소는 안 됨)")).clicked() {
            let mut c = core.lock();
            wp_core::trigger::new_key(&mut c);
        }
    });
    egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
        for a in info["actions"].as_array().into_iter().flatten() {
            let id = a["id"].as_str().unwrap_or("");
            copy_row(ui, a["name"].as_str().unwrap_or(""), &format!("http://127.0.0.1:{port}/api/trigger/{id}?key={key}"));
        }
    });
}

fn run_bg(f: impl FnOnce() -> Result<Value, String> + Send + 'static) {
    ST.lock().msg = wp_core::i18n::tr("보내는 중…").into();
    std::thread::spawn(move || {
        let r = f();
        *RESULT.lock() = Some(match r {
            Ok(v) => format!("보냈습니다 {v}"),
            Err(e) => e,
        });
    });
}

fn devices(ui: &mut egui::Ui, core: &Shared) {
    let (comp, devs) = {
        let c = core.lock();
        (c.settings.get()["companion"].clone(), c.settings.get()["devices"].clone())
    };
    ui.label(RichText::new(wp_core::i18n::tr("Companion (Bitfocus Companion 3 이상)")).strong());
    ui.horizontal(|ui| {
        let mut host = comp["host"].as_str().unwrap_or("127.0.0.1").to_string();
        let mut port = comp["port"].as_u64().unwrap_or(8000).to_string();
        ui.label(wp_core::i18n::tr("주소"));
        if ui.add(egui::TextEdit::singleline(&mut host).desired_width(130.0)).changed() {
            changed(core, |d| d["companion"]["host"] = json!(host.trim()));
        }
        ui.label(wp_core::i18n::tr("포트"));
        if ui.add(egui::TextEdit::singleline(&mut port).desired_width(60.0)).changed() {
            if let Ok(n) = port.trim().parse::<u16>() {
                changed(core, |d| d["companion"]["port"] = json!(n));
            }
        }
    });
    {
        let mut s = ST.lock();
        ui.horizontal(|ui| {
            ui.label(wp_core::i18n::tr("단추 위치"));
            ui.add(egui::TextEdit::singleline(&mut s.comp_loc).hint_text("1/0/2, 2/1/0").desired_width(150.0));
            if ui.button(wp_core::i18n::tr("눌러 보기")).clicked() {
                let (c2, loc) = (comp.clone(), s.comp_loc.clone());
                drop(s);
                run_bg(move || wp_core::companion::press(&c2, &loc));
            }
        });
    }
    ui.separator();
    ui.label(RichText::new(wp_core::i18n::tr("장비 직접 (Companion 없이)")).strong());
    let mut del = None;
    let list = devs.as_array().cloned().unwrap_or_default();
    for d in &list {
        ui.horizontal(|ui| {
            let kind = if d["kind"] == "x32" { wp_core::i18n::tr_s("X32 음향") } else { wp_core::i18n::tr_s("PTZ 카메라") };
            ui.label(format!("{} · {} · {}:{}", d["name"].as_str().unwrap_or(""), kind, d["host"].as_str().unwrap_or(""), d["port"]));
            let mut s = ST.lock();
            ui.add(egui::TextEdit::singleline(&mut s.dv_action).hint_text(if d["kind"] == "x32" { "ch1 on, scene 3" } else { wp_core::i18n::tr_s("프리셋 번호·home") }).desired_width(140.0));
            if ui.button(wp_core::i18n::tr("보내기")).clicked() {
                let (all, id, act) = (devs.clone(), d["id"].as_str().unwrap_or("").to_string(), s.dv_action.clone());
                drop(s);
                run_bg(move || wp_core::devices::run(&all, &id, &act));
            } else {
                drop(s);
            }
            if ui.small_button(wp_core::i18n::tr("지우기")).clicked() {
                del = d["id"].as_str().map(String::from);
            }
        });
    }
    if let Some(id) = del {
        changed(core, |d| {
            if let Some(a) = d["devices"].as_array_mut() {
                a.retain(|x| x["id"] != id.as_str());
            }
        });
    }
    let mut s = ST.lock();
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("dvkind").selected_text(if s.dv_kind == "x32" { "X32·M32" } else { "PTZ (VISCA)" }).show_ui(ui, |ui| {
            ui.selectable_value(&mut s.dv_kind, "x32".to_string(), "X32·M32");
            ui.selectable_value(&mut s.dv_kind, "visca".to_string(), "PTZ (VISCA)");
        });
        ui.add(egui::TextEdit::singleline(&mut s.dv_name).hint_text(wp_core::i18n::tr("이름")).desired_width(80.0));
        ui.add(egui::TextEdit::singleline(&mut s.dv_host).hint_text("192.168.1.50").desired_width(110.0));
        ui.add(egui::TextEdit::singleline(&mut s.dv_port).hint_text(wp_core::i18n::tr("포트")).desired_width(50.0));
        if s.dv_kind == "visca" {
            ui.selectable_value(&mut s.dv_proto, "sony".to_string(), wp_core::i18n::tr("Sony 머리글"));
            ui.selectable_value(&mut s.dv_proto, "raw".to_string(), wp_core::i18n::tr("그대로"));
        }
        if ui.button(wp_core::i18n::tr("넣기")).clicked() {
            let x = json!({ "kind": s.dv_kind, "name": s.dv_name, "host": s.dv_host, "port": s.dv_port.trim().parse::<u16>().ok(), "proto": s.dv_proto });
            match wp_core::devices::normalize_device(&x) {
                Ok(d) => {
                    s.dv_name.clear();
                    s.dv_host.clear();
                    s.dv_port.clear();
                    changed(core, |st| {
                        if !st["devices"].is_array() {
                            st["devices"] = json!([]);
                        }
                        st["devices"].as_array_mut().unwrap().push(d);
                    });
                }
                Err(e) => s.msg = e,
            }
        }
    });
    if !s.msg.is_empty() {
        ui.label(RichText::new(&s.msg).small().color(Color32::from_rgb(0xff, 0xd4, 0x79)));
    }
    ui.label(RichText::new(wp_core::i18n::tr("X32: ch1 on · ch5 fader 0.75 · main off · scene 3 · dca1 off  /  PTZ: 프리셋 0~254 · home")).small().color(Color32::GRAY));
}

fn lan(ui: &mut egui::Ui, core: &Shared) {
    let st = { let mut c = core.lock(); wp_core::lan::info(&mut c) };
    let mut en = st["enabled"].as_bool().unwrap_or(false);
    ui.label(wp_core::i18n::tr("송출 PC 가 따로 있으면, 그 PC 의 OBS 에서 이 PC 의 출력 화면을 브라우저 소스로 엽니다 (열쇠가 든 주소로만)."));
    ui.horizontal(|ui| {
        if ui.checkbox(&mut en, wp_core::i18n::tr("내부망 출력 켜기")).changed() {
            changed(core, |d| d["lanOutput"]["enabled"] = json!(en));
        }
        ui.label(wp_core::i18n::tr("포트"));
        let mut p = st["port"].as_u64().unwrap_or(37550).to_string();
        if ui.add(egui::TextEdit::singleline(&mut p).desired_width(60.0)).lost_focus() {
            if let Ok(n) = p.trim().parse::<u16>() {
                changed(core, |d| d["lanOutput"]["port"] = json!(n));
            }
        }
        if ui.button(wp_core::i18n::tr("새 열쇠")).clicked() {
            let mut c = core.lock();
            wp_core::lan::new_key(&mut c);
        }
    });
    let err = st["error"].as_str().unwrap_or("");
    if !err.is_empty() {
        ui.colored_label(Color32::from_rgb(0xff, 0x8a, 0x8a), err);
    }
    for (k, label) in [("main", "예배당 화면"), ("overlay", "투명 자막"), ("stage", "무대 화면")] {
        for u in st[k].as_array().into_iter().flatten() {
            copy_row(ui, label, u.as_str().unwrap_or(""));
        }
    }
}

/// 리모컨에서 '끄기' 를 눌렀는지 (조작 화면이 예배당 창을 닫는다)
pub fn take_off_request() -> bool {
    wp_core::remote::OFF_REQUEST.swap(false, std::sync::atomic::Ordering::SeqCst)
}
