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
}

fn exe() -> std::path::PathBuf {
    let me = std::env::current_exe().unwrap_or_default();
    let dir = me.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    dir.join(if cfg!(windows) { "wp-output.exe" } else { "wp-output" })
}

#[cfg(windows)]
fn no_window(c: &mut Command) -> &mut Command {
    use std::os::windows::process::CommandExt;
    c.creation_flags(0x0800_0000) // CREATE_NO_WINDOW
}
#[cfg(not(windows))]
fn no_window(c: &mut Command) -> &mut Command {
    c
}

impl OutWin {
    #[cfg(test)]
    pub(crate) fn for_test() -> Self {
        Self { child: None, monitors: vec![], error: String::new() }
    }
    pub fn new() -> Self {
        let mut o = Self { child: None, monitors: vec![], error: String::new() };
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
                self.error = format!("출력 창 프로그램을 찾지 못했습니다: {e}");
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
                _ => {
                    self.child = None;
                    false
                }
            },
            None => false,
        }
    }

    pub fn open(&mut self, port: u16, monitor: usize, windowed: bool) {
        self.close();
        let url = format!("http://127.0.0.1:{port}/outputs/main?window=1");
        let mut cmd = Command::new(exe());
        cmd.args(["--url", &url, "--monitor", &monitor.to_string()]);
        if windowed {
            cmd.arg("--windowed");
        }
        match cmd.spawn() {
            Ok(c) => {
                self.child = Some(c);
                self.error.clear();
            }
            Err(e) => self.error = format!("출력 창을 열지 못했습니다: {e}"),
        }
    }

    pub fn close(&mut self) {
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
