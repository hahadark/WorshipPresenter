//! PC 안 음성 인식 (설교 성경 구절 따라가기 · 방송 실시간 자막 · 예배 후 녹음 받아 적기)
//!
//! whisper.cpp (MIT, ggml-org/whisper.cpp) 의 `whisper-server` 를 따로 띄우고 127.0.0.1 로 WAV 를 보낸다. 음성은 PC 밖으로 나가지 않는다.
//! - 설치판에는 넣지 않는다. 처음 켤 때 받는 곳·크기·라이선스를 보여 주고 동의를 받은 뒤 받는다.
//! - 받는 파일은 주소와 SHA-256 을 여기 박아 두고, 받은 뒤 대조해 다르면 지우고 쓰지 않는다.
//! - Windows: 미리 빌드된 CPU 판(whisper-bin-x64.zip)에서 서버와 DLL 만 꺼낸다.
//! - 맥: 미리 빌드된 판이 없어 Homebrew `brew install whisper-cpp` 의 whisper-server 가 있으면 그것을 쓴다(없으면 안내).
//! - 모델: ggml small(q5_1, 190MB) 이 한국어에 쓸 만하고, 코어가 적은 PC 는 base(q5_1, 60MB).

use anyhow::{anyhow, bail, Context, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

/// 받을 파일 하나
#[derive(Debug, Clone, Copy)]
pub struct Asset {
    pub id: &'static str,
    pub name: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    pub size: u64,
    pub license: &'static str,
    pub home: &'static str,
}

/// whisper.cpp b5907 Windows x64 CPU 판 (서버 · DLL)
pub const ENGINE_WIN: Asset = Asset {
    id: "engine",
    name: "whisper.cpp b5907 (Windows x64)",
    url: "https://github.com/ggml-org/whisper.cpp/releases/download/b5907/whisper-bin-x64.zip",
    sha256: "ecca07b277d943e9b9f4712856a144b857271a6b333104e511e1988e80db582b",
    size: 9_168_428,
    license: "MIT",
    home: "https://github.com/ggml-org/whisper.cpp",
};

pub const MODELS: [Asset; 2] = [
    Asset {
        id: "small",
        name: "Whisper small (q5_1) — 한국어 권장",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small-q5_1.bin",
        sha256: "ae85e4a935d7a567bd102fe55afc16bb595bdb618e11b2fc7591bc08120411bb",
        size: 190_085_487,
        license: "MIT (OpenAI Whisper 모델)",
        home: "https://huggingface.co/ggerganov/whisper.cpp",
    },
    Asset {
        id: "base",
        name: "Whisper base (q5_1) — 느린 PC 용, 정확도 낮음",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base-q5_1.bin",
        sha256: "422f1ae452ade6f30a004d7e5c6a43195e4433bc370bf23fac9cc591f01a8898",
        size: 59_707_625,
        license: "MIT (OpenAI Whisper 모델)",
        home: "https://huggingface.co/ggerganov/whisper.cpp",
    },
];

pub fn model(id: &str) -> &'static Asset {
    MODELS.iter().find(|m| m.id == id).unwrap_or(&MODELS[0])
}

/// 코어 수로 고른 기본 모델 (GPU 판을 쓰지 않으므로 CPU 로 정한다): 논리 코어 8개 이상 small, 아니면 base
pub fn default_model() -> &'static str {
    if std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4) >= 8 { "small" } else { "base" }
}

/// 인식에 쓸 스레드: 방송 PC 부담을 줄이려고 코어의 절반, 많아도 4
pub fn threads() -> usize {
    (std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4) / 2).clamp(1, 4)
}

pub fn dir(data_dir: &Path) -> PathBuf {
    data_dir.join("stt")
}
pub fn model_path(data_dir: &Path, id: &str) -> PathBuf {
    dir(data_dir).join(format!("ggml-{}-q5_1.bin", model(id).id))
}

/// 서버 실행 파일 (없으면 None)
pub fn engine_path(data_dir: &Path) -> Option<PathBuf> {
    if cfg!(windows) {
        let p = dir(data_dir).join("bin").join("whisper-server.exe");
        return p.is_file().then_some(p);
    }
    ["/opt/homebrew/bin/whisper-server", "/usr/local/bin/whisper-server", "/usr/bin/whisper-server"].iter().map(PathBuf::from).find(|p| p.is_file())
}

