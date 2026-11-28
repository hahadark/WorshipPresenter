//! 조작 화면 그림 메모리 줄이기.
//!
//! egui 기본 그림 읽기는 파일 내용 + 풀어낸 그림(원본 크기) + GPU 텍스처를 프로그램이 끝날 때까지 모두 쥔다.
//! 찬송가 슬라이드(1920×1080)는 장당 약 20MB 라, 예배 한 번(슬라이드 100장)에 전용 메모리가 470MB 늘었다.
//!
//! 여기서는
//! 1. `wpimg://<최대 픽셀>/<파일 경로>` 주소를 읽는 그림 읽기를 따로 둔다: 백그라운드 스레드에서 풀고,
//!    쓰이는 크기(썸네일 512 · 미리보기 1920)에 맞춰 줄여서 올린다. 원래 크기는 source_size 로 그대로 알려 주므로
//!    비율·자리 계산은 바뀌지 않는다.
//! 2. 텍스처로 올린 뒤에는 풀어낸 그림을 버린다 (egui reduce_texture_memory).
//! 3. 텍스처 합이 상한을 넘으면 오래 안 쓴 것부터 버린다 (LRU). 이번 프레임에 그린 것은 버리지 않는다.

use eframe::egui::{self, load::{ImageLoadResult, ImageLoader, ImagePoll, LoadError, SizeHint}, ColorImage};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// 썸네일·작은 그림용 최대 변 (px)
pub const SMALL: u32 = 512;
/// 보통 크기의 미리보기·LIVE (창 1500 폭, 배율 150% 에서 약 960px)
pub const MEDIUM: u32 = 1024;
/// 크게 띄운 미리보기·LIVE 용 최대 변 (px). 조작 화면의 미리보기는 4K 모니터에서도 이보다 작다
pub const LARGE: u32 = 1920;

/// 텍스처 상한 (MB). WP_IMG_CACHE_MB 로 바꿀 수 있다 (점검용)
fn cap_bytes() -> usize {
    static CAP: Lazy<usize> = Lazy::new(|| std::env::var("WP_IMG_CACHE_MB").ok().and_then(|v| v.parse::<usize>().ok()).unwrap_or(128).max(16) * 1024 * 1024);
    *CAP
}

enum Entry {
    Pending,
    Ready(Arc<ColorImage>),
    /// 오류 글, 실패한 때 (조금 뒤 다시 읽어 본다 — 만드는 중이던 파일일 수 있다)
    Failed(String, web_time::Instant),
}

struct Job {
    ctx: egui::Context,
    uri: String,
    path: String,
    max: u32,
}

pub struct WpImgLoader {
    state: Arc<Mutex<HashMap<String, Entry>>>,
    tx: Mutex<std::sync::mpsc::Sender<Job>>,
}

/// 쓰임 기록: 주소 → (마지막으로 그린 프레임, 텍스처 크기 어림 바이트)
static USED: Lazy<Mutex<HashMap<String, (u64, usize)>>> = Lazy::new(Default::default);
/// 풀어낸 그림의 크기 (텍스처 바이트 어림에 씀)
static SIZES: Lazy<Mutex<HashMap<String, usize>>> = Lazy::new(Default::default);
static FRAME: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// 크기를 모르는 그림(아직 읽는 중 · svg · gif)의 어림값
const UNKNOWN_BYTES: usize = 2 * 1024 * 1024;

impl WpImgLoader {
    fn new() -> Self {
        let state: Arc<Mutex<HashMap<String, Entry>>> = Default::default();
        let (tx, rx) = std::sync::mpsc::channel::<Job>();
        let rx = Arc::new(Mutex::new(rx));
        // 그림 풀기는 일꾼 3개가 차례로 (격자에 그림 수백 장이 한꺼번에 보여도 스레드가 수백 개 생기지 않게)
        for i in 0..3 {
            let rx = rx.clone();
            let state = state.clone();
            let _ = std::thread::Builder::new().name(format!("img-decode-{i}")).spawn(move || loop {
                let job = match rx.lock().recv() {
                    Ok(j) => j,
                    Err(_) => break,
                };
                // 기다리는 동안 버려진 것(화면에서 사라짐)은 풀지 않는다
                if !matches!(state.lock().get(&job.uri), Some(Entry::Pending)) {
                    continue;
                }
                let r = std::panic::catch_unwind(|| decode(Path::new(&job.path), job.max)).unwrap_or_else(|_| Err("그림을 푸는 중 오류".into()));
                let mut st = state.lock();
                if matches!(st.get(&job.uri), Some(Entry::Pending)) {
                    match r {
                        Ok(img) => {
                            SIZES.lock().insert(job.uri.clone(), img.pixels.len() * 4 * 4 / 3); // 밉맵 포함 어림
                            st.insert(job.uri.clone(), Entry::Ready(Arc::new(img)));
                        }
                        Err(e) => {
                            st.insert(job.uri.clone(), Entry::Failed(e, web_time::Instant::now()));
                        }
                    }
                }
                drop(st);
                job.ctx.request_repaint();
            });
        }
        WpImgLoader { state, tx: Mutex::new(tx) }
    }
}

