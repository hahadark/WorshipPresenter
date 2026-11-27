//! [갈래 C] 한 PPT 를 여러 순서로 나누기 창 (Electron control/cue.js openSplit).
//!  - 위: 자동으로 나누기 — AI · 규칙(AI 없이) · 채팅 AI 프롬프트 복사 → 답 불러오기. 결과는 아래에서 고친 뒤 '나누기'
//!  - 가운데: 슬라이드 썸네일 격자. 누르면 시작 · Shift+누르면 끝 · **두 번 누르면 그 슬라이드에서 새 순서** (이미 시작이면 앞 순서와 합침)
//!  - 아래: 범위·이름 '범위 더하기' · 순서 목록(이름 고치기 · 앞과 합치기 · 빼기) · 나누기 · 모두 비우기
//! 손으로 고치는 규칙은 wp_core::splitedit, 자동 나누기는 wp_core::pptsplit.
use crate::app::App;
use crate::paint;
use crate::theme::{self, pal, rich, Weight};
use crate::widgets::{self, Btn, Click, Kind, Size};
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use serde_json::Value;
use wp_core::i18n::{tr, tr_args};
use wp_core::pptsplit::{self, Part};
use wp_core::{hymns, splitedit};

#[derive(Clone, Default)]
struct St {
    open: bool,
    /// 나눌 순서 (id 로 찾는다 — 창을 연 뒤 순서가 옮겨져도)
    item_id: String,
    title: String,
    file: String,
    /// 범위·편집을 뺀 원래 슬라이드 전부
    all: Vec<Value>,
    texts: Vec<String>,
    parts: Vec<Part>,
    /// 마지막으로 누른 칸 (Shift 범위의 시작, 0부터)
    from: usize,
    range: String,
    name: String,
    extra: String,
    chat: bool,
    answer: String,
    msg: String,
    msg_err: bool,
    busy: bool,
}

fn sid() -> egui::Id {
    egui::Id::new("wp-pptsplit")
}

/// AI 답 (백그라운드에서 끝나면 여기에 놓고, 창이 다음 프레임에 가져간다): (순서 id, 결과)
static AI_DONE: parking_lot::Mutex<Option<(String, Result<Vec<Part>, String>)>> = parking_lot::Mutex::new(None);

/// 순서 종류 이름 (pptsplit.js KIND)
fn kind_name(k: &str) -> &'static str {
    match k {
        "hymn" => "찬송가",
        "song" => "찬양",
        "bible" => "성경",
        "reading" => "교독문",
        "prayer" => "기도",
        "sermon" => "설교·강의",
        "notice" => "광고·소식",
        "offering" => "봉헌",
        "video" => "영상",
        "" => "",
        _ => "제목·그 밖",
    }
}

/// 슬라이드와 글을 직접 주고 연다 (시험 · open 이 씀)
pub(crate) fn open_with(ctx: &egui::Context, it: &Value, all: Vec<Value>, mut texts: Vec<String>) {
    texts.resize(all.len(), String::new());
    let st = St { open: true, item_id: it["id"].as_str().unwrap_or("").into(), title: it["title"].as_str().unwrap_or("").into(), file: it["file"].as_str().unwrap_or("").into(), all, texts, ..Default::default() };
    ctx.data_mut(|d| d.insert_temp(sid(), st));
}

