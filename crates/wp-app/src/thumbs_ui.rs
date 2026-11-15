//! [트랙 B] 라이브러리 그림·영상 썸네일 격자 (Electron library.js 의 lib-grid 처럼)
//! 썸네일은 백그라운드 스레드 하나가 차례로 만들어 데이터 폴더 thumbs\ 에 캐시하고, 다 되면 화면을 다시 그린다.
//! 화면에 보이는 칸만 요청한다 (ScrollArea::show_rows).

use crate::app::App;
use crate::theme::pal;
use crate::widgets::{self, HoverOp};
use eframe::egui::{self, Color32, RichText, Sense, Stroke, Vec2};
use wp_core::i18n::tr;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

#[derive(Default)]
struct Thumbs {
    queue: VecDeque<PathBuf>,
    queued: HashSet<PathBuf>,
    ready: HashMap<PathBuf, PathBuf>,
    failed: HashMap<PathBuf, PathBuf>,
    worker: bool,
}

static TH: Lazy<Mutex<Thumbs>> = Lazy::new(|| Mutex::new(Thumbs::default()));

/// 썸네일 자리 (아직이면 만들 차례에 넣고 None)
pub fn get(ctx: &egui::Context, data_dir: &Path, src: &Path) -> Option<PathBuf> {
    let mut t = TH.lock();
    let expected = wp_core::thumbs::thumb_path(data_dir, src)?;
    if let Some(p) = t.ready.get(src) {
        if *p == expected && p.exists() {
            return Some(p.clone());
        }
        t.ready.remove(src);
    }
    if t.failed.get(src) == Some(&expected) {
        return None;
    }
    t.failed.remove(src);
    if let Some(p) = wp_core::thumbs::cached(data_dir, src) {
        t.ready.insert(src.to_path_buf(), p.clone());
        return Some(p);
    }
    if t.queued.insert(src.to_path_buf()) {
        // 새로 보인 것을 먼저 (스크롤하면 지금 화면부터)
        t.queue.push_front(src.to_path_buf());
    }
    if !t.worker {
        t.worker = true;
        let (ctx, dir) = (ctx.clone(), data_dir.to_path_buf());
        std::thread::Builder::new()
            .name("thumbs".into())
            .spawn(move || loop {
                let next = TH.lock().queue.pop_front();
                let Some(src) = next else {
                    std::thread::sleep(std::time::Duration::from_millis(150));
                    continue;
                };
                let r = wp_core::thumbs::make(&dir, &src);
                let mut t = TH.lock();
                t.queued.remove(&src);
                match r {
                    Ok(p) => {
                        t.ready.insert(src, p);
                    }
                    Err(e) => {
                        log::warn!("썸네일 실패 {}: {e}", src.display());
                        if let Some(expected) = wp_core::thumbs::thumb_path(&dir, &src) {
                            t.failed.insert(src, expected);
                        }
                    }
                }
                drop(t);
                ctx.request_repaint();
            })
            .ok();
    }
    None
}

/// 지금 만들 차례에 남은 수 (점검용)
pub fn pending() -> usize {
    TH.lock().queue.len()
}

fn file_uri(p: &Path) -> String {
    format!("file:///{}", p.to_string_lossy().replace('\\', "/").trim_start_matches('/'))
}

/// 그림 하나를 바로 송출할 덱 (큐시트 연결 없음, slides.rs 그림 슬라이드와 같은 모양)
pub fn image_deck(f: &Value) -> wp_core::presentation::Deck {
    wp_core::presentation::Deck {
        passage: json!({"title":f["name"],"itemTitle":f["name"],"itemType":"image","cueIndex":-1,"cueId":""}),
        slides: vec![json!({"kind":"image","image":f["url"],"ref":f["name"],"label":"이미지","parts":[]})],
        index: 0,
    }
}

const CELL_W: f32 = 168.0;
const IMG_H: f32 = CELL_W * 9.0 / 16.0;
const ROW_H: f32 = IMG_H + 58.0;

