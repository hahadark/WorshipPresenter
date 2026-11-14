//! [트랙 C] 설정 창 (일반 · 출력 · 단축키 · 조작 화면) — settings.json 의 같은 칸을 고친다

use crate::form::{check, num, pick, text};
use eframe::egui::{self, RichText};
use serde_json::Value;
use wp_core::core::Shared;

#[derive(Default)]
pub struct SettingsUi {
    pub open: bool,
    tab: u8,
}

/// 단축키 충돌: 같은 키가 두 동작에 있거나 reservedKeys(다른 프로그램 키)와 겹치면 그 동작 id 들
pub fn shortcut_conflicts(s: &Value) -> Vec<(String, String)> {
    let norm = |k: &str| k.replace(' ', "").to_lowercase();
    let reserved: Vec<String> = s["reservedKeys"].as_str().unwrap_or("").split(',').map(|x| norm(x.trim())).filter(|x| !x.is_empty()).collect();
    let mut out = vec![];
    let Some(sc) = s["shortcuts"].as_object() else { return out };
    for (id, k) in sc {
        let k = norm(k.as_str().unwrap_or(""));
        if k.is_empty() {
            continue;
        }
        if let Some((other, _)) = sc.iter().find(|(o, v)| *o != id && norm(v.as_str().unwrap_or("")) == k) {
            out.push((id.clone(), format!("'{}' 와 같은 키", other)));
        } else if reserved.contains(&k) {
            out.push((id.clone(), "다른 프로그램(예약 키)과 겹침".into()));
        }
    }
    out
}

/// 조작 화면 테마·글자 크기 적용 (어둡게가 기본, 밝은 테마는 고를 때만)
pub fn apply_ui_prefs(ctx: &egui::Context, s: &Value) {
    // control.css 테마 5개 (dark·light·contrast·warm·navy) → crate::theme (같은 테마면 아무것도 안 함)
    crate::theme::apply(ctx, crate::theme::ThemeId::from_setting(s["ui"]["theme"].as_str().unwrap_or("dark")));
    let scale = s["ui"]["scale"].as_f64().unwrap_or(100.0).clamp(70.0, 200.0) as f32 / 100.0;
    if (ctx.zoom_factor() - scale).abs() > 0.001 {
        ctx.set_zoom_factor(scale);
    }
}

