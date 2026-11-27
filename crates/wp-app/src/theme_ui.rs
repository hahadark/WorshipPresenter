//! [트랙 C] 출력 테마 관리 창 (보기 → 테마 · 빠른 설정 '테마'). docs/theme-management.md · docs/ux-rules.md
//!
//! - 왼쪽: 내 테마 + 기본 테마(구세군·방송 스타일) 카드 격자. 이름 찾기 · 기본/내 테마 거르기 · 지금 쓰는 출력 배지.
//! - 오른쪽: 고른 테마의 출력별 미리보기(예배당 · OBS 자막 · OBS 찬송가), 적용할 출력 고르기, 관리 단추.
//! - 카드를 누르는 것은 **미리보기만** 한다. 실제 출력은 '고른 출력에 적용'을 눌러야 바뀌고, '적용 되돌리기'로 되돌린다.
//! - 일반 적용은 고쳐 둔 디자인을 지우지 않는다. '처음 디자인으로'는 따로 확인받는다.
//! - 여기서 말하는 테마는 **출력 화면 디자인**이다. 프로그램 화면 색(다크·라이트)은 설정에서 바꾼다.
//! - 그림은 미리보기·LIVE 와 같은 paint_slide 만 쓴다.

use crate::paint::{self, paint_slide};
use crate::theme::{self, alpha, pal, Weight};
use crate::widgets::{self, Btn, Kind, Size};
use eframe::egui::{self, Align2, Color32, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Vec2};
use serde_json::{json, Value};
use wp_core::core::Shared;
use wp_core::i18n::{tr, tr_args};
use wp_core::themes::{self, ApplyMode, SLOTS};

/// 이름을 받는 일
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum FormKind {
    /// 지금 출력 디자인을 내 테마로 저장
    SaveCurrent,
    /// 이 테마를 복제
    Duplicate(String),
    /// 내 테마 이름·설명 바꾸기
    Rename(String),
}

#[derive(Clone, Debug)]
pub(crate) struct Form {
    pub(crate) kind: FormKind,
    pub(crate) name: String,
    pub(crate) desc: String,
    focus: bool,
}

/// 한 번 더 묻는 일
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Confirm {
    Delete,
    /// 기본 테마를 처음 디자인으로
    Reset,
}

/// 화면에서 누른 것 (그린 뒤 한꺼번에 처리)
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Do {
    Select(String),
    Apply,
    Undo,
    OpenForm(FormKind),
    SubmitForm,
    CancelForm,
    Edit(String),
    Export,
    Import,
    Ask(Confirm),
    Confirmed,
    CancelConfirm,
}

pub struct ThemeUi {
    pub open: bool,
    /// 고른(미리 보는) 테마 id
    pub(crate) sel: String,
    pub(crate) msg: String,
    pub(crate) msg_err: bool,
    /// 이름 찾기
    pub(crate) query: String,
    /// 0 모두 · 1 기본 테마 · 2 내 테마
    pub(crate) filter: u8,
    /// 견본 글: 0 성경 · 1 찬양 · 2 공지
    pub(crate) sample: usize,
    /// 적용할 출력 (예배당 · OBS 자막 · OBS 찬송가). 찬송가 칸은 따로 켜야 한다
    pub(crate) slots: [bool; 3],
    /// 방금 한 적용·원복을 되돌리는 정보
    pub(crate) undo: Option<themes::ApplyUndo>,
    pub(crate) form: Option<Form>,
    pub(crate) confirm: Option<Confirm>,
    /// 템플릿 편집 창으로 열어 달라는 요청 (track_c 가 take_edit_request 로 가져가 TemplateUi::open_with 에 넘긴다)
    edit_req: Option<String>,
    /// 가져온 테마가 가리키는 파일 (경로, 이 PC 에 있나)
    pub(crate) linked: Vec<(String, bool)>,
}

impl Default for ThemeUi {
    fn default() -> Self {
        Self { open: std::env::args().any(|a| a == "--open-themes"), sel: String::new(), msg: String::new(), msg_err: false, query: String::new(), filter: 0, sample: 0, slots: [true, true, false], undo: None, form: None, confirm: None, edit_req: None, linked: vec![] }
    }
}

/// 미리보기 견본 (성경 · 찬양 · 공지)
fn sample(k: usize) -> Value {
    match k {
        1 => json!({ "kind": "text", "ref": "찬양 제목", "title": "찬양 제목", "label": "1절", "parts": [{ "book": 0, "chapter": 0, "verse": 0, "text": "찬양 가사 첫 줄이 여기에 나옵니다\n둘째 줄도 이렇게 이어집니다" }] }),
        2 => json!({ "kind": "text", "ref": "광고", "title": "광고", "label": "공지", "parts": [{ "book": 0, "chapter": 0, "verse": 0, "text": "오늘 오후 2시 교사 모임이 있습니다\n본당 2층 소예배실" }] }),
        _ => json!({ "kind": "bible", "ref": "요한복음 3:16", "title": "요한복음 3:16", "parts": [
            { "book": 43, "chapter": 3, "verse": 16, "text": "하나님이 세상을 이처럼 사랑하사 독생자를 주셨으니 이는 그를 믿는 자마다 멸망하지 않고 영생을 얻게 하려 하심이라" }
        ] }),
    }
}

const CARD_W: f32 = 196.0;
const THUMB_H: f32 = 110.0;
const DETAIL_W: f32 = 400.0;
const SAMPLES: [&str; 3] = ["성경", "찬양", "공지"];
const FILTERS: [&str; 3] = ["모두", "기본 테마", "내 테마"];

fn slot_color(slot: &str) -> Color32 {
    let p = pal();
    match slot {
        "main" => p.accent,
        "overlay" => p.pv,
        _ => p.gold,
    }
}

/// 단추를 그리고 자리를 남긴다 (합성 입력 시험이 이름으로 누른다)
fn btn(ui: &mut egui::Ui, name: &str, b: Btn) -> bool {
    let r = ui.add(b);
    widgets::mark(name, r.rect.intersect(ui.clip_rect())); // 보이는 부분만 (스크롤로 가려지면 빈 자리)
    r.clicked()
}

