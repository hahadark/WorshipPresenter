//! 순서 편집칸: 배경 음악 + '⚡ 송출할 때 자동으로' (Electron `cue.js:833-852 actionsHtml` · `obsActHtml` · `devicepanel.js actionHtml`)
//!
//! - 저장 키는 Electron 과 같다: `it.music`, `it.musicVolume`, `it.actions.{timer, stageMsg, clearStage, logo, bg, ovOn, ovOff,
//!   stopMusic, macro, stagePin, obsScene, obsStream, obsRecord, companion, dev[]}`. 비우면 키를 지운다.
//! - 정규화(범위·길이)는 저장할 때 `wp_core::actions::normalize_item_actions` 가 한다 (app.rs editor). 편집 중 글은 그대로 둔다.
//! - 실행은 `wp_core::trackd::run_item_actions` (여기서는 호출하지 않음). '보내 보기'·'눌러 보기'는 그 자리에서 한 번 보낸다.
//! - OBS 장면 목록: 도구 → OBS 원격 조작 창의 상태 캐시(`trackd_ui::obs_state`)에 장면이 있으면 콤보, 없으면 글 입력.

use eframe::egui::{self, RichText};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde_json::{json, Value};
use wp_core::core::Shared;
use wp_core::i18n::tr;

/// 접힘 상태 · 매크로 이름 입력칸 (Electron `st.actOpen`)
#[derive(Default)]
struct St {
    open: bool,
    macro_name: Option<String>,
    scroll_once: bool,
}
static ST: Lazy<Mutex<St>> = Lazy::new(|| Mutex::new(St::default()));

/// 점검(--edit-actions)용: 자동 동작 구역을 펼친다/접는다
pub fn set_open(open: bool) {
    let mut s = ST.lock();
    s.open = open;
    s.scroll_once = open;
}

/// 장비 글 힌트 (Electron devicepanel.js HINT)
fn dev_hint(kind: &str) -> String {
    if kind == "x32" { "ch1 on · ch5 fader 0.75 · main off · scene 3".to_string() } else { tr("프리셋 번호 (예: 2) · home") }
}

/// '/lib/file?p=…' 의 파일 이름 (Electron `fileName`)
pub fn file_name(url: &str) -> String {
    let p = url.rsplit("p=").next().unwrap_or(url);
    let dec = percent_encoding::percent_decode_str(p).decode_utf8_lossy();
    dec.rsplit(['\\', '/']).next().unwrap_or("").to_string()
}

/// actions 의 한 칸을 넣거나(값이 있으면) 지운다(빈 글·false·Null). actions 가 비면 키 자체를 지운다.
pub fn set_action(e: &mut Value, k: &str, v: Value) {
    let empty = match &v {
        Value::Null => true,
        Value::String(s) => s.is_empty(),
        Value::Bool(b) => !b,
        Value::Array(a) => a.is_empty(),
        Value::Number(n) => n.as_f64().is_some_and(|x| x <= 0.0),
        _ => false,
    };
    if !e["actions"].is_object() {
        if empty {
            return;
        }
        e["actions"] = json!({});
    }
    let o = e["actions"].as_object_mut().unwrap();
    if empty {
        o.remove(k);
    } else {
        o.insert(k.to_string(), v);
    }
    if o.is_empty() {
        e.as_object_mut().map(|m| m.remove("actions"));
    }
}

/// dev 목록에서 장비 하나의 동작 글을 바꾼다 (빈 글이면 뺀다)
pub fn set_device_action(e: &mut Value, device: &str, action: &str) {
    let mut list: Vec<Value> = e["actions"]["dev"].as_array().cloned().unwrap_or_default().into_iter().filter(|x| x["device"] != device).collect();
    if !action.trim().is_empty() {
        list.push(json!({ "device": device, "action": action }));
    }
    set_action(e, "dev", json!(list));
}

fn dev_action_of(e: &Value, device: &str) -> String {
    e["actions"]["dev"].as_array().into_iter().flatten().find(|x| x["device"] == device).and_then(|x| x["action"].as_str()).unwrap_or("").to_string()
}

/// 순서 동작 '구성 바꾸기' 의 출력 하나: actions.scene[출력] (빈 값 = 바꾸지 않음 → 칸을 지움)
pub fn set_scene_action(e: &mut Value, output: &str, id: &str) {
    let mut m = e["actions"]["scene"].as_object().cloned().unwrap_or_default();
    if id.is_empty() {
        m.remove(output);
    } else {
        m.insert(output.to_string(), json!(id));
    }
    set_action(e, "scene", if m.is_empty() { Value::Null } else { Value::Object(m) });
}

