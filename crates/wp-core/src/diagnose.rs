//! 연결 점검 · 로그 묶음 (Electron diagnose.js · logbundle.js) [트랙 B]
//! 예배 전에 한 번 눌러 보면 되도록: 항목마다 상태(ok · warn · fail · info)와 '무엇이 문제인지 · 어떻게 고치는지'.
//! 모니터처럼 조작 화면만 아는 것은 Desktop 으로 받는다.

use crate::core::Core;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// 조작 화면이 알려 주는 것 (예배당 창 · 모니터 · OBS 원격 조작 상태 캠시)
#[derive(Default, Clone)]
pub struct Desktop {
    pub monitors: usize,
    pub output_running: bool,
    pub output_monitor: String,
    pub same_as_control: bool,
    /// OBS 원격 조작(obs-websocket) 상태 캠시. 연결 점검은 OBS 에 직접 붙지 않고 이 값으로 판정한다.
    /// Null = 아직 확인한 것이 없음. 객체라면 { connected, scene, streaming, recording, error } (있는 칸만).
    pub obs_ws: Value,
}

/// OBS 원격 조작 점검 (Electron diagnose.js:82-88 obsws). 설정 obs.enabled 가 꺼져 있으면 항목 없음.
/// OBS 에 연결하지 않고 설정과 상태 캠시만으로 판정한다 (캐시가 없으면 '확인 못 함' 경고).
pub fn check_obs_ws(settings: &Value, cache: &Value) -> Option<Value> {
    use crate::i18n::{tr, tr_args};
    let o = &settings["obs"];
    if o["enabled"].as_bool() != Some(true) {
        return None;
    }
    let title = tr("OBS 원격 조작");
    let fix_electron = || {
        vec![
            tr("OBS 가 켜져 있는지 확인하세요 (OBS 28 이상)."),
            tr("OBS → 도구 → WebSocket 서버 설정 → 'WebSocket 서버 사용' 을 켜고, 포트(기본 4455)와 비밀번호를 도구 → OBS 원격 조작에 똑같이 넣으세요."),
            tr("OBS 가 다른 PC 에 있으면 주소에 그 PC 의 IP(192.168.x.x)를 넣고, 그 PC 방화벽에서 OBS 를 허용하세요."),
        ]
    };
    let host = o["host"].as_str().filter(|x| !x.trim().is_empty()).unwrap_or("127.0.0.1").trim().to_string();
    let port = o["port"].as_u64().unwrap_or(4455);
    if port == 0 || port > 65535 {
        return Some(item("obsws", &title, "fail", tr_args("포트가 잘못되었습니다: {1}", &[&o["port"]]), vec![tr("도구 → OBS 원격 조작에서 포트를 OBS 의 WebSocket 서버 설정과 같게(기본 4455) 넣으세요.")]));
    }
    if cache["connected"].as_bool() == Some(true) {
        let detail = match cache["scene"].as_str() {
            Some(scene) => tr_args(
                "연결됨 — 장면 '{1}' · {2} · {3}",
                &[&scene, &tr(if cache["streaming"] == true { "방송 중" } else { "방송 안 함" }), &tr(if cache["recording"] == true { "녹화 중" } else { "녹화 안 함" })],
            ),
            None => tr_args("연결됨 — {1}:{2} (OBS 원격 조작 창에서 마지막으로 확인한 상태)", &[&host, &port]),
        };
        return Some(item("obsws", &title, "ok", detail, vec![]));
    }
    if let Some(e) = cache["error"].as_str().filter(|x| !x.is_empty()) {
        return Some(item("obsws", &title, "fail", e.to_string(), fix_electron()));
    }
    if cache["connected"].as_bool() == Some(false) {
        return Some(item("obsws", &title, "fail", tr("OBS 에 연결돼 있지 않습니다."), fix_electron()));
    }
    // 상태 캠시가 없음: 예배 전에 확인해 보라고 알린다 (연결 점검은 OBS 에 직접 붙지 않는다)
    let mut fix = vec![tr("도구 → OBS 원격 조작 창을 열어 연결을 한 번 확인하세요. 그 결과가 여기에 반영됩니다.")];
    fix.extend(fix_electron());
    Some(item("obsws", &title, "warn", tr_args("OBS 연결을 아직 확인하지 않았습니다 ({1}:{2}).", &[&host, &port]), fix))
}