/// 이 운영체제에서 엔진을 받아 올 수 있나 (맥은 Homebrew 로 설치)
pub const CAN_DOWNLOAD_ENGINE: bool = cfg!(all(windows, target_arch = "x86_64"));

pub fn sha256_file(p: &Path) -> Result<String> {
    let mut f = std::fs::File::open(p)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// 받기: .part 로 받고 → 크기·SHA-256 대조 → 맞으면 이름 바꾸기. 다르면 지우고 오류.
/// progress(받은 바이트, 전체), cancel() 이 true 면 멈춤
pub fn download(a: &Asset, dest: &Path, progress: &dyn Fn(u64, u64), cancel: &dyn Fn() -> bool) -> Result<()> {
    if dest.is_file() && sha256_file(dest).ok().as_deref() == Some(a.sha256) {
        return Ok(());
    }
    std::fs::create_dir_all(dest.parent().unwrap())?;
    let part = dest.with_extension("part");
    let mut resp = ureq::get(a.url).call().map_err(|e| anyhow!("{} 받기 실패: {e}", a.name))?;
    let mut rd = resp.body_mut().with_config().limit(a.size + 1024).reader();
    let mut f = std::fs::File::create(&part)?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    let mut got = 0u64;
    loop {
        if cancel() {
            drop(f);
            let _ = std::fs::remove_file(&part);
            bail!("받기를 멈췄습니다.");
        }
        let n = rd.read(&mut buf)?;
        if n == 0 {
            break;
        }
        f.write_all(&buf[..n])?;
        h.update(&buf[..n]);
        got += n as u64;
        progress(got, a.size);
    }
    drop(f);
    let sum: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if got != a.size || sum != a.sha256 {
        let _ = std::fs::remove_file(&part);
        bail!("{} 의 크기·SHA-256 이 맞지 않아 쓰지 않습니다 (받은 {got} 바이트). 다시 받아 보세요.", a.name);
    }
    std::fs::rename(&part, dest)?;
    Ok(())
}

/// Windows 엔진 받기 → zip 에서 서버 · whisper/ggml DLL 만 꺼냄 (SDL2 · llama · parakeet 은 쓰지 않음)
pub fn install_engine(data_dir: &Path, progress: &dyn Fn(u64, u64), cancel: &dyn Fn() -> bool) -> Result<PathBuf> {
    if !CAN_DOWNLOAD_ENGINE {
        bail!("이 컴퓨터에서는 음성 인식 엔진을 받아 올 수 없습니다. 맥은 터미널에서 'brew install whisper-cpp' 로 설치하세요.");
    }
    let zip_path = dir(data_dir).join("whisper-bin-x64.zip");
    download(&ENGINE_WIN, &zip_path, progress, cancel)?;
    let bin = dir(data_dir).join("bin");
    std::fs::create_dir_all(&bin)?;
    let mut z = zip::ZipArchive::new(std::fs::File::open(&zip_path)?).context("엔진 압축 파일")?;
    for i in 0..z.len() {
        let mut e = z.by_index(i)?;
        let name = e.name().unwrap_or_default().rsplit('/').next().unwrap_or("").to_string();
        if !keep_engine_file(&name) {
            continue;
        }
        let mut out = std::fs::File::create(bin.join(&name))?;
        std::io::copy(&mut e, &mut out)?;
    }
    let _ = std::fs::remove_file(&zip_path);
    // 함께 받은 라이선스 안내
    let _ = std::fs::write(bin.join("LICENSE-whisper.cpp.txt"), include_str!("../data/LICENSE-whisper.cpp.txt"));
    engine_path(data_dir).ok_or_else(|| anyhow!("엔진 압축 파일에 whisper-server.exe 가 없습니다."))
}

pub fn keep_engine_file(name: &str) -> bool {
    let n = name.to_lowercase();
    n == "whisper-server.exe" || n == "whisper.dll" || (n.starts_with("ggml") && n.ends_with(".dll"))
}

/// 설교 성경 이름을 잘 받아 적도록 주는 첫 프롬프트 (whisper initial prompt)
pub fn bible_prompt() -> String {
    "설교 말씀. 창세기, 출애굽기, 시편 23편, 잠언, 이사야, 마태복음, 마가복음, 누가복음, 요한복음 3장 16절, 사도행전, 로마서 8장 28절, 고린도전서, 갈라디아서, 에베소서, 빌립보서, 히브리서, 야고보서, 베드로전서, 요한일서, 요한계시록.".into()
}

// ---------------- 서버 ----------------
pub struct Server {
    child: Child,
    pub port: u16,
    #[cfg(windows)]
    _job: Option<isize>,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0").and_then(|l| l.local_addr()).map(|a| a.port()).unwrap_or(38991)
}

/// 조작 화면이 꺼지면 서버도 꺼지게 (Windows 작업 개체)
#[cfg(windows)]
fn job_for(child: &Child) -> Option<isize> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::JobObjects::*;
    unsafe {
        let h = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if h.is_null() {
            return None;
        }
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        SetInformationJobObject(h, JobObjectExtendedLimitInformation, &info as *const _ as _, std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32);
        AssignProcessToJobObject(h, child.as_raw_handle() as _);
        Some(h as isize)
    }
}

