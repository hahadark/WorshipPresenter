//! 워십프리젠터 Rust 판 — 조작 화면(egui) + 출력용 로컬 웹 서버
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod sources_ui; // 인스펙터 '소스' 탭 (로드맵 1단계)
mod scene_ui; // 화면 구성 편집 창 (로드맵 2단계)
mod imgcache; // 그림 메모리: 줄여 읽기 · 상한(LRU)
mod panel_ui;
mod names_ui;
mod firewall_ui; // 방화벽 확인 · 열기 줄 // 이름 자막 창 (명단 · 띄우기 · 모양)
mod quick_ui; // 디자인 2단계: 빠른 설정 안쪽 (tplcontrols basic)
mod center_ui; // 디자인 2단계: 가운데 본문 패널 (머리·목록·격자·주석)
mod workspace_ui;
mod hymns_ui; // [트랙 A]
mod dnd; // 끌어다 놓기·파일 붙여 넣기
mod outwin;
mod camscan; // 카메라 찾기·미리보기 (wp-output 도우미)
mod motion; // 움직이는 배경 (템플릿 background.motion) 조작 화면 그림
mod paint;
mod paint_edit; // [트랙 A]
mod css_bg; // 웹과 같은 CSS 그라데이션 계산
mod slide_layout; // 웹 출력과 같은 슬라이드 자리 계산
mod parity_shot; // 점검용 --parity: 조작 화면 그림 ↔ 웹 출력 비교
mod a_tools; // [트랙 A] 설교 포인트 · 예배 전 화면 · 홈페이지 주보
mod parity_ui; // [트랙 A] 기능 대조에서 빠진 작은 것들
mod parity2_ui; // [트랙 A] 성경 책 목록 · 종류 골라 붙여 넣기 · 썸네일 오른쪽 클릭
mod trackb_ui; // [트랙 B]
mod lyrics_ui; // [트랙 B]
mod music_ui; // [트랙 B]
mod system_ui; // [트랙 B]
mod media_ui;
mod thumbs_ui; // [트랙 B]
mod form; // [트랙 C]
mod history_ui; // [트랙 C]
mod flow_ui; // [트랙 C]
mod keys; // [트랙 C]
mod slideedit_ui; // [트랙 C]
mod theme_ui; // [트랙 C]
mod import_ui; // [트랙 C]
mod commentary_ui; // [트랙 C]
mod menubar; // [트랙 C]
mod settings_ui; // [트랙 C]
mod template_ui; // [트랙 C]
mod tpl_controls; // 템플릿 편집 속성 칸 부품
mod tpl_inspect; // 템플릿 편집 속성 칸
mod track_c; // [트랙 C]
mod tools_ui;
mod trackd_ui; // [트랙 D] // [트랙 D]
mod actions_ui; // 순서 편집칸: 배경 음악 · ⚡ 송출할 때 자동으로
mod theme; // 디자인 기반: control.css 색·글꼴
mod icons; // 디자인 기반: Lucide 아이콘
mod widgets; // 디자인 기반: 공용 위젯
mod probar_ui; // (나) 전문가용 위 컨트롤 막대 · 상태 줄
mod prolayout_ui; // (나) 고정 배치: 큐시트 판 · 뷰어 둘 · 인스펙터 (예전 독 작업공간 대신)
mod timeline_ui; // (나) 아래 예배 시간표 (보기 전용)
mod cue_ui; // 디자인 2단계: 왼쪽 큐시트 패널
mod cuemgr_ui; // [갈래 C] 큐시트 관리 창 (cuemanager.js)
mod pptsplit_ui; // [갈래 C] 한 PPT 를 여러 순서로 나누기 창 (cue.js openSplit)
mod ctx_ui; // [갈래 C] 오른쪽 클릭 메뉴: 본문 줄 · 라이브러리 · 패널 (ctxmenu.js)

use eframe::egui;
use wp_core::lockcheck::Mutex;
use std::path::PathBuf;
use std::sync::Arc;
use wp_core::core::{Core, Ev};