fn scene_combo(ui: &mut egui::Ui, e: &mut Value, output: &str, opts: &[(String, String)]) -> bool {
    let cur = e["actions"]["scene"][output].as_str().unwrap_or("").to_string();
    let mut sel = cur.clone();
    let label = opts.iter().find(|(v, _)| *v == cur).map(|(_, t)| t.clone()).unwrap_or_else(|| if cur.is_empty() { opts[0].1.clone() } else { "?".into() });
    let r = egui::ComboBox::from_id_salt(("act-scene", output)).selected_text(label).width(150.0).show_ui(ui, |ui| {
        for (v, t) in opts {
            ui.selectable_value(&mut sel, v.clone(), t);
        }
    });
    crate::widgets::mark(&format!("act-scene-{output}"), r.response.rect);
    if sel != cur {
        set_scene_action(e, output, &sel);
        true
    } else {
        false
    }
}

/// 콤보 한 개: (값, 표시) 목록에서 고르기. 바뀌었으면 true. 빈 값(첫 항목)은 키를 지운다.
fn combo(ui: &mut egui::Ui, e: &mut Value, k: &str, opts: &[(String, String)], width: f32) -> bool {
    let cur = e["actions"][k].as_str().unwrap_or("").to_string();
    let mut sel = cur.clone();
    let label = opts.iter().find(|(v, _)| *v == cur).map(|(_, t)| t.clone()).unwrap_or_else(|| if cur.is_empty() { opts.first().map(|o| o.1.clone()).unwrap_or_default() } else { cur.clone() });
    egui::ComboBox::from_id_salt(("act-combo", k)).selected_text(label).width(width).show_ui(ui, |ui| {
        for (v, t) in opts {
            ui.selectable_value(&mut sel, v.clone(), t);
        }
    });
    if sel != cur {
        set_action(e, k, json!(sel));
        true
    } else {
        false
    }
}

fn pick_audio_url() -> Option<String> {
    let p = rfd::FileDialog::new().add_filter("음악", &["mp3", "wav", "ogg", "m4a", "aac", "flac", "opus"]).pick_file()?;
    let enc = percent_encoding::utf8_percent_encode(&p.to_string_lossy(), percent_encoding::NON_ALPHANUMERIC).to_string();
    Some(format!("/lib/file?p={enc}"))
}

/// 백그라운드에서 한 번 보내고 결과를 알림으로 (송출 화면이 멈추지 않게)
fn spawn_notify(f: impl FnOnce() -> Result<String, String> + Send + 'static) {
    std::thread::spawn(move || {
        let r = f();
        if let Some(sh) = wp_core::hymns::shared() {
            match r {
                Ok(m) => sh.lock().notify(m, "ok"),
                Err(m) => sh.lock().notify(m, "error"),
            }
        }
    });
}