/// 작은 색 배지 (흐름 안에 놓는다)
fn chip(ui: &mut egui::Ui, text: &str, col: Color32) {
    let g = ui.painter().layout_no_wrap(text.to_string(), theme::font(11.0, Weight::SemiBold), col);
    let (r, _) = ui.allocate_exact_size(Vec2::new(g.size().x + 12.0, g.size().y + 5.0), Sense::hover());
    ui.painter().rect(r, crate::theme::cr(crate::theme::Rad::Sm), alpha(col, 0.16), Stroke::new(1.0, alpha(col, 0.55)), StrokeKind::Inside);
    ui.painter().galley(r.center() - g.size() / 2.0, g, col);
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v[k].as_str().unwrap_or("")
}

/// 화면에 보이는 테마 이름 (기본 테마 이름은 영어 화면에서 번역된다)
fn shown_name(t: &Value) -> String {
    if t["builtin"] == true { wp_core::i18n::tr_name(s(t, "name")) } else { s(t, "name").to_string() }
}

impl ThemeUi {
    /// 템플릿 편집 창으로 열 템플릿 id (있으면 한 번만 돌려준다). track_c 가 프레임마다 불러
    /// `templates.open_with(&id)` 로 잇는다.
    pub fn take_edit_request(&mut self) -> Option<String> {
        self.edit_req.take()
    }

    fn say(&mut self, msg: impl Into<String>, err: bool) {
        self.msg = msg.into();
        self.msg_err = err;
    }

    fn fail(&mut self, core: &Shared, e: String) {
        core.lock().notify(e.clone(), "error");
        self.say(e, true);
    }

