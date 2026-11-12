//! 예배당 모니터 출력 창: 웹뷰(WebView2)로 /outputs/main 을 전체 화면에 띄운다.
//!   wp-output --url http://127.0.0.1:37528/outputs/main?window=1 --monitor 1
//!   wp-output --list        모니터 목록 (한 줄에 하나: 번호<TAB>이름<TAB>x,y,w,h<TAB>primary)
//!   wp-output --windowed    시험용 (작은 창)
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop};
use tao::window::{Fullscreen, WindowBuilder};
use wry::WebViewBuilder;

fn arg(name: &str) -> Option<String> {
    let a: Vec<String> = std::env::args().collect();
    a.iter().position(|x| x == name).and_then(|i| a.get(i + 1).cloned())
}

fn main() -> anyhow::Result<()> {
    let event_loop = EventLoop::new();
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
    let windowed = flag("--windowed");
    let window = WindowBuilder::new()
        .with_title("워십프리젠터 출력")
        .with_decorations(windowed)
        .with_inner_size(tao::dpi::LogicalSize::new(960.0, 540.0))
        .build(&event_loop)?;
    if !windowed {
        let want: usize = arg("--monitor").and_then(|x| x.parse().ok()).unwrap_or(1);
        let mons: Vec<_> = window.available_monitors().collect();
        let mon = mons.get(want).or(mons.last()).cloned();
        window.set_fullscreen(Some(Fullscreen::Borderless(mon)));
        window.set_always_on_top(true);
    }
    let _webview = WebViewBuilder::new()
        .with_url(&url)
        .with_autoplay(true)
        .with_background_color((0, 0, 0, 255))
        .build(&window)?;
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        if let Event::WindowEvent { event: WindowEvent::CloseRequested, .. } = event {
            *control_flow = ControlFlow::Exit;
        }
    });
}
