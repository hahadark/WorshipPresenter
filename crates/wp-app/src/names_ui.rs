//! 이름 자막 창 (송출 → 이름 자막): 설교자 · 기도자 이름을 명단에 적어 두고 눌러서 방송(OBS)·예배당 화면 아래에 띄운다.
//! 이름 자막은 하나뿐인 오버레이(id "nametag", 종류 name)이고, 띄울 때마다 이름 · 직분만 바뀐다.

use crate::app::App;
use crate::widgets::{self, Btn, Kind, Size};
use eframe::egui::{self, RichText};
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde_json::{json, Value};
use wp_core::i18n::{tr, tr_args};

#[derive(Default)]
struct St {
    open: bool,
    /// 명단 고치는 중인 사본 (고칠 때마다 저장)
    people: Option<Vec<(String, String)>>,
    quick_name: String,
    quick_role: String,
    note: String,
}

static ST: Lazy<Mutex<St>> = Lazy::new(Default::default);

pub fn open() {
    let mut s = ST.lock();
    s.open = true;
    s.people = None; // 열 때 저장된 명단을 새로 읽는다
}

fn style_ko(s: &str) -> String {
    tr(match s {
        "box" => "상자",
        "plain" => "글자만",
        _ => "막대",
    })
}

fn pos_ko(p: &str) -> String {
    tr(match p {
        "top-left" => "위 왼쪽",
        "top-center" => "위 가운데",
        "top-right" => "위 오른쪽",
        "middle-left" => "가운데 왼쪽",
        "center" => "한가운데",
        "middle-right" => "가운데 오른쪽",
        "bottom-center" => "아래 가운데",
        "bottom-right" => "아래 오른쪽",
        _ => "아래 왼쪽",
    })
}

