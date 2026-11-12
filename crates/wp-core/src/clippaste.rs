//! [트랙 A] 클립보드 붙여 넣기 → 큐시트의 새 순서 (Electron 판 clippaste.js)
//!  - 글: 장절 → 성경 · 가사 모양 → 찬양 · 그 밖 → 공지 (plan_text)
//!  - PowerPoint 에서 복사한 슬라이드 → <데이터>/pasted/clip-날짜-시각.pptx (grab_slides)
//!      ① 윈도우 .NET 으로 클립보드의 슬라이드 묶음(zip, 뿌리 clipboard/)을 꺼내 보통 .pptx 로 ② 안 되면 PowerPoint 에 붙여 넣어 저장
//!  - 그림: 윈도우 .NET 으로 클립보드 그림 → <데이터>/media/clip-….png (grab_image)

use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

static MARK: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)^\s*[\[(（<]?\s*(\d+\s*절|후렴|브릿지|브리지|간주|전주|엔딩|verse\s*\d*|chorus\s*\d*|bridge|pre-?chorus|intro|outro|ending|tag)\s*[\])）>]?\s*[:.]?\s*$").unwrap());
static ROLE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)^\s*(인도자|사회자|회중|다\s*같이|다함께|leader|people|all)\s*[:：]").unwrap());

fn short(s: &str, n: usize) -> String {
    if s.chars().count() > n { format!("{}…", s.chars().take(n - 1).collect::<String>().trim_end()) } else { s.to_string() }
}

/// 붙여 넣은 글 정리
pub fn clean_text(raw: &str) -> String {
    let s = raw.replace("\r\n", "\n").replace('\r', "\n").replace(['\u{a0}', '\u{3000}'], " ").replace(['\u{200b}', '\u{feff}'], "");
    let s: String = s.lines().map(|l| l.trim_end_matches([' ', '\t'])).collect::<Vec<_>>().join("\n");
    s.trim_matches('\n').chars().take(20000).collect()
}

/// 가사 모양인가
pub fn looks_like_lyrics(lines: &[&str]) -> bool {
    if lines.iter().any(|l| ROLE.is_match(l)) {
        return false;
    }
    if lines.iter().any(|l| MARK.is_match(l)) {
        return true;
    }
    if lines.len() < 4 || lines.iter().any(|l| l.chars().count() > 36) {
        return false;
    }
    let frac = |f: &dyn Fn(&str) -> bool| lines.iter().filter(|l| f(l)).count() as f64 / lines.len() as f64;
    frac(&|l| l.trim_end().ends_with(['.', '。', '!', '?'])) <= 0.1 && frac(&|l| l.chars().any(|c| c.is_ascii_digit())) <= 0.2 && frac(&|l| l.contains([':', '：'])) <= 0.1
}

/// 붙여 넣은 글 → 넣을 순서들 [{ type, extra }]. as_: '' | bible | song | text | title
pub fn plan_text(raw: &str, as_: &str, is_ref: &dyn Fn(&str) -> bool) -> Option<(String, Vec<Value>)> {
    let text = clean_text(raw);
    if text.trim().is_empty() {
        return None;
    }
    let lines: Vec<&str> = text.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
    let first = lines[0];
    let ref_lines = lines.len() <= 8 && lines.iter().all(|l| l.chars().count() <= 60 && l.chars().any(|c| c.is_ascii_digit()) && is_ref(l));
    let kind = if ["bible", "song", "text", "title"].contains(&as_) { as_.to_string() } else if ref_lines { "bible".into() } else if looks_like_lyrics(&lines) { "song".into() } else { "text".into() };
    match kind.as_str() {
        "bible" => {
            let refs: Vec<&str> = lines.iter().filter(|l| l.chars().count() <= 60 && is_ref(l)).take(8).copied().collect();
            if refs.is_empty() {
                return plan_text(&text, "text", is_ref);
            }
            Some((kind, refs.iter().map(|r| json!({ "type": "bible", "extra": { "title": "성경 봉독", "ref": r } })).collect()))
        }
        "song" => {
            let blocks: Vec<&str> = Regex::new(r"\n[ \t]*\n+").unwrap().split(&text).collect();
            let head = blocks[0].trim();
            let alone = blocks.len() > 1 && !head.contains('\n') && !MARK.is_match(head) && head.chars().count() <= 40;
            let title = if alone { head.to_string() } else { short(lines.iter().find(|l| !MARK.is_match(l)).unwrap_or(&first), 30) };
            let lyrics = if alone { blocks[1..].join("\n\n").trim().to_string() } else { text.clone() };
            Some((kind, vec![json!({ "type": "song", "extra": { "title": title, "lyrics": lyrics } })]))
        }
        "title" => {
            let mut e = json!({ "title": short(first, 60) });
            if lines.len() > 1 {
                e["subtitle"] = json!(short(&lines[1..].join(" "), 120));
            }
            Some((kind, vec![json!({ "type": "title", "extra": e })]))
        }
        _ => {
            if lines.len() == 1 {
                let e = if first.chars().count() <= 60 { json!({ "title": first, "body": "" }) } else { json!({ "body": text }) };
                return Some((kind, vec![json!({ "type": "text", "extra": e })]));
            }
            let rest = text[text.find('\n').map(|i| i + 1).unwrap_or(text.len())..].trim_start_matches('\n').to_string();
            let headed = first.chars().count() <= 40 && !first.ends_with(['.', '。', '!', '?', ',']) && !ROLE.is_match(first);
            let e = if headed { json!({ "title": first, "body": rest }) } else { json!({ "body": text }) };
            Some((kind, vec![json!({ "type": "text", "extra": e })]))
        }
    }
}

