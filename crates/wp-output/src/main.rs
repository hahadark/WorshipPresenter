//! 예배당 모니터 출력 창: 웹뷰(WebView2)로 /outputs/main 을 전체 화면에 띄운다.
//!   wp-output --url http://127.0.0.1:37528/outputs/main?window=1 --monitor 1
//!   wp-output --list        모니터 목록 (한 줄에 하나: 번호<TAB>이름<TAB>x,y,w,h<TAB>primary)
//!   wp-output --windowed    시험용 (작은 창) · 카메라 미리보기 (/helper/camera)
//!   wp-output --helper      숨은 창: 카메라 찾기 도우미 (/helper/cameras). 페이지가 ipc 'close' 를 보내면 끝난다
//!   wp-output --title 글    창 제목
//! 창은 숨긴 채 띄우고 첫 화면이 준비되면(최대 5초) 한 번에 보인다 (흰 화면 깜빡임 없음)
//!   wp-output --esc-close   예배당 창: 마우스로 눌러 고른 뒤 Esc 를 누르면 창을 닫고 종료 코드 3 (조작 화면이 '예배당 화면 꺼짐' 으로)
//!                           Electron main.js onOutputKey 의 `Escape → setOutput({ enabled: false })` 와 같은 동작
//!   wp-output --selftest-esc 점검용: 페이지를 연 뒤 페이지 안에서 Esc 키 입력(keydown)을 흉내 낸다 (--esc-close 경로 확인)
//! 카메라 권한: WebView2 는 기본으로 권한 묻는 창을 띄우는데 전체 화면·숨은 창에서는 답할 수 없으므로
//! 카메라·마이크 요청은 바로 허용한다 (조작 PC 의 127.0.0.1 페이지만 연다).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop, EventLoopBuilder, EventLoopProxy};
use tao::window::{Fullscreen, WindowBuilder};
use wry::{PermissionKind, PermissionResponse, WebContext, WebViewBuilder};

/// 페이지가 보내는 ipc 메시지로 창을 닫는다 (도우미 모드)
#[derive(Debug, Clone, Copy)]
enum UserEv {
    Close,
    /// 예배당 창에서 Esc (--esc-close)
    Escape,
    /// 점검용 Esc 흉내 시각
    SelfTest,
    /// 첫 화면을 다 그렸으니 창을 보인다 (페이지의 'ready' 또는 시간 초과)
    Show,
    /// 모니터 살펴보기 (2초마다): 예배당 모니터를 뽑았으면 숨기고, 다시 꽂으면 그 모니터에 다시 띄운다
    Monitors,
}

/// 예배당 모니터가 빠지고 돌아올 때 할 일
#[derive(Debug, PartialEq)]
enum MonitorAct {
    Stay,
    /// 고른 모니터가 없어졌다 → 숨긴다 (윈도우가 '항상 위' 전체 화면 창을 조작 화면 모니터로 옮겨 조작 화면을 가리지 않게)
    Hide,
    /// 고른 모니터가 돌아왔다 → 그 모니터에 전체 화면으로 다시
    Restore,
}

/// want = 고른 모니터 번호, count = 지금 모니터 수, gone = 지금 숨겨 둔 상태인가
fn monitor_act(want: usize, count: usize, gone: bool) -> MonitorAct {
    match (want < count, gone) {
        (false, false) => MonitorAct::Hide,
        (true, true) => MonitorAct::Restore,
        _ => MonitorAct::Stay,
    }
}

/// 창을 숨긴 채 페이지를 불러오고, 첫 화면(글꼴·무대·그림)이 준비되면 'ready' 를 보낸다.
/// 흰 빈 창 → 검은 바탕 → 슬라이드 순서로 깜빡이며 켜지지 않고 완성된 화면이 한 번에 보이게 한다.
/// 무대(#screen)가 없는 페이지(카메라 미리보기 등)는 불러오기가 끝나면 바로 보낸다.
const READY_SCRIPT: &str = r#"(function () {
  // 창이 보이기 전 첫 화면은 서서히 나타나기(디졸브) 없이 바로 완성된 모습으로 그린다
  var boot = document.createElement('style');
  boot.textContent = '*,*::before,*::after{transition:none!important;animation-duration:0s!important;animation-delay:0s!important}';
  (function put() { var r = document.head || document.documentElement; if (r) r.appendChild(boot); else setTimeout(put, 0); })();
  var sent = false;
  function send() {
    if (sent) return; sent = true;
    try { window.ipc.postMessage('ready'); } catch (_) {}
    setTimeout(function () { boot.remove(); }, 200); // 보인 뒤부터는 템플릿의 전환 효과대로
  }
  function settle() { requestAnimationFrame(function () { requestAnimationFrame(send); }); }
  function check(start) {
    var s = document.getElementById('screen');
    var stage = !s || !!s.firstChild;
    var imgs = Array.prototype.every.call(document.images, function (i) { return i.complete; });
    var areas = s ? s.querySelectorAll('.bc-area') : [];
    var shown = Array.prototype.every.call(areas, function (el) { return getComputedStyle(el).opacity === '1'; });
    if ((stage && imgs && shown) || Date.now() - start > 4000) settle(); else setTimeout(function () { check(start); }, 30);
  }
  window.addEventListener('load', function () {
    var go = function () { setTimeout(function () { check(Date.now()); }, 60); };
    if (document.fonts && document.fonts.ready) document.fonts.ready.then(go, go); else go();
  });
})();"#;

