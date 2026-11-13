//! [트랙 A] 기능 대조(PARITY.md)에서 빠진 작은 화면 기능들
//!  - 큐시트 순서 오른쪽 클릭 메뉴 (Electron ctxmenu.js 의 순서 메뉴)
//!  - 순서 넣기: 카메라 · 슬라이드쇼, 그 편집칸 (영상 편집칸도)
//!  - 도움말: 사용 설명서 · README · OBS 안내 · 단축키 보기 · 새 소식 · 문제 알리기

use crate::app::{Act, App};
use eframe::egui::{self, Color32, RichText};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde_json::{json, Value};

const REPO: &str = "https://github.com/hahadark/WorshipPresenter";

static KEYS_OPEN: Lazy<Mutex<bool>> = Lazy::new(|| Mutex::new(false));
static RENAME: Lazy<Mutex<(Option<usize>, String)>> = Lazy::new(|| Mutex::new((None, String::new())));

fn open_link(url: &str) {
    let _ = wp_core::trackd::open_link(url);
}

/// 큐시트 순서 한 줄의 오른쪽 클릭 메뉴
pub fn cue_row_menu(app: &mut App, r: &egui::Response, i: usize, it: &Value, n: usize, acts: &mut Vec<Act>) {
    r.context_menu(|ui| {
        ui.set_min_width(220.0);
        if ui.button(wp_core::i18n::tr("미리보기")).clicked() {
            app.sel = Some(i);
            app.edit = None;
            acts.push(Act::PreviewItem(i));
            ui.close();
        }
        if ui.button(wp_core::i18n::tr("▶ 송출")).clicked() {
            acts.push(Act::SendItem(i));
            ui.close();
        }
        if ui.button(wp_core::i18n::tr("편집칸에서 고치기")).clicked() {
            app.show_item_editor(i);
            ui.close();
        }
        // 제목 바꾸기 (메뉴 안에서 바로)
        {
            let mut rn = RENAME.lock();
            if rn.0 != Some(i) {
                *rn = (Some(i), it["title"].as_str().unwrap_or("").to_string());
            }
            ui.horizontal(|ui| {
                ui.label(wp_core::i18n::tr("제목"));
                let resp = ui.add(egui::TextEdit::singleline(&mut rn.1).desired_width(140.0));
                if (resp.lost_focus() && ui.input(|x| x.key_pressed(egui::Key::Enter))) || ui.small_button(wp_core::i18n::tr("바꾸기")).clicked() {
                    let mut x = it.clone();
                    x["title"] = json!(rn.1.trim());
                    acts.push(Act::UpdateItem(i, x));
                    ui.close();
                }
            });
        }
        // Electron처럼 진행 시계가 켜진 경우에만 예정 시간을 편집한다.
        let times_on = app.core.lock().settings.get()["cueTimes"] == true;
        if times_on {
            ui.menu_button(wp_core::i18n::tr("예정 시간…"), |ui| {
                crate::trackb_ui::duration_editor(app, ui, i, it);
            });
        }
        if ui.add_enabled(crate::hymns_ui::can_open_ppt_tools(it), egui::Button::new(wp_core::i18n::tr("여러 순서로 나누기…"))).clicked() {
            app.sel = Some(i);
            app.edit = None;
            crate::hymns_ui::open_ppt_tools(ui.ctx(), it);
            ui.close();
        }
        ui.separator();
        if it["type"] == "song" && ui.button(wp_core::i18n::tr("찬양 라이브러리에 저장")).clicked() {
            let s = json!({ "id": it["songId"].as_str().unwrap_or(""), "title": it["title"], "lyrics": it["lyrics"], "credit": it["credit"].as_str().unwrap_or(""), "arrangement": it["order"].as_str().unwrap_or("") });
            let mut c = app.core.lock();
            match c.save_song(&s) {
                Ok(saved) => {
                    let mut x = it.clone();
                    x["songId"] = saved["id"].clone();
                    drop(c);
                    acts.push(Act::UpdateItem(i, x));
                    app.core.lock().notify(wp_core::i18n::tr("찬양 라이브러리에 저장했습니다."), "ok");
                }
                Err(e) => c.notify(wp_core::i18n::tr_args("저장하지 못했습니다: {1}", &[&e]), "error"),
            }
            ui.close();
        }
        ui.horizontal(|ui| {
            if ui.add_enabled(i > 0, egui::Button::new(wp_core::i18n::tr("위로"))).clicked() {
                acts.push(Act::MoveItem(i, -1));
                ui.close();
            }
            if ui.add_enabled(i + 1 < n, egui::Button::new(wp_core::i18n::tr("아래로"))).clicked() {
                acts.push(Act::MoveItem(i, 1));
                ui.close();
            }
            if ui.add_enabled(i > 0, egui::Button::new(wp_core::i18n::tr("맨 위로"))).clicked() {
                app.core.lock().move_item_to(i, 0);
                app.sel = Some(0);
                app.edit = None;
                ui.close();
            }
            if ui.add_enabled(i + 1 < n, egui::Button::new(wp_core::i18n::tr("맨 아래로"))).clicked() {
                app.core.lock().move_item_to(i, n - 1);
                app.sel = Some(n - 1);
                app.edit = None;
                ui.close();
            }
        });
        if ui.button(wp_core::i18n::tr("복제")).clicked() {
            let mut c = app.core.lock();
            c.remember_items();
            c.add_item(wp_core::parity_a::duplicate_item(it), Some(i + 1));
            app.sel = Some(i + 1);
            app.edit = None;
            ui.close();
        }
        if ui.button(RichText::new(wp_core::i18n::tr("지우기")).color(Color32::from_rgb(0xff, 0x8a, 0x8a))).clicked() {
            acts.push(Act::RemoveItem(i));
            ui.close();
        }
        ui.separator();
        if ui.button(wp_core::i18n::tr("큐시트를 글로 복사 (순서표)")).clicked() {
            if let Some(t) = app.core.lock().active_cue_text() {
                ui.ctx().copy_text(t);
            }
            app.core.lock().notify(wp_core::i18n::tr("큐시트 순서표를 글로 복사했습니다."), "ok");
            ui.close();
        }
    });
}