    /// 걸러진 목록 (찾는 글 · 기본/내 테마)
    fn visible<'a>(&self, all: &'a [Value]) -> Vec<&'a Value> {
        let q = self.query.trim().to_lowercase();
        all.iter()
            .filter(|t| match self.filter {
                1 => t["builtin"] == true,
                2 => t["builtin"] == false,
                _ => true,
            })
            .filter(|t| q.is_empty() || shown_name(t).to_lowercase().contains(&q) || s(t, "name").to_lowercase().contains(&q) || tr(s(t, "desc")).to_lowercase().contains(&q) || s(t, "desc").to_lowercase().contains(&q))
            .collect()
    }

    /// 지금 고른 출력 칸 가운데 이 테마에 디자인이 있는 것
    fn chosen_slots(&self, t: &Value) -> Vec<&'static str> {
        SLOTS.iter().enumerate().filter(|(i, o)| self.slots[*i] && t["slots"][**o].is_object()).map(|(_, o)| *o).collect()
    }

    /// 누른 것을 처리한다 (설정을 바꾸는 곳은 여기뿐)
    pub(crate) fn run(&mut self, core: &Shared, d: Do) {
        let all = themes::list_all(core.lock().settings.get());
        let cur = all.iter().find(|t| t["id"] == self.sel.as_str()).cloned().unwrap_or(Value::Null);
        let cur_name = shown_name(&cur);
        match d {
            Do::Select(id) => {
                if self.sel != id {
                    self.sel = id;
                    self.confirm = None;
                    if !self.msg_err {
                        // 적용 결과 글은 '되돌리기'가 살아 있는 동안 남긴다
                        if self.undo.is_none() {
                            self.msg.clear();
                        }
                    } else {
                        self.msg.clear();
                    }
                }
            }
            Do::Apply => {
                let slots = self.chosen_slots(&cur);
                if slots.is_empty() {
                    return self.say(tr("적용할 출력을 하나 이상 고르세요."), true);
                }
                let r = themes::apply_theme_ex(&mut core.lock(), &self.sel, &slots, ApplyMode::Keep);
                match r {
                    Ok((outs, u)) => {
                        let names = outs.iter().map(|o| tr(themes::slot_label(o))).collect::<Vec<_>>().join(", ");
                        self.undo = Some(u);
                        self.say(tr_args("'{1}' 테마를 {2}에 적용했습니다.", &[&cur_name, &names]), false);
                        core.lock().notify(tr_args("출력 테마 적용: {1} ({2})", &[&cur_name, &names]), "ok");
                    }
                    Err(e) => self.fail(core, e),
                }
            }
            Do::Undo => {
                if let Some(u) = self.undo.take() {
                    let result = themes::undo_apply(&mut core.lock(), &u);
                    match result {
                        Ok(()) => {
                            self.say(tr("방금 한 적용을 되돌렸습니다. 출력이 전 디자인으로 돌아갔습니다."), false);
                            core.lock().notify(tr("출력 테마 적용을 되돌렸습니다."), "ok");
                        }
                        Err(e) => self.fail(core, e),
                    }
                }
            }
            Do::OpenForm(kind) => {
                let (name, desc) = match &kind {
                    FormKind::SaveCurrent => (tr("내 테마"), String::new()),
                    FormKind::Duplicate(id) => {
                        let t = all.iter().find(|t| t["id"] == id.as_str()).cloned().unwrap_or(Value::Null);
                        (tr_args("{1} 복사본", &[&shown_name(&t)]), String::new())
                    }
                    FormKind::Rename(id) => {
                        let t = all.iter().find(|t| t["id"] == id.as_str()).cloned().unwrap_or(Value::Null);
                        (s(&t, "name").to_string(), s(&t, "desc").to_string())
                    }
                };
                self.confirm = None;
                self.form = Some(Form { kind, name, desc, focus: true });
            }
            Do::CancelForm => self.form = None,
            Do::SubmitForm => {
                let Some(f) = self.form.clone() else { return };
                let r = {
                    let mut c = core.lock();
                    match &f.kind {
                        FormKind::SaveCurrent => themes::save_current_as(&mut c, &f.name, &f.desc).map(|id| (id, "지금 출력 디자인을 내 테마 '{1}'(으)로 저장했습니다. 출력은 바뀌지 않았습니다.")),
                        FormKind::Duplicate(src) => themes::duplicate_theme(&mut c, src, &f.name).map(|id| (id, "내 테마 '{1}'을(를) 만들었습니다. '디자인 편집'으로 고치세요. 원본 테마는 그대로입니다.")),
                        FormKind::Rename(id) => themes::rename_theme(&mut c, id, &f.name, &f.desc).map(|_| (id.clone(), "테마 이름을 '{1}'(으)로 바꿨습니다.")),
                    }
                };
                match r {
                    Ok((id, msg)) => {
                        let name = themes::list_all(core.lock().settings.get()).iter().find(|t| t["id"] == id.as_str()).map(|t| s(t, "name").to_string()).unwrap_or_default();
                        self.form = None;
                        self.sel = id;
                        if self.filter == 1 {
                            self.filter = 0; // 새 내 테마가 보이게
                        }
                        self.say(tr_args(msg, &[&name]), false);
                    }
                    Err(e) => self.say(e, true), // 입력칸은 그대로 둔다
                }
            }
            Do::Edit(tid) => {
                self.edit_req = Some(tid);
                self.say(tr("템플릿 편집 창에서 이 디자인을 엽니다. 고치면 바로 저장됩니다."), false);
            }
            Do::Export => {
                let name = s(&cur, "name").to_string();
                let Some(path) = rfd::FileDialog::new().add_filter("JSON", &["json"]).set_file_name(format!("{}.wptheme.json", safe_file_name(&name))).save_file() else { return };
                self.export_to(core, &path);
            }
            Do::Import => {
                let Some(path) = rfd::FileDialog::new().add_filter("JSON", &["json"]).pick_file() else { return };
                self.import_from(core, &path);
            }
            Do::Ask(c) => {
                self.form = None;
                self.confirm = Some(c);
            }
            Do::CancelConfirm => self.confirm = None,
            Do::Confirmed => match self.confirm.take() {
                Some(Confirm::Delete) => {
                    let r = themes::delete_theme(&mut core.lock(), &self.sel);
                    match r {
                        Ok(kept) => {
                            self.sel.clear();
                            self.say(tr_args("내 테마 '{1}'을(를) 목록에서 지웠습니다. 그 디자인(템플릿 {2}개)은 출력이나 큐시트가 쓰고 있을 수 있어 '출력 템플릿'에 남겨 두었습니다.", &[&cur_name, &kept]), false);
                        }
                        Err(e) => self.fail(core, e),
                    }
                }
                Some(Confirm::Reset) => {
                    let r = themes::restore_original(&mut core.lock(), &self.sel, &["main", "overlay"]);
                    match r {
                        Ok(u) => {
                            self.undo = Some(u);
                            self.say(tr_args("'{1}' 테마를 처음 디자인으로 되돌렸습니다.", &[&cur_name]), false);
                        }
                        Err(e) => self.fail(core, e),
                    }
                }
                None => {}
            },
        }
    }

    /// 고른 테마를 파일로 내보낸다 (테마 디자인만. 다른 설정은 들어가지 않는다)
    pub(crate) fn export_to(&mut self, core: &Shared, path: &std::path::Path) {
        let r = themes::export_theme(core.lock().settings.get(), &self.sel).and_then(|txt| std::fs::write(path, txt).map_err(|e| tr_args("테마 파일을 쓰지 못했습니다: {1}", &[&e])));
        match r {
            Ok(()) => self.say(tr_args("테마를 {1} 에 내보냈습니다. 그림·영상·로고는 파일 경로만 들어 있고 파일 자체는 들어 있지 않습니다.", &[&path.display()]), false),
            Err(e) => self.fail(core, e),
        }
    }

    /// 테마 파일을 내 테마로 가져온다 (출력은 바꾸지 않는다)
    pub(crate) fn import_from(&mut self, core: &Shared, path: &std::path::Path) {
        // 큰 파일은 읽기 전에 거른다
        if std::fs::metadata(path).map(|m| m.len() as usize > themes::IMPORT_MAX_BYTES).unwrap_or(false) {
            return self.fail(core, tr_args("파일이 너무 큽니다 (최대 {1}KB). 테마 파일이 맞는지 확인하세요.", &[&(themes::IMPORT_MAX_BYTES / 1024)]));
        }
        let r = std::fs::read_to_string(path).map_err(|e| tr_args("테마 파일을 읽지 못했습니다: {1}", &[&e])).and_then(|txt| themes::import_theme(&mut core.lock(), &txt));
        match r {
            Ok(im) => {
                self.linked = im.linked_files.iter().map(|f| (f.clone(), std::path::Path::new(f).is_file())).collect();
                self.sel = im.id;
                if self.filter == 1 {
                    self.filter = 0;
                }
                self.say(tr_args("'{1}' 테마를 내 테마로 가져왔습니다. 아직 출력에는 적용하지 않았습니다. 미리보기를 확인한 뒤 적용하세요.", &[&im.name]), false);
            }
            Err(e) => self.fail(core, e),
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, core: &Shared, web_dir: &std::path::Path, data_dir: &std::path::Path) {
        if !self.open {
            return;
        }
        let p = pal();
        let pc = paint::Ctx { web_dir, data_dir };
        let (all, active) = {
            let c = core.lock();
            let st = c.settings.get();
            (themes::list_all(st), st["activeTemplates"].clone())
        };
        let used = |t: &Value| -> Vec<&'static str> { SLOTS.iter().copied().filter(|o| t["slots"][*o]["id"].as_str().is_some_and(|tid| active[*o] == tid)).collect() };
        // 처음에는 지금 예배당에 쓰는 테마(없으면 첫 테마)를 고른다. 고르는 것은 미리보기일 뿐이다
        if self.sel.is_empty() || !all.iter().any(|t| t["id"] == self.sel.as_str()) {
            self.sel = all.iter().find(|t| used(t).contains(&"main")).or(all.first()).map(|t| s(t, "id").to_string()).unwrap_or_default();
        }
        let mut acts: Vec<Do> = vec![];
        let mut open = self.open;
        let screen = ctx.content_rect();
        let size = [(screen.width() - 80.0).clamp(560.0, 1260.0), (screen.height() - 130.0).clamp(380.0, 1000.0)];
        widgets::float_window(ctx, "themes", tr("출력 테마 관리"), size).open(&mut open).collapsible(false).resizable(true).show(ctx, |ui| {
            ui.label(RichText::new(tr("예배당 화면과 OBS 자막에 나가는 '출력 디자인'을 고르고 관리합니다. 테마를 누르면 미리보기만 바뀌고, '고른 출력에 적용'을 눌러야 실제 출력이 바뀝니다. 프로그램 화면 색(다크·라이트)은 설정에서 바꿉니다.")).size(12.5).color(p.muted));
            ui.add_space(6.0);
            // ---- 도구 줄: 찾기 · 거르기 · 견본 · 저장 · 가져오기
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
                let r = ui.add(egui::TextEdit::singleline(&mut self.query).hint_text(tr("테마 이름 찾기")).desired_width(170.0));
                widgets::mark("theme-search", r.rect);
                for (k, label) in FILTERS.iter().enumerate() {
                    if btn(ui, &format!("theme-filter-{k}"), Btn::new(tr(label)).size(Size::Sm).selected(self.filter == k as u8)) {
                        self.filter = k as u8;
                    }
                }
                widgets::vsep(ui, 18.0);
                ui.label(theme::rich(&tr("견본 글"), 12.0, Weight::Medium, p.muted));
                for (k, label) in SAMPLES.iter().enumerate() {
                    if btn(ui, &format!("theme-sample-{k}"), Btn::new(tr(label)).size(Size::Sm).selected(self.sample == k).tooltip(tr("미리보기에 넣어 볼 글을 바꿉니다 (출력은 바뀌지 않습니다)"))) {
                        self.sample = k;
                    }
                }
                widgets::vsep(ui, 18.0);
                if btn(ui, "theme-save-current", Btn::new(tr("지금 출력 디자인을 내 테마로 저장")).icon("plus").size(Size::Sm).tooltip(tr("지금 예배당·OBS 자막·OBS 찬송가에 쓰는 디자인을 한 묶음으로 이름 붙여 저장합니다"))) {
                    acts.push(Do::OpenForm(FormKind::SaveCurrent));
                }
                if btn(ui, "theme-import", Btn::new(tr("테마 파일 가져오기…")).icon("folder-open").size(Size::Sm).tooltip(tr("내보낸 테마 파일(.json)을 내 테마로 가져옵니다. 지금 출력과 있던 테마는 바뀌지 않습니다"))) {
                    acts.push(Do::Import);
                }
            });
            // 자주 쓰는 적용·관리는 미리보기 스크롤 밖에도 둔다.
            if let Some(t) = all.iter().find(|t| s(t, "id") == self.sel) {
                let enabled = self.form.is_none() && self.confirm.is_none();
                ui.add_enabled_ui(enabled, |ui| self.quick_actions(ui, t, &used, &mut acts));
            }
            // ---- 이름 입력
            if self.form.is_some() {
                ui.add_space(6.0);
                widgets::group_frame().show(ui, |ui| self.form_ui(ui, &mut acts));
            }
            // ---- 결과 글 + 되돌리기
            if !self.msg.is_empty() || self.undo.is_some() {
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    if !self.msg.is_empty() {
                        ui.label(theme::rich(&self.msg, 12.5, Weight::Medium, if self.msg_err { p.danger_text } else { p.ok_text }));
                    }
                    if self.undo.is_some() && btn(ui, "theme-undo", Btn::new(tr("적용 되돌리기")).icon("undo-2").size(Size::Sm).tooltip(tr("방금 바꾼 출력 칸과 디자인만 전 상태로 되돌립니다 (다른 설정은 건드리지 않습니다)"))) {
                        acts.push(Do::Undo);
                    }
                });
            }
            if !self.linked.is_empty() {
                ui.add_space(4.0);
                widgets::group_frame().show(ui, |ui| {
                    ui.label(theme::rich(&tr("가져온 테마가 쓰는 그림·영상·로고 (파일 자체는 가져오지 않습니다. 이 PC에 있는지 확인하세요)"), 12.0, Weight::Medium, p.muted));
                    for (f, ok) in &self.linked {
                        ui.horizontal_wrapped(|ui| {
                            chip(ui, &tr(if *ok { "파일 있음" } else { "파일 없음" }), if *ok { p.ok_text } else { p.danger_text });
                            ui.label(RichText::new(f).size(12.0).color(p.text2));
                        });
                    }
                    if ui.add(Btn::new(tr("확인했습니다")).size(Size::Sm)).clicked() {
                        self.linked.clear();
                    }
                });
            }
            ui.add_space(8.0);
            let shown = self.visible(&all);
            let cur = all.iter().find(|t| t["id"] == self.sel.as_str());
            let h = ui.available_height().max(200.0);
            let w = ui.available_width();
            if w >= CARD_W + DETAIL_W + 60.0 {
                // 넓으면 2단: 목록 | 고른 테마
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 16.0;
                    let gw = w - DETAIL_W - 16.0;
                    ui.allocate_ui_with_layout(Vec2::new(gw, h), egui::Layout::top_down(egui::Align::Min), |ui| {
                        egui::ScrollArea::vertical().id_salt("theme-grid").auto_shrink([false, false]).max_height(h).show(ui, |ui| self.grid(ui, &pc, &shown, &used, &mut acts));
                    });
                    ui.allocate_ui_with_layout(Vec2::new(DETAIL_W, h), egui::Layout::top_down(egui::Align::Min), |ui| {
                        egui::ScrollArea::vertical().id_salt("theme-detail").auto_shrink([false, false]).max_height(h).show(ui, |ui| {
                            ui.set_width(DETAIL_W - 14.0);
                            self.detail(ui, &pc, cur, &used, &mut acts);
                        });
                    });
                });
            } else {
                // 좁으면 한 줄로 쌓고 통째로 스크롤
                egui::ScrollArea::vertical().id_salt("theme-stack").auto_shrink([false, false]).max_height(h).show(ui, |ui| {
                    self.grid(ui, &pc, &shown, &used, &mut acts);
                    ui.separator();
                    self.detail(ui, &pc, cur, &used, &mut acts);
                });
            }
        });
        self.open = open;
        for d in acts {
            self.run(core, d);
        }
    }

    /// 미리보기 아래를 찾지 않아도 적용·편집할 수 있는 고정 작업 줄.
    fn quick_actions(&mut self, ui: &mut egui::Ui, t: &Value, used: &dyn Fn(&Value) -> Vec<&'static str>, acts: &mut Vec<Do>) {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
            ui.label(tr("출력에 적용"));
            for (i, slot) in SLOTS.iter().enumerate() {
                let has = t["slots"][*slot].is_object();
                let mut on = self.slots[i] && has;
                let r = ui.add_enabled(has, egui::Checkbox::new(&mut on, tr(themes::slot_label(slot))));
                widgets::mark(&format!("theme-quick-slot-{slot}"), r.rect);
                if has { self.slots[i] = on; } else { r.on_disabled_hover_text(tr("이 테마에는 이 출력에 쓸 디자인이 없습니다")); }
            }
            let chosen = self.chosen_slots(t);
            let using = used(t);
            let already = !chosen.is_empty() && chosen.iter().all(|o| using.contains(o));
            let b = Btn::new(tr(if already { "고른 출력에 이미 쓰는 중" } else { "고른 출력에 적용" })).kind(Kind::Primary).icon("check").size(Size::Sm).enabled(!chosen.is_empty() && !already);
            if btn(ui, "theme-quick-apply", b.tooltip(tr("고른 출력만 이 테마로 바꿉니다. 고르지 않은 출력은 그대로입니다"))) { acts.push(Do::Apply); }
        });
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
            let builtin = t["builtin"] == true;
            let id = s(t, "id").to_string();
            if btn(ui, "theme-quick-dup", Btn::new(tr(if builtin { "복제해서 수정" } else { "복제" })).icon("copy").size(Size::Sm)) {
                acts.push(Do::OpenForm(FormKind::Duplicate(id.clone())));
            }
            if !builtin {
                ui.menu_button(tr("디자인 편집"), |ui| {
                    for slot in SLOTS {
                        if let Some(tid) = t["slots"][slot]["id"].as_str() {
                            if btn(ui, &format!("theme-quick-edit-{slot}"), Btn::new(tr(themes::slot_label(slot))).icon("pencil").size(Size::Sm)) {
                                acts.push(Do::Edit(tid.to_string())); ui.close();
                            }
                        }
                    }
                });
                if btn(ui, "theme-quick-rename", Btn::new(tr("이름·설명 바꾸기")).icon("pencil").size(Size::Sm)) { acts.push(Do::OpenForm(FormKind::Rename(id))); }
            }
            if btn(ui, "theme-quick-export", Btn::new(tr("내보내기…")).icon("download").size(Size::Sm)) { acts.push(Do::Export); }
        });
    }

    /// 이름·설명 입력 줄
    fn form_ui(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Do>) {
        let p = pal();
        let Some(f) = self.form.as_mut() else { return };
        let (title, ok) = match f.kind {
            FormKind::SaveCurrent => ("지금 출력 디자인을 내 테마로 저장", "저장"),
            FormKind::Duplicate(_) => ("복제해서 내 테마 만들기", "복제"),
            FormKind::Rename(_) => ("이름·설명 바꾸기", "바꾸기"),
        };
        ui.label(theme::rich(&tr(title), 13.0, Weight::SemiBold, p.text));
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
            ui.label(tr("이름"));
            let r = ui.add(egui::TextEdit::singleline(&mut f.name).char_limit(60).desired_width(220.0).hint_text(tr("테마 이름")));
            widgets::mark("theme-form-name", r.rect);
            if f.focus {
                r.request_focus();
                f.focus = false;
            }
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            ui.label(tr("설명"));
            let r2 = ui.add(egui::TextEdit::singleline(&mut f.desc).char_limit(300).desired_width(260.0).hint_text(tr("(선택) 언제 쓰는 테마인지")));
            let enter2 = r2.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let can = !f.name.trim().is_empty();
            if (btn(ui, "theme-form-ok", Btn::new(tr(ok)).icon("check").kind(Kind::Primary).size(Size::Sm).enabled(can)) || ((enter || enter2) && can)) && can {
                acts.push(Do::SubmitForm);
            }
            if btn(ui, "theme-form-cancel", Btn::new(tr("그만두기")).size(Size::Sm)) || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                acts.push(Do::CancelForm);
            }
        });
        if matches!(f.kind, FormKind::SaveCurrent | FormKind::Duplicate(_)) {
            ui.label(theme::rich(&tr("새 디자인으로 따로 저장되므로 원래 템플릿과 기본 테마는 바뀌지 않습니다. 같은 이름이 있으면 뒤에 번호를 붙입니다."), 11.5, Weight::Regular, p.muted2));
        }
    }

    /// 카드 격자: 내 테마 먼저, 그다음 기본 테마 묶음
    fn grid(&self, ui: &mut egui::Ui, pc: &paint::Ctx, shown: &[&Value], used: &dyn Fn(&Value) -> Vec<&'static str>, acts: &mut Vec<Do>) {
        let p = pal();
        let smp = sample(self.sample);
        if shown.is_empty() {
            ui.add_space(12.0);
            ui.label(theme::rich(&tr("찾는 이름의 테마가 없습니다. 찾는 글을 지우거나 '모두'를 누르세요."), 13.0, Weight::Regular, p.muted));
        }
        let mut groups: Vec<String> = vec!["내 테마".to_string()];
        for t in shown {
            let g = s(t, "group").to_string();
            if !groups.contains(&g) {
                groups.push(g);
            }
        }
        for g in &groups {
            let in_group: Vec<&&Value> = shown.iter().filter(|t| s(t, "group") == g).collect();
            if in_group.is_empty() {
                // 내 테마가 하나도 없을 때는 만드는 법을 알려 준다 (찾기·거르기로 가려진 때는 아님)
                if g == "내 테마" && self.filter != 1 && self.query.trim().is_empty() {
                    ui.label(theme::rich(&tr("내 테마"), 13.0, Weight::SemiBold, p.muted));
                    ui.label(theme::rich(&tr("아직 내 테마가 없습니다. 위의 '지금 출력 디자인을 내 테마로 저장'이나 기본 테마의 '복제해서 수정'으로 만드세요."), 12.5, Weight::Regular, p.muted2));
                    ui.add_space(8.0);
                }
                continue;
            }
            ui.add_space(4.0);
            let head = if g == "내 테마" { tr("내 테마") } else { tr_args("기본 테마 · {1}", &[&tr(g)]) };
            ui.label(theme::rich(&head, 13.0, Weight::SemiBold, p.muted));
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(12.0, 12.0);
                for t in in_group {
                    let id = s(t, "id");
                    if self.card(ui, pc, &smp, t, self.sel == id, &used(t)).clicked() {
                        acts.push(Do::Select(id.to_string()));
                    }
                }
            });
            ui.add_space(8.0);
        }
    }

    /// 테마 카드: 실제 모양 썸네일 + 이름 + 쓰는 곳 배지. 마우스를 올리면 강조, 고르면 accent 테두리. 누르면 미리보기만
    fn card(&self, ui: &mut egui::Ui, pc: &paint::Ctx, smp: &Value, t: &Value, selected: bool, used: &[&'static str]) -> egui::Response {
        let p = pal();
        let (r, resp) = ui.allocate_exact_size(Vec2::new(CARD_W, THUMB_H + 46.0), Sense::click());
        widgets::mark(&format!("theme-card-{}", s(t, "id")), r);
        let hovered = resp.hovered();
        let bg = if selected { p.sel } else if hovered { p.hover } else { p.panel2 };
        let border = if selected { p.accent } else if hovered { p.line2 } else { p.line };
        ui.painter().rect(r, crate::theme::cr(crate::theme::Rad::Lg), bg, Stroke::new(if selected { 2.0 } else { 1.0 }, border), StrokeKind::Inside);
        let thumb = Rect::from_min_size(r.min + Vec2::new(6.0, 6.0), Vec2::new(CARD_W - 12.0, THUMB_H - 6.0));
        let slot = SLOTS.iter().find(|o| t["slots"][**o].is_object()).copied().unwrap_or("main");
        paint_slide(ui, pc, thumb, Some(smp), &t["slots"][slot], "개역개정", true, false, false);
        let name = shown_name(t);
        let clip = ui.painter().with_clip_rect(Rect::from_min_max(Pos2::new(r.left() + 6.0, thumb.bottom()), r.max - Vec2::new(6.0, 2.0)));
        clip.text(Pos2::new(r.left() + 10.0, thumb.bottom() + 8.0), Align2::LEFT_TOP, &name, theme::font(13.0, if selected { Weight::SemiBold } else { Weight::Regular }), p.text);
        // 쓰는 곳 배지 (+ 고쳐 쓴 기본 테마 표시)
        let mut x = r.left() + 10.0;
        let y = thumb.bottom() + 26.0;
        let modified = t["modified"].as_array().is_some_and(|a| !a.is_empty());
        let mut badges: Vec<(String, Color32)> = used.iter().map(|o| (tr(themes::slot_label(o)), slot_color(o))).collect();
        if modified {
            badges.push((tr("고침"), p.muted));
        }
        for (label, col) in badges {
            let g = clip.layout_no_wrap(label, theme::font(10.5, Weight::SemiBold), col);
            let br = Rect::from_min_size(Pos2::new(x, y), Vec2::new(g.size().x + 10.0, g.size().y + 3.0));
            clip.rect(br, crate::theme::cr(crate::theme::Rad::Sm), alpha(col, 0.16), Stroke::new(1.0, alpha(col, 0.55)), StrokeKind::Inside);
            clip.galley(br.center() - g.size() / 2.0, g, col);
            x = br.right() + 4.0;
        }
        if hovered {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let tip = format!("{}\n{}\n{}", name, tr(s(t, "desc")), tr("누르면 오른쪽에서 미리 봅니다 (출력은 바뀌지 않습니다)"));
        resp.on_hover_text(tip.trim().to_string())
    }

    /// 고른 테마: 미리보기 · 적용 · 관리
    fn detail(&mut self, ui: &mut egui::Ui, pc: &paint::Ctx, cur: Option<&Value>, used: &dyn Fn(&Value) -> Vec<&'static str>, acts: &mut Vec<Do>) {
        let p = pal();
        let Some(t) = cur else {
            ui.label(theme::rich(&tr("왼쪽에서 테마를 고르면 여기에서 미리 봅니다."), 13.0, Weight::Regular, p.muted));
            return;
        };
        let builtin = t["builtin"] == true;
        let id = s(t, "id").to_string();
        let using = used(t);
        ui.horizontal_wrapped(|ui| {
            ui.label(theme::rich(&shown_name(t), 16.0, Weight::Bold, p.text));
            chip(ui, &tr(if builtin { "기본 테마" } else { "내 테마" }), if builtin { p.muted } else { p.accent });
        });
        if !s(t, "desc").is_empty() {
            ui.label(theme::rich(&tr(s(t, "desc")), 12.5, Weight::Regular, p.muted));
        }
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if using.is_empty() {
                ui.label(theme::rich(&tr("지금 출력에는 쓰지 않는 테마입니다."), 12.0, Weight::Regular, p.muted2));
            } else {
                ui.label(theme::rich(&tr("지금 쓰는 출력:"), 12.0, Weight::Medium, p.muted));
                for o in &using {
                    chip(ui, &tr(themes::slot_label(o)), slot_color(o));
                }
            }
        });
        let modified: Vec<String> = t["modified"].as_array().map(|a| a.iter().filter_map(|o| o.as_str()).map(|o| tr(themes::slot_label(o))).collect()).unwrap_or_default();
        if !modified.is_empty() {
            ui.label(theme::rich(&tr_args("이 기본 테마의 {1} 디자인을 고쳐 쓰고 있습니다. 미리보기는 고친 모양이고, 적용해도 고친 것이 그대로 남습니다.", &[&modified.join(", ")]), 12.0, Weight::Regular, p.text2));
        }
        ui.add_space(6.0);
        // ---- 출력별 미리보기 (실제 출력과 같은 그림)
        let w = ui.available_width().min(DETAIL_W);
        let mut preview = |ui: &mut egui::Ui, o: &str, pw: f32| {
            // 찬송가·찬양 칸은 가사 견본으로 본다
            let smp = sample(if o == "overlayLyrics" { 1 } else { self.sample });
            let label = match o {
                "main" => "예배당 화면",
                "overlay" => "OBS 자막",
                _ => "OBS 찬송가·찬양",
            };
            ui.vertical(|ui| {
                ui.set_width(pw);
                ui.label(theme::rich(&tr(label), 12.0, Weight::Medium, p.muted));
                let (r, _) = ui.allocate_exact_size(Vec2::new(pw, pw * 9.0 / 16.0), Sense::hover());
                widgets::mark(&format!("theme-preview-{o}"), r);
                paint_slide(ui, pc, r, Some(&smp), &t["slots"][o], "개역개정", true, false, false);
                ui.painter().rect_stroke(r, crate::theme::cr(crate::theme::Rad::Sm), Stroke::new(1.0, p.line2), StrokeKind::Outside);
            });
        };
        if t["slots"]["main"].is_object() {
            preview(ui, "main", w);
            ui.add_space(4.0);
        }
        // OBS 쪽 미리보기는 나란히 (세로로 길어지지 않게)
        let obs: Vec<&str> = ["overlay", "overlayLyrics"].into_iter().filter(|o| t["slots"][*o].is_object()).collect();
        if !obs.is_empty() {
            let pw = ((w - 8.0) / 2.0).max(120.0);
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                for o in obs {
                    preview(ui, o, pw);
                }
            });
            ui.add_space(4.0);
        }
        if builtin {
            ui.label(theme::rich(&tr("기본 테마에는 OBS 찬송가·찬양 자막 디자인이 따로 없습니다. 그 출력은 지금 쓰는 디자인 그대로 둡니다."), 11.5, Weight::Regular, p.muted2));
        }
        ui.add_space(4.0);
        ui.separator();
        // ---- 적용
        ui.label(theme::rich(&tr("출력에 적용"), 13.0, Weight::SemiBold, p.text));
        ui.label(theme::rich(&tr("이 테마를 쓸 출력을 고르세요."), 12.0, Weight::Regular, p.muted));
        for (i, o) in SLOTS.iter().enumerate() {
            let has = t["slots"][*o].is_object();
            let label = match *o {
                "main" => "예배당 화면",
                "overlay" => "OBS 자막",
                _ => "OBS 찬송가·찬양 자막",
            };
            let mut on = self.slots[i] && has;
            let r = ui.add_enabled(has, egui::Checkbox::new(&mut on, tr(label)));
            widgets::mark(&format!("theme-slot-{o}"), r.rect);
            if has {
                self.slots[i] = on;
            } else {
                r.on_disabled_hover_text(tr("이 테마에는 이 출력에 쓸 디자인이 없습니다"));
            }
        }
        let n_has = SLOTS.iter().filter(|o| t["slots"][**o].is_object()).count();
        let chosen = self.chosen_slots(t);
        ui.horizontal_wrapped(|ui| {
            if btn(ui, "theme-slot-all", Btn::new(tr("모두 고르기")).size(Size::Sm).enabled(chosen.len() < n_has).tooltip(tr("이 테마에 있는 출력 디자인을 모두 고릅니다"))) {
                self.slots = [true, true, true];
            }
        });
        ui.add_space(2.0);
        ui.label(theme::rich(&tr("적용하면 지금 송출 중인 화면(LIVE)의 모양도 바로 바뀝니다. 고쳐 둔 디자인은 지워지지 않고, 적용한 뒤 '적용 되돌리기'로 되돌릴 수 있습니다."), 11.5, Weight::Regular, p.warn_strong));
        ui.add_space(2.0);
        let already = !chosen.is_empty() && chosen.iter().all(|o| using.contains(o));
        let b = Btn::new(if already { tr("고른 출력에 이미 쓰는 중") } else { tr("고른 출력에 적용") }).icon("check").kind(Kind::Primary).min_width(ui.available_width().min(DETAIL_W)).enabled(!chosen.is_empty() && !already);
        if btn(ui, "theme-apply", b.tooltip(tr("고른 출력만 이 테마로 바꿉니다. 고르지 않은 출력은 그대로입니다"))) {
            acts.push(Do::Apply);
        }
        ui.add_space(6.0);
        ui.separator();
        // ---- 관리
        ui.label(theme::rich(&tr("테마 관리"), 13.0, Weight::SemiBold, p.text));
        if !builtin {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
                ui.label(theme::rich(&tr("디자인 편집:"), 12.0, Weight::Medium, p.muted));
                for o in SLOTS {
                    if let Some(tid) = t["slots"][o]["id"].as_str() {
                        if btn(ui, &format!("theme-edit-{o}"), Btn::new(tr(themes::slot_label(o))).icon("pencil").size(Size::Sm).tooltip(tr("템플릿 편집 창에서 이 출력의 디자인을 고칩니다"))) {
                            acts.push(Do::Edit(tid.to_string()));
                        }
                    }
                }
            });
        }
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
            if btn(ui, "theme-dup", Btn::new(tr(if builtin { "복제해서 수정" } else { "복제" })).icon("copy").size(Size::Sm).tooltip(tr("이 테마를 복사해 새 내 테마를 만듭니다. 원본은 바뀌지 않습니다"))) {
                acts.push(Do::OpenForm(FormKind::Duplicate(id.clone())));
            }
            if !builtin && btn(ui, "theme-rename", Btn::new(tr("이름·설명 바꾸기")).icon("pencil").size(Size::Sm)) {
                acts.push(Do::OpenForm(FormKind::Rename(id.clone())));
            }
            if btn(ui, "theme-export", Btn::new(tr("내보내기…")).icon("download").size(Size::Sm).tooltip(tr("이 테마의 디자인만 파일로 저장합니다. 다른 설정·암호·큐시트는 들어가지 않고, 그림·영상·로고는 경로만 들어갑니다"))) {
                acts.push(Do::Export);
            }
            if !builtin && self.confirm != Some(Confirm::Delete) && btn(ui, "theme-delete", Btn::new(tr("삭제")).icon("trash-2").kind(Kind::Danger).size(Size::Sm).tooltip(tr("누르면 한 번 더 묻습니다"))) {
                acts.push(Do::Ask(Confirm::Delete));
            }
            if builtin && !modified.is_empty() && self.confirm != Some(Confirm::Reset) && btn(ui, "theme-reset", Btn::new(tr("처음 디자인으로…")).icon("rotate-ccw").size(Size::Sm).tooltip(tr("이 기본 테마를 고치기 전 모양으로 되돌립니다. 누르면 한 번 더 묻습니다"))) {
                acts.push(Do::Ask(Confirm::Reset));
            }
        });
        if builtin {
            ui.label(theme::rich(&tr("기본 테마는 이름을 바꾸거나 지울 수 없습니다. 고쳐 쓰려면 '복제해서 수정'으로 내 테마를 만드세요."), 11.5, Weight::Regular, p.muted2));
        }
        if let Some(c) = self.confirm {
            let (q, yes) = match c {
                Confirm::Delete => (tr_args("내 테마 '{1}'을(를) 목록에서 지울까요? 그 디자인(템플릿)은 '출력 템플릿'에 남습니다.", &[&shown_name(t)]), "정말 지우기"),
                Confirm::Reset => (tr_args("'{1}' 테마의 고친 디자인({2})을 버리고 처음 디자인으로 되돌릴까요? 지금 이 디자인을 쓰는 출력의 모양이 바로 바뀝니다.", &[&shown_name(t), &modified.join(", ")]), "처음 디자인으로 되돌리기"),
            };
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
                ui.label(RichText::new(q).color(p.danger_text));
                if btn(ui, "theme-confirm-yes", Btn::new(tr(yes)).kind(Kind::Danger).size(Size::Sm)) {
                    acts.push(Do::Confirmed);
                }
                if btn(ui, "theme-confirm-no", Btn::new(tr("그만두기")).size(Size::Sm)) {
                    acts.push(Do::CancelConfirm);
                }
            });
        }
        ui.add_space(8.0);
    }
}