/// 페이지가 'ready' 를 못 보내도(오류·느린 장비) 이 시간이 지나면 창을 보인다
const SHOW_FALLBACK: std::time::Duration = std::time::Duration::from_secs(5);

/// Esc 로 닫았을 때 종료 코드 (조작 화면 outwin.rs 가 이 값을 보고 '예배당 화면을 Esc 로 껐습니다' 를 알린다)
const EXIT_ESC: i32 = 3;

/// 예배당 창 키 처리 (Electron 은 before-input-event, 여기서는 페이지에 심은 스크립트 → ipc).
/// 잡는 단계(capture)에서 받아 출력 페이지의 다른 처리보다 먼저.
const ESC_SCRIPT: &str = r#"window.addEventListener('keydown', function (e) {
  if (e.key === 'Escape' || e.code === 'Escape') { e.preventDefault(); e.stopPropagation(); try { window.ipc.postMessage('escape'); } catch (_) {} }
}, true);"#;

/// 점검용: 페이지 안에서 키 입력처럼 keydown 이벤트를 보낸다
const SELFTEST_SCRIPT: &str = "window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', code: 'Escape', bubbles: true }));";

/// 도우미는 시간이 지나면 스스로 끝난다 (페이지가 멈춰도 숨은 창이 남지 않게)
const HELPER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// 카메라·마이크만 허용, 나머지는 WebView2 기본 처리
fn permission(kind: PermissionKind) -> PermissionResponse {
    match kind {
        PermissionKind::Camera | PermissionKind::Microphone => PermissionResponse::Allow,
        _ => PermissionResponse::Default,
    }
}

/// WebView2 자료 폴더 (쿠키·캐시). 정하지 않으면 WebView2 가 실행 파일 옆(wp-output.exe.WebView2)에 만들려 하는데,
/// '모든 사용자' 로 설치된 PC(Program Files)처럼 그 자리에 쓸 수 없으면 창을 만들다 '액세스가 거부되었습니다'(0x80070005)로
/// 실패해 예배당 화면이 켜지자마자 꺼진다 (2026-11-18 교회 PC). 쓸 수 있는 사용자 폴더를 고른다: --webview-data → %LOCALAPPDATA% → 임시 폴더
fn webview_data_dir() -> Option<std::path::PathBuf> {
    if !cfg!(windows) {
        return None;
    }
    let mut cands: Vec<std::path::PathBuf> = vec![];
    if let Some(d) = arg("--webview-data") {
        cands.push(d.into());
    }
    if let Some(l) = std::env::var_os("LOCALAPPDATA") {
        cands.push(std::path::PathBuf::from(l).join("WorshipPresenter-rs").join("WebView2"));
    }
    cands.push(std::env::temp_dir().join("WorshipPresenter-WebView2"));
    cands.into_iter().find(|d| {
        let probe = d.join(".write-test");
        std::fs::create_dir_all(d).is_ok() && std::fs::write(&probe, b"ok").is_ok() && std::fs::remove_file(&probe).is_ok()
    })
}

fn arg(name: &str) -> Option<String> {
    let a: Vec<String> = std::env::args().collect();
    a.iter().position(|x| x == name).and_then(|i| a.get(i + 1).cloned())
}

