//! [설정 센터] 단축키 쪽: 기능 이름·키로 찾기 · 기본값 표시 · 충돌 요약 · 하나씩/전부 기본값으로
//!
//! 여기서 바꾼 키는 바로 듣는다 — App::shortcuts · trackb_shortcuts · track_c · parity2_ui 가 프레임마다
//! settings.shortcuts 를 읽어 `keys::pressed` 로 본다 (합성 입력 시험 settings_center_tests 가 확인).

use super::{SettingsUi, View};
use crate::widgets::{mark, Btn, Kind as BtnKind, Size};
use eframe::egui::{self, Key, RichText};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use wp_core::core::Shared;
use wp_core::i18n::{tr, tr_args};

/// 보여 주는 차례 (송출 → 미리보기 → 큐시트 → 창)
const ORDER: [&str; 23] = [
    "prevSlide", "nextSlide", "sendPreview", "sendNow", "toggleClear", "toggleOutputWindow", "outputOff",
    "previewPrevSlide", "previewNextSlide", "previewPrevVerse", "previewNextVerse", "previewChapterStart",
    "cuePrev", "cueNext", "cueEdit", "toggleEditor",
    "focusSearch", "openBooks", "openHistory", "quickText", "messages", "webLyrics", "printCue",
];

#[derive(Default)]
pub struct KeysState {
    /// 기능 이름·키로 거르는 글
    pub filter: String,
    /// 고치는 중인 글 (Enter·칸을 떠날 때 저장). 틀린 키는 저장하지 않고 여기 남아 오류를 보여 준다
    drafts: BTreeMap<String, String>,
}

impl KeysState {
    pub fn clear_drafts(&mut self) {
        self.drafts.clear();
    }
    /// 그 칸에 남아 있는(아직 저장되지 않은) 글
    #[cfg(test)]
    pub fn draft(&self, id: &str) -> Option<&str> {
        self.drafts.get(id).map(|s| s.as_str())
    }
}

fn key_of(t: &str) -> Option<Key> {
    if let Some(k) = Key::from_name(t) {
        return Some(k);
    }
    let l = t.to_lowercase();
    let alias = match l.as_str() {
        "esc" => "Escape",
        "return" => "Enter",
        "spacebar" => "Space",
        "up" => "ArrowUp",
        "down" => "ArrowDown",
        "left" => "ArrowLeft",
        "right" => "ArrowRight",
        "pgup" => "PageUp",
        "pgdn" | "pgdown" => "PageDown",
        "del" => "Delete",
        "ins" => "Insert",
        _ => "",
    };
    if !alias.is_empty() {
        return Key::from_name(alias);
    }
    // 대소문자만 다른 이름 (f5 → F5, pagedown → PageDown, k → K)
    Key::ALL.iter().copied().find(|k| k.name().to_lowercase() == l)
}

/// 설정 파일·원본(Electron)과 같은 키 이름: 방향키는 'ArrowLeft' 꼴 (egui 이름은 'Left')
fn key_name(k: Key) -> &'static str {
    match k {
        Key::ArrowLeft => "ArrowLeft",
        Key::ArrowRight => "ArrowRight",
        Key::ArrowUp => "ArrowUp",
        Key::ArrowDown => "ArrowDown",
        _ => k.name(),
    }
}

/// 사람이 쓴 단축키 글을 프로그램이 알아듣는 한 가지 모양으로: "ctrl + shift+f" → "Ctrl+Shift+F".
/// 모르는 키면 None. 돌려준 글은 `keys::parse` 가 반드시 읽는다
pub fn normalize(s: &str) -> Option<String> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let plus = s.ends_with('+'); // "Ctrl++" 나 "+" = 더하기 키
    let body = if plus { &s[..s.len() - 1] } else { s };
    let mut parts: Vec<&str> = body.split('+').map(str::trim).filter(|x| !x.is_empty()).collect();
    let key = if plus { Key::Plus } else { key_of(parts.pop()?)? };
    let (mut c, mut a, mut sh) = (false, false, false);
    for p in parts {
        match p.to_lowercase().as_str() {
            "ctrl" | "control" | "cmd" | "command" | "commandorcontrol" => c = true,
            "alt" | "option" => a = true,
            "shift" => sh = true,
            _ => return None,
        }
    }
    let out = format!("{}{}{}{}", if c { "Ctrl+" } else { "" }, if a { "Alt+" } else { "" }, if sh { "Shift+" } else { "" }, key_name(key));
    crate::keys::parse(&out).map(|_| out)
}

