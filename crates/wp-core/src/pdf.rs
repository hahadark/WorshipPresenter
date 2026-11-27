//! PDF 보여 주기 (예배송출의 'PDF 보여 주기' 를 참고해 새로 씀). 큰 PDF 라이브러리를 넣지 않고 운영체제에 든 것을 쓴다:
//! - Windows 10/11: Windows.Data.Pdf (WinRT, PowerShell 5.1 에서 부름) — 쪽마다 PNG
//! - macOS: PDFKit (osascript JavaScript) — 쪽마다 PNG
//! 그림은 자료 폴더 `pdf/<이름>-<지문>/page-001.png` 에 두고(같은 파일이면 다시 만들지 않음), 큐시트에는 슬라이드쇼 순서
//! (자동 넘김 없음)로 넣는다 → 미리보기 · 송출 · 썸네일은 그림 슬라이드와 같다.
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// 너무 큰 PDF 는 거절 (예배 화면용: 쪽 수 · 크기)
pub const MAX_PAGES: usize = 300;
const MAX_BYTES: u64 = 300 * 1024 * 1024;
/// 그림 가로 (1080p 화면을 채우는 정도)
const WIDTH: u32 = 1920;

#[cfg(windows)]
const PS_PDF: &str = r#"
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Runtime.WindowsRuntime
$ext = [System.WindowsRuntimeSystemExtensions].GetMethods()
$asOp = ($ext | Where-Object { $_.Name -eq 'AsTask' -and $_.GetParameters().Count -eq 1 -and $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperation`1' })[0]
$asAct = ($ext | Where-Object { $_.Name -eq 'AsTask' -and $_.GetParameters().Count -eq 1 -and $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncAction' })[0]
function Await($op, [Type]$t) { $task = $asOp.MakeGenericMethod($t).Invoke($null, @($op)); $task.Wait() | Out-Null; $task.Result }
function AwaitAct($a) { $asAct.Invoke($null, @($a)).Wait() | Out-Null }
[Windows.Storage.StorageFile, Windows.Storage, ContentType = WindowsRuntime] | Out-Null
[Windows.Storage.StorageFolder, Windows.Storage, ContentType = WindowsRuntime] | Out-Null
[Windows.Data.Pdf.PdfDocument, Windows.Data.Pdf, ContentType = WindowsRuntime] | Out-Null
[Windows.Data.Pdf.PdfPageRenderOptions, Windows.Data.Pdf, ContentType = WindowsRuntime] | Out-Null
$file = Await ([Windows.Storage.StorageFile]::GetFileFromPathAsync($env:WP_PDF_IN)) ([Windows.Storage.StorageFile])
$doc = Await ([Windows.Data.Pdf.PdfDocument]::LoadFromFileAsync($file)) ([Windows.Data.Pdf.PdfDocument])
$n = [int]$doc.PageCount
if ($n -gt [int]$env:WP_PDF_MAX) { Write-Output "TOO_MANY $n"; exit }
$folder = Await ([Windows.Storage.StorageFolder]::GetFolderFromPathAsync($env:WP_PDF_OUT)) ([Windows.Storage.StorageFolder])
for ($i = 0; $i -lt $n; $i++) {
  $page = $doc.GetPage([uint32]$i)
  $opt = New-Object Windows.Data.Pdf.PdfPageRenderOptions
  $w = [double]$page.Size.Width; $h = [double]$page.Size.Height
  $W = [uint32]$env:WP_PDF_W
  if ($w -gt 0 -and $h -gt 0 -and ($W / $w * $h) -gt 2160) { $W = [uint32](2160 / $h * $w) }
  $opt.DestinationWidth = $W
  $name = 'page-{0:D3}.png' -f ($i + 1)
  $f = Await ($folder.CreateFileAsync($name, [Windows.Storage.CreationCollisionOption]::ReplaceExisting)) ([Windows.Storage.StorageFile])
  $s = Await ($f.OpenAsync([Windows.Storage.FileAccessMode]::ReadWrite)) ([Windows.Storage.Streams.IRandomAccessStream])
  AwaitAct ($page.RenderToStreamAsync($s, $opt))
  $s.Dispose(); $page.Dispose()
  Write-Output "PAGE $name"
}
Write-Output "DONE $n"
"#;

#[cfg(target_os = "macos")]
const JXA_PDF: &str = r#"
// 0 = kPDFDisplayBoxMediaBox, 4 = NSBitmapImageFileTypePNG (이름 대신 값: JXA 다리에 이름이 없을 수 있음)
ObjC.import('Foundation'); ObjC.import('AppKit'); ObjC.import('PDFKit');
function run(argv) {
  const doc = $.PDFDocument.alloc.initWithURL($.NSURL.fileURLWithPath(argv[0]));
  if (!doc || doc.isNil()) return 'ERR open';
  const n = doc.pageCount, max = parseInt(argv[3], 10);
  if (n > max) return 'TOO_MANY ' + n;
  const W = parseInt(argv[2], 10);
  for (let i = 0; i < n; i++) {
    const page = doc.pageAtIndex(i);
    const b = page.boundsForBox(0);
    const w = b.size.width, h = b.size.height;
    let tw = W, th = Math.round(W / w * h);
    if (th > 2160) { th = 2160; tw = Math.round(2160 / h * w); }
    const img = page.thumbnailOfSizeForBox($.NSMakeSize(tw, th), 0);
    const rep = $.NSBitmapImageRep.imageRepWithData(img.TIFFRepresentation);
    const png = rep.representationUsingTypeProperties(4, $.NSDictionary.dictionary);
    const name = 'page-' + String(i + 1).padStart(3, '0') + '.png';
    png.writeToFileAtomically(argv[1] + '/' + name, true);
  }
  return 'DONE ' + n;
}
"#;

/// 같은 PDF 를 다시 넣으면 그림을 다시 만들지 않게: 이름 + 크기 + 바꾼 시각
fn cache_dir(data_dir: &Path, pdf: &Path) -> Result<PathBuf> {
    let meta = std::fs::metadata(pdf).context("PDF 파일을 읽지 못했습니다")?;
    if meta.len() > MAX_BYTES {
        bail!("PDF 가 너무 큽니다 (300MB 까지).");
    }
    let mtime = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
    let mut h: u64 = 0xcbf2_9ce4_8422_2325; // FNV-1a
    for b in pdf.to_string_lossy().as_bytes().iter().chain(meta.len().to_le_bytes().iter()).chain(mtime.to_le_bytes().iter()) {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    let stem: String = pdf.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default().chars().filter(|c| c.is_alphanumeric() || " -_".contains(*c)).take(40).collect();
    Ok(data_dir.join("pdf").join(format!("{}-{:012x}", stem.trim(), h & 0xffff_ffff_ffff)))
}

fn pages_in(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("page-") && n.to_string_lossy().ends_with(".png"))).collect();
    v.sort();
    v
}

/// PDF → 쪽 그림들 (자료 폴더 안). 이미 만든 것이 있으면 그대로
pub fn render(data_dir: &Path, pdf: &Path) -> Result<Vec<PathBuf>> {
    let dir = cache_dir(data_dir, pdf)?;
    if dir.join("done").is_file() {
        let v = pages_in(&dir);
        if !v.is_empty() {
            return Ok(v);
        }
    }
    let tmp = dir.with_extension("tmp");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp)?;
    let r = convert(pdf, &tmp);
    let out = match r {
        Ok(s) => s,
        Err(e) => {
            let _ = std::fs::remove_dir_all(&tmp);
            return Err(e);
        }
    };
    if let Some(n) = out.lines().find_map(|l| l.strip_prefix("TOO_MANY ")) {
        let _ = std::fs::remove_dir_all(&tmp);
        bail!(crate::i18n::tr_args("쪽이 너무 많습니다 ({1}쪽, {2}쪽까지).", &[&n.trim(), &MAX_PAGES]));
    }
    if pages_in(&tmp).is_empty() {
        let _ = std::fs::remove_dir_all(&tmp);
        bail!("PDF 를 그림으로 바꾸지 못했습니다 (암호가 걸렸거나 손상된 PDF 일 수 있습니다).");
    }
    // Windows 는 화면 배율(예: 150%)만큼 크게 그린다 → 가로 WIDTH 로 줄인다 (메모리 · 디스크)
    for p in pages_in(&tmp) {
        if let Ok(img) = image::open(&p) {
            if img.width() > WIDTH + 8 {
                let h = (img.height() as f64 * WIDTH as f64 / img.width() as f64).round() as u32;
                let _ = img.resize_exact(WIDTH, h.max(1), image::imageops::FilterType::Triangle).save(&p);
            }
        }
    }
    std::fs::write(tmp.join("done"), b"")?;
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::rename(&tmp, &dir)?;
    log::info!("PDF → 그림 {}장: {}", pages_in(&dir).len(), dir.display());
    Ok(pages_in(&dir))
}

#[cfg(windows)]
fn convert(pdf: &Path, out: &Path) -> Result<String> {
    let abs = std::fs::canonicalize(pdf)?;
    // canonicalize 는 \\?\ 를 붙인다 — WinRT 는 그 꼴을 못 읽음
    let p = abs.to_string_lossy().trim_start_matches(r"\\?\").to_string();
    let o = std::fs::canonicalize(out)?.to_string_lossy().trim_start_matches(r"\\?\").to_string();
    crate::clippaste::run_ps(PS_PDF, &[("WP_PDF_IN", &p), ("WP_PDF_OUT", &o), ("WP_PDF_W", &WIDTH.to_string()), ("WP_PDF_MAX", &MAX_PAGES.to_string())]).map_err(|e| anyhow::anyhow!("PDF 를 읽지 못했습니다: {e}"))
}

#[cfg(target_os = "macos")]
fn convert(pdf: &Path, out: &Path) -> Result<String> {
    let o = std::process::Command::new("osascript")
        .args(["-l", "JavaScript", "-e", JXA_PDF])
        .arg(pdf)
        .arg(out)
        .arg(WIDTH.to_string())
        .arg(MAX_PAGES.to_string())
        .output()
        .context("osascript 실행 실패")?;
    let s = String::from_utf8_lossy(&o.stdout).to_string();
    if !o.status.success() || s.starts_with("ERR") {
        bail!("PDF 를 읽지 못했습니다: {}", String::from_utf8_lossy(&o.stderr).lines().next().unwrap_or(s.trim()));
    }
    Ok(s)
}

#[cfg(not(any(windows, target_os = "macos")))]
fn convert(_pdf: &Path, _out: &Path) -> Result<String> {
    bail!("이 운영체제에서는 PDF 를 열 수 없습니다.")
}

/// 쪽 그림들 → 큐시트 순서 (슬라이드쇼, 자동 넘김 없음)
pub fn item(title: &str, pages: &[PathBuf]) -> Value {
    let mut it = crate::core::new_item("slideshow", title);
    it["images"] = json!(pages.iter().map(|p| crate::library::file_url(p)).collect::<Vec<_>>());
    it["body"] = json!("");
    it["autoSec"] = json!(0);
    it["autoLoop"] = json!(false);
    it["countdown"] = json!("");
    it["pdf"] = json!(true);
    it
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 시험용 PDF: 두 쪽 (글자 없이 색 상자) — 손으로 쓴 최소 PDF
    pub(crate) fn tiny_pdf() -> Vec<u8> {
        let objs = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 320 180] /Contents 4 0 R >>".to_string(),
            "<< /Length 31 >>\nstream\n0.2 0.4 0.8 rg 0 0 320 180 re f\nendstream".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 320 180] /Contents 6 0 R >>".to_string(),
            "<< /Length 31 >>\nstream\n0.9 0.3 0.1 rg 0 0 160 180 re f\nendstream".to_string(),
        ];
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offs = vec![];
        for (i, o) in objs.iter().enumerate() {
            offs.push(out.len());
            out.extend(format!("{} 0 obj\n{o}\nendobj\n", i + 1).bytes());
        }
        let xref = out.len();
        out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).bytes());
        for o in offs {
            out.extend(format!("{o:010} 00000 n \n").bytes());
        }
        out.extend(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).bytes());
        out
    }

    #[test]
    fn item_is_manual_slideshow() {
        let it = item("주보", &[PathBuf::from("C:/d/pdf/a/page-001.png")]);
        assert_eq!((it["type"].as_str(), it["autoSec"].as_i64(), it["images"][0].as_str().map(|s| s.starts_with("/lib/file?p="))), (Some("slideshow"), Some(0), Some(true)));
    }

    /// 실제 변환 (Windows 는 Windows.Data.Pdf, 맥은 PDFKit — 맥 CI 에서 돈다). 같은 파일은 두 번째에 다시 만들지 않는다
    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn renders_pages_and_caches() {
        let d = std::env::temp_dir().join(format!("wp-pdf-{}-{}", std::process::id(), crate::util::short_id("t")));
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("시험 주보.pdf");
        std::fs::write(&f, tiny_pdf()).unwrap();
        let pages = render(&d, &f).unwrap();
        assert_eq!(pages.len(), 2);
        let img = image::open(&pages[1]).unwrap().to_rgb8();
        assert_eq!(img.width(), WIDTH);
        let left = img.get_pixel(10, img.height() / 2);
        let right = img.get_pixel(img.width() - 10, img.height() / 2);
        assert!(left[0] > 200 && right[0] > 240 && right[2] > 240, "왼쪽 주황 · 오른쪽 흰 바탕: {left:?} {right:?}");
        let t0 = std::fs::metadata(&pages[0]).unwrap().modified().unwrap();
        let again = render(&d, &f).unwrap();
        assert_eq!(std::fs::metadata(&again[0]).unwrap().modified().unwrap(), t0, "다시 만들지 않음");
        // 망가진 PDF
        let bad = d.join("bad.pdf");
        std::fs::write(&bad, b"%PDF-1.4 garbage").unwrap();
        assert!(render(&d, &bad).is_err());
        assert!(!d.join("pdf").read_dir().unwrap().flatten().any(|e| e.path().extension().is_some_and(|x| x == "tmp")), "실패한 임시 폴더는 지운다");
        let _ = std::fs::remove_dir_all(d);
    }
}