impl Server {
    /// 서버를 띄우고 모델이 다 읽힐 때까지 기다린다 (최대 60초)
    pub fn start(exe: &Path, model: &Path, lang: &str, threads: usize) -> Result<Server> {
        if !model.is_file() {
            bail!("음성 인식 모델이 없습니다. 먼저 받으세요.");
        }
        let port = free_port();
        let mut cmd = Command::new(exe);
        cmd.args(["-m", &model.to_string_lossy(), "-l", lang, "-t", &threads.to_string(), "--host", "127.0.0.1", "--port", &port.to_string(), "-nt", "-nf", "-ac", "768"]);
        // -nf: 온도 되풀이를 끄면 가끔 30초 넘게 걸리던 조각이 없어진다 · -ac 768: 조각이 6초 이하라 소리 문맥을 15초로 줄여 빠르게
        cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        if let Some(d) = exe.parent() {
            cmd.current_dir(d);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // 창 없이
        }
        let child = cmd.spawn().with_context(|| format!("음성 인식 엔진을 켜지 못했습니다 ({})", exe.display()))?;
        #[cfg(windows)]
        let job = job_for(&child);
        let mut s = Server {
            child,
            port,
            #[cfg(windows)]
            _job: job,
        };
        let t0 = std::time::Instant::now();
        loop {
            if let Ok(Some(st)) = s.child.try_wait() {
                bail!("음성 인식 엔진이 바로 꺼졌습니다 ({st}).");
            }
            if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
                break;
            }
            if t0.elapsed().as_secs() > 60 {
                bail!("음성 인식 엔진이 60초 안에 준비되지 않았습니다.");
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        Ok(s)
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    pub fn alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// WAV 한 조각 → 글 (prompt: 앞 문맥·성경 이름)
    pub fn transcribe(&self, wav: &[u8], prompt: &str) -> Result<String> {
        transcribe_at(self.port, wav, prompt)
    }
}

/// multipart 본문 (ureq 3 에는 없어서 직접)
pub fn multipart(boundary: &str, fields: &[(&str, &str)], wav: &[u8]) -> Vec<u8> {
    let mut b = Vec::with_capacity(wav.len() + 1024);
    for (k, v) in fields {
        b.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{k}\"\r\n\r\n{v}\r\n").as_bytes());
    }
    b.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"a.wav\"\r\nContent-Type: audio/wav\r\n\r\n").as_bytes());
    b.extend_from_slice(wav);
    b.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    b
}

pub fn transcribe_at(port: u16, wav: &[u8], prompt: &str) -> Result<String> {
    let boundary = format!("wp{}", crate::util::short_id("b"));
    let body = multipart(&boundary, &[("response_format", "json"), ("temperature", "0.0"), ("prompt", prompt)], wav);
    let mut r = ureq::post(&format!("http://127.0.0.1:{port}/inference"))
        .header("Content-Type", &format!("multipart/form-data; boundary={boundary}"))
        .config()
        .timeout_global(Some(std::time::Duration::from_secs(60)))
        .build()
        .send(&body[..])
        .map_err(|e| anyhow!("음성 인식 요청 실패: {e}"))?;
    let v: Value = r.body_mut().read_json().map_err(|e| anyhow!("음성 인식 응답: {e}"))?;
    Ok(clean_text(v["text"].as_str().unwrap_or("")))
}