/// 순서 넣기 단추 (카메라 · 슬라이드쇼)
pub fn add_buttons(ui: &mut egui::Ui, acts: &mut Vec<Act>) {
    if ui.add(egui::Button::new(RichText::new(wp_core::i18n::tr("카메라")).color(Color32::from_rgb(0xd9, 0x6a, 0x6a)))).on_hover_text(wp_core::i18n::tr("웹캠·캡처보드를 예배당 화면에 (기기는 편집칸에서)")).clicked() {
        acts.push(Act::AddItem("camera", Some(json!({ "title": "카메라", "deviceId": "", "label": "", "fit": "cover" }))));
    }
    if ui.add(egui::Button::new(RichText::new(wp_core::i18n::tr("슬라이드쇼")).color(Color32::from_rgb(0x9b, 0x87, 0xf5)))).on_hover_text(wp_core::i18n::tr("그림 여러 장 + 광고 글을 자동으로 넘기기 (예배 전 화면)")).clicked() {
        acts.push(Act::AddItem("slideshow", Some(json!({ "title": "슬라이드쇼", "images": [], "body": "", "autoSec": 8, "autoLoop": true, "countdown": "", "countdownLabel": "예배 시작까지" }))));
    }
}

/// 카메라 · 슬라이드쇼 · 영상 순서 편집칸 → 바뀌었나
pub fn editor(ui: &mut egui::Ui, e: &mut Value, known_cameras: &[(String, String)]) -> bool {
    let mut changed = false;
    let mut text = |ui: &mut egui::Ui, e: &mut Value, k: &str, hint: &str, multi: bool| {
        let mut s = e[k].as_str().unwrap_or("").to_string();
        let r = if multi { ui.add(egui::TextEdit::multiline(&mut s).desired_rows(4).desired_width(f32::INFINITY).hint_text(hint)) } else { ui.add(egui::TextEdit::singleline(&mut s).desired_width(f32::INFINITY).hint_text(hint)) };
        if r.changed() {
            e[k] = json!(s);
            true
        } else {
            false
        }
    };
    match e["type"].as_str().unwrap_or("") {
        "camera" => {
            ui.label(RichText::new(wp_core::i18n::tr("카메라 기기 (예배당 화면 브라우저의 기기 번호, 비우면 기본 카메라)")).small());
            if !known_cameras.is_empty() {
                let cur = e["deviceId"].as_str().unwrap_or("").to_string();
                let name = known_cameras.iter().find(|c| c.0 == cur).map(|c| c.1.clone()).unwrap_or_else(|| if cur.is_empty() { "기본 카메라".into() } else { cur.chars().take(12).collect() });
                egui::ComboBox::from_id_salt("cam-dev").selected_text(name).show_ui(ui, |ui| {
                    if ui.selectable_label(cur.is_empty(), wp_core::i18n::tr("기본 카메라")).clicked() {
                        e["deviceId"] = json!("");
                        changed = true;
                    }
                    for (id, label) in known_cameras {
                        if ui.selectable_label(&cur == id, label).clicked() {
                            e["deviceId"] = json!(id);
                            e["label"] = json!(label);
                            changed = true;
                        }
                    }
                });
            }
            changed |= text(ui, e, "deviceId", "기기 번호 (직접 넣기)", false);
            let mut fit = e["fit"].as_str().unwrap_or("cover").to_string();
            ui.horizontal(|ui| {
                if ui.radio_value(&mut fit, "cover".into(), wp_core::i18n::tr("화면 가득")).changed() | ui.radio_value(&mut fit, "contain".into(), wp_core::i18n::tr("잘리지 않게")).changed() {
                    e["fit"] = json!(fit);
                    changed = true;
                }
            });
        }
        "slideshow" => {
            let imgs: Vec<String> = e["images"].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(String::from)).collect();
            ui.label(RichText::new(wp_core::i18n::tr_args("그림 {1}장", &[&(imgs.len())])).small());
            let mut del = None;
            for (k, u) in imgs.iter().enumerate() {
                ui.horizontal(|ui| {
                    let name = percent_encoding::percent_decode_str(u).decode_utf8_lossy().rsplit(['\\', '/']).next().unwrap_or("").to_string();
                    ui.label(RichText::new(format!("{}. {}", k + 1, name)).small());
                    if ui.small_button(wp_core::i18n::tr("빼기")).clicked() {
                        del = Some(k);
                    }
                });
            }
            if let Some(k) = del {
                e["images"].as_array_mut().unwrap().remove(k);
                changed = true;
            }
            if ui.button(wp_core::i18n::tr("그림 더하기…")).clicked() {
                if let Some(ps) = rfd::FileDialog::new().add_filter("그림", &["png", "jpg", "jpeg", "webp", "gif", "bmp"]).pick_files() {
                    if !e["images"].is_array() {
                        e["images"] = json!([]);
                    }
                    for p in ps {
                        e["images"].as_array_mut().unwrap().push(json!(wp_core::library::file_url(&p)));
                    }
                    changed = true;
                }
            }
            ui.label(RichText::new(wp_core::i18n::tr("광고 글 ('---' 줄로 장 나눔)")).small());
            changed |= text(ui, e, "body", "", true);
            ui.horizontal(|ui| {
                let mut sec = e["autoSec"].as_f64().unwrap_or(8.0);
                ui.label(wp_core::i18n::tr("넘김"));
                if ui.add(egui::DragValue::new(&mut sec).range(2.0..=3600.0).suffix(wp_core::i18n::tr("초"))).changed() {
                    e["autoSec"] = json!(sec);
                    changed = true;
                }
                let mut lp = e["autoLoop"].as_bool().unwrap_or(true);
                if ui.checkbox(&mut lp, wp_core::i18n::tr("반복")).changed() {
                    e["autoLoop"] = json!(lp);
                    changed = true;
                }
                ui.label(wp_core::i18n::tr("카운트다운"));
                let mut cd = e["countdown"].as_str().unwrap_or("").to_string();
                if ui.add(egui::TextEdit::singleline(&mut cd).hint_text("11:00").desired_width(54.0)).changed() {
                    e["countdown"] = json!(cd);
                    changed = true;
                }
            });
        }
        "video" => {
            let f = e["file"].as_str().unwrap_or("");
            ui.label(RichText::new(percent_encoding::percent_decode_str(f).decode_utf8_lossy().rsplit(['\\', '/']).next().unwrap_or("").to_string()).small());
            if ui.button(wp_core::i18n::tr("영상 고르기…")).clicked() {
                if let Some(p) = rfd::FileDialog::new().add_filter("영상", &["mp4", "m4v", "webm", "mov"]).pick_file() {
                    e["file"] = json!(wp_core::library::file_url(&p));
                    changed = true;
                }
            }
            ui.horizontal(|ui| {
                let mut snd = e["sound"].as_bool().unwrap_or(true);
                if ui.checkbox(&mut snd, wp_core::i18n::tr("소리")).changed() {
                    e["sound"] = json!(snd);
                    changed = true;
                }
                let mut vol = e["volume"].as_f64().unwrap_or(1.0);
                if ui.add(egui::Slider::new(&mut vol, 0.0..=1.0).text("크기")).changed() {
                    e["volume"] = json!(vol);
                    changed = true;
                }
                let mut lp = e["loop"].as_bool().unwrap_or(false);
                if ui.checkbox(&mut lp, wp_core::i18n::tr("반복")).changed() {
                    e["loop"] = json!(lp);
                    changed = true;
                }
            });
            let mut end = e["endAction"].as_str().unwrap_or("hold").to_string();
            ui.horizontal(|ui| {
                ui.label(wp_core::i18n::tr("끝나면"));
                for (v, l) in [("hold", "마지막 장면"), ("black", "검은 화면"), ("next", "다음 순서")] {
                    if ui.radio_value(&mut end, v.into(), l).changed() {
                        e["endAction"] = json!(end);
                        changed = true;
                    }
                }
            });
        }
        _ => {
            ui.label(RichText::new(wp_core::i18n::tr("이 종류는 아직 Rust 판에서 고칠 수 없습니다.")).color(Color32::GRAY));
        }
    }
    changed
}

