// 실행 파일에 아이콘·버전 정보(파일 속성의 버전·제품명)를 넣는다 — 새 크레이트 없이 Windows SDK 의 rc.exe(또는 llvm-rc)로.
// 찾지 못하면 경고만 내고 건너뛴다 (그래도 빌드는 된다). wp-app·wp-output 의 build.rs 가 include! 로 쓴다.
use std::path::{Path, PathBuf};

fn find_rc() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("RC").map(PathBuf::from).filter(|p| p.is_file()) {
        return Some(p);
    }
    let path = std::env::var_os("PATH").unwrap_or_default();
    for name in ["rc.exe", "llvm-rc.exe"] {
        if let Some(p) = std::env::split_paths(&path).map(|d| d.join(name)).find(|p| p.is_file()) {
            return Some(p);
        }
    }
    // Windows SDK: 가장 높은 버전의 호스트용 rc.exe
    let arch = if cfg!(target_arch = "aarch64") { "arm64" } else { "x64" };
    for pf in ["ProgramFiles(x86)", "ProgramFiles"] {
        let Some(base) = std::env::var_os(pf).map(|p| PathBuf::from(p).join("Windows Kits").join("10").join("bin")) else { continue };
        let mut vers: Vec<PathBuf> = std::fs::read_dir(&base).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.join(arch).join("rc.exe").is_file()).collect();
        vers.sort();
        if let Some(v) = vers.pop() {
            return Some(v.join(arch).join("rc.exe"));
        }
    }
    None
}

/// description: 작업 관리자·파일 속성에 보이는 이름, original: 설치된 뒤 파일 이름
pub fn embed(description: &str, original: &str, icon: &Path) {
    println!("cargo:rerun-if-changed={}", icon.display());
    println!("cargo:rerun-if-env-changed=RC");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") || std::env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        return;
    }
    let Some(rc) = find_rc() else {
        println!("cargo:warning=rc.exe 를 찾지 못해 실행 파일에 아이콘·버전 정보를 넣지 않습니다 (Windows SDK 설치 또는 환경변수 RC)");
        return;
    };
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let ver = std::env::var("CARGO_PKG_VERSION").unwrap();
    let mut n: Vec<u32> = ver.split(['.', '-', '+']).take(3).map(|x| x.parse().unwrap_or(0)).collect();
    n.resize(3, 0);
    let icon_path = icon.to_string_lossy().replace('\\', "\\\\");
    let script = format!(
        "#pragma code_page(65001)\n1 ICON \"{icon_path}\"\n1 VERSIONINFO\nFILEVERSION {a},{b},{c},0\nPRODUCTVERSION {a},{b},{c},0\nFILEOS 0x40004L\nFILETYPE 0x1L\nBEGIN\n BLOCK \"StringFileInfo\"\n BEGIN\n  BLOCK \"041204B0\"\n  BEGIN\n   VALUE \"CompanyName\", \"WorshipPresenter\\0\"\n   VALUE \"FileDescription\", \"{description}\\0\"\n   VALUE \"FileVersion\", \"{ver}\\0\"\n   VALUE \"InternalName\", \"{original}\\0\"\n   VALUE \"LegalCopyright\", \"GPL-3.0-or-later\\0\"\n   VALUE \"OriginalFilename\", \"{original}\\0\"\n   VALUE \"ProductName\", \"WorshipPresenter\\0\"\n   VALUE \"ProductVersion\", \"{ver}\\0\"\n  END\n END\n BLOCK \"VarFileInfo\"\n BEGIN\n  VALUE \"Translation\", 0x412, 1200\n END\nEND\n",
        a = n[0],
        b = n[1],
        c = n[2]
    );
    let (rc_file, res) = (out.join("app.rc"), out.join("app.res"));
    std::fs::write(&rc_file, script).unwrap();
    let llvm = rc.file_name().is_some_and(|f| f.to_string_lossy().to_lowercase().starts_with("llvm"));
    let mut cmd = std::process::Command::new(&rc);
    if llvm {
        cmd.arg("/C").arg("65001").arg("/FO").arg(&res).arg(&rc_file);
    } else {
        cmd.arg("/nologo").arg("/fo").arg(&res).arg(&rc_file);
    }
    match cmd.output() {
        Ok(o) if o.status.success() && res.is_file() => println!("cargo:rustc-link-arg-bins={}", res.display()),
        Ok(o) => println!("cargo:warning=rc 실패 ({}): 아이콘·버전 정보 없이 빌드합니다: {}", rc.display(), String::from_utf8_lossy(&o.stdout).trim()),
        Err(e) => println!("cargo:warning=rc 실행 실패 ({}): {e}", rc.display()),
    }
}