/// 받아 적은 글 정리: 줄바꿈·공백 · whisper 가 조용할 때 지어내는 꼬리('시청해 주셔서 감사합니다' 등)
pub fn clean_text(t: &str) -> String {
    let s = t.split_whitespace().collect::<Vec<_>>().join(" ");
    const NOISE: [&str; 6] = ["시청해 주셔서 감사합니다", "구독과 좋아요", "MBC 뉴스", "자막 제공", "[음악]", "(음악)"];
    if NOISE.iter().any(|n| s.contains(n)) && s.chars().count() < 30 {
        return String::new();
    }
    s
}

/// 16kHz 모노 f32 → WAV(16bit PCM)
pub fn wav_bytes(samples: &[f32], rate: u32) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut b = Vec::with_capacity(44 + data_len as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        b.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    b
}

/// 소리 세기 (RMS)
pub fn rms(s: &[f32]) -> f32 {
    if s.is_empty() {
        return 0.0;
    }
    (s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32).sqrt()
}

/// 마이크 소리를 조각으로 자른다: 말이 끝난 쉼(0.5초 조용) 또는 최대 길이에서. 16kHz 모노 기준
pub struct Chunker {
    buf: Vec<f32>,
    silent: usize,
    pub max_sec: f32,
    pub min_sec: f32,
    pub threshold: f32,
}

impl Default for Chunker {
    fn default() -> Self {
        Chunker { buf: vec![], silent: 0, max_sec: 6.0, min_sec: 1.2, threshold: 0.01 }
    }
}

impl Chunker {
    /// 새 소리를 넣고, 자를 때가 되면 조각을 돌려준다 (조용하기만 한 조각은 버림)
    pub fn push(&mut self, s: &[f32]) -> Option<Vec<f32>> {
        const RATE: usize = 16000;
        for frame in s.chunks(RATE / 50) {
            if rms(frame) < self.threshold {
                self.silent += frame.len();
            } else {
                self.silent = 0;
            }
        }
        self.buf.extend_from_slice(s);
        let len = self.buf.len();
        let cut = (len as f32 >= self.max_sec * RATE as f32) || (len as f32 >= self.min_sec * RATE as f32 && self.silent >= RATE / 2);
        if !cut {
            // 오래 조용하면 앞을 버린다 (쌓이지 않게)
            if self.silent >= len && len > RATE * 2 {
                self.buf.clear();
            }
            return None;
        }
        let out = std::mem::take(&mut self.buf);
        self.silent = 0;
        (rms(&out) >= self.threshold * 0.8).then_some(out)
    }
}

/// 받아 적은 글을 남겨 두는 곳 (예배 후 정리의 재료): <자료>/stt/transcript-YYYY-MM-DD.txt  'HH:MM:SS<TAB>글'
pub fn transcript_path(data_dir: &Path, day: &str) -> PathBuf {
    dir(data_dir).join(format!("transcript-{day}.txt"))
}

