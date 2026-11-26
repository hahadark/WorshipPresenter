//! Windows 방화벽: 휴대폰 리모컨 · 내부망 출력 · 휴대폰 카메라처럼 다른 기기가 이 PC 로 들어오는 연결을 받도록.
//! - 확인(status): 읽기만 한다 (관리자 권한 필요 없음). 이 프로그램(실행 파일)에 걸린 받는 쪽 규칙과 지금 네트워크 종류를 본다.
//! - 열기(open): 관리자 승인 창(UAC)을 한 번 띄워 '막기' 규칙(처음 뜬 방화벽 창에서 [취소]를 누르면 생김)을 지우고,
//!   같은 내부망(LocalSubnet)에서만 들어오게 허용하는 규칙 하나를 만든다. 인터넷 쪽에서는 그대로 막힌다.

use serde_json::{json, Value};
use std::path::Path;

pub const RULE_NAME: &str = "WorshipPresenter (휴대폰 · 내부망)";

/// PowerShell 결과 줄 → 상태. 줄 모양: "RULE|Allow|Private,Public" 여러 줄 + "NET|Public,Private" + "FW|Domain,Private,Public"
pub fn parse_status(out: &str) -> Value {
    let mut allow: Vec<String> = vec![]; // 허용 규칙의 프로필
    let mut block: Vec<String> = vec![];
    let mut nets: Vec<String> = vec![];
    let mut fw_on: Vec<String> = vec![];
    for line in out.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let parts: Vec<&str> = line.split('|').collect();
        match parts.as_slice() {
            ["RULE", action, profile, ..] => {
                let p = profile.to_string();
                if action.eq_ignore_ascii_case("Allow") {
                    allow.push(p)
                } else if action.eq_ignore_ascii_case("Block") {
                    block.push(p)
                }
            }
            ["NET", cats] => nets = cats.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
            ["FW", on] => fw_on = on.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect(),
            _ => {}
        }
    }
    // 지금 네트워크 종류(Public/Private/DomainAuthenticated) → 방화벽 프로필 이름
    let prof = |c: &str| match c {
        "DomainAuthenticated" | "Domain" => "Domain".to_string(),
        "Private" => "Private".to_string(),
        _ => "Public".to_string(),
    };
    let cur: Vec<String> = if nets.is_empty() { vec!["Public".into()] } else { nets.iter().map(|c| prof(c)).collect() };
    let covers = |rule_profiles: &str, p: &str| {
        let r = rule_profiles.to_ascii_lowercase();
        r.contains("any") || r.split(',').any(|x| x.trim().eq_ignore_ascii_case(p))
    };
    // 방화벽이 꺼진 프로필이면 규칙과 상관없이 들어온다
    let off = |p: &str| !fw_on.is_empty() && !fw_on.iter().any(|x| x.eq_ignore_ascii_case(p));
    let blocked = cur.iter().any(|p| !off(p) && block.iter().any(|r| covers(r, p)));
    let allowed = cur.iter().all(|p| off(p) || allow.iter().any(|r| covers(r, p)));
    let state = if blocked { "blocked" } else if allowed { "ok" } else { "none" };
    json!({ "state": state, "network": cur, "allowRules": allow.len(), "blockRules": block.len(), "firewallOn": fw_on })
}

/// 사람이 읽을 한 줄
pub fn describe(st: &Value) -> String {
    let net = st["network"].as_array().map(|a| a.iter().filter_map(|x| x.as_str()).map(|x| match x { "Private" => "개인", "Domain" => "도메인", _ => "공용" }).collect::<Vec<_>>().join(", ")).unwrap_or_default();
    match st["state"].as_str().unwrap_or("") {
        "ok" => crate::i18n::tr_args("열려 있음 (지금 네트워크: {1})", &[&net]),
        "blocked" => crate::i18n::tr_args("막혀 있음 — 처음 뜬 방화벽 창에서 [취소]를 눌렀던 것 같습니다 (지금 네트워크: {1})", &[&net]),
        "none" => crate::i18n::tr_args("허용 규칙 없음 — 휴대폰·다른 PC 에서 안 열릴 수 있습니다 (지금 네트워크: {1})", &[&net]),
        _ => st["error"].as_str().unwrap_or("").to_string(),
    }
}

fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

fn status_script(exe: &Path) -> String {
    format!(
        r#"$ErrorActionPreference='SilentlyContinue'
$p = {}
Get-NetFirewallApplicationFilter -Program $p | Get-NetFirewallRule | Where-Object {{ $_.Enabled -eq 'True' -and $_.Direction -eq 'Inbound' }} | ForEach-Object {{ 'RULE|' + $_.Action + '|' + ($_.Profile -replace ', ',',') }}
'NET|' + ((Get-NetConnectionProfile | ForEach-Object {{ [string]$_.NetworkCategory }}) -join ',')
'FW|' + ((Get-NetFirewallProfile | Where-Object {{ $_.Enabled -eq 'True' }} | ForEach-Object {{ $_.Name }}) -join ',')
"#,
        ps_quote(&exe.to_string_lossy())
    )
}