fn label_of(id: &str) -> String {
    wp_core::settings::SHORTCUTS["labels"][id].as_str().map(tr).unwrap_or_else(|| id.to_string())
}

fn default_of(id: &str) -> String {
    wp_core::settings::SHORTCUTS["defaults"][id].as_str().unwrap_or("").to_string()
}

/// 보여 줄 동작 id: 정한 차례 + 설정에만 있는 것(뒤에)
fn ids(s: &Value) -> Vec<String> {
    let mut out: Vec<String> = ORDER.iter().map(|x| x.to_string()).collect();
    for k in s["shortcuts"].as_object().into_iter().flat_map(|o| o.keys()) {
        if !out.contains(k) {
            out.push(k.clone());
        }
    }
    out
}

fn hit(id: &str, cur: &str, q: &str) -> bool {
    let q = q.trim().to_lowercase();
    if q.is_empty() {
        return true;
    }
    let raw = wp_core::settings::SHORTCUTS["labels"][id].as_str().unwrap_or("");
    let hay = format!("{} {} {} {}", raw, label_of(id), cur, default_of(id)).to_lowercase();
    q.split_whitespace().all(|w| hay.contains(w))
}

/// 설정 검색에서 맞는 단축키: [(id, 보일 이름, 지금 키)]
pub fn search(s: &Value, q: &str) -> Vec<(String, String, String)> {
    if q.trim().is_empty() {
        return vec![];
    }
    ids(s).into_iter().filter_map(|id| {
        let cur = s["shortcuts"][&id].as_str().unwrap_or("").to_string();
        hit(&id, &cur, q).then(|| (id.clone(), label_of(&id), cur))
    }).collect()
}

