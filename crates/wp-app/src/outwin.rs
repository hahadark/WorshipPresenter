//! 예배당 모니터 출력 창 (wp-output.exe 를 따로 띄운다)

use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct Monitor {
    pub index: usize,
    pub name: String,
    pub size: (i32, i32),
    pub primary: bool,
    /// 왼쪽 위 자리 (가상 화면 좌표) — '오른쪽 · 왼쪽' 자리 이름에 쓴다
    pub pos: (i32, i32),
}

impl Monitor {
    /// 봉사자가 알아볼 이름: '모니터 2 · 오른쪽' (주 모니터는 '모니터 1 · 조작 화면'). DISPLAY1 같은 장치 이름은 쓰지 않는다
    pub fn label(&self, all: &[Monitor]) -> String {
        use wp_core::i18n::{tr, tr_args};
        let n = self.index + 1;
        if self.primary {
            return tr_args("모니터 {1} · 조작 화면", &[&n]);
        }
        let Some(main) = all.iter().find(|m| m.primary) else { return tr_args("모니터 {1}", &[&n]) };
        let (dx, dy) = (self.pos.0 + self.size.0 / 2 - (main.pos.0 + main.size.0 / 2), self.pos.1 + self.size.1 / 2 - (main.pos.1 + main.size.1 / 2));
        let side = if dx.abs() >= dy.abs() {
            if dx > 0 { tr("오른쪽") } else { tr("왼쪽") }
        } else if dy > 0 {
            tr("아래")
        } else {
            tr("위")
        };
        tr_args("모니터 {1} · {2}", &[&n, &side])
    }
}

pub struct OutWin {
    child: Option<Child>,
    pub monitors: Vec<Monitor>,
    pub error: String,
    /// 창이 스스로 끝났음 (종료 코드). 조작 화면이 한 번 가져가 알림·OBS 연동을 처리한다 (take_exit)
    exited: Option<Option<i32>>,
    /// 점검용으로 창에 더 넘길 인자 (--selftest-esc)
    pub extra_args: Vec<String>,
    /// 창 프로그램이 오류 출력(stderr)에 남긴 글 (스스로 꺼졌을 때 이유를 알리려고)
    stderr: Arc<Mutex<String>>,
    /// 스스로 끝났을 때 오류 출력의 마지막 줄 (take_exit_reason 으로 한 번 가져간다)
    exit_reason: String,
    launch: Option<(u16, usize, bool)>,
    restarts: std::collections::VecDeque<std::time::Instant>,
}

/// 조작 화면이 띄운 창(예배당·무대·카메라 도우미)을 조작 화면 프로세스와 묶는다.
/// 작업 개체(Job)에 KILL_ON_JOB_CLOSE 를 걸어 두면, 조작 화면이 어떻게 끝나든(정상 종료·작업 관리자 종료·오류)
/// 운영체제가 묶인 창과 그 WebView2 하위 프로세스를 함께 끈다. (Electron 판: 조작 화면을 끄면 출력 창도 꺼짐)
/// 조작 화면 자신은 넣지 않는다 — 넣으면 링크로 연 브라우저까지 같이 꺼진다.
#[cfg(windows)]
pub(crate) fn bind_to_app(child: &Child) {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::JobObjects::*;
    static JOB: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    let job = *JOB.get_or_init(|| unsafe {
        let h = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if h.is_null() {
            return 0;
        }
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        SetInformationJobObject(
            h,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const core::ffi::c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        );
        h as usize // 일부러 닫지 않는다: 조작 화면이 끝나며 핸들이 닫힐 때 묶인 창이 꺼진다
    });
    if job != 0 {
        unsafe {
            AssignProcessToJobObject(job as _, child.as_raw_handle() as _);
        }
    }
}
#[cfg(not(windows))]
pub(crate) fn bind_to_app(_child: &Child) {}