fn open_script(exe: &Path) -> String {
    format!(
        r#"$ErrorActionPreference='Stop'
$p = {}
Get-NetFirewallApplicationFilter -Program $p -ErrorAction SilentlyContinue | Get-NetFirewallRule -ErrorAction SilentlyContinue | Where-Object {{ $_.Direction -eq 'Inbound' -and $_.Action -eq 'Block' }} | Remove-NetFirewallRule -ErrorAction SilentlyContinue
Get-NetFirewallRule -DisplayName {} -ErrorAction SilentlyContinue | Remove-NetFirewallRule -ErrorAction SilentlyContinue
New-NetFirewallRule -DisplayName {} -Direction Inbound -Action Allow -Program $p -Profile Any -RemoteAddress LocalSubnet -Description 'WorshipPresenter: phone remote / LAN output / phone camera (local network only)' | Out-Null
"#,
        ps_quote(&exe.to_string_lossy()),
        ps_quote(RULE_NAME),
        ps_quote(RULE_NAME)
    )
}

fn encoded(script: &str) -> String {
    let utf16: Vec<u8> = script.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    crate::dpapi::b64(&utf16)
}

#[cfg(windows)]
fn hidden(cmd: &mut std::process::Command) -> &mut std::process::Command {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x0800_0000) // CREATE_NO_WINDOW
}

/// 지금 상태 (읽기만, 1~3초). 윈도우가 아니면 확인할 것이 없다고 본다
pub fn status(exe: &Path) -> Value {
    #[cfg(windows)]
    {
        let out = hidden(&mut std::process::Command::new("powershell.exe")).args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &encoded(&status_script(exe))]).output();
        match out {
            Ok(o) => parse_status(&String::from_utf8_lossy(&o.stdout)),
            Err(e) => json!({ "state": "error", "error": e.to_string() }),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = exe;
        json!({ "state": "ok", "network": [], "allowRules": 0, "blockRules": 0, "firewallOn": [] })
    }
}

/// 관리자 승인 창을 띄워 규칙을 만든다 (끝날 때까지 기다림). 취소하면 Err
pub fn open(exe: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        let inner = encoded(&open_script(exe));
        let outer = format!(
            "try {{ $r = Start-Process powershell.exe -Verb RunAs -WindowStyle Hidden -Wait -PassThru -ArgumentList '-NoProfile','-NonInteractive','-EncodedCommand','{inner}'; exit $r.ExitCode }} catch {{ exit 1223 }}"
        );
        let st = hidden(&mut std::process::Command::new("powershell.exe")).args(["-NoProfile", "-NonInteractive", "-EncodedCommand", &encoded(&outer)]).status().map_err(|e| e.to_string())?;
        match st.code() {
            Some(0) => Ok(()),
            Some(1223) => Err(crate::i18n::tr("관리자 승인을 취소했습니다.").into()),
            c => Err(crate::i18n::tr_args("방화벽 규칙을 만들지 못했습니다 (코드 {1}).", &[&c.unwrap_or(-1)])),
        }
    }
    #[cfg(not(windows))]
    {
        let _ = exe;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse() {
        // 허용 규칙(모든 프로필) · 개인 네트워크 → 열림
        let s = parse_status("RULE|Allow|Any\nNET|Private\nFW|Domain,Private,Public\n");
        assert_eq!(s["state"], "ok");
        // 처음 방화벽 창에서 취소 → 공용에 막기 규칙
        let s = parse_status("RULE|Block|Public\nRULE|Allow|Private\nNET|Public\nFW|Domain,Private,Public");
        assert_eq!(s["state"], "blocked");
        assert_eq!(s["blockRules"], 1);
        // 개인용으로만 허용했는데 지금 공용 네트워크 → 허용 없음
        let s = parse_status("RULE|Allow|Private\nNET|Public\nFW|Domain,Private,Public");
        assert_eq!(s["state"], "none");
        // 규칙 없음 · 그 프로필의 방화벽이 꺼져 있으면 열림
        assert_eq!(parse_status("NET|Private\nFW|Public")["state"], "ok");
        assert_eq!(parse_status("NET|Private\nFW|Private")["state"], "none");
        // 'Private, Public' 처럼 여러 프로필
        assert_eq!(parse_status("RULE|Allow|Private,Public\nNET|Public\nFW|Public")["state"], "ok");
        // 도메인 네트워크 이름
        assert_eq!(parse_status("RULE|Allow|Domain\nNET|DomainAuthenticated\nFW|Domain")["state"], "ok");
        assert!(describe(&parse_status("NET|Public\nFW|Public")).contains("공용"));
    }

    #[test]
    fn scripts() {
        let p = Path::new(r"C:\Program Files\O'Brien\wp-app.exe");
        let s = open_script(p);
        assert!(s.contains("'C:\\Program Files\\O''Brien\\wp-app.exe'"), "작은따옴표는 두 번");
        assert!(s.contains("-RemoteAddress LocalSubnet") && s.contains("-Action Allow") && s.contains("Remove-NetFirewallRule"));
        assert!(status_script(p).contains("Get-NetConnectionProfile"));
        // 인코딩: UTF-16LE base64 ('가A' = 00 AC 41 00)
        assert_eq!(encoded("가A"), "AKxBAA==");
    }

    /// 실제 이 PC 상태 (읽기만): cargo test -p wp-core firewall::tests::live -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live() {
        let exe = std::env::current_exe().unwrap();
        let s = status(&exe);
        println!("{s}\n{}", describe(&s));
    }
}
