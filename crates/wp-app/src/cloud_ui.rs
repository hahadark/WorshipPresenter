//! 클라우드 연결 창 (웹판 1단계, wp_core::cloud). 기본 꺼짐.
//! - 여는 곳: 메뉴 연결 → 클라우드 연결(`d:cloud`), 휴대폰 리모컨 · 웹 편집기 창의 '클라우드 연결…' 단추, 점검 인자 `--tool cloud`.
//! - 주소 · 연결 열쇠 · 받을 것(리모컨 · 웹 편집기) · 상태 · 클라우드로 보고 있는 화면 수.

use eframe::egui::{self, RichText};
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};
use wp_core::core::{Ev, Shared};

static OPEN: AtomicBool = AtomicBool::new(false);

#[derive(Default)]
struct St {
    key: String,
    msg: String,
}
static ST: once_cell::sync::Lazy<parking_lot::Mutex<St>> = once_cell::sync::Lazy::new(Default::default);

pub fn open() {
    OPEN.store(true, Ordering::SeqCst);
}

/// 다른 창(리모컨 · 웹 편집기)에 두는 여는 단추
pub fn entry_button(ui: &mut egui::Ui) {
    let st = wp_core::cloud::status();
    let label = match st.state.as_str() {
        "online" => wp_core::i18n::tr_args("클라우드 연결됨 (화면 {1}개)…", &[&st.viewers.len()]),
        "off" => wp_core::i18n::tr("클라우드 연결 (교회 밖에서도 열기)…"),
        _ => wp_core::i18n::tr("클라우드 연결 (끊김)…"),
    };
    if ui.button(label).clicked() {
        open();
    }
}

fn changed(core: &Shared, f: impl FnOnce(&mut serde_json::Value)) {
    let mut c = core.lock();
    c.settings.update(|d| {
        if !d["cloud"].is_object() {
            d["cloud"] = json!({});
        }
        f(&mut d["cloud"]);
    });
    c.emit(Ev::Ui);
}

pub fn show(ctx: &egui::Context, core: &Shared) {
    static ARGS: std::sync::Once = std::sync::Once::new();
    ARGS.call_once(|| {
        let a: Vec<String> = std::env::args().collect();
        if a.windows(2).any(|w| w[0] == "--tool" && w[1] == "cloud") {
            open();
        }
    });
    if !OPEN.load(Ordering::SeqCst) {
        return;
    }
    let mut keep = true;
    crate::widgets::float_window(ctx, "cloud", wp_core::i18n::tr("클라우드 연결"), [520.0, 420.0]).open(&mut keep).show(ctx, |ui| body(ui, core));
    if !keep {
        OPEN.store(false, Ordering::SeqCst);
    }
}

