//! [트랙 C] 설정 센터 — 왼쪽 분류 · 오른쪽 상세 · 위 검색. settings.json 의 같은 칸을 고친다.
//!
//! - 항목 목록·검색·한 칸 고치기는 `settings_items.rs`, 단축키 목록은 `settings_keys.rs`.
//! - 값은 바꾼 칸 하나만 코어에 쓴다(patch). 화면에 들고 있던 설정 사본을 통째로 되쓰지 않는다 —
//!   그러면 그 사이 다른 창이 바꾼 템플릿·설정이 옛 값으로 돌아간다.
//! - 원래 따로 있는 창(출력 설정·OBS·리모컨·동기화 등)은 여기서 다시 만들지 않고,
//!   지금 상태 한 줄과 여는 단추만 둔다. 누른 것은 `pending` 에 메뉴 id 로 쌓이고 track_c 가 연다.

#[path = "settings_items.rs"]
pub mod items;
#[path = "settings_keys.rs"]
pub mod keys_page;

use crate::widgets::{mark, Btn, Kind as BtnKind, Size};
use eframe::egui::{self, RichText};
use items::{Cat, Item, Kind, ITEMS};
use serde_json::{json, Value};
use wp_core::core::Shared;
use wp_core::i18n::{tr, tr_args};

/// 설정 화면이 코어 밖에서 알아야 하는 것 (track_c 가 App 에서 뽑아 준다)
#[derive(Default, Clone)]
pub struct Env {
    pub out_running: bool,
    /// 지금 고른 출력 모니터 이름
    pub monitor: String,
    pub data_dir: std::path::PathBuf,
}

/// 되돌리기 한 번에 되돌릴 칸들: (경로, 바꾸기 전 값)
struct Undo {
    label: String,
    entries: Vec<(Vec<String>, Value)>,
}

/// 프레임마다 뜨는 가벼운 사본 (템플릿 본문·송출 기록 같은 큰 것은 빼고)
pub(crate) struct View {
    pub s: Value,
    pub tpls: Vec<(String, String)>,
    pub port: u16,
    pub obs_full: bool,
    pub env: Env,
    pub info: Info,
}

/// 몇 초에 한 번만 다시 세는 것 (DB·폴더를 읽으므로)
#[derive(Default, Clone)]
pub(crate) struct Info {
    versions: usize,
    version_name: String,
    commentaries: usize,
    backups: usize,
}