impl SettingsUi {
    pub fn show(&mut self, ctx: &egui::Context, core: &Shared) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        let mut data = core.lock().settings.get().clone();
        let mut ch = false;
        crate::widgets::float_window(ctx, "settings", wp_core::i18n::tr("설정"), [640.0, 560.0]).open(&mut open).show(ctx, |ui| {
            ui.horizontal(|ui| {
                for (i, n) in ["일반", "출력", "단축키", "조작 화면"].iter().enumerate() {
                    if ui.selectable_label(self.tab == i as u8, wp_core::i18n::tr(n)).clicked() {
                        self.tab = i as u8;
                    }
                }
            });
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("set-grid").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| match self.tab {
                    0 => {
                        ch |= check(ui, &mut data, &["previewMode"], "미리보기를 거쳐 송출 (끄면 누른 것을 바로 송출)");
                        ui.end_row();
                        // 빠른 설정·검색줄과 같은 말 (휴리스틱 평가 6번). Electron settings.js 문구 그대로:
                        // 검색줄 '자동 · 장절 · 본문' = 기본 검색 방식, 빠른 설정 '분할: …' = 분할 방식, '이동: …' = 범위 끝에서
                        ch |= check(ui, &mut data, &["cueAdvance"], "F6 을 누르면 다음 순서로 넘어가 바로 송출 (F5 는 이전 순서의 마지막 슬라이드)");
                        ui.end_row();
                        ch |= check(ui, &mut data, &["cueEdgeHide"], "그 전에 한 번은 글자만 숨기기 (모든 순서 — 순서마다 따로 켤 수도 있음)");
                        ui.end_row();
                        ch |= pick(ui, &mut data, &["searchMode"], "기본 검색 방식", &[("auto", "자동 (숫자가 있으면 장절, 없으면 본문)"), ("reference", "장절만"), ("text", "본문 단어만")]);
                        ui.end_row();
                        ch |= pick(ui, &mut data, &["split", "mode"], "분할 방식", &[("auto", "화면 공간에 맞춰 자동"), ("count", "지정한 절 수만큼")]);
                        ui.end_row();
                        let by_count = data["split"]["mode"] == "count";
                        ui.add_enabled_ui(by_count, |ui| { ch |= num(ui, &mut data, &["split", "count"], "분할 절 수", 1.0, 20.0, 1.0); })
                            .response.on_disabled_hover_text(wp_core::i18n::tr("분할 방식을 '지정한 절 수만큼'으로 고르면 쓸 수 있습니다"));
                        ui.end_row();
                        ch |= pick(ui, &mut data, &["navMode"], "이동 (F5·F6 범위 끝에서)", &[("range", "선택한 범위 안에서만 이동"), ("continuous", "다음(이전) 절로 계속 이어가기")]);
                        ui.end_row();
                    }
                    1 => {
                        ch |= check(ui, &mut data, &["output", "autoOnSend"], "송출하면 예배당 화면 자동 켜기");
                        ui.end_row();
                        ch |= check(ui, &mut data, &["output", "linkObs"], "예배당 화면을 끄면 OBS 화면도 숨기기 (다시 켜거나 송출하면 다시 보임)");
                        ui.end_row();
                        ch |= check(ui, &mut data, &["output", "cursorHidden"], "예배당 화면에서 마우스 커서 숨김");
                        ui.end_row();
                        ch |= check(ui, &mut data, &["output", "obsPip"], "OBS 자막: 그림 슬라이드를 오른쪽 위에 작게");
                        ui.end_row();
                        ch |= text(ui, &mut data, &["logoScreenImage"], "로고 화면 그림 (비우면 템플릿 로고)", 300.0);
                        ui.end_row();
                    }
                    2 => {
                        // [트랙 C] 전역 단축키 (트랙 D winkeys) · 한글 입력 자동 전환
                        ch |= check(ui, &mut data, &["globalShortcuts"], "전역 단축키 (창이 안 보여도 F5·F6 등)");
                        ui.end_row();
                        let ws = wp_core::winkeys::status();
                        ui.label(wp_core::i18n::tr("등록 상태"));
                        let reg: Vec<String> = ws["registered"].as_array().into_iter().flatten().map(|x| x.as_str().map(String::from).unwrap_or_else(|| x.to_string())).collect();
                        let fail: Vec<String> = ws["failed"].as_array().into_iter().flatten().map(|x| x.as_str().map(String::from).unwrap_or_else(|| x.to_string())).collect();
                        ui.label(RichText::new(if !ws["available"].as_bool().unwrap_or(false) { "이 운영체제에서는 쓸 수 없음".to_string() } else if !ws["enabled"].as_bool().unwrap_or(false) { "꺼짐".to_string() } else { format!("등록 {}개{}{}", reg.len(), if fail.is_empty() { String::new() } else { format!(" · 다른 프로그램이 쓰는 키: {}", fail.join(", ")) }, if reg.is_empty() { String::new() } else { format!(" ({})", reg.join(", ")) }) }).small().color(if fail.is_empty() { crate::theme::pal().muted } else { crate::theme::pal().warn }));
                        ui.end_row();
                        ch |= check(ui, &mut data, &["imeAuto"], "가사·공지·즉석 자막 칸에 들어가면 한글 입력으로 (윈도우)");
                        ui.end_row();
                        let labels = wp_core::settings::SHORTCUTS["labels"].clone();
                        let conflicts = shortcut_conflicts(&data);
                        let keys: Vec<String> = data["shortcuts"].as_object().map(|o| o.keys().cloned().collect()).unwrap_or_default();
                        for id in keys {
                            let label = labels[&id].as_str().unwrap_or(&id).to_string();
                            ch |= text(ui, &mut data, &["shortcuts", &id], &label, 160.0);
                            if let Some((_, why)) = conflicts.iter().find(|c| c.0 == id) {
                                ui.label(RichText::new(why).small().color(crate::theme::pal().warn));
                            }
                            ui.end_row();
                        }
                        ch |= text(ui, &mut data, &["reservedKeys"], "다른 프로그램 키 (쉼표로)", 300.0);
                        ui.end_row();
                        if ui.button(wp_core::i18n::tr("기본 단축키로")).clicked() {
                            data["shortcuts"] = wp_core::settings::SHORTCUTS["defaults"].clone();
                            ch = true;
                        }
                        ui.end_row();
                    }
                    _ => {
                        ch |= pick(ui, &mut data, &["ui", "theme"], "테마", &[("dark", "어둡게 (기본)"), ("contrast", "고대비"), ("light", "밝게"), ("warm", "따뜻하게"), ("navy", "푸른 밤"), ("manna", "만나로 (밝음)")]);
                        ui.end_row();
                        ch |= num(ui, &mut data, &["ui", "scale"], "글자 크기(%)", 70.0, 200.0, 5.0);
                        ui.end_row();
                    }
                });
                if self.tab == 2 {
                    ui.label(RichText::new(wp_core::i18n::tr("Rust 판은 아직 F5·F6·F9·F10·Ctrl+K 등 기본 키로 동작합니다. 바꾼 키는 저장만 됩니다.")).small().color(crate::theme::pal().muted));
                }
            });
        });
        if ch {
            let mut c = core.lock();
            c.settings.update(|d| *d = data);
            c.emit(wp_core::core::Ev::Live);
            c.emit(wp_core::core::Ev::Ui);
        }
        self.open = open;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn conflicts() {
        let s = json!({ "shortcuts": { "a": "F5", "b": "f5", "c": "Ctrl+K" }, "reservedKeys": "ctrl + k" });
        let c = shortcut_conflicts(&s);
        assert_eq!(c.len(), 3);
        assert!(c.iter().any(|x| x.0 == "c" && x.1.contains("예약")));
    }
}
