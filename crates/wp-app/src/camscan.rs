//! 카메라 찾기·미리보기 (조작 화면 셋이 함께 쓴다: 라이브러리 카메라 탭 · 큐시트 카메라 편집칸 · 카메라 설정 창)
//!
//! Electron 판은 조작 화면 자체가 Chromium 이라 getUserMedia → enumerateDevices 로 바로 목록을 얻었다.
//! Rust 판 조작 화면(egui)은 브라우저가 아니므로, 예배당 창과 같은 wp-output(WebView2)을 `--helper` 숨은 창으로
//! 잠깐 띄워 `/helper/cameras?token=…` 을 열게 한다. 그 페이지가 목록을 `/api/command cameraDevices` 로 보내고
//! 창을 닫으면 코어(`Core::camera_choices`)가 목록을 갖게 된다. deviceId 는 WebView2 프로필마다 다르므로
//! 반드시 출력 창과 같은 실행 파일·같은 주소(127.0.0.1:포트)로 열어야 출력이 그 id 로 카메라를 연다.
//! 미리보기는 `wp-output --windowed` 로 `/helper/camera?id=…` 를 연다 (출력과 같은 render.js 카메라 설정 적용).

use eframe::egui::{self, RichText};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::process::{Child, Command};
use wp_core::cameras::Choice;
use wp_core::core::Shared;
use wp_core::i18n::{tr, tr_s};

/// 내가 띄운 도우미·미리보기 프로세스 (내 PID 만 끈다). helper = 찾기 도우미(한 번에 하나)
struct Kids {
    helper: Option<Child>,
    previews: Vec<Child>,
}
static KIDS: Lazy<Mutex<Kids>> = Lazy::new(|| Mutex::new(Kids { helper: None, previews: vec![] }));

fn alive(c: &mut Child) -> bool {
    matches!(c.try_wait(), Ok(None))
}

/// 끝난 프로세스를 목록에서 뺀다 → 찾기 도우미가 살아 있나
fn reap() -> bool {
    let mut k = KIDS.lock();
    k.previews.retain_mut(alive);
    if k.helper.as_mut().is_some_and(|c| !alive(c)) {
        k.helper = None;
    }
    k.helper.is_some()
}

fn spawn(args: &[&str]) -> Result<Child, String> {
    let mut cmd = Command::new(crate::outwin::exe());
    cmd.args(args);
    crate::outwin::no_window(&mut cmd);
    let c = cmd.spawn().map_err(|e| format!("{}: {e}", tr("예배당 화면 프로그램을 띄우지 못했습니다")))?;
    crate::outwin::bind_to_app(&c);
    Ok(c)
}

/// 도우미 페이지 주소 (query 는 percent-encoding)
pub fn probe_url(port: u16, token: &str) -> String {
    format!("http://127.0.0.1:{port}/helper/cameras?token={}", enc(token))
}
pub fn preview_url(port: u16, id: &str, label: &str) -> String {
    format!("http://127.0.0.1:{port}/helper/camera?id={}&label={}", enc(id), enc(label))
}
fn enc(s: &str) -> String {
    percent_encoding::utf8_percent_encode(s, percent_encoding::NON_ALPHANUMERIC).to_string()
}

/// 카메라 찾기 시작: 코어에 일회용 토큰을 만들고 숨은 도우미 창을 띄운다. 이미 찾는 중이면 그냥 둔다.
pub fn probe(core: &Shared) {
    let (port, token) = {
        let mut c = core.lock();
        if c.camera_probing() {
            return;
        }
        (c.port, c.start_camera_probe())
    };
    match spawn(&["--helper", "--url", &probe_url(port, &token)]) {
        Ok(child) => KIDS.lock().helper = Some(child),
        Err(e) => core.lock().cancel_camera_probe(&e),
    }
}

/// 찾는 중인데 도우미 창이 결과 없이 꺼졌으면(페이지 오류 등) 찾기를 그만둔다 — 60초를 기다리지 않게
fn watch(core: &Shared) {
    let helper_alive = reap();
    let mut c = core.lock();
    if c.camera_probing() && !helper_alive {
        c.cancel_camera_probe(&tr("도우미 창이 결과를 보내지 않고 끝났습니다. 다시 찾아 보세요."));
    }
}

/// 카메라 미리보기 창 (작은 창, 사용자가 닫는다)
pub fn preview(core: &Shared, id: &str, label: &str) {
    let port = core.lock().port;
    let title = format!("📷 {}", if label.is_empty() { tr("기본 카메라") } else { label.to_string() });
    reap();
    match spawn(&["--windowed", "--title", &title, "--url", &preview_url(port, id, label)]) {
        Ok(child) => KIDS.lock().previews.push(child),
        Err(e) => core.lock().cancel_camera_probe(&e),
    }
}

/// 프로그램이 끝날 때 내가 띄운 도우미·미리보기만 끈다
pub fn kill_all() {
    let mut k = KIDS.lock();
    for mut c in k.helper.take().into_iter().chain(k.previews.drain(..)) {
        let _ = c.kill();
        let _ = c.wait();
    }
}