fn arg(name: &str) -> Option<String> {
    let a: Vec<String> = std::env::args().collect();
    a.iter().position(|x| x == name).and_then(|i| a.get(i + 1).cloned())
}

/// 출력 화면 웹 파일: 실행 파일 옆 web\ (설치판) → 개발 중이면 저장소의 web\
fn web_dir() -> PathBuf {
    let exe_dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|p| p.to_path_buf())).unwrap_or_default();
    // 맥 앱 묶음: Contents/MacOS/(실행 파일) 옆이 아니라 Contents/Resources/web
    for c in [exe_dir.join("web"), exe_dir.join("..").join("Resources").join("web"), PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("web")] {
        if c.join("output").join("index.html").exists() {
            return normalize(&c);
        }
    }
    exe_dir.join("web")
}

/// '..' 를 풀어 둔다 (그림 주소 file:// 에 쓰려고. canonicalize 는 \\?\ 를 붙여서 안 씀)
fn normalize(p: &std::path::Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            std::path::Component::ParentDir => { out.pop(); }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn seed_db() -> Option<PathBuf> {
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    [exe_dir.join("bible.db"), exe_dir.join("..").join("Resources").join("bible.db"), PathBuf::from(r"F:\aside\WorshipCue\data\bible.db")].into_iter().find(|p| p.exists())
}

/// 점검용(디버그 빌드만): WP_DEBUG_ALLOC=1 이면 8MB 넘는 할당이 어디서 나왔는지 로그에 남긴다.
/// 출시 빌드는 기호가 없어 쓸모가 없으므로 넣지 않는다.
#[cfg(debug_assertions)]
mod bigalloc {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::sync::atomic::{AtomicBool, Ordering};
    pub static ON: AtomicBool = AtomicBool::new(false);
    thread_local! { static BUSY: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
    pub struct Tracker;
    fn note(size: usize, what: &str) {
        if size < 8 << 20 || !ON.load(Ordering::Relaxed) {
            return;
        }
        let _ = BUSY.try_with(|b| {
            if b.replace(true) {
                return;
            }
            let bt = std::backtrace::Backtrace::force_capture().to_string();
            let ours: Vec<&str> = bt.lines().map(|l| l.trim()).filter(|l| !l.starts_with("at ") && !l.contains("bigalloc") && !l.contains("backtrace")).take(22).collect();
            log::info!("큰 할당 {what} {:.0}MB: {}", size as f64 / 1048576.0, ours.join(" < "));
            b.set(false);
        });
    }
    unsafe impl GlobalAlloc for Tracker {
        unsafe fn alloc(&self, l: Layout) -> *mut u8 {
            note(l.size(), "alloc");
            unsafe { System.alloc(l) }
        }
        unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
            note(l.size(), "zeroed");
            unsafe { System.alloc_zeroed(l) }
        }
        unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
            unsafe { System.dealloc(p, l) }
        }
        unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
            note(n, "realloc");
            unsafe { System.realloc(p, l, n) }
        }
    }
}
#[cfg(debug_assertions)]
#[global_allocator]
static ALLOC: bigalloc::Tracker = bigalloc::Tracker;

/// 그래픽 백엔드 고르기: 환경 변수 → DX12 그래픽 카드(소프트웨어 그리기 아님)가 있으면 DX12 만 → 아니면 전부
#[cfg(windows)]
fn pick_backends() -> eframe::wgpu::Backends {
    use eframe::wgpu;
    let all = wgpu::Backends::PRIMARY | wgpu::Backends::GL;
    if let Some(b) = wgpu::Backends::from_env() {
        return b;
    }
    let t0 = std::time::Instant::now();
    let ok = std::panic::catch_unwind(|| {
        let inst = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::DX12,
            flags: wgpu::InstanceFlags::from_build_config().with_env(),
            backend_options: wgpu::BackendOptions::from_env_or_default(),
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            display: None,
        });
        let rt = tokio::runtime::Builder::new_current_thread().build().ok()?;
        let list = rt.block_on(inst.enumerate_adapters(wgpu::Backends::DX12));
        Some(list.iter().any(|a| a.get_info().device_type != wgpu::DeviceType::Cpu))
    })
    .ok()
    .flatten()
    .unwrap_or(false);
    log::info!("그래픽: {} ({:.0}ms)", if ok { "DX12" } else { "DX12 그래픽 카드 없음 → 자동 고르기" }, t0.elapsed().as_secs_f64() * 1000.0);
    if ok { wgpu::Backends::DX12 } else { all }
}
#[cfg(not(windows))]
fn pick_backends() -> eframe::wgpu::Backends {
    eframe::wgpu::Backends::from_env().unwrap_or(eframe::wgpu::Backends::PRIMARY | eframe::wgpu::Backends::GL)
}

