//! 예배당 모니터 출력 창 (wp-output.exe 를 따로 띄운다)

use std::process::{Child, Command};

#[derive(Clone, Debug)]
pub struct Monitor {
    pub index: usize,
    pub name: String,
    pub size: (i32, i32),
    pub primary: bool,
}

pub struct OutWin {
    child: Option<Child>,
    pub monitors: Vec<Monitor>,
    pub error: String,
    /// 창이 스스로 끝났음 (종료 코드). 조작 화면이 한 번 가져가 알림·OBS 연동을 처리한다 (take_exit)
    exited: Option<Option<i32>>,
    /// 점검용으로 창에 더 넘길 인자 (--selftest-esc)
    pub extra_args: Vec<String>,
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
        Self { child: None, monitors: vec![], error: String::new(), exited: None, extra_args: vec![] }
    }
    pub fn new() -> Self {
        let mut o = Self { child: None, monitors: vec![], error: String::new(), exited: None, extra_args: vec![] };
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
                    Some(Monitor { index: c[0].parse().ok()?, name: c[1].trim_start_matches(r"\\.\").to_string(), size: (*g.get(2)?, *g.get(3)?), primary: c[3] == "true" })
                })
                .collect(),
            Err(e) => {
                self.error = format!("예배당 화면 프로그램을 찾지 못했습니다: {e}");
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
                    self.child = None;
                    false
                }
            },
            None => false,
        }
    }

    pub fn open(&mut self, port: u16, monitor: usize, windowed: bool) {
        self.close();
        let mut cmd = Command::new(exe());
        cmd.args(self.open_args(port, monitor, windowed));
        match cmd.spawn() {
            Ok(c) => {
                bind_to_app(&c);
                self.child = Some(c);
                self.error.clear();
            }
            Err(e) => self.error = format!("예배당 화면을 열지 못했습니다: {e}"),
        }
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

    /// 시험용: 창이 스스로 끝난 것처럼
    #[cfg(test)]
    pub(crate) fn fake_exit(&mut self, code: Option<i32>) {
        self.child = None;
        self.exited = Some(code);
    }

    pub fn close(&mut self) {
        self.exited = None;
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

impl Drop for OutWin {
    fn drop(&mut self) {
        self.close();
    }
}