/// wp-output 이 Esc 로 닫혔을 때의 종료 코드 (wp-output main.rs EXIT_ESC)
pub const EXIT_ESC: i32 = 3;

pub(crate) fn exe() -> std::path::PathBuf {
    let me = std::env::current_exe().unwrap_or_default();
    let dir = me.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    dir.join(if cfg!(windows) { "wp-output.exe" } else { "wp-output" })
}

#[cfg(windows)]
pub(crate) fn no_window(c: &mut Command) -> &mut Command {
    use std::os::windows::process::CommandExt;
    c.creation_flags(0x0800_0000) // CREATE_NO_WINDOW
}
#[cfg(not(windows))]
pub(crate) fn no_window(c: &mut Command) -> &mut Command {
    c
}

impl OutWin {
    #[cfg(test)]
    pub(crate) fn for_test() -> Self {
        Self { child: None, monitors: vec![], error: String::new(), exited: None, extra_args: vec![], stderr: Default::default(), exit_reason: String::new(), launch: None, restarts: Default::default() }
    }
    pub fn new() -> Self {
        let mut o = Self { child: None, monitors: vec![], error: String::new(), exited: None, extra_args: vec![], stderr: Default::default(), exit_reason: String::new(), launch: None, restarts: Default::default() };
        o.refresh_monitors();
        o
    }