pub fn append_transcript(data_dir: &Path, day: &str, hms: &str, text: &str) {
    if text.trim().is_empty() {
        return;
    }
    let p = transcript_path(data_dir, day);
    let _ = std::fs::create_dir_all(p.parent().unwrap());
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
        let _ = writeln!(f, "{hms}\t{}", text.replace(['\n', '\t'], " "));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn assets_are_pinned() {
        for a in std::iter::once(&ENGINE_WIN).chain(MODELS.iter()) {
            assert!(a.url.starts_with("https://github.com/ggml-org/") || a.url.starts_with("https://huggingface.co/ggerganov/"), "{}", a.url);
            assert_eq!(a.sha256.len(), 64);
            assert!(a.size > 1_000_000);
        }
        assert!(keep_engine_file("whisper-server.exe") && keep_engine_file("ggml-cpu-haswell.dll") && keep_engine_file("whisper.dll"));
        assert!(!keep_engine_file("SDL2.dll") && !keep_engine_file("llama.dll") && !keep_engine_file("whisper-cli.exe"));
    }
    #[test]
    fn wrong_hash_is_refused() {
        // 가짜 주소 대신: 이미 있는 파일이 해시가 맞으면 받지 않는다 · 다르면 받으려 한다 (주소가 없으니 실패)
        let dir = std::env::temp_dir().join(format!("wp-stt-{}", crate::util::short_id("t")));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("x.bin");
        std::fs::write(&f, b"hello").unwrap();
        let a = Asset { id: "x", name: "x", url: "http://127.0.0.1:9/none", sha256: "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824", size: 5, license: "", home: "" };
        assert!(download(&a, &f, &|_, _| {}, &|| false).is_ok(), "해시가 맞으면 그대로");
        let b = Asset { sha256: "00", ..a };
        assert!(download(&b, &f, &|_, _| {}, &|| false).is_err());
        assert_eq!(sha256_file(&f).unwrap(), a.sha256);
    }
    #[test]
    fn wav_and_multipart() {
        let w = wav_bytes(&[0.0, 0.5, -0.5], 16000);
        assert_eq!(&w[..4], b"RIFF");
        assert_eq!(w.len(), 44 + 6);
        let m = multipart("B", &[("prompt", "요한복음")], &w);
        let t = String::from_utf8_lossy(&m);
        assert!(t.contains("name=\"prompt\"\r\n\r\n요한복음") && t.ends_with("--B--\r\n"));
    }
    #[test]
    fn chunker_cuts_on_pause() {
        let mut c = Chunker::default();
        let tone: Vec<f32> = (0..16000 * 2).map(|i| (i as f32 * 0.05).sin() * 0.3).collect();
        assert!(c.push(&tone).is_none(), "말하는 중");
        let quiet = vec![0.0f32; 16000];
        let out = c.push(&quiet).expect("쉼에서 자름");
        assert_eq!(out.len(), 16000 * 3);
        assert!(c.push(&vec![0.0; 16000 * 3]).is_none(), "조용하기만 한 것은 버림");
        let long: Vec<f32> = (0..16000 * 7).map(|i| (i as f32 * 0.05).sin() * 0.3).collect();
        assert!(c.push(&long).is_some(), "최대 길이");
    }
    #[test]
    fn noise_tails_dropped() {
        assert_eq!(clean_text(" 시청해 주셔서 감사합니다.\n"), "");
        assert_eq!(clean_text(" 로마서 8장\n 28절 "), "로마서 8장 28절");
    }
}

/// 지금 지역 날짜·시각 ("2026-11-29", "11:42:05")
pub fn local_day_hms() -> (String, String) {
    let ms = crate::util::now_ms() + crate::localtime::local_offset_ms();
    let days = ms.div_euclid(86_400_000);
    let sec = ms.rem_euclid(86_400_000) / 1000;
    // civil_from_days (Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (format!("{y:04}-{m:02}-{d:02}"), format!("{:02}:{:02}:{:02}", sec / 3600, sec % 3600 / 60, sec % 60))
}