/// 그 순서의 나누기 창을 연다. 슬라이드가 하나뿐이거나 아직 그림을 만드는 중이면 알림만
pub(crate) fn open(app: &App, ctx: &egui::Context, it: &Value) {
    let mut c = app.core.lock();
    // 범위·편집·건너뛰기·차례를 뺀 원래 슬라이드 (cue.js buildSlides({ ...it, range: '' }, meta, { raw: true }))
    let mut raw = it.clone();
    if let Some(o) = raw.as_object_mut() {
        for k in ["range", "edits", "skip", "slideOrder", "extra"] {
            o.remove(k);
        }
    }
    let opts = c.split_opts(Some(&raw));
    let all = match hymns::build_slides(&c, &raw, &opts) {
        Ok(a) => a,
        Err(e) => {
            let k = if e == hymns::BUILDING { "info" } else { "error" };
            c.notify(e, k);
            return;
        }
    };
    if all.len() < 2 {
        c.notify(tr("슬라이드가 하나뿐이라 나눌 수 없습니다."), "warn");
        return;
    }
    let texts = hymns::slide_texts(&c, it["file"].as_str().unwrap_or("")).unwrap_or_default();
    drop(c);
    open_with(ctx, it, all, texts);
}

/// 점검용 (--open pptsplit): 큐시트의 첫 PPT 순서로 나누기 창을 연다 (그림이 만들어질 때까지 몇 번 다시 해 본다)
static WANT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
pub(crate) fn want_open() {
    WANT.store(600, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(test)]
pub(crate) fn parts_now(ctx: &egui::Context) -> Vec<(usize, usize, String)> {
    ctx.data(|d| d.get_temp::<St>(sid())).map(|s| s.parts.iter().map(|p| (p.a, p.b, p.name.clone())).collect()).unwrap_or_default()
}
#[cfg(test)]
pub(crate) fn range_now(ctx: &egui::Context) -> String {
    ctx.data(|d| d.get_temp::<St>(sid())).map(|s| s.range).unwrap_or_default()
}
#[cfg(test)]
pub(crate) fn is_open(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<St>(sid())).is_some_and(|s| s.open)
}

impl St {
    fn set_parts(&mut self, parts: Vec<Part>, how: &str) {
        self.parts = parts;
        self.msg_err = false;
        self.msg = if self.parts.len() > 1 {
            tr_args("{1}: 순서 {2}개 — 이름·경계를 고친 뒤 '나누기' 하세요", &[&tr(how), &self.parts.len()])
        } else {
            tr_args("{1}: 나눌 곳을 찾지 못했습니다. 슬라이드를 두 번 눌러 직접 나누거나 다른 방법을 써 보세요.", &[&tr(how)])
        };
    }
    fn fail(&mut self, e: impl Into<String>) {
        self.msg = e.into();
        self.msg_err = true;
    }
    fn add(&mut self) -> Result<(), String> {
        let next = splitedit::add_range(&mut self.parts, &self.range, &self.name, self.all.len(), &self.title)?;
        self.range = next.map(|n| n.to_string()).unwrap_or_default();
        self.from = next.map(|n| n - 1).unwrap_or(0);
        self.name.clear();
        Ok(())
    }
}

