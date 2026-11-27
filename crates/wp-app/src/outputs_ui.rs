//! 출력 목록 (로드맵 3단계, docs/roadmap-sources.md '3단계 한 것'):
//! 출력 설정 창(trackd_ui output_ui) 안의 '출력 더하기' 구획과, 추가 출력을 모니터에 띄우는 창(wp-output) 관리.
//!
//! - 추가 출력 = 설정 `extraOutputs[]` (wp_core::outputs). 주소 /outputs/<id>, OBS 브라우저 소스 · 다른 PC 브라우저 · 모니터 창 어디서나 연다.
//! - '모니터에 띄우기'(window) 를 켜고 모니터를 고르면 조작 화면이 wp-output 을 하나 더 띄운다 (예배당 창과 같은 프로그램 · 같은 WebView2 자료 폴더).
//!   창에서 Esc 를 누르거나 창이 스스로 꺼지면 '띄우기' 를 끈다 (계속 다시 띄우지 않게). 조작 화면이 끝나면 같이 꺼진다 (outwin::bind_to_app).
//! - 내가 띄운 창(PID)만 끈다.
use crate::app::App;
use crate::theme::{self, pal, Weight};
use crate::widgets::{self, Btn, Size};
use eframe::egui::{self, Id};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde_json::json;
use std::collections::HashMap;
use std::process::{Child, Command};
use wp_core::i18n::{tr, tr_args};

/// 추가 출력 id → (창 프로세스, 띄운 모니터)
static KIDS: Lazy<Mutex<HashMap<String, (Child, i64)>>> = Lazy::new(|| Mutex::new(HashMap::new()));

/// 추가 출력 창 인자 (예배당 창과 같은 모양, 주소만 /outputs/<id>)
pub(crate) fn window_args(port: u16, id: &str, monitor: i64, name: &str) -> Vec<String> {
    vec![
        "--url".into(),
        format!("http://127.0.0.1:{port}/outputs/{id}?window=1"),
        "--monitor".into(),
        monitor.max(0).to_string(),
        "--esc-close".into(),
        "--title".into(),
        format!("{} - {name}", tr("워십프리젠터 출력")),
    ]
}

fn testing() -> bool {
    std::env::args().any(|x| x == "--shot")
}

