//! 연결 → 카메라 동시 열기 시험 (로드맵 3단계 ⑤): 고른 카메라를 예배당 출력 창(wp-output)과 브라우저에서 동시에 열어
//! 되는지 · 안 되는지 · 장치 이름 · 해상도를 글로 보이고 복사한다. 코어 쪽 페이지 · 결과는 wp_core::camtest.
//! 내가 띄운 시험 창(PID)만 끈다. 시험 창은 3분 뒤 스스로 카메라를 닫는다.
use crate::app::App;
use crate::theme::{self, pal, Weight};
use crate::widgets::{self, Btn, Size};
use eframe::egui;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::process::{Child, Command};
use wp_core::i18n::tr;

#[derive(Default)]
struct St {
    open: bool,
    cam: String,
    label: String,
    token: String,
    child: Option<Child>,
}

static ST: Lazy<Mutex<St>> = Lazy::new(|| Mutex::new(St::default()));

pub fn open() {
    ST.lock().open = true;
}

pub fn is_open() -> bool {
    ST.lock().open
}

fn enc(s: &str) -> String {
    percent_encoding::utf8_percent_encode(s, percent_encoding::NON_ALPHANUMERIC).to_string()
}

/// 시험 페이지 주소 (who = window · browser · obs)
pub fn page_url(port: u16, token: &str, who: &str, id: &str, label: &str) -> String {
    format!("http://127.0.0.1:{port}/helper/camtest?token={}&who={who}&id={}&label={}", enc(token), enc(id), enc(label))
}

fn open_in_browser(url: &str) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).creation_flags(0x0800_0000).spawn();
    }
    #[cfg(target_os = "macos")]
    let _ = Command::new("open").arg(url).spawn();
    #[cfg(not(any(windows, target_os = "macos")))]
    let _ = url;
}

fn stop_child(st: &mut St) {
    if let Some(mut c) = st.child.take() {
        let _ = c.kill();
        let _ = c.wait();
    }
}

fn who_name(w: &str) -> String {
    tr(match w {
        "window" => "예배당 출력 창",
        "browser" => "브라우저 (방송 주소 쪽)",
        _ => "OBS 브라우저 소스",
    })
}