/// 편집칸 본체. 바뀌었으면 true (app.rs editor 가 저장할 때 정규화한다)
pub fn item_fields(ui: &mut egui::Ui, e: &mut Value, core: &Shared) -> bool {
    let mut ch = false;
    let settings = core.lock().settings.get().clone();
    let gray = |s: String| RichText::new(s).small().color(crate::theme::pal().muted);

    // ---- 배경 음악 (Electron: 송출하면 반복 재생, 다른 화면을 송출하면 서서히 꺼짐) ----
    ui.label(RichText::new(tr("배경 음악")).small());
    ui.horizontal_wrapped(|ui| {
        let music = e["music"].as_str().unwrap_or("").to_string();
        if music.is_empty() {
            ui.label(gray(tr("없음")));
        } else {
            ui.label(file_name(&music));
            let mut vol = e["musicVolume"].as_f64().unwrap_or(0.8) as f32;
            if ui.add(egui::Slider::new(&mut vol, 0.0..=1.0).step_by(0.05).show_value(false)).on_hover_text(tr("소리 크기")).changed() {
                e["musicVolume"] = json!((vol as f64 * 100.0).round() / 100.0);
                ch = true;
            }
            if ui.button(tr("빼기")).clicked() {
                e["music"] = json!("");
                ch = true;
            }
        }
        if ui.button(if music.is_empty() { tr("음악 고르기…") } else { tr("음악 바꾸기…") }).clicked() {
            if let Some(url) = pick_audio_url() {
                e["music"] = json!(url);
                ch = true;
            }
        }
        ui.label(gray(tr("송출하면 반복 재생, 다른 화면을 송출하면 서서히 꺼짐")));
    });

    // ---- 자동 동작 (접는 구역) ----
    let n = wp_core::actions::count(e);
    let summary = wp_core::actions::summary(&settings, e);
    let mut st = ST.lock();
    ui.label(RichText::new(tr("자동 동작")).small());
    ui.horizontal_wrapped(|ui| {
        let head = format!("{}{} {}", tr("송출할 때 자동으로"), if n > 0 { format!(" ({n})") } else { String::new() }, if st.open { "▴" } else { "▾" });
        let b = egui::Button::new(if n > 0 { RichText::new(head).color(crate::theme::pal().warn) } else { RichText::new(head) });
        if ui.add(b).clicked() {
            st.open = !st.open;
        }
        ui.label(gray(if summary.is_empty() { tr("타이머 시작 · 무대 메시지 · 로고 화면 · 배경 · 오버레이 · 음악 멈춤") } else { summary.clone() }));
    });
    if !st.open {
        return ch;
    }
    let none = String::new();
    let keep = (none.clone(), tr("그대로"));
    egui::Grid::new("act-grid").num_columns(2).spacing([8.0, 4.0]).show(ui, |ui| {
        // 설교 타이머
        ui.label(tr("설교 타이머"));
        ui.horizontal(|ui| {
            let mut t = e["actions"]["timer"].as_f64().unwrap_or(0.0);
            if ui.add(egui::DragValue::new(&mut t).range(0.0..=240.0).speed(1.0)).changed() {
                set_action(e, "timer", json!(t.round()));
                ch = true;
            }
            ui.label(tr("분 시작"));
            ui.label(gray(tr("0 = 안 함")));
        });
        ui.end_row();
        // 무대 메시지
        ui.label(tr("무대 메시지"));
        ui.horizontal(|ui| {
            let mut m = e["actions"]["stageMsg"].as_str().unwrap_or("").to_string();
            if ui.add(egui::TextEdit::singleline(&mut m).desired_width(260.0).hint_text(tr("예: 설교 시작 — 휴대폰을 무음으로"))).changed() {
                set_action(e, "stageMsg", json!(m));
                ch = true;
            }
            let mut cs = e["actions"]["clearStage"].as_bool().unwrap_or(false);
            if ui.checkbox(&mut cs, tr("메시지 내리기")).on_hover_text(tr("무대 화면에 떠 있는 메시지를 없앱니다")).changed() {
                set_action(e, "clearStage", json!(cs));
                ch = true;
            }
        });
        ui.end_row();
        // 로고 · 배경
        ui.label(tr("로고 화면"));
        ui.horizontal(|ui| {
            ch |= combo(ui, e, "logo", &[keep.clone(), ("on".into(), tr("켜기")), ("off".into(), tr("끄기"))], 90.0);
            ui.label(tr("배경"));
            // '지우기'는 화면 비우기·삭제와 헷갈려 '끄기' (요약 문구 '배경 끄기'와 같게)
            ch |= combo(ui, e, "bg", &[keep.clone(), ("hide".into(), tr("끄기")), ("show".into(), tr("다시 보이기"))], 110.0);
        });
        ui.end_row();
        // 오버레이
        ui.label(tr("오버레이"));
        ui.horizontal(|ui| {
            let ovs: Vec<(String, String)> = settings["overlays"].as_array().into_iter().flatten().map(|o| (o["id"].as_str().unwrap_or("").to_string(), o["name"].as_str().unwrap_or("").to_string())).collect();
            let mut on = vec![(none.clone(), "—".to_string())];
            on.extend(ovs.iter().cloned());
            let mut off = vec![(none.clone(), "—".to_string()), ("*".to_string(), tr("모두"))];
            off.extend(ovs.iter().cloned());
            ui.label(tr("켜기"));
            ch |= combo(ui, e, "ovOn", &on, 120.0);
            ui.label(tr("끄기"));
            ch |= combo(ui, e, "ovOff", &off, 120.0);
        });
        ui.end_row();
        // 음악 멈춤
        ui.label(tr("음악"));
        {
            let mut sm = e["actions"]["stopMusic"].as_bool().unwrap_or(false);
            if ui.checkbox(&mut sm, tr("재생 중인 음악 서서히 멈춤")).changed() {
                set_action(e, "stopMusic", json!(sm));
                ch = true;
            }
        }
        ui.end_row();
        // 매크로
        ui.label(tr("매크로"));
        ui.horizontal(|ui| {
            let mut ms = vec![(none.clone(), tr("없음"))];
            ms.extend(settings["macros"].as_array().into_iter().flatten().map(|m| (m["id"].as_str().unwrap_or("").to_string(), m["name"].as_str().unwrap_or("").to_string())));
            ch |= combo(ui, e, "macro", &ms, 140.0);
            if st.macro_name.is_none() {
                if ui.button(tr("이 동작을 매크로로 저장…")).on_hover_text(tr("지금 이 순서의 자동 동작 묶음에 이름을 붙여 단추 하나로 (도구 → 매크로 · Alt+1~9)")).clicked() {
                    if wp_core::actions::count(e) == 0 {
                        core.lock().notify(tr("먼저 '송출할 때 자동으로' 에서 동작을 정하세요."), "warn");
                    } else {
                        st.macro_name = Some(e["title"].as_str().unwrap_or("").to_string());
                    }
                }
            }
        });
        ui.end_row();
        if let Some(mut name) = st.macro_name.take() {
            ui.label("");
            let mut done = false;
            ui.horizontal(|ui| {
                let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(200.0).hint_text(tr("매크로 이름 (예: 예배 시작 · 설교 · 축도 후)")));
                let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if ui.button(tr("저장")).clicked() || enter {
                    match wp_core::actions::macro_from_item(&name, e) {
                        Ok(m) => {
                            let mut c = core.lock();
                            match c.save_macro(&m) {
                                Ok(_) => {
                                    let cnt = c.settings.get()["macros"].as_array().map(|l| l.len()).unwrap_or(0);
                                    let key = if cnt <= 9 { cnt.to_string() } else { "?".to_string() };
                                    c.notify(wp_core::i18n::tr_args("매크로 '{1}' 을(를) 저장했습니다. 도구 → 매크로 · Alt+{2} 로 실행합니다.", &[&m["name"].as_str().unwrap_or(""), &key]), "ok");
                                    done = true;
                                }
                                Err(err) => c.notify(err.to_string(), "error"),
                            }
                        }
                        Err(err) => core.lock().notify(err, "warn"),
                    }
                }
                if ui.button(tr("취소")).clicked() {
                    done = true;
                }
            });
            if !done {
                st.macro_name = Some(name);
            }
            ui.end_row();
        }
        // 무대 화면 고정
        ui.label(tr("무대 화면"));
        {
            let mut sp = vec![keep.clone(), ("live".to_string(), tr("지금 화면 (고정 풀기)")), ("order".to_string(), tr("순서표 고정"))];
            sp.extend(settings["stagePresets"].as_array().into_iter().flatten().map(|p| (p["id"].as_str().unwrap_or("").to_string(), wp_core::i18n::tr_args("고정: {1}", &[&p["name"].as_str().unwrap_or("")]))));
            ch |= combo(ui, e, "stagePin", &sp, 200.0);
        }
        ui.end_row();
        // 화면 구성 바꾸기 (로드맵 4단계): 출력 기본을 바꾸는 남는 동작 — 구성이 있을 때만
        let user_scenes = wp_core::scenes::user_scenes(&settings);
        if !user_scenes.is_empty() {
            ui.label(tr("화면 구성")).on_hover_text(tr("이 순서를 송출할 때 출력의 기본 화면 구성을 바꿉니다. 다음 순서에서도 그대로 남습니다 (이 순서에서만 바꾸려면 인스펙터 '소스' 탭)"));
            ui.horizontal_wrapped(|ui| {
                let mut opts = vec![keep.clone(), (wp_core::scenes::FULL.to_string(), tr("전체 화면 (기본)"))];
                opts.extend(user_scenes.iter().map(|x| (x["id"].as_str().unwrap_or("").to_string(), x["name"].as_str().unwrap_or("").to_string())));
                for (o, n) in wp_core::outputs::list(&settings) {
                    let shown = if o == "main" || o == "overlay" { tr(&n) } else { n.clone() };
                    ui.label(&shown);
                    ch |= scene_combo(ui, e, &o, &opts);
                }
            });
            ui.end_row();
        }
        // OBS (Electron obsActHtml): 연결돼 있으면 장면 목록 콤보, 아니면 글 입력
        ui.label("OBS").on_hover_text(tr("이 순서를 송출할 때 OBS 장면을 바꾸거나 방송·녹화를 시작·종료합니다"));
        ui.horizontal_wrapped(|ui| {
            let (enabled, connected, scenes) = crate::trackd_ui::obs_state(&settings);
            ui.label(tr("OBS 장면"));
            if scenes.is_empty() {
                let mut sc = e["actions"]["obsScene"].as_str().unwrap_or("").to_string();
                if ui.add(egui::TextEdit::singleline(&mut sc).desired_width(120.0).hint_text(tr("장면 이름"))).changed() {
                    set_action(e, "obsScene", json!(sc));
                    ch = true;
                }
            } else {
                let cur = e["actions"]["obsScene"].as_str().unwrap_or("").to_string();
                let mut opts = vec![keep.clone()];
                opts.extend(scenes.iter().map(|s| (s.clone(), s.clone())));
                if !cur.is_empty() && !scenes.contains(&cur) {
                    opts.push((cur.clone(), cur.clone()));
                }
                ch |= combo(ui, e, "obsScene", &opts, 140.0);
            }
            ui.label(tr("방송"));
            ch |= combo(ui, e, "obsStream", &[keep.clone(), ("start".into(), tr("시작")), ("stop".into(), tr("끝"))], 80.0);
            ui.label(tr("녹화"));
            ch |= combo(ui, e, "obsRecord", &[keep.clone(), ("start".into(), tr("시작")), ("stop".into(), tr("끝"))], 80.0);
            if !connected && ui.button(if enabled { tr("OBS 연결 안 됨…") } else { tr("OBS 연결하기…") }).on_hover_text(tr("도구 → OBS 원격 조작")).clicked() {
                crate::trackd_ui::open(crate::trackd_ui::Win::Obs);
            }
        });
        ui.end_row();
        // Companion (휴리스틱 평가 12번: '1/0/2' 같은 값을 쉬운 말로 설명)
        ui.label(tr("Companion 단추")).on_hover_text(tr("스트림덱 Companion 의 단추를 이 순서를 송출할 때 대신 눌러 줍니다"));
        ui.horizontal_wrapped(|ui| {
            let mut cp = e["actions"]["companion"].as_str().unwrap_or("").to_string();
            if ui.add(egui::TextEdit::singleline(&mut cp).desired_width(200.0).hint_text(tr("페이지/줄/칸 (예: 1/0/2)")))
                .on_hover_text(tr("Companion 화면의 단추 자리를 '페이지/줄/칸' 숫자로 씁니다. 예: 1/0/2 = 1페이지 맨 윗줄(0) 세 번째 칸(2). 여러 개는 쉼표로: 1/0/2, 1/1/0"))
                .changed() {
                set_action(e, "companion", json!(cp));
                ch = true;
            }
            let ok = !wp_core::companion::parse_locations(&cp).is_empty();
            if ui.add_enabled(ok, egui::Button::new(tr("지금 한 번 눌러 보기")))
                .on_hover_text(tr("설정한 Companion 단추를 지금 눌러 제대로 동작하는지 확인합니다"))
                .on_disabled_hover_text(tr("왼쪽 칸에 단추 자리를 먼저 쓰세요 (예: 1/0/2)"))
                .clicked() {
                let conf = settings["companion"].clone();
                let loc = cp.clone();
                spawn_notify(move || {
                    wp_core::companion::press(&conf, &loc)
                        .map(|r| wp_core::i18n::tr_args("Companion 단추를 눌렀습니다 ({1}개).", &[&r["pressed"].as_u64().unwrap_or(0)]))
                        .map_err(|e| format!("Companion: {e}"))
                });
            }
            ui.label(gray(tr("ATEM · PTZ · 조명 · 음향"))).on_hover_text(tr("ATEM · PTZ · 조명 · 음향을 Companion 으로 (도구 → 스트림덱·외부 단추에서 주소)"));
        });
        ui.end_row();
        // 장비 (X32 · PTZ) — devicepanel.js actionHtml
        let devs: Vec<Value> = settings["devices"].as_array().cloned().unwrap_or_default();
        if devs.is_empty() {
            ui.label(tr("장비"));
            ui.horizontal_wrapped(|ui| {
                if ui.button(tr("X32 · PTZ 카메라 등록…")).clicked() {
                    crate::trackd_ui::open_devices();
                }
                ui.label(gray(tr("음향 콘솔 · 카메라 프리셋을 순서마다")));
            });
            ui.end_row();
        }
        for (k, d) in devs.iter().enumerate() {
            let id = d["id"].as_str().unwrap_or("").to_string();
            let name = d["name"].as_str().unwrap_or("").to_string();
            ui.label(if k == 0 { tr("장비") } else { String::new() });
            ui.horizontal(|ui| {
                ui.label(RichText::new(&name).strong());
                let mut a = dev_action_of(e, &id);
                if ui.add(egui::TextEdit::singleline(&mut a).desired_width(200.0).hint_text(dev_hint(d["kind"].as_str().unwrap_or("")))).changed() {
                    set_device_action(e, &id, &a);
                    ch = true;
                }
                if ui.add_enabled(!a.trim().is_empty(), egui::Button::new(tr("지금 한 번 보내 보기"))).on_disabled_hover_text(tr("보낼 동작을 먼저 쓰세요")).clicked() {
                    let devices = settings["devices"].clone();
                    let (id2, a2, name2) = (id.clone(), a.clone(), name.clone());
                    spawn_notify(move || wp_core::devices::run(&devices, &id2, &a2).map(|_| wp_core::i18n::tr_args("{1}: 보냈습니다.", &[&name2])).map_err(|e| format!("{name2}: {e}")));
                }
            });
            ui.end_row();
        }
    });
    if std::mem::take(&mut st.scroll_once) {
        ui.scroll_to_cursor(Some(egui::Align::BOTTOM)); // 점검(--edit-actions): 구역 끝(OBS · Companion · 장비)이 보이게 한 번 스크롤
    }
    ch
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_action_adds_and_drops() {
        let mut e = json!({ "type": "bible" });
        set_scene_action(&mut e, "overlay", "sc-a");
        assert_eq!(e["actions"], json!({ "scene": { "overlay": "sc-a" } }));
        set_scene_action(&mut e, "main", "full");
        set_scene_action(&mut e, "overlay", "");
        assert_eq!(e["actions"]["scene"], json!({ "main": "full" }));
        set_scene_action(&mut e, "main", "");
        assert!(e.get("actions").is_none());
    }

    #[test]
    fn set_action_removes_empty_and_actions_key() {
        let mut e = json!({ "title": "설교" });
        set_action(&mut e, "logo", json!(""));
        assert!(e.get("actions").is_none(), "빈 값만 넣으면 actions 키를 만들지 않음");
        set_action(&mut e, "timer", json!(30.0));
        set_action(&mut e, "stopMusic", json!(true));
        assert_eq!(e["actions"]["timer"], 30.0);
        set_action(&mut e, "stopMusic", json!(false));
        set_action(&mut e, "timer", json!(0.0));
        assert!(e.get("actions").is_none(), "모두 비우면 actions 키를 지움 (Electron 은 키를 넣지 않음)");
    }

    #[test]
    fn device_actions_replace_and_drop() {
        let mut e = json!({});
        set_device_action(&mut e, "dvx32", "ch1 on");
        set_device_action(&mut e, "dvptz", "2");
        set_device_action(&mut e, "dvx32", "ch1 off");
        assert_eq!(e["actions"]["dev"].as_array().unwrap().len(), 2);
        assert_eq!(dev_action_of(&e, "dvx32"), "ch1 off");
        set_device_action(&mut e, "dvx32", "  ");
        set_device_action(&mut e, "dvptz", "");
        assert!(e.get("actions").is_none());
    }

    #[test]
    fn file_name_decodes_lib_url() {
        assert_eq!(file_name("/lib/file?p=C%3A%5C%EC%9D%8C%EC%95%85%5Cintro.mp3"), "intro.mp3");
        assert_eq!(file_name("D:/a/b.wav"), "b.wav");
    }

    /// 편집칸이 저장할 때 코어 정규화를 거치면 Electron 과 같은 모양이 된다
    #[test]
    fn edited_item_normalizes_like_electron() {
        let mut e = json!({ "type": "title", "title": "설교", "music": "/lib/file?p=a.mp3", "musicVolume": 0.65 });
        set_action(&mut e, "timer", json!(400.0));
        set_action(&mut e, "companion", json!("1/0/2, 9/9"));
        set_device_action(&mut e, "dvx32", " ch1 on ");
        set_action(&mut e, "obsScene", json!("설교"));
        wp_core::actions::normalize_item_actions(&mut e);
        assert_eq!(e["actions"], json!({ "timer": 240, "companion": "1/0/2", "dev": [{ "device": "dvx32", "action": "ch1 on" }], "obsScene": "설교" }));
        assert_eq!(e["musicVolume"], 0.65);
    }
}
