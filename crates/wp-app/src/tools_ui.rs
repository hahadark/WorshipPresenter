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
    PhoneCam,
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

/// 시험용: 그 도구 창이 열려 있었는지 돌려주고 닫는다 (설정 센터의 '… 열기' 단추 시험)
#[cfg(test)]
pub fn close_for_test(t: Tool) -> bool {
    let mut s = ST.lock();
    let was = s.open.contains(&t);
    s.open.retain(|x| *x != t);
    was
}

/// 임시: 기존 상단 줄에 두는 '도구' 단추 (상단 메뉴 줄이 생기면 그쪽으로)
pub fn menu_button(ui: &mut egui::Ui) {
    ui.menu_button(wp_core::i18n::tr("도구"), |ui| {
        for (t, label) in [(Tool::Remote, "휴대폰 리모컨"), (Tool::Trigger, "스트림덱·외부 단추"), (Tool::Devices, "Companion·장비 (X32·PTZ)"), (Tool::Lan, "내부망 출력 (다른 PC 의 OBS)"), (Tool::PhoneCam, "휴대폰 카메라")] {
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
            crate::widgets::copy_notice(ui.ctx(), url, wp_core::i18n::tr("주소를 복사했습니다."));
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
                match a.get(i + 1).map(|s| s.as_str()) { Some("remote") => open(Tool::Remote), Some("trigger") => open(Tool::Trigger), Some("devices") => open(Tool::Devices), Some("lan") => open(Tool::Lan), Some("phonecam") => open(Tool::PhoneCam), _ => {} }
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
            Tool::PhoneCam => "휴대폰 카메라",
        };
        // 떠 있는 창 기본 자리 (widgets::float_pos) · 제목 번역
        let wid = egui::Id::unique(("wp-float", title));
        crate::widgets::close_on_outside(ctx, wid, &mut keep);
        egui::Window::new(wp_core::i18n::tr(title)).id(wid).default_pos(crate::widgets::float_pos(ctx, wid, 520.0)).open(&mut keep).default_width(520.0).collapsible(false).show(ctx, |ui| match t {
            Tool::Remote => remote(ui, core),
            Tool::Trigger => trigger(ui, core),
            Tool::Devices => devices(ui, core),
            Tool::Lan => lan(ui, core),
            Tool::PhoneCam => phonecam(ui, core),
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
        if ui.add(egui::TextEdit::singleline(&mut p).desired_width(78.0)).lost_focus() {
            if let Ok(n) = p.trim().parse::<u16>() {
                changed(core, |d| d["remote"]["port"] = json!(n));
            }
        }
    });
    let err = st["error"].as_str().unwrap_or("");
    if !err.is_empty() {
        ui.colored_label(Color32::from_rgb(0xff, 0x8a, 0x8a), err);
    }
    if en {
        crate::firewall_ui::row(ui);
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
            // '1/0/2' 같은 값을 쉬운 말로 (휴리스틱 평가 12번)
            ui.label(wp_core::i18n::tr("단추 자리 (페이지/줄/칸)"));
            ui.add(egui::TextEdit::singleline(&mut s.comp_loc).hint_text("1/0/2, 2/1/0").desired_width(150.0))
                .on_hover_text(wp_core::i18n::tr("Companion 화면의 단추 자리를 '페이지/줄/칸' 숫자로 씁니다. 예: 1/0/2 = 1페이지 맨 윗줄(0) 세 번째 칸(2). 여러 개는 쉼표로: 1/0/2, 1/1/0"));
            let ok = !wp_core::companion::parse_locations(&s.comp_loc).is_empty();
            if ui.add_enabled(ok, egui::Button::new(wp_core::i18n::tr("지금 한 번 눌러 보기"))).on_disabled_hover_text(wp_core::i18n::tr("단추 자리를 먼저 쓰세요 (예: 1/0/2)")).clicked() {
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
            if ui.add_enabled(!s.dv_action.trim().is_empty(), egui::Button::new(wp_core::i18n::tr("지금 한 번 보내 보기"))).on_disabled_hover_text(wp_core::i18n::tr("왼쪽 칸에 보낼 동작을 먼저 쓰세요")).clicked() {
                let (all, id, act) = (devs.clone(), d["id"].as_str().unwrap_or("").to_string(), s.dv_action.clone());
                drop(s);
                run_bg(move || wp_core::devices::run(&all, &id, &act));
            } else {
                drop(s);
            }
            let did = d["id"].as_str().unwrap_or("").to_string();
            let q = wp_core::i18n::tr_args("'{1}' 장비 등록을 지웁니다.", &[&(d["name"].as_str().unwrap_or(""))]);
            if crate::widgets::confirm_delete(ui, egui::Id::unique(("dev-del", &did)), &wp_core::i18n::tr("지우기"), &q) {
                del = Some(did);
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
        ui.label(RichText::new(wp_core::i18n::tr_dyn(&s.msg)).small().color(crate::theme::pal().warn));
    }
    ui.label(RichText::new(wp_core::i18n::tr("보낼 동작 예 — X32 음향: ch1 on(1번 채널 켜기) · ch5 fader 0.75(5번 볼륨 75%) · main off · scene 3(장면 3) · dca1 off  /  PTZ 카메라: 프리셋 번호 0~254 · home(처음 자리)")).small().color(crate::theme::pal().muted));
}

fn lan(ui: &mut egui::Ui, core: &Shared) {
    let st = { let mut c = core.lock(); wp_core::lan::info(&mut c) };
    let mut en = st["enabled"].as_bool().unwrap_or(false);
    ui.label(wp_core::i18n::tr("송출 PC 가 따로 있으면, 그 PC 의 OBS 에서 이 PC 의 출력 화면을 브라우저 소스로 엽니다 (열쇠가 든 주소로만)."));
    if en {
        crate::firewall_ui::row(ui); // 다른 PC 에서 안 열리면 대개 방화벽
    }
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

// ---------------- 휴대폰 카메라 ----------------
fn phonecam(ui: &mut egui::Ui, core: &Shared) {
    use wp_core::i18n::tr;
    ui.ctx().request_repaint_after(std::time::Duration::from_secs(1)); // 휴대폰 목록이 저절로 바뀌게
    let (st, mut en) = {
        let c = core.lock();
        (wp_core::phonecam::status(&c), c.settings.get()["phoneCam"]["enabled"].as_bool().unwrap_or(false))
    };
    ui.label(tr("같은 Wi-Fi 의 휴대폰을 카메라로 씁니다. QR 을 찍어 브라우저로 열면 앱 설치 없이 바로 됩니다. 영상은 인터넷을 거치지 않고 이 PC 로 바로 옵니다."));
    ui.horizontal(|ui| {
        if ui.checkbox(&mut en, tr("휴대폰 카메라 켜기")).changed() {
            changed(core, |d| {
                if !d["phoneCam"].is_object() {
                    d["phoneCam"] = json!({});
                }
                d["phoneCam"]["enabled"] = json!(en);
                if d["phoneCam"]["key"].as_str().unwrap_or("").is_empty() {
                    d["phoneCam"]["key"] = json!(wp_core::phonecam::new_key());
                }
            });
        }
        ui.separator();
        ui.label(tr("포트"));
        let mut p = st["port"].as_u64().unwrap_or(wp_core::phonecam::DEFAULT_PORT as u64).to_string();
        if ui.add(egui::TextEdit::singleline(&mut p).desired_width(72.0)).lost_focus() {
            if let Ok(n) = p.trim().parse::<u16>() {
                changed(core, |d| d["phoneCam"]["port"] = json!(n));
            }
        }
        if ui.small_button(tr("새 열쇠")).on_hover_text(tr("QR 주소의 열쇠를 바꿉니다. 이미 연결한 휴대폰은 새 QR 을 다시 찍어야 합니다.")).clicked() {
            changed(core, |d| d["phoneCam"]["key"] = json!(wp_core::phonecam::new_key()));
        }
    });
    let err = st["error"].as_str().unwrap_or("");
    if !err.is_empty() {
        ui.colored_label(Color32::from_rgb(0xff, 0x8a, 0x8a), err);
    }
    if !en {
        return;
    }
    crate::firewall_ui::row(ui);
    if st["running"] == json!(true) {
        let urls: Vec<String> = st["urls"].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(String::from)).collect();
        ui.horizontal(|ui| {
            if let Some(u) = urls.first() {
                qr(ui, u, 170.0);
            }
            ui.vertical(|ui| {
                ui.label(RichText::new(tr("휴대폰 기본 카메라로 QR 을 찍으세요 (카카오톡 안에서 열면 카메라가 안 됩니다).")).small());
                for u in &urls {
                    copy_row(ui, "", u);
                }
                if urls.is_empty() {
                    ui.label(RichText::new(tr("이 PC 의 내부망 주소를 찾지 못했습니다 (Wi-Fi·유선 연결 확인).")).small().color(Color32::GRAY));
                }
                ui.add_space(4.0);
                ui.label(RichText::new(tr("처음 한 번 '연결이 비공개로 설정되어 있지 않습니다' 안내가 나옵니다. 이 PC 가 스스로 만든 인증서라서 나오는 것이니 [고급] → [… (으)로 이동(안전하지 않음)]을 누르세요 (아이폰: [세부사항 보기] → [이 웹 사이트 방문]).")).small().color(Color32::LIGHT_GRAY));
            });
        });
    }
    ui.separator();
    let phones = st["phones"].as_array().cloned().unwrap_or_default();
    if phones.is_empty() {
        ui.label(RichText::new(tr("아직 연결된 휴대폰이 없습니다.")).small().color(Color32::GRAY));
    }
    for p in phones {
        ui.horizontal_wrapped(|ui| {
            let online = p["online"].as_bool().unwrap_or(false);
            ui.label(RichText::new(if online { "●" } else { "○" }).color(if online { Color32::from_rgb(0x8a, 0xe0, 0xa8) } else { Color32::GRAY }));
            ui.label(RichText::new(format!("휴대폰 · {}", p["name"].as_str().unwrap_or(""))).strong());
            let viewers = p["viewers"].as_u64().unwrap_or(0);
            let size = p["size"].as_str().unwrap_or("");
            ui.label(RichText::new(if !online { tr("끊김") } else if viewers > 0 { wp_core::i18n::tr_args("화면 {1}곳에 보내는 중 · {2}", &[&viewers, &size]) } else { wp_core::i18n::tr_args("대기 · {1}", &[&size]) }).small());
            if ui.small_button(tr("큐시트에 카메라 순서로 넣기")).on_hover_text(tr("이 휴대폰 영상을 보여 주는 카메라 순서를 큐시트에 넣습니다")).clicked() {
                let name = format!("휴대폰 · {}", p["name"].as_str().unwrap_or(""));
                let mut it = wp_core::core::new_item("camera", &name);
                it["deviceId"] = p["deviceId"].clone();
                it["label"] = json!(name);
                it["fit"] = json!("cover");
                let mut c = core.lock();
                if c.cue.is_none() {
                    ST.lock().msg = tr("큐시트를 먼저 여세요.");
                } else {
                    c.add_item(it, None);
                    ST.lock().msg = tr("큐시트 끝에 카메라 순서를 넣었습니다.");
                }
            }
        });
    }
    ui.label(RichText::new(tr("다른 곳에서도 이 휴대폰을 고를 수 있습니다: 카메라 순서 · 템플릿 배경 · 화면 층의 카메라 목록에 '휴대폰 · 이름' 으로 나옵니다.")).small().color(Color32::GRAY));
    let msg = ST.lock().msg.clone();
    if !msg.is_empty() {
        ui.label(RichText::new(msg).small().color(Color32::LIGHT_GRAY));
    }
}