impl SettingsUi {
    pub(super) fn keys_page(&mut self, ui: &mut egui::Ui, core: &Shared, v: &View) {
        let p = crate::theme::pal();
        crate::widgets::group_head(ui, &tr("기능별 키"));
        ui.label(RichText::new(tr("키 칸에 'F6' · 'Ctrl+Shift+F' · 'Alt+ArrowLeft' 처럼 쓰고 Enter 를 누르면 바로 적용됩니다. 칸을 비우면 그 단축키를 끕니다.")).small().color(p.muted));
        // 문제 요약
        let problems = super::shortcut_conflicts(&v.s);
        if problems.is_empty() {
            ui.label(RichText::new(tr("겹치는 키 없음")).small().color(p.ok_text));
        } else {
            let list: Vec<String> = problems.iter().map(|(id, why)| format!("{} ({})", label_of(id), why)).collect();
            let r = ui.label(RichText::new(tr_args("고칠 것 {1}개: {2}", &[&problems.len(), &list.join(" · ")])).color(p.warn));
            mark("set-key-problems", r.rect);
        }
        let all = ids(&v.s);
        let changed: Vec<String> = all.iter().filter(|id| !default_of(id).is_empty() && v.s["shortcuts"][id.as_str()].as_str().unwrap_or("") != default_of(id)).cloned().collect();
        ui.horizontal(|ui| {
            crate::icons::show(ui, "search", 14.0, p.muted);
            let r = ui.add(egui::TextEdit::singleline(&mut self.keys.filter).hint_text(tr("기능 이름이나 키로 찾기 (예: 송출, 미리보기, F6, Ctrl)")).desired_width(280.0));
            mark("set-key-filter", r.rect);
            if !self.keys.filter.is_empty() && ui.add(Btn::new(tr("모두 보기")).size(Size::Sm)).clicked() {
                self.keys.filter.clear();
            }
            ui.label(RichText::new(tr_args("기본값과 다른 키 {1}개", &[&changed.len()])).small().color(p.muted));
        });
        let filter = self.keys.filter.clone();
        let mut shown = 0;
        egui::Grid::new("set-keys").num_columns(4).spacing([12.0, 6.0]).striped(true).show(ui, |ui| {
            for h in ["기능", "키", "기본값", ""] {
                ui.label(RichText::new(tr(h)).small().color(p.muted));
            }
            ui.end_row();
            for id in &all {
                let cur = v.s["shortcuts"][id.as_str()].as_str().unwrap_or("").to_string();
                if !hit(id, &cur, &filter) {
                    continue;
                }
                shown += 1;
                let (label, def) = (label_of(id), default_of(id));
                ui.label(&label);
                let mut draft = self.keys.drafts.get(id).cloned().unwrap_or_else(|| cur.clone());
                let r = ui.add_sized([150.0, 26.0], egui::TextEdit::singleline(&mut draft).hint_text(tr("없음 (끔)")));
                mark(&format!("set-key-{id}"), r.rect);
                if r.changed() {
                    self.keys.drafts.insert(id.clone(), draft.clone());
                }
                let mut bad = false;
                if self.keys.drafts.contains_key(id) && !r.has_focus() {
                    // 칸을 떠났다(Enter 포함): 맞는 키면 한 가지 모양으로 고쳐 저장, 틀리면 저장하지 않고 남겨 둔다
                    let t = draft.trim();
                    if t.is_empty() {
                        self.keys.drafts.remove(id);
                        self.set(core, &["shortcuts", id], json!(""), &label);
                    } else if let Some(n) = normalize(t) {
                        self.keys.drafts.remove(id);
                        self.set(core, &["shortcuts", id], json!(n), &label);
                    } else {
                        bad = true;
                    }
                }
                ui.label(RichText::new(if def.is_empty() { "—".to_string() } else { def.clone() }).color(p.muted));
                ui.horizontal(|ui| {
                    if bad {
                        let e = ui.label(RichText::new(tr_args("'{1}' 은 알 수 없는 키라 저장하지 않았습니다. 예: F6, Ctrl+K, PageDown", &[&draft.trim()])).small().color(p.danger_text));
                        mark(&format!("set-key-err-{id}"), e.rect);
                    } else if let Some((_, why)) = problems.iter().find(|c| &c.0 == id) {
                        ui.label(RichText::new(why).small().color(p.warn));
                    }
                    if (cur != def || bad) && !def.is_empty() {
                        let b = ui.add(Btn::new(tr("기본값")).size(Size::Sm).tooltip(tr_args("이 키만 기본값({1})으로 돌립니다", &[&def])));
                        mark(&format!("set-key-def-{id}"), b.rect);
                        if b.clicked() {
                            self.keys.drafts.remove(id);
                            self.set(core, &["shortcuts", id], json!(def), &label);
                        }
                    }
                });
                ui.end_row();
            }
        });
        if shown == 0 {
            ui.label(RichText::new(tr_args("'{1}' 에 맞는 단축키가 없습니다.", &[&filter.trim()])).color(p.muted));
        }
        // 전부 기본값으로 (확인)
        ui.add_space(6.0);
        let cid = egui::Id::unique("set-keys-reset");
        if crate::widgets::confirm_armed(ui, cid) && !changed.is_empty() {
            let names: Vec<String> = changed.iter().map(|id| format!("{} ({} → {})", label_of(id), v.s["shortcuts"][id.as_str()].as_str().filter(|x| !x.is_empty()).map(String::from).unwrap_or_else(|| tr("없음 (끔)")), default_of(id))).collect();
            ui.label(RichText::new(tr_args("바꾼 단축키 {1}개를 기본값으로 돌립니다: {2}", &[&changed.len(), &names.join(" · ")])).color(p.warn));
            ui.horizontal(|ui| {
                let yes = ui.add(Btn::new(tr("모두 기본값으로 돌리기")).kind(BtnKind::Warn).size(Size::Sm));
                mark("set-keys-reset-yes", yes.rect);
                let no = ui.add(Btn::new(tr("그만두기")).size(Size::Sm));
                mark("set-keys-reset-no", no.rect);
                if yes.clicked() {
                    let changes = changed.iter().map(|id| (vec!["shortcuts".to_string(), id.clone()], json!(default_of(id)))).collect();
                    self.keys.drafts.clear();
                    self.set_many(core, changes, &tr("모든 단축키"));
                    crate::widgets::confirm_arm(ui, cid, false);
                } else if no.clicked() {
                    crate::widgets::confirm_arm(ui, cid, false);
                }
            });
        } else {
            let tip = if changed.is_empty() { tr("모든 단축키가 기본값입니다") } else { tr("누르면 무엇이 바뀌는지 보여 주고 한 번 더 묻습니다") };
            let r = ui.add(Btn::new(tr("모든 단축키를 기본값으로…")).icon("undo-2").size(Size::Sm).enabled(!changed.is_empty()).tooltip(tip));
            mark("set-keys-reset", r.rect);
            if r.clicked() {
                crate::widgets::confirm_arm(ui, cid, true);
            }
        }
        ui.add_space(4.0);
        ui.label(RichText::new(tr("메뉴와 송출 단추에는 현재 설정한 키가 표시됩니다. Space(다음) · ← → · Enter · Alt+1~9(매크로)는 고정 키라 여기서 바꾸지 않습니다.")).small().color(p.muted2));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 여러_모양으로_쓴_키를_한_모양으로() {
        assert_eq!(normalize("ctrl + shift+f").as_deref(), Some("Ctrl+Shift+F"));
        assert_eq!(normalize("Control+K").as_deref(), Some("Ctrl+K"));
        assert_eq!(normalize("CMD+k").as_deref(), Some("Ctrl+K"));
        assert_eq!(normalize("f6").as_deref(), Some("F6"));
        assert_eq!(normalize("alt+left").as_deref(), Some("Alt+ArrowLeft"));
        assert_eq!(normalize("esc").as_deref(), Some("Escape"));
        assert_eq!(normalize("pgdn").as_deref(), Some("PageDown"));
        assert_eq!(normalize("Shift+Ctrl+F"), normalize("Ctrl+Shift+F"), "꾸밈 키 차례는 상관없다");
        assert!(normalize("Hyper+Q").is_none());
        assert!(normalize("Ctrl+").is_some(), "Ctrl 과 더하기 키");
        assert!(normalize("Ctrl+없는키").is_none());
        assert!(normalize("  ").is_none());
    }

    #[test]
    fn 기본_단축키는_모두_읽히고_표에_빠진_것이_없다() {
        let defs = wp_core::settings::SHORTCUTS["defaults"].as_object().unwrap();
        for (id, k) in defs {
            let k = k.as_str().unwrap();
            assert_eq!(normalize(k).as_deref(), Some(k), "기본 키가 이미 표준 모양이어야 한다: {id}");
            assert!(ORDER.contains(&id.as_str()), "표 차례에 없는 동작: {id}");
        }
        assert_eq!(ORDER.len(), defs.len());
        assert!(super::super::shortcut_conflicts(&json!({ "shortcuts": defs, "reservedKeys": "" })).is_empty(), "기본 키끼리는 겹치지 않는다");
    }

    #[test]
    fn 기능_이름과_키로_찾는다() {
        let s = json!({ "shortcuts": wp_core::settings::SHORTCUTS["defaults"].clone() });
        let got = |q: &str| search(&s, q).into_iter().map(|x| x.0).collect::<Vec<_>>();
        assert_eq!(got("F6"), vec!["nextSlide"]);
        assert!(got("미리보기").contains(&"sendPreview".to_string()));
        assert!(got("ctrl").len() >= 8);
        assert!(got("").is_empty());
    }
}