/// 설정·큐시트에 나온 카메라들 (deviceId, 이름)
pub fn known_cameras(core: &wp_core::core::Core) -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = vec![];
    for (id, p) in core.settings.get()["cameras"].as_object().cloned().unwrap_or_default() {
        if !id.is_empty() {
            v.push((id.clone(), p["label"].as_str().filter(|x| !x.is_empty()).map(String::from).unwrap_or_else(|| id.chars().take(12).collect())));
        }
    }
    for c in core.cues.iter().filter_map(|c| core.store.get_cue(&c.id)) {
        for it in c.items.iter().filter(|it| it["type"] == "camera") {
            let id = it["deviceId"].as_str().unwrap_or("").to_string();
            if !id.is_empty() && !v.iter().any(|x| x.0 == id) {
                v.push((id.clone(), it["label"].as_str().filter(|x| !x.is_empty()).map(String::from).unwrap_or_else(|| id.chars().take(12).collect())));
            }
        }
    }
    v
}

/// 도움말 메뉴 'a:help-…' (모르는 id 면 false)
pub fn handle_menu(id: &str) -> bool {
    match id {
        "a:help-manual" => open_link(&format!("{REPO}/blob/main/docs/%EC%82%AC%EC%9A%A9%EC%9E%90%EC%84%A4%EB%AA%85%EC%84%9C.md")),
        "a:help-readme" => open_link(&format!("{REPO}#readme")),
        "a:help-obs" => open_link(&format!("{REPO}/blob/main/docs/OBS%EC%84%A4%EC%A0%95%EA%B0%80%EC%9D%B4%EB%93%9C.md")),
        "a:help-news" => open_link(&format!("{REPO}/releases")),
        "a:help-issue" => open_link(&format!("{REPO}/issues")),
        "a:help-keys" => *KEYS_OPEN.lock() = true,
        _ => return false,
    }
    true
}