fn body(ui: &mut egui::Ui, core: &Shared) {
    use crate::widgets::{form_row, group_head};
    let tr = wp_core::i18n::tr;
    let p = crate::theme::pal();
    let (cf, has_key) = { let c = core.lock(); (wp_core::cloud::conf(&c), wp_core::cloud::has_key(&c)) };
    let st = wp_core::cloud::status();
    ui.add(egui::Label::new(RichText::new(tr("예배당 화면 · OBS 자막 · 무대 화면 · 휴대폰 리모컨 · 웹 편집기를 클라우드 주소에서 엽니다 (교회 밖에서도). 이 PC 는 밖으로 나가는 연결 하나만 열고, 포트나 방화벽은 바꾸지 않습니다.")).small().color(p.muted)).wrap());
    ui.add(egui::Label::new(RichText::new(tr("켜면 지금 송출하는 화면 글(가사 · 성경 본문 · 공지)과 리모컨에 보이는 큐시트 순서 이름, 화면이 청한 그림 · 영상이 로그인한 사람만 들어오는 교회 클라우드로 갑니다. 끄면 아무것도 보내지 않습니다.")).small().color(p.muted)).wrap());
    ui.add_space(4.0);
    let mut en = cf.enabled;
    if form_row(ui, "", |ui| ui.checkbox(&mut en, tr("클라우드 연결 켜기"))).changed() {
        changed(core, |d| d["enabled"] = json!(en));
    }
    let mut url = cf.url.clone();
    form_row(ui, &tr("클라우드 주소"), |ui| {
        let r = ui.add(egui::TextEdit::singleline(&mut url).desired_width(300.0).hint_text("https://….workers.dev"));
        if r.lost_focus() && url.trim() != cf.url {
            let u = url.trim().trim_end_matches('/').to_string();
            ST.lock().msg = match wp_core::cloud::bridge_url(&u) { Ok(_) => String::new(), Err(e) => e };
            changed(core, |d| d["url"] = json!(u));
        }
    });
    form_row(ui, &tr("연결 열쇠"), |ui| ui.horizontal(|ui| {
        let mut s = ST.lock();
        ui.add(egui::TextEdit::singleline(&mut s.key).password(true).desired_width(200.0).hint_text(if has_key { tr("넣어 둠 (바꾸려면 새로)") } else { tr("교회.열쇠") }));
        if ui.add_enabled(!s.key.trim().is_empty(), egui::Button::new(tr("넣기"))).clicked() {
            let k = s.key.trim().to_string();
            if wp_core::cloud::key_ok(&k) {
                let mut c = core.lock();
                wp_core::cloud::set_key(&mut c, &k);
                c.emit(Ev::Ui);
                s.key.clear();
                s.msg = tr("연결 열쇠를 넣었습니다 (이 PC 에 암호화해 둠).");
            } else {
                s.msg = tr("열쇠 모양이 아닙니다. 클라우드 주인 화면 → 기기 열쇠 → '설치판 연결' 로 만든 '교회.열쇠' 를 그대로 붙여 넣으세요.");
            }
        }
        if has_key && ui.button(tr("지우기")).clicked() {
            let mut c = core.lock();
            wp_core::cloud::set_key(&mut c, "");
            c.emit(Ev::Ui);
        }
    }));
    let (mut rm, mut ed) = (cf.remote, cf.editor);
    form_row(ui, &tr("받을 것"), |ui| ui.horizontal(|ui| {
        if ui.checkbox(&mut rm, tr("휴대폰 리모컨")).changed() {
            changed(core, |d| d["remote"] = json!(rm));
        }
        if ui.checkbox(&mut ed, tr("웹 편집기 (큐시트 고치기)")).changed() {
            changed(core, |d| d["editor"] = json!(ed));
        }
    }));
    let msg = ST.lock().msg.clone();
    if !msg.is_empty() {
        ui.label(RichText::new(msg).small().color(p.muted));
    }
    crate::cloudsync_ui::section(ui, core); // 2단계 B 자료 동기화
    group_head(ui, &tr("상태"));
    ui.horizontal(|ui| {
        let (mark, text) = match st.state.as_str() {
            "online" => ("ok", tr("연결됨")),
            "connecting" => ("info", tr("잇는 중…")),
            "error" => ("warn", tr("끊김 — 다시 잇는 중")),
            _ => ("info", tr("꺼짐")),
        };
        crate::widgets::status_mark(ui, mark);
        ui.label(text);
        if st.state == "online" {
            ui.label(RichText::new(wp_core::i18n::tr_args("클라우드로 보는 화면 {1}개 · 보낸 화면 {2}번 · 받은 요청 {3}번", &[&st.viewers.len(), &st.sent, &st.requests])).small().color(p.muted));
        }
    });
    if !st.error.is_empty() {
        ui.add(egui::Label::new(RichText::new(&st.error).small().color(p.danger_text)).wrap());
    }
    if st.state == "online" && !st.viewers.is_empty() {
        let mut by: std::collections::BTreeMap<&str, usize> = Default::default();
        for (c, _) in &st.viewers {
            *by.entry(c.as_str()).or_default() += 1;
        }
        let names = by.iter().map(|(k, n)| format!("{k} {n}")).collect::<Vec<_>>().join(" · ");
        ui.label(RichText::new(names).small().monospace());
    }
    if !cf.url.is_empty() {
        ui.horizontal(|ui| {
            for (label, path) in [("주인 화면 열기", "/owner"), ("예배당 화면", "/outputs/main"), ("리모컨", "/remote")] {
                if ui.small_button(tr(label)).clicked() {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(format!("{}{path}", cf.url)));
                }
            }
        });
    }
    ui.label(RichText::new(tr("OBS · 다른 PC · 휴대폰에 넣을 주소와 PIN 은 주인 화면의 '기기 열쇠' 에서 기기마다 만듭니다. 인터넷이 끊겨도 이 PC 의 예배당 화면 · OBS(127.0.0.1)는 그대로 송출되고, 다시 이어지면 클라우드 화면이 지금 것을 받습니다.")).small().color(p.muted));
}