// ---------------- 슬라이드 묶음 → .pptx ----------------
fn zip_names(buf: &[u8]) -> Option<Vec<String>> {
    let z = zip::ZipArchive::new(std::io::Cursor::new(buf)).ok()?;
    Some(z.file_names().filter_map(|n| n.ok().map(|n| n.to_string())).collect())
}

/// .pptx 모양인가 (zip 에 presentation.xml · 슬라이드)
pub fn looks_like_pptx(buf: &[u8]) -> bool {
    if buf.len() < 200 || &buf[..2] != b"PK" {
        return false;
    }
    let Some(names) = zip_names(buf) else { return false };
    let re = Regex::new(r"^ppt/slides/slide\d+\.xml$").unwrap();
    names.iter().any(|n| n == "[Content_Types].xml") && names.iter().any(|n| n == "ppt/presentation.xml") && names.iter().any(|n| re.is_match(n))
}

/// PowerPoint 'Slides Package' (뿌리 clipboard/, 매크로 종류) → 보통 .pptx (이미 .pptx 면 그대로, 아니면 None)
pub fn clip_to_pptx(buf: &[u8]) -> Option<Vec<u8>> {
    if looks_like_pptx(buf) {
        return Some(buf.to_vec());
    }
    if buf.len() < 200 || &buf[..2] != b"PK" {
        return None;
    }
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(buf)).ok()?;
    let names: Vec<String> = z.file_names().filter_map(|n| n.ok().map(|n| n.to_string())).collect();
    let re = Regex::new(r"^clipboard/slides/slide\d+\.xml$").unwrap();
    if !names.iter().any(|n| n == "clipboard/presentation.xml") || !names.iter().any(|n| re.is_match(n)) {
        return None;
    }
    let mut out = std::io::Cursor::new(Vec::new());
    {
        let mut w = zip::ZipWriter::new(&mut out);
        let opt = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for n in names.iter().filter(|n| !n.ends_with('/')) {
            let mut data = vec![];
            z.by_name(n).ok()?.read_to_end(&mut data).ok()?;
            if n == "[Content_Types].xml" || n == "_rels/.rels" {
                let s = String::from_utf8_lossy(&data)
                    .replace("PartName=\"/clipboard/", "PartName=\"/ppt/")
                    .replace("Target=\"/clipboard/", "Target=\"/ppt/")
                    .replace("Target=\"clipboard/", "Target=\"ppt/")
                    .replace("vnd.ms-powerpoint.presentation.macroEnabled.main+xml", "vnd.openxmlformats-officedocument.presentationml.presentation.main+xml")
                    .replace("presentationml.presentation.macroEnabled.main+xml", "presentationml.presentation.main+xml");
                data = s.into_bytes();
            }
            let name = n.strip_prefix("clipboard/").map(|r| format!("ppt/{r}")).unwrap_or(n.clone());
            w.start_file(name, opt).ok()?;
            w.write_all(&data).ok()?;
        }
        w.finish().ok()?;
    }
    let v = out.into_inner();
    looks_like_pptx(&v).then_some(v)
}

const PS_CLIP_DUMP: &str = r#"
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
$d = [System.Windows.Forms.Clipboard]::GetDataObject()
if (-not $d) { Write-Output 'NONE'; exit }
$all = @($d.GetFormats($false))
$f = @($all | Where-Object { $_ -match '^PowerPoint [0-9.]+ Slides Package$' }) + @($all | Where-Object { $_ -match '^PowerPoint [0-9.]+ Internal Slides$' }) | Select-Object -First 1
if (-not $f) { Write-Output 'NONE'; exit }
$s = $d.GetData($f, $false)
if ($s -is [System.IO.MemoryStream]) { $bytes = $s.ToArray() }
elseif ($s -is [byte[]]) { $bytes = $s }
else { Write-Output 'HAVE'; exit }
[System.IO.File]::WriteAllBytes($env:WC_CLIP_OUT, $bytes)
Write-Output ('OK ' + $bytes.Length)
"#;