    pub fn refresh_monitors(&mut self) {
        let out = no_window(Command::new(exe()).arg("--list")).output();
        self.monitors = match out {
            Ok(o) => String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter_map(|l| {
                    let c: Vec<&str> = l.split('\t').collect();
                    if c.len() < 4 {
                        return None;
                    }
                    let g: Vec<i32> = c[2].split(',').filter_map(|x| x.parse().ok()).collect();
                    Some(Monitor { index: c[0].parse().ok()?, name: c[1].trim_start_matches(r"\\.\").to_string(), size: (*g.get(2)?, *g.get(3)?), primary: c[3] == "true", pos: (*g.first()?, *g.get(1)?) })
                })
                .collect(),
            Err(e) => {
                log::error!("예배당 화면 프로그램 확인 실패: {e}");
                self.error = wp_core::i18n::tr("예배당 화면 프로그램이 없습니다. 설치 파일로 다시 설치해 주세요. 자료는 그대로 유지됩니다.");
                vec![]
            }
        };
    }

    /// 기본 모니터: 주 모니터가 아닌 첫 모니터 (하나뿐이면 그것)
    pub fn default_monitor(&self) -> usize {
        self.monitors.iter().find(|m| !m.primary).or(self.monitors.first()).map(|m| m.index).unwrap_or(0)
    }

    pub fn running(&mut self) -> bool {
        match &mut self.child {
            Some(c) => match c.try_wait() {
                Ok(None) => true,
                r => {
                    // 스스로 끝남 (예배당 창에서 Esc · 오류): 조작 화면 상태는 '꺼짐' 으로, 알림은 take_exit 로
                    self.exited = Some(r.ok().flatten().and_then(|s| s.code()));
                    if let Some(mut c) = self.child.take() {
                        let _ = c.wait();
                    }
                    // 읽는 실이 마지막 글을 다 받도록 잠깐 기다린다 (프로세스가 끝나면 파이프가 닫혀 곧 끝남)
                    for _ in 0..20 {
                        if Arc::strong_count(&self.stderr) == 1 {
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                    let err = std::mem::take(&mut *self.stderr.lock().unwrap_or_else(|e| e.into_inner()));
                    self.exit_reason = last_error_line(&err);
                    if !err.trim().is_empty() {
                        log::warn!("예배당 화면 창 오류 출력: {}", err.trim());
                    }
                    false
                }
            },
            None => false,
        }
    }

    pub fn open(&mut self, port: u16, monitor: usize, windowed: bool) {
        self.close();
        self.launch = Some((port, monitor, windowed));
        self.spawn(port, monitor, windowed);
    }

    fn spawn(&mut self, port: u16, monitor: usize, windowed: bool) {
        let mut cmd = Command::new(exe());
        cmd.args(self.open_args(port, monitor, windowed));
        cmd.stderr(Stdio::piped());
        match cmd.spawn() {
            Ok(mut c) => {
                bind_to_app(&c);
                self.stderr = Arc::new(Mutex::new(String::new()));
                self.exit_reason.clear();
                if let Some(mut pipe) = c.stderr.take() {
                    let buf = self.stderr.clone();
                    std::thread::spawn(move || {
                        use std::io::Read;
                        let mut chunk = [0u8; 4096];
                        while let Ok(n) = pipe.read(&mut chunk) {
                            if n == 0 {
                                break;
                            }
                            let mut b = buf.lock().unwrap_or_else(|e| e.into_inner());
                            b.push_str(&String::from_utf8_lossy(&chunk[..n]));
                            if b.len() > 16_000 {
                                let cut = b.len() - 8_000;
                                let cut = (cut..b.len()).find(|&i| b.is_char_boundary(i)).unwrap_or(b.len());
                                b.drain(..cut);
                            }
                        }
                    });
                }
                self.child = Some(c);
                self.error.clear();
            }
            Err(e) => {
                log::error!("예배당 화면 실행 실패: {e}");
                self.error = wp_core::i18n::tr("예배당 화면을 열지 못했습니다. 도움말에서 로그를 확인하고, 설치 파일로 다시 설치해 주세요.");
            }
        }
    }

    /// 의도해서 닫은 창은 그대로 두고, 비정상 종료는 1분에 세 번까지 자동 복구한다.
    pub fn recover(&mut self, code: Option<i32>) -> bool {
        if code == Some(0) || code == Some(EXIT_ESC) || self.launch.is_none() || !self.restart_budget(std::time::Instant::now()) {
            return false;
        }
        let (port, monitor, windowed) = self.launch.unwrap();
        log::warn!("예배당 화면 비정상 종료 {code:?}: 자동 다시 열기 {}/3", self.restarts.len());
        self.spawn(port, monitor, windowed);
        self.child.is_some()
    }

    fn restart_budget(&mut self, now: std::time::Instant) -> bool {
        self.restarts.retain(|at| now.duration_since(*at).as_secs() < 60);
        if self.restarts.len() >= 3 {
            return false;
        }
        self.restarts.push_back(now);
        true
    }

    /// 예배당 창(wp-output) 인자
    pub(crate) fn open_args(&self, port: u16, monitor: usize, windowed: bool) -> Vec<String> {
        let url = format!("http://127.0.0.1:{port}/outputs/main?window=1");
        // --esc-close: 예배당 창을 눌러 고르고 Esc = 예배당 화면 끄기 (Electron main.js onOutputKey)
        let mut a: Vec<String> = vec!["--url".into(), url, "--monitor".into(), monitor.to_string(), "--esc-close".into()];
        a.extend(self.extra_args.iter().cloned());
        if windowed {
            a.push("--windowed".into());
        }
        a
    }

    /// 창이 스스로 끝났으면 한 번만 Some(종료 코드). 조작 화면에서 끈 것(close)은 여기에 안 나온다
    pub fn take_exit(&mut self) -> Option<Option<i32>> {
        let _ = self.running();
        self.exited.take()
    }

    /// 스스로 끝났을 때 창 프로그램이 남긴 오류 (없으면 빈 글). 한 번 가져가면 비워진다
    pub fn take_exit_reason(&mut self) -> String {
        std::mem::take(&mut self.exit_reason)
    }

    /// 시험용: 창이 스스로 끝난 것처럼
    #[cfg(test)]
    pub(crate) fn fake_exit(&mut self, code: Option<i32>) {
        self.child = None;
        self.exited = Some(code);
    }

    pub fn close(&mut self) {
        self.exited = None;
        self.launch = None;
        self.restarts.clear();
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

/// 오류 출력에서 사람이 읽을 마지막 줄 (빈 줄 제외, 너무 길면 자름)
pub(crate) fn last_error_line(err: &str) -> String {
    let line = err.lines().map(str::trim).filter(|l| !l.is_empty()).last().unwrap_or("");
    let line = line.strip_prefix("Error: ").unwrap_or(line);
    if line.chars().count() > 300 { line.chars().take(300).collect::<String>() + "…" } else { line.to_string() }
}

/// 창이 꺼진 이유를 사람이 알아들을 말로 (교회 PC 에서 바로 꺼지던 일, 2026-11-18)
pub(crate) fn explain_exit(reason: &str) -> Option<&'static str> {
    if reason.contains("0x80070005") || reason.contains("액세스가 거부") || reason.to_lowercase().contains("access is denied") {
        Some("WebView2 자료 폴더에 쓸 권한이 없습니다. 새 버전으로 업데이트하거나 관리자에게 설치 폴더 권한을 확인해 달라고 하세요.")
    } else if reason.to_lowercase().contains("webview2") && (reason.contains("0x80070002") || reason.to_lowercase().contains("not found") || reason.contains("찾을 수 없")) {
        Some("Microsoft Edge WebView2 런타임이 없습니다. https://go.microsoft.com/fwlink/p/?LinkId=2124703 에서 설치하세요.")
    } else {
        None
    }
}

impl Drop for OutWin {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod exit_reason_tests {
    use super::*;

    #[test]
    fn restart_is_bounded_and_never_reopens_an_intentional_close() {
        let mut out = OutWin::for_test();
        let now = std::time::Instant::now();
        for _ in 0..3 { assert!(out.restart_budget(now)); }
        assert!(!out.restart_budget(now));
        assert!(out.restart_budget(now + std::time::Duration::from_secs(60)));
        out.launch = Some((37528, 0, true));
        assert!(!out.recover(Some(EXIT_ESC)));
        assert!(!out.recover(Some(0)));
        out.close();
        assert!(!out.recover(None), "조작 화면에서 끄면 다시 켜지지 않는다");
    }

    #[test]
    fn 오류_출력의_마지막_줄을_이유로() {
        let err = "경고 한 줄\r\nError: WebView2 error: WindowsError(Error { code: HRESULT(0x80070005), message: \"액세스가 거부되었습니다.\" })\r\n\r\n";
        let r = last_error_line(err);
        assert!(r.starts_with("WebView2 error"), "{r}");
        assert!(explain_exit(&r).unwrap().contains("권한"));
        assert_eq!(last_error_line("  \n"), "");
        assert!(explain_exit("알 수 없는 오류").is_none());
    }

    #[cfg(windows)]
    #[test]
    fn 스스로_꺼진_창의_오류를_가져간다() {
        // 실제 자식 프로세스: 오류 출력에 쓰고 끝나는 cmd
        let mut o = OutWin::for_test();
        let mut c = std::process::Command::new("cmd");
        c.args(["/c", "echo Error: WebView2 error: 0x80070005 1>&2 & exit 1"]).stderr(Stdio::piped());
        let mut ch = c.spawn().unwrap();
        o.stderr = Arc::new(Mutex::new(String::new()));
        let mut pipe = ch.stderr.take().unwrap();
        let buf = o.stderr.clone();
        std::thread::spawn(move || {
            use std::io::Read;
            let mut s = String::new();
            let _ = pipe.read_to_string(&mut s);
            buf.lock().unwrap().push_str(&s);
        });
        o.child = Some(ch);
        let t = std::time::Instant::now();
        while o.running() && t.elapsed().as_secs() < 10 {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert_eq!(o.take_exit(), Some(Some(1)));
        let r = o.take_exit_reason();
        assert!(r.contains("0x80070005"), "{r}");
        assert!(o.take_exit_reason().is_empty(), "한 번만 가져간다");
    }
}