/// 매 프레임: 설정대로 추가 출력 창을 띄우고 끈다
pub fn frame(app: &mut App) {
    let (extras, port) = {
        let c = app.core.lock();
        (c.extra_outputs(), c.port)
    };
    let mut kids = KIDS.lock();
    // 스스로 끝난 창: 띄우기를 끄고 알림
    let mut ended: Vec<(String, Option<i32>)> = vec![];
    kids.retain(|id, (c, _)| match c.try_wait() {
        Ok(None) => true,
        r => {
            ended.push((id.clone(), r.ok().flatten().and_then(|s| s.code())));
            false
        }
    });
    for (id, code) in ended {
        let name = extras.iter().find(|o| o["id"] == id.as_str()).and_then(|o| o["name"].as_str()).unwrap_or("").to_string();
        let mut c = app.core.lock();
        if let Some(mut o) = c.extra_outputs().into_iter().find(|o| o["id"] == id.as_str()) {
            if o["window"] == true {
                o["window"] = json!(false);
                let _ = c.save_extra_output(&o);
                let msg = if code == Some(crate::outwin::EXIT_ESC) { tr_args("'{1}' 화면에서 Esc 를 눌러 창을 닫았습니다.", &[&name]) } else { tr_args("'{1}' 화면 창이 닫혔습니다.", &[&name]) };
                c.notify(msg, "info");
            }
        }
    }
    // 띄울 것 · 끌 것
    let want: Vec<(String, i64, String)> = extras
        .iter()
        .filter(|o| o["window"] == true && o["monitor"].as_i64().unwrap_or(-1) >= 0)
        .map(|o| (o["id"].as_str().unwrap_or("").to_string(), o["monitor"].as_i64().unwrap_or(0), o["name"].as_str().unwrap_or("").to_string()))
        .collect();
    let stale: Vec<String> = kids.iter().filter(|(id, (_, m))| !want.iter().any(|(w, wm, _)| w == *id && wm == m)).map(|(id, _)| id.clone()).collect();
    for id in stale {
        if let Some((mut c, _)) = kids.remove(&id) {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
    if testing() {
        return; // 점검 실행(--shot)에서는 사용자 모니터에 창을 띄우지 않는다
    }
    for (id, mon, name) in want {
        if kids.contains_key(&id) {
            continue;
        }
        let mut cmd = Command::new(crate::outwin::exe());
        cmd.args(window_args(port, &id, mon, &name));
        match cmd.spawn() {
            Ok(c) => {
                crate::outwin::bind_to_app(&c);
                kids.insert(id, (c, mon));
            }
            Err(e) => {
                drop(kids);
                let mut c = app.core.lock();
                if let Some(mut o) = c.extra_outputs().into_iter().find(|o| o["id"] == id.as_str()) {
                    o["window"] = json!(false);
                    let _ = c.save_extra_output(&o);
                }
                c.notify(format!("{}: {e}", tr("출력 창을 띄우지 못했습니다")), "error");
                return;
            }
        }
    }
}

/// 이 추가 출력의 창이 떠 있나
pub fn running(id: &str) -> bool {
    KIDS.lock().contains_key(id)
}

/// 출력 설정 창 안 '출력' 구획: 소리 내는 출력 · 추가 출력 목록 · 출력 더하기
pub(crate) fn section(ui: &mut egui::Ui, app: &mut App) {
    let p = pal();
    let core = app.core.clone();
    let (extras, list, audio, scenes, tpls, port) = {
        let c = core.lock();
        let s = c.settings.get();
        (
            c.extra_outputs(),
            c.output_list(),
            wp_core::outputs::audio_output(s),
            c.scenes().iter().map(|x| (x["id"].as_str().unwrap_or("").to_string(), x["name"].as_str().unwrap_or("").to_string())).collect::<Vec<_>>(),
            c.settings.templates().iter().map(|t| (t["id"].as_str().unwrap_or("").to_string(), wp_core::i18n::tr_name(t["name"].as_str().unwrap_or("")))).collect::<Vec<_>>(),
            c.port,
        )
    };
    let shown = |id: &str, n: &str| if id == "main" || id == "overlay" { tr(n) } else { n.to_string() };
    ui.label(theme::rich(tr("출력"), 14.0, Weight::Bold, p.text));
    ui.add(egui::Label::new(theme::rich(tr("예배당 · 방송(OBS) 말고도 로비 · 유아실 · 둘째 방송 같은 화면을 더할 수 있습니다. 출력마다 주소가 따로 있고, 모니터에 띄우거나 OBS · 다른 PC 브라우저에서 엽니다."), 11.5, Weight::Regular, p.muted)).wrap());
    ui.add_space(4.0);
    // 소리 내는 출력
    ui.horizontal(|ui| {
        ui.label(theme::rich(tr("영상 · 유튜브 소리"), 12.5, Weight::Regular, p.text2));
        let mut pick = audio.clone();
        let cur = list.iter().find(|(k, _)| *k == audio).map(|(k, n)| shown(k, n)).unwrap_or_default();
        let r = egui::ComboBox::from_id_salt("out-audio").width(200.0).selected_text(cur).show_ui(ui, |ui| {
            for (k, n) in &list {
                ui.selectable_value(&mut pick, k.clone(), shown(k, n));
            }
        });
        widgets::mark("out-audio", r.response.rect);
        if pick != audio {
            let r = core.lock().set_audio_output(&pick);
            if let Err(e) = r {
                core.lock().notify(e.to_string(), "error");
            }
        }
    });
    ui.add(egui::Label::new(theme::rich(tr("소리는 이 출력 하나에서만 납니다 (모니터 창으로 띄웠을 때). 나머지 출력은 소리를 끕니다."), 11.0, Weight::Regular, p.muted2)).wrap());
    ui.add_space(6.0);
    let mons = app.out.monitors.clone();
    for o in &extras {
        let id = o["id"].as_str().unwrap_or("").to_string();
        let mut v = o.clone();
        let mut changed = false;
        let mut delete = false;
        egui::Frame::new().fill(p.sunken).stroke(egui::Stroke::new(1.0, p.line)).corner_radius(theme::cr(theme::Rad::Md)).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                // 이름: 입력 중에는 글만 들고 있다가 칸을 떠날 때 저장
                let nk = Id::new(("out-name", &id));
                let mut name = ui.data(|d| d.get_temp::<String>(nk)).unwrap_or_else(|| v["name"].as_str().unwrap_or("").to_string());
                let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(170.0).hint_text(tr("출력 이름")));
                if r.changed() {
                    ui.data_mut(|d| d.insert_temp(nk, name.clone()));
                }
                if r.lost_focus() {
                    ui.data_mut(|d| d.remove::<String>(nk));
                    if !name.trim().is_empty() && name.trim() != v["name"].as_str().unwrap_or("") {
                        v["name"] = json!(name.trim());
                        changed = true;
                    }
                }
                let on = running(&id);
                let tag = if on { tr("모니터에 떠 있음") } else { String::new() };
                if !tag.is_empty() {
                    widgets::badge(ui, &tag, widgets::Badge::Live);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let k = Id::new(("out-del-ask", &id));
                    let ask = ui.data(|d| d.get_temp::<bool>(k)).unwrap_or(false);
                    if ask {
                        if ui.add(Btn::new(tr("정말 지우기")).size(Size::Sm)).clicked() {
                            delete = true;
                        }
                        if ui.add(Btn::new(tr("그만두기")).size(Size::Sm)).clicked() {
                            ui.data_mut(|d| d.remove::<bool>(k));
                        }
                    } else if ui.add(Btn::icon_only("trash-2").size(Size::Sm).tooltip(tr("이 출력 지우기"))).clicked() {
                        ui.data_mut(|d| d.insert_temp(k, true));
                    }
                });
            });
            ui.horizontal_wrapped(|ui| {
                // 어느 출력처럼
                let mut kind = v["kind"].as_str().unwrap_or("main").to_string();
                let before = kind.clone();
                egui::ComboBox::from_id_salt(("out-kind", &id)).width(150.0).selected_text(tr(if kind == "overlay" { "방송(OBS)처럼" } else { "예배당처럼" })).show_ui(ui, |ui| {
                    ui.selectable_value(&mut kind, "main".into(), tr("예배당처럼"));
                    ui.selectable_value(&mut kind, "overlay".into(), tr("방송(OBS)처럼"));
                })
                .response
                .on_hover_text(tr("템플릿 · 오버레이 · 배경 미디어를 어느 출력과 같게 쓸지"));
                if kind != before {
                    v["kind"] = json!(kind);
                    changed = true;
                }
                // 템플릿
                let mut t = v["template"].as_str().unwrap_or("").to_string();
                let tb = t.clone();
                let tname = tpls.iter().find(|(k, _)| *k == t).map(|(_, n)| n.clone()).unwrap_or_else(|| tr("같은 출력의 템플릿"));
                egui::ComboBox::from_id_salt(("out-tpl", &id)).width(170.0).selected_text(tname).show_ui(ui, |ui| {
                    ui.selectable_value(&mut t, String::new(), tr("같은 출력의 템플릿"));
                    for (k, n) in &tpls {
                        ui.selectable_value(&mut t, k.clone(), n);
                    }
                });
                if t != tb {
                    v["template"] = json!(t);
                    changed = true;
                }
                // 기본 구성
                let cur = core.lock().output_scene(&id);
                let mut sc = cur.clone();
                let sname = scenes.iter().find(|(k, _)| *k == cur).map(|(_, n)| n.clone()).unwrap_or_else(|| tr("전체 화면 (기본)"));
                egui::ComboBox::from_id_salt(("out-scene", &id)).width(170.0).selected_text(sname).show_ui(ui, |ui| {
                    ui.selectable_value(&mut sc, wp_core::scenes::FULL.to_string(), tr("전체 화면 (기본)"));
                    for (k, n) in &scenes {
                        ui.selectable_value(&mut sc, k.clone(), n);
                    }
                })
                .response
                .on_hover_text(tr("이 출력의 기본 화면 구성 (순서마다 바꾸는 것은 인스펙터 '소스' 탭)"));
                if sc != cur {
                    let r = core.lock().set_output_scene(&id, &sc);
                    if let Err(e) = r {
                        core.lock().notify(e.to_string(), "error");
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                // 모니터
                let mon = v["monitor"].as_i64().unwrap_or(-1);
                let mut m = mon;
                let mname = mons.iter().find(|x| x.index as i64 == mon).map(|x| format!("{} {}×{}", x.name, x.size.0, x.size.1)).unwrap_or_else(|| tr("모니터 없음 (주소로만)"));
                egui::ComboBox::from_id_salt(("out-mon", &id)).width(210.0).selected_text(mname).show_ui(ui, |ui| {
                    ui.selectable_value(&mut m, -1, tr("모니터 없음 (주소로만)"));
                    for x in &mons {
                        ui.selectable_value(&mut m, x.index as i64, format!("{} {}×{}{}", x.name, x.size.0, x.size.1, if x.primary { tr_args(" ({1})", &[&tr("주 모니터")]) } else { String::new() }));
                    }
                });
                if m != mon {
                    v["monitor"] = json!(m);
                    if m < 0 {
                        v["window"] = json!(false);
                    }
                    changed = true;
                }
                let on = v["window"] == true;
                let b = ui.add(Btn::new(if on { tr("모니터에서 내리기") } else { tr("모니터에 띄우기") }).icon(if on { "monitor-off" } else { "monitor" }).size(Size::Sm).enabled(m >= 0).tooltip(tr("고른 모니터에 전체 화면으로 띄웁니다 (창을 눌러 고른 뒤 Esc = 닫기)")));
                widgets::mark(&format!("out-window-{id}"), b.rect);
                if b.clicked() {
                    v["window"] = json!(!on);
                    changed = true;
                }
            });
            let url = format!("http://127.0.0.1:{port}/outputs/{id}");
            ui.horizontal(|ui| {
                ui.add(egui::Label::new(egui::RichText::new(&url).monospace().color(p.text2)).selectable(true));
                if ui.add(Btn::new(tr("복사")).icon("copy").size(Size::Sm).tooltip(tr("주소를 복사해 OBS 브라우저 소스나 다른 PC 브라우저에 붙여 넣습니다"))).clicked() {
                    widgets::copy_notice(ui.ctx(), url.clone(), tr("출력 주소를 복사했습니다"));
                }
            });
        });
        ui.add_space(4.0);
        if delete {
            core.lock().delete_extra_output(&id);
        } else if changed {
            let r = core.lock().save_extra_output(&v);
            if let Err(e) = r {
                core.lock().notify(e.to_string(), "error");
            }
        }
    }
    // 더하기
    let k = Id::new("out-add-name");
    let mut name = ui.data(|d| d.get_temp::<String>(k)).unwrap_or_default();
    let mut go = false;
    ui.horizontal(|ui| {
        let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(200.0).hint_text(tr("새 출력 이름 (예: 로비 화면)")));
        go |= r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !name.trim().is_empty();
        let b = ui.add(Btn::new(tr("출력 더하기")).icon("plus").size(Size::Sm).enabled(extras.len() < wp_core::outputs::MAX_EXTRA));
        widgets::mark("out-add", b.rect);
        go |= b.clicked();
    });
    if go {
        let nm = if name.trim().is_empty() { tr_args("출력 {1}", &[&(extras.len() + 3).to_string()]) } else { name.trim().to_string() };
        let r = core.lock().save_extra_output(&wp_core::outputs::new_extra(&nm, "main"));
        match r {
            Ok(_) => name.clear(),
            Err(e) => core.lock().notify(e.to_string(), "error"),
        }
    }
    ui.data_mut(|d| d.insert_temp(k, name));
}

#[cfg(test)]
mod tests {
    #[test]
    fn window_args_point_at_extra_output() {
        let a = super::window_args(37528, "out-lobby", 2, "로비");
        assert!(a.contains(&"http://127.0.0.1:37528/outputs/out-lobby?window=1".to_string()));
        assert!(a.windows(2).any(|w| w[0] == "--monitor" && w[1] == "2"));
        assert!(a.contains(&"--esc-close".to_string()));
    }
}