impl App {
    /// 그림·영상 격자: 썸네일 · 이름 · [순서로] [배경] (LIVE 배경이면 빨간 테두리)
    pub(crate) fn lib_grid(&mut self, ui: &mut egui::Ui, files: &[Value]) {
        let ctx = ui.ctx().clone();
        let p = pal();
        let live_url = self.core.lock().layers.media.as_ref().and_then(|m| m["url"].as_str().map(String::from)).unwrap_or_default();
        let per_row = ((ui.available_width() + 8.0) / (CELL_W + 8.0)).floor().max(1.0) as usize;
        let files: Vec<&Value> = files.iter().take(1000).collect();
        let rows = files.len().div_ceil(per_row);
        let mut act: Option<(&'static str, Value)> = None;
        egui::ScrollArea::vertical().id_salt("libgrid").auto_shrink([false, false]).show_rows(ui, ROW_H, rows, |ui, range| {
            for r in range {
                ui.horizontal(|ui| {
                    for f in files.iter().skip(r * per_row).take(per_row) {
                        let path = PathBuf::from(f["path"].as_str().unwrap_or(""));
                        let url = f["url"].as_str().unwrap_or("").to_string();
                        let video = f["video"] == true;
                        let on = !live_url.is_empty() && live_url == url;
                        ui.vertical(|ui| {
                            ui.set_width(CELL_W);
                            let (rect, resp) = ui.allocate_exact_size(Vec2::new(CELL_W, IMG_H), Sense::click());
                            let hot = ui.rect_contains_pointer(rect);
                            crate::widgets::mark(&format!("lib-cell-{}", f["name"].as_str().unwrap_or("")), rect);
                            if resp.hovered() { ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand); }
                            ui.painter().rect_filled(rect, 4.0, Color32::BLACK);
                            match get(&ctx, &self.data_dir, &path) {
                                Some(t) => {
                                    // paint_at은 지정 사각형에 늘려 그리므로 실제 크기로 비율을 맞춘다.
                                    let image = egui::Image::new(file_uri(&t));
                                    if let Ok(poll) = image.load_for_size(ui.ctx(), rect.size()) {
                                        if let Some(size) = poll.size().filter(|s| s.x > 0.0 && s.y > 0.0) {
                                            let area = rect.shrink(1.0);
                                            let scale = (area.width() / size.x).min(area.height() / size.y);
                                            image.paint_at(ui, egui::Rect::from_center_size(area.center(), size * scale));
                                        }
                                    }
                                }
                                None => {
                                    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, if video { tr("영상") } else { "…".into() }, egui::FontId::proportional(14.0), p.muted);
                                }
                            }
                            if video {
                                crate::icons::paint(ui, egui::Rect::from_min_size(rect.left_bottom() + Vec2::new(6.0, -22.0), Vec2::splat(16.0)), "film", Color32::WHITE);
                            }
                            ui.painter().rect_stroke(rect, 4.0, if on { Stroke::new(2.5, p.live) } else if hot { Stroke::new(2.0, p.accent) } else { Stroke::new(1.0, p.line) }, egui::StrokeKind::Inside);
                            // 마우스 올림 단추 (docs/ux-rules.md 3절): 큐시트에 넣기 · 송출 배경으로(끄기) · 바로 송출
                            let ops = [
                                HoverOp::new("cue", "file-plus", tr("큐시트에 넣기 (고른 순서 다음, 두 번 눌러도)")),
                                if on { HoverOp::new("bgoff", "monitor-off", tr("송출 배경 끄기")).danger() } else { HoverOp::new("bg", "layers", tr("지금 송출 화면의 배경으로 바로 (슬라이드와 따로)")) },
                                HoverOp::new("send", "play", tr(if video { "이 영상 바로 송출 (큐시트에 넣지 않고)" } else { "이 그림 바로 송출 (큐시트에 넣지 않고)" })),
                            ];
                            if let Some(op) = widgets::hover_ops(ui, egui::Id::new(("lib-ops", url.as_str())), rect, &ops) {
                                act = Some((op, (*f).clone()));
                            }
                            let name = f["name"].as_str().unwrap_or("").to_string();
                            if crate::widgets::click2(ui, ("lib-thumb", url.as_str()), &resp) == crate::widgets::Click::Double {
                                act = Some(("cue", (*f).clone()));
                            }
                            ui.add(egui::Label::new(RichText::new(if on { format!("● {name}") } else { name.clone() }).small().color(if on { p.danger_text } else { p.text2 })).truncate()).on_hover_text(f["path"].as_str().unwrap_or(""));
                            ui.horizontal(|ui| {
                                if ui.small_button(wp_core::i18n::tr("순서로")).on_hover_text(wp_core::i18n::tr("고른 순서 다음에 넣기 (두 번 눌러도)")).clicked() {
                                    act = Some(("cue", (*f).clone()));
                                }
                                let bl = if on { wp_core::i18n::tr("배경 끄기") } else { wp_core::i18n::tr("배경으로") };
                                if ui.small_button(bl).on_hover_text(wp_core::i18n::tr("지금 송출 화면의 배경으로 바로 (슬라이드와 따로)")).clicked() {
                                    act = Some((if on { "bgoff" } else { "bg" }, (*f).clone()));
                                }
                            });
                        });
                    }
                });
                ui.add_space(4.0);
            }
        });
        if files.is_empty() {
            ui.label(RichText::new(wp_core::i18n::tr("찾는 그림·영상이 없습니다.")).color(Color32::GRAY));
        }
        if let Some((what, f)) = act {
            let url = f["url"].as_str().unwrap_or("").to_string();
            let name = f["name"].as_str().unwrap_or("").to_string();
            let video = f["video"] == true;
            match what {
                "cue" => {
                    let it = if video { json!({ "type": "video", "title": name, "file": url, "sound": true, "volume": 1.0, "loop": false, "endAction": "hold" }) } else { json!({ "type": "image", "title": name, "image": url }) };
                    let mut x = wp_core::core::new_item(it["type"].as_str().unwrap(), &name);
                    wp_core::settings::merge_into(&mut x, &it);
                    self.add_lib_item(x);
                }
                "bg" => self.core.lock().set_background_media(Some(json!({ "type": if video { "video" } else { "image" }, "url": url, "label": name, "fit": "cover" }))),
                "send" => {
                    // 바로 송출: 큐시트에 넣지 않고 이 그림·영상 한 장 (영상은 media_ui::video_deck 과 같은 모양)
                    let deck = if video { crate::media_ui::video_deck(&f) } else { image_deck(&f) };
                    self.core.lock().send_deck(deck);
                }
                _ => self.core.lock().set_background_media(None),
            }
        }
    }

    pub(crate) fn add_lib_item(&mut self, it: Value) {
        // 넣고 곧바로 미리보기(바로 송출이면 송출) · 같은 것이 이미 있으면 그 순서를 고름 · 연타 무시 (wp_core::pick)
        let at = self.sel.map(|s| s + 1);
        let r = self.core.lock().add_picked_item(it, at, false);
        crate::app::picked_sel(&mut self.sel, &mut self.edit, r);
    }
}