impl App {
    pub(crate) fn pptsplit_window(&mut self, ctx: &egui::Context) {
        let left = WANT.load(std::sync::atomic::Ordering::Relaxed);
        if left > 0 && left % 30 == 0 {
            // 점검용: 첫 PPT 순서 (슬라이드 2장 이상) 를 연다. 열면서 보기 좋게 한 곳을 나눠 둔다
            let items = self.core.lock().cue.as_ref().map(|q| q.items.clone()).unwrap_or_default();
            for it in items.iter().filter(|it| crate::hymns_ui::can_open_ppt_tools(it)) {
                let meta = hymns::load(&self.core.lock(), it["file"].as_str().unwrap_or(""));
                if meta.is_ok_and(|m| m["slides"].as_array().or(m["scores"].as_array()).is_some_and(|a| a.len() >= 2)) {
                    open(self, ctx, it);
                    if let Some(mut s) = ctx.data(|d| d.get_temp::<St>(sid())).filter(|s| s.open) {
                        let mid = s.all.len() / 2 + 1;
                        splitedit::toggle_at(&mut s.parts, mid, s.all.len(), &s.texts, &s.title);
                        ctx.data_mut(|d| d.insert_temp(sid(), s));
                        WANT.store(1, std::sync::atomic::Ordering::Relaxed);
                    }
                    break;
                }
            }
        }
        if left > 0 {
            WANT.store(left - 1, std::sync::atomic::Ordering::Relaxed);
            ctx.request_repaint();
        }
        let Some(mut st) = ctx.data(|d| d.get_temp::<St>(sid())).filter(|s| s.open) else { return };
        // 끝난 AI 답 받기
        if let Some((id, r)) = AI_DONE.lock().take_if(|x| x.0 == st.item_id) {
            let _ = id;
            st.busy = false;
            match r {
                Ok(p) => st.set_parts(p, "AI 로 나눴습니다"),
                Err(e) => st.fail(e),
            }
        }
        let p = pal();
        let total = st.all.len();
        let tpl = self.core.lock().template_for("main");
        let mut keep = true;
        let mut go = false;
        widgets::float_window(ctx, "pptsplit", tr_args("'{1}' 여러 순서로 나누기", &[&st.title]), [880.0, 660.0]).open(&mut keep).show(ctx, |ui| {
            ui.set_max_width(ui.available_width().min(900.0)); // 긴 안내 글이 창을 화면 끝까지 넓히지 않게
            // ---------------- 자동으로 나누기 ----------------
            ui.horizontal_wrapped(|ui| {
                ui.label(rich(tr("자동으로 나누기"), 13.0, Weight::SemiBold, p.text));
                let b = ui.add(Btn::new(tr("AI 로 나누기")).icon("sparkles").kind(Kind::Primary).enabled(!st.busy).tooltip(tr("오픈라우터 API 키로 (주보 AI 와 같은 키·모델)")));
                if b.clicked() {
                    self.split_ai(&mut st);
                }
                let b = ui.add(Btn::new(tr("규칙으로 (AI 없이)")).enabled(!st.busy).tooltip(tr("AI 없이: 순서 이름(찬송·기도·성경봉독·설교·광고·축도 …) · 찬송가 장 번호 · 장절이 적힌 슬라이드에서 나눕니다")));
                widgets::mark("sp-rule", b.rect);
                if b.clicked() {
                    match pptsplit::rule_split(&st.texts, &st.title) {
                        Ok(parts) => st.set_parts(parts, "규칙으로 나눴습니다"),
                        Err(e) => st.fail(e),
                    }
                }
                let b = ui.add(Btn::new(tr("채팅 AI 프롬프트 복사")).icon("clipboard-copy").enabled(!st.busy).tooltip(tr("API 키 없이 ChatGPT · Claude · Gemini 에서")));
                if b.clicked() {
                    let blank = st.texts.iter().all(|t| t.trim().is_empty());
                    let sheets = if blank { total.div_ceil(if total > 160 { 25 } else { 16 }) } else { 0 };
                    match pptsplit::chat_prompt(&st.texts, &st.title, st.extra.trim(), sheets) {
                        Ok(pr) => {
                            widgets::copy_notice(ui.ctx(), pr, tr("프롬프트를 복사했습니다. 채팅 AI 에 붙여 넣고, 답을 'AI 답' 칸에 붙여 넣으세요."));
                            st.chat = true;
                            st.msg_err = false;
                            st.msg = tr_args("프롬프트(슬라이드 {1}장 글자 포함)를 복사했습니다", &[&total]);
                        }
                        Err(e) => st.fail(e),
                    }
                }
            });
            ui.add(egui::TextEdit::singleline(&mut st.extra).desired_width(f32::INFINITY).hint_text(tr("AI 에게 추가 요청 (선택) — 예: 찬양 콘티는 곡마다 따로 · 1부와 2부 사이 영상은 한 순서로")));
            let no_text = st.texts.iter().filter(|t| t.trim().is_empty()).count();
            let mut note = tr_args("슬라이드 {1}장", &[&total]);
            if no_text > 0 {
                note.push_str(&tr_args(" · 글자가 없는 슬라이드 {1}장", &[&no_text]));
            }
            note.push_str(&tr(" — 나온 결과는 아래에서 이름·경계를 고친 뒤 '나누기' 하세요."));
            ui.label(rich(note, 12.0, Weight::Regular, p.muted));
            if st.chat {
                ui.horizontal_wrapped(|ui| {
                    ui.label(rich(tr("프롬프트를 복사했습니다. 채팅 AI 에 붙여 넣고 답을 아래에 붙여 넣으세요."), 12.0, Weight::Regular, p.muted));
                    for (n, u) in [("ChatGPT", "https://chatgpt.com/"), ("Claude", "https://claude.ai/new"), ("Gemini", "https://gemini.google.com/app")] {
                        if ui.add(Btn::new(tr_args("{1} 열기", &[&n])).size(Size::Sm)).clicked() {
                            ui.ctx().open_url(egui::OpenUrl::new_tab(u));
                        }
                    }
                });
                ui.add(egui::TextEdit::multiline(&mut st.answer).desired_rows(3).desired_width(f32::INFINITY).hint_text(tr("AI 의 답을 통째로 붙여 넣으세요 (json 코드 블록이 섞여 있어도 됩니다)")));
                if ui.add(Btn::new(tr("답 불러오기")).icon("check").kind(Kind::Primary)).clicked() {
                    if st.answer.trim().is_empty() {
                        st.fail(tr("AI 의 답을 붙여 넣으세요."));
                    } else {
                        match pptsplit::parse_answer(&st.answer, total, &st.title) {
                            Ok(parts) => st.set_parts(parts, "AI 답을 읽었습니다"),
                            Err(e) => st.fail(e),
                        }
                    }
                }
            }
            if !st.msg.is_empty() {
                ui.label(rich(st.msg.clone(), 12.5, Weight::Regular, if st.msg_err { p.danger_text } else { p.text2 }));
            }
            ui.separator();
            ui.label(rich(tr("손으로: 슬라이드를 누르면 시작, Shift 를 누른 채 누르면 끝 — 이름을 쓰고 '범위 더하기'. 두 번 누르면 그 슬라이드에서 새 순서가 시작됩니다 (이미 시작이면 앞 순서와 합침). '나누기' 하면 이 순서 자리에 순서 여러 개가 생기고, 각 순서는 그 범위의 슬라이드만 송출합니다."), 12.0, Weight::Regular, p.muted));
            // ---------------- 썸네일 격자 (.sp-grid) ----------------
            let sel_range = splitedit::range_of(&st.range);
            let c = paint::Ctx { web_dir: &self.web_dir, data_dir: &self.data_dir };
            let shift = ui.input(|i| i.modifiers.shift);
            let grid_h = (ui.available_height() - 230.0).clamp(170.0, 420.0);
            egui::ScrollArea::vertical().id_salt("sp-grid").max_height(grid_h).auto_shrink([false, true]).show(ui, |ui| {
                let gap = 6.0;
                let w = ui.available_width();
                let cols = (((w + gap) / (150.0 + gap)).floor() as usize).max(1);
                let cw = (w - gap * (cols - 1) as f32) / cols as f32;
                let (tw, th) = (tpl["width"].as_f64().unwrap_or(1920.0) as f32, tpl["height"].as_f64().unwrap_or(1080.0) as f32);
                let sh = (cw - 6.0) * th / tw;
                let ch = sh + 6.0 + 18.0;
                let rows = total.div_ceil(cols);
                for row in 0..rows {
                    let (rr, _) = ui.allocate_exact_size(Vec2::new(w, ch), Sense::hover());
                    for j in 0..cols {
                        let k = row * cols + j;
                        if k >= total {
                            break;
                        }
                        let n = k + 1;
                        let r = Rect::from_min_size(Pos2::new(rr.left() + j as f32 * (cw + gap), rr.top()), Vec2::new(cw, ch));
                        let resp = ui.interact(r, egui::Id::new(("sp-th", k)), Sense::click()).on_hover_text(tr_args("{1}번 슬라이드 — 누르면 시작, Shift+누르면 끝, 두 번 누르면 여기서 새 순서", &[&n]));
                        widgets::mark(&format!("sp-th-{n}"), r);
                        // cue.js:545-557 누르면 시작 · Shift 끝 · 두 번 누르면 나눔 토글 (항목 키로 판정 — 표시가 바뀌어도 같은 칸)
                        match widgets::click2(ui, ("sp-th", st.item_id.as_str(), k), &resp) {
                            Click::Single => {
                                if shift {
                                    st.range = splitedit::range_text(st.from.min(k) + 1, st.from.max(k) + 1);
                                } else {
                                    st.from = k;
                                    st.range = n.to_string();
                                }
                            }
                            Click::Double => splitedit::toggle_at(&mut st.parts, n, total, &st.texts, &st.title),
                            Click::None => {}
                        }
                        if resp.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        if !ui.is_rect_visible(r) {
                            continue;
                        }
                        let own = splitedit::owner(&st.parts, n);
                        let start = own.is_some_and(|o| st.parts[o].a == n);
                        // 순서마다 번갈아 두 색 (.pa / .pb), 고른 범위는 강조 (.sel)
                        let band = own.map(|o| if o % 2 == 0 { p.accent } else { p.gold });
                        let bg = match band {
                            Some(b) => theme::mix(b, 22.0, p.sunken),
                            None if resp.hovered() => p.hover,
                            None => p.sunken,
                        };
                        ui.painter().rect_filled(r, crate::theme::cr(crate::theme::Rad::Sm), bg);
                        let screen = Rect::from_min_size(r.min + Vec2::splat(3.0), Vec2::new(cw - 6.0, sh));
                        let s = &st.all[k];
                        let has_img = s["image"].as_str().is_some_and(|x| !x.is_empty()) && s["kind"] != "video";
                        if has_img || st.texts[k].trim().is_empty() {
                            paint::paint_slide(ui, &c, screen, Some(s), &tpl, "", true, false, false);
                        } else {
                            // 그림이 없는 슬라이드: 글자를 작게 (.sp-tx)
                            ui.painter().rect_filled(screen, 0.0, Color32::BLACK);
                            let t: String = st.texts[k].chars().take(40).collect();
                            let g = ui.painter().layout(t, theme::font(10.5, Weight::Regular), Color32::from_gray(0xdd), screen.width() - 8.0);
                            ui.painter().with_clip_rect(screen).galley(screen.min + Vec2::splat(4.0), g, Color32::WHITE);
                        }
                        // 번호 + 시작이면 순서 이름 (.sp-pn)
                        let mut lj = egui::text::LayoutJob::default();
                        lj.append(&n.to_string(), 0.0, egui::TextFormat { font_id: theme::font(12.0, Weight::Bold), color: p.hl, ..Default::default() });
                        if let (true, Some(o)) = (start, own) {
                            lj.append(&format!("{}. {}", o + 1, st.parts[o].name), 5.0, egui::TextFormat { font_id: theme::font(11.5, Weight::SemiBold), color: p.text, ..Default::default() });
                        }
                        lj.wrap = egui::text::TextWrapping::truncate_at_width(cw - 12.0);
                        let lg = ui.painter().layout_job(lj);
                        ui.painter().galley(Pos2::new(r.left() + 6.0, screen.bottom() + 2.0), lg, p.muted);
                        if start {
                            // 시작 칸은 왼쪽에 굵은 띠 (.start)
                            ui.painter().rect_filled(Rect::from_min_size(r.min, Vec2::new(4.0, r.height())), 0.0, band.unwrap_or(p.accent));
                        }
                        let in_sel = sel_range.is_some_and(|(a, b)| n >= a && n <= b);
                        let border = if in_sel { p.pv } else if resp.hovered() { p.line2 } else { Color32::TRANSPARENT };
                        ui.painter().rect_stroke(r, crate::theme::cr(crate::theme::Rad::Sm), Stroke::new(2.5, border), StrokeKind::Inside);
                    }
                    ui.add_space(gap - ui.spacing().item_spacing.y.min(gap));
                }
            });
            ui.add_space(4.0);
            // ---------------- 범위 · 이름 · 범위 더하기 ----------------
            let mut add = false;
            ui.horizontal(|ui| {
                ui.label(tr("범위"));
                let r = ui.add(egui::TextEdit::singleline(&mut st.range).desired_width(80.0).hint_text("1-4"));
                widgets::mark("sp-range", r.rect);
                ui.label(tr("이름"));
                let bw = 110.0;
                let r = ui.add(egui::TextEdit::singleline(&mut st.name).desired_width((ui.available_width() - bw).max(80.0)).hint_text(tr("예: 광고 · 설교 · 축도")));
                widgets::mark("sp-name", r.rect);
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    add = true;
                    r.request_focus();
                }
                let b = ui.add(Btn::new(tr("범위 더하기")));
                widgets::mark("sp-add", b.rect);
                add |= b.clicked();
            });
            if add {
                if let Err(e) = st.add() {
                    self.core.lock().notify(e, "warn");
                }
            }
            // ---------------- 순서 목록 (.sp-list) ----------------
            let mut rm: Option<usize> = None;
            let mut merge: Option<usize> = None;
            egui::ScrollArea::vertical().id_salt("sp-list").max_height(150.0).auto_shrink([false, true]).show(ui, |ui| {
                for k in 0..st.parts.len() {
                    let (a, b, kind) = (st.parts[k].a, st.parts[k].b, st.parts[k].kind.clone());
                    let adjacent = k > 0 && st.parts[k - 1].b + 1 == a;
                    ui.horizontal(|ui| {
                        ui.add_sized([54.0, 20.0], egui::Label::new(rich(splitedit::range_text(a, b), 12.5, Weight::SemiBold, p.hl)));
                        let kn = kind_name(&kind);
                        if !kn.is_empty() {
                            ui.label(rich(tr(kn), 11.5, Weight::Regular, p.muted));
                        }
                        let r = ui.add(egui::TextEdit::singleline(&mut st.parts[k].name).desired_width((ui.available_width() - 70.0).max(80.0)));
                        widgets::mark(&format!("sp-nm-{k}"), r.rect);
                        if adjacent {
                            let b = ui.add(Btn::new("").icon("arrow-up").size(Size::Sm).tooltip(tr("앞 순서와 합치기")));
                            widgets::mark(&format!("sp-merge-{k}"), b.rect);
                            if b.clicked() {
                                merge = Some(k);
                            }
                        }
                        let b = ui.add(Btn::new("").icon("x").size(Size::Sm).tooltip(tr("빼기 (이 범위의 슬라이드는 넣지 않음)")));
                        widgets::mark(&format!("sp-rm-{k}"), b.rect);
                        if b.clicked() {
                            rm = Some(k);
                        }
                    });
                }
            });
            if let Some(k) = merge {
                splitedit::merge_up(&mut st.parts, k);
            }
            if let Some(k) = rm {
                st.parts.remove(k);
            }
            // ---------------- 나누기 · 모두 비우기 ----------------
            ui.horizontal(|ui| {
                let b = ui.add(Btn::new(tr("나누기")).kind(Kind::Primary).enabled(!st.parts.is_empty()));
                widgets::mark("sp-go", b.rect);
                go = b.clicked();
                let b = ui.add(Btn::new(tr("모두 비우기")).icon("eraser").enabled(!st.parts.is_empty()));
                widgets::mark("sp-clear", b.rect);
                if b.clicked() {
                    st.parts.clear();
                }
                if !st.parts.is_empty() {
                    let left = splitedit::left_over(&st.parts, total);
                    let mut t = tr_args("순서 {1}개", &[&st.parts.len()]);
                    if left > 0 {
                        t.push_str(&tr_args(" · 아직 안 넣은 슬라이드 {1}장", &[&left]));
                    }
                    ui.label(rich(t, 12.5, Weight::Regular, p.muted));
                }
            });
        });
        if go && !st.parts.is_empty() {
            self.split_go(&st);
            st.open = false;
        }
        st.open &= keep;
        ctx.data_mut(|d| d.insert_temp(sid(), st));
    }

    /// '나누기': 그 순서 자리에 순서 여러 개 (cue.js spGo). 슬라이드 번호에 묶인 편집·건너뛰기·차례는 옮기지 않는다
    fn split_go(&mut self, st: &St) {
        let mut c = self.core.lock();
        let Some((i, it)) = c.cue.as_ref().and_then(|q| q.items.iter().position(|x| x["id"] == st.item_id.as_str()).map(|i| (i, q.items[i].clone()))) else {
            c.notify(tr("나눌 순서를 찾지 못했습니다 (지워졌거나 다른 큐시트)."), "error");
            return;
        };
        let parts: Vec<Part> = st
            .parts
            .iter()
            .map(|p| Part { name: if p.name.trim().is_empty() { format!("{} {}-{}", st.title, p.a, p.b) } else { p.name.trim().to_string() }, ..p.clone() })
            .collect();
        let mut made = hymns::split_items(&it, &parts);
        for m in made.iter_mut() {
            if let Some(o) = m.as_object_mut() {
                o.remove("extra");
            }
        }
        let n = made.len();
        c.remember_items(); // Ctrl+Z 로 되돌릴 수 있게
        if let Some(cue) = &mut c.cue {
            cue.items.splice(i..=i, made);
        }
        c.save_cue();
        c.preview_item(i);
        c.notify(tr_args("'{1}' 을(를) 순서 {2}개로 나눴습니다.", &[&st.title, &n]), "ok");
        drop(c);
        self.sel = Some(i);
        self.edit = None;
    }

    /// AI 로 나누기: 백그라운드로 묻고, 답은 창이 받아 목록에 놓는다 (바로 나누지 않음 — 고친 뒤 '나누기')
    fn split_ai(&mut self, st: &mut St) {
        let (ai, key, inp) = {
            let c = self.core.lock();
            (c.settings.get()["ai"].clone(), c.ai_key(), wp_core::pptsheets::inputs(&c, &st.file).unwrap_or_default())
        };
        let model = ai["model"].as_str().unwrap_or("").to_string();
        let endpoint = ai["endpoint"].as_str().unwrap_or("").to_string();
        let (texts, title, extra, id) = (st.texts.clone(), st.title.clone(), st.extra.trim().to_string(), st.item_id.clone());
        st.busy = true;
        st.msg_err = false;
        st.msg = tr_args("AI 가 슬라이드 {1}장을 보는 중…", &[&st.all.len()]);
        hymns::run_bg(
            move || {
                let blank = texts.iter().all(|t| t.trim().is_empty());
                let sheets = if blank { wp_core::pptsheets::build(&inp).map(|s| wp_core::pptsheets::as_data_urls(&s)).unwrap_or_default() } else { vec![] };
                let n = texts.len();
                let _ = n;
                pptsplit::ask_split(&key, &model, &endpoint, &texts, &title, &extra, &sheets).map(|v| {
                    v["parts"].as_array().into_iter().flatten().map(|p| Part { a: p["a"].as_u64().unwrap_or(1) as usize, b: p["b"].as_u64().unwrap_or(1) as usize, name: p["name"].as_str().unwrap_or("").into(), kind: p["kind"].as_str().unwrap_or("").into() }).collect()
                })
            },
            move |c, r| {
                *AI_DONE.lock() = Some((id, r));
                c.emit(wp_core::core::Ev::Ui);
            },
        );
    }
}