/// 파일을 읽어 풀고, 긴 변이 max 를 넘으면 줄인다. source_size 는 원래 크기
fn decode(path: &Path, max: u32) -> Result<ColorImage, String> {
    let rd = image::ImageReader::open(path).map_err(|e| format!("파일을 열 수 없습니다: {e}"))?.with_guessed_format().map_err(|e| e.to_string())?;
    let img = rd.decode().map_err(|e| format!("그림 형식을 읽을 수 없습니다: {e}"))?;
    let (ow, oh) = (img.width(), img.height());
    if ow == 0 || oh == 0 {
        return Err("빈 그림".into());
    }
    let img = if ow > max || oh > max {
        // 넓이 평균으로 줄인다 (thumbnail). resize(Triangle) 는 줄일 때마다 f32 중간 그림(1920×1080 → 17MB)을 만들어
        // 일꾼 셋이 한꺼번에 돌면 수십 MB 가 잠깐씩 잡혔다. 줄이기만 하므로 화질 차이는 눈에 띄지 않는다.
        img.thumbnail(max, max)
    } else {
        img
    };
    let rgba = img.to_rgba8();
    let mut out = ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw());
    out.source_size = egui::Vec2::new(ow as f32, oh as f32);
    Ok(out)
}

impl ImageLoader for WpImgLoader {
    fn id(&self) -> &str {
        "wp_app::imgcache::WpImgLoader"
    }

    fn load(&self, ctx: &egui::Context, uri: &str, _: SizeHint) -> ImageLoadResult {
        let Some(rest) = uri.strip_prefix("wpimg://") else { return Err(LoadError::NotSupported) };
        let Some((max, path)) = rest.split_once('/') else { return Err(LoadError::NotSupported) };
        let max = max.parse::<u32>().unwrap_or(LARGE).clamp(64, 8192);
        let mut st = self.state.lock();
        match st.get(uri) {
            Some(Entry::Ready(image)) => Ok(ImagePoll::Ready { image: image.clone() }),
            Some(Entry::Failed(e, at)) if at.elapsed().as_secs() < 3 => Err(LoadError::Loading(e.clone())),
            Some(Entry::Pending) => Ok(ImagePoll::Pending { size: None }),
            None | Some(Entry::Failed(..)) => {
                st.insert(uri.to_string(), Entry::Pending);
                drop(st);
                if self.tx.lock().send(Job { ctx: ctx.clone(), uri: uri.to_string(), path: path.to_string(), max }).is_err() {
                    self.state.lock().insert(uri.to_string(), Entry::Failed("그림 읽기 일꾼이 멈췄습니다".into(), web_time::Instant::now()));
                }
                Ok(ImagePoll::Pending { size: None })
            }
        }
    }

    fn forget(&self, uri: &str) {
        // 텍스처로 올린 뒤(reduce_texture_memory) 또는 LRU 로 버릴 때. 실패 기록은 남겨 둔다(매 프레임 다시 읽지 않게)
        let mut st = self.state.lock();
        if matches!(st.get(uri), Some(Entry::Ready(_)) | Some(Entry::Pending)) {
            st.remove(uri);
        }
    }

    fn forget_all(&self) {
        self.state.lock().clear();
    }

    fn byte_size(&self) -> usize {
        self.state.lock().values().map(|e| if let Entry::Ready(i) = e { i.pixels.len() * 4 } else { 0 }).sum()
    }

    fn has_pending(&self) -> bool {
        self.state.lock().values().any(|e| matches!(e, Entry::Pending))
    }
}

static LOADER: Lazy<Arc<WpImgLoader>> = Lazy::new(|| Arc::new(WpImgLoader::new()));

/// 시작할 때 한 번 (egui_extras::install_image_loaders 뒤에)
pub fn install(ctx: &egui::Context) {
    ctx.add_image_loader(LOADER.clone());
    // 텍스처로 올린 뒤 파일 내용·풀어낸 그림을 버린다 (같은 그림을 세 벌 쥐지 않게)
    ctx.options_mut(|o| o.reduce_texture_memory = true);
}


/// 우리 그림 읽기가 맡는 형식인지 (움직이는 gif·webp 와 svg 는 egui 기본에 맡긴다)
fn ours(path: &str) -> bool {
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "bmp")
}

/// 파일 경로 → egui 가 읽을 주소. max_px 는 그릴 크기(물리 픽셀)의 긴 변
pub fn uri_for(path: &str, need_px: f32) -> String {
    let p = path.replace('\\', "/");
    let uri = if ours(&p) {
        format!("wpimg://{}/{}", if need_px <= SMALL as f32 { SMALL } else if need_px <= MEDIUM as f32 { MEDIUM } else { LARGE }, p)
    } else {
        format!("file:///{}", p.trim_start_matches('/'))
    };
    touch(&uri);
    uri
}