fn main() -> anyhow::Result<()> {
    // 설치 파일이 설치 직후 부른다: 아무것도 하지 않고 끝난다. 새 실행 파일을 처음 읽을 때 걸리는 백신 검사(이 파일 크기로 2초쯤)를
    // 사용자가 아이콘을 누르기 전에 미리 치러 두려는 것
    if std::env::args().any(|a| a == "--warmup") {
        return Ok(());
    }
    wp_core::firstrun::mark("main");
    let data_dir = arg("--data").map(PathBuf::from).unwrap_or_else(|| dirs_data().join("WorshipPresenter-rs").join("data"));
    let _ = std::fs::create_dir_all(&data_dir);
    wp_core::logger::init(&data_dir); // [트랙 B] 파일 로그 (logs\날짜.log)
    wp_core::logger::install_panic_hook(); // 오류(panic)도 로그에
    wp_core::util::mem_log("시작");
    #[cfg(debug_assertions)]
    bigalloc::ON.store(std::env::var_os("WP_DEBUG_ALLOC").is_some(), std::sync::atomic::Ordering::Relaxed);
    let web = web_dir();
    // [트랙 A] 두 번 실행 막기 (같은 데이터 폴더) · 송출 중 화면 꺼짐 막기
    // '1.x 자료 다시 가져오기' 로 다시 켜질 때: 앞 프로세스가 끝나길 기다린다 (두 번 실행 막기에 걸리지 않게)
    if let Some(pid) = arg("--wait-pid").and_then(|p| p.parse::<u32>().ok()) {
        wp_core::firstrun::wait_for_pid(pid);
    }
    let (mut already, mut _instance) = wp_core::parity_a::single_instance(&data_dir);
    let waiting = std::time::Instant::now();
    let (mut rounds, mut focus_ms, mut focused) = (0u32, 0u128, 0u32); // 기다린 까닭을 로그로 남기려고
    let mut splash_shown = false;
    while already {
        rounds += 1;
        let t_focus = std::time::Instant::now();
        let found = wp_core::firstrun::focus_instance(&data_dir);
        focus_ms += t_focus.elapsed().as_millis();
        focused += found as u32;
        // 먼저 켜진 프로세스 번호로 창을 찾는다 (제목만으로 찾으면 같은 제목의 Electron 1.x 창을 올릴 수 있음)
        if found {
            // 그 창이 닫히는 중일 수도 있다(끄자마자 다시 켠 경우): 잠깐 지켜보고 사라지면 아래에서 기다렸다 켠다
            let mut gone = false;
            for _ in 0..8 {
                std::thread::sleep(std::time::Duration::from_millis(100));
                if !wp_core::firstrun::instance_has_window(&data_dir) {
                    gone = true;
                    break;
                }
            }
            if !gone {
                return Ok(());
            }
        }
        // 창이 없다 = 먼저 켜진 것이 막 꺼지는 중이거나(창은 닫혔지만 프로세스 정리가 안 끝남 · 강제 종료 직후) 아직 켜지는 중.
        // 전에는 여기서 조용히 끝나서 '껐다가 바로 다시 켜면 아무 일도 안 일어나는' 일이 있었다 → 풀릴 때까지 기다렸다 켠다
        if waiting.elapsed() > std::time::Duration::from_secs(15) {
            log::warn!("두 번 실행: 먼저 켜진 것(instance.pid {:?})이 15초 동안 끝나지도 창을 보이지도 않았습니다", std::fs::read_to_string(data_dir.join("instance.pid")).ok());
            wp_core::firstrun::splash_done();
            wp_core::firstrun::error_box(if cfg!(target_os = "macos") {
                "워십프리젠터가 이미 켜져 있습니다.\n\n창이 보이지 않으면 강제 종료(⌘+Option+Esc)에서 WorshipPresenter 를 끝낸 뒤 다시 켜 주세요."
            } else {
                "워십프리젠터가 이미 켜져 있는데 창을 찾지 못했습니다.\n\n작업 관리자(Ctrl+Shift+Esc)에서 WorshipPresenter 를 끝낸 뒤 다시 켜 주세요."
            });
            return Ok(());
        }
        // 강제 종료된 앞 프로세스가 다 정리되기까지 몇 초 걸릴 때가 있다(실측 2.6~8.2초). 그동안 아무것도 안 보이면 안 켜진 줄 안다 → 시작 창을 먼저
        if !splash_shown && waiting.elapsed() > std::time::Duration::from_millis(400) {
            wp_core::firstrun::start_splash();
            splash_shown = true;
        }
        wp_core::firstrun::close_instance_handle(_instance.take());
        std::thread::sleep(std::time::Duration::from_millis(120));
        (already, _instance) = wp_core::parity_a::single_instance(&data_dir);
    }
    if waiting.elapsed().as_millis() > 300 {
        log::info!("두 번 실행: 먼저 켜진 것이 끝나길 {}ms 기다렸다 켭니다 (다시 본 횟수 {rounds}, 창을 찾은 횟수 {focused}, 창 앞으로 가져오기에 쓴 시간 {focus_ms}ms)", waiting.elapsed().as_millis());
    }
    wp_core::firstrun::mark("로그·단일 실행");
    wp_core::firstrun::write_instance_pid(&data_dir);
    // 창이 늦게 뜨면(첫 실행 자료 옮기기·느린 디스크) 작은 '켜는 중…' 창을 먼저 보인다
    if !splash_shown {
        wp_core::firstrun::start_splash();
    }
    wp_core::parity_a::keep_display_awake();
    // 자료 열기·서버 시작은 창·그래픽 준비(0.5초쯤)와 동시에 한다 → 창이 그만큼 빨리 뜬다
    let (ptx, prx) = std::sync::mpsc::channel();
    {
        let (d, w) = (data_dir.clone(), web.clone());
        std::thread::Builder::new().name("prepare".into()).spawn(move || {
            let _ = ptx.send(prepare(d, w));
        })?;
    }
    // 그래픽 장치 준비(0.3~0.5초)도 창 만들기 전에 다른 스레드에서 시작한다
    let gfx = std::thread::Builder::new().name("gfx".into()).spawn(prepare_gfx).ok();
    let icon = image::load_from_memory(include_bytes!("../../../assets/icon.png")).ok().map(|i| {
        let i = i.to_rgba8();
        let (w, h) = i.dimensions();
        egui::IconData { rgba: i.into_raw(), width: w, height: h }
    });
    let mut vp = egui::ViewportBuilder::default().with_title("워십프리젠터").with_inner_size([1500.0, 920.0]).with_min_inner_size([1000.0, 640.0]);
    // 점검용 --win 1600x960: 창 안쪽 크기를 고정 (기준 화면과 같은 크기로 --shot 비교)
    if let Some((w, h)) = arg("--win").and_then(|s| { let (a, b) = s.split_once(['x', 'X'])?; Some((a.trim().parse::<f32>().ok()?, b.trim().parse::<f32>().ok()?)) }) {
        vp = vp.with_inner_size([w, h]).with_min_inner_size([w, h]).with_max_inner_size([w, h]).with_resizable(false).with_position([0.0, 0.0]);
    }
    if let Some(i) = icon {
        vp = vp.with_icon(Arc::new(i));
    }
    wp_core::firstrun::mark("아이콘");
    let mut opts = eframe::NativeOptions { viewport: vp, ..Default::default() };
    // 그래픽 장치를 미리(다른 스레드에서) 만들어 둔 것이 있으면 그것을 쓴다
    if let Some(g) = gfx.and_then(|h| h.join().ok()).flatten() {
        opts.wgpu_options.wgpu_setup = eframe::egui_wgpu::WgpuSetup::Existing(g);
    }
    wp_core::firstrun::mark("그래픽 장치");
    // 미리 준비가 불가능한 환경에서도 같은 메모리 정책을 유지한다.
    if let eframe::egui_wgpu::WgpuSetup::CreateNew(n) = &mut opts.wgpu_options.wgpu_setup {
        use eframe::wgpu;
        n.instance_descriptor.backends = pick_backends();
        n.device_descriptor = Arc::new(|adapter| {
            let base = if adapter.get_info().backend == wgpu::Backend::Gl { wgpu::Limits::downlevel_webgl2_defaults() } else { wgpu::Limits::default() };
            wgpu::DeviceDescriptor {
                label: Some("wp-app"),
                required_limits: wgpu::Limits { max_texture_dimension_2d: 8192, ..base },
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                ..Default::default()
            }
        });
    }
    let web2 = web.clone();
    eframe::run_native(
        "worship-presenter-rs",
        opts,
        Box::new(move |cc| {
            wp_core::firstrun::mark("창·그래픽 준비");

            theme::install_fonts(&cc.egui_ctx); // Pretendard + 맑은 고딕(대체·슬라이드)
            wp_core::firstrun::mark("글꼴");
            egui_extras::install_image_loaders(&cc.egui_ctx);
            imgcache::install(&cc.egui_ctx);
            wp_core::util::mem_log("글꼴 등록");
            theme::apply(&cc.egui_ctx, theme::ThemeId::Pro); // 설정 테마는 첫 프레임에 apply_ui_prefs 가 다시 적용
            // 자료 준비가 끝나길 기다린다 (보통 이미 끝나 있다. 첫 실행 자료 옮기기는 시작 창이 진행률을 보인다)
            let (shared, rx) = match prx.recv() {
                Ok(Ok(v)) => v,
                Ok(Err(e)) => {
                    wp_core::firstrun::error_box(&format!("워십프리젠터를 켜지 못했습니다.\n\n{e:#}\n\n자료 폴더: {}", data_dir.display()));
                    return Err(format!("{e:#}").into());
                }
                Err(e) => return Err(format!("자료 준비가 중간에 끝났습니다: {e}").into()),
            };
            wp_core::firstrun::mark("자료 기다림 끝");
            // 코어가 바뀌면 화면을 다시 그린다
            let ctx = cc.egui_ctx.clone();
            let mut rx = rx;
            std::thread::spawn(move || loop {
                match rx.blocking_recv() {
                    Ok(Ev::Ui) | Ok(Ev::Live) | Ok(Ev::Outputs) | Ok(Ev::Draw) => ctx.request_repaint(), // [트랙 B] Draw
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => ctx.request_repaint(),
                    Err(_) => break,
                }
            });
            let app = app::App::new(shared, web2, data_dir);
            wp_core::firstrun::mark("화면 준비");
            wp_core::util::mem_log("창·그래픽 장치 생김");
            Ok(Box::new(app))
        }),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))
}