const PS_CLIP_PASTE: &str = r#"
$ErrorActionPreference = 'Stop'
$pp = New-Object -ComObject PowerPoint.Application
$p = $null
try {
  $how = 'plain'
  try { $p = $pp.Presentations.Add(-1) } catch { $p = $pp.Presentations.Add(0) }
  try {
    $win = $p.Windows.Item(1)
    $win.Activate()
    $win.ViewType = 7
    $pp.CommandBars.ExecuteMso('PasteSourceFormatting')
    $last = -1
    for ($i = 0; $i -lt 50; $i++) {
      Start-Sleep -Milliseconds 100
      $c = $p.Slides.Count
      if ($c -gt 0 -and $c -eq $last) { break }
      if ($c -gt 0) { $last = $c }
    }
    if ($p.Slides.Count -gt 0) { $how = 'keep' }
  } catch { }
  if ($p.Slides.Count -eq 0) { [void]$p.Slides.Paste() }
  $n = $p.Slides.Count
  if ($n -eq 0) { throw 'no slides on the clipboard' }
  $p.SaveAs($env:WC_PPT_FILE, 24)
  Write-Output "OK $n $how"
} finally {
  if ($p) { try { $p.Saved = -1; $p.Close() } catch { } }
  if ($pp.Presentations.Count -eq 0) { $pp.Quit() }
}
"#;

const PS_CLIP_IMAGE: &str = r#"
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
$img = [System.Windows.Forms.Clipboard]::GetImage()
if (-not $img) { Write-Output 'NONE'; exit }
$img.Save($env:WC_CLIP_OUT, [System.Drawing.Imaging.ImageFormat]::Png)
Write-Output ('OK ' + $img.Width + ' ' + $img.Height)
"#;

const PS_CLIP_INFO: &str = r#"
Add-Type -AssemblyName System.Windows.Forms
$d = [System.Windows.Forms.Clipboard]::GetDataObject()
if (-not $d) { exit }
$f = @($d.GetFormats($false))
if ($f | Where-Object { $_ -match '^PowerPoint [0-9.]+ (Slides Package|Internal Slides)$' }) { Write-Output 'SLIDES' }
if ([System.Windows.Forms.Clipboard]::ContainsImage()) { Write-Output 'IMAGE' }
if ([System.Windows.Forms.Clipboard]::ContainsText()) { Write-Output 'TEXT' }
"#;

/// PowerShell (-STA: 클립보드는 STA 에서만). 환경 변수와 함께
pub fn run_ps(script: &str, env: &[(&str, &str)]) -> Result<String, String> {
    let enc: Vec<u8> = script.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    let b64 = crate::hymns::b64(&enc);
    let mut c = std::process::Command::new("powershell.exe");
    c.args(["-NoProfile", "-NonInteractive", "-STA", "-ExecutionPolicy", "Bypass", "-EncodedCommand", &b64]);
    for (k, v) in env {
        c.env(k, v);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000);
    }
    let o = c.output().map_err(|e| format!("PowerShell 실행 실패: {e}"))?;
    if !o.status.success() {
        return Err(String::from_utf8_lossy(&o.stderr).lines().find(|l| !l.trim().is_empty()).unwrap_or("PowerShell 실행 실패").to_string());
    }
    Ok(String::from_utf8_lossy(&o.stdout).to_string())
}

/// 클립보드에 무엇이 있나: (슬라이드, 그림, 글)
pub fn clipboard_info() -> (bool, bool, bool) {
    let o = run_ps(PS_CLIP_INFO, &[]).unwrap_or_default();
    (o.contains("SLIDES"), o.contains("IMAGE"), o.contains("TEXT"))
}

pub fn pasted_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("pasted")
}

fn stamp() -> String {
    // 지역 시각을 몰라도 겹치지 않으면 된다: UTC 시각
    let s = crate::util::iso_now();
    format!("{}{}{}-{}{}{}", &s[0..4], &s[5..7], &s[8..10], &s[11..13], &s[14..16], &s[17..19])
}

fn next_file(dir: &Path, base: &str, ext: &str) -> PathBuf {
    let mut k = 0;
    loop {
        let f = dir.join(format!("{base}{}.{ext}", if k > 0 { format!("-{}", k + 1) } else { String::new() }));
        if !f.exists() {
            return f;
        }
        k += 1;
    }
}