pub fn frame(app: &mut App, ctx: &egui::Context) {
    if !ST.lock().open {
        return;
    }
    let p = pal();
    let mut keep = true;
    let (choices, port) = {
        let c = app.core.lock();
        (c.camera_choices(), c.port)
    };
    crate::widgets_win::window(ctx, "camtest", tr("카메라 동시 열기 시험"), [620.0, 480.0]).open(&mut keep).show(ctx, |ui| {
        ui.add(egui::Label::new(theme::rich(tr("같은 카메라를 예배당 출력 창과 방송 쪽(브라우저 · OBS)에서 동시에 쓸 수 있는지 봅니다. 실제 웹캠 · 캡처보드는 한 프로그램만 잡게 하는 경우가 있어 교회 PC 에서 직접 확인해야 합니다."), 11.5, Weight::Regular, p.muted)).wrap());
        ui.add_space(6.0);
        let mut st = ST.lock();
        ui.horizontal_wrapped(|ui| {
            ui.label(theme::rich(tr("카메라"), 12.5, Weight::Regular, p.text2));
            let cur = choices.iter().find(|c| c.device_id == st.cam).map(|c| c.label.clone()).unwrap_or_else(|| tr("카메라 고르기"));
            let mut pick = st.cam.clone();
            egui::ComboBox::from_id_salt("camtest-cam").width(300.0).selected_text(cur).show_ui(ui, |ui| {
                for c in choices.iter().filter(|c| !c.device_id.starts_with("phone:")) {
                    ui.selectable_value(&mut pick, c.device_id.clone(), &c.label);
                }
            });
            if pick != st.cam {
                st.cam = pick;
                st.label = choices.iter().find(|c| c.device_id == st.cam).map(|c| c.label.clone()).unwrap_or_default();
            }
            if ui.add(Btn::new(tr("카메라 다시 찾기")).icon("refresh-cw").size(Size::Sm)).clicked() {
                crate::camscan::probe(&app.core);
            }
        });
        if choices.is_empty() {
            ui.add(egui::Label::new(theme::rich(tr("찾은 카메라가 없습니다. '카메라 다시 찾기' 를 눌러 보세요."), 11.5, Weight::Regular, p.muted2)).wrap());
        }
        ui.add_space(4.0);
        let running = !st.token.is_empty() && wp_core::camtest::active();
        ui.horizontal(|ui| {
            let b = ui.add(Btn::new(if running { tr("다시 시험") } else { tr("시험 시작") }).icon("play").kind(widgets::Kind::Primary).enabled(!st.cam.is_empty()).tooltip(tr("예배당 출력 창(작은 창)과 기본 브라우저에서 같은 카메라를 엽니다")));
            widgets::mark("camtest-start", b.rect);
            if b.clicked() {
                stop_child(&mut st);
                let token = wp_core::camtest::start(&st.label);
                st.token = token.clone();
                let url = page_url(port, &token, "window", &st.cam, &st.label);
                let mut cmd = Command::new(crate::outwin::exe());
                cmd.args(["--windowed", "--title", &tr("카메라 동시 열기 시험 - 예배당 출력 창"), "--url", &url]);
                match cmd.spawn() {
                    Ok(c) => {
                        crate::outwin::bind_to_app(&c);
                        st.child = Some(c);
                    }
                    Err(e) => app.core.lock().notify(format!("{}: {e}", tr("출력 창을 띄우지 못했습니다")), "error"),
                }
                // 브라우저는 조금 뒤에 (창이 먼저 장치를 잡은 상태에서 두 번째로 열기 — OBS 가 나중에 여는 것과 같은 차례)
                let burl = page_url(port, &token, "browser", &st.cam, &st.label);
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(2500));
                    open_in_browser(&burl);
                });
            }
            if running && ui.add(Btn::new(tr("그만")).icon("x").size(Size::Sm)).clicked() {
                stop_child(&mut st);
                wp_core::camtest::stop();
                st.token.clear();
            }
        });
        if !st.token.is_empty() {
            let obs = page_url(port, &st.token, "obs", &st.cam, &st.label);
            ui.horizontal(|ui| {
                ui.add(egui::Label::new(theme::rich(tr("OBS 에서도 보려면 이 주소를 브라우저 소스로:"), 11.5, Weight::Regular, p.muted)).wrap());
                if ui.add(Btn::new(tr("복사")).icon("copy").size(Size::Sm)).clicked() {
                    widgets::copy_notice(ui.ctx(), obs.clone(), tr("시험 주소를 복사했습니다"));
                }
            });
        }
        ui.separator();
        let now = wp_core::util::now_ms();
        let rows: Vec<(String, String)> = wp_core::camtest::results().iter().map(|(w, r)| (who_name(w), wp_core::camtest::line(r, now))).collect();
        if rows.is_empty() {
            ui.add(egui::Label::new(theme::rich(if st.token.is_empty() { tr("카메라를 고르고 '시험 시작' 을 누르세요. 브라우저가 카메라를 써도 되는지 물으면 허용을 누릅니다.") } else { tr("여는 중…") }, 12.0, Weight::Regular, p.muted)).wrap());
        }
        for (w, l) in &rows {
            ui.horizontal_wrapped(|ui| {
                ui.label(theme::rich(w, 12.5, Weight::SemiBold, p.text));
                ui.add(egui::Label::new(theme::rich(l, 12.0, Weight::Regular, p.text2)).wrap().selectable(true));
            });
        }
        if !rows.is_empty() {
            let both = rows.len() >= 2 && wp_core::camtest::results().iter().take(2).all(|(_, r)| r["ok"] == true && r["frames"].as_f64().unwrap_or(0.0) > 0.0);
            let verdict = if rows.len() < 2 {
                tr("두 번째 곳(브라우저)을 기다리는 중입니다.")
            } else if both {
                tr("결과: 두 곳에서 동시에 열립니다. 이 카메라는 예배당 화면과 방송에 같이 쓸 수 있습니다.")
            } else {
                tr("결과: 동시에 열리지 않습니다. 이 카메라는 한 곳(예배당 화면 또는 OBS 의 비디오 캡처 장치)에서만 쓰세요.")
            };
            ui.add_space(4.0);
            ui.add(egui::Label::new(theme::rich(&verdict, 12.5, Weight::SemiBold, if both { p.ok_text } else { p.text })).wrap());
            if ui.add(Btn::new(tr("결과 복사")).icon("clipboard-copy").size(Size::Sm)).clicked() {
                let mut txt = wp_core::camtest::summary(&st.label, &rows);
                txt.push_str(&verdict);
                widgets::copy_notice(ui.ctx(), txt, tr("시험 결과를 복사했습니다"));
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }
    });
    if !keep {
        let mut st = ST.lock();
        st.open = false;
        stop_child(&mut st);
        wp_core::camtest::stop();
        st.token.clear();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn page_url_encodes_label() {
        let u = super::page_url(37528, "ct1", "browser", "abc", "USB 카메라 (HD)");
        assert!(u.starts_with("http://127.0.0.1:37528/helper/camtest?token=ct1&who=browser&id=abc&label="));
        assert!(!u.contains(' '));
    }
}