/// 자료 열기 · 백그라운드 일 · 출력 웹 서버 (창 만드는 동안 다른 스레드에서)
fn prepare(data_dir: PathBuf, web: PathBuf) -> anyhow::Result<(wp_core::core::Shared, tokio::sync::broadcast::Receiver<Ev>)> {
    let core = Core::open(data_dir, web, seed_db().as_deref())?;
    wp_core::firstrun::mark("자료 열기");
    let shared = Arc::new(Mutex::new(core));
    wp_core::flow::install(&shared); // [트랙 C] 자동 넘김·글자 자동 숨김 타이머
    wp_core::i18n::set_lang(shared.lock().settings.get()["uiLang"].as_str().unwrap_or("ko")); // [트랙 D] 영어 화면 (출력 웹도)
    wp_core::winkeys::start_global_shortcuts(shared.clone()); // [트랙 D] 전역 단축키
    wp_core::updates::start_telemetry(shared.clone()); // [트랙 D] 익명 사용 통계 (디버그 빌드는 안 보냄)
    if shared.lock().settings.get()["autoUpdate"].as_bool().unwrap_or(true) {
        wp_core::updates::check(&shared, false); // [트랙 D] 새 버전 확인
    }
    wp_core::hymns::install(&shared); // [트랙 A] 찬송가 그림을 백그라운드로 만들고 끝나면 알림
    wp_core::sync::install(&shared); // [트랙 B] 여러 PC 동기화 · 하루 1회 백업
    // 받아 둔 옛 설치 파일 지우기 (설치판만). 점검 실행(--data)은 사용자 폴더를 건드리지 않는다
    if arg("--data").is_none() {
        wp_core::firstrun::start_installer_cleanup(&shared.lock().data_dir);
    }
    // 1.x 폴더에만 있는 붙여 넣은 PPT·찬송가 자료를 이 판 폴더로 챙겨 둔다 (1.x 폴더를 지워도 큐시트가 안 깨지게).
    // 점검 실행(--data)은 임시 폴더마다 280MB 가 쌓이지 않게 건너뛴다 (시험할 때는 WP_KEEP_LEGACY=1)
    if arg("--data").is_none() || std::env::var_os("WP_KEEP_LEGACY").is_some() {
        wp_core::firstrun::start_keep_legacy_files(&shared.lock().data_dir);
    }

    // 웹 서버 (출력 화면) — 따로 tokio 실행기
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build()?;
    let port = rt.block_on(wp_core::server::serve(shared.clone()))?;
    log::info!("로컬 서버: http://127.0.0.1:{port}");
    let rx = shared.lock().tx.subscribe();
    std::thread::spawn(move || rt.block_on(std::future::pending::<()>()));
    wp_core::firstrun::mark("서버");
    Ok((shared, rx))
}