/// 이번 프레임에 이 그림을 썼다
pub fn touch(uri: &str) {
    let f = FRAME.load(std::sync::atomic::Ordering::Relaxed);
    let mut u = USED.lock();
    match u.get_mut(uri) {
        Some(e) => e.0 = f,
        None => {
            u.insert(uri.to_string(), (f, UNKNOWN_BYTES));
        }
    }
}

/// 상한을 넘으면 버릴 것 고르기: 프레임 f 에서, 이번·바로 앞 프레임에 그린 것은 남기고 오래된 것부터.
/// 상한의 3/4 까지 줄인다 (매 프레임 하나씩 지우지 않게). 고른 것은 used 에서 뺀다.
fn pick_evict(used: &mut HashMap<String, (u64, usize)>, f: u64, cap: usize) -> Vec<String> {
    let mut total: usize = used.values().map(|v| v.1).sum();
    if total <= cap {
        return vec![];
    }
    let mut old: Vec<(String, u64, usize)> = used.iter().filter(|(_, v)| v.0 + 1 < f).map(|(k, v)| (k.clone(), v.0, v.1)).collect();
    old.sort_by_key(|x| x.1);
    let mut out = vec![];
    for (k, _, b) in old {
        if total <= cap * 3 / 4 {
            break;
        }
        used.remove(&k);
        total = total.saturating_sub(b);
        out.push(k);
    }
    out
}

/// 프레임 끝에: 텍스처 합이 상한을 넘으면 오래 안 쓴 것부터 버린다. 버린 수를 돌려준다
pub fn end_frame(ctx: &egui::Context) -> usize {
    let f = FRAME.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let gone = {
        let mut u = USED.lock();
        let mut sizes = SIZES.lock();
        for (k, v) in u.iter_mut() {
            if let Some(b) = sizes.get(k) {
                v.1 = *b;
            }
        }
        let gone = pick_evict(&mut u, f, cap_bytes());
        for k in &gone {
            sizes.remove(k);
        }
        gone
    };
    for k in &gone {
        ctx.forget_image(k);
    }
    gone.len()
}

/// 점검용: (기록된 그림 수, 텍스처 어림 MB)
pub fn stats() -> (usize, f32) {
    let u = USED.lock();
    (u.len(), u.values().map(|v| v.1).sum::<usize>() as f32 / 1048576.0)
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_buckets_and_formats() {
        assert_eq!(uri_for(r"C:\a\b.png", 200.0), "wpimg://512/C:/a/b.png");
        assert_eq!(uri_for(r"C:\a\b.JPG", 900.0), "wpimg://1024/C:/a/b.JPG");
        assert_eq!(uri_for(r"C:\a\b.jpeg", 1500.0), "wpimg://1920/C:/a/b.jpeg");
        assert_eq!(uri_for(r"C:\a\b.svg", 200.0), "file:///C:/a/b.svg");
        assert_eq!(uri_for(r"C:\가나\찬송 1.gif", 200.0), "file:///C:/가나/찬송 1.gif");
    }

    #[test]
    fn decode_downsizes_but_keeps_source_size() {
        let dir = std::env::temp_dir().join(format!("wp-imgcache-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("big.png");
        image::RgbaImage::from_pixel(3000, 1500, image::Rgba([10, 20, 30, 255])).save(&p).unwrap();
        let small = decode(&p, SMALL).unwrap();
        assert_eq!(small.size, [512, 256]);
        assert_eq!(small.source_size, egui::Vec2::new(3000.0, 1500.0));
        let large = decode(&p, LARGE).unwrap();
        assert_eq!(large.size, [1920, 960]);
        // 깨진 파일 · 없는 파일은 오류 글 (panic 아님)
        std::fs::write(dir.join("bad.png"), b"not an image").unwrap();
        assert!(decode(&dir.join("bad.png"), SMALL).is_err());
        assert!(decode(&dir.join("none.png"), SMALL).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn lru_evicts_old_but_not_current() {
        let mut used: HashMap<String, (u64, usize)> = HashMap::new();
        for i in 0..6u64 {
            used.insert(format!("{i}.png"), (i, 30));
        }
        // 지금 프레임 7: 5.png(프레임 5)까지 모두 오래됨. 합 180 > 상한 100 → 75 이하가 될 때까지 오래된 것부터
        let gone = pick_evict(&mut used, 7, 100);
        assert_eq!(gone, vec!["0.png", "1.png", "2.png", "3.png"]);
        assert_eq!(used.len(), 2);
        // 상한 안이면 아무것도 버리지 않는다
        assert!(pick_evict(&mut used, 9, 100).is_empty());
        // 이번·바로 앞 프레임에 그린 것은 상한을 넘어도 남긴다 (화면에 보이는 것)
        let mut vis: HashMap<String, (u64, usize)> = (0..5u64).map(|i| (format!("v{i}"), (10, 50))).collect();
        vis.insert("old".into(), (3, 50));
        assert_eq!(pick_evict(&mut vis, 11, 100), vec!["old"]);
        assert_eq!(vis.len(), 5);
    }
}