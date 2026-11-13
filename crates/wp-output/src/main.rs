//! 예배당 모니터 출력 창: 웹뷰(WebView2)로 /outputs/main 을 전체 화면에 띄운다.
//!   wp-output --url http://127.0.0.1:37528/outputs/main?window=1 --monitor 1
//!   wp-output --list        모니터 목록 (한 줄에 하나: 번호<TAB>이름<TAB>x,y,w,h<TAB>primary)
//!   wp-output --windowed    시험용 (작은 창) · 카메라 미리보기 (/helper/camera)
//!   wp-output --helper      숨은 창: 카메라 찾기 도우미 (/helper/cameras). 페이지가 ipc 'close' 를 보내면 끝난다
//!   wp-output --title 글    창 제목
//! 카메라 권한: WebView2 는 기본으로 권한 묻는 창을 띄우는데 전체 화면·숨은 창에서는 답할 수 없으므로
//! 카메라·마이크 요청은 바로 허용한다 (조작 PC 의 127.0.0.1 페이지만 연다).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop, EventLoopBuilder, EventLoopProxy};
use tao::window::{Fullscreen, WindowBuilder};
use wry::{PermissionKind, PermissionResponse, WebViewBuilder};

/// 페이지가 보내는 ipc 메시지로 창을 닫는다 (도우미 모드)
#[derive(Debug, Clone, Copy)]
enum UserEv {
    Close,
}

/// 도우미는 시간이 지나면 스스로 끝난다 (페이지가 멈춰도 숨은 창이 남지 않게)
const HELPER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// 카메라·마이크만 허용, 나머지는 WebView2 기본 처리
fn permission(kind: PermissionKind) -> PermissionResponse {
    match kind {
        PermissionKind::Camera | PermissionKind::Microphone => PermissionResponse::Allow,
        _ => PermissionResponse::Default,
    }
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
        .with_visible(!helper)
        .with_inner_size(tao::dpi::LogicalSize::new(960.0, 540.0))
        .build(&event_loop)?;
    if !windowed {
        let want: usize = arg("--monitor").and_then(|x| x.parse().ok()).unwrap_or(1);
        let mons: Vec<_> = window.available_monitors().collect();
        let mon = mons.get(want).or(mons.last()).cloned();
        window.set_fullscreen(Some(Fullscreen::Borderless(mon)));
        window.set_always_on_top(true);
    }
    let proxy: EventLoopProxy<UserEv> = event_loop.create_proxy();
    let _webview = WebViewBuilder::new()
        .with_url(&url)
        .with_autoplay(true)
        .with_background_color((0, 0, 0, 255))
        .with_permission_handler(permission)
        .with_ipc_handler(move |req| {
            if req.body() == "close" {
                let _ = proxy.send_event(UserEv::Close);
            }
        })
        .build(&window)?;
    let started = std::time::Instant::now();
    event_loop.run(move |event, _, control_flow| {
        *control_flow = if helper { ControlFlow::WaitUntil(started + HELPER_TIMEOUT) } else { ControlFlow::Wait };
        match event {
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } | Event::UserEvent(UserEv::Close) => *control_flow = ControlFlow::Exit,
            _ if helper && started.elapsed() >= HELPER_TIMEOUT => *control_flow = ControlFlow::Exit,
            _ => {}
        }
    });
}