impl App {
    pub(crate) fn names_window(&mut self, ctx: &egui::Context) {
        let mut open = ST.lock().open;
        if !open {
            return;
        }
        widgets::float_window(ctx, "names", tr("이름 자막"), [520.0, 560.0]).open(&mut open).show(ctx, |ui| {
            let p = crate::theme::pal();
            ui.label(RichText::new(tr("설교자 · 기도자 · 특송 이름을 명단에 적어 두고 [띄우기]를 누르면 화면 아래에 이름과 직분이 나옵니다. 휴대폰 리모컨에서도 누를 수 있습니다.")).small().color(p.muted));
            ui.add_space(4.0);
            // 지금 떠 있는 이름
            let on = self.core.lock().name_tag_on();
            ui.horizontal_wrapped(|ui| match &on {
                Some(n) => {
                    widgets::status_dot(ui, p.live, &tr("지금 보이는 중:"));
                    let who = [n["name"].as_str().unwrap_or(""), n["role"].as_str().unwrap_or("")].iter().filter(|x| !x.is_empty()).cloned().collect::<Vec<_>>().join(" · ");
                    ui.label(RichText::new(who).strong());
                    if ui.add(Btn::new(tr("내리기")).icon("eye-off").kind(Kind::Danger).size(Size::Sm)).clicked() {
                        self.core.lock().hide_name_tag();
                    }
                }
                None => {
                    widgets::muted(ui, &tr("지금 띄운 이름 없음"));
                }
            });
            ui.separator();
            // 명단
            let mut st = ST.lock();
            if st.people.is_none() {
                st.people = Some(self.core.lock().people().iter().map(|x| (x["name"].as_str().unwrap_or("").to_string(), x["role"].as_str().unwrap_or("").to_string())).collect());
            }
            let mut list = st.people.take().unwrap_or_default();
            let mut changed = false;
            let mut show: Option<(String, String)> = None;
            let mut remove: Option<usize> = None;
            let cur = on.as_ref().map(|n| (n["name"].as_str().unwrap_or("").to_string(), n["role"].as_str().unwrap_or("").to_string()));
            egui::ScrollArea::vertical().id_salt("names-list").max_height(240.0).show(ui, |ui| {
                if list.is_empty() {
                    widgets::muted(ui, &tr("명단이 비어 있습니다. 아래 [+ 사람 더하기]로 이름을 적어 두세요."));
                }
                for (i, (name, role)) in list.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        let showing = cur.as_ref().is_some_and(|(n, r)| n == name.trim() && r == role.trim());
                        if ui.add(egui::TextEdit::singleline(name).hint_text(tr("이름")).desired_width(130.0)).changed() {
                            changed = true;
                        }
                        if ui.add(egui::TextEdit::singleline(role).hint_text(tr("직분 · 순서 (예: 담임목사 · 설교)")).desired_width(190.0)).changed() {
                            changed = true;
                        }
                        let b = Btn::new(if showing { tr("보이는 중") } else { tr("띄우기") }).size(Size::Sm).kind(if showing { Kind::Danger } else { Kind::Primary });
                        if ui.add_enabled(!name.trim().is_empty(), b).clicked() {
                            show = Some((name.clone(), role.clone()));
                        }
                        if ui.small_button("✕").on_hover_text(tr("명단에서 빼기")).clicked() {
                            remove = Some(i);
                        }
                    });
                }
            });
            if let Some(i) = remove {
                list.remove(i);
                changed = true;
            }
            if ui.add(Btn::new(tr("사람 더하기")).icon("plus").kind(Kind::Ghost).size(Size::Sm)).clicked() {
                list.push((String::new(), String::new()));
            }
            if changed {
                let v: Vec<Value> = list.iter().map(|(n, r)| json!({ "name": n, "role": r })).collect();
                self.core.lock().save_people(&v);
            }
            st.people = Some(list);
            ui.separator();
            // 명단에 없는 이름 바로 띄우기
            ui.horizontal(|ui| {
                ui.label(tr("바로 띄우기"));
                ui.add(egui::TextEdit::singleline(&mut st.quick_name).hint_text(tr("이름")).desired_width(110.0));
                ui.add(egui::TextEdit::singleline(&mut st.quick_role).hint_text(tr("직분")).desired_width(130.0));
                if ui.add_enabled(!st.quick_name.trim().is_empty(), Btn::new(tr("띄우기")).size(Size::Sm)).clicked() {
                    show = Some((st.quick_name.clone(), st.quick_role.clone()));
                }
            });
            if let Some((n, r)) = show {
                // 이미 같은 사람이 떠 있으면 내린다 (같은 단추로 띄우고 내리기)
                let same = cur.as_ref().is_some_and(|(cn, cr)| cn == n.trim() && cr == r.trim());
                let mut c = self.core.lock();
                if same {
                    c.hide_name_tag();
                } else if let Err(e) = c.show_name_tag(&n, &r) {
                    st.note = e.to_string();
                }
            }
            ui.separator();
            // 모양 (하나뿐인 이름 자막 오버레이를 고친다)
            let ov = self.core.lock().overlays().into_iter().find(|o| o["id"] == wp_core::overlays::NAME_TAG_ID).unwrap_or_else(|| wp_core::overlays::normalize_overlay(&json!({ "id": wp_core::overlays::NAME_TAG_ID, "type": "name" })).unwrap_or_default());
            let mut v = ov.clone();
            let mut ch = false;
            ui.horizontal_wrapped(|ui| {
                ui.label(tr("모양"));
                egui::ComboBox::from_id_salt("name-style").selected_text(style_ko(v["style"].as_str().unwrap_or("bar"))).show_ui(ui, |ui| {
                    for s in wp_core::overlays::NAME_STYLES {
                        if ui.selectable_label(v["style"] == s, style_ko(s)).clicked() {
                            v["style"] = json!(s);
                            ch = true;
                        }
                    }
                });
                ui.label(tr("위치"));
                egui::ComboBox::from_id_salt("name-pos").selected_text(pos_ko(v["position"].as_str().unwrap_or(""))).show_ui(ui, |ui| {
                    for p in wp_core::overlays::POSITIONS {
                        if ui.selectable_label(v["position"] == p, pos_ko(p)).clicked() {
                            v["position"] = json!(p);
                            ch = true;
                        }
                    }
                });
                let mut size = v["size"].as_f64().unwrap_or(52.0);
                if ui.add(egui::DragValue::new(&mut size).range(20.0..=160.0).prefix(tr("크기 "))).changed() {
                    v["size"] = json!(size);
                    ch = true;
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label(tr("강조색"));
                let mut accent = v["accent"].as_str().unwrap_or("#3d7bff").to_string();
                if ui.add(egui::TextEdit::singleline(&mut accent).desired_width(80.0)).changed() {
                    v["accent"] = json!(accent);
                    ch = true;
                }
                ui.label(tr("보이는 곳"));
                for (out, lb) in [("main", "예배당"), ("overlay", "OBS")] {
                    let mut has = v["outputs"].as_array().is_some_and(|a| a.iter().any(|x| x == out));
                    if ui.checkbox(&mut has, tr(lb)).changed() {
                        let mut outs: Vec<Value> = v["outputs"].as_array().cloned().unwrap_or_default();
                        outs.retain(|x| x != out);
                        if has {
                            outs.push(json!(out));
                        }
                        v["outputs"] = json!(outs);
                        ch = true;
                    }
                }
            });
            ui.horizontal(|ui| {
                let mut sec = self.core.lock().name_tag_seconds();
                ui.label(tr("저절로 내리기"));
                if ui.add(egui::DragValue::new(&mut sec).range(0.0..=600.0).suffix(tr("초"))).on_hover_text(tr("0 이면 [내리기]를 누를 때까지 계속 보입니다")).changed() {
                    self.core.lock().settings.update(|d| d["nameTagSeconds"] = json!(sec));
                }
                widgets::muted(ui, &tr("(0 = 계속)"));
            });
            if ch {
                if let Err(e) = self.core.lock().save_overlay(&v) {
                    st.note = e.to_string();
                }
            }
            if !st.note.is_empty() {
                ui.label(RichText::new(tr_args("알림: {1}", &[&st.note])).small().color(p.muted));
            }
        });
        ST.lock().open = open;
    }
}
