//! 연결 점검 · 로그 묶음 (Electron diagnose.js · logbundle.js) [트랙 B]
//! 예배 전에 한 번 눌러 보면 되도록: 항목마다 상태(ok · warn · fail · info)와 '무엇이 문제인지 · 어떻게 고치는지'.
//! 모니터처럼 조작 화면만 아는 것은 Desktop 으로 받는다.

use crate::core::Core;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// 조작 화면이 알려 주는 것 (예배당 창 · 모니터)
#[derive(Default, Clone)]
pub struct Desktop {
    pub monitors: usize,
    pub output_running: bool,
    pub output_monitor: String,
    pub same_as_control: bool,
}

fn item(id: &str, title: &str, status: &str, detail: impl Into<String>, fix: Vec<String>) -> Value {
    json!({ "id": id, "title": title, "status": status, "detail": detail.into(), "fix": fix })
}

fn v(xs: &[&str]) -> Vec<String> {
    xs.iter().map(|x| x.to_string()).collect()
}

/// 큐시트 순서가 가리키는 파일이 실제로 있는지 (없으면 경로)
pub fn missing_file(core: &Core, r: &str) -> Option<String> {
    let s = r.trim();
    if s.is_empty() || s.starts_with("http:") || s.starts_with("https:") || s.starts_with("data:") || s.starts_with("blob:") || s.starts_with('#') {
        return None;
    }
    if let Some(m) = s.strip_prefix("/media/") {
        let f = core.media_dir.join(m.split('?').next().unwrap_or(""));
        return (!f.exists()).then(|| f.to_string_lossy().to_string());
    }
    if let Some(q) = s.strip_prefix("/lib/file?p=") {
        let p = percent_decode(q);
        return (!Path::new(&p).is_file()).then_some(p);
    }
    if s.starts_with('/') {
        return None; // 프로그램 안의 그림 (/shared/… · /hymn/…)
    }
    let p = PathBuf::from(s);
    (p.is_absolute() && !p.exists()).then(|| s.to_string())
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(x) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(x);
                i += 3;
                continue;
            }
        }
        out.push(if b[i] == b'+' { b' ' } else { b[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

/// 열린 큐시트를 예배 전에 훑기: 없는 파일 · 빈 가사 · 잘못된 장절
pub fn check_cue(core: &mut Core) -> Value {
    let Some(cue) = core.cue.clone() else {
        return item("cue", "오늘 큐시트", "info", "열린 큐시트가 없습니다.", v(&["큐시트를 열면 순서마다 파일·가사·장절을 함께 점검합니다."]));
    };
    let mut problems: Vec<String> = vec![];
    let ver = core.version_id();
    for (i, it) in cue.items.iter().enumerate() {
        let n = format!("{}번 '{}'", i + 1, it["title"].as_str().unwrap_or(""));
        for k in ["file", "image", "bg", "music"] {
            if let Some(m) = it[k].as_str().and_then(|x| missing_file(core, x)) {
                problems.push(format!("{n}: 파일이 없습니다 ({m})"));
            }
        }
        for img in it["images"].as_array().into_iter().flatten().filter_map(|x| x.as_str()) {
            if let Some(m) = missing_file(core, img) {
                problems.push(format!("{n}: 슬라이드쇼 그림이 없습니다 ({m})"));
            }
        }
        let ty = it["type"].as_str().unwrap_or("");
        let empty = |k: &str| it[k].as_str().unwrap_or("").trim().is_empty();
        if ty == "song" && empty("lyrics") {
            problems.push(format!("{n}: 찬양 가사가 비어 있습니다"));
        }
        if ty == "text" && empty("body") {
            problems.push(format!("{n}: 공지 내용이 비어 있습니다"));
        }
        if ty == "bible" {
            if empty("ref") {
                problems.push(format!("{n}: 성경 장절이 비어 있습니다"));
            } else {
                let vid = it["versionId"].as_str().filter(|x| !x.is_empty() && core.store.has_version(x)).map(String::from).unwrap_or_else(|| ver.clone());
                let r = core.store.resolve_reference(&vid, it["ref"].as_str().unwrap_or(""));
                if r["ok"] != json!(true) {
                    problems.push(format!("{n}: 장절을 찾지 못했습니다 ({})", it["ref"].as_str().unwrap_or("")));
                }
            }
        }
        if (ty == "video" || ty == "audio") && empty("file") {
            problems.push(format!("{n}: 파일을 고르지 않았습니다"));
        }
        if ty == "camera" && empty("deviceId") {
            problems.push(format!("{n}: 카메라를 고르지 않았습니다"));
        }
    }
    let title = format!("오늘 큐시트 · {}", cue.name);
    if problems.is_empty() {
        return item("cue", &title, "ok", format!("순서 {}개 — 파일·가사·장절 모두 괜찮습니다.", cue.items.len()), vec![]);
    }
    let total = problems.len();
    let mut fix: Vec<String> = problems.into_iter().take(20).collect();
    if total > 20 {
        fix.push(format!("… 외 {}곳", total - 20));
    }
    fix.push("순서를 눌러 편집칸에서 파일을 다시 고르거나 내용을 채우세요.".into());
    item("cue", &title, "warn", format!("고칠 곳 {total}곳"), fix)
}

/// 드라이브 남은 공간 (GB)
pub fn free_gb(dir: &Path) -> Option<f64> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let w: Vec<u16> = dir.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
        let mut free: u64 = 0;
        // SAFETY: 널로 끝나는 경로와 u64 출력 포인터
        let ok = unsafe { windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(w.as_ptr(), &mut free, std::ptr::null_mut(), std::ptr::null_mut()) };
        if ok != 0 {
            return Some(free as f64 / 1024f64.powi(3));
        }
        None
    }
    #[cfg(not(windows))]
    {
        let _ = dir;
        None
    }
}

/// 연결 점검 → { at, items, summary: { fail, warn } }
pub fn diagnose(core: &mut Core, d: &Desktop) -> Value {
    let s = core.settings.get().clone();
    let port = core.port;
    let mut out: Vec<Value> = vec![];

    // 1. 예배당 화면 (보조 모니터)
    if d.monitors < 2 {
        out.push(item("display", "예배당 화면 (보조 모니터)", "warn", format!("모니터가 {}대만 보입니다.", d.monitors), v(&["모니터·프로젝터 케이블(HDMI)과 전원을 확인하세요.", "Windows 키 + P → '확장' 을 고르세요 ('복제' 면 조작 화면이 그대로 보입니다).", "연결한 뒤 위 줄에서 예배당 모니터를 고르세요."])));
    } else if d.same_as_control {
        out.push(item("display", "예배당 화면 (보조 모니터)", "warn", "예배당 화면이 조작 화면과 같은 (주) 모니터에 있습니다.", v(&["위 줄의 모니터 고르기에서 다른 모니터를 고르세요."])));
    } else {
        out.push(item("display", "예배당 화면 (보조 모니터)", "ok", format!("모니터 {}대 · {}", d.monitors, if d.output_running { format!("출력 켜짐 ({})", d.output_monitor) } else { "출력 꺼짐 — '예배당 화면 켜기' 로 켭니다".into() }), vec![]));
    }

    // 2. OBS 브라우저 소스
    let count = |o: &str| core.outputs.values().filter(|c| c.client == o && c.kind != "window").count();
    let (nm, no) = (count("main"), count("overlay"));
    if nm + no > 0 {
        out.push(item("obs", "OBS 브라우저 소스", "ok", format!("연결됨 — 전체 {nm}개 · 하단 자막 {no}개"), vec![]));
    } else {
        out.push(item("obs", "OBS 브라우저 소스", "info", "OBS 가 연결돼 있지 않습니다 (방송을 안 하면 괜찮습니다).", vec![
            format!("OBS → 소스 + → 브라우저 → 주소 http://127.0.0.1:{port}/outputs/main (전체) 또는 /outputs/overlay (하단 자막), 너비 1920 · 높이 1080."),
            "이미 넣었다면 그 소스를 오른쪽 클릭 → 속성 → '현재 페이지의 캐시를 새로 고침'.".into(),
            "자막 PC 와 송출 PC 가 따로면 '내부망 출력' 을 켜세요.".into(),
        ]));
    }

    // 3. 내부망 출력
    let lan = crate::lan::status(core);
    if lan["enabled"] == true {
        if lan["running"] == true {
            out.push(item("lan", "내부망 출력", "ok", format!("켜짐 — {}", lan["main"][0].as_str().unwrap_or("")), v(&["다른 PC 에서 안 열리면 Windows 방화벽에서 이 프로그램의 '개인 네트워크' 허용을 확인하세요.", "두 PC 가 같은 공유기(같은 192.168.x.x)에 있어야 합니다."])));
        } else {
            out.push(item("lan", "내부망 출력", "fail", format!("켜 두었지만 열리지 않았습니다{}.", lan["error"].as_str().filter(|x| !x.is_empty()).map(|e| format!(" — {e}")).unwrap_or_default()), vec![format!("포트 {} 을 다른 프로그램이 쓰고 있을 수 있습니다. 도구 → 내부망 출력에서 껐다 켜 보세요.", lan["port"]), "그래도 안 되면 프로그램을 다시 시작하세요.".into()]));
        }
    } else {
        out.push(item("lan", "내부망 출력", "info", "꺼짐 (이 PC 의 OBS 만 쓸 때는 필요 없습니다).", vec![]));
    }

    // 4. 휴대폰 리모컨
    let rm = crate::remote::status(core);
    if rm["enabled"] == true {
        if rm["running"] == true {
            out.push(item("remote", "휴대폰 리모컨", "ok", format!("켜짐 — {}", rm["urls"][0].as_str().unwrap_or("")), v(&["휴대폰이 같은 Wi-Fi 에 있어야 합니다. 안 열리면 방화벽의 '개인 네트워크' 허용을 확인하세요."])));
        } else {
            out.push(item("remote", "휴대폰 리모컨", "fail", format!("켜 두었지만 열리지 않았습니다{}.", rm["error"].as_str().filter(|x| !x.is_empty()).map(|e| format!(" — {e}")).unwrap_or_default()), v(&["도구 → 휴대폰 리모컨 에서 껐다 켜 보세요."])));
        }
    } else {
        out.push(item("remote", "휴대폰 리모컨", "info", "꺼짐.", vec![]));
    }

    // 5. 성경 · 찬송가 · 라이브러리
    let versions = core.store.list_versions();
    if versions.is_empty() {
        out.push(item("bible", "성경 번역본", "fail", "성경 데이터가 없습니다.", v(&["파일 → 성경 번역본 가져오기, 또는 프로그램을 다시 설치하세요."])));
    } else {
        let names: Vec<String> = versions.iter().take(4).map(|x| x.name.clone()).collect();
        out.push(item("bible", "성경 번역본", "ok", format!("{}개 — {}{}", versions.len(), names.join(" · "), if versions.len() > 4 { " …" } else { "" }), vec![]));
    }
    let folders = |k: &str| -> Vec<Value> { s[k].as_array().into_iter().flatten().filter(|f| f["path"].as_str().is_some_and(|p| !p.is_empty())).cloned().collect() };
    let hymn = folders("hymnFolders");
    let gone: Vec<&Value> = hymn.iter().filter(|f| !Path::new(f["path"].as_str().unwrap_or("")).exists()).collect();
    if !gone.is_empty() {
        let mut fix: Vec<String> = gone.iter().map(|f| format!("{} {}", f["name"].as_str().unwrap_or(""), f["path"].as_str().unwrap_or("")).trim().to_string()).collect();
        fix.push("외장 드라이브·원드라이브가 연결돼 있는지 확인하거나, 찬송가 탭에서 폴더를 다시 등록하세요.".into());
        out.push(item("hymn", "찬송가 PPT 폴더", "warn", format!("{}개 폴더를 찾을 수 없습니다.", gone.len()), fix));
    } else if hymn.is_empty() {
        out.push(item("hymn", "찬송가 PPT 폴더", "info", "등록한 폴더가 없습니다 (베들레헴 찬송가만 쓰면 괜찮습니다).", vec![]));
    } else {
        out.push(item("hymn", "찬송가 PPT 폴더", "ok", format!("{}개 폴더 연결됨", hymn.len()), vec![]));
    }
    let lib_gone: Vec<String> = [folders("imageFolders"), folders("audioFolders")].concat().iter().filter_map(|f| f["path"].as_str()).filter(|p| !Path::new(p).exists()).map(String::from).collect();
    if !lib_gone.is_empty() {
        let n = lib_gone.len();
        let mut fix = lib_gone;
        fix.push("드라이브 연결을 확인하거나 라이브러리에서 폴더를 다시 등록하세요.".into());
        out.push(item("library", "라이브러리 폴더 (그림·영상·음악)", "warn", format!("{n}개 폴더를 찾을 수 없습니다."), fix));
    }

    // 6. 오늘 큐시트
    out.push(check_cue(core));

    // 7. 여러 PC 동기화
    let sd = s["syncDir"].as_str().unwrap_or("").trim().to_string();
    if !sd.is_empty() {
        if !Path::new(&sd).is_dir() {
            out.push(item("sync", "여러 PC 동기화", "fail", format!("동기화 폴더가 없습니다: {sd}"), v(&["원드라이브가 켜져 있는지, 폴더 이름이 바뀌지 않았는지 확인하세요.", "파일 → 여러 PC 동기화 에서 폴더를 다시 고르세요."])));
        } else {
            let probe = Path::new(&sd).join(".wp-write-test");
            let ok = std::fs::write(&probe, b"x").is_ok();
            let _ = std::fs::remove_file(&probe);
            out.push(if ok { item("sync", "여러 PC 동기화", "ok", format!("폴더 연결됨 — {sd}"), vec![]) } else { item("sync", "여러 PC 동기화", "fail", "동기화 폴더에 쓸 수 없습니다.", v(&["폴더 권한을 확인하거나 다른 폴더를 고르세요."])) });
        }
    }

    // 8. 저장 공간
    if let Some(gb) = free_gb(&core.data_dir) {
        out.push(if gb < 1.0 { item("disk", "저장 공간", "warn", format!("데이터 드라이브에 {gb:.1}GB 남았습니다."), v(&["필요 없는 파일을 지우세요. 공간이 모자라면 큐시트·백업 저장이 실패할 수 있습니다."])) } else { item("disk", "저장 공간", "ok", format!("{gb:.0}GB 남음"), vec![]) });
    }

    let fail = out.iter().filter(|x| x["status"] == "fail").count();
    let warn = out.iter().filter(|x| x["status"] == "warn").count();
    json!({ "at": crate::util::iso_now(), "items": out, "summary": { "fail": fail, "warn": warn } })
}

const SECRETS: [&str; 7] = ["key", "pin", "token", "password", "secret", "apikey", "keyenc"];

fn secret(k: &str) -> bool {
    let k = k.to_lowercase();
    SECRETS.iter().any(|s| k.ends_with(s))
}

/// 설정에서 비밀(열쇠·PIN·AI 키)을 가리고, 긴 글(그림 데이터 등)은 길이만
pub fn redact(v: &Value, k: &str) -> Value {
    match v {
        Value::String(x) => {
            if secret(k) && !x.is_empty() {
                json!("(숨김)")
            } else if x.chars().count() > 300 {
                json!(format!("(글 {}자)", x.chars().count()))
            } else {
                v.clone()
            }
        }
        Value::Array(a) => {
            if a.len() > 30 {
                json!([format!("(항목 {}개)", a.len())])
            } else {
                Value::Array(a.iter().map(|x| redact(x, k)).collect())
            }
        }
        Value::Object(o) => Value::Object(o.iter().map(|(kk, vv)| (kk.clone(), if secret(kk) && !vv.is_null() && vv != "" { json!("(숨김)") } else { redact(vv, kk) })).collect()),
        _ => v.clone(),
    }
}

/// 로그 묶음: 문제를 알릴 때 보낼 글 파일 하나 → (파일 이름, 글)
/// 큐시트 내용·찬양 가사·성경 본문은 넣지 않는다 (로그에 남은 송출 제목 정도만).
pub fn log_bundle(core: &mut Core, d: &Desktop, days: usize) -> (String, String) {
    let diag = diagnose(core, d);
    let ms = crate::util::now_ms() + crate::localtime::local_offset_ms();
    let (_, hm, _) = crate::localtime::split(ms);
    let day = crate::util::iso_now(); // UTC 날짜지만 파일 이름에만
    let stamp = format!("{}-{}", day[..10].replace('-', ""), hm.replace(':', ""));
    let mut lines: Vec<String> = vec![
        format!("워십프리젠터 (Rust 판) 로그 묶음 — {}", crate::util::iso_now()),
        format!("버전 {} · {} {} · 데이터 폴더 {}", env!("CARGO_PKG_VERSION"), std::env::consts::OS, std::env::consts::ARCH, core.data_dir.display()),
        String::new(),
        "## 연결 점검".into(),
    ];
    for x in diag["items"].as_array().into_iter().flatten() {
        let fix: Vec<String> = x["fix"].as_array().into_iter().flatten().filter_map(|f| f.as_str().map(|s| format!("\n    - {s}"))).collect();
        lines.push(format!("[{}] {}: {}{}", x["status"].as_str().unwrap_or(""), x["title"].as_str().unwrap_or(""), x["detail"].as_str().unwrap_or(""), fix.join("")));
    }
    lines.push(String::new());
    lines.push("## 출력 상태".into());
    lines.push(serde_json::to_string_pretty(&json!({ "outputWindow": d.output_running, "monitor": d.output_monitor, "clients": core.outputs.values().map(|c| json!({ "output": c.client, "kind": c.kind, "connectedAt": c.connected_at })).collect::<Vec<_>>() })).unwrap_or_default());
    lines.push(String::new());
    lines.push("## 설정 (열쇠·PIN·AI 키는 가림)".into());
    lines.push(serde_json::to_string_pretty(&redact(core.settings.get(), "")).unwrap_or_default());
    lines.push(String::new());
    lines.push(format!("## 최근 {days}일 로그"));
    lines.push(crate::logger::recent_logs(&crate::logger::dir(&core.data_dir), days, 400 * 1024));
    (format!("워십프리젠터-로그-{stamp}.txt"), lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn redacts() {
        let s = json!({ "triggerKey": "abc", "remote": { "pin": "1234", "port": 37540 }, "ai": { "keyEnc": "dp1:xx", "model": "m" }, "big": "가".repeat(400), "list": (0..40).collect::<Vec<_>>() });
        let r = redact(&s, "");
        assert_eq!(r["triggerKey"], "(숨김)");
        assert_eq!(r["remote"]["pin"], "(숨김)");
        assert_eq!(r["remote"]["port"], 37540);
        assert_eq!(r["ai"]["keyEnc"], "(숨김)");
        assert_eq!(r["ai"]["model"], "m");
        assert_eq!(r["big"], "(글 400자)");
        assert_eq!(r["list"], json!(["(항목 40개)"]));
    }
    #[test]
    fn decode() {
        assert_eq!(percent_decode("C%3A%5C%EA%B0%80.png"), "C:\\가.png");
    }
    #[test]
    fn full_check() {
        let dir = std::env::temp_dir().join(format!("wp-diag-{}-{}", crate::util::now_ms(), rand::random::<u32>()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("bible.db"), b"").unwrap();
        let mut c = Core::open(dir.clone(), dir.clone(), None).unwrap();
        c.new_cue("주일").unwrap();
        let mut it = crate::core::new_item("image", "그림");
        it["image"] = json!("/lib/file?p=C%3A%5Cno-such-file.png");
        c.add_item(it, None);
        c.add_item(crate::core::new_item("song", "찬양"), None);
        let r = diagnose(&mut c, &Desktop { monitors: 1, ..Default::default() });
        let items = r["items"].as_array().unwrap();
        let get = |id: &str| items.iter().find(|x| x["id"] == id).unwrap().clone();
        assert_eq!(get("display")["status"], "warn");
        assert_eq!(get("bible")["status"], "fail");
        let cue = get("cue");
        assert_eq!(cue["status"], "warn");
        assert!(cue["fix"].to_string().contains("no-such-file"));
        assert!(cue["fix"].to_string().contains("찬양 가사가 비어"));
        let (name, text) = log_bundle(&mut c, &Desktop::default(), 7);
        assert!(name.starts_with("워십프리젠터-로그-") && name.ends_with(".txt"));
        assert!(text.contains("## 연결 점검") && text.contains("## 최근 7일 로그"));
    }
}
