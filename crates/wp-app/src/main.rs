//! 워십프리젠터 Rust 판 — 조작 화면(egui) + 출력용 로컬 웹 서버
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod panel_ui;
mod quick_ui; // 디자인 2단계: 빠른 설정 안쪽 (tplcontrols basic)
mod center_ui; // 디자인 2단계: 가운데 본문 패널 (머리·목록·격자·주석)
mod workspace_ui;
mod hymns_ui; // [트랙 A]
mod outwin;
mod camscan; // 카메라 찾기·미리보기 (wp-output 도우미)
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
mod cue_ui; // 디자인 2단계: 왼쪽 큐시트 패널

use eframe::egui;
use parking_lot::Mutex;
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
    for c in [exe_dir.join("web"), PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("web")] {
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
    [exe_dir.join("bible.db"), PathBuf::from(r"F:\aside\WorshipCue\data\bible.db")].into_iter().find(|p| p.exists())
}

fn main() -> anyhow::Result<()> {
    let data_dir = arg("--data").map(PathBuf::from).unwrap_or_else(|| dirs_data().join("WorshipPresenter-rs").join("data"));
    let _ = std::fs::create_dir_all(&data_dir);
    wp_core::logger::init(&data_dir); // [트랙 B] 파일 로그 (logs\날짜.log)
    let web = web_dir();
    // [트랙 A] 두 번 실행 막기 (같은 데이터 폴더) · 송출 중 화면 꺼짐 막기
    let (already, _instance) = wp_core::parity_a::single_instance(&data_dir);
    if already {
        wp_core::parity_a::focus_existing_window("워십프리젠터");
        return Ok(());
    }
    wp_core::parity_a::keep_display_awake();
    let core = Core::open(data_dir.clone(), web.clone(), seed_db().as_deref())?;
    let shared = Arc::new(Mutex::new(core));
    wp_core::flow::install(&shared); // [트랙 C] 자동 넘김·글자 자동 숨김 타이머
    wp_core::i18n::set_lang(shared.lock().settings.get()["uiLang"].as_str().unwrap_or("ko")); // [트랙 D] 영어 화면 (출력 웹도)
    wp_core::winkeys::start_global_shortcuts(shared.clone()); // [트랙 D] 전역 단축키
    wp_core::updates::start_telemetry(shared.clone()); // [트랙 D] 익명 사용 통계 (디버그 빌드는 안 보냄)
    if shared.lock().settings.get()["autoUpdate"].as_bool().unwrap_or(true) { wp_core::updates::check(&shared, false); } // [트랙 D] 새 버전 확인
    wp_core::hymns::install(&shared); // [트랙 A] 찬송가 그림을 백그라운드로 만들고 끝나면 알림
    wp_core::sync::install(&shared); // [트랙 B] 여러 PC 동기화 · 하루 1회 백업

    // 웹 서버 (출력 화면) — 따로 tokio 실행기
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build()?;
    let port = rt.block_on(wp_core::server::serve(shared.clone()))?;
    log::info!("로컬 서버: http://127.0.0.1:{port}");
    let rx = shared.lock().tx.subscribe();
    std::thread::spawn(move || rt.block_on(std::future::pending::<()>()));

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
    let opts = eframe::NativeOptions { viewport: vp, ..Default::default() };
    let web2 = web.clone();
    eframe::run_native(
        "worship-presenter-rs",
        opts,
        Box::new(move |cc| {
            theme::install_fonts(&cc.egui_ctx); // Pretendard + 맑은 고딕(대체·슬라이드)
            egui_extras::install_image_loaders(&cc.egui_ctx);
            theme::apply(&cc.egui_ctx, theme::ThemeId::Dark); // 설정 테마는 첫 프레임에 apply_ui_prefs 가 다시 적용
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
            Ok(Box::new(app::App::new(shared, web2, data_dir)))
        }),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))
}

fn dirs_data() -> PathBuf {
    std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}