/// 그래픽 장치를 미리 만든다: 윈도우에서는 DX12 만 살펴 본다.
/// eframe 기본값은 Vulkan·DX12·OpenGL 을 모두 살펴 0.7~0.8초 걸렸다(측정). DX12 하나면 0.5초쯤이고 다른 준비와 동시에 할 수 있다.
/// 쓸 만한 DX12 장치(하드웨어)가 없거나 WGPU_BACKEND 를 정해 두었으면 None → eframe 기본 방식 그대로.
fn prepare_gfx() -> Option<eframe::egui_wgpu::WgpuSetupExisting> {
    use eframe::wgpu;
    if !cfg!(windows) || std::env::var_os("WGPU_BACKEND").is_some() {
        return None;
    }
    let rt = tokio::runtime::Builder::new_current_thread().build().ok()?;
    let r = rt.block_on(async {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::DX12,
            flags: wgpu::InstanceFlags::from_build_config().with_env(),
            backend_options: wgpu::BackendOptions::from_env_or_default(),
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            display: None,
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, force_fallback_adapter: false, compatible_surface: None, ..Default::default() })
            .await
            .map_err(|e| e.to_string())?;
        let info = adapter.get_info();
        if info.device_type == wgpu::DeviceType::Cpu {
            return Err(format!("소프트웨어 장치뿐 ({})", info.name));
        }
        // eframe 기본값과 같은 장치 설정 (큰 화면용 텍스처 8192)
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("egui wgpu device"),
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                required_limits: wgpu::Limits { max_texture_dimension_2d: 8192, ..wgpu::Limits::default() },
                ..Default::default()
            })
            .await
            .map_err(|e| e.to_string())?;
        log::info!("그래픽: {} ({:?})", info.name, info.backend);
        Ok::<_, String>(eframe::egui_wgpu::WgpuSetupExisting { instance, adapter, device, queue })
    });
    match r {
        Ok(v) => Some(v),
        Err(e) => {
            log::info!("그래픽 미리 준비 건너뜀 (기본 방식으로): {e}");
            None
        }
    }
}

fn dirs_data() -> PathBuf {
    wp_core::core::user_data_root()
}