/// 파일 이름으로 쓸 수 없는 글자를 뺀다
fn safe_file_name(name: &str) -> String {
    let n: String = name.chars().map(|c| if c.is_control() || "\\/:*?\"<>|".contains(c) { '_' } else { c }).collect();
    let n = n.trim().trim_matches('.').to_string();
    if n.is_empty() { "theme".to_string() } else { n }
}

/// 순서 편집칸: 이 순서에만 쓸 템플릿 (예배당 · OBS 자막) · 테마로 한 번에 (기본 테마 + 내 테마)
pub fn item_templates(ui: &mut egui::Ui, e: &mut serde_json::Value, core: &Shared) -> bool {
    let (tpls, all): (Vec<(String, String)>, Vec<Value>) = {
        let c = core.lock();
        (c.settings.templates().iter().map(|t| (t["id"].as_str().unwrap_or("").to_string(), wp_core::i18n::tr_name(t["name"].as_str().unwrap_or("")))).collect(), themes::list_all(c.settings.get()))
    };
    let mut ch = false;
    ui.label(RichText::new(wp_core::i18n::tr("이 순서에만 쓸 템플릿 (비우면 기본)")).small());
    for (key, label, salt) in [("tplMain", "예배당", "it-tm"), ("tplOverlay", "OBS 자막", "it-to")] {
        ui.horizontal(|ui| {
            ui.label(wp_core::i18n::tr(label));
            let cur = e[key].as_str().unwrap_or("").to_string();
            let name = tpls.iter().find(|t| t.0 == cur).map(|t| t.1.clone()).unwrap_or_else(|| wp_core::i18n::tr("(기본)"));
            egui::ComboBox::from_id_salt(salt).selected_text(name).width(220.0).show_ui(ui, |ui| {
                if ui.selectable_label(cur.is_empty(), wp_core::i18n::tr("(기본)")).clicked() { e[key] = serde_json::json!(""); ch = true; }
                for (id, n) in &tpls {
                    if ui.selectable_label(*id == cur, n).clicked() { e[key] = serde_json::json!(id); ch = true; }
                }
            });
        });
    }
    ui.horizontal(|ui| {
        ui.label(wp_core::i18n::tr("테마로"));
        egui::ComboBox::from_id_salt("it-theme").selected_text(wp_core::i18n::tr("고르기…")).width(220.0).show_ui(ui, |ui| {
            for t in &all {
                if ui.selectable_label(false, shown_name(t)).clicked() {
                    ch |= apply_item_theme(e, core, s(t, "id"));
                }
            }
        });
    });
    ch
}

/// 순서에 테마의 예배당·OBS 자막 디자인을 지정한다 (전체 출력 설정은 바꾸지 않는다). 테마에 없는 칸은 그대로 둔다
pub(crate) fn apply_item_theme(e: &mut Value, core: &Shared, theme_id: &str) -> bool {
    let Ok(ids) = themes::ensure_theme(&mut core.lock(), theme_id) else { return false };
    let mut ch = false;
    for (key, o) in [("tplMain", "main"), ("tplOverlay", "overlay")] {
        if ids[o].is_string() {
            e[key] = ids[o].clone();
            ch = true;
        }
    }
    ch
}

#[cfg(test)]
#[path = "theme_ui_tests.rs"]
mod tests;