/// 단축키 보기 창
pub fn show(app: &mut App, ctx: &egui::Context) {
    let mut open = *KEYS_OPEN.lock();
    if !open {
        return;
    }
    let (labels, keys) = {
        let c = app.core.lock();
        (wp_core::settings::SHORTCUTS["labels"].clone(), c.settings.get()["shortcuts"].clone())
    };
    egui::Window::new(wp_core::i18n::tr("단축키")).open(&mut open).default_width(420.0).show(ctx, |ui| {
        egui::Grid::new("keys").striped(true).num_columns(2).show(ui, |ui| {
            for (k, label) in labels.as_object().cloned().unwrap_or_default() {
                ui.label(RichText::new(keys[&k].as_str().unwrap_or("")).monospace().strong());
                ui.label(label.as_str().unwrap_or(""));
                ui.end_row();
            }
            for (k, l) in [("Ctrl+Z / Ctrl+Y", "실행 취소 · 다시"), ("Ctrl+E", "슬라이드 편집"), ("Ctrl+V", "클립보드에서 붙여 넣기 (새 순서)"), ("Alt+1~9", "매크로"), ("Space", "다음 슬라이드 (입력칸 밖)")] {
                ui.label(RichText::new(k).monospace().strong());
                ui.label(l);
                ui.end_row();
            }
        });
        ui.label(RichText::new(wp_core::i18n::tr("단축키는 설정 → 단축키에서 바꿀 수 있습니다.")).small().color(Color32::GRAY));
    });
    *KEYS_OPEN.lock() = open;
}

/// 성경 결과 한 절: 오른쪽 클릭 → 장절·본문 복사
pub fn verse_menu(resp: &egui::Response, reference: &str, text: &str) {
    resp.context_menu(|ui| {
        if ui.button(wp_core::i18n::tr("장절·본문 복사")).clicked() {
            ui.ctx().copy_text(format!("{reference} {text}"));
            ui.close();
        }
        if ui.button(wp_core::i18n::tr("장절만 복사")).clicked() {
            ui.ctx().copy_text(reference.to_string());
            ui.close();
        }
    });
}