/// 실제 음성 인식 점검 (기본 건너뜀): WP_STT_BENCH=<wav 폴더> · WP_STT_DIR=<엔진·모델 둘 곳> · WP_STT_MODEL=small|base
/// 폴더의 lines.txt(정답 장절|글)와 같은 차례의 d01.wav… / s01.wav… 를 실시간 속도로 넣고 인식률 · 늦음 · CPU 를 적는다.
/// cargo test -p wp-core --release -- --ignored stt_bench --nocapture
#[cfg(test)]
mod bench {
    use super::*;
    fn cpu_secs(pid: u32) -> f64 {
        let o = std::process::Command::new("powershell").args(["-NoProfile", "-Command", &format!("(Get-Process -Id {pid}).CPU")]).output().ok();
        o.and_then(|o| String::from_utf8_lossy(&o.stdout).trim().replace(',', ".").parse().ok()).unwrap_or(0.0)
    }
    fn read_wav(p: &Path) -> Vec<f32> {
        let b = std::fs::read(p).unwrap();
        let i = b.windows(4).position(|w| w == b"data").unwrap() + 8;
        b[i..].chunks(2).map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0).collect()
    }
    #[test]
    #[ignore]
    fn stt_bench() {
        let Ok(dir) = std::env::var("WP_STT_BENCH") else { return };
        let dir = PathBuf::from(dir);
        let data = PathBuf::from(std::env::var("WP_STT_DIR").unwrap());
        let model_id = std::env::var("WP_STT_MODEL").unwrap_or_else(|_| "small".into());
        let t = std::time::Instant::now();
        let exe = engine_path(&data).map(Ok).unwrap_or_else(|| install_engine(&data, &|_, _| {}, &|| false)).unwrap();
        download(model(&model_id), &model_path(&data, &model_id), &|_, _| {}, &|| false).unwrap();
        println!("준비(받기·대조) {:.1}초", t.elapsed().as_secs_f32());
        let t = std::time::Instant::now();
        let s = Server::start(&exe, &model_path(&data, &model_id), "ko", threads()).unwrap();
        println!("엔진 켜기 {:.1}초 · 스레드 {} · 논리 코어 {}", t.elapsed().as_secs_f32(), threads(), std::thread::available_parallelism().unwrap().get());
        // d·s = 고친 규칙을 맞춘 묶음, h·k = 처음 보는 묶음 (d·h 는 음성 합성이 수를 고유어로 읽음, s·k 는 '십오 장' 처럼 한자어로)
        for (prefix, list) in [("d", "lines.txt"), ("s", "lines.txt"), ("h", "lines2.txt"), ("k", "lines2.txt")] {
            let Ok(t) = std::fs::read_to_string(dir.join(list)) else { continue };
            let truth: Vec<String> = t.lines().filter(|l| !l.trim().is_empty()).map(|l| l.split('|').next().unwrap().to_string()).collect();
            let (mut ok, mut audio, mut took_sum, mut worst) = (0, 0.0f32, 0.0f32, 0.0f32);
            let cpu0 = cpu_secs(s.pid());
            let wall = std::time::Instant::now();
            for (k, want) in truth.iter().enumerate() {
                let f = dir.join(format!("{prefix}{:02}.wav", k + 1));
                let pcm = read_wav(&f);
                let secs = pcm.len() as f32 / 16000.0;
                // 실시간처럼: 말이 끝난 뒤에 인식을 시작 → 늦음 = 인식에 걸린 시간 (+ 쉼 0.5초)
                let t0 = std::time::Instant::now();
                let text = s.transcribe(&wav_bytes(&pcm, 16000), &bible_prompt()).unwrap_or_default();
                let took = t0.elapsed().as_secs_f32();
                let refs = crate::refscan::find_refs_spoken(&text);
                let hit = refs.iter().any(|r| r.reference == *want);
                ok += usize::from(hit);
                println!("{prefix}{:02} {} {:.1}s/{:.1}s  {}  → {:?}", k + 1, if hit { "O" } else { "X" }, took, secs, text, refs.iter().map(|r| r.reference.clone()).collect::<Vec<_>>());
                audio += secs;
                took_sum += took;
                worst = worst.max(took);
                // 남은 시간은 쉰다 (실제 말 속도)
                if took < secs {
                    std::thread::sleep(std::time::Duration::from_secs_f32(secs - took));
                }
            }
            let cpu = cpu_secs(s.pid()) - cpu0;
            let w = wall.elapsed().as_secs_f64();
            let cores = std::thread::available_parallelism().unwrap().get() as f64;
            println!("== {prefix} 모델 {model_id}: 인식 {ok}/{} ({:.0}%) · 소리 {audio:.0}초 · 인식 평균 {:.2}초 최대 {worst:.2}초 · 엔진 CPU {cpu:.1}초 / {w:.0}초 = PC 전체의 {:.1}% (코어 하나 기준 {:.0}%)",
                truth.len(), ok as f32 * 100.0 / truth.len() as f32, took_sum / truth.len() as f32, cpu / w / cores * 100.0, cpu / w * 100.0);
        }
        let mem = std::process::Command::new("powershell").args(["-NoProfile", "-Command", &format!("[int]((Get-Process -Id {}).WorkingSet64/1MB)", s.pid())]).output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default();
        println!("엔진 메모리 {mem} MB");
    }
}