/// 클립보드의 PowerPoint 슬라이드 → .pptx 파일 (how: package | keep | plain). 오래 걸릴 수 있으니 다른 스레드에서
pub fn grab_slides(data_dir: &Path, force: bool) -> Result<(PathBuf, &'static str), String> {
    let dir = pasted_dir(data_dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let base = format!("clip-{}", stamp());
    let mut confirmed = force;
    let file = next_file(&dir, &base, "pptx");
    let out = run_ps(PS_CLIP_DUMP, &[("WC_CLIP_OUT", &file.to_string_lossy())]).unwrap_or_default();
    if out.contains("OK ") || out.contains("HAVE") {
        confirmed = true;
    }
    if out.contains("OK ") && file.exists() {
        if let Some(p) = std::fs::read(&file).ok().and_then(|b| clip_to_pptx(&b)) {
            std::fs::write(&file, p).map_err(|e| e.to_string())?;
            return Ok((file, "package"));
        }
        let _ = std::fs::remove_file(&file);
    }
    if !confirmed {
        return Err("클립보드에 PowerPoint 슬라이드가 없습니다. PowerPoint 의 슬라이드 목록에서 슬라이드를 골라 복사한 뒤 다시 붙여 넣으세요.".into());
    }
    if crate::hymns::has_powerpoint() {
        let file = next_file(&dir, &base, "pptx");
        let out = run_ps(PS_CLIP_PASTE, &[("WC_PPT_FILE", &file.to_string_lossy())]).unwrap_or_default();
        if let Some(m) = Regex::new(r"OK (\d+) (keep|plain)").unwrap().captures(&out) {
            if file.exists() {
                return Ok((file, if &m[2] == "keep" { "keep" } else { "plain" }));
            }
        }
    }
    Err("클립보드에서 PowerPoint 슬라이드를 가져오지 못했습니다. PowerPoint 에서 슬라이드를 다시 복사하거나, PPT 파일을 큐시트에 끌어다 놓으세요.".into())
}

/// 클립보드 그림 → <데이터>/media/clip-….png → '/media/…' 주소
pub fn grab_image(data_dir: &Path) -> Result<String, String> {
    let dir = data_dir.join("media");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file = next_file(&dir, &format!("clip-{}", stamp()), "png");
    let out = run_ps(PS_CLIP_IMAGE, &[("WC_CLIP_OUT", &file.to_string_lossy())])?;
    if !out.contains("OK") || !file.exists() {
        return Err("클립보드에 그림이 없습니다.".into());
    }
    Ok(format!("/media/{}", file.file_name().unwrap().to_string_lossy()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn plan() {
        let is_ref = |l: &str| crate::parser::looks_like_reference(l);
        let (k, it) = plan_text("요3:16\n롬8:28", "", &is_ref).unwrap();
        assert_eq!(k, "bible");
        assert_eq!(it.len(), 2);
        let (k, it) = plan_text("나의 찬양\n\n[1절]\n주님 사랑해요\n주님 경배해요\n\n[후렴]\n할렐루야", "", &is_ref).unwrap();
        assert_eq!(k, "song");
        assert_eq!(it[0]["extra"]["title"], "나의 찬양");
        let (k, it) = plan_text("이번 주 광고\n수요예배는 7시에 드립니다.", "", &is_ref).unwrap();
        assert_eq!(k, "text");
        assert_eq!(it[0]["extra"]["title"], "이번 주 광고");
        assert!(plan_text("  \n ", "", &is_ref).is_none());
    }
    #[test]
    fn package() {
        // clipboard/ 뿌리 묶음 → ppt/
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut w = zip::ZipWriter::new(&mut buf);
            let o = zip::write::SimpleFileOptions::default();
            for (n, d) in [("[Content_Types].xml", "<Types><Override PartName=\"/clipboard/presentation.xml\" ContentType=\"application/vnd.ms-powerpoint.presentation.macroEnabled.main+xml\"/></Types>"), ("_rels/.rels", "<Relationships><Relationship Target=\"clipboard/presentation.xml\"/></Relationships>"), ("clipboard/presentation.xml", "<p/>"), ("clipboard/slides/slide1.xml", &"<s/>".repeat(80))] {
                w.start_file(n, o).unwrap();
                w.write_all(d.as_bytes()).unwrap();
            }
            w.finish().unwrap();
        }
        let p = clip_to_pptx(&buf.into_inner()).unwrap();
        assert!(looks_like_pptx(&p));
    }
}