fn main() -> anyhow::Result<()> {
    let event_loop: EventLoop<UserEv> = EventLoopBuilder::<UserEv>::with_user_event().build();
    let flag = |n: &str| std::env::args().any(|x| x == n);
    if flag("--list") {
        let w = WindowBuilder::new().with_visible(false).build(&event_loop)?;
        let primary = w.primary_monitor().map(|m| m.position());
        for (i, m) in w.available_monitors().enumerate() {
            let (p, s) = (m.position(), m.size());
            println!("{}\t{}\t{},{},{},{}\t{}", i, m.name().unwrap_or_default(), p.x, p.y, s.width, s.height, Some(p) == primary);
        }
        return Ok(());
    }
    let url = arg("--url").unwrap_or_else(|| "http://127.0.0.1:37528/outputs/main?window=1".into());
    let helper = flag("--helper");
    let windowed = flag("--windowed") || helper;
    let window = WindowBuilder::new()
        .with_title(arg("--title").unwrap_or_else(|| "워십프리젠터 출력".into()))
        .with_decorations(windowed)
        // 처음에는 숨겨 둔다: 첫 화면이 준비되면 Show 로 보인다 (도우미는 끝까지 숨김)
        .with_visible(false)
        .with_background_color((0, 0, 0, 255))
        .with_inner_size(tao::dpi::LogicalSize::new(960.0, 540.0))
        .build(&event_loop)?;
    let want: usize = arg("--monitor").and_then(|x| x.parse().ok()).unwrap_or(1);
    // 고른 모니터가 처음에 있었을 때만 빠짐 · 돌아옴을 살핀다 (모니터 하나뿐이라 마지막 모니터에 띄운 경우는 그대로)
    let mut watch_monitor = false;
    if !windowed {
        let mons: Vec<_> = window.available_monitors().collect();
        watch_monitor = want > 0 && want < mons.len();
        let mon = mons.get(want).or(mons.last()).cloned();
        window.set_fullscreen(Some(Fullscreen::Borderless(mon)));
        window.set_always_on_top(true);
    }
    let proxy: EventLoopProxy<UserEv> = event_loop.create_proxy();
    let proxy_watch = event_loop.create_proxy();
    let esc_close = flag("--esc-close");
    let selftest = flag("--selftest-esc");
    let mut web_context = WebContext::new(webview_data_dir());
    let mut builder = WebViewBuilder::new_with_web_context(&mut web_context);
    if !helper {
        builder = builder.with_initialization_script(READY_SCRIPT);
        let p = proxy.clone();
        std::thread::spawn(move || {
            std::thread::sleep(SHOW_FALLBACK);
            let _ = p.send_event(UserEv::Show);
        });
    }
    if esc_close {
        builder = builder.with_initialization_script(ESC_SCRIPT);
    }
    if selftest {
        let p = proxy.clone();
        builder = builder.with_on_page_load_handler(move |ev, _| {
            if matches!(ev, wry::PageLoadEvent::Finished) {
                let p = p.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(1500));
                    let _ = p.send_event(UserEv::SelfTest);
                });
            }
        });
    }
    let webview = builder
        .with_url(&url)
        .with_autoplay(true)
        .with_background_color((0, 0, 0, 255))
        .with_permission_handler(permission)
        .with_ipc_handler(move |req| {
            match req.body().as_str() {
                "close" => {
                    let _ = proxy.send_event(UserEv::Close);
                }
                "ready" if !helper => {
                    let _ = proxy.send_event(UserEv::Show);
                }
                "escape" if esc_close => {
                    let _ = proxy.send_event(UserEv::Escape);
                }
                _ => {}
            }
        })
        .build(&window)
        .map_err(|e| {
            // 조작 화면(outwin.rs)이 이 줄을 읽어 이유를 알린다
            eprintln!("wp-output: WebView2 창을 만들지 못했습니다: {e}");
            e
        })?;
    let started = std::time::Instant::now();
    let mut shown = false;
    let mut monitor_gone = false;
    if watch_monitor {
        let p = proxy_watch;
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_secs(2));
            if p.send_event(UserEv::Monitors).is_err() {
                break;
            }
        });
    }
    event_loop.run(move |event, _, control_flow| {
        *control_flow = if helper { ControlFlow::WaitUntil(started + HELPER_TIMEOUT) } else { ControlFlow::Wait };
        match event {
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } | Event::UserEvent(UserEv::Close) => *control_flow = ControlFlow::Exit,
            Event::UserEvent(UserEv::Escape) => {
                println!("esc-close");
                *control_flow = ControlFlow::ExitWithCode(EXIT_ESC);
            }
            Event::UserEvent(UserEv::Show) if !shown && !helper => {
                shown = true;
                if !monitor_gone {
                    window.set_visible(true);
                }
                println!("shown {}ms", started.elapsed().as_millis());
            }
            Event::UserEvent(UserEv::Monitors) => {
                let mons: Vec<_> = window.available_monitors().collect();
                match monitor_act(want, mons.len(), monitor_gone) {
                    MonitorAct::Hide => {
                        monitor_gone = true;
                        window.set_visible(false);
                        println!("monitor-gone {want}/{}", mons.len());
                    }
                    MonitorAct::Restore => {
                        monitor_gone = false;
                        window.set_fullscreen(Some(Fullscreen::Borderless(mons.get(want).cloned())));
                        window.set_always_on_top(true);
                        if shown {
                            window.set_visible(true);
                        }
                        println!("monitor-back {want}");
                    }
                    MonitorAct::Stay => {}
                }
            }
            Event::UserEvent(UserEv::SelfTest) => {
                println!("selftest-esc: keydown Escape");
                let _ = webview.evaluate_script(SELFTEST_SCRIPT);
            }
            _ if helper && started.elapsed() >= HELPER_TIMEOUT => *control_flow = ControlFlow::Exit,
            _ => {}
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{monitor_act, MonitorAct};
    /// 예배 중 예배당 모니터(2번째)를 뽑았다 다시 꽂기
    #[test]
    fn unplug_hides_and_replug_restores() {
        assert_eq!(monitor_act(1, 2, false), MonitorAct::Stay, "꽂혀 있음");
        assert_eq!(monitor_act(1, 1, false), MonitorAct::Hide, "뽑음 → 조작 화면을 가리지 않게 숨김");
        assert_eq!(monitor_act(1, 1, true), MonitorAct::Stay, "빠진 동안은 그대로 숨김");
        assert_eq!(monitor_act(1, 2, true), MonitorAct::Restore, "다시 꽂음 → 그 모니터에 다시");
        assert_eq!(monitor_act(2, 2, false), MonitorAct::Hide, "세 대 중 셋째를 뽑음");
    }
}