/// 무대 화면 모니터 점검 (Electron diagnose.js:68-72 stagemon). Rust 판은 무대 화면이 별도 wp-output 창(또는 브라우저)으로
/// `/outputs/stage` 에 붙으므로, 붙은 연결이 있으면 '켜짐'. 없으면 Electron 처럼 항목을 내지 않는다(무대 화면은 선택).
pub fn check_stage(core: &Core) -> Option<Value> {
    use crate::i18n::{tr, tr_args};
    let n = core.outputs.values().filter(|c| c.client == "stage").count();
    if n == 0 {
        return None;
    }
    let windows = core.outputs.values().filter(|c| c.client == "stage" && c.kind == "window").count();
    let how = if windows > 0 { tr("무대 화면 창") } else { tr("브라우저") };
    Some(item("stagemon", &tr("무대 화면 모니터"), "ok", tr_args("켜짐 ({1} · 연결 {2}개)", &[&how, &n]), vec![]))
}

/// 업데이트 점검 (Electron diagnose.js:130-133 update): 확인 실패는 warn, 받아 둔 것은 info. 그 외에는 항목 없음.
pub fn check_update(up: &Value) -> Option<Value> {
    use crate::i18n::{tr, tr_args};
    match up["state"].as_str() {
        Some("error") => {
            let e = up["error"].as_str().filter(|x| !x.is_empty()).map(|e| format!(" — {e}")).unwrap_or_default();
            Some(item("update", &tr("업데이트"), "warn", tr_args("업데이트 확인에 실패했습니다{1}.", &[&e]), vec![tr("인터넷 연결을 확인하세요. 예배에는 지장이 없습니다."), tr("설정 → 정보 에서 다시 확인할 수 있습니다.")]))
        }
        Some("ready") | Some("downloaded") => Some(item("update", &tr("업데이트"), "info", tr_args("새 버전 {1} 을 받아 두었습니다 — 프로그램을 끌 때 설치됩니다.", &[&up["version"].as_str().unwrap_or("")]), vec![])),
        _ => None,
    }
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
            // 글자 경계가 아닐 수 있다 ("50%할인.png") — 글이 아니라 바이트로 본다
            if let Some(x) = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok()) {
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
    // 1-1. 무대 화면 모니터 (붙은 연결이 있을 때만)
    if let Some(x) = check_stage(core) {
        out.push(x);
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

    // 2-1. OBS 원격 조작 (obs-websocket) — 설정과 상태 캠시로만, OBS 에 직접 붙지 않는다
    if let Some(x) = check_obs_ws(&s, &d.obs_ws) {
        out.push(x);
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

    // 9. 업데이트
    if let Some(x) = check_update(&crate::updates::status(&s)) {
        out.push(x);
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
    /// 검토 1113-5 결함 1: obsws · stagemon · update 항목 (Electron diagnose.js:68-72, 82-88, 130-133)
    #[test]
    fn obsws_stage_update_items() {
        // OBS 원격 조작: 꺼져 있으면 항목 없음
        assert!(check_obs_ws(&json!({ "obs": { "enabled": false } }), &Value::Null).is_none());
        assert!(check_obs_ws(&json!({}), &Value::Null).is_none());
        let on = json!({ "obs": { "enabled": true, "host": "192.168.0.7", "port": 4455 } });
        // 캠시 없음 → warn (OBS 에 직접 붙지 않고 확인을 안내)
        let x = check_obs_ws(&on, &Value::Null).unwrap();
        assert_eq!(x["id"], "obsws");
        assert_eq!(x["status"], "warn");
        assert!(x["detail"].as_str().unwrap().contains("192.168.0.7:4455"));
        assert!(x["fix"].to_string().contains("4455") && x["fix"].to_string().contains("OBS 28"));
        // 연결됨 → ok, Electron 과 같은 장면·방송·녹화 글
        let x = check_obs_ws(&on, &json!({ "connected": true, "scene": "설거", "streaming": true, "recording": false })).unwrap();
        assert_eq!(x["status"], "ok");
        assert_eq!(x["detail"], "연결됨 — 장면 '설거' · 방송 중 · 녹화 안 함");
        // 연결 실패 기록 → fail + 안내
        let x = check_obs_ws(&on, &json!({ "connected": false, "error": "비밀번호가 틀립니다" })).unwrap();
        assert_eq!(x["status"], "fail");
        assert_eq!(x["detail"], "비밀번호가 틀립니다");
        assert_eq!(x["fix"].as_array().unwrap().len(), 3);
        let x = check_obs_ws(&on, &json!({ "connected": false })).unwrap();
        assert_eq!(x["status"], "fail");
        // 포트 0 → fail
        assert_eq!(check_obs_ws(&json!({ "obs": { "enabled": true, "port": 0 } }), &Value::Null).unwrap()["status"], "fail");
        // 업데이트
        assert!(check_update(&json!({ "state": "idle" })).is_none());
        assert!(check_update(&json!({ "state": "latest" })).is_none());
        let u = check_update(&json!({ "state": "error", "error": "오프라인" })).unwrap();
        assert_eq!(u["id"], "update");
        assert_eq!(u["status"], "warn");
        assert_eq!(u["detail"], "업데이트 확인에 실패했습니다 — 오프라인.");
        let u = check_update(&json!({ "state": "ready", "version": "1.2.3" })).unwrap();
        assert_eq!(u["status"], "info");
        assert!(u["detail"].as_str().unwrap().contains("1.2.3"));
        // 무대 화면: 붙은 연결이 없으면 항목 없음, 있으면 ok
        let dir = std::env::temp_dir().join(format!("wp-diag-stage-{}-{}", crate::util::now_ms(), rand::random::<u32>()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("bible.db"), b"").unwrap();
        let mut c = Core::open(dir.clone(), dir.clone(), None).unwrap();
        assert!(check_stage(&c).is_none());
        c.outputs.insert(1, crate::core::OutputClient { client: "stage".into(), kind: "window".into(), connected_at: 0, ack: None });
        let x = check_stage(&c).unwrap();
        assert_eq!(x["id"], "stagemon");
        assert_eq!(x["status"], "ok");
        assert!(x["detail"].as_str().unwrap().contains("무대 화면 창"));
        // 전체 점검에도 들어가고, obs 가 꺼져 있으면 obsws 는 없다
        let r = diagnose(&mut c, &Desktop::default());
        let ids: Vec<&str> = r["items"].as_array().unwrap().iter().filter_map(|x| x["id"].as_str()).collect();
        assert!(ids.contains(&"stagemon") && !ids.contains(&"obsws"));
        c.settings.update(|d| d["obs"] = json!({ "enabled": true }));
        let r = diagnose(&mut c, &Desktop { obs_ws: json!({ "connected": true }), ..Default::default() });
        let ob = r["items"].as_array().unwrap().iter().find(|x| x["id"] == "obsws").unwrap().clone();
        assert_eq!(ob["status"], "ok");
        assert!(ob["detail"].as_str().unwrap().contains("127.0.0.1:4455"));
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

#[cfg(test)]
mod audit_e_boundary_tests {
    #[test]
    fn percent_decode_survives_hangul_after_percent() {
        assert_eq!(super::percent_decode("50%할인.png"), "50%할인.png");
        assert_eq!(super::percent_decode("a%20b+c"), "a b c");
    }
}