#[derive(Default)]
pub struct SettingsUi {
    pub open: bool,
    pub cat: Cat,
    pub query: String,
    /// 검색 결과에서 고른 항목: (이름, 고른 시각) — 잠깐 강조하고 그 자리로 스크롤
    focus: Option<(&'static str, f64)>,
    scroll_to: bool,
    /// 마지막으로 한 일: (글, 오류인가)
    status: Option<(String, bool)>,
    undo: Option<Undo>,
    /// 눌러서 열 창 (메뉴 id). track_c::windows 가 꺼내 handle_menu 로 넘긴다
    pub pending: Vec<&'static str>,
    pub keys: keys_page::KeysState,
    /// 막대를 끄는 동안의 값 (놓을 때 적용: 글자 크기는 끄는 중에 바꾸면 화면이 같이 움직여 못 맞춘다)
    slider: Option<(&'static str, f64)>,
    info: Option<(f64, Info)>,
    /// 시험용: 파일 고르기 창 대신 쓸 파일
    #[cfg(test)]
    pub test_pick: Option<std::path::PathBuf>,
}

/// 단축키 문제: 같은 키가 두 동작에 있음 · reservedKeys(다른 프로그램 키)와 겹침 · 알 수 없는 키.
/// 'Ctrl'·'Control'·'Cmd', 대소문자, 띄어쓰기가 달라도 같은 키로 본다. [(동작 id, 까닭)]
pub fn shortcut_conflicts(s: &Value) -> Vec<(String, String)> {
    let canon = |k: &str| keys_page::normalize(k).map(|x| x.to_lowercase());
    let reserved: Vec<String> = s["reservedKeys"].as_str().unwrap_or("").split(',').filter_map(canon).collect();
    let labels = &wp_core::settings::SHORTCUTS["labels"];
    let mut out = vec![];
    let Some(sc) = s["shortcuts"].as_object() else { return out };
    for (id, k) in sc {
        let k = k.as_str().unwrap_or("").trim();
        if k.is_empty() {
            continue; // 비운 것은 '끔'
        }
        let Some(c) = canon(k) else {
            out.push((id.clone(), tr("알 수 없는 키 — 이 단축키는 듣지 않습니다")));
            continue;
        };
        if let Some((other, _)) = sc.iter().find(|(o, v)| *o != id && canon(v.as_str().unwrap_or("")).as_deref() == Some(c.as_str())) {
            let name = labels[other].as_str().map(tr).unwrap_or_else(|| other.clone());
            out.push((id.clone(), tr_args("'{1}' 와 같은 키", &[&name])));
        } else if reserved.contains(&c) {
            out.push((id.clone(), tr("다른 프로그램(예약 키)과 겹침")));
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

/// 사본에서 빼는 큰 칸
const HEAVY: [&str; 10] = ["templates", "history", "workspaces", "dockLayout", "droppedFiles", "overlays", "macros", "schedule", "stagePresets", "messageTemplates"];

fn file_name(url: &str) -> String {
    let tail = url.rsplit(['/', '\\', '=']).next().unwrap_or(url);
    percent_encoding::percent_decode_str(tail).decode_utf8_lossy().rsplit(['/', '\\']).next().unwrap_or("").to_string()
}

impl SettingsUi {
    /// 분류를 골라 연다 (점검 인자 · 다른 창에서 바로 가기)
    pub fn open_cat(&mut self, cat: Cat) {
        self.open = true;
        self.cat = cat;
        self.query.clear();
    }

    /// 항목으로 간다: 그 분류를 열고, 그 줄을 잠깐 강조하고, 보이게 스크롤
    pub fn goto(&mut self, id: &str, now: f64) {
        if let Some(it) = items::find(id) {
            self.cat = it.cat;
            self.query.clear();
            self.focus = Some((it.id, now));
            self.scroll_to = true;
        }
    }

    #[cfg(test)]
    pub fn focused(&self) -> Option<&'static str> {
        self.focus.map(|f| f.0)
    }

    #[cfg(test)]
    pub fn status_text(&self) -> String {
        self.status.as_ref().map(|s| s.0.clone()).unwrap_or_default()
    }

    fn view(&mut self, core: &Shared, env: &Env, now: f64) -> View {
        let c = core.lock();
        let s = c.settings.get();
        let mut o = serde_json::Map::new();
        for (k, v) in s.as_object().into_iter().flatten() {
            if !HEAVY.contains(&k.as_str()) {
                o.insert(k.clone(), v.clone());
            }
        }
        let tpls = s["templates"].as_array().into_iter().flatten().map(|t| (t["id"].as_str().unwrap_or("").to_string(), wp_core::i18n::tr_name(t["name"].as_str().unwrap_or("")))).collect();
        if self.info.as_ref().is_none_or(|(t, _)| now - t > 3.0 || now < *t) {
            let vs = c.store.list_versions();
            let cur = s["versionId"].as_str().unwrap_or("");
            let info = Info {
                versions: vs.len(),
                version_name: vs.iter().find(|v| v.id == cur).map(|v| v.name.clone()).unwrap_or_default(),
                commentaries: wp_core::commentary::list(&c.store.db).len(),
                backups: wp_core::sync::backups_in(&env.data_dir).len(),
            };
            self.info = Some((now, info));
        }
        View { s: Value::Object(o), tpls, port: c.port, obs_full: c.obs_full, env: env.clone(), info: self.info.as_ref().map(|i| i.1.clone()).unwrap_or_default() }
    }

    /// 한 칸을 바꾸고, 되돌리기와 상태 글을 남긴다
    pub(crate) fn set(&mut self, core: &Shared, path: &[&str], v: Value, label: &str) {
        let old = items::patch(core, path, v.clone());
        if old == v {
            return;
        }
        let p: Vec<String> = path.iter().map(|x| x.to_string()).collect();
        match &mut self.undo {
            // 같은 칸을 이어서 바꾸면 처음 값으로 되돌아가게 둔다
            Some(u) if u.entries.len() == 1 && u.entries[0].0 == p => u.label = label.to_string(),
            _ => self.undo = Some(Undo { label: label.to_string(), entries: vec![(p, old)] }),
        }
        self.status = Some((tr_args("적용됨 · 자동 저장: {1}", &[&label]), false));
    }

    /// 여러 칸을 한 번에 바꾼다 (기본값 복원). 되돌리기는 이 묶음 전체
    pub(crate) fn set_many(&mut self, core: &Shared, changes: Vec<(Vec<String>, Value)>, label: &str) {
        let mut entries = vec![];
        for (p, v) in changes {
            let path: Vec<&str> = p.iter().map(|x| x.as_str()).collect();
            let old = items::patch(core, &path, v.clone());
            if old != v {
                entries.push((p, old));
            }
        }
        if entries.is_empty() {
            return;
        }
        self.status = Some((tr_args("기본값으로 돌렸습니다: {1} ({2}개)", &[&label, &entries.len()]), false));
        self.undo = Some(Undo { label: label.to_string(), entries });
    }

    fn do_undo(&mut self, core: &Shared) {
        let Some(u) = self.undo.take() else { return };
        for (p, old) in &u.entries {
            let path: Vec<&str> = p.iter().map(|x| x.as_str()).collect();
            items::patch(core, &path, old.clone());
        }
        self.keys.clear_drafts();
        self.slider = None;
        self.status = Some((tr_args("되돌렸습니다: {1}", &[&u.label]), false));
    }

    pub(crate) fn fail(&mut self, msg: String) {
        self.status = Some((msg, true));
    }

    pub fn show(&mut self, ctx: &egui::Context, core: &Shared, env: &Env) {
        if !self.open {
            return;
        }
        let now = ctx.input(|i| i.time);
        let v = self.view(core, env, now);
        let scr = ctx.content_rect();
        // 화면 안에 들어가게: 넓으면 980×700, 좁으면 화면에 맞춘다 (글자 크기를 키우면 화면이 좁아진다)
        let w = 980f32.min(scr.width() - 24.0).max(420.0);
        let h = 700f32.min(scr.height() - crate::widgets::FLOAT_TOP - 16.0).max(320.0);
        let mut open = self.open;
        let r = crate::widgets::float_window(ctx, "settings-center", tr("설정"), [w, h])
            .open(&mut open)
            .min_width(w.min(640.0))
            .min_height(h.min(360.0))
            .max_width((scr.width() - 8.0).max(w))
            .max_height((scr.height() - 8.0).max(h))
            .show(ctx, |ui| {
                self.top(ui, core);
                ui.separator();
                let body_h = ui.available_height();
                ui.horizontal_top(|ui| {
                    ui.allocate_ui_with_layout(egui::vec2(190.0, body_h), egui::Layout::top_down_justified(egui::Align::Min), |ui| self.cats(ui));
                    ui.separator();
                    ui.vertical(|ui| {
                        egui::ScrollArea::vertical().id_salt("set-body").auto_shrink([false, false]).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            if self.query.trim().is_empty() {
                                self.page(ui, core, &v, now);
                            } else {
                                self.results(ui, &v, now);
                            }
                        });
                    });
                });
            });
        if let Some(r) = r {
            mark("set-window", r.response.rect);
        }
        self.open = open;
    }

    /// 위 줄: 검색 · 상태 · 되돌리기
    fn top(&mut self, ui: &mut egui::Ui, core: &Shared) {
        let p = crate::theme::pal();
        ui.horizontal(|ui| {
            crate::icons::show(ui, "search", 16.0, p.muted);
            let r = ui.add(egui::TextEdit::singleline(&mut self.query).hint_text(tr("설정 찾기 — 이름 · 설명 · 관련 낱말 (예: 글자 크기, 모니터, OBS, F6)")).desired_width(330.0));
            mark("set-search", r.rect);
            if !self.query.is_empty() && ui.add(Btn::new(tr("지우기")).size(Size::Sm).tooltip(tr("검색을 지우고 분류로 돌아갑니다"))).clicked() {
                self.query.clear();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(label) = self.undo.as_ref().map(|u| u.label.clone()) {
                    let r = ui.add(Btn::new(tr("되돌리기")).icon("undo-2").size(Size::Sm).tooltip(tr_args("방금 바꾼 것을 바꾸기 전으로: {1}", &[&label])));
                    mark("set-undo", r.rect);
                    if r.clicked() {
                        self.do_undo(core);
                    }
                }
                let (txt, color) = match &self.status {
                    Some((t, true)) => (t.clone(), p.danger_text),
                    Some((t, false)) => (format!("✓ {t}"), p.ok_text),
                    None => (tr("바꾸면 바로 적용되고 자동으로 저장됩니다"), p.muted),
                };
                ui.add(egui::Label::new(RichText::new(txt).small().color(color)).truncate());
            });
        });
    }

    /// 왼쪽 분류
    fn cats(&mut self, ui: &mut egui::Ui) {
        let searching = !self.query.trim().is_empty();
        for cat in Cat::ALL {
            let r = ui.selectable_label(!searching && self.cat == cat, RichText::new(tr(cat.name())).size(14.0)).on_hover_text(tr(cat.note()));
            mark(&format!("set-cat-{}", cat.key()), r.rect);
            if r.clicked() {
                self.cat = cat;
                self.query.clear();
                self.focus = None;
            }
        }
    }

    /// 검색 결과: 누르면 그 항목으로
    fn results(&mut self, ui: &mut egui::Ui, v: &View, now: f64) {
        let p = crate::theme::pal();
        let q = self.query.clone();
        let hits = items::search(&q);
        let key_hits = keys_page::search(&v.s, &q);
        if hits.is_empty() && key_hits.is_empty() {
            ui.add_space(12.0);
            ui.label(tr_args("'{1}' 에 맞는 설정이 없습니다.", &[&q.trim()]));
            ui.label(RichText::new(tr("다른 낱말로 찾아보세요. 예: 화면, 글자, 템플릿, OBS, 백업, 리모컨, 단축키")).small().color(p.muted));
            return;
        }
        ui.label(RichText::new(tr_args("찾은 설정 {1}개 — 누르면 그 항목으로 갑니다", &[&(hits.len() + key_hits.len())])).small().color(p.muted));
        let mut go: Option<&'static str> = None;
        for it in hits {
            let r = ui.selectable_label(false, RichText::new(format!("{}  ›  {}", tr(it.cat.name()), tr(it.label))).strong());
            mark(&format!("set-hit-{}", it.id), r.rect);
            let mut sub = tr(it.desc);
            if !it.path.is_empty() && !matches!(it.kind, Kind::Template(_) | Kind::Image) {
                sub = format!("{} {}", tr_args("지금: {1}.", &[&items::value_text(it, items::read(&v.s, it.path))]), sub);
            }
            if !sub.trim().is_empty() {
                ui.label(RichText::new(sub).small().color(p.muted));
            }
            ui.add_space(4.0);
            if r.clicked() {
                go = Some(it.id);
            }
        }
        for (id, label, key) in key_hits {
            let r = ui.selectable_label(false, RichText::new(format!("{}  ›  {}", tr(Cat::Keys.name()), label)).strong());
            mark(&format!("set-hit-key-{id}"), r.rect);
            ui.label(RichText::new(tr_args("지금 키: {1}", &[&(if key.is_empty() { tr("없음 (끔)") } else { key })])).small().color(p.muted));
            ui.add_space(4.0);
            if r.clicked() {
                self.cat = Cat::Keys;
                self.keys.filter = label;
                self.query.clear();
                self.focus = None;
            }
        }
        if let Some(id) = go {
            self.goto(id, now);
        }
    }

    /// 분류 한 쪽
    fn page(&mut self, ui: &mut egui::Ui, core: &Shared, v: &View, now: f64) {
        let p = crate::theme::pal();
        let cat = self.cat;
        ui.label(RichText::new(tr(cat.name())).size(18.0).strong());
        ui.label(RichText::new(tr(cat.note())).color(p.muted));
        ui.add_space(6.0);
        if cat == Cat::Obs {
            ui.label(RichText::new(tr("OBS 브라우저 소스 주소")).strong());
        }
        let mut advanced = vec![];
        for it in ITEMS.iter().filter(|i| i.cat == cat) {
            if it.advanced {
                advanced.push(it);
            } else {
                self.item(ui, core, v, it, now);
            }
        }
        if cat == Cat::Keys {
            ui.add_space(4.0);
            self.keys_page(ui, core, v);
        }
        if !advanced.is_empty() {
            ui.add_space(6.0);
            // 검색으로 고급 항목에 왔으면 펼친다
            let force = self.focus.filter(|f| advanced.iter().any(|a| a.id == f.0) && now - f.1 < 0.5).map(|_| true);
            let head = egui::CollapsingHeader::new(tr("고급 설정 (평소에는 바꿀 필요가 없습니다)")).id_salt(("set-adv", cat.key())).open(force).show(ui, |ui| {
                for it in advanced {
                    self.item(ui, core, v, it, now);
                }
            });
            mark(&format!("set-adv-{}", cat.key()), head.header_response.rect);
        }
        self.reset_row(ui, core, v, cat);
    }

    /// '이 분류를 기본값으로' — 한 번 더 묻고, 한 뒤에는 되돌리기로 돌아올 수 있다
    fn reset_row(&mut self, ui: &mut egui::Ui, core: &Shared, v: &View, cat: Cat) {
        let p = crate::theme::pal();
        let changed: Vec<&Item> = ITEMS.iter().filter(|i| i.cat == cat && i.reset && items::read(&v.s, i.path) != &items::default_of(i.path)).collect();
        if !ITEMS.iter().any(|i| i.cat == cat && i.reset) {
            return;
        }
        ui.add_space(10.0);
        ui.separator();
        let id = egui::Id::unique(("set-reset", cat.key()));
        if crate::widgets::confirm_armed(ui, id) && !changed.is_empty() {
            let names: Vec<String> = changed.iter().map(|i| format!("'{}'", tr(i.label))).collect();
            ui.label(RichText::new(tr_args("이 분류에서 바꾼 설정 {1}개를 기본값으로 돌립니다: {2}", &[&changed.len(), &names.join(", ")])).color(p.warn));
            ui.label(RichText::new(tr("큐시트 · 성경 · 템플릿 · 연결 주소와 비밀번호는 건드리지 않습니다. 돌린 뒤 위의 '되돌리기'로 다시 돌아올 수 있습니다.")).small().color(p.muted));
            ui.horizontal(|ui| {
                let yes = ui.add(Btn::new(tr("기본값으로 돌리기")).kind(BtnKind::Warn).size(Size::Sm));
                mark("set-reset-yes", yes.rect);
                let no = ui.add(Btn::new(tr("그만두기")).size(Size::Sm));
                mark("set-reset-no", no.rect);
                if yes.clicked() {
                    let changes = changed.iter().map(|i| (i.path.iter().map(|x| x.to_string()).collect(), items::default_of(i.path))).collect();
                    self.set_many(core, changes, &tr(cat.name()));
                    crate::widgets::confirm_arm(ui, id, false);
                } else if no.clicked() {
                    crate::widgets::confirm_arm(ui, id, false);
                }
            });
        } else {
            ui.horizontal(|ui| {
                let tip = if changed.is_empty() { tr("이 분류는 모두 기본값입니다") } else { tr("누르면 무엇이 바뀌는지 보여 주고 한 번 더 묻습니다") };
                let r = ui.add(Btn::new(tr("이 분류를 기본값으로…")).icon("undo-2").size(Size::Sm).enabled(!changed.is_empty()).tooltip(tip));
                mark("set-reset", r.rect);
                if r.clicked() {
                    crate::widgets::confirm_arm(ui, id, true);
                }
                let note = if changed.is_empty() { tr("이 분류는 모두 기본값입니다") } else { tr_args("기본값과 다른 설정 {1}개", &[&changed.len()]) };
                ui.label(RichText::new(note).small().color(p.muted));
            });
        }
    }

    /// 항목 한 줄 (검색에서 왔으면 잠깐 강조)
    fn item(&mut self, ui: &mut egui::Ui, core: &Shared, v: &View, it: &'static Item, now: f64) {
        let p = crate::theme::pal();
        let focused = self.focus.is_some_and(|(id, t)| id == it.id && now - t < 2.5 && now >= t);
        let frame = egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(8, 6))
            .corner_radius(6)
            .fill(if focused { p.accent_bg } else { egui::Color32::TRANSPARENT })
            .stroke(if focused { egui::Stroke::new(1.5, p.accent) } else { egui::Stroke::NONE });
        let r = frame
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                self.item_inner(ui, core, v, it);
            })
            .response;
        mark(&format!("set-item-{}", it.id), r.rect);
        if focused {
            if self.scroll_to {
                r.scroll_to_me(Some(egui::Align::Center));
                self.scroll_to = false;
            }
            ui.ctx().request_repaint();
        }
    }

    fn item_inner(&mut self, ui: &mut egui::Ui, core: &Shared, v: &View, it: &'static Item) {
        let p = crate::theme::pal();
        let label = tr(it.label);
        let desc = tr(it.desc);
        let cur = items::read(&v.s, it.path).clone();
        let def = if it.path.is_empty() { Value::Null } else { items::default_of(it.path) };
        let ctl = format!("set-ctl-{}", it.id);
        let title = |ui: &mut egui::Ui| {
            let r = ui.label(RichText::new(&label).strong());
            if !desc.is_empty() {
                r.on_hover_text(&desc);
            }
        };
        let mut show_value = false;
        match it.kind {
            Kind::Toggle => {
                let mut b = cur.as_bool().unwrap_or_else(|| def.as_bool().unwrap_or(false));
                let r = ui.checkbox(&mut b, RichText::new(&label).strong());
                mark(&ctl, r.rect);
                if !desc.is_empty() {
                    r.clone().on_hover_text(&desc);
                }
                if r.changed() {
                    self.set(core, it.path, json!(b), &label);
                }
                show_value = true;
            }
            Kind::Choice(opts) => {
                title(ui);
                let s = cur.as_str().unwrap_or("").to_string();
                // 저장된 값이 목록에 없으면(옛 테마 이름 등) 실제로 쓰이는 것을 보여 준다
                let shown_id = if it.id == "ui.theme" { crate::theme::ThemeId::from_setting(&s).id().to_string() } else { s.clone() };
                let shown = opts.iter().find(|o| o.0 == shown_id).map(|o| tr(o.1)).unwrap_or(shown_id.clone());
                let mut pick = None;
                let r = egui::ComboBox::from_id_salt(("set-combo", it.id)).selected_text(shown).width(300.0).show_ui(ui, |ui| {
                    for (val, name) in opts {
                        let o = ui.selectable_label(shown_id == *val, tr(name));
                        mark(&format!("set-opt-{}-{}", it.id, val), o.rect);
                        if o.clicked() {
                            pick = Some(*val);
                        }
                    }
                });
                mark(&ctl, r.response.rect);
                if let Some(val) = pick {
                    self.set(core, it.path, json!(val), &label);
                }
                show_value = true;
            }
            Kind::Slider { lo, hi, step, unit, recommend } => {
                title(ui);
                let saved = cur.as_f64().unwrap_or(recommend).clamp(lo, hi);
                let mut x = self.slider.filter(|s| s.0 == it.id).map(|s| s.1).unwrap_or(saved);
                ui.horizontal(|ui| {
                    ui.spacing_mut().slider_width = 320.0;
                    let r = ui.add(egui::Slider::new(&mut x, lo..=hi).step_by(step).suffix(unit));
                    mark(&ctl, r.rect);
                    if r.dragged() {
                        self.slider = Some((it.id, x)); // 끄는 동안은 막대만 움직이고
                    } else if r.drag_stopped() || r.changed() {
                        self.slider = None; // 놓거나 숫자를 쳐 넣으면 적용
                        self.set(core, it.path, json!(x.round() as i64), &label);
                    }
                    let rec = ui.add(Btn::new(tr_args("추천 {1}{2}", &[&(recommend as i64), &unit])).size(Size::Sm).enabled((saved - recommend).abs() > 0.01).tooltip(tr("처음 설치했을 때의 크기로 돌립니다")));
                    mark(&format!("set-rec-{}", it.id), rec.rect);
                    if rec.clicked() {
                        self.slider = None;
                        self.set(core, it.path, json!(recommend as i64), &label);
                    }
                });
                show_value = true;
            }
            Kind::Int { lo, hi } => {
                title(ui);
                let enabled = it.id != "split.count" || v.s["split"]["mode"] == "count";
                let mut x = cur.as_i64().unwrap_or(lo);
                let r = ui.add_enabled(enabled, egui::DragValue::new(&mut x).range(lo..=hi).speed(if hi > 1000 { 1.0 } else { 0.1 }));
                mark(&ctl, r.rect);
                if r.changed() {
                    self.set(core, it.path, json!(x.clamp(lo, hi)), &label);
                }
                if !enabled {
                    r.on_disabled_hover_text(tr("분할 방식을 '지정한 절 수만큼'으로 고르면 쓸 수 있습니다"));
                }
                show_value = true;
            }
            Kind::Text => {
                title(ui);
                let mut s = cur.as_str().unwrap_or("").to_string();
                let r = ui.add(egui::TextEdit::singleline(&mut s).desired_width(360.0));
                mark(&ctl, r.rect);
                if r.changed() {
                    self.set(core, it.path, json!(s), &label);
                }
            }
            Kind::Image => {
                title(ui);
                let url = cur.as_str().unwrap_or("").to_string();
                ui.horizontal(|ui| {
                    let r = ui.add(Btn::new(tr(if url.is_empty() { "그림 고르기…" } else { "다른 그림…" })).icon("folder-open").size(Size::Sm).tooltip(tr("png · jpg · webp 그림 파일을 고릅니다")));
                    mark(&ctl, r.rect);
                    if r.clicked() {
                        if let Some(f) = self.pick_image() {
                            match crate::tpl_inspect::import_media(&v.env.data_dir, &f, "logo") {
                                Ok(u) => self.set(core, it.path, json!(u), &label),
                                Err(e) => self.fail(tr_args("그림을 가져오지 못했습니다: {1}", &[&e])),
                            }
                        }
                    }
                    let c = ui.add(Btn::new(tr("지우기")).size(Size::Sm).enabled(!url.is_empty()).tooltip(tr("그림을 빼고 템플릿의 로고를 씁니다 (그림 파일은 지우지 않습니다)")));
                    mark(&format!("set-clear-{}", it.id), c.rect);
                    if c.clicked() {
                        self.set(core, it.path, json!(""), &label);
                    }
                    ui.label(RichText::new(if url.is_empty() { tr("정한 그림 없음 — 템플릿의 로고를 씁니다") } else { tr_args("지금: {1}", &[&file_name(&url)]) }).small().color(p.muted));
                });
            }
            Kind::Template(_) => {
                title(ui);
                let s = cur.as_str().unwrap_or("").to_string();
                let shown = v.tpls.iter().find(|t| t.0 == s).map(|t| t.1.clone()).unwrap_or_else(|| tr("(없는 템플릿)"));
                let mut pick = None;
                let r = egui::ComboBox::from_id_salt(("set-combo", it.id)).selected_text(shown).width(300.0).show_ui(ui, |ui| {
                    for (id, name) in &v.tpls {
                        let o = ui.selectable_label(*id == s, name);
                        mark(&format!("set-opt-{}-{}", it.id, id), o.rect);
                        if o.clicked() {
                            pick = Some(id.clone());
                        }
                    }
                });
                mark(&ctl, r.response.rect);
                if let Some(id) = pick {
                    self.set(core, it.path, json!(id), &label);
                }
            }
            Kind::Link(menu) => {
                ui.horizontal_wrapped(|ui| {
                    title(ui);
                    let sum = self.summary(it.id, v);
                    if !sum.is_empty() {
                        ui.label(RichText::new(format!("— {sum}")).color(p.text2));
                    }
                });
                let r = ui.add(Btn::new(tr(it.btn)).icon_end("chevron-right").size(Size::Sm).tooltip(tr("이 창을 엽니다 (설정 창은 그대로 둡니다)")));
                mark(&format!("set-open-{}", it.id), r.rect);
                if r.clicked() {
                    self.pending.push(menu);
                }
            }
            Kind::Copy(path) => {
                let url = format!("http://127.0.0.1:{}{}", v.port, path);
                ui.horizontal(|ui| {
                    title(ui);
                    ui.label(RichText::new(&url).monospace().color(p.text2));
                    let r = ui.add(Btn::new(tr("복사")).icon("copy").size(Size::Sm).tooltip(tr("주소를 복사합니다. OBS 브라우저 소스의 주소 칸에 붙여 넣으세요.")));
                    mark(&ctl, r.rect);
                    if r.clicked() {
                        crate::widgets::copy_notice(ui.ctx(), url.clone(), crate::track_c::obs_copied_msg(it.label));
                        self.status = Some((tr_args("주소를 복사했습니다: {1}", &[&url]), false));
                    }
                });
            }
            Kind::Special => self.special(ui, core, v, it, &label),
        }
        if !desc.is_empty() {
            ui.label(RichText::new(&desc).small().color(p.muted));
        }
        if show_value {
            let same = cur == def || (cur.is_null() && !def.is_null());
            ui.horizontal(|ui| {
                let txt = if same { tr_args("지금: {1} (기본값)", &[&items::value_text(it, &def)]) } else { tr_args("지금: {1} · 기본값: {2}", &[&items::value_text(it, &cur), &items::value_text(it, &def)]) };
                ui.label(RichText::new(txt).small().color(if same { p.muted2 } else { p.text2 }));
                if !same {
                    let r = ui.add(Btn::new(tr("기본값으로")).size(Size::Sm).tooltip(tr("이 설정 하나만 기본값으로 돌립니다")));
                    mark(&format!("set-def-{}", it.id), r.rect);
                    if r.clicked() {
                        self.slider = None;
                        self.set(core, it.path, def.clone(), &label);
                    }
                }
            });
        }
    }

    /// 따로 그리는 항목
    fn special(&mut self, ui: &mut egui::Ui, core: &Shared, v: &View, it: &'static Item, label: &str) {
        let p = crate::theme::pal();
        match it.id {
            "obsfull" => {
                let mut b = v.obs_full;
                let r = ui.checkbox(&mut b, RichText::new(label).strong());
                mark("set-ctl-obsfull", r.rect);
                if r.changed() {
                    self.pending.push("c:obsfull"); // 보기 메뉴와 같은 길 (켜기·끄기 + 알림)
                }
            }
            "globalkeys-status" => {
                let ws = wp_core::winkeys::status();
                let list = |k: &str| -> Vec<String> { ws[k].as_array().into_iter().flatten().map(|x| x.as_str().map(String::from).unwrap_or_else(|| x.to_string())).collect() };
                let (reg, fail) = (list("registered"), list("failed"));
                let txt = if !ws["available"].as_bool().unwrap_or(false) {
                    tr("이 운영체제에서는 쓸 수 없음")
                } else if !ws["enabled"].as_bool().unwrap_or(false) {
                    tr("꺼짐")
                } else {
                    tr_args("등록 {1}개{2}{3}", &[&reg.len(), &(if fail.is_empty() { String::new() } else { tr_args(" · 다른 프로그램이 쓰는 키: {1}", &[&fail.join(", ")]) }), &(if reg.is_empty() { String::new() } else { format!(" ({})", reg.join(", ")) })])
                };
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(label).strong());
                    ui.label(RichText::new(txt).color(if fail.is_empty() { p.text2 } else { p.warn }));
                });
            }
            "version" => {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(label).strong());
                    ui.label(format!("{} {}", tr("워십프리젠터"), env!("CARGO_PKG_VERSION")));
                });
                let skip = v.s["skipVersion"].as_str().unwrap_or("").to_string();
                if !skip.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(tr_args("{1} 버전은 건너뛰기로 했습니다 (알리지 않고 설치하지 않음).", &[&skip])).small().color(p.warn));
                        let r = ui.add(Btn::new(tr("건너뛰기 취소")).size(Size::Sm));
                        mark("set-unskip", r.rect);
                        if r.clicked() {
                            self.set(core, &["skipVersion"], json!(""), &tr("건너뛰기 취소"));
                        }
                    });
                }
            }
            _ => {}
        }
    }

    /// 창 열기 줄에 보이는 지금 상태 한 줄 (비밀번호·PIN·키 값은 보이지 않는다)
    pub(crate) fn summary(&self, id: &str, v: &View) -> String {
        let s = &v.s;
        let on = |b: bool| tr(if b { "켜짐" } else { "꺼짐" });
        match id {
            "out-open" => {
                let m = if v.env.monitor.is_empty() { String::new() } else { tr_args(" · 모니터: {1}", &[&v.env.monitor]) };
                format!("{}{}", tr_args("예배당 화면 {1}", &[&on(v.env.out_running)]), m)
            }
            "templates" => tr_args("템플릿 {1}개", &[&v.tpls.len()]),
            "obs-remote" => {
                if !s["obs"]["enabled"].as_bool().unwrap_or(false) {
                    on(false)
                } else {
                    crate::trackd_ui::obs_status_line(s).map(|x| x.0).unwrap_or_else(|| on(true))
                }
            }
            "lan" | "lan2" => {
                if s["lanOutput"]["enabled"].as_bool().unwrap_or(false) {
                    tr_args("켜짐 · 포트 {1}", &[&s["lanOutput"]["port"]])
                } else {
                    on(false)
                }
            }
            "remote" => {
                if s["remote"]["enabled"].as_bool().unwrap_or(false) {
                    tr_args("켜짐 · 포트 {1}", &[&s["remote"]["port"]])
                } else {
                    on(false)
                }
            }
            "trigger" => tr(if s["triggerKey"].as_str().unwrap_or("").is_empty() { "보안 키 없음" } else { "보안 키 정해 둠" }),
            "devices" => tr_args("장비 {1}개 · Companion {2}:{3}", &[&s["devices"].as_array().map(|a| a.len()).unwrap_or(0), &s["companion"]["host"].as_str().unwrap_or(""), &s["companion"]["port"]]),
            "cameras" => tr_args("카메라 {1}개", &[&s["cameras"].as_object().map(|a| a.len()).unwrap_or(0)]),
            "sync" => {
                let dir = s["syncDir"].as_str().unwrap_or("");
                let d = if dir.is_empty() { tr("동기화 폴더를 정하지 않음") } else { tr_args("동기화 폴더: {1}", &[&dir]) };
                format!("{d} · {}", tr_args("자동 백업 {1}개", &[&v.info.backups]))
            }
            "bible" => {
                if v.info.version_name.is_empty() {
                    tr_args("번역본 {1}개", &[&v.info.versions])
                } else {
                    tr_args("번역본 {1}개 · 지금 쓰는 번역본: {2}", &[&v.info.versions, &v.info.version_name])
                }
            }
            "commentary" => tr_args("주석서 {1}개", &[&v.info.commentaries]),
            _ => String::new(),
        }
    }

    fn pick_image(&mut self) -> Option<std::path::PathBuf> {
        #[cfg(test)]
        {
            self.test_pick.take()
        }
        #[cfg(not(test))]
        {
            rfd::FileDialog::new().add_filter(tr("그림"), &["png", "jpg", "jpeg", "webp", "gif", "bmp"]).pick_file()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conflicts() {
        let s = json!({ "shortcuts": { "a": "F5", "b": "f5", "c": "Ctrl+K" }, "reservedKeys": "ctrl + k" });
        let c = shortcut_conflicts(&s);
        assert_eq!(c.len(), 3);
        assert!(c.iter().any(|x| x.0 == "c" && x.1.contains("예약")));
    }

    #[test]
    fn 별칭이_달라도_같은_키로_보고_모르는_키와_비운_키를_가린다() {
        let s = json!({ "shortcuts": { "a": "Control+Shift+F", "b": "shift + ctrl + f", "c": "Hyper+Q", "d": "", "e": "Cmd+K", "f": "PageDown" }, "reservedKeys": "CTRL+k, pgdn" });
        let c = shortcut_conflicts(&s);
        let why = |id: &str| c.iter().find(|x| x.0 == id).map(|x| x.1.clone());
        assert!(why("a").unwrap().contains("같은 키"));
        assert!(why("b").unwrap().contains("같은 키"));
        assert!(why("c").unwrap().contains("알 수 없는 키"));
        assert!(why("d").is_none(), "비운 키는 끈 것");
        assert!(why("e").unwrap().contains("예약"), "Cmd 와 Ctrl 은 같은 꾸밈 키");
        assert!(why("f").unwrap().contains("예약"), "pgdn 은 PageDown");
    }

    /// 설정 화면 항목의 글은 모두 영어 번역이 있어야 한다 (uiLang=en 에서 한국어가 섞이지 않게)
    #[test]
    fn 설정_항목_글에_영어_번역이_있다() {
        let en: Value = serde_json::from_str(include_str!("../../wp-core/data/en.json")).unwrap();
        let mut missing = vec![];
        let mut need = |s: &str| {
            if !s.is_empty() && en[s].as_str().is_none_or(|x| x.is_empty()) && s.chars().any(|c| ('가'..='힣').contains(&c)) {
                missing.push(s.to_string());
            }
        };
        for c in Cat::ALL {
            need(c.name());
            need(c.note());
        }
        for it in ITEMS {
            need(it.label);
            need(it.desc);
            need(it.btn);
            if let Kind::Choice(opts) = it.kind {
                for o in opts {
                    need(o.1);
                }
            }
        }
        assert!(missing.is_empty(), "en.json 에 없는 글 {}개:\n{}", missing.len(), missing.join("\n"));
    }
}