/// 화면 셋이 함께 쓰는 목록 (연결된 것 → 설정에만 있는 것 → 큐시트에만 있는 것)
pub fn choices(core: &wp_core::core::Core) -> Vec<Choice> {
    core.camera_choices()
}

/// 편집칸 콤보용 (deviceId, 이름). 연결된 카메라가 먼저 온다.
pub fn known_cameras(core: &wp_core::core::Core) -> Vec<(String, String)> {
    choices(core).into_iter().map(|c| (c.device_id, c.label)).collect()
}

/// 상태 글: 찾는 중 / 몇 대 연결 / 오류 / 아직 안 찾음 → (글, 오류인가)
pub fn status_text(core: &wp_core::core::Core) -> (String, bool) {
    let l = &core.b.cams;
    if core.camera_probing() {
        return (tr("카메라를 찾는 중…"), false);
    }
    if !l.error.is_empty() && l.devices.is_empty() {
        return (format!("{} {}", tr("카메라를 찾지 못했습니다:"), l.error), true);
    }
    if l.updated_at == 0 {
        return (tr("아직 찾지 않았습니다. '카메라 찾기'를 누르면 연결된 카메라 이름이 나옵니다."), false);
    }
    let ago = (wp_core::util::now_ms() - l.updated_at).max(0) / 1000;
    let when = if ago < 60 { tr("방금") } else if ago < 3600 { wp_core::i18n::tr_args("{1}분 전", &[&(ago / 60)]) } else { wp_core::i18n::tr_args("{1}시간 전", &[&(ago / 3600)]) };
    (wp_core::i18n::tr_args("연결된 카메라 {1}대 ({2})", &[&l.devices.len(), &when]), false)
}

/// 공용 조각: [카메라 찾기] 단추 + 상태 글. 화면 셋이 모두 이걸 쓴다.
pub fn scan_bar(ui: &mut egui::Ui, core: &Shared) {
    watch(core);
    let (text, is_err, probing) = {
        let c = core.lock();
        let (t, e) = status_text(&c);
        (t, e, c.camera_probing())
    };
    ui.horizontal_wrapped(|ui| {
        if ui.add_enabled(!probing, egui::Button::new(tr("카메라 찾기"))).on_hover_text(tr("예배당 화면과 같은 브라우저로 연결된 카메라를 찾습니다. 카메라를 잠깐 켜고 바로 끕니다.")).clicked() {
            probe(core);
        }
        ui.label(RichText::new(text).small().color(if is_err { crate::theme::pal().danger_text } else { crate::theme::pal().muted }));
    });
    if probing {
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
    }
}

/// 목록 한 줄의 표시 이름: 연결·설정 상태 표시를 붙인다
pub fn display_name(c: &Choice) -> String {
    let mut s = format!("📷 {}", c.label);
    if c.connected {
        s.push_str(tr_s(" · 연결됨"));
    }
    if c.configured {
        s.push_str(tr_s(" · 설정됨"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_are_encoded() {
        assert_eq!(probe_url(37528, "ab/c d"), "http://127.0.0.1:37528/helper/cameras?token=ab%2Fc%20d");
        let u = preview_url(1, "x&y", "USB 캡처");
        assert!(u.starts_with("http://127.0.0.1:1/helper/camera?id=x%26y&label=USB%20"), "{u}");
    }

    /// 화면 셋(라이브러리 카메라 탭 · 큐시트 카메라 편집칸 · 카메라 설정 창)이 같은 목록과 같은 찾기 조각을 쓴다
    #[test]
    fn three_screens_share_the_list() {
        let panel = include_str!("panel_ui.rs");
        let i = panel.find("fn panel_cameras").unwrap();
        let body = &panel[i..i + panel[i..].find("fn panel_templates").unwrap()];
        assert!(body.contains("camscan::choices") && body.contains("camscan::scan_bar"), "라이브러리 카메라 탭");
        assert!(!body.contains("[\"cameras\"]"), "설정을 직접 읽지 않고 공용 목록을 쓴다");

        let app = include_str!("app.rs");
        let i = app.find("fn editor(").unwrap();
        let body = &app[i..i + 700];
        assert!(body.contains("camscan::known_cameras") && body.contains("camscan::scan_bar"), "큐시트 카메라 편집칸");
        let parity = include_str!("parity_ui.rs");
        let i = parity.find("pub fn known_cameras").unwrap();
        assert!(parity[i..i + 200].contains("camscan::known_cameras"));

        let sys = include_str!("system_ui.rs");
        let i = sys.find("fn cameras_window").unwrap();
        let body = &sys[i..i + sys[i..].find("ST.lock().cameras = open;").unwrap()];
        assert!(body.contains("camscan::choices") && body.contains("camscan::scan_bar"), "카메라 설정 창");
    }

    #[test]
    fn display_marks() {
        let c = Choice { device_id: "a".into(), label: "캠".into(), connected: true, configured: false };
        assert_eq!(display_name(&c), "📷 캠 · 연결됨");
        let c2 = Choice { configured: true, ..c };
        assert_eq!(display_name(&c2), "📷 캠 · 연결됨 · 설정됨");
    }
}
